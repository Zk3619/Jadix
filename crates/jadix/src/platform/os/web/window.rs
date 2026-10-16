//! Web canvas attach. Mirrors Iced `iced_winit` (`with_canvas` / replace `#target`).

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    options: &WindowOptions,
    _composition_swapchain: bool,
) -> WindowAttributes {
    use winit::platform::web::WindowAttributesWeb;

    let mut web = WindowAttributesWeb::default()
        .with_prevent_default(true)
        .with_focusable(true);

    if let Some(target) = options.web_canvas_target.as_deref() {
        if let Some(canvas) = lookup_canvas(target) {
            web = web.with_canvas(Some(canvas)).with_append(false);
        } else {
            web = web.with_append(false);
        }
    } else {
        web = web.with_append(true);
    }
    attrs.with_platform_attributes(Box::new(web))
}

pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    _composition_swapchain: bool,
) {
    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowExtWeb;

    let Some(canvas) = window.canvas() else {
        return;
    };
    let _ = canvas.set_attribute("style", "display: block; width: 100%; height: 100%");

    let Some(target) = options.web_canvas_target.as_deref() else {
        return;
    };
    if canvas.id() == target {
        return;
    }

    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(Some(node)) = document.query_selector(&css_target(target)) else {
        if let Some(body) = document.body() {
            let _ = body.append_child(&canvas);
        }
        return;
    };
    if node.dyn_ref::<web_sys::HtmlCanvasElement>().is_some() && node.id() == canvas.id() {
        return;
    }
    let _ = node.replace_with_with_node_1(&canvas);
}

fn css_target(target: &str) -> String {
    if target.starts_with('#') || target.starts_with('.') {
        target.to_string()
    } else {
        format!("#{target}")
    }
}

fn lookup_canvas(target: &str) -> Option<web_sys::HtmlCanvasElement> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()?.document()?;
    document
        .query_selector(&css_target(target))
        .ok()
        .flatten()?
        .dyn_into()
        .ok()
}
