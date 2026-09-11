//! Author-facing anchored-overlay motion kind.
//!
//! Leaf module: no engine import. Fade/expand implementation is
//! [`crate::utils::floating_motion`] `engine`; authors should use [`crate::utils::anchored_overlay`].

/// Re-exported by [`crate::utils::anchored_overlay`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatingMotionKind {
    #[default]
    Fade,
    Expand,
}
