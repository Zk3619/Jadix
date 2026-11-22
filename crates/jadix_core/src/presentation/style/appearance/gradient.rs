//! Geometry and CSS stop fixup shared by native renderers. Coordinates are points.
use super::{GradientKind, GradientPosition, GradientSize, ViewGradient};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct GradientPaint {
    pub kind: &'static str,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub radii: [f64; 2],
    pub angle: f64,
    pub colors: Vec<[u8; 4]>,
    pub positions: Vec<f64>,
}

impl ViewGradient {
    /// Resolve for the actual painted size, including omitted stops, hard stops,
    /// off-center ellipses and repetitions. Returned stops cover exactly 0..=1.
    pub fn resolve(&self, size: [f64; 2]) -> Option<GradientPaint> {
        if !self.is_valid() || size.iter().any(|x| !x.is_finite() || *x <= 0. || *x > 1_000_000.) {
            return None;
        }
        let [w, h] = size;
        let mut paint = GradientPaint { kind: "linear", start: [0.; 2], end: [0.; 2],
            radii: [0.; 2], angle: 0., colors: Vec::new(), positions: Vec::new() };
        let length;
        let mut reach = 1.;
        match self.geometry {
            GradientKind::Linear { .. } | GradientKind::LinearCorner { .. } => {
                let angle = match self.geometry {
                    GradientKind::Linear { angle } => f64::from(angle),
                    GradientKind::LinearCorner { right, bottom } => (if right { h } else { -h })
                        .atan2(if bottom { -w } else { w }),
                    _ => unreachable!(),
                };
                let (dx, dy) = (angle.sin(), -angle.cos());
                length = (w * dx).abs() + (h * dy).abs();
                paint.start = [w / 2. - dx * length / 2., h / 2. - dy * length / 2.];
                paint.end = [w / 2. + dx * length / 2., h / 2. + dy * length / 2.];
            }
            GradientKind::Radial { circle, size, center } => {
                paint.kind = "radial";
                paint.start = [w * f64::from(center[0]), h * f64::from(center[1])];
                let [cx, cy] = paint.start;
                let xs = [cx.abs(), (w - cx).abs()];
                let ys = [cy.abs(), (h - cy).abs()];
                let closest = matches!(size, GradientSize::ClosestSide | GradientSize::ClosestCorner);
                let corner = matches!(size, GradientSize::ClosestCorner | GradientSize::FarthestCorner);
                let choose = |a: f64, b: f64| if closest { a.min(b) } else { a.max(b) };
                let (mut rx, mut ry) = (choose(xs[0], xs[1]), choose(ys[0], ys[1]));
                if circle {
                    let values = if corner { [xs[0].hypot(ys[0]), xs[0].hypot(ys[1]), xs[1].hypot(ys[0]), xs[1].hypot(ys[1])] }
                        else { [xs[0], xs[1], ys[0], ys[1]] };
                    rx = values.into_iter().reduce(choose)?; ry = rx;
                } else if corner { rx *= 2f64.sqrt(); ry *= 2f64.sqrt(); }
                length = rx;
                if rx > 0. && ry > 0. {
                    for x in [0., w] { for y in [0., h] {
                        reach = f64::max(reach, ((x - cx) / rx).hypot((y - cy) / ry));
                    }}
                }
                paint.radii = [rx * reach, ry * reach];
            }
            GradientKind::Conic { angle, center } => {
                paint.kind = "conic";
                paint.start = [w * f64::from(center[0]), h * f64::from(center[1])];
                paint.angle = f64::from(angle);
                // Conic point positions have no length and resolve to the origin.
                length = 0.;
            }
        }
        let mut stops: Vec<Option<f64>> = self.stops.iter().map(|stop| match stop.position {
            None => None,
            Some(GradientPosition::Fraction(x)) => Some(f64::from(x)),
            Some(GradientPosition::Points(x)) => Some(if length > 0. { f64::from(x) / length } else { 0. }),
        }).collect();
        let last = stops.len() - 1;
        stops[0].get_or_insert(0.); stops[last].get_or_insert(1.);
        let mut previous = f64::NEG_INFINITY;
        for value in stops.iter_mut().flatten() { *value = value.max(previous); previous = *value; }
        let mut start = 0;
        for end in 1..stops.len() {
            if let Some(b) = stops[end] {
                let a = stops[start]?;
                for (i, value) in stops.iter_mut().enumerate().take(end).skip(start + 1) {
                    *value = Some(a + (b - a) * (i - start) as f64 / (end - start) as f64);
                }
                start = end;
            }
        }
        let stops: Vec<f64> = stops.into_iter().collect::<Option<_>>()?;
        let low = stops[0]; let period = stops[last] - low;
        let sample = |mut x: f64| {
            if self.repeating {
                if period <= f64::EPSILON { return self.stops[last].color; }
                x = low + (x - low).rem_euclid(period);
            }
            // partition_point also gives the latter color at coincident hard stops.
            let upper = stops.partition_point(|stop| *stop <= x);
            if upper == 0 { return self.stops[0].color; }
            if upper == stops.len() { return self.stops[last].color; }
            mix(self.stops[upper - 1].color, self.stops[upper].color,
                (x - stops[upper - 1]) / (stops[upper] - stops[upper - 1]))
        };
        paint.positions.push(0.); paint.colors.push(sample(0.));
        if self.repeating && period > f64::EPSILON {
            let from = ((-stops[last]) / period).floor();
            let to = ((reach - low) / period).ceil();
            // Subpixel repetition can otherwise allocate unbounded native stop arrays.
            if to - from > 16_384. / stops.len() as f64 {
                // Below the stop budget's sampling resolution, use the mean of
                // a complete period, independent of the rectangle's phase.
                let mut rgba = [0.; 4];
                for i in 1..stops.len() {
                    let weight = (stops[i] - stops[i - 1]) / period / 2.;
                    for color in [self.stops[i - 1].color, self.stops[i].color] {
                        rgba[3] += f64::from(color[3]) * weight;
                        for channel in 0..3 { rgba[channel] += f64::from(color[channel]) * f64::from(color[3]) * weight; }
                    }
                }
                let mut color = [0; 4]; color[3] = rgba[3].round() as u8;
                if rgba[3] > 0. { for i in 0..3 { color[i] = (rgba[i] / rgba[3]).round() as u8; } }
                paint.colors[0] = color;
                paint.colors.push(color); paint.positions.push(1.);
                return Some(paint);
            } else {
                for cycle in from as i64..=to as i64 {
                    for (i, stop) in stops.iter().enumerate() {
                        let x = *stop + cycle as f64 * period;
                        if x >= 0. && x <= reach { paint.positions.push(x / reach); paint.colors.push(self.stops[i].color); }
                    }
                }
            }
        } else if !self.repeating {
            for (i, x) in stops.iter().enumerate() {
                if *x >= 0. && *x <= reach { paint.positions.push(x / reach); paint.colors.push(self.stops[i].color); }
            }
        }
        paint.positions.push(1.); paint.colors.push(sample(reach));
        Some(paint)
    }
}

fn mix(a: [u8; 4], b: [u8; 4], t: f64) -> [u8; 4] {
    let t = t.clamp(0., 1.);
    let alpha = f64::from(a[3]) * (1. - t) + f64::from(b[3]) * t;
    let mut result = [0; 4]; result[3] = alpha.round() as u8;
    if alpha > 0. { for i in 0..3 {
        result[i] = ((f64::from(a[i]) * f64::from(a[3]) * (1. - t)
            + f64::from(b[i]) * f64::from(b[3]) * t) / alpha).round() as u8;
    }}
    result
}
