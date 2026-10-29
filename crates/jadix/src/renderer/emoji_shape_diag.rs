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
    buffer.set_size(Some(800.0), Some(80.0));
    buffer.set_text(
        text,
        &glyphon::Attrs::new().family(family),
        glyphon::Shaping::Advanced,
        None,
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
#[path = "tests/emoji_shape_diag.rs"]
mod tests;
