//! Embedded native websites: WebView2 composition on Windows, WKWebView on
//! macOS/iOS, WebKitGTK on Linux/X11, and android.webkit.WebView on Android.
//! All backends consume the same logical bounds, clipping and lifecycle data.

use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use jadix_macros::component;

#[cfg(test)]
use crate::core::reactivity::Signal;
#[cfg(windows)]
use crate::platform::webview::webview_snapshot;
#[cfg(test)]
use crate::platform::webview::{collect_specs, deliver_message};
pub use crate::platform::webview::{webview_status, webview_supported, WebViewStatus};

pub(crate) fn webview_html_with_messages(
    id: String,
    html: String,
    handler: impl Fn(String) + 'static,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "webview");
    let lifetime = crate::platform::webview::message_lifetime(&id, handler);
    let mut element = build_webview(id, String::new(), Some(html));
    element.set_attr("jadix_webview_messages", "1");
    element.child(lifetime)
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
    let content = crate::platform::webview::surface(id, url, html).child(message);
    let mut element = crate::authoring::SlotMap::default().layers(
        Some(Element::new().bg("background")), content, None,
    ).w("100%").h("100%").min_w(0.0).min_h(0.0);
    element.set_attr("data-slot", "webview");
    element
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
