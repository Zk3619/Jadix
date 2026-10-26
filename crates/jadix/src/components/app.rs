//! # App
//!
//! 程序文档壳：窗口 chrome + CSS 根字号（ADR-9 与 [`app()`](crate::app::app) 同字段）。
//!
//! **RSX 推荐**：[`launch`](crate::app::launch) + `<App …>` — 配置写在标签上。
//!
//! **Builder 推荐**：[`app`](crate::app::app) + `.mount` — 配置写在 builder，内容在 `.mount`。
//!
//! 创建期 chrome（`frameless` / `transparent` / `blur` / `devtools`）在 builder `.mount` 最可靠；
//! `<App>` 会立即应用字体 / rem，并在窗口已存在时 live 更新标题与客户区尺寸。

use crate::app::entry::{materialize_app_shell, AppConfig};
use crate::app::WindowBackdrop;
use crate::core::tree::Element;
use crate::utils::take_packed;
use jadix_macros::component;

/// RSX `<App>` — same fields as [`crate::app::app()`].
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn App(
    title: Option<String>,
    icon: Option<crate::app::AppIcon>,
    window_label: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    min_width: Option<f64>,
    min_height: Option<f64>,
    max_width: Option<f64>,
    max_height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
    resizable: Option<bool>,
    resize_border: Option<f64>,
    maximized: Option<bool>,
    fullscreen: Option<bool>,
    fullscreen_shortcuts: Option<bool>,
    close_with_primary: Option<bool>,
    always_on_top: Option<bool>,
    decorations: Option<bool>,
    frameless: Option<bool>,
    transparent: Option<bool>,
    blur: Option<bool>,
    backdrop: Option<String>,
    blur_amount: Option<f64>,
    glass_tint: Option<String>,
    glass_tint_dark: Option<String>,
    luminosity_opacity: Option<f64>,
    aero_color_balance: Option<f64>,
    aero_afterglow_balance: Option<f64>,
    aero_blur_balance: Option<f64>,
    aero_afterglow: Option<String>,
    aero_reflection: Option<f64>,
    aero_parallax: Option<f64>,
    font: Option<String>,
    font_family: Option<String>,
    font_size: Option<f64>,
    font_weight: Option<u16>,
    synthetic_bold: Option<bool>,
    opacity_groups: Option<bool>,
    locale: Option<String>,
    lang: Option<String>,
    i18n: Option<String>,
    timezone: Option<String>,
    devtools: Option<bool>,
    children: Option<Element>,
) -> Element {
    let mut config = AppConfig {
        title,
        icon,
        window_label,
        inner_width: width,
        inner_height: height,
        min_size: min_width.zip(min_height).map(|(w, h)| [w, h]),
        max_size: max_width.zip(max_height).map(|(w, h)| [w, h]),
        position: x.zip(y).map(|(x, y)| [x, y]),
        resizable,
        resize_border,
        maximized,
        fullscreen,
        fullscreen_shortcuts,
        close_with_primary,
        instance_policy: None,
        always_on_top,
        decorations,
        transparent,
        blur,
        backdrop: backdrop.map(|s| WindowBackdrop::from(s.as_str())),
        blur_amount: blur_amount.map(|v| v as f32),
        glass_tint: glass_tint.and_then(|s| crate::app::glass_params::parse_glass_tint(&s)),
        glass_tint_dark: glass_tint_dark
            .and_then(|s| crate::app::glass_params::parse_glass_tint(&s)),
        luminosity_opacity: luminosity_opacity.map(|v| v as f32),
        aero_color_balance: aero_color_balance.map(|v| v as f32),
        aero_afterglow_balance: aero_afterglow_balance.map(|v| v as f32),
        aero_blur_balance: aero_blur_balance.map(|v| v as f32),
        aero_afterglow: aero_afterglow.and_then(|s| crate::app::glass_params::parse_glass_tint(&s)),
        aero_reflection: aero_reflection.map(|v| v as f32),
        aero_parallax: aero_parallax.map(|v| v as f32),
        font_family: font_family.or(font),
        font_size: font_size.map(|v| v as f32),
        font_weight,
        synthetic_bold,
        opacity_groups,
        locale: locale.or(lang),
        i18n,
        timezone,
        #[cfg(all(debug_assertions, feature = "devtools"))]
        devtools,
    };
    #[cfg(not(all(debug_assertions, feature = "devtools")))]
    {
        let _ = devtools;
    }
    if frameless == Some(true) {
        config.decorations = Some(false);
    }
    let kids = children.map(take_packed).unwrap_or_default();
    materialize_app_shell(config, kids)
}
