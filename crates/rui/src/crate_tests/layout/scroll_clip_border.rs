//! Regression: overflow clip must not bake into rounded-rect geometry.
//!
//! Intersecting `render_rect` into the scroll viewport before drawing made card top/bottom
//! borders stick to the viewport edge while scrolling. Geometry stays full-size; clip is a
//! fragment discard (stored on [`InstanceInput::clip_*`]).

use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::InstanceInput;

#[test]
fn scrolled_card_border_keeps_true_top_not_viewport_edge() {
    // Parent is a 200×200 overflow clip viewport. Child card is 120 tall at y=0 in content
    // space; scroll_offset_y=40 moves it to render y=-40 (above the clip). The visible
    // intersection top is 0 — the old bug drew the border on that intersection.
    let mut root = Element::new()
        .w(200.0)
        .h(200.0)
        .bg([0.95, 0.95, 0.95, 1.0])
        .overflow_y("hidden")
        .overflow_x("hidden")
        .child(
            Element::new()
                .w(180.0)
                .h(120.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .border([0.2, 0.2, 0.2, 1.0], 1.0)
                .rounded(8.0)
                .template_id("scroll-card"),
        );
    root.scroll_offset = [0.0, 40.0];

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);

    let card_inst = result
        .instances
        .iter()
        .find(|i| i.border_width > 0.5 && (i.rect_size[1] - 120.0).abs() < 1.0 && i.color[0] > 0.9)
        .or_else(|| {
            result
                .instances
                .iter()
                .find(|i| i.border_width > 0.5 && (i.rect_size[1] - 120.0).abs() < 1.0)
        })
        .expect("card instance with border");

    assert!(
        card_inst.rect_pos[1] < -1.0,
        "card geometry top must stay above viewport (scrolled out), got y={}",
        card_inst.rect_pos[1]
    );
    assert!(
        card_inst.clip_size[0] >= 0.0 && card_inst.clip_size[1] >= 0.0,
        "overflow clip must be set on instance, got clip_size={:?}",
        card_inst.clip_size
    );
    assert!(
        (card_inst.rect_pos[1] - card_inst.clip_pos[1]).abs() > 5.0,
        "geometry top {:.3} must not equal clip top {:.3} (sticky border bug)",
        card_inst.rect_pos[1],
        card_inst.clip_pos[1]
    );
    let _ = InstanceInput::NO_CLIP_SIZE;
}
