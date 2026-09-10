#![allow(unused_imports)]

pub(crate) use rui::app::navigator::{navigation_host, Navigator, Presentation, PushOptions};
pub(crate) use rui::app::{
    close_window, open_secondary_window, open_secondary_window_with, request_exit,
    run_on_ui_thread, set_window_minimized, shared_slot, start_window_drag, system_tray_native,
    toggle_window_maximized, tray_icon, Router, TrayMenuItem, WindowOptions,
};
pub(crate) use rui::classes;
pub(crate) use rui::component;
pub(crate) use rui::components::*;
pub(crate) use rui::components::{tabs_headless, TABSTRIP_FOCUS_PREFIX};
pub(crate) use rui::core::reactivity::{batch, use_signal, Signal};
pub(crate) use rui::core::tree::{Element, LayoutStatus};
pub(crate) use rui::primitives::{
    boxed, canvas, circle, hstack, interactive, polyline, r#box, scene3d, svg, svg_path, vstack,
    Canvas2D, CanvasSize, Portal, PortalProps, Scene3dCamera,
};
pub(crate) use rui::style::{parse_color, parse_text_color, TextSpan};
pub(crate) use rui::ui;
pub(crate) use rui::utils::animated_icon;
pub(crate) use rui::utils::lucide::lucide_glyph;
pub(crate) use rui::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
pub(crate) use rui::{register_cursor_rgba8, CursorKind, MouseEvent};
pub(crate) use std::cell::{Cell, RefCell};
pub(crate) use std::collections::HashSet;
pub(crate) use std::rc::Rc;
