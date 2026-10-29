use super::{pick_composite_alpha, surface_clear_color};

#[test]
fn opaque_clear_follows_theme_background() {
    let prior = crate::style::get_theme();
    crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
    let [r, g, b, _] =
        crate::style::display_srgba_to_linear_srgba(crate::style::get_theme().colors.background);
    let clear = surface_clear_color(false);
    assert_eq!(
        [clear.r, clear.g, clear.b, clear.a],
        [r as f64, g as f64, b as f64, 1.0]
    );
    assert_eq!(surface_clear_color(true), wgpu::Color::TRANSPARENT);
    crate::style::set_theme(prior);
}

#[test]
fn opaque_prefers_opaque_mode() {
    let modes = [
        wgpu::CompositeAlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::Opaque,
    ];
    assert_eq!(
        pick_composite_alpha(&modes, false),
        wgpu::CompositeAlphaMode::Opaque
    );
    assert_eq!(surface_clear_color(false).a, 1.0);
}

#[test]
fn compositor_prefers_premultiplied() {
    let modes = [
        wgpu::CompositeAlphaMode::Opaque,
        wgpu::CompositeAlphaMode::PreMultiplied,
    ];
    assert_eq!(
        pick_composite_alpha(&modes, true),
        wgpu::CompositeAlphaMode::PreMultiplied
    );
    assert_eq!(surface_clear_color(true).a, 0.0);
}
