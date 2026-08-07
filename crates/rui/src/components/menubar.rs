//! # Menubar
//!
//! 水平菜单栏：每项为下拉触发器（复用 portal / floating）。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::popup::{
    mark_floating, panel_block_clicks, underlay_close_on_click, PopupPlacement,
};

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

impl From<MenubarBuilder> for Element {
    fn from(builder: MenubarBuilder) -> Self {
        let MenubarBuilder { id, sections } = builder;
        let mut row: Element = hstack().gap("0.25rem").items_center().w("100%").into();
        for section in sections {
            let sid = format!("{id}:{}", section.id);
            let title = section.title.clone();
            let trigger_id = format!("{sid}:trigger");
            let items = section.items;
            let open = use_signal(format!("{sid}__open").as_str(), || false);
            let view_id = format!("{sid}__view");
            let section_el = reactive_view(view_id, move || {
                let _ = open.get();
                let open_t = open;
                let trigger: Element = button(&trigger_id)
                    .variant("ghost")
                    .size("sm")
                    .on_click(move || open_t.set(!open_t.peek()))
                    .text(title.clone())
                    .into();
                let mut root: Element = vstack().relative(true).child(trigger).into();
                if let Some(oz) =
                    portal_sync(&sid, open, OverlayKind::Menu, OverlayMode::Stacked)
                {
                    if open.peek() {
                        root = root.child(underlay_close_on_click(open, oz.underlay));
                        let mut menu = vstack().gap("0.125rem").p("0.25rem").min_w("8rem");
                        for (i, item) in items.iter().enumerate() {
                            let run = item.on_select.clone();
                            let o_sig = open;
                            let bid = format!("{sid}:item{i}");
                            let row: Element = button(&bid)
                                .variant("ghost")
                                .size("sm")
                                .w("100%")
                                .on_click(move || {
                                    run();
                                    o_sig.set(false);
                                })
                                .text(item.label.clone())
                                .into();
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
                        mark_floating(
                            &mut panel,
                            &sid,
                            &trigger_id,
                            PopupPlacement::BottomStart,
                        );
                        panel = panel_block_clicks(panel, oz.panel);
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
