#![cfg(feature = "native-host")]

use jadix_native::{
    host::{ApplyReceipt, PlatformHost},
    layout::{LayoutError, MeasureRequest},
    presentation::*,
};
use jadix_shell::{
    native::NativeMount,
    wire::{ComponentRegistry, Envelope},
    PackageManifest, Session,
};
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

#[derive(Default)]
struct Host {
    batches: Vec<CommitBatch>,
    events: Vec<PlatformEvent>,
    fail: bool,
    disposed: bool,
    popovers: bool,
    desktop: bool,
    updater: Option<jadix_core::updater::Updater>,
    shares: Vec<(
        jadix_core::desktop::Share,
        std::sync::mpsc::Sender<jadix_core::desktop::ShareResult>,
    )>,
    urls: Vec<(
        String,
        std::sync::mpsc::Sender<jadix_core::desktop::OpenUrlResult>,
    )>,
}

#[cfg(feature = "native-vm")]
#[test]
fn shared_sdk_native_application_moves_keyed_nodes_and_relays_events() {
    use jadix_shell::{application::ScriptApplication, native::NativeApplication};
    let (_, registry, _) = setup();
    let script = ScriptApplication::development(
        "tests.native.application",
        include_str!("fixtures/structural.js"),
        registry,
        Default::default(),
    )
    .unwrap();
    let lifetime = script.lifetime();
    let mut app = NativeApplication::new(script, Host::default(), Default::default());
    assert_eq!(app.publish([400., 300.], 0).unwrap(), 1);
    let button = app.application().node_at(&[0]).unwrap();
    let a = app.application().node_at(&[1]).unwrap();
    let b = app.application().node_at(&[2]).unwrap();
    for phase in 1..=10 {
        app.root_mut().host_mut().events.push(PlatformEvent {
            node: button,
            payload: EventPayload::Activate,
        });
        app.poll().unwrap();
        assert_eq!(app.publish([400., 300.], 0).unwrap(), 1);
        assert_eq!(
            app.application().node_at(&[1]),
            Some(if phase % 2 == 1 { b } else { a })
        );
        let batch = app.root().host().batches.last().unwrap();
        assert!(!batch
            .operations
            .iter()
            .any(|op| matches!(op, Operation::Remove {node} if *node == a || *node == b)));
        assert!(batch
            .operations
            .iter()
            .any(|op| matches!(op, Operation::Move { .. })));
    }
    assert_eq!(app.publish([240., 200.], 1).unwrap(), 0);
    assert_eq!(app.application().node_at(&[2]), Some(b));
    drop(app);
    assert!(lifetime.is_disposed());
}

impl PlatformHost for Host {
    fn updater(&self) -> Option<jadix_core::updater::Updater> { self.updater.clone() }
    fn show_share(
        &mut self,
        share: &jadix_core::desktop::Share,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>, String> {
        if !self.desktop {
            return Err("sharing unavailable".into());
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.shares.push((share.clone(), sender));
        Ok(receiver)
    }
    fn open_url(
        &mut self,
        url: &str,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        if !self.desktop || url.contains("denied") {
            return Err("opening denied".into());
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.urls.push((url.into(), sender));
        Ok(receiver)
    }
    fn capabilities(&self) -> capabilities::BackendCapabilities {
        let mut capabilities = capabilities::BackendCapabilities::headless();
        if !self.popovers {
            capabilities
                .node_kinds
                .retain(|kind| *kind != capabilities::NodeKind::Popover);
        }
        capabilities
    }
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: 100.0_f64.min(request.key.max_width),
            height: 32.0_f64.min(request.key.max_height),
            baseline: Some(24.0_f64.min(request.key.max_height)),
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        unreachable!()
    }
    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        _: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        if self.fail {
            return Err("injected native failure".into());
        }
        self.batches.push(batch.clone());
        Ok(ApplyReceipt::for_batch(batch))
    }
    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        Ok(std::mem::take(&mut self.events))
    }
    fn dispose(&mut self) {
        self.disposed = true;
    }
}

#[cfg(feature = "native-vm")]
#[test]
fn sdk_updater_drives_the_shared_service_and_rejects_arbitrary_sources() {
    use jadix_shell::{application::ScriptApplication, native::NativeApplication};
    let (_, registry, _) = setup();
    let sdk = [include_str!("../sdk/src/utf8.mjs"), include_str!("../sdk/src/generated.mjs"), include_str!("../sdk/src/index.mjs")]
        .join("\n").lines().filter(|line| !line.starts_with("import ") && !line.starts_with("export {"))
        .map(|line| line.strip_prefix("export ").unwrap_or(line)).collect::<Vec<_>>().join("\n");
    let source = format!("{sdk}\n{}", r#"
        const app = createApplication(); const seen = {};
        const save = (name, value) => { seen[name] = value; app.mount(Text({text: JSON.stringify(seen)})); };
        app.updater({action: 'check'}).then(value => save('phase', value.phase));
        app.updater({action: 'check', url: 'https://untrusted.test/latest.json'}).then(
            () => save('untrusted', false), () => save('untrusted', true));
    "#);
    let script = ScriptApplication::development("tests.updater.native", &source, registry, Default::default()).unwrap();
    let service = jadix_core::updater::Updater::new(Some(jadix_core::updater::Config {
        id: "example.app".into(), version: "1.0.0".into(), github: Some("example/app".into()),
        url: None, channels: Default::default(), public_key: None,
    }), "macos-aarch64", true).unwrap();
    let mut app = NativeApplication::new(script, Host { updater: Some(service.clone()), ..Default::default() }, Default::default());
    app.publish([400., 300.], 0).unwrap(); app.poll().unwrap(); app.publish([400., 300.], 0).unwrap();
    assert_eq!(service.snapshot().phase, jadix_core::updater::Phase::Checking);
    let NodeContent::Text(text) = &app.application().roots().first().unwrap().descriptor.content else { panic!("expected SDK result") };
    let result: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(result, serde_json::json!({"phase":"checking","untrusted":true}));
    assert!(service.take_commands().iter().any(|command| matches!(command, jadix_core::updater::Command::Manifest { url, .. } if url == "https://github.com/example/app/releases/latest/download/latest.json")));
}

#[cfg(feature = "native-vm")]
#[test]
fn sdk_desktop_promises_round_trip_through_the_native_application_and_end_with_the_root() {
    use jadix_shell::{application::ScriptApplication, native::NativeApplication};
    let (_, registry, _) = setup();
    // Execute the actual SDK modules in the same QuickJS global, without a
    // generated fixture that could silently drift from the shipped bridge.
    let sdk = [
        include_str!("../sdk/src/utf8.mjs"),
        include_str!("../sdk/src/generated.mjs"),
        include_str!("../sdk/src/index.mjs"),
    ]
    .join("\n")
    .lines()
    .filter(|line| !line.starts_with("import ") && !line.starts_with("export {"))
    .map(|line| line.strip_prefix("export ").unwrap_or(line))
    .collect::<Vec<_>>()
    .join("\n");
    let source = format!(
        "{sdk}\n{}",
        r#"
        const app = createApplication();
        const seen = [];
        const observe = (name, promise) => promise.then(
          value => { seen.push([name, value]); app.mount(Text({text: JSON.stringify(seen)})); },
          error => { seen.push([name, error.message]); app.mount(Text({text: JSON.stringify(seen)})); });
        observe('share', app.share({subject: 'Subject', text: 'Shared text'}));
        observe('url', app.openUrl('https://example.com'));
        observe('denied', app.openUrl('https://denied.example.com'));
        observe('disconnected', app.openUrl('https://example.com/disconnected'));
        observe('late', app.openUrl('https://example.com/late'));
    "#
    );
    let script = ScriptApplication::development(
        "tests.desktop.native",
        &source,
        registry,
        Default::default(),
    )
    .unwrap();
    let mut app = NativeApplication::new(
        script,
        Host {
            desktop: true,
            ..Default::default()
        },
        Default::default(),
    );
    app.publish([400., 300.], 0).unwrap();
    let host = app.root_mut().host_mut();
    assert_eq!(host.shares.len(), 1);
    assert_eq!(host.shares[0].0.subject.as_deref(), Some("Subject"));
    assert_eq!(host.shares[0].0.text.as_deref(), Some("Shared text"));
    assert_eq!(host.urls.len(), 3);
    host.shares
        .pop()
        .unwrap()
        .1
        .send(Ok(jadix_core::desktop::ShareOutcome::Shared {
            service: "Copy".into(),
        }))
        .unwrap();
    host.urls.remove(0).1.send(Ok(true)).unwrap();
    drop(host.urls.remove(0));
    app.poll().unwrap();
    app.publish([400., 300.], 0).unwrap();
    let node = app.application().roots().first().unwrap();
    let NodeContent::Text(text) = &node.descriptor.content else {
        panic!("expected SDK result text");
    };
    let results: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        results,
        serde_json::json!([
            ["denied", "opening denied"], ["share", {"status":"shared", "service":"Copy"}],
            ["url", true], ["disconnected", "desktop request disconnected"]
        ])
    );
    let late = app.root_mut().host_mut().urls.pop().unwrap().1;
    app.dispose();
    assert!(late.send(Ok(true)).is_err());
    assert!(app.application().lifetime().is_disposed());
    assert!(app.application().roots().is_empty());
}

fn setup() -> (Session, ComponentRegistry, NativeMount<Host>) {
    setup_with_host(Host::default())
}

#[test]
fn generic_interaction_mounts_delivers_and_rejects_invalid_policy_atomically() {
    let (mut session, registry, mut mount) = setup();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = seen.clone();
    let handle = session.export_event(mount.root().model().id(), registry.get("Box").unwrap().events["interaction"].clone(),
        move |value| observed.borrow_mut().push(serde_json::to_value(value).unwrap())).unwrap();
    let tree = |axis| serde_json::json!({"component":"Box","key":"gesture",
        "props":{"interaction":{"touch":true,"pan":axis,"panActivation":{"activeOffsetX":12.5,"failOffsetY":8}}},"events":{"interaction":handle}});
    let message = envelope(&session, &mount, &registry, tree("horizontal"));
    mount.present(&session, &registry, &message, [320.,240.], 0).unwrap();
    let id = mount.node_at(&[]).unwrap();
    let event = interaction::InteractionEvent { kind:interaction::InteractionKind::Pan,
        phase:interaction::InteractionPhase::Changed, position:[2.0,3.0], local_position:None, timestamp:1.0,
        modifiers:Default::default(), translation:Some([1.0,0.0]), velocity:Some([10.0,0.0]),
        value:None, rate:None, touches:vec![], active_touches:vec![] };
    mount.root_mut().host_mut().events.push(PlatformEvent { node:id, payload:EventPayload::Interaction(event) });
    for delivery in mount.poll(&session).unwrap() { delivery.dispatch().unwrap(); }
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0]["translation"], serde_json::json!([1.0,0.0]));
    let contact = interaction::TouchPoint { id:4, position:[2.0,3.0], force:0.5,
        tilt:[30.0,-45.0], kind:interaction::TouchKind::Pencil };
    let event = interaction::InteractionEvent { kind:interaction::InteractionKind::Touch,
        phase:interaction::InteractionPhase::Started, position:[2.0,3.0], local_position:None, timestamp:2.0,
        modifiers:Default::default(), translation:None, velocity:None, value:None, rate:None,
        touches:vec![contact.clone()], active_touches:vec![contact] };
    mount.root_mut().host_mut().events.push(PlatformEvent { node:id, payload:EventPayload::Interaction(event) });
    for delivery in mount.poll(&session).unwrap() { delivery.dispatch().unwrap(); }
    assert_eq!(seen.borrow()[1]["touches"][0]["tilt"], serde_json::json!([30.0,-45.0]));
    assert_eq!(seen.borrow()[1]["activeTouches"][0]["kind"], "pencil");
    let before = mount.root().host().batches.len();
    let invalid = envelope(&session, &mount, &registry, tree("diagonal"));
    assert!(mount.present(&session, &registry, &invalid, [320.,240.], 0).is_err());
    assert_eq!(mount.root().host().batches.len(), before);
    let mut invalid_gate = tree("horizontal");
    invalid_gate["props"]["interaction"]["panActivation"]["failOffsetY"] = serde_json::json!(-1);
    let invalid = envelope(&session, &mount, &registry, invalid_gate);
    assert!(mount.present(&session, &registry, &invalid, [320.,240.], 0).is_err());
    assert_eq!(mount.root().host().batches.len(), before);
    assert_eq!(mount.root().model().node(id).unwrap().descriptor.interaction.pan_activation.as_ref().unwrap().active_offset_x, Some(12.5));
    let message = envelope(&session, &mount, &registry, tree("vertical"));
    mount.present(&session, &registry, &message, [320.,240.], 0).unwrap();
    assert_eq!(mount.node_at(&[]), Some(id));
    assert_eq!(mount.root().model().node(id).unwrap().descriptor.interaction.pan, Some(interaction::PanAxis::Vertical));
}

#[test]
fn generic_keyboard_policy_and_events_round_trip_and_update_without_remounting() {
    let (mut session, registry, mut mount) = setup();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut handles = serde_json::Map::new();
    for name in ["key", "shortcut"] {
        let observed = seen.clone();
        let handle = session.export_event(mount.root().model().id(),
            registry.get("Box").unwrap().events[name].clone(),
            move |value| observed.borrow_mut().push(serde_json::to_value(value).unwrap())).unwrap();
        handles.insert(name.into(), serde_json::to_value(handle).unwrap());
    }
    let tree = |key_down, shortcuts| serde_json::json!({"component":"Box", "key":"keyboard",
        "props":{"keyboard":{"focusable":true,"keyDown":key_down,"shortcuts":shortcuts},"focusRequest":1},
        "events":handles});
    let shortcuts = serde_json::json!([{"id":"save","key":"s","modifiers":{"meta":true}}]);
    let message = envelope(&session, &mount, &registry, tree(true, shortcuts.clone()));
    mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
    let id = mount.node_at(&[]).unwrap();
    // The platform reports successful responder ownership before key events.
    mount.root_mut().host_mut().events.extend([
        PlatformEvent { node:id, payload:EventPayload::Focus },
        PlatformEvent { node:id, payload:EventPayload::Key(keyboard::KeyEvent {
            phase:keyboard::KeyPhase::Down, key:"Ж".into(), code:4, modifiers:Default::default(), repeat:false }) },
        PlatformEvent { node:id, payload:EventPayload::Shortcut { id:"save".into() } },
    ]);
    for delivery in mount.poll(&session).unwrap() { delivery.dispatch().unwrap(); }
    assert_eq!(*seen.borrow(), vec![serde_json::json!({"phase":"down","key":"Ж","code":4.0,
        "modifiers":{"shift":false,"control":false,"alt":false,"meta":false,"capsLock":false},"repeat":false}),
        serde_json::json!({"id":"save"})]);
    let message = envelope(&session, &mount, &registry, tree(false, shortcuts.clone()));
    mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
    assert_eq!(mount.node_at(&[]), Some(id));
    assert!(!mount.root().model().node(id).unwrap().descriptor.keyboard.key_down);
    let before = mount.root().host().batches.len();
    let duplicate = serde_json::json!([shortcuts[0].clone(), shortcuts[0].clone()]);
    let message = envelope(&session, &mount, &registry, tree(true, duplicate));
    assert!(mount.present(&session, &registry, &message, [320., 240.], 0).is_err());
    assert_eq!(mount.root().host().batches.len(), before);
    assert!(!mount.root().model().node(id).unwrap().descriptor.keyboard.key_down);
}

#[test]
fn shared_selection_roles_mount_and_update_through_generic_properties() {
    let (session, registry, mut mount) = setup();
    for role in ["listbox", "heading", "option"] {
        let tree = serde_json::json!({"component":"Box", "key":"role", "props":{"accessibleRole":role,"accessibleName":"Emoji"}});
        let message = envelope(&session, &mount, &registry, tree);
        mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
        let id = mount.node_at(&[]).unwrap();
        let descriptor = &mount.root().model().node(id).unwrap().descriptor;
        assert_eq!(descriptor.accessible_role.unwrap().aria_role(), role);
        assert_eq!(descriptor.accessible_name.as_deref(), Some("Emoji"));
    }
    let tree = serde_json::json!({"component":"Box", "key":"role", "props":{"accessibleRole":"unrecognized"}});
    let message = envelope(&session, &mount, &registry, tree);
    assert!(mount.present(&session, &registry, &message, [320., 240.], 0).is_err());
}

#[test]
fn accessibility_relations_resolve_forward_keys_and_drop_missing_or_ambiguous_targets() {
    let (session, registry, mut mount) = setup();
    let tree = |children| serde_json::json!({"component":"Box", "props":{}, "children":[
        {"component":"Input", "key":"search", "props":{
            "value":"", "accessibleActiveDescendant":"choice", "accessibleControls":["list", "missing", "list"]}},
        {"component":"Box", "key":"list", "props":{"accessibleRole":"listbox"}, "children":children}
    ]});
    let option = serde_json::json!({"component":"Button", "key":"choice", "props":{"label":"A", "accessibleRole":"option"}});
    let message = envelope(&session, &mount, &registry, tree(serde_json::json!([option.clone()])));
    mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
    let input = mount.node_at(&[0]).unwrap();
    let list = mount.node_at(&[1]).unwrap();
    let choice = mount.node_at(&[1, 0]).unwrap();
    let relations = &mount.root().model().node(input).unwrap().descriptor.accessibility_relations;
    assert_eq!(relations.active_descendant, Some(choice));
    assert_eq!(relations.controls, vec![list]);

    // Equal keys under different parents are legal, but cannot name a unique AX target.
    let branches = serde_json::json!([
        {"component":"Box", "key":"a", "props":{}, "children":[option.clone()]},
        {"component":"Box", "key":"b", "props":{}, "children":[option]}
    ]);
    for children in [branches, serde_json::json!([])] {
        let message = envelope(&session, &mount, &registry, tree(children));
        mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
        assert_eq!(mount.node_at(&[0]), Some(input));
        let relations = &mount.root().model().node(input).unwrap().descriptor.accessibility_relations;
        assert_eq!(relations.active_descendant, None);
        assert_eq!(relations.controls, vec![list]);
        assert!(mount.root().model().node(choice).is_none());
    }
}

fn setup_with_host(host: Host) -> (Session, ComponentRegistry, NativeMount<Host>) {
    let mut session = Session::new(4);
    session
        .verify(PackageManifest {
            package: "tests.native".into(),
            protocol_version: 1,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    session.load(|| Ok(())).unwrap();
    let mount = NativeMount::new(&mut session, host, Default::default()).unwrap();
    session.activate().unwrap();
    let mut registry = ComponentRegistry::default();
    let schemas: Vec<component::ComponentSchema> = serde_json::from_str(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap();
    for schema in schemas {
        registry.register(schema).unwrap();
    }
    (session, registry, mount)
}

fn envelope(
    session: &Session,
    mount: &NativeMount<Host>,
    registry: &ComponentRegistry,
    tree: serde_json::Value,
) -> Envelope {
    let value = serde_json::json!({"protocol":1,"session":session.token(),"command":{"type":"mount","root":mount.root().model().id().token(),"tree":tree}});
    Envelope::decode(&serde_json::to_vec(&value).unwrap(), registry).unwrap()
}

#[test]
fn wheel_events_reach_registered_scroll_callbacks_with_logical_deltas() {
    let (mut session, registry, mut mount) = setup();
    let seen = Rc::new(std::cell::RefCell::new(None));
    let observed = seen.clone();
    let handle = session
        .export_event(
            mount.root().model().id(),
            registry.get("Scroll").unwrap().events["wheel"].clone(),
            move |value| *observed.borrow_mut() = Some(value),
        )
        .unwrap();
    let tree = serde_json::json!({
        "component":"Scroll",
        "key":"scroll",
        "props":{},
        "events":{"wheel":handle}
    });
    let message = envelope(&session, &mount, &registry, tree);
    mount
        .present(&session, &registry, &message, [320.0, 240.0], 0)
        .unwrap();
    let node = mount.node_at(&[]).unwrap();
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node,
        payload: EventPayload::Wheel {
            delta_x: -2.5,
            delta_y: 48.0,
        },
    });
    mount
        .poll(&session)
        .unwrap()
        .pop()
        .unwrap()
        .dispatch()
        .unwrap();
    let observed = serde_json::to_value(seen.borrow().as_ref().unwrap()).unwrap();
    assert_eq!(observed, serde_json::json!({"deltaX":-2.5,"deltaY":48.0}));
}

fn input(key: &str, value: &str) -> serde_json::Value {
    serde_json::json!({"component":"Input","key":key,"props":{"value":value},"style":{"width":180,"height":32}})
}

fn popover_setup() -> (Session, ComponentRegistry, NativeMount<Host>) {
    let (session, mut registry, mount) = setup_with_host(Host {
        popovers: true,
        ..Default::default()
    });
    for schema in serde_json::from_str::<Vec<component::ComponentSchema>>(include_str!(
        "../../jadix_components/schema/extended-v1.json"
    ))
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    (session, registry, mount)
}

fn popover_tree() -> serde_json::Value {
    serde_json::json!({"component":"Box","props":{},"children":[
        {"component":"PopoverContent","key":"panel","props":{"anchor":"trigger","open":true,"placement":"right-start","gap":8},"style":{"width":180,"height":80},"children":[input("draft", "中文😀")]},
        {"component":"Button","key":"trigger","props":{"label":"Open"},"style":{"width":120,"height":32}}
    ]})
}

#[test]
fn keyed_popover_anchors_resolve_forward_references_moves_dismissals_and_disposal() {
    let (mut session, registry, mut mount) = popover_setup();
    let seen = Rc::new(Cell::new(0));
    let observed = seen.clone();
    let handle = session
        .export_event(
            mount.root().model().id(),
            registry.get("PopoverContent").unwrap().events["dismiss"].clone(),
            move |_| observed.set(observed.get() + 1),
        )
        .unwrap();
    let mut tree = popover_tree();
    tree["children"][0]["events"] = serde_json::json!({"dismiss":handle});
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let panel = mount.node_at(&[0]).unwrap();
    let editor = mount.node_at(&[0, 0]).unwrap();
    let trigger = mount.node_at(&[1]).unwrap();
    assert!(
        matches!(mount.root().model().node(panel).unwrap().descriptor.content, NodeContent::Popover { anchor, placement: PopoverPlacement::RightStart, gap: 8, revision: 0, .. } if anchor == trigger)
    );
    tree["children"].as_array_mut().unwrap().swap(0, 1);
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_eq!(mount.node_at(&[1]), Some(panel));
    assert_eq!(mount.node_at(&[1, 0]), Some(editor));
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: panel,
        payload: EventPayload::Dismiss {
            backdrop: true,
            revision: 1,
        },
    });
    let delivery = mount.poll(&session).unwrap().pop().unwrap();
    delivery.dispatch().unwrap();
    assert_eq!(seen.get(), 1);
    tree["children"][1]["props"]["open"] = false.into();
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert!(matches!(
        mount.root().model().node(panel).unwrap().descriptor.content,
        NodeContent::Popover {
            open: false,
            revision: 1,
            ..
        }
    ));
    tree["children"][1]["props"]["open"] = true.into();
    let message = envelope(&session, &mount, &registry, tree);
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_eq!(mount.node_at(&[1, 0]), Some(editor));
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: panel,
        payload: EventPayload::Dismiss {
            backdrop: false,
            revision: 3,
        },
    });
    let late = mount.poll(&session).unwrap().pop().unwrap();
    session.dispose();
    assert!(late.dispatch().is_err());
    assert_eq!(seen.get(), 1);
}

#[test]
fn popover_options_and_notifications_round_trip_through_shell() {
    let (mut session, registry, mut mount) = popover_setup();
    let mut tree = popover_tree();
    tree["children"][0]["props"]["close_on_escape"] = false.into();
    tree["children"][0]["props"]["margin"] = 12.into();
    let popover = registry.get("PopoverContent").unwrap();
    let canceled = Rc::new(Cell::new(0));
    let positioned = Rc::new(Cell::new(0));
    let opened = Rc::new(Cell::new(0));
    let closed = Rc::new(Cell::new(0));
    for (event, counter) in [
        ("cancel", canceled.clone()),
        ("positionChange", positioned.clone()),
        ("open", opened.clone()),
        ("close", closed.clone()),
    ] {
        let handle = session
            .export_event(
                mount.root().model().id(),
                popover.events[event].clone(),
                move |_| counter.set(counter.get() + 1),
            )
            .unwrap();
        tree["children"][0]["events"][event] = serde_json::to_value(handle).unwrap();
    }
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let panel = mount.node_at(&[0]).unwrap();
    let style = &mount.root().model().node(panel).unwrap().descriptor.style;
    assert_eq!(style.popover_margin, Some(12));
    assert_eq!(style.popover_close_on_escape, Some(false));
    for payload in [
        EventPayload::PopoverLifecycle { open: true },
        EventPayload::PopoverCancel { backdrop: false },
        EventPayload::PopoverPosition {
            position: Some(PopoverPosition {
                frame: Rect {
                    x: 10.0,
                    y: 20.0,
                    width: 80.0,
                    height: 40.0,
                },
                placement: PopoverPlacement::RightStart,
                available_width: 120.0,
                available_height: 90.0,
            }),
        },
        EventPayload::PopoverPosition { position: None },
    ] {
        mount.root_mut().host_mut().events.push(PlatformEvent {
            node: panel,
            payload,
        });
    }
    let deliveries = mount.poll(&session).unwrap();
    assert_eq!(deliveries.len(), 4);
    for delivery in deliveries {
        delivery.dispatch().unwrap();
    }
    assert_eq!(canceled.get(), 1);
    assert_eq!(positioned.get(), 2);
    assert_eq!(opened.get(), 1);
    assert_eq!(closed.get(), 0);
    tree["children"][0]["props"]["open"] = false.into();
    let message = envelope(&session, &mount, &registry, tree);
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: panel,
        payload: EventPayload::PopoverLifecycle { open: false },
    });
    mount
        .poll(&session)
        .unwrap()
        .pop()
        .unwrap()
        .dispatch()
        .unwrap();
    assert_eq!(closed.get(), 1);
}

#[test]
fn invalid_popover_links_and_options_keep_the_previous_native_candidate_intact() {
    let (session, registry, mut mount) = popover_setup();
    let valid = popover_tree();
    let message = envelope(&session, &mount, &registry, valid.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let panel = mount.node_at(&[0]);
    let commits = mount.root().host().batches.len();
    let mut invalid = Vec::new();
    let mut missing = valid.clone();
    missing["children"][0]["props"]["anchor"] = "missing".into();
    invalid.push(missing);
    let mut ambiguous = valid.clone();
    ambiguous["children"].as_array_mut().unwrap().push(serde_json::json!({"component":"Box","props":{},"children":[{"component":"Button","key":"trigger","props":{"label":"ambiguous"}}]}));
    invalid.push(ambiguous);
    let mut descendant = valid.clone();
    let trigger = descendant["children"]
        .as_array_mut()
        .unwrap()
        .pop()
        .unwrap();
    descendant["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(trigger);
    invalid.push(descendant);
    for gap in [-1.0, 0.5, 1025.0] {
        let mut tree = valid.clone();
        tree["children"][0]["props"]["gap"] = gap.into();
        invalid.push(tree);
    }
    let mut placement = valid.clone();
    placement["children"][0]["props"]["placement"] = "unknown".into();
    invalid.push(placement);
    for tree in invalid {
        let message = envelope(&session, &mount, &registry, tree);
        assert!(mount
            .present(&session, &registry, &message, [500.0, 400.0], 0)
            .is_err());
        assert_eq!(mount.node_at(&[0]), panel);
        assert_eq!(mount.root().host().batches.len(), commits);
        assert!(!mount.root().host().disposed);
    }
    let (other_session, mut other_registry, mut unsupported) = setup();
    other_registry
        .register(registry.get("PopoverContent").unwrap().clone())
        .unwrap();
    let message = envelope(&other_session, &unsupported, &other_registry, valid);
    assert!(unsupported
        .present(&other_session, &other_registry, &message, [500.0, 400.0], 0)
        .is_err());
    assert!(unsupported.root().host().batches.is_empty());
    assert!(!unsupported.root().host().disposed);
}

#[test]
fn keyed_native_moves_retain_identity_and_remounts_get_new_identifiers() {
    let (session, registry, mut mount) = setup();
    let mut tree = serde_json::json!({"component":"Box","props":{},"children":[input("one","一😀"),input("two","二")]});
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let first = mount.node_at(&[0]).unwrap();
    let second = mount.node_at(&[1]).unwrap();
    tree["children"].as_array_mut().unwrap().swap(0, 1);
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_eq!(mount.node_at(&[1]), Some(first));
    assert_eq!(mount.node_at(&[0]), Some(second));
    assert!(mount
        .root()
        .host()
        .batches
        .last()
        .unwrap()
        .operations
        .iter()
        .all(|operation| !matches!(
            operation,
            Operation::Create { .. } | Operation::Remove { .. }
        )));
    let batches = mount.root().host().batches.len();
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_eq!(mount.root().host().batches.len(), batches);
    tree["children"] = serde_json::json!([]);
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    tree["children"] = serde_json::json!([input("one", "一😀")]);
    let message = envelope(&session, &mount, &registry, tree);
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_ne!(mount.node_at(&[0]), Some(first));
}

#[test]
fn native_edit_preview_survives_unrelated_render_and_rejected_commit_is_reconciled() {
    let (mut session, registry, mut mount) = setup();
    let seen = Rc::new(Cell::new(0));
    let observed = seen.clone();
    let schema = registry.get("Input").unwrap().events["edit"].clone();
    let handle = session
        .export_event(mount.root().model().id(), schema, move |_| {
            observed.set(observed.get() + 1)
        })
        .unwrap();
    let mut tree = input("editor", "business");
    tree["events"] = serde_json::json!({"edit":handle});
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let editor = mount.node_at(&[]).unwrap();
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: editor,
        payload: EventPayload::Edit {
            value: "中😀".into(),
            revision: 1,
            selection_utf16: [3, 3],
            composing_utf16: Some([0, 3]),
        },
    });
    let delivery = mount.poll(&session).unwrap().pop().unwrap();
    delivery.dispatch().unwrap();
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert!(
        matches!(&mount.root().model().node(editor).unwrap().descriptor.content,NodeContent::Input{value,revision:1,..} if value=="中😀")
    );
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: editor,
        payload: EventPayload::Edit {
            value: "中😀".into(),
            revision: 2,
            selection_utf16: [3, 3],
            composing_utf16: None,
        },
    });
    let delivery = mount.poll(&session).unwrap().pop().unwrap();
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert!(
        matches!(&mount.root().model().node(editor).unwrap().descriptor.content,NodeContent::Input{value,revision:3,..} if value=="business")
    );
    session.dispose();
    assert!(delivery.dispatch().is_err());
    assert_eq!(seen.get(), 1);
    assert!(mount.poll(&session).is_err());
    assert!(mount.root().host().disposed);
}

#[test]
fn duplicate_keys_foreign_roots_and_failed_native_apply_do_not_publish_a_candidate() {
    let (session, registry, mut mount) = setup();
    let message = envelope(&session, &mount, &registry, input("one", "a"));
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let original = mount.node_at(&[]).unwrap();
    let mut foreign = message.clone();
    let jadix_shell::wire::Command::Mount { root, .. } = &mut foreign.command else {
        unreachable!()
    };
    *root = RootId::new().token();
    assert!(mount
        .present(&session, &registry, &foreign, [500.0, 400.0], 0)
        .is_err());
    assert_eq!(mount.root().host().batches.len(), 1);
    let duplicate = serde_json::json!({"protocol":1,"session":session.token(),"command":{"type":"mount","root":mount.root().model().id().token(),"tree":{"component":"Box","props":{},"children":[input("same","a"),input("same","b")]}}});
    assert!(Envelope::decode(&serde_json::to_vec(&duplicate).unwrap(), &registry).is_err());
    mount.root_mut().host_mut().fail = true;
    let message = envelope(&session, &mount, &registry, input("one", "b"));
    assert!(mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .is_err());
    assert!(mount.root().host().disposed);
    assert!(mount.root().model().node(original).is_none());
    assert_eq!(mount.node_at(&[]), None);
}

#[test]
fn extended_controls_reconcile_revisioned_business_values_and_container_type_changes() {
    let (mut session, mut registry, mut mount) = setup();
    let schemas: Vec<component::ComponentSchema> = serde_json::from_str(include_str!(
        "../../jadix_components/schema/extended-v1.json"
    ))
    .unwrap();
    for schema in schemas {
        registry.register(schema).unwrap();
    }
    let mut alternate = registry.get("Box").unwrap().clone();
    alternate.name = "Panel".into();
    registry.register(alternate).unwrap();
    let tree = serde_json::json!({"component":"Box","props":{},"children":[
        {"component":"Checkbox","key":"checked","props":{"label":"check","checked":false}},
        {"component":"Switch","props":{"label":"switch","checked":true}},
        {"component":"Textarea","props":{"value":"多行\n😀"}},
        {"component":"Select","props":{"value":"a","options":[{"value":"a","label":"Alpha","enabled":true}]}},
        {"component":"DialogContent","props":{"open":false},"children":[{"component":"Text","props":{"text":"modal"}}]}
    ]});
    let message = envelope(&session, &mount, &registry, tree.clone());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    let checkbox = mount.node_at(&[0]).unwrap();
    mount.root_mut().host_mut().events.push(PlatformEvent {
        node: checkbox,
        payload: EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    });
    assert!(mount.poll(&session).unwrap().is_empty());
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert!(matches!(
        mount
            .root()
            .model()
            .node(checkbox)
            .unwrap()
            .descriptor
            .content,
        NodeContent::Checkbox {
            checked: false,
            revision: 2,
            ..
        }
    ));
    let mut tree = tree;
    tree["component"] = "Panel".into();
    let message = envelope(&session, &mount, &registry, tree);
    mount
        .present(&session, &registry, &message, [500.0, 400.0], 0)
        .unwrap();
    assert_ne!(mount.node_at(&[0]), Some(checkbox));
    assert!(mount.root().model().node(checkbox).is_none());
    session.dispose();
}

#[test]
fn max_length_admission_is_bounded_and_rejected_updates_do_not_publish() {
    let (session, registry, mut mount) = popover_setup();
    for component in ["Input", "Textarea"] {
        let valid = serde_json::json!({"component":component,"key":"field",
            "props":{"value":"external over-limit", "maxLength":0}});
        let message = envelope(&session, &mount, &registry, valid.clone());
        mount
            .present(&session, &registry, &message, [500., 400.], 0)
            .unwrap();
        let commits = mount.root().host().batches.len();
        for limit in [-1.0, 0.5, 1048577.0] {
            let mut invalid = valid.clone();
            invalid["props"]["maxLength"] = limit.into();
            let message = envelope(&session, &mount, &registry, invalid);
            assert!(mount
                .present(&session, &registry, &message, [500., 400.], 0)
                .is_err());
            assert_eq!(mount.root().host().batches.len(), commits);
        }
    }
}

#[test]
fn button_content_retains_child_layout_accessibility_and_activation() {
    let (mut session, registry, mut mount) = setup();
    let activations = Rc::new(Cell::new(0));
    let observed = activations.clone();
    let handle = session.export_event(mount.root().model().id(),
        registry.get("Button").unwrap().events["activate"].clone(),
        move |_| observed.set(observed.get() + 1)).unwrap();
    let tree = |label| serde_json::json!({"component":"Button", "key":"choice",
        "props":{"label":label}, "style":{"width":80,"height":40},
        "events":{"activate":handle}, "children":[
            {"component":"Box", "key":"content", "props":{}, "children":[
                {"component":"Text", "key":"caption", "props":{"text":"👍"}, "style":{"width":24,"height":24}}
            ]}
        ]});
    let message = envelope(&session, &mount, &registry, tree("thumbs up"));
    mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
    let button = mount.node_at(&[]).unwrap();
    let caption = mount.node_at(&[0, 0]).unwrap();
    let node = mount.root().model().node(button).unwrap();
    assert!(matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label.is_empty()));
    assert_eq!(node.descriptor.accessible_name.as_deref(), Some("thumbs up"));
    assert_eq!(mount.root().model().node(caption).unwrap().descriptor.frame.width, 24.);
    let message = envelope(&session, &mount, &registry, tree("like"));
    mount.present(&session, &registry, &message, [320., 240.], 0).unwrap();
    assert_eq!(mount.node_at(&[]), Some(button));
    assert_eq!(mount.node_at(&[0, 0]), Some(caption));
    assert_eq!(mount.root().model().node(button).unwrap().descriptor.accessible_name.as_deref(), Some("like"));
    mount.root_mut().host_mut().events.push(PlatformEvent { node: button, payload: EventPayload::Activate });
    mount.poll(&session).unwrap().pop().unwrap().dispatch().unwrap();
    assert_eq!(activations.get(), 1);
}

#[cfg(feature = "native-vm")]
#[test]
fn sdk_direct_events_checkpoint_and_restore_share_native_identity() {
    use jadix_shell::{application::ScriptApplication, native::NativeApplication};
    let script = ScriptApplication::development_standard(
        "tests.mobile.sdk",
        include_str!("../../../examples/native-controls/shell/mobile.bundle.js"),
        Default::default(),
    ).unwrap();
    let mut app = NativeApplication::new(script, Host::default(), Default::default());
    app.publish([400., 700.], 0).unwrap();
    let editor = app.application().node_at(&[1]).unwrap();
    let button = app.application().node_at(&[2]).unwrap();
    app.event(PlatformEvent { node: button, payload: EventPayload::Activate }).unwrap();
    app.publish([400., 700.], 0).unwrap();
    assert_eq!(app.application().node_at(&[2]), Some(editor));
    assert_eq!(app.application().checkpoint().unwrap()["count"], 1);
    let state = serde_json::json!({"version":1,"count":2,"value":"中文😀 العربية\u{2028} safe"});
    app.application().restore_checkpoint(&state.to_string()).unwrap();
    app.publish([400., 700.], 0).unwrap();
    assert_eq!(app.application().checkpoint().unwrap(), state);
    assert_eq!(app.application().node_at(&[1]), Some(editor));
    assert!(app.application().restore_checkpoint("{").is_err());
    assert!(app.application().restore_checkpoint(&" ".repeat(65_537)).is_err());
    assert_eq!(app.application().checkpoint().unwrap(), state);
    assert!(app.event_json("{}").is_err());
    app.dispose();
    assert!(app.event(PlatformEvent { node: button, payload: EventPayload::Activate }).is_err());
    assert!(app.application().checkpoint().is_err());
    assert!(app.application().restore_checkpoint("null").is_err());
}
