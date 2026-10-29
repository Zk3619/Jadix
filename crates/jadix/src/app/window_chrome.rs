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

pub use jadix_window_materials::WindowBackdrop;

/// Persistence identity for the primary window when [`Self::window_label`] is unset.
pub const PRIMARY_WINDOW_LABEL: &str = "main";

/// Creation-time window attributes (title, size, decorations, compositor alpha).
///
/// 产品面通过 [`crate::prelude`] 中的 `launch` / `app().mount` / `<App>` 配置；
/// 本类型是 [`super::run_with`] 与副窗的底层载荷。
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    pub title: String,
    /// Native window icon; installation / launcher icons belong to the app bundle.
    pub icon: Option<super::AppIcon>,
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
    /// Enable desktop fullscreen shortcuts for this window.
    pub fullscreen_shortcuts: bool,
    pub always_on_top: bool,
    /// Secondary windows close with the primary unless explicitly detached.
    pub close_with_primary: bool,
    /// Process startup policy; only read when launching the primary application.
    pub instance_policy: super::instance::InstancePolicy,
    /// OS title bar / border. `false` = frameless (draw your own chrome).
    pub decorations: bool,
    /// Per-pixel alpha is composited with the desktop. Pair with translucent fills.
    pub transparent: bool,
    /// OS backdrop blur (macOS vibrancy, Windows 11 Acrylic/Mica, Wayland kwin).
    /// Implies compositor alpha even if [`Self::transparent`] is false.
    pub blur: bool,
    /// Windows material when [`Self::blur`] is set.
    pub backdrop: WindowBackdrop,
    pub follow_theme: bool,
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
    #[cfg(all(debug_assertions, feature = "devtools"))]
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
            icon: None,
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
            fullscreen_shortcuts: true,
            always_on_top: false,
            close_with_primary: true,
            instance_policy: Default::default(),
            decorations: true,
            transparent: false,
            blur: false,
            backdrop: WindowBackdrop::Acrylic,
            follow_theme: false,
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
            #[cfg(all(debug_assertions, feature = "devtools"))]
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

    /// Set the initial native window icon (Windows / supported Linux compositors).
    pub fn icon(mut self, icon: super::AppIcon) -> Self {
        self.icon = Some(icon);
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

    pub fn fullscreen_shortcuts(mut self, enabled: bool) -> Self {
        self.fullscreen_shortcuts = enabled;
        self
    }

    /// Keep a secondary window alive after the primary closes when false.
    pub fn close_with_primary(mut self, enabled: bool) -> Self {
        self.close_with_primary = enabled;
        self
    }

    pub fn instance_policy(mut self, policy: super::instance::InstancePolicy) -> Self {
        self.instance_policy = policy;
        self
    }

    pub fn single_instance(self, id: impl Into<String>) -> Self {
        self.instance_policy(super::instance::InstancePolicy::Single { id: id.into() })
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

    pub fn follow_theme(mut self, enabled: bool) -> Self {
        self.follow_theme = enabled;
        self
    }

    pub(crate) fn themed(&self) -> Self {
        let mut options = self.clone();
        if self.follow_theme {
            let preset = crate::style::theme_preset();
            let windows_material = {
                #[cfg(all(target_os = "windows", feature = "windows-dwm"))]
                {
                    crate::platform::windows::glass::system_backdrop_available()
                }
                #[cfg(not(all(target_os = "windows", feature = "windows-dwm")))]
                {
                    cfg!(all(
                        not(target_os = "windows"),
                        feature = "window-materials"
                    ))
                }
            };
            let liquid_material = cfg!(all(
                not(target_os = "windows"),
                feature = "window-materials"
            )) || cfg!(all(target_os = "windows", feature = "windows-dcomp"));
            options.blur = match preset {
                crate::style::ThemePreset::Fluent => windows_material,
                crate::style::ThemePreset::LiquidGlass => liquid_material,
                _ => false,
            };
            options.transparent = options.blur;
            options.backdrop = match preset {
                crate::style::ThemePreset::Fluent if windows_material => WindowBackdrop::Mica,
                crate::style::ThemePreset::LiquidGlass if liquid_material => {
                    WindowBackdrop::Acrylic
                }
                _ => WindowBackdrop::None,
            };
        }
        options
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
        if self.follow_theme
            && crate::style::theme_preset() == crate::style::ThemePreset::LiquidGlass
        {
            return cfg!(all(windows, feature = "windows-dcomp"));
        }
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
    #[cfg(any(test, all(windows, feature = "windows-dwm")))]
    pub(crate) fn uses_system_backdrop(&self) -> bool {
        let options = self.themed();
        options.blur && !options.wants_custom_dcomp()
    }

    /// Enable runtime Inspector ([`super::devtools`], F12). Requires crate feature `devtools`.
    #[cfg(all(debug_assertions, feature = "devtools"))]
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

    /// GPU alpha must remain available across runtime theme changes. This does
    /// not require Win32's native transparent-window initialization.
    pub fn compositor_alpha(&self) -> bool {
        self.transparent || self.blur || self.follow_theme
    }

    /// `WS_EX_NOREDIRECTIONBITMAP` + DXGI composition surface.
    ///
    /// Reserve the composition surface before a following theme changes to
    /// glass. GPU alpha is independent of Win32's native transparent flag.
    pub(crate) fn wants_dcomp_swapchain(&self) -> bool {
        self.compositor_alpha() && cfg!(all(windows, feature = "windows-dcomp"))
    }

    /// Win11: extend DWM glass into the client so Acrylic/Mica fills the window.
    #[cfg(any(test, all(windows, feature = "windows-dwm")))]
    pub(crate) fn windows_extends_dwm_frame(&self) -> bool {
        self.uses_system_backdrop()
    }

    /// Win11: punch clear pixels through to the desktop (no system material).
    #[cfg(any(test, all(windows, feature = "windows-dwm")))]
    pub(crate) fn windows_punch_through_desktop(&self) -> bool {
        self.transparent && !self.blur
    }

    pub fn to_window_attributes(&self) -> WindowAttributes {
        self.window_attributes(self.wants_dcomp_swapchain())
    }

    /// `composition_swapchain`: Windows `WS_EX_NOREDIRECTIONBITMAP` so a DXGI
    /// composition surface / Vulkan premul swapchain can composite over the desktop.
    /// The shared GL fallback keeps redirection enabled.
    pub(crate) fn window_attributes(&self, composition_swapchain: bool) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_theme(native_theme_preference())
            .with_title(&self.title)
            .with_window_icon(self.icon.as_ref().map(super::AppIcon::to_winit))
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
            // On Windows, blur/follow-theme owns the DWM/DComp backdrop and
            // must keep the native transparent flag off. The GPU surface can
            // still use premultiplied alpha through `compositor_alpha()`.
            .with_transparent(
                self.transparent || (cfg!(not(windows)) && (self.blur || self.follow_theme)),
            )
            .with_blur(
                self.themed().blur
                    && !self.wants_dcomp_swapchain()
                    && cfg!(any(
                        all(windows, feature = "windows-dwm"),
                        all(not(windows), feature = "window-materials")
                    )),
            );

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
        attrs = crate::platform::decorate_window_attributes(
            attrs,
            &self.themed(),
            composition_swapchain,
        );
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
/// `composition_swapchain` describes the GPU alpha surface, not ownership of
/// its background: a WebView composition target can still use SystemBackdrop.
pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    // Win32 refreshes caption colors by toggling WM_NCACTIVATE. Reapplying
    // the same theme on every surface resize makes the title text flash.
    let theme = native_theme();
    if window.theme() != Some(theme) {
        window.set_theme(native_theme_preference());
    }
    crate::platform::apply_created_window(window, &options.themed(), composition_swapchain);
}

pub(crate) fn native_theme() -> winit::window::Theme {
    if crate::style::is_dark_theme() {
        winit::window::Theme::Dark
    } else {
        winit::window::Theme::Light
    }
}

pub(crate) fn native_theme_preference() -> Option<winit::window::Theme> {
    match crate::style::theme_preference() {
        crate::style::ThemePreference::System => None,
        _ => Some(native_theme()),
    }
}

pub(crate) fn update_system_theme(theme: Option<winit::window::Theme>) {
    if let Some(theme) = theme {
        crate::style::update_system_theme(match theme {
            winit::window::Theme::Light => crate::style::ThemeMode::Light,
            winit::window::Theme::Dark => crate::style::ThemeMode::Dark,
        });
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

pub fn set_window_fullscreen(fullscreen: bool) {
    if let Some(window) = super::multi_window::current_window() {
        let _ = window.set_fullscreen(fullscreen);
    }
}

pub fn toggle_window_fullscreen() {
    if let Some(window) = super::multi_window::current_window() {
        let _ = window.toggle_fullscreen();
    }
}

pub fn window_is_fullscreen() -> bool {
    with_active_window(|w| w.fullscreen().is_some()).unwrap_or(false)
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
#[path = "tests/window_chrome.rs"]
mod tests;
