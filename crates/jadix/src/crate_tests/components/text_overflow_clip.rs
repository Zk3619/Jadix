//! Gallery Input showcase: `text-overflow: clip` on a leading-none Label must
//! crop the line horizontally without shaving CJK ink against the 1em strut.

use crate::components::{Label, LabelProps};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;

#[test]
fn text_overflow_clip_keeps_cjk_ink_outside_leading_none_strut() {
    let el = Label(LabelProps {
        id: "gal-s2-clip".into(),
        text: Some(String::new()),
        center: Some(false),
        for_target: None,
        truncate: None,
        text_overflow: Some("clip".into()),
        children: Some(Element::new().text("clip：超长单行会被裁切不会出现省略号")),
        base: Some(Box::new(|b| b.w("100%"))),
    });
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(el.style.overflow.y, taffy::style::Overflow::Visible);

    let mut tree = LayoutTree::new();
    // Gallery: HStack w=10rem wrapping a VStack of clip/ellipsis/newline Labels.
    let mut root = Element::new()
        .w(160.0)
        .h(200.0)
        .flex_col()
        .flex_items("start")
        .child(Element::new().w(160.0).h(24.0))
        .child(el);
    let result = tree.build_and_layout(&mut root, 160.0, 200.0, [-1.0, -1.0]);
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.text.contains("超长单行"))
        .expect("clip caption");
    let clip = info.clip_bounds.expect("text-overflow clip must scissor");
    assert!(
        (clip[2] - info.width).abs() < 1.5,
        "inline clip must match the leaf width; clip_w={} leaf_w={}",
        clip[2],
        info.width
    );
    assert!(
        clip[1] < info.y - 1.0 && clip[1] + clip[3] > info.y + info.height + 1.0,
        "block axis must not scissor to leading-none; leaf=({:.1},{:.1}) clip=({:.1},{:.1},{:.1},{:.1})",
        info.y,
        info.height,
        clip[1],
        clip[3],
        clip[0],
        clip[2]
    );
}
