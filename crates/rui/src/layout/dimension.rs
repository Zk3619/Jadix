use taffy::prelude::{Dimension, LengthPercentage};

pub const CSS_ROOT_FONT_SIZE_PX: f32 = 16.0;

fn parse_css_number(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok()
}

fn parse_css_length_px(s: &str, em_base_px: f32) -> Option<f32> {
    let trimmed = s.trim();
    if let Some(raw) = trimmed.strip_suffix("px") {
        return parse_css_number(raw);
    }
    if let Some(raw) = trimmed.strip_suffix("rem") {
        return parse_css_number(raw).map(|v| v * CSS_ROOT_FONT_SIZE_PX);
    }
    if let Some(raw) = trimmed.strip_suffix("em") {
        return parse_css_number(raw).map(|v| v * em_base_px);
    }
    // Unsuffixed numeric values default to rem (ShadCN-friendly semantics).
    parse_css_number(trimmed).map(|v| v * CSS_ROOT_FONT_SIZE_PX)
}

pub trait IntoDimension {
    fn into_dimension(self) -> Dimension;
}

impl IntoDimension for f32 {
    fn into_dimension(self) -> Dimension {
        Dimension::Length(self)
    }
}

impl IntoDimension for Dimension {
    fn into_dimension(self) -> Dimension {
        self
    }
}

impl IntoDimension for i32 {
    fn into_dimension(self) -> Dimension {
        Dimension::Length(self as f32)
    }
}

impl IntoDimension for &str {
    fn into_dimension(self) -> Dimension {
        let s = self.trim();
        if let Some(rest) = s.strip_suffix('%') {
            if let Ok(val) = rest.parse::<f32>() {
                return Dimension::Percent(val / 100.0);
            }
        } else if let Some(rest) = s.strip_suffix("vw").or_else(|| s.strip_suffix("vh")) {
            // Taffy's Dimension has no viewport unit, fallback to percent semantics.
            if let Ok(val) = rest.parse::<f32>() {
                return Dimension::Percent(val / 100.0);
            }
        } else if let Some(val) = parse_css_length_px(s, CSS_ROOT_FONT_SIZE_PX) {
            return Dimension::Length(val);
        }
        Dimension::Auto
    }
}

pub trait IntoLengthPercentage {
    fn into_length_percentage(self) -> LengthPercentage;
}

impl<T: IntoDimension> IntoLengthPercentage for T {
    fn into_length_percentage(self) -> LengthPercentage {
        match self.into_dimension() {
            Dimension::Length(l) => LengthPercentage::Length(l),
            Dimension::Percent(p) => LengthPercentage::Percent(p),
            _ => LengthPercentage::Length(0.0),
        }
    }
}

pub trait IntoF32 {
    fn into_f32(self) -> f32;
}

impl IntoF32 for f32 {
    fn into_f32(self) -> f32 {
        self
    }
}

impl IntoF32 for &str {
    fn into_f32(self) -> f32 {
        parse_css_length_px(self, CSS_ROOT_FONT_SIZE_PX).unwrap_or(0.0)
    }
}

pub trait IntoF32Raw {
    fn into_f32_raw(self) -> f32;
}

impl IntoF32Raw for f32 {
    fn into_f32_raw(self) -> f32 {
        self
    }
}

impl IntoF32Raw for &str {
    fn into_f32_raw(self) -> f32 {
        self.trim().parse().unwrap_or(0.0)
    }
}
