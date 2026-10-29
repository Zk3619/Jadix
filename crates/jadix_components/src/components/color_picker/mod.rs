//! # ColorPicker
//!
//! Compare Reader `color-picker` + [solid-color](https://github.com/xbmlz/solid-color)
//! (HSV area + hue/transparency slider + color palette + format): Corresponds to HTML `input type="color"`.
//!
//! **格式**：`hex` / `hex8` / `rgb` / `rgba` / `hsl` / `hsla`；面板内左侧
//! [`select`] changes the type, and the right side [`input`] edits the color value, always on the same line.
//! **默认变体**：不再按是否支持透明度拆两套 UI（格式列表固定六项；色相下滑条含 alpha）。
//! **variant**: `default` floating layer; `inline` expansion; and `compact` / `sketch` / `chrome` /
//! `github` / `twitter` / `material` / `block` / `circle` / `slider`。
//! **键盘**：触发器 Enter/Space 开合、Esc 关闭；面积 ←→ 饱和度 ↑↓ 明度；
//! Hue/Transparency slider ←→ step.
//!
//! ## 模块拆分（产品层）
//! - [`model`]：色空间 / 解析 / 格式化
//! - [`drag`]：SV 拖拽会话
//! - [`view`]：纹理与面板 UI
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <ColorPicker id="accent" value="#3b82f6" />
//!     <ColorPicker id="fill" variant="inline" format="hsla" />
//!     <ColorPicker id="tw" variant="twitter" value="#0693E3" />
//!     <ColorPicker id="c" valueSignal={color_sig} variant="sketch" />
//! }
//! color_picker("accent")
//!     .default_value("#3b82f6")
//!     .variant(ColorPickerVariant::Chrome)
//! ```

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{
    bind_style, bind_text, reactive_component_live, show_reactive, StylePatch,
};
use crate::components::base::{SlotChrome, SlotMap};
use crate::components::button::button;
use crate::components::input::input;
use crate::components::select::{select, SelectItemData};
use crate::components::separator::separator;
use crate::components::slider::slider;
use crate::core::reactivity::{create_effect, use_signal, with_child_owner, Signal};
use crate::core::tree::{parse_text_color, CursorKind, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::controlled::{resolve_open_signal, resolve_string_signal};
use crate::utils::popup::{PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use taffy::prelude::LengthPercentageAuto;
use winit::keyboard::{Key, NamedKey};

fn trigger_focus_id(id: &str) -> String {
    format!("{id}:trigger")
}

fn close_picker(open: Signal<bool>, trigger_fid: impl Into<String>) {
    open.set(false);
    focus_id_only(trigger_fid.into());
}

mod drag;
mod model;
pub mod view;

pub use model::{
    format_color, hsl_to_rgb, hsv_to_rgb, parse_color_str, rgb_to_hsl, rgb_to_hsv, ColorFormat,
    ColorPickerVariant, Rgba,
};
use view::{build_color_picker_view, follow_fmt_str, follow_hue_into_rgba, ColorPickerRenderProps};

crate::__component_contract! {
ColorPickerProps [];
/// HTML `input type="color"` + Reader 式 HSV 面板。
#[allow(non_snake_case)]
pub fn ColorPicker(
identity [] id: String,
plain [] value: Option<String>,
plain [] value_signal: Option<Signal<String>>,
plain [] variant: Option<String>,
plain [] format: Option<String>,
plain [] show_alpha: Option<bool>,
default [] disabled: bool,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = color_picker(&id)
        .disabled(disabled)
        .show_alpha(show_alpha.unwrap_or(true));
    if let Some(sig) = value_signal {
        b = b.value_signal(sig);
    }
    if let Some(v) = value {
        b = b.default_value(v);
    }
    if let Some(v) = variant {
        b = b.variant(ColorPickerVariant::parse(&v));
    }
    if let Some(f) = format {
        b = b.format(ColorFormat::parse(&f));
    }
    b.into()
}
}

pub struct ColorPickerBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    variant: ColorPickerVariant,
    format: ColorFormat,
    show_alpha: bool,
    disabled: bool,
    chrome: SlotChrome,
    slots: SlotMap,
}

pub fn color_picker(id: impl Into<String>) -> ColorPickerBuilder {
    ColorPickerBuilder {
        id: id.into(),
        value: None,
        default_value: "#3b82f6".into(),
        variant: ColorPickerVariant::Default,
        format: ColorFormat::Hex,
        show_alpha: true,
        disabled: false,
        chrome: SlotChrome::default(),
        slots: SlotMap::default(),
    }
}

impl ColorPickerBuilder {
    crate::impl_slot_chrome_methods!();
    crate::impl_slot_map_methods!();

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
    pub fn variant(mut self, v: ColorPickerVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn format(mut self, f: ColorFormat) -> Self {
        self.format = f;
        self
    }
    pub fn show_alpha(mut self, on: bool) -> Self {
        self.show_alpha = on;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

impl From<ColorPickerBuilder> for Element {
    fn from(builder: ColorPickerBuilder) -> Self {
        let ColorPickerBuilder {
            id,
            value,
            default_value,
            variant,
            format,
            show_alpha,
            disabled,
            chrome,
            slots,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "color_picker");
        let initial = parse_color_str(&default_value).unwrap_or(Rgba::opaque(59, 130, 246));
        let out = resolve_string_signal(value, &id, "value", format_color(initial, format));
        let parsed0 = parse_color_str(&out.peek()).unwrap_or(initial);
        let rgba = use_signal(format!("{id}__cp_rgba").as_str(), || parsed0);
        let (h0, s0, v0) = rgb_to_hsv(parsed0.r, parsed0.g, parsed0.b);
        let hue0 = if s0 > 0.001 && v0 > 0.001 { h0 } else { 210.0 };
        let hue = use_signal(format!("{id}__cp_hue").as_str(), move || hue0);
        let sat = use_signal(format!("{id}__cp_sat").as_str(), move || s0);
        let val = use_signal(format!("{id}__cp_val").as_str(), move || v0);
        let fmt_sig = use_signal(format!("{id}__cp_fmt").as_str(), || format);
        let fmt0 = format.as_str().to_string();
        let fmt_str = use_signal(format!("{id}__cp_fmt_str").as_str(), move || fmt0.clone());
        let open = resolve_open_signal(None, &id, false);
        follow_hue_into_rgba(rgba, hue, sat, val, out, fmt_sig);
        follow_fmt_str(fmt_str, fmt_sig, out, rgba);
        // Defer SV/hue CPU ramps until the panel mounts (`sv_area` /
        // `sync_sv_visuals`). Eager generation here made every closed Gallery
        // ColorPicker pay 192×144 RGBA on forms scroll (10+ pickers → hitch).
        let view_id = format!("{id}__cp_view");
        let props = ColorPickerRenderProps {
            id,
            variant,
            format,
            show_alpha,
            disabled,
        };
        let mut slots = slots;
        slots = slots.slot("color-picker-trigger", |c| chrome.overlay(c));
        reactive_component_live(view_id, props, move |p| {
            build_color_picker_view(p, open, out, rgba, hue, sat, val, fmt_sig, fmt_str, &slots)
        })
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/mod.rs"]
mod tests;
