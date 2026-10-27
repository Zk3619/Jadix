#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: [f32; 4],
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 0.0],
            offset: [0.0, 0.0],
            blur: 0.0,
            spread: 0.0,
            inset: false,
        }
    }
}

fn parse_css_len_px(token: &str, root_font_size_px: f32) -> Option<f32> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f32>() {
        // Unsuffixed numeric values default to rem (align with IntoF32/IntoDimension).
        return Some(v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("px") {
        return raw.trim().parse::<f32>().ok();
    }
    if let Some(raw) = t.strip_suffix("rem") {
        return raw
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("em") {
        return raw
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v * root_font_size_px);
    }
    None
}

/// Parse a single `box-shadow` (no comma-separated multi-shadow yet; one shadow covers component roots).
///
/// Semantics follow MDN: `<color>? && <length>{2,4} && inset?`.
/// `color` uses this project's semantic color system (`parse_color`).
pub fn parse_box_shadow(s: impl AsRef<str>) -> BoxShadow {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return BoxShadow::default();
    }

    let root_font_size_px = jadix_layout::dimension::css_root_font_size_px();

    let mut out = BoxShadow::default();
    let mut lens: Vec<f32> = Vec::with_capacity(4);
    for tok in raw.split_whitespace() {
        if tok.eq_ignore_ascii_case("inset") {
            out.inset = true;
            continue;
        }
        if let Some(v) = parse_css_len_px(tok, root_font_size_px) {
            if lens.len() < 4 {
                lens.push(v);
            }
            continue;
        }
        // Assume it's a color keyword/token. Keep the first color-like token.
        if out.color[3] == 0.0 {
            out.color = super::parse_color(tok);
        }
    }

    // MDN length rules:
    // - 2 lengths => offset-x/offset-y
    // - 3 lengths => + blur (negative not allowed → 0)
    // - 4 lengths => + spread (can be negative)
    if !lens.is_empty() {
        out.offset[0] = lens[0];
    }
    if lens.len() >= 2 {
        out.offset[1] = lens[1];
    }
    if lens.len() >= 3 {
        out.blur = lens[2].max(0.0);
    } else {
        out.blur = 0.0;
    }
    if lens.len() >= 4 {
        out.spread = lens[3];
    } else {
        out.spread = 0.0;
    }
    out
}

/// Split multi-layer `box-shadow` on commas (ignore commas inside parentheses for `rgb(...)`).
pub fn parse_box_shadows(s: impl AsRef<str>) -> Vec<BoxShadow> {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in raw.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = raw[start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    if parts.is_empty() {
        return vec![parse_box_shadow(raw)];
    }
    parts.into_iter().map(parse_box_shadow).collect()
}
