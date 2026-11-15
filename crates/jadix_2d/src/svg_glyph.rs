//! Static SVG-in-OpenType glyphs. No file, network or system font resolver.
use crate::*;
use resvg::{tiny_skia, usvg};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

pub(crate) fn prepare_svg(
    doc: ttf_parser::svg::SvgDocument<'_>,
    id: u16,
    upem: u16,
    size: f32,
    palette: &[[f32; 4]],
    options: ColorGlyphOptions,
    byte_budget: usize,
) -> Result<(Scene2D, [f32; 4], usize), ColorGlyphError> {
    if doc.data.len() > options.max_svg_bytes {
        return Err(ColorGlyphError::SvgBudget);
    }
    let mut data = Vec::new();
    if doc.data.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(doc.data)
            .take(options.max_svg_bytes as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| ColorGlyphError::MalformedSvg)?;
    } else {
        data.extend_from_slice(doc.data);
    }
    if data.len() > options.max_svg_bytes {
        return Err(ColorGlyphError::SvgBudget);
    }
    let text = std::str::from_utf8(&data).map_err(|_| ColorGlyphError::MalformedSvg)?;
    let text = palette_vars(text, palette)?;
    if text.len() > options.max_svg_bytes {
        return Err(ColorGlyphError::SvgBudget);
    }
    let xml = usvg::roxmltree::Document::parse(&text).map_err(|_| ColorGlyphError::MalformedSvg)?;
    if xml.descendants().count() > options.limits.max_commands {
        return Err(ColorGlyphError::SvgBudget);
    }
    for n in xml.descendants().filter(|n| n.is_element()) {
        if n.ancestors().count() > options.limits.max_nesting
            || matches!(
                n.tag_name().name(),
                "text"
                    | "script"
                    | "animate"
                    | "animateTransform"
                    | "animateMotion"
                    | "set"
                    | "foreignObject"
                    | "feImage"
            )
        {
            return Err(ColorGlyphError::UnsupportedSvg);
        }
        for a in n.attributes() {
            if a.name() == "href"
                && !a.value().starts_with('#')
                && !(n.tag_name().name() == "image" && a.value().starts_with("data:"))
            {
                return Err(ColorGlyphError::UnsupportedSvg);
            }
        }
        if n.tag_name().name() == "style" && n.text().is_some_and(|t| t.contains("@import")) {
            return Err(ColorGlyphError::UnsupportedSvg);
        }
    }
    // OpenType selects a glyph as if referenced by <use>, including nodes in defs.
    // Let usvg resolve inheritance/references and the root viewBox itself.
    let root = xml.root_element();
    let glyph_id = format!("glyph{id}");
    if !xml
        .descendants()
        .any(|n| n.attribute("id") == Some(glyph_id.as_str()))
    {
        return Err(ColorGlyphError::MalformedSvg);
    }
    let mut quote = None;
    let open = text[root.range().start..]
        .char_indices()
        .find_map(|(i, c)| {
            if Some(c) == quote {
                quote = None;
            } else if quote.is_none() {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '>' {
                    return Some(root.range().start + i + 1);
                }
            }
            None
        })
        .ok_or(ColorGlyphError::MalformedSvg)?;
    // Unlike standalone SVG, an absent width/height uses the em viewport even
    // when a differently sized viewBox exists. Do not infer it from viewBox.
    let mut prefix = text[..open - 1].to_owned();
    for attr in ["width", "height"] {
        if root.attribute(attr).is_none() {
            prefix.push_str(&format!(" {attr}=\"{upem}\""));
        }
    }
    prefix.push('>');
    let selected = if root.attribute("id") == Some(glyph_id.as_str()) {
        format!("{prefix}{}", &text[open..])
    } else {
        let close = text[..root.range().end]
            .rfind("</")
            .ok_or(ColorGlyphError::MalformedSvg)?;
        format!(
            "{prefix}<defs>{}</defs><use href=\"#{glyph_id}\"/>{}",
            &text[open..close],
            &text[close..]
        )
    };
    let rejected = Arc::new(AtomicBool::new(false));
    let decoded = Arc::new(AtomicUsize::new(0));
    let rejected_data = rejected.clone();
    let rejected_string = rejected.clone();
    let [r, g, b, a] = options
        .foreground
        .map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
    let opts = usvg::Options {
        default_size: usvg::Size::from_wh(upem as f32, upem as f32)
            .ok_or(ColorGlyphError::MalformedSvg)?,
        style_sheet: Some(format!("svg {{color: #{r:02x}{g:02x}{b:02x}{a:02x}; overflow: visible !important; clip: auto !important;}}")),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(move |_, _| {
                rejected_string.store(true, Ordering::Relaxed);
                None
            }),
            resolve_data: Box::new(move |mime, data, opts| {
                let dims = ::image::ImageReader::new(std::io::Cursor::new(data.as_slice()))
                    .with_guessed_format()
                    .ok()
                    .and_then(|r| r.into_dimensions().ok());
                if let Some((w, h)) = dims {
                    let bytes = (w as usize).saturating_mul(h as usize).saturating_mul(4);
                    if w > 2048
                        || h > 2048
                        || decoded
                            .fetch_add(bytes, Ordering::Relaxed)
                            .saturating_add(bytes)
                            > byte_budget
                    {
                        rejected_data.store(true, Ordering::Relaxed);
                        return None;
                    }
                    if ::image::load_from_memory(&data).is_err() {
                        rejected_data.store(true, Ordering::Relaxed);
                        return None;
                    }
                    let result = usvg::ImageHrefResolver::default_data_resolver()(mime, data, opts);
                    if result.is_none() {
                        rejected_data.store(true, Ordering::Relaxed);
                    }
                    result
                } else {
                    rejected_data.store(true, Ordering::Relaxed);
                    None
                }
            }),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&selected, &opts).map_err(|_| ColorGlyphError::MalformedSvg)?;
    if rejected.load(Ordering::Relaxed) {
        return Err(ColorGlyphError::UnsupportedSvg);
    }
    let bbox = tree.root().abs_layer_bounding_box();
    if [bbox.x(), bbox.y(), bbox.width(), bbox.height()]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1e6)
    {
        return Err(ColorGlyphError::MalformedSvg);
    }
    let scale = size / upem as f32 * options.raster_scale;
    let width = (bbox.width() * scale).ceil();
    let height = (bbox.height() * scale).ceil();
    if !(1. ..=2048.).contains(&width)
        || !(1. ..=2048.).contains(&height)
        || width * height * 4. > byte_budget as f32
    {
        return Err(ColorGlyphError::SvgBudget);
    }
    let (width, height) = (width as u32, height as u32);
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(ColorGlyphError::SvgBudget)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale).pre_translate(-bbox.x(), -bbox.y()),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    for p in rgba.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = (u32::from(*c) * 255 + a / 2)
                .checked_div(a)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    let image = Image2D::from_rgba8(width, height, &rgba).ok_or(ColorGlyphError::MalformedSvg)?;
    let bytes = rgba.len();
    let x = bbox.x();
    let y = bbox.y();
    let w = width as f32 / scale;
    let h = height as f32 / scale;
    let scene = Scene2D::record_with_limits([1, 1], options.limits, |c| {
        c.scale(1., -1.);
        c.set_image_sampling(ImageSampling::Bilinear);
        c.draw_image(&CanvasImageSource::Image(&image), x, y, w, h);
    })?;
    Ok((scene, [x, -y - h, x + w, -y], bytes))
}

fn palette_vars(text: &str, palette: &[[f32; 4]]) -> Result<String, ColorGlyphError> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("var(") {
        output.push_str(&rest[..at]);
        rest = &rest[at + 4..];
        let mut depth = 1;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
        }
        let end = end.ok_or(ColorGlyphError::MalformedSvg)?;
        let (name, fallback) = rest[..end]
            .split_once(',')
            .map_or((&rest[..end], None), |(n, f)| (n, Some(f)));
        let index = name
            .trim()
            .strip_prefix("--color")
            .and_then(|n| n.parse::<usize>().ok())
            .ok_or(ColorGlyphError::UnsupportedSvg)?;
        if let Some(c) = palette.get(index) {
            let [r, g, b, a] = c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
            output.push_str(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"));
        } else if let Some(f) = fallback {
            output.push_str(f.trim());
        } else {
            return Err(ColorGlyphError::InvalidPalette);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}
