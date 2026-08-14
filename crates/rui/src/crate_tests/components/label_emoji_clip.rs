//! Gallery-shaped repro: Label + leading_normal + face emoji must keep the
//! custom emoji quad inside TextBounds (no flat-bottom shave).

use crate::components::base::BaseComponentBuilder;
use crate::components::{Label, LabelProps};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::emoji_layout;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn find_text<'a>(el: &'a Element, needle: &str) -> Option<&'a Element> {
    if el.text.as_deref().is_some_and(|t| t.contains(needle)) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(f) = find_text(c, needle) {
            return Some(f);
        }
    }
    None
}

/// Simulate glyphon custom-glyph clip (see glyphon `prepare_glyph`).
fn clipped_bottom_overshoot(
    area_top: f32,
    glyph_top: f32,
    glyph_h: f32,
    clip_y: f32,
    clip_h: f32,
) -> f32 {
    let y = (area_top + glyph_top).round() as i32;
    let height = glyph_h.round() as i32;
    let bounds_max_y = (clip_y + clip_h).ceil() as i32;
    let max_y = y + height;
    (max_y - bounds_max_y).max(0) as f32
}

#[test]
fn gallery_label_leading_normal_face_emoji_fits_text_bounds() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let text = "Emoji: 😀 😁 😂";
    let el = Label(LabelProps {
        id: "gal-emoji-clip".into(),
        text: Some(String::new()),
        size: None,
        color: None,
        weight: None,
        center: Some(false),
        r#for: None,
        truncate: None,
        text_overflow: None,
        children: Some(Element::new().text(text)),
        base: Some(Box::new(|mut b: BaseComponentBuilder| {
            b = b.w("100%");
            b = b.leading_normal();
            b
        })),
    });

    assert_eq!(
        el.text_style.line_height_scale,
        Some(1.5),
        "base leading_normal must override Label leading-none default"
    );
    assert!(
        el.text.as_deref().is_some_and(|t| t.contains('😀')),
        "string children must be adopted onto Label text"
    );

    let size = el.text_style.size;
    let lh = size * 1.5;

    let mut tree = LayoutTree::new();
    // Column + align-start so the label is content-height (gallery VStack), not flex-stretched.
    let mut root = Element::new()
        .w(900.0)
        .h(200.0)
        .flex_col()
        .flex_items("start")
        .child(el);
    let result = tree.build_and_layout(&mut root, 900.0, 200.0, [-1.0, -1.0]);

    let leaf = find_text(&root, "😀").expect("emoji label leaf");
    let leaf_h = leaf.layout_bounds[3];
    let info = result
        .text_nodes
        .iter()
        .find(|n| n.text.contains('😀'))
        .expect("TextLayoutInfo for emoji label");
    // Leaf itself does not set overflow — clip comes from ancestors. When none, paint
    // uses the leaf rect as the effective clip for this assertion.
    let clip_h = info.clip_bounds.map(|c| c[3]).unwrap_or(info.height);
    let clip_y = info.clip_bounds.map(|c| c[1]).unwrap_or(info.y);

    eprintln!(
        "[emoji-clip] size={size:.2} lh={lh:.2} leaf_h={leaf_h:.2} \
         scale={:?} clip_h={clip_h:.2} box=({:.1},{:.1},{:.1},{:.1}) clip={:?}",
        leaf.text_style.line_height_scale,
        info.x,
        info.y,
        info.width,
        info.height,
        info.clip_bounds
    );

    assert!(
        (leaf_h - lh).abs() < 1.25,
        "leaf height should track leading-normal line box; leaf_h={leaf_h} lh={lh}"
    );

    // Shape with the same Metrics/line box paint uses, then place the custom quad.
    let layout = emoji_layout::plan_emoji_layout_text(text);
    let emoji_start = layout.find(emoji_layout::EMOJI_LAYOUT_SLOT).unwrap();
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(&mut fs, Some(info.width.max(1.0)), Some(lh + 2.0));
    buffer.set_text(
        &mut fs,
        &layout,
        glyphon::Attrs::new().family(glyphon::Family::SansSerif),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut fs, false);

    let pixel_size = crate::renderer::prepare_text::emoji_paint_pixel_size(size) as f32;
    let (left, top) = emoji_layout::custom_emoji_left_centered_top(
        &buffer,
        &layout,
        emoji_start,
        pixel_size,
        pixel_size,
    )
    .expect("emoji slot");
    let bottom = top + pixel_size;
    let overshoot = clipped_bottom_overshoot(info.y, top, pixel_size, clip_y, clip_h);

    eprintln!(
        "[emoji-clip] quad left={left:.2} top={top:.2} bottom={bottom:.2} \
         pixel={pixel_size} overshoot_px={overshoot}"
    );

    assert!(
        overshoot < 0.5,
        "face emoji bottom must fit TextBounds; overshoot={overshoot} \
         (leaf_h={leaf_h} clip_h={clip_h} quad_bottom={bottom})"
    );
}

#[test]
fn face_emoji_raster_at_label_size_keeps_bottom_ink() {
    let size = 14u16;
    let r = crate::renderer::prepare_text::load_font_fallback_raster("😀", size, 400)
        .expect("grinning face");
    let w = r.width as usize;
    let h = r.height as usize;
    // Intact circle: bottom-most ink row is a short chord, not a wide flat cut.
    let row_ink = |row: usize| {
        (0..w)
            .filter(|&x| r.rgba[(row * w + x) * 4 + 3] > 32)
            .count()
    };
    let mut last_ink_row = None;
    let mut max_row_ink = 0usize;
    for row in 0..h {
        let n = row_ink(row);
        max_row_ink = max_row_ink.max(n);
        if n > 0 {
            last_ink_row = Some(row);
        }
    }
    let last = last_ink_row.expect("face has ink");
    let chord = row_ink(last);
    eprintln!(
        "😀@{size} {}x{} max_row_ink={max_row_ink} last_ink_row={last} chord={chord}",
        w, h
    );
    assert!(max_row_ink > 0, "face must retain ink");
    assert!(
        (chord as f32) < (max_row_ink as f32) * 0.55,
        "bottom chord must taper (shave leaves a wide flat edge); chord={chord} max={max_row_ink}"
    );
}

#[test]
fn leading_none_leaf_documents_face_shave_failure_mode() {
    // Documents why leading-none + full-em custom quad shaves circular faces.
    let size = 14.0_f32;
    let lh = size; // leading-none
    let pixel = size.ceil();
    let top = (lh - pixel).max(0.0) * 0.5;
    let overshoot = clipped_bottom_overshoot(0.0, top, pixel, 0.0, lh);
    eprintln!("leading-none overshoot={overshoot} (pixel={pixel} lh={lh})");
    // Exact fit → 0 overshoot in ideal math; snap can still shave by 1px.
    // The visual failure is: circle ink reaches the clip edge with no margin.
    assert!(top + pixel >= lh - 0.01);
}
