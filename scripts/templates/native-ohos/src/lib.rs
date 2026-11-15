use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::{create_root, OwnerHandle};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::EventPayload,
};

#[cfg(target_env = "ohos")]
mod ohos;

pub struct NativeApp<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    projection: ElementProjection,
    element: Element,
    count: Signal<usize>,
    value: Signal<String>,
    checked: Signal<bool>,
    owner: Option<OwnerHandle>,
    viewport: [f32; 2],
    environment: u64,
}

impl<Host: PlatformHost> NativeApp<Host> {
    pub fn new(
        host: Host,
        viewport: [f32; 2],
        count: usize,
        value: String,
    ) -> Result<Self, String> {
        if value.len() > 64 * 1024 {
            return Err("initial input is too large".into());
        }
        let root = NativeRoot::new(host);
        let projection = ElementProjection::with_lifetime(root.model().lifetime());
        let (owner, (count, value, checked, element)) = create_root(|| {
            let count = create_signal(count);
            let value = create_signal(value);
            let checked = create_signal(false);
            let increment: Element = button("increment")
                .child(text("Continue"))
                .on_click(move || count.set(count.get().saturating_add(1)))
                .into();
            let editor: Element = input("name")
                .placeholder("Name / 姓名")
                .value_signal(value)
                .into();
            let consent: Element = checkbox("consent")
                .checked_signal(checked)
                .child(text("Keep this preference"))
                .into();
            let element = vstack()
                .p(16.0)
                .gap(12.0)
                .child(text("@NAME@"))
                .child(
                    jadix_components::app::reactive::bind_text("count", move || {
                        format!("Count: {}", count.get())
                    })
                    .h(32.0),
                )
                .child(increment.h(44.0))
                .child(editor.h(48.0))
                .child(consent.h(48.0));
            (count, value, checked, element)
        });
        let mut app = Self {
            root,
            projection,
            element,
            count,
            value,
            checked,
            owner: Some(owner),
            viewport,
            environment: 1,
        };
        app.render()?;
        Ok(app)
    }

    fn render(&mut self) -> Result<(), String> {
        let mut nodes = vec![self
            .projection
            .snapshot(&mut self.element)
            .map_err(|error| format!("{error:?}"))?];
        self.root
            .present(&mut nodes, self.viewport, self.environment)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        self.viewport = viewport;
        self.environment = environment;
        self.render()
    }

    pub fn event(&mut self, json: &str) -> Result<(), String> {
        let event = jadix_native::wire::decode_event(json, self.root.model())
            .map_err(|error| error.to_string())?;
        self.root.event(&event).map_err(|error| error.to_string())?;
        if matches!(
            event.payload,
            EventPayload::Activate | EventPayload::Edit { .. } | EventPayload::Toggle { .. }
        ) {
            self.projection
                .dispatch(&mut self.element, &event)
                .map_err(|error| format!("{error:?}"))?;
            self.render()?;
        }
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<usize, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut changed = false;
        for event in &events {
            if matches!(
                event.payload,
                EventPayload::Activate | EventPayload::Edit { .. } | EventPayload::Toggle { .. }
            ) {
                self.projection
                    .dispatch(&mut self.element, event)
                    .map_err(|error| format!("{error:?}"))?;
                changed = true;
            }
        }
        if changed {
            self.render()?;
        }
        Ok(events.len())
    }

    pub fn state(&self) -> String {
        serde_json::json!({"schema": 1, "count": self.count.get(), "value": self.value.get(),
            "checked": self.checked.get()})
        .to_string()
    }

    pub fn restore(&mut self, json: &str) -> Result<(), String> {
        if json.len() > 128 * 1024 || self.root.model().lifetime().is_disposed() {
            return Err("saved state is too large or root is disposed".into());
        }
        let state: serde_json::Value =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        if state["schema"].as_u64() != Some(1) {
            return Err("unsupported saved state schema".into());
        }
        let count = state["count"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("invalid saved count")?;
        let value = state["value"]
            .as_str()
            .filter(|value| value.len() <= 64 * 1024)
            .ok_or("invalid saved input")?
            .to_string();
        let checked = state["checked"]
            .as_bool()
            .ok_or("invalid saved preference")?;
        self.count.set(count);
        self.value.set(value);
        self.checked.set(checked);
        self.render()
    }
}

impl<Host: PlatformHost> Drop for NativeApp<Host> {
    fn drop(&mut self) {
        self.root.dispose();
        self.projection.dispose();
        self.owner.take();
    }
}
