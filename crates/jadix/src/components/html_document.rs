//! Browser-backed HTML/CSS documents. The imported document cannot execute scripts.

use crate::core::{reactivity::Signal, tree::Element};

pub fn html_document_view(id: impl Into<String>, html: impl Into<String>) -> Element {
    crate::components::webview::webview_html(id, page(&html.into(), false, false))
}

pub fn html_document_view_with_assets(
    id: impl Into<String>,
    html: impl Into<String>,
    root: Option<&std::path::Path>,
) -> Element {
    crate::components::webview::webview_html(id, page_with_assets(&html.into(), false, false, root))
}

pub fn html_document_fragment(id: impl Into<String>, html: &str) -> Element {
    fragment(id.into(), html, None, || {})
}

pub fn html_document_fragment_with_assets(
    id: impl Into<String>,
    html: &str,
    root: Option<&std::path::Path>,
) -> Element {
    fragment(id.into(), html, root, || {})
}

fn fragment(
    id: String,
    html: &str,
    root: Option<&std::path::Path>,
    activate: impl Fn() + 'static,
) -> Element {
    let height = Signal::new(48.0_f32);
    let previous = std::cell::Cell::new(48.0_f32);
    crate::components::webview::webview_html_with_messages(
        id,
        page_with_assets(html, false, true, root)
            .replace("const fragment=false", "const fragment=true"),
        move |message| {
            if let Some(value) = message
                .strip_prefix("height:")
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
            {
                height.try_set(value.clamp(24.0, 16384.0));
            } else if message == "reveal-source" {
                activate();
            }
        },
    )
    .h(48.0)
    .reactive_sync(move |element, layout| {
        let next = height.peek();
        if previous.replace(next) != next {
            *element = std::mem::take(element).h(next);
            layout.mark_element_layout_dirty(element);
            crate::core::tree::LayoutStatus::LayoutChanged
        } else {
            crate::core::tree::LayoutStatus::Clean
        }
    })
}

pub(crate) fn html_document_source_view(
    id: String,
    html: &str,
    root: Option<&std::path::Path>,
    activate: impl Fn() + 'static,
) -> Element {
    fragment(id, html, root, activate)
}

pub fn html_document_editor(id: impl Into<String>, html: Signal<String>) -> Element {
    html_document_editor_with_assets(id, html, None)
}

pub fn html_document_editor_with_assets(
    id: impl Into<String>,
    html: Signal<String>,
    root: Option<&std::path::Path>,
) -> Element {
    crate::components::webview::webview_html_with_messages(
        id.into(),
        page_with_assets(&html.peek(), true, false, root),
        move |message| {
            if let Ok(value) = serde_json::from_str::<String>(&message) {
                html.try_set(value);
            }
        },
    )
}

fn page_with_assets(
    html: &str,
    editable: bool,
    reveal: bool,
    root: Option<&std::path::Path>,
) -> String {
    let images = crate::utils::document_assets::html_images(html, root);
    let images = serde_json::to_string(&images)
        .unwrap()
        .replace('<', "\\u003c");
    page(html, editable, reveal).replacen(
        "const localImages={}",
        &format!("const localImages={images}"),
        1,
    )
}

fn page(html: &str, editable: bool, reveal: bool) -> String {
    let source = serde_json::to_string(html).unwrap().replace('<', "\\u003c");
    include_str!("html_document.html")
        .replace("__EDITABLE__", if editable { "true" } else { "false" })
        .replace("__REVEAL_SOURCE__", if reveal { "true" } else { "false" })
        .replace("__DOCUMENT_SOURCE__", &source)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_keeps_full_document_inside_scriptless_frame() {
        let source = "<!doctype html><html><head><style>td{color:red}</style></head><body><table><tr><td>A &amp; B</td></tr></table><script>alert(1)</script></body></html>";
        let page = page(source, true, false);
        assert!(page.contains("sandbox=\"allow-same-origin\""));
        assert!(!page.contains("<script>alert"));
        assert!(page.contains("\\u003cstyle>td{color:red}"));
        assert!(page.contains("documentElement.cloneNode(true)"));
    }
}
