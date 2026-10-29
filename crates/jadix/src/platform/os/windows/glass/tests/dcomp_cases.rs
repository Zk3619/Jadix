#[cfg(test)]
#[test]
#[ignore = "requires Windows system composition effects; run explicitly with --ignored"]
fn theme_window_recipes_use_supported_sdk_effects_without_fallback() {
    ensure_dispatcher_queue().unwrap();
    let previous = crate::style::get_theme();
    let compositor = Compositor::new().unwrap();
    for preset in [
        crate::style::ThemePreset::Fluent,
        crate::style::ThemePreset::LiquidGlass,
    ] {
        crate::style::set_theme_preset(preset);
        for mode in [
            crate::style::ThemeMode::Light,
            crate::style::ThemeMode::Dark,
        ] {
            crate::style::set_theme_mode(mode);
            let options = WindowOptions::new().follow_theme(true).themed();
            create_recipe_brush(&compositor, &options, true, crate::style::is_dark_theme())
                .unwrap();
        }
    }
    crate::style::set_theme(previous);
}
