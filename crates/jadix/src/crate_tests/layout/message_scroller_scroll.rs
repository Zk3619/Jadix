//! MessageScroller: `defaultScrollPosition=end` must paint the live edge, not an empty
//! tail from `min-h-full` / inflated `content_h`.

use crate::app::draw_cull::{
    clip_text_nodes_to_rect, padded_xywh, page_stack_band_submit_text, CullRects,
    DAMAGE_CULL_PAD_LOGICAL, SCROLL_LAYER_TEXT_PAD_LOGICAL,
};
use crate::app::reactive::{reactive_view_sized, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::bubble::bubble;
use crate::components::label::label;
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
    "MessageScrollerContent sets role=log and aria-relevant=additions by default, so screen readers announce new messages as they stream in.\n\nThe scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.",
];

fn gallery_wrap_under_jump_root() -> Element {
    let mut content = message_scroller_content();
    for i in 0..5 {
        content =
            content.child(message_scroller_item().message_id(format!("m{i}")).child(
                Element::new().flex_col().w("100%").min_w(0.0).child(
                    Element::new().w("100%").text(format!(
                        "Turn {i}: enough text to wrap across multiple lines."
                    )),
                ),
            ));
    }
    content = content.child(
        message_scroller_item()
            .message_id("m-gallery-wrap")
            .child(
                Element::new()
                    .flex_col()
                    .w("100%")
                    .min_w(0.0)
                    .gap(8.0)
                    .child(Element::new().w("100%").text(
                        "MessageScrollerContent sets role=log and aria-relevant=additions by default, so screen readers announce new messages as they stream in.",
                    ))
                    .child(Element::new().w("100%").text(
                        "The scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.",
                    ))
                    .child(Element::new().w("100%").text(
                        "Got it. Stay at the live edge and I'll keep following.",
                    )),
            ),
    );
    let scroller: Element = message_scroller_provider()
        .id("ms-gallery-wrap")
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
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(scroller)
}

/// Gallery `gal_ms_bubble_body`: `\n\n` splits become `<Label leading=normal>`.
fn gallery_label_wrap_under_jump_root() -> Element {
    let p2 = "The scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.";
    let body = label("gal-ms-p2")
        .text(p2)
        .text_size(14.0)
        .text_leading_normal()
        .w("100%");
    let turn: Element =
        message_scroller_item()
            .message_id("m-gallery-label-wrap")
            .child(message().align("start").child(
                message_content().child(bubble().variant("ghost").align("start").child(body)),
            ))
            .into();
    let mut content = message_scroller_content();
    for i in 0..5 {
        content =
            content.child(message_scroller_item().message_id(format!("m{i}")).child(
                Element::new().flex_col().w("100%").min_w(0.0).child(
                    Element::new().w("100%").text(format!(
                        "Turn {i}: enough text to wrap across multiple lines."
                    )),
                ),
            ));
    }
    content = content.child(turn);
    let scroller: Element = message_scroller_provider()
        .id("ms-gallery-label-wrap")
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
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(scroller)
}

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
fn scaled_message_scroller_end_keeps_messages_in_view() {
    create_root(|| {
        let mut root = Element::new().w(800.0).h(700.0).child(
            gallery_like_message_scroller_root()
                .scale(0.57)
                .transform_origin("left top"),
        );
        let mut tree = LayoutTree::new();
        for frame in 0..6 {
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(
                &mut root, &mut tree, &mut impact, 800.0, 700.0, [0.0, 0.0],
            );
            let result = tree.build_and_layout(&mut root, 800.0, 700.0, [-1.0, -1.0]);
            publish_scroll_metrics(&result.scroll_metrics);
            if frame >= 2 {
                let viewport = find_slot_ms(&root, "message-scroller-viewport").expect("viewport");
                let viewport = crate::core::tree::visual_paint_aabb(viewport, viewport.paint_scroll_acc);
                assert!(result.text_nodes.iter().any(|text| {
                    text.text.contains("MessageScrollerContent sets role")
                        && text.y < viewport[1] + viewport[3]
                        && text.y + text.height * text.paint_scale > viewport[1]
                }), "frame {frame}: scaled live edge must contain message text");
            }
        }
    });
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
            // Cache replay is tested at a presented pose, not the alpha-zero
            // first-enter seed (which correctly contributes no painted bounds).
            settle_latest_jump_pose(&mut root);
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
#[cfg(feature = "animation-base")]
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
            assert_eq!(
                btn.attrs.get("data-active").map(String::as_str),
                Some("false"),
                "at live edge Latest must retarget hide"
            );
            assert!(
                btn.style.opacity > 0.5,
                "hide t=0 must keep the shown visual so the fade can run, opacity={}",
                btn.style.opacity
            );
            assert!(
                btn.style.translate[1].abs() < 4.0,
                "hide t=0 must start the slide from rest, translate={:?}",
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
                hidden.text_nodes.iter().any(|t| t.text == "Latest"),
                "hide t=0 must still emit Latest glyphs (fade, not a pop-hide)"
            );
        });
    });
}

#[test]
#[cfg(feature = "animation-base")]
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
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
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
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(80.0);
            let mid = {
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
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

fn production_jump_chrome_pass(
    root: &mut Element,
    tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    sync_reactive_elements_subtree(root, tree, impact, 320.0, 384.0, [0.0, 0.0]);
    crate::style::sync_class_variants_subtree(
        root,
        [-1.0e9, -1.0e9],
        "",
        false,
        320.0,
        384.0,
        tree,
        impact,
    );
}

/// Enter is a 200ms CSS tween. Tests that need the settled pill snap the shown
/// pose; follow now leaves collect at the hide `from` (opacity 0). Do not mix a
/// virtual clock with tweens started on `Instant` — that samples hide at t≈1.
#[cfg(feature = "animation-base")]
fn settle_latest_hide_pose(root: &mut Element) {
    let Some(btn) = find_slot_ms_mut(root, "message-scroller-button") else {
        return;
    };
    crate::utils::motion::snap_motion_baseline(btn);
    btn.style.opacity = 0.0;
    btn.style.scale = [0.95, 0.95];
    btn.style.translate = [0.0, 8.0];
    btn.style.translate_pct = [0.0, 0.0];
    crate::style::patch_class_set_base_motion(btn);
    crate::utils::motion::snap_motion_baseline(btn);
}

fn settle_latest_jump_pose(root: &mut Element) {
    let Some(btn) = find_slot_ms_mut(root, "message-scroller-button") else {
        return;
    };
    crate::utils::motion::snap_motion_baseline(btn);
    btn.style.opacity = 1.0;
    btn.style.scale = [1.0, 1.0];
    btn.style.translate = [0.0, 0.0];
    btn.style.translate_pct = [0.0, 0.0];
    crate::style::patch_class_set_base_motion(btn);
    crate::utils::motion::snap_motion_baseline(btn);
}

#[test]
#[cfg(feature = "animation-base")]
fn message_scroller_latest_class_restore_must_keep_css_enter_mid() {
    // Production: `button()` class snapshot is taken before hide pose / opacity
    // transition. `resolve_class_variants` used to clone that snapshot over the
    // live style, dropping `transition: opacity, transform` so CSS tick discarded
    // the tween and Latest popped in at opacity 1 (shadow dest-reset punched the
    // wrapping thread).
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            {
                let btn = find_slot(&root, "message-scroller-button").expect("jump button");
                assert!(
                    btn.style.opacity < 0.05,
                    "seed from hidden, opacity={}",
                    btn.style.opacity
                );
                assert!(
                    btn.style
                        .transition
                        .iter()
                        .any(|e| e.property == crate::style::TransitionProperty::Opacity),
                    "hide pose must keep opacity transition after class restore, got {:?}",
                    btn.style.transition
                );
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            crate::style::set_css_transition_timeline_ms(80.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            crate::app::damage::clear_frame_paint_nodes();
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let follow_dirties = crate::app::damage::take_frame_paint_nodes();
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.08 && btn.style.opacity < 0.92,
                "class restore + follow must keep CSS enter mid, opacity={}",
                btn.style.opacity
            );
            assert!(
                btn.style.translate[1] > 0.3 && btn.style.translate[1] < 7.7,
                "enter must slide up through a mid translateY, got {:?}",
                btn.style.translate
            );
            assert!(
                follow_dirties
                    .iter()
                    .all(|d| d.kind != crate::app::damage::PaintDirtyKind::Node
                        || d.key != u64::from(btn.layout_node.expect("jump taffy"))),
                "follow must not dest-reset the pill every tween frame, got {follow_dirties:?}"
            );

            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
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
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            crate::style::set_css_transition_timeline_ms(280.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide_mid = find_slot(&root, "message-scroller-button")
                .expect("jump button")
                .style
                .opacity;
            assert!(
                hide_mid > 0.08 && hide_mid < 0.92,
                "class restore must keep CSS hide mid, opacity={hide_mid}"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
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
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(200.0);
            {
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
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
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
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
            crate::app::damage::clear_frame_paint_nodes();
            {
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
                let _ = btn.tick_css_transitions(0.0);
            }
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let settle_dirties = crate::app::damage::take_frame_paint_nodes();
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity < 0.05,
                "after hide duration Latest must settle hidden, opacity={}",
                btn.style.opacity
            );
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "live-edge collect must drop Latest glyphs after CSS hide"
            );
            assert!(
                settle_dirties.iter().any(|d| d.key == btn_key),
                "hide settle must dest-reset the last faded pill or PartialLoad keeps a ghost, got {settle_dirties:?}"
            );
            assert!(
                hidden.text_nodes.iter().any(|t| t.text.contains("Turn ")),
                "hide settle dest-reset must keep wrapping thread glyphs"
            );
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let hide_dirty = crate::app::damage::take_frame_paint_nodes()
                .into_iter()
                .find(|d| d.key == btn_key)
                .expect("hide settle dest-reset dirty");
            let last = crate::app::draw_cull::last_present_visual_slots(
                &hidden.node_paints,
                &hidden.instances,
                &[],
                &hidden.text_nodes,
                &[],
            );
            let prev = last.iter().find(|(k, _)| *k == btn_key).map(|(_, b)| *b);
            if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                hide_dirty.key,
                hide_dirty.bounds,
                &hidden.node_paints,
                &hidden.instances,
                &[],
                &[],
                1.0,
                prev,
            ) {
                assert!(
                    reset.rect[2] > 8.0 && reset.rect[3] > 8.0,
                    "hide settle wipe must cover the faded pill, got {:?}",
                    reset.rect
                );
                let overlapping: Vec<_> = hidden
                    .text_nodes
                    .iter()
                    .filter(|t| t.text != "Latest")
                    .filter(|t| crate::app::draw_cull::text_hits_damage(t, reset.rect))
                    .cloned()
                    .collect();
                if !overlapping.is_empty() {
                    let underlay =
                        crate::app::draw_cull::node_present_layered_underlay_capture_text(
                            &[hide_dirty],
                            &hidden.text_nodes,
                            &hidden.node_paints,
                        );
                    assert!(
                        !underlay.is_empty(),
                        "hide settle dest-reset must recapture wrapping in the pill hole or the thread stays punched"
                    );
                }
            }
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn message_scroller_latest_enter_t0_must_not_punch_thread_and_hide_must_fade() {
    // Production: follow writes the CSS *target* after the page tick. Collect then
    // painted opacity 1 + shadow dest-reset (blank white pill, no label). The next
    // CSS t=0 frame skipped glyphs. Hide wrote 0 on the same path and popped out.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_dirties = crate::app::damage::take_frame_paint_nodes();
            let viewport_key = u64::from(
                find_slot(&root, "message-scroller-viewport")
                    .expect("message viewport")
                    .layout_node
                    .expect("viewport taffy"),
            );
            assert!(
                enter_dirties.iter().all(|dirty| dirty.key != viewport_key),
                "scroll-only sync must not emit a full-viewport paint record: {enter_dirties:?}"
            );
            let enter0 = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert_eq!(
                btn.attrs.get("data-active").map(String::as_str),
                Some("true"),
                "scrolling away must pose Latest shown"
            );
            assert!(
                btn.style.opacity < 0.05,
                "enter t=0 must collect the hide `from`, not the opaque target, opacity={}",
                btn.style.opacity
            );
            assert!(
                enter0.text_nodes.iter().all(|t| t.text != "Latest"),
                "enter t=0 has no label ink; dest-reset must not invent a blank pill"
            );
            assert!(
                enter_dirties.iter().all(|d| d.key != btn_key),
                "enter t=0 must not dest-reset the empty pill+shadow over wrapping text, got {enter_dirties:?}"
            );

            crate::style::set_css_transition_timeline_ms(80.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_mid = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.08 && btn.style.opacity < 0.92,
                "enter must fade through a mid opacity, got {}",
                btn.style.opacity
            );
            assert!(
                enter_mid.text_nodes.iter().any(|t| t.text == "Latest"),
                "enter mid must emit Latest glyphs (not a blank fill)"
            );

            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
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
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide0 = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            assert!(
                btn.style.opacity > 0.5,
                "hide t=0 must keep the shown visual so the fade can run, opacity={}",
                btn.style.opacity
            );
            assert!(
                hide0.text_nodes.iter().any(|t| t.text == "Latest"),
                "hide t=0 must still paint Latest (lost exit was a pop to opacity 0)"
            );

            crate::style::set_css_transition_timeline_ms(280.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide_mid = find_slot(&root, "message-scroller-button")
                .expect("jump button")
                .style
                .opacity;
            assert!(
                hide_mid > 0.08 && hide_mid < 0.92,
                "hide must fade through a mid opacity, got {hide_mid}"
            );

            crate::style::set_css_transition_timeline_ms(400.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide_settle_dirties = crate::app::damage::take_frame_paint_nodes();
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity < 0.05,
                "hide settle must reach opacity 0, got {}",
                btn.style.opacity
            );
            assert!(
                hidden.text_nodes.iter().all(|t| t.text != "Latest"),
                "hide settle must drop Latest glyphs"
            );
            assert!(
                hide_settle_dirties.iter().any(|d| d.key == btn_key),
                "hide settle must dest-reset the faded ghost pill, got {hide_settle_dirties:?}"
            );
            assert!(
                hidden.text_nodes.iter().any(|t| t.text.contains("Turn ")),
                "hide settle must not wipe wrapping thread out of collect"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn message_scroller_latest_enter_halo_must_wipe_and_hide_must_restore_thread() {
    // Production CSS enter: dest-reset smaller than the toast-matched SDF left
    // every fade frame in PartialLoad (stacked ring). Overlay rail occupancy
    // then nipped a thumb remnant. Hide-settle used hidden last-present as
    // prev, skipped underlay, and left a pill-sized hole in wrapping copy.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root().bg([1.0, 1.0, 1.0, 1.0]);
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _enter0 = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);

            crate::style::set_css_transition_timeline_ms(40.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_early = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let early_present = crate::app::draw_cull::last_present_visual_slots(
                &enter_early.node_paints,
                &enter_early.instances,
                &[],
                &enter_early.text_nodes,
                &[],
            );

            crate::style::set_css_transition_timeline_ms(80.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_dirties = crate::app::damage::take_frame_paint_nodes();
            let enter_mid = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity > 0.08 && btn.style.opacity < 0.92,
                "enter mid must fade, opacity={}",
                btn.style.opacity
            );
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let mid_dirty = crate::app::damage::take_frame_paint_nodes()
                .into_iter()
                .chain(enter_dirties)
                .find(|d| d.key == btn_key)
                .expect("enter mid dest-reset dirty");
            let prev_early = early_present
                .iter()
                .find(|(k, _)| *k == btn_key)
                .map(|(_, b)| *b);
            let btn_paint = enter_mid
                .node_paints
                .iter()
                .find(|p| p.key == btn_key)
                .expect("Latest collect slot");
            let reset = crate::app::draw_cull::node_present_dest_reset_full(
                mid_dirty.key,
                mid_dirty.bounds,
                &enter_mid.node_paints,
                &enter_mid.instances,
                &[],
                &[],
                1.0,
                prev_early,
            );
            assert!(
                btn_paint.layered,
                "enter Latest is a stacking layer, layered={}",
                btn_paint.layered
            );
            let reset = reset.expect(
                "enter fade with last-present dest-resets leftover so the slide cannot ghost",
            );
            let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            assert!(
                reset.rect[2] <= pill[2] + 72.0,
                "enter fade dest-reset must stay on the pill+halo, wipe={:?} pill={pill:?}",
                reset.rect
            );
            assert!(
                crate::app::draw_cull::node_present_flush_layered_underlay_glyphs(
                    mid_dirty.key,
                    mid_dirty.bounds,
                    &enter_mid.node_paints,
                ),
                "enter fade dest-reset must recapture wrapping in the wipe"
            );
            let wipe_clip = crate::app::draw_cull::node_present_glyph_clip_logical(
                &mid_dirty,
                Some(reset.rect),
                1.0,
            );
            let wrap_runs: Vec<_> = enter_mid
                .text_nodes
                .iter()
                .filter(|t| t.text != "Latest")
                .cloned()
                .collect();
            if wrap_runs
                .iter()
                .any(|t| crate::app::draw_cull::text_hits_damage(t, reset.rect))
            {
                let underlay =
                    crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
                        &[mid_dirty],
                        &enter_mid.text_nodes,
                        &enter_mid.node_paints,
                        &[(mid_dirty.key, wipe_clip)],
                        &[],
                        1.0,
                    );
                assert!(
                    !underlay.is_empty(),
                    "enter fade dest-reset must restore wrapping in the wipe or AABB CLEAR is a white ring"
                );
                let plate = enter_mid.instances[btn_paint.instance.0..btn_paint.instance.1]
                    .iter()
                    .find(|inst| inst.color[3] > 0.5 && inst.rect_size[0] > 8.0)
                    .expect("jump chip fill");
                assert!(
                    plate.color[3] > 0.95,
                    "entrance opacity must not punch the occluding plate, or the line shows through, alpha={} opacity={}",
                    plate.color[3],
                    btn.style.opacity
                );
            }

            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let shown_present = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            );
            let shown_prev = shown_present
                .iter()
                .find(|(k, _)| *k == btn_key)
                .map(|(_, b)| *b);
            assert!(
                shown_prev.is_some_and(|b| b[2] > 8.0 && b[3] > 8.0),
                "settled Latest last-present must keep the pill+halo, got {shown_prev:?}"
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
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y.max(0.0));
            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            crate::style::set_css_transition_timeline_ms(400.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide_dirties = crate::app::damage::take_frame_paint_nodes();
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity < 0.05,
                "hide settle must reach opacity 0, got {}",
                btn.style.opacity
            );
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let hide_dirty = crate::app::damage::take_frame_paint_nodes()
                .into_iter()
                .chain(hide_dirties)
                .find(|d| d.key == btn_key)
                .expect("hide settle dest-reset dirty");
            let hide_reset = crate::app::draw_cull::node_present_dest_reset_full(
                hide_dirty.key,
                hide_dirty.bounds,
                &hidden.node_paints,
                &hidden.instances,
                &[],
                &[],
                1.0,
                shown_prev,
            )
            .expect("hide settle dest-reset");
            if let Some(bar) = find_overlay_scrollbar(&root) {
                let bar_box = bar.layout_bounds;
                assert!(
                    crate::app::damage::intersect_rects(hide_reset.rect, bar_box).is_none(),
                    "hide dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar_box:?}",
                    hide_reset.rect
                );
            }
            let overlapping: Vec<_> = hidden
                .text_nodes
                .iter()
                .filter(|t| t.text != "Latest" && t.text.contains("Turn "))
                .filter(|t| crate::app::draw_cull::text_hits_damage(t, hide_reset.rect))
                .cloned()
                .collect();
            assert!(
                !overlapping.is_empty(),
                "precondition: hide wipe must hit wrapping thread, wipe={:?}",
                hide_reset.rect
            );
            let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let wipe_below = hide_reset.rect[1] + hide_reset.rect[3] - (pill[1] + pill[3]);
            assert!(
                wipe_below <= 22.0,
                "Latest dest-reset must stay on the pill+halo, not a band below the thread\n wipe_below={wipe_below} wipe={:?} pill={pill:?}",
                hide_reset.rect
            );
            let wipe_clip = crate::app::draw_cull::node_present_glyph_clip_logical(
                &hide_dirty,
                Some(hide_reset.rect),
                1.0,
            );
            let underlay = crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
                &[hide_dirty],
                &hidden.text_nodes,
                &hidden.node_paints,
                &[(hide_dirty.key, wipe_clip)],
                &[],
                1.0,
            );
            assert!(
                !underlay.is_empty(),
                "hide settle dest-reset must recapture wrapping in the dest-reset wipe, wipe={:?} dirty={:?}",
                hide_reset.rect,
                hide_dirty.bounds
            );
            assert!(
                crate::app::draw_cull::node_present_restore_underlay(
                    hide_dirty.key,
                    hide_dirty.bounds,
                    &hidden.node_paints,
                ),
                "hide settle must restore underlay fills under the dest-reset hole"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn gallery_latest_hide_must_restore_wrapping_under_dest_reset() {
    // Gallery last turn wraps under Latest. Hide-settle dest-reset CLEARs that
    // pill hole; PartialLoad must recapture "removed from the tab order…".
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_wrap_under_jump_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            use_signal(
                &scroll_offset_y_signal_key("ms-gallery-wrap:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            settle_latest_jump_pose(&mut root);
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity > 0.5,
                "scrolled away must show Latest, opacity={}",
                btn.style.opacity
            );
            let shown_prev = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            )
            .into_iter()
            .find(|(k, _)| *k == btn_key)
            .map(|(_, b)| b);

            let max_y = use_signal(
                &scroll_content_h_signal_key("ms-gallery-wrap:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-gallery-wrap:viewport"),
                    || 0.0_f32,
                )
                .peek();
            use_signal(
                &scroll_offset_y_signal_key("ms-gallery-wrap:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y.max(0.0));
            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            crate::style::set_css_transition_timeline_ms(400.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let hide_dirties = crate::app::damage::take_frame_paint_nodes();
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            assert!(
                btn.style.opacity < 0.05,
                "hide settle must reach opacity 0, got {}",
                btn.style.opacity
            );
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let hide_dirty = crate::app::damage::take_frame_paint_nodes()
                .into_iter()
                .chain(hide_dirties)
                .find(|d| d.key == btn_key)
                .expect("hide settle dest-reset dirty");
            let hide_reset = crate::app::draw_cull::node_present_dest_reset_full(
                hide_dirty.key,
                hide_dirty.bounds,
                &hidden.node_paints,
                &hidden.instances,
                &[],
                &[],
                1.0,
                shown_prev,
            )
            .expect("hide settle dest-reset");
            let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let wipe_below = hide_reset.rect[1] + hide_reset.rect[3] - (pill[1] + pill[3]);
            let wipe_pad_x = (hide_reset.rect[2] - pill[2]).max(0.0);
            assert!(
                wipe_below <= 22.0,
                "Latest wipe must not hang a band under wrapping copy\n wipe_below={wipe_below} wipe={:?} pill={pill:?}",
                hide_reset.rect
            );
            assert!(
                wipe_pad_x <= 32.0,
                "Latest wipe must stay on the pill+halo, not expand into wrapping\n wipe_pad_x={wipe_pad_x} wipe={:?} pill={pill:?}",
                hide_reset.rect
            );
            let wipe_clip = crate::app::draw_cull::node_present_glyph_clip_logical(
                &hide_dirty,
                Some(hide_reset.rect),
                1.0,
            );
            let wrap = hidden
                .text_nodes
                .iter()
                .find(|t| t.text.contains("Got it. Stay at the"))
                .or_else(|| {
                    hidden
                        .text_nodes
                        .iter()
                        .find(|t| t.text.contains("scroll button is a real button"))
                })
                .expect("gallery wrapping copy under Latest");
            let wrap_ink = [wrap.x, wrap.y, wrap.width.max(0.0), wrap.height.max(0.0)];
            assert!(
                crate::app::damage::intersect_rects(wrap_ink, hide_reset.rect).is_some()
                    || crate::app::draw_cull::text_hits_damage(wrap, hide_reset.rect),
                "hide wipe must punch the wrapping paragraph, wipe={:?} wrap_ink={wrap_ink:?} clip={:?}",
                hide_reset.rect,
                wrap.clip_bounds
            );
            let underlay = crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
                &[hide_dirty],
                &hidden.text_nodes,
                &hidden.node_paints,
                &[(hide_dirty.key, wipe_clip)],
                &[],
                1.0,
            );
            let captured = underlay.iter().find(|t| {
                t.text.contains("Got it. Stay at the")
                    || t.text.contains("scroll button is a real button")
            });
            let wipe_cx = hide_reset.rect[0] + hide_reset.rect[2] * 0.5;
            let wipe_cy = hide_reset.rect[1] + hide_reset.rect[3] * 0.5;
            let clip_covers_wipe_center = captured.and_then(|t| t.clip_bounds).is_some_and(|c| {
                wipe_cx >= c[0]
                    && wipe_cy >= c[1]
                    && wipe_cx <= c[0] + c[2]
                    && wipe_cy <= c[1] + c[3]
            });
            let clip_stays_in_wipe = captured
                .and_then(|t| t.clip_bounds)
                .is_some_and(|c| c[2] < wrap.width - 8.0 && c[3] < hide_dirty.bounds[3] + 16.0);
            let layout_kept = captured
                .is_some_and(|t| (t.x - wrap.x).abs() < 1.0 && (t.width - wrap.width).abs() < 1.0);
            assert!(
                captured.is_some() && clip_covers_wipe_center && clip_stays_in_wipe && layout_kept,
                "hide settle must recapture wrapping in the dest-reset hole without wrap-box TextBounds\n wipe={:?} dirty={:?} wrap_ink={wrap_ink:?} clip={:?} recapture_clip={:?} underlay={} restore={}",
                hide_reset.rect,
                hide_dirty.bounds,
                wrap.clip_bounds,
                captured.and_then(|t| t.clip_bounds),
                underlay.len(),
                crate::app::draw_cull::node_present_restore_underlay(
                    hide_dirty.key,
                    hide_dirty.bounds,
                    &hidden.node_paints,
                )
            );
            assert!(
                crate::app::draw_cull::node_present_restore_underlay(
                    hide_dirty.key,
                    hide_dirty.bounds,
                    &hidden.node_paints,
                ),
                "hide settle must restore underlay fills under the dest-reset hole"
            );
            assert!(
                crate::app::draw_cull::node_present_needs_glyphs_scaled(
                    hide_dirty.key,
                    hide_dirty.bounds,
                    &hidden.node_paints,
                    &hidden.instances,
                    &[],
                    &[],
                    1.0,
                    shown_prev,
                ),
                "hide settle dest-reset must enter glyph prepare or encode CLEARs without recapture"
            );
            assert!(
                underlay.iter().all(|t| t.text != "Latest"),
                "underlay must not stamp Latest into the thread / composer"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

/// Production enter/exit dest-reset punched a large hole in Gallery Label wrap
/// ("removed from the tab order…"). The Element.text hide canary stayed green
/// because collect height / last-present / dirties differ from `<Label>`.
#[test]
#[cfg(feature = "animation-base")]
fn gallery_label_latest_enter_exit_must_not_punch_wrap() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_label_wrap_under_jump_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            let max_y0 = use_signal(
                &scroll_content_h_signal_key("ms-gallery-label-wrap:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-gallery-label-wrap:viewport"),
                    || 0.0_f32,
                )
                .peek();
            // A few px off the live edge: Latest shows and still overlaps the
            // last Label. A large delta drops that paragraph below the clip.
            use_signal(
                &scroll_offset_y_signal_key("ms-gallery-label-wrap:viewport"),
                || 0.0_f32,
            )
            .set_silent((max_y0 - 20.0).max(0.0));
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            crate::style::set_css_transition_timeline_ms(200.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            settle_latest_jump_pose(&mut root);
            let shown = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let dock = find_parent_of_slot(&root, "message-scroller-button").expect("jump dock");
            let scroller = find_slot(&root, "message-scroller").expect("scroller");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            let dock_key = u64::from(dock.layout_node.expect("dock taffy"));
            let scroller_key = u64::from(scroller.layout_node.expect("scroller taffy"));
            assert!(
                btn.style.opacity > 0.5,
                "scrolled away must show Latest, opacity={}",
                btn.style.opacity
            );
            let shown_present = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            );
            let wrap = shown
                .text_nodes
                .iter()
                .find(|t| t.text.contains("scroll button is a real button"))
                .unwrap_or_else(|| {
                    panic!(
                        "gallery Label wrap missing; texts={:?}",
                        shown
                            .text_nodes
                            .iter()
                            .map(|t| t.text.chars().take(40).collect::<String>())
                            .collect::<Vec<_>>()
                    )
                });
            let wrap_ink = [wrap.x, wrap.y, wrap.width.max(0.0), wrap.height.max(0.0)];
            let dump =
                format!(
                "wrap_ink={wrap_ink:?} clip={:?} btn_prev={:?} dock_prev={:?} scroller_prev={:?}",
                wrap.clip_bounds,
                shown_present.iter().find(|(k, _)| *k == btn_key).map(|(_, b)| *b),
                shown_present.iter().find(|(k, _)| *k == dock_key).map(|(_, b)| *b),
                shown_present
                    .iter()
                    .find(|(k, _)| *k == scroller_key)
                    .map(|(_, b)| *b),
            );
            assert!(
                wrap.height > 28.0,
                "Label leading=normal wrap must collect the full ink box, not a first-line strut\n{dump}"
            );

            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let mut enter_dirties = crate::app::damage::take_frame_paint_nodes();
            crate::app::damage::expand_paint_dirties_for_translated_slots(
                &mut enter_dirties,
                &shown_present,
                &[],
            );
            assert_latest_wipe_restores_wrap(
                "enter",
                &enter_dirties,
                &shown,
                &shown_present,
                wrap,
                btn,
                btn_key,
                dock_key,
                scroller_key,
                &dump,
            );

            crate::app::damage::clear_frame_paint_nodes();
            settle_latest_hide_pose(&mut root);
            let hidden = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            crate::app::damage::clear_frame_paint_nodes();
            crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
            let mut hide_all = crate::app::damage::take_frame_paint_nodes();
            let hidden_present = crate::app::draw_cull::last_present_visual_slots(
                &hidden.node_paints,
                &hidden.instances,
                &[],
                &hidden.text_nodes,
                &[],
            );
            crate::app::damage::expand_paint_dirties_for_translated_slots(
                &mut hide_all,
                &hidden_present,
                &shown_present,
            );
            let wrap = hidden
                .text_nodes
                .iter()
                .find(|t| t.text.contains("scroll button is a real button"))
                .expect("gallery Label wrap after hide");
            assert_latest_wipe_restores_wrap(
                "hide",
                &hide_all,
                &hidden,
                &shown_present,
                wrap,
                btn,
                btn_key,
                dock_key,
                scroller_key,
                &dump,
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[cfg(feature = "animation-base")]
fn assert_latest_wipe_restores_wrap(
    phase: &str,
    dirties: &[crate::app::damage::PaintDirty],
    result: &crate::layout::layout_tree::RenderResult,
    prev: &[(u64, [f32; 4])],
    wrap: &crate::layout::layout_tree::TextLayoutInfo,
    btn: &Element,
    _btn_key: u64,
    dock_key: u64,
    scroller_key: u64,
    dump: &str,
) {
    let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
    let wrap_ink = [wrap.x, wrap.y, wrap.width.max(0.0), wrap.height.max(0.0)];
    assert!(
        dirties
            .iter()
            .any(|d| d.kind == crate::app::damage::PaintDirtyKind::Node),
        "{phase}: Latest CSS still dirties the overlay slot, got {dirties:?}\n{dump}"
    );
    let mut saw_hide_wipe = false;
    for d in dirties
        .iter()
        .filter(|d| d.kind == crate::app::damage::PaintDirtyKind::Node)
    {
        let reset = crate::app::draw_cull::node_present_dest_reset_full(
            d.key,
            d.bounds,
            &result.node_paints,
            &result.instances,
            &[],
            &[],
            1.0,
            prev.iter().find(|(k, _)| *k == d.key).map(|(_, b)| *b),
        );
        let Some(reset) = reset else {
            continue;
        };
        let wipe = reset.rect;
        if crate::app::damage::intersect_rects(wipe, wrap_ink).is_none()
            && !crate::app::draw_cull::text_hits_damage(wrap, wipe)
        {
            continue;
        }
        assert!(
            d.key != scroller_key && d.key != dock_key,
            "{phase}: stacking-context / dock dest-reset punched wrap\n wipe={wipe:?} dirty={:?} pill={pill:?} wrap={wrap_ink:?}\n{dump}",
            d.bounds
        );
        assert!(
            wipe[2] <= pill[2] + 72.0,
            "{phase}: Latest wipe too wide for the pill\n key={} wipe={wipe:?} dirty={:?} pill={pill:?} wrap={wrap_ink:?}\n{dump}",
            d.key,
            d.bounds
        );
        let wipe_clip = crate::app::draw_cull::node_present_glyph_clip_logical(d, Some(wipe), 1.0);
        let underlay = crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
            &[*d],
            &result.text_nodes,
            &result.node_paints,
            &[(d.key, wipe_clip)],
            &[],
            1.0,
        );
        let captured = underlay
            .iter()
            .find(|t| t.text.contains("scroll button is a real button"));
        let cx = wipe[0] + wipe[2] * 0.5;
        let cy = wipe[1] + wipe[3] * 0.5;
        let area_covers = captured.is_some_and(|t| {
            let box_h = t.height.max(t.clip_bounds.map(|c| c[3]).unwrap_or(0.0));
            let box_w = t.width.max(t.clip_bounds.map(|c| c[2]).unwrap_or(0.0));
            cx >= t.x && cy >= t.y && cx <= t.x + box_w && cy <= t.y + box_h
        });
        let clip_covers = captured
            .and_then(|t| t.clip_bounds)
            .is_some_and(|c| cx >= c[0] && cy >= c[1] && cx <= c[0] + c[2] && cy <= c[1] + c[3]);
        assert!(
            captured.is_some() && (area_covers || clip_covers),
            "{phase}: dest-reset must recapture Label wrap in the wipe\n wipe={wipe:?} dirty={:?} wrap={wrap_ink:?} clip={:?} underlay={} area={area_covers} clip_hit={clip_covers}\n{dump}",
            d.bounds,
            wrap.clip_bounds,
            underlay.len()
        );
        if let Some(t) = captured {
            assert!(
                (t.x - wrap.x).abs() < 1.0 && (t.width - wrap.width).abs() < 1.0,
                "{phase}: recapture must keep wrap layout origin/width\n run=({},{}) wrap=({},{})\n{dump}",
                t.x,
                t.width,
                wrap.x,
                wrap.width
            );
            let clip = t.clip_bounds.expect("wrap recapture clip");
            assert!(
                clip[2] < wrap.width - 8.0 && clip[3] < 80.0,
                "{phase}: glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} wipe_clip={wipe_clip:?} wrap_w={}\n{dump}",
                wrap.width
            );
        }
        let late = crate::app::draw_cull::node_present_capture_text_indices(
            &[*d],
            &result.text_nodes,
            &result.node_paints,
        );
        let leaked: Vec<&str> = result
            .text_nodes
            .iter()
            .enumerate()
            .filter(|(i, t)| late.contains(i) && t.text.contains("scroll button is a real button"))
            .map(|(_, t)| t.text.as_str())
            .collect();
        assert!(
            leaked.is_empty(),
            "{phase}: post-overlay capture must not stamp wrapping onto Latest, leaked={leaked:?}\n{dump}"
        );
        saw_hide_wipe = true;
    }
    assert!(
        saw_hide_wipe,
        "{phase}: hide settle must dest-reset the pill and recapture wrap\n dirties={dirties:?} wrap={wrap_ink:?} pill={pill:?}\n{dump}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
            assert_eq!(
                btn.attrs.get("data-active").map(String::as_str),
                Some("false"),
                "follow-after-layout must retarget Latest hide at the live edge"
            );
            assert!(
                btn.style.opacity > 0.5,
                "follow hide t=0 must keep the shown visual (fade), opacity={}",
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
                hidden.text_nodes.iter().any(|t| t.text == "Latest"),
                "follow hide t=0 must still emit Latest glyphs (fade, not a pop-hide)"
            );
        });
    });
}

#[test]
#[cfg(feature = "animation-base")]
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
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
                assert_eq!(
                    btn.attrs.get("data-active").map(String::as_str),
                    Some("true"),
                    "follow-after-layout must pose Latest shown after scrolling away"
                );
                let _ = btn.tick_css_transitions(0.0);
            }
            crate::style::set_css_transition_timeline_ms(80.0);
            {
                let btn =
                    find_slot_ms_mut(&mut root, "message-scroller-button").expect("jump button");
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

#[test]
#[cfg(feature = "animation-base")]
fn red_latest_scroll_entry_exit_keeps_button_background_instance() {
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
            tree.set_collect_skip_allowed(true);
            let _ = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(0.0);
            let _ = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            crate::style::set_css_transition_timeline_ms(80.0);
            let _ = find_slot_ms_mut(&mut root, "message-scroller-button")
                .unwrap()
                .tick_css_transitions(0.0);
            let shown =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            let btn = find_slot(&root, "message-scroller-button").expect("button");
            let key = u64::from(btn.layout_node.expect("button taffy"));
            let paint = shown
                .node_paints
                .iter()
                .find(|p| p.key == key)
                .expect("button paint");
            let instances = &shown.instances[paint.instance.0..paint.instance.1];
            assert!(
                instances
                    .iter()
                    .any(|i| i.color[3] > 0.01 && i.rect_size[0] < 100.0),
                "button background must be collected: {instances:?}"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn red_latest_small_scroll_present_damage_stays_local() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = tall_message_scroller_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..5 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            let content_h = use_signal(
                &scroll_content_h_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek();
            let viewport_h = use_signal(
                &scroll_viewport_h_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek();
            let max_y = (content_h - viewport_h).max(0.0);
            let at_end = use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .peek();
            assert!(
                max_y > 0.0,
                "fixture must scroll: content={content_h} viewport={viewport_h}"
            );
            assert!(
                (at_end - max_y).abs() < 1.0,
                "fixture must seed at live edge: offset={at_end} max={max_y}"
            );

            tree.set_collect_skip_allowed(true);
            let baseline =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            let baseline_present = crate::app::draw_cull::last_present_visual_slots(
                &baseline.node_paints,
                &baseline.instances,
                &[],
                &baseline.text_nodes,
                &[],
            );
            crate::app::damage::clear_frame_paint_nodes();
            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent((max_y - 16.0).max(0.0));
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            let btn_key = {
                let btn = find_slot(&root, "message-scroller-button").expect("button");
                u64::from(btn.layout_node.expect("button taffy"))
            };
            let viewport_key = u64::from(
                find_slot(&root, "message-scroller-viewport")
                    .unwrap()
                    .layout_node
                    .unwrap(),
            );
            let shown =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let entry_dirties = crate::app::damage::take_frame_paint_nodes();
            let paints = shown.node_paints.clone();
            let shown_present = crate::app::draw_cull::last_present_visual_slots(
                &shown.node_paints,
                &shown.instances,
                &[],
                &shown.text_nodes,
                &[],
            );
            let mut expanded_entry = entry_dirties.clone();
            crate::app::damage::expand_paint_dirties_for_translated_slots(
                &mut expanded_entry,
                &shown_present,
                &baseline_present,
            );
            assert!(
                !paints
                    .iter()
                    .any(|p| p.key == btn_key && p.bounds[2] > 300.0),
                "button paint must stay pill-sized: {paints:?}"
            );
            assert!(
                !entry_dirties.iter().any(|d| d.key == viewport_key),
                "small scroll must not dirty the full viewport: {entry_dirties:?}"
            );
            for d in expanded_entry.iter().filter(|d| d.key == btn_key) {
                if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &shown.node_paints,
                    &shown.instances,
                    &[],
                    &[],
                    1.0,
                    baseline_present
                        .iter()
                        .find(|(k, _)| *k == btn_key)
                        .map(|(_, b)| *b),
                ) {
                    assert!(
                        reset.rect[2] < 160.0 && reset.rect[3] < 96.0,
                        "button reset must stay local: dirty={d:?} reset={reset:?}"
                    );
                }
            }

            // Keep the first visible animation frame in the same retained/present
            // path as production. The bug only showed up after the button had
            // non-zero opacity, so t=0 alone cannot validate the reset boundary.
            crate::style::set_css_transition_timeline_ms(80.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            let mid = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let mid_dirties = crate::app::damage::take_frame_paint_nodes();
            let mid_button = find_slot(&root, "message-scroller-button").expect("button");
            assert!(mid_button.style.opacity > 0.08 && mid_button.style.opacity < 0.92);
            let mid_reset = mid_dirties
                .iter()
                .filter(|d| d.key == btn_key)
                .filter_map(|d| {
                    crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &mid.node_paints,
                        &mid.instances,
                        &[],
                        &[],
                        1.0,
                        baseline_present
                            .iter()
                            .find(|(k, _)| *k == btn_key)
                            .map(|(_, b)| *b),
                    )
                })
                .collect::<Vec<_>>();
            assert!(
                !mid_dirties.iter().any(|d| d.key == viewport_key),
                "mid enter must not dirty viewport: {mid_dirties:?}"
            );
            assert!(
                mid_reset
                    .iter()
                    .all(|r| r.rect[2] < 160.0 && r.rect[3] < 96.0),
                "mid enter reset must stay local: {mid_reset:?}"
            );

            use_signal(
                &scroll_offset_y_signal_key("ms-layout-test:viewport"),
                || 0.0_f32,
            )
            .set_silent(max_y);
            crate::style::set_css_transition_timeline_ms(200.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            let hide =
                tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, true);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let exit_dirties = crate::app::damage::take_frame_paint_nodes();
            let exit_opacity = find_slot(&root, "message-scroller-button")
                .expect("button")
                .style
                .opacity;
            let exit_reset = exit_dirties
                .iter()
                .filter(|d| d.key == btn_key)
                .filter_map(|d| {
                    crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &hide.node_paints,
                        &hide.instances,
                        &[],
                        &[],
                        1.0,
                        baseline_present
                            .iter()
                            .find(|(k, _)| *k == btn_key)
                            .map(|(_, b)| *b),
                    )
                })
                .collect::<Vec<_>>();
            assert!(
                exit_opacity > 0.08 && exit_opacity < 0.92,
                "exit must still be a fade frame: {exit_opacity}"
            );
            assert!(
                !exit_dirties.iter().any(|d| d.key == viewport_key),
                "exit must not dirty viewport: {exit_dirties:?}"
            );
            assert!(
                exit_reset
                    .iter()
                    .all(|r| r.rect[2] < 160.0 && r.rect[3] < 96.0),
                "exit reset must stay local: {exit_reset:?}"
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
                dirty.bounds,
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
                let clip = n.clip_bounds.expect(
                    "underlay recapture must clip wrapping messages to the dest-reset hole",
                );
                assert!(
                    crate::app::damage::intersect_rects(clip, hole).is_some(),
                    "wrapping restore must overlap the dest-reset hole\n clip={clip:?} hole={hole:?} run=({},{},{},{})",
                    n.x,
                    n.y,
                    n.width,
                    n.height
                );
                assert!(
                    clip[2] < n.width - 8.0 && clip[3] < 80.0,
                    "glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} hole={hole:?} run=({},{},{},{})",
                    n.x,
                    n.y,
                    n.width,
                    n.height
                );
            }
            if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                &result.node_paints,
                &result.instances,
                &[],
                &[],
                1.0,
                Some(dirty.bounds),
            ) {
                let pill = btn.layout_bounds;
                let pill_bottom = pill[1] + pill[3];
                assert!(
                    reset.rect[1] + reset.rect[3] >= pill_bottom + 8.0,
                    "Latest dest-reset must wipe the outset halo or PartialLoad stacks the fade\n wipe={:?} pill={pill:?}",
                    reset.rect
                );
                assert!(
                    reset.rect[2] < scroller.layout_bounds[2] * 0.7,
                    "hover dest-reset must stay on the pill+halo, not the 100% dock\n wipe={:?} pill={pill:?}",
                    reset.rect
                );
                let halo_runs: Vec<_> = overlapping
                    .iter()
                    .filter(|(_, t)| crate::app::draw_cull::text_hits_damage(t, reset.rect))
                    .filter(|(i, _)| !late.contains(i))
                    .collect();
                if !halo_runs.is_empty() {
                    assert!(
                        !underlay.is_empty(),
                        "dest-reset of the halo must restore wrapping glyphs in that hole"
                    );
                }
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
            let vp_el = find_slot(&root, "message-scroller-viewport").expect("viewport");
            let vp = vp_el.layout_bounds;
            let vp_id = vp_el
                .focus_id
                .as_deref()
                .and_then(|id| {
                    id.strip_prefix(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
                })
                .unwrap_or("ms-layout-test:viewport")
                .to_string();
            let btn_vis = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let pins = crate::app::damage::out_of_flow_scroll_blit_pins(&root, &[vp_id], &[vp]);
            assert!(
                pins.iter()
                    .any(|p| crate::app::damage::intersect_rects(*p, btn_vis)
                        .is_some_and(|h| h[2] > 8.0 && h[3] > 8.0)),
                "active Latest must pin every scroll blit, got {pins:?} btn={btn_vis:?}"
            );
            assert!(
                pins.iter()
                    .all(|p| crate::app::damage::intersect_rects(*p, bar_box).is_none()),
                "Latest scroll-blit pin must stay off the overlay rail, pins={pins:?} bar={bar_box:?}"
            );
            let smeared = crate::app::damage::smear_out_of_flow_after_strip_blit(
                &pins,
                &[vp],
                &[[0.0, 48.0]],
            );
            assert!(
                smeared.iter().any(|p| p[3] > 0.5 && p[1] + 1.0 < btn_vis[1]),
                "strip blit must dest-reset Latest occupancy after the copy, smeared={smeared:?} btn={btn_vis:?}"
            );
        });
    });
}

fn find_overlay_scrollbar(el: &Element) -> Option<&Element> {
    if el.attrs.get("jadix_scrollbar_overlay").map(String::as_str) == Some("1") {
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
            let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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
            if let Some(hidden_dock) = hidden_present
                .iter()
                .find(|(k, _)| *k == dock_key)
                .map(|(_, b)| *b)
            {
                let shown_dock = shown_present
                    .iter()
                    .find(|(k, _)| *k == dock_key)
                    .map(|(_, b)| *b)
                    .expect("dock last-present while shown");
                assert!(
                    hidden_dock[3] <= pill[3] + 8.0 && shown_dock[3] <= pill[3] + 8.0,
                    "dock last-present must not union the pill shadow/scale\n{dump}"
                );
                assert!(
                    !crate::app::damage::node_present_slot_moved(hidden_dock, shown_dock),
                    "Latest show must not look like a translated dock\n{dump}"
                );
            }
            assert!(
                !dirties.iter().any(|d| d.key == dock_key),
                "full-width dock must not dest-reset on Latest show\n{dump}"
            );
            let max_w = pill[2] + 72.0;
            let jump_dirties: Vec<_> = dirties
                .iter()
                .filter(|d| d.key == btn_key || d.key == dock_key)
                .cloned()
                .collect();
            assert!(
                !jump_dirties.is_empty(),
                "Latest show still dirties the overlay slot (src-over redraw)\n{dump}"
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
                let prev = hidden_present
                    .iter()
                    .find(|(k, _)| *k == d.key)
                    .map(|(_, b)| *b);
                let reset = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &shown.node_paints,
                    &shown.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                );
                if let Some(reset) = reset {
                    assert!(
                        reset.rect[2] <= max_w,
                        "shown Latest dest-reset must stay on the pill, not a wrapping band\n wipe={:?} dirty={:?}\n{dump}",
                        reset.rect,
                        d.bounds
                    );
                    assert!(
                        crate::app::damage::intersect_rects(reset.rect, bar_box).is_none(),
                        "shown Latest dest-reset must not punch the overlay scrollbar\n wipe={:?}\n{dump}",
                        reset.rect
                    );
                    assert!(
                        crate::app::draw_cull::node_present_flush_layered_underlay_glyphs(
                            d.key,
                            d.bounds,
                            &shown.node_paints,
                        ),
                        "leftover dest-reset must recapture wrapping in the wipe\n{dump}"
                    );
                } else {
                    assert!(
                        prev.is_none_or(|b| b[2] <= 0.5 || b[3] <= 0.5),
                        "first visible Latest src-overs wrapping; leftover last-present must dest-reset\n prev={prev:?} dirty={:?}\n{dump}",
                        d.bounds
                    );
                }
                assert!(
                    scissor[2] <= max_w,
                    "Latest scissor must stay on the pill, not a wrapping band\n scissor={scissor:?} dirty={:?}\n{dump}",
                    d.bounds
                );
                assert!(
                    crate::app::damage::intersect_rects(scissor, bar_box).is_none(),
                    "scissor must not punch the overlay scrollbar\n scissor={scissor:?}\n{dump}"
                );
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
            let _ = tree.build_and_layout(&mut root, 400.0, 800.0, [-1.0, -1.0]);
            settle_latest_jump_pose(&mut root);
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

/// Gallery MessageScrollerShowcase used to graft a `jadix_fragment` (default row flex)
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
                    frag.set_attr("jadix_fragment", "1");
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
            frag.set_attr("jadix_fragment", "1");
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
    frag.set_attr("jadix_fragment", "1");
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
                                        children: Some(ms_nav_btn(
                                            "gal-workspace",
                                            "Jadix Gallery",
                                        )),
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
                                        children: Some(ms_nav_btn(
                                            "gal-workspace",
                                            "Jadix Gallery",
                                        )),
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

/// A wrap line that crosses the pill is painted on top of Latest when its
/// glyphon submission is the chip fill's band or any later one (rects, then
/// every run in that band) or when node-present flushes it after the plate.
fn wrap_lines_drawn_with_jump_plate(
    root: &Element,
    scene: &crate::layout::layout_tree::RenderResult,
    viewport_id: &str,
) -> (String, Vec<String>) {
    let btn = find_slot(root, "message-scroller-button").expect("jump button");
    let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
    let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
    let ranges = scene.overlay_split.page_stack_band_ranges();
    let btn_paint = scene
        .node_paints
        .iter()
        .find(|p| p.key == btn_key)
        .expect("button slot");
    let fill_idx = scene.instances[btn_paint.instance.0..btn_paint.instance.1]
        .iter()
        .enumerate()
        .find(|(_, inst)| inst.color[3] > 0.5 && inst.rect_size[0] > 8.0)
        .map(|(i, _)| btn_paint.instance.0 + i);
    let fill_band = fill_idx.and_then(|idx| {
        ranges
            .iter()
            .position(|r| r.instance.0 <= idx && idx < r.instance.1)
    });
    // Draw uses paint ops, not the instance-index band. A `Rects` run that
    // starts before the dock band and is extended across it paints the chip
    // fill, then that same band's glyphon (the wrap line) lands on Latest.
    let fill_op = fill_idx.and_then(|idx| {
        scene.paint_ops.iter().position(|op| match op {
            crate::layout::layout_tree::PaintOp::Rects { start, count } => {
                let s = *start as usize;
                s <= idx && idx < s + *count as usize
            }
            _ => false,
        })
    });
    let op_band = fill_op.and_then(|oi| {
        ranges
            .iter()
            .position(|r| r.paint_ops.0 <= oi && oi < r.paint_ops.1)
    });
    let draw_band = op_band.or(fill_band);
    let mut report = format!(
        "viewport={viewport_id} opacity={} pill={pill:?} fill_idx={fill_idx:?} fill_band={fill_band:?} fill_op={fill_op:?} op_band={op_band:?} bands={} btn_text={:?}\n",
        btn.style.opacity,
        ranges.len(),
        btn_paint.text
    );
    for (bi, r) in ranges.iter().enumerate() {
        let snippets: Vec<_> = scene.text_nodes[r.text.0..r.text.1]
            .iter()
            .map(|t| {
                let s = t.text.chars().take(20).collect::<String>();
                format!("{s}@({:.0},{:.0})", t.x, t.y)
            })
            .collect();
        report.push_str(&format!(
            "  band {bi} inst {:?} text {:?} :: {snippets:?}\n",
            r.instance, r.text
        ));
    }
    let mut bleed = Vec::new();
    let mut hits = 0u32;
    for (ti, t) in scene.text_nodes.iter().enumerate() {
        if t.text == "Latest" || t.text.chars().count() < 8 {
            continue;
        }
        let Some(prepared) = t.prepared.as_ref() else {
            continue;
        };
        let lines = crate::text::measure::layout_lines(prepared, t.width.max(1.0));
        let lh = prepared.line_height.max(1.0);
        let mut hits_pill = false;
        for (li, line) in lines.iter().enumerate() {
            let line_box = [t.x, t.y + li as f32 * lh, line.width.max(1.0), lh];
            if crate::app::damage::intersect_rects(line_box, pill).is_some() {
                hits_pill = true;
                hits += 1;
                report.push_str(&format!(
                    "  line hit ti={ti} li={li} {:?} box={line_box:?}\n",
                    line.text.chars().take(32).collect::<String>()
                ));
            }
        }
        if !hits_pill {
            continue;
        }
        let text_band = ranges.iter().position(|r| r.text.0 <= ti && ti < r.text.1);
        report.push_str(&format!(
            "  wrap ti={ti} band={text_band:?} y={:.1} h={:.1} clip={:?}\n",
            t.y, t.height, t.clip_bounds
        ));
        if draw_band.is_none() || text_band.is_none_or(|b| draw_band.is_some_and(|f| b >= f)) {
            bleed.push(format!(
                "wrap band {text_band:?} is not strictly before the chip draw band {draw_band:?} (instance {fill_band:?}, paint-op {op_band:?})"
            ));
        }
        let plate = crate::app::draw_cull::opaque_slot_fill_aabb(
            btn_key,
            pill,
            &scene.node_paints,
            &scene.instances,
        );
        report.push_str(&format!("  plate={plate:?}\n"));
        for (li, line) in lines.iter().enumerate() {
            let line_box = [t.x, t.y + li as f32 * lh, line.width.max(1.0), lh];
            let Some(hit) = crate::app::damage::intersect_rects(line_box, pill) else {
                continue;
            };
            let cx = hit[0] + hit[2] * 0.5;
            let cy = hit[1] + hit[3] * 0.5;
            let covered = plate.is_some_and(|p| {
                cx >= p[0] && cy >= p[1] && cx <= p[0] + p[2] && cy <= p[1] + p[3]
            });
            if !covered {
                bleed.push(format!(
                    "line center ({cx:.1},{cy:.1}) is inside the pill but not the opaque fill {plate:?}"
                ));
            }
        }
        if btn_paint.text.0 <= ti && ti < btn_paint.text.1 {
            bleed.push(format!("wrap ti={ti} is inside the button slot text range"));
        }
    }
    if hits == 0 {
        bleed.push("no wrap line crosses the pill".into());
    }
    crate::app::damage::clear_frame_paint_nodes();
    crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
    let dirties = crate::app::damage::take_frame_paint_nodes();
    let hole = crate::app::draw_cull::node_present_clipped_capture_text_wipes(
        &dirties,
        &scene.text_nodes,
        &scene.node_paints,
        &[],
    );
    for t in &hole {
        if t.text == "Latest" || t.text.chars().count() < 8 {
            continue;
        }
        let crosses = t
            .clip_bounds
            .is_none_or(|c| crate::app::damage::intersect_rects(c, pill).is_some());
        if crosses {
            bleed.push(format!(
                "post-pill capture {}",
                t.text.chars().take(24).collect::<String>()
            ));
        }
    }
    report.push_str(&format!(
        "  hole={:?}\n",
        hole.iter()
            .map(|t| t.text.chars().take(20).collect::<String>())
            .collect::<Vec<_>>()
    ));
    (report, bleed)
}

/// Gallery nests MessageScroller under a page ScrollArea. The flat tree keeps
/// the wrap line in an earlier band; this is the tree the screenshots come
/// from, with the pill at rest.
#[test]
fn message_scroller_nested_label_wrap_stays_under_settled_pill() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            {
                let mut root = gallery_like_message_scroller_with_jump();
                let mut tree = LayoutTree::new();
                let mut impact = LayoutStatus::Clean;
                const W: f32 = 400.0;
                const H: f32 = 800.0;
                for _ in 0..4 {
                    sync_reactive_elements_subtree(
                        &mut root,
                        &mut tree,
                        &mut impact,
                        W,
                        H,
                        [0.0, 0.0],
                    );
                    crate::style::sync_class_variants_subtree(
                        &mut root,
                        [-1.0e9, -1.0e9],
                        "",
                        false,
                        W,
                        H,
                        &mut tree,
                        &mut impact,
                    );
                    let _ = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                }
                let max_y = use_signal(
                    &scroll_content_h_signal_key("ms-gallery-jump:viewport"),
                    || 0.0_f32,
                )
                .peek()
                    - use_signal(
                        &scroll_viewport_h_signal_key("ms-gallery-jump:viewport"),
                        || 0.0_f32,
                    )
                    .peek();
                use_signal(
                    &scroll_offset_y_signal_key("ms-gallery-jump:viewport"),
                    || 0.0_f32,
                )
                .set_silent((max_y - 48.0).max(0.0));
                sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, W, H, [0.0, 0.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                settle_latest_jump_pose(&mut root);
                let scene = tree.build_and_layout(&mut root, W, H, [-1.0, -1.0]);
                let (report, bleed) =
                    wrap_lines_drawn_with_jump_plate(&root, &scene, "ms-gallery-jump:viewport");
                assert!(
                    report.contains("plate="),
                    "geometry report missing the opaque fill\n{report}"
                );
                assert!(
                    bleed.is_empty(),
                    "settled nested gallery draws the wrap line with the chip\n{bleed:?}\n{report}"
                );
            }
            {
                let mut root = gallery_label_wrap_under_jump_root();
                let mut tree = LayoutTree::new();
                let mut impact = LayoutStatus::Clean;
                for _ in 0..4 {
                    production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                    let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                }
                let max_y = use_signal(
                    &scroll_content_h_signal_key("ms-gallery-label-wrap:viewport"),
                    || 0.0_f32,
                )
                .peek()
                    - use_signal(
                        &scroll_viewport_h_signal_key("ms-gallery-label-wrap:viewport"),
                        || 0.0_f32,
                    )
                    .peek();
                use_signal(
                    &scroll_offset_y_signal_key("ms-gallery-label-wrap:viewport"),
                    || 0.0_f32,
                )
                .set_silent((max_y - 48.0).max(0.0));
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                settle_latest_jump_pose(&mut root);
                let scene = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                let (report, bleed) = wrap_lines_drawn_with_jump_plate(
                    &root,
                    &scene,
                    "ms-gallery-label-wrap:viewport",
                );
                assert!(
                    bleed.is_empty(),
                    "settled label wrap draws the line with the chip\n{bleed:?}\n{report}"
                );
            }
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

fn line_crosses_plate(t: &crate::layout::layout_tree::TextLayoutInfo, plate: [f32; 4]) -> bool {
    if t.text == "Latest" || t.text.chars().count() < 8 {
        return false;
    }
    let Some(prepared) = t.prepared.as_ref() else {
        return false;
    };
    let lines = crate::text::measure::layout_lines(prepared, t.width.max(1.0));
    let lh = prepared.line_height.max(1.0);
    lines.iter().enumerate().any(|(li, line)| {
        let line_box = [t.x, t.y + li as f32 * lh, line.width.max(1.0), lh];
        crate::app::damage::intersect_rects(line_box, plate).is_some()
    })
}

fn plate_channel_delta(
    a: &crate::renderer::offscreen::RgbaImage,
    b: &crate::renderer::offscreen::RgbaImage,
    plate: [f32; 4],
) -> u32 {
    let x0 = (plate[0] + 1.0).ceil() as i32;
    let y0 = (plate[1] + 1.0).ceil() as i32;
    let x1 = (plate[0] + plate[2] - 1.0).floor() as i32;
    let y1 = (plate[1] + plate[3] - 1.0).floor() as i32;
    if x1 <= x0 || y1 <= y0 {
        return 0;
    }
    let mut n = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let (Some(pa), Some(pb)) = (a.pixel(x as u32, y as u32), b.pixel(x as u32, y as u32))
            else {
                continue;
            };
            if pa.iter().zip(pb).any(|(c, d)| c.abs_diff(d) > 30) {
                n += 1;
            }
        }
    }
    n
}

/// Band-order composite (rects, then that band's glyphs) versus the same
/// composite with the crossing paragraph omitted. A difference inside the
/// opaque plate means the wrap line is visible on the chip.
fn draw_pill_instance_range<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a crate::renderer::JadixPipeline,
    buffers: &'a [wgpu::Buffer],
    draws: &'a [(usize, u32)],
    lo: usize,
    hi: usize,
) {
    for &(buf_i, count) in &draws[lo..hi] {
        pipeline.bind_sdf(pass, false, pipeline.dummy_backdrop());
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, buffers[buf_i].slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, 0..count);
    }
}

fn jump_plate_band_images(
    gpu: &crate::renderer::offscreen::OffscreenGpu,
    scene: &crate::layout::layout_tree::RenderResult,
    plate: [f32; 4],
    omit_crossing: bool,
    rects_then_text: bool,
) -> crate::renderer::offscreen::RgbaImage {
    use crate::layout::layout_tree::PaintOp;
    use crate::renderer::text::JadixTextRenderer;
    use wgpu::util::DeviceExt;

    const W: u32 = 320;
    const H: u32 = 384;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: W,
        height: H,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = crate::renderer::JadixPipeline::from_format(&gpu.device, format, W, H);
    pipeline.update_resolution(&gpu.queue, W, H);
    let bands = scene.overlay_split.page_stack_band_ranges();
    let mut renderers = Vec::new();
    for band in &bands {
        let mut renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
        let text: Vec<_> = scene.text_nodes[band.text.0..band.text.1]
            .iter()
            .filter(|t| !(omit_crossing && line_crosses_plate(t, plate)))
            .cloned()
            .collect();
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut renderer,
            &gpu.device,
            &gpu.queue,
            W,
            H,
            &text,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        renderers.push(renderer);
    }
    let mut buffers: Vec<wgpu::Buffer> = Vec::new();
    let mut draws: Vec<(usize, u32)> = Vec::new();
    let push_slice = |buffers: &mut Vec<wgpu::Buffer>,
                      draws: &mut Vec<(usize, u32)>,
                      instances: &[crate::renderer::InstanceInput]| {
        if instances.is_empty() {
            return;
        }
        let buf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pill-band-instances"),
                contents: bytemuck::cast_slice(instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let count = instances.len() as u32;
        buffers.push(buf);
        draws.push((buffers.len() - 1, count));
    };
    let mut band_draw_end = Vec::with_capacity(bands.len());
    for band in &bands {
        if band.paint_ops.0 < band.paint_ops.1 {
            for op in &scene.paint_ops[band.paint_ops.0..band.paint_ops.1] {
                if let PaintOp::Rects { start, count } = *op {
                    if count == 0 {
                        continue;
                    }
                    let s = start as usize;
                    let e = s + count as usize;
                    push_slice(&mut buffers, &mut draws, &scene.instances[s..e]);
                }
            }
        } else if band.instance.0 < band.instance.1 {
            push_slice(
                &mut buffers,
                &mut draws,
                &scene.instances[band.instance.0..band.instance.1],
            );
        }
        band_draw_end.push(draws.len());
    }
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("latest-pill-composite"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("latest pill composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if rects_then_text {
            draw_pill_instance_range(&mut pass, &pipeline, &buffers, &draws, 0, draws.len());
            for renderer in &renderers {
                renderer.render(&mut pass);
            }
        } else {
            let mut lo = 0usize;
            for (end, renderer) in band_draw_end.iter().zip(&renderers) {
                draw_pill_instance_range(&mut pass, &pipeline, &buffers, &draws, lo, *end);
                renderer.render(&mut pass);
                lo = *end;
            }
        }
    }
    crate::renderer::offscreen::readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder)
        .expect("pill composite readback")
}

fn plate_then_glyphs(
    gpu: &crate::renderer::offscreen::OffscreenGpu,
    scene: &crate::layout::layout_tree::RenderResult,
    btn_key: u64,
    text: &[crate::layout::layout_tree::TextLayoutInfo],
) -> crate::renderer::offscreen::RgbaImage {
    use crate::renderer::text::JadixTextRenderer;
    use wgpu::util::DeviceExt;

    const W: u32 = 320;
    const H: u32 = 384;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: W,
        height: H,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = crate::renderer::JadixPipeline::from_format(&gpu.device, format, W, H);
    pipeline.update_resolution(&gpu.queue, W, H);
    let slot = scene
        .node_paints
        .iter()
        .find(|p| p.key == btn_key)
        .expect("button paint");
    let instances = &scene.instances[slot.instance.0..slot.instance.1];
    let buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pill-slot-instances"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let draws = [(0usize, instances.len() as u32)];
    let buffers = [buf];
    let mut renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    crate::renderer::prepare_text::prepare_text_frame_at(
        &mut renderer,
        &gpu.device,
        &gpu.queue,
        W,
        H,
        text,
        1.0,
        false,
        &Default::default(),
        &Default::default(),
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pill-then-glyphs"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("plate then glyphs"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw_pill_instance_range(&mut pass, &pipeline, &buffers, &draws, 0, draws.len());
        renderer.render(&mut pass);
    }
    crate::renderer::offscreen::readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder)
        .expect("plate-then-glyphs readback")
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn message_scroller_latest_pill_pixels_hide_wrapping_line() {
    crate::style::set_css_transition_virtual_timeline(true);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let Some(gpu) = crate::renderer::offscreen::OffscreenGpu::new() else {
                eprintln!("no GPU adapter; pill pixel check skipped");
                return;
            };
            for settle in [false, true] {
                crate::style::set_css_transition_timeline_ms(0.0);
                let mut root = gallery_wrap_under_jump_root();
                let mut tree = LayoutTree::new();
                let mut impact = LayoutStatus::Clean;
                for _ in 0..4 {
                    production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                    let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                }
                let max_y = use_signal(
                    &scroll_content_h_signal_key("ms-gallery-wrap:viewport"),
                    || 0.0_f32,
                )
                .peek()
                    - use_signal(
                        &scroll_viewport_h_signal_key("ms-gallery-wrap:viewport"),
                        || 0.0_f32,
                    )
                    .peek();
                use_signal(
                    &scroll_offset_y_signal_key("ms-gallery-wrap:viewport"),
                    || 0.0_f32,
                )
                .set_silent((max_y - 48.0).max(0.0));
                if settle {
                    production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                    settle_latest_jump_pose(&mut root);
                } else {
                    crate::style::set_css_transition_timeline_ms(0.0);
                    production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                    let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                    crate::style::set_css_transition_timeline_ms(80.0);
                    production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                    sync_message_scroller_follow_after_layout(&mut root, &mut tree);
                }
                let scene = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                let btn = find_slot(&root, "message-scroller-button").expect("jump button");
                let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
                let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
                let plate = crate::app::draw_cull::opaque_slot_fill_aabb(
                    btn_key,
                    pill,
                    &scene.node_paints,
                    &scene.instances,
                )
                .expect("opaque plate");
                let crossing: Vec<String> = scene
                    .text_nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| line_crosses_plate(t, plate))
                    .map(|(i, t)| {
                        format!(
                            "ti={i} y={:.1} h={:.1} clip={:?} {:?}",
                            t.y,
                            t.height,
                            t.clip_bounds,
                            t.text.chars().take(40).collect::<String>()
                        )
                    })
                    .collect();
                assert!(
                    !crossing.is_empty(),
                    "settle={settle} wrap line must cross the opaque plate or the pixel check is vacuous"
                );
                let bare = jump_plate_band_images(&gpu, &scene, plate, true, false);
                let ordered = jump_plate_band_images(&gpu, &scene, plate, false, false);
                let rects_then_text = jump_plate_band_images(&gpu, &scene, plate, false, true);
                let sanity = plate_channel_delta(&rects_then_text, &bare, plate);
                let band_delta = plate_channel_delta(&ordered, &bare, plate);
                let dark = |img: &crate::renderer::offscreen::RgbaImage| {
                    img.pixels
                        .chunks_exact(4)
                        .filter(|p| p[0] < 80 && p[1] < 80 && p[2] < 80)
                        .count()
                };
                assert!(
                    sanity > 8,
                    "settle={settle} drawing every glyph after every rect did not change the plate interior (plate={plate:?} sanity={sanity} dark rects={} bare={} ordered={} crossing={crossing:?}). The wrap line is not in this composite."
                    , dark(&rects_then_text), dark(&bare), dark(&ordered)
                );
                assert!(
                    band_delta <= 8,
                    "settle={settle} page-stack band order leaves the wrap line on the chip (delta={band_delta} plate={plate:?})"
                );
                crate::app::damage::clear_frame_paint_nodes();
                crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
                let mut dirties = crate::app::damage::take_frame_paint_nodes();
                if let Some(vp) = find_slot(&root, "message-scroller-viewport") {
                    if let Some(key) = vp.layout_node.map(u64::from) {
                        dirties.push(crate::app::damage::PaintDirty {
                            key,
                            bounds: crate::core::tree::visual_paint_aabb(vp, vp.paint_scroll_acc),
                            kind: crate::app::damage::PaintDirtyKind::Region,
                        });
                    }
                }
                crate::app::draw_cull::prepare_node_present_dirties_ex(
                    &mut dirties,
                    &scene.node_paints,
                    &[],
                    &scene.instances,
                    &[],
                    &[],
                    1.0,
                    None,
                    &[],
                );
                let wipes: Vec<(u64, [f32; 4])> = dirties
                    .iter()
                    .filter_map(|d| {
                        let reset = crate::app::draw_cull::node_present_dest_reset_full(
                            d.key,
                            d.bounds,
                            &scene.node_paints,
                            &scene.instances,
                            &[],
                            &[],
                            1.0,
                            None,
                        )?;
                        Some((
                            d.key,
                            crate::app::draw_cull::node_present_glyph_clip_logical(
                                d,
                                Some(reset.rect),
                                1.0,
                            ),
                        ))
                    })
                    .collect();
                let mut flush =
                    crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
                        &dirties,
                        &scene.text_nodes,
                        &scene.node_paints,
                        &wipes,
                        &scene.instances,
                        1.0,
                    );
                flush.extend(
                    crate::app::draw_cull::node_present_clipped_capture_text_wipes_excluding_overlay(
                        &dirties,
                        &scene.text_nodes,
                        &scene.node_paints,
                        &wipes,
                        &[],
                        &scene.instances,
                        1.0,
                    ),
                );
                // The hole flush owns "Latest". Only foreign runs (the wrapping
                // paragraph) are a bug when they change pixels inside the plate.
                let foreign: Vec<_> = flush
                    .iter()
                    .filter(|t| t.text.chars().count() > 16)
                    .cloned()
                    .collect();
                let own: Vec<_> = flush
                    .iter()
                    .filter(|t| t.text.chars().count() <= 16)
                    .cloned()
                    .collect();
                let with_own = plate_then_glyphs(&gpu, &scene, btn_key, &own);
                let with_foreign = plate_then_glyphs(&gpu, &scene, btn_key, &flush);
                let flush_delta = plate_channel_delta(&with_foreign, &with_own, plate);
                let foreign_dbg: Vec<String> = foreign
                    .iter()
                    .map(|t| {
                        format!(
                            "clip={:?} {:?}",
                            t.clip_bounds,
                            t.text.chars().take(28).collect::<String>()
                        )
                    })
                    .collect();
                assert!(
                    flush_delta <= 8,
                    "settle={settle} foreign glyphs after the plate draw the wrap line (delta={flush_delta} plate={plate:?} foreign={foreign_dbg:?})"
                );
                // Scroll repair is a viewport Region with no button Node dirty.
                // That replay draws the plate, then flushes every glyph in the
                // viewport — the path that leaves the line on Latest after enter.
                let region_only: Vec<_> = dirties
                    .iter()
                    .filter(|d| d.kind == crate::app::damage::PaintDirtyKind::Region)
                    .copied()
                    .collect();
                let region_hole =
                    crate::app::draw_cull::node_present_clipped_capture_text_wipes_excluding_overlay(
                        &region_only,
                        &scene.text_nodes,
                        &scene.node_paints,
                        &[],
                        &[],
                        &scene.instances,
                        1.0,
                    );
                let region_foreign: Vec<_> = region_hole
                    .iter()
                    .filter(|t| t.text.chars().count() > 16)
                    .cloned()
                    .collect();
                let region_own: Vec<_> = region_hole
                    .iter()
                    .filter(|t| t.text.chars().count() <= 16)
                    .cloned()
                    .collect();
                let region_with_own = plate_then_glyphs(&gpu, &scene, btn_key, &region_own);
                let region_with_foreign = plate_then_glyphs(&gpu, &scene, btn_key, &region_hole);
                let region_delta =
                    plate_channel_delta(&region_with_foreign, &region_with_own, plate);
                let region_dbg: Vec<String> = region_foreign
                    .iter()
                    .map(|t| {
                        format!(
                            "clip={:?} {:?}",
                            t.clip_bounds,
                            t.text.chars().take(28).collect::<String>()
                        )
                    })
                    .collect();
                assert!(
                    region_delta <= 8,
                    "settle={settle} viewport Region flush draws the wrap line on the plate (delta={region_delta} plate={plate:?} foreign={region_dbg:?})"
                );
            }
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}

/// Wrapping line that crosses the jump pill must be painted in an earlier
/// page-stack band than the chip fill, and must not be in the post-pill glyph
/// capture. Same-band glyphon (rects then every run) and a post-pill flush both
/// draw the line on top of the opaque chip.
#[test]
fn message_scroller_latest_pill_must_not_share_band_with_wrapping_line() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = gallery_wrap_under_jump_root();
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            for _ in 0..4 {
                production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
                let _ = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
                sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            }
            let max_y = use_signal(
                &scroll_content_h_signal_key("ms-gallery-wrap:viewport"),
                || 0.0_f32,
            )
            .peek()
                - use_signal(
                    &scroll_viewport_h_signal_key("ms-gallery-wrap:viewport"),
                    || 0.0_f32,
                )
                .peek();
            // Just off the live edge: the last wrap line still crosses the pill.
            use_signal(
                &scroll_offset_y_signal_key("ms-gallery-wrap:viewport"),
                || 0.0_f32,
            )
            .set_silent((max_y - 48.0).max(0.0));
            crate::style::set_css_transition_timeline_ms(0.0);
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_early = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let early_present = crate::app::draw_cull::last_present_visual_slots(
                &enter_early.node_paints,
                &enter_early.instances,
                &[],
                &enter_early.text_nodes,
                &[],
            );
            crate::style::set_css_transition_timeline_ms(80.0);
            crate::app::damage::clear_frame_paint_nodes();
            production_jump_chrome_pass(&mut root, &mut tree, &mut impact);
            sync_message_scroller_follow_after_layout(&mut root, &mut tree);
            let enter_dirties = crate::app::damage::take_frame_paint_nodes();
            let enter_mid = tree.build_and_layout(&mut root, 320.0, 384.0, [-1.0, -1.0]);
            let btn = find_slot(&root, "message-scroller-button").expect("jump button");
            let btn_key = u64::from(btn.layout_node.expect("jump taffy"));
            let pill = crate::core::tree::visual_paint_aabb(btn, btn.paint_scroll_acc);
            let ranges = enter_mid.overlay_split.page_stack_band_ranges();
            let btn_paint = enter_mid
                .node_paints
                .iter()
                .find(|p| p.key == btn_key)
                .expect("button slot");
            let fill_idx = enter_mid.instances[btn_paint.instance.0..btn_paint.instance.1]
                .iter()
                .enumerate()
                .find(|(_, inst)| inst.color[3] > 0.5 && inst.rect_size[0] > 8.0)
                .map(|(i, _)| btn_paint.instance.0 + i)
                .expect("opaque chip fill");
            let fill_band = ranges
                .iter()
                .position(|r| r.instance.0 <= fill_idx && fill_idx < r.instance.1)
                .expect("fill band");
            let mut report = format!(
                "opacity={} pill={pill:?} fill_idx={fill_idx} fill_band={fill_band} bands={}\n",
                btn.style.opacity,
                ranges.len()
            );
            for (bi, r) in ranges.iter().enumerate() {
                let snippets: Vec<_> = enter_mid.text_nodes[r.text.0..r.text.1]
                    .iter()
                    .map(|t| {
                        let s = t.text.chars().take(24).collect::<String>();
                        format!("{s}@({:.0},{:.0})", t.x, t.y)
                    })
                    .collect();
                report.push_str(&format!(
                    "  band {bi} inst {:?} text {:?} :: {snippets:?}\n",
                    r.instance, r.text
                ));
            }
            let mut bleed = Vec::new();
            for (ti, t) in enter_mid.text_nodes.iter().enumerate() {
                if t.text == "Latest" || t.text.chars().count() < 8 {
                    continue;
                }
                let Some(prepared) = t.prepared.as_ref() else {
                    continue;
                };
                let lines = crate::text::measure::layout_lines(prepared, t.width.max(1.0));
                let lh = prepared.line_height.max(1.0);
                let mut hits_pill = false;
                for (li, line) in lines.iter().enumerate() {
                    let line_box = [t.x, t.y + li as f32 * lh, line.width.max(1.0), lh];
                    if crate::app::damage::intersect_rects(line_box, pill).is_some() {
                        hits_pill = true;
                        report.push_str(&format!(
                            "  line hit ti={ti} li={li} {:?} box={line_box:?}\n",
                            line.text
                        ));
                    }
                }
                if !hits_pill {
                    continue;
                }
                let text_band = ranges.iter().position(|r| r.text.0 <= ti && ti < r.text.1);
                report.push_str(&format!(
                    "  wrap ti={ti} band={text_band:?} y={:.1} h={:.1} clip={:?} text={:?}\n",
                    t.y,
                    t.height,
                    t.clip_bounds,
                    t.text.chars().take(48).collect::<String>()
                ));
                if text_band.is_some_and(|b| b >= fill_band) {
                    bleed.push(format!("same-or-later band {text_band:?}"));
                }
            }
            let prev_early = early_present
                .iter()
                .find(|(k, _)| *k == btn_key)
                .map(|(_, b)| *b);
            let mut dirties = enter_dirties;
            if !dirties.iter().any(|d| d.key == btn_key) {
                crate::app::damage::clear_frame_paint_nodes();
                crate::app::damage::push_frame_paint_element(btn, btn.paint_scroll_acc);
                dirties.extend(crate::app::damage::take_frame_paint_nodes());
            }
            report.push_str(&format!("  dirties={dirties:?}\n  prev={prev_early:?}\n"));
            let wipes: Vec<(u64, [f32; 4])> = dirties
                .iter()
                .filter_map(|d| {
                    let reset = crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &enter_mid.node_paints,
                        &enter_mid.instances,
                        &[],
                        &[],
                        1.0,
                        if d.key == btn_key { prev_early } else { None },
                    )?;
                    Some((
                        d.key,
                        crate::app::draw_cull::node_present_glyph_clip_logical(
                            d,
                            Some(reset.rect),
                            1.0,
                        ),
                    ))
                })
                .collect();
            let hole = crate::app::draw_cull::node_present_clipped_capture_text_wipes(
                &dirties,
                &enter_mid.text_nodes,
                &enter_mid.node_paints,
                &wipes,
            );
            let under = crate::app::draw_cull::node_present_layered_underlay_capture_text_wipes(
                &dirties,
                &enter_mid.text_nodes,
                &enter_mid.node_paints,
                &wipes,
                &enter_mid.instances,
                1.0,
            );
            report.push_str(&format!(
                "  wipes={wipes:?}\n  hole={:?}\n  underlay={:?}\n",
                hole.iter()
                    .map(|t| {
                        format!(
                            "{} clip={:?}",
                            t.text.chars().take(28).collect::<String>(),
                            t.clip_bounds
                        )
                    })
                    .collect::<Vec<_>>(),
                under
                    .iter()
                    .map(|t| {
                        format!(
                            "{} clip={:?}",
                            t.text.chars().take(28).collect::<String>(),
                            t.clip_bounds
                        )
                    })
                    .collect::<Vec<_>>()
            ));
            for t in &hole {
                if t.text != "Latest" && t.text.chars().count() > 8 {
                    let crosses = t
                        .clip_bounds
                        .is_some_and(|c| crate::app::damage::intersect_rects(c, pill).is_some());
                    if crosses {
                        bleed.push(format!(
                            "post-pill capture {}",
                            t.text.chars().take(24).collect::<String>()
                        ));
                    }
                }
            }
            let plate = dirties.iter().find_map(|d| {
                crate::app::draw_cull::opaque_slot_fill_aabb(
                    d.key,
                    d.bounds,
                    &enter_mid.node_paints,
                    &enter_mid.instances,
                )
            });
            let plate = plate.expect("opaque jump plate");
            report.push_str(&format!("  plate={plate:?}\n"));
            for t in &under {
                if let Some(clip) = t.clip_bounds {
                    if crate::app::damage::intersect_rects(clip, plate).is_some() {
                        bleed.push(format!(
                            "underlay clip covers the plate {}",
                            t.text.chars().take(24).collect::<String>()
                        ));
                    }
                }
            }
            // Scroll damage is a viewport Region plus the button. Region replay
            // draws rects, then the shared glyph flush runs after the plate.
            // A wrap line in that flush is the thread on Latest.
            let mut escalated = dirties.clone();
            if let Some(vp) = find_slot(&root, "message-scroller-viewport") {
                if let Some(key) = vp.layout_node.map(u64::from) {
                    escalated.push(crate::app::damage::PaintDirty {
                        key,
                        bounds: crate::core::tree::visual_paint_aabb(vp, vp.paint_scroll_acc),
                        kind: crate::app::damage::PaintDirtyKind::Region,
                    });
                }
            }
            crate::app::draw_cull::prepare_node_present_dirties_ex(
                &mut escalated,
                &enter_mid.node_paints,
                &early_present,
                &enter_mid.instances,
                &[],
                &[],
                1.0,
                None,
                &[],
            );
            report.push_str(&format!("  escalated={escalated:?}\n"));
            let esc_wipes: Vec<(u64, [f32; 4])> = escalated
                .iter()
                .filter_map(|d| {
                    let reset = crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &enter_mid.node_paints,
                        &enter_mid.instances,
                        &[],
                        &[],
                        1.0,
                        if d.key == btn_key { prev_early } else { None },
                    )?;
                    Some((
                        d.key,
                        crate::app::draw_cull::node_present_glyph_clip_logical(
                            d,
                            Some(reset.rect),
                            1.0,
                        ),
                    ))
                })
                .collect();
            let esc_hole =
                crate::app::draw_cull::node_present_clipped_capture_text_wipes_excluding_overlay(
                    &escalated,
                    &enter_mid.text_nodes,
                    &enter_mid.node_paints,
                    &esc_wipes,
                    &[],
                    &enter_mid.instances,
                    1.0,
                );
            report.push_str(&format!(
                "  esc_hole={:?}\n",
                esc_hole
                    .iter()
                    .map(|t| {
                        format!(
                            "{} clip={:?}",
                            t.text.chars().take(28).collect::<String>(),
                            t.clip_bounds
                        )
                    })
                    .collect::<Vec<_>>()
            ));
            for t in &esc_hole {
                if t.text == "Latest" || t.text.chars().count() <= 8 {
                    continue;
                }
                let hits_plate = t
                    .clip_bounds
                    .is_none_or(|c| crate::app::damage::intersect_rects(c, plate).is_some());
                if hits_plate {
                    bleed.push(format!(
                        "post-escalation flush covers the plate {}",
                        t.text.chars().take(24).collect::<String>()
                    ));
                }
            }
            assert!(
                report.contains("line hit"),
                "precondition: a wrap line must cross the pill or the check is vacuous\n{report}"
            );
            assert!(
                bleed.is_empty(),
                "wrapping line is drawn with the chip\n{bleed:?}\n{report}"
            );
        });
    });
    crate::style::set_css_transition_virtual_timeline(false);
}
