//! # AlertDialog
//!
//! 确认对话框：Header（标题）/ Body（说明）/ Footer（Cancel · Confirm）。
//! Body 可滚动；Footer 固定。复用 [`crate::components::internal::dialog_shell`]；**独立产品 API**。

use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::components::internal::dialog_shell::{
    dialog_content_frame, modal_sync, DialogSections,
};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;

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
            let Some(zs) = modal_sync(&id, open) else {
                return Element::new();
            };
            if !open.peek() {
                return Element::new();
            }

            let header: Element = vstack()
                .w("100%")
                .child(text(title.as_str()).size(18.0).weight(600.0))
                .into();

            let body: Element = text(description.as_str()).size(14.0).weight(400.0).into();

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

            let footer: Element = hstack()
                .gap("0.5rem")
                .justify("end")
                .w("100%")
                .child(cancel)
                .child(confirm)
                .into();

            dialog_content_frame(
                open,
                &zs,
                DialogSections {
                    header: Some(header),
                    body,
                    footer: Some(footer),
                    body_scroll_id: format!("{id}__body"),
                },
            )
        })
    }
}
