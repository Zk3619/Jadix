//! Menu CheckboxItem / RadioItem 控制面（对齐 [`checkbox`] / [`radio_group`] chrome）。
//!
//! 选中态走 `checkbox_sync` / `radio_selected_sync` + 每帧 paint sync，**禁止**
//! `reactive_view` 订阅选中信号——否则切换选项会在关菜单前先重建指示器，出现
//!「背景先关 → 内容闪一下」的竞态。

use crate::core::reactivity::Signal;
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::r#box::{hstack, r#box};
use crate::style::is_dark_theme;
use crate::utils::lucide_glyph;
use crate::utils::text_node::build_icon_text_node;

const ATTR_INDICATOR: &str = "data-slot";
const SLOT_CHECKBOX_INDICATOR: &str = "checkbox-indicator";
const SLOT_RADIO_INDICATOR: &str = "radio-indicator";
/// 与 [`crate::components::radio_group`] 的 `rui_radio_value` 一致，供全局 sync 读取。
const ATTR_RADIO_VALUE: &str = "rui_radio_value";

const BOX_PX: f32 = 16.0;
const CHECK_PX: f32 = 12.0;
const DOT_PX: f32 = 10.0;
const BORDER_W: f32 = 1.5;

fn unchecked_bg() -> [f32; 4] {
    if is_dark_theme() {
        let mut c = parse_color("input");
        c[3] = 0.3;
        c
    } else {
        let mut c = parse_color("background");
        c[3] = 1.0;
        c
    }
}

fn unchecked_border() -> [f32; 4] {
    parse_color("primary")
}

fn checked_chrome() -> ([f32; 4], [f32; 4], [u8; 4]) {
    (
        parse_color("primary"),
        parse_color("primary"),
        parse_text_color("primary-foreground"),
    )
}

fn check_glyph() -> String {
    lucide_glyph("check").unwrap_or_else(|| "✓".into())
}

fn indicator_fg(on: bool) -> [u8; 4] {
    if on {
        parse_text_color("primary-foreground")
    } else {
        [0, 0, 0, 0]
    }
}

fn build_check_indicator(fg: [u8; 4]) -> Element {
    let mut icon = build_icon_text_node(check_glyph())
        .w(CHECK_PX)
        .h(CHECK_PX)
        .min_w(CHECK_PX)
        .min_h(CHECK_PX)
        .text_size(CHECK_PX)
        .text_color(fg)
        .text_center()
        .flex_shrink(0.0);
    icon.attrs.insert(
        ATTR_INDICATOR.to_string(),
        SLOT_CHECKBOX_INDICATOR.to_string(),
    );
    icon.attrs
        .insert("text_align".to_string(), "center".to_string());
    icon
}

/// solid-ui Checkbox Control：`size-4 rounded-sm border-primary`；挂 `checkbox_sync`。
pub(crate) fn menu_checkbox_control(checked: Signal<bool>) -> Element {
    let on = checked.peek();
    let (bg, border) = if on {
        let (bg, border, _) = checked_chrome();
        (bg, border)
    } else {
        (unchecked_bg(), unchecked_border())
    };
    let fg = indicator_fg(on);
    let mut control: Element = hstack()
        .w(BOX_PX)
        .h(BOX_PX)
        .min_w(BOX_PX)
        .min_h(BOX_PX)
        .rounded("0.125rem")
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .into();
    control = control
        .border(border, BORDER_W)
        .bg(bg)
        .child(build_check_indicator(fg));
    control.checkbox_sync = Some(checked);
    control.pointer_events_none()
}

fn radio_control_bg() -> [f32; 4] {
    parse_color("background")
}

fn radio_control_border() -> [f32; 4] {
    parse_color("primary")
}

fn indicator_dot_color(on: bool) -> [f32; 4] {
    if on {
        parse_color("primary")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn build_radio_dot(on: bool) -> Element {
    let mut dot: Element = r#box()
        .w(DOT_PX)
        .h(DOT_PX)
        .min_w(DOT_PX)
        .min_h(DOT_PX)
        .rounded("9999px")
        .bg(indicator_dot_color(on))
        .flex_shrink(0.0)
        .into();
    dot.attrs
        .insert(ATTR_INDICATOR.to_string(), SLOT_RADIO_INDICATOR.to_string());
    dot
}

/// solid-ui Radio ItemControl：圆形 `size-4` + 中心实心点；挂 `radio_selected_sync`。
pub(crate) fn menu_radio_control(selected: Signal<String>, value: &str) -> Element {
    let on = selected.peek() == value;
    let mut control: Element = hstack()
        .w(BOX_PX)
        .h(BOX_PX)
        .min_w(BOX_PX)
        .min_h(BOX_PX)
        .rounded("9999px")
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .into();
    control = control
        .border(radio_control_border(), BORDER_W)
        .bg(radio_control_bg())
        .child(build_radio_dot(on));
    control.radio_selected_sync = Some(selected);
    control
        .attrs
        .insert(ATTR_RADIO_VALUE.to_string(), value.to_string());
    control.pointer_events_none()
}
