//! Desktop IME defaults: winit cursor area only (no Imm / candidate HWND).

use winit::window::Window;

pub(crate) fn apply_ime_cursor_area(
    window: &Window,
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    ui_scale: f32,
    scroll_acc: [f32; 2],
    composing: bool,
) {
    let _ = (ui_scale, scroll_acc, composing);
    window.set_ime_cursor_area(
        winit::dpi::LogicalPosition::new(area_x as f64, area_y as f64),
        winit::dpi::LogicalSize::new(area_w.max(1.0) as f64, area_h.max(1.0) as f64),
    );
}

pub(crate) fn sync_focus_owner(
    _window: &Window,
    _focus_id: Option<&str>,
    _on_leave_field: impl FnOnce(&str),
) {
}

pub(crate) fn release_ui_control(_window: &Window, _cancel_comp: bool) {}

pub(crate) fn cancel_active_composition() {}

pub(crate) fn end_composition_ui() {}

pub(crate) fn ime_disallow(_window: &Window) {}
