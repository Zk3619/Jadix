use std::collections::{BTreeMap, BTreeSet};
use std::{marker::PhantomData, rc::Rc};

pub use jadix_core::presentation;
use jadix_core::{create_root, OwnerHandle};
use presentation::{RootId, RootLifetime};

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
pub mod application;
#[cfg(all(feature = "gpu-host", not(target_arch = "wasm32")))]
pub mod gpu;
pub mod handles;
pub mod mount;
#[cfg(feature = "native-host")]
pub mod native;
#[cfg(all(feature = "native-store", not(target_arch = "wasm32")))]
pub mod ota;
pub mod package;
#[cfg(all(feature = "native-store", not(target_arch = "wasm32")))]
pub mod publication;
pub mod runtime;
pub mod schema;
#[cfg(all(feature = "native-store", not(target_arch = "wasm32")))]
pub mod store;
pub mod wire;
use schema::{MethodContract, Schema, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Created,
    Verified,
    Authorized,
    Loading,
    Active,
    Faulted,
    Disposing,
    Disposed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellError {
    InvalidState,
    IncompatibleProtocol,
    InvalidManifest,
    PermissionDenied,
    UnknownMethod,
    RequestLimit,
    StaleRequest,
    InvalidArguments,
    InvalidResult,
    UnknownMount,
    StaleHandle,
    InvalidHandleKind,
    HandleLimit,
    MountLimit,
    ReentrantCallback,
    InvalidLimits,
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ShellError {}

#[derive(Clone, Debug)]
pub struct PackageManifest {
    pub package: String,
    pub protocol_version: u32,
    pub requested_permissions: BTreeSet<String>,
}

#[derive(Default)]
pub struct HostRegistry {
    methods: BTreeMap<String, MethodContract>,
}

impl HostRegistry {
    pub fn register(&mut self, method: String, permission: String) -> Result<(), ShellError> {
        self.register_contract(
            method,
            MethodContract {
                permission,
                arguments: Schema::Null,
                result: Schema::Null,
            },
        )
    }

    pub fn register_contract(
        &mut self,
        method: String,
        contract: MethodContract,
    ) -> Result<(), ShellError> {
        if method.trim().is_empty()
            || contract.permission.trim().is_empty()
            || self.methods.contains_key(&method)
            || !contract.arguments.is_valid()
            || !contract.result.is_valid()
        {
            return Err(ShellError::InvalidManifest);
        }
        self.methods.insert(method, contract);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId {
    session: RootId,
    sequence: u64,
}

struct PendingRequest {
    result: Schema,
    root: Option<RootId>,
}

pub struct Session {
    ui_thread: PhantomData<Rc<()>>,
    id: RootId,
    state: SessionState,
    owner: Option<OwnerHandle>,
    manifest: Option<PackageManifest>,
    permissions: BTreeSet<String>,
    pending: BTreeMap<RequestId, PendingRequest>,
    roots: BTreeMap<RootId, RootLifetime>,
    next_request: u64,
    request_limit: usize,
    handles: handles::HandleTable,
    limits: runtime::ResourceLimits,
}

impl Session {
    pub fn new(request_limit: usize) -> Self {
        Self::with_limits(runtime::ResourceLimits {
            requests: request_limit.min(1024),
            ..Default::default()
        })
        .expect("bounded session limits")
    }

    pub fn with_limits(limits: runtime::ResourceLimits) -> Result<Self, ShellError> {
        if !limits.is_valid() {
            return Err(ShellError::InvalidLimits);
        }
        let id = RootId::new();
        Ok(Self {
            ui_thread: PhantomData,
            id,
            state: SessionState::Created,
            owner: None,
            manifest: None,
            permissions: BTreeSet::new(),
            pending: BTreeMap::new(),
            roots: BTreeMap::new(),
            next_request: 0,
            request_limit: limits.requests,
            handles: handles::HandleTable::new(id.token(), limits.handles),
            limits,
        })
    }

    pub fn state(&self) -> SessionState {
        self.state
    }
    pub fn token(&self) -> String {
        self.id.token()
    }

    pub fn validate_mount(
        &self,
        envelope: &wire::Envelope,
        registry: &wire::ComponentRegistry,
    ) -> Result<(), ShellError> {
        if !matches!(self.state, SessionState::Loading | SessionState::Active) {
            return Err(ShellError::InvalidState);
        }
        if envelope.session != self.token() || envelope.protocol != presentation::PROTOCOL_VERSION {
            return Err(ShellError::StaleHandle);
        }
        let wire::Command::Mount { root, tree } = &envelope.command else {
            return Err(ShellError::InvalidArguments);
        };
        let lifetime = self
            .roots
            .values()
            .find(|lifetime| !lifetime.is_disposed() && lifetime.id().token() == *root)
            .ok_or(ShellError::UnknownMount)?;
        registry
            .validate_with(tree, |node, contract| {
                for (name, handle) in &node.events {
                    self.handles
                        .check_event(handle, lifetime.id(), &contract.events[name])?;
                }
                Ok(())
            })
            .map_err(|error| match error {
                wire::ValidationError::Wire(_) => ShellError::InvalidArguments,
                wire::ValidationError::Visitor(error) => error,
            })?;
        Ok(())
    }
    pub fn pending_requests(&self) -> usize {
        self.pending
            .values()
            .filter(|request| request.root.is_none_or(|root| self.has_root(root)))
            .count()
    }

    fn has_root(&self, root: RootId) -> bool {
        self.roots
            .get(&root)
            .is_some_and(|lifetime| !lifetime.is_disposed())
    }

    fn prune_roots(&mut self) {
        self.handles.prune();
        self.roots.retain(|_, lifetime| !lifetime.is_disposed());
        self.pending.retain(|_, request| {
            request
                .root
                .is_none_or(|root| self.roots.contains_key(&root))
        });
    }

    pub fn mount_root(&mut self) -> Result<RootLifetime, ShellError> {
        if !matches!(self.state, SessionState::Loading | SessionState::Active) {
            return Err(ShellError::InvalidState);
        }
        self.prune_roots();
        if self.roots.len() >= self.limits.mounts {
            return Err(ShellError::MountLimit);
        }
        let lifetime = RootLifetime::new(RootId::new());
        self.roots.insert(lifetime.id(), lifetime.clone());
        Ok(lifetime)
    }

    pub fn unmount_root(&mut self, root: RootId) -> Result<(), ShellError> {
        if !matches!(self.state, SessionState::Loading | SessionState::Active) {
            return Err(ShellError::InvalidState);
        }
        let lifetime = self.roots.remove(&root).ok_or(ShellError::UnknownMount)?;
        lifetime.dispose();
        self.handles.prune();
        self.pending.retain(|_, request| request.root != Some(root));
        Ok(())
    }

    pub fn verify(&mut self, manifest: PackageManifest) -> Result<(), ShellError> {
        if self.state != SessionState::Created {
            return Err(ShellError::InvalidState);
        }
        if manifest.protocol_version != presentation::PROTOCOL_VERSION {
            return Err(ShellError::IncompatibleProtocol);
        }
        if manifest.package.trim().is_empty()
            || manifest
                .requested_permissions
                .iter()
                .any(|permission| permission.trim().is_empty())
        {
            return Err(ShellError::InvalidManifest);
        }
        self.manifest = Some(manifest);
        self.state = SessionState::Verified;
        Ok(())
    }

    fn handle_root(&self, root: RootId) -> Result<RootLifetime, ShellError> {
        if !matches!(self.state, SessionState::Loading | SessionState::Active) {
            return Err(ShellError::InvalidState);
        }
        self.roots
            .get(&root)
            .filter(|lifetime| !lifetime.is_disposed())
            .cloned()
            .ok_or(ShellError::UnknownMount)
    }

    pub fn export_state(
        &mut self,
        root: RootId,
        signal: jadix_core::Signal<Value>,
        schema: Schema,
    ) -> Result<handles::Handle, ShellError> {
        self.handles.state(self.handle_root(root)?, signal, schema)
    }

    pub fn export_event(
        &mut self,
        root: RootId,
        schema: Schema,
        callback: impl FnMut(Value) + 'static,
    ) -> Result<handles::Handle, ShellError> {
        self.handles
            .event(self.handle_root(root)?, schema, callback)
    }

    pub fn read_state(&self, handle: &handles::Handle) -> Result<Value, ShellError> {
        self.handles.read(handle)
    }
    pub fn write_state(&self, handle: &handles::Handle, value: Value) -> Result<(), ShellError> {
        self.handles.write(handle, value)
    }
    pub fn event_delivery(
        &self,
        handle: &handles::Handle,
        arguments: Value,
    ) -> Result<handles::EventDelivery, ShellError> {
        self.handles.delivery(handle, arguments)
    }
    pub fn release_handle(&mut self, handle: &handles::Handle) -> Result<(), ShellError> {
        self.handles.release(handle)
    }

    pub fn authorize(&mut self, grants: &BTreeSet<String>) -> Result<(), ShellError> {
        if self.state != SessionState::Verified {
            return Err(ShellError::InvalidState);
        }
        let requested = &self.manifest.as_ref().unwrap().requested_permissions;
        if !requested.is_subset(grants) {
            return Err(ShellError::PermissionDenied);
        }
        self.permissions = requested.clone();
        self.state = SessionState::Authorized;
        Ok(())
    }

    pub fn load<T>(
        &mut self,
        build: impl FnOnce() -> Result<T, ShellError>,
    ) -> Result<T, ShellError> {
        if self.state != SessionState::Authorized {
            return Err(ShellError::InvalidState);
        }
        self.state = SessionState::Loading;
        let (owner, output) = create_root(build);
        self.owner = Some(owner);
        match output {
            Ok(output) => Ok(output),
            Err(error) => {
                self.fault()?;
                Err(error)
            }
        }
    }

    pub fn activate(&mut self) -> Result<(), ShellError> {
        if self.state != SessionState::Loading {
            return Err(ShellError::InvalidState);
        }
        self.state = SessionState::Active;
        Ok(())
    }

    pub fn request(
        &mut self,
        registry: &HostRegistry,
        method: &str,
    ) -> Result<RequestId, ShellError> {
        self.request_with(registry, method, &Value::Null)
    }

    pub fn request_with(
        &mut self,
        registry: &HostRegistry,
        method: &str,
        arguments: &Value,
    ) -> Result<RequestId, ShellError> {
        self.request_scoped(registry, None, method, arguments)
    }

    pub fn request_for_root(
        &mut self,
        registry: &HostRegistry,
        root: RootId,
        method: &str,
        arguments: &Value,
    ) -> Result<RequestId, ShellError> {
        self.request_scoped(registry, Some(root), method, arguments)
    }

    fn request_scoped(
        &mut self,
        registry: &HostRegistry,
        root: Option<RootId>,
        method: &str,
        arguments: &Value,
    ) -> Result<RequestId, ShellError> {
        if self.state != SessionState::Active {
            return Err(ShellError::InvalidState);
        }
        self.prune_roots();
        if root.is_some_and(|root| !self.has_root(root)) {
            return Err(ShellError::UnknownMount);
        }
        let contract = registry
            .methods
            .get(method)
            .ok_or(ShellError::UnknownMethod)?;
        if !self.permissions.contains(&contract.permission) {
            return Err(ShellError::PermissionDenied);
        }
        if !contract.arguments.accepts(arguments) {
            return Err(ShellError::InvalidArguments);
        }
        if self.pending.len() >= self.request_limit {
            return Err(ShellError::RequestLimit);
        }
        self.next_request = self
            .next_request
            .checked_add(1)
            .ok_or(ShellError::RequestLimit)?;
        let request = RequestId {
            session: self.id,
            sequence: self.next_request,
        };
        self.pending.insert(
            request,
            PendingRequest {
                result: contract.result.clone(),
                root,
            },
        );
        Ok(request)
    }

    pub fn complete(&mut self, request: RequestId) -> Result<(), ShellError> {
        self.complete_with(request, &Value::Null)
    }

    pub fn complete_with(&mut self, request: RequestId, result: &Value) -> Result<(), ShellError> {
        if self.state != SessionState::Active {
            return Err(ShellError::StaleRequest);
        }
        self.prune_roots();
        let pending = self.pending.get(&request).ok_or(ShellError::StaleRequest)?;
        if !pending.result.accepts(result) {
            return Err(ShellError::InvalidResult);
        }
        self.pending.remove(&request);
        Ok(())
    }

    pub fn cancel(&mut self, request: RequestId) -> Result<(), ShellError> {
        self.prune_roots();
        if self.state != SessionState::Active || self.pending.remove(&request).is_none() {
            return Err(ShellError::StaleRequest);
        }
        Ok(())
    }

    pub fn fault(&mut self) -> Result<(), ShellError> {
        if !matches!(self.state, SessionState::Loading | SessionState::Active) {
            return Err(ShellError::InvalidState);
        }
        self.state = SessionState::Faulted;
        self.revoke();
        Ok(())
    }

    fn revoke(&mut self) {
        for (_, lifetime) in std::mem::take(&mut self.roots) {
            lifetime.dispose();
        }
        self.pending.clear();
        self.handles.clear();
        self.permissions.clear();
        self.owner.take();
    }

    pub fn dispose(&mut self) {
        if self.state == SessionState::Disposed {
            return;
        }
        self.state = SessionState::Disposing;
        self.revoke();
        self.state = SessionState::Disposed;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.dispose();
    }
}
