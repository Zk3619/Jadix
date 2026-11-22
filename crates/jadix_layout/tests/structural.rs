use jadix_core::presentation::{style::NodeStyle, *};
use jadix_layout::presentation::*;
fn descriptor(content: NodeContent) -> NodeDescriptor {
    NodeDescriptor {
        content,
        style: NodeStyle {
            shrink: 0.,
            ..Default::default()
        },
        frame: Rect::default(),
        accessible_description: None,
        accessible_name: None,
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
    }
}
fn measure(r: MeasureRequest, _: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    Ok(MeasureResult {
        key: r.key,
        width: 30.,
        height: 16.,
        baseline: None,
    })
}
fn commit(
    layout: &mut StructuralLayout,
    life: &RootLifetime,
    operations: Vec<Operation>,
) -> LayoutStats {
    let revision = layout.revision();
    let stats = layout
        .commit(
            &CommitBatch {
                root: life.id(),
                previous: revision,
                revision: revision + 1,
                operations,
            },
            [800., 2000.],
            0,
            measure,
        )
        .unwrap();
    let mut fresh = layout.roots().to_vec();
    PresentationLayout::new(life.clone())
        .layout(&mut fresh, [800., 2000.], 0, measure)
        .unwrap();
    assert_eq!(layout.roots(), fresh);
    stats
}
#[test]
fn buttons_keep_intrinsic_metrics_when_content_children_mount_and_unmount() {
    let life = RootLifetime::new(RootId::new());
    let button = NodeId::new(life.id());
    let child = NodeId::new(life.id());
    let mut layout = StructuralLayout::new(life.clone());
    commit(&mut layout, &life, vec![Operation::Create {
        node: button,
        parent: None,
        index: 0,
        descriptor: descriptor(NodeContent::Button { label: "plain".into(), enabled: true }),
    }]);
    assert_eq!(layout.roots()[0].descriptor.frame.height, 16.);
    let mut content = descriptor(NodeContent::Container);
    content.style.height = Some(48.);
    commit(&mut layout, &life, vec![Operation::Create {
        node: child,
        parent: Some(button),
        index: 0,
        descriptor: content,
    }]);
    assert_eq!(layout.roots()[0].descriptor.frame.height, 48.);
    commit(&mut layout, &life, vec![Operation::Remove { node: child }]);
    assert_eq!(layout.roots()[0].descriptor.frame.height, 16.);
    commit(&mut layout, &life, vec![Operation::Update {
        node: button,
        descriptor: descriptor(NodeContent::Button { label: "updated".into(), enabled: true }),
    }]);
    assert_eq!(layout.roots()[0].descriptor.frame.height, 16.);
}

#[test]
fn actual_delta_sync_visits_one_update_and_only_changed_parents_for_structure() {
    let life = RootLifetime::new(RootId::new());
    let root = NodeId::new(life.id());
    let mut layout = StructuralLayout::new(life.clone());
    let ids: Vec<_> = (0..1000).map(|_| NodeId::new(life.id())).collect();
    let mut operations = vec![Operation::Create {
        node: root,
        parent: None,
        index: 0,
        descriptor: descriptor(NodeContent::Container),
    }];
    operations.extend(
        ids.iter()
            .enumerate()
            .map(|(index, node)| Operation::Create {
                node: *node,
                parent: Some(root),
                index,
                descriptor: descriptor(NodeContent::Text(index.to_string())),
            }),
    );
    let first = commit(&mut layout, &life, operations);
    assert_eq!(first.sync_nodes, 1002);
    assert!(!first.incremental_sync);
    let update = commit(
        &mut layout,
        &life,
        vec![Operation::Update {
            node: ids[500],
            descriptor: descriptor(NodeContent::Text("updated".into())),
        }],
    );
    assert_eq!(update.sync_nodes, 1);
    assert!(update.incremental_sync);
    assert_eq!(update.engine_created, 0);
    let inserted = NodeId::new(life.id());
    let insert = commit(
        &mut layout,
        &life,
        vec![Operation::Create {
            node: inserted,
            parent: Some(root),
            index: 3,
            descriptor: descriptor(NodeContent::Text("insert".into())),
        }],
    );
    assert_eq!(insert.sync_nodes, 2);
    assert_eq!(insert.engine_created, 1);
    let moved = commit(
        &mut layout,
        &life,
        vec![Operation::Move {
            node: inserted,
            parent: Some(root),
            index: 900,
        }],
    );
    assert_eq!(moved.sync_nodes, 1);
    let removed = commit(
        &mut layout,
        &life,
        vec![Operation::Remove { node: inserted }],
    );
    assert_eq!(removed.sync_nodes, 1);
    assert_eq!(removed.engine_removed, 1);
    assert!(layout
        .commit(
            &CommitBatch {
                root: life.id(),
                previous: layout.revision(),
                revision: layout.revision() + 1,
                operations: vec![Operation::Create {
                    node: inserted,
                    parent: None,
                    index: 0,
                    descriptor: descriptor(NodeContent::Container)
                }]
            },
            [800., 2000.],
            0,
            measure
        )
        .is_err());
}
#[test]
fn cycles_failed_measure_environment_and_ancestor_reversal_are_atomic() {
    let life = RootLifetime::new(RootId::new());
    let a = NodeId::new(life.id());
    let b = NodeId::new(life.id());
    let leaf = NodeId::new(life.id());
    let mut layout = StructuralLayout::new(life.clone());
    commit(
        &mut layout,
        &life,
        vec![
            Operation::Create {
                node: a,
                parent: None,
                index: 0,
                descriptor: descriptor(NodeContent::Container),
            },
            Operation::Create {
                node: b,
                parent: Some(a),
                index: 0,
                descriptor: descriptor(NodeContent::Container),
            },
            Operation::Create {
                node: leaf,
                parent: Some(b),
                index: 0,
                descriptor: descriptor(NodeContent::Text("leaf".into())),
            },
        ],
    );
    let before = layout.roots().to_vec();
    let revision = layout.revision();
    let batch = |operations| CommitBatch {
        root: life.id(),
        previous: revision,
        revision: revision + 1,
        operations,
    };
    assert!(layout
        .commit(
            &batch(vec![Operation::Move {
                node: a,
                parent: Some(b),
                index: 0
            }]),
            [800., 2000.],
            0,
            measure
        )
        .is_err());
    assert_eq!(layout.roots(), before);
    assert_eq!(layout.revision(), revision);
    let update = batch(vec![Operation::Update {
        node: leaf,
        descriptor: descriptor(NodeContent::Text("change".into())),
    }]);
    assert!(layout
        .commit(&update, [800., 2000.], 0, |_, _| Err(LayoutError::Engine))
        .is_err());
    assert_eq!(layout.roots(), before);
    let retry = layout.commit(&update, [800., 2000.], 0, measure).unwrap();
    assert!(!retry.incremental_sync);
    let reversal = commit(
        &mut layout,
        &life,
        vec![
            Operation::Move {
                node: b,
                parent: None,
                index: 0,
            },
            Operation::Move {
                node: a,
                parent: Some(b),
                index: 0,
            },
        ],
    );
    assert!(reversal.incremental_sync);
    let env = layout
        .commit(
            &CommitBatch {
                root: life.id(),
                previous: layout.revision(),
                revision: layout.revision() + 1,
                operations: vec![],
            },
            [800., 2000.],
            1,
            measure,
        )
        .unwrap();
    assert!(!env.incremental_sync);
}
#[test]
fn structural_overlays_and_their_removal_use_explicit_full_sync_fallback() {
    let life = RootLifetime::new(RootId::new());
    let mut layout = StructuralLayout::new(life.clone());
    let dialog = NodeId::new(life.id());
    let child = NodeId::new(life.id());
    let stats = commit(
        &mut layout,
        &life,
        vec![
            Operation::Create {
                node: dialog,
                parent: None,
                index: 0,
                descriptor: descriptor(NodeContent::Dialog {
                    open: true,
                    revision: 0,
                    close_on_overlay_click: true,
                }),
            },
            Operation::Create {
                node: child,
                parent: Some(dialog),
                index: 0,
                descriptor: descriptor(NodeContent::Text("panel".into())),
            },
        ],
    );
    assert!(!stats.incremental_sync);
    assert!(
        !commit(
            &mut layout,
            &life,
            vec![Operation::Update {
                node: child,
                descriptor: descriptor(NodeContent::Text("updated".into()))
            }]
        )
        .incremental_sync
    );
    let removed = commit(&mut layout, &life, vec![Operation::Remove { node: dialog }]);
    assert!(!removed.incremental_sync);
    assert_eq!(removed.engine_removed, 3);
    assert!(commit(&mut layout, &life, vec![]).incremental_sync);
}

#[test]
fn delta_admission_rejects_invalid_descriptors_and_preserves_frames_revision_and_retry() {
    let life = RootLifetime::new(RootId::new());
    let mut layout = StructuralLayout::new(life.clone());
    let root = NodeId::new(life.id());
    let leaf = NodeId::new(life.id());
    commit(
        &mut layout,
        &life,
        vec![
            Operation::Create {
                node: root,
                parent: None,
                index: 0,
                descriptor: descriptor(NodeContent::Container),
            },
            Operation::Create {
                node: leaf,
                parent: Some(root),
                index: 0,
                descriptor: descriptor(NodeContent::Text("中文".into())),
            },
        ],
    );
    let mut width = descriptor(NodeContent::Text("text".into()));
    width.style.width = Some(f32::NAN);
    let mut inline = descriptor(NodeContent::Text("中文".into()));
    inline.style.inline_styles = vec![jadix_core::presentation::style::InlineStyle {
        range: [1, 4],
        underline: Some(true),
        ..Default::default()
    }];
    let bad_image = descriptor(NodeContent::Image {
        source: "data:image/png;base64,bad".into(),
        alt: "invalid".into(),
    });
    let parent_as_leaf = descriptor(NodeContent::Text("cannot own a child".into()));
    for (id, d, expected) in [
        (leaf, width, LayoutError::InvalidStyle),
        (leaf, inline, LayoutError::InvalidStyle),
        (leaf, bad_image, LayoutError::InvalidTree),
        (root, parent_as_leaf, LayoutError::InvalidTree),
    ] {
        let before = layout.roots().to_vec();
        let revision = layout.revision();
        let result = layout.commit(
            &CommitBatch {
                root: life.id(),
                previous: revision,
                revision: revision + 1,
                operations: vec![Operation::Update {
                    node: id,
                    descriptor: d,
                }],
            },
            [800., 2000.],
            0,
            |_, _| panic!("invalid descriptor reached measurement"),
        );
        assert_eq!(result, Err(expected));
        assert_eq!(layout.roots(), before);
        assert_eq!(layout.revision(), revision);
        commit(
            &mut layout,
            &life,
            vec![Operation::Update {
                node: leaf,
                descriptor: descriptor(NodeContent::Text(format!("valid {revision}"))),
            }],
        );
    }
}

#[test]
fn borrowed_candidate_keeps_authored_frames_and_discards_rejected_root_order_and_retirement() {
    let life = RootLifetime::new(RootId::new());
    let mut layout = StructuralLayout::new(life.clone());
    let a = NodeId::new(life.id());
    let b = NodeId::new(life.id());
    commit(
        &mut layout,
        &life,
        vec![
            Operation::Create {
                node: a,
                parent: None,
                index: 0,
                descriptor: descriptor(NodeContent::Text("first".into())),
            },
            Operation::Create {
                node: b,
                parent: None,
                index: 1,
                descriptor: descriptor(NodeContent::Text("second".into())),
            },
        ],
    );
    let before = layout.roots().to_vec();
    let revision = layout.revision();
    assert_ne!(before[1].descriptor.frame, Rect::default());
    let rejected = CommitBatch {
        root: life.id(),
        previous: revision,
        revision: revision + 1,
        operations: vec![
            Operation::Move {
                node: b,
                parent: None,
                index: 0,
            },
            Operation::Remove { node: a },
        ],
    };
    assert_eq!(
        layout.commit_checked(&rejected, [800., 2000.], 0, measure, |candidate| {
            assert_eq!(candidate.len(), 1);
            assert_eq!(candidate[0].id, b);
            assert_eq!(
                candidate[0].descriptor.frame,
                Rect::default(),
                "host validation consumes authored descriptors"
            );
            Err(LayoutError::MeasurementUnavailable(b))
        }),
        Err(LayoutError::MeasurementUnavailable(b))
    );
    assert_eq!(layout.roots(), before);
    assert_eq!(layout.revision(), revision);
    commit(
        &mut layout,
        &life,
        vec![Operation::Update {
            node: a,
            descriptor: descriptor(NodeContent::Text("still live".into())),
        }],
    );
    assert_eq!(layout.roots()[0].id, a);
    assert_eq!(layout.roots()[1].id, b);
}

#[test]
fn failed_candidate_measurements_do_not_retire_unpublished_nodes_or_poison_a_different_retry() {
    let life = RootLifetime::new(RootId::new());
    let mut layout = StructuralLayout::new(life.clone());
    let parent = NodeId::new(life.id());
    let a = NodeId::new(life.id());
    let abandoned = NodeId::new(life.id());
    let replacement = NodeId::new(life.id());
    commit(
        &mut layout,
        &life,
        vec![
            Operation::Create {
                node: parent,
                parent: None,
                index: 0,
                descriptor: descriptor(NodeContent::Container),
            },
            Operation::Create {
                node: a,
                parent: Some(parent),
                index: 0,
                descriptor: descriptor(NodeContent::Text("a".into())),
            },
        ],
    );
    let before = layout.roots().to_vec();
    let revision = layout.revision();
    let batch = CommitBatch {
        root: life.id(),
        previous: revision,
        revision: revision + 1,
        operations: vec![
            Operation::Create {
                node: abandoned,
                parent: Some(parent),
                index: 0,
                descriptor: descriptor(NodeContent::Text("unpublished".into())),
            },
            Operation::Update {
                node: a,
                descriptor: descriptor(NodeContent::Text("fail".into())),
            },
        ],
    };
    let result = layout.commit(&batch, [800., 2000.], 0, |r, d| {
        if r.key.node == a {
            Err(LayoutError::MeasurementUnavailable(a))
        } else {
            measure(r, d)
        }
    });
    assert_eq!(result, Err(LayoutError::MeasurementUnavailable(a)));
    assert_eq!(layout.roots(), before);
    assert_eq!(layout.revision(), revision);
    commit(
        &mut layout,
        &life,
        vec![Operation::Create {
            node: replacement,
            parent: Some(parent),
            index: 0,
            descriptor: descriptor(NodeContent::Text("replacement".into())),
        }],
    );
    commit(
        &mut layout,
        &life,
        vec![Operation::Create {
            node: abandoned,
            parent: Some(parent),
            index: 1,
            descriptor: descriptor(NodeContent::Text("now accepted".into())),
        }],
    );
    assert_eq!(
        layout.roots()[0]
            .children
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        [replacement, abandoned, a]
    );
}

#[test]
fn warm_leaf_batches_match_checked_candidates_across_duplicate_updates_resize_and_rebuilds() {
    fn bounded_measure(
        r: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        Ok(MeasureResult {
            key: r.key,
            width: 30_f64.min(r.key.max_width),
            height: 16_f64.min(r.key.max_height),
            baseline: None,
        })
    }
    let life = RootLifetime::new(RootId::new());
    let root = NodeId::new(life.id());
    let ids: Vec<_> = (0..12).map(|_| NodeId::new(life.id())).collect();
    let mut mount = vec![Operation::Create {
        node: root,
        parent: None,
        index: 0,
        descriptor: descriptor(NodeContent::Container),
    }];
    mount.extend(
        ids.iter()
            .enumerate()
            .map(|(index, &node)| Operation::Create {
                node,
                parent: Some(root),
                index,
                descriptor: descriptor(NodeContent::Text(index.to_string())),
            }),
    );
    let mut fast = StructuralLayout::new(life.clone());
    let mut checked = StructuralLayout::new(life.clone());
    let mut viewport = [800., 2000.];
    let mut environment = 0;
    let mut batch = CommitBatch {
        root: life.id(),
        previous: 0,
        revision: 1,
        operations: mount,
    };
    fast.commit(&batch, viewport, environment, bounded_measure)
        .unwrap();
    checked
        .commit_checked(&batch, viewport, environment, bounded_measure, |_| Ok(()))
        .unwrap();
    for turn in 0..72 {
        let id = ids[(turn * 7) % ids.len()];
        let mut next = descriptor(match turn % 3 {
            0 => NodeContent::Text(format!("changed {turn}")),
            1 => NodeContent::Button {
                label: format!("button {turn}"),
                enabled: turn % 2 == 0,
            },
            _ => NodeContent::Container,
        });
        next.style.width = Some(24. + (turn % 8) as f32 * 17.);
        next.style.height = Some(10. + (turn % 5) as f32 * 11.);
        next.frame = Rect {
            x: turn as f64,
            y: 7.,
            width: 1.,
            height: 2.,
        };
        next.accessible_name = Some(format!("name {turn}"));
        batch.previous = fast.revision();
        batch.revision = batch.previous + 1;
        let mut invalid_then_replaced = next.clone();
        invalid_then_replaced.style.width = Some(f32::NAN);
        batch.operations = vec![
            Operation::Update {
                node: id,
                descriptor: invalid_then_replaced,
            },
            Operation::Update {
                node: id,
                descriptor: next,
            },
        ];
        if turn % 13 == 0 {
            viewport[0] += 8.;
        }
        if turn % 17 == 0 {
            environment += 1;
        }
        if turn % 19 == 0 {
            fast.release_layout_cache();
            checked.release_layout_cache();
        }
        fast.commit(&batch, viewport, environment, bounded_measure)
            .unwrap();
        checked
            .commit_checked(&batch, viewport, environment, bounded_measure, |_| Ok(()))
            .unwrap();
        assert_eq!(fast.roots(), checked.roots(), "turn={turn}");
        assert_eq!(fast.revision(), checked.revision());
    }
    // The authored records updated by the warm path also support later creates.
    let child = NodeId::new(life.id());
    batch.previous = fast.revision();
    batch.revision = batch.previous + 1;
    batch.operations = vec![Operation::Create {
        node: child,
        parent: Some(ids[2]),
        index: 0,
        descriptor: descriptor(NodeContent::Text("new child".into())),
    }];
    fast.commit(&batch, viewport, environment, bounded_measure)
        .unwrap();
    checked
        .commit_checked(&batch, viewport, environment, bounded_measure, |_| Ok(()))
        .unwrap();
    assert_eq!(fast.roots(), checked.roots());
}

#[test]
fn warm_leaf_failure_mid_measure_preserves_every_frame_and_checked_authored_descriptors() {
    let life = RootLifetime::new(RootId::new());
    let ids: Vec<_> = (0..3).map(|_| NodeId::new(life.id())).collect();
    let mut layout = StructuralLayout::new(life.clone());
    commit(
        &mut layout,
        &life,
        ids.iter()
            .enumerate()
            .map(|(index, &node)| Operation::Create {
                node,
                parent: None,
                index,
                descriptor: descriptor(NodeContent::Text(format!("first {index}"))),
            })
            .collect(),
    );
    let before = layout.roots().to_vec();
    let previous = layout.revision();
    let batch = CommitBatch {
        root: life.id(),
        previous,
        revision: previous + 1,
        operations: ids
            .iter()
            .map(|&node| Operation::Update {
                node,
                descriptor: descriptor(NodeContent::Text("new".into())),
            })
            .collect(),
    };
    let mut measured = std::collections::BTreeSet::new();
    let failure = layout
        .commit(&batch, [800., 2000.], 0, |request, node| {
            measured.insert(request.key.node);
            if measured.len() >= 2 {
                Err(LayoutError::MeasurementUnavailable(request.key.node))
            } else {
                measure(request, node)
            }
        })
        .unwrap_err();
    let LayoutError::MeasurementUnavailable(failed_node) = failure else {
        panic!("unexpected measurement error: {failure:?}")
    };
    assert!(measured.contains(&failed_node));
    assert!(measured.len() >= 2);
    assert_eq!(layout.revision(), previous);
    assert_eq!(layout.roots(), before);
    let stats = layout.commit(&batch, [800., 2000.], 0, measure).unwrap();
    assert!(
        !stats.incremental_sync,
        "failed engine must rebuild on retry"
    );
    let mut next = batch.clone();
    next.previous += 1;
    next.revision += 1;
    next.operations = vec![Operation::Update {
        node: ids[0],
        descriptor: descriptor(NodeContent::Text("warm accepted".into())),
    }];
    assert!(
        layout
            .commit(&next, [800., 2000.], 0, measure)
            .unwrap()
            .incremental_sync
    );
    let before = layout.roots().to_vec();
    next.previous += 1;
    next.revision += 1;
    next.operations = vec![Operation::Update {
        node: ids[1],
        descriptor: descriptor(NodeContent::Text("rejected".into())),
    }];
    assert_eq!(
        layout.commit_checked(&next, [800., 2000.], 0, measure, |candidate| {
            assert_eq!(
                candidate[0].descriptor.content,
                NodeContent::Text("warm accepted".into())
            );
            for node in candidate {
                assert_eq!(node.descriptor.frame, Rect::default());
            }
            Err(LayoutError::InvalidTree)
        }),
        Err(LayoutError::InvalidTree)
    );
    assert_eq!(layout.roots(), before);
    assert_eq!(layout.revision(), next.previous);
}

#[test]
fn warm_leaf_revision_root_unknown_and_oversized_batch_rejections_do_not_publish() {
    let life = RootLifetime::new(RootId::new());
    let id = NodeId::new(life.id());
    let mut layout = StructuralLayout::new(life.clone());
    commit(
        &mut layout,
        &life,
        vec![Operation::Create {
            node: id,
            parent: None,
            index: 0,
            descriptor: descriptor(NodeContent::Text("before".into())),
        }],
    );
    let before = layout.roots().to_vec();
    let good = CommitBatch {
        root: life.id(),
        previous: layout.revision(),
        revision: layout.revision() + 1,
        operations: vec![Operation::Update {
            node: id,
            descriptor: descriptor(NodeContent::Text("after".into())),
        }],
    };
    for variant in 0..5 {
        let mut bad = good.clone();
        match variant {
            0 => bad.root = RootId::new(),
            1 => bad.previous -= 1,
            2 => bad.revision += 1,
            3 => {
                bad.operations = vec![Operation::Update {
                    node: NodeId::new(life.id()),
                    descriptor: descriptor(NodeContent::Text("unknown".into())),
                }]
            }
            _ => bad.operations = vec![good.operations[0].clone(); 65537],
        }
        assert!(layout
            .commit(&bad, [800., 2000.], 0, |_, _| panic!(
                "bad batch reached measurement"
            ))
            .is_err());
        assert_eq!(layout.roots(), before);
        assert_eq!(layout.revision(), good.previous);
    }
    layout.commit(&good, [800., 2000.], 0, measure).unwrap();
    assert_eq!(
        layout.roots()[0].descriptor.content,
        NodeContent::Text("after".into())
    );
}
