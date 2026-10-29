//! Any SVG primitive → cubic Bézier segments.

use std::collections::HashMap;

use super::parse::{parse_path, RawSeg, RawSubpath};
use super::serialize::cubics_to_path_d;
use super::types::{morph_err, CubicPath, IconInput, MorphError};

/// Control-point offset for a quarter circle: (4/3)·tan(π/8) ≈ 0.5523.
pub const KAPPA: f64 = 0.5522847498307936;
const TAU: f64 = std::f64::consts::TAU;

struct Builder {
    pts: Vec<f64>,
    cx: f64,
    cy: f64,
}

impl Builder {
    fn new(x0: f64, y0: f64) -> Self {
        Self {
            pts: vec![x0, y0],
            cx: x0,
            cy: y0,
        }
    }

    fn cubic(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        self.pts.extend_from_slice(&[x1, y1, x2, y2, x, y]);
        self.cx = x;
        self.cy = y;
    }

    fn line(&mut self, x: f64, y: f64) {
        if (x - self.cx).abs() < 1e-12 && (y - self.cy).abs() < 1e-12 {
            return;
        }
        let cx = self.cx;
        let cy = self.cy;
        self.cubic(
            cx + (x - cx) / 3.0,
            cy + (y - cy) / 3.0,
            cx + (2.0 * (x - cx)) / 3.0,
            cy + (2.0 * (y - cy)) / 3.0,
            x,
            y,
        );
    }

    fn quad(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        let cx = self.cx;
        let cy = self.cy;
        self.cubic(
            cx + (2.0 / 3.0) * (x1 - cx),
            cy + (2.0 / 3.0) * (y1 - cy),
            x + (2.0 / 3.0) * (x1 - x),
            y + (2.0 / 3.0) * (y1 - y),
            x,
            y,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "SVG arcs supply radii, rotation, flags and endpoint coordinates"
    )]
    fn arc(&mut self, rx0: f64, ry0: f64, rot_deg: f64, large: u8, sweep: u8, x: f64, y: f64) {
        let x1 = self.cx;
        let y1 = self.cy;
        if (x - x1).abs() < 1e-12 && (y - y1).abs() < 1e-12 {
            return;
        }
        let mut rx = rx0.abs();
        let mut ry = ry0.abs();
        if rx < 1e-12 || ry < 1e-12 {
            self.line(x, y);
            return;
        }
        let phi = rot_deg * std::f64::consts::PI / 180.0;
        let cos_p = phi.cos();
        let sin_p = phi.sin();
        let hx = (x1 - x) / 2.0;
        let hy = (y1 - y) / 2.0;
        let x1p = cos_p * hx + sin_p * hy;
        let y1p = -sin_p * hx + cos_p * hy;
        let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lam > 1.0 {
            let s = lam.sqrt();
            rx *= s;
            ry *= s;
        }
        let rx2 = rx * rx;
        let ry2 = ry * ry;
        let xp2 = x1p * x1p;
        let yp2 = y1p * y1p;
        let mut rad = (rx2 * ry2 - rx2 * yp2 - ry2 * xp2) / (rx2 * yp2 + ry2 * xp2);
        if rad < 0.0 {
            rad = 0.0;
        }
        let co = (if large == sweep { -1.0 } else { 1.0 }) * rad.sqrt();
        let cxp = (co * rx * y1p) / ry;
        let cyp = (-co * ry * x1p) / rx;
        let ccx = cos_p * cxp - sin_p * cyp + (x1 + x) / 2.0;
        let ccy = sin_p * cxp + cos_p * cyp + (y1 + y) / 2.0;
        let th1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
        let mut dth = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx) - th1;
        if sweep == 0 && dth > 0.0 {
            dth -= TAU;
        } else if sweep == 1 && dth < 0.0 {
            dth += TAU;
        }
        let slices = (dth.abs() / (std::f64::consts::PI / 2.0) - 1e-9)
            .ceil()
            .max(1.0) as usize;
        let delta = dth / slices as f64;
        let alpha = (4.0 / 3.0) * (delta / 4.0).tan();
        let ex = |t: f64| ccx + rx * t.cos() * cos_p - ry * t.sin() * sin_p;
        let ey = |t: f64| ccy + rx * t.cos() * sin_p + ry * t.sin() * cos_p;
        let dx = |t: f64| -rx * t.sin() * cos_p - ry * t.cos() * sin_p;
        let dy = |t: f64| -rx * t.sin() * sin_p + ry * t.cos() * cos_p;
        let mut t0 = th1;
        let mut p0x = x1;
        let mut p0y = y1;
        for s in 1..=slices {
            let t1 = th1 + delta * s as f64;
            let p1x = if s == slices { x } else { ex(t1) };
            let p1y = if s == slices { y } else { ey(t1) };
            self.cubic(
                p0x + alpha * dx(t0),
                p0y + alpha * dy(t0),
                p1x - alpha * dx(t1),
                p1y - alpha * dy(t1),
                p1x,
                p1y,
            );
            t0 = t1;
            p0x = p1x;
            p0y = p1y;
        }
    }

    fn finish(mut self, closed: bool) -> Option<CubicPath> {
        if closed {
            let x0 = self.pts[0];
            let y0 = self.pts[1];
            self.line(x0, y0);
        }
        if self.pts.len() < 8 {
            return None;
        }
        Some(CubicPath {
            pts: self.pts,
            closed,
        })
    }
}

fn lower_subpath(raw: &RawSubpath) -> Option<CubicPath> {
    let mut b = Builder::new(raw.x0, raw.y0);
    for s in &raw.segs {
        match *s {
            RawSeg::L(x, y) => b.line(x, y),
            RawSeg::C(x1, y1, x2, y2, x, y) => b.cubic(x1, y1, x2, y2, x, y),
            RawSeg::Q(x1, y1, x, y) => b.quad(x1, y1, x, y),
            RawSeg::A(rx, ry, rot, large, sweep, x, y) => b.arc(rx, ry, rot, large, sweep, x, y),
        }
    }
    b.finish(raw.closed)
}

fn attr_num(attrs: &HashMap<String, String>, key: &str, fallback: f64) -> f64 {
    attrs
        .get(key)
        .and_then(|v| v.parse().ok())
        .filter(|x: &f64| x.is_finite())
        .unwrap_or(fallback)
}

fn parse_points(v: &str) -> Result<Vec<f64>, MorphError> {
    let s = v.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let nums: Vec<f64> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| morph_err(format!("invalid points: \"{s}\"")))?;
    if nums.iter().any(|x| !x.is_finite()) {
        return Err(morph_err(format!("invalid points: \"{s}\"")));
    }
    Ok(nums)
}

fn poly_path(nums: &[f64], closed: bool) -> Option<CubicPath> {
    if nums.len() < 4 {
        return None;
    }
    let mut b = Builder::new(nums[0], nums[1]);
    let mut i = 2;
    while i + 1 < nums.len() {
        b.line(nums[i], nums[i + 1]);
        i += 2;
    }
    b.finish(closed)
}

fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Option<CubicPath> {
    if rx < 1e-12 || ry < 1e-12 {
        return None;
    }
    let kx = KAPPA * rx;
    let ky = KAPPA * ry;
    let e = cx + rx;
    let w = cx - rx;
    let s = cy + ry;
    let n = cy - ry;
    let mut b = Builder::new(e, cy);
    b.cubic(e, cy + ky, cx + kx, s, cx, s);
    b.cubic(cx - kx, s, w, cy + ky, w, cy);
    b.cubic(w, cy - ky, cx - kx, n, cx, n);
    b.cubic(cx + kx, n, e, cy - ky, e, cy);
    b.finish(true)
}

fn rect_path(attrs: &HashMap<String, String>) -> Option<CubicPath> {
    let x = attr_num(attrs, "x", 0.0);
    let y = attr_num(attrs, "y", 0.0);
    let w = attr_num(attrs, "width", 0.0);
    let h = attr_num(attrs, "height", 0.0);
    if w < 1e-12 || h < 1e-12 {
        return None;
    }
    let mut rx = attrs
        .get("rx")
        .and_then(|v| v.parse().ok())
        .unwrap_or(f64::NAN);
    let mut ry = attrs
        .get("ry")
        .and_then(|v| v.parse().ok())
        .unwrap_or(f64::NAN);
    if rx.is_nan() {
        rx = if ry.is_nan() { 0.0 } else { ry };
    }
    if ry.is_nan() {
        ry = rx;
    }
    rx = rx.max(0.0).min(w / 2.0);
    ry = ry.max(0.0).min(h / 2.0);
    if rx < 1e-12 || ry < 1e-12 {
        return poly_path(&[x, y, x + w, y, x + w, y + h, x, y + h], true);
    }
    let xa = x + rx;
    let xb = x + w - rx;
    let xr = x + w;
    let ya = y + ry;
    let yb = y + h - ry;
    let yd = y + h;
    let kx = KAPPA * rx;
    let ky = KAPPA * ry;
    let mut b = Builder::new(xa, y);
    b.line(xb, y);
    b.cubic(xb + kx, y, xr, ya - ky, xr, ya);
    b.line(xr, yb);
    b.cubic(xr, yb + ky, xb + kx, yd, xb, yd);
    b.line(xa, yd);
    b.cubic(xa - kx, yd, x, yb + ky, x, yb);
    b.line(x, ya);
    b.cubic(x, ya - ky, xa - kx, y, xa, y);
    b.finish(true)
}

/// Icon (IconNode or `d` string) → list of cubic subpaths.
pub fn icon_to_cubics(input: &IconInput) -> Result<Vec<CubicPath>, MorphError> {
    let mut out = Vec::new();
    match input {
        IconInput::D(d) => {
            for s in parse_path(d)? {
                if let Some(p) = lower_subpath(&s) {
                    out.push(p);
                }
            }
        }
        IconInput::Nodes(nodes) => {
            for (tag, attrs) in nodes {
                match tag.as_str() {
                    "path" => {
                        let d = attrs.get("d").map(|s| s.as_str()).unwrap_or("");
                        for s in parse_path(d)? {
                            if let Some(p) = lower_subpath(&s) {
                                out.push(p);
                            }
                        }
                    }
                    "line" => {
                        let mut b =
                            Builder::new(attr_num(attrs, "x1", 0.0), attr_num(attrs, "y1", 0.0));
                        b.line(attr_num(attrs, "x2", 0.0), attr_num(attrs, "y2", 0.0));
                        if let Some(p) = b.finish(false) {
                            out.push(p);
                        }
                    }
                    "circle" => {
                        let r = attr_num(attrs, "r", 0.0);
                        if let Some(p) = ellipse_path(
                            attr_num(attrs, "cx", 0.0),
                            attr_num(attrs, "cy", 0.0),
                            r,
                            r,
                        ) {
                            out.push(p);
                        }
                    }
                    "ellipse" => {
                        if let Some(p) = ellipse_path(
                            attr_num(attrs, "cx", 0.0),
                            attr_num(attrs, "cy", 0.0),
                            attr_num(attrs, "rx", 0.0),
                            attr_num(attrs, "ry", 0.0),
                        ) {
                            out.push(p);
                        }
                    }
                    "rect" => {
                        if let Some(p) = rect_path(attrs) {
                            out.push(p);
                        }
                    }
                    "polyline" => {
                        let pts =
                            parse_points(attrs.get("points").map(|s| s.as_str()).unwrap_or(""))?;
                        if let Some(p) = poly_path(&pts, false) {
                            out.push(p);
                        }
                    }
                    "polygon" => {
                        let pts =
                            parse_points(attrs.get("points").map(|s| s.as_str()).unwrap_or(""))?;
                        if let Some(p) = poly_path(&pts, true) {
                            out.push(p);
                        }
                    }
                    other => return Err(morph_err(format!("unsupported tag <{other}>"))),
                }
            }
        }
    }
    Ok(out)
}

fn parse_view_box(vb: &str) -> Result<[f64; 4], MorphError> {
    let nums: Vec<f64> = vb
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    if nums.len() == 1 {
        let s = nums[0];
        if s > 0.0 && s.is_finite() {
            return Ok([0.0, 0.0, s, s]);
        }
    }
    if nums.len() == 4
        && nums[2] > 0.0
        && nums[3] > 0.0
        && nums[0].is_finite()
        && nums[1].is_finite()
    {
        return Ok([nums[0], nums[1], nums[2], nums[3]]);
    }
    Err(morph_err(format!("invalid viewBox \"{vb}\"")))
}

/// Re-grid an icon drawn on `viewBox` onto the shared `grid` (24 by default).
pub fn fit_icon(input: &IconInput, view_box: &str, grid: f64) -> Result<String, MorphError> {
    let [min_x, min_y, w, h] = parse_view_box(view_box)?;
    let s = (grid / w).min(grid / h);
    let tx = (grid - w * s) / 2.0 - min_x * s;
    let ty = (grid - h * s) / 2.0 - min_y * s;
    let mut paths = icon_to_cubics(input)?;
    for p in &mut paths {
        for i in (0..p.pts.len()).step_by(2) {
            p.pts[i] = p.pts[i] * s + tx;
            p.pts[i + 1] = p.pts[i + 1] * s + ty;
        }
    }
    Ok(cubics_to_path_d(&paths))
}

/// Canonical cubic `d` for an icon (4-decimal snap).
pub fn canonical_d(input: &IconInput) -> Result<String, MorphError> {
    Ok(cubics_to_path_d(&icon_to_cubics(input)?))
}
