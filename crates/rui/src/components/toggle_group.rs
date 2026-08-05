//! # ToggleGroup
//!
//! 对齐 solid-ui / Kobalte **ToggleGroup**：一组两态按钮。
//! - `multiple=false`（默认）：单选，值 `Signal<String>`（空串表示未选）
//! - `multiple=true`：多选，值用 `Signal<Vec<String>>`
//! - 选中态通过组件根的 [`crate::core::tree::ReactiveSync`] 原位同步，不订阅根 UI effect。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!   toggle_group("align")
//!     .options(vec![("left".into(),"L".into()), ("center".into(),"C".into())])
//!     .value_signal(align)
//! )
//! ```

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

pub struct ToggleGroupBuilder {
    id: String,
    multiple: bool,
    value_single: Option<Signal<String>>,
    value_multi: Option<Signal<Vec<String>>>,
    default_value: String,
    options: Vec<(String, String)>,
    disabled: bool,
    variant: String,
}

/// RSX `<ToggleGroup>`：选项走 `options` 更稳妥；亦可用 builder。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToggleGroup(
    id: String,
    multiple: bool,
    value: Option<Signal<String>>,
    default_value: String,
    options: Option<Vec<(String, String)>>,
    disabled: bool,
    variant: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = toggle_group(&id)
        .multiple(multiple)
        .default_value(default_value)
        .disabled(disabled)
        .variant(variant.unwrap_or_else(|| "outline".into()));
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    if let Some(opts) = options {
        b = b.options(opts);
    }
    b.into()
}

pub fn toggle_group(id: impl Into<String>) -> ToggleGroupBuilder {
    ToggleGroupBuilder {
        id: id.into(),
        multiple: false,
        value_single: None,
        value_multi: None,
        default_value: String::new(),
        options: Vec::new(),
        disabled: false,
        variant: "outline".into(),
    }
}

impl ToggleGroupBuilder {
    pub fn multiple(mut self, v: bool) -> Self {
        self.multiple = v;
        self
    }
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value_single = Some(s);
        self
    }
    pub fn value_multi_signal(mut self, s: Signal<Vec<String>>) -> Self {
        self.value_multi = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
    pub fn options(mut self, opts: Vec<(String, String)>) -> Self {
        self.options = opts;
        self
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = v.into();
        self
    }
}

fn toggle_chip_colors(
    pressed: bool,
    disabled: bool,
    outline: bool,
) -> ([f32; 4], [f32; 4], [u8; 4]) {
    let mut bg = if pressed {
        parse_color("accent")
    } else {
        parse_color("transparent")
    };
    let mut border = if outline {
        parse_color("input")
    } else {
        parse_color("transparent")
    };
    let mut fg = if pressed {
        parse_text_color("accent-foreground")
    } else {
        parse_text_color("foreground")
    };
    if disabled {
        bg[3] *= 0.5;
        border[3] *= 0.5;
        fg[3] = (fg[3] as f32 * 0.5) as u8;
    }
    (bg, border, fg)
}

fn item_el(
    group_id: &str,
    value: &str,
    label: &str,
    pressed: bool,
    disabled: bool,
    outline: bool,
    on_click: impl FnMut() + 'static,
) -> Element {
    let (bg, border, fg) = toggle_chip_colors(pressed, disabled, outline);
    let mut chip: Element = r#box()
        .px("0.75rem")
        .py("0.375rem")
        .rounded("0.375rem")
        .bg(bg)
        .border(border, if outline { 1.0 } else { 0.0 })
        .into();
    chip = chip.child(build_text_node(
        label,
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: fg,
            ..Default::default()
        },
    ));
    let focus_id = format!("{group_id}:{value}");
    let mut out: Element = interactive(chip)
        .focus_id(focus_id)
        .focusable(!disabled)
        .on_click(on_click)
        .into();
    if disabled {
        out = out.cursor_not_allowed();
    } else {
        out = out.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    out
}

impl From<ToggleGroupBuilder> for Element {
    fn from(builder: ToggleGroupBuilder) -> Self {
        let outline = builder.variant == "outline";
        let disabled = builder.disabled;
        let group_id = builder.id.clone();
        let multiple = builder.multiple;
        let option_values: Vec<String> = builder
            .options
            .iter()
            .map(|(value, _)| value.clone())
            .collect();

        let mut row: Element = hstack().gap("0.25rem").items_center().into();

        if multiple {
            let scope = format!("{}__tg_multi", builder.id);
            let sig = builder
                .value_multi
                .unwrap_or_else(|| use_signal(scope.as_str(), Vec::new));
            let selected = sig.peek();
            for (val, label) in builder.options {
                let pressed = selected.iter().any(|v| v == &val);
                let sig_c = sig;
                let v_c = val.clone();
                row = row.child(item_el(
                    &group_id,
                    &val,
                    &label,
                    pressed,
                    disabled,
                    outline,
                    move || {
                        if disabled {
                            return;
                        }
                        let mut cur = sig_c.get();
                        if let Some(i) = cur.iter().position(|x| x == &v_c) {
                            cur.remove(i);
                        } else {
                            cur.push(v_c.clone());
                        }
                        sig_c.set(cur);
                    },
                ));
            }
            let sync_sig = sig;
            row = row.reactive_sync(move |root, _layout_tree| {
                sync_toggle_group_pressed(root, &option_values, outline, disabled, |value| {
                    sync_sig.peek().iter().any(|v| v == value)
                })
            });
        } else {
            let scope = format!("{}__tg", builder.id);
            let default = builder.default_value.clone();
            let sig = builder
                .value_single
                .unwrap_or_else(|| use_signal(scope.as_str(), || default));
            let current = sig.peek();
            for (val, label) in builder.options {
                let pressed = current == val;
                let sig_c = sig;
                let v_c = val.clone();
                row = row.child(item_el(
                    &group_id,
                    &val,
                    &label,
                    pressed,
                    disabled,
                    outline,
                    move || {
                        if disabled {
                            return;
                        }
                        if sig_c.get() == v_c {
                            sig_c.set(String::new());
                        } else {
                            sig_c.set(v_c.clone());
                        }
                    },
                ));
            }
            let sync_sig = sig;
            row = row.reactive_sync(move |root, _layout_tree| {
                let current = sync_sig.peek();
                sync_toggle_group_pressed(root, &option_values, outline, disabled, |value| {
                    current == *value
                })
            });
        }

        row.template_id = Some(builder.id);
        row
    }
}

fn sync_toggle_group_pressed(
    root: &mut Element,
    option_values: &[String],
    outline: bool,
    disabled: bool,
    mut is_pressed: impl FnMut(&str) -> bool,
) -> crate::core::tree::LayoutStatus {
    let mut changed = false;
    for (option, value) in root.children.iter_mut().zip(option_values.iter()) {
        let pressed = is_pressed(value.as_str());
        let (want_bg, want_border, want_fg) = toggle_chip_colors(pressed, disabled, outline);
        if option.style.bg_color != want_bg {
            option.style.bg_color = want_bg;
            changed = true;
        }
        if option.style.border_color != want_border {
            option.style.border_color = want_border;
            changed = true;
        }
        if option.style.border_width != if outline { 1.0 } else { 0.0 } {
            option.style.border_width = if outline { 1.0 } else { 0.0 };
            changed = true;
        }
        if let Some(label) = option.children.get_mut(0) {
            if label.text_style.color != want_fg {
                label.text_style.color = want_fg;
                changed = true;
            }
        }
    }
    if changed {
        crate::core::tree::LayoutStatus::StyleChanged
    } else {
        crate::core::tree::LayoutStatus::Clean
    }
}
