//! # ColorPicker
//!
//! 对照 Reader `color-picker`（HSV 面积 + 色相/透明度滑块 + 色板 + 格式）：
//! 对应 HTML `input type="color"`，桌面无 EyeDropper。
//!
//! **格式**：`hex` `#RRGGBB`；`hex8` `#RRGGBBAA`；`rgb` / `rgba`；`hsl` / `hsla`。
//! **variant**：`default` 触发器+浮层；`inline` 直接展开面板。
//! **键盘**：触发器 Enter/Space 开合、Esc 关闭；面积 ←→ 饱和度 ↑↓ 明度；
//! 色相/透明度滑块 ←→ 步进（Shift ×10）。
//!
//! ## 用法
//! ```ignore
//! ui! {
//!     <ColorPicker id="accent" value="#3b82f6" />
//!     <ColorPicker id="fill" variant="inline" showAlpha={true} format="hsla" />
//!     <ColorPicker id="c" valueSignal={color_sig} variant="default" />
//! }
//! color_picker("accent")
//!     .default_value("#3b82f6")
//!     .variant(ColorPickerVariant::Inline)
//!     .show_alpha(true)
//! ```

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_component_live;
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, CursorKind, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    apply_stored_floating_place, floating_portal_anchor, mark_floating, panel_block_clicks,
    PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W,
};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

fn trigger_focus_id(id: &str) -> String {
    format!("{id}:trigger")
}

fn close_picker(open: Signal<bool>, trigger_fid: impl Into<String>) {
    open.set(false);
    focus_id_only(trigger_fid.into());
}

const PRESET_SWATCHES: &[&str] = &[
    "#000000", "#ffffff", "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6",
    "#8b5cf6", "#ec4899", "#64748b", "#0f172a",
];

/// 0–255 RGB + 0–1 alpha。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Rgba {
    pub fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub fn to_f32(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a.clamp(0.0, 1.0),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorFormat {
    #[default]
    Hex,
    Hex8,
    Rgb,
    Rgba,
    Hsl,
    Hsla,
}

impl ColorFormat {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hex8" => Self::Hex8,
            "rgb" => Self::Rgb,
            "rgba" => Self::Rgba,
            "hsl" => Self::Hsl,
            "hsla" => Self::Hsla,
            _ => Self::Hex,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Hex8 => "hex8",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
            Self::Hsl => "hsl",
            Self::Hsla => "hsla",
        }
    }
    pub fn supports_alpha(self) -> bool {
        matches!(self, Self::Hex8 | Self::Rgba | Self::Hsla)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPickerVariant {
    #[default]
    Default,
    Inline,
}

impl ColorPickerVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline" => Self::Inline,
            _ => Self::Default,
        }
    }
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = h.rem_euclid(360.0);
    let s = if max < 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / d) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s * 100.0, l * 100.0)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c * 0.5;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn parse_color_str(raw: &str) -> Option<Rgba> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|t| t.strip_suffix(')'))
    {
        let parts: Vec<_> = inner.split(',').map(|t| t.trim()).collect();
        if parts.len() >= 3 {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            let a = if parts.len() >= 4 {
                parts[3].parse().unwrap_or(1.0)
            } else {
                1.0
            };
            return Some(Rgba { r, g, b, a });
        }
    }
    if let Some(inner) = lower
        .strip_prefix("hsla(")
        .or_else(|| lower.strip_prefix("hsl("))
        .and_then(|t| t.strip_suffix(')'))
    {
        let parts: Vec<_> = inner
            .split(',')
            .map(|t| t.trim().trim_end_matches('%'))
            .collect();
        if parts.len() >= 3 {
            let h: f32 = parts[0].parse().ok()?;
            let s: f32 = parts[1].parse().ok()?;
            let l: f32 = parts[2].parse().ok()?;
            let a = if parts.len() >= 4 {
                parts[3].parse().unwrap_or(1.0)
            } else {
                1.0
            };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            return Some(Rgba { r, g, b, a });
        }
    }
    parse_hex(s.trim_start_matches('#'))
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    let h = hex.trim();
    match h.len() {
        3 => {
            let r = u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?;
            Some(Rgba::opaque(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            Some(Rgba::opaque(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            let a = u8::from_str_radix(&h[6..8], 16).ok()? as f32 / 255.0;
            Some(Rgba { r, g, b, a })
        }
        _ => None,
    }
}

pub fn format_color(c: Rgba, fmt: ColorFormat) -> String {
    match fmt {
        ColorFormat::Hex => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        ColorFormat::Hex8 => {
            let a = (c.a.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, a)
        }
        ColorFormat::Rgb => format!("rgb({}, {}, {})", c.r, c.g, c.b),
        ColorFormat::Rgba => format!("rgba({}, {}, {}, {:.2})", c.r, c.g, c.b, c.a),
        ColorFormat::Hsl => {
            let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
            format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s, l)
        }
        ColorFormat::Hsla => {
            let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
            format!("hsla({:.0}, {:.0}%, {:.0}%, {:.2})", h, s, l, c.a)
        }
    }
}

fn hue_color(h: f32) -> [f32; 4] {
    let (r, g, b) = hsv_to_rgb(h.rem_euclid(360.0), 1.0, 1.0);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

fn sv_texture_key(id: &str, hue: f32) -> String {
    format!("rui_cp_sv:{id}:{:.0}", hue.rem_euclid(360.0))
}

fn hue_texture_key() -> &'static str {
    "rui_cp_hue_ramp"
}

fn ensure_sv_texture(id: &str, hue: f32) -> String {
    let key = sv_texture_key(id, hue);
    let w = 96u32;
    let h = 72u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let v = 1.0 - y as f32 / (h - 1).max(1) as f32;
        for x in 0..w {
            let s = x as f32 / (w - 1).max(1) as f32;
            let (r, g, b) = hsv_to_rgb(hue.rem_euclid(360.0), s, v);
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    crate::queue_image_rgba8(key.clone(), w, h, rgba);
    key
}

fn ensure_hue_texture() {
    let w = 180u32;
    let h = 8u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for x in 0..w {
        let hue = x as f32 / (w - 1).max(1) as f32 * 360.0;
        let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
        for y in 0..h {
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    crate::queue_image_rgba8(hue_texture_key(), w, h, rgba);
}

#[derive(Clone, Copy)]
enum ColorDragKind {
    Sv,
    Hue,
    Alpha,
}

#[derive(Clone)]
struct ColorDrag {
    kind: ColorDragKind,
    bounds: Rc<Cell<[f32; 4]>>,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    show_alpha: bool,
}

thread_local! {
    static COLOR_DRAG: RefCell<Option<ColorDrag>> = const { RefCell::new(None) };
}

fn begin_color_drag(drag: ColorDrag) {
    COLOR_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

fn end_color_drag() {
    COLOR_DRAG.with(|d| *d.borrow_mut() = None);
    crate::app::release_pointer_capture();
}

pub(crate) fn is_color_dragging() -> bool {
    COLOR_DRAG.with(|d| d.borrow().is_some())
}

pub(crate) fn tick_color_drag(x: f32, y: f32) -> bool {
    COLOR_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        apply_color_pointer(&drag, x, y);
        true
    })
}

pub(crate) fn clear_color_drag_if_any() {
    if is_color_dragging() {
        end_color_drag();
    }
}

fn apply_color_pointer(drag: &ColorDrag, x: f32, y: f32) {
    let b = drag.bounds.get();
    if b[2] <= 1.0 || b[3] <= 1.0 {
        return;
    }
    let tx = ((x - b[0]) / b[2]).clamp(0.0, 1.0);
    let ty = ((y - b[1]) / b[3]).clamp(0.0, 1.0);
    match drag.kind {
        ColorDragKind::Sv => {
            let hue = drag.hue.peek();
            let (r, g, bch) = hsv_to_rgb(hue, tx, 1.0 - ty);
            let a = drag.rgba.peek().a;
            drag.rgba.set(Rgba {
                r,
                g,
                b: bch,
                a: if drag.show_alpha { a } else { 1.0 },
            });
        }
        ColorDragKind::Hue => {
            let hue = tx * 360.0;
            drag.hue.set(hue);
            let cur = drag.rgba.peek();
            let (_, s, v) = rgb_to_hsv(cur.r, cur.g, cur.b);
            let (r, g, bch) = hsv_to_rgb(hue, s.max(0.001), v.max(0.001));
            drag.rgba.set(Rgba {
                r,
                g,
                b: bch,
                a: cur.a,
            });
        }
        ColorDragKind::Alpha => {
            let mut c = drag.rgba.peek();
            c.a = tx;
            drag.rgba.set(c);
        }
    }
}

fn trigger_label(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 0.875 * 16.0,
            weight: 400,
            color: parse_text_color("foreground"),
            ..Default::default()
        },
    )
}

fn picker_trigger(id: &str, leading: Element, main: Element, disabled: bool) -> Element {
    let mut border = parse_color("input");
    let mut bg = parse_color("transparent");
    if disabled {
        border[3] *= 0.5;
        bg[3] *= 0.5;
    }
    let mut row: Element = hstack()
        .h("2.25rem")
        .w("100%")
        .min_w("0")
        .rounded("0.375rem")
        .px("0.75rem")
        .gap("0.5rem")
        .items_center()
        .bg(bg)
        .border(border, 1.0)
        .shadow([0.0, 0.0, 0.0, 0.06], 1.0)
        .into();
    row = row.child(leading).child(Element::from(
        hstack()
            .flex_grow(1.0)
            .min_w("0")
            .items_center()
            .child(main),
    ));
    let cursor = if disabled {
        row.cursor_not_allowed()
    } else {
        row.cursor_kind(CursorKind::Pointer)
    };
    let trigger_fid = trigger_focus_id(id);
    let mut out: Element = interactive(cursor)
        .focus_id(trigger_fid.as_str())
        .focusable(!disabled)
        .into();
    out.template_id = Some(trigger_fid);
    out
}

/// HTML `input type="color"` + Reader 式 HSV 面板。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ColorPicker(
    id: String,
    value: Option<String>,
    value_signal: Option<Signal<String>>,
    variant: Option<String>,
    format: Option<String>,
    show_alpha: bool,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = color_picker(&id).disabled(disabled).show_alpha(show_alpha);
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

pub struct ColorPickerBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    variant: ColorPickerVariant,
    format: ColorFormat,
    show_alpha: bool,
    disabled: bool,
}

pub fn color_picker(id: impl Into<String>) -> ColorPickerBuilder {
    ColorPickerBuilder {
        id: id.into(),
        value: None,
        default_value: "#3b82f6".into(),
        variant: ColorPickerVariant::Default,
        format: ColorFormat::Hex,
        show_alpha: false,
        disabled: false,
    }
}

impl ColorPickerBuilder {
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

#[derive(Clone, PartialEq)]
struct ColorPickerRenderProps {
    id: String,
    variant: ColorPickerVariant,
    format: ColorFormat,
    show_alpha: bool,
    disabled: bool,
}

fn sync_string_from_rgba(out: Signal<String>, c: Rgba, fmt: ColorFormat) {
    out.set(format_color(c, fmt));
}

fn build_panel(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    show_alpha: bool,
    disabled: bool,
    out: Signal<String>,
) -> Element {
    let c = rgba.get();
    let hue_v = hue.get();
    let fmt = fmt_sig.get();
    let (_, sat, val) = rgb_to_hsv(c.r, c.g, c.b);
    ensure_hue_texture();
    let sv_key = ensure_sv_texture(id, hue_v);

    let mut area: Element = r#box().w("100%").h("9rem").rounded("0.375rem").into();
    area = area.overflow("hidden").border(tokens::border(), 1.0);
    area.image_texture = Some(sv_key);
    let mut knob: Element = r#box().w("0.75rem").h("0.75rem").rounded("9999px").into();
    knob = knob
        .absolute()
        .left(format!("calc({:.2}% - 6px)", sat * 100.0))
        .top(format!("calc({:.2}% - 6px)", (1.0 - val) * 100.0))
        .bg(c.to_f32())
        .border([1.0, 1.0, 1.0, 1.0], 2.0)
        .pointer_events("none");
    area = area.relative().child(knob);

    let sv_bounds = Rc::new(Cell::new([0.0f32; 4]));
    if !disabled {
        let b_sv = sv_bounds.clone();
        let drag = ColorDrag {
            kind: ColorDragKind::Sv,
            bounds: sv_bounds.clone(),
            rgba,
            hue,
            show_alpha,
        };
        let fid = format!("{id}:sv");
        area = area.on_pointer_down({
            let drag = drag.clone();
            let fid = fid.clone();
            let out = out;
            let fmt_sig = fmt_sig;
            move |e| {
                if e.phase == EventPhase::Capture {
                    return;
                }
                let b = if e.current_bounds[2] > 0.0 {
                    e.current_bounds
                } else {
                    b_sv.get()
                };
                b_sv.set(b);
                apply_color_pointer(&drag, e.x, e.y);
                sync_string_from_rgba(out, drag.rgba.peek(), fmt_sig.peek());
                begin_color_drag(drag.clone());
                crate::app::set_pointer_capture(fid.clone());
                e.stop_propagation();
            }
        });
        area = interactive(area)
            .focus_id(fid.as_str())
            .focusable(true)
            .focus_ring(true)
            .on_key_down({
                let rgba_k = rgba;
                let hue_k = hue;
                let out_k = out;
                let fmt_k = fmt_sig;
                move |ev| {
                    let cur = rgba_k.peek();
                    let (_, mut s, mut v) = rgb_to_hsv(cur.r, cur.g, cur.b);
                    let step = 0.02;
                    match &ev.logical_key {
                        Key::Named(NamedKey::ArrowLeft) => s = (s - step).max(0.0),
                        Key::Named(NamedKey::ArrowRight) => s = (s + step).min(1.0),
                        Key::Named(NamedKey::ArrowUp) => v = (v + step).min(1.0),
                        Key::Named(NamedKey::ArrowDown) => v = (v - step).max(0.0),
                        Key::Named(NamedKey::Home) => s = 0.0,
                        Key::Named(NamedKey::End) => s = 1.0,
                        Key::Named(NamedKey::PageUp) => v = 1.0,
                        Key::Named(NamedKey::PageDown) => v = 0.0,
                        _ => return false,
                    }
                    let (r, g, b) = hsv_to_rgb(hue_k.peek(), s, v);
                    rgba_k.set(Rgba { r, g, b, a: cur.a });
                    sync_string_from_rgba(out_k, rgba_k.peek(), fmt_k.peek());
                    true
                }
            })
            .into();
        area = area.cursor_kind(CursorKind::Pointer);
        let sync_b = sv_bounds;
        area = area.reactive_sync(move |root, _| {
            sync_b.set(root.layout_bounds);
            crate::core::tree::LayoutStatus::Clean
        });
    }

    let mut hue_track: Element = r#box().w("100%").h("0.75rem").rounded("9999px").into();
    hue_track.image_texture = Some(hue_texture_key().into());
    hue_track = hue_track.overflow("hidden").relative();
    let mut hue_knob: Element = r#box().w("0.75rem").h("0.75rem").rounded("9999px").into();
    hue_knob = hue_knob
        .absolute()
        .left(format!(
            "calc({:.2}% - 6px)",
            (hue_v.rem_euclid(360.0) / 360.0) * 100.0
        ))
        .top("-2px")
        .bg(hue_color(hue_v))
        .border([1.0, 1.0, 1.0, 1.0], 2.0)
        .pointer_events("none");
    hue_track = hue_track.child(hue_knob);
    let hue_bounds = Rc::new(Cell::new([0.0f32; 4]));
    if !disabled {
        let b_h = hue_bounds.clone();
        let drag = ColorDrag {
            kind: ColorDragKind::Hue,
            bounds: hue_bounds.clone(),
            rgba,
            hue,
            show_alpha,
        };
        let fid = format!("{id}:hue");
        hue_track = hue_track.on_pointer_down({
            let drag = drag.clone();
            let fid = fid.clone();
            let out = out;
            let fmt_sig = fmt_sig;
            move |e| {
                if e.phase == EventPhase::Capture {
                    return;
                }
                let b = if e.current_bounds[2] > 0.0 {
                    e.current_bounds
                } else {
                    b_h.get()
                };
                b_h.set(b);
                apply_color_pointer(&drag, e.x, e.y);
                sync_string_from_rgba(out, drag.rgba.peek(), fmt_sig.peek());
                begin_color_drag(drag.clone());
                crate::app::set_pointer_capture(fid.clone());
                e.stop_propagation();
            }
        });
        hue_track = interactive(hue_track)
            .focus_id(fid.as_str())
            .focusable(true)
            .focus_ring(true)
            .on_key_down({
                let hue_k = hue;
                let rgba_k = rgba;
                let out_k = out;
                let fmt_k = fmt_sig;
                move |ev| {
                    let step = 1.0;
                    let mut h = hue_k.peek();
                    match &ev.logical_key {
                        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                            h -= step;
                        }
                        Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => {
                            h += step;
                        }
                        Key::Named(NamedKey::Home) => h = 0.0,
                        Key::Named(NamedKey::End) => h = 360.0,
                        _ => return false,
                    }
                    h = h.rem_euclid(360.0);
                    hue_k.set(h);
                    let cur = rgba_k.peek();
                    let (_, s, v) = rgb_to_hsv(cur.r, cur.g, cur.b);
                    let (r, g, b) = hsv_to_rgb(h, s.max(0.001), v.max(0.001));
                    rgba_k.set(Rgba { r, g, b, a: cur.a });
                    sync_string_from_rgba(out_k, rgba_k.peek(), fmt_k.peek());
                    true
                }
            })
            .into();
        hue_track = hue_track.cursor_kind(CursorKind::Pointer);
        let sync_h = hue_bounds;
        hue_track = hue_track.reactive_sync(move |root, _| {
            sync_h.set(root.layout_bounds);
            crate::core::tree::LayoutStatus::Clean
        });
    }

    let mut sliders = vstack()
        .gap("0.5rem")
        .w("100%")
        .child(area)
        .child(hue_track);

    if show_alpha {
        let mut alpha_track: Element = r#box().w("100%").h("0.75rem").rounded("9999px").into();
        alpha_track = alpha_track
            .bg([0.85, 0.85, 0.85, 1.0])
            .linear_gradient(
                [
                    c.r as f32 / 255.0,
                    c.g as f32 / 255.0,
                    c.b as f32 / 255.0,
                    0.0,
                ],
                [
                    c.r as f32 / 255.0,
                    c.g as f32 / 255.0,
                    c.b as f32 / 255.0,
                    1.0,
                ],
                0.0,
            )
            .relative()
            .overflow("hidden")
            .border(tokens::border(), 1.0);
        let mut a_knob: Element = r#box().w("0.75rem").h("0.75rem").rounded("9999px").into();
        a_knob = a_knob
            .absolute()
            .left(format!("calc({:.2}% - 6px)", c.a * 100.0))
            .top("-2px")
            .bg(c.to_f32())
            .border([1.0, 1.0, 1.0, 1.0], 2.0)
            .pointer_events("none");
        alpha_track = alpha_track.child(a_knob);
        let a_bounds = Rc::new(Cell::new([0.0f32; 4]));
        if !disabled {
            let b_a = a_bounds.clone();
            let drag = ColorDrag {
                kind: ColorDragKind::Alpha,
                bounds: a_bounds.clone(),
                rgba,
                hue,
                show_alpha,
            };
            let fid = format!("{id}:alpha");
            alpha_track = alpha_track.on_pointer_down({
                let drag = drag.clone();
                let fid = fid.clone();
                let out = out;
                let fmt_sig = fmt_sig;
                move |e| {
                    if e.phase == EventPhase::Capture {
                        return;
                    }
                    let b = if e.current_bounds[2] > 0.0 {
                        e.current_bounds
                    } else {
                        b_a.get()
                    };
                    b_a.set(b);
                    apply_color_pointer(&drag, e.x, e.y);
                    sync_string_from_rgba(out, drag.rgba.peek(), fmt_sig.peek());
                    begin_color_drag(drag.clone());
                    crate::app::set_pointer_capture(fid.clone());
                    e.stop_propagation();
                }
            });
            alpha_track = interactive(alpha_track)
                .focus_id(fid.as_str())
                .focusable(true)
                .focus_ring(true)
                .on_key_down({
                    let rgba_k = rgba;
                    let out_k = out;
                    let fmt_k = fmt_sig;
                    move |ev| {
                        let mut cur = rgba_k.peek();
                        let step = 0.02;
                        match &ev.logical_key {
                            Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                                cur.a = (cur.a - step).max(0.0);
                            }
                            Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => {
                                cur.a = (cur.a + step).min(1.0);
                            }
                            Key::Named(NamedKey::Home) => cur.a = 0.0,
                            Key::Named(NamedKey::End) => cur.a = 1.0,
                            _ => return false,
                        }
                        rgba_k.set(cur);
                        sync_string_from_rgba(out_k, cur, fmt_k.peek());
                        true
                    }
                })
                .into();
            alpha_track = alpha_track.cursor_kind(CursorKind::Pointer);
            let sync_a = a_bounds;
            alpha_track = alpha_track.reactive_sync(move |root, _| {
                sync_a.set(root.layout_bounds);
                crate::core::tree::LayoutStatus::Clean
            });
        }
        sliders = sliders.child(alpha_track);
    }

    let formats = if show_alpha {
        [ColorFormat::Hex8, ColorFormat::Rgba, ColorFormat::Hsla]
    } else {
        [ColorFormat::Hex, ColorFormat::Rgb, ColorFormat::Hsl]
    };
    let mut fmt_row: Element = hstack().gap("0.25rem").into();
    for f in formats {
        let fmt_s = fmt_sig;
        let out_s = out;
        let rgba_s = rgba;
        let chip: Element = button(format!("{id}:fmt{}", f.as_str()))
            .variant(if fmt == f { "default" } else { "outline" })
            .size("sm")
            .on_click(move || {
                fmt_s.set(f);
                sync_string_from_rgba(out_s, rgba_s.peek(), f);
            })
            .text(f.as_str().to_ascii_uppercase())
            .into();
        fmt_row = fmt_row.child(chip);
    }

    let hex_id = format!("{id}__hex");
    let hex_sig = use_signal(input_value_signal_key(&hex_id).as_str(), String::new);
    hex_sig.set(format_color(c, fmt));
    let mut hex_in: Element = input(&hex_id).placeholder("#RRGGBB").into();
    hex_in = hex_in.on_key_down({
        let hex_sig = hex_sig;
        let rgba_s = rgba;
        let hue_s = hue;
        let out_s = out;
        let fmt_s = fmt_sig;
        move |ev| {
            if matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
                if let Some(parsed) = parse_color_str(&hex_sig.peek()) {
                    rgba_s.set(parsed);
                    let (h, s, v) = rgb_to_hsv(parsed.r, parsed.g, parsed.b);
                    if s > 0.001 && v > 0.001 {
                        hue_s.set(h);
                    }
                    sync_string_from_rgba(out_s, parsed, fmt_s.peek());
                }
                return true;
            }
            false
        }
    });

    let mut swatches: Element = hstack().w("100%").gap("0.375rem").wrap("wrap").into();
    for (i, sw) in PRESET_SWATCHES.iter().enumerate() {
        let parsed = parse_color_str(sw).unwrap_or(Rgba::opaque(59, 130, 246));
        let mut cell: Element = r#box().w("1.5rem").h("1.5rem").rounded("0.25rem").into();
        cell = cell.bg(parsed.to_f32()).border(tokens::border(), 1.0);
        let rgba_s = rgba;
        let hue_s = hue;
        let out_s = out;
        let fmt_s = fmt_sig;
        let chip: Element = button(format!("{id}:sw{i}"))
            .variant("ghost")
            .size("sm")
            .on_click(move || {
                rgba_s.set(parsed);
                let (h, s, v) = rgb_to_hsv(parsed.r, parsed.g, parsed.b);
                if s > 0.001 && v > 0.001 {
                    hue_s.set(h);
                }
                sync_string_from_rgba(out_s, parsed, fmt_s.peek());
            })
            .child(cell)
            .into();
        swatches = swatches.child(chip);
    }

    let mut panel: Element = vstack()
        .gap("0.5rem")
        .p("0.75rem")
        .w("17.5rem")
        .child(sliders)
        .child(fmt_row)
        .child(hex_in)
        .child(swatches)
        .into();
    panel = panel
        .bg(tokens::popover())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.12], 8.0);
    panel
}

fn build_color_picker_view(
    props: ColorPickerRenderProps,
    open: Signal<bool>,
    out: Signal<String>,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
) -> Element {
    let ColorPickerRenderProps {
        id,
        variant,
        format: _,
        show_alpha,
        disabled,
    } = props;
    let _ = open.get();
    let c = rgba.get();
    let formatted = format_color(c, fmt_sig.get());
    if out.peek() != formatted {
        out.set(formatted.clone());
    }

    let mut swatch: Element = r#box().w("1.25rem").h("1.25rem").rounded("0.25rem").into();
    swatch = swatch.bg(c.to_f32()).border(tokens::border(), 1.0);
    let main = trigger_label(&formatted);

    if variant == ColorPickerVariant::Inline {
        let panel = build_panel(&id, rgba, hue, fmt_sig, show_alpha, disabled, out);
        return vstack()
            .gap("0.5rem")
            .w("100%")
            .child(
                hstack()
                    .gap("0.5rem")
                    .items_center()
                    .child(swatch)
                    .child(main),
            )
            .child(panel)
            .into();
    }

    let mut trigger = picker_trigger(&id, swatch, main, disabled);
    if !disabled {
        let open_t = open;
        let trig = trigger_focus_id(&id);
        trigger = trigger
            .on_click({
                let trig = trig.clone();
                move || {
                    if open_t.peek() {
                        close_picker(open_t, trig.clone());
                    } else {
                        open_t.set(true);
                    }
                }
            })
            .on_key_down({
                let open_k = open;
                let trig = trig.clone();
                move |ev| match &ev.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        open_k.set(false);
                        focus_by_id(trig.clone());
                        true
                    }
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        if open_k.peek() {
                            open_k.set(false);
                            focus_by_id(trig.clone());
                        } else {
                            open_k.set(true);
                        }
                        true
                    }
                    _ => false,
                }
            });
    }

    let mut view: Element = vstack().gap("0").child(trigger).into();
    if disabled {
        return view;
    }
    if let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) {
        if open.peek() {
            let mut panel = build_panel(&id, rgba, hue, fmt_sig, show_alpha, disabled, out);
            panel = panel.absolute();
            mark_floating(
                &mut panel,
                &id,
                &trigger_focus_id(&id),
                PopupPlacement::BottomStart,
            );
            panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
            panel.set_attr(ATTR_FLOATING_W, "17.5rem");
            panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
            apply_stored_floating_place(&mut panel, &id);
            panel = panel_block_clicks(panel, oz.panel);
            view = view.child(floating_portal_anchor(panel));
        }
    }
    view
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
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "color_picker");
        let initial = parse_color_str(&default_value).unwrap_or(Rgba::opaque(59, 130, 246));
        let out = value.unwrap_or_else(|| {
            let s = format_color(initial, format);
            use_signal(format!("{id}__cp_value").as_str(), move || s.clone())
        });
        let parsed0 = parse_color_str(&out.peek()).unwrap_or(initial);
        let rgba = use_signal(format!("{id}__cp_rgba").as_str(), || parsed0);
        let (h0, s0, v0) = rgb_to_hsv(parsed0.r, parsed0.g, parsed0.b);
        let hue0 = if s0 > 0.001 && v0 > 0.001 { h0 } else { 210.0 };
        let hue = use_signal(format!("{id}__cp_hue").as_str(), move || hue0);
        let fmt_sig = use_signal(format!("{id}__cp_fmt").as_str(), || format);
        let open = use_signal(format!("{id}__cp_open").as_str(), || false);
        let view_id = format!("{id}__cp_view");
        let props = ColorPickerRenderProps {
            id,
            variant,
            format,
            show_alpha,
            disabled,
        };
        reactive_component_live(view_id, props, move |p| {
            build_color_picker_view(p, open, out, rgba, hue, fmt_sig)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = parse_color_str("#3b82f6").unwrap();
        assert_eq!(c.r, 0x3b);
        assert_eq!(format_color(c, ColorFormat::Hex), "#3b82f6");
    }

    #[test]
    fn hsv_primary_blue() {
        let (h, s, v) = rgb_to_hsv(59, 130, 246);
        assert!(h > 200.0 && h < 230.0, "h={h}");
        assert!(s > 0.7 && v > 0.9);
        let (r, g, b) = hsv_to_rgb(h, s, v);
        assert!((r as i16 - 59).abs() < 3);
        assert!((g as i16 - 130).abs() < 3);
        assert!((b as i16 - 246).abs() < 3);
    }

    #[test]
    fn parse_rgb_and_hsl() {
        let c = parse_color_str("rgb(255, 0, 0)").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
        let c = parse_color_str("hsl(120, 100%, 50%)").unwrap();
        assert!(c.g > 200 && c.r < 20 && c.b < 20);
    }
}
