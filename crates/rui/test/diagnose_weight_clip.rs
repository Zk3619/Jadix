//! Regression: layout measure must use the same face weight as prepare_text.

use crate::components::button::button;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::hstack;
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::text::{install_shared_font_system, intrinsic_width, prepare};
use crate::utils::font::normalize_weight_for_family;

#[test]
fn measure_uses_normalized_weight_so_captions_fit() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let family = best_font_family(&fs);
    let raw = 500_u16;
    let norm = normalize_weight_for_family(fs.db(), family, raw);
    assert_ne!(
        norm, raw,
        "precondition: Segoe-like stacks should remap 500 → nearest face (often 600)"
    );

    let labels = ["Ghost", "Secondary", "Outline", "Link", "Destructive", "Disabled"];
    for text in labels {
        let prepared = prepare(text, 14.0, raw, 21.0);
        let measure_w = intrinsic_width(&prepared);

        let mut buf = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
        buf.set_size(&mut fs, Some(16_384.0), Some(23.0));
        buf.set_text(
            &mut fs,
            text,
            glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(norm)),
            glyphon::Shaping::Basic,
        );
        buf.shape_until_scroll(&mut fs, false);
        let render_w = buf.layout_runs().map(|r| r.line_w).fold(0.0_f32, f32::max);

        assert!(
            measure_w + 0.05 >= render_w,
            "`{text}`: measure {measure_w:.3} must not undersize render@{norm} {render_w:.3}"
        );

        let mut buf2 = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
        buf2.set_size(&mut fs, Some(measure_w.max(1.0)), Some(84.0));
        buf2.set_text(
            &mut fs,
            text,
            glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(norm)),
            glyphon::Shaping::Basic,
        );
        buf2.shape_until_scroll(&mut fs, false);
        assert_eq!(
            buf2.layout_runs().count(),
            1,
            "`{text}` must stay one line at measured width {measure_w:.3}"
        );
    }

    let mut row = hstack().w("100%").gap("0.5rem").wrap("wrap").items_center();
    for (i, text) in labels.iter().enumerate() {
        row = row.child(Element::from(
            button(format!("w-{i}")).child(Element::new().text(*text)),
        ));
    }
    let mut root = Element::new().w(900.0).h(120.0).child(Element::from(row));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 900.0, 120.0, [-1.0, -1.0]);
    for n in &result.text_nodes {
        let prepared = n.prepared.as_ref().expect("prepared");
        let metrics = crate::text::layout(prepared, n.width.max(1.0));
        assert_eq!(
            metrics.line_count, 1,
            "`{}` wrapped after layout at box_w={:.3}",
            n.text, n.width
        );
    }
}
