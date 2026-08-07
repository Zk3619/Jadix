//! Regression: button captions stay single-line after layout.
//!
//! Guards two measure/render mismatches that clipped trailing glyphs:
//! 1. Taffy integer rounding (disabled in `LayoutTree::new`)
//! 2. Layout measure using raw weight 500 / `Family::SansSerif` while paint uses
//!    normalized Semibold + `best_font_family`

use crate::components::button::button;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::hstack;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

#[test]
fn button_captions_stay_single_line_after_layout() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let labels = [
        "Ghost",
        "Secondary",
        "Outline",
        "Destructive",
        "Link",
        "Disabled",
        "Default (0)",
    ];

    let mut row = hstack().w("100%").gap("0.5rem").wrap("wrap").items_center();
    for (i, text) in labels.iter().enumerate() {
        row = row.child(Element::from(
            button(format!("cap-{i}")).child(Element::new().text(*text)),
        ));
    }
    let mut root = Element::new().w(800.0).h(200.0).child(Element::from(row));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 200.0, [-1.0, -1.0]);

    assert_eq!(result.text_nodes.len(), labels.len());
    for n in &result.text_nodes {
        let prepared = n.prepared.as_ref().expect("text leaf has prepared");
        let intrinsic = crate::text::intrinsic_width(prepared);
        let metrics = crate::text::layout(prepared, n.width.max(1.0));
        assert!(
            n.width + 0.05 >= intrinsic,
            "`{}`: layout box {:.3} must cover intrinsic {:.3}",
            n.text,
            n.width,
            intrinsic
        );
        assert_eq!(
            metrics.line_count, 1,
            "`{}` wrapped into {} lines at box_w={:.3} (intrinsic={:.3})",
            n.text, metrics.line_count, n.width, intrinsic
        );
    }
}
