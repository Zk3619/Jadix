//! # ToggleGroup
//!
//! 对齐 solid-ui / Kobalte **ToggleGroup**：一组两态按钮。
//! - `multiple=false`（默认）：单选，值 `Signal<String>`（空串表示未选；[`required`](ToggleGroupBuilder::required) 时禁止空选）
//! - `multiple=true`：多选，值用 `Signal<Vec<String>>`
//! - `joined=true`：连体分段外观（[`crate::components::tabs_list`] 使用）
//! - 单选支持 ←/→/↑/↓ / Home / End **只移焦点环**；Enter / Space / 点击才改选中
//! - 选中态通过组件根的 [`crate::core::tree::ReactiveSync`] 原位同步，不订阅根 UI effect。
//!
//! ## Usage
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
use crate::style::{control_focus_ring_active, focus_ring_color, FOCUS_RING_BLUR};
use crate::utils::roving::{focus_roving_key, FocusRovingAxis};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use winit::event::KeyEvent;

pub struct ToggleGroupBuilder {
    id: String,
    multiple: bool,
    value_single: Option<Signal<String>>,
    value_multi: Option<Signal<Vec<String>>>,
    default_value: String,
    options: Vec<(String, String)>,
    disabled: bool,
    variant: String,
    joined: bool,
    /// Single-select only: refuse empty selection (TabsList).
    required: bool,
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
        joined: false,
        required: false,
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
    /// 连体分段：共享外框、段间无缝、首尾圆角（TabsList）。
    pub fn joined(mut self, v: bool) -> Self {
        self.joined = v;
        self
    }
    /// 单选必选一项（点击已选项不取消；空值回落到 default / 首项）。
    pub fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }
}

/// Resulting value after clicking a single-select chip.
pub(crate) fn single_select_after_click(current: &str, clicked: &str, required: bool) -> String {
    if current == clicked {
        if required {
            current.to_string()
        } else {
            String::new()
        }
    } else {
        clicked.to_string()
    }
}

/// Resolve a non-empty selection when `required` (invalid / empty → default → first).
pub(crate) fn coerce_required_single(
    current: &str,
    values: &[String],
    default_value: &str,
) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    if values.iter().any(|v| v == current) {
        return None;
    }
    if !default_value.is_empty() && values.iter().any(|v| v == default_value) {
        return Some(default_value.to_string());
    }
    Some(values[0].clone())
}

fn toggle_chip_colors(
    pressed: bool,
    disabled: bool,
    outline: bool,
    joined: bool,
) -> ([f32; 4], [f32; 4], [u8; 4]) {
    let mut bg = if pressed {
        if joined {
            parse_color("background")
        } else {
            parse_color("accent")
        }
    } else {
        parse_color("transparent")
    };
    let mut border = if outline && !joined {
        parse_color("input")
    } else {
        parse_color("transparent")
    };
    let mut fg = if pressed {
        if joined {
            parse_text_color("foreground")
        } else {
            parse_text_color("accent-foreground")
        }
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

fn joined_radii(index: usize, count: usize, r: f32) -> [f32; 4] {
    if count <= 1 {
        return [r, r, r, r];
    }
    if index == 0 {
        [r, 0.0, 0.0, r]
    } else if index + 1 == count {
        [0.0, r, r, 0.0]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Arrow / Home / End: move focus ring only (selection unchanged).
/// Enter / Space: not handled here — falls through to `on_click` to activate selection.
fn single_select_focus_roving_handler(
    group_id: String,
    values: Vec<String>,
    index: usize,
    disabled: bool,
) -> impl FnMut(KeyEvent) -> bool {
    let ids: Vec<String> = values.iter().map(|v| format!("{group_id}:{v}")).collect();
    move |ev: KeyEvent| {
        if disabled {
            return false;
        }
        focus_roving_key(&ev, &ids, index, FocusRovingAxis::Both)
    }
}

fn item_el(
    group_id: &str,
    value: &str,
    label: &str,
    pressed: bool,
    disabled: bool,
    outline: bool,
    joined: bool,
    index: usize,
    count: usize,
    on_click: impl FnMut() + 'static,
    on_key_down: Option<Box<dyn FnMut(KeyEvent) -> bool>>,
) -> Element {
    let (bg, border, fg) = toggle_chip_colors(pressed, disabled, outline, joined);
    let border_w = if outline && !joined { 1.0 } else { 0.0 };
    let mut chip: Element = r#box()
        .px("0.75rem")
        .py("0.375rem")
        .bg(bg)
        .border(border, border_w)
        .into();
    if joined {
        chip.style.radii = joined_radii(index, count, 0.25 * 16.0);
        if pressed {
            chip = chip.shadow([0.0, 0.0, 0.0, 0.08], 2.0);
        }
    } else {
        chip = chip.rounded("0.375rem");
    }
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
    let fid_click = focus_id.clone();
    let mut handler = on_click;
    let mut builder = interactive(chip)
        .focus_id(focus_id)
        .focusable(!disabled)
        .focus_ring(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal("__rui_button_focus_id", String::new).set(fid_click.clone());
            }
            handler();
        });
    if let Some(key_cb) = on_key_down {
        builder = builder.on_key_down(key_cb);
    }
    let mut out: Element = builder.into();
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
        let joined = builder.joined;
        let required = builder.required && !builder.multiple;
        let group_id = builder.id.clone();
        let multiple = builder.multiple;
        let option_values: Vec<String> = builder
            .options
            .iter()
            .map(|(value, _)| value.clone())
            .collect();
        let count = builder.options.len();

        let mut row: Element = if joined {
            hstack()
                .gap("0")
                .items_center()
                .p("0.125rem")
                .rounded("0.375rem")
                .bg(parse_color("muted"))
                .border(parse_color("input"), 1.0)
                .into()
        } else {
            hstack().gap("0.25rem").items_center().into()
        };

        if multiple {
            let scope = format!("{}__tg_multi", builder.id);
            let sig = builder
                .value_multi
                .unwrap_or_else(|| use_signal(scope.as_str(), Vec::new));
            let selected = sig.peek();
            for (i, (val, label)) in builder.options.into_iter().enumerate() {
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
                    joined,
                    i,
                    count,
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
                    None,
                ));
            }
            let sync_sig = sig;
            row = row.reactive_sync(move |root, _layout_tree| {
                sync_toggle_group_pressed(
                    root,
                    &option_values,
                    outline,
                    disabled,
                    joined,
                    |value| sync_sig.peek().iter().any(|v| v == value),
                )
            });
        } else {
            let scope = format!("{}__tg", builder.id);
            let default = builder.default_value.clone();
            let sig = builder
                .value_single
                .unwrap_or_else(|| use_signal(scope.as_str(), || default.clone()));
            if required {
                if let Some(fixed) = coerce_required_single(&sig.peek(), &option_values, &default) {
                    sig.set(fixed);
                }
            }
            let current = sig.peek();
            for (i, (val, label)) in builder.options.into_iter().enumerate() {
                let pressed = current == val;
                let sig_c = sig;
                let v_c = val.clone();
                let req = required;
                let values_k = option_values.clone();
                let gid = group_id.clone();
                row = row.child(item_el(
                    &group_id,
                    &val,
                    &label,
                    pressed,
                    disabled,
                    outline,
                    joined,
                    i,
                    count,
                    move || {
                        if disabled {
                            return;
                        }
                        let next = single_select_after_click(&sig_c.get(), &v_c, req);
                        if sig_c.peek() != next {
                            sig_c.set(next);
                        }
                    },
                    Some(Box::new(single_select_focus_roving_handler(
                        gid, values_k, i, disabled,
                    ))),
                ));
            }
            let sync_sig = sig;
            let values_sync = option_values.clone();
            let default_sync = default;
            row = row.reactive_sync(move |root, _layout_tree| {
                if required {
                    if let Some(fixed) =
                        coerce_required_single(&sync_sig.peek(), &values_sync, &default_sync)
                    {
                        // Silent: paint sync may hold AppState; avoid effect flush / reconcile.
                        sync_sig.set_silent(fixed);
                    }
                }
                let current = sync_sig.peek();
                sync_toggle_group_pressed(
                    root,
                    &option_values,
                    outline,
                    disabled,
                    joined,
                    |value| current == *value,
                )
            });
        }

        row.template_id = Some(builder.id);
        if required {
            row.set_attr("rui_toggle_required", "1");
        }
        row
    }
}

fn sync_toggle_group_pressed(
    root: &mut Element,
    option_values: &[String],
    outline: bool,
    disabled: bool,
    joined: bool,
    mut is_pressed: impl FnMut(&str) -> bool,
) -> crate::core::tree::LayoutStatus {
    let mut changed = false;
    let count = option_values.len();
    let border_w = if outline && !joined { 1.0 } else { 0.0 };
    for (i, (option, value)) in root
        .children
        .iter_mut()
        .zip(option_values.iter())
        .enumerate()
    {
        let pressed = is_pressed(value.as_str());
        let (want_bg, want_border, want_fg) =
            toggle_chip_colors(pressed, disabled, outline, joined);
        if option.style.bg_color != want_bg {
            option.style.bg_color = want_bg;
            changed = true;
        }
        if option.style.border_color != want_border {
            option.style.border_color = want_border;
            changed = true;
        }
        if option.style.border_width != border_w {
            option.style.border_width = border_w;
            changed = true;
        }
        if joined {
            let want_radii = joined_radii(i, count, 0.25 * 16.0);
            if option.style.radii != want_radii {
                option.style.radii = want_radii;
                changed = true;
            }
            // Prefer keyboard focus ring over pressed chip shadow (same token/blur as Input).
            let show_ring = option
                .focus_id
                .as_deref()
                .is_some_and(|fid| control_focus_ring_active(fid, option.focusable, disabled));
            let (want_shadow, want_blur) = if show_ring {
                (focus_ring_color(), FOCUS_RING_BLUR)
            } else if pressed {
                ([0.0, 0.0, 0.0, 0.08], 2.0)
            } else {
                ([0.0, 0.0, 0.0, 0.0], 0.0)
            };
            if (option.style.shadow_blur - want_blur).abs() > 0.01
                || (option.style.shadow_color[0] - want_shadow[0]).abs() > 0.001
                || (option.style.shadow_color[1] - want_shadow[1]).abs() > 0.001
                || (option.style.shadow_color[2] - want_shadow[2]).abs() > 0.001
                || (option.style.shadow_color[3] - want_shadow[3]).abs() > 0.001
            {
                option.style.shadow_color = want_shadow;
                option.style.shadow_blur = want_blur;
                option.style.shadow_offset = [0.0, 0.0];
                option.style.shadow_spread = 0.0;
                changed = true;
            }
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

impl crate::core::tree::IntoElementList for ToggleGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_click_keeps_selection() {
        assert_eq!(single_select_after_click("list", "list", true), "list");
        assert_eq!(single_select_after_click("list", "grid", true), "grid");
        assert_eq!(single_select_after_click("list", "list", false), "");
    }

    #[test]
    fn coerce_required_falls_back() {
        let vals = vec!["list".into(), "grid".into()];
        assert_eq!(coerce_required_single("list", &vals, "grid"), None);
        assert_eq!(
            coerce_required_single("", &vals, "grid"),
            Some("grid".into())
        );
        assert_eq!(
            coerce_required_single("nope", &vals, ""),
            Some("list".into())
        );
    }
}
