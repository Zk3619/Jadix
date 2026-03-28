use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::popup::register_last_popup_for_escape;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::tokens;

pub struct DialogBuilder {
    open: Signal<bool>,
    title: String,
    body: Option<Element>,
}

/// Styled modal panel; call inside `<Show when={open.get()}>` or reactive block. Registers Escape via [`register_last_popup_for_escape`].
pub fn dialog_panel(open: Signal<bool>, title: impl Into<String>) -> DialogBuilder {
    DialogBuilder {
        open,
        title: title.into(),
        body: None,
    }
}

impl DialogBuilder {
    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.body = Some(e.into());
        self
    }
}

impl From<DialogBuilder> for Element {
    fn from(builder: DialogBuilder) -> Self {
        register_last_popup_for_escape(builder.open);
        let mut panel: Element = vstack()
            .gap("0.75rem")
            .p("1.25rem")
            .rounded("0.5rem")
            .max_w("28rem")
            .into();
        panel = panel.bg(tokens::background());
        panel.style.z_index = 1000;
        panel.style.border_width = 1.0;
        panel.style.border_color = tokens::border();
        let title_el: Element = text(builder.title.as_str()).size(18.0).weight(600.0).into();
        panel = panel.child(title_el);
        if let Some(body) = builder.body {
            panel = panel.child(body);
        }
        panel
    }
}
