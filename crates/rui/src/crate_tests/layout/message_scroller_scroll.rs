//! MessageScroller: `defaultScrollPosition=end` must paint the live edge, not an empty
//! tail from `min-h-full` / inflated `content_h`.

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
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::scroll_viewport::scroll_offset_y_signal_key;
use crate::core::tree::{sync_reactive_elements_subtree, Element, LayoutStatus, LayoutTree};
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
            assert!(
                btn.style.opacity > 0.5,
                "not-at-end must show the jump button, opacity={}",
                btn.style.opacity
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
