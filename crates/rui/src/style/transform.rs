//! CSS Transform support: `transform`, `transform-origin`, `transform-box`, `transform-style`.
//!
//! Implements the CSS Transforms Module Level 1 & 2 value types, parsing,
//! and 2D affine matrix resolution. 3D functions are stored but projected
//! to 2D at resolve time (true `preserve-3d` requires a 3D pipeline).

use crate::layout::dimension::{parse_css_angle, parse_css_dim, CssAngle, CssDim};

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

/// A single CSS `<transform-function>`.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformFunction {
    // 2D
    Translate(CssDim, CssDim),
    TranslateX(CssDim),
    TranslateY(CssDim),
    Scale(f32, Option<f32>),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(CssAngle),
    Skew(CssAngle, Option<CssAngle>),
    SkewX(CssAngle),
    SkewY(CssAngle),
    Matrix([f32; 6]),
    // 3D (stored; projected to 2D at resolve)
    Translate3d(CssDim, CssDim, CssDim),
    TranslateZ(CssDim),
    Scale3d(f32, f32, f32),
    ScaleZ(f32),
    Rotate3d(f32, f32, f32, CssAngle),
    RotateX(CssAngle),
    RotateY(CssAngle),
    RotateZ(CssAngle),
    Matrix3d([f32; 16]),
    Perspective(CssDim),
}

/// Ordered list of transform functions (CSS `transform` value).
pub type TransformList = Vec<TransformFunction>;

/// CSS `transform-origin` x-axis component.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformOriginX {
    Left,
    Center,
    Right,
    Length(CssDim),
}

/// CSS `transform-origin` y-axis component.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformOriginY {
    Top,
    Center,
    Bottom,
    Length(CssDim),
}

/// CSS `transform-origin`.  Default: `50% 50% 0`.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformOrigin {
    pub x: TransformOriginX,
    pub y: TransformOriginY,
    pub z: f32,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            x: TransformOriginX::Center,
            y: TransformOriginY::Center,
            z: 0.0,
        }
    }
}

/// CSS `transform-box`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransformBox {
    ContentBox,
    #[default]
    BorderBox,
    FillBox,
    StrokeBox,
    ViewBox,
}

/// CSS `transform-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransformStyle {
    #[default]
    Flat,
    Preserve3d,
}

// ---------------------------------------------------------------------------
// 2D affine matrix helpers  [a, b, c, d, tx, ty]
//
//  | a  c  tx |
//  | b  d  ty |
//  | 0  0   1 |
// ---------------------------------------------------------------------------

pub type Mat2d = [f32; 6];

pub const MAT2D_IDENTITY: Mat2d = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

pub fn mat2d_multiply(a: &Mat2d, b: &Mat2d) -> Mat2d {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

pub fn mat2d_translate(tx: f32, ty: f32) -> Mat2d {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

pub fn mat2d_scale(sx: f32, sy: f32) -> Mat2d {
    [sx, 0.0, 0.0, sy, 0.0, 0.0]
}

pub fn mat2d_rotate(rad: f32) -> Mat2d {
    let (s, c) = rad.sin_cos();
    [c, s, -s, c, 0.0, 0.0]
}

pub fn mat2d_skew(ax: f32, ay: f32) -> Mat2d {
    [1.0, ay.tan(), ax.tan(), 1.0, 0.0, 0.0]
}

/// Determinant of the 2x2 linear part.
pub fn mat2d_det(m: &Mat2d) -> f32 {
    m[0] * m[3] - m[1] * m[2]
}

/// Inverse of a 2D affine matrix. Returns identity if singular.
pub fn mat2d_inverse(m: &Mat2d) -> Mat2d {
    let det = mat2d_det(m);
    if det.abs() < 1e-12 {
        return MAT2D_IDENTITY;
    }
    let inv = 1.0 / det;
    [
        m[3] * inv,
        -m[1] * inv,
        -m[2] * inv,
        m[0] * inv,
        (m[2] * m[5] - m[3] * m[4]) * inv,
        (m[1] * m[4] - m[0] * m[5]) * inv,
    ]
}

/// Apply a 2D affine matrix to a point.
pub fn mat2d_transform_point(m: &Mat2d, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Check if a matrix is the identity (within epsilon).
pub fn mat2d_is_identity(m: &Mat2d) -> bool {
    (m[0] - 1.0).abs() < 1e-6
        && m[1].abs() < 1e-6
        && m[2].abs() < 1e-6
        && (m[3] - 1.0).abs() < 1e-6
        && m[4].abs() < 1e-6
        && m[5].abs() < 1e-6
}

// ---------------------------------------------------------------------------
// Resolve CssDim to px (simplified: no viewport context, only px/% supported)
// ---------------------------------------------------------------------------

fn dim_to_px(d: &CssDim, ref_size: f32) -> f32 {
    match d {
        CssDim::Px(v) => *v,
        CssDim::Percent(p) => *p * ref_size,
        CssDim::Em(v) => *v * 16.0, // root em fallback
        CssDim::Rem(v) => *v * 16.0,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// TransformFunction -> Mat2d
// ---------------------------------------------------------------------------

fn func_to_mat2d(f: &TransformFunction, w: f32, h: f32) -> Mat2d {
    match f {
        TransformFunction::Translate(x, y) => mat2d_translate(dim_to_px(x, w), dim_to_px(y, h)),
        TransformFunction::TranslateX(x) => mat2d_translate(dim_to_px(x, w), 0.0),
        TransformFunction::TranslateY(y) => mat2d_translate(0.0, dim_to_px(y, h)),
        TransformFunction::Translate3d(x, y, _z) => {
            mat2d_translate(dim_to_px(x, w), dim_to_px(y, h))
        }
        TransformFunction::TranslateZ(_) => MAT2D_IDENTITY,
        TransformFunction::Scale(sx, sy) => {
            let sy = sy.unwrap_or(*sx);
            mat2d_scale(*sx, sy)
        }
        TransformFunction::ScaleX(sx) => mat2d_scale(*sx, 1.0),
        TransformFunction::ScaleY(sy) => mat2d_scale(1.0, *sy),
        TransformFunction::Scale3d(sx, sy, _sz) => mat2d_scale(*sx, *sy),
        TransformFunction::ScaleZ(_) => MAT2D_IDENTITY,
        TransformFunction::Rotate(a) | TransformFunction::RotateZ(a) => mat2d_rotate(a.to_rad()),
        TransformFunction::RotateX(a) => {
            // Project: scale Y by cos(angle)
            mat2d_scale(1.0, a.to_rad().cos())
        }
        TransformFunction::RotateY(a) => {
            // Project: scale X by cos(angle)
            mat2d_scale(a.to_rad().cos(), 1.0)
        }
        TransformFunction::Rotate3d(x, y, z, a) => {
            // Simplified: if mostly Z-rotation, do 2D rotate; otherwise approximate
            let len = (x * x + y * y + z * z).sqrt();
            if len < 1e-6 {
                return MAT2D_IDENTITY;
            }
            let (nx, _ny, nz) = (x / len, y / len, z / len);
            // For pure Z-axis rotation
            if nx.abs() < 1e-6 && nz.abs() > 0.99 {
                return mat2d_rotate(a.to_rad() * nz.signum());
            }
            // General case: just rotate by the Z component projection
            mat2d_rotate(a.to_rad() * nz)
        }
        TransformFunction::Skew(ax, ay) => {
            let ay = ay.unwrap_or(CssAngle::Deg(0.0));
            mat2d_skew(ax.to_rad(), ay.to_rad())
        }
        TransformFunction::SkewX(a) => mat2d_skew(a.to_rad(), 0.0),
        TransformFunction::SkewY(a) => mat2d_skew(0.0, a.to_rad()),
        TransformFunction::Matrix(vals) => *vals,
        TransformFunction::Matrix3d(m) => {
            // Extract 2D affine from 4x4 column-major
            [m[0], m[1], m[4], m[5], m[12], m[13]]
        }
        TransformFunction::Perspective(_) => MAT2D_IDENTITY, // no-op in 2D
    }
}

// ---------------------------------------------------------------------------
// Resolve origin to px
// ---------------------------------------------------------------------------

fn origin_x_px(ox: &TransformOriginX, w: f32) -> f32 {
    match ox {
        TransformOriginX::Left => 0.0,
        TransformOriginX::Center => w * 0.5,
        TransformOriginX::Right => w,
        TransformOriginX::Length(d) => dim_to_px(d, w),
    }
}

fn origin_y_px(oy: &TransformOriginY, h: f32) -> f32 {
    match oy {
        TransformOriginY::Top => 0.0,
        TransformOriginY::Center => h * 0.5,
        TransformOriginY::Bottom => h,
        TransformOriginY::Length(d) => dim_to_px(d, h),
    }
}

// ---------------------------------------------------------------------------
// Public resolve API
// ---------------------------------------------------------------------------

/// Resolve a `TransformList` + origin into a 2D affine matrix.
///
/// Per CSS spec the effective transform is:
///   translate(origin) * product_of_functions * translate(-origin)
pub fn resolve_transform_matrix(
    list: &TransformList,
    origin: &TransformOrigin,
    box_w: f32,
    box_h: f32,
) -> Mat2d {
    if list.is_empty() {
        return MAT2D_IDENTITY;
    }
    let ox = origin_x_px(&origin.x, box_w);
    let oy = origin_y_px(&origin.y, box_h);

    let mut product = MAT2D_IDENTITY;
    for f in list {
        let m = func_to_mat2d(f, box_w, box_h);
        product = mat2d_multiply(&product, &m);
    }

    // origin shift
    let pre = mat2d_translate(ox, oy);
    let post = mat2d_translate(-ox, -oy);
    mat2d_multiply(&pre, &mat2d_multiply(&product, &post))
}

/// Combine the CSS independent transform properties (`translate`, `rotate`, `scale`)
/// with the `transform` function list, respecting the CSS spec application order:
///   translate -> rotate -> scale -> transform
///
/// Returns the composed 2D affine matrix.
pub fn compute_combined_transform(
    translate: &[f32; 2],
    rotate_deg: f32,
    scale: &[f32; 2],
    transform_list: Option<&TransformList>,
    origin: &TransformOrigin,
    box_w: f32,
    box_h: f32,
) -> Mat2d {
    let has_indep = translate[0].abs() > 1e-6
        || translate[1].abs() > 1e-6
        || rotate_deg.abs() > 1e-6
        || (scale[0] - 1.0).abs() > 1e-6
        || (scale[1] - 1.0).abs() > 1e-6;
    let has_list = transform_list.map_or(false, |l| !l.is_empty());

    if !has_indep && !has_list {
        return MAT2D_IDENTITY;
    }

    let ox = origin_x_px(&origin.x, box_w);
    let oy = origin_y_px(&origin.y, box_h);

    let mut m = MAT2D_IDENTITY;

    // Independent properties: translate -> rotate -> scale
    if has_indep {
        if translate[0].abs() > 1e-6 || translate[1].abs() > 1e-6 {
            m = mat2d_multiply(&m, &mat2d_translate(translate[0], translate[1]));
        }
        if rotate_deg.abs() > 1e-6 {
            let rad = rotate_deg * std::f32::consts::PI / 180.0;
            m = mat2d_multiply(&m, &mat2d_rotate(rad));
        }
        if (scale[0] - 1.0).abs() > 1e-6 || (scale[1] - 1.0).abs() > 1e-6 {
            m = mat2d_multiply(&m, &mat2d_scale(scale[0], scale[1]));
        }
    }

    // Then the `transform` property list
    if let Some(list) = transform_list {
        for f in list {
            let fm = func_to_mat2d(f, box_w, box_h);
            m = mat2d_multiply(&m, &fm);
        }
    }

    // Wrap with origin
    let pre = mat2d_translate(ox, oy);
    let post = mat2d_translate(-ox, -oy);
    mat2d_multiply(&pre, &mat2d_multiply(&m, &post))
}

// ---------------------------------------------------------------------------
// Parsing: transform
// ---------------------------------------------------------------------------

/// Parse a CSS `transform` property value.
/// Returns `None` for `"none"`, `Some(list)` for a valid function list.
pub fn parse_transform(s: &str) -> Option<TransformList> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") || s.is_empty() {
        return None;
    }

    let mut list = Vec::new();
    let mut rest = s;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let open = rest.find('(')?;
        let name = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let close = find_matching_paren(after_open)?;
        let args_str = &after_open[..close];
        rest = after_open[close + 1..].trim_start();

        let func = parse_transform_func(name, args_str)?;
        list.push(func);
    }

    if list.is_empty() {
        None
    } else {
        Some(list)
    }
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

fn split_args(s: &str) -> Vec<&str> {
    s.split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .collect()
}

fn parse_num(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok()
}

fn parse_dim_or_zero(s: &str) -> Option<CssDim> {
    let s = s.trim();
    if s == "0" {
        return Some(CssDim::Px(0.0));
    }
    parse_css_dim(s)
}

fn parse_angle_or_zero(s: &str) -> Option<CssAngle> {
    parse_css_angle(s.trim())
}

fn parse_transform_func(name: &str, args: &str) -> Option<TransformFunction> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "translate" => {
            let parts = split_args(args);
            match parts.len() {
                1 => {
                    let x = parse_dim_or_zero(parts[0])?;
                    Some(TransformFunction::Translate(x, CssDim::Px(0.0)))
                }
                2 => {
                    let x = parse_dim_or_zero(parts[0])?;
                    let y = parse_dim_or_zero(parts[1])?;
                    Some(TransformFunction::Translate(x, y))
                }
                _ => None,
            }
        }
        "translatex" => {
            let x = parse_dim_or_zero(args)?;
            Some(TransformFunction::TranslateX(x))
        }
        "translatey" => {
            let y = parse_dim_or_zero(args)?;
            Some(TransformFunction::TranslateY(y))
        }
        "translatez" => {
            let z = parse_dim_or_zero(args)?;
            Some(TransformFunction::TranslateZ(z))
        }
        "translate3d" => {
            let parts = split_args(args);
            if parts.len() != 3 {
                return None;
            }
            let x = parse_dim_or_zero(parts[0])?;
            let y = parse_dim_or_zero(parts[1])?;
            let z = parse_dim_or_zero(parts[2])?;
            Some(TransformFunction::Translate3d(x, y, z))
        }
        "scale" => {
            let parts = split_args(args);
            match parts.len() {
                1 => {
                    let sx = parse_num(parts[0])?;
                    Some(TransformFunction::Scale(sx, None))
                }
                2 => {
                    let sx = parse_num(parts[0])?;
                    let sy = parse_num(parts[1])?;
                    Some(TransformFunction::Scale(sx, Some(sy)))
                }
                _ => None,
            }
        }
        "scalex" => {
            let sx = parse_num(args)?;
            Some(TransformFunction::ScaleX(sx))
        }
        "scaley" => {
            let sy = parse_num(args)?;
            Some(TransformFunction::ScaleY(sy))
        }
        "scalez" => {
            let sz = parse_num(args)?;
            Some(TransformFunction::ScaleZ(sz))
        }
        "scale3d" => {
            let parts = split_args(args);
            if parts.len() != 3 {
                return None;
            }
            let sx = parse_num(parts[0])?;
            let sy = parse_num(parts[1])?;
            let sz = parse_num(parts[2])?;
            Some(TransformFunction::Scale3d(sx, sy, sz))
        }
        "rotate" | "rotatez" => {
            let a = parse_angle_or_zero(args)?;
            if lower == "rotatez" {
                Some(TransformFunction::RotateZ(a))
            } else {
                Some(TransformFunction::Rotate(a))
            }
        }
        "rotatex" => {
            let a = parse_angle_or_zero(args)?;
            Some(TransformFunction::RotateX(a))
        }
        "rotatey" => {
            let a = parse_angle_or_zero(args)?;
            Some(TransformFunction::RotateY(a))
        }
        "rotate3d" => {
            let parts = split_args(args);
            if parts.len() != 4 {
                return None;
            }
            let x = parse_num(parts[0])?;
            let y = parse_num(parts[1])?;
            let z = parse_num(parts[2])?;
            let a = parse_angle_or_zero(parts[3])?;
            Some(TransformFunction::Rotate3d(x, y, z, a))
        }
        "skew" => {
            let parts = split_args(args);
            match parts.len() {
                1 => {
                    let ax = parse_angle_or_zero(parts[0])?;
                    Some(TransformFunction::Skew(ax, None))
                }
                2 => {
                    let ax = parse_angle_or_zero(parts[0])?;
                    let ay = parse_angle_or_zero(parts[1])?;
                    Some(TransformFunction::Skew(ax, Some(ay)))
                }
                _ => None,
            }
        }
        "skewx" => {
            let a = parse_angle_or_zero(args)?;
            Some(TransformFunction::SkewX(a))
        }
        "skewy" => {
            let a = parse_angle_or_zero(args)?;
            Some(TransformFunction::SkewY(a))
        }
        "matrix" => {
            let parts = split_args(args);
            if parts.len() != 6 {
                return None;
            }
            let vals: Option<Vec<f32>> = parts.iter().map(|p| parse_num(p)).collect();
            let v = vals?;
            Some(TransformFunction::Matrix([
                v[0], v[1], v[2], v[3], v[4], v[5],
            ]))
        }
        "matrix3d" => {
            let parts = split_args(args);
            if parts.len() != 16 {
                return None;
            }
            let vals: Option<Vec<f32>> = parts.iter().map(|p| parse_num(p)).collect();
            let v = vals?;
            let mut arr = [0.0f32; 16];
            arr.copy_from_slice(&v);
            Some(TransformFunction::Matrix3d(arr))
        }
        "perspective" => {
            let s = args.trim();
            if s.eq_ignore_ascii_case("none") {
                return Some(TransformFunction::Perspective(CssDim::Px(0.0)));
            }
            let d = parse_dim_or_zero(s)?;
            Some(TransformFunction::Perspective(d))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parsing: transform-origin
// ---------------------------------------------------------------------------

/// Parse a CSS `transform-origin` value (1, 2, or 3 values).
pub fn parse_transform_origin(s: &str) -> TransformOrigin {
    let s = s.trim();
    if s.is_empty() {
        return TransformOrigin::default();
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.len() {
        1 => parse_origin_one(parts[0]),
        2 => parse_origin_two(parts[0], parts[1]),
        3 => {
            let mut o = parse_origin_two(parts[0], parts[1]);
            if let Some(z) = parse_num(parts[2].strip_suffix("px").unwrap_or(parts[2])) {
                o.z = z;
            }
            o
        }
        _ => TransformOrigin::default(),
    }
}

fn parse_origin_x_keyword(s: &str) -> Option<TransformOriginX> {
    match s.to_ascii_lowercase().as_str() {
        "left" => Some(TransformOriginX::Left),
        "center" => Some(TransformOriginX::Center),
        "right" => Some(TransformOriginX::Right),
        _ => None,
    }
}

fn parse_origin_y_keyword(s: &str) -> Option<TransformOriginY> {
    match s.to_ascii_lowercase().as_str() {
        "top" => Some(TransformOriginY::Top),
        "center" => Some(TransformOriginY::Center),
        "bottom" => Some(TransformOriginY::Bottom),
        _ => None,
    }
}

fn parse_origin_one(s: &str) -> TransformOrigin {
    // keyword or length/percentage -> x-offset, y defaults to center
    if let Some(x) = parse_origin_x_keyword(s) {
        return TransformOrigin {
            x,
            ..Default::default()
        };
    }
    if let Some(y) = parse_origin_y_keyword(s) {
        return TransformOrigin {
            y,
            ..Default::default()
        };
    }
    if let Some(d) = parse_css_dim(s) {
        return TransformOrigin {
            x: TransformOriginX::Length(d),
            ..Default::default()
        };
    }
    TransformOrigin::default()
}

fn parse_origin_two(a: &str, b: &str) -> TransformOrigin {
    // Handle keyword pairs (can be in either order)
    let ax = parse_origin_x_keyword(a);
    let ay = parse_origin_y_keyword(a);
    let bx = parse_origin_x_keyword(b);
    let by = parse_origin_y_keyword(b);

    // Both keywords
    if let (Some(x), Some(y)) = (ax.clone(), by.clone()) {
        return TransformOrigin { x, y, z: 0.0 };
    }
    // Reversed keywords (y-keyword first, x-keyword second)
    if let (Some(y), Some(x)) = (ay.clone(), bx.clone()) {
        return TransformOrigin { x, y, z: 0.0 };
    }

    // First is keyword or length, second is keyword or length
    let x = if let Some(xk) = ax {
        xk
    } else if let Some(d) = parse_css_dim(a) {
        TransformOriginX::Length(d)
    } else {
        TransformOriginX::Center
    };

    let y = if let Some(yk) = by.or(ay) {
        yk
    } else if let Some(d) = parse_css_dim(b) {
        TransformOriginY::Length(d)
    } else {
        TransformOriginY::Center
    };

    TransformOrigin { x, y, z: 0.0 }
}

/// Parse CSS `transform-box` property.
pub fn parse_transform_box(s: &str) -> TransformBox {
    match s.trim().to_ascii_lowercase().as_str() {
        "content-box" => TransformBox::ContentBox,
        "border-box" => TransformBox::BorderBox,
        "fill-box" => TransformBox::FillBox,
        "stroke-box" => TransformBox::StrokeBox,
        "view-box" => TransformBox::ViewBox,
        _ => TransformBox::BorderBox,
    }
}

/// Parse CSS `transform-style` property.
pub fn parse_transform_style(s: &str) -> TransformStyle {
    match s.trim().to_ascii_lowercase().as_str() {
        "preserve-3d" => TransformStyle::Preserve3d,
        _ => TransformStyle::Flat,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_none() {
        assert!(parse_transform("none").is_none());
    }

    #[test]
    fn parse_rotate_deg() {
        let list = parse_transform("rotate(45deg)").unwrap();
        assert_eq!(list.len(), 1);
        assert!(
            matches!(list[0], TransformFunction::Rotate(CssAngle::Deg(d)) if (d - 45.0).abs() < 1e-6)
        );
    }

    #[test]
    fn parse_multi_functions() {
        let list = parse_transform("translateX(10px) rotate(90deg) scale(2, 0.5)").unwrap();
        assert_eq!(list.len(), 3);
        assert!(
            matches!(&list[0], TransformFunction::TranslateX(CssDim::Px(v)) if (*v - 10.0).abs() < 1e-6)
        );
        assert!(
            matches!(&list[1], TransformFunction::Rotate(CssAngle::Deg(d)) if (*d - 90.0).abs() < 1e-6)
        );
        assert!(
            matches!(&list[2], TransformFunction::Scale(sx, Some(sy)) if (*sx - 2.0).abs() < 1e-6 && (*sy - 0.5).abs() < 1e-6)
        );
    }

    #[test]
    fn parse_matrix() {
        let list = parse_transform("matrix(1, 0, 0, 1, 10, 20)").unwrap();
        assert_eq!(list.len(), 1);
        if let TransformFunction::Matrix(v) = &list[0] {
            assert!((v[4] - 10.0).abs() < 1e-6);
            assert!((v[5] - 20.0).abs() < 1e-6);
        } else {
            panic!("expected Matrix");
        }
    }

    #[test]
    fn parse_skew() {
        let list = parse_transform("skewX(30deg)").unwrap();
        assert!(
            matches!(&list[0], TransformFunction::SkewX(CssAngle::Deg(d)) if (*d - 30.0).abs() < 1e-6)
        );
    }

    #[test]
    fn resolve_translate_origin() {
        let list = vec![TransformFunction::TranslateX(CssDim::Px(10.0))];
        let origin = TransformOrigin::default(); // center
        let m = resolve_transform_matrix(&list, &origin, 100.0, 100.0);
        // With center origin, translateX should just add tx
        assert!((m[4] - 10.0).abs() < 1e-3);
        assert!(m[5].abs() < 1e-3);
    }

    #[test]
    fn resolve_rotate_45() {
        let list = vec![TransformFunction::Rotate(CssAngle::Deg(45.0))];
        let origin = TransformOrigin {
            x: TransformOriginX::Length(CssDim::Px(0.0)),
            y: TransformOriginY::Length(CssDim::Px(0.0)),
            z: 0.0,
        };
        let m = resolve_transform_matrix(&list, &origin, 100.0, 100.0);
        let cos45 = 45.0f32.to_radians().cos();
        assert!((m[0] - cos45).abs() < 1e-3);
    }

    #[test]
    fn parse_origin_keywords() {
        let o = parse_transform_origin("left top");
        assert_eq!(o.x, TransformOriginX::Left);
        assert_eq!(o.y, TransformOriginY::Top);

        let o2 = parse_transform_origin("center");
        assert_eq!(o2.x, TransformOriginX::Center);
        assert_eq!(o2.y, TransformOriginY::Center);
    }

    #[test]
    fn parse_origin_lengths() {
        let o = parse_transform_origin("50% 50%");
        assert!(
            matches!(o.x, TransformOriginX::Length(CssDim::Percent(p)) if (p - 0.5).abs() < 1e-6)
        );
        assert!(
            matches!(o.y, TransformOriginY::Length(CssDim::Percent(p)) if (p - 0.5).abs() < 1e-6)
        );
    }

    #[test]
    fn mat2d_inverse_identity() {
        let inv = mat2d_inverse(&MAT2D_IDENTITY);
        assert!((inv[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn combined_translate_rotate_scale() {
        let m = compute_combined_transform(
            &[10.0, 20.0],
            45.0,
            &[2.0, 2.0],
            None,
            &TransformOrigin::default(),
            100.0,
            100.0,
        );
        // Should not be identity
        assert!(!mat2d_is_identity(&m));
    }
}
