pub mod renderer;
pub mod style;
pub mod core;
pub mod layout;

/// 兼容旧路径 `crate::render::*`。
pub use renderer as render;

/// 兼容旧路径 `crate::text::RuiTextRenderer`。
pub mod text {
    pub use crate::renderer::RuiTextRenderer;
}
pub mod components;
pub mod primitives;
pub mod app;
pub mod utils;

/// Backward-compatible path for macro-generated code.
pub mod tree {
    pub use crate::core::tree::*;
}

/// Backward-compatible path for existing user code.
pub mod reactivity {
    pub use crate::core::reactivity::*;
}

pub use rui_macros::*;
pub use core::tree::CursorKind;

pub fn init() {
}
