//! Lifecycle shared by non-Windows native platform views.
use super::{collect_specs, webview_status, WebViewSpec, WebViewStatus};
use crate::core::tree::Element;
use std::{collections::HashMap, sync::Arc};
use winit::{event::WindowEvent, window::Window};

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    all(target_os = "linux", not(target_env = "ohos"))
))]
#[path = "system_wry.rs"]
mod engine;
#[cfg(target_os = "android")]
#[path = "system_android.rs"]
mod engine;
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "android",
    all(target_os = "linux", not(target_env = "ohos"))
)))]
mod engine {
    use super::*;
    pub struct Browser;
    pub fn initialize() -> Result<(), String> {
        Ok(())
    }
    pub fn pump() {}
    impl Browser {
        pub fn new(_: &Arc<dyn Window>, _: &WebViewSpec, _: f32) -> Result<Self, String> {
            Err("This host needs a native platform WebView bridge".into())
        }
        pub fn update(&self, _: &WebViewSpec, _: f32) -> Result<(), String> {
            Ok(())
        }
        pub fn visible(&self, _: bool) {}
        pub fn poll(&self) -> Option<WebViewStatus> {
            None
        }
    }
}
struct Entry {
    browser: engine::Browser,
    spec: WebViewSpec,
}
pub(crate) struct NativeWebViews {
    entries: HashMap<String, Entry>,
    failures: HashMap<String, (String, Option<String>)>,
    initialized: Result<(), String>,
}
fn blocked() -> bool {
    crate::app::portal_overlay::top_stacked_panel_z().is_some()
        || crate::utils::drag::runtime::state().dragging
        || crate::utils::drag::routing::blocks_native_input()
}
impl NativeWebViews {
    pub(crate) fn new(_: &dyn Window) -> Self {
        Self {
            entries: HashMap::new(),
            failures: HashMap::new(),
            initialized: engine::initialize(),
        }
    }
    pub(crate) fn sync(
        &mut self,
        window: &Arc<dyn Window>,
        root: &Element,
        scale: f32,
        viewport: [f32; 4],
    ) {
        let specs = collect_specs(root, viewport);
        self.entries
            .retain(|id, _| specs.iter().any(|s| &s.id == id));
        self.failures
            .retain(|id, _| specs.iter().any(|s| &s.id == id));
        let covered = blocked();
        for spec in specs {
            let source = (spec.url.clone(), spec.html.clone());
            if self
                .failures
                .get(&spec.id)
                .is_some_and(|old| old != &source)
            {
                self.failures.remove(&spec.id);
            }
            if !self.entries.contains_key(&spec.id)
                && spec.visible
                && !self.failures.contains_key(&spec.id)
            {
                let result = self
                    .initialized
                    .clone()
                    .and_then(|()| engine::Browser::new(window, &spec, scale));
                match result {
                    Ok(browser) => {
                        self.entries.insert(
                            spec.id.clone(),
                            Entry {
                                browser,
                                spec: spec.clone(),
                            },
                        );
                    }
                    Err(error) => {
                        webview_status(&spec.id).try_set(WebViewStatus::Failed(error));
                        self.failures.insert(spec.id.clone(), source);
                    }
                }
            }
            if let Some(entry) = self.entries.get_mut(&spec.id) {
                if let Err(error) = entry.browser.update(&spec, scale) {
                    webview_status(&spec.id).try_set(WebViewStatus::Failed(error));
                }
                entry.browser.visible(spec.visible && !covered);
                entry.spec = spec;
            }
        }
    }
    // Platform views dispatch their own mouse/touch/keyboard events.
    pub(crate) fn handle_event(&mut self, _: &WindowEvent) -> bool {
        let covered = blocked();
        for entry in self.entries.values() {
            entry.browser.visible(entry.spec.visible && !covered);
        }
        false
    }
    pub(crate) fn pump(&mut self, window: &Arc<dyn Window>) -> bool {
        if self.entries.is_empty() || self.initialized.is_err() {
            return false;
        }
        engine::pump();
        let mut pending = false;
        for (id, entry) in &self.entries {
            if let Some(status) = entry.browser.poll() {
                let signal = webview_status(id);
                if signal.peek() != status {
                    signal.try_set(status);
                    window.request_redraw();
                }
            }
            pending |= webview_status(id).peek() == WebViewStatus::Loading;
        }
        // GTK owns sockets/timers that do not wake winit's X11 connection.
        pending
            || cfg!(any(
                target_os = "android",
                all(target_os = "linux", not(target_env = "ohos"))
            ))
    }
}
