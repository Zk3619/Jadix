//! Elevation shadows and overlay scrims.
//!
//! Product chrome must not scatter `shadow([0,0,0,α], blur)` or `bg([0,0,0,α])`.
//! Ink is display-black (shadcn `rgb(0 0 0 / α)`), so light/dark both dim rather than
//! inverting to a glow when `foreground` is near-white.

/// Elevation rungs used by floating chrome (card → dialog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elevation {
    /// Hairline / input chrome (~0.06 α).
    Xs,
    /// Card (~0.08 α).
    Sm,
    /// Popover / floating (~0.12 α).
    Md,
    /// Menu (~0.14 α).
    Lg,
    /// Dialog / sheet / command palette (~0.16 α).
    Xl,
}

impl Elevation {
    pub const fn alpha(self) -> f32 {
        match self {
            Self::Xs => 0.06,
            Self::Sm => 0.08,
            Self::Md => 0.12,
            Self::Lg => 0.14,
            Self::Xl => 0.16,
        }
    }

    /// Default blur in px for this rung (call sites may pass a tighter/looser blur).
    pub const fn blur(self) -> f32 {
        match self {
            Self::Xs => 1.0,
            Self::Sm => 4.0,
            Self::Md => 10.0,
            Self::Lg => 12.0,
            Self::Xl => 16.0,
        }
    }
}

/// Shadow color for `Element::shadow` (black × elevation alpha).
pub const fn shadow_rgba(e: Elevation) -> [f32; 4] {
    shadow_ink(e.alpha())
}

/// Black ink at an arbitrary alpha (thumb / `shadow-xs` chrome that is not a named rung).
pub const fn shadow_ink(alpha: f32) -> [f32; 4] {
    [
        0.0,
        0.0,
        0.0,
        if alpha < 0.0 {
            0.0
        } else if alpha > 1.0 {
            1.0
        } else {
            alpha
        },
    ]
}

pub const fn shadow_blur(e: Elevation) -> f32 {
    e.blur()
}

/// `(color, blur)` pair matching [`shadow_rgba`] / [`shadow_blur`].
pub const fn elevation_shadow(e: Elevation) -> ([f32; 4], f32) {
    (shadow_rgba(e), shadow_blur(e))
}

/// Overlay dimmer: black × `alpha` (Navigator sheet scrim, etc.).
pub const fn overlay_scrim(alpha: f32) -> [f32; 4] {
    shadow_ink(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_alphas_match_legacy_chrome() {
        assert!((shadow_rgba(Elevation::Xs)[3] - 0.06).abs() < 1e-6);
        assert!((shadow_rgba(Elevation::Sm)[3] - 0.08).abs() < 1e-6);
        assert!((shadow_rgba(Elevation::Md)[3] - 0.12).abs() < 1e-6);
        assert!((shadow_rgba(Elevation::Lg)[3] - 0.14).abs() < 1e-6);
        assert!((shadow_rgba(Elevation::Xl)[3] - 0.16).abs() < 1e-6);
        assert_eq!(shadow_rgba(Elevation::Md)[0], 0.0);
    }

    #[test]
    fn overlay_scrim_clamps_alpha() {
        assert!((overlay_scrim(0.35)[3] - 0.35).abs() < 1e-6);
        assert_eq!(overlay_scrim(-1.0)[3], 0.0);
        assert_eq!(overlay_scrim(2.0)[3], 1.0);
    }
}
