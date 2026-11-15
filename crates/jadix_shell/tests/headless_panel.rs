use std::{cell::Cell, collections::BTreeSet, rc::Rc};

use jadix_core::{create_signal, on_cleanup};
use jadix_native::{presentation::*, HeadlessRoot};
use jadix_shell::{HostRegistry, PackageManifest, Session, ShellError};

#[test]
fn one_session_owner_drives_native_commits_and_revokes_late_host_results() {
    let mut session = Session::new(4);
    session
        .verify(PackageManifest {
            package: "example.counter".into(),
            protocol_version: PROTOCOL_VERSION,
            requested_permissions: BTreeSet::from(["counter.save".into()]),
        })
        .unwrap();
    session
        .authorize(&BTreeSet::from(["counter.save".into()]))
        .unwrap();
    let cleanup = Rc::new(Cell::new(false));
    let observed = cleanup.clone();
    let count = session
        .load(|| {
            on_cleanup(move || observed.set(true));
            Ok(create_signal(0usize))
        })
        .unwrap();
    let lifetime = session.mount_root().unwrap();
    let mut root =
        HeadlessRoot::with_lifetime(lifetime, capabilities::BackendCapabilities::headless());
    let node = NodeId::new(root.id());
    let descriptor = |count| NodeDescriptor {
        content: NodeContent::Button {
            label: format!("Count {count}"),
            enabled: true,
        },
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        accessibility_relations: Default::default(),
        frame: Rect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 40.0,
        },
        accessible_description: None,
        accessible_name: None,
        style: Default::default(),
    };
    root.commit(CommitBatch {
        root: root.id(),
        previous: 0,
        revision: 1,
        operations: vec![Operation::Create {
            node,
            parent: None,
            index: 0,
            descriptor: descriptor(count.get()),
        }],
    })
    .unwrap();
    session.activate().unwrap();
    root.accept_event(&PlatformEvent {
        node,
        payload: EventPayload::Activate,
    })
    .unwrap();
    count.set(count.get() + 1);
    root.commit(CommitBatch {
        root: root.id(),
        previous: 1,
        revision: 2,
        operations: vec![Operation::Update {
            node,
            descriptor: descriptor(count.get()),
        }],
    })
    .unwrap();
    assert_eq!(
        root.node(node).unwrap().descriptor.content,
        NodeContent::Button {
            label: "Count 1".into(),
            enabled: true
        }
    );
    let mut registry = HostRegistry::default();
    registry
        .register("counter.save".into(), "counter.save".into())
        .unwrap();
    let pending = session
        .request_for_root(
            &registry,
            root.id(),
            "counter.save",
            &jadix_shell::schema::Value::Null,
        )
        .unwrap();
    session.dispose();
    assert!(cleanup.get());
    assert_eq!(session.complete(pending), Err(ShellError::StaleRequest));
    assert!(root.node(node).is_none());
}
