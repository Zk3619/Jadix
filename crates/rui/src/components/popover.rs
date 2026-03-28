use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::popup::{panel_block_clicks, register_last_popup_for_escape};
use crate::primitives::r#box::r#box;

pub struct PopoverBuilder {
    open: Signal<bool>,
    body: Option<Element>,
    z: i32,
}

/// Anchored-style floating panel (non-modal). Registers Escape with the popup stack.
pub fn popover_panel(open: Signal<bool>) -> PopoverBuilder {
    PopoverBuilder {
        open,
        body: None,
        z: 950,
    }
}

impl PopoverBuilder {
    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.body = Some(e.into());
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z = z;
        self
    }
}

impl From<PopoverBuilder> for Element {
    fn from(builder: PopoverBuilder) -> Self {
        register_last_popup_for_escape(builder.open);
        if !builder.open.get() {
            return Element::new();
        }
        let mut inner: Element = r#box()
            .p("0.75rem")
            .rounded("0.375rem")
            .max_w("20rem")
            .into();
        inner.style.border_width = 1.0;
        inner.style.border_color = crate::tokens::border();
        inner = inner.bg(crate::tokens::background());
        if let Some(body) = builder.body {
            inner = inner.child(body);
        }
        panel_block_clicks(inner, builder.z)
    }
}
