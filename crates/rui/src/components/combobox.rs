//! # Combobox
//!
//! 可过滤下拉：Input 触发器 + 选项列表；复用 portal / floating sync。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::tokens;
use crate::utils::popup::{
    mark_floating, panel_block_clicks, underlay_close_on_click, PopupPlacement,
};
use winit::keyboard::{Key, NamedKey};

pub struct ComboboxBuilder {
    id: String,
    value: Option<Signal<String>>,
    options: Vec<(String, String)>,
    placeholder: String,
    placement: PopupPlacement,
}

pub fn combobox(id: impl Into<String>) -> ComboboxBuilder {
    ComboboxBuilder {
        id: id.into(),
        value: None,
        options: Vec::new(),
        placeholder: "Search…".into(),
        placement: PopupPlacement::BottomStart,
    }
}

impl ComboboxBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn options(
        mut self,
        opts: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.options = opts
            .into_iter()
            .map(|(v, l)| (v.into(), l.into()))
            .collect();
        self
    }

    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }
}

impl From<ComboboxBuilder> for Element {
    fn from(builder: ComboboxBuilder) -> Self {
        let ComboboxBuilder {
            id,
            value,
            options,
            placeholder,
            placement,
        } = builder;
        let open_key = format!("{id}__combo_open");
        let open = use_signal(open_key.as_str(), || false);
        let vk = input_value_signal_key(&id);
        let val = value.unwrap_or_else(|| use_signal(vk.as_str(), String::new));
        // Keep external signal in sync with input's keyed store when provided.
        let _ = val;
        let view_id = format!("{id}__combo_view");
        let trigger_fid = format!("input:{id}");

        reactive_view(view_id, move || {
            let val_sig = use_signal(input_value_signal_key(&id).as_str(), String::new);
            let query = val_sig.get();
            let _ = open.get();
            let o_focus = open;
            let mut trigger: Element = input(&id).placeholder(placeholder.as_str()).into();
            trigger = trigger
                .on_click(move || o_focus.set(true))
                .on_key_down(move |ev| {
                    if matches!(
                        ev.logical_key,
                        Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::Enter)
                    ) {
                        o_focus.set(true);
                        return true;
                    }
                    if matches!(ev.logical_key, Key::Named(NamedKey::Escape)) {
                        o_focus.set(false);
                        return true;
                    }
                    false
                });

            let mut root = vstack().child(trigger);
            if let Some(oz) = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked) {
                if open.peek() {
                    root = root.child(underlay_close_on_click(open, oz.underlay));
                    let q = query.to_ascii_lowercase();
                    let filtered: Vec<_> = options
                        .iter()
                        .filter(|(_, l)| q.is_empty() || l.to_ascii_lowercase().contains(&q))
                        .cloned()
                        .collect();
                    let mut rows = vstack().w("100%");
                    for (i, (ov, ol)) in filtered.iter().enumerate() {
                        let v_sig = val_sig;
                        let o_sig = open;
                        let ov = ov.clone();
                        let ol = ol.clone();
                        let bid = format!("{id}__opt{i}");
                        let row: Element = button(&bid)
                            .variant("ghost")
                            .size("sm")
                            .w("100%")
                            .on_click(move || {
                                v_sig.set(ov.clone());
                                o_sig.set(false);
                            })
                            .text(ol)
                            .into();
                        rows = rows.child(row);
                    }
                    let scroll_id = format!("{id}:menu_scroll");
                    let mut menu: Element = scroll_area(&scroll_id)
                        .overflow_y("auto")
                        .w("100%")
                        .child(rows)
                        .into();
                    menu = menu.max_h("12rem");
                    menu = menu
                        .bg(tokens::background())
                        .rounded("0.375rem")
                        .border_style("solid")
                        .border_width(1.0)
                        .border_color(tokens::border())
                        .absolute();
                    mark_floating(&mut menu, &id, &trigger_fid, placement);
                    menu.set_attr("rui_floating_match_trigger", "1");
                    menu = panel_block_clicks(menu, oz.panel);
                    root = root.child(menu);
                }
            }
            root.into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combobox_builds() {
        let _el: Element = combobox("cb1")
            .options([("a", "Alpha"), ("b", "Beta")])
            .into();
    }
}
