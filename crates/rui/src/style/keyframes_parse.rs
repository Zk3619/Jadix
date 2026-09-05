//! Parse CSS `@keyframes { ... }` text into [`register_keyframes`].

use super::animatable::{AnimatableValue, PropertyId, parse_css_length_px};
use super::element_style::TimingFunction;
use super::keyframes::{KeyframeStop, register_keyframes};
use super::{parse_color, parse_filter, parse_transform};

/// Parse one or more `@keyframes name { ... }` rules and register them.
/// Returns registered names.
pub fn parse_and_register_keyframes(css: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut rest = css;
    loop {
        let Some(at) = rest.find("@keyframes") else {
            break;
        };
        rest = &rest[at + "@keyframes".len()..];
        let rest_trim = rest.trim_start();
        let name_end = rest_trim
            .find(|c: char| c == '{' || c.is_whitespace())
            .ok_or_else(|| "expected keyframe name".to_string())?;
        let name = rest_trim[..name_end].trim();
        if name.is_empty() {
            return Err("empty @keyframes name".into());
        }
        let after_name = rest_trim[name_end..].trim_start();
        let body_start = after_name
            .find('{')
            .ok_or_else(|| format!("@keyframes {name}: missing block"))?;
        let (block, consumed) = extract_brace_block(&after_name[body_start..])?;
        let stops = parse_keyframe_block(block)?;
        register_keyframes(name, stops);
        names.push(name.to_string());
        rest = &after_name[body_start + consumed..];
    }
    if names.is_empty() {
        return Err("no @keyframes rule found".into());
    }
    Ok(names)
}

fn extract_brace_block(s: &str) -> Result<(&str, usize), String> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("expected '{'".into());
    }
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&s[1..i], i + 1));
                }
            }
            _ => {}
        }
    }
    Err("unclosed {".into())
}

fn parse_keyframe_block(body: &str) -> Result<Vec<KeyframeStop>, String> {
    let mut stops: Vec<KeyframeStop> = Vec::new();
    let mut rest = body;
    while let Some(rel) = rest.find('{') {
        let sel = rest[..rel].trim();
        if sel.is_empty() {
            rest = &rest[rel + 1..];
            continue;
        }
        let (decl, n) = extract_brace_block(&rest[rel..])?;
        let offsets = parse_offsets(sel)?;
        let (props, timing) = parse_declarations(decl);
        for off in offsets {
            let mut stop = KeyframeStop::at(off).with_props(props.clone());
            if let Some(t) = timing {
                stop = stop.timing(t);
            }
            stops.push(stop);
        }
        rest = &rest[rel + n..];
    }
    Ok(stops)
}

fn parse_offsets(sel: &str) -> Result<Vec<f32>, String> {
    let mut out = Vec::new();
    for part in sel.split(',') {
        let p = part.trim().to_ascii_lowercase();
        if p.is_empty() {
            continue;
        }
        let off = match p.as_str() {
            "from" => 0.0,
            "to" => 1.0,
            s if s.ends_with('%') => {
                let n: f32 = s
                    .trim_end_matches('%')
                    .trim()
                    .parse()
                    .map_err(|_| format!("bad offset {s}"))?;
                (n / 100.0).clamp(0.0, 1.0)
            }
            _ => return Err(format!("bad keyframe selector `{part}`")),
        };
        out.push(off);
    }
    Ok(out)
}

fn parse_declarations(
    decl: &str,
) -> (
    std::collections::BTreeMap<PropertyId, AnimatableValue>,
    Option<TimingFunction>,
) {
    let mut props = std::collections::BTreeMap::new();
    let mut timing = None;
    for line in decl.split(';') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        if key.eq_ignore_ascii_case("animation-timing-function")
            || key.eq_ignore_ascii_case("animation-timing")
        {
            timing = Some(TimingFunction::from_css(val));
            continue;
        }
        if let Some(id) = PropertyId::from_css(key) {
            if let Some(av) = parse_declared_value(id, val) {
                props.insert(id, av);
            }
        }
    }
    (props, timing)
}

pub(crate) fn parse_declared_value(id: PropertyId, raw: &str) -> Option<AnimatableValue> {
    match id {
        PropertyId::Opacity | PropertyId::Zoom | PropertyId::FlexGrow | PropertyId::FlexShrink => {
            raw.trim().trim_end_matches('%').parse().ok().map(|n: f32| {
                if raw.contains('%') {
                    AnimatableValue::Scalar(n / 100.0)
                } else {
                    AnimatableValue::Scalar(n)
                }
            })
        }
        PropertyId::GridTemplateRows | PropertyId::GridTemplateColumns => {
            super::animatable::parse_fr_tracks(raw)
        }
        PropertyId::BackgroundPosition => {
            super::animatable::parse_background_position(raw).map(AnimatableValue::Vec2)
        }
        PropertyId::ClipPath => super::clip_path::parse_clip_path(raw).map(AnimatableValue::Clip),
        PropertyId::TransformOrigin => parse_origin_frac(raw),
        PropertyId::OffsetDistance => parse_offset_distance(raw),
        PropertyId::OffsetRotate => parse_angle_deg(raw).map(AnimatableValue::Scalar),
        PropertyId::Rotate => parse_angle_deg(raw).map(AnimatableValue::Scalar),
        PropertyId::Scale => parse_scale(raw),
        PropertyId::Translate => parse_translate(raw),
        PropertyId::TranslatePct => parse_translate_pct(raw),
        PropertyId::Transform => parse_transform_value(raw),
        PropertyId::Filter => Some(AnimatableValue::Filter(parse_filter(raw))),
        PropertyId::BackgroundColor
        | PropertyId::BorderColor
        | PropertyId::OutlineColor
        | PropertyId::Color
        | PropertyId::TextDecorationColor => Some(AnimatableValue::Color(parse_color(raw))),
        PropertyId::MaxHeight
        | PropertyId::MaxWidth
        | PropertyId::Height
        | PropertyId::Width
        | PropertyId::MinHeight
        | PropertyId::MinWidth
        | PropertyId::FontSize
        | PropertyId::LineHeight
        | PropertyId::LetterSpacing
        | PropertyId::OutlineWidth
        | PropertyId::OutlineOffset
        | PropertyId::PaddingTop
        | PropertyId::PaddingRight
        | PropertyId::PaddingBottom
        | PropertyId::PaddingLeft
        | PropertyId::MarginTop
        | PropertyId::MarginRight
        | PropertyId::MarginBottom
        | PropertyId::MarginLeft
        | PropertyId::Top
        | PropertyId::Right
        | PropertyId::Bottom
        | PropertyId::Left
        | PropertyId::RowGap
        | PropertyId::ColumnGap
        | PropertyId::BorderTopWidth
        | PropertyId::BorderRightWidth
        | PropertyId::BorderBottomWidth
        | PropertyId::BorderLeftWidth
        | PropertyId::FlexBasis
        | PropertyId::BackdropFilter
        | PropertyId::BorderTopLeftRadius
        | PropertyId::BorderTopRightRadius
        | PropertyId::BorderBottomRightRadius
        | PropertyId::BorderBottomLeftRadius => {
            if raw.eq_ignore_ascii_case("auto") {
                Some(AnimatableValue::Auto)
            } else if let Some(d) = crate::layout::dimension::parse_css_dim(raw) {
                if d.is_calc_size() || d.contains_size_keyword() {
                    Some(AnimatableValue::Dim(d))
                } else {
                    parse_css_length_px(raw).map(AnimatableValue::Scalar)
                }
            } else {
                parse_css_length_px(raw).map(AnimatableValue::Scalar)
            }
        }
        PropertyId::BorderRadius => parse_radii(raw),
        PropertyId::Padding | PropertyId::Margin | PropertyId::Inset | PropertyId::Gap => {
            parse_box_quad(raw)
        }
        PropertyId::BoxShadow => parse_shadow(raw),
        PropertyId::BorderWidth => parse_box_quad(raw),
    }
}

fn parse_origin_frac(raw: &str) -> Option<AnimatableValue> {
    let origin = super::transform::parse_transform_origin(raw);
    Some(AnimatableValue::Vec2(super::transform::origin_frac(
        &origin,
    )))
}

fn parse_offset_distance(raw: &str) -> Option<AnimatableValue> {
    let s = raw.trim();
    if let Some(n) = s.strip_suffix('%') {
        return n
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| AnimatableValue::Scalar((v / 100.0).clamp(0.0, 1.0)));
    }
    s.parse::<f32>()
        .ok()
        .map(|v| AnimatableValue::Scalar(v.clamp(0.0, 1.0)))
}

fn parse_angle_deg(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("deg") {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("turn") {
        return n.trim().parse::<f32>().ok().map(|v| v * 360.0);
    }
    if let Some(n) = s.strip_suffix("rad") {
        return n.trim().parse::<f32>().ok().map(|v| v.to_degrees());
    }
    s.parse().ok()
}

fn parse_scale(s: &str) -> Option<AnimatableValue> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("scale(").and_then(|r| r.strip_suffix(')')) {
        let mut it = inner.split(',').map(|p| p.trim());
        let x: f32 = it.next()?.parse().ok()?;
        let y = it.next().and_then(|p| p.parse().ok()).unwrap_or(x);
        return Some(AnimatableValue::Vec2([x, y]));
    }
    let n: f32 = s.parse().ok()?;
    Some(AnimatableValue::Vec2([n, n]))
}

fn parse_translate(s: &str) -> Option<AnimatableValue> {
    let s = s.trim();
    if let Some(inner) = s
        .strip_prefix("translate(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let mut parts = inner.split(',').map(|p| p.trim());
        let x = parse_css_length_px(parts.next()?)?;
        let y = parts.next().and_then(parse_css_length_px).unwrap_or(0.0);
        return Some(AnimatableValue::Vec2([x, y]));
    }
    parse_css_length_px(s).map(|x| AnimatableValue::Vec2([x, 0.0]))
}

fn parse_translate_pct(s: &str) -> Option<AnimatableValue> {
    let s = s.trim();
    let mut parts = s.split_whitespace();
    let x = parse_pct_frac(parts.next()?)?;
    let y = parts
        .next()
        .and_then(parse_pct_frac)
        .unwrap_or(0.0);
    Some(AnimatableValue::Vec2([x, y]))
}

fn parse_pct_frac(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('%') {
        return n.trim().parse::<f32>().ok().map(|v| v / 100.0);
    }
    s.parse().ok()
}

fn parse_transform_value(s: &str) -> Option<AnimatableValue> {
    let list = parse_transform(s)?;
    let mut tx = 0.0f32;
    let mut ty = 0.0f32;
    let mut sx = 1.0f32;
    let mut sy = 1.0f32;
    let mut rot = 0.0f32;
    for f in &list {
        use super::transform::TransformFunction;
        match f {
            TransformFunction::Translate(x, y) => {
                if let crate::layout::dimension::CssDim::Px(v) = x {
                    tx += *v;
                }
                if let crate::layout::dimension::CssDim::Px(v) = y {
                    ty += *v;
                }
            }
            TransformFunction::TranslateX(x) => {
                if let crate::layout::dimension::CssDim::Px(v) = x {
                    tx += *v;
                }
            }
            TransformFunction::TranslateY(y) => {
                if let crate::layout::dimension::CssDim::Px(v) = y {
                    ty += *v;
                }
            }
            TransformFunction::Scale(x, y) => {
                sx *= *x;
                sy *= y.unwrap_or(*x);
            }
            TransformFunction::ScaleX(x) => sx *= *x,
            TransformFunction::ScaleY(y) => sy *= *y,
            TransformFunction::Rotate(angle) => rot += angle_to_deg(angle),
            TransformFunction::RotateZ(angle) => rot += angle_to_deg(angle),
            _ => {}
        }
    }
    Some(AnimatableValue::Transform([tx, ty, sx, sy, rot]))
}

fn angle_to_deg(a: &crate::layout::dimension::CssAngle) -> f32 {
    match a {
        crate::layout::dimension::CssAngle::Deg(v) => *v,
        crate::layout::dimension::CssAngle::Turn(v) => *v * 360.0,
        crate::layout::dimension::CssAngle::Rad(v) => v.to_degrees(),
        crate::layout::dimension::CssAngle::Grad(v) => *v * 0.9,
    }
}

fn parse_radii(s: &str) -> Option<AnimatableValue> {
    let nums: Vec<f32> = s
        .split_whitespace()
        .filter_map(parse_css_length_px)
        .collect();
    match nums.as_slice() {
        [a] => Some(AnimatableValue::Vec4([*a, *a, *a, *a])),
        [a, b] => Some(AnimatableValue::Vec4([*a, *b, *a, *b])),
        [a, b, c] => Some(AnimatableValue::Vec4([*a, *b, *c, *b])),
        [a, b, c, d, ..] => Some(AnimatableValue::Vec4([*a, *b, *c, *d])),
        _ => None,
    }
}

fn parse_box_quad(s: &str) -> Option<AnimatableValue> {
    parse_radii(s)
}

fn parse_shadow(s: &str) -> Option<AnimatableValue> {
    let layers = super::parse_box_shadows(s);
    let sh = layers.into_iter().next()?;
    Some(AnimatableValue::Shadow([
        sh.color[0],
        sh.color[1],
        sh.color[2],
        sh.color[3],
        sh.offset[0],
        sh.offset[1],
        sh.blur,
        sh.spread,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::keyframes::{lookup_keyframes, sample_keyframes};

    #[test]
    fn parses_from_to_opacity() {
        parse_and_register_keyframes(
            "@keyframes fade-x { from { opacity: 0; } to { opacity: 1; } }",
        )
        .unwrap();
        let stops = lookup_keyframes("fade-x").unwrap();
        let mid = sample_keyframes(&stops, 0.5);
        assert!((mid.opacity.unwrap() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn parses_percent_and_color() {
        parse_and_register_keyframes(
            "@keyframes flash-bg { 0% { background-color: #000000; } 100% { background-color: #ffffff; } }",
        )
        .unwrap();
        let stops = lookup_keyframes("flash-bg").unwrap();
        assert!(
            stops
                .iter()
                .any(|s| s.get(PropertyId::BackgroundColor).is_some())
        );
    }
}
