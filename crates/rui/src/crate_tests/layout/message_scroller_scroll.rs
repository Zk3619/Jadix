//! MessageScroller: `defaultScrollPosition=end` must paint the live edge, not an empty
//! tail from `min-h-full` / inflated `content_h`.

use crate::components::bubble::bubble;
use crate::components::message::{message, message_content};
use crate::components::message_scroller::{
    message_scroller, message_scroller_content, message_scroller_item, message_scroller_provider,
    message_scroller_viewport,
};
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::scroll_viewport::scroll_offset_y_signal_key;
use crate::core::tree::{sync_reactive_elements_subtree, Element, LayoutStatus, LayoutTree};

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
        content = content.child(
            message_scroller_item()
                .message_id(format!("m{i}"))
                .child(
                    Element::new()
                        .flex_col()
                        .w("100%")
                        .min_w(0.0)
                        .child(
                            Element::new()
                                .w("100%")
                                .text(format!(
                                    "Turn {i}: enough text to wrap across multiple lines in a narrow chat column."
                                )),
                        ),
                ),
        );
    }
    let scroller: Element = message_scroller_provider()
        .id("ms-layout-test")
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller()
                .h("100%")
                .child(message_scroller_viewport().child(content)),
        )
        .into();
    Element::new()
        .w(320.0)
        .h(384.0)
        .overflow("hidden")
        .child(scroller)
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
                .child(
                    message()
                        .align("end")
                        .child(message_content().child(bubble().variant("muted").align("end").child(body))),
                )
        } else {
            message_scroller_item()
                .message_id(format!("m{i}"))
                .child(
                    message()
                        .align("start")
                        .child(message_content().child(bubble().variant("ghost").align("start").child(body))),
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
        .child(Element::new().h(400.0).w("100%").bg([0.95, 0.95, 0.95, 1.0]))
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
    let sy = use_signal(&scroll_offset_y_signal_key("ms-gallery-like:viewport"), || 0.0_f32).peek();
    let metrics = result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "ms-gallery-like:viewport")
        .map(|(_, _, max, ch)| (max[1], ch[1]))
        .unwrap_or((0.0, 0.0));
    let ys: Vec<f32> = result.text_nodes.iter().map(|t| t.y).collect();
    (sy, metrics.0, metrics.1, result.text_nodes.len(), ys)
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
                last_sy =
                    crate::core::reactivity::use_signal(&scroll_offset_y_signal_key("ms-layout-test:viewport"), || 0.0_f32)
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
            let result = tree.build_and_layout_fast(&mut root, 320.0, 384.0, [-1.0, -1.0], true, false);
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key("ms-layout-test:viewport"), || 0.0_f32)
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
        });
    });
}
