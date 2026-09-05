//! CSS [`filter`](https://developer.mozilla.org/en-US/docs/Web/CSS/filter) property.
//!
//! Parsing follows Filter Effects Module Level 1. Rendering lowers this list into an
//! SVG [`FilterGraph`](super::svg_filter::FilterGraph) (see `svg_filter::lower_css`).

use crate::layout::dimension::{
    CSS_ROOT_FONT_SIZE_PX, CssAngle, CssDim, parse_css_angle, parse_css_dim,
};

use super::color::try_parse_color;

/// A single CSS `<filter-function>` or `<url>`.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterFunction {
    /// `blur(<length>?)` — default 0.
    Blur(Option<CssDim>),
    /// `brightness([<number>|<percentage>]?)` — multiplier, default 1.
    Brightness(Option<f32>),
    /// `contrast([<number>|<percentage>]?)` — default 1.
    Contrast(Option<f32>),
    /// `drop-shadow([<color>? && <length>{2,3}])` — no inset / spread.
    DropShadow {
        color: Option<[f32; 4]>,
        offset_x: CssDim,
        offset_y: CssDim,
        blur: Option<CssDim>,
    },
    /// `grayscale([<number>|<percentage>]?)` — 0..1, default 1 when omitted? Spec: missing = 1 (fully).
    Grayscale(Option<f32>),
    /// `hue-rotate([<angle>|0]?)` — default 0deg.
    HueRotate(Option<CssAngle>),
    /// `invert([<number>|<percentage>]?)` — default 1 when omitted.
    Invert(Option<f32>),
    /// `opacity([<number>|<percentage>]?)` — default 1 when omitted.
    Opacity(Option<f32>),
    /// `saturate([<number>|<percentage>]?)` — default 1 when omitted.
    Saturate(Option<f32>),
    /// `sepia([<number>|<percentage>]?)` — default 1 when omitted.
    Sepia(Option<f32>),
    /// `url(...)` — SVG filter reference (resolved at paint time).
    Url(String),
}

/// Ordered CSS `filter` value list. `None` on [`Style`](super::Style) means `none`.
pub type FilterList = Vec<FilterFunction>;

/// Extra pixels the filter result can paint past the border box (Gaussian kernel /
/// drop-shadow offset). Used for clip-cull and the capture region.
pub fn filter_list_paint_outset(list: &FilterList, lay_w: f32, lay_h: f32) -> f32 {
    // feGaussianBlur practical support is ~3σ; CSS `blur()` length is stdDeviation.
    const KERNEL: f32 = 3.0;
    fn len_px(d: &CssDim) -> f32 {
        match d {
            CssDim::Px(v) => v.abs(),
            CssDim::Rem(v) | CssDim::Em(v) => (*v * CSS_ROOT_FONT_SIZE_PX).abs(),
            _ => 0.0,
        }
    }
    let mut pad = 0.0_f32;
    let mut has_url = false;
    for f in list {
        match f {
            FilterFunction::Blur(d) => {
                let s = d.as_ref().map(len_px).unwrap_or(0.0);
                pad = pad.max(s * KERNEL);
            }
            FilterFunction::DropShadow {
                offset_x,
                offset_y,
                blur,
                ..
            } => {
                let b = blur.as_ref().map(len_px).unwrap_or(0.0) * KERNEL;
                pad = pad.max(len_px(offset_x).max(len_px(offset_y)) + b);
            }
            FilterFunction::Url(_) => has_url = true,
            _ => {}
        }
    }
    if has_url {
        // SVG default filter region is x/y = -10%, width/height = 120%.
        pad = pad.max(0.1 * lay_w.max(lay_h).max(0.0));
    }
    pad
}

/// Parse a CSS `filter` property value.
///
/// Returns `None` for `"none"` / empty, or when any function argument is invalid
/// (entire property becomes `none` per CSS).
pub fn parse_filter(s: &str) -> Option<FilterList> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return None;
    }

    let mut list = Vec::new();
    let mut rest = s;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }

        // url(...) — may appear without being a "name(" filter-function in some parsers;
        // treat as a function with name `url`.
        let open = rest.find('(')?;
        let name = rest[..open].trim();
        if name.is_empty() {
            return None;
        }
        let after_open = &rest[open + 1..];
        let close = find_matching_paren(after_open)?;
        let args_str = &after_open[..close];
        rest = after_open[close + 1..].trim_start();

        let func = parse_filter_func(name, args_str)?;
        list.push(func);
    }

    if list.is_empty() { None } else { Some(list) }
}

fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

fn parse_filter_func(name: &str, args: &str) -> Option<FilterFunction> {
    let lower = name.to_ascii_lowercase();
    let args = args.trim();
    match lower.as_str() {
        "blur" => {
            if args.is_empty() {
                return Some(FilterFunction::Blur(None));
            }
            let d = parse_length_arg(args)?;
            Some(FilterFunction::Blur(Some(d)))
        }
        "brightness" => Some(FilterFunction::Brightness(parse_factor_arg(args))),
        "contrast" => Some(FilterFunction::Contrast(parse_factor_arg(args))),
        "grayscale" => Some(FilterFunction::Grayscale(parse_factor_arg(args))),
        "invert" => Some(FilterFunction::Invert(parse_factor_arg(args))),
        "opacity" => Some(FilterFunction::Opacity(parse_factor_arg(args))),
        "saturate" => Some(FilterFunction::Saturate(parse_factor_arg(args))),
        "sepia" => Some(FilterFunction::Sepia(parse_factor_arg(args))),
        "hue-rotate" => {
            if args.is_empty() {
                return Some(FilterFunction::HueRotate(None));
            }
            let a = parse_css_angle(args)?;
            Some(FilterFunction::HueRotate(Some(a)))
        }
        "drop-shadow" => parse_drop_shadow(args),
        "url" => {
            let url = parse_url_arg(args)?;
            Some(FilterFunction::Url(url))
        }
        _ => None,
    }
}

/// Factor args: empty → None (caller uses function default); otherwise number or %.
fn parse_factor_arg(args: &str) -> Option<f32> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }
    parse_number_or_percent(args)
}

fn parse_number_or_percent(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(raw) = s.strip_suffix('%') {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v / 100.0);
    }
    s.parse::<f32>().ok()
}

fn parse_length_arg(s: &str) -> Option<CssDim> {
    let s = s.trim();
    if s == "0" {
        return Some(CssDim::Px(0.0));
    }
    // Percentages are invalid for blur length.
    if s.contains('%') {
        return None;
    }
    parse_css_dim(s)
}

fn parse_url_arg(args: &str) -> Option<String> {
    let s = args.trim();
    if s.is_empty() {
        return None;
    }
    let unquoted =
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            &s[1..s.len() - 1]
        } else {
            s
        };
    let unquoted = unquoted.trim();
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

fn parse_drop_shadow(args: &str) -> Option<FilterFunction> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }

    // Tokens are whitespace-separated; colors may be rgb(...)/hsl(...)/oklch(...) with spaces.
    let tokens = split_drop_shadow_tokens(args);
    if tokens.is_empty() {
        return None;
    }

    let mut color: Option<[f32; 4]> = None;
    let mut lengths: Vec<CssDim> = Vec::new();

    for tok in &tokens {
        if lengths.len() < 3 {
            if let Some(d) = parse_length_arg(tok) {
                lengths.push(d);
                continue;
            }
        }
        if color.is_none() {
            if let Some(c) = try_parse_color(tok) {
                color = Some(c);
                continue;
            }
        }
        return None;
    }

    if lengths.len() < 2 {
        return None;
    }
    if lengths.len() > 3 {
        return None;
    }

    Some(FilterFunction::DropShadow {
        color,
        offset_x: lengths[0].clone(),
        offset_y: lengths[1].clone(),
        blur: lengths.get(2).cloned(),
    })
}

/// Split drop-shadow args on whitespace outside of `()` / quotes.
fn split_drop_shadow_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl FilterFunction {
    /// Effective blur radius in CSS px (root font = 16). Missing → 0.
    pub fn blur_px_default_root(self_dim: &Option<CssDim>) -> f32 {
        match self_dim {
            None => 0.0,
            Some(CssDim::Px(v)) => (*v).max(0.0),
            Some(CssDim::Rem(v)) => (*v * CSS_ROOT_FONT_SIZE_PX).max(0.0),
            Some(CssDim::Em(v)) => (*v * CSS_ROOT_FONT_SIZE_PX).max(0.0),
            Some(_) => 0.0,
        }
    }

    /// Serialize one function back to a CSS-ish string (DevTools / dump).
    pub fn to_css_string(&self) -> String {
        match self {
            Self::Blur(None) => "blur()".into(),
            Self::Blur(Some(d)) => format!("blur({})", dim_css(d)),
            Self::Brightness(None) => "brightness()".into(),
            Self::Brightness(Some(v)) => format!("brightness({})", v),
            Self::Contrast(None) => "contrast()".into(),
            Self::Contrast(Some(v)) => format!("contrast({})", v),
            Self::Grayscale(None) => "grayscale()".into(),
            Self::Grayscale(Some(v)) => format!("grayscale({})", v),
            Self::Invert(None) => "invert()".into(),
            Self::Invert(Some(v)) => format!("invert({})", v),
            Self::Opacity(None) => "opacity()".into(),
            Self::Opacity(Some(v)) => format!("opacity({})", v),
            Self::Saturate(None) => "saturate()".into(),
            Self::Saturate(Some(v)) => format!("saturate({})", v),
            Self::Sepia(None) => "sepia()".into(),
            Self::Sepia(Some(v)) => format!("sepia({})", v),
            Self::HueRotate(None) => "hue-rotate()".into(),
            Self::HueRotate(Some(a)) => format!("hue-rotate({}deg)", a.to_deg()),
            Self::Url(u) => format!("url(\"{u}\")"),
            Self::DropShadow {
                color,
                offset_x,
                offset_y,
                blur,
            } => {
                let mut s = format!("drop-shadow({} {}", dim_css(offset_x), dim_css(offset_y));
                if let Some(b) = blur {
                    s.push(' ');
                    s.push_str(&dim_css(b));
                }
                if let Some(c) = color {
                    s.push_str(&format!(
                        " rgba({},{},{},{:.3})",
                        (c[0] * 255.0).round() as i32,
                        (c[1] * 255.0).round() as i32,
                        (c[2] * 255.0).round() as i32,
                        c[3]
                    ));
                }
                s.push(')');
                s
            }
        }
    }
}

fn dim_css(d: &CssDim) -> String {
    match d {
        CssDim::Px(v) => format!("{v}px"),
        CssDim::Rem(v) => format!("{v}rem"),
        CssDim::Em(v) => format!("{v}em"),
        CssDim::Percent(v) => format!("{}%", v * 100.0),
        other => format!("{other:?}"),
    }
}

/// Serialize a filter list (or `none`).
pub fn filter_list_to_css(list: Option<&FilterList>) -> String {
    match list {
        None => "none".into(),
        Some(l) if l.is_empty() => "none".into(),
        Some(l) => l
            .iter()
            .map(FilterFunction::to_css_string)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Whether the list contains a `url()` (blocks continuous interpolation).
pub fn filter_list_has_url(list: &[FilterFunction]) -> bool {
    list.iter().any(|f| matches!(f, FilterFunction::Url(_)))
}

/// Default “no-op” value for a function kind (for list-length padding during interpolation).
pub fn filter_identity_like(sample: &FilterFunction) -> FilterFunction {
    match sample {
        FilterFunction::Blur(_) => FilterFunction::Blur(Some(CssDim::Px(0.0))),
        FilterFunction::Brightness(_) => FilterFunction::Brightness(Some(1.0)),
        FilterFunction::Contrast(_) => FilterFunction::Contrast(Some(1.0)),
        FilterFunction::Grayscale(_) => FilterFunction::Grayscale(Some(0.0)),
        FilterFunction::HueRotate(_) => FilterFunction::HueRotate(Some(CssAngle::Deg(0.0))),
        FilterFunction::Invert(_) => FilterFunction::Invert(Some(0.0)),
        FilterFunction::Opacity(_) => FilterFunction::Opacity(Some(1.0)),
        FilterFunction::Saturate(_) => FilterFunction::Saturate(Some(1.0)),
        FilterFunction::Sepia(_) => FilterFunction::Sepia(Some(0.0)),
        FilterFunction::DropShadow { .. } => FilterFunction::DropShadow {
            color: Some([0.0, 0.0, 0.0, 0.0]),
            offset_x: CssDim::Px(0.0),
            offset_y: CssDim::Px(0.0),
            blur: Some(CssDim::Px(0.0)),
        },
        FilterFunction::Url(u) => FilterFunction::Url(u.clone()),
    }
}

/// Lerp two filter lists when kinds align (no url). Returns `None` → use discrete end.
pub fn interpolate_filter_lists(
    from: Option<&FilterList>,
    to: Option<&FilterList>,
    t: f32,
) -> Option<FilterList> {
    let t = t.clamp(0.0, 1.0);
    let empty: FilterList = Vec::new();
    let a = from.unwrap_or(&empty);
    let b = to.unwrap_or(&empty);
    if filter_list_has_url(a) || filter_list_has_url(b) {
        return None;
    }
    let n = a.len().max(b.len());
    if n == 0 {
        return Some(Vec::new());
    }
    // Pad shorter list with identity of the longer list's corresponding entry.
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let fa = a.get(i);
        let fb = b.get(i);
        let (left, right) = match (fa, fb) {
            (Some(l), Some(r)) => {
                if std::mem::discriminant(l) != std::mem::discriminant(r) {
                    return None;
                }
                (l.clone(), r.clone())
            }
            (Some(l), None) => (l.clone(), filter_identity_like(l)),
            (None, Some(r)) => (filter_identity_like(r), r.clone()),
            (None, None) => unreachable!(),
        };
        out.push(lerp_filter_fn(&left, &right, t)?);
    }
    Some(out)
}

fn lerp_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_dim(a: &CssDim, b: &CssDim, t: f32) -> Option<CssDim> {
    // Prefer px space when both resolve simply.
    let ap = dim_to_px_simple(a)?;
    let bp = dim_to_px_simple(b)?;
    Some(CssDim::Px(lerp_f(ap, bp, t)))
}

fn dim_to_px_simple(d: &CssDim) -> Option<f32> {
    match d {
        CssDim::Px(v) => Some(*v),
        CssDim::Rem(v) | CssDim::Em(v) => Some(*v * CSS_ROOT_FONT_SIZE_PX),
        _ => None,
    }
}

fn factor_or(v: Option<f32>, default: f32) -> f32 {
    v.unwrap_or(default)
}

fn lerp_filter_fn(a: &FilterFunction, b: &FilterFunction, t: f32) -> Option<FilterFunction> {
    match (a, b) {
        (FilterFunction::Blur(la), FilterFunction::Blur(lb)) => {
            let da = la.clone().unwrap_or(CssDim::Px(0.0));
            let db = lb.clone().unwrap_or(CssDim::Px(0.0));
            Some(FilterFunction::Blur(Some(lerp_dim(&da, &db, t)?)))
        }
        (FilterFunction::Brightness(la), FilterFunction::Brightness(lb)) => Some(
            FilterFunction::Brightness(Some(lerp_f(factor_or(*la, 1.0), factor_or(*lb, 1.0), t))),
        ),
        (FilterFunction::Contrast(la), FilterFunction::Contrast(lb)) => Some(
            FilterFunction::Contrast(Some(lerp_f(factor_or(*la, 1.0), factor_or(*lb, 1.0), t))),
        ),
        (FilterFunction::Grayscale(la), FilterFunction::Grayscale(lb)) => Some(
            FilterFunction::Grayscale(Some(lerp_f(factor_or(*la, 0.0), factor_or(*lb, 0.0), t))),
        ),
        (FilterFunction::Invert(la), FilterFunction::Invert(lb)) => Some(FilterFunction::Invert(
            Some(lerp_f(factor_or(*la, 0.0), factor_or(*lb, 0.0), t)),
        )),
        (FilterFunction::Opacity(la), FilterFunction::Opacity(lb)) => Some(
            FilterFunction::Opacity(Some(lerp_f(factor_or(*la, 1.0), factor_or(*lb, 1.0), t))),
        ),
        (FilterFunction::Saturate(la), FilterFunction::Saturate(lb)) => Some(
            FilterFunction::Saturate(Some(lerp_f(factor_or(*la, 1.0), factor_or(*lb, 1.0), t))),
        ),
        (FilterFunction::Sepia(la), FilterFunction::Sepia(lb)) => Some(FilterFunction::Sepia(
            Some(lerp_f(factor_or(*la, 0.0), factor_or(*lb, 0.0), t)),
        )),
        (FilterFunction::HueRotate(la), FilterFunction::HueRotate(lb)) => {
            let da = la.map(|a| a.to_deg()).unwrap_or(0.0);
            let db = lb.map(|a| a.to_deg()).unwrap_or(0.0);
            Some(FilterFunction::HueRotate(Some(CssAngle::Deg(lerp_f(
                da, db, t,
            )))))
        }
        (
            FilterFunction::DropShadow {
                color: ca,
                offset_x: ax,
                offset_y: ay,
                blur: ab,
            },
            FilterFunction::DropShadow {
                color: cb,
                offset_x: bx,
                offset_y: by,
                blur: bb,
            },
        ) => {
            let c0 = ca.unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let c1 = cb.unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let color = Some([
                lerp_f(c0[0], c1[0], t),
                lerp_f(c0[1], c1[1], t),
                lerp_f(c0[2], c1[2], t),
                lerp_f(c0[3], c1[3], t),
            ]);
            let blur_a = ab.clone().unwrap_or(CssDim::Px(0.0));
            let blur_b = bb.clone().unwrap_or(CssDim::Px(0.0));
            Some(FilterFunction::DropShadow {
                color,
                offset_x: lerp_dim(ax, bx, t)?,
                offset_y: lerp_dim(ay, by, t)?,
                blur: Some(lerp_dim(&blur_a, &blur_b, t)?),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_none() {
        assert!(parse_filter("none").is_none());
        assert!(parse_filter("").is_none());
    }

    #[test]
    fn parse_blur() {
        let list = parse_filter("blur(5px)").unwrap();
        assert_eq!(list.len(), 1);
        assert!(
            matches!(&list[0], FilterFunction::Blur(Some(CssDim::Px(v))) if (*v - 5.0).abs() < 1e-6)
        );
        assert!((filter_list_paint_outset(&list, 80.0, 80.0) - 15.0).abs() < 1e-6);
    }

    #[test]
    fn parse_brightness_percent_and_number() {
        let a = parse_filter("brightness(0.4)").unwrap();
        assert!(matches!(&a[0], FilterFunction::Brightness(Some(v)) if (*v - 0.4).abs() < 1e-6));
        let b = parse_filter("brightness(200%)").unwrap();
        assert!(matches!(&b[0], FilterFunction::Brightness(Some(v)) if (*v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn parse_multi() {
        let list = parse_filter("contrast(175%) brightness(1.03)").unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn parse_drop_shadow() {
        let list = parse_filter("drop-shadow(16px 16px 20px red)").unwrap();
        assert!(matches!(
            &list[0],
            FilterFunction::DropShadow {
                color: Some(_),
                offset_x: CssDim::Px(ox),
                offset_y: CssDim::Px(oy),
                blur: Some(CssDim::Px(b)),
            } if (*ox - 16.0).abs() < 1e-6 && (*oy - 16.0).abs() < 1e-6 && (*b - 20.0).abs() < 1e-6
        ));
    }

    #[test]
    fn parse_url() {
        let list = parse_filter("url(\"filters.svg#filter-id\")").unwrap();
        assert!(matches!(&list[0], FilterFunction::Url(u) if u == "filters.svg#filter-id"));
        let list2 = parse_filter("url(#blurMe)").unwrap();
        assert!(matches!(&list2[0], FilterFunction::Url(u) if u == "#blurMe"));
    }

    #[test]
    fn invalid_arg_yields_none() {
        assert!(parse_filter("blur(50%)").is_none());
        assert!(parse_filter("nope(1)").is_none());
    }

    #[test]
    fn hue_rotate_and_grayscale() {
        let list = parse_filter("hue-rotate(90deg) grayscale(80%)").unwrap();
        assert_eq!(list.len(), 2);
        assert!(
            matches!(&list[0], FilterFunction::HueRotate(Some(a)) if (a.to_deg() - 90.0).abs() < 1e-6)
        );
        assert!(matches!(&list[1], FilterFunction::Grayscale(Some(v)) if (*v - 0.8).abs() < 1e-6));
    }
}
