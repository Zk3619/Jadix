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

#[test]
fn icon_button_lucide_glyph_stays_centered() {
    use crate::utils::build_icon_text_node;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    // Lucide private-use glyphs — any single codepoint is enough for layout centering.
    let glyph = "\u{e02a}";
    let mut root = Element::new().w(200.0).h(80.0).child(Element::from(
        button("icon-center")
            .size("icon")
            .child(build_icon_text_node(glyph)),
    ));
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);

    fn find_btn(el: &Element) -> Option<&Element> {
        if el.template_id.as_deref() == Some("icon-center") {
            return Some(el);
        }
        el.children.iter().find_map(find_btn)
    }
    let btn = find_btn(&root).expect("icon button");
    assert!(
        btn.children
            .iter()
            .any(|c| c.text_style.white_space != crate::style::WhiteSpace::Nowrap),
        "lucide caption must not get whitespace-nowrap"
    );
    let icon = btn
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("icon leaf");
    let bx = btn.layout_bounds;
    let ix = icon.layout_bounds;
    let btn_cx = bx[0] + bx[2] * 0.5;
    let btn_cy = bx[1] + bx[3] * 0.5;
    let icon_cx = ix[0] + ix[2] * 0.5;
    let icon_cy = ix[1] + ix[3] * 0.5;
    assert!(
        (icon_cx - btn_cx).abs() < 1.5,
        "icon must be horizontally centered: btn_cx={btn_cx:.3} icon_cx={icon_cx:.3} btn={bx:?} icon={ix:?}"
    );
    assert!(
        (icon_cy - btn_cy).abs() < 1.5,
        "icon must be vertically centered: btn_cy={btn_cy:.3} icon_cy={icon_cy:.3}"
    );
}
