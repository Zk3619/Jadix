//! Shared internationalization with optional GPU menu integration.
pub use jadix_core::i18n::*;
#[cfg(feature = "gpu")]
impl From<WritingDirection> for crate::style::TextDirection {
    fn from(direction: WritingDirection) -> Self {
        match direction { WritingDirection::Ltr => Self::Ltr, WritingDirection::Rtl => Self::Rtl }
    }
}
#[cfg(feature = "gpu")]
pub(crate) fn install_host_callbacks() {
    jadix_core::i18n::set_host_callbacks(crate::app::runtime::request_redraw_all,
        |job| crate::app::ui_bridge::run_on_ui_thread(job));
}
#[cfg(feature = "gpu")]
pub(crate) fn take_locale_rebuild() -> bool {
    let changed = jadix_core::i18n::take_locale_rebuild();
    if changed { crate::app::native_menu::refresh_locale(); }
    changed
}
