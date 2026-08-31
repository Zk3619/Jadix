//! Compact scales (TanStack `@tanstack/charts/scales/*` narrowing).
//! linear / band / point are commonly used surfaces; [`scale_log`] / [`scale_time`] are **Rust-specific scale**
//!（对位 TanStack「compact → 专用算法」边界，**不是** JS `d3-scale`）。

use std::f32::consts::TAU;

/// Linear scale: domain → range (TanStack compact `scaleLinear` narrowing).
pub fn scale_linear(domain: (f32, f32), range: (f32, f32)) -> impl Fn(f32) -> f32 {
    let (d0, d1) = domain;
    let (r0, r1) = range;
    let span = (d1 - d0).abs().max(1e-6);
    let sign = if d1 >= d0 { 1.0 } else { -1.0 };
    move |v| r0 + (v - d0) * sign * (r1 - r0) / span
}

/// Band scale：n 个类别均分 range，返回 (bandwidth, x_for_index)。
/// `padding` ∈ [0, 1) is the proportion of class spacing to step (≈ TanStack `scaleBand().padding`).
pub fn scale_band(n: usize, range: (f32, f32), padding: f32) -> (f32, impl Fn(usize) -> f32) {
    let n = n.max(1);
    let (r0, r1) = range;
    let total = (r1 - r0).max(0.0);
    let pad = padding.clamp(0.0, 0.9);
    let step = total / n as f32;
    let bw = (step * (1.0 - pad)).max(1.0);
    let gap = step - bw;
    (bw, move |i| r0 + i as f32 * step + gap * 0.5)
}

/// Point scale: category center point, no bandwidth (≈ TanStack `scalePoint`).
pub fn scale_point(n: usize, range: (f32, f32), padding: f32) -> impl Fn(usize) -> f32 {
    let n = n.max(1);
    let (r0, r1) = range;
    let pad = padding.clamp(0.0, 0.9);
    let inner = (r1 - r0) * (1.0 - pad);
    let start = r0 + ((r1 - r0) - inner) * 0.5;
    let step = if n == 1 { 0.0 } else { inner / (n - 1) as f32 };
    move |i| start + i as f32 * step
}

/// Polar pie angle distribution: starting from `start`, cut a full circle according to the proportion of positive values.
/// 返回 `(start_rad, end_rad)`，约定 0 = 正右、屏幕 y 向下时增大角顺时针。
pub fn pie_layout(values: &[f32], start: f32) -> Vec<(f32, f32)> {
    pie_layout_span(values, start, start + TAU)
}

/// Polar angle assignment: restricted to `[start, end]` (gauge / semicircle).
pub fn pie_layout_span(values: &[f32], start: f32, end: f32) -> Vec<(f32, f32)> {
    let span = end - start;
    let sum = values.iter().map(|v| v.max(0.0)).sum::<f32>().max(1e-6);
    let mut a = start;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        let d = (v.max(0.0) / sum) * span;
        out.push((a, a + d));
        a += d;
    }
    out
}

/// Log scale (base > 1). Non-positive domain values ​​are clamped to `1e-12`.
pub fn scale_log(domain: (f32, f32), range: (f32, f32), base: f32) -> impl Fn(f32) -> f32 {
    let base = base.max(1.0001);
    let ln_b = base.ln();
    let d0 = domain.0.max(1e-12).ln() / ln_b;
    let d1 = domain.1.max(1e-12).ln() / ln_b;
    let (r0, r1) = range;
    let span = (d1 - d0).abs().max(1e-6);
    let sign = if d1 >= d0 { 1.0 } else { -1.0 };
    move |v| {
        let t = v.max(1e-12).ln() / ln_b;
        r0 + (t - d0) * sign * (r1 - r0) / span
    }
}

/// 时间 scale：Unix **毫秒** → 像素。内部用 f64，避免 f32 在 1e12 ms 丢精度。
pub fn scale_time(domain: (i64, i64), range: (f32, f32)) -> impl Fn(i64) -> f32 {
    let (d0, d1) = domain;
    let (r0, r1) = range;
    let span = (d1 as f64 - d0 as f64).abs().max(1.0);
    let sign = if d1 >= d0 { 1.0 } else { -1.0 };
    move |v| (r0 as f64 + (v as f64 - d0 as f64) * sign * (r1 - r0) as f64 / span) as f32
}

/// UTC 标签。`span_ms` 决定粒度（时:分 / 月-日 / 年-月-日）。
pub fn format_time_label(unix_ms: i64, span_ms: i64) -> String {
    let (y, m, d, hh, mm) = unix_ms_to_utc(unix_ms);
    if span_ms.abs() <= 2 * 86_400_000 {
        format!("{hh:02}:{mm:02}")
    } else if span_ms.abs() <= 90 * 86_400_000 {
        format!("{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    }
}

/// Howard Hinnant `civil_from_days`: Unix milliseconds → UTC (y, month, day, hour, min).
pub fn unix_ms_to_utc(ms: i64) -> (i32, u8, u8, u8, u8) {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400) as i64;
    let hour = (rem / 3600) as u8;
    let min = ((rem % 3600) / 60) as u8;
    // days since 1970-01-01 → days since 0000-03-01 (Hinnant serial)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 {
        (mp + 3) as u8
    } else {
        (mp - 9) as u8
    };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month, d, hour, min)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleKind {
    Linear,
    Band,
    Point,
    Log,
    Time,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub kind: ScaleKind,
    pub padding: f32,
    pub nice: bool,
    pub grid: bool,
    pub axis: bool,
    pub label: Option<String>,
    pub domain_num: Option<(f32, f32)>,
    pub domain_cats: Option<Vec<String>>,
    pub domain_time: Option<(i64, i64)>,
    pub log_base: f32,
}

impl AxisSpec {
    pub fn linear() -> Self {
        Self {
            kind: ScaleKind::Linear,
            padding: 0.0,
            nice: false,
            grid: false,
            axis: true,
            label: None,
            domain_num: None,
            domain_cats: None,
            domain_time: None,
            log_base: 10.0,
        }
    }

    pub fn band() -> Self {
        Self {
            kind: ScaleKind::Band,
            padding: 0.2,
            nice: false,
            grid: false,
            axis: true,
            label: None,
            domain_num: None,
            domain_cats: None,
            domain_time: None,
            log_base: 10.0,
        }
    }

    pub fn point() -> Self {
        Self {
            kind: ScaleKind::Point,
            padding: 0.1,
            nice: false,
            grid: false,
            axis: true,
            label: None,
            domain_num: None,
            domain_cats: None,
            domain_time: None,
            log_base: 10.0,
        }
    }

    pub fn log() -> Self {
        Self {
            kind: ScaleKind::Log,
            padding: 0.0,
            nice: false,
            grid: true,
            axis: true,
            label: None,
            domain_num: None,
            domain_cats: None,
            domain_time: None,
            log_base: 10.0,
        }
    }

    pub fn time() -> Self {
        Self {
            kind: ScaleKind::Time,
            padding: 0.0,
            nice: false,
            grid: false,
            axis: true,
            label: None,
            domain_num: None,
            domain_cats: None,
            domain_time: None,
            log_base: 10.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedScales {
    pub x_kind: ScaleKind,
    pub y_kind: ScaleKind,
    pub plot_w: f32,
    pub plot_h: f32,
    pub pad: [f32; 4], // l t r b
    pub x_cats: Vec<String>,
    pub x_domain: (f32, f32),
    pub y_domain: (f32, f32),
    pub x_time_domain: Option<(i64, i64)>,
    pub bandwidth: f32,
    pub log_base_x: f32,
    pub log_base_y: f32,
    x_pad: f32,
}

impl ResolvedScales {
    pub fn inner_w(&self) -> f32 {
        (self.plot_w - self.pad[0] - self.pad[2]).max(1.0)
    }

    pub fn inner_h(&self) -> f32 {
        (self.plot_h - self.pad[1] - self.pad[3]).max(1.0)
    }

    fn x_range(&self) -> (f32, f32) {
        (self.pad[0], self.plot_w - self.pad[2])
    }

    fn y_range(&self) -> (f32, f32) {
        (self.plot_h - self.pad[3], self.pad[1])
    }

    /// 类别左缘（band）或中心（point）；linear 时按 index 均匀。
    pub fn x_index(&self, i: usize) -> f32 {
        match self.x_kind {
            ScaleKind::Band => {
                let (_bw, x_of) = scale_band(self.x_cats.len().max(1), self.x_range(), self.x_pad);
                x_of(i)
            }
            ScaleKind::Point => {
                scale_point(self.x_cats.len().max(1), self.x_range(), self.x_pad)(i)
            }
            ScaleKind::Linear | ScaleKind::Log | ScaleKind::Time => {
                let n = self.x_cats.len().max(1);
                let x_of = scale_linear((0.0, (n.saturating_sub(1).max(1)) as f32), self.x_range());
                x_of(i as f32)
            }
        }
    }

    pub fn x_center(&self, i: usize) -> f32 {
        match self.x_kind {
            ScaleKind::Band => self.x_index(i) + self.bandwidth * 0.5,
            _ => self.x_index(i),
        }
    }

    pub fn x_num(&self, v: f32) -> f32 {
        map_numeric(
            self.x_kind,
            self.x_domain,
            self.x_range(),
            v,
            self.log_base_x,
        )
    }

    pub fn x_time(&self, unix_ms: i64) -> f32 {
        if let Some(dom) = self.x_time_domain {
            scale_time(dom, self.x_range())(unix_ms)
        } else {
            self.x_num(unix_ms as f32)
        }
    }

    pub fn y(&self, v: f32) -> f32 {
        map_numeric(
            self.y_kind,
            self.y_domain,
            self.y_range(),
            v,
            self.log_base_y,
        )
    }

    pub fn polar_cx(&self) -> f32 {
        self.plot_w * 0.5
    }
    pub fn polar_cy(&self) -> f32 {
        self.plot_h * 0.5
    }
    pub fn polar_r_max(&self) -> f32 {
        self.plot_w.min(self.plot_h) * 0.42
    }
    pub fn polar_xy(&self, angle: f32, r: f32) -> crate::primitives::path::Point2 {
        (
            self.polar_cx() + r * angle.cos(),
            self.polar_cy() + r * angle.sin(),
        )
    }
    pub fn polar_radius(&self, v: f32, inner_frac: f32) -> f32 {
        let r0 = self.polar_r_max() * inner_frac.clamp(0.0, 0.85);
        let r1 = self.polar_r_max();
        map_numeric(
            self.y_kind,
            self.y_domain,
            (r0, r1),
            v.max(0.0),
            self.log_base_y,
        )
    }
    pub fn polar_angle_center(&self, i: usize, n: usize, start: f32, end: f32) -> f32 {
        let n = n.max(1);
        let span = end - start;
        if n == 1 {
            start + span * 0.5
        } else {
            start + span * (i as f32 + 0.5) / n as f32
        }
    }
    pub fn polar_angle_band(
        &self,
        i: usize,
        n: usize,
        start: f32,
        end: f32,
        pad: f32,
    ) -> (f32, f32) {
        let n = n.max(1);
        let span = (end - start) / n as f32;
        let gap = span * pad.clamp(0.0, 0.8);
        let a0 = start + i as f32 * span + gap * 0.5;
        (a0, a0 + (span - gap).max(1e-4))
    }

    pub fn y0(&self) -> f32 {
        self.y(0.0f32.max(self.y_domain.0).min(self.y_domain.1))
    }

    /// 定量值 → 水平像素（`barX`：值在 X，类在 Y）。
    pub fn x_value(&self, v: f32) -> f32 {
        scale_linear(self.y_domain, self.x_range())(v)
    }

    pub fn x_value0(&self) -> f32 {
        self.x_value(0.0f32.max(self.y_domain.0).min(self.y_domain.1))
    }

    /// 类别 → 垂直 band 左缘（`barX`）。
    pub fn y_index(&self, i: usize) -> f32 {
        let n = self.x_cats.len().max(1);
        let (r0, r1) = (self.pad[1], self.plot_h - self.pad[3]);
        let (_bw, y_of) = scale_band(n, (r0, r1), self.x_pad);
        y_of(i)
    }

    pub fn bandwidth_y(&self) -> f32 {
        let n = self.x_cats.len().max(1);
        let (r0, r1) = (self.pad[1], self.plot_h - self.pad[3]);
        scale_band(n, (r0, r1), self.x_pad).0
    }

    pub fn cat_index(&self, label: &str) -> Option<usize> {
        self.x_cats.iter().position(|c| c == label)
    }
}

fn nice_log_domain(min: f32, max: f32, base: f32) -> (f32, f32) {
    let ln_b = base.max(1.0001).ln();
    let lo_exp = (min.max(1e-12).ln() / ln_b).floor();
    let hi_exp = (max.max(min * 1.0001).ln() / ln_b).ceil();
    let lo = base.powf(lo_exp);
    let hi = base.powf(hi_exp.max(lo_exp + 1.0));
    (lo.max(1e-12), hi)
}

pub(crate) fn nice_domain(mut d0: f32, mut d1: f32) -> (f32, f32) {
    if (d1 - d0).abs() < 1e-6 {
        d1 = d0 + 1.0;
    }
    if d1 < d0 {
        std::mem::swap(&mut d0, &mut d1);
    }
    let span = d1 - d0;
    let mag = 10f32.powf(span.log10().floor());
    let step = if span / mag <= 2.0 {
        mag / 2.0
    } else if span / mag <= 5.0 {
        mag
    } else {
        mag * 2.0
    };
    let lo = (d0 / step).floor() * step;
    let hi = (d1 / step).ceil() * step;
    (lo, hi.max(lo + step))
}

fn map_numeric(
    kind: ScaleKind,
    domain: (f32, f32),
    range: (f32, f32),
    v: f32,
    log_base: f32,
) -> f32 {
    match kind {
        ScaleKind::Log => scale_log(domain, range, log_base)(v),
        _ => scale_linear(domain, range)(v),
    }
}

fn numeric_domain(spec: &AxisSpec, values: &[f32], include_zero: bool, is_log: bool) -> (f32, f32) {
    if let Some(d) = spec.domain_num {
        return d;
    }
    if values.is_empty() {
        return if is_log { (1.0, 10.0) } else { (0.0, 1.0) };
    }
    let mut min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let mut max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if is_log {
        min = min.max(1e-12);
        max = max.max(min * 1.0001);
        return if spec.nice {
            nice_log_domain(min, max, spec.log_base.max(1.0001))
        } else {
            (min, max)
        };
    }
    if include_zero {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    if spec.nice {
        nice_domain(min, max)
    } else {
        (min, max.max(min + 1e-6))
    }
}

pub(crate) fn resolve_scales(
    plot_w: f32,
    plot_h: f32,
    x: &AxisSpec,
    y: &AxisSpec,
    x_cats: Vec<String>,
    x_nums: &[f32],
    x_times: &[i64],
    y_nums: &[f32],
    y_include_zero: bool,
) -> ResolvedScales {
    resolve_scales_pad(
        plot_w,
        plot_h,
        x,
        y,
        x_cats,
        x_nums,
        x_times,
        y_nums,
        y_include_zero,
        [8.0, 8.0, 8.0, 8.0],
    )
}

pub(crate) fn resolve_scales_pad(
    plot_w: f32,
    plot_h: f32,
    x: &AxisSpec,
    y: &AxisSpec,
    x_cats: Vec<String>,
    x_nums: &[f32],
    x_times: &[i64],
    y_nums: &[f32],
    y_include_zero: bool,
    pad: [f32; 4],
) -> ResolvedScales {
    let x_time_domain = x.domain_time.or_else(|| {
        if x_times.is_empty() {
            None
        } else {
            let min = x_times.iter().copied().min().unwrap_or(0);
            let max = x_times.iter().copied().max().unwrap_or(0);
            Some((min, max.max(min + 1)))
        }
    });
    let x_kind = if x.kind == ScaleKind::Time
        || (x.kind == ScaleKind::Linear
            && x_time_domain.is_some()
            && x_nums.is_empty()
            && x_cats.is_empty())
    {
        ScaleKind::Time
    } else {
        x.kind
    };
    let x_domain = if x_kind == ScaleKind::Time {
        if let Some((a, b)) = x_time_domain {
            (a as f32, b as f32)
        } else {
            (0.0, 1.0)
        }
    } else {
        numeric_domain(x, x_nums, false, x_kind == ScaleKind::Log)
    };
    let y_domain = numeric_domain(
        y,
        y_nums,
        y_include_zero && y.kind != ScaleKind::Log,
        y.kind == ScaleKind::Log,
    );
    let n_cat = x_cats.len().max(1);
    let x_range = (pad[0], plot_w - pad[2]);
    let (bandwidth, x_pad) = match x_kind {
        ScaleKind::Band => {
            let (bw, _) = scale_band(n_cat, x_range, x.padding);
            (bw, x.padding)
        }
        _ => (0.0, x.padding),
    };
    ResolvedScales {
        x_kind,
        y_kind: y.kind,
        plot_w,
        plot_h,
        pad,
        x_cats,
        x_domain,
        y_domain,
        x_time_domain,
        bandwidth,
        log_base_x: x.log_base.max(1.0001),
        log_base_y: y.log_base.max(1.0001),
        x_pad,
    }
}
