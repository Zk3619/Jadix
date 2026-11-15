//! Bounded, caller-context CSS color evaluation. This is not a stylesheet cascade.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssColorError {
    Syntax,
    Unsupported,
    Limit,
    UnresolvedVariable,
    InvalidMath,
}
impl std::fmt::Display for CssColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CSS color: {self:?}")
    }
}
impl std::error::Error for CssColorError {}
#[derive(Clone, Debug)]
pub struct ColorContext2D {
    pub current_color: Color2D,
    pub dark: bool,
    /// Custom-property token values. Caller supplies computed values/cascade.
    pub variables: BTreeMap<String, String>,
}
impl Default for ColorContext2D {
    fn default() -> Self {
        Self {
            current_color: Color2D::parse("black").unwrap(),
            dark: false,
            variables: Default::default(),
        }
    }
}
impl ColorContext2D {
    /// Evaluates literals, var/fallback, currentColor, light-dark, two-color mix,
    /// modern relative colors and scalar calc/min/max/clamp. Maximum input and
    /// expansion 4096 bytes; nesting <=32. Escapes/comments and CSS-wide keywords
    /// require a stylesheet tokenizer/resolver and fail explicitly.
    pub fn resolve(&self, css: &str) -> Result<Color2D, CssColorError> {
        if css.len() > 4096 {
            return Err(CssColorError::Limit);
        }
        if !css.is_ascii() || css.contains('\\') || css.contains("/*") {
            return Err(CssColorError::Unsupported);
        }
        let color = self.color(&self.substitute(css, 0)?, 0)?;
        if color.0.components.iter().any(|v| !v.is_finite()) {
            return Err(CssColorError::InvalidMath);
        }
        Ok(color)
    }
    fn substitute(&self, css: &str, depth: usize) -> Result<String, CssColorError> {
        if depth > 32 || css.len() > 4096 {
            return Err(CssColorError::Limit);
        }
        if !css.is_ascii() || css.contains('\\') || css.contains("/*") {
            return Err(CssColorError::Unsupported);
        }
        let mut result = String::new();
        let mut at = 0;
        while at < css.len() {
            if css
                .get(at..at + 4)
                .is_some_and(|s| s.eq_ignore_ascii_case("var("))
                && (at == 0 || !css.as_bytes()[at - 1].is_ascii_alphanumeric())
            {
                let end = close(css, at + 3)?;
                let parts = split(&css[at + 4..end], ',')?;
                let key = parts[0].trim();
                if !key.starts_with("--")
                    || key.len() < 3
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    return Err(CssColorError::Syntax);
                }
                let value = if let Some(value) = self.variables.get(key) {
                    value.as_str()
                } else if parts.len() >= 2 {
                    &css[at + 4 + parts[0].len() + 1..end]
                } else {
                    return Err(CssColorError::UnresolvedVariable);
                };
                result.push_str(&self.substitute(value, depth + 1)?);
                at = end + 1;
            } else {
                result.push(css.as_bytes()[at] as char);
                at += 1;
            }
            if result.len() > 4096 {
                return Err(CssColorError::Limit);
            }
        }
        Ok(result)
    }
    fn color(&self, css: &str, depth: usize) -> Result<Color2D, CssColorError> {
        if depth > 32 {
            return Err(CssColorError::Limit);
        }
        let css = css.trim();
        if css.eq_ignore_ascii_case("currentcolor") {
            return Ok(self.current_color);
        }
        if let Ok(c) = Color2D::parse(css) {
            return Ok(c);
        }
        let (function, args) = function(css)?;
        if function.eq_ignore_ascii_case("light-dark") {
            let parts = split(args, ',')?;
            if parts.len() != 2 {
                return Err(CssColorError::Syntax);
            }
            let light = self.color(parts[0], depth + 1)?;
            let dark = self.color(parts[1], depth + 1)?;
            return Ok(if self.dark { dark } else { light });
        }
        if function.eq_ignore_ascii_case("color-mix") {
            let parts = split(args, ',')?;
            if parts.len() != 3 {
                return Err(CssColorError::Syntax);
            }
            let lower_header = parts[0].to_ascii_lowercase();
            let header = tokens(&lower_header)?;
            if header.len() < 2 || header[0] != "in" {
                return Err(CssColorError::Syntax);
            }
            let cs = space(header[1])?;
            let hue = if header.len() == 2 {
                ColorHueDirection::Shorter
            } else if header.len() == 4 && header[3] == "hue" {
                match header[2] {
                    "shorter" => ColorHueDirection::Shorter,
                    "longer" => ColorHueDirection::Longer,
                    "increasing" => ColorHueDirection::Increasing,
                    "decreasing" => ColorHueDirection::Decreasing,
                    _ => return Err(CssColorError::Syntax),
                }
            } else {
                return Err(CssColorError::Syntax);
            };
            if header.len() == 4
                && !matches!(
                    cs,
                    ColorSpaceTag::Hsl
                        | ColorSpaceTag::Hwb
                        | ColorSpaceTag::Lch
                        | ColorSpaceTag::Oklch
                )
            {
                return Err(CssColorError::Syntax);
            }
            let (a, wa) = self.mix_leg(parts[1], depth + 1)?;
            let (b, wb) = self.mix_leg(parts[2], depth + 1)?;
            let (wa, wb) = match (wa, wb) {
                (Some(a), Some(b)) => (a, b),
                (Some(a), None) => (a, 1. - a),
                (None, Some(b)) => (1. - b, b),
                (None, None) => (0.5, 0.5),
            };
            let total = wa + wb;
            if total <= 0. {
                return Err(CssColorError::Syntax);
            }
            let mut color = a.0.interpolate(b.0, cs, hue).eval(wb / total);
            color.components[3] *= total.min(1.);
            return Ok(Color2D(color));
        }
        self.channels(function, args, depth + 1)
    }
    fn mix_leg(&self, leg: &str, depth: usize) -> Result<(Color2D, Option<f32>), CssColorError> {
        let parts = tokens(leg)?;
        let last = parts.last().ok_or(CssColorError::Syntax)?;
        if let Some(v) = last.strip_suffix('%') {
            let value = v.parse::<f32>().map_err(|_| CssColorError::Syntax)? / 100.;
            if !(0. ..=1.).contains(&value) {
                return Err(CssColorError::Syntax);
            }
            let at = leg.trim_end().len() - last.len();
            Ok((self.color(&leg[..at], depth)?, Some(value)))
        } else {
            Ok((self.color(leg, depth)?, None))
        }
    }
    fn channels(&self, name: &str, args: &str, depth: usize) -> Result<Color2D, CssColorError> {
        let mut parts = tokens(args)?;
        let relative = parts
            .first()
            .is_some_and(|s| s.eq_ignore_ascii_case("from"));
        let source = if relative {
            if parts.len() < 2 {
                return Err(CssColorError::Syntax);
            }
            let value = self.color(parts[1], depth + 1)?;
            parts.drain(..2);
            Some(value)
        } else {
            None
        };
        let name = name.to_ascii_lowercase();
        let cs = match name.as_str() {
            "rgb" | "rgba" => ColorSpaceTag::Srgb,
            "hsl" | "hsla" => ColorSpaceTag::Hsl,
            "hwb" => ColorSpaceTag::Hwb,
            "lab" => ColorSpaceTag::Lab,
            "lch" => ColorSpaceTag::Lch,
            "oklab" => ColorSpaceTag::Oklab,
            "oklch" => ColorSpaceTag::Oklch,
            "color" => {
                if parts.is_empty() {
                    return Err(CssColorError::Syntax);
                }
                let cs = space(parts.remove(0))?;
                if matches!(
                    cs,
                    ColorSpaceTag::Lab
                        | ColorSpaceTag::Lch
                        | ColorSpaceTag::Oklab
                        | ColorSpaceTag::Oklch
                        | ColorSpaceTag::Hsl
                        | ColorSpaceTag::Hwb
                ) {
                    return Err(CssColorError::Syntax);
                }
                cs
            }
            _ => return Err(CssColorError::Unsupported),
        };
        let (names, scales) = match cs {
            ColorSpaceTag::Hsl => (["h", "s", "l"], [360., 100., 100.]),
            ColorSpaceTag::Hwb => (["h", "w", "b"], [360., 100., 100.]),
            ColorSpaceTag::Lab => (["l", "a", "b"], [100., 125., 125.]),
            ColorSpaceTag::Lch => (["l", "c", "h"], [100., 150., 360.]),
            ColorSpaceTag::Oklab => (["l", "a", "b"], [1., 0.4, 0.4]),
            ColorSpaceTag::Oklch => (["l", "c", "h"], [1., 0.4, 360.]),
            ColorSpaceTag::XyzD50 | ColorSpaceTag::XyzD65 => (["x", "y", "z"], [1.; 3]),
            _ => (
                ["r", "g", "b"],
                [if name == "rgb" || name == "rgba" {
                    255.
                } else {
                    1.
                }; 3],
            ),
        };
        let base = source
            .map(|c| c.0.convert(cs).components)
            .unwrap_or([0., 0., 0., 1.]);
        let factor = if name == "rgb" || name == "rgba" {
            255.
        } else {
            1.
        };
        let variables = [
            (names[0], base[0] * factor),
            (names[1], base[1] * factor),
            (names[2], base[2] * factor),
            ("alpha", base[3]),
        ];
        if parts.len() != 3 && !(parts.len() == 5 && parts[3] == "/") {
            return Err(CssColorError::Syntax);
        }
        let mut value = [0.; 4];
        let mut missing = ::color::Missing::EMPTY;
        for i in 0..3 {
            if parts[i].eq_ignore_ascii_case("none") {
                missing = missing | ::color::Missing::single(i);
            } else {
                value[i] = math(
                    parts[i],
                    scales[i],
                    if relative { &variables } else { &[] },
                    depth,
                    names[i] == "h",
                )?;
            }
        }
        if factor != 1. {
            for v in &mut value[..3] {
                *v /= factor;
            }
        }
        value[3] = if parts.len() == 5 {
            if parts[4].eq_ignore_ascii_case("none") {
                missing = missing | ::color::Missing::single(3);
                0.
            } else {
                math(
                    parts[4],
                    1.,
                    if relative { &variables } else { &[] },
                    depth,
                    false,
                )?
                .clamp(0., 1.)
            }
        } else if relative {
            base[3]
        } else {
            1.
        };
        if !relative {
            match cs {
                ColorSpaceTag::Srgb if name != "color" => {
                    for v in &mut value[..3] {
                        *v = v.clamp(0., 1.);
                    }
                }
                ColorSpaceTag::Lab | ColorSpaceTag::Lch => value[0] = value[0].clamp(0., 100.),
                ColorSpaceTag::Oklab | ColorSpaceTag::Oklch => value[0] = value[0].clamp(0., 1.),
                _ => {}
            }
        }
        if value.iter().any(|v| !v.is_finite() || v.abs() > 16384.) {
            return Err(CssColorError::InvalidMath);
        }
        Ok(Color2D(DynamicColor {
            cs,
            flags: Flags::from_missing(missing),
            components: value,
        }))
    }
}
fn space(s: &str) -> Result<ColorSpaceTag, CssColorError> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "srgb" => ColorSpaceTag::Srgb,
        "srgb-linear" => ColorSpaceTag::LinearSrgb,
        "display-p3" => ColorSpaceTag::DisplayP3,
        "a98-rgb" => ColorSpaceTag::A98Rgb,
        "prophoto-rgb" => ColorSpaceTag::ProphotoRgb,
        "rec2020" => ColorSpaceTag::Rec2020,
        "xyz" | "xyz-d65" => ColorSpaceTag::XyzD65,
        "xyz-d50" => ColorSpaceTag::XyzD50,
        "lab" => ColorSpaceTag::Lab,
        "lch" => ColorSpaceTag::Lch,
        "oklab" => ColorSpaceTag::Oklab,
        "oklch" => ColorSpaceTag::Oklch,
        "hsl" => ColorSpaceTag::Hsl,
        "hwb" => ColorSpaceTag::Hwb,
        _ => return Err(CssColorError::Unsupported),
    })
}
fn function(s: &str) -> Result<(&str, &str), CssColorError> {
    let at = s.find('(').ok_or(CssColorError::Syntax)?;
    let end = close(s, at)?;
    if end != s.len() - 1 {
        return Err(CssColorError::Syntax);
    }
    Ok((&s[..at], &s[at + 1..end]))
}
fn close(s: &str, at: usize) -> Result<usize, CssColorError> {
    let mut depth = 0;
    for (i, b) in s.bytes().enumerate().skip(at) {
        if b == b'(' {
            depth += 1;
            if depth > 32 {
                return Err(CssColorError::Limit);
            }
        }
        if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
    }
    Err(CssColorError::Syntax)
}
fn split(s: &str, delimiter: char) -> Result<Vec<&str>, CssColorError> {
    let mut depth = 0;
    let mut start = 0;
    let mut result = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Err(CssColorError::Syntax);
                }
                depth -= 1;
            }
            c if c == delimiter && depth == 0 => {
                result.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        if depth > 32 {
            return Err(CssColorError::Limit);
        }
    }
    if depth != 0 {
        return Err(CssColorError::Syntax);
    }
    result.push(s[start..].trim());
    Ok(result)
}
fn tokens(s: &str) -> Result<Vec<&str>, CssColorError> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < s.len() {
        let b = s.as_bytes()[at];
        if b.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let start = at;
        if b == b'/' {
            at += 1;
        } else {
            while at < s.len()
                && !s.as_bytes()[at].is_ascii_whitespace()
                && s.as_bytes()[at] != b'/'
            {
                if s.as_bytes()[at] == b'(' {
                    at = close(s, at)?;
                }
                at += 1;
            }
        }
        result.push(&s[start..at]);
    }
    Ok(result)
}
fn math(
    s: &str,
    percent: f32,
    vars: &[(&str, f32)],
    depth: usize,
    angle: bool,
) -> Result<f32, CssColorError> {
    if depth > 32 {
        return Err(CssColorError::Limit);
    }
    if let Ok((name, args)) = function(s) {
        match name.to_ascii_lowercase().as_str() {
            "calc" => {
                let mut p = Math {
                    input: args.as_bytes(),
                    at: 0,
                    percent,
                    vars,
                    depth,
                    angle,
                };
                let value = p.sum()?;
                p.ws();
                if p.at != p.input.len() {
                    return Err(CssColorError::InvalidMath);
                }
                return finite_value(value);
            }
            "min" | "max" | "clamp" => {
                let values = split(args, ',')?
                    .iter()
                    .map(|s| math(s, percent, vars, depth + 1, angle))
                    .collect::<Result<Vec<_>, _>>()?;
                return finite_value(match name.to_ascii_lowercase().as_str() {
                    "min" => values
                        .into_iter()
                        .reduce(f32::min)
                        .ok_or(CssColorError::InvalidMath)?,
                    "max" => values
                        .into_iter()
                        .reduce(f32::max)
                        .ok_or(CssColorError::InvalidMath)?,
                    "clamp" if values.len() == 3 => values[0].max(values[1].min(values[2])),
                    _ => return Err(CssColorError::InvalidMath),
                });
            }
            _ => {}
        }
    }
    let mut p = Math {
        input: s.as_bytes(),
        at: 0,
        percent,
        vars,
        depth,
        angle,
    };
    let value = p.atom()?;
    p.ws();
    if p.at != p.input.len() {
        return Err(CssColorError::InvalidMath);
    }
    finite_value(value)
}
fn finite_value(value: f32) -> Result<f32, CssColorError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CssColorError::InvalidMath)
    }
}
struct Math<'a> {
    input: &'a [u8],
    at: usize,
    percent: f32,
    vars: &'a [(&'a str, f32)],
    depth: usize,
    angle: bool,
}
impl Math<'_> {
    fn ws(&mut self) {
        while self.input.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn sum(&mut self) -> Result<f32, CssColorError> {
        let mut a = self.product()?;
        loop {
            self.ws();
            match self.input.get(self.at) {
                Some(b'+') => {
                    self.at += 1;
                    a += self.product()?;
                }
                Some(b'-') => {
                    self.at += 1;
                    a -= self.product()?;
                }
                _ => return finite_value(a),
            }
        }
    }
    fn product(&mut self) -> Result<f32, CssColorError> {
        let mut a = self.atom()?;
        loop {
            self.ws();
            match self.input.get(self.at) {
                Some(b'*') => {
                    self.at += 1;
                    a *= self.atom()?;
                }
                Some(b'/') => {
                    self.at += 1;
                    a /= self.atom()?;
                }
                _ => return finite_value(a),
            }
        }
    }
    fn atom(&mut self) -> Result<f32, CssColorError> {
        self.ws();
        if self.depth > 32 {
            return Err(CssColorError::Limit);
        }
        let start = self.at;
        let sign = match self.input.get(self.at) {
            Some(b'-') => {
                self.at += 1;
                -1.
            }
            Some(b'+') => {
                self.at += 1;
                1.
            }
            _ => 1.,
        };
        if self.input.get(self.at) == Some(&b'(') {
            self.at += 1;
            self.depth += 1;
            let value = self.sum()?;
            self.depth -= 1;
            self.ws();
            if self.input.get(self.at) != Some(&b')') {
                return Err(CssColorError::InvalidMath);
            }
            self.at += 1;
            return finite_value(value * sign);
        }
        if self.input.get(self.at).is_some_and(u8::is_ascii_alphabetic) {
            let name_start = self.at;
            while self
                .input
                .get(self.at)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
            {
                self.at += 1;
            }
            let name = std::str::from_utf8(&self.input[name_start..self.at]).unwrap();
            if self.input.get(self.at) == Some(&b'(') {
                let text = std::str::from_utf8(self.input).unwrap();
                let end = close(text, self.at)?;
                let value = math(
                    &text[name_start..=end],
                    self.percent,
                    self.vars,
                    self.depth + 1,
                    self.angle,
                )?;
                self.at = end + 1;
                return finite_value(value * sign);
            }
            return self
                .vars
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, v)| v * sign)
                .ok_or(CssColorError::InvalidMath);
        }
        while self
            .input
            .get(self.at)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
        {
            self.at += 1;
        }
        if self
            .input
            .get(self.at)
            .is_some_and(|b| *b == b'e' || *b == b'E')
        {
            self.at += 1;
            if self
                .input
                .get(self.at)
                .is_some_and(|b| *b == b'+' || *b == b'-')
            {
                self.at += 1;
            }
            while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        let number = std::str::from_utf8(&self.input[start..self.at])
            .unwrap()
            .parse::<f32>()
            .map_err(|_| CssColorError::InvalidMath)?;
        let unit = self.at;
        while self
            .input
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'%')
        {
            self.at += 1;
        }
        let unit = std::str::from_utf8(&self.input[unit..self.at]).unwrap();
        if !self.angle && matches!(unit, "deg" | "rad" | "turn" | "grad") {
            return Err(CssColorError::InvalidMath);
        }
        finite_value(
            number
                * match unit {
                    "" | "deg" => 1.,
                    "%" => self.percent / 100.,
                    "turn" => 360.,
                    "grad" => 0.9,
                    "rad" => 180. / std::f32::consts::PI,
                    _ => return Err(CssColorError::InvalidMath),
                },
        )
    }
}
