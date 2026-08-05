//! # Slider
//!
//! 水平滑块，值域 `0..=1`：轨道 + 拇指（flex-grow 定位）。默认 `max_w≈280px`。
//!
//! ## 用法
//! ```ignore
//! Element::from(slider("volume").value_signal(level))
//! // 也可 <Slider id="volume" …>（若宏属性齐全）
//! ```
//!
//! ## 注意
//! - **当前点击轨道是步进 +0.08**（到头回绕到 0），不是指针拖拽。
//! - 键盘：方向键约 ±0.05。
//! - 值变化通过组件根的 [`crate::core::tree::ReactiveSync`] 原位同步，不订阅根 UI effect。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;
use rui_macros::component;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

pub struct SliderBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    disabled: bool,
    children: Vec<Element>,
}

/// Slider component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Slider(
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    disabled: bool,
    on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let mut builder = slider(&id).default_value(default_value).disabled(disabled);

    if let Some(sig) = value {
        builder = builder.value_signal(sig);
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

pub fn slider(id: impl Into<String>) -> SliderBuilder {
    SliderBuilder {
        id: id.into(),
        value: None,
        default_value: 0.35,
        disabled: false,
        children: Vec::new(),
    }
}

impl SliderBuilder {
    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v.clamp(0.0, 1.0);
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

impl From<SliderBuilder> for Element {
    fn from(builder: SliderBuilder) -> Self {
        let scope = format!("{}__slider", builder.id);
        let val = builder
            .value
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_value));
        let disabled = builder.disabled;
        let id = builder.id.clone();

        // Initial snapshot only. Subsequent changes are applied in place by `ReactiveSync`.
        let v = val.peek().clamp(0.0, 1.0);
        let lg = v.max(0.0001);
        let rg = (1.0 - v).max(0.0001);

        let thumb: Element = r#box()
            .w("1rem")
            .h("1rem")
            .rounded("9999px")
            .bg(tokens::primary())
            .into();

        let left_spacer: Element = r#box().flex_grow(lg).h("1px").into();
        let right_spacer: Element = r#box().flex_grow(rg).h("1px").into();
        let mut track: Element = hstack()
            .w("100%")
            .max_w("280px")
            .h("1.5rem")
            .items_center()
            .p("2px")
            .rounded("9999px")
            .child(left_spacer)
            .child(thumb)
            .child(right_spacer)
            .children(builder.children)
            .into();
        track = track.bg(tokens::input());

        let val_k = val;
        let mut el: Element = interactive(track)
            .focus_id(id)
            .focusable(!disabled)
            .on_click(move || {
                if disabled {
                    return;
                }
                let x = val_k.get() + 0.08;
                val_k.set(if x > 1.0 { 0.0 } else { x });
            })
            .on_key_down(move |ev: KeyEvent| {
                if disabled {
                    return false;
                }
                match ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        val_k.set((val_k.get() - 0.05).max(0.0));
                        true
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        val_k.set((val_k.get() + 0.05).min(1.0));
                        true
                    }
                    _ => false,
                }
            })
            .into();
        let sync_value = val;
        el = el.reactive_sync(move |root, layout_tree| {
            let value = sync_value.peek().clamp(0.0, 1.0);
            let want_left = value.max(0.0001);
            let want_right = (1.0 - value).max(0.0001);
            let mut changed = false;

            if let Some(left) = root.children.get_mut(0) {
                if (left.style.flex_grow - want_left).abs() > f32::EPSILON {
                    left.style.flex_grow = want_left;
                    if left.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(left);
                    }
                    changed = true;
                }
            }
            if let Some(right) = root.children.get_mut(2) {
                if (right.style.flex_grow - want_right).abs() > f32::EPSILON {
                    right.style.flex_grow = want_right;
                    if right.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(right);
                    }
                    changed = true;
                }
            }

            if changed {
                crate::core::tree::LayoutStatus::LayoutChanged
            } else {
                crate::core::tree::LayoutStatus::Clean
            }
        });
        el.template_id = Some(builder.id);
        el
    }
}
