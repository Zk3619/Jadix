//! Headless toggle (boolean) for Switch, Checkbox, toggle buttons.
//! Keyboard: [`crate::app::tree_hit::dispatch_key_for_focused`] maps Space/Enter to [`Element::on_click`].
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;

pub struct ToggleBuilder {
    id: String,
    pressed: Option<Signal<bool>>,
    default_pressed: bool,
    disabled: bool,
    on_change: Option<std::boxed::Box<dyn FnMut(bool)>>,
}

pub fn toggle(id: impl Into<String>) -> ToggleBuilder {
    ToggleBuilder {
        id: id.into(),
        pressed: None,
        default_pressed: false,
        disabled: false,
        on_change: None,
    }
}

impl ToggleBuilder {
    /// Controlled: external owns the boolean signal.
    pub fn pressed_signal(mut self, s: Signal<bool>) -> Self {
        self.pressed = Some(s);
        self
    }

    pub fn default_pressed(mut self, v: bool) -> Self {
        self.default_pressed = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn on_change(mut self, f: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(std::boxed::Box::new(f));
        self
    }
}

impl From<ToggleBuilder> for Element {
    fn from(builder: ToggleBuilder) -> Self {
        let scope = format!("{}__toggle", builder.id);
        let sig = builder.pressed.unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_pressed));
        let sig_click = sig;
        let mut on_change = builder.on_change;
        let disabled = builder.disabled;
        let id = builder.id;

        interactive(Element::new())
            .focus_id(id)
            .focusable(!disabled)
            .on_click(move || {
                if disabled {
                    return;
                }
                let next = !sig_click.get();
                sig_click.set(next);
                if let Some(cb) = on_change.as_mut() {
                    cb(next);
                }
            })
            .into()
    }
}
