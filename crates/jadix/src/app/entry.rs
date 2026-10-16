//! Program entry: [`launch`] (RSX) and [`app`]`.mount` (Builder).
//!
//! | 推荐 | 入口 | 配置 |
//! |------|------|------|
//! | RSX | [`launch`] | [`crate::components::App`] 属性 |
//! | Builder | [`app`]`.mount` | builder 链 |
//!
//! ```ignore
//! launch(|| ui! {
//!     <App title="My App" font_size={18}>
//!         <Button>"保存"</Button>
//!     </App>
//! });
//!
//! app().title("My App").font_size(18).mount(|| ui! {
//!     <Button>"保存"</Button>
//! });
//! ```
//!
//! 兼容：[`super::run`] / [`super::run_with`]、[`AppBuilder::run`]（整棵原始树）、[`crate::main`]。

use crate::core::tree::Element;
use crate::utils::font::{set_text_defaults, text_defaults};
use crate::utils::take_packed;

use super::window_chrome::{WindowBackdrop, WindowOptions};

/// Document / window patch. Unset fields keep current process / [`WindowOptions::default`] values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppConfig {
    pub title: Option<String>,
    pub window_label: Option<String>,
    pub inner_width: Option<f64>,
    pub inner_height: Option<f64>,
    pub decorations: Option<bool>,
    pub transparent: Option<bool>,
    pub blur: Option<bool>,
    pub backdrop: Option<WindowBackdrop>,
    pub blur_amount: Option<f32>,
    pub glass_tint: Option<[u8; 4]>,
    pub glass_tint_dark: Option<[u8; 4]>,
    pub luminosity_opacity: Option<f32>,
    pub aero_color_balance: Option<f32>,
    pub aero_afterglow_balance: Option<f32>,
    pub aero_blur_balance: Option<f32>,
    pub aero_afterglow: Option<[u8; 4]>,
    pub aero_reflection: Option<f32>,
    pub aero_parallax: Option<f32>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub synthetic_bold: Option<bool>,
    /// BCP 47 locale (`zh-CN`). With feature `i18n`, becomes the active locale
    /// for this window (`signal_key_scope`); primary window also sets the process default.
    pub locale: Option<String>,
    /// Path to `i18n.toml` (optional `dir = "locales"`). Loaded with feature `i18n`.
    pub i18n: Option<String>,
    /// IANA timezone (`Asia/Shanghai`). Separate from the message catalog.
    pub timezone: Option<String>,
    #[cfg(feature = "devtools")]
    pub devtools: Option<bool>,
}

impl AppConfig {
    pub fn into_window_options(self) -> WindowOptions {
        let mut o = WindowOptions::default();
        if let Some(title) = self.title {
            o.title = title;
        }
        o.window_label = self.window_label;
        if self.inner_width.is_some() {
            o.inner_width = self.inner_width;
        }
        if self.inner_height.is_some() {
            o.inner_height = self.inner_height;
        }
        if let Some(d) = self.decorations {
            o.decorations = d;
        }
        if let Some(t) = self.transparent {
            o.transparent = t;
        }
        if let Some(b) = self.blur {
            o.blur = b;
        }
        if let Some(bd) = self.backdrop {
            o.backdrop = bd;
        }
        if let Some(v) = self.blur_amount {
            o.blur_amount = Some(v);
        }
        if let Some(v) = self.glass_tint {
            o.glass_tint = Some(v);
        }
        if let Some(v) = self.glass_tint_dark {
            o.glass_tint_dark = Some(v);
        }
        if let Some(v) = self.luminosity_opacity {
            o.luminosity_opacity = Some(v);
        }
        if let Some(v) = self.aero_color_balance {
            o.aero_color_balance = Some(v);
        }
        if let Some(v) = self.aero_afterglow_balance {
            o.aero_afterglow_balance = Some(v);
        }
        if let Some(v) = self.aero_blur_balance {
            o.aero_blur_balance = Some(v);
        }
        if let Some(v) = self.aero_afterglow {
            o.aero_afterglow = Some(v);
        }
        if let Some(v) = self.aero_reflection {
            o.aero_reflection = Some(v);
        }
        if let Some(v) = self.aero_parallax {
            o.aero_parallax = Some(v);
        }
        o.default_font_family = self.font_family;
        o.default_font_size = self.font_size;
        o.default_font_weight = self.font_weight;
        o.synthetic_bold = self.synthetic_bold;
        #[cfg(feature = "devtools")]
        if let Some(d) = self.devtools {
            o.devtools = d;
        }
        o
    }

    /// Apply font / rem root (always) and live window title / size when a window exists.
    pub fn apply_document(&self) {
        let mut d = text_defaults();
        let mut font_changed = false;
        if let Some(ref family) = self.font_family {
            if d.family.as_deref() != Some(family.as_str()) {
                d.family = Some(family.clone());
                font_changed = true;
            }
        }
        if let Some(size) = self.font_size {
            let size = size.max(1.0);
            if (d.size_px - size).abs() > 1e-6 {
                d.size_px = size;
                font_changed = true;
            }
        }
        if let Some(weight) = self.font_weight {
            let weight = weight.clamp(100, 900);
            if d.weight != weight {
                d.weight = weight;
                font_changed = true;
            }
        }
        if let Some(synth) = self.synthetic_bold {
            if d.synthetic_bold != synth {
                d.synthetic_bold = synth;
                font_changed = true;
            }
        }
        if font_changed {
            set_text_defaults(d);
        }

        if let Some(ref path) = self.i18n {
            apply_i18n_file(path);
        }
        if let Some(ref loc) = self.locale {
            #[cfg(feature = "i18n")]
            crate::i18n::set_locale(loc.as_str());
            #[cfg(not(feature = "i18n"))]
            let _ = loc;
        }
        if let Some(ref tz) = self.timezone {
            #[cfg(feature = "i18n")]
            crate::i18n::set_timezone(tz.as_str());
            #[cfg(not(feature = "i18n"))]
            let _ = tz;
        }

        if let Some(ref title) = self.title {
            super::window_chrome::set_window_title(title);
        }
        match (self.inner_width, self.inner_height) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                super::window_chrome::request_window_inner_size(w, h);
            }
            _ => {}
        }
    }
}

fn apply_i18n_file(path: &str) {
    #[cfg(feature = "i18n")]
    {
        use std::sync::Mutex;
        static LAST: Mutex<Option<String>> = Mutex::new(None);
        let mut g = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_deref() == Some(path) {
            return;
        }
        *g = Some(path.to_string());
        drop(g);
        if let Err(e) = crate::i18n::load(path) {
            log::warn!("i18n: failed to load {path}: {e}");
        }
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = path;
    }
}

/// Fluent program entry (`app().title(…).font_size(…).mount(…)`).
#[derive(Default)]
pub struct AppBuilder {
    config: AppConfig,
    children: Vec<Element>,
}

/// Start an [`AppBuilder`]. Equivalent RSX tag is [`crate::components::App`].
pub fn app() -> AppBuilder {
    AppBuilder::new()
}

/// **RSX 程序入口**：启动事件循环，每帧执行 `root` 得到 UI 树。
///
/// 把窗口 / 根字号写在 [`crate::components::App`] 上；Builder 侧用 [`app()`]`.mount`。
pub fn launch<F: FnMut() -> Element + 'static>(root: F) {
    super::run(root);
}

impl AppBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.config.title = Some(title.into());
        self
    }

    pub fn window_label(mut self, label: impl Into<String>) -> Self {
        let s = label.into();
        self.config.window_label = if s.trim().is_empty() { None } else { Some(s) };
        self
    }

    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.config.inner_width = Some(width);
        self.config.inner_height = Some(height);
        self
    }

    pub fn width(mut self, width: f64) -> Self {
        self.config.inner_width = Some(width);
        self
    }

    pub fn height(mut self, height: f64) -> Self {
        self.config.inner_height = Some(height);
        self
    }

    pub fn decorations(mut self, decorations: bool) -> Self {
        self.config.decorations = Some(decorations);
        self
    }

    pub fn frameless(self) -> Self {
        self.decorations(false)
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.config.transparent = Some(transparent);
        self
    }

    pub fn blur(mut self, blur: bool) -> Self {
        self.config.blur = Some(blur);
        self
    }

    pub fn backdrop(mut self, backdrop: impl Into<WindowBackdrop>) -> Self {
        self.config.backdrop = Some(backdrop.into());
        self
    }

    pub fn blur_amount(mut self, amount: f32) -> Self {
        self.config.blur_amount = Some(amount);
        self
    }

    pub fn glass_tint(mut self, tint: impl Into<super::window_chrome::GlassTintArg>) -> Self {
        self.config.glass_tint = tint.into().0;
        self
    }

    pub fn glass_tint_dark(mut self, tint: impl Into<super::window_chrome::GlassTintArg>) -> Self {
        self.config.glass_tint_dark = tint.into().0;
        self
    }

    pub fn luminosity_opacity(mut self, opacity: f32) -> Self {
        self.config.luminosity_opacity = Some(opacity);
        self
    }

    pub fn aero_color_balance(mut self, v: f32) -> Self {
        self.config.aero_color_balance = Some(v);
        self
    }

    pub fn aero_afterglow_balance(mut self, v: f32) -> Self {
        self.config.aero_afterglow_balance = Some(v);
        self
    }

    pub fn aero_blur_balance(mut self, v: f32) -> Self {
        self.config.aero_blur_balance = Some(v);
        self
    }

    pub fn aero_afterglow(mut self, tint: impl Into<super::window_chrome::GlassTintArg>) -> Self {
        self.config.aero_afterglow = tint.into().0;
        self
    }

    pub fn aero_reflection(mut self, intensity: f32) -> Self {
        self.config.aero_reflection = Some(intensity);
        self
    }

    pub fn aero_parallax(mut self, amount: f32) -> Self {
        self.config.aero_parallax = Some(amount);
        self
    }

    /// CSS `font-family` list (document default).
    pub fn font(mut self, family: impl Into<String>) -> Self {
        self.config.font_family = Some(family.into());
        self
    }

    /// CSS root / `1rem` / unspecified text size, logical px (builder numbers are px).
    pub fn font_size(mut self, px: impl Into<f64>) -> Self {
        self.config.font_size = Some((px.into() as f32).max(1.0));
        self
    }

    pub fn font_weight(mut self, weight: u16) -> Self {
        self.config.font_weight = Some(weight.clamp(100, 900));
        self
    }

    pub fn synthetic_bold(mut self, enabled: bool) -> Self {
        self.config.synthetic_bold = Some(enabled);
        self
    }

    /// BCP 47 locale (`zh-CN` / `ar`).
    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.config.locale = Some(locale.into());
        self
    }

    /// Alias of [`Self::locale`] (`lang="zh-CN"`).
    pub fn lang(self, locale: impl Into<String>) -> Self {
        self.locale(locale)
    }

    /// Load `i18n.toml` (and `dir = "locales"` if set). Requires feature `i18n`.
    pub fn i18n(mut self, path: impl Into<String>) -> Self {
        self.config.i18n = Some(path.into());
        self
    }

    /// IANA timezone (`Asia/Shanghai`). Requires feature `i18n`; not a catalog key.
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.config.timezone = Some(tz.into());
        self
    }

    /// Enable runtime Inspector (F12). Requires crate feature `devtools`.
    #[cfg(feature = "devtools")]
    pub fn devtools(mut self, enabled: bool) -> Self {
        self.config.devtools = Some(enabled);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Element>,
    {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn into_window_options(self) -> WindowOptions {
        self.config.into_window_options()
    }

    /// 兼容：用本 builder 的 [`WindowOptions`] 启动，闭包返回**整棵**原始树（不自动包文档壳）。
    ///
    /// 新代码请用 [`Self::mount`]；RSX 请用 [`launch`].
    pub fn run<F: FnMut() -> Element + 'static>(self, ui: F) {
        super::run_with(self.config.into_window_options(), ui);
    }

    /// **Builder 程序入口**：本 builder 的配置作为文档壳，每帧把 `body` 当作其子节点重建。
    pub fn mount<F: FnMut() -> Element + 'static>(self, mut body: F) {
        let config = self.config;
        let opts = config.clone().into_window_options();
        super::run_with(opts, move || {
            materialize_app_shell(config.clone(), take_packed(body()))
        });
    }

    /// [`Self::mount`] 的别名（不在默认文档中推荐）。
    pub fn launch<F: FnMut() -> Element + 'static>(self, body: F) {
        self.mount(body);
    }

    pub(crate) fn into_parts(self) -> (AppConfig, Vec<Element>) {
        (self.config, self.children)
    }
}

/// Full-window document shell used by [`AppBuilder`] (as [`Element`]) and `<App>`.
pub(crate) fn materialize_app_shell(config: AppConfig, children: Vec<Element>) -> Element {
    config.apply_document();
    let mut el: Element = crate::primitives::r#box::r#box()
        .w("100%")
        .h("100%")
        .flex_col()
        .children(children)
        .into();
    el.set_attr("data-slot", "app");
    #[cfg(feature = "i18n")]
    {
        el.set_attr("lang", crate::i18n::locale().tag());
        el.set_attr("dir", crate::i18n::writing_direction().as_str());
        if crate::i18n::writing_direction().is_rtl() {
            el = el.text_direction(crate::style::TextDirection::Rtl);
        }
    }
    #[cfg(not(feature = "i18n"))]
    {
        el.set_attr("lang", config.locale.as_deref().unwrap_or("en"));
    }
    el
}

impl From<AppBuilder> for Element {
    fn from(builder: AppBuilder) -> Self {
        let (config, children) = builder.into_parts();
        materialize_app_shell(config, children)
    }
}

impl crate::core::tree::IntoElementList for AppBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_builder_maps_title_size_font_and_frameless() {
        let o = app()
            .title("Hello")
            .window_label("main")
            .size(640.0, 480.0)
            .font("Inter, sans-serif")
            .font_size(20)
            .font_weight(500)
            .frameless()
            .transparent(true)
            .into_window_options();
        assert_eq!(o.title, "Hello");
        assert_eq!(o.window_label.as_deref(), Some("main"));
        assert_eq!(o.inner_width, Some(640.0));
        assert_eq!(o.inner_height, Some(480.0));
        assert!(!o.decorations);
        assert!(o.transparent);
        assert_eq!(o.default_font_family.as_deref(), Some("Inter, sans-serif"));
        assert_eq!(o.default_font_size, Some(20.0));
        assert_eq!(o.default_font_weight, Some(500));
    }

    #[test]
    fn app_builder_maps_glass_recipe_knobs() {
        let o = app()
            .blur(true)
            .backdrop("aero")
            .blur_amount(18.0)
            .glass_tint("#80FF0000")
            .glass_tint_dark("#80202020")
            .luminosity_opacity(0.4)
            .aero_color_balance(0.1)
            .aero_afterglow("#40FFE8A0")
            .aero_reflection(0.2)
            .into_window_options();
        assert!(o.blur);
        assert_eq!(o.backdrop, WindowBackdrop::Aero);
        assert_eq!(o.blur_amount, Some(18.0));
        assert_eq!(o.glass_tint, Some([0x80, 0xff, 0x00, 0x00]));
        assert_eq!(o.glass_tint_dark, Some([0x80, 0x20, 0x20, 0x20]));
        assert_eq!(o.luminosity_opacity, Some(0.4));
        assert_eq!(o.aero_color_balance, Some(0.1));
        assert_eq!(o.aero_afterglow, Some([0x40, 0xff, 0xe8, 0xa0]));
        assert_eq!(o.aero_reflection, Some(0.2));
        assert!(o.wants_custom_dcomp());
    }

    #[test]
    fn unset_app_builder_keeps_window_option_defaults() {
        let d = WindowOptions::default();
        let o = app().into_window_options();
        assert_eq!(o.title, d.title);
        assert_eq!(o.inner_width, d.inner_width);
        assert_eq!(o.decorations, d.decorations);
        assert!(o.default_font_size.is_none());
        assert!(o.default_font_family.is_none());
    }

    #[test]
    fn app_builder_locale_is_recorded() {
        let cfg = app().locale("zh-CN").i18n("i18n.toml").config().clone();
        assert_eq!(cfg.locale.as_deref(), Some("zh-CN"));
        assert_eq!(cfg.i18n.as_deref(), Some("i18n.toml"));
    }

    #[test]
    fn app_element_is_full_size_document_shell() {
        let el: Element = app().title("Shell").child(Element::new().text("hi")).into();
        assert_eq!(el.get_attr("data-slot").map(String::as_str), Some("app"));
        assert_eq!(el.children.len(), 1);
        assert_eq!(el.children[0].text.as_deref(), Some("hi"));
    }
}
