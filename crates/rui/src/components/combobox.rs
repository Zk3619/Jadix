//! # Combobox
//!
//! 可过滤下拉：Input 触发器 + 选项列表；复用 portal / floating sync。
//! 打开时 ArrowUp/Down 走 [`crate::utils::roving`]（roving tabindex）。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::utils::text_field::bind_controlled_value;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::tokens;
use crate::utils::popup::{
    mark_floating, panel_block_clicks, PopupPlacement,
};
use crate::utils::roving::{roving_edge, roving_move};
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
        let active_key = format!("{id}__combo_active");
        let active = use_signal(active_key.as_str(), || 0usize);
        let vk = input_value_signal_key(&id);
        if let Some(sig) = value {
            bind_controlled_value(&id, sig);
        } else {
            let _ = use_signal(vk.as_str(), String::new);
        }
        let view_id = format!("{id}__combo_view");
        let trigger_fid = format!("input:{id}");

        reactive_view(view_id, move || {
            let val_sig = use_signal(input_value_signal_key(&id).as_str(), String::new);
            let query = val_sig.get();
            let _ = open.get();
            let active_ix = active.get();
            let o_focus = open;
            let a_sig = active;
            let opts_for_keys = options.clone();
            let mut trigger: Element = input(&id).placeholder(placeholder.as_str()).into();
            trigger = trigger
                .accesskit_role("combobox")
                .accesskit_name(placeholder.as_str());
            trigger = trigger
                .on_click(move || {
                    a_sig.set(0);
                    o_focus.set(true);
                })
                .on_key_down(move |ev| {
                    let q = val_sig.peek().to_ascii_lowercase();
                    let filtered: Vec<_> = opts_for_keys
                        .iter()
                        .filter(|(_, l)| q.is_empty() || l.to_ascii_lowercase().contains(&q))
                        .cloned()
                        .collect();
                    let n = filtered.len();
                    match &ev.logical_key {
                        Key::Named(NamedKey::ArrowDown) => {
                            if !o_focus.peek() {
                                a_sig.set(0);
                                o_focus.set(true);
                            } else if let Some(next) = roving_move(n, a_sig.peek(), 1) {
                                a_sig.set(next);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if o_focus.peek() {
                                if let Some(next) = roving_move(n, a_sig.peek(), -1) {
                                    a_sig.set(next);
                                }
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Home) if o_focus.peek() => {
                            if let Some(i) = roving_edge(n, false) {
                                a_sig.set(i);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::End) if o_focus.peek() => {
                            if let Some(i) = roving_edge(n, true) {
                                a_sig.set(i);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::Enter) => {
                            if !o_focus.peek() {
                                a_sig.set(0);
                                o_focus.set(true);
                                return true;
                            }
                            if let Some((ov, _)) = filtered.get(a_sig.peek()) {
                                val_sig.set(ov.clone());
                                o_focus.set(false);
                                return true;
                            }
                        }
                        Key::Named(NamedKey::Escape) => {
                            o_focus.set(false);
                            return true;
                        }
                        _ => {}
                    }
                    false
                });

            let mut root = vstack().child(trigger);
            if let Some(oz) = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked) {
                if open.peek() {
                    let q = query.to_ascii_lowercase();
                    let filtered: Vec<_> = options
                        .iter()
                        .filter(|(_, l)| q.is_empty() || l.to_ascii_lowercase().contains(&q))
                        .cloned()
                        .collect();
                    let mut rows = vstack().w("100%");
                    rows.el.attrs
                        .insert("accesskit_role".to_string(), "listbox".to_string());
                    for (i, (ov, ol)) in filtered.iter().enumerate() {
                        let v_sig = val_sig;
                        let o_sig = open;
                        let a_row = active;
                        let ov = ov.clone();
                        let ol = ol.clone();
                        let bid = format!("{id}__opt{i}");
                        let highlighted = i == active_ix;
                        let mut row: Element = button(&bid)
                            .variant(if highlighted { "secondary" } else { "ghost" })
                            .size("sm")
                            .w("100%")
                            .on_click(move || {
                                a_row.set(i);
                                v_sig.set(ov.clone());
                                o_sig.set(false);
                            })
                            .text(ol)
                            .into();
                        row.attrs
                            .insert("accesskit_role".to_string(), "option".to_string());
                        rows = rows.child(row);
                    }
                    let scroll_id = format!("{id}:menu_scroll");
                    // ShadCN ComboboxContent: w = anchor, min-w = anchor+1.75rem, max-h-96.
                    let mut menu: Element = scroll_area(&scroll_id)
                        .overflow_y("auto")
                        .w("auto")
                        .h("auto")
                        .child(rows)
                        .into();
                    menu = menu.max_h("24rem");
                    menu = menu
                        .bg(tokens::background())
                        .rounded("0.375rem")
                        .border_style("solid")
                        .border_width(1.0)
                        .border_color(tokens::border())
                        .absolute();
                    // ListBox role lives on the options stack (`rows`); outer chrome stays unlabeled.
                    mark_floating(&mut menu, &id, &trigger_fid, placement);
                    menu.set_attr("rui_floating_match_trigger", "1");
                    menu.set_attr("rui_floating_min_w", "trigger+1.75rem");
                    menu.set_attr("rui_floating_max_h", "24rem");
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
