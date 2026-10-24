use latex_rust::{Color, Dim, MathFont, MathStyle, SvgOptions};
use std::{
    cell::{OnceCell, RefCell},
    collections::VecDeque,
};

#[derive(Clone)]
pub struct RenderedMath {
    pub svg: String,
    pub width: f32,
    pub height: f32,
}
type CacheKey = (String, bool, u32, [u8; 4]);
thread_local! {
    static FONT: OnceCell<Result<MathFont, String>> = const { OnceCell::new() };
    static CACHE: RefCell<VecDeque<(CacheKey, Result<RenderedMath, String>)>> = const { RefCell::new(VecDeque::new()) };
}

pub fn render_svg(
    latex: &str,
    display: bool,
    size: f32,
    color: [u8; 4],
) -> Result<RenderedMath, String> {
    if latex.len() > 64 * 1024 {
        return Err("Formula exceeds the 64 KiB rendering limit".into());
    }
    let size = size.clamp(8.0, 96.0);
    let key = (latex.to_string(), display, size.to_bits(), color);
    if let Some(cached) = CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.clone())
    }) {
        return cached;
    }
    let result = FONT.with(|font| -> Result<RenderedMath, String> {
        let font = font
            .get_or_init(|| MathFont::stix_two_math().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(Clone::clone)?;
        let ast = latex_rust::parse(latex).map_err(|error| error.to_string())?;
        let options = SvgOptions {
            display,
            font_size_pt: Dim::ratio((size * 75.0).round() as i64, 100),
            color: Color::rgb(color[0], color[1], color[2]),
        };
        let tree = latex_rust::layout_with_em_size_pt(
            &ast,
            font,
            if display {
                MathStyle::Display
            } else {
                MathStyle::Text
            },
            &options.font_size_pt,
        )
        .map_err(|error| error.to_string())?;
        let width = f32::from_bits(tree.width.to_ieee32_bits()) * size;
        let height = f32::from_bits((&tree.height + &tree.depth).to_ieee32_bits()) * size;
        if !width.is_finite() || !height.is_finite() || width > 32768.0 || height > 8192.0 {
            return Err("Formula dimensions exceed the rendering budget".into());
        }
        let svg =
            latex_rust::render_svg(&tree, font, &options).map_err(|error| error.to_string())?;
        let svg = svg
            .strip_prefix("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
            .unwrap_or(&svg)
            .trim_start();
        Ok(RenderedMath {
            svg: svg.into(),
            width: width.max(1.0),
            height: height.max(1.0),
        })
    });
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 64 {
            cache.pop_front();
        }
        cache.push_back((key, result.clone()));
    });
    result
}
