//! # AlertDialog
//!
//! 确认对话框：标题 + 正文 + Cancel / Confirm。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::popup::{panel_block_clicks, portal_panel, underlay_close_on_click};

pub struct AlertDialogBuilder {
    id: String,
    open: Option<Signal<bool>>,
    title: String,
    description: String,
    confirm_label: String,
    cancel_label: String,
    on_confirm: Option<std::rc::Rc<dyn Fn()>>,
    on_cancel: Option<std::rc::Rc<dyn Fn()>>,
}

pub fn alert_dialog(id: impl Into<String>, title: impl Into<String>) -> AlertDialogBuilder {
    AlertDialogBuilder {
        id: id.into(),
        open: None,
        title: title.into(),
        description: String::new(),
        confirm_label: "Confirm".into(),
        cancel_label: "Cancel".into(),
        on_confirm: None,
        on_cancel: None,
    }
}

impl AlertDialogBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }
    pub fn confirm_label(mut self, s: impl Into<String>) -> Self {
        self.confirm_label = s.into();
        self
    }
    pub fn cancel_label(mut self, s: impl Into<String>) -> Self {
        self.cancel_label = s.into();
        self
    }
    pub fn on_confirm(mut self, f: impl Fn() + 'static) -> Self {
        self.on_confirm = Some(std::rc::Rc::new(f));
        self
    }
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancel = Some(std::rc::Rc::new(f));
        self
    }
}

impl From<AlertDialogBuilder> for Element {
    fn from(builder: AlertDialogBuilder) -> Self {
        let AlertDialogBuilder {
            id,
            open,
            title,
            description,
            confirm_label,
            cancel_label,
            on_confirm,
            on_cancel,
        } = builder;
        let open =
            open.unwrap_or_else(|| use_signal(format!("{id}__alert_open").as_str(), || false));
        let view_id = format!("{id}__alert_view");

        reactive_view(view_id, move || {
            let _ = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
                return Element::new();
            };
            if !open.peek() {
                return Element::new();
            }

            let o_cancel = open;
            let cancel_cb = on_cancel.clone();
            let cancel: Element = button(format!("{id}:cancel"))
                .variant("outline")
                .on_click(move || {
                    if let Some(cb) = &cancel_cb {
                        cb();
                    }
                    o_cancel.set(false);
                })
                .text(cancel_label.clone())
                .into();

            let o_ok = open;
            let ok_cb = on_confirm.clone();
            let confirm: Element = button(format!("{id}:ok"))
                .variant("default")
                .on_click(move || {
                    if let Some(cb) = &ok_cb {
                        cb();
                    }
                    o_ok.set(false);
                })
                .text(confirm_label.clone())
                .into();

            let actions: Element = hstack()
                .gap("0.5rem")
                .justify("end")
                .w("100%")
                .child(cancel)
                .child(confirm)
                .into();

            let mut panel: Element = vstack()
                .gap("0.75rem")
                .p("1.25rem")
                .max_w("24rem")
                .child(text(title.as_str()).size(18.0).weight(600.0))
                .child(text(description.as_str()).size(14.0).weight(400.0))
                .child(actions)
                .into();
            panel = panel
                .bg(tokens::background())
                .rounded("0.5rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border());

            let mut root = vstack().child(underlay_close_on_click(open, oz.underlay));
            root = root.child(portal_panel(panel_block_clicks(panel, oz.panel), oz.panel));
            root.into()
        })
    }
}
