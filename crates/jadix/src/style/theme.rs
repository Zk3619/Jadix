pub use jadix_theme::theme::*;

fn install() {
    jadix_theme::set_invalidation_callback(|| {
        super::stylesheet::watch::mark_style_hot_rebuild();
        crate::app::runtime::request_redraw_all();
    });
}
pub fn get_theme() -> Theme {
    install();
    jadix_theme::get_theme()
}
pub fn with_theme<R>(callback: impl FnOnce(&Theme) -> R) -> R {
    install();
    jadix_theme::with_theme(callback)
}
pub fn set_theme(theme: Theme) {
    install();
    jadix_theme::set_theme(theme);
}
pub fn set_theme_mode(mode: ThemeMode) {
    install();
    jadix_theme::set_theme_mode(mode);
}
pub fn set_theme_preference(preference: ThemePreference) {
    install();
    jadix_theme::set_theme_preference(preference);
}
pub fn update_system_theme(mode: ThemeMode) {
    install();
    jadix_theme::update_system_theme(mode);
}
