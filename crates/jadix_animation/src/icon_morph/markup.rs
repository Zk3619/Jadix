//! SVG markup → [`IconInput`] (morphicons `svgToIcon`).

use std::collections::HashMap;

use super::normalize::fit_icon;
use super::types::{morph_err, IconInput, MorphError};

const GEOMETRY: &[&str] = &[
    "path", "line", "circle", "ellipse", "rect", "polyline", "polygon",
];
const SKIP: &[&str] = &[
    "svg", "g", "title", "desc", "defs", "mask", "clippath", "symbol",
];

fn strip_containers(markup: &str) -> String {
    let mut s = markup.to_string();
    for tag in ["defs", "mask", "clipPath", "clippath", "symbol"] {
        loop {
            let open = format!("<{tag}");
            let close = format!("</{tag}>");
            let Some(start) = s.to_ascii_lowercase().find(&open.to_ascii_lowercase()) else {
                break;
            };
            let Some(end_rel) = s[start..]
                .to_ascii_lowercase()
                .find(&close.to_ascii_lowercase())
            else {
                break;
            };
            let end = start + end_rel + close.len();
            s.replace_range(start..end, "");
        }
    }
    s
}

fn read_attrs(raw: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b':')
        {
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        let name = raw[start..i].to_string();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        i += 1;
        let vstart = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        let val = raw[vstart..i.min(raw.len())].to_string();
        if i < bytes.len() {
            i += 1;
        }
        attrs.insert(name, val);
    }
    attrs
}

fn view_box_of(markup: &str) -> Option<String> {
    let lower = markup.to_ascii_lowercase();
    let svg = lower.find("<svg")?;
    let rest = &markup[svg..];
    let end = rest.find('>')?;
    let head = &rest[..end];
    let attrs = read_attrs(head);
    attrs
        .get("viewBox")
        .or_else(|| attrs.get("viewbox"))
        .cloned()
}

/// Parse SVG markup into an [`IconInput`].
///
/// `lenient`: skip the fill-drawn / transform / unknown-tag rejects (Lucide inner
/// markup has neither `stroke` nor `fill="none"` on each path).
pub fn markup_to_icon(markup: &str, lenient: bool) -> Result<IconInput, MorphError> {
    let trimmed = markup.trim_start();
    if !trimmed.starts_with('<') {
        return Err(morph_err(
            "svgToIcon expects SVG markup — a d string is already an IconInput",
        ));
    }
    let body = strip_containers(markup);
    if !lenient {
        let has_stroke = body.to_ascii_lowercase().contains("stroke=")
            && !body.to_ascii_lowercase().contains("stroke=\"none\"")
            && !body.to_ascii_lowercase().contains("stroke='none'");
        let fill_none = body.to_ascii_lowercase().contains("fill=\"none\"")
            || body.to_ascii_lowercase().contains("fill='none'");
        if !has_stroke && !fill_none {
            return Err(morph_err(
                "SVG looks fill-drawn (no stroke) — only stroke-centerline icons morph",
            ));
        }
    }
    let mut nodes: Vec<(String, HashMap<String, String>)> = Vec::new();
    let mut rest = body.as_str();
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with('/') {
            if let Some(gt) = rest.find('>') {
                rest = &rest[gt + 1..];
            }
            continue;
        }
        if rest.starts_with('!') || rest.starts_with('?') {
            if let Some(gt) = rest.find('>') {
                rest = &rest[gt + 1..];
            }
            continue;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        let tag = rest[..name_end].to_ascii_lowercase();
        let after = &rest[name_end..];
        let gt = after.find('>').unwrap_or(after.len());
        let attrs_src = after[..gt].trim_end_matches('/');
        rest = if gt < after.len() {
            &after[gt + 1..]
        } else {
            ""
        };
        if SKIP.contains(&tag.as_str()) {
            continue;
        }
        if !GEOMETRY.contains(&tag.as_str()) {
            if lenient {
                continue;
            }
            return Err(morph_err(format!("unsupported SVG element <{tag}>")));
        }
        let attrs = read_attrs(attrs_src);
        if !lenient && attrs.contains_key("transform") {
            return Err(morph_err(format!("<{tag}> has a transform (unsupported)")));
        }
        nodes.push((tag, attrs));
    }
    if nodes.is_empty() {
        return Err(morph_err("no morphable geometry in SVG"));
    }
    let input = IconInput::Nodes(nodes);
    if let Some(vb) = view_box_of(markup) {
        let d = fit_icon(&input, &vb, 24.0)?;
        return Ok(IconInput::D(d));
    }
    Ok(input)
}
