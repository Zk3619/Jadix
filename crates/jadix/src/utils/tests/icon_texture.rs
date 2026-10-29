use super::*;

fn ink_bbox_center(side: u32, rgba: &[u8]) -> (f32, f32) {
    let mut min_x = side;
    let mut min_y = side;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for y in 0..side {
        for x in 0..side {
            if rgba[((y * side + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    assert!(max_x >= min_x, "expected ink");
    ((min_x + max_x) as f32 * 0.5, (min_y + max_y) as f32 * 0.5)
}

#[test]
fn loader_circle_svg_raster_nonempty_square() {
    let (side, h, rgba) =
        rasterize_svg_rgba("loader-circle", 32.0, [255, 255, 255, 255], 2.0).expect("svg");
    assert_eq!(side, h);
    assert!(side >= 32);
    let ink = rgba.chunks(4).filter(|p| p[3] > 0).count();
    assert!(ink > 40, "expected stroke ink, got {ink}");
}

#[test]
fn circle_icon_ink_bbox_near_center() {
    let (side, _, rgba) =
        rasterize_svg_rgba("circle", 32.0, [255, 255, 255, 255], 2.0).expect("svg");
    let (cx, cy) = ink_bbox_center(side, &rgba);
    let mid = (side as f32 - 1.0) * 0.5;
    assert!(
        (cx - mid).abs() < 2.5 && (cy - mid).abs() < 2.5,
        "ink bbox center ({cx:.2},{cy:.2}) far from {mid}"
    );
}

#[test]
fn font_chevron_ink_bbox_near_center() {
    let glyph = crate::utils::lucide::lucide_glyph("chevron-down").expect("glyph");
    let ch = glyph.chars().next().expect("char");
    let (side, h, rgba) = rasterize_glyph_rgba(ch, 32.0, [255, 255, 255, 255], 1.0).expect("font");
    assert_eq!(side, h);
    let (cx, cy) = ink_bbox_center(side, &rgba);
    let mid = (side as f32 - 1.0) * 0.5;
    assert!(
        (cx - mid).abs() < 2.5 && (cy - mid).abs() < 2.5,
        "font chevron ink ({cx:.2},{cy:.2}) far from square center {mid} — rotation would offset"
    );
}

#[test]
fn font_glyph_honors_dpr_supersample() {
    let glyph = crate::utils::lucide::lucide_glyph("chevron-down").expect("glyph");
    let ch = glyph.chars().next().expect("char");
    let (side, _, _) = rasterize_glyph_rgba(ch, 16.0, [255, 255, 255, 255], 2.0).expect("font");
    assert_eq!(
        side, 32,
        "dpr 2 at 16px must raster 32px square, got {side}"
    );
}

#[cfg(not(feature = "svg"))]
#[test]
fn lucide_inner_parses_path_d() {
    let layers = lucide_inner_to_layers(
        r#"<path d="M21 12a9 9 0 1 1-6.219-8.56" />"#,
        [0.0, 0.0, 0.0, 1.0],
        2.0,
    );
    assert!(!layers.is_empty());
    assert!(layers[0].0.len() > 4);
}
