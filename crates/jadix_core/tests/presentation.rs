use jadix_core::presentation::{hybrid::*, *};

fn geometry(node: NodeId, revision: u64, presentation: Presentation, x: f64) -> HybridGeometry {
    HybridGeometry {
        node,
        revision,
        presentation,
        frame: Rect {
            x,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        },
        clips: Vec::new(),
        visible: true,
        interactive: true,
        z_index: 0,
    }
}

#[test]
fn mixed_frame_waits_for_both_presenters_before_publishing_hits() {
    let root = RootId::new();
    let node = NodeId::new(root);
    let gpu = NodeId::new(root);
    let mut coordinator = HybridCoordinator::new(root);
    assert!(coordinator.stage(
        1,
        vec![
            geometry(node, 1, Presentation::Native, 0.0),
            geometry(gpu, 1, Presentation::Gpu, 200.0)
        ]
    ));
    assert!(!coordinator.confirm(1, true, false));
    assert_eq!(coordinator.hit(10.0, 10.0), None);
    assert!(coordinator.confirm(1, true, true));
    assert_eq!(coordinator.hit(10.0, 10.0).unwrap().node, node);
    assert!(coordinator.stage(2, vec![geometry(node, 2, Presentation::Native, 400.0)]));
    assert!(!coordinator.confirm(2, true, false));
    assert_eq!(coordinator.hit(10.0, 10.0).unwrap().node, node);
    assert_eq!(coordinator.hit(410.0, 10.0), None);
    assert!(coordinator.cancel(2));
    assert_eq!(coordinator.visible_revision(), 1);
}

#[test]
fn clip_visibility_and_paint_order_choose_one_input_owner() {
    let root = RootId::new();
    let mut coordinator = HybridCoordinator::new(root);
    let gpu = geometry(NodeId::new(root), 1, Presentation::Gpu, 0.0);
    let mut native = geometry(NodeId::new(root), 1, Presentation::Native, 0.0);
    native.clips.push(Rect {
        x: 20.0,
        y: 0.0,
        width: 20.0,
        height: 40.0,
    });
    assert!(coordinator.stage(1, vec![gpu.clone(), native.clone()]));
    assert!(coordinator.confirm(1, true, true));
    assert_eq!(coordinator.hit(10.0, 10.0).unwrap().node, gpu.node);
    assert_eq!(coordinator.hit(25.0, 10.0).unwrap().node, native.node);
    assert_eq!(coordinator.hit(40.0, 10.0).unwrap().node, gpu.node);
}

#[test]
fn stale_duplicate_foreign_and_invalid_geometry_never_stage() {
    let root = RootId::new();
    let mut coordinator = HybridCoordinator::new(root);
    let entry = geometry(NodeId::new(root), 1, Presentation::Native, 0.0);
    assert!(!coordinator.stage(1, vec![entry.clone(), entry.clone()]));
    assert!(!coordinator.stage(2, vec![entry.clone()]));
    assert!(!coordinator.stage(
        1,
        vec![geometry(
            NodeId::new(RootId::new()),
            1,
            Presentation::Native,
            0.0
        )]
    ));
    let mut invalid = entry.clone();
    invalid.frame.width = -1.0;
    assert!(!coordinator.stage(1, vec![invalid]));
    assert!(coordinator.stage(1, vec![entry]));
    assert!(!coordinator.confirm(2, true, true));
    assert!(coordinator.confirm(1, true, true));
    assert!(!coordinator.stage(1, vec![]));
}

#[test]
fn measurement_results_bind_all_cache_versions_and_constraints() {
    let key = MeasureKey {
        node: NodeId::new(RootId::new()),
        content_revision: 1,
        style_revision: 2,
        environment_revision: 3,
        max_width: 100.0,
        max_height: 50.0,
    };
    let result = MeasureResult {
        key,
        width: 80.0,
        height: 40.0,
        baseline: Some(30.0),
    };
    assert!(result.matches(key));
    for changed in [
        MeasureKey {
            content_revision: 2,
            ..key
        },
        MeasureKey {
            style_revision: 3,
            ..key
        },
        MeasureKey {
            environment_revision: 4,
            ..key
        },
        MeasureKey {
            max_width: 90.0,
            ..key
        },
        MeasureKey {
            node: NodeId::new(key.node.root),
            ..key
        },
    ] {
        assert!(!result.matches(changed));
    }
    assert!(!MeasureResult {
        width: f64::NAN,
        ..result
    }
    .matches(key));
    assert!(!MeasureResult {
        baseline: Some(60.0),
        ..result
    }
    .matches(key));
    assert!(!MeasureResult {
        width: 120.0,
        ..result
    }
    .matches(key));
}

#[test]
fn utf16_selection_accepts_unicode_boundaries_and_rejects_surrogate_splits() {
    let text = "a\u{1f600}\u{4e2d}e\u{301}";
    assert!(valid_utf16_range(text, [1, 3]));
    assert!(valid_utf16_range(text, [4, 6]));
    assert!(valid_utf16_range("", [0, 0]));
    assert!(!valid_utf16_range(text, [2, 3]));
    assert!(!valid_utf16_range(text, [3, 1]));
    assert!(!valid_utf16_range(text, [0, 7]));
}
