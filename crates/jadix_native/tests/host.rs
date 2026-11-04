use jadix_native::{BackendError, host::*, layout::*, presentation::*, wire};

#[test]
fn grid_ownership_coordinates_and_duplicate_cells_fail_before_host_publication() {
    let mut root = NativeRoot::new(host(false));
    let mut roots = nodes(root.model().id());
    let editor = roots[0].clone();
    roots[0].id = NodeId::new(root.model().id());
    roots[0].descriptor.content = NodeContent::Container;
    roots[0].descriptor.accessible_role = Some(AccessibleRole::GridCell { row: 1, column: 2 });
    roots[0].children.push(editor);
    let cell = roots.remove(0);
    let mut grid = cell.clone();
    grid.id = NodeId::new(root.model().id());
    grid.descriptor.accessible_role = Some(AccessibleRole::Grid {
        rows: 3,
        columns: 4,
    });
    grid.children = vec![cell];
    let mut header = grid.clone();
    header.id = NodeId::new(root.model().id());
    header.children.clear();
    header.descriptor.content = NodeContent::Text("Column 2".into());
    header.descriptor.accessible_role = Some(AccessibleRole::GridColumnHeader { column: 2 });
    grid.children.push(header);
    let mut roots = vec![grid];
    root.present(&mut roots, [320.0, 480.0], 0).unwrap();
    let revision = root.model().revision();
    let publications = root.host().publishes.get();
    let valid = roots.clone();
    for case in 0..9 {
        let mut candidate = valid.clone();
        match case {
            0 => {
                candidate[0].descriptor.accessible_role = Some(AccessibleRole::Grid {
                    rows: 1,
                    columns: 4,
                })
            }
            1 => {
                candidate[0].descriptor.accessible_role = Some(AccessibleRole::Grid {
                    rows: u32::MAX,
                    columns: 4,
                })
            }
            2 => candidate[0].descriptor.accessible_role = None,
            3 => {
                let mut duplicate = candidate[0].children[0].clone();
                duplicate.id = NodeId::new(root.model().id());
                duplicate.children.clear();
                candidate[0].children.push(duplicate);
            }
            4 => {
                candidate[0].children[0].descriptor.accessible_role =
                    Some(AccessibleRole::GridCell {
                        row: u32::MAX,
                        column: 2,
                    })
            }
            5 => {
                candidate[0].children[0].id = NodeId::new(root.model().id());
                candidate[0].children[0].children.clear();
                candidate[0].children[0].descriptor.content =
                    NodeContent::Text("not a semantic container".into());
            }
            6 => {
                candidate[0].children[1].descriptor.accessible_role =
                    Some(AccessibleRole::GridColumnHeader { column: 4 })
            }
            7 => {
                let mut duplicate = candidate[0].children[1].clone();
                duplicate.id = NodeId::new(root.model().id());
                candidate[0].children.push(duplicate);
            }
            _ => {
                let header = candidate[0].children.pop().unwrap();
                candidate.push(header);
            }
        }
        let outcome = root.present(&mut candidate, [320.0, 480.0], 1);
        assert!(
            matches!(
                &outcome,
                Err(HostError::Backend(BackendError::InvalidGrid(_)))
            ),
            "case {case}: {outcome:?}"
        );
        assert_eq!(root.model().revision(), revision);
        assert_eq!(root.host().publishes.get(), publications);
    }
}

#[test]
fn expansion_requests_require_a_live_enabled_expandable_button() {
    let mut root = NativeRoot::new(host(false));
    let mut roots = nodes(root.model().id());
    let identifier = roots[0].id;
    roots[0].descriptor.content = NodeContent::Button {
        label: "Details".into(),
        enabled: true,
    };
    let event = |expanded| PlatformEvent {
        node: identifier,
        payload: EventPayload::Expansion { expanded },
    };
    root.present(&mut roots, [320.0, 480.0], 0).unwrap();
    assert_eq!(root.event(&event(true)), Err(BackendError::InvalidEvent));
    roots[0].descriptor.accessible_expanded = Some(false);
    root.present(&mut roots, [320.0, 480.0], 1).unwrap();
    let encoded = serde_json::json!({"root": root.model().id().token(), "node": identifier.token(), "payload": {"kind": "expansion", "expanded": true}});
    let decoded = wire::decode_event(&encoded.to_string(), root.model()).unwrap();
    assert_eq!(decoded, event(true));
    root.event(&decoded).unwrap();
    root.event(&event(false)).unwrap();
    roots[0].descriptor.accessible_role = Some(AccessibleRole::TabItem);
    root.present(&mut roots, [320.0, 480.0], 2).unwrap();
    assert_eq!(root.event(&event(true)), Err(BackendError::InvalidEvent));
    roots[0].descriptor.accessible_role = None;
    roots[0].descriptor.content = NodeContent::Button {
        label: "Details".into(),
        enabled: false,
    };
    root.present(&mut roots, [320.0, 480.0], 3).unwrap();
    assert_eq!(root.event(&event(true)), Err(BackendError::InvalidEvent));
    root.present(&mut [], [320.0, 480.0], 4).unwrap();
    assert!(root.event(&event(true)).is_err());
}

#[test]
fn tree_actions_require_live_enabled_semantic_items_and_matching_capabilities() {
    let mut root = NativeRoot::new(host(false));
    let mut roots = nodes(root.model().id());
    let identifier = roots[0].id;
    roots[0].descriptor.content = NodeContent::Button {
        label: "File".into(),
        enabled: true,
    };
    roots[0].descriptor.accessible_role = Some(AccessibleRole::TreeItem);
    roots[0].descriptor.accessible_selected = Some(false);
    let action = |action| PlatformEvent {
        node: identifier,
        payload: EventPayload::Tree { action },
    };
    root.present(&mut roots, [320.0, 480.0], 0).unwrap();
    for selection in [
        TreeAction::Select,
        TreeAction::AddToSelection,
        TreeAction::RemoveFromSelection,
    ] {
        root.event(&action(selection)).unwrap();
    }
    assert_eq!(
        root.event(&action(TreeAction::Expand)),
        Err(BackendError::InvalidEvent)
    );
    roots[0].descriptor.accessible_expanded = Some(false);
    root.present(&mut roots, [320.0, 480.0], 1).unwrap();
    root.event(&action(TreeAction::Expand)).unwrap();
    root.event(&action(TreeAction::Collapse)).unwrap();
    roots[0].descriptor.accessible_role = None;
    root.present(&mut roots, [320.0, 480.0], 2).unwrap();
    assert_eq!(
        root.event(&action(TreeAction::Select)),
        Err(BackendError::InvalidEvent)
    );
    roots[0].descriptor.accessible_role = Some(AccessibleRole::TreeItem);
    roots[0].descriptor.content = NodeContent::Button {
        label: "File".into(),
        enabled: false,
    };
    root.present(&mut roots, [320.0, 480.0], 3).unwrap();
    assert_eq!(
        root.event(&action(TreeAction::Select)),
        Err(BackendError::InvalidEvent)
    );
    root.present(&mut [], [320.0, 480.0], 4).unwrap();
    assert_eq!(
        root.event(&action(TreeAction::Select)),
        Err(BackendError::UnknownNode(identifier))
    );
    root.dispose();
    assert_eq!(
        root.event(&action(TreeAction::Select)),
        Err(BackendError::Disposed)
    );
}
use std::{cell::Cell, rc::Rc};

struct Host {
    publishes: Rc<Cell<usize>>,
    disposals: Rc<Cell<usize>>,
    fail: bool,
}
impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: request.key,
            width: 80.0f64.min(request.key.max_width),
            height: 24.0f64.min(request.key.max_height),
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        self.publishes.set(self.publishes.get() + 1);
        if self.fail {
            Err("injected failure".into())
        } else {
            Ok(())
        }
    }
    fn dispose(&mut self) {
        self.disposals.set(self.disposals.get() + 1);
    }
}
fn host(fail: bool) -> Host {
    Host {
        publishes: Rc::new(Cell::new(0)),
        disposals: Rc::new(Cell::new(0)),
        fail,
    }
}
fn nodes(root: RootId) -> Vec<PresentationNode> {
    vec![PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                value: String::new(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: Default::default(),
        },
        children: vec![],
    }]
}

#[test]
fn unchanged_frames_do_not_publish_and_disposal_is_idempotent() {
    let host = host(false);
    let publishes = host.publishes.clone();
    let disposals = host.disposals.clone();
    let mut root = NativeRoot::new(host);
    let mut nodes = nodes(root.model().id());
    root.present(&mut nodes, [320.0, 480.0], 1).unwrap();
    assert_eq!(
        root.present(&mut nodes, [320.0, 480.0], 1)
            .unwrap()
            .measurements,
        0
    );
    assert_eq!(publishes.get(), 1);
    root.present(&mut [], [320.0, 480.0], 1).unwrap();
    assert_eq!(publishes.get(), 2);
    assert!(root.model().roots().is_empty());
    let empty: serde_json::Value = serde_json::from_str(&wire::snapshot(
        root.model().id(),
        root.model().revision(),
        &[],
    ))
    .unwrap();
    assert_eq!(empty["root"], root.model().id().token());
    root.dispose();
    root.dispose();
    drop(root);
    assert_eq!(disposals.get(), 1);
}

#[test]
fn environment_remeasures_without_republishing_unchanged_geometry() {
    let host = host(false);
    let publishes = host.publishes.clone();
    let mut root = NativeRoot::new(host);
    let mut nodes = nodes(root.model().id());
    let first = root.present(&mut nodes, [320.0, 480.0], 1).unwrap();
    assert!(first.measurements > 0);
    let frame = nodes[0].descriptor.frame;
    root.event(&PlatformEvent {
        node: nodes[0].id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    let refreshed = root.present(&mut nodes, [320.0, 480.0], 2).unwrap();
    assert!(refreshed.measurements > 0);
    assert_eq!(nodes[0].descriptor.frame, frame);
    assert_eq!(root.model().focused(), Some(nodes[0].id));
    assert_eq!(publishes.get(), 1);
    assert_eq!(
        root.present(&mut nodes, [320.0, 480.0], 2)
            .unwrap()
            .measurements,
        0
    );
    assert_eq!(publishes.get(), 1);
}

#[test]
fn external_revocation_cleans_platform_views_before_rejecting_the_next_commit() {
    let host = host(false);
    let disposals = host.disposals.clone();
    let mut root = NativeRoot::new(host);
    let mut nodes = nodes(root.model().id());
    root.present(&mut nodes, [320.0, 480.0], 1).unwrap();
    root.model().lifetime().dispose();
    assert!(matches!(
        root.present(&mut nodes, [320.0, 480.0], 1),
        Err(HostError::Backend(BackendError::Disposed))
    ));
    assert_eq!(disposals.get(), 1);
}

#[test]
fn partial_platform_failure_revokes_the_entire_root_before_cleanup() {
    let host = host(true);
    let disposals = host.disposals.clone();
    let mut root = NativeRoot::new(host);
    let lifetime = root.model().lifetime();
    let mut nodes = nodes(root.model().id());
    assert!(matches!(
        root.present(&mut nodes, [320.0, 480.0], 1),
        Err(HostError::Platform(_))
    ));
    assert!(lifetime.is_disposed());
    assert!(root.model().roots().is_empty());
    assert_eq!(disposals.get(), 1);
}

#[test]
fn wire_preserves_decimal_identity_and_utf16_edit_validation() {
    let mut root = NativeRoot::new(host(false));
    let mut nodes = nodes(root.model().id());
    root.present(&mut nodes, [320.0, 480.0], 1).unwrap();
    let payload = serde_json::json!({ "root": root.model().id().token(), "node": nodes[0].id.token(), "payload": { "kind": "edit", "value": "中😀", "revision": "1", "selection": [3, 3], "composing": [0, 3] } });
    let event = wire::decode_event(&payload.to_string(), root.model()).unwrap();
    root.event(&event).unwrap();
    assert_eq!(root.event(&event), Err(BackendError::InvalidEvent));
    let mut malformed = payload.clone();
    malformed["payload"]["revision"] = serde_json::json!("2");
    malformed["payload"]["selection"] = serde_json::json!([2, 2]);
    let event = wire::decode_event(&malformed.to_string(), root.model()).unwrap();
    assert_eq!(root.event(&event), Err(BackendError::InvalidEvent));
    malformed["root"] = serde_json::json!("0");
    assert_eq!(
        wire::decode_event(&malformed.to_string(), root.model()),
        Err(BackendError::WrongRoot)
    );
}
