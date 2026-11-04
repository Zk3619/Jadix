use jadix_core::presentation::{hybrid::*, *};

#[test]
fn focus_has_one_presenter_owner_and_a_late_sibling_blur_cannot_clear_it() {
    let root = RootId::new();
    let native = NodeId::new(root);
    let gpu = NodeId::new(root);
    let mut coordinator = HybridCoordinator::new(root);
    assert!(coordinator.stage(
        1,
        vec![
            geometry(native, 1, Presentation::Native),
            geometry(gpu, 1, Presentation::Gpu)
        ]
    ));
    assert!(coordinator.confirm(1, true, true));
    assert!(coordinator.focus(1, native));
    assert!(coordinator.focus(1, gpu));
    assert!(!coordinator.blur(1, native));
    assert_eq!(coordinator.focused().unwrap().node, gpu);
    assert_eq!(
        coordinator.focused().unwrap().presentation,
        Presentation::Gpu
    );
    assert!(coordinator.blur(1, gpu));
    assert!(coordinator.focused().is_none());
}

fn geometry(node: NodeId, revision: u64, presentation: Presentation) -> HybridGeometry {
    HybridGeometry {
        node,
        revision,
        presentation,
        frame: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        },
        clips: vec![],
        visible: true,
        interactive: true,
        z_index: 0,
    }
}

#[test]
fn cloned_root_lifetimes_share_revocation_but_dropping_a_clone_does_not_revoke() {
    let lifetime = RootLifetime::new(RootId::new());
    let retained = lifetime.clone();
    drop(lifetime.clone());
    assert!(!retained.is_disposed());
    assert_eq!(lifetime.id(), retained.id());
    let other = RootLifetime::new(RootId::new());
    retained.dispose();
    assert!(lifetime.is_disposed());
    retained.dispose();
    assert!(!other.is_disposed());
}

#[test]
fn focus_uses_only_the_current_visible_revision_and_retains_identity_after_move() {
    let lifetime = RootLifetime::new(RootId::new());
    let node = NodeId::new(lifetime.id());
    let mut coordinator = HybridCoordinator::with_lifetime(lifetime);
    let entry = geometry(node, 1, Presentation::Native);
    assert!(coordinator.stage(1, vec![entry.clone()]));
    assert!(!coordinator.focus(1, node));
    assert!(coordinator.confirm(1, false, true));
    assert!(!coordinator.focus(0, node));
    assert!(!coordinator.focus(1, NodeId::new(RootId::new())));
    assert!(coordinator.focus(1, node));
    let mut moved = entry;
    moved.revision = 2;
    moved.frame.x = 50.0;
    assert!(coordinator.stage(2, vec![moved]));
    assert_eq!(coordinator.focused().unwrap().frame.x, 0.0);
    assert!(!coordinator.confirm(2, true, false));
    assert!(coordinator.confirm(2, false, true));
    assert_eq!(coordinator.focused().unwrap().frame.x, 50.0);
    assert!(!coordinator.blur(1, node));
    assert_eq!(coordinator.focused().unwrap().node, node);
    assert!(coordinator.blur(2, node));
    assert!(coordinator.focused().is_none());
}

#[test]
fn confirmed_removal_disabling_and_hiding_clear_focus_but_cancellation_preserves_it() {
    for change in 0..3 {
        let root = RootId::new();
        let node = NodeId::new(root);
        let mut coordinator = HybridCoordinator::new(root);
        let entry = geometry(node, 1, Presentation::Gpu);
        assert!(coordinator.stage(1, vec![entry.clone()]));
        assert!(coordinator.confirm(1, true, false));
        assert!(coordinator.focus(1, node));
        assert!(coordinator.stage(2, vec![]));
        assert!(coordinator.cancel(2));
        assert_eq!(coordinator.focused().unwrap().node, node);
        let mut changed = entry;
        changed.revision = 2;
        if change == 1 {
            changed.interactive = false;
        }
        if change == 2 {
            changed.visible = false;
        }
        let entries = if change == 0 { vec![] } else { vec![changed] };
        assert!(coordinator.stage(2, entries));
        assert_eq!(coordinator.focused().unwrap().node, node);
        assert!(coordinator.confirm(2, true, false));
        assert!(coordinator.focused().is_none());
        assert!(!coordinator.focus(2, node));
    }
}

#[test]
fn focused_presenter_cannot_switch_even_if_focus_arrives_after_staging() {
    let root = RootId::new();
    let node = NodeId::new(root);
    let mut coordinator = HybridCoordinator::new(root);
    assert!(coordinator.stage(1, vec![geometry(node, 1, Presentation::Native)]));
    assert!(coordinator.confirm(1, false, true));
    assert!(coordinator.focus(1, node));
    assert!(!coordinator.stage(2, vec![geometry(node, 2, Presentation::Gpu)]));
    assert!(coordinator.blur(1, node));
    assert!(coordinator.stage(2, vec![geometry(node, 2, Presentation::Gpu)]));
    assert!(coordinator.focus(1, node));
    assert!(!coordinator.confirm(2, true, true));
    assert_eq!(coordinator.visible_revision(), 1);
    assert_eq!(
        coordinator.focused().unwrap().presentation,
        Presentation::Native
    );
    assert!(coordinator.blur(1, node));
    assert!(coordinator.confirm(2, true, true));
}

#[test]
fn revocation_blocks_visible_and_pending_input_focus_and_publication() {
    let lifetime = RootLifetime::new(RootId::new());
    let node = NodeId::new(lifetime.id());
    let mut coordinator = HybridCoordinator::with_lifetime(lifetime.clone());
    assert!(coordinator.stage(1, vec![geometry(node, 1, Presentation::Native)]));
    assert!(coordinator.confirm(1, false, true));
    assert!(coordinator.focus(1, node));
    assert!(coordinator.stage(2, vec![geometry(node, 2, Presentation::Native)]));
    lifetime.dispose();
    assert!(coordinator.hit(1.0, 1.0).is_none());
    assert!(coordinator.focused().is_none());
    assert!(!coordinator.confirm(2, true, true));
    assert!(!coordinator.cancel(2));
    assert!(!coordinator.focus(1, node));
    assert!(!coordinator.blur(1, node));
    coordinator.dispose();
    assert!(!coordinator.stage(3, vec![]));
}
