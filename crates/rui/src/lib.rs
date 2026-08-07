// Structural: callback/handler signatures and renderer/layout collectors are
// intentionally dense; refactoring them into newtypes is tracked separately.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod core;
pub mod layout;
pub mod renderer;
pub mod style;

/// 兼容旧路径 `crate::render::*`。
pub use renderer as render;

pub mod accesskit_bridge;
pub mod app;
pub mod components;
pub mod primitives;
/// Text measure (`prepare`/`layout`) + render bridge (`RuiTextRenderer`).
pub mod text;
pub mod tokens;
pub mod utils;

/// Crate-local tests live under `crates/rui/test/` and compile inside the library
/// (via `#[path]`) so they can access `pub(crate)` helpers.
#[cfg(test)]
#[path = "../test/reactive.rs"]
mod reactive_tests;

#[cfg(test)]
#[path = "../test/components_reactive_sync.rs"]
mod components_reactive_sync_tests;

#[cfg(test)]
#[path = "../test/bind_text.rs"]
mod bind_text_tests;

#[cfg(test)]
#[path = "../test/layout_mark_dirty.rs"]
mod layout_mark_dirty_tests;

#[cfg(test)]
#[path = "../test/headless_layout.rs"]
mod headless_layout_tests;

/// Backward-compatible path for macro-generated code.
pub mod tree {
    pub use crate::core::tree::*;
}

/// Backward-compatible path for existing user code.
pub mod reactivity {
    pub use crate::core::reactivity::*;
    pub use crate::core::resource::{Resource, ResourceState};
}

pub use core::tree::CursorKind;
pub use rui_macros::*;

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
        $crate::app::reactive::reactive_view($id, $closure)
    }};
    ($closure:expr) => {{
        {
            let __rui_anchor_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::reactive_view(__rui_anchor_id, $closure)
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

/// Sugar over [`crate::app::reactive::bind_text`] for Solid-style in-place text updates.
///
/// - `bind_text!(id, || expr)` — stable explicit id
/// - `bind_text!(|| expr)` — default id `file:line:column`
#[macro_export]
macro_rules! bind_text {
    ($id:expr, $closure:expr) => {{
        $crate::app::reactive::bind_text($id, $closure)
    }};
    ($closure:expr) => {{
        {
            let __rui_bind_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::bind_text(__rui_bind_id, $closure)
        }
    }};
}

/// Sugar over [`crate::app::reactive::bind_attr`] / [`bind_prop`](crate::app::reactive::bind_prop).
#[macro_export]
macro_rules! bind_attr {
    ($el:expr, $id:expr, $compute:expr, $apply:expr) => {{
        $crate::app::reactive::bind_attr($el, $id, $compute, $apply)
    }};
}

/// Sugar over [`crate::app::reactive::bind_style`].
///
/// - `bind_style!(el, id, || StylePatch { … })`
/// - `bind_style!(el, || StylePatch { … })` — id = `file:line:column`
#[macro_export]
macro_rules! bind_style {
    ($el:expr, $id:expr, $closure:expr) => {{
        $crate::app::reactive::bind_style($el, $id, $closure)
    }};
    ($el:expr, $closure:expr) => {{
        {
            let __rui_bind_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::bind_style($el, __rui_bind_id, $closure)
        }
    }};
}

pub fn init() {}

/// 将 RGBA8 贴图排队上传到 GPU，供 [`components::Image`](crate::components::Image) 等通过 `texture_key` 引用。
pub fn queue_image_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    crate::app::pending_images::queue_image_rgba8(id, width, height, rgba);
}

pub use utils::visible_item_range;
