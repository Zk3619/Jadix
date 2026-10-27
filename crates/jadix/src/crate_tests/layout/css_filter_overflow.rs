//! CSS `filter` must capture the full SourceGraphic, then clip the *output*
//! to ancestor overflow — not drop the wrap (or pre-clip the source) when the
//! box straddles the viewport.

use crate::core::tree::{Element, LayoutTree};
use crate::layout::RenderResult;
use crate::renderer::{take_capture_jobs, take_filter_jobs};

fn drain_filter_queues() {
    let _ = take_capture_jobs();
    let _ = take_filter_jobs();
}

fn filter_draw(result: &RenderResult) -> Option<&crate::renderer::TexturedDraw> {
    result
        .textured_draws
        .iter()
        .find(|d| d.texture_key.starts_with("jadix_filter:"))
}

#[test]
fn css_filter_offscreen_capture_preserves_preceding_text() {
    for offset in [-240.0, -80.0, 0.0, 120.0, 240.0] {
        drain_filter_queues();
        let mut root = Element::new()
            .w(300.0)
            .h(100.0)
            .flex_row()
            .child(Element::new().w(100.0).h(100.0).text("sidebar caption"))
            .child(
                Element::new()
                    .w(200.0)
                    .h(100.0)
                    .overflow("hidden")
                    .position("relative")
                    .child(
                        Element::new()
                            .absolute()
                            .left(20.0)
                            .top(offset)
                            .w(80.0)
                            .h(80.0)
                            .filter("blur(4px)")
                            .bg([1.0, 0.0, 0.0, 1.0]),
                    ),
            );
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0; 2]);
        assert!(
            result.text_nodes.iter().any(|text| text.text == "sidebar caption"),
            "filter capture at offset {offset} removed preceding text"
        );
        for paint in &result.node_paints {
            assert!(paint.text.0 <= paint.text.1 && paint.text.1 <= result.text_nodes.len());
        }
        drain_filter_queues();
    }
}

#[test]
fn css_filter_scroll_collect_keeps_node_paint_ranges_valid() {
    drain_filter_queues();
    let mut root = Element::new()
        .w(100.0)
        .h(100.0)
        .flex_col()
        .overflow("hidden")
        .child(
            Element::new()
                .w(80.0)
                .h(20.0)
                .bg([0.0, 0.0, 1.0, 1.0])
                .text("before"),
        )
        .child(
            Element::new()
                .w(80.0)
                .h(60.0)
                .filter("blur(4px)")
                .bg([1.0, 0.0, 0.0, 1.0])
                .child(Element::new().w(40.0).h(40.0).bg([0.0, 1.0, 0.0, 1.0])),
        );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 100.0, 100.0, [-1.0, -1.0]);
    drain_filter_queues();
    tree.set_collect_skip_allowed(true);
    for offset in [0.0, 10.0, 100.0, 0.0] {
        root.scroll_offset[1] = offset;
        let result = tree.build_and_layout_fast(&mut root, 100.0, 100.0, [-1.0, -1.0], true, false);
        assert!(
            filter_draw(&result).is_some(),
            "filter output must remain visible"
        );
        for paint in &result.node_paints {
            for (range, len) in [
                (paint.instance, result.instances.len()),
                (paint.textured, result.textured_draws.len()),
                (paint.text, result.text_nodes.len()),
                (paint.chrome, result.input_chrome_instances.len()),
                (paint.paint_ops, result.paint_ops.len()),
            ] {
                assert!(
                    range.0 <= range.1 && range.1 <= len,
                    "invalid range {range:?}, len={len}"
                );
            }
        }
        drain_filter_queues();
    }
}

/// Half of a filtered box sits left of an `overflow: hidden` clip.
#[test]
fn css_filter_partially_offscreen_still_composites() {
    drain_filter_queues();
    let mut root = Element::new()
        .w(100.0)
        .h(100.0)
        .overflow("hidden")
        .position("relative")
        .child(
            Element::new()
                .absolute()
                .left(-40.0)
                .top(10.0)
                .w(80.0)
                .h(80.0)
                .filter("blur(4px)")
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 100.0, 100.0, [-1.0, -1.0]);
    let draw = filter_draw(&result).expect(
        "partially clipped CSS filter must still composite (overflow hidden), not skip the wrap",
    );
    assert!(
        draw.rect[0] >= -0.5 && draw.rect[0] < 1.0,
        "output must be clipped to the viewport left edge, got x={}",
        draw.rect[0]
    );
    assert!(
        draw.rect[2] > 30.0 && draw.rect[2] < 90.0,
        "output width must be the visible sliver (~40px), got w={}",
        draw.rect[2]
    );

    let caps = take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_filter_cap:"))
        .expect("filter capture job");
    assert!(
        cap.instances
            .iter()
            .any(|i| i.rect_size[0] > 70.0 && i.rect_pos[0] < -1.0),
        "SourceGraphic must include the off-screen half, not a pre-clipped sliver: {:?}",
        cap.instances
            .iter()
            .map(|i| (i.rect_pos, i.rect_size))
            .collect::<Vec<_>>()
    );
    let _ = take_filter_jobs();
}

/// Filter wrapper has no fill; only a child paint exists, and that child lies
/// fully left of the clip (a few pixels of padding still peek in). Skipping the
/// wrap here is the gallery "filter vanished at the screen edge" bug.
#[test]
fn css_filter_padding_peek_still_captures_offscreen_child() {
    drain_filter_queues();
    let mut root = Element::new()
        .w(100.0)
        .h(100.0)
        .overflow("hidden")
        .position("relative")
        .child(
            Element::new()
                .absolute()
                .left(-75.0)
                .top(10.0)
                .w(80.0)
                .h(80.0)
                .p(8.0)
                .filter("blur(4px)")
                .child(
                    Element::new()
                        .w(64.0)
                        .h(64.0)
                        .flex_shrink(0.0)
                        .bg([0.0, 1.0, 0.0, 1.0]),
                ),
        );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 100.0, 100.0, [-1.0, -1.0]);
    let draw = filter_draw(&result).expect(
        "a filter whose SourceGraphic child is fully outside the clip must still composite the clipped output",
    );
    assert!(
        draw.rect[2] > 1.0 && draw.rect[3] > 1.0,
        "overflow hidden must keep the visible sliver, got {:?}",
        draw.rect
    );
    let caps = take_capture_jobs();
    let cap = caps
        .iter()
        .find(|c| c.key.starts_with("jadix_filter_cap:"))
        .expect("filter capture job");
    assert!(
        cap.instances.iter().any(|i| i.rect_pos[0] < -1.0),
        "SourceGraphic must include the off-screen child, not skip it: {:?}",
        cap.instances
            .iter()
            .map(|i| (i.rect_pos, i.rect_size))
            .collect::<Vec<_>>()
    );
    let _ = take_filter_jobs();
}
