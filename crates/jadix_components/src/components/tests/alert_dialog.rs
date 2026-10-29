use super::*;
use crate::utils::modal_shell::{
    default_modal_overlay_color, DEFAULT_MODAL_OVERLAY_BLUR, DEFAULT_MODAL_OVERLAY_OPACITY,
};

#[test]
fn alert_dialog_semantic_overlay_differs_by_variant() {
    let neutral = alert_shell_overlay_opts(
        &AlertDialogContentOpts::default(),
        AlertDialogVariant::Default,
    );
    let warning = alert_shell_overlay_opts(
        &AlertDialogContentOpts::default(),
        AlertDialogVariant::Warning,
    );
    assert_ne!(neutral.color, warning.color);
}

#[test]
fn alert_dialog_default_overlay_does_not_close_on_click() {
    let opts = alert_shell_overlay_opts(
        &AlertDialogContentOpts::default(),
        AlertDialogVariant::Default,
    );
    assert!(!opts.close_on_click);
    assert!(
        (opts.color[3] - DEFAULT_MODAL_OVERLAY_OPACITY).abs() < 0.001,
        "expected alpha {}",
        DEFAULT_MODAL_OVERLAY_OPACITY
    );
    assert!(
        (opts.backdrop_blur - DEFAULT_MODAL_OVERLAY_BLUR).abs() < 0.001,
        "expected blur {}",
        DEFAULT_MODAL_OVERLAY_BLUR
    );
    let _ = default_modal_overlay_color(None, None);
}

#[test]
fn alert_dialog_size_widths() {
    assert_eq!(AlertDialogSize::Default.width(), WIDTH_DEFAULT);
    assert_eq!(AlertDialogSize::Sm.width(), WIDTH_SM);
    assert_eq!(AlertDialogSize::from("sm"), AlertDialogSize::Sm);
}

#[test]
fn alert_dialog_title_slot_and_scope_root() {
    let title = AlertDialogTitle(AlertDialogTitleProps {
        children: Some(Element::new().text("Sure?")),
        base: None,
        ..Default::default()
    });
    assert_eq!(
        title.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_TITLE)
    );
    let root = alert_dialog_scope("test-scope", None, || Element::new());
    assert_eq!(
        root.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_ROOT)
    );
}

#[test]
fn alert_dialog_destructive_variant_chrome_attrs() {
    use crate::style::parse_color;

    let mut title = AlertDialogTitle(AlertDialogTitleProps {
        children: Some(Element::new().text("Sure?")),
        base: None,
        ..Default::default()
    });
    let mut desc = AlertDialogDescription(AlertDialogDescriptionProps {
        children: Some(Element::new().text("Details")),
        base: None,
        ..Default::default()
    });
    let chrome = alert_dialog_chrome(AlertDialogVariant::Destructive, false);
    paint_alert_dialog_tree(&mut title, &chrome);
    paint_alert_dialog_tree(&mut desc, &chrome);
    let mut shell = Element::new();
    stamp_alert_dialog_shell_meta(
        &mut shell,
        "test-modal",
        AlertDialogVariant::Destructive,
        false,
        true,
        None,
        None,
    );
    sync_alert_dialog_variant_chrome(&mut shell, AlertDialogVariant::Destructive);
    shell.children.push(title);
    shell.children.push(desc);
    shell.set_attr(ATTR_SLOT, SLOT_CONTENT);
    assert_eq!(
        shell
            .attrs
            .get(ATTR_ALERT_DIALOG_VARIANT)
            .map(String::as_str),
        Some("destructive")
    );
    assert_eq!(shell.style.border_color, parse_color("destructive/50"));
    let mut title_ok = false;
    let mut desc_ok = false;
    fn walk(
        el: &Element,
        title_fg: [u8; 4],
        desc_fg: [u8; 4],
        title_ok: &mut bool,
        desc_ok: &mut bool,
    ) {
        match el.text.as_deref() {
            Some("Sure?") => {
                assert_eq!(el.text_style.color, Some(title_fg));
                *title_ok = true;
            }
            Some("Details") => {
                assert_eq!(el.text_style.color, Some(desc_fg));
                *desc_ok = true;
            }
            _ => {}
        }
        for child in &el.children {
            walk(child, title_fg, desc_fg, title_ok, desc_ok);
        }
    }
    let title_fg = chrome.fg;
    let desc_fg = chrome.muted_fg;
    walk(&shell, title_fg, desc_fg, &mut title_ok, &mut desc_ok);
    assert!(title_ok);
    assert!(desc_ok);
}

#[test]
fn apply_bound_alert_dialog_variant_updates_shell() {
    let mut shell = Element::new();
    shell.set_attr(ATTR_SLOT, SLOT_CONTENT);
    stamp_alert_dialog_shell_meta(
        &mut shell,
        "test-modal",
        AlertDialogVariant::Default,
        false,
        true,
        None,
        None,
    );
    sync_alert_dialog_variant_chrome(&mut shell, AlertDialogVariant::Default);
    assert_eq!(
        apply_bound_alert_dialog_variant(&mut shell, "warning"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        shell
            .attrs
            .get(ATTR_ALERT_DIALOG_VARIANT)
            .map(String::as_str),
        Some("warning")
    );
    assert_eq!(
        apply_bound_alert_dialog_variant(&mut shell, "warning"),
        LayoutStatus::Clean
    );
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = alert_dialog("x").child(|| Element::new()).into();
}

fn find_tid_mut<'a>(el: &'a mut Element, tid: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    el.children.iter_mut().find_map(|c| find_tid_mut(c, tid))
}

#[test]
fn alert_dialog_action_click_closes_open() {
    use crate::core::reactivity::create_root;

    let (_owner, (open, mut action)) = create_root(|| {
        let open = use_signal("ad-act-open", || true);
        let action = alert_dialog_scope("ad-act", Some(open), || {
            let btn: Element = button("ad-act-allow").text("Allow").into();
            AlertDialogAction(AlertDialogActionProps {
                children: Some(btn),
                base: None,
                ..Default::default()
            })
        });
        (open, action)
    });
    let allow = find_tid_mut(&mut action, "ad-act-allow").expect("Allow button");
    allow.on_click.as_ref().expect("chained close").borrow_mut()();
    assert!(
        !open.get(),
        "AlertDialogAction must chain open.set(false) like Cancel"
    );
}

#[test]
fn alert_dialog_action_preserves_child_button_template_id() {
    use crate::core::reactivity::create_root;

    let (_owner, action) = create_root(|| {
        alert_dialog_scope("ad-stab", None, || {
            let btn: Element = button("ad-stab-allow").text("Allow").into();
            AlertDialogAction(AlertDialogActionProps {
                children: Some(btn),
                base: None,
                ..Default::default()
            })
        })
    });
    assert!(
        find_tid(&action, "ad-stab-allow").is_some(),
        "Action must wrap the child Button without rewriting its template_id"
    );
}

fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, tid))
}

#[cfg(feature = "animation-base")]
fn overlay_opacity(el: &Element, tid: &str) -> Option<f32> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el.style.opacity);
    }
    el.children.iter().find_map(|c| overlay_opacity(c, tid))
}

#[cfg(feature = "animation-base")]
fn overlay_has_fade_out(el: &Element, tid: &str) -> bool {
    if el.template_id.as_deref() == Some(tid) {
        return el.style.animations.iter().any(|a| a.name == "fade-out-0");
    }
    el.children.iter().any(|c| overlay_has_fade_out(c, tid))
}

#[cfg(feature = "animation-base")]
fn find_text_color(el: &Element, needle: &str) -> Option<[u8; 4]> {
    if el.text.as_deref() == Some(needle) {
        return el.text_style.color;
    }
    el.children.iter().find_map(|c| find_text_color(c, needle))
}

#[test]
#[cfg(feature = "animation-base")]
fn alert_dialog_action_click_keeps_exit_fade_after_rebuild() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::hoist_portal_subtrees;
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_allow_exit__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_allow_exit").open(open).child(move || {
                alert_dialog_content(AlertDialogContentOpts::default(), || {
                    AlertDialogAction(AlertDialogActionProps {
                        children: Some(button("ad_allow_exit__allow").text("Allow").into()),
                        base: None,
                        ..Default::default()
                    })
                })
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        for _ in 0..20 {
            let _ = tick_hoisted_modal_css(&mut tree);
        }
    }

    let click = {
        let mut tree = live.borrow_mut();
        let allow = find_tid_mut(&mut tree, "ad_allow_exit__allow").expect("Allow");
        allow.on_click.as_ref().expect("close").clone()
    };
    click.borrow_mut()();
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(
            flush_modal_shell_motion_subtree(&mut tree),
            "close rebuild must seed exit"
        );
        let _ = tick_hoisted_modal_css(&mut tree);
        let tid = "ad_allow_exit__modal_overlay";
        let opacity = overlay_opacity(&tree, tid).expect("overlay after Allow click");
        assert!(
            opacity > 0.7,
            "Allow close must not skip fade-out, overlay opacity={opacity}"
        );
        assert!(
            overlay_has_fade_out(&tree, tid),
            "fade-out-0 must still be playing after Allow click"
        );
    }
    // Second remount after `needs_exit_anim` was consumed (gallery chrome
    // / variant paint). Must re-seed, not dest-reset the scrim.
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(
            flush_modal_shell_motion_subtree(&mut tree),
            "second close rebuild must keep exit, not finish"
        );
        let tid = "ad_allow_exit__modal_overlay";
        let opacity = overlay_opacity(&tree, tid).expect("overlay after second remount");
        assert!(
            opacity > 0.5,
            "second remount must not skip fade-out, overlay opacity={opacity}"
        );
        assert!(
            overlay_has_fade_out(&tree, tid),
            "fade-out-0 must survive a second island remount"
        );
    }
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[cfg(feature = "animation-base")]
fn alert_dialog_action_click_keeps_description_variant_color() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::renderer::take_capture_jobs;
    use crate::style::parse_text_color;
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    const DESC: &str = "You can change this later in Settings.";
    let want = alert_dialog_chrome(AlertDialogVariant::Success, false).muted_fg;
    let muted_token = parse_text_color("muted-foreground");
    let fg_token = parse_text_color("foreground");

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_desc_exit__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_desc_exit").open(open).child(move || {
                alert_dialog_content(
                    AlertDialogContentOpts {
                        variant: AlertDialogVariant::Success,
                        ..Default::default()
                    },
                    || {
                        vstack()
                            .child(AlertDialogDescription(AlertDialogDescriptionProps {
                                children: Some(Element::new().text(DESC)),
                                base: None,
                            }))
                            .child(AlertDialogAction(AlertDialogActionProps {
                                children: Some(button("ad_desc_exit__allow").text("Allow").into()),
                                base: None,
                                ..Default::default()
                            }))
                            .into()
                    },
                )
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        for _ in 0..20 {
            let _ = tick_hoisted_modal_css(&mut tree);
        }
        let got = find_text_color(&tree, DESC).expect("description while open");
        assert_eq!(got, want, "open description must keep success muted");
    }

    let click = {
        let mut tree = live.borrow_mut();
        let allow = find_tid_mut(&mut tree, "ad_desc_exit__allow").expect("Allow");
        allow.on_click.as_ref().expect("close").clone()
    };
    click.borrow_mut()();
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        let _ = tick_hoisted_modal_css(&mut tree);
        let got = find_text_color(&tree, DESC).expect("description after Allow");
        assert_ne!(
            got, muted_token,
            "close must not fall back to muted-foreground"
        );
        assert_ne!(
            got, fg_token,
            "close must not fall back to default foreground"
        );
        assert_eq!(got, want, "exit description must keep success muted");

        let _ = take_capture_jobs();
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [-1.0, -1.0]);
        let jobs = take_capture_jobs();
        let job = jobs
            .iter()
            .find(|j| j.text_nodes.iter().any(|n| n.text == DESC))
            .expect("zoom capture after Allow must include description");
        let node = job
            .text_nodes
            .iter()
            .find(|n| n.text == DESC)
            .expect("capture description");
        assert_eq!(
            node.style.color_resolved(),
            want,
            "zoom capture after Allow must keep success muted"
        );
    }
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[cfg(feature = "animation-base")]
fn alert_dialog_open_in_wrap_row_keeps_neighbor_gap_and_enter_fade() {
    // Gallery 1c-ad unpacks Trigger+Content into a wrap HStack. A 0×0 static
    // island host between buttons doubles `gap`; opening makes it absolute so
    // only the clicked trigger's neighbor spacing jumps. Dest-reset then
    // punches that caption through the scrim and StructureChanged skips fade.
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::core::tree::IntoElementList;
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::primitives::r#box::hstack;
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    fn pack_dialog(id: &str, open: Signal<bool>, btn_id: &str, label: &str) -> Element {
        alert_dialog_scope(id, Some(open), || {
            let trigger = AlertDialogTrigger(AlertDialogTriggerProps {
                id: String::new(),
                children: Some(
                    button(btn_id)
                        .variant("outline")
                        .size("sm")
                        .text(label)
                        .into(),
                ),
                base: None,
                ..Default::default()
            });
            let content = alert_dialog_content(AlertDialogContentOpts::default(), || {
                AlertDialogTitle(AlertDialogTitleProps {
                    children: Some(Element::new().text("Sure?")),
                    base: None,
                    ..Default::default()
                })
            });
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![trigger, content])
        })
    }

    fn btn_box(el: &Element, tid: &str) -> [f32; 4] {
        let btn = find_tid(el, tid).unwrap_or_else(|| panic!("missing `{tid}`"));
        btn.layout_bounds
    }

    fn neighbor_gap(el: &Element, a: &str, b: &str) -> f32 {
        let aa = btn_box(el, a);
        let bb = btn_box(el, b);
        bb[0] - (aa[0] + aa[2])
    }

    fn has_fade_in(el: &Element) -> bool {
        el.style.animations.iter().any(|a| a.name == "fade-in-0")
            || el.children.iter().any(has_fade_in)
    }

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, "one two Sure?");
    install_shared_font_system(&mut fs);

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open_a) = create_root(|| {
        let open_a = use_signal("ad_gap_a__open", || false);
        let open_b = use_signal("ad_gap_b__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let mut row = hstack()
                .gap("0.5rem")
                .items("center")
                .wrap("wrap")
                .w("100%");
            for child in pack_dialog("ad-gap-a", open_a, "ad-gap-a-btn", "one").into_element_list()
            {
                row = row.child(child);
            }
            for child in pack_dialog("ad-gap-b", open_b, "ad-gap-b-btn", "two").into_element_list()
            {
                row = row.child(child);
            }
            *live_e.borrow_mut() = row.into();
        });
        open_a
    });

    let layout_once = |live: &RefCell<Element>, layout: &mut LayoutTree| {
        drain_deferred_view_remounts();
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        let _ = flush_modal_shell_motion_subtree(&mut tree);
        layout.build_and_layout(&mut tree, 800.0, 400.0, [0.0, 0.0])
    };

    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    let _ = layout_once(&live, &mut layout);
    let gap_closed = neighbor_gap(&live.borrow(), "ad-gap-a-btn", "ad-gap-b-btn");
    assert!(
            gap_closed > 4.0 && gap_closed < 16.0,
            "closed wrap row should keep a single 0.5rem gap, not a doubled flex hole, gap={gap_closed}"
        );

    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open_a.set(true);
    drain_deferred_view_remounts();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(
            flush_modal_shell_motion_subtree(&mut tree),
            "first open must seed enter motion"
        );
        assert!(
            has_fade_in(&tree),
            "first open must seed fade-in-0; a layout jump FullClear looks like a skipped enter"
        );
        let _ = tick_hoisted_modal_css(&mut tree);
    }
    let open_result = layout_once(&live, &mut layout);
    let gap_open = neighbor_gap(&live.borrow(), "ad-gap-a-btn", "ad-gap-b-btn");
    assert!(
        (gap_open - gap_closed).abs() < 1.0,
        "opening must not change the gap to the next trigger (closed={gap_closed} open={gap_open})"
    );

    let one = open_result
        .text_nodes
        .iter()
        .position(|n| n.text == "one")
        .expect("clicked trigger caption");
    assert!(
            one < open_result.overlay_split.text_start,
            "clicked trigger text must stay in the page band under the scrim, not overlay_split.text_start={}",
            open_result.overlay_split.text_start
        );

    let split_i = open_result
        .overlay_split
        .instance_start
        .min(open_result.instances.len());
    let btn = btn_box(&live.borrow(), "ad-gap-a-btn");
    let scrim_covers_trigger = open_result.instances[split_i..].iter().any(|i| {
        i.rect_size[0] > 400.0
            && i.rect_pos[0] <= btn[0] + 1.0
            && i.rect_pos[1] <= btn[1] + 1.0
            && i.rect_pos[0] + i.rect_size[0] >= btn[0] + btn[2] - 1.0
            && i.rect_pos[1] + i.rect_size[1] >= btn[1] + btn[3] - 1.0
    });
    assert!(
            scrim_covers_trigger,
            "overlay occupancy must include the fullscreen scrim over the clicked trigger, otherwise hover dest-reset punches the caption through; overlay sizes={:?}",
            open_result.instances[split_i..]
                .iter()
                .map(|i| (i.rect_pos, i.rect_size, i.color[3]))
                .collect::<Vec<_>>()
        );

    let overlay = {
        fn overlay_opacity(el: &Element) -> Option<f32> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t.ends_with("__modal_overlay"))
            {
                return Some(el.style.opacity);
            }
            el.children.iter().find_map(overlay_opacity)
        }
        overlay_opacity(&live.borrow()).expect("overlay after first tick")
    };
    assert!(
        overlay < 0.35,
        "first present after open must still be fading, not a skipped enter; opacity={overlay}"
    );
    assert!(
        has_fade_in(&live.borrow()),
        "enter fade-in-0 must survive the first layout after open"
    );

    crate::style::utility::set_frame_dt(0.0);
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[cfg(feature = "animation-base")]
fn alert_dialog_open_must_become_visible_across_layout_frames() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_open_vis__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_open_vis").open(open).child(move || {
                alert_dialog_content(AlertDialogContentOpts::default(), || {
                    AlertDialogTitle(AlertDialogTitleProps {
                        children: Some(Element::new().text("Open me")),
                        base: None,
                    })
                    .into()
                })
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    let mut opacities = Vec::new();
    for _ in 0..10 {
        drain_deferred_view_remounts();
        drain_pending_modal_unmounts();
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        let _ = flush_modal_shell_motion_subtree(&mut tree);
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
        let _ = tick_hoisted_modal_css(&mut tree);
        opacities.push(overlay_opacity(&tree, "ad_open_vis__modal_overlay"));
    }
    let first = opacities[0].expect("first open frame must keep the overlay");
    let last = opacities[9].expect("dialog must stay mounted while entering");
    assert!(
        first < 0.35,
        "first open frame must still be fading in, not skipped to rest; opacities={opacities:?}"
    );
    assert!(
            last > 0.45,
            "dialog must become visible across open frames, not stay opacity 0; opacities={opacities:?}"
        );
    assert!(
        last > first + 0.15,
        "enter opacity must keep rising; opacities={opacities:?}"
    );

    crate::style::utility::set_frame_dt(0.0);
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

fn card_chrome_instances(
    instances: &[crate::renderer::InstanceInput],
) -> Vec<(usize, [f32; 2], f32, f32)> {
    instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.rect_size[0] > 180.0 && (i.shadow_blur > 4.0 || i.border_width > 0.4))
        .map(|(idx, i)| (idx, i.rect_size, i.shadow_blur, i.border_width))
        .collect()
}

#[test]
fn alert_dialog_info_footer_hover_must_not_stack_card_chrome() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::damage::{
        damage_covers_full, intersect_rects, overlay_encode_occupancy, overlay_encode_scissor,
        overlay_mounted_replace_holes, take_frame_paint_nodes, PaintDirtyKind,
    };
    use crate::app::draw_cull::padded_xywh;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::{hoist_portal_subtrees, hoisted_portal_stamps, LayoutTree};
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_hover_stack__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_hover_stack").open(open).child(move || {
                alert_dialog_content(
                    AlertDialogContentOpts {
                        variant: AlertDialogVariant::Info,
                        ..Default::default()
                    },
                    || {
                        vstack()
                            .gap("1rem")
                            .child(AlertDialogTitle(AlertDialogTitleProps {
                                children: Some(Element::new().text("Builder + Contentent")),
                                base: None,
                            }))
                            .child(AlertDialogCancel(AlertDialogCancelProps {
                                id: "ad_hover_stack__cancel".into(),
                                children: Some(
                                    button("ad_hover_stack__cancel").text("Cancel").into(),
                                ),
                                base: None,
                                ..Default::default()
                            }))
                            .child(AlertDialogAction(AlertDialogActionProps {
                                children: Some(button("ad_hover_stack__ok").text("OK").into()),
                                base: None,
                                ..Default::default()
                            }))
                            .into()
                    },
                )
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    let open_result = {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        for _ in 0..24 {
            let _ = tick_hoisted_modal_css(&mut tree);
        }
        layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0])
    };

    let chrome = card_chrome_instances(&open_result.instances);
    assert_eq!(
            chrome.len(),
            1,
            "info card must emit live border/shadow once; stacking copies thicken the corner halo: {chrome:?}"
        );

    let stamps = hoisted_portal_stamps(&live.borrow());
    assert!(
        stamps.len() >= 2,
        "overlay + stage stamps required; stamps={stamps:?}"
    );

    let (ok, card) = {
        let tree = live.borrow();
        let ok = find_tid(&tree, "ad_hover_stack__ok").expect("OK");
        let card = find_tid(&tree, "ad_hover_stack__modal_zoom").expect("zoom card");
        assert!(
                ok.has_overlay_stamp(),
                "OK must inherit overlay stamp or hover dest-resets as a page Node and stacks card chrome"
            );
        (ok.layout_bounds, card.layout_bounds)
    };

    let _ = take_frame_paint_nodes();
    {
        let tree = live.borrow();
        let ok = find_tid(&tree, "ad_hover_stack__ok").expect("OK");
        crate::app::damage::push_frame_paint_element(ok, ok.paint_scroll_offset());
    }
    let dirties = take_frame_paint_nodes();
    let overlay_dirties: Vec<_> = dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Overlay)
        .copied()
        .collect();
    assert!(
        !overlay_dirties.is_empty(),
        "OK hover must be Overlay dirty; dirties={dirties:?}"
    );

    for d in &overlay_dirties {
        let holes = overlay_mounted_replace_holes(&stamps, &stamps, d.key, Some(d.bounds));
        let occ = overlay_encode_occupancy(d.key, Some(d.bounds), &stamps, None);
        let scissor = overlay_encode_scissor(Some(d.bounds), occ, None)
            .expect("hover must still have an overlay encode scissor");
        let card_halo = padded_xywh(card, 16.0);
        if let Some(halo_in_scissor) = intersect_rects(card_halo, scissor) {
            assert!(
                    holes.iter().any(|h| damage_covers_full(halo_in_scissor, *h)),
                    "OK hover dest-reset must cover card chrome in the encode scissor or src-over stacks the blue corner; holes={holes:?} scissor={scissor:?} ok={ok:?} card={card:?} stamps={stamps:?}"
                );
        }
        assert!(
                holes.iter().any(|h| damage_covers_full(scissor, *h)),
                "OK hover dest-reset must cover encode or PAGE slivers remain; holes={holes:?} scissor={scissor:?}"
            );
        assert!(
                holes.iter().any(|h| damage_covers_full(*h, scissor)),
                "OK hover encode must cover dest-reset or leftover chrome stacks; holes={holes:?} scissor={scissor:?}"
            );
        assert!(
                !holes.iter().any(|h| damage_covers_full(card_halo, *h)),
                "OK hover must not dest-reset pad(card) (punches the blur ring); holes={holes:?} card_halo={card_halo:?}"
            );
    }

    crate::style::utility::set_frame_dt(0.0);
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn alert_dialog_info_cancel_hover_must_not_leave_card_bottom_residual() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::damage::{
        damage_covers_full, intersect_rects, overlay_encode_occupancy, overlay_encode_scissor,
        overlay_mounted_replace_holes, take_frame_paint_nodes, PaintDirtyKind,
    };
    use crate::app::draw_cull::padded_xywh;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::{hoist_portal_subtrees, hoisted_portal_stamps, LayoutTree};
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_cancel_hover__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_cancel_hover").open(open).child(move || {
                alert_dialog_content(
                    AlertDialogContentOpts {
                        variant: AlertDialogVariant::Info,
                        ..Default::default()
                    },
                    || {
                        vstack()
                            .gap("1rem")
                            .child(AlertDialogTitle(AlertDialogTitleProps {
                                children: Some(Element::new().text("Builder + Contentent")),
                                base: None,
                            }))
                            .child(
                                hstack()
                                    .gap("0.5rem")
                                    .justify("end")
                                    .child(AlertDialogCancel(AlertDialogCancelProps {
                                        id: "ad_cancel_hover__cancel".into(),
                                        children: Some(
                                            button("ad_cancel_hover__cancel").text("Cancel").into(),
                                        ),
                                        base: None,
                                        ..Default::default()
                                    }))
                                    .child(AlertDialogAction(AlertDialogActionProps {
                                        children: Some(
                                            button("ad_cancel_hover__ok").text("OK").into(),
                                        ),
                                        base: None,
                                        ..Default::default()
                                    })),
                            )
                            .into()
                    },
                )
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        for _ in 0..24 {
            let _ = tick_hoisted_modal_css(&mut tree);
        }
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
    }

    let (cancel, card) = {
        let tree = live.borrow();
        let cancel = find_tid(&tree, "ad_cancel_hover__cancel").expect("Cancel");
        let card = find_tid(&tree, "ad_cancel_hover__modal_zoom").expect("zoom card");
        assert!(
            cancel.has_overlay_stamp(),
            "Cancel must inherit overlay stamp or hover dest-resets as a page Node"
        );
        (cancel.layout_bounds, card.layout_bounds)
    };

    let stamps = hoisted_portal_stamps(&live.borrow());
    let _ = take_frame_paint_nodes();
    {
        let tree = live.borrow();
        let cancel = find_tid(&tree, "ad_cancel_hover__cancel").expect("Cancel");
        crate::app::damage::push_frame_paint_element(cancel, cancel.paint_scroll_offset());
    }
    let dirties = take_frame_paint_nodes();
    let overlay_dirties: Vec<_> = dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Overlay)
        .copied()
        .collect();
    assert!(
        !overlay_dirties.is_empty(),
        "Cancel hover must be Overlay dirty; dirties={dirties:?}"
    );

    let card_bottom = [card[0], card[1] + card[3] - 2.0, card[2], 2.0];
    let card_pad = padded_xywh(card, 16.0);
    let far_scrim = [card[0] - 16.0, card[1] - 16.0, 8.0, 8.0];
    for d in &overlay_dirties {
        let holes = overlay_mounted_replace_holes(&stamps, &stamps, d.key, Some(d.bounds));
        let occ = overlay_encode_occupancy(d.key, Some(d.bounds), &stamps, None);
        let scissor = overlay_encode_scissor(Some(d.bounds), occ, None)
            .expect("Cancel hover must still have an overlay encode scissor");
        if let Some(nearby) = intersect_rects(card_bottom, scissor) {
            assert!(
                    holes.iter().any(|h| damage_covers_full(nearby, *h)),
                    "Cancel dest-reset must cover nearby bottom chrome in the encode scissor; holes={holes:?} nearby={nearby:?} cancel={cancel:?} card={card:?}"
                );
        }
        assert!(
                !holes.iter().any(|h| damage_covers_full(card_pad, *h)),
                "Cancel hover must not dest-reset pad(card) (punches the blur ring); holes={holes:?} card_pad={card_pad:?}"
            );
        assert!(
            !holes
                .iter()
                .any(|h| intersect_rects(*h, far_scrim).is_some()),
            "Cancel hover must not PAGE-punch the far blur ring; holes={holes:?} far={far_scrim:?}"
        );
        assert!(
                holes.iter().any(|h| damage_covers_full(scissor, *h)),
                "Cancel dest-reset must cover encode or PAGE sliver remains; holes={holes:?} scissor={scissor:?}"
            );
        assert!(
                holes.iter().any(|h| damage_covers_full(*h, scissor)),
                "Cancel encode must cover dest-reset or leftover chrome stacks; holes={holes:?} scissor={scissor:?}"
            );
    }

    crate::style::utility::set_frame_dt(0.0);
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
#[cfg(feature = "animation-base")]
fn alert_dialog_info_ok_click_must_keep_exit_mid_across_layout_frames() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::utils::modal_motion::{
        drain_pending_modal_unmounts, flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
    };

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("ad_exit_layout__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = alert_dialog("ad_exit_layout").open(open).child(move || {
                alert_dialog_content(
                    AlertDialogContentOpts {
                        variant: AlertDialogVariant::Info,
                        ..Default::default()
                    },
                    || {
                        AlertDialogAction(AlertDialogActionProps {
                            children: Some(button("ad_exit_layout__ok").text("OK").into()),
                            base: None,
                            ..Default::default()
                        })
                    },
                )
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    drain_deferred_view_remounts();
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    open.set(true);
    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    {
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        assert!(flush_modal_shell_motion_subtree(&mut tree));
        for _ in 0..24 {
            let _ = tick_hoisted_modal_css(&mut tree);
        }
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
    }

    let click = {
        let mut tree = live.borrow_mut();
        find_tid_mut(&mut tree, "ad_exit_layout__ok")
            .expect("OK")
            .on_click
            .as_ref()
            .expect("close")
            .clone()
    };
    click.borrow_mut()();
    drain_deferred_view_remounts();

    let mut opacities = Vec::new();
    for frame in 0..8 {
        // Gallery RedrawRequested: present, then hover callbacks, then drain.
        // A flush that finishes too early unmounts here and the next frame pops.
        drain_deferred_view_remounts();
        drain_pending_modal_unmounts();
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        let _ = flush_modal_shell_motion_subtree(&mut tree);
        let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
        let _ = tick_hoisted_modal_css(&mut tree);
        let tid = "ad_exit_layout__modal_overlay";
        let opacity = overlay_opacity(&tree, tid);
        opacities.push(opacity);
        if frame == 0 {
            let o = opacity.expect("first exit layout frame must keep the overlay");
            assert!(
                    o > 0.55,
                    "first gallery-like exit frame must still be fading, not popped; opacity={o} frames={opacities:?}"
                );
            assert!(
                overlay_has_fade_out(&tree, tid),
                "fade-out-0 must survive the first layout after OK"
            );
            use crate::app::damage::{
                overlay_band_encode_flags, overlay_band_promote_dest_reset_siblings,
                overlay_mounted_replace_holes, take_frame_paint_nodes, PaintDirtyKind,
            };
            use crate::layout::layout_tree::hoisted_portal_stamps;
            let stamps = hoisted_portal_stamps(&tree);
            let dirties = take_frame_paint_nodes();
            let overlay_dirties: Vec<_> = dirties
                .iter()
                .filter(|d| d.kind == PaintDirtyKind::Overlay)
                .copied()
                .collect();
            assert!(
                !overlay_dirties.is_empty(),
                "first exit tick must dest-reset overlay/zoom; dirties={dirties:?}"
            );
            let mut holes = Vec::new();
            let dirty_keys: Vec<u64> = overlay_dirties.iter().map(|d| d.key).collect();
            for d in &overlay_dirties {
                holes.extend(overlay_mounted_replace_holes(
                    &stamps,
                    &stamps,
                    d.key,
                    Some(d.bounds),
                ));
            }
            let band_keys: Vec<u64> = stamps.iter().map(|(k, _)| *k).collect();
            let mut flags = overlay_band_encode_flags(&band_keys, &dirty_keys, false, &band_keys);
            overlay_band_promote_dest_reset_siblings(
                &mut flags,
                &band_keys,
                &dirty_keys,
                &stamps,
                &holes,
            );
            assert!(
                    flags.iter().any(|f| *f),
                    "exit dest-reset must encode at least one overlay band or PAGE wipe is a pop; holes={holes:?} flags={flags:?} stamps={stamps:?}"
                );
            if stamps.len() >= 2 {
                assert!(
                        flags.iter().filter(|f| **f).count() >= 2,
                        "exit dest-reset of overlay+card must encode both bands; holes={holes:?} flags={flags:?} stamps={stamps:?} dirty={overlay_dirties:?}"
                    );
            }
        }
    }
    let live_frames = opacities.iter().filter(|o| o.is_some()).count();
    assert!(
            live_frames >= 4,
            "exit must stay mounted across several layout frames, not overlay-then-card pop; opacities={opacities:?}"
        );
    let first = opacities[0].unwrap();
    let mid = opacities[3].unwrap_or(0.0);
    assert!(
        mid + 0.04 < first,
        "exit opacity must keep moving across layout frames; opacities={opacities:?}"
    );

    crate::style::utility::set_frame_dt(0.0);
    drain_pending_modal_unmounts();
    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    clear_overlay_stack();
    reset_registered_anchors();
}
