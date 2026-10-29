use super::*;
use crate::app::portal_overlay::clear_overlay_stack;
use crate::core::reactivity::Signal;
use crate::core::tree::PointerEvents;
use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree, PORTAL_LAYER_ID};
use crate::utils::modal_shell::{
    default_modal_overlay_color, ShellOverlayOpts, DEFAULT_MODAL_OVERLAY_BLUR,
    DEFAULT_MODAL_OVERLAY_OPACITY,
};

#[test]
fn themed_close_button_keeps_absolute_position_after_variant_resolution() {
    use crate::style::{
        get_theme, resolve_class_variants, set_theme, Theme, ThemeMode, ThemePreset, VariantContext,
    };
    use taffy::style::{LengthPercentageAuto, Position};

    let previous = get_theme();
    crate::core::reactivity::create_root(|| {
        for preset in ThemePreset::ALL {
            set_theme(Theme::preset(preset, ThemeMode::Light));
            let mut panel = dialog_shell_panel(
                "themed-close",
                Element::new(),
                Some(button("themed-close:chrome_close").into()),
                false,
                None,
                None,
                None,
                None,
                SlotChrome::default(),
            );
            let close = crate::core::tree::find_in_tree_mut(&mut panel, |element| {
                element.focus_id.as_deref() == Some("themed-close:chrome_close")
            })
            .expect("dialog close button");
            resolve_class_variants(close, &VariantContext::default());
            assert_eq!(close.style.position, Position::Absolute, "{preset:?}");
            assert_eq!(
                close.style.inset.top,
                LengthPercentageAuto::length(16.0),
                "{preset:?}"
            );
            assert_eq!(
                close.style.inset.right,
                LengthPercentageAuto::length(16.0),
                "{preset:?}"
            );
            assert_eq!(close.style.z_index, 2, "{preset:?}");
        }
    });
    set_theme(previous);
}

#[test]
fn responsive_modal_actions_fit_narrow_windows_and_restore_desktop_rows() {
    use crate::style::{resolve_class_variants, VariantContext};
    use taffy::style::FlexDirection;

    crate::core::reactivity::create_root(|| {
        let actions = || {
            let mut pack = Element::new();
            pack.set_attr("jadix_fragment", "1");
            pack.children(vec![
                button("responsive-cancel").text("Cancel changes").into(),
                button("responsive-confirm").text("Save changes").into(),
            ])
        };
        for mut footer in [
            DialogFooter(DialogFooterProps {
                children: Some(actions()),
                base: None,
            }),
            crate::components::alert_dialog::AlertDialogFooter(
                crate::components::alert_dialog::AlertDialogFooterProps {
                    children: Some(actions()),
                    base: None,
                },
            ),
        ] {
            footer = footer.class("gap-3");
            let mut root = Element::new().child(footer);
            let mut tree = LayoutTree::new();
            for width in [320.0, 639.5, 640.0, 1024.0, 320.0] {
                resolve_class_variants(
                    &mut root.children[0],
                    &VariantContext {
                        viewport_w: width,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    root.children[0].style.flex_direction,
                    if width < 640.0 {
                        FlexDirection::Column
                    } else {
                        FlexDirection::Row
                    }
                );
                root = root.w(width - 64.0);
                tree.build_and_layout(&mut root, width, 800.0, [-1.0, -1.0]);
                let row = &root.children[0];
                for action in &row.children {
                    assert!(action.layout_bounds[2] > 0.0);
                    assert!(action.layout_bounds[0] >= row.layout_bounds[0] - 0.5);
                    assert!(
                        action.layout_bounds[0] + action.layout_bounds[2]
                            <= row.layout_bounds[0] + row.layout_bounds[2] + 0.5,
                        "action must stay reachable at {width}px: {:?}",
                        action.layout_bounds
                    );
                }
                if width < 640.0 {
                    for action in &row.children {
                        assert!(
                            (action.layout_bounds[2] - row.layout_bounds[2]).abs() < 0.5,
                            "narrow footer actions must fill the row at {width}px"
                        );
                    }
                    assert!(row.children[1].layout_bounds[1] > row.children[0].layout_bounds[1]);
                } else {
                    let last = row.children.last().unwrap().layout_bounds;
                    assert!((last[0] + last[2] - row.layout_bounds[0] - row.layout_bounds[2]).abs() < 0.5,
                            "desktop footer actions must align right at {width}px: last={last:?} row={:?}", row.layout_bounds);
                    assert!(
                        (row.children[1].layout_bounds[1] - row.children[0].layout_bounds[1]).abs()
                            < 0.5
                    );
                }
            }
        }
    });
}

#[test]
fn responsive_dialog_panel_keeps_viewport_gutters() {
    for width in [320.0, 375.0, 639.5, 640.0, 768.0, 1280.0] {
        let panel = dialog_shell_panel(
            "responsive-panel",
            Element::new().h(80.0),
            None,
            false,
            None,
            None,
            None,
            None,
            SlotChrome::default(),
        );
        let mut root = Element::new().w(width).child(panel);
        LayoutTree::new().build_and_layout(&mut root, width, 800.0, [-1.0, -1.0]);
        let panel = &root.children[0];
        assert!(
            (panel.layout_bounds[2] - (width - 32.0).min(512.0)).abs() < 0.5,
            "width={width}: {:?}",
            panel.layout_bounds
        );
        assert!((panel.children[0].layout_bounds[2] - panel.layout_bounds[2]).abs() < 0.5);
    }
}

#[test]
fn dialog_shell_shadow_matches_elevation_xl() {
    let el = apply_dialog_shell_surface(Element::new(), false);
    assert_eq!(
        el.style.shadow_color,
        crate::tokens::shadow_rgba(crate::tokens::Elevation::Xl)
    );
    assert!((el.style.shadow_blur - 16.0).abs() < 0.01);
}

#[test]
fn dialog_default_overlay_matches_modal_shell() {
    let opts = dialog_shell_overlay_opts(&DialogContentOpts::default());
    assert!(
        (opts.color[3] - DEFAULT_MODAL_OVERLAY_OPACITY).abs() < 0.001,
        "expected alpha {}, got {}",
        DEFAULT_MODAL_OVERLAY_OPACITY,
        opts.color[3]
    );
    assert!(
        (opts.backdrop_blur - DEFAULT_MODAL_OVERLAY_BLUR).abs() < 0.001,
        "expected blur {}, got {}",
        DEFAULT_MODAL_OVERLAY_BLUR,
        opts.backdrop_blur
    );
}

#[test]
fn dialog_panel_is_viewport_centered_and_empty_stage_hits_underlay() {
    use crate::app::pointer_events::hit_path;
    use crate::layout::layout_tree::ATTR_PORTAL_KEY;

    clear_overlay_stack();
    let open = Signal::new(true);
    let overlay_opts = ShellOverlayOpts {
        color: default_modal_overlay_color(None, Some(0.5)),
        close_on_click: true,
        backdrop_blur: 0.0,
    };
    let inner = vstack().w("100%").child(Element::new().text("Body")).into();
    let shell = dialog_shell_frame(
        "dlg-center",
        open,
        inner,
        &overlay_opts,
        None,
        false,
        None,
        None,
        None,
        None,
        SlotChrome::default(),
    );

    let mut root = Element::new().w(800.0).h(600.0).child(shell);
    assert!(hoist_portal_subtrees(&mut root) >= 2);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

    let layer = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        .expect("portal layer");
    let panel = layer
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_PORTAL_KEY)
                .map(|s| s.as_str())
                .is_some_and(|k| k.starts_with("panel:"))
        })
        .and_then(|stage| {
            stage
                .children
                .iter()
                .find(|c| c.template_id.as_deref() == Some("dlg-center__modal_panel"))
        })
        .expect("dialog panel on stage");
    let [px, _, pw, _] = panel.layout_bounds;
    let panel_center = px + pw * 0.5;
    assert!(
        (panel_center - 400.0).abs() < 2.0,
        "panel must be horizontally centered in 800px viewport, center={panel_center}"
    );
    assert!(
        pw < 600.0,
        "panel must shrink-wrap dialog chrome, not span viewport; w={pw}"
    );

    let corner = hit_path(&root, 790.0, 10.0);
    assert!(
        corner.iter().any(|e| {
            e.portal_key
                .as_deref()
                .is_some_and(|k| k.starts_with("underlay:"))
        }),
        "clicks outside shrink-wrapped panel must reach underlay"
    );
    assert_eq!(
        panel.pointer_events_mode(),
        PointerEvents::Auto,
        "panel/card swallow must stay auto"
    );
    clear_overlay_stack();
}

#[test]
fn dialog_open_builds_content_body_once() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::modal_motion::{
        any_modal_css_needs_frame, set_modal_css_needs_frame_for_test,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let builds = Rc::new(Cell::new(0u32));
    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("dlg_open_once__open", || false);
        let builds = Rc::clone(&builds);
        let live_e = Rc::clone(&live);
        crate::core::reactivity::create_effect(move || {
            let builds_once = Rc::clone(&builds);
            let el = dialog("dlg_open_once").open(open).child(move || {
                dialog_content(DialogContentOpts::default(), move || {
                    builds_once.set(builds_once.get() + 1);
                    Element::new().text("body")
                })
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    assert_eq!(
        builds.get(),
        0,
        "closed DialogContent must not run the body"
    );

    set_modal_css_needs_frame_for_test(false);
    open.set(true);
    let first = builds.get();
    assert_eq!(
        first, 1,
        "first open must build DialogContent body once (not a mounted.set re-flush), got {first}"
    );
    assert!(
            any_modal_css_needs_frame(),
            "first open.set must arm about_to_wait before the first tick, or caret WaitUntil (~520ms) sleeps through the first present"
        );

    fn has_tid(el: &Element, id: &str) -> bool {
        if el.template_id.as_deref() == Some(id) {
            return true;
        }
        el.children.iter().any(|c| has_tid(c, id))
    }
    assert!(
        has_tid(&live.borrow(), "dlg_open_once__modal_overlay"),
        "first open.set must hoist overlay in the same turn, not wait for idle retry"
    );

    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn dialog_close_does_not_rebuild_content_body() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::modal_motion::flush_modal_shell_motion_subtree;

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let builds = Rc::new(Cell::new(0u32));
    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("dlg_close_once__open", || false);
        let builds = Rc::clone(&builds);
        let live_e = Rc::clone(&live);
        crate::core::reactivity::create_effect(move || {
            let builds_once = Rc::clone(&builds);
            let el = dialog("dlg_close_once").open(open).child(move || {
                dialog_content(DialogContentOpts::default(), move || {
                    builds_once.set(builds_once.get() + 1);
                    Element::new().text("body")
                })
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    open.set(true);
    drain_deferred_view_remounts();
    assert_eq!(builds.get(), 1, "open must build body once");

    open.set(false);
    drain_deferred_view_remounts();
    assert_eq!(
            builds.get(),
            1,
            "close must keep the live DialogContent tree through Exiting; rebuilding overlay/zoom is the exit hitch"
        );

    if cfg!(feature = "animation-base") {
        let mut tree = live.borrow_mut();
        assert!(
            flush_modal_shell_motion_subtree(&mut tree),
            "close must seed exit keyframes on the live tree"
        );
        fn has_fade_out(el: &Element) -> bool {
            el.style.animations.iter().any(|a| a.name == "fade-out-0")
                || el.children.iter().any(has_fade_out)
        }
        assert!(
            has_fade_out(&tree),
            "exit keyframes must land on the existing overlay/zoom, not a rebuilt island"
        );
    }

    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_open_first_paint_is_already_fading() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::modal_motion::{flush_modal_shell_motion_subtree, tick_hoisted_modal_css};

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("dlg_first_paint__open", || false);
        let live_e = Rc::clone(&live);
        crate::core::reactivity::create_effect(move || {
            let el = dialog("dlg_first_paint").open(open).child(move || {
                dialog_content(DialogContentOpts::default(), || Element::new().text("body"))
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(2.0);
    open.set(true);

    let opacity = {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(
            flush_modal_shell_motion_subtree(&mut tree),
            "first paint must seed enter keyframes"
        );
        assert!(
            tick_hoisted_modal_css(&mut tree),
            "first paint must step CSS"
        );
        fn overlay_opacity(el: &Element) -> Option<f32> {
            if el.template_id.as_deref() == Some("dlg_first_paint__modal_overlay") {
                return Some(el.style.opacity);
            }
            el.children.iter().find_map(overlay_opacity)
        }
        overlay_opacity(&tree).expect("overlay on first paint")
    };
    assert!(
            opacity < 0.35,
            "click → first RedrawRequested must present a fading dialog, not a skipped enter, got {opacity}"
        );

    crate::style::utility::set_frame_dt(1.0 / 60.0);
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}
