/// 2D 点（逻辑像素，叶局部坐标，y 向下）。
pub type Point2 = (f32, f32);

/// 路径绘制命令。曲线活到光栅 / tessellate 边界（[`flatten_path`]）。
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point2),
    LineTo(Point2),
    QuadTo {
        ctrl: Point2,
        to: Point2,
    },
    CubicTo {
        c1: Point2,
        c2: Point2,
        to: Point2,
    },
    /// SVG 椭圆弧（终点参数：`rx,ry x-rot large sweep x,y`）。
    Arc {
        radii: Point2,
        x_rot_deg: f32,
        large: bool,
        sweep: bool,
        to: Point2,
    },
    Close,
}

impl PathCommand {
    /// Pen after this command (`Close` has none).
    pub fn pen(&self) -> Option<Point2> {
        match *self {
            Self::MoveTo(p) | Self::LineTo(p) => Some(p),
            Self::QuadTo { to, .. } | Self::CubicTo { to, .. } | Self::Arc { to, .. } => Some(to),
            Self::Close => None,
        }
    }

    pub fn map_xy(self, mut f: impl FnMut(Point2) -> Point2) -> Self {
        match self {
            Self::MoveTo(p) => Self::MoveTo(f(p)),
            Self::LineTo(p) => Self::LineTo(f(p)),
            Self::QuadTo { ctrl, to } => Self::QuadTo {
                ctrl: f(ctrl),
                to: f(to),
            },
            Self::CubicTo { c1, c2, to } => Self::CubicTo {
                c1: f(c1),
                c2: f(c2),
                to: f(to),
            },
            Self::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => Self::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to: f(to),
            },
            Self::Close => Self::Close,
        }
    }
}

/// SVG 子集 `d`：保留 C/Q/A（不再先展平）。
pub fn encode_path_d(cmds: &[PathCommand]) -> String {
    let mut parts = Vec::with_capacity(cmds.len());
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo((x, y)) => parts.push(format!("M{x:.3},{y:.3}")),
            PathCommand::LineTo((x, y)) => parts.push(format!("L{x:.3},{y:.3}")),
            PathCommand::QuadTo {
                ctrl: (cx, cy),
                to: (x, y),
            } => parts.push(format!("Q{cx:.3},{cy:.3} {x:.3},{y:.3}")),
            PathCommand::CubicTo {
                c1: (x1, y1),
                c2: (x2, y2),
                to: (x, y),
            } => parts.push(format!("C{x1:.3},{y1:.3} {x2:.3},{y2:.3} {x:.3},{y:.3}")),
            PathCommand::Arc {
                radii: (rx, ry),
                x_rot_deg,
                large,
                sweep,
                to: (x, y),
            } => parts.push(format!(
                "A{rx:.3},{ry:.3} {x_rot_deg:.3} {} {} {x:.3},{y:.3}",
                if large { 1 } else { 0 },
                if sweep { 1 } else { 0 },
            )),
            PathCommand::Close => parts.push("Z".into()),
        }
    }
    parts.join(" ")
}

/// 解析 HTML/SVG path `d`（绝对/相对 M/L/H/V/C/S/Q/T/A/Z）。曲线保留为 [`PathCommand`] 变体。
pub fn decode_path_d(d: &str) -> Vec<PathCommand> {
    let parser = PathDParser::new(d);
    parser.parse()
}

/// Flatten curves to `MoveTo` / `LineTo` / `Close` for raster, tessellate, and hit-testing.
pub fn flatten_path(cmds: &[PathCommand]) -> Vec<PathCommand> {
    let mut out = Vec::with_capacity(cmds.len());
    let mut cur = (0.0f32, 0.0f32);
    let mut subpath_start = cur;
    for cmd in cmds {
        match *cmd {
            PathCommand::MoveTo(p) => {
                out.push(PathCommand::MoveTo(p));
                cur = p;
                subpath_start = p;
            }
            PathCommand::LineTo(p) => {
                out.push(PathCommand::LineTo(p));
                cur = p;
            }
            PathCommand::QuadTo { ctrl, to } => {
                flatten_quad(cur, ctrl, to, &mut out);
                cur = to;
            }
            PathCommand::CubicTo { c1, c2, to } => {
                flatten_cubic(cur, c1, c2, to, &mut out);
                cur = to;
            }
            PathCommand::Arc {
                radii,
                x_rot_deg,
                large,
                sweep,
                to,
            } => {
                flatten_arc(
                    cur,
                    radii.0.abs(),
                    radii.1.abs(),
                    x_rot_deg,
                    large,
                    sweep,
                    to,
                    &mut out,
                );
                cur = to;
            }
            PathCommand::Close => {
                out.push(PathCommand::Close);
                cur = subpath_start;
            }
        }
    }
    out
}

struct PathDParser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> PathDParser<'a> {
    fn new(d: &'a str) -> Self {
        Self {
            chars: d.chars().peekable(),
        }
    }

    fn skip_sep(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || c == ',' {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn peek_cmd(&mut self) -> Option<char> {
        self.skip_sep();
        self.chars
            .peek()
            .copied()
            .filter(|c| c.is_ascii_alphabetic())
    }

    fn next_number(&mut self) -> Option<f32> {
        self.skip_sep();
        let mut s = String::new();
        if let Some(&c) = self.chars.peek() {
            if c == '-' || c == '+' {
                s.push(c);
                self.chars.next();
            }
        }
        let mut saw_digit = false;
        let mut saw_dot = false;
        let mut saw_exp = false;
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() {
                saw_digit = true;
                s.push(c);
                self.chars.next();
            } else if c == '.' && !saw_dot && !saw_exp {
                saw_dot = true;
                s.push(c);
                self.chars.next();
            } else if (c == 'e' || c == 'E') && saw_digit && !saw_exp {
                saw_exp = true;
                s.push(c);
                self.chars.next();
                if let Some(&s2) = self.chars.peek() {
                    if s2 == '+' || s2 == '-' {
                        s.push(s2);
                        self.chars.next();
                    }
                }
            } else {
                break;
            }
        }
        if !saw_digit {
            return None;
        }
        s.parse().ok()
    }

    fn parse(mut self) -> Vec<PathCommand> {
        let mut out = Vec::new();
        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        let mut sub_x = 0.0f32;
        let mut sub_y = 0.0f32;
        let mut last_c2: Option<(f32, f32)> = None; // last cubic control (absolute)
        let mut last_q: Option<(f32, f32)> = None; // last quad control
        let mut cur_cmd: Option<char> = None;

        while self.chars.peek().is_some() {
            if let Some(c) = self.peek_cmd() {
                self.chars.next();
                cur_cmd = Some(c);
            }
            let Some(cmd) = cur_cmd else {
                break;
            };
            let abs = cmd.is_ascii_uppercase();
            let op = cmd.to_ascii_uppercase();

            match op {
                'Z' => {
                    out.push(PathCommand::Close);
                    cx = sub_x;
                    cy = sub_y;
                    last_c2 = None;
                    last_q = None;
                    cur_cmd = None;
                }
                'M' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::MoveTo((nx, ny)));
                    cx = nx;
                    cy = ny;
                    sub_x = nx;
                    sub_y = ny;
                    last_c2 = None;
                    last_q = None;
                    // Implicit LineTo for subsequent coordinate pairs.
                    cur_cmd = Some(if abs { 'L' } else { 'l' });
                    loop {
                        self.skip_sep();
                        if self
                            .chars
                            .peek()
                            .copied()
                            .is_some_and(|c| c.is_ascii_alphabetic())
                        {
                            break;
                        }
                        let Some(x) = self.next_number() else {
                            break;
                        };
                        let Some(y) = self.next_number() else {
                            break;
                        };
                        let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                        out.push(PathCommand::LineTo((nx, ny)));
                        cx = nx;
                        cy = ny;
                    }
                }
                'L' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (nx, ny) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::LineTo((nx, ny)));
                    cx = nx;
                    cy = ny;
                    last_c2 = None;
                    last_q = None;
                }
                'H' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let nx = if abs { x } else { cx + x };
                    out.push(PathCommand::LineTo((nx, cy)));
                    cx = nx;
                    last_c2 = None;
                    last_q = None;
                }
                'V' => {
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let ny = if abs { y } else { cy + y };
                    out.push(PathCommand::LineTo((cx, ny)));
                    cy = ny;
                    last_c2 = None;
                    last_q = None;
                }
                'C' => {
                    let Some(x1) = self.next_number() else {
                        break;
                    };
                    let Some(y1) = self.next_number() else {
                        break;
                    };
                    let Some(x2) = self.next_number() else {
                        break;
                    };
                    let Some(y2) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x1, y1, x2, y2, x, y) = if abs {
                        (x1, y1, x2, y2, x, y)
                    } else {
                        (cx + x1, cy + y1, cx + x2, cy + y2, cx + x, cy + y)
                    };
                    out.push(PathCommand::CubicTo {
                        c1: (x1, y1),
                        c2: (x2, y2),
                        to: (x, y),
                    });
                    last_c2 = Some((x2, y2));
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                'S' => {
                    let Some(x2) = self.next_number() else {
                        break;
                    };
                    let Some(y2) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x2, y2, x, y) = if abs {
                        (x2, y2, x, y)
                    } else {
                        (cx + x2, cy + y2, cx + x, cy + y)
                    };
                    let (x1, y1) = match last_c2 {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    out.push(PathCommand::CubicTo {
                        c1: (x1, y1),
                        c2: (x2, y2),
                        to: (x, y),
                    });
                    last_c2 = Some((x2, y2));
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                'Q' => {
                    let Some(x1) = self.next_number() else {
                        break;
                    };
                    let Some(y1) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x1, y1, x, y) = if abs {
                        (x1, y1, x, y)
                    } else {
                        (cx + x1, cy + y1, cx + x, cy + y)
                    };
                    out.push(PathCommand::QuadTo {
                        ctrl: (x1, y1),
                        to: (x, y),
                    });
                    last_q = Some((x1, y1));
                    last_c2 = None;
                    cx = x;
                    cy = y;
                }
                'T' => {
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x, y) = if abs { (x, y) } else { (cx + x, cy + y) };
                    let (x1, y1) = match last_q {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    out.push(PathCommand::QuadTo {
                        ctrl: (x1, y1),
                        to: (x, y),
                    });
                    last_q = Some((x1, y1));
                    last_c2 = None;
                    cx = x;
                    cy = y;
                }
                'A' => {
                    let Some(rx) = self.next_number() else {
                        break;
                    };
                    let Some(ry) = self.next_number() else {
                        break;
                    };
                    let Some(x_axis_rot) = self.next_number() else {
                        break;
                    };
                    let Some(large_arc) = self.next_number() else {
                        break;
                    };
                    let Some(sweep) = self.next_number() else {
                        break;
                    };
                    let Some(x) = self.next_number() else {
                        break;
                    };
                    let Some(y) = self.next_number() else {
                        break;
                    };
                    let (x, y) = if abs { (x, y) } else { (cx + x, cy + y) };
                    out.push(PathCommand::Arc {
                        radii: (rx.abs(), ry.abs()),
                        x_rot_deg: x_axis_rot,
                        large: large_arc != 0.0,
                        sweep: sweep != 0.0,
                        to: (x, y),
                    });
                    last_c2 = None;
                    last_q = None;
                    cx = x;
                    cy = y;
                }
                _ => {
                    // Unknown: skip one number if any to avoid infinite loop
                    let _ = self.next_number();
                    cur_cmd = None;
                }
            }
        }
        out
    }
}

fn mid_pt(a: Point2, b: Point2) -> Point2 {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Maximum chord error in raster pixels (coordinates already include display DPI).
pub const FLATTEN_FLATNESS: f32 = 0.125;
const FLATTEN_MAX_DEPTH: u8 = 10;

fn flatten_cubic(p0: Point2, p1: Point2, p2: Point2, p3: Point2, out: &mut Vec<PathCommand>) {
    flatten_cubic_rec(p0, p1, p2, p3, 0, out);
}

fn flatten_cubic_rec(
    p0: Point2,
    p1: Point2,
    p2: Point2,
    p3: Point2,
    depth: u8,
    out: &mut Vec<PathCommand>,
) {
    let err = dist_to_segment(p1, p0, p3).max(dist_to_segment(p2, p0, p3));
    if depth >= FLATTEN_MAX_DEPTH || err <= FLATTEN_FLATNESS {
        out.push(PathCommand::LineTo(p3));
        return;
    }
    let m01 = mid_pt(p0, p1);
    let m12 = mid_pt(p1, p2);
    let m23 = mid_pt(p2, p3);
    let m012 = mid_pt(m01, m12);
    let m123 = mid_pt(m12, m23);
    let m = mid_pt(m012, m123);
    flatten_cubic_rec(p0, m01, m012, m, depth.saturating_add(1), out);
    flatten_cubic_rec(m, m123, m23, p3, depth.saturating_add(1), out);
}

fn flatten_quad(p0: Point2, p1: Point2, p2: Point2, out: &mut Vec<PathCommand>) {
    flatten_quad_rec(p0, p1, p2, 0, out);
}

fn flatten_quad_rec(p0: Point2, p1: Point2, p2: Point2, depth: u8, out: &mut Vec<PathCommand>) {
    if depth >= FLATTEN_MAX_DEPTH || dist_to_segment(p1, p0, p2) <= FLATTEN_FLATNESS {
        out.push(PathCommand::LineTo(p2));
        return;
    }
    let m01 = mid_pt(p0, p1);
    let m12 = mid_pt(p1, p2);
    let m = mid_pt(m01, m12);
    flatten_quad_rec(p0, m01, m, depth.saturating_add(1), out);
    flatten_quad_rec(m, m12, p2, depth.saturating_add(1), out);
}

/// SVG elliptical arc → polyline (W3C endpoint-to-center parameterization).
#[expect(
    clippy::too_many_arguments,
    reason = "SVG endpoint-to-center conversion takes the endpoint arc parameters and output buffer"
)]
fn flatten_arc(
    p0: Point2,
    mut rx: f32,
    mut ry: f32,
    x_axis_rot_deg: f32,
    large_arc: bool,
    sweep: bool,
    p1: Point2,
    out: &mut Vec<PathCommand>,
) {
    if (p0.0 - p1.0).abs() < 1e-6 && (p0.1 - p1.1).abs() < 1e-6 {
        return;
    }
    if rx < 1e-6 || ry < 1e-6 {
        out.push(PathCommand::LineTo(p1));
        return;
    }
    let phi = x_axis_rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (p0.0 - p1.0) * 0.5;
    let dy = (p0.1 - p1.1) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let mut rx_sq = rx * rx;
    let mut ry_sq = ry * ry;
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;
    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
        rx_sq = rx * rx;
        ry_sq = ry * ry;
    }
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let mut sq = ((rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq)
        / (rx_sq * y1p_sq + ry_sq * x1p_sq))
        .max(0.0)
        .sqrt();
    sq *= sign;
    let cxp = sq * (rx * y1p) / ry;
    let cyp = sq * -(ry * x1p) / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (p0.0 + p1.0) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (p0.1 + p1.1) * 0.5;

    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let n = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if n < 1e-12 {
            return 0.0;
        }
        let mut a = ((ux * vx + uy * vy) / n).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }

    let segments = arc_segments(rx.max(ry), dtheta);
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let theta = theta1 + dtheta * t;
        let (st, ct) = theta.sin_cos();
        let x = cos_phi * rx * ct - sin_phi * ry * st + cx;
        let y = sin_phi * rx * ct + cos_phi * ry * st + cy;
        out.push(PathCommand::LineTo((x, y)));
    }
    // Snap endpoint (numerical drift on long arcs).
    if let Some(PathCommand::LineTo(p)) = out.last_mut() {
        *p = p1;
    }
}

pub fn dist_to_segment(p: Point2, a: Point2, b: Point2) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return (p.0 - a.0).hypot(p.1 - a.1);
    }
    let t = ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

pub fn arc_segments(radius: f32, angle: f32) -> usize {
    let step = (2.0 * (1.0 - FLATTEN_FLATNESS / radius.max(0.5)).acos())
        .clamp(0.001, std::f32::consts::PI / 6.0);
    (angle.abs() / step).ceil().clamp(1.0, 4096.0) as usize
}

#[cfg(test)]
#[path = "tests/path.rs"]
mod tests;
