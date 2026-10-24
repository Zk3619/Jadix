// Structural: callback/handler signatures and renderer/layout collectors are
// intentionally dense; refactoring them into newtypes is tracked separately.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod core;
pub mod layout;
pub mod media;
#[cfg(feature = "net")]
pub mod net;
pub mod motion;
pub mod renderer;
pub mod style;

/// 兼容旧路径 `crate::render::*`。
pub use renderer as render;

#[cfg(feature = "a11y")]
pub mod a11y;
pub mod app;
pub mod components;
#[cfg(feature = "i18n")]
/// Locale overlays, ICU-lite format, TOML catalogs. Off by default; see [`tr_default`].
pub mod i18n;
/// 可选运行时日志（`--features runtime-logging`）：控制台 + `JADIX_LOG_FILE`。
pub mod logging;
/// Optional frame timing and OS resource sampling capability.
#[cfg(feature = "performance")]
pub mod performance;
/// OS hooks + winit event-loop host (Windows / macOS / iOS / Android / Web;
/// HarmonyOS via [`platform::set_external_host`]).
/// 架构见 `docs/engine/platform.md`。
pub mod platform;
/// 默认应用入口：`use jadix::prelude::*;`
///
/// 程序启动：[`prelude::launch`] + [`components::App`]，或 [`app::app`]`.mount`。
/// 组件双书写面见 `docs/COMPONENT_LIBRARY.md` §3 / ADR-9；属性分层见同文档 §1。
/// 兼容 [`run`] / [`run_with`] 在 crate 根保留。
pub mod prelude;
pub mod primitives;
/// Text measure (`prepare`/`layout`) + render bridge (`JadixTextRenderer`).
pub mod text;
pub mod tokens;
pub mod utils;

/// Crate-local scenario tests (`pub(crate)` access) in `src/crate_tests/`.
#[cfg(test)]
mod crate_tests;

// ui! expansions in crate-local tests use the public crate name.
#[cfg(test)]
extern crate self as jadix;
// Native drag regressions mount the Gallery source, including every provider.
#[cfg(all(test, feature = "gallery-tests"))]
#[path = "../../../examples/gallery/src/imports.rs"]
mod imports;
#[cfg(all(test, feature = "gallery-tests"))]
#[allow(dead_code, non_snake_case)]
#[path = "../../../examples/gallery/src/pages/mod.rs"]
mod pages;
#[cfg(all(test, feature = "gallery-tests"))]
#[allow(dead_code, non_snake_case)]
#[path = "../../../examples/gallery/src/shared.rs"]
mod shared;
#[cfg(all(test, feature = "gallery-tests"))]
pub(crate) fn gallery_icon() -> crate::app::AppIcon {
    crate::app::AppIcon::from_bytes(include_bytes!("../../../examples/gallery/assets/app-icon.png"))
        .expect("bundled Gallery icon")
}
#[cfg(all(test, feature = "gallery-tests"))]
pub(crate) mod gallery_drag {
    pub(crate) use super::pages::drag::*;
}
#[cfg(all(test, feature = "gallery-tests"))]
#[allow(dead_code, non_snake_case)]
#[path = "../../../examples/gallery/src/app.rs"]
mod gallery_app;
#[cfg(all(test, feature = "gallery-tests"))]
#[allow(dead_code)]
#[path = "../../../examples/gallery/src/styles.rs"]
mod styles;

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
pub use jadix_macros::*;

/// `true` when `src/i18n` (catalogs / locale overlay) is compiled in.
pub const fn i18n_enabled() -> bool {
    cfg!(feature = "i18n")
}

/// `{name}` substitution for [`tr_with_default`] when feature `i18n` is off.
pub(crate) fn i18n_interpolate(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('}') {
            let name = &rest[..end];
            if let Some((_, v)) = args.iter().find(|(n, _)| *n == name) {
                out.push_str(v);
            } else {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
            rest = &rest[end + 1..];
        } else {
            out.push('{');
            out.push_str(rest);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// App chrome: look up `key`, or use the English source string.
///
/// With `--features i18n` this registers **en-US** then translates. Without the
/// feature it returns `en_us`. Jadix widgets that ship Chinese use [`tr_default_zh`].
/// Optional third locale at the call site: [`tr_default!`].
pub fn tr_default(key: &str, en_us: &str) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::tr_default(key, en_us)
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = key;
        String::from(en_us)
    }
}

/// Jadix component chrome: register **en-US** and **zh-CN**, then look up.
pub fn tr_default_zh(key: &str, en_us: &str, zh_cn: &str) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::tr_default_zh(key, en_us, zh_cn)
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = key;
        String::from(if chrome_locale().starts_with("zh") { zh_cn } else { en_us })
    }
}

/// [`tr_default`] with named `{placeholder}` args.
pub fn tr_with_default(key: &str, en_us: &str, args: &[(&str, &str)]) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::tr_with_default(key, en_us, args)
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = key;
        i18n_interpolate(en_us, args)
    }
}

/// [`tr_default_zh`] with named `{placeholder}` args.
pub fn tr_with_default_zh(key: &str, en_us: &str, zh_cn: &str, args: &[(&str, &str)]) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::tr_with_default_zh(key, en_us, zh_cn, args)
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = key;
        i18n_interpolate(if chrome_locale().starts_with("zh") { zh_cn } else { en_us }, args)
    }
}

thread_local! {
    static CHROME_LOCALE: std::cell::RefCell<String> = std::cell::RefCell::new(
        std::env::var("JADIX_LOCALE").unwrap_or_else(|_| "en-US".into()).to_ascii_lowercase()
    );
}

/// Lightweight component language, available without the full i18n feature.
pub fn chrome_locale() -> String {
    #[cfg(feature = "i18n")]
    { crate::i18n::locale().to_string() }
    #[cfg(not(feature = "i18n"))]
    { CHROME_LOCALE.with(|locale| locale.borrow().clone()) }
}

/// Sets component language; with i18n enabled this also selects the catalog locale.
pub fn set_chrome_locale(tag: &str) {
    CHROME_LOCALE.with(|locale| *locale.borrow_mut() = tag.to_ascii_lowercase());
    #[cfg(feature = "i18n")]
    crate::i18n::set_locale(tag);
    crate::app::runtime::request_redraw_all();
}

/// First weekday: 0 = Sunday. With `i18n`, follows CLDR / ICU for the active locale.
pub fn first_weekday() -> u32 {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::first_weekday()
    }
    #[cfg(not(feature = "i18n"))]
    {
        0
    }
}

/// `true` when the active locale is RTL (Arabic, Hebrew, …).
pub fn chrome_rtl() -> bool {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::chrome_rtl()
    }
    #[cfg(not(feature = "i18n"))]
    {
        false
    }
}

/// Chevron name for previous/next chrome (flips in RTL).
pub(crate) fn chrome_nav_chevron(previous: bool) -> &'static str {
    match (chrome_rtl(), previous) {
        (false, true) | (true, false) => "chevron-left",
        (false, false) | (true, true) => "chevron-right",
    }
}

#[cfg(not(feature = "i18n"))]
const ENGLISH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

#[cfg(not(feature = "i18n"))]
fn english_month(month: u32) -> &'static str {
    ENGLISH_MONTHS[month.saturating_sub(1).min(11) as usize]
}

/// Long date in the active locale (`August 14, 2026` / `2026年8月14日`).
pub fn format_date_long(year: i32, month: u32, day: u32) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::format_date(year, month, day, crate::i18n::DateStyle::Long)
    }
    #[cfg(not(feature = "i18n"))]
    {
        format!("{} {day}, {year}", english_month(month))
    }
}

/// Year-month caption in the active locale (`August 2026` / `2026年8月`).
pub fn format_year_month(year: i32, month: u32) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::format_year_month(year, month)
    }
    #[cfg(not(feature = "i18n"))]
    {
        format!("{} {year}", english_month(month))
    }
}

/// Date + time in the active locale (no timezone conversion).
pub fn format_datetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    #[cfg(feature = "i18n")]
    {
        crate::i18n::format_datetime(year, month, day, hour, minute, second)
    }
    #[cfg(not(feature = "i18n"))]
    {
        format!(
            "{}, {hour:02}:{minute:02}:{second:02}",
            format_date_long(year, month, day)
        )
    }
}

/// `tr_default!("key", "English")` or `tr_default!("key", "English", "中文")`.
#[macro_export]
macro_rules! tr_default {
    ($key:expr, $en:expr $(,)?) => {
        $crate::tr_default($key, $en)
    };
    ($key:expr, $en:expr, $zh:expr $(,)?) => {
        $crate::tr_default_zh($key, $en, $zh)
    };
}

/// `tr_with_default!("key", "Hello {name}", args)` or with a zh-CN template before `args`.
#[macro_export]
macro_rules! tr_with_default {
    ($key:expr, $en:expr, $args:expr $(,)?) => {
        $crate::tr_with_default($key, $en, $args)
    };
    ($key:expr, $en:expr, $zh:expr, $args:expr $(,)?) => {
        $crate::tr_with_default_zh($key, $en, $zh, $args)
    };
}

/// Translate a catalog key (`t!("save")` / `t!("hello", name = user)`).
///
/// Requires `--features i18n` for overlay lookup. Without the feature, returns
/// the key (or interpolates `{name}` in the key string).
#[macro_export]
macro_rules! t {
    ($key:expr) => {{
        #[cfg(feature = "i18n")]
        {
            $crate::i18n::tr($key)
        }
        #[cfg(not(feature = "i18n"))]
        {
            ::std::string::String::from($key)
        }
    }};
    ($key:expr, $($name:ident = $val:expr),+ $(,)?) => {{
        let __jadix_i18n_vals = [$($crate::__t_arg($val)),+];
        let __jadix_i18n_names = [$(stringify!($name)),+];
        let __jadix_i18n_pairs: ::std::vec::Vec<(&str, &str)> = __jadix_i18n_names
            .iter()
            .zip(__jadix_i18n_vals.iter())
            .map(|(n, v)| (*n, v.as_str()))
            .collect();
        #[cfg(feature = "i18n")]
        {
            $crate::i18n::tr_with($key, &__jadix_i18n_pairs)
        }
        #[cfg(not(feature = "i18n"))]
        {
            let _ = __jadix_i18n_pairs;
            ::std::string::String::from($key)
        }
    }};
}

#[doc(hidden)]
pub fn __t_arg(v: impl ToString) -> String {
    v.to_string()
}

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
            let __jadix_anchor_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::reactive_view(__jadix_anchor_id, $closure)
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
        let mut __jadix_classes = ::std::string::String::new();
        $(
            $crate::style::push_class_part(&mut __jadix_classes, $part);
        )+
        __jadix_classes
    }};
}

/// Sugar over [`crate::app::reactive::for_keyed`] for inline list rendering in `ui!`.
///
/// `for_keyed!(scope_id, items_expr, key_of, render)`
#[macro_export]
macro_rules! for_keyed {
    ($scope_id:expr, $items:expr, $key_of:expr, $render:expr) => {{
        let __jadix_items = $items;
        $crate::app::reactive::for_keyed($scope_id, &__jadix_items, $key_of, $render)
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
            let __jadix_bind_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::bind_text(__jadix_bind_id, $closure)
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
            let __jadix_bind_id: &'static str = concat!(file!(), ":", line!(), ":", column!());
            $crate::app::reactive::bind_style($el, __jadix_bind_id, $closure)
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
        let mut __jadix_rest = $props;
        let __jadix_local = ($(std::mem::take(&mut __jadix_rest.$field),)+);
        (__jadix_local, __jadix_rest)
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

pub use crate::layout::dimension::{css_root_font_size_px, em, px, rem};
pub use crate::utils::font::{
    default_ui_mono_family_list, default_ui_sans_family_list, default_ui_serif_family_list,
    root_font_size_px, set_root_font_size, set_text_defaults, text_defaults, TextDefaults,
};
pub use app::damage::{
    arm_policy_snapshot, full_clear_frame_count, partial_load_frame_count,
    reset_frame_policy_counters, suppress_policy_counts, take_policy_snapshot, PolicySnapshot,
};
pub use app::frame_stats::{
    last_patched_dl, last_scene_batches, last_scene_uploads, last_skipped_siblings,
    patched_dl_count, reset_collect_skip_counters, skipped_siblings_count, SceneBatches,
    SceneUploads,
};
pub use app::pointer_events::{EventPhase, MouseEvent, PointerEvent};
pub use app::props::{
    children, merge_props, resolve_children, Children, ChildrenAccessor, MergeProps,
};
#[cfg(target_os = "android")]
pub use app::run_android;
#[cfg(target_env = "ohos")]
pub use app::run_ohos;
pub use app::{
    launch, mouse_content_pan_enabled, page_scroll_locked, parse_scroll_freeze, run, run_with,
    set_frame_rate_limit, set_mouse_content_pan_enabled, set_page_scroll_locked, set_scroll_freeze,
    with_ui_build_stack, ScrollFreeze, WindowBackdrop, WindowOptions,
};
pub use utils::{
    activate_focus_trap, deactivate_focus_trap, item_height_for_text, sync_focus_trap,
    visible_item_range, FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};

#[cfg(test)]
mod tr_default_shim_tests {
    #[test]
    fn yields_source_string() {
        assert_eq!(
            crate::tr_default("jadix.pagination.previous", "Previous"),
            "Previous"
        );
    }

    #[test]
    fn interpolates_placeholders() {
        assert_eq!(
            crate::tr_with_default(
                "jadix.date_picker.week_of",
                "Week of {date}",
                &[("date", "August 14, 2026")]
            ),
            "Week of August 14, 2026"
        );
    }

    #[test]
    fn english_source_is_used_by_default() {
        assert_eq!(crate::tr_default("jadix.pagination.next", "Next"), "Next");
        assert_eq!(
            crate::tr_default_zh("jadix.pagination.next", "Next", "下一页"),
            "Next"
        );
        assert_eq!(
            crate::tr_default!("jadix.pagination.previous", "Previous", "上一页"),
            "Previous"
        );
    }
}
