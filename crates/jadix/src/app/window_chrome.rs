//! Window chrome: frameless, transparent, and OS backdrop blur.
//!
//! 程序入口见 [`crate::prelude`]（`launch` / `app().mount` / `<App>`）。
//! 创建期选项仍落在 [`WindowOptions`]（[`super::run_with`] 兼容、副窗
//! [`open_secondary_window_with`](super::multi_window::open_secondary_window_with)）。

use winit::dpi::LogicalSize;
use winit::window::{Window, WindowAttributes};

use super::glass_params::{
    clamp_blur_amount, clamp_unit, parse_glass_tint, GlassTint, DEFAULT_AERO_AFTERGLOW_BALANCE,
    DEFAULT_AERO_BLUR_BALANCE, DEFAULT_AERO_COLOR_BALANCE, DEFAULT_AERO_PARALLAX,
    DEFAULT_AERO_REFLECTION, DEFAULT_BLUR_AMOUNT, DEFAULT_LUMINOSITY_OPACITY,
};

/// Windows backdrop material (system DWM on Win11, custom DComp recipes when requested).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBackdrop {
    /// OS default (usually title-bar only).
    Auto,
    /// No system backdrop.
    None,
    /// Windows 11 Mica (`DWMSBT_MAINWINDOW`).
    Mica,
    /// Windows 11 Acrylic / frosted (`DWMSBT_TRANSIENTWINDOW`).
    #[default]
    Acrylic,
    /// Windows 11 Mica Alt (`DWMSBT_TABBEDWINDOW`).
    MicaAlt,
    /// Custom Gaussian blur of the desktop (DirectComposition recipe).
    Blur,
    /// Windows 7 Aero glass recipe (DirectComposition).
    Aero,
}

impl From<&str> for WindowBackdrop {
    fn from(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "mica" => Self::Mica,
            "mica-alt" | "mica_alt" | "micaalt" => Self::MicaAlt,
            "none" => Self::None,
            "auto" => Self::Auto,
            "blur" => Self::Blur,
            "aero" => Self::Aero,
            _ => Self::Acrylic,
        }
    }
}

impl From<String> for WindowBackdrop {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

/// Persistence identity for the primary window when [`Self::window_label`] is unset.
pub const PRIMARY_WINDOW_LABEL: &str = "main";

/// Creation-time window attributes (title, size, decorations, compositor alpha).
///
/// 产品面通过 [`crate::prelude`] 中的 `launch` / `app().mount` / `<App>` 配置；
/// 本类型是 [`super::run_with`] 与副窗的底层载荷。
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    pub title: String,
    /// Stable id for `jadix_window_state`. Primary defaults to [`PRIMARY_WINDOW_LABEL`];
    /// secondary windows without a label are not persisted.
    pub window_label: Option<String>,
    pub inner_width: Option<f64>,
    pub inner_height: Option<f64>,
    /// Logical client-area constraints. Invalid/non-finite sizes are ignored.
    pub min_size: Option<[f64; 2]>,
    pub max_size: Option<[f64; 2]>,
    /// Logical outer position; window managers (notably Wayland) may ignore it.
    pub position: Option<[f64; 2]>,
    /// Allow user resizing. Custom borders must also check the live window policy.
    pub resizable: bool,
    /// Opt-in client-side native resize band, in logical pixels (Windows/Linux).
    /// Only active without OS decorations; zero keeps it disabled.
    pub resize_border: f64,
    pub maximized: bool,
    pub fullscreen: bool,
    pub always_on_top: bool,
    /// OS title bar / border. `false` = frameless (draw your own chrome).
    pub decorations: bool,
    /// Per-pixel alpha is composited with the desktop. Pair with translucent fills.
    pub transparent: bool,
    /// OS backdrop blur (macOS vibrancy, Windows 11 Acrylic/Mica, Wayland kwin).
    /// Implies compositor alpha even if [`Self::transparent`] is false.
    pub blur: bool,
    /// Windows material when [`Self::blur`] is set.
    pub backdrop: WindowBackdrop,
    /// Custom Gaussian radius. `None` = [`DEFAULT_BLUR_AMOUNT`] when a recipe needs it.
    pub blur_amount: Option<f32>,
    /// ARGB overlay (`[a,r,g,b]`). `None` = recipe default (translucent white / black).
    /// Light / default tint; dark mode uses [`Self::glass_tint_dark`] when set.
    pub glass_tint: Option<GlassTint>,
    /// Dark-mode tint. `None` = reuse [`Self::glass_tint`].
    pub glass_tint_dark: Option<GlassTint>,
    /// Acrylic / Mica luminosity layer. `None` = derive from tint (HSV-V clamp).
    pub luminosity_opacity: Option<f32>,
    /// Aero color / afterglow / blur layer weights. `None` = Win7 Sky defaults.
    pub aero_color_balance: Option<f32>,
    pub aero_afterglow_balance: Option<f32>,
    pub aero_blur_balance: Option<f32>,
    /// Independent Aero afterglow color. `None` = same as the resolved tint.
    pub aero_afterglow: Option<GlassTint>,
    /// Aero specular overlay intensity (0 = off). `None` = default on Aero, off otherwise.
    pub aero_reflection: Option<f32>,
    /// Parallax of the Aero specular vs the monitor (0 = locked to the window).
    pub aero_parallax: Option<f32>,
    /// Enable runtime Inspector (F12). Requires `jadix` crate feature `devtools`.
    #[cfg(feature = "devtools")]
    pub devtools: bool,
    /// CSS `font-family` list applied as the document default (overrides `JADIX_FONT_FAMILY`).
    pub default_font_family: Option<String>,
    /// Default font size in logical px (overrides `JADIX_FONT_SIZE`, cascade root, CSS `1rem`).
    pub default_font_size: Option<f32>,
    /// Default `font-weight` (overrides `JADIX_FONT_WEIGHT`, cascade root).
    pub default_font_weight: Option<u16>,
    /// CSS `font-synthesis: weight`. `None` = env `JADIX_SYNTHETIC_BOLD` (default on).
    pub synthetic_bold: Option<bool>,
    /// Web only: CSS selector or element id for the winit canvas (Iced `target`).
    ///
    /// `None` appends a new canvas to `document.body`. `#foo` / `foo` replaces that node.
    pub web_canvas_target: Option<String>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: std::env::var("JADIX_WINDOW_TITLE").unwrap_or_else(|_| "Jadix Engine".into()),
            window_label: None,
            inner_width: None,
            inner_height: None,
            min_size: None,
            max_size: None,
            position: None,
            resizable: true,
            resize_border: 0.0,
            maximized: false,
            fullscreen: false,
            always_on_top: false,
            decorations: true,
            transparent: false,
            blur: false,
            backdrop: WindowBackdrop::Acrylic,
            blur_amount: None,
            glass_tint: None,
            glass_tint_dark: None,
            luminosity_opacity: None,
            aero_color_balance: None,
            aero_afterglow_balance: None,
            aero_blur_balance: None,
            aero_afterglow: None,
            aero_reflection: None,
            aero_parallax: None,
            #[cfg(feature = "devtools")]
            devtools: false,
            default_font_family: None,
            default_font_size: None,
            default_font_weight: None,
            synthetic_bold: None,
            web_canvas_target: None,
        }
    }
}

impl WindowOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Persistence identity (`jadix_window_state`). Empty string clears the label.
    pub fn window_label(mut self, label: impl Into<String>) -> Self {
        let s = label.into();
        self.window_label = if s.trim().is_empty() { None } else { Some(s) };
        self
    }

    /// Label passed to [`super::WindowLifecycleHook::on_created`].
    ///
    /// Primary windows fall back to [`PRIMARY_WINDOW_LABEL`]; unlabeled secondaries
    /// return `None` and are not tracked.
    pub fn persistence_label(&self, primary: bool) -> Option<&str> {
        match self.window_label.as_deref().map(str::trim) {
            Some(l) if !l.is_empty() => Some(l),
            _ if primary => Some(PRIMARY_WINDOW_LABEL),
            _ => None,
        }
    }

    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.inner_width = Some(width);
        self.inner_height = Some(height);
        self
    }

    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn min_size(mut self, width: f64, height: f64) -> Self {
        self.min_size = Some([width, height]);
        self
    }

    pub fn max_size(mut self, width: f64, height: f64) -> Self {
        self.max_size = Some([width, height]);
        self
    }

    pub fn position(mut self, x: f64, y: f64) -> Self {
        self.position = Some([x, y]);
        self
    }

    pub fn resizable(mut self, enabled: bool) -> Self {
        self.resizable = enabled;
        self
    }

    pub fn resize_border(mut self, inset: f64) -> Self {
        self.resize_border = if inset.is_finite() {
            inset.max(0.0)
        } else {
            0.0
        };
        self
    }

    pub fn maximized(mut self, enabled: bool) -> Self {
        self.maximized = enabled;
        self
    }

    /// Borderless fullscreen on the current monitor (OS permission/policy applies).
    pub fn fullscreen(mut self, enabled: bool) -> Self {
        self.fullscreen = enabled;
        self
    }

    pub fn always_on_top(mut self, enabled: bool) -> Self {
        self.always_on_top = enabled;
        self
    }

    /// Shorthand for [`Self::decorations`]`(false)`.
    pub fn frameless(self) -> Self {
        self.decorations(false)
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    pub fn backdrop(mut self, backdrop: WindowBackdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// Custom blur radius in D2D standard-deviation units (clamped 0..=250).
    pub fn blur_amount(mut self, amount: f32) -> Self {
        self.blur_amount = Some(clamp_blur_amount(amount));
        self
    }

    /// Glass tint as ARGB bytes, `#RRGGBB`, or `#AARRGGBB`.
    pub fn glass_tint(mut self, tint: impl Into<GlassTintArg>) -> Self {
        self.glass_tint = tint.into().0;
        self
    }

    /// Dark-mode glass tint (`#RRGGBB` / `#AARRGGBB`). Unset = reuse [`Self::glass_tint`].
    pub fn glass_tint_dark(mut self, tint: impl Into<GlassTintArg>) -> Self {
        self.glass_tint_dark = tint.into().0;
        self
    }

    pub fn luminosity_opacity(mut self, opacity: f32) -> Self {
        self.luminosity_opacity = Some(clamp_unit(opacity));
        self
    }

    pub fn aero_color_balance(mut self, v: f32) -> Self {
        self.aero_color_balance = Some(clamp_unit(v));
        self
    }

    pub fn aero_afterglow_balance(mut self, v: f32) -> Self {
        self.aero_afterglow_balance = Some(clamp_unit(v));
        self
    }

    pub fn aero_blur_balance(mut self, v: f32) -> Self {
        self.aero_blur_balance = Some(clamp_unit(v));
        self
    }

    /// Aero afterglow color, independent of [`Self::glass_tint`].
    pub fn aero_afterglow(mut self, tint: impl Into<GlassTintArg>) -> Self {
        self.aero_afterglow = tint.into().0;
        self
    }

    /// Aero specular overlay intensity (0 disables).
    pub fn aero_reflection(mut self, intensity: f32) -> Self {
        self.aero_reflection = Some(clamp_unit(intensity));
        self
    }

    /// Aero specular parallax vs the monitor (0 = glued to the window).
    pub fn aero_parallax(mut self, amount: f32) -> Self {
        self.aero_parallax = Some(clamp_unit(amount));
        self
    }

    pub fn resolved_blur_amount(&self) -> f32 {
        self.blur_amount.unwrap_or(DEFAULT_BLUR_AMOUNT)
    }

    pub fn resolved_luminosity_opacity(&self) -> f32 {
        self.luminosity_opacity
            .unwrap_or(DEFAULT_LUMINOSITY_OPACITY)
    }

    pub fn resolved_aero_color_balance(&self) -> f32 {
        self.aero_color_balance
            .unwrap_or(DEFAULT_AERO_COLOR_BALANCE)
    }

    pub fn resolved_aero_afterglow_balance(&self) -> f32 {
        self.aero_afterglow_balance
            .unwrap_or(DEFAULT_AERO_AFTERGLOW_BALANCE)
    }

    pub fn resolved_aero_blur_balance(&self) -> f32 {
        self.aero_blur_balance.unwrap_or(DEFAULT_AERO_BLUR_BALANCE)
    }

    pub fn resolved_glass_tint(&self) -> GlassTint {
        self.resolved_glass_tint_for(crate::style::is_dark_theme())
    }

    pub fn resolved_glass_tint_for(&self, dark: bool) -> GlassTint {
        if dark {
            self.glass_tint_dark
                .or(self.glass_tint)
                .unwrap_or([0x64, 0x20, 0x20, 0x20])
        } else {
            self.glass_tint.unwrap_or([0x64, 0xff, 0xff, 0xff])
        }
    }

    pub fn resolved_aero_afterglow(&self) -> GlassTint {
        self.resolved_aero_afterglow_for(crate::style::is_dark_theme())
    }

    pub fn resolved_aero_afterglow_for(&self, dark: bool) -> GlassTint {
        self.aero_afterglow
            .unwrap_or_else(|| self.resolved_glass_tint_for(dark))
    }

    pub fn resolved_aero_reflection(&self) -> f32 {
        self.aero_reflection
            .unwrap_or(if self.backdrop == WindowBackdrop::Aero {
                DEFAULT_AERO_REFLECTION
            } else {
                0.0
            })
    }

    pub fn resolved_aero_parallax(&self) -> f32 {
        self.aero_parallax.unwrap_or(DEFAULT_AERO_PARALLAX)
    }

    /// Custom DComp recipes: Blur / Aero, or Acrylic/Mica with overridden recipe knobs.
    pub fn wants_custom_dcomp(&self) -> bool {
        if !self.blur {
            return false;
        }
        matches!(
            self.backdrop,
            WindowBackdrop::Blur | WindowBackdrop::Aero | WindowBackdrop::MicaAlt
        ) || self.blur_amount.is_some()
            || self.glass_tint.is_some()
            || self.glass_tint_dark.is_some()
            || self.luminosity_opacity.is_some()
            || self.aero_color_balance.is_some()
            || self.aero_afterglow_balance.is_some()
            || self.aero_blur_balance.is_some()
            || self.aero_afterglow.is_some()
            || self.aero_reflection.is_some()
            || self.aero_parallax.is_some()
    }

    /// Win11 `DWMWA_SYSTEMBACKDROP_TYPE` path (no custom recipe knobs).
    pub(crate) fn uses_system_backdrop(&self) -> bool {
        self.blur && !self.wants_custom_dcomp()
    }

    /// Enable runtime Inspector ([`super::devtools`], F12). Requires crate feature `devtools`.
    #[cfg(feature = "devtools")]
    pub fn devtools(mut self, enabled: bool) -> Self {
        self.devtools = enabled;
        self
    }

    /// Web canvas mount point (`#id`, `.class`, or bare id). Ignored on native hosts.
    pub fn canvas_target(mut self, target: impl Into<String>) -> Self {
        let s = target.into();
        self.web_canvas_target = if s.trim().is_empty() { None } else { Some(s) };
        self
    }

    /// Document default `font-family` (CSS comma-separated list).
    pub fn default_font(mut self, family: impl Into<String>) -> Self {
        self.default_font_family = Some(family.into());
        self
    }

    /// Document default font size in logical pixels (CSS root / `1rem`).
    pub fn default_font_size(mut self, px: f32) -> Self {
        self.default_font_size = Some(px.max(1.0));
        self
    }

    /// Document default `font-weight` (100–900).
    pub fn default_font_weight(mut self, weight: u16) -> Self {
        self.default_font_weight = Some(weight.clamp(100, 900));
        self
    }

    /// CSS `font-synthesis: weight` when a requested face (e.g. Medium 500) is missing.
    pub fn synthetic_bold(mut self, enabled: bool) -> Self {
        self.synthetic_bold = Some(enabled);
        self
    }

    /// Push these options into process-wide [`crate::utils::font::TextDefaults`].
    pub(crate) fn install_text_defaults(&self) {
        let mut d = crate::utils::font::text_defaults();
        if let Some(ref family) = self.default_font_family {
            d.family = Some(family.clone());
        }
        if let Some(size) = self.default_font_size {
            d.size_px = size;
        }
        if let Some(weight) = self.default_font_weight {
            d.weight = weight;
        }
        if let Some(synth) = self.synthetic_bold {
            d.synthetic_bold = synth;
        }
        crate::utils::font::set_text_defaults(d);
    }

    /// Surface / clear color must expose alpha (transparent **or** blur).
    pub fn compositor_alpha(&self) -> bool {
        self.transparent || self.blur
    }

    /// `WS_EX_NOREDIRECTIONBITMAP` + DXGI composition surface.
    ///
    /// With `backend-dx12`, every compositor-alpha window presents through a
    /// DComp visual tree (recipes on a backdrop sprite, UI on a SurfaceHandle).
    /// DX12 HWND swapchains only advertise `Opaque`, so they cannot show glass.
    pub(crate) fn wants_dcomp_swapchain(&self) -> bool {
        self.compositor_alpha() && cfg!(all(windows, feature = "backend-dx12"))
    }

    /// Win11: extend DWM glass into the client so Acrylic/Mica fills the window.
    pub(crate) fn windows_extends_dwm_frame(&self) -> bool {
        self.uses_system_backdrop()
    }

    /// Win11: punch clear pixels through to the desktop (no system material).
    pub(crate) fn windows_punch_through_desktop(&self) -> bool {
        self.transparent && !self.blur
    }

    pub fn to_window_attributes(&self) -> WindowAttributes {
        self.window_attributes(self.wants_dcomp_swapchain())
    }

    /// `composition_swapchain`: Windows `WS_EX_NOREDIRECTIONBITMAP` so a DXGI
    /// composition surface / Vulkan premul swapchain can composite over the desktop.
    /// Must stay **off** for DWM SystemBackdrop and the shared GL path.
    pub(crate) fn window_attributes(&self, composition_swapchain: bool) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_theme(Some(native_theme()))
            .with_title(&self.title)
            .with_visible(false)
            .with_resizable(self.resizable)
            .with_maximized(self.maximized)
            .with_fullscreen(
                self.fullscreen
                    .then_some(winit::monitor::Fullscreen::Borderless(None)),
            )
            .with_window_level(if self.always_on_top {
                winit::window::WindowLevel::AlwaysOnTop
            } else {
                winit::window::WindowLevel::Normal
            })
            .with_decorations(self.decorations)
            .with_transparent(self.compositor_alpha())
            .with_blur(self.blur && !self.wants_dcomp_swapchain());

        if let (Some(w), Some(h)) = (self.inner_width, self.inner_height) {
            if valid_size([w, h]) {
                attrs = attrs.with_surface_size(LogicalSize::new(w, h));
            }
        } else if let (Ok(w), Ok(h)) = (
            std::env::var("JADIX_WINDOW_WIDTH"),
            std::env::var("JADIX_WINDOW_HEIGHT"),
        ) {
            if let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) {
                if valid_size([w, h]) {
                    attrs = attrs.with_surface_size(LogicalSize::new(w, h));
                }
            }
        }

        let min = self.min_size.filter(|s| valid_size(*s));
        if let Some([w, h]) = min {
            attrs = attrs.with_min_surface_size(LogicalSize::new(w, h));
        }
        if let Some([mut w, mut h]) = self.max_size.filter(|s| valid_size(*s)) {
            if let Some([min_w, min_h]) = min {
                w = w.max(min_w);
                h = h.max(min_h);
            }
            attrs = attrs.with_max_surface_size(LogicalSize::new(w, h));
        }
        if let Some([x, y]) = self.position.filter(|p| p.iter().all(|v| v.is_finite())) {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }
        attrs = crate::platform::decorate_window_attributes(attrs, self, composition_swapchain);
        attrs
    }
}

fn valid_size(size: [f64; 2]) -> bool {
    size.iter().all(|v| v.is_finite() && *v > 0.0)
}

impl super::App {
    /// Native chrome is dispatched before the content tree, for primary and secondary alike.
    pub(super) fn handle_native_resize_border(
        &mut self,
        id: winit::window::WindowId,
        event: &winit::event::WindowEvent,
        event_loop: &dyn winit::event_loop::ActiveEventLoop,
    ) -> bool {
        use std::sync::atomic::Ordering;
        use winit::cursor::CursorIcon;
        use winit::event::{ButtonSource, ElementState, MouseButton, PointerSource, WindowEvent};
        use winit::window::ResizeDirection::*;
        let Some(handle) = super::runtime::window_handle(id) else {
            return false;
        };
        let inset = f64::from_bits(handle.chrome.inset.load(Ordering::Relaxed));
        if inset <= 0.0 {
            return false;
        }
        let position = match event {
            WindowEvent::PointerMoved {
                position,
                source: PointerSource::Mouse,
                ..
            } => *position,
            WindowEvent::PointerButton {
                position,
                button: ButtonSource::Mouse(MouseButton::Left),
                state,
                ..
            } => {
                if *state == ElementState::Released {
                    return handle.chrome.pressed.swap(false, Ordering::Relaxed);
                }
                // Native resize can consume the release. A fresh press always
                // starts a new gesture, including a press in the content area.
                handle.chrome.pressed.store(false, Ordering::Relaxed);
                *position
            }
            WindowEvent::PointerLeft { .. }
            | WindowEvent::Focused(false)
            | WindowEvent::SurfaceResized(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                handle.chrome.pressed.store(false, Ordering::Relaxed);
                handle.clear_resize_cursor();
                return false;
            }
            _ => return false,
        };
        let Ok(window) = handle.upgrade() else {
            return false;
        };
        let position = position.to_logical::<f64>(window.scale_factor());
        let Some(direction) = handle
            .resize_direction_at(position.x, position.y, inset)
            .ok()
            .flatten()
        else {
            handle.clear_resize_cursor();
            return false;
        };
        let cursor = match direction {
            North => CursorIcon::NResize,
            South => CursorIcon::SResize,
            West => CursorIcon::WResize,
            East => CursorIcon::EResize,
            NorthWest => CursorIcon::NwResize,
            NorthEast => CursorIcon::NeResize,
            SouthWest => CursorIcon::SwResize,
            SouthEast => CursorIcon::SeResize,
        };
        window.set_cursor(cursor.into());
        let was_hovered = handle.chrome.hovered.swap(true, Ordering::Relaxed);
        if matches!(
            event,
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                ..
            }
        ) {
            handle.chrome.pressed.store(true, Ordering::Relaxed);
            if let Err(e) = handle.start_resize(direction) {
                log::debug!("native resize border: {e}");
                handle.chrome.pressed.store(false, Ordering::Relaxed);
            }
        }
        // Clear content hover once when entering the native band.
        if !was_hovered {
            if self.is_secondary_window(id) {
                self.handle_secondary_cursor_left(id, event_loop);
            } else {
                self.with_primary_input(id, |app| app.handle_cursor_left(id, event_loop));
            }
        }
        window.set_cursor(cursor.into());
        true
    }
}

/// Platform follow-up after `create_window` (DWM client-area glass, runtime blur).
///
/// Idempotent: call again after the GPU surface is configured / resized / shown.
///
/// `composition_swapchain` is set when the window presents a premultiplied Vulkan/DXGI
/// swapchain (`WS_EX_NOREDIRECTIONBITMAP`). In that case DWM empty-region blur-behind
/// is skipped — it targets the redirection bitmap we just disabled.
pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    window.set_theme(Some(native_theme()));
    crate::platform::apply_created_window(window, options, composition_swapchain);
}

pub(crate) fn native_theme() -> winit::window::Theme {
    if crate::style::is_dark_theme() {
        winit::window::Theme::Dark
    } else {
        winit::window::Theme::Light
    }
}

/// Builder argument for [`WindowOptions::glass_tint`].
#[derive(Clone, Copy, Debug)]
pub struct GlassTintArg(pub Option<GlassTint>);

impl From<GlassTint> for GlassTintArg {
    fn from(v: GlassTint) -> Self {
        Self(Some(v))
    }
}

impl From<&str> for GlassTintArg {
    fn from(s: &str) -> Self {
        Self(parse_glass_tint(s))
    }
}

impl From<String> for GlassTintArg {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

fn with_active_window<R>(f: impl FnOnce(&dyn Window) -> R) -> Option<R> {
    super::runtime::with_active_winit_window(f)
}

/// Begin an OS window move. Call from **mousedown** on a custom title bar (not `click`).
pub fn start_window_drag() {
    let err = with_active_window(|w| w.drag_window().err()).flatten();
    if let Some(e) = err {
        log::debug!("start_window_drag: {e}");
    }
}

pub fn set_window_maximized(maximized: bool) {
    with_active_window(|w| w.set_maximized(maximized));
}

pub fn window_is_maximized() -> bool {
    with_active_window(|w| w.is_maximized()).unwrap_or(false)
}

/// Whether the active window allows resizing. False when no window is active.
/// Check when drawing custom resize hit zones/cursors; `start_resize` rechecks on press.
pub fn window_is_resizable() -> bool {
    with_active_window(|w| w.is_resizable()).unwrap_or(false)
}

pub(super) use winit::chrome::resize_direction_at;

pub fn toggle_window_maximized() {
    with_active_window(|w| w.set_maximized(!w.is_maximized()));
}

pub fn set_window_minimized(minimized: bool) {
    with_active_window(|w| w.set_minimized(minimized));
}

/// Request closing the active window, honoring [`super::WindowLifecycleHook::on_close_requested`].
pub fn close_window() {
    if let Some(window) = super::multi_window::current_window() {
        if let Err(e) = window.close() {
            log::warn!("close_window: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_constraints_reject_nonfinite_values_and_normalize_bounds() {
        use winit::dpi::{LogicalSize, Size};
        let attrs = WindowOptions::new()
            .min_size(400.0, 300.0)
            .max_size(200.0, 900.0)
            .resizable(false)
            .maximized(true)
            .always_on_top(true)
            .fullscreen(true)
            .position(-100.0, 20.0)
            .to_window_attributes();
        assert_eq!(
            attrs.min_surface_size,
            Some(Size::Logical(LogicalSize::new(400.0, 300.0)))
        );
        assert_eq!(
            attrs.max_surface_size,
            Some(Size::Logical(LogicalSize::new(400.0, 900.0)))
        );
        assert!(!attrs.resizable);
        assert!(attrs.maximized);
        assert!(attrs.fullscreen.is_some());
        assert_eq!(attrs.window_level, winit::window::WindowLevel::AlwaysOnTop);
        assert!(attrs.position.is_some());
        let invalid = WindowOptions::new()
            .size(f64::INFINITY, 20.0)
            .min_size(0.0, 10.0)
            .max_size(f64::NAN, 100.0)
            .position(f64::NAN, 10.0)
            .to_window_attributes();
        assert!(invalid.surface_size.is_none());
        assert!(invalid.min_surface_size.is_none());
        assert!(invalid.max_surface_size.is_none());
        assert!(invalid.position.is_none());
    }

    #[test]
    fn default_options_are_decorated_opaque() {
        let o = WindowOptions::new();
        assert!(o.decorations);
        assert!(!o.transparent);
        assert!(!o.blur);
        assert!(!o.compositor_alpha());
        assert_eq!(o.persistence_label(true), Some(PRIMARY_WINDOW_LABEL));
        assert_eq!(o.persistence_label(false), None);
        assert!(o.web_canvas_target.is_none());
        assert_eq!(
            WindowOptions::new()
                .canvas_target("#app")
                .web_canvas_target
                .as_deref(),
            Some("#app")
        );
        let attrs = o.to_window_attributes();
        assert!(attrs.decorations);
        assert!(!attrs.transparent);
        assert!(!attrs.blur);
    }

    #[test]
    fn frameless_blur_enables_compositor_alpha() {
        let o = WindowOptions::new()
            .title("Glass")
            .size(400.0, 280.0)
            .frameless()
            .blur(true)
            .backdrop(WindowBackdrop::Acrylic);
        assert!(!o.decorations);
        assert!(o.compositor_alpha());
        let attrs = o.to_window_attributes();
        assert!(!attrs.decorations);
        assert!(attrs.transparent);
        assert_eq!(attrs.blur, !cfg!(all(windows, feature = "backend-dx12")));
        assert_eq!(attrs.title, "Glass");
    }

    #[test]
    fn transparent_without_blur_still_compositor_alpha() {
        let o = WindowOptions::new().transparent(true);
        assert!(o.compositor_alpha());
        assert!(o.to_window_attributes().transparent);
        assert!(!o.to_window_attributes().blur);
        assert!(o.windows_punch_through_desktop());
        assert!(!o.windows_extends_dwm_frame());
    }

    #[test]
    fn blur_extends_dwm_frame_transparent_does_not() {
        let blur = WindowOptions::new().transparent(true).blur(true);
        assert!(blur.windows_extends_dwm_frame());
        assert!(!blur.windows_punch_through_desktop());
        let clear = WindowOptions::new().transparent(true);
        assert!(!clear.windows_extends_dwm_frame());
        assert!(clear.windows_punch_through_desktop());
        let opaque = WindowOptions::new();
        assert!(!opaque.windows_extends_dwm_frame());
        assert!(!opaque.windows_punch_through_desktop());
    }

    #[test]
    fn blur_and_aero_request_custom_dcomp() {
        let blur = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Blur);
        assert!(blur.wants_custom_dcomp());
        assert!(!blur.uses_system_backdrop());
        assert!(blur.compositor_alpha());
        let aero = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Aero)
            .blur_amount(12.0)
            .glass_tint("#64FFFFFF");
        assert!(aero.wants_custom_dcomp());
        assert_eq!(aero.resolved_blur_amount(), 12.0);
        assert_eq!(
            aero.resolved_glass_tint_for(false),
            [0x64, 0xff, 0xff, 0xff]
        );
        let pair = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Aero)
            .glass_tint("#64FFFFFF")
            .glass_tint_dark("#80202020")
            .aero_afterglow("#40FFE8A0")
            .aero_reflection(0.2);
        assert_eq!(
            pair.resolved_glass_tint_for(false),
            [0x64, 0xff, 0xff, 0xff]
        );
        assert_eq!(pair.resolved_glass_tint_for(true), [0x80, 0x20, 0x20, 0x20]);
        assert_eq!(
            pair.resolved_aero_afterglow_for(false),
            [0x40, 0xff, 0xe8, 0xa0]
        );
        assert!((pair.resolved_aero_reflection() - 0.2).abs() < 1e-5);
        assert!(pair.wants_custom_dcomp());
    }

    #[test]
    fn stock_acrylic_stays_on_system_backdrop() {
        let o = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Acrylic);
        assert!(!o.wants_custom_dcomp());
        assert!(o.uses_system_backdrop());
        assert!(o.windows_extends_dwm_frame());
    }

    #[test]
    fn acrylic_with_custom_radius_requests_dcomp() {
        let o = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Acrylic)
            .blur_amount(30.0)
            .luminosity_opacity(0.5);
        assert!(o.wants_custom_dcomp());
        assert!(!o.uses_system_backdrop());
        assert_eq!(o.resolved_luminosity_opacity(), 0.5);
    }

    #[test]
    fn compositor_alpha_requests_dcomp_swapchain_with_dx12() {
        let acrylic = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Acrylic);
        let transparent = WindowOptions::new().transparent(true);
        let custom = WindowOptions::new()
            .blur(true)
            .backdrop(WindowBackdrop::Blur);
        let expected = cfg!(all(windows, feature = "backend-dx12"));
        assert_eq!(acrylic.wants_dcomp_swapchain(), expected);
        assert_eq!(transparent.wants_dcomp_swapchain(), expected);
        assert_eq!(custom.wants_dcomp_swapchain(), expected);
        assert_eq!(acrylic.to_window_attributes().blur, !expected);
    }

    #[test]
    fn window_label_overrides_persistence_identity() {
        let o = WindowOptions::new().window_label("editor");
        assert_eq!(o.persistence_label(true), Some("editor"));
        assert_eq!(o.persistence_label(false), Some("editor"));
        let cleared = WindowOptions::new().window_label("editor").window_label("");
        assert_eq!(cleared.persistence_label(true), Some(PRIMARY_WINDOW_LABEL));
        assert_eq!(cleared.persistence_label(false), None);
    }
}
