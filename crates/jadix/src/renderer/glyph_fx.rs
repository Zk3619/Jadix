//! Glyph / cluster-level motion (B17). Applied in prepare, not by splitting DOM spans.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphFxKind {
    None,
    Blur,
    Split,
    Scramble,
    CountUp,
    Dissolve,
    /// shadcn `shimmer` — highlight band sweeps along the glyphs.
    Shimmer,
}

impl GlyphFxKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "blur" | "blurtext" => Self::Blur,
            "split" | "splittext" => Self::Split,
            "scramble" | "decrypt" => Self::Scramble,
            "countup" | "count" => Self::CountUp,
            "dissolve" | "particle" => Self::Dissolve,
            "shimmer" => Self::Shimmer,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphFx {
    pub kind: GlyphFxKind,
    /// 0..=1 progress from the timeline.
    pub t: f32,
}

impl GlyphFx {
    pub fn new(kind: GlyphFxKind, t: f32) -> Self {
        Self {
            kind,
            t: t.clamp(0.0, 1.0),
        }
    }

    /// Per-cluster opacity. `i` in `0..n`.
    pub fn opacity(&self, i: usize, n: usize) -> f32 {
        let n = n.max(1) as f32;
        let u = i as f32 / n;
        match self.kind {
            GlyphFxKind::None => 1.0,
            GlyphFxKind::Blur => (self.t * 1.4 - u * 0.4).clamp(0.15, 1.0),
            GlyphFxKind::Split => {
                if u <= self.t {
                    1.0
                } else {
                    0.0
                }
            }
            GlyphFxKind::Scramble => 0.35 + 0.65 * self.t,
            GlyphFxKind::CountUp => self.t,
            GlyphFxKind::Dissolve => (self.t - u * 0.7).clamp(0.0, 1.0),
            GlyphFxKind::Shimmer => 1.0,
        }
    }

    /// Extra offset in logical px (dx, dy).
    pub fn offset(&self, i: usize, n: usize) -> (f32, f32) {
        let n = n.max(1) as f32;
        let u = i as f32 / n;
        match self.kind {
            GlyphFxKind::Split => {
                let wave = (1.0 - self.t).max(0.0);
                (0.0, (u - 0.5) * 18.0 * wave)
            }
            GlyphFxKind::Blur => (0.0, (1.0 - self.t) * 4.0),
            GlyphFxKind::Dissolve => ((u - 0.5) * (1.0 - self.t) * 12.0, (1.0 - self.t) * 8.0),
            _ => (0.0, 0.0),
        }
    }
}

pub const ATTR_GLYPH_FX: &str = "jadix_glyph_fx";
pub const ATTR_SHIMMER_DURATION_MS: &str = "jadix_shimmer_duration_ms";
pub const ATTR_SHIMMER_SPREAD_PX: &str = "jadix_shimmer_spread_px";
pub const ATTR_SHIMMER_ANGLE_DEG: &str = "jadix_shimmer_angle_deg";
pub const ATTR_SHIMMER_COLOR: &str = "jadix_shimmer_color";
pub const ATTR_SHIMMER_REVERSE: &str = "jadix_shimmer_reverse";
pub const ATTR_SHIMMER_ONCE: &str = "jadix_shimmer_once";
pub const ATTR_SHIMMER_COUNT: &str = "jadix_shimmer_count";
pub const ATTR_SHIMMER_EPOCH: &str = "jadix_shimmer_epoch";
/// Container overlay (`shimmer-box` / `animate-shimmer`). Paints above this
/// node's contents (including glyphs). Does not cascade to children.
pub const ATTR_SHIMMER_BOX: &str = "jadix_shimmer_box";

/// Attrs that cascade from a `class="shimmer"` container onto descendant text leaves.
/// Container overlay stays on the declaring node (`ATTR_SHIMMER_BOX` is not inherited).
pub const SHIMMER_INHERIT_ATTRS: &[&str] = &[
    ATTR_GLYPH_FX,
    ATTR_SHIMMER_DURATION_MS,
    ATTR_SHIMMER_SPREAD_PX,
    ATTR_SHIMMER_ANGLE_DEG,
    ATTR_SHIMMER_COLOR,
    ATTR_SHIMMER_REVERSE,
    ATTR_SHIMMER_ONCE,
    ATTR_SHIMMER_COUNT,
    ATTR_SHIMMER_EPOCH,
];

/// Paint parameters for a shadcn `shimmer` sweep.
#[derive(Clone, Debug, PartialEq)]
pub struct ShimmerParams {
    pub duration_ms: f32,
    pub spread_px: Option<f32>,
    pub angle_deg: f32,
    pub color: Option<[u8; 4]>,
    pub reverse: bool,
    /// `None` = infinite loop. `Some(n)` plays `n` sweeps then settles.
    pub count: Option<f32>,
    pub epoch_s: f32,
}

impl Default for ShimmerParams {
    fn default() -> Self {
        Self {
            duration_ms: 2000.0,
            spread_px: None,
            angle_deg: 20.0,
            color: None,
            reverse: false,
            count: None,
            epoch_s: 0.0,
        }
    }
}

impl ShimmerParams {
    pub fn from_shared_attrs(attrs: &HashMap<String, String>) -> Self {
        Self {
            duration_ms: attrs
                .get(ATTR_SHIMMER_DURATION_MS)
                .and_then(|s| s.parse().ok())
                .unwrap_or(2000.0),
            spread_px: attrs
                .get(ATTR_SHIMMER_SPREAD_PX)
                .and_then(|s| s.parse().ok()),
            angle_deg: attrs
                .get(ATTR_SHIMMER_ANGLE_DEG)
                .and_then(|s| s.parse().ok())
                .unwrap_or(20.0),
            color: attrs.get(ATTR_SHIMMER_COLOR).and_then(|s| parse_rgba8(s)),
            reverse: attrs
                .get(ATTR_SHIMMER_REVERSE)
                .map(|s| s == "1")
                .unwrap_or(false),
            count: parse_count_attr(attrs),
            epoch_s: attrs
                .get(ATTR_SHIMMER_EPOCH)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
        }
    }

    /// Text-clip shimmer (`class="shimmer"`).
    pub fn from_attrs(attrs: &HashMap<String, String>) -> Option<Self> {
        let raw = attrs.get(ATTR_GLYPH_FX)?;
        if GlyphFxKind::parse(raw) != GlyphFxKind::Shimmer {
            return None;
        }
        Some(Self::from_shared_attrs(attrs))
    }

    /// Container overlay (`class="shimmer-box"` / `animate-shimmer`).
    pub fn box_from_attrs(attrs: &HashMap<String, String>) -> Option<Self> {
        if attrs.get(ATTR_SHIMMER_BOX).map(|s| s.as_str()) != Some("1") {
            return None;
        }
        Some(Self::from_shared_attrs(attrs))
    }

    pub fn text_active(attrs: &HashMap<String, String>) -> bool {
        attrs
            .get(ATTR_GLYPH_FX)
            .is_some_and(|s| GlyphFxKind::parse(s) == GlyphFxKind::Shimmer)
    }

    pub fn box_active(attrs: &HashMap<String, String>) -> bool {
        attrs.get(ATTR_SHIMMER_BOX).map(|s| s.as_str()) == Some("1")
    }

    pub fn effect_active(attrs: &HashMap<String, String>) -> bool {
        Self::text_active(attrs) || Self::box_active(attrs)
    }

    /// Sweep progress in `0..=1` within the current cycle.
    pub fn progress(&self, now_s: f32) -> f32 {
        let dur = (self.duration_ms / 1000.0).max(0.001);
        let elapsed = (now_s - self.epoch_s).max(0.0);
        let cycles = elapsed / dur;
        match self.count {
            Some(n) if n <= 0.0 || cycles >= n => 1.0,
            _ => cycles.rem_euclid(1.0),
        }
    }

    pub fn finished(&self, now_s: f32) -> bool {
        let Some(n) = self.count else {
            return false;
        };
        let dur = (self.duration_ms / 1000.0).max(0.001);
        (now_s - self.epoch_s).max(0.0) >= n * dur
    }

    /// Highlight color: explicit token, or a sheen derived from `currentColor`.
    pub fn highlight(&self, base: [u8; 4]) -> [u8; 4] {
        if let Some(c) = self.color {
            return c;
        }
        default_shimmer_highlight(base)
    }

    /// Default CSS spread: `calc(3ch + 40px)` with `1ch ≈ 0.5em`.
    pub fn spread_px_or_default(&self, font_size: f32) -> f32 {
        self.spread_px
            .unwrap_or_else(|| 3.0 * font_size.max(1.0) * 0.5 + 40.0)
    }
}

fn parse_count_attr(attrs: &HashMap<String, String>) -> Option<f32> {
    if let Some(raw) = attrs.get(ATTR_SHIMMER_COUNT) {
        let s = raw.trim();
        if s.eq_ignore_ascii_case("inf") || s.eq_ignore_ascii_case("infinite") {
            return None;
        }
        if let Ok(n) = s.parse::<f32>() {
            if n > 0.0 && n.is_finite() {
                return Some(n);
            }
        }
    }
    if attrs.get(ATTR_SHIMMER_ONCE).map(|s| s.as_str()) == Some("1") {
        Some(1.0)
    } else {
        None
    }
}

/// Same sweep as text [`shimmer_band_mix`]: `u` is normalized x across the box.
pub fn shimmer_box_mix_at(params: &ShimmerParams, now_s: f32, u: f32, box_w: f32) -> f32 {
    let t = params.progress(now_s);
    let spread = params
        .spread_px
        .map(|px| (px / box_w.max(1.0)).clamp(0.12, 0.85))
        .unwrap_or(0.22);
    shimmer_band_mix(u, t, spread, params.reverse, params.angle_deg)
}

/// White sheen (matches glyph shimmer). Peak alpha only — column mix supplies the falloff.
pub fn shimmer_box_highlight_rgba(params: &ShimmerParams, _surface: [f32; 4]) -> [f32; 4] {
    if let Some(c) = params.color {
        return [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            ((c[3] as f32 / 255.0) * 0.55).clamp(0.15, 0.75),
        ];
    }
    let peak = if crate::style::is_dark_theme() {
        0.38
    } else {
        0.28
    };
    [1.0, 1.0, 1.0, peak]
}

fn parse_rgba8(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    Some([
        parts[0].trim().parse().ok()?,
        parts[1].trim().parse().ok()?,
        parts[2].trim().parse().ok()?,
        parts[3].trim().parse().ok()?,
    ])
}

pub fn format_rgba8(c: [u8; 4]) -> String {
    format!("{},{},{},{}", c[0], c[1], c[2], c[3])
}

fn default_shimmer_highlight(base: [u8; 4]) -> [u8; 4] {
    // Dark: brighten so the band stays visible. Light: lift toward white (sheen).
    let t = if crate::style::is_dark_theme() {
        0.58
    } else {
        0.40
    };
    [
        lerp_u8(base[0], 255, t),
        lerp_u8(base[1], 255, t),
        lerp_u8(base[2], 255, t),
        base[3],
    ]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Mix amount for a glyph at normalized line position `u` (0..=1).
///
/// `spread` is the band half-width as a fraction of the line. Soft edges mimic the
/// CSS gradient used by shadcn `background-clip: text`.
pub fn shimmer_band_mix(u: f32, t: f32, spread: f32, reverse: bool, angle_deg: f32) -> f32 {
    let t = if reverse { 1.0 - t } else { t };
    let shear = (angle_deg / 90.0) * 0.08;
    let u = u + shear;
    let spread = spread.clamp(0.08, 0.9);
    let center = t * (1.0 + 2.0 * spread) - spread;
    let dist = (u - center).abs();
    let raw = (1.0 - (dist / spread).min(1.0)).max(0.0);
    raw * raw
}

pub fn mix_rgba(base: [u8; 4], highlight: [u8; 4], t: f32) -> [u8; 4] {
    [
        lerp_u8(base[0], highlight[0], t),
        lerp_u8(base[1], highlight[1], t),
        lerp_u8(base[2], highlight[2], t),
        lerp_u8(base[3], highlight[3], t),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_hides_unrevealed_clusters() {
        let fx = GlyphFx::new(GlyphFxKind::Split, 0.4);
        assert!(fx.opacity(0, 10) > 0.9);
        assert!(fx.opacity(9, 10) < 0.1);
    }

    #[test]
    fn parse_aliases() {
        assert_eq!(GlyphFxKind::parse("BlurText"), GlyphFxKind::Blur);
        assert_eq!(GlyphFxKind::parse("split"), GlyphFxKind::Split);
        assert_eq!(GlyphFxKind::parse("shimmer"), GlyphFxKind::Shimmer);
    }

    #[test]
    fn shimmer_band_peaks_under_the_sweep() {
        let mid = shimmer_band_mix(0.5, 0.5, 0.25, false, 20.0);
        let edge = shimmer_band_mix(0.0, 0.5, 0.25, false, 20.0);
        assert!(mid > 0.7, "mid={mid}");
        assert!(edge < mid);
        let rev = shimmer_band_mix(0.1, 0.1, 0.2, true, 0.0);
        let fwd = shimmer_band_mix(0.1, 0.1, 0.2, false, 0.0);
        assert!(rev < fwd);
    }

    #[test]
    fn shimmer_once_clamps_progress() {
        let p = ShimmerParams {
            duration_ms: 1000.0,
            count: Some(1.0),
            epoch_s: 0.0,
            ..Default::default()
        };
        assert!((p.progress(0.5) - 0.5).abs() < 1e-4);
        assert_eq!(p.progress(2.0), 1.0);
        assert!(p.finished(1.0));
    }

    #[test]
    fn shimmer_count_two_cycles_then_settles() {
        let p = ShimmerParams {
            duration_ms: 1000.0,
            count: Some(2.0),
            epoch_s: 0.0,
            ..Default::default()
        };
        assert!((p.progress(0.25) - 0.25).abs() < 1e-4);
        assert!((p.progress(1.25) - 0.25).abs() < 1e-4);
        assert!(!p.finished(1.5));
        assert!(p.finished(2.0));
        assert_eq!(p.progress(2.5), 1.0);
    }

    #[test]
    fn shimmer_box_mix_matches_text_falloff() {
        let params = ShimmerParams {
            duration_ms: 1000.0,
            epoch_s: 0.0,
            angle_deg: 0.0,
            ..Default::default()
        };
        let mid = shimmer_box_mix_at(&params, 0.5, 0.5, 200.0);
        let edge = shimmer_box_mix_at(&params, 0.5, 0.0, 200.0);
        let far = shimmer_box_mix_at(&params, 0.5, 1.0, 200.0);
        assert!(mid > 0.7, "mid-cycle center should peak, got {mid}");
        assert!(
            edge < 0.15,
            "edges must fade like glyph shimmer, got {edge}"
        );
        assert!(far < 0.15, "far edge must fade, got {far}");
        let left = shimmer_box_mix_at(&params, 0.5, 0.35, 200.0);
        let right = shimmer_box_mix_at(&params, 0.5, 0.65, 200.0);
        assert!(
            (left - right).abs() < 0.2,
            "falloff should be a soft bell, left={left} right={right}"
        );
    }

    #[test]
    fn shimmer_box_highlight_is_a_white_sheen() {
        let params = ShimmerParams::default();
        let sheen = shimmer_box_highlight_rgba(&params, crate::style::parse_color("secondary"));
        assert!(
            sheen[0] + sheen[1] + sheen[2] > 2.5,
            "box sheen must lighten like text shimmer, got {sheen:?}"
        );
        assert!(sheen[3] >= 0.2 && sheen[3] <= 0.45, "peak alpha {sheen:?}");
    }
}
