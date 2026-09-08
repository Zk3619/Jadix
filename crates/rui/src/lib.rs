// Structural: callback/handler signatures and renderer/layout collectors are
// intentionally dense; refactoring them into newtypes is tracked separately.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod core;
pub mod layout;
pub mod media;
pub mod renderer;
pub mod style;

/// 兼容旧路径 `crate::render::*`。
pub use renderer as render;

pub mod a11y;
pub mod app;
pub mod components;
/// 可选运行时日志（`--features runtime-logging`）：控制台 + `RUI_LOG_FILE`。
pub mod logging;
/// OS hooks: desktop defaults + OS overrides (Windows DWM / PE stack, Unix metrics).
/// 架构与重构方向见 `docs/engine/platform.md`。
pub mod platform;
/// 默认应用入口：`use rui::prelude::*;`（标签 + Builder 双书写面，见规范 §3 / ADR-9）。
pub mod prelude;
pub mod primitives;
/// Text measure (`prepare`/`layout`) + render bridge (`RuiTextRenderer`).
pub mod text;
pub mod tokens;
pub mod utils;

/// Crate-local scenario tests (`pub(crate)` access) in `src/crate_tests/`.
#[cfg(test)]
mod crate_tests;

/// Backward-compatible path for macro-generated code.
pub mod tree {
    pub use crate::core::tree::*;
}

/// Backward-compatible path for existing user code.
pub mod reactivity {
    pub use crate::core::reactivity::*;
    pub use crate::core::resource::{Resource, ResourceState};
}

pub use core::tree::{CursorKind, CursorValue, PathHitPolicy, PointerEvents};
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

/// Join utility class fragments into one space-separated string (ShadCN `cn`, without the cryptic name).
///
/// Skips empty / `None`. `("token", bool)` includes the token only when the flag is true
/// (Solid `classList`). Pass the result to RSX `class={…}` or [`style::with_class_list`](crate::style::with_class_list).
///
/// ```ignore
/// class={classes!("flex gap-2", extra, ("animate-spin", busy))}
/// ```
#[macro_export]
macro_rules! classes {
    () => {
        ::std::string::String::new()
    };
    ($($part:expr),+ $(,)?) => {{
        let mut __rui_classes = ::std::string::String::new();
        $(
            $crate::style::push_class_part(&mut __rui_classes, $part);
        )+
        __rui_classes
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

/// Solid `mergeProps`: later `Some` wins. `#[component]` `*Props` impl [`MergeProps`](crate::app::props::MergeProps).
///
/// ```ignore
/// let props = merge_props!(defaults, incoming, overrides);
/// ```
#[macro_export]
macro_rules! merge_props {
    ($first:expr $(,)?) => {
        $first
    };
    ($first:expr, $($rest:expr),+ $(,)?) => {{
        $crate::app::props::MergeProps::merge_with(
            $first,
            $crate::merge_props!($($rest),+),
        )
    }};
}

/// Solid `splitProps`: take listed fields (`mem::take`), return `(tuple, rest)` for `..rest` spread.
///
/// ```ignore
/// let ((id, class), rest) = split_props!(props, id, class);
/// ```
#[macro_export]
macro_rules! split_props {
    ($props:expr, $($field:ident),+ $(,)?) => {{
        let mut __rui_rest = $props;
        let __rui_local = ($(std::mem::take(&mut __rui_rest.$field),)+);
        (__rui_local, __rui_rest)
    }};
}

pub fn init() {}

/// 将 RGBA8 贴图排队上传到 GPU，供 [`components::Image`](crate::components::Image) 等通过 `texture` 引用。
pub fn queue_image_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    crate::app::pending_images::queue_image_rgba8(id, width, height, rgba);
}

/// Queue a stream frame for [`components::Image`](crate::components::Image) `texture`.
///
/// RGBA8 always. YUV (`Nv12` / `I420` / `P010`) requires `--features image-stream`.
/// Not a decoder or player — see [`media::queue_media_frame`](crate::media::queue_media_frame)
/// and `docs/engine/media.md`.
pub fn queue_media_frame(id: impl Into<String>, frame: crate::media::MediaFrame<'_>) {
    crate::media::queue_media_frame(id, frame);
}

/// 解码图像字节并排队（全格式光栅 + 可选 SVG；见 `svg` feature）。
pub fn queue_image_from_bytes(id: impl Into<String>, bytes: &[u8]) -> bool {
    crate::app::pending_images::queue_image_from_bytes(id, bytes)
}

/// 从本地路径 / `data:` URL 解码并排队。默认禁止 `http(s)`；
/// `features = ["remote-asset"]` 时对远程 URL 调用已安装的 [`RemoteAssetFetcher`]。
pub fn queue_image_from_src(id: impl Into<String>, src: impl AsRef<str>) -> bool {
    crate::app::pending_images::queue_image_from_src(id, src.as_ref())
}

pub fn queue_image_from_path(id: impl Into<String>, path: impl AsRef<str>) -> bool {
    crate::app::pending_images::queue_image_from_path(id, path)
}

/// 解码图像为 RGBA8（不上传 GPU）。
pub fn decode_image_bytes(bytes: &[u8]) -> Option<crate::media::DecodedImage> {
    crate::media::decode_bytes(bytes, crate::media::DecodeOptions::image())
}

/// 从路径或 `data:` URL 解码为 RGBA8（同步；不含 `http(s)`）。
pub fn decode_image_src(src: impl AsRef<str>) -> Option<crate::media::DecodedImage> {
    crate::media::load_src(src.as_ref(), crate::media::DecodeOptions::image())
}

pub use crate::media::{
    clear_remote_asset_fetcher, fetch_remote_bytes, is_remote_src, remote_asset_enabled,
    remote_asset_fetcher_installed, set_remote_asset_fetcher, MediaFrame, MediaPlanes, PixelLayout,
    RemoteAssetFetcher, TextureColorSpace,
};

/// 注册自定义图标名（覆盖 Lucide / [`set_icon_provider`]）。路径、`data:` URL 或内联 SVG。
pub fn register_icon(name: impl Into<String>, source: impl Into<crate::utils::IconSource>) {
    crate::utils::register_icon(name, source)
}

/// 替换自定义 [`IconProvider`](crate::utils::IconProvider)（Lucide 仍为回退；[`register_icon`] 优先）。
pub fn set_icon_provider(provider: std::sync::Arc<dyn crate::utils::IconProvider>) {
    crate::utils::set_icon_provider(provider)
}

pub use crate::utils::{
    clear_icon_provider, clear_registered_icons, icon_named, icon_src, unregister_icon,
    IconProvider, IconSource,
};

/// 图像处理（resize / blur / encode 等），基于 [`image`](https://github.com/image-rs/image)。
pub mod image_ops {
    pub use crate::media::{
        blur, brighten, contrast, crop, encode_jpeg, encode_png, encode_webp, flip_horizontal,
        flip_vertical, grayscale, huerotate, invert, resize, rotate180, rotate270, rotate90,
        to_rgba_image, unsharpen, FilterType,
    };
}

/// 注册自定义光标 RGBA（`cursor: url(#id)` / `url(cursor:id)`）。
pub fn register_cursor_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    crate::style::register_cursor_rgba8(id, width, height, rgba);
}

pub use crate::utils::font::{
    default_ui_mono_family_list, default_ui_sans_family_list, default_ui_serif_family_list,
    set_text_defaults, text_defaults, TextDefaults,
};
pub use app::damage::{
    full_clear_frame_count, partial_load_frame_count, reset_frame_policy_counters,
};
pub use app::pointer_events::{EventPhase, MouseEvent, PointerEvent};
pub use app::props::{
    children, merge_props, resolve_children, Children, ChildrenAccessor, MergeProps,
};
pub use app::{
    mouse_content_pan_enabled, run, run_with, set_mouse_content_pan_enabled, with_ui_build_stack,
    WindowBackdrop, WindowOptions,
};
pub use utils::{
    activate_focus_trap, deactivate_focus_trap, item_height_for_text, sync_focus_trap,
    visible_item_range, FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};
