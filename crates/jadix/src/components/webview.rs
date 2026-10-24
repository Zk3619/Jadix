//! Embedded native websites: WebView2 composition on Windows, WKWebView on
//! macOS/iOS, WebKitGTK on Linux/X11, and android.webkit.WebView on Android.
//! All backends consume the same logical bounds, clipping and lifecycle data.

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use jadix_macros::component;
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

pub(crate) fn webview_html_with_messages(
    id: String,
    html: String,
    handler: impl Fn(String) + 'static,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "webview");
    let key = (crate::app::window_scope::active(), id.clone());
    let handler: Rc<dyn Fn(String)> = Rc::new(handler);
    MESSAGES.with(|handlers| handlers.borrow_mut().insert(key.clone(), handler.clone()));
    let mut element = build_webview(id.clone(), String::new(), Some(html));
    element.set_attr("jadix_webview_messages", "1");
    let lifetime = Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_none()
        .on_unmount(move || {
            MESSAGES.with(|handlers| {
                let mut handlers = handlers.borrow_mut();
                if handlers
                    .get(&key)
                    .is_some_and(|current| Rc::ptr_eq(current, &handler))
                {
                    handlers.remove(&key);
                }
            });
        });
    element.child(lifetime)
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
pub(super) fn webview_snapshot(id: &str) -> Signal<bool> {
    use_signal(&format!("{id}__webview_snapshot"), || false)
}

pub fn webview(id: impl Into<String>, url: impl Into<String>) -> Element {
    build_webview(id.into(), url.into(), None)
}

pub fn webview_html(id: impl Into<String>, html: impl Into<String>) -> Element {
    build_webview(id.into(), String::new(), Some(html.into()))
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn WebView(id: String, url: String, html: Option<String>) -> Element {
    build_webview(id, url, html)
}

fn build_webview(id: String, url: String, html: Option<String>) -> Element {
    let id = crate::utils::resolve_instance_id(id, "webview");
    let status = webview_status(&id);
    #[cfg(windows)]
    let snapshot = webview_snapshot(&id);
    #[cfg(windows)]
    let texture = format!("{id}__webview_snapshot_texture");
    let message =
        crate::app::reactive::reactive_view(format!("{id}__webview_message"), move || {
            #[cfg(windows)]
            if snapshot.get() {
                let mut image = Element::new().w("100%").h("100%").pointer_events_none();
                image.image_texture = Some(texture.clone());
                return image;
            }
            let text = match status.get() {
                WebViewStatus::Loading => "Loading website…".to_string(),
                WebViewStatus::Ready => String::new(),
                WebViewStatus::Failed(message) => format!("WebView: {message}"),
                WebViewStatus::Unsupported => {
                    "No native WebView backend is available for this window host.".to_string()
                }
            };
            vstack()
                .w("100%")
                .h("100%")
                .items("center")
                .justify_center()
                .child(Element::new().text(text))
                .into()
        });
    let mut element: Element = vstack()
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .overflow("hidden")
        .child(message)
        .into();
    element.set_attr("data-slot", "webview");
    element.set_attr("jadix_webview_id", id);
    element.set_attr("jadix_webview_url", url);
    if let Some(html) = html {
        element.set_attr("jadix_webview_html", html);
    }
    element
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_message_handler_survives_reactive_root_hook_replacement() {
        let (_owner, ()) = crate::core::reactivity::create_root(|| {
            let received = Signal::new(String::new());
            let element = webview_html_with_messages(
                "message-lifetime".into(),
                "<p>test</p>".into(),
                move |message| received.set(message),
            );
            let element = element.on_unmount(|| {});
            deliver_message("message-lifetime", "saved".into());
            assert_eq!(received.peek(), "saved");
            drop(element);
            deliver_message("message-lifetime", "stale".into());
            assert_eq!(received.peek(), "saved");
        });
    }

    #[test]
    fn geometry_preserves_document_size_and_clips_the_visible_region() {
        let mut child = Element::new();
        child.set_attr("jadix_webview_id", "geometry");
        child.layout_bounds = [10.0, 20.0, 300.0, 200.0];
        let mut root = Element::new().overflow("hidden").child(child);
        root.layout_bounds = [0.0, 0.0, 150.0, 100.0];
        let specs = collect_specs(&root, [0.0, 0.0, 800.0, 600.0]);
        assert_eq!(specs[0].bounds, [10.0, 20.0, 300.0, 200.0]);
        assert_eq!(specs[0].clip, [10.0, 20.0, 140.0, 80.0]);
        assert!(specs[0].visible);
        root.set_attr("jadix_nav_buried", "1");
        assert!(!collect_specs(&root, [0.0, 0.0, 800.0, 600.0])[0].visible);
    }
}
