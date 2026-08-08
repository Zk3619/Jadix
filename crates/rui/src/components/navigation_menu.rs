//! # NavigationMenu
//!
//! 对齐 ShadCN / solid-ui（Kobalte）**NavigationMenu**：顶栏/区段的链接导航 + 可选富内容下拉。
//!
//! | 模块 | 职责 |
//! |---|---|
//! | [`crate::components::navbar`] | 顶栏布局壳（brand / center / actions）；center 可嵌本组件 |
//! | [`crate::components::menubar`] | 应用命令菜单（File / Edit） |
//! | [`crate::app::navigator::navigation_host`] | `Navigator` 页面栈视图（与本组件无关） |
//! | **本模块** | 导览链接 + Trigger/Content/Viewport |
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!   navigation_menu("main")
//!     .link("docs", "Docs", || { /* … */ })
//!     .trigger(
//!       "comp",
//!       "Components",
//!       vec![
//!         NavigationMenuLink::new("alert", "Alert Dialog", || {})
//!           .description("Modal confirmation"),
//!       ],
//!     ),
//! )
//! ```

use std::rc::Rc;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::popup::{mark_floating, panel_block_clicks, PopupPlacement};
use crate::utils::roving::roving_move;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

/// Content / 顶栏直达链接。
#[derive(Clone)]
pub struct NavigationMenuLink {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub on_select: Rc<dyn Fn()>,
}

impl NavigationMenuLink {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            on_select: Rc::new(on_select),
        }
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }
}

/// List 中的一项：直达 Link，或 Trigger + Content 链接组。
#[derive(Clone)]
pub enum NavigationMenuItem {
    Link(NavigationMenuLink),
    Trigger {
        id: String,
        label: String,
        links: Vec<NavigationMenuLink>,
    },
}

pub struct NavigationMenuBuilder {
    id: String,
    items: Vec<NavigationMenuItem>,
    /// 共享 Viewport 槽（solid-ui 默认开启）。
    viewport: bool,
}

pub fn navigation_menu(id: impl Into<String>) -> NavigationMenuBuilder {
    NavigationMenuBuilder {
        id: id.into(),
        items: Vec::new(),
        viewport: true,
    }
}

impl NavigationMenuBuilder {
    pub fn viewport(mut self, enabled: bool) -> Self {
        self.viewport = enabled;
        self
    }

    pub fn item(mut self, item: NavigationMenuItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn link(
        self,
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        self.item(NavigationMenuItem::Link(NavigationMenuLink::new(
            id, label, on_select,
        )))
    }

    pub fn trigger(
        self,
        id: impl Into<String>,
        label: impl Into<String>,
        links: Vec<NavigationMenuLink>,
    ) -> Self {
        self.item(NavigationMenuItem::Trigger {
            id: id.into(),
            label: label.into(),
            links,
        })
    }
}

fn accent_muted() -> [f32; 4] {
    let mut c = parse_color("accent");
    c[3] *= 0.5;
    c
}

fn trigger_chip(focus_id: &str, label: &str, expanded: bool, on_click: impl FnMut() + 'static) -> Element {
    let bg = if expanded {
        accent_muted()
    } else {
        tokens::background()
    };
    let chip: Element = r#box()
        .h("2.5rem")
        .px("1rem")
        .rounded("0.375rem")
        .bg(bg)
        .child(build_text_node(
            label,
            TextNodeConfig {
                size_px: 14.0,
                weight: 500,
                color: parse_text_color("foreground"),
                ..Default::default()
            },
        ))
        .into();
    interactive(chip)
        .focus_id(focus_id)
        .focusable(true)
        .on_click(on_click)
        .into()
}

/// Build a content link; when `description` is set, use a stacked body.
fn content_link_el(
    menu_id: &str,
    link: &NavigationMenuLink,
    expanded: Signal<Option<String>>,
) -> Element {
    let run = link.on_select.clone();
    let bid = format!("{menu_id}:link:{}", link.id);
    if let Some(desc) = &link.description {
        let col: Element = vstack()
            .gap("0.25rem")
            .w("100%")
            .child(build_text_node(
                &link.label,
                TextNodeConfig {
                    size_px: 14.0,
                    weight: 500,
                    color: parse_text_color("foreground"),
                    ..Default::default()
                },
            ))
            .child(build_text_node(
                desc,
                TextNodeConfig {
                    size_px: 12.0,
                    weight: 400,
                    color: parse_text_color("muted-foreground"),
                    ..Default::default()
                },
            ))
            .into();
        let hit: Element = r#box()
            .w("100%")
            .p("0.5rem")
            .rounded("0.375rem")
            .child(col)
            .into();
        interactive(hit)
            .focus_id(bid)
            .focusable(true)
            .on_click(move || {
                run();
                expanded.set(None);
            })
            .into()
    } else {
        button(&bid)
            .variant("ghost")
            .size("sm")
            .w("100%")
            .on_click(move || {
                run();
                expanded.set(None);
            })
            .text(link.label.clone())
            .into()
    }
}

fn build_content_panel(
    menu_id: &str,
    trigger_id: &str,
    overlay_key: &str,
    links: &[NavigationMenuLink],
    expanded: Signal<Option<String>>,
    panel_z: i32,
) -> Element {
    let mut list = vstack().gap("0.125rem").p("0.75rem").min_w("14rem");
    for link in links {
        list = list.child(content_link_el(menu_id, link, expanded));
    }
    let mut panel: Element = list
        .bg(tokens::background())
        .rounded("0.375rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.12], 8.0)
        .absolute(true)
        .into();
    mark_floating(
        &mut panel,
        overlay_key,
        trigger_id,
        PopupPlacement::BottomStart,
    );
    panel.set_attr("rui_floating_match_trigger", "0");
    panel.set_attr("rui_floating_min_w", "14rem");
    panel_block_clicks(panel, panel_z)
}

impl From<NavigationMenuBuilder> for Element {
    fn from(builder: NavigationMenuBuilder) -> Self {
        let NavigationMenuBuilder {
            id,
            items,
            viewport,
        } = builder;

        let expanded = use_signal(format!("{id}__navmenu_expanded").as_str(), || {
            None::<String>
        });
        let focus_idx = use_signal(format!("{id}__navmenu_focus").as_str(), || 0usize);

        // One open signal per trigger (created outside reactive_view — hook discipline).
        let mut trigger_meta: Vec<(String, String, Vec<NavigationMenuLink>, Signal<bool>)> =
            Vec::new();
        for item in &items {
            if let NavigationMenuItem::Trigger { id: tid, links, .. } = item {
                let overlay_key = format!("{id}:{tid}");
                let open = use_signal(format!("{overlay_key}__open").as_str(), || false);
                trigger_meta.push((tid.clone(), overlay_key, links.clone(), open));
            }
        }

        let item_count = items.len();
        let view_id = format!("{id}__navmenu_view");
        let root_id = id.clone();
        let items_c = items;

        let mut root = reactive_view(view_id, move || {
            let open_key = expanded.get();
            let _ = focus_idx.get();
            // Keep portal_sync open flags in sync with shared expanded key.
            for (tid, _, _, open_sig) in &trigger_meta {
                open_sig.set(open_key.as_deref() == Some(tid.as_str()));
            }

            let exp = expanded;
            let foc = focus_idx;
            let n = item_count;

            let mut list_row: Element = hstack().gap("0.25rem").items_center().into();
            let mut panels: Vec<Element> = Vec::new();

            for (i, item) in items_c.iter().enumerate() {
                match item {
                    NavigationMenuItem::Link(link) => {
                        let run = link.on_select.clone();
                        let fid = format!("{root_id}:item:{i}");
                        let chip = trigger_chip(&fid, &link.label, false, move || run());
                        list_row = list_row.child(chip);
                    }
                    NavigationMenuItem::Trigger {
                        id: tid,
                        label,
                        links,
                    } => {
                        let trigger_focus = format!("{root_id}:{tid}:trigger");
                        let overlay_key = format!("{root_id}:{tid}");
                        let is_open = open_key.as_deref() == Some(tid.as_str());
                        let tid_c = tid.clone();
                        let exp_t = exp;
                        let chip =
                            trigger_chip(&trigger_focus, label, is_open, move || {
                                if exp_t.peek().as_deref() == Some(tid_c.as_str()) {
                                    exp_t.set(None);
                                } else {
                                    exp_t.set(Some(tid_c.clone()));
                                }
                            });

                        let mut wrap: Element = r#box().relative(true).child(chip).into();
                        wrap.template_id = Some(format!("{root_id}:item:{i}"));

                        let open_sig = trigger_meta
                            .iter()
                            .find(|(t, _, _, _)| t == tid)
                            .map(|(_, _, _, s)| *s);

                        if let Some(open_sig) = open_sig {
                            if let Some(oz) = portal_sync(
                                &overlay_key,
                                open_sig,
                                OverlayKind::Menu,
                                OverlayMode::Stacked,
                            ) {
                                if is_open {
                                    let panel = build_content_panel(
                                        &root_id,
                                        &trigger_focus,
                                        &overlay_key,
                                        links,
                                        exp,
                                        oz.panel,
                                    );
                                    if viewport {
                                        panels.push(panel);
                                    } else {
                                        wrap = wrap.child(panel);
                                    }
                                }
                            }
                        }

                        list_row = list_row.child(wrap);
                    }
                }
            }

            let list_focus = format!("navmenu:{root_id}");
            let foc_k = foc;
            let exp_k = exp;
            let mut list: Element = interactive(list_row)
                .focus_id(list_focus)
                .focusable(true)
                .on_key_down(move |ev| {
                    use winit::event::ElementState;
                    use winit::keyboard::{Key, NamedKey};
                    if ev.state != ElementState::Pressed || ev.repeat {
                        return false;
                    }
                    match &ev.logical_key {
                        Key::Named(NamedKey::ArrowRight) => {
                            if let Some(next) = roving_move(n, foc_k.get(), 1) {
                                foc_k.set(next);
                            }
                            true
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if let Some(next) = roving_move(n, foc_k.get(), -1) {
                                foc_k.set(next);
                            }
                            true
                        }
                        Key::Named(NamedKey::Escape) => {
                            if exp_k.peek().is_some() {
                                exp_k.set(None);
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                })
                .into();
            list.set_attr("rui_navigation_menu_list", "1");

            let mut shell: Element = vstack().gap("0").relative(true).child(list).into();
            if viewport {
                let mut viewport_slot: Element = r#box().w("100%").h("0").relative(true).into();
                viewport_slot.set_attr("rui_navigation_menu_viewport", "1");
                for p in panels {
                    viewport_slot = viewport_slot.child(p);
                }
                shell = shell.child(viewport_slot);
            }
            shell.template_id = Some(root_id.clone());
            shell.set_attr("rui_navigation_menu", "1");
            shell
        });

        root.template_id = Some(id);
        root
    }
}
