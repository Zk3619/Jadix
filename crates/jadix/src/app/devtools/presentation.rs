use std::cell::{Cell, RefCell};

use crate::app::{runtime, window_scope, WindowOptions, WindowSession};
use crate::core::tree::{Element, LayoutStatus};

use super::{panel, DevToolsState};

#[derive(Clone, Copy)]
enum InspectorMessage {
    Refresh,
}

thread_local! {
    static SESSION: RefCell<Option<WindowSession<(), InspectorMessage>>> = const { RefCell::new(None) };
    static REDOCKING: Cell<bool> = const { Cell::new(false) };
    static RESIZE_START: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
    static RESIZE_HOVER: Cell<bool> = const { Cell::new(false) };
}

pub fn is_detached() -> bool {
    SESSION.with(|session| session.borrow().is_some())
}

pub fn toggle_detached() {
    if is_detached() {
        REDOCKING.set(true);
        close_secondary();
        return;
    }
    let _scope = window_scope::enter_primary();
    let session = WindowSession::new(());
    session.on_child_message(|InspectorMessage::Refresh| {
        DevToolsState::bump();
    });
    session.on_opened(|_| {
        if REDOCKING.get() {
            close_secondary();
        }
        DevToolsState::bump();
    });
    session.on_closed(|_| {
        SESSION.with(|session| session.borrow_mut().take());
        if !REDOCKING.replace(false) {
            DevToolsState::set_open(false);
        }
        DevToolsState::bump();
    });
    SESSION.with(|slot| *slot.borrow_mut() = Some(session.clone()));
    let result = session.open(
        WindowOptions::new()
            .title("Jadix DevTools")
            .size(840.0, 760.0)
            .min_size(400.0, 300.0),
        |_| runtime::with_primary_root(panel::dock_panel_from_root).unwrap_or_else(Element::new),
    );
    if let Err(error) = result {
        REDOCKING.set(true);
        SESSION.with(|session| session.borrow_mut().take());
        DevToolsState::record_change("DevTools", &error);
    }
    DevToolsState::bump();
}

fn close_secondary() {
    let window = SESSION.with(|session| session.borrow().as_ref().and_then(WindowSession::window));
    if let Some(window) = window {
        let _ = window.close();
    }
}

pub(crate) fn sync(root: &Element) {
    if !DevToolsState::is_open() {
        close_secondary();
        return;
    }
    let window = SESSION.with(|session| session.borrow().as_ref().and_then(WindowSession::window));
    if let Some(window) = window {
        let _scope = window_scope::enter_secondary_id(window.id());
        let panel = panel::dock_panel_from_root(root);
        runtime::replace_secondary_root(window.id(), panel);
    }
}

pub(crate) fn request_refresh_from_secondary() {
    let session = SESSION.with(|session| session.borrow().clone());
    if let Some(session) = session {
        let _ = session.send_to_parent(InspectorMessage::Refresh);
    }
}

pub(crate) fn is_detached_window(id: winit::window::WindowId) -> bool {
    SESSION.with(|session| {
        session
            .borrow()
            .as_ref()
            .and_then(WindowSession::window)
            .is_some_and(|window| window.id() == id)
    })
}

pub(crate) fn window_resized(id: winit::window::WindowId) {
    if is_detached_window(id) {
        request_refresh_from_secondary();
    }
}

pub(crate) fn panel_width() -> f32 {
    if is_detached() {
        if let Some(width) = runtime::with_active_winit_window(|window| {
            window.surface_size().width as f32 / window.scale_factor() as f32
        }) {
            return width;
        }
    }
    DevToolsState::panel_width()
}

fn stop_resize() {
    if RESIZE_START.replace(None).is_some() {
        crate::app::release_pointer_capture();
    }
}

pub(crate) fn resize_handle() -> Element {
    use crate::app::pointer_events::{EventPhase, PointerEvent};
    Element::new()
        .focus_id("__jadix_devtools_resize")
        .absolute()
        .z_index(20)
        .left(0.0)
        .top(0.0)
        .w(6.0)
        .h("100%")
        .cursor_kind(crate::style::CursorKind::ColResize)
        .bg([0.0; 4])
        .on_hover(|inside| {
            RESIZE_HOVER.set(inside);
            runtime::request_redraw();
        })
        .on_pointer_down(|event: &PointerEvent| {
            if event.phase == EventPhase::Capture || event.button != winit::event::MouseButton::Left
            {
                return;
            }
            RESIZE_START.set(Some((event.x, DevToolsState::panel_width())));
            crate::app::set_pointer_capture("__jadix_devtools_resize");
            event.stop_propagation();
            event.prevent_default();
        })
        .on_mousemove(|event: &PointerEvent| {
            if let Some((start, width)) = RESIZE_START.get() {
                if event.buttons & 1 == 0 {
                    stop_resize();
                    return;
                }
                let available = runtime::with_active_winit_window(|window| {
                    window.surface_size().width as f32 / window.scale_factor() as f32
                })
                .unwrap_or(1600.0);
                DevToolsState::set_panel_width(
                    (width + start - event.x).min((available - 180.0).max(320.0)),
                );
                event.stop_propagation();
            }
        })
        .on_pointer_up(|_| stop_resize())
        .reactive_sync(|handle, _| {
            let active = RESIZE_HOVER.get() || RESIZE_START.get().is_some();
            let color = if active {
                crate::style::parse_color("#2563eb")
            } else {
                [0.0; 4]
            };
            let width = if active { 2.0 } else { 0.0 };
            if handle.style.resolved_border_colors()[3] == color
                && (handle.style.resolved_border_widths()[3] - width).abs() < 0.01
            {
                LayoutStatus::Clean
            } else {
                handle.style.bg_color = [0.0; 4];
                handle.style.set_border_side_color(3, color);
                handle.style.border_style = crate::style::LineStyle::Solid;
                handle.style.set_border_side_width(3, width);
                handle.style.shadow_color = [0.0; 4];
                handle.style.shadow_blur = 0.0;
                LayoutStatus::StyleChanged
            }
        })
}

#[cfg(test)]
#[path = "tests/presentation.rs"]
mod tests;
