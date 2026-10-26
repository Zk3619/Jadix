//! Native browser surface service: window-owned engines, geometry, status and IPC.
//! Product components supply presentation chrome around these platform surfaces.

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

thread_local! {
    static MESSAGES: RefCell<HashMap<(crate::app::window_scope::WindowScope, String), Rc<dyn Fn(String)>>> = RefCell::new(HashMap::new());
}

pub(crate) fn deliver_message(id: &str, message: String) {
    if message.len() > 16 * 1024 * 1024 {
        return;
    }
    let key = (crate::app::window_scope::active(), id.to_string());
    let handler = MESSAGES.with(|handlers| handlers.borrow().get(&key).cloned());
    if let Some(handler) = handler {
        handler(message);
    }
}

#[cfg(windows)]
pub(crate) mod native;
#[cfg(not(windows))]
#[path = "webview/system.rs"]
pub(crate) mod native;

/// Whether a native engine is compiled. The actual window host can still reject
/// embedding (for example Linux/Wayland rather than X11).
pub const fn webview_supported() -> bool {
    cfg!(any(
        windows,
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebViewStatus {
    Loading,
    Ready,
    Failed(String),
    Unsupported,
}

pub fn webview_status(id: &str) -> Signal<WebViewStatus> {
    use_signal(&format!("{id}__webview_status"), || {
        if webview_supported() {
            WebViewStatus::Loading
        } else {
            WebViewStatus::Unsupported
        }
    })
}

#[cfg(windows)]
pub fn webview_snapshot(id: &str) -> Signal<bool> {
    use_signal(&format!("{id}__webview_snapshot"), || false)
}

/// Describe a native surface using logical retained-tree bounds and clipping.
/// The owning window manages engine creation, navigation and teardown.
pub fn surface(id: String, url: String, html: Option<String>) -> Element {
    let mut element: Element = crate::primitives::r#box::vstack()
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .overflow("hidden")
        .into();
    element.set_attr("jadix_webview_id", id);
    element.set_attr("jadix_webview_url", url);
    if let Some(html) = html {
        element.set_attr("jadix_webview_html", html);
    }
    element
}

/// Retain a bounded message callback until this node is unmounted. Replacement
/// handlers with the same window and surface ID survive older node cleanup.
pub fn message_lifetime(id: &str, handler: impl Fn(String) + 'static) -> Element {
    let key = (crate::app::window_scope::active(), id.to_string());
    let handler: Rc<dyn Fn(String)> = Rc::new(handler);
    MESSAGES.with(|handlers| handlers.borrow_mut().insert(key.clone(), handler.clone()));
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_none()
        .on_unmount(move || {
            let removed = MESSAGES.with(|handlers| {
                let mut handlers = handlers.borrow_mut();
                if handlers
                    .get(&key)
                    .is_some_and(|current| Rc::ptr_eq(current, &handler))
                {
                    handlers.remove(&key)
                } else {
                    None
                }
            });
            drop(removed);
        })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WebViewSpec {
    pub id: String,
    pub url: String,
    pub html: Option<String>,
    pub messages: bool,
    pub bounds: [f32; 4],
    pub clip: [f32; 4],
    pub visible: bool,
}

pub(crate) fn collect_specs(root: &Element, viewport: [f32; 4]) -> Vec<WebViewSpec> {
    fn visit(
        element: &Element,
        scroll: [f32; 2],
        clip: [f32; 4],
        visible: bool,
        out: &mut Vec<WebViewSpec>,
    ) {
        let scroll = crate::app::tree_hit::escape_scroll_acc(element, scroll);
        let bounds = crate::core::tree::visual_paint_aabb(element, scroll);
        let visible = visible
            && element.style.display != taffy::style::Display::None
            && element.style.opacity > 0.001
            && element.get_attr("jadix_nav_buried").map(String::as_str) != Some("1");
        let (clip_x, clip_y) = crate::core::tree::overflow_clip_axes(element);
        let mut child_clip = clip;
        for (axis, enabled) in [(0, clip_x), (1, clip_y)] {
            if enabled {
                let start = clip[axis].max(bounds[axis]);
                let end = (clip[axis] + clip[axis + 2]).min(bounds[axis] + bounds[axis + 2]);
                child_clip[axis] = start;
                child_clip[axis + 2] = (end - start).max(0.0);
            }
        }
        if let Some(id) = element.get_attr("jadix_webview_id") {
            let clipped = intersect(bounds, child_clip);
            out.push(WebViewSpec {
                id: id.clone(),
                url: element
                    .get_attr("jadix_webview_url")
                    .cloned()
                    .unwrap_or_default(),
                html: element.get_attr("jadix_webview_html").cloned(),
                messages: element.get_attr("jadix_webview_messages").is_some(),
                bounds,
                clip: clipped,
                visible: visible && clipped[2] > 0.0 && clipped[3] > 0.0,
            });
        }
        let scroll = crate::app::tree_hit::next_hit_scroll_acc(element, scroll);
        for child in &element.children {
            visit(child, scroll, child_clip, visible, out);
        }
    }
    let mut specs = Vec::new();
    visit(root, [0.0; 2], viewport, true, &mut specs);
    specs
}

fn intersect(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    let x = left[0].max(right[0]);
    let y = left[1].max(right[1]);
    [
        x,
        y,
        ((left[0] + left[2]).min(right[0] + right[2]) - x).max(0.0),
        ((left[1] + left[3]).min(right[1] + right[3]) - y).max(0.0),
    ]
}
