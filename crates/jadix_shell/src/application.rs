//! A single existing budgeted QuickJS runtime with host-independent mount/event IO.
//! Bundle JS/TSX with the SDK's esbuild toolchain; no second interpreter is used.
use crate::{
    mount::{Mount, MountError},
    runtime::{
        native::{BudgetedRuntime, VmError},
        ResourceLimits,
    },
    wire::{Command, ComponentRegistry, Envelope},
    PackageManifest, Session, ShellError,
};
use jadix_core::presentation::{
    CommitBatch, NodeContent, Operation, PlatformEvent, PresentationNode, RootLifetime,
};
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Debug)]
pub enum ApplicationError {
    Session(ShellError),
    Vm(VmError),
    Mount(MountError),
    Host(String),
    Protocol,
}
impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ApplicationError {}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum BridgeRequest {
    Identity,
    Register {
        component: String,
        event: String,
        key: String,
    },
    Release {
        key: String,
    },
    Mount {
        tree: crate::wire::ElementSpec,
    },
    Patch {
        patches: Vec<crate::mount::LeafPatch>,
    },
}
enum Pending {
    Mount(Envelope),
    Patch(Vec<crate::mount::LeafPatch>),
}
// Borrow snapshots only for structural mounts. Leaf publication carries its
// existing rollback journal, so no untouched descriptor is cloned or compared.
enum Publication<'a> {
    Mount {
        before: &'a [PresentationNode],
        after: &'a [PresentationNode],
    },
    Leaves {
        before: &'a [(Vec<usize>, NodeContent)],
        after: &'a [PresentationNode],
    },
}
impl Publication<'_> {
    fn roots(&self) -> &[PresentationNode] {
        match self {
            Self::Mount { after, .. } | Self::Leaves { after, .. } => after,
        }
    }
}
type Deliveries = Rc<RefCell<VecDeque<(String, crate::schema::Value)>>>;
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct ApplicationWork {
    pub message_bytes: u64,
    pub full_mounts: u64,
    pub leaf_patches: u64,
    pub event_lookups: u64,
}
struct State {
    session: Session,
    registry: ComponentRegistry,
    mount: Mount,
    pending: VecDeque<Pending>,
    handles: BTreeMap<String, crate::handles::Handle>,
    deliveries: Deliveries,
    failed: bool,
    work: ApplicationWork,
}
impl State {
    fn request(&mut self, request: BridgeRequest) -> Result<serde_json::Value, ShellError> {
        if self.failed || self.mount.lifetime().is_disposed() {
            return Err(ShellError::InvalidState);
        }
        match request {
            BridgeRequest::Identity => Ok(
                serde_json::json!({"session":self.session.token(),"root":self.mount.lifetime().id().token()}),
            ),
            BridgeRequest::Register {
                component,
                event,
                key,
            } => {
                if key.is_empty() || key.len() > 128 || self.handles.contains_key(&key) {
                    return Err(ShellError::InvalidArguments);
                }
                let schema = self
                    .registry
                    .get(&component)
                    .and_then(|s| s.events.get(&event))
                    .cloned()
                    .ok_or(ShellError::InvalidArguments)?;
                let queue = self.deliveries.clone();
                let callback_key = key.clone();
                let handle = self.session.export_event(
                    self.mount.lifetime().id(),
                    schema,
                    move |value| {
                        queue.borrow_mut().push_back((callback_key.clone(), value));
                    },
                )?;
                self.handles.insert(key, handle.clone());
                Ok(serde_json::to_value(handle).expect("handle is JSON"))
            }
            BridgeRequest::Release { key } => {
                let handle = self.handles.remove(&key).ok_or(ShellError::StaleHandle)?;
                self.session.release_handle(&handle)?;
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Mount { tree } => {
                if self.pending.len() >= 16 {
                    return Err(ShellError::RequestLimit);
                }
                let envelope = Envelope {
                    protocol: 1,
                    session: self.session.token(),
                    command: Command::Mount {
                        root: self.mount.lifetime().id().token(),
                        tree: Box::new(tree),
                    },
                };
                self.session.validate_mount(&envelope, &self.registry)?;
                self.pending.push_back(Pending::Mount(envelope));
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Patch { patches } => {
                if self.pending.len() >= 16 {
                    return Err(ShellError::RequestLimit);
                }
                // The current binding is checked at ordered publication, after
                // any queued mounts. Admission bounds the untrusted message.
                if patches.is_empty()
                    || patches.len() > 1024
                    || patches.iter().any(|p| p.path.len() > 32)
                {
                    return Err(ShellError::InvalidArguments);
                }
                self.pending.push_back(Pending::Patch(patches));
                Ok(serde_json::Value::Null)
            }
        }
    }
    fn fault(&mut self) {
        self.failed = true;
        self.pending.clear();
        self.deliveries.borrow_mut().clear();
        let _ = self.session.fault();
        self.mount.clear();
    }
}
pub struct ScriptApplication {
    vm: BudgetedRuntime,
    state: Rc<RefCell<State>>,
    roots: Vec<PresentationNode>,
    structural: Option<bool>,
    commit_revision: u64,
    changes: Option<BTreeSet<Vec<usize>>>,
}
impl ScriptApplication {
    /// Trusted development bundle. Production callers must use `from_package`.
    pub fn development(
        package: &str,
        source: &str,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        Self::load(
            PackageManifest {
                package: package.into(),
                protocol_version: 1,
                requested_permissions: BTreeSet::new(),
            },
            &BTreeSet::new(),
            source,
            registry,
            limits,
        )
    }
    /// Reverify the signed entry and every resource before any JS executes.
    #[cfg(feature = "native-store")]
    pub fn from_package(
        package: &crate::store::StoredPackage,
        policy: &crate::package::PackagePolicy,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        package
            .signed
            .verify(policy, &package.resources)
            .map_err(|_| ApplicationError::Protocol)?;
        let manifest = &package.signed.manifest;
        let source = std::str::from_utf8(
            package
                .resources
                .get(&manifest.entry)
                .ok_or(ApplicationError::Protocol)?,
        )
        .map_err(|_| ApplicationError::Protocol)?;
        Self::load(
            PackageManifest {
                package: manifest.package.clone(),
                protocol_version: manifest.protocol,
                requested_permissions: manifest.permissions.keys().cloned().collect(),
            },
            &policy.permissions,
            source,
            registry,
            limits,
        )
    }
    fn load(
        manifest: PackageManifest,
        grants: &BTreeSet<String>,
        source: &str,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        let mut session = Session::with_limits(limits).map_err(ApplicationError::Session)?;
        session
            .verify(manifest)
            .map_err(ApplicationError::Session)?;
        session
            .authorize(grants)
            .map_err(ApplicationError::Session)?;
        session.load(|| Ok(())).map_err(ApplicationError::Session)?;
        let lifetime = session.mount_root().map_err(ApplicationError::Session)?;
        let state = Rc::new(RefCell::new(State {
            session,
            registry,
            mount: Mount::new(lifetime),
            pending: VecDeque::new(),
            handles: BTreeMap::new(),
            deliveries: Rc::new(RefCell::new(VecDeque::new())),
            failed: false,
            work: Default::default(),
        }));
        let vm = BudgetedRuntime::new(limits).map_err(ApplicationError::Vm)?;
        let host = state.clone();
        vm.bind_json("__jadixHost", move |input| {
            host.borrow_mut().work.message_bytes += input.len() as u64;
            let result = if input.len() > crate::wire::MAX_MESSAGE_BYTES {
                Err(ShellError::InvalidArguments)
            } else {
                serde_json::from_str::<BridgeRequest>(&input)
                    .map_err(|_| ShellError::InvalidArguments)
                    .and_then(|request| host.borrow_mut().request(request))
            };
            match result {
                Ok(value) => serde_json::json!({"ok":true,"value":value}),
                Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
            }
            .to_string()
        })
        .map_err(ApplicationError::Vm)?;
        let app = Self {
            vm,
            state,
            roots: Vec::new(),
            structural: None,
            commit_revision: 0,
            changes: None,
        };
        app.turn(source, true)?;
        app.state
            .borrow_mut()
            .session
            .activate()
            .map_err(ApplicationError::Session)?;
        Ok(app)
    }
    fn turn(&self, source: &str, startup: bool) -> Result<(), ApplicationError> {
        let source = format!("{source}\n;''");
        let result = if startup {
            self.vm.eval_startup(&source)
        } else {
            self.vm.eval_string(&source)
        }
        .and_then(|_| self.vm.drain_jobs());
        if let Err(error) = result {
            self.state.borrow_mut().fault();
            return Err(ApplicationError::Vm(error));
        }
        Ok(())
    }
    pub fn lifetime(&self) -> RootLifetime {
        self.state.borrow().mount.lifetime().clone()
    }
    pub fn node_at(&self, path: &[usize]) -> Option<jadix_core::presentation::NodeId> {
        self.state.borrow().mount.node_at(path)
    }
    pub fn roots(&self) -> &[PresentationNode] {
        &self.roots
    }
    pub fn work(&self) -> ApplicationWork {
        self.state.borrow().work
    }
    #[cfg(feature = "gpu-host")]
    pub(crate) fn take_changes(&mut self) -> Option<BTreeSet<Vec<usize>>> {
        self.changes.replace(BTreeSet::new())
    }
    /// Host publication precedes binding acceptance. Full snapshots remain O(N).
    /// A rejecting host must leave its tree unchanged. The failed candidate and
    /// later mounts stay queued for retry; already accepted mounts are not replayed.
    pub fn publish(
        &mut self,
        mut host: impl FnMut(&[PresentationNode]) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        self.publish_inner(false, &mut |publication| host(publication.roots()))
    }
    /// Full mounts use a keyed snapshot diff. Validated leaf packets generate
    /// only addressed updates, without cloning or comparing the whole tree.
    /// Host validation, layout and output costs are separate from publication.
    /// Returns accepted SDK mounts, including unchanged trees that skip the
    /// host callback and do not consume a host revision.
    pub fn publish_commits(
        &mut self,
        mut host: impl FnMut(&jadix_core::presentation::CommitBatch) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        let mut revision = self.commit_revision;
        let root = self.lifetime().id();
        let result = self.publish_inner(true, &mut |publication| {
            let batch = match publication {
                Publication::Mount { before, after } => {
                    crate::mount::commits(root, revision, before, after)
                        .map_err(|e| e.to_string())?
                }
                Publication::Leaves { before, after } => CommitBatch {
                    root,
                    previous: revision,
                    revision: revision
                        .checked_add(1)
                        .ok_or_else(|| MountError::Revision.to_string())?,
                    operations: before
                        .iter()
                        .filter_map(|(path, content)| {
                            let node = presentation_at(after, path).expect("validated leaf path");
                            (node.descriptor.content != *content).then(|| Operation::Update {
                                node: node.id,
                                descriptor: node.descriptor.clone(),
                            })
                        })
                        .collect(),
                },
            };
            if !batch.operations.is_empty() {
                host(&batch)?;
                revision = batch.revision;
            }
            Ok(())
        });
        self.commit_revision = revision;
        result
    }
    fn publish_inner(
        &mut self,
        structural: bool,
        host: &mut impl FnMut(Publication<'_>) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        {
            let state = self.state.borrow();
            if state.failed || state.mount.lifetime().is_disposed() {
                return Err(ApplicationError::Session(ShellError::InvalidState));
            }
        }
        if self.structural.is_some_and(|mode| mode != structural) {
            return Err(ApplicationError::Protocol);
        }
        self.structural = Some(structural);
        let mut count = 0;
        loop {
            let mut state = self.state.borrow_mut();
            let Some(pending) = state.pending.front() else {
                break;
            };
            if let Pending::Patch(patches) = pending {
                let patches = state
                    .mount
                    .prepare_leaves(&state.session, &state.registry, patches)
                    .map_err(ApplicationError::Mount)?;
                // Stage and validate all paths before any mutation. A rejecting
                // host restores only changed contents, keeping retry order.
                let before = patches
                    .iter()
                    .map(|p| {
                        presentation_at(&self.roots, &p.path)
                            .filter(|n| n.children.is_empty())
                            .map(|n| (p.path.clone(), n.descriptor.content.clone()))
                            .ok_or(ApplicationError::Protocol)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for p in &patches {
                    presentation_at_mut(&mut self.roots, &p.path)
                        .unwrap()
                        .descriptor
                        .content = p.content.clone();
                }
                if let Err(error) = host(Publication::Leaves {
                    before: &before,
                    after: &self.roots,
                }) {
                    for (path, content) in before {
                        presentation_at_mut(&mut self.roots, &path)
                            .unwrap()
                            .descriptor
                            .content = content;
                    }
                    return Err(ApplicationError::Host(error));
                }
                if let Some(changes) = &mut self.changes {
                    changes.extend(patches.iter().map(|p| p.path.clone()));
                }
                state.mount.accept_leaves(patches);
                state.work.leaf_patches += before.len() as u64;
                state.pending.pop_front();
                count += 1;
                continue;
            }
            let Pending::Mount(envelope) = pending else {
                unreachable!()
            };
            let mut candidate = state
                .mount
                .prepare(&state.session, &state.registry, envelope)
                .map_err(ApplicationError::Mount)?;
            if let Err(error) = host(Publication::Mount {
                before: &self.roots,
                after: &candidate.roots,
            }) {
                return Err(ApplicationError::Host(error));
            }
            let roots = std::mem::take(&mut candidate.roots);
            state
                .mount
                .accept(candidate)
                .map_err(ApplicationError::Mount)?;
            state.pending.pop_front();
            self.roots = roots;
            state.work.full_mounts += 1;
            self.changes = None;
            count += 1;
        }
        Ok(count)
    }
    pub fn events(
        &mut self,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<(), ApplicationError> {
        let events: Vec<_> = events.into_iter().take(1025).collect();
        if events.len() > 1024 {
            return Err(ApplicationError::Protocol);
        }
        let deliveries = {
            let mut state = self.state.borrow_mut();
            state.work.event_lookups += events.len() as u64;
            let State { mount, session, .. } = &mut *state;
            mount
                .events(session, events)
                .map_err(ApplicationError::Mount)?
        };
        for delivery in deliveries {
            delivery.dispatch().map_err(ApplicationError::Session)?;
        }
        loop {
            let next = { self.state.borrow().deliveries.borrow_mut().pop_front() };
            let Some((key, value)) = next else {
                break;
            };
            self.turn(
                &format!(
                    "globalThis.__jadixDispatch({},{})",
                    serde_json::to_string(&key).unwrap(),
                    serde_json::to_string(&value).unwrap()
                ),
                false,
            )?;
        }
        Ok(())
    }
    pub fn dispose(&mut self) {
        self.state.borrow_mut().fault();
        self.roots.clear();
    }
}

pub(crate) fn presentation_at<'a>(
    roots: &'a [PresentationNode],
    path: &[usize],
) -> Option<&'a PresentationNode> {
    let mut node = roots.first()?;
    for i in path {
        node = node.children.get(*i)?;
    }
    Some(node)
}
fn presentation_at_mut<'a>(
    roots: &'a mut [PresentationNode],
    path: &[usize],
) -> Option<&'a mut PresentationNode> {
    let mut node = roots.first_mut()?;
    for i in path {
        node = node.children.get_mut(*i)?;
    }
    Some(node)
}
