//! Toggle activation and transient press state are separate from selection.
use super::{button, Element};
use jadix::portable::{use_scoped_signal, Signal};
use jadix::presentation::{interaction::{InteractionKind, InteractionPhase, InteractionPolicy}, keyboard::{KeyPhase, KeyboardPolicy}};
use std::rc::Rc;

pub struct ToggleBuilder { id: String, value: Signal<bool>, pressed: Option<Signal<bool>>, label: String, disabled: bool }
pub fn toggle(id: impl Into<String>, value: Signal<bool>) -> ToggleBuilder {
    ToggleBuilder { id: id.into(), value, pressed: None, label: String::new(), disabled: false }
}
impl ToggleBuilder {
    pub fn text(mut self, text: impl Into<String>) -> Self { self.label = text.into(); self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn pressed_signal(mut self, pressed: Signal<bool>) -> Self { self.pressed = Some(pressed); self }
    pub fn build(self) -> Element {
        let Self { id, value, pressed, label, disabled } = self;
        let pressed = pressed.unwrap_or_else(|| use_scoped_signal(&format!("{id}__pressed"), || false));
        let mut root: Element = button(&id).text(label).disabled(disabled).on_click(move || {
            if !disabled { value.set(!value.peek()); }
        }).into();
        root.descriptor.interaction = InteractionPolicy { press: !disabled, ..Default::default() };
        root.descriptor.keyboard = KeyboardPolicy { focusable: !disabled, key_down: !disabled, key_up: !disabled, ..Default::default() };
        root.descriptor.accessibility.disabled = disabled;
        root.descriptor_binding = Some(Rc::new(move |descriptor| {
            descriptor.accessibility.pressed = Some(value.get());
            descriptor.style.background = if pressed.get() { [32, 80, 160, 255] } else if value.get() { [44, 108, 224, 255] } else { [225, 228, 232, 255] };
        }));
        root.on_interaction(move |event| {
            if !disabled && event.kind == InteractionKind::Press {
                pressed.set(event.phase == InteractionPhase::Started);
            }
        }).on_key(move |event| {
            if !disabled && matches!(event.key.as_str(), " " | "Space" | "Enter") {
                pressed.set(event.phase == KeyPhase::Down);
            }
            if event.key == "Escape" { pressed.set(false); }
        }).and_on_focus_change(move |focused, _| { if !focused { pressed.set(false); } })
    }
}
impl From<ToggleBuilder> for Element { fn from(builder: ToggleBuilder) -> Self { builder.build() } }
