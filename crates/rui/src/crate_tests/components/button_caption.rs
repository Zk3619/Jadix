//! Button caption measure / layout: single-line after layout + weight match.
//!
//! Guards measure/render mismatches that clipped trailing glyphs:
//! 1. Taffy integer rounding (disabled in `LayoutTree::new`)
//! 2. Layout measure using the same family/weight match as paint (`resolve_face_weight`)

use crate::components::button::button;
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::hstack;
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::text::{install_shared_font_system, intrinsic_width, prepare};
use crate::utils::font::resolve_face_weight;

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

#[test]
fn measure_uses_normalized_weight_so_captions_fit() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let family = best_font_family(&fs);
    let raw = 500_u16;
    let resolved = resolve_face_weight(fs.db(), family, raw);
    let norm = resolved.attrs_weight;

    let labels = [
        "Ghost",
        "Secondary",
        "Outline",
        "Link",
        "Destructive",
        "Disabled",
    ];
    for text in labels {
        let prepared = prepare(text, 14.0, raw, 21.0);
        let measure_w = intrinsic_width(&prepared);

        let mut buf = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
        buf.set_size(Some(16_384.0), Some(23.0));
        buf.set_text(
            text,
            &glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(norm)),
            glyphon::Shaping::Basic,
            None,
        );
        buf.shape_until_scroll(&mut fs, false);
        let render_w = buf.layout_runs().map(|r| r.line_w).fold(0.0_f32, f32::max);

        assert!(
            measure_w + 0.05 >= render_w,
            "`{text}`: measure {measure_w:.3} must not undersize render@{norm} {render_w:.3}"
        );

        let mut buf2 = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
        buf2.set_size(Some(measure_w.max(1.0)), Some(84.0));
        buf2.set_text(
            text,
            &glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(norm)),
            glyphon::Shaping::Basic,
            None,
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

/// Mixed captions must stay one paint line whose box covers glyphon ink.
///
/// Measure shapes CJK per character (Advanced) and Latin as a Basic run, then
/// sums them. Paint shapes the whole string Advanced. If the sum is short, the
/// button box ends near the script boundary: hover dest-reset updates 关闭 and
/// leaves "Builder Trigger" on the previous frame's pixels.
#[test]
fn mixed_cjk_latin_caption_box_covers_advanced_ink() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let label = "关闭 Builder Trigger";
    let mut root = Element::new().w(800.0).h(160.0).child(Element::from(
        button("mix-cap")
            .variant("outline")
            .size("sm")
            .child(Element::new().text(label)),
    ));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 160.0, [-1.0, -1.0]);
    assert_eq!(result.text_nodes.len(), 1, "one caption leaf");
    let n = &result.text_nodes[0];
    assert_eq!(n.text, label);
    assert_eq!(
        n.style.white_space,
        crate::style::WhiteSpace::Nowrap,
        "button captions are nowrap"
    );

    let prepared = n.prepared.as_ref().expect("prepared");
    let intrinsic = intrinsic_width(prepared);
    let metrics = crate::text::layout(prepared, n.width.max(1.0));
    assert_eq!(metrics.line_count, 1, "layout wrapped the mixed caption");

    let family = best_font_family(&fs);
    let norm = resolve_face_weight(fs.db(), family, 500).attrs_weight;
    let mut buf = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    buf.set_size(Some(16_384.0), None);
    buf.set_wrap(glyphon::Wrap::None);
    buf.set_text(
        label,
        &glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(norm)),
        glyphon::Shaping::Advanced,
        None,
    );
    buf.shape_until_scroll(&mut fs, false);
    let runs = buf.layout_runs().count();
    let mut ink = 0.0_f32;
    let mut cjk_end = 0.0_f32;
    let mut latin_start = f32::MAX;
    for run in buf.layout_runs() {
        for g in run.glyphs.iter() {
            ink = ink.max(g.x + g.w);
            let ch = label[g.start..g.end].chars().next().unwrap_or('\0');
            if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
                cjk_end = cjk_end.max(g.x + g.w);
            } else if ch.is_ascii_alphabetic() {
                latin_start = latin_start.min(g.x);
            }
        }
    }
    assert_eq!(runs, 1, "Advanced nowrap paint must be one line");
    assert!(
        cjk_end > 0.0 && latin_start.is_finite() && cjk_end <= latin_start + 8.0,
        "CJK and Latin must share one line (cjk_end={cjk_end:.2} latin_start={latin_start:.2})"
    );
    assert!(
        n.width + 0.5 >= ink,
        "caption box {:.2} must cover Advanced ink {:.2} (intrinsic {:.2}); otherwise hover clears only the CJK side",
        n.width,
        ink,
        intrinsic
    );
}

/// Rapid toggle swaps `打开` / `关闭` and the Latin suffix must not be a second
/// paint region. StyleChanged records only the current box. If that box misses
/// the previous suffix and the slot is not treated as moved, Load keeps the
/// old "Trigger" pixels for a frame.
#[test]
fn toggle_prefix_clear_covers_previous_suffix() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    fn placed(label: &str) -> (crate::core::tree::Element, [f32; 4], [f32; 4], f32, f32) {
        let mut root = Element::new().w(800.0).h(160.0).child(Element::from(
            button("mix-cap")
                .variant("outline")
                .size("sm")
                .child(Element::new().text(label)),
        ));
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 160.0, [-1.0, -1.0]);
        let n = result
            .text_nodes
            .iter()
            .find(|n| n.text == label)
            .expect("caption");
        let button = root.children[0].layout_bounds;
        let text_box = [n.x, n.y, n.width, n.height];
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        let family = best_font_family(&fs);
        let norm = resolve_face_weight(fs.db(), family, 500).attrs_weight;
        let mut buf = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
        buf.set_size(Some(16_384.0), None);
        buf.set_wrap(glyphon::Wrap::None);
        buf.set_text(
            label,
            &glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(norm)),
            glyphon::Shaping::Advanced,
            None,
        );
        buf.shape_until_scroll(&mut fs, false);
        let mut latin_start = f32::MAX;
        let mut latin_end = 0.0_f32;
        for run in buf.layout_runs() {
            for g in run.glyphs.iter() {
                let ch = label[g.start..g.end].chars().next().unwrap_or('\0');
                if ch.is_ascii_alphabetic() {
                    latin_start = latin_start.min(g.x);
                    latin_end = latin_end.max(g.x + g.w);
                }
            }
        }
        (root, button, text_box, n.x + latin_start, n.x + latin_end)
    }

    for (open_s, close_s) in [
        ("打开 Builder Trigger", "关闭 Builder Trigger"),
        ("打开 RSX Trigger", "关闭 RSX Trigger"),
    ] {
        let (open_root, open_btn, open_text, open_latin0, open_latin1) = placed(open_s);
        let (_close_root, close_btn, close_text, close_latin0, close_latin1) = placed(close_s);
        let open_leaf = open_root
            .children
            .iter()
            .find_map(|el| find_text_leaf(el, open_s))
            .expect("open leaf");
        let needs = crate::app::reactive::bind_text_metric_needs_layout(open_leaf, open_s, close_s);
        let needs_back =
            crate::app::reactive::bind_text_metric_needs_layout(open_leaf, close_s, open_s);
        let text_moved =
            crate::app::damage::node_present_slot_moved(open_text, close_text);
        let btn_moved = crate::app::damage::node_present_slot_moved(open_btn, close_btn);
        let old_suffix = [open_latin0, open_text[1], (open_latin1 - open_latin0).max(0.0), open_text[3]];
        let new_suffix = [close_latin0, close_text[1], (close_latin1 - close_latin0).max(0.0), close_text[3]];
        let clear_is_new_text = crate::app::damage::rect_contains_rect(close_text, old_suffix);
        let clear_is_new_btn = crate::app::damage::rect_contains_rect(close_btn, old_suffix);
        let dx = (open_latin0 - close_latin0).abs();
        assert!(
            dx <= 0.5 && open_text == close_text,
            "{open_s:?} → {close_s:?} must keep one suffix box (dx={dx:.2} open={open_text:?} close={close_text:?} needs={needs}/{needs_back})"
        );
        let _ = (old_suffix, new_suffix, text_moved, btn_moved, clear_is_new_text, clear_is_new_btn);
    }
}

fn find_text_leaf<'a>(el: &'a Element, text: &str) -> Option<&'a Element> {
    if el.text.as_deref() == Some(text) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_text_leaf(c, text))
}
