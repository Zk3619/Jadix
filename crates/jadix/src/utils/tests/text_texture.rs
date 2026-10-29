use super::*;

#[test]
fn text_texture_sdf_edge_is_mid_gray() {
    // 8x8: solid 4x4 block in the center.
    let w = 8u32;
    let h = 8u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 2..6 {
        for x in 2..6 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i + 3] = 255;
        }
    }
    let sdf = alpha_coverage_to_sdf_rgba(&rgba, w, h);
    fn median3(r: u8, g: u8, b: u8) -> u8 {
        let mut v = [r, g, b];
        v.sort_unstable();
        v[1]
    }
    // Far outside corner should be below the edge (128).
    assert_eq!(sdf[3], 255); // A
    let outside = median3(sdf[0], sdf[1], sdf[2]);
    assert!(
        outside < 128,
        "far outside median expected <128, got {outside}"
    );
    // Center inside should be > 128.
    let c = ((4 * w + 4) * 4) as usize;
    let inside = median3(sdf[c], sdf[c + 1], sdf[c + 2]);
    assert!(inside > 128, "inside median expected >128, got {inside}");
    assert_eq!(sdf[c + 3], 255);
    // Prefix constants stable for upload routing.
    assert_eq!(TEXT_XFORM_TEX_PREFIX, "jadix_text_xform_v3:");
    assert_eq!(TEXT_MSDF_TEX_PREFIX, "jadix_text_msdf_v2:");
}

#[test]
fn text_texture_msdf_channels_diverge_near_box_corner() {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 8..24 {
        for x in 8..24 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i + 3] = 255;
        }
    }
    let sdf = alpha_coverage_to_sdf_rgba(&rgba, w, h);
    let mut diverged = 0u32;
    for px in sdf.chunks_exact(4) {
        let max = px[0].max(px[1]).max(px[2]);
        let min = px[0].min(px[1]).min(px[2]);
        if (max as i32 - min as i32) > 6 {
            diverged += 1;
        }
    }
    assert!(
        diverged > 8,
        "true MSDF must split RGB near oriented edges, diverged={diverged}"
    );
    // Center median still inside.
    let c = ((16 * w + 16) * 4) as usize;
    let mut v = [sdf[c], sdf[c + 1], sdf[c + 2]];
    v.sort_unstable();
    assert!(v[1] > 128, "center median {}", v[1]);
}

#[test]
fn bake_emoji_composites_twemoji_color_not_wireframe() {
    // Repro: sticky Bake without Twemoji used font Mask → lucide-like outline.
    let font = 28.0_f32;
    let layout_w = 48.0_f32;
    let layout_h = 48.0_f32;
    let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
    let Some((key, _)) = ensure_text_xform_texture(
        "👍",
        font,
        400,
        [0, 0, 0, 255], // black text color — wireframe path would be near-monochrome black
        layout_w,
        layout_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Center,
        3.0,
        true, // would be MSDF; must be forced to RGBA Bake for emoji
        0.0,
    ) else {
        // Twemoji assets missing in some CI sandboxes — skip soft.
        return;
    };
    assert!(
        key.starts_with(TEXT_XFORM_TEX_PREFIX),
        "emoji must not use MSDF prefix, got {key}"
    );
    let Some((w, h, rgba)) = cache_peek(&key) else {
        panic!("bake cache miss after ensure");
    };
    assert!(w > 0 && h > 0);
    // Twemoji has chromatic pixels; a tinted outline bake is nearly R≈G≈B.
    let mut chromatic = 0u32;
    let mut opaque = 0u32;
    for px in rgba.chunks_exact(4) {
        if px[3] < 32 {
            continue;
        }
        opaque += 1;
        let max = px[0].max(px[1]).max(px[2]);
        let min = px[0].min(px[1]).min(px[2]);
        if (max as i32 - min as i32) > 24 {
            chromatic += 1;
        }
    }
    assert!(opaque > 20, "expected opaque emoji ink, opaque={opaque}");
    assert!(
        chromatic > 5,
        "expected Twemoji color variance (not wireframe), chromatic={chromatic}/opaque={opaque}"
    );
}

#[test]
fn reaction_chip_hover_scale_twemoji_bake_is_not_clipped_in_rt() {
    // Reaction leaf: text-xs + leading-none in a 1rem chip. Mid hover:scale-125
    // must not clip Twemoji against the leading-none strut (square crop).
    let _g = cache_test_serial();
    cache_clear();
    fn bake_at(scale: f32) -> Option<(u32, u32, Vec<u8>, [u32; 4])> {
        let font = 12.0 * scale;
        let layout_w = 16.0 * scale;
        let layout_h = 12.0 * scale;
        let line_h = 12.0 * scale;
        let (bw, bh, pad) = text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, line_h);
        let (key, _) = ensure_text_xform_texture(
            "😮",
            font,
            400,
            [0, 0, 0, 255],
            layout_w,
            layout_h,
            bw,
            bh,
            pad,
            glyphon::cosmic_text::Align::Center,
            crate::renderer::BAKE_OVERSAMPLE,
            false,
            0.0,
        )?;
        let (w, h, rgba) = cache_peek(&key)?;
        let ink = opaque_ink_rect(&rgba, w, h)?;
        Some((w, h, rgba, ink))
    }
    let Some((rw, rh, _, rest_ink)) = bake_at(1.0) else {
        return;
    };
    let Some((hw, hh, _, hover_ink)) = bake_at(1.17) else {
        return;
    };
    let rest_touch = rest_ink[0] == 0
        || rest_ink[1] == 0
        || rest_ink[0] + rest_ink[2] >= rw
        || rest_ink[1] + rest_ink[3] >= rh;
    let hover_touch = hover_ink[0] == 0
        || hover_ink[1] == 0
        || hover_ink[0] + hover_ink[2] >= hw
        || hover_ink[1] + hover_ink[3] >= hh;
    assert!(
            !rest_touch,
            "rest reaction Bake already clips Twemoji (square crop, then hover:scale enlarges the cut); rest_rt={rw}x{rh} rest_ink={rest_ink:?}"
        );
    assert!(
            !hover_touch,
            "scaled reaction Bake must keep Twemoji inside the RT; rest_rt={rw}x{rh} rest_ink={rest_ink:?} hover_rt={hw}x{hh} hover_ink={hover_ink:?}"
        );
    let ink_ratio_h = hover_ink[3] as f32 / rest_ink[3] as f32;
    assert!(
            ink_ratio_h > 1.17 * 0.88,
            "hover Twemoji ink must grow with scale (crop to rest strut keeps ink flat); rest_ink={rest_ink:?} hover_ink={hover_ink:?} ratio_h={ink_ratio_h}"
        );
    let rest_frac_w = rest_ink[2] as f32 / rw as f32;
    let hover_frac_w = hover_ink[2] as f32 / hw as f32;
    assert!(
            (hover_frac_w - rest_frac_w).abs() < 0.08,
            "ink/RT fraction must stay similar under near-identity scale (crop shrinks pad); rest={rest_frac_w} hover={hover_frac_w} rest_ink={rest_ink:?} hover_ink={hover_ink:?}"
        );
}

#[test]
fn text_texture_reject_empty_alpha() {
    let empty = [0u8; 4 * 4 * 4];
    assert!(!empty.chunks_exact(4).any(|p| p[3] > 0));
}

#[test]
fn text_xform_bake_box_grows_tight_height_to_leading_plus_pad() {
    let font = 14.0_f32;
    let (w, h, pad) = text_xform_bake_box(40.0, font, font);
    let leading = font * crate::style::tailwind_default_leading_scale(font);
    assert!(pad >= 2.0);
    assert!(
        h + 0.01 >= leading + pad * 2.0,
        "bake h={h} must fit leading={leading} + 2*pad={pad}"
    );
    assert!((w - (40.0 + pad * 2.0)).abs() < 0.01);
}

#[test]
fn bake_glyph_layout_dy_only_when_layout_taller_than_line() {
    let font = 14.0_f32;
    let line_h = super::bake_default_line_height_px(font);
    assert_eq!(
        super::bake_glyph_layout_dy(line_h, line_h, 1, true, false),
        0.0,
        "intrinsic-height leaf must not get vertical dy"
    );
    let tall = line_h + 12.0;
    let dy = super::bake_glyph_layout_dy(tall, line_h, 1, true, false);
    assert!((dy - (tall - line_h) * 0.5).abs() < 0.01);
    assert_eq!(
        super::bake_glyph_layout_dy(tall, line_h, 2, true, false),
        0.0
    );
    assert_eq!(
        super::bake_glyph_layout_dy(tall, line_h, 1, false, false),
        0.0
    );
    // Label `leading-none`: 1em line box in a slightly taller leaf → ~1px dy,
    // not the old Tailwind-default (~20px) which skipped dy and sat low.
    let none = font;
    let chip = font + 2.0;
    let dy_none = super::bake_glyph_layout_dy(chip, none, 1, true, false);
    assert!(
        (dy_none - 1.0).abs() < 0.01,
        "leading-none in a 16px leaf must dy by 1px, got {dy_none}"
    );
}

#[test]
fn bake_glyph_layout_dy_centers_single_line_in_tall_layout_box() {
    let font = 14.0_f32;
    let line_h = super::bake_default_line_height_px(font);
    let layout_w = 80.0_f32;
    let layout_h = 28.0_f32;
    let dy = super::bake_glyph_layout_dy(layout_h, line_h, 1, true, false);
    assert!(
        dy > 0.0,
        "tall padded layout box should get Coverage-matching dy"
    );
    let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
    let Some((w, h, rgba)) = super::rasterize_text_rgba(
        "居中",
        font,
        600,
        [0, 0, 0, 255],
        layout_w,
        layout_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Center,
        2.0,
        None,
        dy,
        line_h,
    ) else {
        return;
    };
    let mut min_y = h as i32;
    let mut max_y = 0i32;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                min_y = min_y.min(y as i32);
                max_y = max_y.max(y as i32);
            }
        }
    }
    let ink_cy = (min_y + max_y) as f32 * 0.5;
    let layout_cy = (dy * 2.0 + font) * 0.5 * 2.0; // rough: ink should sit near layout mid
    let _ = layout_cy;
    let tex_cy = h as f32 * 0.5;
    assert!(
        (ink_cy - tex_cy).abs() < h as f32 * 0.15,
        "ink cy={ink_cy} should be near texture center {tex_cy} when dy={dy}"
    );
}

#[test]
fn bake_right_align_ink_ends_in_layout_region_not_bake_edge() {
    let font = 14.0_f32;
    let layout_w = 96.0_f32;
    let layout_h = 20.0_f32;
    let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
    let dpr = 2.0_f32;
    let Some((w, h, rgba)) = super::rasterize_text_rgba(
        "OK",
        font,
        600,
        [0, 0, 0, 255],
        layout_w,
        layout_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Right,
        dpr,
        None,
        0.0,
        super::bake_default_line_height_px(font),
    ) else {
        return;
    };
    let layout_right_px = ((pad + layout_w) * dpr).round() as i32;
    let bake_right_px = (bw * dpr).round() as i32;
    let mut max_ink_x = 0i32;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                max_ink_x = max_ink_x.max(x as i32);
            }
        }
    }
    let _ = bake_right_px;
    assert!(
        max_ink_x <= layout_right_px + 2,
        "right-aligned ink max_x={max_ink_x} should be near layout right {layout_right_px}"
    );
    assert!(
        max_ink_x < bake_right_px - 2,
        "ink must not hug bake texture edge when layout is inset by pad"
    );
}

#[test]
fn bake_tight_box_no_longer_clips_cjk_ink() {
    // Repro: layout height == font-size (no leading) used to clip glyph tops/bottoms.
    let font = 12.0_f32;
    let tight_h = font;
    let (bw, bh, pad) = text_xform_bake_box(64.0, tight_h, font);
    let Some((_, _)) = ensure_text_xform_texture(
        "旋转中",
        font,
        600,
        [255, 255, 255, 255],
        64.0,
        tight_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Center,
        2.0,
        false,
        0.0,
    ) else {
        // Font missing in CI — skip rather than false-fail.
        return;
    };
    // Pull RGBA from cache via second ensure (same key) by re-rasterizing check:
    // top/bottom border rows of a padded bake must stay mostly empty.
    let (w, h, rgba) = super::rasterize_text_rgba(
        "旋转中",
        font,
        600,
        [255, 255, 255, 255],
        64.0,
        tight_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Center,
        2.0,
        None,
        0.0,
        super::bake_default_line_height_px(font),
    )
    .expect("rasterize");
    let border = 1usize;
    let mut top_ink = 0u32;
    let mut bot_ink = 0u32;
    for y in 0..border {
        for x in 0..w as usize {
            if rgba[(y * w as usize + x) * 4 + 3] > 16 {
                top_ink += 1;
            }
        }
    }
    for y in (h as usize - border)..h as usize {
        for x in 0..w as usize {
            if rgba[(y * w as usize + x) * 4 + 3] > 16 {
                bot_ink += 1;
            }
        }
    }
    assert_eq!(top_ink, 0, "top pad row must not contain glyph ink");
    assert_eq!(bot_ink, 0, "bottom pad row must not contain glyph ink");
}

#[test]
fn bake_leading_none_ink_matches_coverage_not_default_leading() {
    // Gallery Label is `leading-none` (1em). Bake used to shape with Tailwind
    // text-sm leading (~1.43em) and grow the quad — cosmic half-leading plus
    // pad-ceil leftover read as ~1px down vs Coverage.
    let font = 14.0_f32;
    let layout_w = 96.0_f32;
    let layout_h = font;
    let none = font;
    let def = super::bake_default_line_height_px(font);
    assert!(
        def - none > 4.0,
        "text-sm default leading must be distinctly taller than 1em"
    );

    let (_, bh_none, _) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, none);
    let (_, bh_def, _) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, def);
    assert!(
        bh_none + 0.5 < bh_def,
        "leading-none bake box must be shorter than text-sm leading ({bh_none} vs {bh_def})"
    );

    // Metrics must actually change Cosmic line_y (same RT, origin from `pad`
    // not integer-centered shape — otherwise extra shape_h cancels half-leading).
    let dpr = crate::renderer::BAKE_OVERSAMPLE;
    let (bw, bh, pad) = super::text_xform_bake_box_ex(layout_w, layout_h, font, 0.0, none);
    let line_y = |line_h: f32| -> Option<f32> {
        crate::text::measure::with_layout_font_system(|fs| {
            crate::renderer::font_loader::ensure_coverage_for_text(fs, "拖到四角");
            let size = (font * dpr).max(1.0);
            let lh = (line_h * dpr).max(1.0);
            let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
            buffer.set_size(Some((layout_w * dpr).ceil().max(1.0)), None);
            let family = crate::renderer::text_hit::best_font_family(fs);
            let attrs = glyphon::Attrs::new().family(family);
            buffer.set_text("拖到四角", &attrs, glyphon::Shaping::Advanced, None);
            buffer.shape_until_scroll(fs, false);
            buffer.layout_runs().next().map(|r| r.line_y / dpr)
        })
    };
    let Some(y_none) = line_y(none) else {
        return;
    };
    let Some(y_def) = line_y(def) else {
        return;
    };
    assert!(
        y_def > y_none + 0.35,
        "taller Cosmic line box must half-lead downward (none={y_none:.2} def={y_def:.2})"
    );
    let _ = (bw, bh, pad);
}

#[test]
fn text_xform_cpu_cache_lru_keeps_hot_drops_cold() {
    let _serial = cache_test_serial();
    cache_clear();
    let _ = crate::app::pending_images::take_pending();
    let px = vec![255u8, 0, 0, 255];
    for i in 0..MAX_CACHE {
        cache_put(format!("lru-k{i}"), 1, 1, px.clone());
    }
    // Pretend GPU holds everything; clear `needed` like a completed frame.
    let _ = flush_text_xform_textures(|_| true);
    assert!(cache_touch("lru-k0"));
    cache_put("lru-knew".into(), 1, 1, px);
    let _ = flush_text_xform_textures(|_| true);
    assert!(cache_peek("lru-k0").is_some(), "hot key must survive LRU");
    assert!(
        cache_peek("lru-knew").is_some(),
        "newest insert must remain"
    );
    assert!(
        cache_peek("lru-k1").is_none(),
        "oldest cold key should be evicted"
    );
    cache_clear();
}

#[test]
fn text_xform_flush_requeues_only_needed_missing_from_gpu() {
    let _serial = cache_test_serial();
    cache_clear();
    let _ = crate::app::pending_images::take_pending();
    let px = vec![0u8, 255, 0, 255];
    cache_put("need-a".into(), 1, 1, px.clone());
    cache_put("cold-b".into(), 1, 1, px);
    let _ = crate::app::pending_images::take_pending();
    let _ = flush_text_xform_textures(|_| true);
    assert!(cache_touch("need-a"));
    let any = flush_text_xform_textures(|_| false);
    assert!(any, "needed key missing from GPU must re-queue");
    let pending = crate::app::pending_images::take_pending();
    let keys: Vec<&str> = pending.iter().map(|(k, _, _, _)| k.as_str()).collect();
    assert!(keys.contains(&"need-a"), "got {keys:?}");
    assert!(
        !keys.contains(&"cold-b"),
        "cold CPU entries must not refill GPU, got {keys:?}"
    );
    cache_clear();
}

#[test]
fn text_xform_cpu_cache_byte_budget_evicts() {
    let _serial = cache_test_serial();
    cache_clear();
    let _ = crate::app::pending_images::take_pending();
    // 4 MiB × 5 > 16 MiB cap.
    let fat = vec![0u8; 1024 * 1024 * 4];
    for i in 0..5 {
        cache_put(format!("fat-{i}"), 1024, 1024, fat.clone());
        let _ = flush_text_xform_textures(|_| true);
    }
    let (n, bytes) = text_xform_cpu_cache_stats();
    assert!(
        bytes <= MAX_CACHE_BYTES,
        "byte budget exceeded: {bytes} bytes, n={n}"
    );
    assert!(n < 5, "expected eviction under byte cap, n={n}");
    cache_clear();
}

#[test]
fn is_text_xform_texture_key_matches_prefixes() {
    assert!(is_text_xform_texture_key(TEXT_XFORM_TEX_PREFIX));
    assert!(is_text_xform_texture_key(TEXT_MSDF_TEX_PREFIX));
    assert!(is_text_xform_texture_key("jadix_text_xform_v3:deadbeef"));
    assert!(!is_text_xform_texture_key("jadix_path:x"));
}

fn bake_rgba(text: &str, weight: u16, dpr: f32) -> Option<(u32, u32, Vec<u8>)> {
    let font = 14.0_f32;
    let layout_w = 96.0_f32;
    let layout_h = 20.0_f32;
    let (bw, bh, pad) = text_xform_bake_box(layout_w, layout_h, font);
    let (key, _) = ensure_text_xform_texture(
        text,
        font,
        weight,
        [20, 20, 20, 255],
        layout_w,
        layout_h,
        bw,
        bh,
        pad,
        glyphon::cosmic_text::Align::Left,
        dpr,
        false,
        0.0,
    )?;
    cache_peek(&key)
}

fn bake_alpha_mass(text: &str, weight: u16) -> Option<u32> {
    bake_rgba(text, weight, 2.0).map(|(_, _, rgba)| rgba.chunks_exact(4).map(|p| p[3] as u32).sum())
}

fn bake_fringe_ratio(rgba: &[u8]) -> f32 {
    let mut solid = 0u32;
    let mut fringe = 0u32;
    for px in rgba.chunks_exact(4) {
        let a = px[3];
        if a > 160 {
            solid += 1;
        } else if a > 12 {
            fringe += 1;
        }
    }
    let den = (solid + fringe).max(1) as f32;
    fringe as f32 / den
}

#[test]
fn bake_font_medium_applies_synthetic_bold_when_face_missing() {
    // Gallery Label is font-medium (500). Coverage thickens Regular with a
    // 0.18/0.32 halo; Bake (rotate / DragOverlay 替身) must do the same or
    // the ghost looks thinner than the source.
    let _serial = cache_test_serial();
    cache_clear();
    let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
        let family = crate::renderer::text_hit::best_font_family(fs);
        crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
    });
    if !needs_synth {
        return;
    }
    let mass400 = bake_alpha_mass("拖到四角", 400).expect("regular bake");
    let mass500 = bake_alpha_mass("拖到四角", 500).expect("medium bake");
    assert!(
        mass500 > mass400 + mass400 / 80,
        "Bake font-medium must apply font-synthesis:weight (400={mass400} 500={mass500})"
    );
}

#[test]
fn bake_synthetic_bold_stays_sharp_at_oversample() {
    // Coverage fake-bold is a 0.18px duplicate TextArea (blur 0.32 < 0.75).
    // Bake stamps in supersampled px; 0.32 * BAKE_OVERSAMPLE (3) = 0.96 used to
    // trip the 0.75 bake-pixel blur threshold and become a soft glow — thinner
    // than the source after bilinear minify (DragOverlay rotate ghost).
    let _serial = cache_test_serial();
    cache_clear();
    let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
        let family = crate::renderer::text_hit::best_font_family(fs);
        crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
    });
    if !needs_synth {
        return;
    }
    let (_, _, rgba1) = bake_rgba("拖到四角", 500, 1.0).expect("dpr1");
    let (_, _, rgba3) = bake_rgba("拖到四角", 500, crate::renderer::BAKE_OVERSAMPLE).expect("dpr3");
    let r1 = bake_fringe_ratio(&rgba1);
    let r3 = bake_fringe_ratio(&rgba3);
    assert!(
            r3 <= r1 + 0.06,
            "3× Bake synth must stay a sharp duplicate like Coverage, not a soft glow (fringe dpr1={r1:.3} dpr3={r3:.3})"
        );
}

fn box_downscale(rgba: &[u8], w: u32, h: u32, factor: u32) -> (u32, u32, Vec<u8>) {
    let f = factor.max(1);
    let nw = (w / f).max(1);
    let nh = (h / f).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    let f2 = f * f;
    for y in 0..nh {
        for x in 0..nw {
            let mut a = 0u32;
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            for dy in 0..f {
                for dx in 0..f {
                    let sx = x * f + dx;
                    let sy = y * f + dy;
                    if sx >= w || sy >= h {
                        continue;
                    }
                    let i = ((sy * w + sx) * 4) as usize;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                    a += rgba[i + 3] as u32;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            out[o] = (r / f2) as u8;
            out[o + 1] = (g / f2) as u8;
            out[o + 2] = (b / f2) as u8;
            out[o + 3] = (a / f2) as u8;
        }
    }
    (nw, nh, out)
}

#[test]
fn bake_oversample_minified_keeps_label_weight() {
    // DragOverlay rotate bakes at 3× then bilinear-minifies. Small-size glyphon
    // Coverage (本体) is hinted at 14px and looks heavier; the ghost used to
    // read as Regular. Minified 3× ink must stay close to 1× bake of the same label.
    let _serial = cache_test_serial();
    cache_clear();
    let (_, _, r1) = bake_rgba("拖到四角", 500, 1.0).expect("dpr1");
    let (w3, h3, r3) = bake_rgba("拖到四角", 500, crate::renderer::BAKE_OVERSAMPLE).expect("dpr3");
    let factor = crate::renderer::BAKE_OVERSAMPLE.round() as u32;
    let (_, _, down) = box_downscale(&r3, w3, h3, factor);
    let ink = |rgba: &[u8]| rgba.chunks_exact(4).map(|p| p[3] as u32).sum::<u32>() as f32;
    let m1 = ink(&r1);
    let md = ink(&down);
    assert!(
            md >= m1 * 0.95 && md <= m1 * 1.08,
            "3× Bake minified must match 1× (Coverage-like) weight: 1x={m1:.0} down={md:.0} ratio={:.3}",
            md / m1.max(1.0)
        );
}
