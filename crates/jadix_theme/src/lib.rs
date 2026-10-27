pub mod color;
pub mod color_space;
pub mod glass;
pub mod shadow;
pub mod theme;
pub mod theme_override;
pub mod theme_style;
pub use color::*;
pub use color_space::*;
pub use glass::*;
pub use jadix_animation::TimingFunction;
pub use shadow::*;
pub use theme::*;
pub use theme_override::*;
pub use theme_style::*;

thread_local! {
    static INVALIDATE: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
}
pub fn set_invalidation_callback(callback: fn()) {
    INVALIDATE.with(|slot| slot.set(Some(callback)));
}
fn notify_change() {
    if let Some(callback) = INVALIDATE.with(std::cell::Cell::get) {
        callback();
    }
}
