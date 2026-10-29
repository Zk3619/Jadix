//! CSS gradients — aligned with MDN
//! [Using CSS gradients](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Images/Using_gradients).
//!
//! Supports `linear-gradient` / `radial-gradient` / `conic-gradient` and their
//! `repeating-*` variants: multiple color stops, dual-position bands, color hints,
//! angle / `to <side>` directions, radial shape/size/`at`, and conic `from` / `at`.
//!
//! Angle convention for **builder / Tailwind** APIs (`angle_deg`, `bg-gradient-to-*`):
//! `0` = left→right, `90` = top→bottom (Tailwind-compatible).
//! CSS `linear-gradient(<angle>, …)` uses the CSS Images angle (`0deg` = bottom→top);
//! [`parse_gradient`] converts to the internal convention (`internal = css - 90`).

use super::color_space::{encode_linear_srgb_to_display_srgb, linearize_display_srgb};

/// CSS `<named-color>` + project tokens / hex / `rgb()` via [`try_parse_color`].
fn parse_gradient_color(s: &str) -> Option<[f32; 4]> {
    super::color::try_parse_color(s.trim())
}

/// One color stop. Positions are fractions of the gradient line / arc (`0`..=`1`),
/// CSS angles/360 for conic, or absolute CSS `px` when [`Self::position_px`] is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub color: [f32; 4],
    /// Start position; `None` = auto-spaced when resolving.
    pub position: Option<f32>,
    /// Optional second position (CSS Images Level 4 color band).
    pub position_end: Option<f32>,
    /// When true, `position` is CSS px along the gradient line (not 0..=1).
    pub position_px: bool,
    /// When true, `position_end` is CSS px.
    pub position_end_px: bool,
}

impl ColorStop {
    pub fn new(color: [f32; 4]) -> Self {
        Self {
            color,
            position: None,
            position_end: None,
            position_px: false,
            position_end_px: false,
        }
    }

    pub fn at(color: [f32; 4], position: f32) -> Self {
        Self {
            color,
            position: Some(position),
            position_end: None,
            position_px: false,
            position_end_px: false,
        }
    }

    pub fn band(color: [f32; 4], start: f32, end: f32) -> Self {
        Self {
            color,
            position: Some(start),
            position_end: Some(end),
            position_px: false,
            position_end_px: false,
        }
    }
}

/// Interpolation hint between two adjacent color stops (CSS color hint).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorHint {
    /// Position of the midpoint color between the previous and next stop (`0`..=`1`).
    pub position: f32,
}

/// Linear gradient (multi-stop).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    /// Internal angle: `0` = left→right, `90` = top→bottom.
    pub angle_deg: f32,
    pub stops: Vec<ColorStop>,
    /// Color hints interleaved with stops (index `i` sits between resolved stop `i` and `i+1`).
    pub hints: Vec<Option<ColorHint>>,
    pub repeating: bool,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            angle_deg: 90.0,
            stops: Vec::new(),
            hints: Vec::new(),
            repeating: false,
        }
    }
}

impl LinearGradient {
    pub fn two_stop(from: [f32; 4], to: [f32; 4], angle_deg: f32) -> Self {
        Self {
            angle_deg,
            stops: vec![ColorStop::at(from, 0.0), ColorStop::at(to, 1.0)],
            hints: Vec::new(),
            repeating: false,
        }
    }

    fn ensure_two_stops(&mut self) {
        if self.stops.is_empty() {
            self.stops.push(ColorStop::at([0.0, 0.0, 0.0, 0.0], 0.0));
            self.stops.push(ColorStop::at([0.0, 0.0, 0.0, 0.0], 1.0));
        } else if self.stops.len() == 1 {
            let c = self.stops[0].color;
            self.stops[0].position = Some(0.0);
            self.stops.push(ColorStop::at(c, 1.0));
        }
    }

    /// Tailwind `from-*` / legacy first stop color.
    pub fn set_from(&mut self, color: [f32; 4]) {
        self.ensure_two_stops();
        self.stops[0].color = color;
        if self.stops[0].position.is_none() {
            self.stops[0].position = Some(0.0);
        }
    }

    /// Tailwind `to-*` / legacy last stop color.
    pub fn set_to(&mut self, color: [f32; 4]) {
        self.ensure_two_stops();
        let last = self.stops.len() - 1;
        self.stops[last].color = color;
        if self.stops[last].position.is_none() {
            self.stops[last].position = Some(1.0);
        }
    }

    /// Tailwind `via-*` middle stop (inserts / updates a stop at ~50%).
    pub fn set_via(&mut self, color: [f32; 4]) {
        self.ensure_two_stops();
        if self.stops.len() == 2 {
            self.stops.insert(1, ColorStop::at(color, 0.5));
        } else {
            // Prefer an existing mid stop; otherwise insert before last.
            let mid = self.stops.len() / 2;
            self.stops[mid].color = color;
            if self.stops[mid].position.is_none() {
                self.stops[mid].position = Some(0.5);
            }
        }
    }

    pub fn from_color(&self) -> [f32; 4] {
        self.stops
            .first()
            .map(|s| s.color)
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }

    pub fn to_color(&self) -> [f32; 4] {
        self.stops
            .last()
            .map(|s| s.color)
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }
}

/// Radial size keyword (CSS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RadialSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    #[default]
    FarthestCorner,
}

/// Explicit radial radii (ellipse: two; circle: one stored in `x`, `y` ignored / equal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadialExtent {
    Keyword(RadialSize),
    /// Horizontal / vertical radius as fraction of box width / height (`0`..=`1+`).
    Ellipse {
        rx: f32,
        ry: f32,
    },
    /// Circle radius as fraction of the smaller box side.
    Circle(f32),
}

impl Default for RadialExtent {
    fn default() -> Self {
        Self::Keyword(RadialSize::FarthestCorner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RadialShape {
    #[default]
    Ellipse,
    Circle,
}

/// Center as fractions of the box (`0`..=`1`). Default `50% 50%`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPosition {
    pub x: f32,
    pub y: f32,
}

impl Default for GradientPosition {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub shape: RadialShape,
    pub extent: RadialExtent,
    pub position: GradientPosition,
    pub stops: Vec<ColorStop>,
    pub hints: Vec<Option<ColorHint>>,
    pub repeating: bool,
}

impl Default for RadialGradient {
    fn default() -> Self {
        Self {
            shape: RadialShape::Ellipse,
            extent: RadialExtent::default(),
            position: GradientPosition::default(),
            stops: Vec::new(),
            hints: Vec::new(),
            repeating: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConicGradient {
    /// Start angle in **internal** degrees (`0` = right / 3 o'clock for sampling math;
    /// CSS `from 0deg` = top; converted at parse time).
    pub from_deg: f32,
    pub position: GradientPosition,
    pub stops: Vec<ColorStop>,
    pub hints: Vec<Option<ColorHint>>,
    pub repeating: bool,
}

impl Default for ConicGradient {
    fn default() -> Self {
        Self {
            // CSS default: from top, going clockwise → internal 270° if 0=right.
            // We store CSS-from converted: CSS 0deg (top) → internal -90 / 270.
            from_deg: -90.0,
            position: GradientPosition::default(),
            stops: Vec::new(),
            hints: Vec::new(),
            repeating: false,
        }
    }
}

/// Background gradient value.
#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear(LinearGradient),
    Radial(RadialGradient),
    Conic(ConicGradient),
}

impl Default for Gradient {
    fn default() -> Self {
        Self::Linear(LinearGradient::default())
    }
}

impl Gradient {
    pub fn as_linear_mut(&mut self) -> &mut LinearGradient {
        match self {
            Self::Linear(g) => g,
            _ => {
                *self = Self::Linear(LinearGradient::default());
                match self {
                    Self::Linear(g) => g,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn stops(&self) -> &[ColorStop] {
        match self {
            Self::Linear(g) => &g.stops,
            Self::Radial(g) => &g.stops,
            Self::Conic(g) => &g.stops,
        }
    }
}

// ─── Resolve + sample ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct ResolvedStop {
    color: [f32; 4],
    linear_rgb: [f32; 3],
    pos: f32,
}

fn to_frac(value: f32, is_px: bool, ref_len_px: f32) -> f32 {
    if is_px {
        value / ref_len_px.max(1e-6)
    } else {
        value
    }
}

fn expand_and_resolve_stops(
    stops: &[ColorStop],
    hints: &[Option<ColorHint>],
    ref_len_px: f32,
) -> Vec<ResolvedStop> {
    if stops.is_empty() {
        return Vec::new();
    }

    // Expand dual-position stops into consecutive same-color stops.
    let mut raw: Vec<([f32; 4], Option<f32>)> = Vec::new();
    for s in stops {
        raw.push((
            s.color,
            s.position.map(|v| to_frac(v, s.position_px, ref_len_px)),
        ));
        if let Some(end) = s.position_end {
            raw.push((s.color, Some(to_frac(end, s.position_end_px, ref_len_px))));
        }
    }

    let n = raw.len();
    let mut positions: Vec<Option<f32>> = raw.iter().map(|(_, p)| *p).collect();

    // First / last defaults.
    if positions[0].is_none() {
        positions[0] = Some(0.0);
    }
    if positions[n - 1].is_none() {
        positions[n - 1] = Some(1.0);
    }

    // Fill unspecified runs by linear interpolation between defined neighbors.
    let mut i = 0;
    while i < n {
        if positions[i].is_some() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && positions[i].is_none() {
            i += 1;
        }
        let left = positions[start - 1].unwrap_or(0.0);
        let right = if i < n {
            positions[i].unwrap_or(1.0)
        } else {
            1.0
        };
        let gaps = (i - start + 1) as f32;
        for (k, slot) in positions.iter_mut().enumerate().take(i).skip(start) {
            let t = (k - start + 1) as f32 / gaps;
            *slot = Some(left + (right - left) * t);
        }
    }

    // Enforce non-decreasing.
    let mut last = 0.0_f32;
    for p in &mut positions {
        let v = p.unwrap_or(last).max(last);
        *p = Some(v);
        last = v;
    }

    let resolved: Vec<ResolvedStop> = raw
        .into_iter()
        .zip(positions)
        .map(|((color, _), pos)| ResolvedStop {
            color,
            linear_rgb: linearize_display_srgb([color[0], color[1], color[2]]),
            pos: pos.unwrap_or(0.0),
        })
        .collect();

    let _ = hints;
    resolved
}

fn sample_resolved(
    stops: &[ResolvedStop],
    hints: &[Option<ColorHint>],
    t: f32,
    repeating: bool,
) -> [f32; 4] {
    sample_resolved_with_encoder(
        stops,
        hints,
        t,
        repeating,
        encode_linear_srgb_to_display_srgb,
    )
}

fn sample_resolved_with_encoder(
    stops: &[ResolvedStop],
    hints: &[Option<ColorHint>],
    mut t: f32,
    repeating: bool,
    encode: impl Fn([f32; 3]) -> [f32; 3],
) -> [f32; 4] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    if stops.len() == 1 {
        return stops[0].color;
    }

    let first = stops[0].pos;
    let last = stops[stops.len() - 1].pos;
    let span = (last - first).abs().max(1e-6);

    if repeating {
        // Map into [first, last) then wrap.
        let mut u = (t - first) / span;
        u -= u.floor();
        t = first + u * span;
    } else {
        t = t.clamp(first.min(last), first.max(last));
    }

    if t <= stops[0].pos {
        return stops[0].color;
    }
    if t >= stops[stops.len() - 1].pos {
        return stops[stops.len() - 1].color;
    }

    for i in 0..stops.len() - 1 {
        let a = stops[i];
        let b = stops[i + 1];
        if t < a.pos || t > b.pos {
            continue;
        }
        if (b.pos - a.pos).abs() < 1e-6 {
            return b.color;
        }
        let mut u = (t - a.pos) / (b.pos - a.pos);
        // Color hint: remap so the midpoint color lands at hint.position.
        if let Some(Some(hint)) = hints.get(i) {
            let h = ((hint.position - a.pos) / (b.pos - a.pos)).clamp(1e-4, 1.0 - 1e-4);
            // Piecewise linear through (0,0), (h, 0.5), (1,1).
            u = if u <= h {
                0.5 * (u / h)
            } else {
                0.5 + 0.5 * ((u - h) / (1.0 - h))
            };
        }
        let u = u.clamp(0.0, 1.0);
        let rgb = encode(std::array::from_fn(|i| {
            a.linear_rgb[i] * (1.0 - u) + b.linear_rgb[i] * u
        }));
        return [
            rgb[0],
            rgb[1],
            rgb[2],
            a.color[3] * (1.0 - u) + b.color[3] * u,
        ];
    }
    stops[stops.len() - 1].color
}

fn sample_stops(
    stops: &[ColorStop],
    hints: &[Option<ColorHint>],
    t: f32,
    repeating: bool,
    ref_len_px: f32,
) -> [f32; 4] {
    let resolved = expand_and_resolve_stops(stops, hints, ref_len_px);
    sample_resolved(&resolved, hints, t, repeating)
}

/// Sample gradient color at normalized box coords (`x`,`y` in `0`..=`1`).
/// Uses a 100×100px reference box (legacy / unit tests). Prefer
/// [`sample_gradient_in_box`] when the paint size is known.
pub fn sample_gradient(grad: &Gradient, x: f32, y: f32) -> [f32; 4] {
    sample_gradient_in_box(grad, x, y, 100.0, 100.0)
}

/// Sample with explicit box size in CSS px (for correct `Npx` color-stop resolution).
pub fn sample_gradient_in_box(grad: &Gradient, x: f32, y: f32, w_px: f32, h_px: f32) -> [f32; 4] {
    let w = w_px.max(1e-6);
    let h = h_px.max(1e-6);
    match grad {
        Gradient::Linear(g) => {
            let rad = g.angle_deg.to_radians();
            let dx = rad.cos();
            let dy = rad.sin();
            let px = x - 0.5;
            let py = y - 0.5;
            let proj = px * dx + py * dy;
            let half = 0.5 * (dx.abs() + dy.abs()).max(1e-6);
            let t = (proj / half + 1.0) * 0.5;
            // Projection span of the unit square, mapped to CSS px along the gradient line.
            let line_len = (dx.abs() * w + dy.abs() * h).max(1.0);
            sample_stops(&g.stops, &g.hints, t, g.repeating, line_len)
        }
        Gradient::Radial(g) => {
            let cx = g.position.x;
            let cy = g.position.y;
            let (rx, ry) = resolve_radial_radii(g, 1.0, 1.0);
            let nx = if rx > 1e-6 { (x - cx) / rx } else { 0.0 };
            let ny = if ry > 1e-6 { (y - cy) / ry } else { 0.0 };
            let t = (nx * nx + ny * ny).sqrt();
            // Ending-shape size in CSS px (circle → isotropic; ellipse → geometric mean).
            let (rx_px, ry_px) = resolve_radial_radii(g, w, h);
            let ray_len = (rx_px * ry_px).sqrt().max(1.0);
            sample_stops(&g.stops, &g.hints, t, g.repeating, ray_len)
        }
        Gradient::Conic(g) => {
            let cx = g.position.x;
            let cy = g.position.y;
            let ang = (y - cy).atan2(x - cx).to_degrees();
            let mut deg = ang - g.from_deg;
            deg = ((deg % 360.0) + 360.0) % 360.0;
            let t = deg / 360.0;
            // Conic stops are turn fractions; px lengths are rare — use min side as ref.
            let ref_len = w.min(h).max(1.0);
            sample_stops(&g.stops, &g.hints, t, g.repeating, ref_len)
        }
    }
}

/// Exact RGBA8 quantization boundaries for this platform's sRGB conversion.
/// Find the first positive f32 producing each byte; no interpolation or gamma
/// approximation is involved. Initialized only when a gradient needs it.
fn linear_srgb_byte_thresholds() -> &'static [f32; 255] {
    static THRESHOLDS: std::sync::OnceLock<[f32; 255]> = std::sync::OnceLock::new();
    THRESHOLDS.get_or_init(|| {
        std::array::from_fn(|byte| {
            let mut low = 0_u32;
            let mut high = 1.0_f32.to_bits();
            while low < high {
                let mid = low + (high - low) / 2;
                let encoded = super::color_space::encode_linear_srgb_component(f32::from_bits(mid));
                let quantized = encoded.clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
                if quantized > byte as u8 {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            f32::from_bits(low)
        })
    })
}

#[inline]
fn quantized_linear_srgb_component(linear: f32, thresholds: &[f32; 255]) -> f32 {
    thresholds.partition_point(|threshold| linear >= *threshold) as u32 as f32 / 255.0
}

/// Resolve geometry and stops once per texture, not once per pixel/subsample.
fn prepared_gradient_sampler(
    grad: &Gradient,
    w: f32,
    h: f32,
) -> impl Fn(f32, f32) -> [f32; 4] + '_ {
    let (stops, hints, repeating, ref_len) = match grad {
        Gradient::Linear(g) => {
            let rad = g.angle_deg.to_radians();
            (
                &g.stops,
                &g.hints,
                g.repeating,
                (rad.cos().abs() * w + rad.sin().abs() * h).max(1.0),
            )
        }
        Gradient::Radial(g) => {
            let (rx, ry) = resolve_radial_radii(g, w, h);
            (&g.stops, &g.hints, g.repeating, (rx * ry).sqrt().max(1.0))
        }
        Gradient::Conic(g) => (&g.stops, &g.hints, g.repeating, w.min(h).max(1.0)),
    };
    let stops = expand_and_resolve_stops(stops, hints, ref_len);
    // Quantize only a single sample. SSAA must average unquantized colors first.
    let byte_thresholds = (!gradient_needs_ssaa(grad)).then(linear_srgb_byte_thresholds);
    let (dx, dy, half) = if let Gradient::Linear(g) = grad {
        let rad = g.angle_deg.to_radians();
        let (dx, dy) = (rad.cos(), rad.sin());
        (dx, dy, 0.5 * (dx.abs() + dy.abs()).max(1e-6))
    } else {
        (0.0, 0.0, 1.0)
    };
    let (rx, ry) = if let Gradient::Radial(g) = grad {
        resolve_radial_radii(g, 1.0, 1.0)
    } else {
        (1.0, 1.0)
    };
    move |x, y| {
        let t = match grad {
            Gradient::Linear(_) => (((x - 0.5) * dx + (y - 0.5) * dy) / half + 1.0) * 0.5,
            Gradient::Radial(g) => {
                let nx = if rx > 1e-6 {
                    (x - g.position.x) / rx
                } else {
                    0.0
                };
                let ny = if ry > 1e-6 {
                    (y - g.position.y) / ry
                } else {
                    0.0
                };
                (nx * nx + ny * ny).sqrt()
            }
            Gradient::Conic(g) => {
                let deg = (y - g.position.y).atan2(x - g.position.x).to_degrees() - g.from_deg;
                ((deg % 360.0) + 360.0) % 360.0 / 360.0
            }
        };
        sample_resolved_with_encoder(&stops, hints, t, repeating, |linear| {
            if let Some(thresholds) = byte_thresholds {
                linear.map(|channel| quantized_linear_srgb_component(channel, thresholds))
            } else {
                encode_linear_srgb_to_display_srgb(linear)
            }
        })
    }
}

fn resolve_radial_radii(g: &RadialGradient, w: f32, h: f32) -> (f32, f32) {
    let cx = g.position.x * w;
    let cy = g.position.y * h;
    match g.extent {
        RadialExtent::Ellipse { rx, ry } => (rx * w, ry * h),
        RadialExtent::Circle(r) => {
            let s = w.min(h);
            (r * s, r * s)
        }
        RadialExtent::Keyword(kw) => {
            let dist_side = |to_x: f32, to_y: f32| ((cx - to_x).abs(), (cy - to_y).abs());
            let (dl, dt) = dist_side(0.0, 0.0);
            let (dr, _) = dist_side(w, 0.0);
            let (_, db) = dist_side(0.0, h);
            let sides = [dl, dr, dt, db];
            let corners = [
                (cx.hypot(cy)),
                ((w - cx).hypot(cy)),
                (cx.hypot(h - cy)),
                ((w - cx).hypot(h - cy)),
            ];
            match (g.shape, kw) {
                (RadialShape::Circle, RadialSize::ClosestSide) => {
                    let r = sides.into_iter().fold(f32::INFINITY, f32::min);
                    (r, r)
                }
                (RadialShape::Circle, RadialSize::FarthestSide) => {
                    let r = sides.into_iter().fold(0.0_f32, f32::max);
                    (r, r)
                }
                (RadialShape::Circle, RadialSize::ClosestCorner) => {
                    let r = corners.into_iter().fold(f32::INFINITY, f32::min);
                    (r, r)
                }
                (RadialShape::Circle, RadialSize::FarthestCorner) => {
                    let r = corners.into_iter().fold(0.0_f32, f32::max);
                    (r, r)
                }
                (RadialShape::Ellipse, RadialSize::ClosestSide) => {
                    (sides[0].min(sides[1]), sides[2].min(sides[3]))
                }
                (RadialShape::Ellipse, RadialSize::FarthestSide) => {
                    (sides[0].max(sides[1]), sides[2].max(sides[3]))
                }
                (RadialShape::Ellipse, RadialSize::ClosestCorner) => {
                    // Approximate: use closest side axes scaled to reach closest corner.
                    let rx = sides[0].min(sides[1]).max(1e-6);
                    let ry = sides[2].min(sides[3]).max(1e-6);
                    let t = corners
                        .into_iter()
                        .map(|c| {
                            // Not exact; fall back to isotropic closest corner.
                            c
                        })
                        .fold(f32::INFINITY, f32::min);
                    let _ = (rx, ry);
                    (t, t)
                }
                (RadialShape::Ellipse, RadialSize::FarthestCorner) => {
                    // Distance to farthest corner in ellipse-normalized space with aspect.
                    let rx = (cx.max(w - cx)).max(1e-6);
                    let ry = (cy.max(h - cy)).max(1e-6);
                    (rx, ry)
                }
            }
        }
    }
}

/// Rasterize gradient into RGBA8 (`width * height * 4`).
pub fn rasterize_gradient(grad: &Gradient, width: u32, height: u32) -> Vec<u8> {
    rasterize_gradient_with_radii(grad, width, height, [0.0; 4])
}

fn gradient_needs_ssaa(grad: &Gradient) -> bool {
    match grad {
        Gradient::Linear(g) => g.repeating || has_hard_edge_stops(&g.stops),
        Gradient::Radial(g) => g.repeating || has_hard_edge_stops(&g.stops),
        Gradient::Conic(g) => g.repeating || has_hard_edge_stops(&g.stops),
    }
}

fn has_hard_edge_stops(stops: &[ColorStop]) -> bool {
    // Dual-position bands and adjacent equal positions create hard edges that alias badly.
    stops.iter().any(|s| s.position_end.is_some())
        || stops
            .windows(2)
            .any(|w| match (w[0].position, w[1].position) {
                (Some(a), Some(b)) => {
                    (a - b).abs() < 1e-5 && !w[0].position_px && !w[1].position_px
                }
                _ => false,
            })
}

/// Rasterize with optional CSS border-radius (TL, TR, BR, BL in px) baked into alpha.
///
/// Uses 2×2 supersampling for repeating / hard-edge gradients to reduce stair-step aliasing.
pub fn rasterize_gradient_with_radii(
    grad: &Gradient,
    width: u32,
    height: u32,
    radii: [f32; 4],
) -> Vec<u8> {
    let sample = prepared_gradient_sampler(grad, width.max(1) as f32, height.max(1) as f32);
    // Cold large textures can consume a whole frame even with prepared stops.
    // Rows are independent; keep small / repeated-row fills on the UI thread.
    let horizontal = matches!(grad, Gradient::Linear(g) if g.angle_deg.to_radians().sin() == 0.0);
    // Several Gallery cards enter together at roughly 180×108 px. Keeping
    // those independent rows serial made a cold frame pay every raster in
    // sequence; the sampler is immutable and the output is disjoint.
    let workers = if !horizontal && u64::from(width) * u64::from(height) >= 16_384 {
        static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        *WORKERS.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| n.get().min(4)))
    } else {
        1
    };
    rasterize_gradient_with_workers(grad, width, height, radii, sample, workers)
}

#[cfg(test)]
fn rasterize_gradient_with_sampler(
    grad: &Gradient,
    width: u32,
    height: u32,
    radii: [f32; 4],
    sample: impl Fn(f32, f32) -> [f32; 4] + Sync,
) -> Vec<u8> {
    rasterize_gradient_with_workers(grad, width, height, radii, sample, 1)
}

fn rasterize_gradient_with_workers(
    grad: &Gradient,
    width: u32,
    height: u32,
    radii: [f32; 4],
    sample: impl Fn(f32, f32) -> [f32; 4] + Sync,
    workers: usize,
) -> Vec<u8> {
    let w = width.max(1);
    let h = height.max(1);
    let mut out = vec![0u8; (w * h * 4) as usize];
    let wf = w as f32;
    let hf = h as f32;
    let has_radii = radii.iter().any(|r| *r > 0.5);
    let ssaa = if gradient_needs_ssaa(grad) {
        2u32
    } else {
        1u32
    };
    let inv = 1.0 / (ssaa * ssaa) as f32;
    let horizontal = matches!(grad, Gradient::Linear(g) if g.angle_deg.to_radians().sin() == 0.0);
    let top = radii[0].max(radii[1]).max(0.0).ceil() as u32;
    let bottom = radii[2].max(radii[3]).max(0.0).ceil() as u32;
    let row_bytes = w as usize * 4;
    let render_rows = |start_y: u32, out: &mut [u8]| {
        let mut repeated_row = None;
        for local_y in 0..out.len() / row_bytes {
            let y = start_y + local_y as u32;
            let copy_row = horizontal && (!has_radii || (y >= top && y < h.saturating_sub(bottom)));
            if copy_row {
                if let Some(source) = repeated_row {
                    let dest = local_y * row_bytes;
                    out.copy_within(source..source + row_bytes, dest);
                    continue;
                }
                repeated_row = Some(local_y * row_bytes);
            }
            for x in 0..w {
                let mut acc = [0.0_f32; 4];
                for sy in 0..ssaa {
                    for sx in 0..ssaa {
                        let u = (x as f32 + (sx as f32 + 0.5) / ssaa as f32) / wf;
                        let v = (y as f32 + (sy as f32 + 0.5) / ssaa as f32) / hf;
                        let mut c = sample(u, v);
                        if has_radii {
                            let cover = rounded_rect_coverage(
                                x as f32 + (sx as f32 + 0.5) / ssaa as f32,
                                y as f32 + (sy as f32 + 0.5) / ssaa as f32,
                                wf,
                                hf,
                                radii,
                            );
                            c[3] *= cover;
                        }
                        acc[0] += c[0];
                        acc[1] += c[1];
                        acc[2] += c[2];
                        acc[3] += c[3];
                    }
                }
                let i = local_y * row_bytes + x as usize * 4;
                out[i] = (acc[0] * inv).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
                out[i + 1] = (acc[1] * inv).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
                out[i + 2] = (acc[2] * inv).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
                out[i + 3] = (acc[3] * inv).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
            }
        }
    };
    let workers = workers.max(1).min(h as usize);
    if workers == 1 {
        render_rows(0, &mut out);
    } else {
        let rows_per_chunk = (h as usize).div_ceil(workers);
        std::thread::scope(|scope| {
            let mut chunks = out.chunks_mut(rows_per_chunk * row_bytes);
            let first = chunks.next().unwrap();
            for (index, chunk) in chunks.enumerate() {
                let render_rows = &render_rows;
                scope.spawn(move || render_rows(((index + 1) * rows_per_chunk) as u32, chunk));
            }
            render_rows(0, first);
        });
    }
    out
}

/// Approximate coverage for a point inside a rounded rect (1 = inside, 0 = outside).
fn rounded_rect_coverage(px: f32, py: f32, w: f32, h: f32, radii: [f32; 4]) -> f32 {
    // radii: TL, TR, BR, BL (same order as Style::radii after resolve).
    let r_tl = radii[0].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_tr = radii[1].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_br = radii[2].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_bl = radii[3].min(w * 0.5).min(h * 0.5).max(0.0);

    let in_core = px >= r_tl.max(r_bl)
        && px <= w - r_tr.max(r_br)
        && py >= r_tl.max(r_tr)
        && py <= h - r_bl.max(r_br);
    if in_core {
        return 1.0;
    }

    let (cx, cy, r) = if px < r_tl && py < r_tl {
        (r_tl, r_tl, r_tl)
    } else if px > w - r_tr && py < r_tr {
        (w - r_tr, r_tr, r_tr)
    } else if px > w - r_br && py > h - r_br {
        (w - r_br, h - r_br, r_br)
    } else if px < r_bl && py > h - r_bl {
        (r_bl, h - r_bl, r_bl)
    } else if px < 0.0 || py < 0.0 || px > w || py > h {
        return 0.0;
    } else {
        return 1.0;
    };
    if r <= 0.0 {
        return 1.0;
    }
    let d = (px - cx).hypot(py - cy);
    (r + 0.5 - d).clamp(0.0, 1.0)
}

/// Stable texture key for a gradient paint (quantized size + radii).
pub fn gradient_texture_key(grad: &Gradient, w: u32, h: u32, radii: [f32; 4]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{grad:?}").hash(&mut hasher);
    w.hash(&mut hasher);
    h.hash(&mut hasher);
    for r in radii {
        ((r * 10.0).round() as i32).hash(&mut hasher);
    }
    format!("jadix-grad:{:016x}", hasher.finish())
}

struct GradCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

thread_local! {
    static GRADIENT_CPU: std::cell::RefCell<std::collections::HashMap<String, GradCpuEntry>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// Keys referenced during the current layout/collect pass.
    static GRADIENT_NEEDED: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Window DPI / `scale_factor` used when sizing gradient textures (logical → physical).
    static GRADIENT_RASTER_DPI: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Set the DPI used by [`gradient_texture_size`] during the next layout/collect.
pub fn set_gradient_raster_dpi(dpi: f32) {
    GRADIENT_RASTER_DPI.set(dpi.clamp(1.0, 3.0));
}

pub fn gradient_raster_dpi() -> f32 {
    GRADIENT_RASTER_DPI.get().clamp(1.0, 3.0)
}

/// Logical box size → texture pixel size (DPI-aware, capped).
pub fn gradient_texture_size(logical_w: f32, logical_h: f32) -> (u32, u32) {
    let dpi = gradient_raster_dpi();
    let tw = (logical_w * dpi).round().clamp(2.0, 1024.0) as u32;
    let th = (logical_h * dpi).round().clamp(2.0, 1024.0) as u32;
    (tw, th)
}

/// Scale CSS corner radii into texture pixel space.
pub fn gradient_texture_radii(logical_radii: [f32; 4]) -> [f32; 4] {
    let dpi = gradient_raster_dpi();
    [
        logical_radii[0] * dpi,
        logical_radii[1] * dpi,
        logical_radii[2] * dpi,
        logical_radii[3] * dpi,
    ]
}

/// Rasterize (cached) and mark this key as needed for the current frame.
/// Actual GPU upload is done by [`flush_gradient_textures`] so TexturePool eviction
/// can rehydrate without relying on a one-shot pending queue.
pub fn ensure_gradient_texture(
    grad: &Gradient,
    width: u32,
    height: u32,
    radii: [f32; 4],
) -> String {
    let width = width.max(1);
    let height = height.max(1);
    let key = gradient_texture_key(grad, width, height, radii);
    GRADIENT_CPU.with(|c| {
        let mut map = c.borrow_mut();
        if !map.contains_key(&key) {
            let rgba = rasterize_gradient_with_radii(grad, width, height, radii);
            map.insert(
                key.clone(),
                GradCpuEntry {
                    width,
                    height,
                    rgba,
                },
            );
        }
    });
    GRADIENT_NEEDED.with(|n| {
        n.borrow_mut().insert(key.clone());
    });
    key
}

/// Upload any needed gradient textures that are missing from the GPU pool.
/// Returns `true` if any texture was queued/uploaded (caller should request redraw).
pub fn flush_gradient_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = GRADIENT_NEEDED
        .with(|n| std::mem::take(&mut *n.borrow_mut()))
        .into_iter()
        .collect();
    let mut any = false;
    GRADIENT_CPU.with(|c| {
        let map = c.borrow();
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = map.get(&key) {
                crate::app::pending_images::queue_image_rgba8(
                    &key,
                    entry.width,
                    entry.height,
                    entry.rgba.clone(),
                );
                any = true;
            }
        }
    });
    any
}

/// Whether strip painting can approximate this gradient well — **deprecated**:
/// always prefer texture paint (strips leave seams with corner radii).
#[deprecated(note = "always use ensure_gradient_texture")]
#[allow(dead_code)]
pub fn prefers_strip_paint(_grad: &Gradient) -> bool {
    false
}

// ─── CSS parsing ──────────────────────────────────────────────────────────────

/// Parse a CSS gradient function (`linear-gradient(...)`, `radial-gradient(...)`,
/// `conic-gradient(...)`, or `repeating-*` variants). Returns `None` on failure.
pub fn parse_gradient(s: impl AsRef<str>) -> Option<Gradient> {
    let s = s.as_ref().trim();
    if s.is_empty() {
        return None;
    }
    let open = s.find('(')?;
    let name = s[..open].trim().to_ascii_lowercase();
    let close = s.rfind(')')?;
    if close <= open {
        return None;
    }
    let args = s[open + 1..close].trim();
    let repeating = name.starts_with("repeating-");
    let kind = name.strip_prefix("repeating-").unwrap_or(name.as_str());
    match kind {
        "linear-gradient" => parse_linear_args(args, repeating).map(Gradient::Linear),
        "radial-gradient" => parse_radial_args(args, repeating).map(Gradient::Radial),
        "conic-gradient" => parse_conic_args(args, repeating).map(Gradient::Conic),
        _ => None,
    }
}

fn split_gradient_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = args[start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

fn parse_css_angle_deg(tok: &str) -> Option<f32> {
    let t = tok.trim().to_ascii_lowercase();
    if let Some(raw) = t.strip_suffix("deg") {
        return raw.trim().parse().ok();
    }
    if let Some(raw) = t.strip_suffix("turn") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v * 360.0);
    }
    if let Some(raw) = t.strip_suffix("rad") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v.to_degrees());
    }
    if let Some(raw) = t.strip_suffix("grad") {
        let v: f32 = raw.trim().parse().ok()?;
        return Some(v * 0.9);
    }
    None
}

/// CSS Images angle → internal (`0` = left→right).
fn css_angle_to_internal(css_deg: f32) -> f32 {
    css_deg - 90.0
}

fn parse_to_side_angle(tok: &str) -> Option<f32> {
    let t = tok.trim().to_ascii_lowercase();
    if !t.starts_with("to ") {
        return None;
    }
    let rest = t.trim_start_matches("to ").trim();
    // Internal angles matching Tailwind bg-gradient-to-*.
    Some(match rest {
        "right" => 0.0,
        "left" => 180.0,
        "bottom" => 90.0,
        "top" => 270.0,
        "bottom right" | "right bottom" => 45.0,
        "bottom left" | "left bottom" => 135.0,
        "top left" | "left top" => 225.0,
        "top right" | "right top" => 315.0,
        _ => return None,
    })
}

fn parse_position_fraction(tok: &str) -> Option<f32> {
    parse_stop_offset(tok).map(|(v, is_px)| if is_px { v / 100.0 } else { v })
}

/// Parse a color-stop offset: `%` / keywords → fraction; `Npx` → absolute px (`is_px=true`).
fn parse_stop_offset(tok: &str) -> Option<(f32, bool)> {
    let t = tok.trim().to_ascii_lowercase();
    if let Some(raw) = t.strip_suffix('%') {
        let v: f32 = raw.trim().parse().ok()?;
        return Some((v / 100.0, false));
    }
    match t.as_str() {
        "left" | "top" => Some((0.0, false)),
        "center" => Some((0.5, false)),
        "right" | "bottom" => Some((1.0, false)),
        _ => {
            if let Some(raw) = t.strip_suffix("px") {
                let v: f32 = raw.trim().parse().ok()?;
                return Some((v, true));
            }
            None
        }
    }
}

fn parse_color_stop_item(item: &str) -> Option<(ColorStop, Option<ColorHint>)> {
    let item = item.trim();
    // Color hint alone: a percentage / length without a color.
    if let Some((p, is_px)) = parse_stop_offset(item) {
        if parse_gradient_color(item).is_none() {
            // Hints are midpoints along the gradient line; px → frac via 100px heuristic
            // (hints are uncommon with absolute lengths).
            let pos = if is_px { p / 100.0 } else { p };
            return Some((
                ColorStop::new([0.0, 0.0, 0.0, 0.0]), // placeholder — caller detects hint-only
                Some(ColorHint { position: pos }),
            ));
        }
    }

    // Split tokens but keep rgb(...)/oklch(...) together.
    let tokens = split_ws_preserving_parens(item);
    if tokens.is_empty() {
        return None;
    }

    let color = parse_gradient_color(tokens[0])?;

    let mut positions: Vec<(f32, bool)> = Vec::new();
    for tok in tokens.iter().skip(1) {
        if let Some(o) = parse_stop_offset(tok) {
            positions.push(o);
        } else if let Some(deg) = parse_css_angle_deg(tok) {
            positions.push((((deg % 360.0) + 360.0) % 360.0 / 360.0, false));
        }
    }

    let stop = match positions.as_slice() {
        [] => ColorStop::new(color),
        [(a, apx)] => ColorStop {
            color,
            position: Some(*a),
            position_end: None,
            position_px: *apx,
            position_end_px: false,
        },
        [(a, apx), (b, bpx), ..] => ColorStop {
            color,
            position: Some(*a),
            position_end: Some(*b),
            position_px: *apx,
            position_end_px: *bpx,
        },
    };
    Some((stop, None))
}

fn split_ws_preserving_parens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => {
                depth += 1;
                if start.is_none() {
                    start = Some(i);
                }
            }
            ')' => {
                depth = depth.saturating_sub(1);
            }
            c if c.is_whitespace() && depth == 0 => {
                if let Some(st) = start {
                    out.push(s[st..i].trim());
                    start = None;
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(st) = start {
        let t = s[st..].trim();
        if !t.is_empty() {
            out.push(t);
        }
    }
    out
}

fn parse_stop_list(parts: &[&str]) -> (Vec<ColorStop>, Vec<Option<ColorHint>>) {
    let mut stops = Vec::new();
    let mut hints: Vec<Option<ColorHint>> = Vec::new();
    let mut pending_hint: Option<ColorHint> = None;

    for part in parts {
        let Some((stop, hint_only)) = parse_color_stop_item(part) else {
            continue;
        };
        if let Some(h) = hint_only {
            // Bare hint between stops — attach to previous gap.
            if stops.is_empty() {
                pending_hint = Some(h);
            } else {
                while hints.len() < stops.len() - 1 {
                    hints.push(None);
                }
                if hints.len() == stops.len() - 1 {
                    hints.push(Some(h));
                } else {
                    pending_hint = Some(h);
                }
            }
            continue;
        }
        // Detect hint-only placeholder (zero alpha black + hint) from parse_color_stop_item
        // — already handled above via hint_only.
        if stop.color == [0.0, 0.0, 0.0, 0.0]
            && stop.position.is_none()
            && parse_gradient_color(part).is_none()
        {
            continue;
        }
        if let Some(h) = pending_hint.take() {
            while hints.len() < stops.len() {
                hints.push(None);
            }
            if !stops.is_empty() && hints.len() == stops.len() - 1 {
                hints.push(Some(h));
            }
        }
        stops.push(stop);
    }
    (stops, hints)
}

fn parse_linear_args(args: &str, repeating: bool) -> Option<LinearGradient> {
    let parts = split_gradient_args(args);
    if parts.is_empty() {
        return None;
    }
    let mut angle_deg = 90.0_f32; // CSS default = to bottom = internal 90
    let mut start = 0usize;
    if let Some(a) = parse_to_side_angle(parts[0]) {
        angle_deg = a;
        start = 1;
    } else if let Some(css) = parse_css_angle_deg(parts[0]) {
        angle_deg = css_angle_to_internal(css);
        start = 1;
    }
    let (stops, hints) = parse_stop_list(&parts[start..]);
    if stops.len() < 2 {
        return None;
    }
    Some(LinearGradient {
        angle_deg,
        stops,
        hints,
        repeating,
    })
}

fn parse_at_position(tokens: &[&str]) -> Option<(GradientPosition, usize)> {
    // Look for `at` keyword.
    let at_i = tokens.iter().position(|t| t.eq_ignore_ascii_case("at"))?;
    let rest = &tokens[at_i + 1..];
    if rest.is_empty() {
        return None;
    }
    let x = parse_position_fraction(rest[0]).unwrap_or(0.5);
    let (y, consumed) = if rest.len() >= 2 {
        if let Some(yy) = parse_position_fraction(rest[1]) {
            (yy, 2)
        } else {
            (x, 1) // single value repeats
        }
    } else {
        (x, 1)
    };
    Some((GradientPosition { x, y }, at_i + 1 + consumed))
}

fn parse_radial_args(args: &str, repeating: bool) -> Option<RadialGradient> {
    let parts = split_gradient_args(args);
    if parts.is_empty() {
        return None;
    }
    let mut shape = RadialShape::Ellipse;
    let mut extent = RadialExtent::default();
    let mut position = GradientPosition::default();
    let mut start = 0usize;

    // First arg may be geometry: [shape || size] [at position]
    let first_tokens = split_ws_preserving_parens(parts[0]);
    let looks_like_geo = first_tokens.iter().any(|t| {
        let l = t.to_ascii_lowercase();
        matches!(
            l.as_str(),
            "circle"
                | "ellipse"
                | "closest-side"
                | "closest-corner"
                | "farthest-side"
                | "farthest-corner"
                | "at"
        ) || parse_position_fraction(t).is_some()
            || t.ends_with("px")
            || t.ends_with('%')
    }) && parse_gradient_color(parts[0]).is_none();

    if looks_like_geo {
        start = 1;
        let mut i = 0usize;
        while i < first_tokens.len() {
            let l = first_tokens[i].to_ascii_lowercase();
            match l.as_str() {
                "circle" => {
                    shape = RadialShape::Circle;
                    i += 1;
                }
                "ellipse" => {
                    shape = RadialShape::Ellipse;
                    i += 1;
                }
                "closest-side" => {
                    extent = RadialExtent::Keyword(RadialSize::ClosestSide);
                    i += 1;
                }
                "closest-corner" => {
                    extent = RadialExtent::Keyword(RadialSize::ClosestCorner);
                    i += 1;
                }
                "farthest-side" => {
                    extent = RadialExtent::Keyword(RadialSize::FarthestSide);
                    i += 1;
                }
                "farthest-corner" => {
                    extent = RadialExtent::Keyword(RadialSize::FarthestCorner);
                    i += 1;
                }
                "at" => {
                    if let Some((pos, _)) = parse_at_position(&first_tokens[i..]) {
                        position = pos;
                    }
                    break;
                }
                _ => {
                    // Explicit size: one or two lengths/percentages.
                    if let Some(a) = parse_position_fraction(first_tokens[i]) {
                        if i + 1 < first_tokens.len() {
                            if let Some(b) = parse_position_fraction(first_tokens[i + 1]) {
                                extent = RadialExtent::Ellipse { rx: a, ry: b };
                                shape = RadialShape::Ellipse;
                                i += 2;
                                continue;
                            }
                        }
                        extent = RadialExtent::Circle(a);
                        shape = RadialShape::Circle;
                        i += 1;
                        continue;
                    }
                    i += 1;
                }
            }
        }
    }

    let (stops, hints) = parse_stop_list(&parts[start..]);
    if stops.len() < 2 {
        return None;
    }
    Some(RadialGradient {
        shape,
        extent,
        position,
        stops,
        hints,
        repeating,
    })
}

fn parse_conic_args(args: &str, repeating: bool) -> Option<ConicGradient> {
    let parts = split_gradient_args(args);
    if parts.is_empty() {
        return None;
    }
    let mut from_deg = -90.0_f32; // CSS default from top
    let mut position = GradientPosition::default();
    let mut start = 0usize;

    let first_tokens = split_ws_preserving_parens(parts[0]);
    let looks_like_geo = first_tokens
        .iter()
        .any(|t| t.eq_ignore_ascii_case("from") || t.eq_ignore_ascii_case("at"))
        && parse_gradient_color(parts[0]).is_none();

    if looks_like_geo {
        start = 1;
        let mut i = 0usize;
        while i < first_tokens.len() {
            let l = first_tokens[i].to_ascii_lowercase();
            if l == "from" {
                if i + 1 < first_tokens.len() {
                    if let Some(css) = parse_css_angle_deg(first_tokens[i + 1]) {
                        // CSS from 0deg = top; atan2 0 = right → internal from = css - 90
                        from_deg = css_angle_to_internal(css);
                    }
                    i += 2;
                    continue;
                }
            }
            if l == "at" {
                if let Some((pos, _)) = parse_at_position(&first_tokens[i..]) {
                    position = pos;
                }
                break;
            }
            i += 1;
        }
    }

    let (stops, hints) = parse_stop_list(&parts[start..]);
    if stops.len() < 2 {
        return None;
    }
    Some(ConicGradient {
        from_deg,
        position,
        stops,
        hints,
        repeating,
    })
}

#[cfg(test)]
#[path = "tests/gradient.rs"]
mod tests;
