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
pub mod tokens;
pub mod accesskit_bridge;
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

/// Default stable anchor id for [`app::reactive::reactive_anchor`] / [`app::reactive::reactive_placeholder`]
/// (file + line + column). Prefer explicit ids when you need hot-reload or multi-instance keys.
#[macro_export]
macro_rules! reactive_anchor_id {
    () => {
        concat!(file!(), ":", line!(), ":", column!())
    };
}

/// Sugar over [`crate::app::reactive::reactive_anchor`] + [`crate::app::reactive::reactive_placeholder`]:
/// registers the subtree effect and returns the placeholder element for the same `template_id`.
///
/// - `reactive!(id, closure)` — stable explicit id (hot reload / multi-instance friendly).
/// - `reactive!(closure)` — default id `file:line:column` at the call site (via [`crate::reactive_anchor_id!`]).
#[macro_export]
macro_rules! reactive {
    ($id:expr, $closure:expr) => {{
        $crate::app::reactive::reactive_anchor($id, $closure);
        $crate::app::reactive::reactive_placeholder($id)
    }};
    ($closure:expr) => {{
        {
            let __rui_anchor_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::reactive_anchor(__rui_anchor_id, $closure);
            $crate::app::reactive::reactive_placeholder(__rui_anchor_id)
        }
    }};
}

/// Sugar over [`crate::app::reactive::for_keyed`] for inline list rendering in `ui!`.
///
/// `for_keyed!(scope_id, items_expr, key_of, render)`
#[macro_export]
macro_rules! for_keyed {
    ($scope_id:expr, $items:expr, $key_of:expr, $render:expr) => {{
        let __rui_items = $items;
        $crate::app::reactive::for_keyed($scope_id, &__rui_items, $key_of, $render)
    }};
}

pub fn init() {
}

/// 将 RGBA8 贴图排队上传到 GPU，供 [`components::Image`](crate::components::Image) 等通过 `texture_key` 引用。
pub fn queue_image_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    crate::app::pending_images::queue_image_rgba8(id, width, height, rgba);
}

pub use utils::visible_item_range;
