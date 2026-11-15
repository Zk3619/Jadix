use jadix_core::presentation::{style::NodeStyle, *};
use jadix_layout::presentation::*;
fn descriptor(text: &str) -> NodeDescriptor {
    NodeDescriptor {
        content: NodeContent::Text(text.into()),
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
        accessibility_relations: Default::default(),
    }
}
fn ink(request: MeasureRequest, node: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    let text = match &node.content {
        NodeContent::Text(text) => text.as_str(),
        NodeContent::Button { label, .. } => label,
        _ => "",
    };
    let width = text
        .lines()
        .map(|line| {
            line.chars()
                .map(|c| {
                    if c == 'W' {
                        13.
                    } else if c == 'i' {
                        3.
                    } else {
                        8.
                    }
                })
                .sum::<f64>()
        })
        .fold(0_f64, f64::max)
        .min(request.key.max_width);
    let height = (text.lines().count().max(1) as f64 * 16.).min(request.key.max_height);
    Ok(MeasureResult {
        key: request.key,
        width,
        height,
        baseline: Some((if text.contains('!') { 8_f64 } else { 12_f64 }).min(height)),
    })
}
fn mount() -> (RootLifetime, StructuralLayout, Vec<NodeId>) {
    let life = RootLifetime::new(RootId::new());
    let ids: Vec<_> = (0..3).map(|_| NodeId::new(life.id())).collect();
    let mut parent = descriptor("");
    parent.content = NodeContent::Container;
    let mut layout = StructuralLayout::new(life.clone());
    layout
        .commit(
            &CommitBatch {
                root: life.id(),
                previous: 0,
                revision: 1,
                operations: vec![
                    Operation::Create {
                        node: ids[0],
                        parent: None,
                        index: 0,
                        descriptor: parent,
                    },
                    Operation::Create {
                        node: ids[1],
                        parent: Some(ids[0]),
                        index: 0,
                        descriptor: descriptor("ab"),
                    },
                    Operation::Create {
                        node: ids[2],
                        parent: Some(ids[0]),
                        index: 1,
                        descriptor: descriptor("ef"),
                    },
                ],
            },
            [320., 480.],
            0,
            ink,
        )
        .unwrap();
    (life, layout, ids)
}
fn batch(
    layout: &StructuralLayout,
    life: &RootLifetime,
    operations: Vec<Operation>,
) -> CommitBatch {
    CommitBatch {
        root: life.id(),
        previous: layout.revision(),
        revision: layout.revision() + 1,
        operations,
    }
}
fn fresh(layout: &StructuralLayout, life: &RootLifetime, viewport: [f32; 2]) {
    let mut nodes = layout.roots().to_vec();
    PresentationLayout::new(life.clone())
        .layout(&mut nodes, viewport, 0, ink)
        .unwrap();
    assert_eq!(layout.roots(), nodes);
}
#[test]
fn complete_queries_prove_same_geometry_but_equal_length_different_ink_and_baseline_fall_back() {
    let (life, mut layout, ids) = mount();
    for (text, reused) in [
        ("cd", true),
        ("ii", false),
        ("WW", false),
        ("ab", false),
        ("a!", false),
        ("b!", true),
    ] {
        let update = batch(
            &layout,
            &life,
            vec![Operation::Update {
                node: ids[1],
                descriptor: descriptor(text),
            }],
        );
        let stats = layout.commit(&update, [320., 480.], 0, ink).unwrap();
        assert_eq!(stats.geometry_reused, reused, "text={text}");
        assert!(stats.measurements > 0);
        fresh(&layout, &life, [320., 480.]);
    }
}
#[test]
fn request_eviction_fails_closed_then_new_complete_content_can_prove_reuse_again() {
    let (life, mut layout, ids) = mount();
    for width in 100..145 {
        layout.relayout([width as f32, 480.], 0, ink).unwrap();
    }
    let viewport = [144., 480.];
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: descriptor("cd"),
        }],
    );
    assert!(
        !layout
            .commit(&update, viewport, 0, ink)
            .unwrap()
            .geometry_reused,
        "evicted requests must prevent a geometry proof"
    );
    fresh(&layout, &life, viewport);
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: descriptor("ef"),
        }],
    );
    assert!(
        layout
            .commit(&update, viewport, 0, ink)
            .unwrap()
            .geometry_reused
    );
    fresh(&layout, &life, viewport);
}
#[test]
fn multi_leaf_probe_failure_and_size_change_do_not_publish_partial_cache_or_snapshots() {
    let (life, mut layout, ids) = mount();
    let before = layout.roots().to_vec();
    let update = batch(
        &layout,
        &life,
        vec![
            Operation::Update {
                node: ids[1],
                descriptor: descriptor("cd"),
            },
            Operation::Update {
                node: ids[2],
                descriptor: descriptor("gh"),
            },
        ],
    );
    let mut visited = std::collections::BTreeSet::new();
    assert!(layout
        .commit(&update, [320., 480.], 0, |r, n| {
            visited.insert(r.key.node);
            if visited.len() > 1 {
                Err(LayoutError::MeasurementUnavailable(r.key.node))
            } else {
                ink(r, n)
            }
        })
        .is_err());
    assert!(visited.len() > 1);
    assert_eq!(layout.revision(), update.previous);
    assert_eq!(layout.roots(), before);
    assert!(
        !layout
            .commit(&update, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    let mixed = batch(
        &layout,
        &life,
        vec![
            Operation::Update {
                node: ids[1],
                descriptor: descriptor("ef"),
            },
            Operation::Update {
                node: ids[2],
                descriptor: descriptor("WW"),
            },
        ],
    );
    assert!(
        !layout
            .commit(&mixed, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    fresh(&layout, &life, [320., 480.]);
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: descriptor("gh"),
        }],
    );
    assert!(
        layout
            .commit(&update, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    fresh(&layout, &life, [320., 480.]);
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: descriptor("jk"),
        }],
    );
    assert_eq!(
        layout.commit(&update, [320., 480.], 0, |r, n| {
            let mut result = ink(r, n)?;
            result.key.content_revision -= 1;
            Ok(result)
        }),
        Err(LayoutError::InvalidMeasurement(ids[1]))
    );
    assert_eq!(layout.revision(), update.previous);
}
#[test]
fn accepted_root_reorder_reparent_and_sibling_shifts_keep_sparse_snapshot_positions_correct() {
    let (life, mut layout, ids) = mount();
    let other = NodeId::new(life.id());
    let leaf = NodeId::new(life.id());
    let mut container = descriptor("");
    container.content = NodeContent::Container;
    let structure = batch(
        &layout,
        &life,
        vec![
            Operation::Create {
                node: other,
                parent: None,
                index: 1,
                descriptor: container,
            },
            Operation::Create {
                node: leaf,
                parent: Some(other),
                index: 0,
                descriptor: descriptor("mn"),
            },
            Operation::Move {
                node: ids[1],
                parent: Some(other),
                index: 0,
            },
            Operation::Move {
                node: other,
                parent: None,
                index: 0,
            },
            Operation::Move {
                node: ids[2],
                parent: None,
                index: 2,
            },
        ],
    );
    layout.commit(&structure, [320., 480.], 0, ink).unwrap();
    let update = batch(
        &layout,
        &life,
        vec![
            Operation::Update {
                node: ids[1],
                descriptor: descriptor("cd"),
            },
            Operation::Update {
                node: ids[2],
                descriptor: descriptor("gh"),
            },
            Operation::Update {
                node: leaf,
                descriptor: descriptor("op"),
            },
        ],
    );
    assert!(
        layout
            .commit(&update, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    fresh(&layout, &life, [320., 480.]);
    assert_eq!(layout.roots()[0].id, other);
    assert_eq!(
        layout.roots()[0].children[0].descriptor.content,
        NodeContent::Text("cd".into())
    );
    assert_eq!(
        layout.roots()[0].children[1].descriptor.content,
        NodeContent::Text("op".into())
    );
    assert_eq!(
        layout.roots()[2].descriptor.content,
        NodeContent::Text("gh".into())
    );
}
#[test]
fn metadata_style_environment_viewport_and_checked_callbacks_keep_explicit_full_fallbacks() {
    let (life, mut layout, ids) = mount();
    let mut next = descriptor("cd");
    next.accessible_name = Some("description".into());
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: next.clone(),
        }],
    );
    assert!(
        !layout
            .commit(&update, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    next.content = NodeContent::Text("ef".into());
    next.style.font_size += 1.;
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: next.clone(),
        }],
    );
    assert!(
        !layout
            .commit(&update, [320., 480.], 0, ink)
            .unwrap()
            .geometry_reused
    );
    next.content = NodeContent::Text("gh".into());
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: next.clone(),
        }],
    );
    assert!(
        !layout
            .commit(&update, [328., 480.], 1, ink)
            .unwrap()
            .geometry_reused
    );
    next.content = NodeContent::Text("jk".into());
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: next,
        }],
    );
    let mut called = false;
    assert!(
        !layout
            .commit_checked(&update, [328., 480.], 1, ink, |nodes| {
                called = true;
                assert_eq!(nodes[0].children[0].descriptor.frame, Rect::default());
                Ok(())
            })
            .unwrap()
            .geometry_reused
    );
    assert!(called);
}

#[test]
fn rejected_historical_probe_falls_back_instead_of_rejecting_a_valid_current_layout() {
    for invalid_result in [false, true] {
        let (life, mut layout, ids) = mount();
        fn same_shape(a: MeasureRequest, b: MeasureRequest) -> bool {
            a.key.node == b.key.node
                && a.key.max_width == b.key.max_width
                && a.key.max_height == b.key.max_height
                && a.width_mode == b.width_mode
                && a.height_mode == b.height_mode
        }
        let mut old = Vec::new();
        let mut tree = layout.roots().to_vec();
        PresentationLayout::new(life.clone())
            .layout(&mut tree, [320., 480.], 0, |r, n| {
                if r.key.node == ids[1] {
                    old.push(r);
                }
                ink(r, n)
            })
            .unwrap();
        layout.relayout([640., 480.], 0, ink).unwrap();
        let mut current = Vec::new();
        let mut tree = layout.roots().to_vec();
        PresentationLayout::new(life.clone())
            .layout(&mut tree, [640., 480.], 0, |r, n| {
                if r.key.node == ids[1] {
                    current.push(r);
                }
                ink(r, n)
            })
            .unwrap();
        let stale = *old
            .iter()
            .find(|&&before| !current.iter().any(|&now| same_shape(before, now)))
            .expect("fixture must include an obsolete measurement constraint");
        let update = batch(
            &layout,
            &life,
            vec![Operation::Update {
                node: ids[1],
                descriptor: descriptor("cd"),
            }],
        );
        let mut rejected = 0;
        let stats = layout
            .commit(&update, [640., 480.], 0, |r, n| {
                if same_shape(r, stale) {
                    rejected += 1;
                    if invalid_result {
                        let mut result = ink(r, n)?;
                        result.key.content_revision += 1;
                        Ok(result)
                    } else {
                        Err(LayoutError::MeasurementUnavailable(r.key.node))
                    }
                } else {
                    ink(r, n)
                }
            })
            .unwrap();
        assert!(
            rejected > 0,
            "a historical-only query must actually have been probed"
        );
        assert!(!stats.geometry_reused);
        fresh(&layout, &life, [640., 480.]);
    }
}

#[test]
fn disposal_during_geometry_proof_cancels_before_any_acceptance_or_fallback() {
    for unavailable in [false, true] {
        let (life, mut layout, ids) = mount();
        let revision = layout.revision();
        let update = batch(
            &layout,
            &life,
            vec![Operation::Update {
                node: ids[1],
                descriptor: descriptor("cd"),
            }],
        );
        let mut calls = 0;
        let result = layout.commit(&update, [320., 480.], 0, |r, n| {
            calls += 1;
            life.dispose();
            if unavailable {
                Err(LayoutError::MeasurementUnavailable(r.key.node))
            } else {
                ink(r, n)
            }
        });
        assert!(matches!(result, Err(LayoutError::Disposed)));
        assert_eq!(
            calls, 1,
            "disposed measurement must not start fallback layout"
        );
        assert_eq!(layout.revision(), revision);
        assert!(layout.roots().is_empty());
        assert!(matches!(
            layout.commit(&update, [320., 480.], 0, ink),
            Err(LayoutError::Disposed)
        ));
    }
}

#[test]
fn geometry_frame_stream_follows_reordered_nested_and_multiple_roots() {
    let (life, mut layout, ids) = mount();
    let other = NodeId::new(life.id());
    let nested = NodeId::new(life.id());
    let empty = NodeId::new(life.id());
    let mut parent = descriptor("");
    parent.content = NodeContent::Container;
    let mut zero = descriptor("");
    zero.style.width = Some(0.);
    zero.style.height = Some(0.);
    let change = batch(
        &layout,
        &life,
        vec![
            Operation::Create {
                node: other,
                parent: None,
                index: 0,
                descriptor: parent.clone(),
            },
            Operation::Create {
                node: nested,
                parent: Some(ids[0]),
                index: 1,
                descriptor: parent,
            },
            Operation::Move {
                node: ids[2],
                parent: Some(nested),
                index: 0,
            },
            Operation::Move {
                node: ids[1],
                parent: Some(other),
                index: 0,
            },
            Operation::Create {
                node: empty,
                parent: Some(nested),
                index: 1,
                descriptor: zero,
            },
        ],
    );
    layout.commit(&change, [320., 480.], 0, ink).unwrap();
    fresh(&layout, &life, [320., 480.]);
    for text in ["WWWW", "i", "a!\nsecond", "WW", "ab"] {
        let change = batch(
            &layout,
            &life,
            vec![
                Operation::Update {
                    node: ids[1],
                    descriptor: descriptor(text),
                },
                Operation::Update {
                    node: ids[2],
                    descriptor: descriptor(text),
                },
            ],
        );
        let stats = layout.commit(&change, [320., 480.], 0, ink).unwrap();
        assert!(!stats.geometry_reused);
        fresh(&layout, &life, [320., 480.]);
    }
}

#[test]
fn warm_viewport_relayout_only_synchronizes_root_and_preserves_retry_and_revision() {
    let (life, mut layout, ids) = mount();
    let revision = layout.revision();
    for viewport in [[24., 96.], [640., 240.], [0., 0.], [320., 480.]] {
        let stats = layout.relayout(viewport, 0, ink).unwrap();
        assert_eq!(stats.sync_nodes, 1, "only viewport root is synchronized");
        assert!(stats.incremental_sync);
        assert_eq!(stats.engine_created, 0);
        assert_eq!(layout.revision(), revision);
        fresh(&layout, &life, viewport);
    }
    let accepted = layout.roots().to_vec();
    assert_eq!(
        layout.relayout([f32::NAN, 10.], 0, ink),
        Err(LayoutError::InvalidViewport)
    );
    assert_eq!(layout.roots(), accepted);
    let failed = layout.relayout([9., 31.], 0, |request, _| {
        Err(LayoutError::MeasurementUnavailable(request.key.node))
    });
    assert!(matches!(
        failed,
        Err(LayoutError::MeasurementUnavailable(_))
    ));
    assert_eq!(layout.roots(), accepted);
    assert_eq!(layout.revision(), revision);
    // Failed compute drops engine/cache; retry reconstructs the accepted tree.
    let retry = layout.relayout([9., 31.], 0, ink).unwrap();
    assert!(retry.engine_created > 0);
    fresh(&layout, &life, [9., 31.]);
    let update = batch(
        &layout,
        &life,
        vec![Operation::Update {
            node: ids[1],
            descriptor: descriptor("ii"),
        }],
    );
    layout.commit(&update, [9., 31.], 0, ink).unwrap();
    fresh(&layout, &life, [9., 31.]);
    let changed_environment = layout.relayout([9., 31.], 1, ink).unwrap();
    assert!(changed_environment.engine_created > 0);
    assert_eq!(layout.revision(), revision + 1);
    layout.release_layout_cache();
    assert!(layout.relayout([18., 31.], 1, ink).unwrap().engine_created > 0);
    life.dispose();
    assert_eq!(
        layout.relayout([18., 31.], 1, ink),
        Err(LayoutError::Disposed)
    );
}

#[test]
fn overlay_relayout_fallback_keeps_snapshot_on_measure_failure() {
    let life = RootLifetime::new(RootId::new());
    let dialog = NodeId::new(life.id());
    let child = NodeId::new(life.id());
    let mut layout = StructuralLayout::new(life.clone());
    let mut d = descriptor("");
    d.content = NodeContent::Dialog {
        open: true,
        revision: 0,
        close_on_overlay_click: true,
    };
    let mount = batch(
        &layout,
        &life,
        vec![
            Operation::Create {
                node: dialog,
                parent: None,
                index: 0,
                descriptor: d,
            },
            Operation::Create {
                node: child,
                parent: Some(dialog),
                index: 0,
                descriptor: descriptor("panel text"),
            },
        ],
    );
    layout.commit(&mount, [320., 480.], 0, ink).unwrap();
    let accepted = layout.roots().to_vec();
    assert!(layout
        .relayout([8., 60.], 1, |r, _| Err(
            LayoutError::MeasurementUnavailable(r.key.node)
        ))
        .is_err());
    assert_eq!(layout.roots(), accepted);
    let stats = layout.relayout([8., 60.], 1, ink).unwrap();
    assert!(!stats.incremental_sync);
    let mut reference = accepted;
    PresentationLayout::new(life)
        .layout(&mut reference, [8., 60.], 1, ink)
        .unwrap();
    assert_eq!(layout.roots(), reference);
    assert_eq!(layout.revision(), 1);
}
