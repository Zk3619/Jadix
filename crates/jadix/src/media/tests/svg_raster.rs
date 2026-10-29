use super::*;

#[test]
fn raster_cache_preserves_all_textures_needed_by_one_frame() {
    RASTER_CPU.with(|c| c.borrow_mut().clear());
    RASTER_NEEDED.with(|n| n.borrow_mut().clear());
    let doc = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="red"/></svg>"#;
    let keys: Vec<_> = (0..MAX_SVG_RASTER_CACHE + 1)
        .map(|i| {
            ensure_svg_document_texture_with_key(format!("raster-frame-test:{i}"), doc, 1, 1)
                .unwrap()
        })
        .collect();
    assert!(flush_svg_textures(|_| false));
    for key in keys {
        assert!(
            crate::app::pending_images::take_pending_key(&key).is_some(),
            "cache pressure dropped a texture before the frame uploaded it"
        );
    }
    RASTER_CPU.with(|c| c.borrow_mut().clear());
}

#[test]
fn unpremultiply_restores_straight_rgb_on_partial_alpha() {
    let mut px = [25u8, 25, 25, 128];
    unpremultiply_rgba8(&mut px);
    assert!(
        (px[0] as i16 - 50).abs() <= 1 && (px[1] as i16 - 50).abs() <= 1,
        "premul 25 @ a=128 must unpremul to ~50, got {:?}",
        px
    );
    assert_eq!(px[3], 128);
}

#[test]
fn lucide_minus_partial_coverage_keeps_foreground_rgb() {
    let inner = r#"<path d="M18 6 6 18" /> <path d="m6 6 12 12" />"#;
    let fg = [25u8, 25, 25, 255];
    let (_, _, rgba) = rasterize_lucide_icon_rgba(inner, fg, 96).expect("raster");
    let mut aa = 0usize;
    let mut opaque = 0usize;
    for p in rgba.chunks_exact(4) {
        let a = p[3];
        if a >= 250 {
            opaque += 1;
            assert!(
                (p[0] as i16 - fg[0] as i16).abs() <= 4 && (p[1] as i16 - fg[1] as i16).abs() <= 4,
                "opaque stroke must be foreground, got rgba={p:?}"
            );
            continue;
        }
        if !(40..220).contains(&a) {
            continue;
        }
        aa += 1;
        assert!(
            (p[0] as i16 - fg[0] as i16).abs() <= 12
                && (p[1] as i16 - fg[1] as i16).abs() <= 12
                && (p[2] as i16 - fg[2] as i16).abs() <= 12,
            "AA pixel must keep straight foreground RGB, got rgba={p:?} fg={fg:?}"
        );
    }
    assert!(opaque > 20, "expected opaque stroke ink, got {opaque}");
    assert!(aa > 8, "expected antialiased stroke samples, got {aa}");
}
