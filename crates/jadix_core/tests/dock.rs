use jadix_core::dock::{Bounds, Dock, DockError, DockLimits, DockNode, DropLocation, Edge, Pane};

fn bounds() -> Bounds {
    Bounds {
        origin: [80.0, 80.0],
        size: [960.0, 640.0],
    }
}

fn dock() -> Dock {
    Dock::new(
        (1..=4)
            .map(|key| Pane {
                key,
                title: format!("Document {key} / 中文 😀"),
                closable: key != 1,
            })
            .collect(),
        bounds(),
        DockLimits::default(),
    )
    .unwrap()
}

#[test]
fn split_gestures_have_one_undo_step_and_cancel_preserves_redo() {
    let mut model = dock();
    model.relocate(2, DropLocation::Split { stack: 1, edge: Edge::Right }).unwrap();
    let split = model.surfaces()[0].root.key();
    let original = model.to_json().unwrap();
    model.begin_resize(split).unwrap();
    for step in 0..1000 { model.resize_split(split, 0.5 + step as f64 / 2500.).unwrap(); }
    assert!(model.finish_resize(false).unwrap());
    let resized = model.to_json().unwrap();
    model.undo().unwrap();
    assert_eq!(model.to_json().unwrap(), original);
    model.begin_resize(split).unwrap();
    model.resize_split(split, 0.2).unwrap();
    assert!(model.finish_resize(true).unwrap());
    assert_eq!(model.to_json().unwrap(), original);
    assert!(model.redo().unwrap());
    assert_eq!(model.to_json().unwrap(), resized);
    model.begin_resize(split).unwrap();
    model.resize_split(split, 0.3).unwrap();
    model.activate(3).unwrap(); // An independent command closes the gesture.
    model.undo().unwrap();
    assert!(matches!(model.surfaces()[0].root, DockNode::Split { ratio, .. } if ratio == 0.3));
    model.undo().unwrap();
    assert_eq!(model.to_json().unwrap(), resized);
}

#[test]
fn split_move_float_close_and_undo_keep_unique_business_keys_and_monotonic_layout_ids() {
    let mut model = dock();
    model
        .relocate(
            2,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Right,
            },
        )
        .unwrap();
    let (_, right) = model.stack_of(2).unwrap();
    let split = model.surfaces()[0].root.key();
    model.resize_split(split, 0.7).unwrap();
    model
        .relocate(
            3,
            DropLocation::Tab {
                stack: right,
                index: 1,
            },
        )
        .unwrap();
    model
        .relocate(2, DropLocation::Floating { bounds: bounds() })
        .unwrap();
    let (window, floating_stack) = model.stack_of(2).unwrap();
    assert_eq!(model.surfaces().len(), 2);
    model.dock_surface(window, right).unwrap();
    assert_eq!(model.stack_of(2), Some((1, right)));
    assert!(model.undo().unwrap());
    assert_eq!(model.stack_of(2), Some((window, floating_stack)));
    assert!(model.redo().unwrap());
    model.close(3).unwrap();
    assert_eq!(model.stack_of(3), None);
    assert!(model.undo().unwrap());
    assert_eq!(model.stack_of(3), Some((1, right)));
    model
        .relocate(2, DropLocation::Floating { bounds: bounds() })
        .unwrap();
    let (next_window, next_stack) = model.stack_of(2).unwrap();
    assert!(next_window > window);
    assert!(next_stack > floating_stack);
    assert!(!model.redo().unwrap());
    assert_eq!(model.close(1), Err(DockError::ProtectedPane));
    let revision = model.revision();
    model.activate(2).unwrap();
    assert_eq!(model.revision(), revision);
}

#[test]
fn failed_layout_commands_are_atomic_and_empty_stacks_collapse_without_emptying_main_window() {
    let mut model = dock();
    let before = model.to_json().unwrap();
    assert_eq!(
        model.relocate(
            2,
            DropLocation::Split {
                stack: 99,
                edge: Edge::Left
            }
        ),
        Err(DockError::UnknownStack)
    );
    assert_eq!(
        model.relocate(2, DropLocation::Tab { stack: 1, index: 9 }),
        Err(DockError::InvalidPosition)
    );
    assert_eq!(
        model.resize_split(1, f64::NAN),
        Err(DockError::InvalidRatio)
    );
    assert_eq!(model.to_json().unwrap(), before);
    assert_eq!(model.revision(), 0);
    model
        .relocate(
            2,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Top,
            },
        )
        .unwrap();
    model
        .relocate(2, DropLocation::Tab { stack: 1, index: 0 })
        .unwrap();
    assert!(
        matches!(&model.surfaces()[0].root, DockNode::Stack { panes, .. } if panes == &[2, 1, 3, 4])
    );
    model
        .relocate(2, DropLocation::Tab { stack: 1, index: 4 })
        .unwrap();
    assert!(
        matches!(&model.surfaces()[0].root, DockNode::Stack { panes, .. } if panes == &[1, 3, 4, 2])
    );
    for pane in [2, 3, 4] {
        model
            .relocate(pane, DropLocation::Floating { bounds: bounds() })
            .unwrap();
    }
    let before = model.to_json().unwrap();
    assert_eq!(
        model.relocate(1, DropLocation::Floating { bounds: bounds() }),
        Err(DockError::MainWindowEmpty)
    );
    assert_eq!(model.to_json().unwrap(), before);
}

#[test]
fn drag_cancel_stale_revision_foreign_owner_and_failed_drop_cannot_move_a_pane() {
    let mut model = dock();
    let before = model.to_json().unwrap();
    let token = model.begin_drag(2).unwrap();
    assert_eq!(model.begin_drag(3), Err(DockError::DragBusy));
    assert_eq!(
        model.drop_drag(
            &token,
            DropLocation::Tab {
                stack: 99,
                index: 0
            }
        ),
        Err(DockError::UnknownStack)
    );
    assert_eq!(model.dragging(), Some(2));
    model.cancel_drag(&token).unwrap();
    assert_eq!(model.dragging(), None);
    assert_eq!(model.to_json().unwrap(), before);
    assert_eq!(
        model.drop_drag(&token, DropLocation::Floating { bounds: bounds() }),
        Err(DockError::StaleDrag)
    );
    let token = model.begin_drag(2).unwrap();
    let mut foreign = dock();
    foreign.begin_drag(2).unwrap();
    assert_eq!(foreign.cancel_drag(&token), Err(DockError::StaleDrag));
    model.activate(3).unwrap();
    assert_eq!(model.cancel_drag(&token), Err(DockError::StaleDrag));
    let token = model.begin_drag(2).unwrap();
    model
        .drop_drag(
            &token,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Bottom,
            },
        )
        .unwrap();
    assert_eq!(model.dragging(), None);
    assert_ne!(model.stack_of(2).unwrap().1, 1);
}

#[test]
fn bounded_versioned_persistence_rejects_duplicate_unknown_and_unplaced_panels_without_partial_restore(
) {
    let mut model = dock();
    model
        .relocate(
            3,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Right,
            },
        )
        .unwrap();
    model
        .relocate(4, DropLocation::Floating { bounds: bounds() })
        .unwrap();
    let bytes = model.to_json().unwrap();
    let restored = Dock::from_json(&bytes, DockLimits::default()).unwrap();
    assert_eq!(restored.to_json().unwrap(), bytes);
    for mutation in [0, 1, 2, 3] {
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        match mutation {
            0 => value["version"] = 2.into(),
            1 => value["panes"][0]["key"] = 2.into(),
            2 => value["surfaces"][1]["root"]["active"] = 999.into(),
            _ => value["extra"] = true.into(),
        }
        assert!(model.restore(&serde_json::to_vec(&value).unwrap()).is_err());
        assert_eq!(model.to_json().unwrap(), bytes);
    }
    assert!(matches!(
        Dock::from_json(
            &bytes,
            DockLimits {
                panes: 3,
                ..Default::default()
            }
        ),
        Err(DockError::Budget)
    ));
    assert!(matches!(
        Dock::from_json(
            &bytes,
            DockLimits {
                serialized_bytes: bytes.len() - 1,
                ..Default::default()
            }
        ),
        Err(DockError::Budget)
    ));
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["surfaces"][0]["root"]["ratio"] = 0.01.into();
    assert_eq!(
        model.restore(&serde_json::to_vec(&value).unwrap()),
        Err(DockError::InvalidRatio)
    );
}

#[test]
fn history_depth_and_surface_limits_bound_resources_and_restore_is_itself_undoable() {
    let mut model = Dock::new(
        dock().panes().to_vec(),
        bounds(),
        DockLimits {
            windows: 1,
            history: 2,
            depth: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        model.relocate(2, DropLocation::Floating { bounds: bounds() }),
        Err(DockError::Budget)
    );
    model
        .relocate(
            2,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Right,
            },
        )
        .unwrap();
    let before = model.to_json().unwrap();
    assert_eq!(
        model.relocate(
            3,
            DropLocation::Split {
                stack: 1,
                edge: Edge::Bottom
            }
        ),
        Err(DockError::Budget)
    );
    assert_eq!(model.to_json().unwrap(), before);
    for pane in [3, 4, 1] {
        model.activate(pane).unwrap();
    }
    assert!(model.undo().unwrap());
    assert!(model.undo().unwrap());
    assert!(!model.undo().unwrap());
    model.restore(&dock().to_json().unwrap()).unwrap();
    assert!(matches!(model.surfaces()[0].root, DockNode::Stack { .. }));
    assert!(model.undo().unwrap());
    assert!(matches!(model.surfaces()[0].root, DockNode::Split { .. }));
}

#[test]
fn rejoining_tabbed_surfaces_preserves_selected_documents_in_one_undoable_operation() {
    let mut model = dock();
    model
        .relocate(2, DropLocation::Floating { bounds: bounds() })
        .unwrap();
    let (surface, stack) = model.stack_of(2).unwrap();
    model
        .relocate(3, DropLocation::Tab { stack, index: 1 })
        .unwrap();
    model.dock_surface(surface, 1).unwrap();
    assert!(matches!(
        model.surfaces()[0].root,
        DockNode::Stack { active: 3, .. }
    ));
    model.undo().unwrap();
    assert_eq!(model.stack_of(3), Some((surface, stack)));
    let before = model.to_json().unwrap();
    assert_eq!(
        model.dock_surface_with_active(surface, 1, Some(99)),
        Err(DockError::UnknownPane)
    );
    assert_eq!(model.to_json().unwrap(), before);
    model.dock_surface_with_active(surface, 1, Some(1)).unwrap();
    assert!(matches!(
        model.surfaces()[0].root,
        DockNode::Stack { active: 1, .. }
    ));
    model.undo().unwrap();
    assert_eq!(model.surfaces().len(), 2);
}
