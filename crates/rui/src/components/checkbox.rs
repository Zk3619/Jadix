//! # Checkbox
//!
//! 约 1rem 方框复选框；选中时显示 ✓。支持受控 / 非受控与禁用。
//!
//! ## 用法
//! ```ignore
//! <Checkbox />
//! <Checkbox disabled=true />
//! // 受控：
//! { Element::from(checkbox("demo").checked_signal(checked_sig)) }
//! ```
//!
//! ## 注意
//! - 未传 `checked` 时用内部信号 `{id}__checkbox`；请保证 `id` 唯一。
//! - 状态用 `peek` + 每帧 style sync，不在根上订阅 effect，以免多余 rebuild。
//! - 可聚焦；Space / Enter 切换与按钮同路径。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::hstack;
use crate::tokens;
use rui_macros::component;

pub struct CheckboxBuilder {
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    children: Vec<Element>,
}

/// Checkbox component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Checkbox(
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let mut builder = checkbox(&id)
        .default_checked(default_checked)
        .disabled(disabled);

    if let Some(sig) = checked {
        builder = builder.checked_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let mut handler = on_click;
        let mut old_on_click = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old_on_click {
                rc.borrow_mut()();
            }
            (handler.0)(());
        });
    }
    el
}

pub fn checkbox(id: impl Into<String>) -> CheckboxBuilder {
    CheckboxBuilder {
        id: id.into(),
        checked: None,
        default_checked: false,
        disabled: false,
        children: Vec::new(),
    }
}

impl CheckboxBuilder {
    pub fn checked_signal(mut self, s: Signal<bool>) -> Self {
        self.checked = Some(s);
        self
    }

    pub fn default_checked(mut self, v: bool) -> Self {
        self.default_checked = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
}

impl From<CheckboxBuilder> for Element {
    fn from(builder: CheckboxBuilder) -> Self {
        let scope = format!("{}__checkbox", builder.id);
        let sig = builder
            .checked
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_checked));
        let on = sig.peek();
        let disabled = builder.disabled;
        let id = builder.id.clone();

        let mut root: Element = hstack()
            .w("1rem")
            .h("1rem")
            .min_w("1rem")
            .min_h("1rem")
            .rounded("0.25rem")
            .items_center()
            .justify_center()
            .into();
        root.style.border_width = 1.0;
        root.style.border_color = tokens::border();
        root.text = Some("✓".into());
        root.text_style.color = if on {
            tokens::primary_foreground_text()
        } else {
            [0, 0, 0, 0]
        };
        root.style.bg_color = if on {
            tokens::primary()
        } else {
            tokens::background()
        };
        root.checkbox_sync = Some(sig);

        let sig_a = sig;
        let mut el: Element = interactive(root)
            .focus_id(id)
            .focusable(!disabled)
            .on_click(move || {
                if !disabled {
                    sig_a.set(!sig_a.get());
                }
            })
            .children(builder.children)
            .into();
        el.template_id = Some(builder.id);
        el
    }
}

impl crate::core::tree::IntoElementList for CheckboxBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub(crate) fn sync_checkbox_checked_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if let Some(sig) = el.checkbox_sync {
        let on = sig.peek();
        let want_bg = if on {
            tokens::primary()
        } else {
            tokens::background()
        };
        let want_color = if on {
            tokens::primary_foreground_text()
        } else {
            [0, 0, 0, 0]
        };
        let changed = el.style.bg_color != want_bg || el.text_style.color != want_color;
        if changed {
            el.style.bg_color = want_bg;
            el.text_style.color = want_color;
            if el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
            *impact = impact.merge(LayoutStatus::StyleChanged);
        }
    }
    for child in &mut el.children {
        sync_checkbox_checked_subtree(child, layout_tree, impact);
    }
}
