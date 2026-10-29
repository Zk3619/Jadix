use super::*;
use crate::core::reactivity::{create_effect, create_root, use_signal};

#[test]
fn toggle_single_mode() {
    let mut open = vec!["a".into()];
    toggle_open_item(&mut open, "a", false);
    assert!(open.is_empty());
    toggle_open_item(&mut open, "b", false);
    assert_eq!(open, vec!["b"]);
    toggle_open_item(&mut open, "c", false);
    assert_eq!(open, vec!["c"]);
}

#[test]
fn toggle_multiple_mode() {
    let mut open = Vec::<String>::new();
    toggle_open_item(&mut open, "a", true);
    toggle_open_item(&mut open, "b", true);
    assert_eq!(open, vec!["a", "b"]);
    toggle_open_item(&mut open, "a", true);
    assert_eq!(open, vec!["b"]);
}

#[test]
#[cfg(feature = "animation-base")]
fn accordion_exit_run_inactive_after_keyframes_finish() {
    let mut el = Element::new();
    el.layout_bounds = [0.0, 0.0, 100.0, 48.0];
    let bounds = el.layout_bounds;
    el.used_layout.stamp_border_box(bounds, false);
    crate::utils::collapse_panel::seed_collapse_exit_anim(
        &mut el,
        crate::utils::CollapseKeyframes::ACCORDION,
    );
    assert!(
        crate::style::animation_run_active(&el.style, &el.animation_snapshot),
        "fresh accordion-up must count as animating"
    );
    for _ in 0..20 {
        let _ = el.tick_css_animations(0.016);
    }
    assert!(
        !crate::style::animation_run_active(&el.style, &el.animation_snapshot),
        "finished accordion-up must not keep LayoutChanged pumping"
    );
}

#[test]
fn collapsed_panel_height_is_zero_when_padding_is_on_inner() {
    use crate::core::tree::LayoutTree;
    let (_h, _) = create_root(|| {
        create_effect(|| {
            let inner: Element = vstack()
                .w("100%")
                .pb(tokens::SPACE_4)
                .child(
                    Element::new()
                        .text("Yes. Builder 与 RSX 共用 accordion_scope / accordion_content。"),
                )
                .into();
            let wrap: Element = vstack()
                .w(320.0)
                .min_w(0.0)
                .min_h(0.0)
                .overflow_y("hidden")
                .max_h(0.0)
                .child(inner)
                .into();
            let mut root = Element::new().w(400.0).h(400.0).child(wrap);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
            let h = root.children[0].layout_bounds[3];
            assert!(
                h <= 1.0,
                "clip shell with min-h:0 and padding on inner must collapse to ~0, got {h}"
            );
        });
    });
}

#[test]
fn scope_sets_template_and_slot() {
    let (_h, el) = create_root(|| {
        accordion_scope("acc-test", None, vec!["one".into()], false, false, || {
            Element::new()
        })
    });
    assert_eq!(el.template_id.as_deref(), Some("acc-test"));
    assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
}

#[test]
fn builder_renders_items() {
    let (_h, el): (_, Element) = create_root(|| {
        accordion("acc-b")
            .default_value(vec!["x".into()])
            .item("x", "Q", || Element::new().text("A"))
            .into()
    });
    assert_eq!(el.template_id.as_deref(), Some("acc-b"));
    assert_eq!(el.children.len(), 1);
    assert_eq!(
        el.children[0].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_ITEM)
    );
}

#[test]
fn trigger_chevron_is_named_lucide_for_square_svg_rotate() {
    let (_h, el): (_, Element) = create_root(|| {
        accordion("acc-chev")
            .item("x", "Q", || Element::new().text("A"))
            .into()
    });
    let icon = find_slot_child(&el, SLOT_TRIGGER_ICON).expect("accordion chevron");
    assert_eq!(
        icon.get_attr(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str),
        Some("chevron-down"),
        "named SVG texture keeps ink centered under 180° rotate"
    );
    assert_eq!(icon.text_style.font_family.as_deref(), Some("lucide"));
    assert!((icon.text_style.size_resolved() - CHEVRON_PX).abs() < 0.01);
}

#[test]
fn accordion_content_macro_shell_has_panel_slot() {
    let open = use_signal("__acc_content_open", || vec!["v".into()]);
    let (_h, el) = create_root(|| {
        accordion_scope("acc-m", Some(open), Vec::new(), false, false, move || {
            accordion_item_scope("v", false, move || {
                accordion_content(|| Element::new().text("body"))
            })
        })
    });
    let panel = el
        .children
        .first()
        .and_then(|item| find_slot_child(item, SLOT_CONTENT));
    assert!(panel.is_some(), "expected accordion content panel");
}

#[test]
fn collapsed_content_is_display_none_and_item_has_zero_gap() {
    let (_h, el): (_, Element) = create_root(|| {
        accordion("acc-hidden")
            .item("x", "Q", || Element::new().text("A"))
            .into()
    });
    let item = &el.children[0];
    let gap = crate::layout::lp_length_px(item.style.gap.height).unwrap_or(0.0);
    assert!(
        gap.abs() < 0.01,
        "AccordionItem must not flex-gap Trigger/Content (got {gap})"
    );
    let panel = find_slot_child(item, SLOT_CONTENT).expect("content");
    assert_eq!(
        panel.style.display,
        Display::None,
        "collapsed panel matches CSS hidden — not a 0×0 flex sibling"
    );
}

#[test]
fn open_content_placeholder_stays_in_flow() {
    let (_h, el): (_, Element) = create_root(|| {
        accordion("acc-open")
            .default_value(vec!["x".into()])
            .item("x", "Q", || Element::new().text("A"))
            .into()
    });
    let panel = find_slot_child(&el, SLOT_CONTENT).expect("content");
    assert_ne!(
        panel.style.display,
        Display::None,
        "open panel must stay in-flow so height animation can run"
    );
}

#[test]
fn collapsed_display_none_skips_author_item_gap() {
    let (_h, mut root): (_, Element) = create_root(|| {
        accordion("acc-item-gap")
            .item("x", "Q", || Element::new().text("A"))
            .into()
    });
    let item = &mut root.children[0];
    item.style.gap = taffy::geometry::Size {
        width: taffy::style::LengthPercentage::length(16.0),
        height: taffy::style::LengthPercentage::length(16.0),
    };
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
    let item = &root.children[0];
    let trigger_shell = &item.children[0];
    let extra = item.layout_bounds[3] - trigger_shell.layout_bounds[3];
    assert!(
        extra < 4.0,
        "Item gap must not apply around display:none content (extra={extra})"
    );
}

#[test]
fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
    use crate::core::tree::LayoutTree;
    let (_h, ()) = create_root(|| {
        let p = use_accordion_panel_presence("acc_enter_measure");
        sync_accordion_panel_presence(&p, false);
        sync_accordion_panel_presence(&p, true);
        assert!(p.needs_enter());
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let mut tree = LayoutTree::new();
        let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
        assert!(
            p.needs_enter(),
            "must keep needs_enter until height is known"
        );
        assert!(
            !el.style
                .animations
                .iter()
                .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
            "must not seed down with 0 height"
        );
        el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
        let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
        assert!(!p.needs_enter());
        assert!(
            el.style
                .animations
                .iter()
                .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
            "first real expand must run accordion-down"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn grid_fr_enter_does_not_wait_for_used_height() {
    use crate::core::tree::LayoutTree;
    let (_h, ()) = create_root(|| {
        set_accordion_use_grid_fr(Some(true));
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                set_accordion_use_grid_fr(None);
            }
        }
        let _reset = Reset;
        let p = use_accordion_panel_presence("acc_enter_grid_fr");
        sync_accordion_panel_presence(&p, false);
        sync_accordion_panel_presence(&p, true);
        assert!(p.needs_enter());
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let mut tree = LayoutTree::new();
        let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
        assert!(!p.needs_enter(), "grid-fr must not wait for used height");
        assert!(
            !el.style
                .animations
                .iter()
                .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
            "grid-fr path must not seed accordion-down"
        );
        assert_eq!(el.style.display, Display::Grid);
        assert!(
            el.style
                .transition
                .iter()
                .any(|e| e.property == TransitionProperty::GridTemplateRows),
            "must attach grid-template-rows transition"
        );
        assert!(
            el.transition_snapshot.has_active_tweens(),
            "0fr→1fr tween must start on seed"
        );
        let used = first_track_fr(&el.style.grid_template_rows).unwrap_or(-1.0);
        assert!(used < 0.05, "tick(0) must leave used at 0fr, got {used}");
    });
}
