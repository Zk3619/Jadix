use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::{create_root, OwnerHandle};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::EventPayload,
};

#[cfg(all(
    feature = "field-workflow",
    any(
        feature = "gpu",
        feature = "notes",
        feature = "selection",
        feature = "popover",
        feature = "extended",
        feature = "large-list"
    )
))]
compile_error!("field-workflow requires its own native application without other sample features");

#[cfg(not(feature = "gpu"))]
pub mod field;

#[cfg(all(
    feature = "selection",
    any(
        feature = "gpu",
        feature = "extended",
        feature = "large-list",
        feature = "notes"
    )
))]
compile_error!("selection requires its own native controls page without other sample features");

#[cfg(all(
    feature = "popover",
    any(
        feature = "gpu",
        feature = "extended",
        feature = "large-list",
        feature = "notes",
        feature = "selection"
    )
))]
compile_error!("popover requires its own native controls page without other sample features");

#[cfg(feature = "popover")]
#[path = "popover.rs"]
mod popover_sample;

pub mod dock;
mod editing;
#[cfg(not(feature = "gpu"))]
pub mod notes;
pub mod table;
pub mod tree;
pub mod workbench;
#[cfg(all(
    feature = "notes",
    not(feature = "gpu"),
    not(feature = "field-workflow")
))]
#[cfg(any(target_os = "android", target_arch = "wasm32", feature = "uikit"))]
type PlatformApp<Host> = notes::NotesApp<Host>;
#[cfg(all(
    not(all(feature = "notes", not(feature = "gpu"))),
    not(feature = "field-workflow")
))]
#[cfg(any(target_os = "android", target_arch = "wasm32", feature = "uikit"))]
type PlatformApp<Host> = NativeApp<Host>;
#[cfg(all(feature = "field-workflow", not(feature = "gpu")))]
#[cfg(any(target_os = "android", target_arch = "wasm32", feature = "uikit"))]
type PlatformApp<Host> = field::FieldApp<Host>;

#[cfg(target_os = "android")]
mod android;
#[cfg(feature = "uikit")]
mod apple;
#[cfg(all(
    test,
    feature = "uikit",
    feature = "field-workflow",
    not(feature = "gpu")
))]
#[path = "tests/apple_field.rs"]
mod apple_field_tests;
#[cfg(all(test, feature = "uikit", feature = "notes", not(feature = "gpu")))]
#[path = "tests/apple_notes.rs"]
mod apple_notes_tests;
#[cfg(all(test, feature = "uikit", feature = "popover", not(feature = "gpu")))]
#[path = "tests/apple_popover.rs"]
mod apple_popover_tests;
#[cfg(all(test, feature = "uikit", feature = "selection", not(feature = "gpu")))]
#[path = "tests/apple_select.rs"]
mod apple_select_tests;
#[cfg(all(
    test,
    feature = "uikit",
    not(feature = "notes"),
    not(feature = "gpu"),
    not(feature = "selection"),
    not(feature = "popover"),
    not(feature = "field-workflow")
))]
#[path = "tests/apple.rs"]
mod apple_tests;
#[cfg(all(feature = "ohos", target_env = "ohos"))]
mod ohos;
#[cfg(all(test, not(feature = "gpu"), not(feature = "popover")))]
#[path = "tests/state.rs"]
mod state_tests;
#[cfg(target_arch = "wasm32")]
mod web;

pub struct NativeApp<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    projection: ElementProjection,
    element: Element,
    count: Signal<usize>,
    value: Signal<String>,
    extensions: Option<(Signal<bool>, Signal<String>)>,
    owner: Option<OwnerHandle>,
    viewport: [f32; 2],
    environment: u64,
}

#[allow(
    clippy::useless_conversion,
    reason = "shared authoring also compiles with GPU builders"
)]
#[cfg(any(not(feature = "selection"), feature = "gpu", feature = "extended"))]
fn label(value: impl ToString, height: f32) -> Element {
    let element: Element = text(value.to_string()).into();
    element.h(height).flex_shrink(0.0)
}

#[allow(
    clippy::useless_conversion,
    reason = "shared authoring also compiles with GPU builders"
)]
pub fn page(count: Signal<usize>, value: Signal<String>) -> Element {
    #[cfg(feature = "popover")]
    {
        popover_sample::page(count, value)
    }
    #[cfg(all(feature = "selection", not(feature = "gpu")))]
    {
        selection_page(count, value)
    }
    #[cfg(all(
        not(feature = "popover"),
        any(not(feature = "selection"), feature = "gpu")
    ))]
    {
        #[cfg(all(feature = "ohos", target_env = "ohos", not(feature = "gpu")))]
        let content = vstack()
            .child({
                let choices: Element = radio_group("native-appearance")
                    .default_value("system")
                    .options([
                        radio_option("system", "System / ????"),
                        radio_option("light", "Light / ??"),
                        radio_option("unavailable", "Unavailable").disabled(true),
                    ])
                    .into();
                choices
            })
            .child(editor_dialog(value))
            .child(page_content(count, value));
        #[cfg(not(all(feature = "ohos", target_env = "ohos", not(feature = "gpu"))))]
        let content = page_content(count, value);
        scroll_area("rows").child(content).into()
    }
}

#[cfg(all(feature = "selection", not(feature = "gpu")))]
const FRUIT_OPTIONS: [(&str, &str); 3] = [
    ("apple", "Apple / 苹果"),
    ("banana", "Banana / 香蕉"),
    ("blocked", "Disabled option"),
];

#[cfg(all(feature = "selection", not(feature = "gpu")))]
fn selection_page(count: Signal<usize>, value: Signal<String>) -> Element {
    let options = FRUIT_OPTIONS[..2]
        .iter()
        .map(|(value, label)| ((*value).into(), (*label).into()))
        .collect::<Vec<_>>();
    let choose = ui! {
        <Select id="fruit" value={value} placeholder="Choose fruit">
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
                <SelectItem value="apple">"Apple / 苹果"</SelectItem>
                <SelectItem value="banana">"Banana / 香蕉"</SelectItem>
                <SelectItem value="blocked" disabled=true>"Disabled option"</SelectItem>
            </SelectContent>
        </Select>
    };
    let disabled: Element = select("disabled-fruit")
        .options(options)
        .default_value("apple")
        .placeholder("Disabled fruit")
        .disabled(true)
        .into();
    let reset: Element = button("reset")
        .child(text("Reset selection"))
        .on_click(move || value.set(String::new()))
        .into();
    let increment: Element = button("increment")
        .child(text("Continue"))
        .on_click(move || count.set(count.get().saturating_add(1)))
        .into();
    vstack()
        .p(12.0)
        .gap(8.0)
        .child(text("Native Select"))
        .child(choose.h(44.0))
        .child(disabled.h(44.0))
        .child(reset.h(44.0))
        .child(increment.h(44.0))
        .child(jadix_components::app::reactive::bind_text(
            "selection-value",
            move || format!("Selected: {}", value.get()),
        ))
}

#[allow(
    clippy::useless_conversion,
    reason = "shared authoring also compiles with GPU builders"
)]
#[cfg(all(
    not(feature = "popover"),
    any(not(feature = "selection"), feature = "gpu", feature = "extended")
))]
fn page_content(count: Signal<usize>, value: Signal<String>) -> Element {
    let rows = if cfg!(feature = "large-list") {
        1000
    } else {
        100
    };
    vstack()
        .p(12.0)
        .gap(8.0)
        .flex_shrink(0.0)
        .child(label("Jadix native controls", 28.0))
        .child(
            jadix_components::app::reactive::bind_text("summary", move || {
                format!("Count: {} / Name: {}", count.get(), value.get())
            })
            .h(28.0)
            .flex_shrink(0.0),
        )
        .child({
            let button: Element = button("increment")
                .on_click(move || count.set(count.get().saturating_add(1)))
                .child(text("Continue"))
                .into();
            button.h(44.0).flex_shrink(0.0)
        })
        .child({
            let input: Element = input("name")
                .value_signal(value)
                .placeholder("Name / 中文")
                .on_submit(move || count.set(count.get().saturating_add(1)))
                .into();
            input.h(44.0).flex_shrink(0.0)
        })
        .child({
            let button: Element = button("reset")
                .on_click(move || value.set(String::new()))
                .child(text("Reset"))
                .into();
            button.h(44.0).flex_shrink(0.0)
        })
        .children((0..rows).map(|index| label(format!("Row {index:03} — 中文 😀"), 32.0)))
        .into()
}

#[cfg(all(
    not(feature = "gpu"),
    not(feature = "selection"),
    not(feature = "popover"),
    any(feature = "extended", all(feature = "ohos", target_env = "ohos"))
))]
fn editor_dialog(value: Signal<String>) -> Element {
    dialog("edit").child(move || {
        let trigger: Element = button("open-editor").child(text("Open editor")).into();
        let trigger = DialogTrigger(DialogTriggerProps {
            children: Some(trigger.h(44.0).flex_shrink(0.0)),
            ..Default::default()
        });
        let mut panel = dialog_content(DialogContentOpts::default(), move || {
            let field: Element = input("dialog-name")
                .value_signal(value)
                .placeholder("Dialog name / 中文")
                .into();
            let done: Element = button("dialog-done")
                .on_click(dialog_close)
                .child(text("Done"))
                .into();
            let content = vstack()
                .gap(12.0)
                .child(label("Edit details", 28.0))
                .child(field.h(44.0).flex_shrink(0.0))
                .child(done.h(44.0).flex_shrink(0.0))
                .child(dialog("confirm").child(|| {
                    let trigger: Element = button("open-confirm").child(text("Advanced")).into();
                    let trigger = DialogTrigger(DialogTriggerProps {
                        children: Some(trigger.h(44.0).flex_shrink(0.0)),
                        ..Default::default()
                    });
                    let mut nested = dialog_content(DialogContentOpts::default(), || {
                        let done: Element = button("return-editor")
                            .on_click(dialog_close)
                            .child(text("Return"))
                            .into();
                        vstack()
                            .gap(12.0)
                            .child(label("Confirm changes", 28.0))
                            .child(done.h(44.0).flex_shrink(0.0))
                    });
                    nested.descriptor.accessible_name = Some("Confirm changes".into());
                    vstack().flex_shrink(0.0).child(trigger).child(nested)
                }));
            DialogBody(DialogBodyProps {
                children: Some(content),
                ..Default::default()
            })
        });
        panel.descriptor.accessible_name = Some("Edit details".into());
        vstack().flex_shrink(0.0).child(trigger).child(panel)
    })
}

impl<Host: PlatformHost> NativeApp<Host> {
    pub fn new(
        host: Host,
        viewport: [f32; 2],
        count: usize,
        value: String,
    ) -> Result<Self, String> {
        let root = NativeRoot::new(host);
        let projection = ElementProjection::with_lifetime(root.model().lifetime());
        let (owner, (count, value, element, extensions)) = create_root(|| {
            let count = create_signal(count);
            let value = create_signal(value);
            #[cfg(not(all(feature = "extended", not(feature = "gpu"))))]
            let element = page(count, value);
            #[cfg(not(all(feature = "extended", not(feature = "gpu"))))]
            let extensions: Option<(Signal<bool>, Signal<String>)> = None;
            #[cfg(all(feature = "extended", not(feature = "gpu")))]
            let (element, extensions) = {
                let checked = create_signal(false);
                let note = create_signal(String::new());
                let password = create_signal(String::new());
                let element = vstack()
                    .p(12.0)
                    .gap(8.0)
                    .flex_shrink(0.0)
                    .child(label("Extended native controls", 28.0))
                    .child({
                        let image: Element = image()
                            .src(include_str!("../assets/pixel.txt").trim())
                            .alt("Native image")
                            .into();
                        image.w(24.0).h(24.0).flex_shrink(0.0)
                    })
                    .child({
                        let image: Element = image()
                            .src(include_str!("../assets/broken-pixel.txt").trim())
                            .alt("Unavailable image")
                            .into();
                        image.w(24.0).h(24.0).flex_shrink(0.0)
                    })
                    .child(
                        jadix_components::app::reactive::bind_text("checked", move || {
                            format!("Checked: {} / Note: {}", checked.get(), note.get())
                        })
                        .h(40.0)
                        .flex_shrink(0.0),
                    )
                    .child({
                        let checkbox: Element = checkbox("accept")
                            .checked_signal(checked)
                            .child(text("Accept"))
                            .into();
                        checkbox.h(44.0).flex_shrink(0.0)
                    })
                    .child({
                        let toggle: Element = switch("airplane-mode")
                            .checked_signal(checked)
                            .child(text("Airplane mode"))
                            .into();
                        toggle.h(44.0).flex_shrink(0.0)
                    })
                    .child({
                        let input: Element = input("password")
                            .value_signal(password)
                            .placeholder("Password / 密码")
                            .password(true)
                            .into();
                        input.h(44.0).flex_shrink(0.0)
                    })
                    .child({
                        let input: Element = textarea("note")
                            .value_signal(note)
                            .placeholder("Multiline / 多行")
                            .into();
                        input.h(96.0).flex_shrink(0.0)
                    })
                    .child(editor_dialog(value))
                    .child(page_content(count, value));
                (scroll_area("rows").child(element), Some((checked, note)))
            };
            (count, value, element, extensions)
        });
        let mut app = Self {
            root,
            projection,
            element,
            count,
            value,
            extensions,
            owner: Some(owner),
            viewport,
            environment: 1,
        };
        app.render()?;
        Ok(app)
    }

    pub fn render(&mut self) -> Result<(), String> {
        let mut nodes = vec![self
            .projection
            .snapshot(&mut self.element)
            .map_err(|error| format!("{error:?}"))?];
        self.root
            .present(&mut nodes, self.viewport, self.environment)
            .map_err(|error| error.to_string())?;
        #[cfg(not(feature = "gpu"))]
        if let Some(node) = self.projection.take_focus_request(&self.element) {
            self.root
                .focus(Some(node))
                .map_err(|error| error.to_string())?;
        }
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
        if self.dispatch_event(&event)? {
            self.render()?;
        }
        Ok(())
    }

    fn dispatch_event(
        &mut self,
        event: &jadix_native::presentation::PlatformEvent,
    ) -> Result<bool, String> {
        if matches!(
            event.payload,
            EventPayload::Activate
                | EventPayload::Edit { .. }
                | EventPayload::Submit { .. }
                | EventPayload::Select { .. }
                | EventPayload::Toggle { .. }
                | EventPayload::Dismiss { .. }
                | EventPayload::Navigate { .. }
        ) {
            self.projection
                .dispatch(&mut self.element, event)
                .map_err(|error| format!("{error:?}"))?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn poll_events(&mut self) -> Result<usize, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut changed = false;
        for event in &events {
            changed |= self.dispatch_event(event)?;
        }
        if changed {
            self.render()?;
        }
        Ok(events.len())
    }

    pub fn state(&self) -> String {
        let mut state = serde_json::json!({"count": self.count.get(), "value": self.value.get()});
        if let Some((checked, note)) = self.extensions {
            state["extensions"] = serde_json::json!({"checked": checked.get(), "note": note.get()});
        }
        state.to_string()
    }

    pub fn restore(&mut self, json: &str) -> Result<(), String> {
        if self.root.model().lifetime().is_disposed() {
            return Err("disposed native root".into());
        }
        if json.len() > 2 * jadix_native::wire::MAX_EVENT_BYTES {
            return Err("saved state too large".into());
        }
        let state: serde_json::Value =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        let count = state["count"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("invalid saved count")?;
        let value = state["value"]
            .as_str()
            .ok_or("invalid saved input")?
            .to_string();
        #[cfg(all(feature = "selection", not(feature = "gpu")))]
        if !value.is_empty() && !FRUIT_OPTIONS.iter().any(|(option, _)| *option == value) {
            return Err("invalid saved selection".into());
        }
        let extensions = if self.extensions.is_some() {
            Some((
                state["extensions"]["checked"]
                    .as_bool()
                    .ok_or("invalid saved checkbox")?,
                state["extensions"]["note"]
                    .as_str()
                    .ok_or("invalid saved note")?
                    .to_string(),
            ))
        } else {
            None
        };
        self.count.set(count);
        self.value.set(value);
        if let (Some((checked, note)), Some((next_checked, next_note))) =
            (self.extensions, extensions)
        {
            checked.set(next_checked);
            note.set(next_note);
        }
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
