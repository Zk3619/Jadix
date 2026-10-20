#![allow(unused_imports)]

pub(crate) use jadix::app::navigator::{Navigator, Presentation, PushOptions, navigation_host};
pub(crate) use jadix::app::{
    Router, TrayMenuItem, WindowBackdrop, WindowOptions, close_window, open_secondary_window,
    open_secondary_window_with, request_exit, run_on_ui_thread, set_window_minimized, shared_slot,
    start_window_drag, system_tray_native, toggle_window_maximized, tray_icon,
};
pub(crate) use jadix::classes;
pub(crate) use jadix::component;
pub(crate) use jadix::components::*;
pub(crate) use jadix::components::{TABSTRIP_FOCUS_PREFIX, tabs_headless};
pub(crate) use jadix::core::reactivity::{Signal, batch, create_effect, use_signal};
pub(crate) use jadix::core::tree::{Element, LayoutStatus};
pub(crate) use jadix::primitives::{
    Canvas2D, CanvasSize, Portal, PortalProps, Scene3dCamera, r#box, boxed, canvas, circle, hstack,
    interactive, polyline, scene3d, shader, svg, svg_path, vstack,
};
pub(crate) use jadix::style::{TextSpan, parse_color, parse_text_color};
pub(crate) use jadix::ui;
pub(crate) use jadix::utils::animated_icon;
pub(crate) use jadix::utils::lucide::lucide_glyph;
pub(crate) use jadix::utils::morph_icon;
pub(crate) use jadix::utils::text_node::{TextNodeConfig, build_icon_text_node, build_text_node};
pub(crate) use jadix::{CursorKind, MouseEvent, register_cursor_rgba8};
pub(crate) use std::cell::{Cell, RefCell};
pub(crate) use std::collections::HashSet;
pub(crate) use std::rc::Rc;
