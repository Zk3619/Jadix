//! Gallery inset shell: flex children must not spill past SidebarInset border-box.

use crate::app::damage::{
    clipped_island_structure_damage, expand_paint_dirties_for_translated_slots, paint_element_key,
    push_frame_paint_key, take_frame_paint_nodes, PaintDirtyKind,
};
use crate::app::draw_cull::node_present_scissor_logical;
use crate::app::reactive::{bind_prop, match_when_reactive, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::app::test_support::reconcile_elements;
use crate::components::scroll_area::scroll_area;
use crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
use crate::components::sidebar::{
    apply_bound_sidebar_menu_button_active, sidebar, sidebar_menu_button, sidebar_provider_scope,
    SidebarInset, SidebarInsetProps,
};
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::{hstack, vstack};
use std::cell::RefCell;
use std::rc::Rc;

fn card_like() -> Element {
    let mut row = hstack().w("100%").min_w(0.0).gap(8.0).wrap("wrap");
    for _ in 0..9 {
        row = row.child(Element::new().min_w(80.0).h(32.0).bg([0.1, 0.1, 0.1, 1.0]));
    }
    vstack()
        .w("100%")
        .min_w(0.0)
        .p(16.0)
        .border([0.8, 0.8, 0.8, 1.0], 1.0)
        .child(Element::new().text("1) Button Variants + State"))
        .child(row)
        .into()
}

fn max_right_overflow(inset: &Element) -> f32 {
    let ib = inset.layout_bounds;
    let inset_r = ib[0] + ib[2];
    let mut worst = 0.0_f32;
    fn walk(el: &Element, inset_r: f32, worst: &mut f32) {
        let b = el.layout_bounds;
        *worst = worst.max((b[0] + b[2]) - inset_r);
        for c in &el.children {
            walk(c, inset_r, worst);
        }
    }
    walk(inset, inset_r, &mut worst);
    worst
}

fn gallery_inset_shell(scroll: Element) -> Element {
    sidebar_provider_scope("gal", None, true, Some("inset"), move || {
        let trigger = Element::from(
            hstack()
                .w("100%")
                .p("0.5rem")
                .flex_shrink(0.0)
                .child(Element::new().w(28.0).h(28.0).bg([0.9, 0.9, 0.9, 1.0])),
        );
        let body = Element::from(vstack().min_w(0.0).min_h(0.0).flex_grow(1.0).child(scroll));
        let inset_children = {
            let mut pack = Element::new();
            pack.set_attr("jadix_fragment", "1");
            pack.children(vec![trigger, body])
        };
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            sidebar("gal-sidebar").variant("inset").into(),
            SidebarInset(SidebarInsetProps {
                variant: Some("inset".into()),
                children: Some(inset_children),
                base: None,
            }),
        ])
    })
}

fn find_inset<'a>(root: &'a Element) -> &'a Element {
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    find_slot(root, "sidebar-inset").expect("inset")
}

#[test]
fn inset_scroll_children_do_not_exceed_inset_width() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let scroll_body = vstack()
                .w("100%")
                .p("1.5rem")
                .child(Element::new().text("基础"))
                .child(card_like());
            let scroll = Element::from(
                scroll_area("test-scroll")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(scroll_body),
            );
            let mut root = gallery_inset_shell(scroll);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 600.0, [-1.0, -1.0]);
            let inset = find_inset(&root);
            let overflow = max_right_overflow(inset);
            assert!(
                overflow < 1.0,
                "descendants must not exceed SidebarInset width; overflow={overflow}px bounds={:?}",
                inset.layout_bounds
            );
        });
    });
}

#[test]
fn inset_fills_provider_height_without_descendant_spill() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let scroll_body = vstack()
                .w("100%")
                .p("1.5rem")
                .child(Element::new().text("基础"))
                .child(card_like());
            let scroll = Element::from(
                scroll_area("test-scroll")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(scroll_body),
            );
            let mut root = gallery_inset_shell(scroll);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 600.0, [-1.0, -1.0]);
            let inset = find_inset(&root);
            let ib = inset.layout_bounds;
            fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
                if el.template_id.as_deref() == Some(id) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_tid(c, id))
            }
            let scroll_shell = find_tid(&root, "test-scroll").expect("scroll shell");
            let sb = scroll_shell.layout_bounds;
            let inset_b = ib[1] + ib[3];
            let shell_b = sb[1] + sb[3];
            assert!(
                shell_b <= inset_b + 1.0,
                "ScrollArea chrome must fit inside SidebarInset; shell={sb:?} inset={ib:?}"
            );
            assert!(
                sb[0] >= ib[0] - 1.0 && sb[0] + sb[2] <= ib[0] + ib[2] + 1.0,
                "ScrollArea chrome must not spill horizontally; shell={sb:?} inset={ib:?}"
            );
        });
    });
}

fn wrap_chip_row(min_w_zero: bool) -> Element {
    // Gallery CursorShowcase: many chips, wrap, often without min_w(0).
    let mut row = hstack().w("100%").gap(8.0).wrap("wrap");
    if min_w_zero {
        row = row.min_w(0.0);
    }
    for _ in 0..28 {
        row = row.child(
            Element::new()
                .min_w(72.0)
                .h(28.0)
                .bg([0.15, 0.15, 0.15, 1.0]),
        );
    }
    row.into()
}

fn page_scroll(id: &str, body: Element) -> Element {
    scroll_area(id)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .child(
            vstack()
                .w("100%")
                .p("1.5rem")
                .child(Element::new().text("page"))
                .child(body),
        )
        .into()
}

fn scroll_max_x(result: &crate::core::tree::RenderResult, id: &str) -> (f32, f32, f32) {
    result
        .scroll_metrics
        .iter()
        .find(|(rest, ..)| rest == id)
        .map(|(_, bounds, max, content)| (bounds[2], max[0], content[0]))
        .unwrap_or_else(|| panic!("missing scroll metrics for {id}"))
}

#[test]
fn wrap_chips_without_min_w_zero_must_not_enable_x_scroll() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let scroll = page_scroll("chip-scroll", wrap_chip_row(false));
            let mut root = gallery_inset_shell(scroll);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 1000.0, 700.0, [-1.0, -1.0]);
            let (vp_w, max_x, content_w) = scroll_max_x(&result, "chip-scroll");
            assert!(
                max_x < 0.5,
                "wrapping chips must not enable overflow_x auto thumb \
                 (content_w={content_w:.1} vp_w={vp_w:.1} max_x={max_x:.1})"
            );
        });
    });
}

#[test]
fn switching_wide_page_to_wrap_must_not_keep_x_scroll() {
    // Live Gallery: Match replaces scroll-basics with scroll-events. A leftover
    // max-content width from the previous page must not light the new horizontal bar.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let wide = page_scroll(
                "scroll-a",
                vstack()
                    .w("100%")
                    .child(Element::new().w(1400.0).h(40.0).bg([0.2, 0.2, 0.2, 1.0]))
                    .into(),
            );
            let mut old = gallery_inset_shell(wide);
            let mut layout = LayoutTree::new();
            let first = layout.build_and_layout(&mut old, 1000.0, 700.0, [-1.0, -1.0]);
            let (_, wide_max_x, _) = scroll_max_x(&first, "scroll-a");
            assert!(
                wide_max_x > 8.0,
                "setup: wide page must actually overflow x (max_x={wide_max_x})"
            );

            let wrap = page_scroll("scroll-b", wrap_chip_row(false));
            let mut new = gallery_inset_shell(wrap);
            let _ = reconcile_elements(&mut new, &mut old);
            // Live Gallery Match swaps the island then `build_and_layout_fast_ex`
            // (page StructureChanged does not full-rebuild Taffy).
            let second =
                layout.build_and_layout_fast(&mut new, 1000.0, 700.0, [-1.0, -1.0], false, false);
            let (vp_w, max_x, content_w) = scroll_max_x(&second, "scroll-b");
            let inset = find_inset(&new);
            let ib = inset.layout_bounds;
            let root_w = new.layout_bounds[2];
            assert!(
                root_w <= 1000.0 + 1.0,
                "page switch must not widen the window (root_w={root_w:.1})"
            );
            assert!(
                max_x < 0.5,
                "after switching off a wide page, wrap content must not keep a horizontal bar \
                 (content_w={content_w:.1} vp_w={vp_w:.1} max_x={max_x:.1} inset={ib:?} root_w={root_w:.1})"
            );
        });
    });
}

fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, id))
}

fn replace_child_with_tid(root: &mut Element, id: &str, fresh: Element) {
    fn walk(el: &mut Element, id: &str, fresh: &mut Option<Element>) -> bool {
        for c in &mut el.children {
            if c.template_id.as_deref() == Some(id) {
                *c = fresh.take().expect("fresh page");
                return true;
            }
            if walk(c, id, fresh) {
                return true;
            }
        }
        false
    }
    let mut fresh = Some(fresh);
    assert!(
        walk(root, id, &mut fresh),
        "expected template_id {id} in shell"
    );
}

#[test]
fn in_place_island_swap_must_keep_scroll_shell_inside_inset() {
    // Live path: `reconcile_subtree` assigns the new fragment onto the Match island
    // then `build_and_layout_fast_ex`. A full-window overlay H-bar after switching
    // means the new ScrollArea outer was measured as the window, not the inset.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_inset_shell(page_scroll("scroll-a", wrap_chip_row(false)));
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 1000.0, 700.0, [-1.0, -1.0]);
            let inset_before = find_inset(&root).layout_bounds;

            replace_child_with_tid(
                &mut root,
                "scroll-a",
                page_scroll("scroll-b", wrap_chip_row(false)),
            );
            let result =
                layout.build_and_layout_fast(&mut root, 1000.0, 700.0, [-1.0, -1.0], false, false);
            let inset = find_inset(&root).layout_bounds;
            let shell = find_tid(&root, "scroll-b").expect("scroll-b shell");
            let sb = shell.layout_bounds;
            let (vp_w, max_x, content_w) = scroll_max_x(&result, "scroll-b");
            assert!(
                sb[0] >= inset[0] - 1.0,
                "ScrollArea outer must stay inside inset after island swap; \
                 shell={sb:?} inset={inset:?} (before={inset_before:?})"
            );
            assert!(
                sb[0] + sb[2] <= inset[0] + inset[2] + 1.0,
                "ScrollArea outer must not span under the sidebar; \
                 shell={sb:?} inset={inset:?}"
            );
            assert!(
                max_x < 0.5,
                "island swap must not enable a horizontal thumb \
                 (content_w={content_w:.1} vp_w={vp_w:.1} max_x={max_x:.1} shell={sb:?} inset={inset:?})"
            );
        });
    });
}

fn find_tid_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &mut el.children {
        if let Some(hit) = find_tid_mut(c, id) {
            return Some(hit);
        }
    }
    None
}

#[test]
fn match_island_reuses_scroll_shell_must_not_enable_x_scroll() {
    // `reactive_anchor` stamps `fragment.template_id = anchor_id` before reconcile.
    // Gallery Match therefore reuses the previous page's ScrollArea Taffy nodes
    // (same island tid) instead of treating scroll-basics vs scroll-events as
    // StructureChanged. That leftover measure must not light overflow-x:auto.
    const ISLAND: &str = "examples/gallery/App:section_body";
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            fn stamp_island(mut page: Element) -> Element {
                page.set_attr(
                    "jadix_source_tid",
                    page.template_id.clone().unwrap_or_default(),
                );
                page.template_id = Some(ISLAND.into());
                page
            }

            let mut root = gallery_inset_shell(stamp_island(page_scroll(
                "scroll-a",
                vstack()
                    .w("100%")
                    .child(Element::new().w(1400.0).h(80.0).bg([0.2, 0.2, 0.2, 1.0]))
                    .into(),
            )));
            let mut layout = LayoutTree::new();
            let first = layout.build_and_layout(&mut root, 1000.0, 700.0, [-1.0, -1.0]);
            let (_, wide_max_x, _) = scroll_max_x(&first, "scroll-a");
            assert!(
                wide_max_x > 8.0,
                "setup: wide island page must overflow x (max_x={wide_max_x})"
            );

            let mut next_page = stamp_island(page_scroll("scroll-b", wrap_chip_row(false)));
            let island = find_tid_mut(&mut root, ISLAND).expect("island");
            let _ = reconcile_elements(&mut next_page, island);
            *island = next_page;

            let second =
                layout.build_and_layout_fast(&mut root, 1000.0, 700.0, [-1.0, -1.0], false, false);
            let (vp_w, max_x, content_w) = scroll_max_x(&second, "scroll-b");
            let inset = find_inset(&root).layout_bounds;
            let shell = find_tid(&root, ISLAND).expect("island after swap");
            let sb = shell.layout_bounds;
            assert!(
                max_x < 0.5,
                "Match island reuse must not keep horizontal overflow from the previous page \
                 (content_w={content_w:.1} vp_w={vp_w:.1} max_x={max_x:.1} shell={sb:?} inset={inset:?})"
            );
            assert!(
                sb[0] + sb[2] <= inset[0] + inset[2] + 1.0,
                "reused ScrollArea must not span under the sidebar; shell={sb:?} inset={inset:?}"
            );
        });
    });
}

fn collect_x_tracks(el: &Element, out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = el.focus_id.as_deref() {
        if fid.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX) && fid.ends_with(":x") {
            out.push((fid.to_string(), el.layout_bounds));
        }
    }
    for c in &el.children {
        collect_x_tracks(c, out);
    }
}

fn painted_window_h_bar(
    instances: &[crate::renderer::pipeline::InstanceInput],
    window_w: f32,
    window_h: f32,
) -> Option<[f32; 4]> {
    instances.iter().find_map(|i| {
        let w = i.rect_size[0];
        let h = i.rect_size[1];
        let y = i.rect_pos[1];
        if w > window_w * 0.75
            && h > 2.0
            && h < 20.0
            && y + h > window_h - 24.0
            && i.color[3] > 0.08
        {
            Some([i.rect_pos[0], y, w, h])
        } else {
            None
        }
    })
}

/// Live Gallery: `<Match>` is a 100%×100% placeholder; the island effect stamps
/// `template_id` then `try_reconcile_subtree`. After switching sections the
/// overlay H-bar must stay inside SidebarInset — not a window-bottom track.
#[test]
fn match_when_reactive_page_switch_must_not_paint_window_h_bar() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    const ISLAND: &str = "examples/gallery/App:section_body";
    const W: f32 = 1280.0;
    const H: f32 = 800.0;

    mark_next_effect_as_ui_root();
    let (_owner, section) = create_root(|| {
        let section = use_signal("gal_nav_section_hbar", || "basics".to_string());
        create_effect({
            let section = section.clone();
            let live = Rc::clone(&live);
            move || {
                let sec = section.clone();
                let body = match_when_reactive(ISLAND, move || match sec.get().as_str() {
                    "events" => page_scroll("scroll-events", wrap_chip_row(false)),
                    _ => page_scroll(
                        "scroll-basics",
                        vstack()
                            .w("100%")
                            .child(Element::new().w(1600.0).h(80.0).bg([0.2, 0.2, 0.2, 1.0]))
                            .into(),
                    ),
                });
                *live.borrow_mut() = gallery_inset_shell(body);
            }
        });
        section
    });

    let mut layout = LayoutTree::new();
    {
        let mut root = live.borrow_mut();
        let first = layout.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
        let (_, wide_max_x, _) = scroll_max_x(&first, "scroll-basics");
        assert!(
            wide_max_x > 8.0,
            "setup: basics island must overflow x (max_x={wide_max_x})"
        );
    }

    section.set("events".to_string());

    {
        let mut root = live.borrow_mut();
        // Live page switch is StructureChanged inside inset overflow:clip, so
        // `page_geom_dirty` stays false and scrollbar prepass stays ViewportsOnly.
        // Overlay collect then paints from `scrollbar_track_overlay_abs`.
        let second = layout.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], false, true);
        let (vp_w, max_x, content_w) = scroll_max_x(&second, "scroll-events");
        let inset = find_inset(&root).layout_bounds;
        let mut tracks = Vec::new();
        collect_x_tracks(&root, &mut tracks);
        let window_bar = painted_window_h_bar(&second.instances, W, H);
        let spanning = tracks.iter().find(|(_, b)| {
            b[2] > 1.0 && b[3] > 1.0 && b[0] < inset[0] - 8.0 && b[0] + b[2] > W * 0.85
        });
        assert!(
            max_x < 0.5,
            "Match When switch must not enable horizontal overflow \
             (content_w={content_w:.1} vp_w={vp_w:.1} max_x={max_x:.1} tracks={tracks:?} inset={inset:?})"
        );
        let live_x = tracks.iter().find(|(_, b)| b[2] > 1.0 && b[3] > 1.0);
        assert!(
            live_x.is_none(),
            "overlay collect must collapse the H-bar when max_x≈0 \
             (otherwise the track is narrower than the viewport and paints as if scrollable); \
             live_x={live_x:?} tracks={tracks:?} vp_w={vp_w:.1} content_w={content_w:.1}"
        );
        assert!(
            spanning.is_none(),
            "overlay H-bar must not span under the sidebar after page switch; \
             spanning={spanning:?} tracks={tracks:?} inset={inset:?}"
        );
        assert!(
            window_bar.is_none(),
            "must not paint a window-bottom H-bar after switching to wrap content; \
             painted={window_bar:?} tracks={tracks:?} inset={inset:?}"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn tall_page_body() -> Element {
    vstack()
        .w("100%")
        .children((0..24).map(|i| {
            Element::new()
                .w("100%")
                .h(40.0)
                .bg([0.2, 0.45, 0.9, 1.0])
                .template_id(format!("tall-row-{i}"))
        }))
        .into()
}

fn find_y_track(el: &Element, rest: &str) -> Option<[f32; 4]> {
    let want = format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}{rest}:y");
    fn walk(el: &Element, want: &str) -> Option<[f32; 4]> {
        if el.focus_id.as_deref() == Some(want) {
            return Some(el.layout_bounds);
        }
        el.children.iter().find_map(|c| walk(c, want))
    }
    walk(el, &want)
}

fn find_y_thumb(el: &Element, rest: &str) -> Option<[f32; 4]> {
    let want = format!(
        "{}{rest}:y",
        crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX
    );
    fn walk(el: &Element, want: &str) -> Option<[f32; 4]> {
        if el.focus_id.as_deref() == Some(want) {
            return Some(el.layout_bounds);
        }
        el.children.iter().find_map(|c| walk(c, want))
    }
    walk(el, &want)
}

fn thumb_alpha(result: &crate::core::tree::RenderResult, thumb: [f32; 4]) -> f32 {
    result
        .instances
        .iter()
        .filter(|i| {
            (i.rect_pos[0] - thumb[0]).abs() < 3.0
                && (i.rect_pos[1] - thumb[1]).abs() < 3.0
                && i.rect_size[0] > 1.0
        })
        .map(|i| i.color[3])
        .fold(0.0_f32, f32::max)
}

fn sync_scroll_signals_from_metrics(result: &crate::core::tree::RenderResult, rest: &str) {
    use crate::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_viewport_h_signal_key,
        scroll_viewport_w_signal_key,
    };
    let (_, bounds, _, content) = result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == rest)
        .expect("scroll metrics");
    use_signal(&scroll_viewport_w_signal_key(rest), || 0.0_f32).set(bounds[2]);
    use_signal(&scroll_viewport_h_signal_key(rest), || 0.0_f32).set(bounds[3]);
    use_signal(&scroll_content_w_signal_key(rest), || 0.0_f32).set(content[0]);
    use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).set(content[1]);
}

fn settle_auto_y(rest: &str, target: f32) {
    use crate::style::{tick_animations, use_ui_transition_cubic};
    let tw = use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:y"), 0.0);
    tw.set(target);
    tick_animations(0.08);
    tick_animations(0.08);
}

/// Live Gallery: Match reuses the section_body island across sidebar pages.
/// After the switch, ViewportsOnly + overlay collect must still mount a live
/// Y track for the *new* rest and paint it when the auto cubic is shown.
/// A leftover collapsed track / stale rest focus_id leaves later pages with
/// no bar on wheel or hover.
#[test]
fn match_when_page_switch_must_paint_overlay_y_bar() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    const ISLAND: &str = "examples/gallery/App:section_body_ybar";
    const W: f32 = 1000.0;
    const H: f32 = 500.0;

    mark_next_effect_as_ui_root();
    let (_owner, section) = create_root(|| {
        let section = use_signal("gal_nav_section_ybar", || "a".to_string());
        create_effect({
            let section = section.clone();
            let live = Rc::clone(&live);
            move || {
                let sec = section.clone();
                let body = match_when_reactive(ISLAND, move || match sec.get().as_str() {
                    "b" => page_scroll("scroll-b", tall_page_body()),
                    _ => page_scroll("scroll-a", tall_page_body()),
                });
                *live.borrow_mut() = gallery_inset_shell(body);
            }
        });
        section
    });

    let mut layout = LayoutTree::new();
    {
        let mut root = live.borrow_mut();
        let first = layout.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
        let (_, _, max_y, content) = first
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == "scroll-a")
            .map(|(_, b, m, c)| (*b, m[0], m[1], *c))
            .expect("scroll-a");
        assert!(
            max_y > 8.0 && content[1] > H,
            "setup: page A must overflow y (max_y={max_y} content_h={})",
            content[1]
        );
        sync_scroll_signals_from_metrics(&first, "scroll-a");
        settle_auto_y("scroll-a", 1.0);
        let shown = layout.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], true, true);
        let thumb = find_y_thumb(&root, "scroll-a").expect("Y thumb on first page");
        let a = thumb_alpha(&shown, thumb);
        assert!(
            a > 0.85 && thumb[3] > 20.0,
            "setup: first page must paint Y thumb (a={a} thumb={thumb:?})"
        );
    }

    section.set("b".to_string());

    {
        let mut root = live.borrow_mut();
        // Live route Match that FullClears also runs Full scrollbar prepass **before**
        // post-render publishes the new rest's vw/vh/ch. Zero signals must not collapse
        // the overlay track — ViewportsOnly never restores hit geometry afterward.
        crate::app::update_scrollbar_elements_for_test(
            &mut root,
            &mut layout,
            crate::app::ScrollbarUpdateMode::Full,
        );
        let second = layout.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
        let metric = second
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == "scroll-b");
        let Some((_, bounds, max, content)) = metric else {
            panic!(
                "after switch, scroll-b metrics missing; have {:?}",
                second
                    .scroll_metrics
                    .iter()
                    .map(|(id, ..)| id.clone())
                    .collect::<Vec<_>>()
            );
        };
        assert!(
            max[1] > 8.0,
            "page B must overflow y after Match switch (max_y={} content={:?} vp={:?})",
            max[1],
            content,
            bounds
        );
        sync_scroll_signals_from_metrics(&second, "scroll-b");
        crate::app::update_scrollbar_elements_for_test(
            &mut root,
            &mut layout,
            crate::app::ScrollbarUpdateMode::ViewportsOnly,
        );
        settle_auto_y("scroll-b", 1.0);
        let shown = layout.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], true, true);
        let track = find_y_track(&root, "scroll-b");
        let thumb = find_y_thumb(&root, "scroll-b");
        let a = thumb.map(|t| thumb_alpha(&shown, t)).unwrap_or(0.0);
        let old_track = find_y_track(&root, "scroll-a");
        let edge_hit = track.and_then(|t| {
            let path =
                crate::app::pointer_events::hit_path(&root, t[0] + t[2] * 0.5, t[1] + t[3] * 0.5);
            crate::app::pointer_events::path_deepest_focus_id(&path).map(str::to_string)
        });
        assert!(
            track.is_some_and(|t| t[2] > 2.0 && t[3] > 40.0)
                && thumb.is_some_and(|t| t[3] > 8.0)
                && a > 0.85
                && edge_hit
                    .as_deref()
                    .is_some_and(|id| id.contains("scrollbar") && id.contains("scroll-b")),
            "after Match page switch, new rest must keep a hittable Y bar that paints\n\
             track={track:?} thumb={thumb:?} alpha={a} edge_hit={edge_hit:?}\n\
             leftover_old_track={old_track:?}\n\
             metrics_b={:?}",
            (bounds, max, content)
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn data_active(el: &Element) -> Option<&str> {
    el.attrs.get("data-active").map(String::as_str)
}

fn collect_text_blobs(result: &crate::core::tree::RenderResult) -> Vec<String> {
    result.text_nodes.iter().map(|n| n.text.clone()).collect()
}

/// Live Gallery: 从「指针事件」切到「基础」时，标题/描述 Match 岛与侧栏 isActive
/// bind 必须一起更新。以前 PartialLoad dest-reset 在 Taffy id 对不上时直接放弃，
/// 旧标题叠在新页上，侧栏仍高亮上一页。
#[test]
fn match_when_events_to_basics_must_update_title_and_sidebar() {
    reset_registered_anchors();
    clear_test_reconcile_root();
    let _ = take_frame_paint_nodes();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    const TITLE: &str = "examples/gallery/App:section_title";
    const DESC: &str = "examples/gallery/App:section_desc";
    const BODY: &str = "examples/gallery/App:section_body";
    const W: f32 = 1280.0;
    const H: f32 = 800.0;

    mark_next_effect_as_ui_root();
    let (_owner, section) = create_root(|| {
        let section = use_signal("gal_nav_section_switch", || "events".to_string());
        create_effect({
            let section = section.clone();
            let live = Rc::clone(&live);
            move || {
                let sec = section.clone();
                let shell = sidebar_provider_scope("gal-switch", None, true, Some("inset"), {
                    let sec = sec.clone();
                    move || {
                        let basics_btn = bind_prop(
                            sidebar_menu_button("gal-nav-basics").text("基础").into(),
                            "bind-gal-nav-basics",
                            {
                                let sec = sec.clone();
                                move || sec.get() == "basics"
                            },
                            apply_bound_sidebar_menu_button_active,
                        );
                        let events_btn = bind_prop(
                            sidebar_menu_button("gal-nav-events")
                                .text("指针事件")
                                .into(),
                            "bind-gal-nav-events",
                            {
                                let sec = sec.clone();
                                move || sec.get() == "events"
                            },
                            apply_bound_sidebar_menu_button_active,
                        );
                        let title = match_when_reactive(TITLE, {
                            let sec = sec.clone();
                            move || {
                                let text = if sec.get() == "events" {
                                    "指针事件"
                                } else {
                                    "基础"
                                };
                                Element::new()
                                    .w("100%")
                                    .min_w(0.0)
                                    .text_size(20.0)
                                    .font_weight(700.0)
                                    .text(text)
                            }
                        });
                        let desc = match_when_reactive(DESC, {
                            let sec = sec.clone();
                            move || {
                                let text = if sec.get() == "events" {
                                    "crates/jadix/src/app/pointer_events.rs"
                                } else {
                                    "Button 变体 / 尺寸 / hover 缓动 · Badge"
                                };
                                Element::new()
                                    .w("100%")
                                    .min_w(0.0)
                                    .text_size(14.0)
                                    .text(text)
                            }
                        });
                        let body = match_when_reactive(BODY, {
                            let sec = sec.clone();
                            move || {
                                if sec.get() == "events" {
                                    page_scroll("scroll-events", Element::new().text("events-body"))
                                } else {
                                    page_scroll(
                                        "scroll-basics",
                                        Element::new().text("MessageScroller"),
                                    )
                                }
                            }
                        });
                        let header = Element::from(
                            vstack()
                                .w("100%")
                                .flex_shrink(0.0)
                                .min_w(0.0)
                                .gap(2.0)
                                .child(title)
                                .child(desc),
                        );
                        let page = Element::from(
                            vstack()
                                .w("100%")
                                .min_w(0.0)
                                .min_h(0.0)
                                .flex_grow(1.0)
                                .child(header)
                                .child(body),
                        );
                        let trigger = Element::from(
                            hstack()
                                .w("100%")
                                .p("0.5rem")
                                .flex_shrink(0.0)
                                .child(Element::new().w(28.0).h(28.0).bg([0.9, 0.9, 0.9, 1.0])),
                        );
                        let inset_children = {
                            let mut pack = Element::new();
                            pack.set_attr("jadix_fragment", "1");
                            pack.children(vec![trigger, page])
                        };
                        let mut frag = Element::new();
                        frag.set_attr("jadix_fragment", "1");
                        frag.children(vec![
                            sidebar("gal-sidebar")
                                .variant("inset")
                                .items(vec![basics_btn, events_btn])
                                .into(),
                            SidebarInset(SidebarInsetProps {
                                variant: Some("inset".into()),
                                children: Some(inset_children),
                                base: None,
                            }),
                        ])
                    }
                });
                *live.borrow_mut() = shell;
            }
        });
        section
    });

    let mut layout = LayoutTree::new();
    let (title_key, title_prev, events_bg) = {
        let mut root = live.borrow_mut();
        let first = layout.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
        let texts = collect_text_blobs(&first);
        assert!(
            texts.iter().any(|t| t.contains("指针事件")),
            "setup: events title must collect; texts={texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("events-body")),
            "setup: events body must collect; texts={texts:?}"
        );
        let events = find_tid(&root, "gal-nav-events").expect("gal-nav-events");
        let basics = find_tid(&root, "gal-nav-basics").expect("gal-nav-basics");
        assert_eq!(
            data_active(events),
            Some("true"),
            "setup: events nav active"
        );
        assert_eq!(data_active(basics), Some("false"), "setup: basics nav idle");
        let title = find_tid(&root, TITLE).expect("title island");
        let title_box = title.layout_bounds;
        let title_run = first
            .text_nodes
            .iter()
            .find(|n| n.text.contains("指针事件"))
            .expect("title glyphs");
        let glyph = [title_run.x, title_run.y, title_run.width, title_run.height];
        let prev = crate::app::damage::union_rects(title_box, glyph);
        (paint_element_key(title), prev, events.style.bg_color)
    };

    section.set("basics".to_string());

    {
        let mut root = live.borrow_mut();
        let inset_clip = find_inset(&root).layout_bounds;
        if let Some(d) = clipped_island_structure_damage(title_prev, Some(inset_clip)) {
            push_frame_paint_key(title_key, d);
        }
        let dirties = take_frame_paint_nodes();
        layout.set_collect_skip_allowed(true);
        let second = layout.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], true, true);
        let texts = collect_text_blobs(&second);
        let title_el = find_tid(&root, TITLE).expect("title island after switch");
        fn first_text(el: &Element) -> Option<&str> {
            if let Some(t) = el.text.as_deref() {
                if !t.is_empty() {
                    return Some(t);
                }
            }
            el.children.iter().find_map(first_text)
        }
        assert_eq!(
            first_text(title_el),
            Some("基础"),
            "title island must switch to 基础"
        );
        let events_title_runs = second
            .text_nodes
            .iter()
            .filter(|n| n.text.contains("指针事件"))
            .count();
        assert_eq!(
            events_title_runs,
            1,
            "指针事件 must remain only as the sidebar caption, not a leftover page title; texts={texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("基础")),
            "after switch, collect must paint 基础; texts={texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("MessageScroller")),
            "body must switch to basics; texts={texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("events-body")),
            "old events body must unmount; texts={texts:?}"
        );
        let events_desc_runs = second
            .text_nodes
            .iter()
            .filter(|n| n.text.contains("pointer_events.rs"))
            .count();
        assert_eq!(
            events_desc_runs, 0,
            "events description must not overlap the basics header; texts={texts:?}"
        );
        let events = find_tid(&root, "gal-nav-events").expect("gal-nav-events after");
        let basics = find_tid(&root, "gal-nav-basics").expect("gal-nav-basics after");
        assert_eq!(
            data_active(basics),
            Some("true"),
            "basics nav must become active"
        );
        assert_eq!(
            data_active(events),
            Some("false"),
            "events nav must leave active"
        );
        assert!(
            (0..4).any(|c| (events.style.bg_color[c] - events_bg[c]).abs() > 0.05)
                || events.style.bg_color[3] < events_bg[3] - 0.05,
            "events nav chrome must drop the active fill; was {events_bg:?} now {:?}",
            events.style.bg_color
        );

        let prev_slots = [(title_key, title_prev)];
        let curr_slots: Vec<(u64, [f32; 4])> = second
            .node_paints
            .iter()
            .filter(|p| !p.overlay)
            .map(|p| (p.key, p.bounds))
            .collect();
        let mut dest = dirties;
        expand_paint_dirties_for_translated_slots(&mut dest, &curr_slots, &prev_slots);
        let title_dirty = dest
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && d.key == title_key)
            .or_else(|| dest.iter().find(|d| d.kind == PaintDirtyKind::Node));
        if let Some(d) = title_dirty {
            let wipe = node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &prev_slots,
                &second.node_paints,
                8.0,
            );
            assert!(
                wipe[2] > 0.5 && wipe[3] > 0.5,
                "title island dest-reset must not collapse; wipe={wipe:?} dirty={:?}",
                d.bounds
            );
            let glyph_right = title_prev[0] + title_prev[2] - 4.0;
            let glyph_mid_y = title_prev[1] + title_prev[3] * 0.5;
            assert!(
                glyph_right >= wipe[0]
                    && glyph_right < wipe[0] + wipe[2]
                    && glyph_mid_y >= wipe[1]
                    && glyph_mid_y < wipe[1] + wipe[3],
                "dest-reset must cover leftover 指针事件 glyphs\n wipe={wipe:?} prev={title_prev:?}"
            );
        }
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn page_cards(labels: &[&str]) -> Element {
    let mut col = vstack().w("100%").min_w(0.0).gap(16.0);
    for label in labels {
        col = col.child(
            vstack()
                .w("100%")
                .min_w(0.0)
                .p(16.0)
                .border([0.8, 0.8, 0.8, 1.0], 1.0)
                .child(Element::new().text(*label)),
        );
    }
    col.into()
}

/// Live Gallery: 从「浮层 / 反馈」切到「数据」时，Match 岛复用 ScrollArea。
/// 以前 PartialLoad occupancy dest-reset / skip-replay 把 ViewportModal 卡
/// 叠在数据表上。
#[test]
fn match_when_overlays_to_data_must_drop_viewport_modal() {
    reset_registered_anchors();
    clear_test_reconcile_root();
    let _ = take_frame_paint_nodes();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    const TITLE: &str = "examples/gallery/App:section_title";
    const BODY: &str = "examples/gallery/App:section_body";
    const W: f32 = 1280.0;
    const H: f32 = 800.0;

    mark_next_effect_as_ui_root();
    let (_owner, section) = create_root(|| {
        let section = use_signal("gal_nav_section_overlays_data", || "overlays".to_string());
        create_effect({
            let section = section.clone();
            let live = Rc::clone(&live);
            move || {
                let sec = section.clone();
                let shell = sidebar_provider_scope("gal-ov-data", None, true, Some("inset"), {
                    let sec = sec.clone();
                    move || {
                        let overlays_btn = bind_prop(
                            sidebar_menu_button("gal-nav-overlays")
                                .text("浮层 / 反馈")
                                .into(),
                            "bind-gal-nav-overlays",
                            {
                                let sec = sec.clone();
                                move || sec.get() == "overlays"
                            },
                            apply_bound_sidebar_menu_button_active,
                        );
                        let data_btn = bind_prop(
                            sidebar_menu_button("gal-nav-data").text("数据").into(),
                            "bind-gal-nav-data",
                            {
                                let sec = sec.clone();
                                move || sec.get() == "data"
                            },
                            apply_bound_sidebar_menu_button_active,
                        );
                        let title = match_when_reactive(TITLE, {
                            let sec = sec.clone();
                            move || {
                                let text = if sec.get() == "overlays" {
                                    "浮层 / 反馈"
                                } else {
                                    "数据"
                                };
                                Element::new()
                                    .w("100%")
                                    .min_w(0.0)
                                    .text_size(20.0)
                                    .font_weight(700.0)
                                    .text(text)
                            }
                        });
                        let body = match_when_reactive(BODY, {
                            let sec = sec.clone();
                            move || {
                                if sec.get() == "overlays" {
                                    page_scroll(
                                        "scroll-overlays",
                                        page_cards(&[
                                            "ViewportModal 无头壳（居中 Dialog / 贴边 Sheet）",
                                            "打开 RSX ViewportModal",
                                            "YELLOW 对比 BLUR",
                                            "Point · 点定位 + 标题栏拖拽",
                                        ]),
                                    )
                                } else {
                                    page_scroll(
                                        "scroll-data",
                                        page_cards(&[
                                            "Name",
                                            "Role",
                                            "Score",
                                            "Page 1 / 4 · 40 rows",
                                        ]),
                                    )
                                }
                            }
                        });
                        let header = Element::from(
                            vstack()
                                .w("100%")
                                .flex_shrink(0.0)
                                .min_w(0.0)
                                .gap(2.0)
                                .child(title),
                        );
                        let page = Element::from(
                            vstack()
                                .w("100%")
                                .min_w(0.0)
                                .min_h(0.0)
                                .flex_grow(1.0)
                                .child(header)
                                .child(body),
                        );
                        let trigger = Element::from(
                            hstack()
                                .w("100%")
                                .p("0.5rem")
                                .flex_shrink(0.0)
                                .child(Element::new().w(28.0).h(28.0).bg([0.9, 0.9, 0.9, 1.0])),
                        );
                        let inset_children = {
                            let mut pack = Element::new();
                            pack.set_attr("jadix_fragment", "1");
                            pack.children(vec![trigger, page])
                        };
                        let mut frag = Element::new();
                        frag.set_attr("jadix_fragment", "1");
                        frag.children(vec![
                            sidebar("gal-sidebar")
                                .variant("inset")
                                .items(vec![overlays_btn, data_btn])
                                .into(),
                            SidebarInset(SidebarInsetProps {
                                variant: Some("inset".into()),
                                children: Some(inset_children),
                                base: None,
                            }),
                        ])
                    }
                });
                *live.borrow_mut() = shell;
            }
        });
        section
    });

    let mut layout = LayoutTree::new();
    let (body_key, body_prev, leftover_glyph) = {
        let mut root = live.borrow_mut();
        let first = layout.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
        let texts = collect_text_blobs(&first);
        assert!(
            texts.iter().any(|t| t.contains("ViewportModal")),
            "setup: overlays body must collect ViewportModal; texts={texts:?}"
        );
        let overlays = find_tid(&root, "gal-nav-overlays").expect("gal-nav-overlays");
        let data = find_tid(&root, "gal-nav-data").expect("gal-nav-data");
        assert_eq!(
            data_active(overlays),
            Some("true"),
            "setup: overlays nav active"
        );
        assert_eq!(data_active(data), Some("false"), "setup: data nav idle");
        let body = find_tid(&root, BODY).expect("body island");
        let run = first
            .text_nodes
            .iter()
            .find(|n| n.text.contains("ViewportModal"))
            .expect("ViewportModal glyphs");
        let glyph = [run.x, run.y, run.width, run.height];
        (paint_element_key(body), body.layout_bounds, glyph)
    };

    section.set("data".to_string());

    {
        let mut root = live.borrow_mut();
        let inset_clip = find_inset(&root).layout_bounds;
        if let Some(d) = clipped_island_structure_damage(body_prev, Some(inset_clip)) {
            push_frame_paint_key(body_key, d);
        }
        let dirties = take_frame_paint_nodes();
        layout.set_collect_skip_allowed(true);
        let second = layout.build_and_layout_fast(&mut root, W, H, [-1.0, -1.0], true, true);
        let texts = collect_text_blobs(&second);
        assert!(
            texts.iter().any(|t| t.contains("Name")),
            "after switch, collect must paint the data table; texts={texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("ViewportModal")),
            "overlays ViewportModal must unmount, not mix with the data page; texts={texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("点定位")),
            "overlays Point leftover must unmount; texts={texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("YELLOW")),
            "overlays YELLOW leftover must unmount; texts={texts:?}"
        );
        let overlays = find_tid(&root, "gal-nav-overlays").expect("gal-nav-overlays after");
        let data = find_tid(&root, "gal-nav-data").expect("gal-nav-data after");
        assert_eq!(
            data_active(data),
            Some("true"),
            "data nav must become active"
        );
        assert_eq!(
            data_active(overlays),
            Some("false"),
            "overlays nav must leave active"
        );

        let prev_slots = [(body_key, body_prev)];
        let curr_slots: Vec<(u64, [f32; 4])> = second
            .node_paints
            .iter()
            .filter(|p| !p.overlay)
            .map(|p| (p.key, p.bounds))
            .collect();
        let mut dest = dirties;
        expand_paint_dirties_for_translated_slots(&mut dest, &curr_slots, &prev_slots);
        let body_dirty = dest
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && d.key == body_key)
            .or_else(|| dest.iter().find(|d| d.kind == PaintDirtyKind::Node));
        if let Some(d) = body_dirty {
            let wipe = node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &prev_slots,
                &second.node_paints,
                8.0,
            );
            let gx = leftover_glyph[0] + leftover_glyph[2] * 0.5;
            let gy = leftover_glyph[1] + leftover_glyph[3] * 0.5;
            assert!(
                gx >= wipe[0]
                    && gx < wipe[0] + wipe[2]
                    && gy >= wipe[1]
                    && gy < wipe[1] + wipe[3],
                "dest-reset must cover leftover ViewportModal glyphs\n wipe={wipe:?} glyph={leftover_glyph:?} dirty={:?}",
                d.bounds
            );
        }
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}
