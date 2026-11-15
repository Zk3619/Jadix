#[cfg(not(windows))]
fn main() {
    eprintln!("native-tsx requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = application::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod application {
    use jadix_native::{presentation::*, windows::WindowsHost};
    use jadix_shell::{
        handles::Handle,
        native::NativeMount,
        package::{PackagePolicy, SignedManifest, Target},
        runtime::native::BudgetedRuntime,
        schema::Value,
        store::{PackageStore, StoredPackage},
        wire::{ComponentRegistry, Envelope},
        HostRegistry, PackageManifest, Session, ShellError,
    };
    use std::{
        cell::{Cell, RefCell},
        collections::{BTreeMap, BTreeSet},
        io::Write,
        os::windows::process::CommandExt,
        rc::Rc,
        time::{Duration, Instant},
    };

    struct WorkspaceAutosave {
        saved: serde_json::Value,
        pending: Option<(serde_json::Value, Duration, Duration)>,
        retry_after: Duration,
    }

    impl WorkspaceAutosave {
        fn new(saved: serde_json::Value) -> Self {
            Self {
                saved,
                pending: None,
                retry_after: Duration::ZERO,
            }
        }

        fn observe(&mut self, value: serde_json::Value, now: Duration) {
            if value == self.saved {
                self.pending = None;
                self.retry_after = Duration::ZERO;
            } else if self
                .pending
                .as_ref()
                .is_none_or(|pending| pending.0 != value)
            {
                let first = self.pending.as_ref().map_or(now, |pending| pending.1);
                let deadline =
                    (now + Duration::from_millis(250)).min(first + Duration::from_secs(2));
                self.pending = Some((value, first, deadline));
            }
        }

        fn wait_ms(&self, now: Duration, composing: bool) -> u32 {
            if composing {
                return u32::MAX;
            }
            self.pending.as_ref().map_or(u32::MAX, |pending| {
                let remaining = pending.2.max(self.retry_after).saturating_sub(now);
                remaining
                    .as_millis()
                    .saturating_add(u128::from(remaining.subsec_nanos() % 1_000_000 != 0))
                    .min(u128::from(u32::MAX - 1)) as u32
            })
        }

        fn save_due(
            &mut self,
            now: Duration,
            composing: bool,
            persist: impl FnOnce(&serde_json::Value) -> Result<(), String>,
        ) -> Result<bool, String> {
            if self.wait_ms(now, composing) != 0 {
                return Ok(false);
            }
            let value = &self.pending.as_ref().ok_or("missing pending checkpoint")?.0;
            if let Err(error) = persist(value) {
                self.retry_after = now + Duration::from_secs(5);
                return Err(error);
            }
            self.saved = self.pending.take().unwrap().0;
            self.retry_after = Duration::ZERO;
            Ok(true)
        }
    }

    struct App {
        session: Session,
        mount: NativeMount<WindowsHost>,
        registry: ComponentRegistry,
        vm: Rc<BudgetedRuntime>,
        failure: Rc<RefCell<Option<String>>>,
        composing: Rc<Cell<bool>>,
        configuration: serde_json::Value,
        activate: Handle,
        viewport: [f32; 2],
        environment: u64,
        bundle: serde_json::Value,
        release: u64,
        storage_version: u32,
    }

    impl App {
        fn new() -> Result<Self, String> {
            Self::load(1, &serde_json::Value::Null)
        }

        fn development_package(release: u64) -> Result<StoredPackage, String> {
            let bundle = include_str!("../shell/panel.bundle.js");
            let metadata: serde_json::Value =
                serde_json::from_str(include_str!("../shell/panel.bundle.json"))
                    .map_err(|error| error.to_string())?;
            if metadata["bundle_sha256"] != jadix_shell::package::sha256(bundle.as_bytes())
                || metadata["sources"]["panel.tsx"]
                    != jadix_shell::package::sha256(include_bytes!("../shell/panel.tsx"))
            {
                return Err("TSX source and bundle differ; run node examples/native-controls/shell/build.mjs".into());
            }
            let packages: serde_json::Value =
                serde_json::from_str(include_str!("../shell/development-packages.json"))
                    .map_err(|error| error.to_string())?;
            let signed = SignedManifest::decode(
                &serde_json::to_vec(&packages["releases"][release.to_string()])
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("{error:?}"))?;
            let source = if release == 3 {
                packages["broken_source"]
                    .as_str()
                    .ok_or("missing broken update fixture")?
            } else {
                bundle
            };
            let resources =
                BTreeMap::from([("panel.bundle.js".into(), source.as_bytes().to_vec())]);
            let identity = jadix_shell::package::sha256(
                &serde_json::to_vec(&signed).map_err(|error| error.to_string())?,
            );
            Ok(StoredPackage {
                identity,
                signed,
                resources,
            })
        }

        fn policy() -> Result<PackagePolicy, String> {
            let schema = jadix_shell::package::sha256(
                &[
                    include_bytes!("../../../crates/jadix_components/schema/portable-v1.json")
                        .as_slice(),
                    &[0],
                    include_bytes!("../../../crates/jadix_components/schema/extended-v1.json")
                        .as_slice(),
                ]
                .concat(),
            );
            let key = jadix_shell::package::decode_hex::<32>(
                "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c",
            )
            .ok_or("invalid development public key")?;
            Ok(PackagePolicy {
                package: "examples.native.tsx".into(),
                publisher: "jadix-development".into(),
                target: Target::Native,
                host_version: "0.1.0".into(),
                minimum_release: 1,
                component_schema: schema,
                capabilities: BTreeSet::new(),
                permissions: BTreeSet::from(["panel.read".into()]),
                keys: BTreeMap::from([("development-key".into(), key)]),
                revoked_keys: BTreeSet::new(),
            })
        }

        fn load(release: u64, restored: &serde_json::Value) -> Result<Self, String> {
            Self::load_stored(Self::development_package(release)?, restored)
        }

        fn load_stored(
            package: StoredPackage,
            restored: &serde_json::Value,
        ) -> Result<Self, String> {
            let StoredPackage {
                signed, resources, ..
            } = package;
            let policy = Self::policy()?;
            signed
                .verify(&policy, &resources)
                .map_err(|error| format!("package verification failed: {error:?}"))?;
            let source = std::str::from_utf8(
                resources
                    .get(&signed.manifest.entry)
                    .ok_or("missing stored entry")?,
            )
            .map_err(|error| error.to_string())?;
            let release = signed
                .manifest
                .release
                .parse::<u64>()
                .map_err(|error| error.to_string())?;
            let storage_version = signed.manifest.storage_version;
            let metadata: serde_json::Value =
                serde_json::from_str(include_str!("../shell/panel.bundle.json"))
                    .map_err(|error| error.to_string())?;
            let vm = Rc::new(
                BudgetedRuntime::new(Default::default()).map_err(|error| format!("{error:?}"))?,
            );
            let mut session = Session::new(8);
            session
                .verify(PackageManifest {
                    package: signed.manifest.package.clone(),
                    protocol_version: signed.manifest.protocol,
                    requested_permissions: signed.manifest.permissions.keys().cloned().collect(),
                })
                .map_err(|error| error.to_string())?;
            session
                .authorize(&BTreeSet::from(["panel.read".into()]))
                .map_err(|error| error.to_string())?;
            session
                .load(|| {
                    vm.eval_string(&format!(
                        "{source}\nJSON.stringify(NativePanel.initialize({restored}, {storage_version}));"
                    ))
                    .map(|_| ())
                    .map_err(|_| ShellError::InvalidArguments)
                })
                .map_err(|error| error.to_string())?;
            let mut registry = ComponentRegistry::default();
            for source in [
                include_str!("../../../crates/jadix_components/schema/portable-v1.json"),
                include_str!("../../../crates/jadix_components/schema/extended-v1.json"),
            ] {
                let schemas: Vec<component::ComponentSchema> =
                    serde_json::from_str(source).map_err(|error| error.to_string())?;
                for schema in schemas {
                    registry
                        .register(schema)
                        .map_err(|error| format!("{error:?}"))?;
                }
            }
            let host = WindowsHost::new("Jadix Native TSX panel", [900, 420])?;
            let viewport = host.viewport();
            let mount = NativeMount::new(&mut session, host, Default::default())
                .map_err(|error| error.to_string())?;
            let failure = Rc::new(RefCell::new(None));
            let composing = Rc::new(Cell::new(false));
            let mut handles = Vec::new();
            for (component, event) in [
                ("Input", "edit"),
                ("Button", "activate"),
                ("PopoverContent", "dismiss"),
            ] {
                let vm = vm.clone();
                let failure = failure.clone();
                let composing = composing.clone();
                let schema = registry
                    .get(component)
                    .ok_or("missing component contract")?
                    .events[event]
                    .clone();
                let handle = session
                    .export_event(mount.root().model().id(), schema, move |arguments| {
                        if event == "edit" {
                            composing.set(matches!(&arguments, Value::Object(fields) if fields.contains_key("composing")));
                        }
                        let result = serde_json::to_string(&arguments)
                            .map_err(|error| error.to_string())
                            .and_then(|arguments| {
                                vm.eval_string(&format!(
                                    "JSON.stringify(NativePanel.event({event:?}, {arguments}))"
                                ))
                                .map_err(|error| format!("TSX event failed: {error:?}"))
                            });
                        if let Err(error) = result {
                            *failure.borrow_mut() = Some(error);
                        }
                    })
                    .map_err(|error| error.to_string())?;
                handles.push(handle);
            }
            let edit = handles.remove(0);
            let activate = handles.remove(0);
            let dismiss = handles.remove(0);
            let configuration = serde_json::json!({"session":session.token(),"root":mount.root().model().id().token(),"edit":edit,"activate":activate,"dismiss":dismiss});
            session.activate().map_err(|error| error.to_string())?;
            let mut app = Self {
                session,
                mount,
                registry,
                vm,
                failure,
                composing,
                configuration,
                activate,
                viewport,
                environment: 0,
                bundle: metadata,
                release,
                storage_version,
            };
            app.render()?;
            app.mount.root().host().show();
            app.mount.root().host().pump();
            app.poll()?;
            Ok(app)
        }

        fn render(&mut self) -> Result<(), String> {
            let source = format!("JSON.stringify(NativePanel.mount({}))", self.configuration);
            let json = self
                .vm
                .eval_string(&source)
                .map_err(|error| format!("{error:?}"))?;
            let envelope = Envelope::decode(json.as_bytes(), &self.registry)
                .map_err(|error| format!("{error:?}"))?;
            self.mount
                .present(
                    &self.session,
                    &self.registry,
                    &envelope,
                    self.viewport,
                    self.environment,
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        }

        fn poll(&mut self) -> Result<bool, String> {
            let deliveries = self
                .mount
                .poll(&self.session)
                .map_err(|error| error.to_string())?;
            let changed = !deliveries.is_empty();
            for delivery in deliveries {
                delivery.dispatch().map_err(|error| error.to_string())?;
                if let Some(error) = self.failure.borrow_mut().take() {
                    self.session.fault().map_err(|error| error.to_string())?;
                    self.mount.dispose();
                    return Err(error);
                }
            }
            self.vm.drain_jobs().map_err(|error| format!("{error:?}"))?;
            if changed {
                self.render()?;
            }
            Ok(changed)
        }

        fn state(&self) -> Result<serde_json::Value, String> {
            let json = self
                .vm
                .eval_string("JSON.stringify(NativePanel.snapshot())")
                .map_err(|error| format!("{error:?}"))?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        }

        fn checkpoint_close(
            &self,
            store: &mut PackageStore,
            policy: &PackagePolicy,
        ) -> Result<(), String> {
            let saved = if self.composing.get() {
                Err("Finish text composition before closing the workspace".into())
            } else {
                self.checkpoint().and_then(|state| {
                    store
                        .save_business(policy, &state)
                        .map_err(|error| error.to_string())
                })
            };
            self.mount.root().host().resolve_close(saved.is_ok())?;
            saved
        }

        fn checkpoint(&self) -> Result<serde_json::Value, String> {
            let json = self
                .vm
                .eval_string(&format!(
                    "JSON.stringify(NativePanel.checkpoint({}))",
                    self.storage_version
                ))
                .map_err(|error| format!("{error:?}"))?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        }

        fn migrate_business(
            package: &StoredPackage,
            from: u32,
            to: u32,
            value: &serde_json::Value,
        ) -> Result<serde_json::Value, jadix_shell::store::StoreError> {
            use jadix_shell::store::StoreError;
            let policy = Self::policy().map_err(|_| StoreError::Migration)?;
            package
                .signed
                .verify(&policy, &package.resources)
                .map_err(|_| StoreError::Migration)?;
            let source = std::str::from_utf8(
                package
                    .resources
                    .get(&package.signed.manifest.entry)
                    .ok_or(StoreError::Migration)?,
            )
            .map_err(|_| StoreError::Migration)?;
            let vm = BudgetedRuntime::new(Default::default()).map_err(|_| StoreError::Migration)?;
            let result = vm
                .eval_string(&format!(
                    "{source}\nJSON.stringify(NativePanel.migrateStorage({from}, {to}, {value}));"
                ))
                .map_err(|_| StoreError::Migration)?;
            serde_json::from_str(&result).map_err(|_| StoreError::Migration)
        }
    }

    fn child_run(arguments: &[&std::ffi::OsStr], expected: i32) -> Result<(), String> {
        let mut child =
            std::process::Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
                .args(arguments)
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|error| error.to_string())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return if status.code() == Some(expected) {
                    Ok(())
                } else {
                    Err(format!("native TSX recovery process failed: {status}"))
                };
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(String::from("native TSX recovery process timed out"));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn write_report(path: &std::path::Path, report: &serde_json::Value) -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        writeln!(
            file,
            "{}",
            serde_json::to_string_pretty(report).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn autosave_coalesces_edits_without_starving_or_waking_idle() {
            let mut autosave = WorkspaceAutosave::new(serde_json::json!(0));
            assert_eq!(autosave.wait_ms(Duration::ZERO, false), u32::MAX);
            for index in 0..20 {
                autosave.observe(
                    serde_json::json!(index + 1),
                    Duration::from_millis(index * 100),
                );
            }
            assert_eq!(autosave.wait_ms(Duration::from_millis(1999), false), 1);
            let mut written = serde_json::Value::Null;
            assert!(autosave
                .save_due(Duration::from_secs(2), false, |value| {
                    written = value.clone();
                    Ok(())
                })
                .unwrap());
            assert_eq!(written, serde_json::json!(20));
            assert_eq!(autosave.wait_ms(Duration::from_secs(3), false), u32::MAX);
            autosave.observe(written, Duration::from_secs(4));
            assert!(!autosave
                .save_due(Duration::from_secs(5), false, |_| panic!("idle write"))
                .unwrap());
        }

        #[test]
        fn autosave_defers_composition_and_preserves_failed_latest_checkpoint() {
            let mut autosave = WorkspaceAutosave::new(serde_json::json!("old"));
            autosave.observe(serde_json::json!("committed"), Duration::ZERO);
            assert_eq!(autosave.wait_ms(Duration::from_secs(1), true), u32::MAX);
            assert!(!autosave
                .save_due(Duration::from_secs(1), true, |_| panic!(
                    "composition write"
                ))
                .unwrap());
            assert!(autosave
                .save_due(Duration::from_secs(1), false, |_| Err(
                    "disk unavailable".into()
                ))
                .is_err());
            autosave.observe(serde_json::json!("最新😀"), Duration::from_secs(2));
            assert_eq!(autosave.wait_ms(Duration::from_secs(2), false), 4000);
            assert!(!autosave
                .save_due(Duration::from_secs(3), false, |_| panic!("early retry"))
                .unwrap());
            assert!(autosave
                .save_due(Duration::from_secs(6), false, |value| {
                    assert_eq!(value, "最新😀");
                    Ok(())
                })
                .unwrap());
        }

        #[test]
        fn autosave_undo_to_saved_state_cancels_pending_retry() {
            let mut autosave = WorkspaceAutosave::new(serde_json::json!({"count":0}));
            autosave.observe(serde_json::json!({"count":1}), Duration::ZERO);
            assert!(autosave
                .save_due(Duration::from_secs(1), false, |_| Err("failure".into()))
                .is_err());
            autosave.observe(serde_json::json!({"count":0}), Duration::from_secs(2));
            assert_eq!(autosave.wait_ms(Duration::from_secs(6), false), u32::MAX);
            assert!(!autosave
                .save_due(Duration::from_secs(6), false, |_| panic!("reverted write"))
                .unwrap());
        }
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        if let [mode, directory] = arguments.as_slice() {
            if mode == "--crash-store" {
                let policy = App::policy()?;
                let mut store =
                    PackageStore::open(directory, &policy).map_err(|error| error.to_string())?;
                let package = App::development_package(3)?;
                store
                    .stage(&package.signed, &package.resources, &policy)
                    .map_err(|error| error.to_string())?;
                std::process::exit(73);
            }
        }
        if let [mode, directory, nonce, output] = arguments.as_slice() {
            if mode == "--autosave-crash" || mode == "--journal-crash" {
                let policy = App::policy()?;
                let mut store =
                    PackageStore::open(directory, &policy).map_err(|error| error.to_string())?;
                let package = App::development_package(1)?;
                let activation = store
                    .stage(&package.signed, &package.resources, &policy)
                    .map_err(|error| error.to_string())?;
                let mut app = App::load_stored(package, &serde_json::Value::Null)?;
                store
                    .accept_with_business(&activation, &policy, &app.checkpoint()?)
                    .map_err(|error| error.to_string())?;
                let mut autosave = WorkspaceAutosave::new(app.checkpoint()?);
                let editor = app
                    .mount
                    .node_at(&[1, 0])
                    .ok_or("missing autosave editor")?;
                let window = app
                    .mount
                    .root()
                    .host()
                    .control_identity(editor)
                    .ok_or("missing autosave HWND")?;
                let origin = Instant::now();
                app.mount.root().host().select_range(editor, [0, 4])?;
                app.mount
                    .root()
                    .host()
                    .replace_selection(editor, "自动保存后退出😀")?;
                if !app.poll()? {
                    return Err("autosave did not observe the native edit".into());
                }
                autosave.observe(app.checkpoint()?, origin.elapsed());
                let commits = app.mount.root().host().stats().commits;
                if mode == "--journal-crash" {
                    let package = store
                        .current(&policy)
                        .map_err(|error| error.to_string())?
                        .ok_or("missing active journal package")?;
                    let checkpoint = std::path::Path::new(directory)
                        .join("checkpoints")
                        .join(format!("{}.json", package.identity));
                    std::fs::remove_file(&checkpoint).map_err(|error| error.to_string())?;
                    std::fs::create_dir(&checkpoint).map_err(|error| error.to_string())?;
                    if !matches!(
                        store.save_business(&policy, &app.checkpoint()?),
                        Err(jadix_shell::store::StoreError::Format)
                    ) || !std::path::Path::new(directory)
                        .join("business-journal.json")
                        .is_file()
                        || app.mount.root().host().control_identity(editor) != Some(window)
                        || app.mount.root().host().stats().commits != commits
                    {
                        return Err(
                            "failed native checkpoint did not retain its recovery journal".into(),
                        );
                    }
                    write_report(
                        std::path::Path::new(output),
                        &serde_json::json!({
                            "nonce":nonce, "process":std::process::id(), "business_state":app.state()?,
                            "checkpoint_replacement_failed":true, "durable_journal_retained":true,
                            "editor_retained":true, "normal_close_executed":false,
                            "power_loss_accepted":false
                        }),
                    )?;
                    std::process::exit(76);
                }
                loop {
                    app.mount.root().host().pump();
                    if app.poll()? && !app.composing.get() {
                        autosave.observe(app.checkpoint()?, origin.elapsed());
                    }
                    if autosave.save_due(origin.elapsed(), app.composing.get(), |value| {
                        store
                            .save_business(&policy, value)
                            .map_err(|error| error.to_string())
                    })? {
                        break;
                    }
                    if origin.elapsed() > Duration::from_secs(3) {
                        return Err("automatic checkpoint did not reach its deadline".into());
                    }
                    app.mount
                        .root()
                        .host()
                        .wait_for_events(autosave.wait_ms(origin.elapsed(), app.composing.get()))?;
                }
                if app.mount.root().host().stats().commits != commits
                    || app.mount.root().host().control_identity(editor) != Some(window)
                    || autosave.wait_ms(origin.elapsed(), false) != u32::MAX
                {
                    return Err(
                        "autosave replaced the editor, committed UI or kept an idle timer".into(),
                    );
                }
                write_report(
                    std::path::Path::new(output),
                    &serde_json::json!({
                        "nonce":nonce, "process":std::process::id(), "business_state":app.state()?,
                        "storage_checkpoint":app.checkpoint()?, "automatic_checkpoint_saved":true,
                        "idle_native_commits":0, "editor_retained":true, "idle_timer_disarmed":true,
                        "elapsed_ms":origin.elapsed().as_millis(), "normal_close_executed":false
                    }),
                )?;
                std::process::exit(74);
            }
            if mode == "--recover-store" {
                let policy = App::policy()?;
                let store =
                    PackageStore::open(directory, &policy).map_err(|error| error.to_string())?;
                let package = store
                    .current(&policy)
                    .map_err(|error| error.to_string())?
                    .ok_or("no stable stored TSX package")?;
                let identity = package.identity.clone();
                let restored = store
                    .business(&policy)
                    .map_err(|error| error.to_string())?
                    .unwrap_or(serde_json::Value::Null);
                let app = App::load_stored(package, &restored)?;
                let editor = [0, 1]
                    .into_iter()
                    .filter_map(|index| app.mount.node_at(&[1, index]))
                    .find(|node| {
                        app.mount.root().model().node(*node).is_some_and(|node| {
                            matches!(node.descriptor.content, NodeContent::Input { .. })
                        })
                    })
                    .ok_or("missing recovered TSX editor")?;
                if app.mount.root().host().text(editor)?
                    != app.state()?["draft"]
                        .as_str()
                        .ok_or("invalid recovered draft")?
                {
                    return Err(
                        "recovered native text differs from persistent business state".into(),
                    );
                }
                let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true, "process": std::process::id(),
                    "release": app.release, "identity": identity, "pending_candidate_recovered": store.recovered_pending(),
                    "business_journal_recovered": store.recovered_business(),
                    "highest_release": store.highest_release(), "storage_version": app.storage_version,
                    "business_state": app.state()?, "storage_checkpoint": app.checkpoint()?, "native": app.mount.root().host().diagnostics()});
                return write_report(std::path::Path::new(output), &report);
            }
        }
        let probe = match arguments.as_slice() {
            [] => None,
            [mode, _] if mode == "--workspace" => None,
            [mode, nonce, output]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                Some((nonce, std::path::Path::new(output)))
            }
            _ => {
                return Err(
                    "Usage: native-tsx [--workspace DIRECTORY | --probe NONCE NEW_REPORT_JSON]"
                        .into(),
                );
            }
        };
        if probe.is_some_and(|(_, output)| output.exists()) {
            return Err("probe output already exists".into());
        }
        let policy = App::policy()?;
        let workspace = match arguments.as_slice() {
            [mode, path] if mode == "--workspace" => Some(std::path::Path::new(path)),
            _ => None,
        };
        let mut workspace_store = workspace
            .map(|directory| {
                PackageStore::open(directory, &policy).map_err(|error| error.to_string())
            })
            .transpose()?;
        let mut app = if let Some(store) = &mut workspace_store {
            if let Some(package) = store.current(&policy).map_err(|error| error.to_string())? {
                let restored = store
                    .business(&policy)
                    .map_err(|error| error.to_string())?
                    .unwrap_or(serde_json::Value::Null);
                App::load_stored(package, &restored)?
            } else {
                let package = App::development_package(1)?;
                let activation = store
                    .stage(&package.signed, &package.resources, &policy)
                    .map_err(|error| error.to_string())?;
                let app = App::load_stored(
                    store
                        .candidate(&activation, &policy)
                        .map_err(|error| error.to_string())?,
                    &serde_json::Value::Null,
                )?;
                store
                    .accept_with_business(&activation, &policy, &app.checkpoint()?)
                    .map_err(|error| error.to_string())?;
                app
            }
        } else {
            App::new()?
        };
        if workspace_store.is_some() {
            app.mount.root().host().defer_close()?;
        }
        if let Some((nonce, output)) = probe {
            let directory = output
                .parent()
                .ok_or("probe report requires a parent directory")?
                .join("tsx-store");
            let mut store =
                PackageStore::open(&directory, &policy).map_err(|error| error.to_string())?;
            let package = App::development_package(1)?;
            let initial = store
                .stage(&package.signed, &package.resources, &policy)
                .map_err(|error| error.to_string())?;
            store
                .accept(&initial, &policy)
                .map_err(|error| error.to_string())?;
            let editor = app.mount.node_at(&[1, 0]).ok_or("missing TSX editor")?;
            let button = app.mount.node_at(&[1, 1]).ok_or("missing TSX button")?;
            let editor_identity = app
                .mount
                .root()
                .host()
                .control_identity(editor)
                .ok_or("missing native EDIT")?;
            app.mount.root().host().select_range(editor, [0, 4])?;
            app.mount
                .root()
                .host()
                .replace_selection(editor, "TSX 实际输入😀")?;
            app.poll()?;
            if app.state()?["draft"] != "TSX 实际输入😀" {
                return Err("native edit did not reach the TSX business state".into());
            }
            app.mount.root().host().activate(button)?;
            app.poll()?;
            if app.state()?["count"] != 1
                || app.mount.node_at(&[1, 1]) != Some(editor)
                || app.mount.root().host().control_identity(editor) != Some(editor_identity)
            {
                return Err("keyed TSX update replaced the native editor".into());
            }
            app.mount.root().host().undo(editor)?;
            app.poll()?;
            if app.state()?["draft"] != "中文😀" {
                return Err("native undo did not survive keyed movement".into());
            }
            let panel = app
                .mount
                .node_at(&[4])
                .ok_or("missing TSX native Popover")?;
            if !matches!(app.mount.root().model().node(panel).ok_or("unmounted TSX Popover")?.descriptor.content, NodeContent::Popover { anchor, open: true, .. } if anchor == button)
            {
                return Err("TSX Popover did not track its reordered keyed native anchor".into());
            }
            let panel_window = app
                .mount
                .root()
                .host()
                .control_identity(panel)
                .ok_or("missing native Popover window")?;
            if unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(panel_window as _)
            } == 0
            {
                return Err("TSX Popover native window did not open".into());
            }
            app.mount.root().host().dismiss(panel, false)?;
            app.poll()?;
            if !matches!(
                app.mount
                    .root()
                    .model()
                    .node(panel)
                    .ok_or("dismiss replaced TSX Popover")?
                    .descriptor
                    .content,
                NodeContent::Popover { open: false, .. }
            ) || unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(panel_window as _)
            } != 0
            {
                return Err("native Popover dismissal did not reach TSX controlled state".into());
            }
            let before_idle = app.mount.root().model().revision();
            app.render()?;
            app.render()?;
            if app.mount.root().model().revision() != before_idle {
                return Err("idle TSX render published native mutations".into());
            }
            let mut registry = HostRegistry::default();
            registry
                .register("panel.read".into(), "panel.read".into())
                .map_err(|error| error.to_string())?;
            registry
                .register("panel.save".into(), "panel.save".into())
                .map_err(|error| error.to_string())?;
            let root = app.mount.root().model().id();
            if app
                .session
                .request_for_root(&registry, root, "panel.save", &Value::Null)
                != Err(ShellError::PermissionDenied)
            {
                return Err("ungranted host method was callable".into());
            }
            let pending = app
                .session
                .request_for_root(&registry, root, "panel.read", &Value::Null)
                .map_err(|error| error.to_string())?;
            let delayed = app
                .session
                .event_delivery(&app.activate, Value::Null)
                .map_err(|error| error.to_string())?;
            let state_before_update = app.state()?;
            store
                .save_business(&policy, &app.checkpoint()?)
                .map_err(|error| error.to_string())?;
            let package = App::development_package(3)?;
            let failed = store
                .stage(&package.signed, &package.resources, &policy)
                .map_err(|error| error.to_string())?;
            if App::load_stored(
                store
                    .candidate(&failed, &policy)
                    .map_err(|error| error.to_string())?,
                &state_before_update,
            )
            .is_ok()
                || app.state()? != state_before_update
                || app.mount.root().host().control_identity(editor) != Some(editor_identity)
            {
                return Err("failed signed update changed the active native panel".into());
            }
            store.reject(&failed).map_err(|error| error.to_string())?;
            let package = App::development_package(2)?;
            let update = store
                .stage(&package.signed, &package.resources, &policy)
                .map_err(|error| error.to_string())?;
            if store
                .candidate_business(&update, &policy, |_, _, _| {
                    Err(jadix_shell::store::StoreError::Migration)
                })
                .is_ok()
                || store
                    .current(&policy)
                    .map_err(|error| error.to_string())?
                    .ok_or("missing stable package")?
                    .signed
                    .manifest
                    .release
                    != "1"
                || app.state()? != state_before_update
            {
                return Err("failed business migration changed the active product".into());
            }
            let restored = store
                .candidate_business(&update, &policy, |from, to, value| {
                    App::migrate_business(&package, from, to, value)
                })
                .map_err(|error| error.to_string())?
                .unwrap_or(serde_json::Value::Null);
            let candidate = App::load_stored(
                store
                    .candidate(&update, &policy)
                    .map_err(|error| error.to_string())?,
                &restored,
            )?;
            if candidate.state()? != state_before_update
                || candidate.release != 2
                || candidate.storage_version != 2
                || restored
                    != serde_json::json!({"profile": {"name": state_before_update["draft"]}, "counter": state_before_update["count"]})
            {
                return Err("signed update lost the business snapshot".into());
            }
            store
                .accept_with_business(&update, &policy, &candidate.checkpoint()?)
                .map_err(|error| error.to_string())?;
            let mut previous = std::mem::replace(&mut app, candidate);
            previous.session.dispose();
            let late_event_rejected = delayed.dispatch().is_err();
            let late_request_rejected =
                previous.session.complete(pending) == Err(ShellError::StaleRequest);
            previous.mount.dispose();
            drop(previous);
            drop(store);
            let migrated_recovery_path = output
                .parent()
                .unwrap()
                .join("tsx-store-migrated-recovery.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--recover-store"),
                    directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    migrated_recovery_path.as_os_str(),
                ],
                0,
            )?;
            let migrated_recovery: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&migrated_recovery_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if migrated_recovery["nonce"] != *nonce
                || migrated_recovery["release"] != 2
                || migrated_recovery["storage_version"] != 2
                || migrated_recovery["business_state"] != state_before_update
                || migrated_recovery["storage_checkpoint"] != restored
                || migrated_recovery["process"] == std::process::id()
            {
                return Err(
                    "another process could not restore migrated schema-two business data".into(),
                );
            }
            let mut store =
                PackageStore::open(&directory, &policy).map_err(|error| error.to_string())?;
            let rollback_business = store
                .previous_business(&policy)
                .map_err(|error| error.to_string())?
                .unwrap_or(serde_json::Value::Null);
            let rollback = App::load_stored(
                store
                    .previous(&policy)
                    .map_err(|error| error.to_string())?
                    .ok_or("no previous native TSX package")?,
                &rollback_business,
            )?;
            if rollback.state()? != state_before_update || rollback.release != 1 {
                return Err("explicit rollback lost the business snapshot".into());
            }
            store.rollback(&policy).map_err(|error| error.to_string())?;
            drop(std::mem::replace(&mut app, rollback));
            let state = app.state()?;
            let native = app.mount.root().host().diagnostics();
            let inspection = app.mount.root().inspection(false);
            app.mount.root().host().defer_close()?;
            let identity = store
                .current(&policy)
                .map_err(|error| error.to_string())?
                .ok_or("missing active package before checkpoint close")?
                .identity;
            let checkpoint = directory
                .join("checkpoints")
                .join(format!("{identity}.json"));
            let backup = checkpoint.with_extension("saved");
            std::fs::rename(&checkpoint, &backup).map_err(|error| error.to_string())?;
            std::fs::create_dir(&checkpoint).map_err(|error| error.to_string())?;
            app.mount.root().host().request_close()?;
            app.mount.root().host().pump();
            app.poll()?;
            let close_failed = app.mount.root().host().close_requested()
                && app.checkpoint_close(&mut store, &policy).is_err()
                && !app.mount.root().host().is_closed();
            std::fs::remove_dir(&checkpoint).map_err(|error| error.to_string())?;
            std::fs::rename(&backup, &checkpoint).map_err(|error| error.to_string())?;
            if !close_failed || app.state()? != state {
                return Err("checkpoint failure closed the workspace or lost its state".into());
            }
            app.mount.root().host().request_close()?;
            app.mount.root().host().pump();
            app.poll()?;
            if !app.mount.root().host().close_requested() {
                return Err("workspace close request was not delivered".into());
            }
            app.checkpoint_close(&mut store, &policy)?;
            if !app.mount.root().host().is_closed()
                || store
                    .business(&policy)
                    .map_err(|error| error.to_string())?
                    .as_ref()
                    != Some(&state)
            {
                return Err("workspace closed without its business checkpoint".into());
            }
            app.session.dispose();
            let root_disposed = app.mount.poll(&app.session).is_err()
                && app.mount.root().model().lifetime().is_disposed();
            if !late_event_rejected || !late_request_rejected || !root_disposed {
                return Err("session revocation left a native capability live".into());
            }
            drop(store);
            child_run(
                &[std::ffi::OsStr::new("--crash-store"), directory.as_os_str()],
                73,
            )?;
            let recovery_path = output.parent().unwrap().join("tsx-store-recovery.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--recover-store"),
                    directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    recovery_path.as_os_str(),
                ],
                0,
            )?;
            let recovery: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&recovery_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if recovery["nonce"] != *nonce
                || recovery["release"] != 1
                || recovery["highest_release"] != 2
                || recovery["pending_candidate_recovered"] != true
                || recovery["process"] == std::process::id()
                || recovery["business_state"] != state
            {
                return Err(
                    "independent native TSX process did not recover the stable signed activation"
                        .into(),
                );
            }
            let autosave_directory = output.parent().unwrap().join("tsx-autosave-store");
            let autosave_path = output.parent().unwrap().join("tsx-autosave-exit.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--autosave-crash"),
                    autosave_directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    autosave_path.as_os_str(),
                ],
                74,
            )?;
            let autosave_recovery_path =
                output.parent().unwrap().join("tsx-autosave-recovery.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--recover-store"),
                    autosave_directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    autosave_recovery_path.as_os_str(),
                ],
                0,
            )?;
            let autosave_exit: serde_json::Value = serde_json::from_slice(
                &std::fs::read(autosave_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let autosave_recovery: serde_json::Value = serde_json::from_slice(
                &std::fs::read(autosave_recovery_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if autosave_exit["nonce"] != *nonce
                || autosave_recovery["nonce"] != *nonce
                || autosave_exit["business_state"]["draft"] != "自动保存后退出😀"
                || autosave_exit["business_state"] != autosave_recovery["business_state"]
                || autosave_exit["storage_checkpoint"] != autosave_recovery["storage_checkpoint"]
                || autosave_exit["process"] == autosave_recovery["process"]
                || autosave_recovery["process"] == std::process::id()
            {
                return Err("automatic checkpoint did not survive independent process exit".into());
            }
            let journal_directory = output.parent().unwrap().join("tsx-journal-store");
            let journal_exit_path = output.parent().unwrap().join("tsx-journal-exit.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--journal-crash"),
                    journal_directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    journal_exit_path.as_os_str(),
                ],
                76,
            )?;
            let journal: serde_json::Value = serde_json::from_slice(
                &std::fs::read(journal_directory.join("business-journal.json"))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let identity = journal["checkpoint"]["identity"]
                .as_str()
                .ok_or("missing journal identity")?;
            if identity.len() != 64 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("invalid journal identity".into());
            }
            std::fs::remove_dir(
                journal_directory
                    .join("checkpoints")
                    .join(format!("{identity}.json")),
            )
            .map_err(|error| error.to_string())?;
            let journal_recovery_path = output.parent().unwrap().join("tsx-journal-recovery.json");
            child_run(
                &[
                    std::ffi::OsStr::new("--recover-store"),
                    journal_directory.as_os_str(),
                    std::ffi::OsStr::new(nonce),
                    journal_recovery_path.as_os_str(),
                ],
                0,
            )?;
            let journal_exit: serde_json::Value = serde_json::from_slice(
                &std::fs::read(journal_exit_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let journal_recovery: serde_json::Value = serde_json::from_slice(
                &std::fs::read(journal_recovery_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if journal_exit["nonce"] != *nonce
                || journal_recovery["nonce"] != *nonce
                || journal_exit["checkpoint_replacement_failed"] != true
                || journal_exit["durable_journal_retained"] != true
                || journal_recovery["business_journal_recovered"] != true
                || journal_recovery["business_state"] != journal_exit["business_state"]
                || journal_recovery["business_state"]["draft"] != "自动保存后退出😀"
                || journal_exit["process"] == journal_recovery["process"]
                || journal_recovery["process"] == std::process::id()
                || journal_directory.join("business-journal.json").exists()
            {
                return Err("native EDIT did not recover the failed write from its journal".into());
            }
            let mut report = serde_json::json!({"schema":1,"nonce":nonce,"native_controls":true,"application":"native-tsx", "compiler":app.bundle,"vm":"QuickJS 0.14.0 Rust binding","business_state":state,"native":native,"inspection":inspection,
                "native_edit_to_tsx":true,"keyed_native_editor_retained":true,"native_undo_retained":true,"keyed_popover_anchor":true,"native_popover_dismiss_to_tsx":true,"idle_native_commits":0,"permission_denied":true,"late_event_rejected":late_event_rejected,"late_request_rejected":late_request_rejected,"session_root_disposed":root_disposed,
                "signed_plugin_loading":true,"development_key_only":true,"release_history":[1,2,1],"failed_signed_update_preserved_active_panel":true,"explicit_rollback_restored_business_state":true,"persistent_activation":true,"persistent_business_state":true,"rollback_previous_version_checkpoint":true,"schema_migration_exercised":true,"schema_versions":[1,2,1],"failed_migration_preserved_active_panel":true,"independent_migrated_process_recovery":migrated_recovery,"independent_process_recovery":recovery,"cross_version_native_identity_retained":false,
                "checkpoint_save_failure_keeps_window_open":true,"checkpoint_close_saved":true,
                "automatic_checkpoint_exit":autosave_exit,"automatic_checkpoint_recovery":autosave_recovery,
                "physical_ime_accepted":false,"all_native_components_projected":false,"android_ohos_tsx_accepted":false});
            report["business_journal_exit"] = journal_exit;
            report["business_journal_recovery"] = journal_recovery;
            report["power_loss_accepted"] = serde_json::json!(false);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(|error| error.to_string())?;
            writeln!(
                file,
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
            println!("{}", serde_json::json!({"output":output,"state":state}));
            return Ok(());
        }
        let origin = Instant::now();
        let mut autosave = workspace_store
            .as_ref()
            .map(|_| app.checkpoint().map(WorkspaceAutosave::new))
            .transpose()?;
        loop {
            app.mount.root().host().pump();
            if app.mount.root().host().is_closed() {
                return Ok(());
            }
            if app.poll()? && !app.composing.get() {
                if let Some(autosave) = &mut autosave {
                    autosave.observe(app.checkpoint()?, origin.elapsed());
                }
            }
            if let (Some(autosave), Some(store)) = (&mut autosave, &mut workspace_store) {
                if let Err(error) =
                    autosave.save_due(origin.elapsed(), app.composing.get(), |value| {
                        store
                            .save_business(&policy, value)
                            .map_err(|error| error.to_string())
                    })
                {
                    eprintln!(
                        "Automatic save failed; retaining the pending checkpoint for retry: {error}"
                    );
                }
            }
            if app.mount.root().host().close_requested() {
                if let Some(store) = &mut workspace_store {
                    match app.checkpoint_close(store, &policy) {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            eprintln!("Workspace remains open because saving failed: {error}")
                        }
                    }
                }
            }
            let environment = app.mount.root().host().environment();
            if environment != app.environment {
                app.environment = environment;
                app.viewport = app.mount.root().host().viewport();
                app.render()?;
            }
            let wait = autosave.as_ref().map_or(u32::MAX, |autosave| {
                autosave.wait_ms(origin.elapsed(), app.composing.get())
            });
            app.mount.root().host().wait_for_events(wait)?;
        }
    }
}
