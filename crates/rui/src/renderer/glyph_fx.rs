//! Glyph / cluster-level motion (B17). Applied in prepare, not by splitting DOM spans.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphFxKind {
    None,
    Blur,
    Split,
    Scramble,
    CountUp,
    Dissolve,
}

impl GlyphFxKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "blur" | "blurtext" => Self::Blur,
            "split" | "splittext" => Self::Split,
            "scramble" | "decrypt" => Self::Scramble,
            "countup" | "count" => Self::CountUp,
            "dissolve" | "particle" => Self::Dissolve,
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

pub const ATTR_GLYPH_FX: &str = "rui_glyph_fx";

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
    }
}
