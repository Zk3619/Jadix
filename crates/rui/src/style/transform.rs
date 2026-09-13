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

/// Keep the 2×2 linear part; zero translation.
///
/// [`compute_combined_transform`] wraps CSS origin in **top-left** space. SDF /
/// textured VS pivot about the instance **rect center**, so that wrap's `tx/ty`
/// must not be copied onto the instance. Shader origin lives in
/// [`mat2d_center_pivot_origin_tx_ty`].
pub fn mat2d_linear(m: &Mat2d) -> Mat2d {
    [m[0], m[1], m[2], m[3], 0.0, 0.0]
}

/// Replace translation; keep the 2×2 linear part.
pub fn mat2d_set_translation(m: &Mat2d, tx: f32, ty: f32) -> Mat2d {
    [m[0], m[1], m[2], m[3], tx, ty]
}

/// Extra `tx/ty` so a **center-pivoted** VS (`center + R*local + txty`) matches
/// CSS `transform-origin`: `origin + R*(p - origin)`.
///
/// `txty = (I - linear) * (origin - size/2)`. Center origin → `[0, 0]`.
pub fn mat2d_center_pivot_origin_tx_ty(
    linear: &Mat2d,
    origin: [f32; 2],
    box_wh: [f32; 2],
) -> [f32; 2] {
    let dx = origin[0] - box_wh[0] * 0.5;
    let dy = origin[1] - box_wh[1] * 0.5;
    let (rx, ry) = mat2d_transform_vec(linear, dx, dy);
    [dx - rx, dy - ry]
}

/// World paint affine: ancestor×local 2×2, plus origin `tx/ty` transformed by
/// the ancestor linear (same VS contract as [`mat2d_center_pivot_origin_tx_ty`]).
pub fn mat2d_world_center_pivot(
    ancestor_linear: &Mat2d,
    local_linear: &Mat2d,
    origin_in_box: [f32; 2],
    box_wh: [f32; 2],
) -> Mat2d {
    mat2d_world_center_pivot_with_shift(
        ancestor_linear,
        local_linear,
        origin_in_box,
        box_wh,
        [0.0, 0.0],
    )
}

/// [`mat2d_world_center_pivot`] plus CSS `transform` list translation.
///
/// `compute_combined_transform` wraps origin in top-left space; [`mat2d_linear`]
/// then drops **all** translation, including list `translate()`. VS `tx/ty` is
/// origin correction **plus** that list shift, then transformed by the ancestor 2×2.
pub fn mat2d_world_center_pivot_with_shift(
    ancestor_linear: &Mat2d,
    local_linear: &Mat2d,
    origin_in_box: [f32; 2],
    box_wh: [f32; 2],
    list_t: [f32; 2],
) -> Mat2d {
    let world_linear = mat2d_multiply(ancestor_linear, local_linear);
    let origin_txty = mat2d_center_pivot_origin_tx_ty(local_linear, origin_in_box, box_wh);
    let (wtx, wty) = mat2d_transform_vec(
        ancestor_linear,
        origin_txty[0] + list_t[0],
        origin_txty[1] + list_t[1],
    );
    mat2d_set_translation(&world_linear, wtx, wty)
}

/// True when the 2×2 part is (nearly) a positive axis-aligned scale — no rotate/skew.
pub fn mat2d_is_axis_aligned(m: &Mat2d) -> bool {
    m[1].abs() < 1e-5 && m[2].abs() < 1e-5
}

/// Apply only the 2×2 linear part (ignore translation).
pub fn mat2d_transform_vec(m: &Mat2d, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y, m[1] * x + m[3] * y)
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

/// CSS `transform-origin` in box px from the top-left (keywords / % / length).
pub fn origin_px(origin: &TransformOrigin, w: f32, h: f32) -> [f32; 2] {
    [origin_x_px(&origin.x, w), origin_y_px(&origin.y, h)]
}

/// Scale a border box about a CSS origin (px from the box top-left).
///
/// Center origin matches the old `(size - scaled) * 0.5` inset used by collect
/// vis-bake and [`crate::core::tree::visual_paint_aabb`].
pub fn scale_box_about_origin(xywh: [f32; 4], origin: [f32; 2], scale: [f32; 2]) -> [f32; 4] {
    let sx = if scale[0].is_finite() && scale[0] > 0.0 {
        scale[0]
    } else {
        1.0
    };
    let sy = if scale[1].is_finite() && scale[1] > 0.0 {
        scale[1]
    } else {
        1.0
    };
    [
        xywh[0] + origin[0] * (1.0 - sx),
        xywh[1] + origin[1] * (1.0 - sy),
        xywh[2] * sx,
        xywh[3] * sy,
    ]
}

/// World AABB of a textured / SDF quad after the same center-pivot linear
/// transform the VS uses: `center + R*local + txty`.
pub fn aabb_of_centered_transform(rect: [f32; 4], m: &Mat2d) -> [f32; 4] {
    let [x, y, w, h] = rect;
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let corners = [
        [-0.5 * w, -0.5 * h],
        [0.5 * w, -0.5 * h],
        [0.5 * w, 0.5 * h],
        [-0.5 * w, 0.5 * h],
    ];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for [lx, ly] in corners {
        let px = cx + m[0] * lx + m[2] * ly + m[4];
        let py = cy + m[1] * lx + m[3] * ly + m[5];
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    [
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
    ]
}

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

/// `transform-origin` as fractions of the border box (0–1), independent of used size.
pub fn origin_frac(origin: &TransformOrigin) -> [f32; 2] {
    [origin_axis_frac_x(&origin.x), origin_axis_frac_y(&origin.y)]
}

fn origin_axis_frac_x(ox: &TransformOriginX) -> f32 {
    match ox {
        TransformOriginX::Left => 0.0,
        TransformOriginX::Center => 0.5,
        TransformOriginX::Right => 1.0,
        TransformOriginX::Length(CssDim::Percent(p)) => *p,
        TransformOriginX::Length(_) => 0.5,
    }
}

fn origin_axis_frac_y(oy: &TransformOriginY) -> f32 {
    match oy {
        TransformOriginY::Top => 0.0,
        TransformOriginY::Center => 0.5,
        TransformOriginY::Bottom => 1.0,
        TransformOriginY::Length(CssDim::Percent(p)) => *p,
        TransformOriginY::Length(_) => 0.5,
    }
}

/// Write animated `transform-origin` fractions back onto the specified origin.
pub fn set_origin_frac(origin: &mut TransformOrigin, frac: [f32; 2]) {
    origin.x = TransformOriginX::Length(CssDim::Percent(frac[0].clamp(0.0, 1.0)));
    origin.y = TransformOriginY::Length(CssDim::Percent(frac[1].clamp(0.0, 1.0)));
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

/// Independent `rotate` plus the CSS `transform` list, **without** origin wrap,
/// independent translate, or scale.
///
/// Collect / VS bake scale into the instance rect and independent translate into
/// `render_x/y`. Origin lives in [`mat2d_center_pivot_origin_tx_ty`]. The remaining
/// matrix translation is list `translate()` / `translateX/Y` the VS must keep.
pub fn rotate_and_list_mat2d(
    rotate_deg: f32,
    transform_list: Option<&TransformList>,
    box_w: f32,
    box_h: f32,
) -> Mat2d {
    let mut m = MAT2D_IDENTITY;
    if rotate_deg.abs() > 1e-6 {
        m = mat2d_multiply(&m, &mat2d_rotate(rotate_deg * std::f32::consts::PI / 180.0));
    }
    if let Some(list) = transform_list {
        for f in list {
            m = mat2d_multiply(&m, &func_to_mat2d(f, box_w, box_h));
        }
    }
    m
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
    fn center_pivot_origin_tx_ty_is_zero_at_center() {
        let r = mat2d_rotate(45.0f32.to_radians());
        let txty = mat2d_center_pivot_origin_tx_ty(&r, [50.0, 50.0], [100.0, 100.0]);
        assert!(
            txty[0].abs() < 1e-4 && txty[1].abs() < 1e-4,
            "center origin must not add shader translation, got {txty:?}"
        );
    }

    #[test]
    fn center_pivot_origin_tx_ty_left_top_rotate_90() {
        // y-down rotate 90°: (x,y) → (-y, x). About top-left, box center (50,50)
        // maps to (-50, 50). Shader: center + txty = (50,50)+txty must match.
        let r = mat2d_rotate(90.0f32.to_radians());
        let txty = mat2d_center_pivot_origin_tx_ty(&r, [0.0, 0.0], [100.0, 100.0]);
        assert!(
            (txty[0] + 100.0).abs() < 1e-3 && txty[1].abs() < 1e-3,
            "left-top 90° on 100×100 → txty (-100, 0), got {txty:?}"
        );
        let world = mat2d_world_center_pivot(&MAT2D_IDENTITY, &r, [0.0, 0.0], [100.0, 100.0]);
        assert!((world[4] + 100.0).abs() < 1e-3 && world[5].abs() < 1e-3);
        let identity_txty =
            mat2d_center_pivot_origin_tx_ty(&MAT2D_IDENTITY, [0.0, 0.0], [100.0, 100.0]);
        assert!(
            identity_txty[0].abs() < 1e-4 && identity_txty[1].abs() < 1e-4,
            "identity linear must not shift a non-center origin, got {identity_txty:?}"
        );
    }

    #[test]
    fn scale_box_about_origin_center_matches_old_inset() {
        let center = scale_box_about_origin([10.0, 20.0, 100.0, 40.0], [50.0, 20.0], [2.0, 2.0]);
        assert!((center[0] - (10.0 - 50.0)).abs() < 1e-4);
        assert!((center[1] - (20.0 - 20.0)).abs() < 1e-4);
        assert!((center[2] - 200.0).abs() < 1e-4 && (center[3] - 80.0).abs() < 1e-4);
        let left_top = scale_box_about_origin([10.0, 20.0, 100.0, 40.0], [0.0, 0.0], [2.0, 2.0]);
        assert!(
            (left_top[0] - 10.0).abs() < 1e-4 && (left_top[1] - 20.0).abs() < 1e-4,
            "left-top origin must keep the top-left fixed, got {left_top:?}"
        );
        assert!((left_top[2] - 200.0).abs() < 1e-4);
        assert!(
            (left_top[0] - center[0]).abs() > 1.0,
            "left-top and center scale must place the box differently"
        );
    }

    #[test]
    fn aabb_of_centered_transform_identity_keeps_rect() {
        let r = [10.0, 20.0, 100.0, 40.0];
        let out = aabb_of_centered_transform(r, &MAT2D_IDENTITY);
        for i in 0..4 {
            assert!(
                (out[i] - r[i]).abs() < 1e-4,
                "identity AABB {out:?} vs {r:?}"
            );
        }
    }

    #[test]
    fn aabb_of_centered_transform_left_top_rotate_90() {
        let r90 = mat2d_rotate(90.0f32.to_radians());
        let m = mat2d_world_center_pivot(&MAT2D_IDENTITY, &r90, [0.0, 0.0], [100.0, 100.0]);
        let aabb = aabb_of_centered_transform([0.0, 0.0, 100.0, 100.0], &m);
        assert!(
            (aabb[0] + 100.0).abs() < 1e-3 && aabb[1].abs() < 1e-3,
            "left-top 90° AABB should sit at (-100, 0), got {aabb:?}"
        );
        assert!((aabb[2] - 100.0).abs() < 1e-3 && (aabb[3] - 100.0).abs() < 1e-3);
    }

    #[test]
    fn cascade_linear_multiply_composes_rotates() {
        let a = mat2d_rotate(45.0f32.to_radians());
        let b = mat2d_rotate(45.0f32.to_radians());
        let c = mat2d_multiply(&a, &b);
        let expected = mat2d_rotate(90.0f32.to_radians());
        for i in 0..4 {
            assert!(
                (c[i] - expected[i]).abs() < 1e-4,
                "cascade linear mismatch at {i}: {} vs {}",
                c[i],
                expected[i]
            );
        }
        assert!(!mat2d_is_axis_aligned(&a));
        assert!(mat2d_is_axis_aligned(&MAT2D_IDENTITY));
        let (vx, vy) = mat2d_transform_vec(&mat2d_rotate(90.0f32.to_radians()), 1.0, 0.0);
        assert!((vx - 0.0).abs() < 1e-4 && (vy - 1.0).abs() < 1e-4);
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

    #[test]
    fn rotate_and_list_keeps_translate() {
        let list = parse_transform("translate(40px, 10px)").unwrap();
        let m = rotate_and_list_mat2d(0.0, Some(&list), 100.0, 80.0);
        assert!((m[4] - 40.0).abs() < 1e-4 && (m[5] - 10.0).abs() < 1e-4);
        assert!(mat2d_is_identity(&mat2d_linear(&m)));
    }
}
