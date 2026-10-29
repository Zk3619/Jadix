use super::*;
use crate::core::reactivity::create_root;

#[test]
fn collapse_work_discovers_mount_before_restamp_and_consumes_damage() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    let mut root = Element::new().child(Element::new()).child(Element::new());
    let mut tree = LayoutTree::new();
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.subtree_collapse_work);
    let mut visited = 0;
    walk_collapse_work(&mut root, &mut |_| visited += 1);
    assert_eq!(visited, 0);

    let mut panel = Element::new().reactive_sync(|el, _| {
        el.attrs.remove("jadix_collapse_measuring");
        el.set_attr("jadix_collapse_damage", "1");
        LayoutStatus::LayoutChanged
    });
    panel.layout_bounds[3] = 64.0;
    panel.set_attr("jadix_collapse_measuring", "1");
    root.children[1].children.push(panel);
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    assert!(seed_measured_collapse_panels(&mut root, &mut tree));
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.subtree_collapse_work);
    assert!(!root.children[0].subtree_collapse_work);
    flush_collapse_layout_damage(&mut root);
    assert!(!root.children[1].children[0]
        .attrs
        .contains_key("jadix_collapse_damage"));

    // Static custom nodes may queue damage without a synchronizer.
    root.children[1].children.clear();
    root.children[1].set_attr("jadix_collapse_damage", "1");
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.subtree_collapse_work);
    flush_collapse_layout_damage(&mut root);
    assert!(!root.children[1].attrs.contains_key("jadix_collapse_damage"));
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.subtree_collapse_work);
    crate::app::damage::clear_frame_paint_nodes();
}

#[test]
#[ignore = "isolated collapse discovery CPU comparison"]
fn collapse_static_walk_cost() {
    let mut content = Element::new();
    content.children = (0..1600).map(|_| Element::new()).collect();
    let mut root = Element::new().child(
        Element::new()
            .reactive_sync(|_, _| LayoutStatus::Clean)
            .child(content),
    );
    let mut tree = LayoutTree::new();
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    for eager in [true, false] {
        let begin = Instant::now();
        for _ in 0..2000 {
            if eager {
                // The original two discovery passes, on an idle static list.
                crate::core::tree::walk_elements_mut(&mut root, |el| {
                    std::hint::black_box(el.attrs.contains_key("jadix_collapse_measuring"));
                });
                crate::core::tree::walk_elements_mut(&mut root, |el| {
                    assert!(el.attrs.remove("jadix_collapse_damage").is_none());
                });
            } else {
                assert!(!seed_measured_collapse_panels(&mut root, &mut tree));
                flush_collapse_layout_damage(&mut root);
            }
        }
        eprintln!(
            "collapse-discovery eager={eager} us/frame={:.3}",
            begin.elapsed().as_secs_f64() * 1_000_000.0 / 2000.0
        );
    }
}

#[test]
fn reopen_after_finished_exit_cancels_pending_unmount() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("reopen_pending_unmount");
        sync_collapse_panel_presence(&p, true);
        sync_collapse_panel_presence(&p, false);
        p.finish_exit();
        // Input can arrive between the final closing present and the next tick.
        sync_collapse_panel_presence(&p, true);
        drain_pending_unmounts();
        assert_eq!(p.phase(), PresencePhase::Present);
        assert!(
            p.mounted().peek(),
            "stale exit must not hide reopened content"
        );
    });
}

#[test]
fn finished_exit_still_unmounts_closed_content() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("closed_pending_unmount");
        sync_collapse_panel_presence(&p, true);
        sync_collapse_panel_presence(&p, false);
        p.finish_exit();
        drain_pending_unmounts();
        assert!(!p.mounted().peek());
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn reverse_exit_continues_from_current_clip() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("reverse_exit_clip");
        sync_collapse_panel_presence(&p, true);
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 80.0];
        let bounds = el.layout_bounds;
        el.used_layout.stamp_border_box(bounds, false);
        let keys = CollapseKeyframes::COLLAPSIBLE;
        sync_collapse_panel_presence(&p, false);
        apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default());
        el.tick_css_animations(0.05);
        let before = crate::layout::dim_length_px(el.style.max_size.height).unwrap();
        sync_collapse_panel_presence(&p, true);
        apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default());
        el.tick_css_animations(0.0);
        let after = crate::layout::dim_length_px(el.style.max_size.height).unwrap();
        assert!(
            (before - after).abs() < 0.1,
            "reverse jumped: {before} -> {after}"
        );
    });
}

#[test]
fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("util_enter_measure");
        sync_collapse_panel_presence(&p, false);
        sync_collapse_panel_presence(&p, true);
        assert!(p.needs_enter());
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let mut tree = LayoutTree::new();
        let keys = CollapseKeyframes::COLLAPSIBLE;
        let _ = apply_collapse_motion(&mut el, &mut tree, &p, keys);
        assert!(
            p.needs_enter(),
            "must keep needs_enter until height is known"
        );
        assert!(
            !el.style.animations.iter().any(|a| a.name == keys.down),
            "must not seed down with 0 height"
        );
        el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
        let _ = apply_collapse_motion(&mut el, &mut tree, &p, keys);
        assert!(!p.needs_enter());
        assert!(
            el.style.animations.iter().any(|a| a.name == keys.down),
            "first real expand must run collapsible-down"
        );
    });
}

#[test]
fn accordion_idle_resets_max_height_to_auto() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("idle_auto");
        sync_collapse_panel_presence(&p, false);
        sync_collapse_panel_presence(&p, true);
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
        el.used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 64.0], false);
        let keys = CollapseKeyframes::ACCORDION;
        assert!(matches!(
            apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default()),
            CollapseHeightEffect::SeededEnter
        ));
        let _ = el.tick_css_animations(0.0);
        for _ in 0..40 {
            let _ = el.tick_css_animations(0.016);
        }
        assert!(!animation_run_active(&el.style, &el.animation_snapshot));
        let effect = apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default());
        assert!(matches!(effect, CollapseHeightEffect::Idle));
        assert!(
            matches!(
                crate::layout::dim_kind(el.style.max_size.height),
                crate::layout::DimKind::Auto
            ),
            "in-page accordion Idle must return to auto"
        );
    });
}

#[test]
fn expand_idle_keeps_last_max_height() {
    let (_o, ()) = create_root(|| {
        let p = use_collapse_panel_presence("idle_keep");
        sync_collapse_panel_presence(&p, false);
        sync_collapse_panel_presence(&p, true);
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 192.0];
        el.used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
        let keys = CollapseKeyframes::ACCORDION;
        let opts = CollapseHeightOpts {
            pin_exit_height: true,
            keep_open_max_height: true,
        };
        assert!(matches!(
            apply_collapse_height(&mut el, &p, keys, opts),
            CollapseHeightEffect::SeededEnter
        ));
        let _ = el.tick_css_animations(0.0);
        for _ in 0..40 {
            let _ = el.tick_css_animations(0.016);
        }
        let last = crate::layout::dim_length_px(el.style.max_size.height);
        assert!(
            last.is_some_and(|h| (h - 192.0).abs() < 1.0),
            "last keyframe must reach content height, got {last:?}"
        );
        let effect = apply_collapse_height(&mut el, &p, keys, opts);
        assert!(matches!(effect, CollapseHeightEffect::Idle));
        let kept = crate::layout::dim_length_px(el.style.max_size.height);
        assert!(
            kept.is_some_and(|h| (h - last.unwrap()).abs() < 0.01),
            "Expand Idle must keep last keyframe max-height, got {kept:?} last {last:?}"
        );
        let _ = el.tick_css_animations(0.0);
        assert!(
            crate::layout::dim_length_px(el.style.max_size.height).is_some(),
            "cleared snapshot must not restore max-height auto"
        );
    });
}
