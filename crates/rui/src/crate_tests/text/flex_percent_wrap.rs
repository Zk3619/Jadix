//! `<Box>` is flex-row: N children with `w=100%` become N× parent width.
//! Gallery inherit demo must use `VStack` so mixed-language samples wrap.

use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::{r#box, vstack};

fn long_label() -> Element {
    Element::new()
        .text("The quick brown fox 中文字重 Ag 0123")
        .text_size(14.0)
        .w("100%")
        .min_w("0")
}

#[test]
fn box_row_percent_children_exceed_parent_width() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::from(
                r#box()
                    .w(320.0)
                    .p(8.0)
                    .child(long_label())
                    .child(long_label())
                    .child(long_label()),
            );
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 320.0, 400.0, [-1.0, -1.0]);
            let span: f32 = root.children.iter().map(|c| c.layout_bounds[2]).sum();
            assert!(
                span > 320.0 * 1.5,
                "row Box + three w=100% labels should sit side-by-side (got span {span})"
            );
        });
    });
}

#[test]
fn vstack_percent_children_stay_inside_parent_width() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut root = Element::from(
                vstack()
                    .w(320.0)
                    .min_w("0")
                    .p(8.0)
                    .child(long_label())
                    .child(long_label())
                    .child(long_label()),
            );
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 320.0, 400.0, [-1.0, -1.0]);
            for (i, child) in root.children.iter().enumerate() {
                let w = child.layout_bounds[2];
                assert!(
                    w <= 320.0 + 1.0,
                    "VStack child {i} width {w} must not exceed parent 320"
                );
            }
        });
    });
}
