//! Headless tabs: selected index signal + optional keyboard (Left/Right) when focused.
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;

/// Prefix for [`TabsHeadlessBuilder`] focus id. Mapped to AccessKit `TabList` in `accesskit_bridge`.
pub const TABSTRIP_FOCUS_PREFIX: &str = "tabstrip:";

pub struct TabsHeadlessBuilder {
    id: String,
    count: usize,
    selected: Option<Signal<usize>>,
    default_selected: usize,
}

pub fn tabs_headless(id: impl Into<String>, tab_count: usize) -> TabsHeadlessBuilder {
    TabsHeadlessBuilder {
        id: id.into(),
        count: tab_count.max(1),
        selected: None,
        default_selected: 0,
    }
}

impl TabsHeadlessBuilder {
    pub fn selected_signal(mut self, s: Signal<usize>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn default_selected(mut self, i: usize) -> Self {
        self.default_selected = i;
        self
    }
}

impl From<TabsHeadlessBuilder> for Element {
    fn from(builder: TabsHeadlessBuilder) -> Self {
        let scope = format!("{}__tabs", builder.id);
        let sel = builder
            .selected
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_selected.min(builder.count.saturating_sub(1))));
        let sel_k = sel;
        let n = builder.count;
        let focus_id = format!("{TABSTRIP_FOCUS_PREFIX}{}", builder.id);
        let mut el: Element = interactive(Element::new())
            .focus_id(focus_id)
            .focusable(true)
            .on_key_down(move |ev| {
                use winit::event::ElementState;
                use winit::keyboard::{Key, NamedKey};
                if ev.state != ElementState::Pressed || ev.repeat {
                    return false;
                }
                let mut i = sel_k.get();
                match ev.logical_key {
                    Key::Named(NamedKey::ArrowRight) => {
                        i = (i + 1).min(n.saturating_sub(1));
                        sel_k.set(i);
                        true
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        i = i.saturating_sub(1);
                        sel_k.set(i);
                        true
                    }
                    _ => false,
                }
            })
            .into();
        el.template_id = Some(builder.id);
        el
    }
}
