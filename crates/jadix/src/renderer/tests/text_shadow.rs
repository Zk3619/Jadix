use super::{generate_shadow_bitmap, text_shadow_active};
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;

fn glyph_ink_bottom(
    buffer: &glyphon::Buffer,
    fs: &mut glyphon::FontSystem,
    swash: &mut glyphon::SwashCache,
) -> Option<i32> {
    let mut max_y = i32::MIN;
    let mut any = false;
    for run in buffer.layout_runs() {
        let line_y_px = run.line_y.round() as i32;
        for g in run.glyphs.iter() {
            let physical = g.physical((0.0, 0.0), 1.0);
            let Some(img) = swash.get_image_uncached(fs, physical.cache_key) else {
                continue;
            };
            any = true;
            let py = line_y_px + physical.y - img.placement.top;
            max_y = max_y.max(py + img.placement.height as i32);
        }
    }
    any.then_some(max_y)
}

#[test]
fn shadow_coverage_clipping_matches_scalar_composite() {
    let coverage: Vec<u8> = (0..35).map(|i| [0, 1, 64, 127, 255][i % 5]).collect();
    for y in -7..10 {
        for x in -9..12 {
            let mut expected: Vec<u16> = (0..99).map(|i| (i * 37 % 256) as u16).collect();
            let mut actual = expected.clone();
            // Multiple translucent overlapping stamps must preserve integer rounding.
            for alpha in [0_u8, 1, 17, 128, 254, 255] {
                for row in 0..5_i32 {
                    for col in 0..7_i32 {
                        let (bx, by) = (x + col, y + row);
                        if (0..11).contains(&bx) && (0..9).contains(&by) {
                            let src =
                                coverage[(row * 7 + col) as usize] as u16 * alpha as u16 / 255;
                            let dst = &mut expected[(by * 11 + bx) as usize];
                            *dst = src + *dst - src * *dst / 255;
                        }
                    }
                }
                super::stamp_shadow_glyph(&mut actual, [11, 9], &coverage, [7, 5], [x, y], alpha);
                assert_eq!(actual, expected, "origin=({x},{y}) alpha={alpha}");
            }
        }
    }
}

#[test]
fn text_sm_line_box_covers_gallery_descenders() {
    // Tailwind `text-sm` is 14px / 1.25rem. That strut covers Latin `g`/`y`;
    // `leading-none` (1em) does not — but CSS still paints the overflow.
    // Do not invent an ink pad; production `set_size(w, None)` plus the
    // menu-button token (not a taller Cosmic buffer) is the browser model.
    let size = 14.0_f32;
    let sm_lh = size * crate::style::TAILWIND_TEXT_SM_LEADING_SCALE;
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    let family = best_font_family(&fs);
    let mut swash = glyphon::SwashCache::new();
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(700));

    let mut none_box = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, size));
    none_box.set_size(Some(400.0), None);
    none_box.set_text("Gallery", &attrs, glyphon::Shaping::Advanced, None);
    none_box.shape_until_scroll(&mut fs, false);
    let Some(ink_none) = glyph_ink_bottom(&none_box, &mut fs, &mut swash) else {
        return;
    };

    let mut sm = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, sm_lh));
    sm.set_size(Some(400.0), None);
    sm.set_text("Gallery", &attrs, glyphon::Shaping::Advanced, None);
    sm.shape_until_scroll(&mut fs, false);
    let Some(ink_sm) = glyph_ink_bottom(&sm, &mut fs, &mut swash) else {
        return;
    };

    assert!(
        (ink_none as f32) > size + 0.5,
        "Gallery descenders sit below the 1em em-box (ink={ink_none} em={size})"
    );
    assert!(
        (ink_sm as f32) <= sm_lh + 1.0,
        "text-sm line box must cover those descenders without a pad (ink={ink_sm} lh={sm_lh})"
    );
}

#[test]
fn text_shadow_active_allows_zero_blur_with_offset() {
    assert!(text_shadow_active(Some([0, 0, 0, 255]), [4.0, 0.0], 0.0));
    assert!(text_shadow_active(Some([0, 0, 0, 255]), [0.0, 0.0], 2.0));
    assert!(!text_shadow_active(Some([0, 0, 0, 255]), [0.0, 0.0], 0.0));
    assert!(!text_shadow_active(None, [4.0, 4.0], 2.0));
    assert!(!text_shadow_active(Some([0, 0, 0, 0]), [4.0, 4.0], 2.0));
}

#[test]
fn synthetic_bold_must_not_bake_textured_shadow() {
    // Gallery chase: sidebar 「数据」 / CSS-filter captions use font-medium
    // synthesis (blur 0.32). A baked quad desyncs from Coverage CJK.
    assert!(
        !super::uses_textured_shadow_bitmap(false, 0.32),
        "synthetic-bold must stay on the glyphon layer"
    );
    assert!(
        !super::uses_textured_shadow_bitmap(true, 0.32),
        "sharp user text-shadow must stay on the glyphon layer"
    );
    assert!(
        super::uses_textured_shadow_bitmap(true, 4.0),
        "soft user text-shadow may still bake a bitmap"
    );
}

#[test]
fn shadow_tex_instance_clip_disables_on_empty() {
    assert_eq!(
        super::shadow_tex_instance_clip(None),
        ([0.0, 0.0], [-1.0, -1.0])
    );
    assert_eq!(
        super::shadow_tex_instance_clip(Some([10.0, 20.0, 0.0, 8.0])),
        ([0.0, 0.0], [-1.0, -1.0])
    );
    assert_eq!(
        super::shadow_tex_instance_clip(Some([10.0, 20.0, 100.0, 40.0])),
        ([10.0, 20.0], [100.0, 40.0])
    );
}

#[test]
fn integerize_clip_rect_matches_glyphon_text_bounds_edges() {
    let i = super::integerize_clip_rect([10.2, 20.8, 30.3, 40.1]).expect("positive");
    assert!((i[0] - 10.0).abs() < 1e-4, "floor(10.2)=10, got {}", i[0]);
    assert!((i[1] - 20.0).abs() < 1e-4, "floor(20.8)=20, got {}", i[1]);
    let right = i[0] + i[2];
    let bottom = i[1] + i[3];
    assert!(
        (right - 40.0).abs() < 1e-4,
        "floor(10.2+30.3)=40, got {right}"
    );
    assert!(
        (bottom - 60.0).abs() < 1e-4,
        "floor(20.8+40.1)=60, got {bottom}"
    );
    assert!(super::integerize_clip_rect([5.0, 5.0, 0.0, 8.0]).is_none());
}

#[test]
fn integerize_clip_rect_does_not_expand_past_overflow_edge() {
    // Gallery sidebar: last visible label clip bottom 463.25, footer rule at 463.25.
    let i = super::integerize_clip_rect([16.0, 458.0, 42.0, 5.25]).expect("positive");
    let bottom = i[1] + i[3];
    assert!(
        bottom <= 463.25,
        "far-edge floor must not paint past overflow:hidden (got {bottom})"
    );
    assert!(
        (bottom - 463.0).abs() < 1e-4,
        "floor(463.25)=463, got {bottom}"
    );
}

#[test]
fn paint_overflow_clip_sliver_skips_instead_of_unclipping() {
    assert!(
        super::paint_overflow_clip(Some([0.0, 463.0, 80.0, 0.4]), true).is_none(),
        "sub-pixel leftover at the clip edge must not disable the scissor"
    );
    assert_eq!(
        super::paint_overflow_clip(None, false),
        Some(None),
        "no overflow ancestor still draws unclipped"
    );
    assert!(
        super::paint_overflow_clip(None, true).is_none(),
        "off-screen intersect must skip, not fill the viewport"
    );
}

#[test]
fn shadow_bitmap_keeps_positive_dy_mass_below_centroid() {
    // Regression: textured UV now maps top→row0; Y-flipping the shadow bitmap
    // inverted soft shadows after the Bake/capture UV fix.
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    let family = best_font_family(&fs);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(24.0, 36.0));
    buffer.set_size(Some(200.0), Some(48.0));
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(700));
    buffer.set_text("Shadow", &attrs, glyphon::Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut fs, false);
    let mut swash = glyphon::SwashCache::new();
    let Some(bmp) =
        generate_shadow_bitmap(&buffer, &mut fs, &mut swash, [0, 0, 0, 200], 0.0, 8.0, 2.0)
    else {
        // Fonts unavailable in some CI images.
        return;
    };
    assert!(bmp.width > 0 && bmp.height > 0);
    assert!(bmp.rgba.chunks_exact(4).any(|p| p[3] > 0));
    let raster_count = swash.image_cache.len();
    assert!(
        raster_count > 0,
        "shadow must share cached glyph rasters with text"
    );
    let repeated =
        generate_shadow_bitmap(&buffer, &mut fs, &mut swash, [0, 0, 0, 200], 0.0, 8.0, 2.0)
            .unwrap();
    assert_eq!(swash.image_cache.len(), raster_count);
    assert_eq!(
        bmp.rgba, repeated.rgba,
        "cached shadow must preserve every pixel"
    );

    let mut sum_a = 0.0_f64;
    let mut sum_ay = 0.0_f64;
    for y in 0..bmp.height as usize {
        for x in 0..bmp.width as usize {
            let a = bmp.rgba[(y * bmp.width as usize + x) * 4 + 3] as f64;
            if a > 0.0 {
                sum_a += a;
                sum_ay += a * y as f64;
            }
        }
    }
    assert!(sum_a > 0.0);
    let cy = sum_ay / sum_a;
    // draw_rect origin is content_min - pad; with +dy the alpha mass sits in the
    // lower half of the bitmap (not the top — that was the inverted/flipped bug).
    assert!(
        cy > bmp.height as f64 * 0.35,
        "shadow alpha centroid y={cy} should not sit in the top third of h={}",
        bmp.height
    );
}
