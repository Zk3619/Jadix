//! MessageScroller: `defaultScrollPosition=end` must paint the live edge, not an empty
//! tail from `min-h-full` / inflated `content_h`.

use crate::app::draw_cull::{
    clip_text_nodes_to_rect, padded_xywh, page_stack_band_submit_text, CullRects,
    DAMAGE_CULL_PAD_LOGICAL, SCROLL_LAYER_TEXT_PAD_LOGICAL,
};
use crate::app::reactive::{reactive_view_sized, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::bubble::bubble;
use crate::components::message::{message, message_content};
use crate::components::message_scroller::{
    message_scroller, message_scroller_button, message_scroller_content, message_scroller_item,
    message_scroller_provider, message_scroller_stick_to_end, message_scroller_viewport,
    sync_message_scroller_follow_after_layout,
};
use crate::components::scroll_area::scroll_area;
use crate::components::sidebar::{
    sidebar, sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_menu,
    sidebar_provider_scope, Sidebar, SidebarContent, SidebarContentProps, SidebarFooter,
    SidebarFooterProps, SidebarHeader, SidebarHeaderProps, SidebarInset, SidebarInsetProps,
    SidebarMenuButton, SidebarMenuButtonProps, SidebarProps, SidebarSeparator,
    SidebarSeparatorProps,
};
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_y_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::{sync_reactive_elements_subtree, Element, LayoutStatus, LayoutTree};
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use std::cell::RefCell;
use std::rc::Rc;

const GAL_MS_TEXTS: &[&str] = &[
    "I'm building a chat for our app and the scroll behavior is driving me nuts. Every time the AI streams a reply, the whole thread jumps around.",
    "That's the classic streaming scroll problem. Wrap your message list in MessageScroller and turn on autoScroll — the viewport pins to the bottom as tokens arrive, so users always see the latest text land in place.\n\nThe important part: it only auto-scrolls while the reader is already at the bottom.",
    "Okay, but when someone sends a new message the view still feels jarring — like the whole conversation reloads from the top.",
    "MessageScrollerItem fixes that with turn anchoring. Set scrollAnchor on the turn that should settle near the top instead of blindly snapping to the document bottom.",
    "And if they've scrolled up to re-read an older answer? I don't want to yank them back down.",
    "You won't. Auto-scroll only runs when the viewport is already pinned to the bottom, so scrolling up is a deliberate opt-out.",
    "Last one — does this work with assistive tech?",
    "MessageScrollerContent sets role=log and aria-relevant=additions by default, so screen readers announce new messages as they stream in.",
];

fn tall_message_scroller_root() -> Element {
    let mut content = message_scroller_content();
    for i in 0..8 {
        content =
            content.child(message_scroller_item().message_id(format!("m{i}")).child(
                Element::new().flex_col().w("100%").min_w(0.0).child(
                    Element::new().w("100%").text(format!(
                    "Turn {i}: enough text to wrap across multiple lines in a narrow chat column."
                )),
                ),
            ));
    }
    let scroller: Element = message_scroller_provider()
        .id("ms-layout-test")
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller()
                .h("100%")
                .child(message_scroller_viewport().child(content))
                .child(message_scroller_button().direction("end")),
        )
        .into();
    Element::new()
        .w(320.0)
        .h(384.0)
        .overflow("hidden")
        .child(scroller)
}

fn gallery_like_message_scroller_with_jump() -> Element {
    let mut content = message_scroller_content();
    for (i, text) in GAL_MS_TEXTS.iter().enumerate() {
        content = content.child(bubble_turn(i, text));
    }
    let scroller: Element = message_scroller_provider()
        .id("ms-gallery-jump")
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller()
                .h("100%")
                .child(message_scroller_viewport().child(content))
                .child(message_scroller_button().direction("end")),
        )
        .into();
    let page = Element::new().w(400.0).h(900.0).flex_col().child(
        Element::new()
            .w(384.0)
            .h(384.0)
            .overflow("hidden")
            .child(scroller),
    );
    Element::from(
        scroll_area("outer-jump")
            .w(400.0)
            .h(800.0)
            .overflow_y("auto")
            .child(page),
    )
}

fn gallery_like_message_scroller_root() -> Element {
    let mut content = message_scroller_content();
    for (i, text) in GAL_MS_TEXTS.iter().enumerate() {
        let user = i % 2 == 0;
        let body = Element::new().text((*text).to_string());
        let bubble_child = if user {
            message_scroller_item()
                .message_id(format!("m{i}"))
                .scroll_anchor(true)
                .child(message().align("end").child(
                    message_content().child(bubble().variant("muted").align("end").child(body)),
                ))
        } else {
            message_scroller_item().message_id(format!("m{i}")).child(
                message().align("start").child(
                    message_content().child(bubble().variant("ghost").align("start").child(body)),
                ),
            )
        };
        content = content.child(bubble_child);
    }
    let scroller: Element = message_scroller_provider()
        .id("ms-gallery-like")
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller()
                .h("100%")
                .child(message_scroller_viewport().child(content)),
        )
        .into();
    let page = Element::new()
        .w(400.0)
        .h(900.0)
        .flex_col()
        .child(
            Element::new()
                .h(400.0)
                .w("100%")
                .bg([0.95, 0.95, 0.95, 1.0]),
        )
        .child(
            Element::new()
                .w(400.0)
                .h(384.0)
                .overflow("hidden")
                .child(scroller),
        );
    Element::from(
        scroll_area("outer-basics")
            .w(400.0)
            .h(800.0)
            .overflow_y("auto")
            .child(page),
    )
}

fn run_gallery_like_scroll_only() -> (f32, f32, f32, usize, Vec<f32>) {
    let mut root = gallery_like_message_scroller_root();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
    // Simulate page scrolled so the chat card is in view.
    use crate::core::scroll_viewport::scroll_offset_y_signal_key as outer_sy;
    use_signal(&outer_sy("outer-basics"), || 0.0_f32).set_silent(200.0);
    for _ in 0..3 {
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 800.0, [0.0, 0.0]);
        let _ = tree.build_and_layout_fast(&mut root, 400.0, 800.0, [-1.0, -1.0], true, false);
    }
    let result = tree.build_and_layout_fast(&mut root, 400.0, 800.0, [-1.0, -1.0], true, false);
    let sy = use_signal(
        &scroll_offset_y_signal_key("ms-gallery-like:viewport"),
        || 0.0_f32,
    )
    .peek();
    let metrics = result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "ms-gallery-like:viewport")
        .map(|(_, _, max, ch)| (max[1], ch[1]))
        .unwrap_or((0.0, 0.0));
    let ys: Vec<f32> = result.text_nodes.iter().map(|t| t.y).collect();
    (sy, metrics.0, metrics.1, result.text_nodes.len(), ys)
}

fn bubble_turn(i: usize, text: &str) -> Element {
    let user = i % 2 == 0;
    let body = Element::new().text(text.to_string());
    if user {
        message_scroller_item()
            .message_id(format!("m{i}"))
            .scroll_anchor(true)
            .child(
                message().align("end").child(
                    message_content().child(bubble().variant("muted").align("end").child(body)),
                ),
            )
            .into()
    } else {
        message_scroller_item()
            .message_id(format!("m{i}"))
            .child(message().align("start").child(
                message_content().child(bubble().variant("ghost").align("start").child(body)),
            ))
            .into()
    }
}

/// Gallery shape: outer ScrollArea + 24rem chat box + reactive island wrapping turns.
fn gallery_reactive_island_nested_root(rows: Element) -> Element {
    let scroller: Element = message_scroller_provider()
        .id("ms-gal-island")
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller()
                .h("100%")
                .child(message_scroller_viewport().child(message_scroller_content().child(rows))),
        )
        .into();
    let page = Element::new()
        .w(400.0)
        .h(900.0)
        .flex_col()
        .child(
            Element::new()
                .h(400.0)
                .w("100%")
                .bg([0.95, 0.95, 0.95, 1.0]),
        )
        .child(
            Element::new()
                .w(400.0)
                .h(384.0)
                .overflow("hidden")
                .child(scroller),
        );
    Element::from(
        scroll_area("outer-gal-island")
            .w(400.0)
            .h(800.0)
            .overflow_y("auto")
            .child(page),
    )
}

#[test]
fn message_scroller_end_scroll_paints_visible_rows() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            let mut last_sy = 0.0_f32;

            let mut last_result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            for frame in 0..6 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                last_result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                let result = &last_result;
                last_sy = crate::core::reactivity::use_signal(
                    &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                    || 0.0_f32,
                )
                .peek();

                let metrics = result
                    .scroll_metrics
                    .iter()
                    .find(|(id, ..)| id == "ms-layout-test:viewport");
                if let Some((_, bounds, max_scroll, content_size)) = metrics {
                    assert!(
                        content_size[1] > bounds[3] + 8.0,
                        "frame {frame}: content should overflow viewport (ch={} vh={})",
                        content_size[1],
                        bounds[3]
                    );
                    if frame >= 2 {
                        assert!(
                            max_scroll[1] > 8.0,
                            "frame {frame}: max_scroll_y should be positive"
                        );
                        assert!(
                            last_sy >= max_scroll[1] - 16.0,
                            "frame {frame}: scroll should anchor near end (sy={} max_y={})",
                            last_sy,
                            max_scroll[1]
                        );
                    }
                }
            }

            assert!(
                last_result.text_nodes.len() >= 3,
                "end-anchored scroller must paint message rows (got {} text, sy={last_sy})",
                last_result.text_nodes.len()
            );
        });
    });
}

#[test]
fn message_scroller_scroll_only_band_cull_keeps_end_edge_visible() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let result =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, false);
            let sy = crate::core::reactivity::use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek();
            let visible: Vec<f32> = result
                .text_nodes
                .iter()
                .filter(|t| t.y >= -1.0 && t.y + t.height <= 385.0)
                .map(|t| t.y)
                .collect();
            let all_y: Vec<f32> = result.text_nodes.iter().map(|t| t.y).collect();
            assert!(
                !visible.is_empty(),
                "scroll_only collect must keep end-edge text visible (sy={sy}, text={}, visible_y={visible:?}, all_y={all_y:?})",
                result.text_nodes.len(),
            );
        });
    });
}

#[test]
fn gallery_like_nested_scroll_only_keeps_end_edge_visible() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let (sy, max_y, ch, text_n, ys) = run_gallery_like_scroll_only();
            let visible: Vec<f32> = ys
                .iter()
                .copied()
                .filter(|y| *y >= -1.0 && *y <= 800.0)
                .collect();
            assert!(
                !visible.is_empty(),
                "gallery-like nested scroll_only must paint end-edge text (sy={sy} max_y={max_y} ch={ch} text={text_n} ys={ys:?} visible={visible:?})"
            );
            assert!(
                max_y > 8.0,
                "outer scroll_only must not publish inner max_scroll=0 (sy={sy} max_y={max_y} ch={ch})"
            );
            assert!(
                sy > 8.0,
                "outer scroll_only must not clamp inner offset to 0 (sy={sy} max_y={max_y})"
            );
        });
    });
}

#[test]
fn message_scroller_jump_button_layers_above_message_glyphs() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let scroller = find_slot(&root, "message-scroller").expect("scroller");
            let b = btn.layout_bounds;
            let s = scroller.layout_bounds;
            let visual_x = b[0] + b[2] * btn.style.translate_pct[0];
            let visual_y = b[1] + b[3] * btn.style.translate_pct[1];
            assert!(
                btn.style.opacity > 0.5,
                "not-at-end must show the jump button, opacity={}",
                btn.style.opacity
            );
            assert!(
                btn.style.translate[1].abs() < 0.5,
                "shown Latest must rest at translateY(0), got {:?}",
                btn.style.translate
            );
            assert!(
                btn.style
                    .transition
                    .iter()
                    .any(|e| e.property == crate::style::TransitionProperty::Transform),
                "Latest must CSS-transition transform for slide enter/exit, got {:?}",
                btn.style.transition
            );
            assert!(
                btn.style.translate_pct[0].abs() < 0.02,
                "centering must live on the dock, not translateX; translate_pct={:?}",
                btn.style.translate_pct
            );
            assert!(
                b[2] > 24.0 && b[2] < s[2] * 0.7,
                "jump button must hug the Latest label, not stretch/clip; bounds={b:?} scroller={s:?}"
            );
            let btn_cx = visual_x + b[2] * 0.5;
            let scroller_cx = s[0] + s[2] * 0.5;
            assert!(
                visual_x > s[0] + 8.0 && visual_x + b[2] < s[0] + s[2] - 8.0,
                "jump button must stay fully inside the scroller; \
                 visual=({visual_x:.1},{visual_y:.1}) size={}x{} scroller={s:?}",
                b[2],
                b[3]
            );
            assert!(
                (btn_cx - scroller_cx).abs() < s[2] * 0.2,
                "jump button must sit near the scroller center, not the left clip; \
                 btn_cx={btn_cx:.1} scroller_cx={scroller_cx:.1} scroller={s:?}"
            );
            let latest = result
                .text_nodes
                .iter()
                .find(|t| t.text == "Latest")
                .expect("Latest label");
            assert!(
                latest.x > s[0] + 16.0,
                "Latest glyph must not sit on the left clip edge; x={} scroller={s:?}",
                latest.x
            );
            assert!(
                btn.focusable,
                "visible jump button must stay in the tab order"
            );
            let ranges = result.overlay_split.page_stack_band_ranges();
            assert!(
                ranges.len() >= 2,
                "root stacking-context must split viewport vs jump button, got {}",
                ranges.len()
            );
            let last = ranges.last().unwrap();
            assert!(
                result.text_nodes[last.text.0..last.text.1]
                    .iter()
                    .any(|t| t.text == "Latest"),
                "jump label must paint in the last band, above message glyphs"
            );
            let earlier_has_latest = ranges[..ranges.len() - 1].iter().any(|r| {
                result.text_nodes[r.text.0..r.text.1]
                    .iter()
                    .any(|t| t.text == "Latest")
            });
            assert!(
                !earlier_has_latest,
                "jump label must not share the message band"
            );
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                result
                    .node_paints
                    .iter()
                    .any(|p| p.key == btn_key && p.layered),
                "jump button collect slot must be a later page-stack band"
            );
        });
    });
}

#[test]
fn message_scroller_jump_slot_survives_fingerprint_skip() {
    // Absolute dock skip used to record one parent NodePaint. last-present then
    // dropped the button key; hide+scroll blit pinned only the slid box and
    // copied Latest into the overlap.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            tree.set_collect_skip_allowed(true);
            let _ = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], false, false);
            let skipped =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], false, false);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                skipped
                    .node_paints
                    .iter()
                    .any(|p| p.key == btn_key && p.layered),
                "fingerprint skip must keep the jump button dest-reset slot"
            );
            let prev = crate::app::draw_cull::last_present_visual_slots(
                &skipped.node_paints,
                &skipped.instances,
                &[],
                &skipped.text_nodes,
                &[],
            );
            assert!(
                prev.iter().any(|(k, _)| *k == btn_key),
                "last-present must keep the button key across skip frames"
            );
        });
    });
}

#[test]
fn message_scroller_jump_button_hides_latest_glyphs_at_live_edge() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            assert!(
                shown.text_nodes.iter().any(|t| t.text == "Latest"),
                "scrolled-away must emit Latest"
            );

            let max_y = use_signal(
                &scroll_content_h_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-layout-test:viewport"),
                    || 0.0_f32,
                )
                .peek();
            crate::app::damage::clear_frame_paint_nodes();
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y.max(0.0));
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let dirties = crate::app::damage::take_frame_paint_nodes();
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity < 0.05,
                "at live edge the jump button must hide, opacity={}",
                btn.style.opacity
            );
            assert!(
                btn.style.translate[1] > 4.0,
                "hide must slide down (enter was up); translate={:?}",
                btn.style.translate
            );
            assert!(
                btn.style.translate_pct[1].abs() < 0.02,
                "Y-slide is pixel translate, not translate_pct; {:?}",
                btn.style.translate_pct
            );
            assert!(
                dirties
                    .iter()
                    .any(|d| d.kind == crate::app::damage::PaintDirtyKind::Node),
                "StyleChanged hide must dest-reset the pill as a Node, got {dirties:?}"
            );
            assert!(
                dirties
                    .iter()
                    .all(|d| d.kind != crate::app::damage::PaintDirtyKind::OverlayRestore),
                "in-page jump chrome is not a portal OverlayRestore, got {dirties:?}"
            );
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "at live edge Latest glyphs must leave collect (glyphon retain leftover)"
            );
        });
    });
}

#[test]
fn message_scroller_latest_follow_pass_must_not_clobber_css_enter_mid() {
    // Production: reactive_sync writes the shown target, CSS ticks mid, then
    // `sync_message_scroller_follow_after_layout` re-runs apply_button_active.
    // Writing opacity 1 over that mid painted the end pose every frame.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
                assert!(
                    btn.style.opacity < 0.05,
                    "seed from hidden, opacity={}",
                    btn.style.opacity
                );
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(80.0);
            let mid = {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
                btn.style.opacity
            };
            assert!(
                mid > 0.08 && mid < 0.92,
                "enter must be mid-tween before follow, opacity={mid}"
            );
            let ty_mid = find_slot(&root, "message-scroller-button")
                .expect("jump button")
                .style
                .translate[1];
            assert!(
                ty_mid > 0.3 && ty_mid < 7.7,
                "enter must slide up through a mid translateY, got {ty_mid}"
            );
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                (btn.style.opacity - mid).abs() < 0.02,
                "follow pass must not clobber CSS enter mid {mid}, got {}",
                btn.style.opacity
            );
            assert!(
                (btn.style.translate[1] - ty_mid).abs() < 0.2,
                "follow pass must keep slide mid {ty_mid}, got {:?}",
                btn.style.translate
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn message_scroller_latest_css_hide_must_settle_hidden_at_live_edge() {
    // Production: hide writes opacity 0, CSS ticks `from` (~1) at t=0, follow
    // must not snap that back to the shown pose. After duration, Latest is gone.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(200.0);
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
                assert!(
                    btn.style.opacity > 0.9,
                    "precondition: enter settled shown, opacity={}",
                    btn.style.opacity
                );
            }
            let max_y = use_signal(
                &scroll_content_h_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-layout-test:viewport"),
                    || 0.0_f32,
                )
                .peek();
            crate::style::set_css_transition_timeline_ms(200.0);
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y.max(0.0));
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let mid = find_slot(&root, "message-scroller-button")
                .expect("jump button")
                .style
                .opacity;
            assert!(
                mid > 0.08,
                "hide must start from the shown visual, opacity={mid}"
            );
            crate::style::set_css_transition_timeline_ms(400.0);
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity < 0.05,
                "after hide duration Latest must settle hidden, opacity={}",
                btn.style.opacity
            );
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "live-edge collect must drop Latest glyphs after CSS hide"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn message_scroller_jump_button_hides_after_follow_without_second_sync() {
    // Production: pre-layout reactive_sync walks the dock before the viewport, then
    // `sync_message_scroller_follow_after_layout` snaps offset to end. Without a
    // second button sync, Latest stays opacity 1 on the live-edge frame.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            assert!(
                shown.text_nodes.iter().any(|t| t.text == "Latest"),
                "scrolled-away must emit Latest"
            );
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.5,
                "precondition: Latest visible, opacity={}",
                btn.style.opacity
            );

            let max_y = use_signal(
                &scroll_content_h_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-layout-test:viewport"),
                    || 0.0_f32,
                )
                .peek();
            crate::app::damage::clear_frame_paint_nodes();
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y.max(0.0));
            // `finish_collect` runs follow then collect — no extra pre-layout reactive_sync.
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity < 0.05,
                "follow-after-layout must hide Latest at the live edge, opacity={}",
                btn.style.opacity
            );
            let dirties = crate::app::damage::take_frame_paint_nodes();
            assert!(
                dirties
                    .iter()
                    .any(|d| d.kind == crate::app::damage::PaintDirtyKind::Node),
                "hide after follow must dest-reset the pill, got {dirties:?}"
            );
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "live-edge collect must drop Latest glyphs, leftover retain"
            );
        });
    });
}

#[test]
fn message_scroller_latest_must_collect_on_scroll_only_leave_live_edge() {
    // Wheel/smooth-scroll is `scroll_only` + `reuse_idle` hash. The jump dock is a
    // sibling of the viewport, so it was treated as an idle wrapper and skip-replayed
    // the opacity-0 collect. Latest stayed invisible until hover busted the hash.
    //
    // Enter is a CSS tween: follow writes the shown target, then
    // `resync_pointer_hover_before_collect` ticks at t=0 (opacity ≈ 0). The
    // skip-replay bug is that later ticks still replay that empty collect.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity < 0.05,
                "precondition: live edge hides Latest, opacity={}",
                btn.style.opacity
            );
            tree.set_collect_skip_allowed(true);
            let hidden =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "precondition: hidden collect must not emit Latest glyphs"
            );

            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            let _primed =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                assert_eq!(
                    btn.attrs.get("data-active").map(String::as_str),
                    Some("true"),
                    "follow-after-layout must pose Latest shown after scrolling away"
                );
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(80.0);
            {
                let btn = find_slot_ms_mut(&mut root, "message-scroller-button")
                    .expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
                assert!(
                    btn.style.opacity > 0.3 && btn.style.opacity < 0.95,
                    "enter must be mid-tween before collect, opacity={}",
                    btn.style.opacity
                );
            }
            let shown =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.3 && btn.style.opacity < 0.95,
                "follow/collect must not clobber CSS enter mid, opacity={}",
                btn.style.opacity
            );
            assert!(
                shown.text_nodes.iter().any(|t| t.text == "Latest"),
                "scroll_only collect must not skip-replay opacity-0 Latest; got {} text runs",
                shown.text_nodes.len()
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

fn find_parent_of_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el
        .children
        .iter()
        .any(|c| c.attrs.get("data-slot").map(String::as_str) == Some(slot))
    {
        return Some(el);
    }
    el.children
        .iter()
        .find_map(|c| find_parent_of_slot(c, slot))
}

/// Hover dest-reset of Latest used to punch wrapping message glyphs: the full-width
/// dock last-present, or AABB-intersect recapture of a whole paragraph, dest-reset
/// a hole in the thread that PartialLoad never rebuilt.
#[test]
fn message_scroller_latest_hover_must_not_dirty_message_list() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let dock = find_parent_of_slot(&root, "message-scroller-button").expect("jump dock");
            let scroller = find_slot(&root, "message-scroller").expect("scroller");
            assert!(
                btn.style.opacity > 0.5,
                "not-at-end must show Latest, opacity={}",
                btn.style.opacity
            );
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            let btn_vis = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let dock_vis = crate::core::tree::visual_paint_aabb(dock, dock.paint_scroll_acc);
            assert!(
                dock_vis[2] > btn_vis[2] + 40.0,
                "precondition: dock is the full-width bar, dock={dock_vis:?} pill={btn_vis:?}"
            );

            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let dirties = crate::app::damage::take_frame_paint_nodes();
            assert_eq!(
                dirties.len(),
                1,
                "hover must dest-reset only the pill, got {dirties:?}"
            );
            let dirty = dirties[0];
            assert_eq!(dirty.key, btn_key);
            assert_eq!(dirty.kind, crate::app::damage::PaintDirtyKind::Node);
            assert!(
                dirty.bounds[2] < scroller.layout_bounds[2] * 0.7,
                "hover dirty must stay on the pill, not the 100% dock\n dirty={:?} dock={dock_vis:?} scroller={:?}",
                dirty.bounds,
                scroller.layout_bounds
            );

            let overlapping: Vec<(usize, &crate::layout::layout_tree::TextLayoutInfo)> = result
                .text_nodes
                .iter()
                .enumerate()
                .filter(|(_, t)| t.text.contains("Turn "))
                .filter(|(_, t)| crate::app::draw_cull::text_hits_damage(t, dirty.bounds))
                .collect();
            assert!(
                !overlapping.is_empty(),
                "precondition: wrapping messages must sit under the Latest hole, dirty={:?}",
                dirty.bounds
            );
            let late = crate::app::draw_cull::node_present_capture_text_indices(
                &[dirty],
                &result.text_nodes,
                &result.node_paints,
            );
            let leaked: Vec<&str> = overlapping
                .iter()
                .filter(|(i, _)| late.contains(i))
                .map(|(_, t)| t.text.as_str())
                .collect();
            assert!(
                leaked.is_empty(),
                "post-overlay capture must not stamp wrapping messages on Latest, leaked={leaked:?}"
            );
            let underlay = crate::app::draw_cull::node_present_layered_underlay_capture_text(
                &[dirty],
                &result.text_nodes,
                &result.node_paints,
            );
            let hole = crate::app::draw_cull::padded_xywh(
                btn.layout_bounds,
                crate::app::draw_cull::NODE_PRESENT_GLYPH_CLIP_PAD,
            );
            let behind_pill: Vec<_> = overlapping
                .iter()
                .filter(|(_, t)| crate::app::draw_cull::text_hits_damage(t, btn.layout_bounds))
                .collect();
            if !behind_pill.is_empty() {
                assert!(
                    !underlay.is_empty(),
                    "dest-reset of the opaque pill must restore wrapping glyphs behind it"
                );
            }
            for n in &underlay {
                let clip = n
                    .clip_bounds
                    .expect("underlay recapture must clip wrapping messages to the pill");
                assert!(
                    clip[2] <= hole[2] + 1.0 && clip[3] <= hole[3] + 1.0,
                    "wrapping message restore must stay in the opaque pill, not the shadow pad\n clip={clip:?} hole={hole:?} run=({},{},{},{})",
                    n.x,
                    n.y,
                    n.width,
                    n.height
                );
                assert!(
                    clip[1] + clip[3] <= hole[1] + hole[3] + 1.0,
                    "underlay recapture must not include wrapping lines below Latest\n clip={clip:?} pill={hole:?}"
                );
            }
            if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                &result.node_paints,
                &result.instances,
                &[],
                1.0,
                Some(dirty.bounds),
            ) {
                let pill_bottom = btn.layout_bounds[1] + btn.layout_bounds[3];
                assert!(
                    reset.rect[1] + reset.rect[3] <= pill_bottom + 3.0,
                    "Latest dest-reset must not punch wrapping text below the pill\n wipe={:?} pill={:?}",
                    reset.rect,
                    btn.layout_bounds
                );
                let pill = btn.layout_bounds;
                assert!(
                    reset.rect[2] <= pill[2] + 6.0 && reset.rect[3] <= pill[3] + 6.0,
                    "hover dest-reset must stay on the rounded fill, not a shadow AABB\n wipe={:?} pill={pill:?}",
                    reset.rect
                );
            }
        });
    });
}

#[test]
fn message_scroller_latest_dest_reset_must_not_punch_overlay_scrollbar() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.5,
                "not-at-end must show Latest, opacity={}",
                btn.style.opacity
            );
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            let bar = find_overlay_scrollbar(&root).expect("overlay auto scrollbar");
            let bar_box = bar.layout_bounds;
            assert!(
                bar_box[2] > 0.0 && bar_box[2] < 24.0,
                "precondition: overlay rail is a thin strip, got {bar_box:?}"
            );

            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let dirties = crate::app::damage::take_frame_paint_nodes();
            let dirty = dirties
                .iter()
                .find(|d| d.key == btn_key)
                .copied()
                .expect("Latest dest-reset");
            let prev = crate::app::draw_cull::last_present_visual_slots(
                &result.node_paints,
                &result.instances,
                &[],
                &result.text_nodes,
                &[],
            );
            let scissor = crate::app::draw_cull::node_present_scissor_logical(
                dirty.kind,
                dirty.key,
                dirty.bounds,
                &prev,
                &result.node_paints,
                8.0,
            );
            assert!(
                crate::app::damage::intersect_rects(scissor, bar_box).is_none(),
                "Latest scissor must not include the overlay scrollbar\n scissor={scissor:?} bar={bar_box:?} dirty={:?}",
                dirty.bounds
            );
            if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                &result.node_paints,
                &result.instances,
                &[],
                1.0,
                prev.iter().find(|(k, _)| *k == btn_key).map(|(_, b)| *b),
            ) {
                assert!(
                    crate::app::damage::intersect_rects(reset.rect, bar_box).is_none(),
                    "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar_box:?}",
                    reset.rect
                );
            }
            let vp = find_slot(&root, "message-scroller-viewport")
                .expect("viewport")
                .layout_bounds;
            let pins = crate::app::damage::message_scroller_jump_scroll_blit_pins(&root, &[vp]);
            assert!(
                pins.iter().any(|p| p[2] > 0.0 && p[3] > 0.0),
                "active Latest must pin every scroll blit, got {pins:?}"
            );
            assert!(
                pins.iter()
                    .all(|p| crate::app::damage::intersect_rects(*p, bar_box).is_none()),
                "Latest scroll-blit pin must stay off the overlay rail, pins={pins:?} bar={bar_box:?}"
            );
        });
    });
}

fn find_overlay_scrollbar(el: &Element) -> Option<&Element> {
    if el.attrs.get("rui_scrollbar_overlay").map(String::as_str) == Some("1") {
        return Some(el);
    }
    el.children.iter().find_map(find_overlay_scrollbar)
}

#[test]
fn message_scroller_latest_show_dest_reset_stays_on_the_pill() {
    // Production hide→show unions last-present (full-width dock / 48px clamp /
    // 8px node scissor) into a band that dest-resets wrapping text and the
    // overlay rail. Tests that only `push_frame_paint_element(btn)` miss this.
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            let mut hidden_present = Vec::new();
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    320.0,
                    384.0,
                    [0.0, 0.0],
                );
                let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                hidden_present = crate::app::draw_cull::last_present_visual_slots(
                    &hidden.node_paints,
                    &hidden.instances,
                    &[],
                    &hidden.text_nodes,
                    &[],
                );
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let dock = find_parent_of_slot(&root, "message-scroller-button").expect("jump dock");
            let scroller = find_slot(&root, "message-scroller").expect("scroller");
            assert!(
                btn.style.opacity > 0.5,
                "not-at-end must show Latest, opacity={}",
                btn.style.opacity
            );
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            let dock_key = u64::from(dock.layout_node.expect("dock taffy"));
            let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let bar = find_overlay_scrollbar(&root).expect("overlay auto scrollbar");
            let bar_box = bar.layout_bounds;
            let shown_present = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            );
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let mut dirties = crate::app::damage::take_frame_paint_nodes();
            crate::app::damage::expand_paint_dirties_for_translated_slots(
                &mut dirties,
                &shown_present,
                &hidden_present,
            );
            let dock_paint = shown.node_paints.iter().find(|p| p.key == dock_key);
            let btn_paint = shown.node_paints.iter().find(|p| p.key == btn_key);
            let dump = format!(
                "pill={pill:?} bar={bar_box:?} scroller={:?} dock_vis={:?}\n hidden_btn={:?} shown_btn={:?} hidden_dock={:?} shown_dock={:?}\n dock_paint={dock_paint:?}\n btn_paint={btn_paint:?}\n dirties={dirties:?}",
                scroller.layout_bounds,
                crate::core::tree::visual_paint_aabb(dock, dock.paint_scroll_acc),
                hidden_present.iter().find(|(k, _)| *k == btn_key).map(|(_, b)| *b),
                shown_present.iter().find(|(k, _)| *k == btn_key).map(|(_, b)| *b),
                hidden_present.iter().find(|(k, _)| *k == dock_key).map(|(_, b)| *b),
                shown_present.iter().find(|(k, _)| *k == dock_key).map(|(_, b)| *b),
            );
            let hidden_dock = hidden_present
                .iter()
                .find(|(k, _)| *k == dock_key)
                .map(|(_, b)| *b)
                .expect("dock last-present while hidden");
            let shown_dock = shown_present
                .iter()
                .find(|(k, _)| *k == dock_key)
                .map(|(_, b)| *b)
                .expect("dock last-present while shown");
            assert!(
                hidden_dock[2] > pill[2] + 40.0 && shown_dock[2] > pill[2] + 40.0,
                "precondition: dock last-present is the full-width bar\n{dump}"
            );
            assert!(
                hidden_dock[3] <= pill[3] + 8.0 && shown_dock[3] <= pill[3] + 8.0,
                "dock last-present must not union the pill shadow/scale\n{dump}"
            );
            assert!(
                !crate::app::damage::node_present_slot_moved(hidden_dock, shown_dock),
                "Latest show must not look like a translated dock\n{dump}"
            );
            assert!(
                !dirties.iter().any(|d| d.key == dock_key),
                "full-width dock must not dest-reset on Latest show\n{dump}"
            );
            let max_w = pill[2] + 24.0;
            let max_bottom = pill[1] + pill[3] + 16.0;
            let jump_dirties: Vec<_> = dirties
                .iter()
                .filter(|d| d.key == btn_key || d.key == dock_key)
                .cloned()
                .collect();
            assert!(
                !jump_dirties.is_empty(),
                "Latest show must dest-reset the pill\n{dump}"
            );
            for d in &jump_dirties {
                let scissor = crate::app::draw_cull::node_present_scissor_logical(
                    d.kind,
                    d.key,
                    d.bounds,
                    &hidden_present,
                    &shown.node_paints,
                    crate::app::draw_cull::node_present_scissor_pad(d.kind),
                );
                let reset = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &shown.node_paints,
                    &shown.instances,
                    &[],
                    1.0,
                    hidden_present
                        .iter()
                        .find(|(k, _)| *k == d.key)
                        .map(|(_, b)| *b),
                );
                let reset_rect = reset.as_ref().map(|r| r.rect);
                assert!(
                    scissor[2] <= max_w,
                    "Latest dest-reset/scissor must stay on the pill, not a wrapping band\n scissor={scissor:?} dirty={:?} reset={reset_rect:?}\n{dump}",
                    d.bounds
                );
                assert!(
                    crate::app::damage::intersect_rects(scissor, bar_box).is_none(),
                    "scissor must not punch the overlay scrollbar\n scissor={scissor:?}\n{dump}"
                );
                if let Some(wipe) = reset_rect {
                    assert!(
                        wipe[2] <= max_w,
                        "dest-reset wipe too wide\n wipe={wipe:?}\n{dump}"
                    );
                    assert!(
                        wipe[1] + wipe[3] <= max_bottom,
                        "dest-reset must not punch wrapping text below the pill\n wipe={wipe:?} pill={pill:?}\n{dump}"
                    );
                    assert!(
                        crate::app::damage::intersect_rects(wipe, bar_box).is_none(),
                        "dest-reset must not punch the overlay scrollbar\n wipe={wipe:?}\n{dump}"
                    );
                }
            }
        });
    });
}

#[test]
fn gallery_nested_jump_button_stays_inside_chat_clip() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_like_message_scroller_with_jump();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    400.0,
                    800.0,
                    [0.0, 0.0],
                );
                let _ = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-gallery-jump:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                400.0,
                800.0,
                [0.0, 0.0],
            );
            let result = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let scroller = find_slot(&root, "message-scroller").expect("scroller");
            let b = btn.layout_bounds;
            let s = scroller.layout_bounds;
            let visual_x = b[0] + b[2] * btn.style.translate_pct[0];
            assert!(
                btn.style.opacity > 0.5,
                "scrolled-away-from-end must show Latest, opacity={}",
                btn.style.opacity
            );
            assert!(
                btn.style.translate_pct[0].abs() < 0.02,
                "centering must live on the dock, not translateX; translate_pct={:?}",
                btn.style.translate_pct
            );
            let latest = result
                .text_nodes
                .iter()
                .find(|t| t.text == "Latest")
                .expect("Latest label");
            let btn_cx = visual_x + b[2] * 0.5;
            let scroller_cx = s[0] + s[2] * 0.5;
            assert!(
                visual_x > s[0] + 4.0 && visual_x + b[2] < s[0] + s[2] - 4.0,
                "Latest chrome must not clip on the chat's left edge; \
                 visual_x={visual_x:.1} btn={b:?} scroller={s:?} label=({:.1},{:.1}) clip={:?}",
                latest.x,
                latest.y,
                latest.clip_bounds
            );
            assert!(
                latest.x > s[0] + 12.0,
                "Latest glyph must sit inside the chat, not the left clip; x={} clip={:?} scroller={s:?}",
                latest.x,
                latest.clip_bounds
            );
            assert!(
                (btn_cx - scroller_cx).abs() < s[2] * 0.2,
                "Latest pill must sit near the chat center; \
                 btn_cx={btn_cx:.1} scroller_cx={scroller_cx:.1} label_x={:.1} scroller={s:?}",
                latest.x
            );
        });
    });
}

#[test]
fn gallery_like_outer_scroll_only_keeps_inner_wheel_target() {
    use crate::app::scroll_support::collect_scroll_rest_chain_at;

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_like_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            let _ = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
            use crate::core::scroll_viewport::scroll_offset_y_signal_key as outer_sy;
            use_signal(&outer_sy("outer-basics"), || 0.0_f32).set_silent(200.0);
            for _ in 0..3 {
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    400.0,
                    800.0,
                    [0.0, 0.0],
                );
                let _ =
                    tree.build_and_layout_fast(&mut root, 400.0, 800.0, [-1.0, -1.0], true, false);
            }
            let result =
                tree.build_and_layout_fast(&mut root, 400.0, 800.0, [-1.0, -1.0], true, false);
            let inner_id = "ms-gallery-like:viewport";
            let metrics = result
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == inner_id)
                .expect("inner viewport metrics after outer scroll_only");
            let bounds = metrics.1;
            let max_y = metrics.2[1];
            assert!(
                max_y > 8.0,
                "inner must stay scrollable after outer scroll_only, max_y={max_y}"
            );
            let sy = use_signal(&scroll_offset_y_signal_key(inner_id), || 0.0_f32).peek();
            assert!(
                sy > 8.0,
                "inner offset must stay at the live edge, sy={sy} max_y={max_y}"
            );
            let hx = bounds[0] + bounds[2] * 0.5;
            let hy = bounds[1] + bounds[3] * 0.5;
            let mut chain = Vec::new();
            collect_scroll_rest_chain_at(&root, hx, hy, [0.0, 0.0], &mut chain);
            assert_eq!(
                chain.first().map(String::as_str),
                Some(inner_id),
                "pointer over the chat must hit the inner viewport first (chain={chain:?} point=({hx:.1},{hy:.1}) bounds={bounds:?})"
            );
        });
    });
}

fn count_slot(el: &Element, slot: &str) -> usize {
    let here = usize::from(el.attrs.get("data-slot").map(String::as_str) == Some(slot));
    here + el
        .children
        .iter()
        .map(|c| count_slot(c, slot))
        .sum::<usize>()
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

/// Gallery MessageScrollerShowcase used to graft a `rui_fragment` (default row flex)
/// through `reactive_view_sized`. Item `w(100%)` then collapsed and the chat went blank.
#[test]
fn message_scroller_reactive_fragment_island_paints_rows() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let rows = reactive_view_sized("ms-frag-rows__view", "100%", "max-content", || {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    let kids: Vec<Element> = (0..8)
                            .map(|i| {
                                message_scroller_item()
                                    .message_id(format!("m{i}"))
                                    .child(Element::new().w("100%").text(format!(
                                        "Turn {i}: enough text to wrap across multiple lines in a narrow chat column."
                                    )))
                                    .into()
                            })
                            .collect();
                    frag.children(kids)
                });
                let scroller: Element = message_scroller_provider()
                    .id("ms-frag-test")
                    .auto_scroll(true)
                    .default_scroll_position("end")
                    .child(message_scroller().h("100%").child(
                        message_scroller_viewport().child(message_scroller_content().child(rows)),
                    ))
                    .into();
                *live.borrow_mut() = Element::new()
                    .w(320.0)
                    .h(384.0)
                    .overflow("hidden")
                    .child(scroller);
            }
        });
    });

    let mut root = live_check.borrow_mut();
    let items = count_slot(&root, "message-scroller-item");
    assert_eq!(
        items, 8,
        "reconcile must graft eight MessageScrollerItem rows into the island"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let mut last = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
    for _ in 0..6 {
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 320.0, 384.0, [0.0, 0.0]);
        last = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
    }
    let visible: Vec<f32> = last
        .text_nodes
        .iter()
        .filter(|t| t.y >= -1.0 && t.y + t.height <= 385.0 && t.height > 4.0)
        .map(|t| t.y)
        .collect();
    let all_y: Vec<f32> = last.text_nodes.iter().map(|t| t.y).collect();
    assert!(
        visible.len() >= 2,
        "fragment island inside MessageScrollerContent must paint stacked turns (text={}, visible_y={visible:?}, all_y={all_y:?})",
        last.text_nodes.len()
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn gallery_nested_reactive_island_keeps_end_edge_visible() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let rows =
                    reactive_view_sized("ms-gal-island-rows__view", "100%", "max-content", || {
                        let mut col = crate::primitives::r#box::vstack()
                            .w("100%")
                            .min_w(0.0)
                            .h("max-content")
                            .gap("1.5rem");
                        for (i, text) in GAL_MS_TEXTS.iter().enumerate() {
                            col = col.child(bubble_turn(i, text));
                        }
                        col.into()
                    });
                *live.borrow_mut() = gallery_reactive_island_nested_root(rows);
            }
        });
    });

    let mut root = live_check.borrow_mut();
    assert_eq!(
        count_slot(&root, "message-scroller-item"),
        GAL_MS_TEXTS.len(),
        "reactive island must graft every turn before layout"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
    use_signal(&scroll_offset_y_signal_key("outer-gal-island"), || 0.0_f32).set_silent(200.0);
    let mut last_text = 0usize;
    let mut last_visible: Vec<(String, f32)> = Vec::new();
    for _ in 0..6 {
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 800.0, [0.0, 0.0]);
        let result = tree.build_and_layout_fast(&mut root, 400.0, 800.0, [-1.0, -1.0], true, false);
        last_text = result.text_nodes.len();
        last_visible = result
            .text_nodes
            .iter()
            .filter(|t| t.y >= -1.0 && t.y <= 800.0 && t.height > 4.0)
            .map(|t| (t.text.chars().take(24).collect::<String>(), t.y))
            .collect();
    }
    let sy = use_signal(
        &scroll_offset_y_signal_key("ms-gal-island:viewport"),
        || 0.0_f32,
    )
    .peek();
    assert!(
        last_visible.len() >= 2,
        "end-anchored gallery island must show multiple message lines (sy={sy} text={last_text} visible={last_visible:?})"
    );
    let end_edge = last_visible.iter().any(|(t, _)| {
        t.contains("assistive") || t.contains("screen readers") || t.contains("Auto-scroll")
    });
    assert!(
        end_edge,
        "defaultScrollPosition=end must keep the live edge in view, not blank tail (sy={sy} visible={last_visible:?})"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn message_scroller_for_keyed_island_paints_rows() {
    use crate::app::reactive::for_keyed_reactive;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let rows = for_keyed_reactive(
                    "ms-keyed-rows",
                    || {
                        GAL_MS_TEXTS
                            .iter()
                            .enumerate()
                            .map(|(i, text)| (i, (*text).to_string()))
                            .collect::<Vec<_>>()
                    },
                    |(i, _)| i.to_string(),
                    |(i, text)| bubble_turn(*i, &text),
                );
                *live.borrow_mut() = Element::new().w(320.0).h(384.0).overflow("hidden").child(
                    message_scroller_provider()
                        .id("ms-keyed-test")
                        .auto_scroll(true)
                        .default_scroll_position("end")
                        .child(
                            message_scroller().h("100%").child(
                                message_scroller_viewport()
                                    .child(message_scroller_content().child(rows)),
                            ),
                        ),
                );
            }
        });
    });

    let mut root = live_check.borrow_mut();
    assert_eq!(
        count_slot(&root, "message-scroller-item"),
        GAL_MS_TEXTS.len(),
        "for_keyed island must graft every turn before layout"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let mut last_text = 0usize;
    for _ in 0..6 {
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 320.0, 384.0, [0.0, 0.0]);
        let result = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, false);
        last_text = result.text_nodes.len();
    }
    assert!(
        last_text >= 3,
        "for_keyed island inside MessageScrollerContent must paint rows (text={last_text})"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn message_scroller_remount_preserves_scroll_when_auto_scroll_off() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let bump = use_signal("ms-remount-bump", || 0u32);
    let viewport = "ms-remount-test:viewport";
    let sy_key = scroll_offset_y_signal_key(viewport);

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let _ = bump.get();
                let mut content = message_scroller_content();
                for i in 0..8 {
                    content =
                        content.child(message_scroller_item().message_id(format!("m{i}")).child(
                            Element::new().text(format!(
                                "Turn {i}: enough text to overflow the viewport when stacked."
                            )),
                        ));
                }
                let scroller: Element = message_scroller_provider()
                    .id("ms-remount-test")
                    .auto_scroll(false)
                    .default_scroll_position("end")
                    .child(
                        message_scroller()
                            .h("100%")
                            .child(message_scroller_viewport().child(content)),
                    )
                    .into();
                *live.borrow_mut() = Element::new()
                    .w(320.0)
                    .h(384.0)
                    .overflow("hidden")
                    .child(scroller);
            }
        });
    });

    {
        let mut root = live_check.borrow_mut();
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..4 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        }
        use_signal(&sy_key, || 0.0_f32).set_silent(120.0);
    }

    bump.set(1);

    {
        let mut root = live_check.borrow_mut();
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..4 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        }
    }

    let sy = use_signal(&sy_key, || 0.0_f32).peek();
    assert!(
        (sy - 120.0).abs() < 1.0,
        "provider remount with auto_scroll=false must not reset scroll (sy={sy})"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// `ui!` runs `children_none_if_vacuous` on component children. A nested rows island is an
/// empty placeholder until its effect grafts — that shell must not be dropped.
#[test]
fn ui_like_vacuous_filter_keeps_nested_rows_placeholder() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let section = use_signal("ms-ui-vacuous-section", || "basics".to_string());
    let thread = use_signal("ms-ui-vacuous-thread", || {
        GAL_MS_TEXTS
            .iter()
            .take(3)
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let body = switch_reactive(
                    "ms-ui-vacuous-switch",
                    move || section.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key != "basics" {
                                return Element::new().text("other");
                            }
                            let thread = thread;
                            let rows = reactive_view_sized(
                                "ms-ui-vacuous-rows",
                                "100%",
                                "max-content",
                                move || {
                                    let mut col = crate::primitives::r#box::vstack()
                                        .w("100%")
                                        .min_w(0.0)
                                        .h("max-content")
                                        .gap("1.5rem");
                                    for (i, text) in thread.get().iter().enumerate() {
                                        col = col.child(bubble_turn(i, text));
                                    }
                                    col.into()
                                },
                            );
                            let rows = children_none_if_vacuous(Some(rows)).expect(
                                "ui! children_none_if_vacuous must keep nested reactive placeholders",
                            );
                            gallery_reactive_island_nested_root(rows)
                        })
                    },
                );
                *live.borrow_mut() = body;
            }
        });
    });

    assert_eq!(
        count_slot(&live_check.borrow(), "message-scroller-item"),
        3,
        "rows island inside static Provider must graft after ui!-style vacuous filtering"
    );

    thread.set(
        GAL_MS_TEXTS
            .iter()
            .take(4)
            .map(|s| (*s).to_string())
            .collect(),
    );
    assert_eq!(
        count_slot(&live_check.borrow(), "message-scroller-item"),
        4,
        "static Provider must keep grafting when only the rows island updates"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Gallery send path: static Provider + nested rows island. Follow must jump to the new
/// end after Taffy measures the appended turn (pre-layout `reactive_sync` cannot).
#[test]
fn message_scroller_nested_rows_island_auto_scrolls_on_append() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let section = use_signal("ms-append-section", || "basics".to_string());
    let thread = use_signal("ms-append-thread", || {
        GAL_MS_TEXTS
            .iter()
            .take(3)
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });
    let viewport = "ms-append-test:viewport";

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let body = switch_reactive(
                    "ms-append-switch",
                    move || section.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key != "basics" {
                                return Element::new().text("other");
                            }
                            let thread = thread;
                            let rows = reactive_view_sized(
                                "ms-append-rows",
                                "100%",
                                "max-content",
                                move || {
                                    let mut col = crate::primitives::r#box::vstack()
                                        .w("100%")
                                        .min_w(0.0)
                                        .h("max-content")
                                        .gap("1.5rem");
                                    for (i, text) in thread.get().iter().enumerate() {
                                        col = col.child(bubble_turn(i, text));
                                    }
                                    col.into()
                                },
                            );
                            let rows = children_none_if_vacuous(Some(rows))
                                .expect("rows placeholder must survive ui! vacuous filter");
                            let scroller: Element = message_scroller_provider()
                                .id("ms-append-test")
                                .auto_scroll(true)
                                .default_scroll_position("end")
                                .child(
                                    message_scroller().h("100%").child(
                                        message_scroller_viewport()
                                            .child(message_scroller_content().child(rows)),
                                    ),
                                )
                                .into();
                            Element::new()
                                .w(320.0)
                                .h(384.0)
                                .overflow("hidden")
                                .child(scroller)
                        })
                    },
                );
                *live.borrow_mut() = body;
            }
        });
    });

    let mut tree = LayoutTree::new();
    let sy_before;
    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..6 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        }
        sy_before = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        assert!(
            sy_before > 8.0,
            "end-anchored nested island must start away from y=0 (sy={sy_before})"
        );
    }

    thread.set(
        GAL_MS_TEXTS
            .iter()
            .take(4)
            .map(|s| (*s).to_string())
            .collect(),
    );

    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        // One Gallery-like frame: pre-layout sync, Taffy, post-layout follow.
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 320.0, 384.0, [0.0, 0.0]);
        // Follow must run *inside* `build_and_layout` (before collect). A post-collect
        // jump updates the offset signal but this frame still paints the old edge —
        // Gallery send then never redraws because `request_redraw` fails while AppState
        // is borrowed.
        let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        let sy = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        let max_y = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == viewport)
            .map(|(_, _, max_scroll, _)| max_scroll[1])
            .unwrap_or(0.0);
        assert!(
            sy > sy_before + 4.0,
            "auto-scroll on append must move toward the new end (before={sy_before} after={sy} max={max_y})"
        );
        assert!(
            sy + 16.0 >= max_y,
            "follow must land near the live edge (sy={sy} max={max_y})"
        );
        let needle = "turn anchoring";
        let painted = result.text_nodes.iter().find(|t| t.text.contains(needle));
        let Some(node) = painted else {
            panic!(
                "same-frame collect after send must paint the appended turn (sy={sy} texts={})",
                result.text_nodes.len()
            );
        };
        assert!(
            node.y + node.height > 0.0 && node.y < 384.0,
            "appended turn must land in the painted viewport (y={} h={} sy={sy})",
            node.y,
            node.height
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Gallery send: auto-scroll is on and the reader is still on the last measured end,
/// but `following` was released (max jitter). Append must still jump to the new bottom.
#[test]
fn message_scroller_send_follows_when_following_flag_was_released() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let section = use_signal("ms-send-follow-section", || "basics".to_string());
    let thread = use_signal("ms-send-follow-thread", || {
        GAL_MS_TEXTS
            .iter()
            .take(3)
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });
    let viewport = "ms-send-follow:viewport";

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let body = switch_reactive(
                    "ms-send-follow-switch",
                    move || section.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key != "basics" {
                                return Element::new().text("other");
                            }
                            let thread = thread;
                            let rows = reactive_view_sized(
                                "ms-send-follow-rows",
                                "100%",
                                "max-content",
                                move || {
                                    let mut col = crate::primitives::r#box::vstack()
                                        .w("100%")
                                        .min_w(0.0)
                                        .h("max-content")
                                        .gap("1.5rem");
                                    for (i, text) in thread.get().iter().enumerate() {
                                        col = col.child(bubble_turn(i, text));
                                    }
                                    col.into()
                                },
                            );
                            let rows = children_none_if_vacuous(Some(rows))
                                .expect("rows placeholder must survive ui! vacuous filter");
                            let scroller: Element = message_scroller_provider()
                                .id("ms-send-follow")
                                .auto_scroll(true)
                                .default_scroll_position("end")
                                .child(
                                    message_scroller()
                                        .h("100%")
                                        .child(
                                            message_scroller_viewport()
                                                .child(message_scroller_content().child(rows)),
                                        )
                                        .child(message_scroller_button().direction("end")),
                                )
                                .into();
                            Element::new()
                                .w(320.0)
                                .h(384.0)
                                .overflow("hidden")
                                .child(scroller)
                        })
                    },
                );
                *live.borrow_mut() = body;
            }
        });
    });

    let mut tree = LayoutTree::new();
    let sy_before;
    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..6 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        }
        sy_before = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        assert!(
            sy_before > 8.0,
            "must start at the live edge (sy={sy_before})"
        );
        use_signal("ms-send-follow:following", || true).set_silent(false);
    }

    thread.set(
        GAL_MS_TEXTS
            .iter()
            .take(4)
            .map(|s| (*s).to_string())
            .collect(),
    );
    message_scroller_stick_to_end("ms-send-follow");

    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 320.0, 384.0, [0.0, 0.0]);
        let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        let sy = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        let max_y = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == viewport)
            .map(|(_, _, max_scroll, _)| max_scroll[1])
            .unwrap_or(0.0);
        assert!(
            result
                .text_nodes
                .iter()
                .any(|t| t.text.contains("turn anchoring") && t.y + t.height > 0.0 && t.y < 384.0),
            "send follow must paint the new turn this frame (sy={sy} max={max_y})"
        );
        assert!(
            sy > sy_before + 4.0,
            "send while auto-scroll is on must follow even if following was released (before={sy_before} after={sy} max={max_y})"
        );
        assert!(
            sy + 16.0 >= max_y,
            "send follow must land near the new live edge (sy={sy} max={max_y})"
        );
        assert!(
            use_signal("ms-send-follow:following", || false).peek(),
            "follow flag must re-engage after a live-edge send"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Gallery Send calls `message_scroller_stick_to_end` outside the Provider. That must
/// jump even when the reader is *not* on the live edge (following released, offset mid-list).
#[test]
fn message_scroller_stick_to_end_jumps_from_mid_list() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let section = use_signal("ms-stick-section", || "basics".to_string());
    let thread = use_signal("ms-stick-thread", || {
        GAL_MS_TEXTS
            .iter()
            .take(3)
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });
    let viewport = "ms-stick:viewport";

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let body = switch_reactive(
                    "ms-stick-switch",
                    move || section.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key != "basics" {
                                return Element::new().text("other");
                            }
                            let thread = thread;
                            let rows = reactive_view_sized(
                                "ms-stick-rows",
                                "100%",
                                "max-content",
                                move || {
                                    let mut col = crate::primitives::r#box::vstack()
                                        .w("100%")
                                        .min_w(0.0)
                                        .h("max-content")
                                        .gap("1.5rem");
                                    for (i, text) in thread.get().iter().enumerate() {
                                        col = col.child(bubble_turn(i, text));
                                    }
                                    col.into()
                                },
                            );
                            let rows = children_none_if_vacuous(Some(rows))
                                .expect("rows placeholder must survive ui! vacuous filter");
                            let scroller: Element = message_scroller_provider()
                                .id("ms-stick")
                                .auto_scroll(true)
                                .default_scroll_position("end")
                                .child(
                                    message_scroller().h("100%").child(
                                        message_scroller_viewport()
                                            .child(message_scroller_content().child(rows)),
                                    ),
                                )
                                .into();
                            Element::new()
                                .w(320.0)
                                .h(384.0)
                                .overflow("hidden")
                                .child(scroller)
                        })
                    },
                );
                *live.borrow_mut() = body;
            }
        });
    });

    let mut tree = LayoutTree::new();
    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..6 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                320.0,
                384.0,
                [0.0, 0.0],
            );
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        }
        let sy = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        assert!(sy > 8.0, "must start scrolled (sy={sy})");
        use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).set_silent(0.0);
        use_signal("ms-stick:following", || true).set_silent(false);
    }

    thread.set(
        GAL_MS_TEXTS
            .iter()
            .take(4)
            .map(|s| (*s).to_string())
            .collect(),
    );
    message_scroller_stick_to_end("ms-stick");

    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 320.0, 384.0, [0.0, 0.0]);
        let result = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
        let sy = use_signal(&scroll_offset_y_signal_key(viewport), || 0.0_f32).peek();
        let max_y = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == viewport)
            .map(|(_, _, max_scroll, _)| max_scroll[1])
            .unwrap_or(0.0);
        assert!(
            sy + 16.0 >= max_y && sy > 8.0,
            "Send stick-to-end must jump from mid-list (sy={sy} max={max_y})"
        );
        assert!(
            result
                .text_nodes
                .iter()
                .any(|t| t.text.contains("turn anchoring") && t.y + t.height > 0.0 && t.y < 384.0),
            "Send stick-to-end must paint the new turn this frame (sy={sy} max={max_y})"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn ms_nav_btn(id: &str, label: &str) -> Element {
    SidebarMenuButton(SidebarMenuButtonProps {
        id: id.into(),
        is_active: None,
        variant: None,
        size: None,
        on_click: None,
        children: Some(Element::from(label)),
        base: None,
    })
}

fn fmt_rect(r: [f32; 4]) -> String {
    format!("[{:.1},{:.1} {:.1}x{:.1}]", r[0], r[1], r[2], r[3])
}

fn find_slot_ms<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot_ms(c, slot))
}

fn find_slot_ms_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children
        .iter_mut()
        .find_map(|c| find_slot_ms_mut(c, slot))
}

fn rail_captions_in_submit(
    prev: &[crate::layout::layout_tree::TextLayoutInfo],
    curr: &[crate::layout::layout_tree::TextLayoutInfo],
    ranges: &[crate::layout::layout_tree::OverlayBandRange],
    scroll_clip: Option<CullRects>,
) -> Vec<String> {
    let mut stolen = Vec::new();
    let push_hit = |stolen: &mut Vec<String>,
                    submitted: &[crate::layout::layout_tree::TextLayoutInfo]| {
        stolen.extend(
            submitted
                .iter()
                .filter(|n| n.text == "布局" || n.text == "Gallery" || n.text == "文本")
                .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height]))),
        );
    };
    if ranges.is_empty() {
        if let Some(submitted) =
            page_stack_band_submit_text(prev, curr, 0, curr.len(), scroll_clip, true)
        {
            push_hit(&mut stolen, &submitted);
        }
        return stolen;
    }
    for (i, band) in ranges.iter().enumerate() {
        let hi = if i + 1 == ranges.len() {
            curr.len()
        } else {
            band.text.1
        };
        if let Some(submitted) =
            page_stack_band_submit_text(prev, curr, band.text.0, hi, scroll_clip, true)
        {
            push_hit(&mut stolen, &submitted);
        }
    }
    stolen
}

/// Gallery: inset shell + page ScrollArea + MessageScroller. Auto-scroll /
/// stick-to-end PartialLoad must not src-over sidebar captions into retain.
#[test]
fn message_scroller_scroll_cull_must_not_include_sidebar_captions() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let (_handle, _) = create_root(|| {
        let mut content = message_scroller_content();
        for (i, text) in GAL_MS_TEXTS.iter().enumerate() {
            content = content.child(bubble_turn(i, text));
        }
        let chat: Element = message_scroller_provider()
            .id("gal-ms-showcase")
            .auto_scroll(true)
            .default_scroll_position("end")
            .child(
                message_scroller()
                    .h("100%")
                    .child(message_scroller_viewport().child(content))
                    .child(message_scroller_button().direction("end")),
            )
            .into();

        let inset_body = crate::primitives::r#box::vstack()
            .w("100%")
            .min_w(0.0)
            .min_h(0.0)
            .flex_grow(1.0)
            .child(
                scroll_area("scroll-basics")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .child(
                        crate::primitives::r#box::vstack()
                            .w("100%")
                            .gap("1rem")
                            .p("1.5rem")
                            .child(Element::new().h(80.0).w("100%"))
                            .child(
                                Element::new()
                                    .w("100%")
                                    .h(384.0)
                                    .overflow("hidden")
                                    .child(chat),
                            ),
                    ),
            );

        let mut root = sidebar_provider_scope("gal-shell", None, true, Some("inset"), || {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![
                sidebar("gal-sidebar")
                    .variant("inset")
                    .w("13rem")
                    .item(ms_nav_btn("nav-basics", "基础"))
                    .item(ms_nav_btn("nav-pointer", "指针事件"))
                    .item(ms_nav_btn("nav-form", "表单"))
                    .item(ms_nav_btn("nav-overlay", "浮层/反馈"))
                    .item(ms_nav_btn("nav-data", "数据"))
                    .item(ms_nav_btn("nav-draw", "绘制"))
                    .item(ms_nav_btn("nav-nav", "导航"))
                    .item(ms_nav_btn("nav-layout", "布局"))
                    .item(ms_nav_btn("nav-gallery", "Gallery"))
                    .item(ms_nav_btn("nav-text", "文本"))
                    .into(),
                SidebarInset(SidebarInsetProps {
                    variant: Some("inset".into()),
                    children: Some(inset_body.into()),
                    base: None,
                }),
            ])
        });

        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
        for _ in 0..4 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                1280.0,
                800.0,
                [0.0, 0.0],
            );
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
        }
        message_scroller_stick_to_end("gal-ms-showcase");
        sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            1280.0,
            800.0,
            [0.0, 0.0],
        );
        sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);

        let rail = find_slot_ms(&root, "sidebar-inner")
            .or_else(|| find_slot_ms(&root, "sidebar"))
            .expect("sidebar rail");
        let rail_bounds = rail.layout_bounds;
        let rail_right = rail_bounds[0] + rail_bounds[2];

        let captions: Vec<_> = result
            .text_nodes
            .iter()
            .filter(|n| n.text == "布局" || n.text == "Gallery" || n.text == "文本")
            .collect();
        assert_eq!(
            captions.len(),
            3,
            "expected 布局/Gallery/文本, got {:?}",
            result
                .text_nodes
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>()
        );

        let metrics_dump: Vec<String> = result
            .scroll_metrics
            .iter()
            .map(|(id, b, max, ch)| {
                format!(
                    "{id} bounds={} max_y={:.1} ch={:.1}",
                    fmt_rect(*b),
                    max[1],
                    ch[1]
                )
            })
            .collect();

        let ms_bounds = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id.contains("gal-ms-showcase"))
            .map(|(_, b, _, _)| *b)
            .expect("MessageScroller viewport metrics");

        assert!(
            ms_bounds[0] >= rail_right - 1.0,
            "MessageScroller viewport must sit to the right of the rail\n\
             ms={} rail={}\n metrics={metrics_dump:?}",
            fmt_rect(ms_bounds),
            fmt_rect(rail_bounds)
        );

        for (id, bounds, _, _) in &result.scroll_metrics {
            let cx = bounds[0] + bounds[2] * 0.5;
            if cx < rail_right {
                // Sidebar's own ScrollArea is allowed to include rail captions.
                continue;
            }
            let cull = padded_xywh(*bounds, SCROLL_LAYER_TEXT_PAD_LOGICAL);
            let stolen: Vec<String> = captions
                .iter()
                .filter(|n| crate::app::draw_cull::text_hits_damage(n, cull))
                .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
                .collect();
            assert!(
                stolen.is_empty(),
                "scroll-layer text clip for `{id}` includes sidebar captions {stolen:?}\n\
                 viewport={} cull={}\n rail={}\n metrics={metrics_dump:?}",
                fmt_rect(*bounds),
                fmt_rect(cull),
                fmt_rect(rail_bounds)
            );
        }

        let padded_inset = result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id.contains("scroll-basics"))
            .map(|(_, b, _, _)| padded_xywh(*b, DAMAGE_CULL_PAD_LOGICAL));
        let culled = clip_text_nodes_to_rect(
            &result.text_nodes,
            padded_xywh(ms_bounds, SCROLL_LAYER_TEXT_PAD_LOGICAL),
        );
        let stolen: Vec<String> = culled
            .iter()
            .filter(|n| {
                n.x + n.width * 0.5 < rail_right - 1.0
                    && (n.text == "布局" || n.text == "Gallery" || n.text == "文本")
            })
            .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
            .collect();
        assert!(
            stolen.is_empty(),
            "MessageScroller follow/stick PartialLoad would prepare sidebar captions {stolen:?}\n\
             ms={} rail={}\n padded_inset={padded_inset:?}\n metrics={metrics_dump:?}",
            fmt_rect(ms_bounds),
            fmt_rect(rail_bounds)
        );

        // Real Send: insert a turn inside MessageScrollerContent (middle of collect
        // order), then stick-to-end. Index-stable fake push at the vec tail hid this.
        let before_nodes = result.text_nodes.clone();
        let before_ranges = result.overlay_split.page_stack_band_ranges();
        let content = find_slot_ms_mut(&mut root, "message-scroller-content")
            .expect("message-scroller-content");
        content
            .children
            .push(bubble_turn(GAL_MS_TEXTS.len(), "send-follow-up"));
        message_scroller_stick_to_end("gal-ms-showcase");
        sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        let after = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
        let after_ms = after
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id.contains("gal-ms-showcase"))
            .map(|(_, b, _, _)| *b)
            .unwrap_or(ms_bounds);
        let after_ranges = after.overlay_split.page_stack_band_ranges();
        let dump_rail = |nodes: &[crate::layout::layout_tree::TextLayoutInfo]| -> Vec<String> {
            nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.text == "布局" || n.text == "Gallery" || n.text == "文本")
                .map(|(i, n)| {
                    format!(
                        "#{i} {}@{} clip={:?}",
                        n.text,
                        fmt_rect([n.x, n.y, n.width, n.height]),
                        n.clip_bounds
                    )
                })
                .collect()
        };
        let stolen_plain =
            rail_captions_in_submit(&before_nodes, &after.text_nodes, &after_ranges, None);
        let stolen_clip = rail_captions_in_submit(
            &before_nodes,
            &after.text_nodes,
            &after_ranges,
            Some(CullRects::one(padded_xywh(
                after_ms,
                SCROLL_LAYER_TEXT_PAD_LOGICAL,
            ))),
        );
        assert!(
            stolen_plain.is_empty() && stolen_clip.is_empty(),
            "Gallery Send PartialLoad must not submit sidebar captions\n\
             plain={stolen_plain:?}\n clip={stolen_clip:?}\n\
             before_rail={:?}\n after_rail={:?}\n\
             before_bands={:?} after_bands={:?}\n\
             ms={} after_ms={}",
            dump_rail(&before_nodes),
            dump_rail(&after.text_nodes),
            before_ranges.iter().map(|b| b.text).collect::<Vec<_>>(),
            after_ranges.iter().map(|b| b.text).collect::<Vec<_>>(),
            fmt_rect(ms_bounds),
            fmt_rect(after_ms)
        );
    });
}

fn pack_ms_frag(kids: Vec<Element>) -> Element {
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    frag.children(kids)
}

struct FooterSnap {
    bounds: [f32; 4],
    inner_bottom: f32,
    host_h: f32,
    host_grow: f32,
    text_y: f32,
    layout_y: f32,
    nav_text_y: f32,
    sidebar_sy: f32,
    ms_sy: f32,
    chrome_y: f32,
}

fn footer_snap(
    root: &Element,
    tree: &LayoutTree,
    result: &crate::layout::layout_tree::RenderResult,
) -> FooterSnap {
    let inner = find_slot_ms(root, "sidebar-inner").expect("sidebar-inner");
    let host = find_slot_ms(root, "sidebar-content-host").expect("sidebar-content-host");
    let footer = find_slot_ms(root, "sidebar-footer").expect("sidebar-footer");
    let inner_b = inner.layout_bounds;
    let host_b = host.layout_bounds;
    let footer_b = footer.layout_bounds;
    let taffy_y = footer
        .layout_node
        .and_then(|nid| tree.taffy.layout(nid).ok().map(|l| l.location.y))
        .unwrap_or(-1.0);
    let footer_text = result
        .text_nodes
        .iter()
        .filter(|n| n.text == "Gallery")
        .filter(|n| {
            n.x + n.width * 0.5 >= footer_b[0] - 1.0
                && n.x + n.width * 0.5 <= footer_b[0] + footer_b[2] + 1.0
                && n.y + n.height * 0.5 >= footer_b[1] - 8.0
                && n.y + n.height * 0.5 <= footer_b[1] + footer_b[3] + 8.0
        })
        .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let nav_gallery = result
        .text_nodes
        .iter()
        .filter(|n| n.text == "Gallery")
        .filter(|n| {
            footer_text
                .map(|f| (n.y - f.y).abs() > 0.5 || (n.x - f.x).abs() > 0.5)
                .unwrap_or(true)
        })
        .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let chrome_y = result
        .instances
        .iter()
        .filter(|i| {
            i.rect_size[0] > 8.0
                && i.rect_size[1] > 8.0
                && i.rect_pos[0] + i.rect_size[0] * 0.5 >= footer_b[0]
                && i.rect_pos[0] + i.rect_size[0] * 0.5 <= footer_b[0] + footer_b[2]
                && i.rect_pos[1] + i.rect_size[1] * 0.5 >= footer_b[1] - 4.0
                && i.rect_pos[1] + i.rect_size[1] * 0.5 <= footer_b[1] + footer_b[3] + 4.0
        })
        .map(|i| i.rect_pos[1])
        .fold(f32::INFINITY, f32::min);
    FooterSnap {
        bounds: footer_b,
        inner_bottom: inner_b[1] + inner_b[3],
        host_h: host_b[3],
        host_grow: host.style.flex_grow,
        text_y: footer_text.map(|n| n.y).unwrap_or(-1.0),
        layout_y: taffy_y,
        nav_text_y: nav_gallery.map(|n| n.y).unwrap_or(-1.0),
        sidebar_sy: use_signal(
            &scroll_offset_y_signal_key("gal-shell__sidebar-content"),
            || 0.0_f32,
        )
        .peek(),
        ms_sy: use_signal(
            &scroll_offset_y_signal_key("gal-ms-showcase:viewport"),
            || 0.0_f32,
        )
        .peek(),
        chrome_y,
    }
}

fn fmt_footer_snap(s: &FooterSnap) -> String {
    format!(
        "footer={} taffy_y={:.1} text_y={:.1} chrome_y={:.1} inner_bottom={:.1} host_h={:.1} grow={:.1} nav_gallery_y={:.1} sidebar_sy={:.1} ms_sy={:.1}",
        fmt_rect(s.bounds),
        s.layout_y,
        s.text_y,
        s.chrome_y,
        s.inner_bottom,
        s.host_h,
        s.host_grow,
        s.nav_text_y,
        s.sidebar_sy,
        s.ms_sy,
    )
}

/// Gallery Send must not lift SidebarFooter (chrome + "Gallery" label) onto 布局/Gallery/文本.
///
/// Footer sits *outside* SidebarContent's scroll viewport. Stick-to-end on MessageScroller
/// used to apply that scroll to the rail and pack the user row over the last nav items,
/// leaving the footer slot empty.
#[test]
fn gallery_send_must_not_lift_sidebar_footer() {
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let thread = use_signal("gal-ms-footer-lift-thread", || {
        GAL_MS_TEXTS
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let thread = thread;
                let rows = reactive_view_sized(
                    "gal-ms-footer-lift-rows",
                    "100%",
                    "max-content",
                    move || {
                        let mut col = crate::primitives::r#box::vstack()
                            .w("100%")
                            .min_w(0.0)
                            .h("max-content")
                            .gap("1.5rem");
                        for (i, text) in thread.get().iter().enumerate() {
                            col = col.child(bubble_turn(i, text));
                        }
                        col.into()
                    },
                );
                let rows = children_none_if_vacuous(Some(rows))
                    .expect("rows placeholder must survive ui! vacuous filter");
                let chat: Element = message_scroller_provider()
                    .id("gal-ms-showcase")
                    .auto_scroll(true)
                    .default_scroll_position("end")
                    .child(
                        message_scroller()
                            .h("100%")
                            .child(
                                message_scroller_viewport()
                                    .child(message_scroller_content().child(rows)),
                            )
                            .child(message_scroller_button().direction("end")),
                    )
                    .into();
                let inset_body = crate::primitives::r#box::vstack()
                    .w("100%")
                    .min_w(0.0)
                    .min_h(0.0)
                    .flex_grow(1.0)
                    .child(
                        scroll_area("scroll-basics")
                            .flex_grow(1.0)
                            .min_h(0.0)
                            .overflow_y("auto")
                            .child(
                                crate::primitives::r#box::vstack()
                                    .w("100%")
                                    .gap("1rem")
                                    .p("1.5rem")
                                    .child(Element::new().h(80.0).w("100%"))
                                    .child(
                                        Element::new()
                                            .w("100%")
                                            .h(384.0)
                                            .overflow("hidden")
                                            .child(chat),
                                    ),
                            ),
                    );
                let nav = pack_ms_frag(vec![
                    ms_nav_btn("nav-basics", "基础"),
                    ms_nav_btn("nav-pointer", "指针事件"),
                    ms_nav_btn("nav-form", "表单"),
                    ms_nav_btn("nav-overlay", "浮层/反馈"),
                    ms_nav_btn("nav-data", "数据"),
                    ms_nav_btn("nav-draw", "绘制"),
                    ms_nav_btn("nav-nav", "导航"),
                    ms_nav_btn("nav-layout", "布局"),
                    ms_nav_btn("nav-gallery", "Gallery"),
                    ms_nav_btn("nav-text", "文本"),
                    ms_nav_btn("nav-theme", "主题"),
                    ms_nav_btn("nav-about", "关于"),
                ]);
                let body =
                    sidebar_provider_scope("gal-shell", None, true, Some("inset"), move || {
                        pack_ms_frag(vec![
                            Sidebar(SidebarProps {
                                id: "gal-sidebar".into(),
                                rail_w: Some("13rem".into()),
                                side: None,
                                variant: Some("inset".into()),
                                collapsible: Some("icon".into()),
                                children: Some(pack_ms_frag(vec![
                                    SidebarHeader(SidebarHeaderProps {
                                        children: Some(ms_nav_btn("gal-workspace", "RUI Gallery")),
                                        base: None,
                                    }),
                                    SidebarSeparator(SidebarSeparatorProps {
                                        children: None,
                                        base: None,
                                    }),
                                    SidebarContent(SidebarContentProps {
                                        children: Some(nav),
                                        base: None,
                                    }),
                                    SidebarSeparator(SidebarSeparatorProps {
                                        children: None,
                                        base: None,
                                    }),
                                    SidebarFooter(SidebarFooterProps {
                                        children: Some(ms_nav_btn("gal-user", "Gallery")),
                                        base: None,
                                    }),
                                ])),
                                base: None,
                            }),
                            SidebarInset(SidebarInsetProps {
                                variant: Some("inset".into()),
                                children: Some(inset_body.into()),
                                base: None,
                            }),
                        ])
                    });
                *live.borrow_mut() = body;
            }
        });
    });

    let mut tree = LayoutTree::new();
    let mut before;
    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..4 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                1280.0,
                800.0,
                [0.0, 0.0],
            );
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
        }
        message_scroller_stick_to_end("gal-ms-showcase");
        sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            1280.0,
            800.0,
            [0.0, 0.0],
        );
        sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
        before = footer_snap(&root, &tree, &result);
        assert!(
            before.bounds[3] > 8.0,
            "footer must have height before send\n{}",
            fmt_footer_snap(&before)
        );
        assert!(
            (before.bounds[1] + before.bounds[3] - before.inner_bottom).abs() < 8.0,
            "footer must sit on the rail bottom before send\n{}",
            fmt_footer_snap(&before)
        );
        assert!(
            before.bounds[1] > before.nav_text_y + 8.0,
            "footer must sit below nav Gallery before send\n{}",
            fmt_footer_snap(&before)
        );

        // AppState maximize uses the fast collect-skip path (not full Taffy rebuild).
        tree.set_collect_skip_allowed(true);
        let _ = tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [-1.0, -1.0], false, false);
        let maxed_result =
            tree.build_and_layout_fast(&mut root, 1280.0, 1200.0, [-1.0, -1.0], false, false);
        let maxed = footer_snap(&root, &tree, &maxed_result);
        assert!(
            maxed.bounds[1] > before.bounds[1] + 80.0,
            "maximize must move Footer down with the rail, not keep the 800h Y\n\
             before={}\nmaxed={}",
            fmt_footer_snap(&before),
            fmt_footer_snap(&maxed)
        );
        assert!(
            (maxed.bounds[1] + maxed.bounds[3] - maxed.inner_bottom).abs() < 8.0,
            "footer must sit on the maximized rail bottom\n{}",
            fmt_footer_snap(&maxed)
        );
        before = maxed;
    }

    let mut next = thread.get();
    next.push("send-follow-up that must not lift the sidebar footer".into());
    thread.set(next);
    message_scroller_stick_to_end("gal-ms-showcase");

    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            1280.0,
            1200.0,
            [0.0, 0.0],
        );
        sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        let after_result =
            tree.build_and_layout_fast(&mut root, 1280.0, 1200.0, [-1.0, -1.0], false, false);
        let after = footer_snap(&root, &tree, &after_result);
        let dump = format!(
            "before={}\nafter={}",
            fmt_footer_snap(&before),
            fmt_footer_snap(&after)
        );
        let footer_el = find_slot_ms(&root, "sidebar-footer").expect("sidebar-footer");
        let ms_bounds = after_result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id.contains("gal-ms-showcase"))
            .map(|(_, b, _, _)| *b)
            .expect("MessageScroller viewport");
        let footer_key = footer_el.layout_node.map(u64::from).unwrap_or(0);
        assert!(
            !crate::app::damage::paint_dirty_matches_slot(
                footer_key,
                ms_bounds,
                footer_key,
                after.bounds,
            ),
            "recycled Taffy key must not pair the chat island with SidebarFooter\n{dump}"
        );
        let collision = crate::app::damage::PaintDirty {
            key: footer_key,
            bounds: ms_bounds,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        if let Some((rect, _)) = crate::app::draw_cull::node_present_dest_reset(
            collision.key,
            collision.bounds,
            &after_result.node_paints,
            &after_result.instances,
        ) {
            assert!(
                crate::app::damage::intersect_rects(rect, after.bounds).is_none(),
                "Send node-present dest-reset wiped SidebarFooter {}\n{dump}",
                fmt_rect(rect)
            );
        }
        let captured = crate::app::draw_cull::node_present_capture_text_indices(
            &[collision],
            &after_result.text_nodes,
            &after_result.node_paints,
        );
        let stolen: Vec<&str> = captured
            .iter()
            .filter_map(|&i| after_result.text_nodes.get(i))
            .filter(|n| n.text == "布局" || n.text == "Gallery" || n.text == "文本")
            .map(|n| n.text.as_str())
            .collect();
        assert!(
            stolen.is_empty(),
            "Send node-present must not capture rail captions {stolen:?}\n{dump}"
        );
        assert!(
            after.bounds[1] >= before.bounds[1] - 2.0,
            "Send/stick-to-end lifted SidebarFooter layout_bounds up\n{dump}"
        );
        assert!(
            after.text_y < 0.0 || after.text_y >= before.text_y - 2.0,
            "Send/stick-to-end lifted SidebarFooter glyphs up\n{dump}"
        );
        assert!(
            after.chrome_y.is_infinite() || after.chrome_y >= before.chrome_y - 2.0,
            "Send/stick-to-end lifted SidebarFooter chrome up\n{dump}"
        );
        assert!(
            (after.bounds[1] + after.bounds[3] - after.inner_bottom).abs() < 8.0,
            "footer left the rail bottom after send (empty footer slot)\n{dump}"
        );
        assert!(
            after.bounds[1] > after.nav_text_y + 8.0,
            "footer overlapped nav Gallery / 布局 / 文本 after send\n{dump}"
        );
        assert!(
            after.sidebar_sy.abs() < 1.0,
            "sidebar content picked up MessageScroller scroll\n{dump}"
        );
        assert!(
            (after.host_h - before.host_h).abs() < 2.0,
            "SidebarContent host height collapsed and packed footer up\n{dump}"
        );
        let (skipped, patched) = tree.last_collect_skip_counts();
        assert!(
            skipped >= 1,
            "Send must replay the clean sidebar/footer subtree, not recollect it\n\
             skipped={skipped} patched={patched}\n{dump}"
        );

        // AppState restore/minimize is Clean + scroll_only; Footer must follow the rail.
        let restored_result =
            tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [-1.0, -1.0], true, false);
        let restored = footer_snap(&root, &tree, &restored_result);
        assert!(
            (restored.bounds[1] + restored.bounds[3] - restored.inner_bottom).abs() < 8.0,
            "restore/minimize must sit Footer on the short rail bottom\n{}",
            fmt_footer_snap(&restored)
        );
        assert!(
            restored.bounds[1] + 80.0 < before.bounds[1],
            "restore/minimize must move Footer up with the rail, not keep the 1200h Y\n\
             maxed={}\nrestored={}",
            fmt_footer_snap(&before),
            fmt_footer_snap(&restored)
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn publish_scroll_metrics(metrics: &[(String, [f32; 4], [f32; 2], [f32; 2])]) {
    for (rest, bounds, _max, content) in metrics {
        use_signal(&scroll_viewport_w_signal_key(rest), || 0.0_f32).set_silent(bounds[2]);
        use_signal(&scroll_viewport_h_signal_key(rest), || 0.0_f32).set_silent(bounds[3]);
        use_signal(&scroll_content_w_signal_key(rest), || 0.0_f32).set_silent(content[0]);
        use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).set_silent(content[1]);
    }
}

fn scroll_metric<'a>(
    metrics: &'a [(String, [f32; 4], [f32; 2], [f32; 2])],
    needle: &str,
) -> Option<&'a (String, [f32; 4], [f32; 2], [f32; 2])> {
    metrics.iter().find(|(id, ..)| id.contains(needle))
}

/// AppState maximize is Clean + scroll_only + overlay collect. Last-frame sidebar
/// signals still say "overflow" from the short window; MessageScroller follow must
/// not keep that overlay track painted on the tall rail.
#[test]
fn maximize_must_not_paint_sidebar_scrollbar_from_message_scroller() {
    use crate::utils::children_none_if_vacuous;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    let thread = use_signal("gal-ms-sb-max-thread", || {
        GAL_MS_TEXTS
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    });

    mark_next_effect_as_ui_root();
    let (_owner, _) = create_root(|| {
        create_effect({
            let live = Rc::clone(&live);
            move || {
                let thread = thread;
                let rows =
                    reactive_view_sized("gal-ms-sb-max-rows", "100%", "max-content", move || {
                        let mut col = crate::primitives::r#box::vstack()
                            .w("100%")
                            .min_w(0.0)
                            .h("max-content")
                            .gap("1.5rem");
                        for (i, text) in thread.get().iter().enumerate() {
                            col = col.child(bubble_turn(i, text));
                        }
                        col.into()
                    });
                let rows = children_none_if_vacuous(Some(rows))
                    .expect("rows placeholder must survive ui! vacuous filter");
                let chat: Element = message_scroller_provider()
                    .id("gal-ms-showcase")
                    .auto_scroll(true)
                    .default_scroll_position("end")
                    .child(
                        message_scroller()
                            .h("100%")
                            .child(
                                message_scroller_viewport()
                                    .child(message_scroller_content().child(rows)),
                            )
                            .child(message_scroller_button().direction("end")),
                    )
                    .into();
                let inset_body = crate::primitives::r#box::vstack()
                    .w("100%")
                    .min_w(0.0)
                    .min_h(0.0)
                    .flex_grow(1.0)
                    .child(
                        scroll_area("scroll-basics")
                            .flex_grow(1.0)
                            .min_h(0.0)
                            .overflow_y("auto")
                            .child(
                                crate::primitives::r#box::vstack()
                                    .w("100%")
                                    .gap("1rem")
                                    .p("1.5rem")
                                    .child(Element::new().h(80.0).w("100%"))
                                    .child(
                                        Element::new()
                                            .w("100%")
                                            .h(384.0)
                                            .overflow("hidden")
                                            .child(chat),
                                    )
                                    .child(Element::new().h(2000.0).w("100%")),
                            ),
                    );
                let nav_items = pack_ms_frag(vec![
                    ms_nav_btn("nav-basics", "基础"),
                    ms_nav_btn("nav-pointer", "指针事件"),
                    ms_nav_btn("nav-form", "表单"),
                    ms_nav_btn("nav-overlay", "浮层/反馈"),
                    ms_nav_btn("nav-data", "数据"),
                    ms_nav_btn("nav-draw", "绘制"),
                    ms_nav_btn("nav-nav", "导航"),
                    ms_nav_btn("nav-layout", "布局"),
                    ms_nav_btn("nav-gallery", "Gallery"),
                    ms_nav_btn("nav-text", "文本"),
                    ms_nav_btn("nav-theme", "主题"),
                    ms_nav_btn("nav-about", "关于"),
                    ms_nav_btn("nav-extra-a", "额外甲"),
                    ms_nav_btn("nav-extra-b", "额外乙"),
                    ms_nav_btn("nav-extra-c", "额外丙"),
                ]);
                let nav: Element = sidebar_group()
                    .child(sidebar_group_label().text("导航"))
                    .child(sidebar_group_content().child(sidebar_menu().child(nav_items)))
                    .into();
                let body =
                    sidebar_provider_scope("gal-shell", None, true, Some("inset"), move || {
                        pack_ms_frag(vec![
                            Sidebar(SidebarProps {
                                id: "gal-sidebar".into(),
                                rail_w: Some("13rem".into()),
                                side: None,
                                variant: Some("inset".into()),
                                collapsible: Some("icon".into()),
                                children: Some(pack_ms_frag(vec![
                                    SidebarHeader(SidebarHeaderProps {
                                        children: Some(ms_nav_btn("gal-workspace", "RUI Gallery")),
                                        base: None,
                                    }),
                                    SidebarSeparator(SidebarSeparatorProps {
                                        children: None,
                                        base: None,
                                    }),
                                    SidebarContent(SidebarContentProps {
                                        children: Some(nav),
                                        base: None,
                                    }),
                                    SidebarSeparator(SidebarSeparatorProps {
                                        children: None,
                                        base: None,
                                    }),
                                    SidebarFooter(SidebarFooterProps {
                                        children: Some(ms_nav_btn("gal-user", "Gallery")),
                                        base: None,
                                    }),
                                ])),
                                base: None,
                            }),
                            SidebarInset(SidebarInsetProps {
                                variant: Some("inset".into()),
                                children: Some(inset_body.into()),
                                base: None,
                            }),
                        ])
                    });
                *live.borrow_mut() = body;
            }
        });
    });

    let mut tree = LayoutTree::new();
    const SIDEBAR_REST: &str = "gal-shell__sidebar-content";
    const MS_REST: &str = "gal-ms-showcase:viewport";
    {
        let mut root = live_check.borrow_mut();
        let mut impact = LayoutStatus::Clean;
        for _ in 0..3 {
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                1280.0,
                360.0,
                [0.0, 0.0],
            );
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _ = tree.build_and_layout(&mut root, 1280.0, 360.0, [-1.0, -1.0]);
        }
        message_scroller_stick_to_end("gal-ms-showcase");
        sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            1280.0,
            360.0,
            [0.0, 0.0],
        );
        sync_message_scroller_follow_after_layout(&mut root, &mut tree);
        let short = tree.build_and_layout(&mut root, 1280.0, 360.0, [-1.0, -1.0]);
        publish_scroll_metrics(&short.scroll_metrics);

        let short_sb =
            scroll_metric(&short.scroll_metrics, SIDEBAR_REST).expect("sidebar metrics at 360h");
        let short_ms = scroll_metric(&short.scroll_metrics, MS_REST).expect("ms metrics at 360h");
        assert!(
            short_sb.2[1] > 8.0,
            "short window must overflow SidebarContent so maximize can inherit stale overlay signals, max={:?} vh={} ch={}",
            short_sb.2,
            short_sb.1[3],
            short_sb.3[1]
        );
        assert!(
            short_ms.2[1] > 8.0,
            "MessageScroller must stay independently scrollable, max={:?}",
            short_ms.2
        );

        tree.set_collect_skip_allowed(true);
        let maxed = tree.build_and_layout_fast(&mut root, 1280.0, 1200.0, [-1.0, -1.0], true, true);
        let maxed_sb =
            scroll_metric(&maxed.scroll_metrics, SIDEBAR_REST).expect("sidebar metrics at 1200h");
        let maxed_ms = scroll_metric(&maxed.scroll_metrics, MS_REST).expect("ms metrics at 1200h");
        let page = scroll_metric(&maxed.scroll_metrics, "scroll-basics");
        let dump = format!(
            "short_sb=bounds={:?} max={:?} content={:?}\n\
             maxed_sb=bounds={:?} max={:?} content={:?}\n\
             short_ms=max={:?} content={:?}\n\
             maxed_ms=max={:?} content={:?}\n\
             page={:?}",
            short_sb.1,
            short_sb.2,
            short_sb.3,
            maxed_sb.1,
            maxed_sb.2,
            maxed_sb.3,
            short_ms.2,
            short_ms.3,
            maxed_ms.2,
            maxed_ms.3,
            page.map(|p| (p.1, p.2, p.3)),
        );
        assert!(
            maxed_sb.2[1] < 0.5,
            "maximized rail must not stay scrollable\n{dump}"
        );
        assert!(
            maxed_ms.2[1] > 8.0,
            "MessageScroller overflow must not be folded into the sidebar\n{dump}"
        );
        assert!(
            (maxed_sb.3[1] - maxed_ms.3[1]).abs() > 40.0,
            "sidebar content_h must not copy MessageScroller content_h\n{dump}"
        );

        let rail_right = maxed_sb.1[0] + maxed_sb.1[2];
        let thin_bars: Vec<_> = maxed
            .instances
            .iter()
            .filter(|i| i.rect_size[0] > 4.0 && i.rect_size[0] < 28.0 && i.rect_size[1] > 80.0)
            .map(|i| {
                (
                    i.rect_pos[0],
                    i.rect_pos[1],
                    i.rect_size[0],
                    i.rect_size[1],
                    i.color[3],
                )
            })
            .collect();
        let rail_bars: Vec<_> = thin_bars
            .iter()
            .filter(|b| b.0 < rail_right + 16.0)
            .cloned()
            .collect();
        assert!(
            rail_bars.is_empty(),
            "maximize painted a scrollbar on the sidebar edge (MessageScroller must not \
             trigger the rail overlay)\nrail_right={rail_right:.1} bars={rail_bars:?}\n\
             all_thin={thin_bars:?}\n{dump}"
        );
    }

    clear_test_reconcile_root();
    reset_registered_anchors();
}
