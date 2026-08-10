//! Diagnostic harness for emoji shaping / Twemoji / font-fallback failure modes.
//! Not a product API — agent-runnable pass/fail signals for incomplete emoji render.

use unicode_segmentation::UnicodeSegmentation;

use super::emoji_detect;
use super::font_loader;
use super::twemoji_name;

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct GlyphProbe {
    start: usize,
    end: usize,
    font: String,
    content: &'static str,
    w: u32,
    h: u32,
}

fn content_label(c: glyphon::SwashContent) -> &'static str {
    match c {
        glyphon::SwashContent::Mask => "Mask",
        glyphon::SwashContent::SubpixelMask => "SubpixelMask",
        glyphon::SwashContent::Color => "Color",
    }
}

fn shape_probe(
    font_system: &mut glyphon::FontSystem,
    swash: &mut glyphon::SwashCache,
    family: glyphon::Family<'_>,
    text: &str,
    size: f32,
) -> Vec<GlyphProbe> {
    let mut buffer = glyphon::Buffer::new(font_system, glyphon::Metrics::new(size, size * 1.25));
    buffer.set_size(font_system, Some(800.0), Some(80.0));
    buffer.set_text(
        font_system,
        text,
        glyphon::Attrs::new().family(family),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(font_system, false);
    let mut metas: Vec<(usize, usize, glyphon::fontdb::ID, glyphon::CacheKey)> = Vec::new();
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            let physical = g.physical((0.0, 0.0), 1.0);
            metas.push((g.start, g.end, g.font_id, physical.cache_key));
        }
    }
    let mut out = Vec::new();
    for (start, end, font_id, cache_key) in metas {
        let face_name = font_system
            .db()
            .face(font_id)
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .unwrap_or_else(|| "?".into());
        let (content, w, h) = match swash.get_image_uncached(font_system, cache_key) {
            Some(img) => (
                content_label(img.content),
                img.placement.width,
                img.placement.height,
            ),
            None => ("None", 0, 0),
        };
        out.push(GlyphProbe {
            start,
            end,
            font: face_name,
            content,
            w,
            h,
        });
    }
    out
}

fn first_color_glyph_count(probes: &[GlyphProbe]) -> usize {
    probes.iter().filter(|p| p.content == "Color").count()
}

/// Feedback loop: matrix emoji sequences under UI-sans vs color-emoji family.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::emoji_matrix::EMOJI_GPU_COMPAT_SECTION1_SAMPLES;
    use crate::renderer::text_hit::best_font_family;

    const PROBES: &[(&str, &str)] = &[
        ("family", "👨‍👩‍👧‍👦"),
        ("profession", "🧑‍💻"),
        ("skin", "👍🏽"),
        ("flag_cn", "🇨🇳"),
        ("flag_us", "🇺🇸"),
        ("keycap", "1️⃣"),
        ("simple", "😀"),
    ];

    #[test]
    fn grapheme_clusters_stay_intact() {
        for (name, seq) in PROBES {
            let gs: Vec<&str> = seq.graphemes(true).collect();
            assert_eq!(
                gs.len(),
                1,
                "{name}: expected one grapheme, got {gs:?} codepoints={:?}",
                seq.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>()
            );
            assert!(emoji_detect::grapheme_is_emoji(seq), "{name} not detected as emoji");
        }
    }

    #[test]
    fn twemoji_stems_are_stable() {
        assert_eq!(
            twemoji_name::twemoji_stem("👨‍👩‍👧‍👦"),
            "1f468-200d-1f469-200d-1f467-200d-1f466"
        );
        assert_eq!(twemoji_name::twemoji_stem("🇨🇳"), "1f1e8-1f1f3");
        assert_eq!(twemoji_name::twemoji_stem("1️⃣"), "31-20e3");
    }

    #[test]
    fn diagnose_before_vs_after_emoji_font_load() {
        // Production order: shape main buffer via ensure_coverage_for_text (emoji deferred),
        // THEN custom-glyph path calls ensure_emoji_font_fallback. Reproduce both stages.
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        let mut swash = glyphon::SwashCache::new();
        let ui = best_font_family(&fs);

        println!("=== BEFORE ensure_emoji_font_fallback (matches main-buffer shape timing) ===");
        let mut before_rows = Vec::new();
        for (name, seq) in PROBES {
            // Same coverage gate as prepare_text (does not open seguimj by default).
            font_loader::ensure_coverage_for_text(&mut fs, seq);
            let glyphs = shape_probe(&mut fs, &mut swash, ui, seq, 24.0);
            println!(
                "{name}: glyphs={} color={} mask={} | {glyphs:?}",
                glyphs.len(),
                first_color_glyph_count(&glyphs),
                glyphs.iter().filter(|p| p.content == "Mask").count(),
            );
            before_rows.push((*name, glyphs.len(), first_color_glyph_count(&glyphs)));
        }

        font_loader::ensure_emoji_font_fallback(&mut fs);
        let emoji_family = crate::utils::font::resolve_family(
            fs.db(),
            "Segoe UI Emoji, Apple Color Emoji, Noto Color Emoji, emoji, sans-serif",
        );
        println!("=== AFTER ensure_emoji_font_fallback ===");
        for (name, seq) in PROBES {
            let ui_glyphs = shape_probe(&mut fs, &mut swash, ui, seq, 24.0);
            let em_glyphs = shape_probe(&mut fs, &mut swash, emoji_family, seq, 24.0);
            println!(
                "{name}: ui_glyphs={} ui_color={} | em_glyphs={} em_color={} | ui={:?} | em={:?}",
                ui_glyphs.len(),
                first_color_glyph_count(&ui_glyphs),
                em_glyphs.len(),
                first_color_glyph_count(&em_glyphs),
                ui_glyphs,
                em_glyphs
            );
        }

        // Hard signal: family must fragment into >1 glyph under the deferred-emoji path,
        // or flags must yield zero Color (font-fallback reject) — both match the gallery.
        let family_before = before_rows.iter().find(|(n, _, _)| *n == "family").unwrap();
        let flag_before = before_rows.iter().find(|(n, _, _)| *n == "flag_cn").unwrap();
        assert!(
            family_before.1 > 1 || flag_before.2 == 0,
            "expected family multi-glyph or flag zero-Color before emoji font load; got family={:?} flag={:?}",
            family_before,
            flag_before
        );
        let _ = EMOJI_GPU_COMPAT_SECTION1_SAMPLES;
    }

    #[test]
    fn diagnose_ui_sans_vs_emoji_font_glyph_counts() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_emoji_font_fallback(&mut fs);
        let mut swash = glyphon::SwashCache::new();

        let ui = best_font_family(&fs);
        let emoji_family = crate::utils::font::resolve_family(
            fs.db(),
            "Segoe UI Emoji, Apple Color Emoji, Noto Color Emoji, emoji, sans-serif",
        );

        println!("=== emoji shape diag (UI sans vs emoji face) ===");
        let mut ui_splits_complex = false;

        for (name, seq) in PROBES {
            let ui_glyphs = shape_probe(&mut fs, &mut swash, ui, seq, 24.0);
            let em_glyphs = shape_probe(&mut fs, &mut swash, emoji_family, seq, 24.0);
            println!(
                "{name}: ui_glyphs={} color={} | emoji_face_glyphs={} color={} | ui={:?} | em={:?}",
                ui_glyphs.len(),
                first_color_glyph_count(&ui_glyphs),
                em_glyphs.len(),
                first_color_glyph_count(&em_glyphs),
                ui_glyphs,
                em_glyphs
            );
            if matches!(*name, "family" | "flag_cn" | "flag_us" | "keycap") && ui_glyphs.len() > 1
            {
                ui_splits_complex = true;
            }
        }

        assert!(
            ui_splits_complex,
            "expected UI-sans shaping to split at least one complex emoji (repro signal missing — env/fonts changed?)"
        );
    }

    /// Historical signal: Segoe emits 4 Color glyphs for the family ZWJ. Kept to
    /// document why first-Color-only fallback was wrong (see composite test).
    #[test]
    fn font_fallback_first_color_only_drops_rest() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_emoji_font_fallback(&mut fs);
        let mut swash = glyphon::SwashCache::new();
        let emoji_family = crate::utils::font::resolve_family(
            fs.db(),
            "Segoe UI Emoji, Apple Color Emoji, Noto Color Emoji, emoji, sans-serif",
        );

        let probes = shape_probe(&mut fs, &mut swash, emoji_family, "👨‍👩‍👧‍👦", 32.0);
        let color_n = first_color_glyph_count(&probes);
        println!(
            "family: layout_glyphs={} color_glyphs={color_n} probes={probes:?}",
            probes.len()
        );
        assert!(
            color_n > 1,
            "expected Segoe to emit multiple Color glyphs for family (repro signal for composite fallback)"
        );
    }

    #[test]
    fn font_fallback_isolated_fs_covers_matrix_and_keeps_ui_fs_clean() {
        // UI FontSystem must stay free of Segoe UI Emoji after fallback rasters.
        let mut ui_fs = font_loader::create_font_system();
        font_loader::apply_default_families(ui_fs.db_mut());
        assert!(
            !ui_fs
                .db()
                .faces()
                .any(|f| f.families.iter().any(|(n, _)| n == "Segoe UI Emoji")),
            "fresh UI FS should not already contain Segoe UI Emoji"
        );

        for (name, seq) in [
            ("family", "👨‍👩‍👧‍👦"),
            ("profession", "🧑‍💻"),
            ("skin", "👍🏽"),
            ("flag_cn", "🇨🇳"),
            ("keycap", "1️⃣"),
            ("simple", "😀"),
        ] {
            let raster = crate::renderer::prepare_text::load_font_fallback_raster(seq, 32, 400);
            if name == "flag_cn" {
                // Segoe flags are Mask RI letters — must not invent a gray tofu raster.
                assert!(
                    raster.is_none(),
                    "flag_cn: Mask-only RI fallback must be None (Twemoji supplies color flags)"
                );
                println!("flag_cn: fallback correctly rejected");
                continue;
            }
            assert!(
                raster.is_some(),
                "{name}: isolated font fallback must produce a raster (got None)"
            );
            let r = raster.unwrap();
            assert!(r.width > 0 && r.height > 0, "{name}: empty dimensions");
            let nonzero_alpha = r.rgba.chunks(4).filter(|px| px[3] > 0).count();
            assert!(
                nonzero_alpha > 16,
                "{name}: expected ink in fallback raster, alpha_px={nonzero_alpha}"
            );
            println!(
                "{name}: fallback {}x{} ink_px={nonzero_alpha}",
                r.width, r.height
            );
        }

        assert!(
            !ui_fs
                .db()
                .faces()
                .any(|f| f.families.iter().any(|(n, _)| n == "Segoe UI Emoji")),
            "fallback rasters must not pollute the UI FontSystem with Segoe UI Emoji"
        );
    }

    #[test]
    fn twemoji_png_presence_for_matrix() {
        let candidates = [
            std::path::PathBuf::from("assets/twemoji/72x72"),
            std::path::PathBuf::from("crates/rui/assets/twemoji/72x72"),
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/twemoji/72x72"),
        ];
        let dir = candidates.into_iter().find(|p| p.is_dir());
        println!("twemoji_dir={dir:?}");
        let Some(dir) = dir else {
            println!(
                "[warn] Twemoji PNG dir missing — gallery color emoji needs assets/twemoji/72x72 (docs/emoji_gpu_compat_matrix.md §9). Font-fallback isolation test still guards the Color-leak bug."
            );
            return;
        };
        let mut missing = Vec::new();
        for (name, seq) in PROBES {
            let stem = twemoji_name::twemoji_stem(seq);
            let path = dir.join(format!("{stem}.png"));
            let exists = path.is_file();
            println!("{name}: stem={stem} exists={exists} path={path:?}");
            if !exists {
                missing.push(*name);
            }
        }
        assert!(
            missing.is_empty(),
            "Twemoji dir present but PNGs missing for {missing:?}"
        );
    }

    #[test]
    fn twemoji_loads_colorful_flag_when_assets_present() {
        let dir_candidates = [
            std::path::PathBuf::from("assets/twemoji/72x72"),
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/twemoji/72x72"),
        ];
        if dir_candidates.iter().all(|p| !p.is_dir()) {
            println!("skip: Twemoji dir missing");
            return;
        }
        let r = crate::renderer::prepare_text::load_twemoji_raster("🇨🇳", 32)
            .expect("🇨🇳 Twemoji PNG must decode when assets are present");
        assert!(r.width > 0 && r.height > 0);
        let mut chroma = 0.0_f32;
        let mut n = 0.0_f32;
        for px in r.rgba.chunks(4) {
            if px[3] < 32 {
                continue;
            }
            let max = px[0].max(px[1]).max(px[2]) as f32;
            let min = px[0].min(px[1]).min(px[2]) as f32;
            chroma += max - min;
            n += 1.0;
        }
        let c = if n == 0.0 { 0.0 } else { chroma / n };
        println!("🇨🇳 twemoji {}x{} chroma={c:.1}", r.width, r.height);
        assert!(c >= 24.0, "Twemoji flag must be colorful, chroma={c}");
    }

    #[test]
    fn flag_emoji_must_not_paint_regional_indicator_tofu() {
        // Windows Segoe often only exposes Mask RI letters for 🇨🇳 via swash.
        // Painting those as custom glyphs looks like faint gray "CN"/"US" (gallery 6b).
        for seq in ["🇨🇳", "🇺🇸"] {
            let paint = crate::renderer::prepare_text::emoji_paint_pixel_size(14.0);
            let over = paint.saturating_mul(2).clamp(paint, 128);
            let raw = crate::renderer::prepare_text::load_font_fallback_raster(seq, over, 400);
            assert!(
                raw.is_none(),
                "{seq}: Mask-only RI fallback must be rejected (got {}x{})",
                raw.as_ref().map(|r| r.width).unwrap_or(0),
                raw.as_ref().map(|r| r.height).unwrap_or(0)
            );
            let stem = crate::renderer::twemoji_name::twemoji_stem(seq);
            let candidates = [
                std::path::PathBuf::from("assets/twemoji/72x72"),
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/twemoji/72x72"),
            ];
            if let Some(dir) = candidates.into_iter().find(|p| p.is_dir()) {
                let path = dir.join(format!("{stem}.png"));
                assert!(
                    path.is_file(),
                    "Twemoji dir exists but flag PNG missing: {path:?}"
                );
                println!("{seq}: font fallback rejected; Twemoji PNG ok ({path:?})");
            } else {
                println!(
                    "{seq}: font fallback rejected; sync assets/twemoji/72x72 for gallery flags"
                );
            }
        }
    }

    #[test]
    fn font_fallback_composites_family_not_first_glyph_only() {
        let family = crate::renderer::prepare_text::load_font_fallback_raster("👨‍👩‍👧‍👦", 32, 400)
            .expect("family fallback raster");
        // First-Color-only returned ~16x32; a composite of the four CBDT layers is wider
        // and/or has substantially more ink than a single adult glyph.
        let ink = family.rgba.chunks(4).filter(|px| px[3] > 0).count();
        println!("family composite {}x{} ink={ink}", family.width, family.height);
        assert!(
            family.width >= 24 && ink > 400,
            "expected composited family raster, got {}x{} ink={ink}",
            family.width,
            family.height
        );
    }

    #[test]
    fn face_emoji_raster_has_ink_near_bottom_edge() {
        // If we clip even a few px off a full-bleed circular face, it looks "shaved flat".
        let r = crate::renderer::prepare_text::load_font_fallback_raster("😀", 32, 400)
            .expect("grinning face fallback");
        let w = r.width as usize;
        let h = r.height as usize;
        let bottom_rows = 3usize.min(h);
        let mut ink_bottom = 0usize;
        for row in (h - bottom_rows)..h {
            for col in 0..w {
                if r.rgba[(row * w + col) * 4 + 3] > 32 {
                    ink_bottom += 1;
                }
            }
        }
        println!("😀 {}x{} ink_in_bottom_{bottom_rows}_rows={ink_bottom}", w, h);
        assert!(
            ink_bottom > 0,
            "face emoji ink reaches the bottom of the raster — TextBounds must not shave those rows"
        );
    }

    #[test]
    fn custom_emoji_bottom_must_fit_leaf_clip_with_leading_normal() {
        // Production: leaf height ≈ line_height (leading-normal). Custom quad is em-square,
        // centered in the line. Any clip shorter than line_height will flatten faces.
        let size = 14.0_f32;
        let lh = size * 1.5;
        let emoji_h = size;
        let top = (lh - emoji_h) * 0.5;
        let bottom = top + emoji_h;
        // Simulated tight clip mistaken for "font-size tall" leaf (leading-none residual).
        let tight_clip_h = size;
        let overshoot_if_tight = bottom - tight_clip_h;
        println!("lh={lh} emoji=[{top},{bottom}] tight_clip={tight_clip_h} overshoot={overshoot_if_tight}");
        assert!(
            overshoot_if_tight > 1.0,
            "documents the failure mode when leaf clip ≈ font-size"
        );
        // Correct clip must be at least line_height.
        assert!(bottom <= lh + 0.01);
    }

    #[test]
    fn custom_emoji_quad_vs_line_box_clip_signal() {
        // Centered placement must keep the em-square inside the line box.
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        let ui = best_font_family(&fs);
        let size = 14.0_f32;
        let lh = size * 1.5;
        let text = "Hello 😀 world";
        let layout = crate::renderer::emoji_layout::plan_emoji_layout_text(text);
        let emoji_start = layout
            .find(crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT)
            .unwrap();

        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(&mut fs, Some(400.0), Some(lh + 2.0));
        buffer.set_text(
            &mut fs,
            &layout,
            glyphon::Attrs::new().family(ui),
            glyphon::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut fs, false);

        let (left, top) = crate::renderer::emoji_layout::custom_emoji_left_centered_top(
            &buffer, &layout, emoji_start, size, size,
        )
        .expect("emoji slot position");
        let bottom = top + size;
        let mut line_top = 0.0_f32;
        let mut line_bottom = lh;
        for run in buffer.layout_runs() {
            line_top = run.line_top;
            line_bottom = run.line_top + run.line_height;
        }
        println!(
            "centered left={left:.2} top={top:.2} bottom={bottom:.2} | line_top={line_top:.2} line_bottom={line_bottom:.2}"
        );
        assert!(
            top >= line_top - 0.1 && bottom <= line_bottom + 0.1,
            "centered emoji must fit in line box; top={top} bottom={bottom} line=[{line_top},{line_bottom}]"
        );
    }

    /// With EM SPACE slots, ZWJ clusters must not reserve >>1em.
    #[test]
    fn gallery_emoji_row_slot_plan_collapses_zwj_advance() {
        let line = "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦 🧑‍💻 🏳️‍🌈";
        let layout = crate::renderer::emoji_layout::plan_emoji_layout_text(line);
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_coverage_for_text(&mut fs, line);
        let ui = best_font_family(&fs);
        let size = 14.0_f32;
        let lh = size * 1.5;

        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(&mut fs, Some(1200.0), Some(lh * 2.0));
        buffer.set_text(
            &mut fs,
            &layout,
            glyphon::Attrs::new().family(ui),
            glyphon::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut fs, false);

        let mut byte = 0usize;
        for g in line.graphemes(true) {
            let is_emoji = emoji_detect::grapheme_is_emoji(g);
            let start = byte;
            if is_emoji {
                byte += crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT.len_utf8();
            } else {
                byte += g.len();
            }
            if !is_emoji {
                continue;
            }
            let mut advance = 0.0_f32;
            for run in buffer.layout_runs() {
                for lg in run.glyphs.iter() {
                    if start >= lg.start && start < lg.end {
                        advance += lg.w;
                    }
                }
            }
            let ratio = advance / size;
            let name = twemoji_name::twemoji_stem(g);
            println!("slot emoji={name} advance={advance:.1} ratio={ratio:.2}");
            assert!(
                ratio < 1.35,
                "{name}: expected ~1em slot advance after plan, got ratio={ratio}"
            );
        }
        let _ = EMOJI_GPU_COMPAT_SECTION1_SAMPLES;
    }

    #[test]
    fn face_emoji_colr_raster_does_not_shave_bottom() {
        // Regression for swash ColorOutline ceil(min.y) crop on Segoe COLR faces.
        // Intact circle: last ink row is a short chord; a shaved face leaves a wide flat edge.
        let r = crate::renderer::prepare_text::load_font_fallback_raster("😀", 64, 400)
            .expect("grinning face");
        let w = r.width as usize;
        let h = r.height as usize;
        let row_ink = |row: usize| {
            (0..w)
                .filter(|&x| r.rgba[(row * w + x) * 4 + 3] > 32)
                .count()
        };
        let mut last = 0usize;
        let mut max_row = 0usize;
        for row in 0..h {
            let n = row_ink(row);
            max_row = max_row.max(n);
            if n > 0 {
                last = row;
            }
        }
        let chord = row_ink(last);
        println!("😀 COLR {w}x{h} max_row={max_row} last={last} chord={chord}");
        assert!(max_row > 0);
        assert!(
            (chord as f32) < (max_row as f32) * 0.55,
            "bottom must taper, not shave flat; chord={chord} max={max_row}"
        );
    }

    #[test]
    fn measure_fitted14_yellow_fill() {
        fn yellow_fill(rgba: &[u8], w: usize, h: usize) -> (usize, usize, f32, f32) {
            let mut minx = w;
            let mut miny = h;
            let mut maxx = 0usize;
            let mut maxy = 0usize;
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    let (r, g, b, a) = (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]);
                    if a > 40 && r > 180 && g > 140 && b < 100 {
                        minx = minx.min(x);
                        miny = miny.min(y);
                        maxx = maxx.max(x);
                        maxy = maxy.max(y);
                    }
                }
            }
            let bw = maxx + 1 - minx;
            let bh = (maxy + 1 - miny).max(1);
            (
                bw,
                bh,
                bw as f32 / bh as f32,
                (bw.max(bh) as f32) / (w as f32),
            )
        }
        for seq in ["😀", "😂", "🤣"] {
            let paint = crate::renderer::prepare_text::emoji_paint_pixel_size(14.0);
            let over = paint.saturating_mul(2).clamp(paint, 128);
            let raw =
                crate::renderer::prepare_text::load_font_fallback_raster(seq, over, 400).unwrap();
            let fitted =
                crate::renderer::prepare_text::fit_emoji_to_pixel_square(&raw, paint);
            let (bw, bh, ar, fill) =
                yellow_fill(&fitted.rgba, fitted.width as usize, fitted.height as usize);
            println!(
                "{seq} paint={paint} yellow={bw}x{bh} aspect={ar:.3} max_dim/em={fill:.3}"
            );
            assert!(
                fill >= 0.82,
                "{seq}: yellow face must fill most of the paint square, fill={fill}"
            );
            // Tilted glyphs (🤣) have non-square yellow AABB even when optically round.
            assert!(
                (ar - 1.0).abs() < 0.22,
                "{seq}: yellow face must stay roughly round, aspect={ar}"
            );
        }
    }

    #[test]
    fn rofl_emoji_bottom_not_shaved_after_letterbox_and_line_clip() {
        // Gallery: leading-normal Label, custom emoji = full em-square centered in line.
        // 🤣 is tilted — ink often touches the texture bottom; any TextBounds overshoot
        // shaves the tears into a flat chord (user: "第三个底部被削平").
        let size = 14.0_f32;
        let lh = size * 1.5;
        let pixel = crate::renderer::prepare_text::emoji_paint_pixel_size(size);
        let over = pixel.saturating_mul(2).clamp(pixel, 128);
        let raw = crate::renderer::prepare_text::load_font_fallback_raster("🤣", over, 400)
            .expect("rofl");
        let fitted = crate::renderer::prepare_text::fit_emoji_to_pixel_square(&raw, pixel);
        let dst = fitted.width as usize;
        let last_row_ink = (0..dst)
            .filter(|&x| fitted.rgba[((dst - 1) * dst + x) * 4 + 3] > 32)
            .count();
        let first_row_ink = (0..dst)
            .filter(|&x| fitted.rgba[x * 4 + 3] > 32)
            .count();
        let top = (lh - pixel as f32).max(0.0) * 0.5;
        let bottom = top + pixel as f32;
        let overshoot = (bottom - lh).max(0.0);
        println!(
            "🤣 raw={}x{} fitted={}x{} last_row_ink={last_row_ink} first_row_ink={first_row_ink} \
             quad=[{top:.2},{bottom:.2}] lh={lh} overshoot={overshoot}",
            raw.width, raw.height, fitted.width, fitted.height
        );
        assert_eq!(
            last_row_ink, 0,
            "fitted 🤣 must not put ink on the last row (got {last_row_ink})"
        );
        assert_eq!(
            first_row_ink, 0,
            "fitted 🤣 must keep top pad (got {first_row_ink})"
        );
        assert!(
            overshoot < 0.5,
            "centered em-square must fit leading-normal line box"
        );
    }
}
