use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::popup::register_last_popup_for_escape;
use crate::primitives::r#box::vstack;

pub struct DropdownBuilder {
    id: String,
    open: Option<Signal<bool>>,
    trigger: Element,
    menu: Element,
}

/// Vertical stack: `trigger` then `menu`. Call [`register_last_popup_for_escape`] on the returned `open` signal (set here) so Escape closes. Toggle `open` from the trigger’s `on_click`.
pub fn dropdown(id: impl Into<String>, trigger: impl Into<Element>, menu: impl Into<Element>) -> DropdownBuilder {
    DropdownBuilder {
        id: id.into(),
        open: None,
        trigger: trigger.into(),
        menu: menu.into(),
    }
}

impl DropdownBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
}

impl From<DropdownBuilder> for Element {
    fn from(builder: DropdownBuilder) -> Self {
        let scope = format!("{}__dropdown_open", builder.id);
        let open = builder
            .open
            .unwrap_or_else(|| use_signal(scope.as_str(), || false));
        register_last_popup_for_escape(open);
        let mut menu = builder.menu;
        menu.style.z_index = 950;
        vstack().child(builder.trigger).child(menu).into()
    }
}
