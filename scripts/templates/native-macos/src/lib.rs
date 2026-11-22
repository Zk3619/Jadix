use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::{create_root, OwnerHandle};
use jadix_native::{host::{NativeRoot, PlatformHost}, appkit::AppKitWindowContent};

// Edit this function to build your application's retained native component tree.
fn page(count: Signal<usize>, value: Signal<String>) -> Element {
    vstack().p(20.0).gap(12.0)
        .child(jadix_components::app::reactive::bind_text("count", move || format!("Count: {}", count.get())).h(32.0))
        .child(input("name").value_signal(value).placeholder("Your name"))
        .child(button("increment").on_click(move || count.set(count.get().saturating_add(1))).child(text("Continue")))
        .into()
}
struct PlatformApp<H: PlatformHost> {
    root: NativeRoot<H>, projection: ElementProjection, element: Element,
    _owner: OwnerHandle, count: Signal<usize>, value: Signal<String>, viewport: [f32; 2], environment: u64,
}
impl<H: PlatformHost> PlatformApp<H> {
    fn new(host: H, viewport: [f32; 2], initial: usize, text: String) -> Result<Self, String> {
        let root = NativeRoot::new(host);
        let projection = ElementProjection::with_lifetime(root.model().lifetime()).with_locale_context(root.locale_context());
        let (owner, (count, value, element)) = create_root(|| {
            if let Some(windows) = root.host().window_declarations() { jadix_core::window::provide_window_declarations(windows); }
            let count = create_signal(initial); let value = create_signal(text);
            (count, value, page(count, value))
        });
        let mut app = Self {root, projection, element, _owner: owner, count, value, viewport, environment: 0};
        app.render()?; Ok(app)
    }
    fn new_with_content(host: H, viewport: [f32; 2], _content: AppKitWindowContent) -> Result<Self, String> {
        Self::new(host, viewport, 0, String::new())
    }
    fn render(&mut self) -> Result<(), String> {
        let mut nodes = vec![self.projection.snapshot(&mut self.element).map_err(|e| format!("{e:?}"))?];
        self.root.present(&mut nodes, self.viewport, self.environment).map_err(|e| e.to_string())?;
        if let Some(node) = self.projection.take_focus_request(&self.element) { self.root.focus(Some(node)).map_err(|e| e.to_string())?; }
        Ok(())
    }
    fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        self.viewport = viewport; self.environment = environment; self.render()
    }
    fn event(&mut self, json: &str) -> Result<(), String> {
        let event = jadix_native::wire::decode_event(json, self.root.model()).map_err(|e| e.to_string())?;
        self.root.event(&event).map_err(|e| e.to_string())?;
        self.projection.dispatch(&mut self.element, &event).map_err(|e| format!("{e:?}"))?;
        self.render()
    }
    fn state(&self) -> String { serde_json::json!({"count":self.count.get(), "value":self.value.get()}).to_string() }
    fn restore(&mut self, json: &str) -> Result<(), String> {
        if json.len() > 32768 { return Err("state exceeds budget".into()); }
        let state: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let count = state["count"].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("invalid count")?;
        let value = state["value"].as_str().ok_or("invalid value")?.to_owned();
        self.count.set(count); self.value.set(value); self.render()
    }
}
#[allow(unexpected_cfgs)]
mod bridge { use super::PlatformApp; jadix_native::appkit_application!(PlatformApp); }
