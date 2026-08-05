//! # Switch
//!
//! 胶囊轨道 + 拇指开关（开/关）。尺寸约 2.75×1.5rem；支持受控 / 非受控与禁用。
//!
//! ## 用法
//! ```ignore
//! <Switch />
//! { Element::from(switch("demo").checked_signal(on_sig)) }
//! ```
//!
//! ## 注意
//! - 与 Checkbox 相同：`peek` + 每帧 sync，避免根订阅触发整树 rebuild。
//! - Builder 上的 `w()` 为 no-op（尺寸固定）。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{Element, IntoDimension, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;
use rui_macros::component;
use taffy::prelude::JustifyContent;

pub struct SwitchBuilder {
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    children: Vec<Element>,
}

/// Switch component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Switch(
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let mut builder = switch(&id)
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

pub fn switch(id: impl Into<String>) -> SwitchBuilder {
    SwitchBuilder {
        id: id.into(),
        checked: None,
        default_checked: false,
        disabled: false,
        children: Vec::new(),
    }
}

impl SwitchBuilder {
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

    pub fn w(self, w: impl IntoDimension) -> Self {
        let _ = w;
        self
    }
}

impl From<SwitchBuilder> for Element {
    fn from(builder: SwitchBuilder) -> Self {
        let scope = format!("{}__switch", builder.id);
        let sig = builder
            .checked
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_checked));
        let disabled = builder.disabled;
        let id = builder.id.clone();

        let thumb = r#box()
            .w("1.25rem")
            .h("1.25rem")
            .rounded("9999px")
            .bg(tokens::background())
            .into();

        // `peek`：不把 checked 订阅到根 effect；轨道/拇指由 `sync_switch_checked_subtree` 每帧对齐。
        let on = sig.peek();
        let inner_el = build_switch_inner_row(on, thumb);

        let track_bg = if on {
            tokens::primary()
        } else {
            tokens::input()
        };
        let mut track: Element = hstack()
            .w("2.75rem")
            .h("1.5rem")
            .rounded("9999px")
            .p("2px")
            .into();
        track = track.bg(track_bg);
        track = track.child(inner_el);
        track.switch_checked_sync = Some(sig);

        let sig_a = sig;
        let mut el: Element = interactive(track)
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

fn build_switch_inner_row(on: bool, thumb: Element) -> Element {
    let mut row: Element = hstack()
        .w("100%")
        .h("100%")
        .items_center()
        .child(thumb)
        .into();
    row.style.justify_content = Some(if on {
        JustifyContent::FlexEnd
    } else {
        JustifyContent::FlexStart
    });
    row
}

impl crate::core::tree::IntoElementList for SwitchBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub(crate) fn sync_switch_checked_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if let Some(sig) = el.switch_checked_sync {
        let on = sig.peek();
        let want_bg = if on {
            tokens::primary()
        } else {
            tokens::input()
        };
        let want_justify = if on {
            JustifyContent::FlexEnd
        } else {
            JustifyContent::FlexStart
        };
        let inner_mismatch = el
            .children
            .first()
            .is_some_and(|inner| inner.style.justify_content != Some(want_justify));
        let changed = el.style.bg_color != want_bg || inner_mismatch;
        if changed {
            el.style.bg_color = want_bg;
            if let Some(inner) = el.children.get_mut(0) {
                inner.style.justify_content = Some(want_justify);
                if inner.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(inner);
                }
            }
            if el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
            *impact = impact.merge(LayoutStatus::LayoutChanged);
        }
    }
    for child in &mut el.children {
        sync_switch_checked_subtree(child, layout_tree, impact);
    }
}
