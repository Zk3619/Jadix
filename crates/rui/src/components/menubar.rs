//! # Menubar
//!
//! 水平菜单栏：每项为下拉触发器（复用 portal / floating）。
//! - 关闭时：←→ / Home / End **只移动触发器焦点环**
//! - 打开时：↓ 进入首项；↑↓ / Home / End 在菜单项间移焦点环；←→ 切换相邻菜单
//! - Enter / Space：激活当前焦点项（或打开/切换菜单）
//! - 互斥关其它菜单由 [`portal_sync`](crate::app::portal_overlay::portal_sync) Menu 排他完成。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::popup::{mark_floating, mount_anchored_overlay, PopupPlacement};
use crate::utils::roving::{focus_by_id, focus_roving_key, roving_move, FocusRovingAxis};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

#[derive(Clone)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub on_select: std::rc::Rc<dyn Fn()>,
}

impl MenuItem {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on_select: std::rc::Rc::new(on_select),
        }
    }
}

#[derive(Clone)]
pub struct MenuSection {
    pub id: String,
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl MenuSection {
    pub fn new(id: impl Into<String>, title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            items,
        }
    }
}

pub struct MenubarBuilder {
    id: String,
    sections: Vec<MenuSection>,
}

pub fn menubar(id: impl Into<String>) -> MenubarBuilder {
    MenubarBuilder {
        id: id.into(),
        sections: Vec::new(),
    }
}

impl MenubarBuilder {
    pub fn sections(mut self, sections: Vec<MenuSection>) -> Self {
        self.sections = sections;
        self
    }
    pub fn section(mut self, section: MenuSection) -> Self {
        self.sections.push(section);
        self
    }
}

fn item_focus_id(sid: &str, i: usize) -> String {
    format!("{sid}:item{i}")
}

fn open_neighbor_menu(id: &str, section_ids: &[String], from: usize, delta: i32) {
    let Some(next) = roving_move(section_ids.len(), from, delta) else {
        return;
    };
    let other = &section_ids[next];
    let other_open = use_signal(format!("{id}:{other}__open").as_str(), || false);
    let other_active = use_signal(format!("{id}:{other}__active").as_str(), || 0usize);
    other_active.set(0);
    other_open.set(true);
    let other_sid = format!("{id}:{other}");
    focus_by_id(item_focus_id(&other_sid, 0));
}

impl From<MenubarBuilder> for Element {
    fn from(builder: MenubarBuilder) -> Self {
        let MenubarBuilder { id, sections } = builder;
        let section_ids: Vec<String> = sections.iter().map(|s| s.id.clone()).collect();
        let trigger_ids: Vec<String> = section_ids
            .iter()
            .map(|s| format!("{id}:{s}:trigger"))
            .collect();
        let n_sections = section_ids.len();

        let mut row: Element = hstack().gap("0.25rem").items_center().w("100%").into();
        row = row.accesskit_role("menubar");
        for (sec_i, section) in sections.into_iter().enumerate() {
            let sid = format!("{id}:{}", section.id);
            let title = section.title.clone();
            let trigger_id = format!("{sid}:trigger");
            let items = section.items;
            let view_id = format!("{sid}__view");
            let open = use_signal(format!("{sid}__open").as_str(), || false);
            let active = use_signal(format!("{sid}__active").as_str(), || 0usize);
            let section_ids_nav = section_ids.clone();
            let trigger_ids_nav = trigger_ids.clone();
            let id_nav = id.clone();
            let item_ids: Vec<String> = (0..items.len()).map(|i| item_focus_id(&sid, i)).collect();

            let section_el = reactive_view(view_id, move || {
                let _ = open.get();
                let active_ix = active.get();
                let open_t = open;
                let active_t = active;
                let n_items = items.len();
                let section_ids_k = section_ids_nav.clone();
                let trigger_ids_k = trigger_ids_nav.clone();
                let id_k = id_nav.clone();
                let sec_i_k = sec_i;
                let n_sec = n_sections;
                let item_ids_k = item_ids.clone();
                let sid_k = sid.clone();

                let mut trigger: Element = button(&trigger_id)
                    .variant("ghost")
                    .size("sm")
                    .on_click(move || {
                        let next = !open_t.peek();
                        if next {
                            active_t.set(0);
                            open_t.set(true);
                            if n_items > 0 {
                                focus_by_id(item_focus_id(&sid_k, 0));
                            }
                        } else {
                            open_t.set(false);
                        }
                    })
                    .text(title.clone())
                    .into();
                trigger = trigger.accesskit_role("menuitem");

                let open_keys = open;
                let active_keys = active;
                let sid_keys = sid.clone();
                let id_for_trigger_keys = id_k.clone();
                let section_ids_for_trigger = section_ids_k.clone();
                trigger = trigger.on_key_down(move |ev| {
                    if ev.state != ElementState::Pressed || ev.repeat {
                        return false;
                    }
                    if !open_keys.peek() {
                        // Closed: move focus among triggers only.
                        return focus_roving_key(
                            &ev,
                            &trigger_ids_k,
                            sec_i_k,
                            FocusRovingAxis::Horizontal,
                        );
                    }
                    match &ev.logical_key {
                        Key::Named(NamedKey::ArrowDown) => {
                            if n_items > 0 {
                                focus_by_id(item_focus_id(&sid_keys, 0));
                                active_keys.set(0);
                            }
                            true
                        }
                        Key::Named(NamedKey::ArrowRight) if n_sec > 0 => {
                            open_keys.set(false);
                            open_neighbor_menu(
                                &id_for_trigger_keys,
                                &section_ids_for_trigger,
                                sec_i_k,
                                1,
                            );
                            true
                        }
                        Key::Named(NamedKey::ArrowLeft) if n_sec > 0 => {
                            open_keys.set(false);
                            open_neighbor_menu(
                                &id_for_trigger_keys,
                                &section_ids_for_trigger,
                                sec_i_k,
                                -1,
                            );
                            true
                        }
                        _ => false,
                    }
                });

                let mut root: Element = vstack().relative(true).child(trigger).into();
                if let Some(oz) = portal_sync(&sid, open, OverlayKind::Menu, OverlayMode::Stacked) {
                    if open.peek() {
                        let mut menu = vstack().gap("0.125rem").p("0.25rem").min_w("8rem");
                        for (i, item) in items.iter().enumerate() {
                            let run = item.on_select.clone();
                            let o_sig = open;
                            let a_row = active;
                            let bid = item_focus_id(&sid, i);
                            let highlighted = i == active_ix;
                            let item_ids_row = item_ids_k.clone();
                            let section_ids_row = section_ids_k.clone();
                            let id_row = id_k.clone();
                            let open_row = open;
                            let mut row: Element = button(&bid)
                                .variant(if highlighted { "secondary" } else { "ghost" })
                                .size("sm")
                                .w("100%")
                                .on_click(move || {
                                    a_row.set(i);
                                    run();
                                    o_sig.set(false);
                                })
                                .text(item.label.clone())
                                .into();
                            row = row.accesskit_role("menuitem");
                            row = row.on_key_down(move |ev| {
                                if ev.state != ElementState::Pressed || ev.repeat {
                                    return false;
                                }
                                match &ev.logical_key {
                                    Key::Named(NamedKey::ArrowRight) if n_sec > 0 => {
                                        open_row.set(false);
                                        open_neighbor_menu(&id_row, &section_ids_row, sec_i_k, 1);
                                        true
                                    }
                                    Key::Named(NamedKey::ArrowLeft) if n_sec > 0 => {
                                        open_row.set(false);
                                        open_neighbor_menu(&id_row, &section_ids_row, sec_i_k, -1);
                                        true
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        open_row.set(false);
                                        focus_by_id(format!(
                                            "{}:{}:trigger",
                                            id_row, section_ids_row[sec_i_k]
                                        ));
                                        true
                                    }
                                    _ => focus_roving_key(
                                        &ev,
                                        &item_ids_row,
                                        i,
                                        FocusRovingAxis::Vertical,
                                    ),
                                }
                            });
                            menu = menu.child(row);
                        }
                        let mut panel: Element = menu
                            .bg(tokens::background())
                            .rounded("0.375rem")
                            .border_style("solid")
                            .border_width(1.0)
                            .border_color(tokens::border())
                            .absolute(true)
                            .into();
                        panel = panel.accesskit_role("menu");
                        mark_floating(&mut panel, &sid, &trigger_id, PopupPlacement::BottomStart);
                        panel = mount_anchored_overlay(panel, oz.panel);
                        root = root.child(panel);
                    }
                }
                root
            });
            row = row.child(section_el);
        }
        row
    }
}
