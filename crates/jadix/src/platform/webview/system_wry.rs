//! WKWebView and WebKitGTK. Native input stays with the system web engine.
use super::super::{webview_status, WebViewSpec, WebViewStatus};
use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder,
};
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "system_apple_clip.rs"]
mod apple;

pub fn initialize() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    gtk::init().map_err(|e| format!("WebKitGTK initialization: {e}"))?;
    Ok(())
}
pub fn pump() {
    #[cfg(target_os = "linux")]
    for _ in 0..32 {
        if !gtk::events_pending() {
            break;
        }
        gtk::main_iteration_do(false);
    }
}
fn rect(bounds: [f32; 4]) -> Rect {
    let [x, y, w, h] = bounds;
    Rect {
        position: LogicalPosition::new(x as f64, y as f64).into(),
        size: LogicalSize::new(w.max(1.0) as f64, h.max(1.0) as f64).into(),
    }
}
pub struct Browser {
    view: WebView,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    clip: apple::ClipView,
    source: RefCell<(String, Option<String>)>,
    pending_navigation: Arc<Mutex<Option<String>>>,
}
impl Browser {
    pub fn new(window: &Arc<dyn Window>, spec: &WebViewSpec, _: f32) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        if !matches!(
            window.window_handle().map_err(|e| e.to_string())?.as_raw(),
            RawWindowHandle::Xlib(_)
        ) {
            return Err("WebKitGTK embedding currently requires an X11 window; Wayland needs a GTK host bridge".into());
        }
        let id = spec.id.clone();
        let wake = window.clone();
        let pending_navigation = Arc::new(Mutex::new(None));
        let pending = pending_navigation.clone();
        let popup_wake = window.clone();
        let message_id = spec.id.clone();
        let messages = spec.messages;
        let message_wake = window.clone();
        let scope = crate::app::window_scope::active();
        let builder = WebViewBuilder::new()
            .with_ipc_handler(move |request| {
                if messages {
                    let _scope = crate::app::window_scope::enter(scope);
                    super::super::deliver_message(&message_id, request.body().clone());
                    message_wake.request_redraw();
                }
            })
            .with_visible(false)
            .with_bounds(rect(spec.bounds))
            .with_new_window_req_handler(move |url, _| {
                *pending.lock().unwrap() = Some(url);
                popup_wake.request_redraw();
                NewWindowResponse::Deny
            })
            .with_on_page_load_handler(move |event, _| {
                webview_status(&id).try_set(match event {
                    PageLoadEvent::Started => WebViewStatus::Loading,
                    PageLoadEvent::Finished => WebViewStatus::Ready,
                });
                wake.request_redraw();
            });
        let builder = if let Some(html) = &spec.html {
            builder.with_html(html)
        } else {
            builder.with_url(&spec.url)
        };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let clip = apple::ClipView::new(window.as_ref())?;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let view = builder.build_as_child(&clip).map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        let view = builder.build_as_child(&window).map_err(|e| e.to_string())?;
        let browser = Self {
            view,
            source: RefCell::new((spec.url.clone(), spec.html.clone())),
            pending_navigation,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            clip,
        };
        browser.update(spec, 1.0)?;
        Ok(browser)
    }
    pub fn update(&self, spec: &WebViewSpec, _: f32) -> Result<(), String> {
        let source = (spec.url.clone(), spec.html.clone());
        if *self.source.borrow() != source {
            if let Some(html) = &spec.html {
                self.view.load_html(html)
            } else {
                self.view.load_url(&spec.url)
            }
            .map_err(|e| e.to_string())?;
            *self.source.borrow_mut() = source;
            webview_status(&spec.id).try_set(WebViewStatus::Loading);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        self.clip.update(&self.view, spec)?;
        #[cfg(target_os = "linux")]
        {
            use gtk::prelude::*;
            use wry::WebViewExtUnix;
            self.view
                .set_bounds(rect(spec.bounds))
                .map_err(|e| e.to_string())?;
            // Shape the native child, preserving the full document viewport.
            if let Some(top) = self
                .view
                .webview()
                .toplevel()
                .and_then(|w| w.downcast::<gtk::Window>().ok())
            {
                if let Some(window) = top.window() {
                    let [x, y, w, h] = spec.clip;
                    let region =
                        gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
                            (x - spec.bounds[0]).round() as i32,
                            (y - spec.bounds[1]).round() as i32,
                            w.round().max(0.0) as i32,
                            h.round().max(0.0) as i32,
                        ));
                    window.shape_combine_region(Some(&region), 0, 0);
                    window.input_shape_combine_region(Some(&region), 0, 0);
                }
            }
        }
        Ok(())
    }
    pub fn visible(&self, visible: bool) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        self.clip.visible(visible);
        let _ = self.view.set_visible(visible);
    }
    pub fn poll(&self) -> Option<WebViewStatus> {
        let url = self.pending_navigation.lock().unwrap().take()?;
        Some(match self.view.load_url(&url) {
            Ok(()) => WebViewStatus::Loading,
            Err(error) => WebViewStatus::Failed(error.to_string()),
        })
    }
}
