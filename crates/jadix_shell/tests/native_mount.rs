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
            width: 100.0,
            height: 32.0,
            baseline: Some(24.0),
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

fn setup() -> (Session, ComponentRegistry, NativeMount<Host>) {
    setup_with_host(Host::default())
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
