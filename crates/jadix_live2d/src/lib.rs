//! Opt-in Live2D-compatible runtime with the same GPU stack as Jadix.
//!
//! The portable backend is Mocari, not the official Cubism Core. See the crate
//! README for compatibility limits and model licensing requirements.
#![forbid(unsafe_code)]

pub use mocari::{assets, core, error, expression, json, moc3, motion, runtime};
pub use mocari::{ExpressionManager, HitAreaInfo, ModelRuntime, MotionPlayer, ParameterInfo};
mod player;
pub mod render;
pub use player::{Error, Player, Result};
mod view;
pub use view::View;
