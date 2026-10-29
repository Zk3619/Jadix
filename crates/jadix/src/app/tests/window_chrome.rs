#[test]
fn following_theme_keeps_alpha_and_resolves_material_without_losing_explicit_options() {
    let previous = crate::style::get_theme();
    let options = super::WindowOptions::new().follow_theme(true);
    for (preset, backdrop, blur) in [
        (
            crate::style::ThemePreset::Fluent,
            super::WindowBackdrop::Mica,
            true,
        ),
        (
            crate::style::ThemePreset::LiquidGlass,
            super::WindowBackdrop::Acrylic,
            true,
        ),
        (
            crate::style::ThemePreset::Material3,
            super::WindowBackdrop::None,
            false,
        ),
        (
            crate::style::ThemePreset::Shadcn,
            super::WindowBackdrop::None,
            false,
        ),
    ] {
        if !preset.is_enabled() {
            continue;
        }
        crate::style::set_theme_preset(preset);
        let resolved = options.themed();
        let available = match preset {
            crate::style::ThemePreset::Fluent => cfg!(any(
                all(windows, feature = "windows-dwm"),
                all(not(windows), feature = "window-materials")
            )),
            crate::style::ThemePreset::LiquidGlass => cfg!(any(
                all(windows, feature = "windows-dcomp"),
                all(not(windows), feature = "window-materials")
            )),
            _ => true,
        };
        let blur = blur && available;
        let backdrop = if blur {
            backdrop
        } else {
            super::WindowBackdrop::None
        };
        assert_eq!(resolved.backdrop, backdrop);
        assert_eq!(resolved.blur, blur);
        assert!(options.compositor_alpha());
        assert_eq!(
            options.wants_dcomp_swapchain(),
            cfg!(all(windows, feature = "windows-dcomp"))
        );
        #[cfg(windows)]
        assert!(!options.to_window_attributes().transparent);
    }
    let explicit = super::WindowOptions::new()
        .blur(true)
        .backdrop(super::WindowBackdrop::Aero);
    assert_eq!(explicit.themed().backdrop, super::WindowBackdrop::Aero);
    crate::style::set_theme(previous);
}
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
    assert_eq!(attrs.transparent, !cfg!(windows));
    assert_eq!(
        attrs.blur,
        cfg!(any(
            all(
                windows,
                feature = "windows-dwm",
                not(feature = "windows-dcomp")
            ),
            all(not(windows), feature = "window-materials")
        ))
    );
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
fn compositor_alpha_requests_dcomp_swapchain_with_dcomp_feature() {
    let acrylic = WindowOptions::new()
        .blur(true)
        .backdrop(WindowBackdrop::Acrylic);
    let transparent = WindowOptions::new().transparent(true);
    let custom = WindowOptions::new()
        .blur(true)
        .backdrop(WindowBackdrop::Blur);
    let expected = cfg!(all(windows, feature = "windows-dcomp"));
    assert_eq!(acrylic.wants_dcomp_swapchain(), expected);
    assert_eq!(transparent.wants_dcomp_swapchain(), expected);
    assert_eq!(custom.wants_dcomp_swapchain(), expected);
    assert_eq!(
        acrylic.to_window_attributes().blur,
        !expected
            && cfg!(any(
                all(windows, feature = "windows-dwm"),
                all(not(windows), feature = "window-materials")
            ))
    );
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
