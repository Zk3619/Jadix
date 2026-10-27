#![allow(unused_imports)]

pub(crate) use jadix::app::navigator::{navigation_host, Navigator, Presentation, PushOptions};
pub(crate) use jadix::app::{
    close_window, open_secondary_window, open_secondary_window_with, request_exit,
    run_on_ui_thread, set_window_minimized, shared_slot, start_window_drag, system_tray_native,
    toggle_window_maximized, tray_icon, Router, TrayMenuItem, WindowBackdrop, WindowOptions,
};
pub(crate) use jadix::classes;
pub(crate) use jadix::component;
pub(crate) use jadix::core::reactivity::{batch, create_effect, use_signal, Signal};
pub(crate) use jadix::core::tree::{Element, LayoutStatus};
pub(crate) use jadix::primitives::{
    boxed, canvas, circle, hstack, interactive, polyline, r#box, scene3d, shader, svg, svg_path,
    vstack, Canvas2D, CanvasSize, Portal, PortalProps, Scene3dCamera,
};
pub(crate) use jadix::style::{parse_color, parse_text_color, TextSpan};
pub(crate) use jadix_components::components::*;
pub(crate) use jadix_components::components::{tabs_headless, TABSTRIP_FOCUS_PREFIX};
macro_rules! ui {
    ($($tokens:tt)*) => {
        jadix::__ui_with_runtime!(jadix; components = jadix_components::components; dnd = jadix_dnd::utils::drag; icons = jadix_lucide::utils::lucide_animated; $($tokens)*)
    };
}
pub(crate) use jadix::utils::lucide_glyph;
pub(crate) use jadix::utils::morph_icon;
pub(crate) use jadix::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
pub(crate) use jadix::{register_cursor_rgba8, CursorKind, MouseEvent};
pub(crate) use jadix_components::components::dock::{
    DockLayout, DockOrientation, DockPlacement, DockState, DockTab,
};
pub(crate) use jadix_lucide::animated_icon;
pub(crate) use std::cell::{Cell, RefCell};
pub(crate) use std::collections::HashSet;
pub(crate) use std::rc::Rc;
pub(crate) use ui;
