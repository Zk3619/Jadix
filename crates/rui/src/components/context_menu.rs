//! # ContextMenu
//!
//! 右键菜单：触发器 `on_context_menu` 打开；underlay 点击关闭；Menu overlay。
//! 打开时 ArrowUp/Down / Home / End 走 [`crate::utils::roving`] 高亮菜单项。

use std::rc::Rc;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    mark_floating, panel_block_clicks, PopupPlacement,
};
use crate::utils::roving::{roving_edge, roving_move};
use winit::keyboard::{Key, NamedKey};

#[derive(Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

pub struct ContextMenuBuilder {
    id: String,
    items: Vec<ContextMenuItem>,
    open: Option<Signal<bool>>,
    placement: PopupPlacement,
}

pub fn context_menu(id: impl Into<String>) -> ContextMenuBuilder {
    ContextMenuBuilder {
        id: id.into(),
        items: Vec::new(),
        open: None,
        placement: PopupPlacement::BottomStart,
    }
}

impl ContextMenuBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    pub fn item(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.items.push(ContextMenuItem {
            label: label.into(),
            on_click: Rc::new(on_click),
        });
        self
    }

    pub fn trigger(self, build: impl FnOnce() -> Element) -> Element {
        let open_key = format!("{}__ctx_open", self.id);
        let open = self
            .open
            .unwrap_or_else(|| use_signal(open_key.as_str(), || false));
        let active_key = format!("{}__ctx_active", self.id);
        let active = use_signal(active_key.as_str(), || 0usize);
        let trigger_fid = format!("{}__ctx_trigger", self.id);
        let mut trigger = build();
        if trigger.focus_id.is_none() {
            trigger = trigger.focus_id(trigger_fid.as_str());
        }
        // Custom (non-Button) triggers need an explicit focus-visible ring.
        if trigger.button_hover_paint.is_none() && trigger.input_paint_sync.is_none() {
            trigger.focusable = true;
            trigger.focus_ring = true;
        }
        let trigger_fid = trigger.focus_id.clone().unwrap_or(trigger_fid);
        let open_ctx = open;
        let active_open = active;
        let fid_focus = trigger_fid.clone();
        trigger = trigger.on_context_menu(move |_ev| {
            active_open.set(0);
            // Mouse open: focus trigger for arrow-key roving, but keep focus-visible false (no ring).
            use_signal("__rui_button_focus_id", String::new).set(fid_focus.clone());
            open_ctx.set(true);
        });
        let open_keys = open;
        let active_keys = active;
        let items_for_keys = self.items.clone();
        let n_items = items_for_keys.len();
        trigger = trigger.on_key_down(move |ev| {
            if !open_keys.peek() {
                return false;
            }
            match &ev.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    if let Some(next) = roving_move(n_items, active_keys.peek(), 1) {
                        active_keys.set(next);
                    }
                    true
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if let Some(next) = roving_move(n_items, active_keys.peek(), -1) {
                        active_keys.set(next);
                    }
                    true
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(i) = roving_edge(n_items, false) {
                        active_keys.set(i);
                    }
                    true
                }
                Key::Named(NamedKey::End) => {
                    if let Some(i) = roving_edge(n_items, true) {
                        active_keys.set(i);
                    }
                    true
                }
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                    let ix = active_keys.peek();
                    if let Some(it) = items_for_keys.get(ix) {
                        (it.on_click)();
                        open_keys.set(false);
                        return true;
                    }
                    false
                }
                _ => false,
            }
        });

        let id = self.id.clone();
        let items = self.items;
        let placement = self.placement;
        let view_id = format!("{id}__ctx_view");
        let menu_open = open;

        let menu = reactive_view(view_id, move || {
            let is_open = menu_open.get();
            let active_ix = active.get();
            let Some(oz) = portal_sync(&id, menu_open, OverlayKind::Menu, OverlayMode::Stacked)
            else {
                return Element::new();
            };
            if !is_open {
                return Element::new();
            }
            let mut rows = vstack().w("10rem");
            for (i, it) in items.iter().enumerate() {
                let cb = it.on_click.clone();
                let o = menu_open;
                let a_row = active;
                let label = it.label.clone();
                let bid = format!("{id}__item{i}");
                let highlighted = i == active_ix;
                let mut row: Element = button(&bid)
                    .variant(if highlighted { "secondary" } else { "ghost" })
                    .size("sm")
                    .w("100%")
                    .on_click(move || {
                        a_row.set(i);
                        cb();
                        o.set(false);
                    })
                    .text(label)
                    .into();
                row = row.accesskit_role("menuitem");
                rows = rows.child(row);
            }
            let mut panel: Element = r#box()
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border())
                .bg(tokens::background())
                .p("0.25rem")
                .child(rows)
                .into();
            panel = panel.accesskit_role("menu");
            mark_floating(&mut panel, &id, &trigger_fid, placement);
            panel = panel.absolute();
            panel_block_clicks(panel, oz.panel)
        });

        vstack().child(trigger).child(menu).into()
    }
}

impl From<ContextMenuBuilder> for Element {
    fn from(builder: ContextMenuBuilder) -> Self {
        builder.trigger(|| {
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(crate::primitives::text::text("Right-click me"));
            el
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn context_menu_records_items() {
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        let _el: Element = context_menu("cm1").item("Copy", move || f.set(true)).into();
        assert!(!fired.get());
    }
}
