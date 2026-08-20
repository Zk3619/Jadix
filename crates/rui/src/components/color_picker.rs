//! # ColorPicker
//!
//! 对照 Reader `color-picker` + [solid-color](https://github.com/xbmlz/solid-color)
//! （HSV 面积 + 色相/透明度滑块 + 色板 + 格式）：对应 HTML `input type="color"`。
//!
//! **格式**：`hex` / `hex8` / `rgb` / `rgba` / `hsl` / `hsla`；面板内左侧
//! [`select`] 切类型、右侧 [`input`] 编辑色值，始终同一行。
//! **默认变体**：不再按是否支持透明度拆两套 UI（格式列表固定六项；色相下滑条含 alpha）。
//! **variant**：`default` 浮层；`inline` 展开；以及 `compact` / `sketch` / `chrome` /
//! `github` / `twitter` / `material` / `block` / `circle` / `slider`。
//! **键盘**：触发器 Enter/Space 开合、Esc 关闭；面积 ←→ 饱和度 ↑↓ 明度；
//! 色相/透明度滑块 ←→ 步进。
//!
//! ## 用法
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
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{bind_style, bind_text, reactive_component_live, StylePatch};
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::components::select::{select, SelectItemData};
use crate::components::separator::separator;
use crate::components::slider::slider;
use crate::core::reactivity::{create_effect, use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, CursorKind, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    apply_stored_floating_place, mark_floating, mount_anchored_overlay, PopupPlacement,
    ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W,
};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use taffy::prelude::LengthPercentageAuto;
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

const FORMAT_OPTIONS: &[(ColorFormat, &str)] = &[
    (ColorFormat::Hex, "HEX"),
    (ColorFormat::Hex8, "HEX8"),
    (ColorFormat::Rgb, "RGB"),
    (ColorFormat::Rgba, "RGBA"),
    (ColorFormat::Hsl, "HSL"),
    (ColorFormat::Hsla, "HSLA"),
];

const SWATCHES_DEFAULT: &[&str] = PRESET_SWATCHES;
const SWATCHES_TWITTER: &[&str] = &[
    "#FF6900", "#FCB900", "#7BDCB5", "#00D084", "#8ED1FC", "#0693E3", "#ABB8C3", "#EB144C",
    "#F78DA7", "#9900EF",
];
const SWATCHES_GITHUB: &[&str] = &[
    "#B80000", "#DB3E00", "#FCCB00", "#008B02", "#006B76", "#1273DE", "#004DCF", "#5300EB",
    "#EB9694", "#FAD0C3", "#FEF3BD", "#C1E1C5", "#BEDADC", "#C4DEF6", "#BED3F3", "#D4C4FB",
];
const SWATCHES_MATERIAL: &[&str] = &[
    "#F44336", "#E91E63", "#9C27B0", "#673AB7", "#3F51B5", "#2196F3", "#03A9F4", "#00BCD4",
    "#009688", "#4CAF50", "#8BC34A", "#CDDC39", "#FFEB3B", "#FFC107", "#FF9800", "#FF5722",
    "#795548", "#9E9E9E", "#607D8B", "#000000",
];
const SWATCHES_BLOCK: &[&str] = &[
    "#D9E3F0", "#F47373", "#697689", "#37D67A", "#2CCCE4", "#555555", "#DCE775", "#FF8A65",
    "#BA68C8",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPickerVariant {
    #[default]
    Default,
    Inline,
    Compact,
    Sketch,
    Chrome,
    Github,
    Twitter,
    Material,
    Block,
    Circle,
    Slider,
}

impl ColorPickerVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline" => Self::Inline,
            "compact" => Self::Compact,
            "sketch" => Self::Sketch,
            "chrome" => Self::Chrome,
            "github" => Self::Github,
            "twitter" => Self::Twitter,
            "material" => Self::Material,
            "block" => Self::Block,
            "circle" => Self::Circle,
            "slider" => Self::Slider,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Inline => "inline",
            Self::Compact => "compact",
            Self::Sketch => "sketch",
            Self::Chrome => "chrome",
            Self::Github => "github",
            Self::Twitter => "twitter",
            Self::Material => "material",
            Self::Block => "block",
            Self::Circle => "circle",
            Self::Slider => "slider",
        }
    }

    fn panel_width(self) -> &'static str {
        match self {
            Self::Compact | Self::Twitter => "15.75rem",
            Self::Sketch => "18.75rem",
            Self::Github => "16.75rem",
            Self::Block => "11.5rem",
            Self::Circle => "15rem",
            _ => "17.5rem",
        }
    }

    fn area_height(self) -> &'static str {
        match self {
            Self::Compact | Self::Github | Self::Material => "6rem",
            _ => "9rem",
        }
    }

    fn close_on_swatch(self) -> bool {
        matches!(self, Self::Twitter | Self::Block | Self::Circle)
    }

    fn circle_swatches(self) -> bool {
        matches!(self, Self::Github | Self::Circle)
    }

    fn swatches(self) -> &'static [&'static str] {
        match self {
            Self::Twitter => SWATCHES_TWITTER,
            Self::Github => SWATCHES_GITHUB,
            Self::Material => SWATCHES_MATERIAL,
            Self::Block | Self::Circle => SWATCHES_BLOCK,
            Self::Chrome => SWATCHES_TWITTER,
            _ => SWATCHES_DEFAULT,
        }
    }

    fn swatch_columns(self) -> u16 {
        match self {
            Self::Twitter | Self::Chrome => 5,
            Self::Github => 8,
            Self::Material => 5,
            Self::Block => 3,
            Self::Circle => 5,
            _ => 6,
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

fn sv_texture_key(id: &str, hue: f32) -> String {
    format!("rui_cp_sv:{id}:{:.0}", hue.rem_euclid(360.0))
}

fn hue_texture_key() -> &'static str {
    "rui_cp_hue_ramp"
}

thread_local! {
    static TEX_KEYS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

fn tex_cached(key: String, build: impl FnOnce() -> (u32, u32, Vec<u8>)) -> String {
    let hit = TEX_KEYS.with(|s| s.borrow().contains(&key));
    if !hit {
        let (w, h, rgba) = build();
        TEX_KEYS.with(|s| {
            s.borrow_mut().insert(key.clone());
        });
        crate::queue_image_rgba8(key.clone(), w, h, rgba);
    }
    key
}

fn apply_rounded_rect_alpha(rgba: &mut [u8], w: u32, h: u32, radius: f32) {
    let wf = w as f32;
    let hf = h as f32;
    let r = radius.min(wf * 0.5).min(hf * 0.5).max(0.0);
    if r < 0.5 {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let qx = (px - wf * 0.5).abs() - wf * 0.5 + r;
            let qy = (py - hf * 0.5).abs() - hf * 0.5 + r;
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            let sdf = outside + inside - r;
            let a = (0.75 - sdf).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
}

fn ensure_sv_texture(id: &str, hue: f32) -> String {
    let hue = hue.rem_euclid(360.0);
    tex_cached(sv_texture_key(id, hue), move || {
        let w = 192u32;
        let h = 144u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            // Textured-quad UV maps widget top → texture v=1 (last row). Row 0 is the
            // widget bottom, so value 0 (black) lives here to match pointer `1 - ty`.
            let v = y as f32 / (h - 1).max(1) as f32;
            for x in 0..w {
                let s = x as f32 / (w - 1).max(1) as f32;
                let (r, g, b) = hsv_to_rgb(hue, s, v);
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * (0.375 / 9.0));
        (w, h, rgba)
    })
}

fn ensure_hue_texture() {
    tex_cached(hue_texture_key().to_string(), || {
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
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    });
}

fn alpha_texture_key(c: Rgba) -> String {
    format!("rui_cp_alpha:{}:{}:{}", c.r, c.g, c.b)
}

fn ensure_alpha_texture(c: Rgba) -> String {
    tex_cached(alpha_texture_key(c), move || {
        let w = 180u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let cell = 4u32;
        for y in 0..h {
            for x in 0..w {
                let a = x as f32 / (w - 1).max(1) as f32;
                let checker = if ((x / cell) + (y / cell)) % 2 == 0 {
                    220.0
                } else {
                    160.0
                };
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = (checker * (1.0 - a) + c.r as f32 * a).round() as u8;
                rgba[i + 1] = (checker * (1.0 - a) + c.g as f32 * a).round() as u8;
                rgba[i + 2] = (checker * (1.0 - a) + c.b as f32 * a).round() as u8;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    })
}

#[derive(Clone, Copy)]
enum ColorDragKind {
    Sv,
}

#[derive(Clone)]
struct ColorDrag {
    kind: ColorDragKind,
    bounds: Rc<Cell<[f32; 4]>>,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    show_alpha: bool,
    out: Signal<String>,
    fmt_sig: Signal<ColorFormat>,
    sv_id: Option<String>,
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
        sync_string_from_rgba(drag.out, drag.rgba.peek(), drag.fmt_sig.peek());
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
    let ColorDragKind::Sv = drag.kind;
    let sat = tx;
    let val = 1.0 - ty;
    drag.sat.set(sat);
    drag.val.set(val);
    let (r, g, bch) = hsv_to_rgb(drag.hue.peek(), sat, val);
    let a = drag.rgba.peek().a;
    drag.rgba.set(Rgba {
        r,
        g,
        b: bch,
        a: if drag.show_alpha { a } else { 1.0 },
    });
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
    show_alpha: Option<bool>,
    disabled: bool,
    children: Option<Element>,
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
        show_alpha: true,
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

const ATTR_CP_SLOT: &str = "data-slot";
const SLOT_SV_TEX: &str = "cp-sv-tex";
const SLOT_SV_KNOB: &str = "cp-sv-knob";

fn find_tid_mut<'a>(el: &'a mut Element, tid: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    for i in 0..el.children.len() {
        if contains_tid(&el.children[i], tid) {
            if el.children[i].template_id.as_deref() == Some(tid) {
                return Some(&mut el.children[i]);
            }
            return find_tid_mut(&mut el.children[i], tid);
        }
    }
    None
}

fn contains_tid(el: &Element, tid: &str) -> bool {
    el.template_id.as_deref() == Some(tid) || el.children.iter().any(|c| contains_tid(c, tid))
}

fn sync_trigger_preview(
    root: &mut Element,
    id: &str,
    c: Rgba,
    fmt: ColorFormat,
) -> crate::core::tree::LayoutStatus {
    let mut changed = crate::core::tree::LayoutStatus::Clean;
    let want_bg = c.to_f32();
    let swatch_id = format!("{id}__cp_trig_swatch");
    if let Some(sw) = find_tid_mut(root, &swatch_id) {
        if sw.style.bg_color != want_bg {
            sw.style.bg_color = want_bg;
            changed = crate::core::tree::LayoutStatus::StyleChanged;
        }
    }
    let label_id = format!("{id}__cp_trig_label");
    let want = format_color(c, fmt);
    if let Some(label) = find_tid_mut(root, &label_id) {
        if label.text.as_deref() != Some(want.as_str()) {
            label.text = Some(want);
            changed = crate::core::tree::LayoutStatus::StyleChanged;
        }
    }
    changed
}

fn with_trigger_preview_sync(
    view: Element,
    id: String,
    rgba: Signal<Rgba>,
    fmt_sig: Signal<ColorFormat>,
) -> Element {
    view.reactive_sync(move |root, _| sync_trigger_preview(root, &id, rgba.peek(), fmt_sig.peek()))
}

fn follow_hue_into_rgba(
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    out: Signal<String>,
    fmt_sig: Signal<ColorFormat>,
) {
    create_effect(move || {
        let hue_v = hue.get();
        let s = sat.peek();
        let v = val.peek();
        let c = rgba.peek();
        let (r, g, b) = hsv_to_rgb(hue_v.rem_euclid(360.0), s, v);
        if r == c.r && g == c.g && b == c.b {
            return;
        }
        let next = Rgba { r, g, b, a: c.a };
        rgba.set(next);
        sync_string_from_rgba(out, next, fmt_sig.peek());
    });
}

fn has_cp_slot(el: &Element, slot: &str) -> bool {
    el.attrs.get(ATTR_CP_SLOT).map(|s| s.as_str()) == Some(slot)
        || el.children.iter().any(|c| has_cp_slot(c, slot))
}

fn find_cp_slot_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    if el.attrs.get(ATTR_CP_SLOT).map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    for i in 0..el.children.len() {
        if has_cp_slot(&el.children[i], slot) {
            if el.children[i].attrs.get(ATTR_CP_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(&mut el.children[i]);
            }
            return find_cp_slot_mut(&mut el.children[i], slot);
        }
    }
    None
}

/// Half of the 0.75rem (12px) SV knob — paint offset so the *center* sits on (S, V).
const SV_KNOB_HALF_PX: f32 = 6.0;

fn sv_knob_frac(sat: f32, val: f32) -> (f32, f32) {
    (sat.clamp(0.0, 1.0), (1.0 - val).clamp(0.0, 1.0))
}

fn inset_percent_mismatch(cur: LengthPercentageAuto, want: f32) -> bool {
    match crate::layout::lpa_kind(cur) {
        crate::layout::DimKind::Percent(p) => (p - want).abs() > 0.0005,
        _ => true,
    }
}

/// Place the knob with Taffy `%` insets so the first layout (before bounds exist) is
/// already on the current S/V. Pixel `translate` only centers the 12px handle.
fn place_sv_knob(
    knob: &mut Element,
    layout_tree: Option<&mut LayoutTree>,
    c: Rgba,
    sat: f32,
    val: f32,
) -> LayoutStatus {
    let (left, top) = sv_knob_frac(sat, val);
    let want_t = [-SV_KNOB_HALF_PX, -SV_KNOB_HALF_PX];
    let mut status = LayoutStatus::Clean;
    if inset_percent_mismatch(knob.style.inset.left, left)
        || inset_percent_mismatch(knob.style.inset.top, top)
    {
        knob.style.inset.left = LengthPercentageAuto::percent(left);
        knob.style.inset.top = LengthPercentageAuto::percent(top);
        if let Some(lt) = layout_tree {
            if knob.layout_node.is_some() {
                lt.update_taffy_node_style(knob);
            }
        }
        status = LayoutStatus::LayoutChanged;
    }
    if knob.style.translate != want_t {
        knob.style.translate = want_t;
        status = status.merge(LayoutStatus::StyleChanged);
    }
    if knob.style.bg_color != c.to_f32() {
        knob.style.bg_color = c.to_f32();
        status = status.merge(LayoutStatus::StyleChanged);
    }
    status
}

fn sync_sv_visuals(
    root: &mut Element,
    layout_tree: &mut LayoutTree,
    id: &str,
    c: Rgba,
    hue: f32,
    sat: f32,
    val: f32,
) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    let key = ensure_sv_texture(id, hue);
    if let Some(tex) = find_cp_slot_mut(root, SLOT_SV_TEX) {
        if tex.image_texture.as_deref() != Some(key.as_str()) {
            tex.image_texture = Some(key);
            status = LayoutStatus::StyleChanged;
        }
    }
    if let Some(knob) = find_cp_slot_mut(root, SLOT_SV_KNOB) {
        status = status.merge(place_sv_knob(knob, Some(layout_tree), c, sat, val));
    }
    status
}

fn apply_sv_from_rgb(parsed: Rgba, hue: Signal<f32>, sat: Signal<f32>, val: Signal<f32>) {
    let (h, s, v) = rgb_to_hsv(parsed.r, parsed.g, parsed.b);
    if s > 0.001 && v > 0.001 {
        hue.set(h);
    }
    sat.set(s);
    val.set(v);
}

fn apply_parsed(
    parsed: Rgba,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    out: Signal<String>,
    fmt: ColorFormat,
) {
    rgba.set(parsed);
    apply_sv_from_rgb(parsed, hue, sat, val);
    sync_string_from_rgba(out, parsed, fmt);
}

fn wire_pointer_drag(mut el: Element, fid: String, drag: ColorDrag) -> Element {
    let b_ref = drag.bounds.clone();
    el = el.on_pointer_down({
        let drag = drag.clone();
        let fid = fid.clone();
        move |e| {
            if e.phase == EventPhase::Capture {
                return;
            }
            let b = if e.current_bounds[2] > 0.0 {
                e.current_bounds
            } else {
                b_ref.get()
            };
            b_ref.set(b);
            apply_color_pointer(&drag, e.x, e.y);
            sync_string_from_rgba(drag.out, drag.rgba.peek(), drag.fmt_sig.peek());
            begin_color_drag(drag.clone());
            crate::app::set_pointer_capture(fid.clone());
            e.stop_propagation();
        }
    });
    el = el.cursor_kind(CursorKind::Pointer);
    el.reactive_sync(move |root, layout_tree| {
        // Do not overwrite drag bounds with `layout_bounds`: those are unscrolled
        // abs coords, while pointer `x/y` and `current_bounds` are window-space
        // (scroll already subtracted). Inline pickers inside a ScrollArea would
        // map every move to ty=0 → V=1 (knob glued to the top).
        if let Some(id) = drag.sv_id.as_deref() {
            sync_sv_visuals(
                root,
                layout_tree,
                id,
                drag.rgba.peek(),
                drag.hue.peek(),
                drag.sat.peek(),
                drag.val.peek(),
            )
        } else {
            LayoutStatus::Clean
        }
    })
}

fn sv_area(
    id: &str,
    height: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let c = rgba.peek();
    let hue_v = hue.peek();
    let sv_key = ensure_sv_texture(id, hue_v);

    let mut tex: Element = r#box().w("100%").h("100%").rounded("0.375rem").into();
    // Opaque fill under the ramp: rounded-texture alpha is 0 in the corners, otherwise
    // the page (not the popover) shows through those pixels.
    tex = tex
        .bg(tokens::popover())
        .overflow("hidden")
        .pointer_events("none");
    tex.image_texture = Some(sv_key);
    tex.attrs
        .insert(ATTR_CP_SLOT.to_string(), SLOT_SV_TEX.to_string());

    let mut knob: Element = r#box().w("0.75rem").h("0.75rem").rounded("9999px").into();
    knob = knob
        .absolute()
        .bg(c.to_f32())
        .border([1.0, 1.0, 1.0, 1.0], 2.0)
        .pointer_events("none");
    let _ = place_sv_knob(&mut knob, None, c, sat.peek(), val.peek());
    knob.attrs
        .insert(ATTR_CP_SLOT.to_string(), SLOT_SV_KNOB.to_string());

    let mut area: Element = r#box()
        .w("100%")
        .h(height)
        .rounded("0.375rem")
        .relative(true)
        .bg(tokens::popover())
        .border(tokens::border(), 1.0)
        .child(tex)
        .child(knob)
        .into();
    if disabled {
        return area;
    }
    let drag = ColorDrag {
        kind: ColorDragKind::Sv,
        bounds: Rc::new(Cell::new([0.0f32; 4])),
        rgba,
        hue,
        sat,
        val,
        show_alpha,
        out,
        fmt_sig,
        sv_id: Some(id.to_string()),
    };
    let fid = format!("{id}:sv");
    area = wire_pointer_drag(area, fid.clone(), drag);
    interactive(area)
        .focus_id(fid.as_str())
        .focusable(true)
        .focus_ring(true)
        .on_key_down({
            let rgba_k = rgba;
            let hue_k = hue;
            let sat_k = sat;
            let val_k = val;
            let out_k = out;
            let fmt_k = fmt_sig;
            move |ev| {
                let cur = rgba_k.peek();
                let mut s = sat_k.peek();
                let mut v = val_k.peek();
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
                sat_k.set(s);
                val_k.set(v);
                let (r, g, b) = hsv_to_rgb(hue_k.peek(), s, v);
                rgba_k.set(Rgba { r, g, b, a: cur.a });
                sync_string_from_rgba(out_k, rgba_k.peek(), fmt_k.peek());
                true
            }
        })
        .into()
}

fn hue_slider(
    id: &str,
    _rgba: Signal<Rgba>,
    hue: Signal<f32>,
    _fmt_sig: Signal<ColorFormat>,
    _out: Signal<String>,
    disabled: bool,
) -> Element {
    ensure_hue_texture();
    slider(format!("{id}:hue"))
        .min(0.0)
        .max(360.0)
        .step(1.0)
        .value_signal(hue)
        .track_texture(hue_texture_key())
        .disabled(disabled)
        .thumb_fill({
            let hue = hue;
            move || {
                let (r, g, b) = hsv_to_rgb(hue.peek().rem_euclid(360.0), 1.0, 1.0);
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
            }
        })
        .into()
}

fn alpha_slider(
    id: &str,
    rgba: Signal<Rgba>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let alpha = use_signal(format!("{id}__cp_alpha").as_str(), || {
        rgba.peek().a.clamp(0.0, 1.0)
    });
    create_effect({
        let rgba = rgba;
        let out = out;
        let fmt_sig = fmt_sig;
        let alpha = alpha;
        move || {
            let a = alpha.get();
            let c = rgba.peek();
            let want = if show_alpha { a.clamp(0.0, 1.0) } else { 1.0 };
            if (c.a - want).abs() <= 0.001 {
                return;
            }
            let mut next = c;
            next.a = want;
            rgba.set(next);
            sync_string_from_rgba(out, next, fmt_sig.peek());
        }
    });
    slider(format!("{id}:alpha"))
        .min(0.0)
        .max(1.0)
        .step(0.01)
        .value_signal(alpha)
        .track_texture(ensure_alpha_texture(rgba.peek()))
        .track_texture_fn({
            let rgba = rgba;
            move || ensure_alpha_texture(rgba.peek())
        })
        .disabled(disabled)
        .thumb_fill({
            let rgba = rgba;
            move || rgba.peek().to_f32()
        })
        .on_sync({
            let alpha = alpha;
            move || {
                let c = rgba.peek();
                let want = if show_alpha { c.a.clamp(0.0, 1.0) } else { 1.0 };
                if (want - alpha.peek()).abs() > 0.001 {
                    alpha.set_silent(want);
                }
            }
        })
        .into()
}

fn hsv_sliders(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let alpha_on = show_alpha && fmt_sig.peek().supports_alpha();
    let mut sliders = vstack()
        .gap("0.5rem")
        .w("100%")
        .child(sv_area(
            id,
            variant.area_height(),
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(hue_slider(id, rgba, hue, fmt_sig, out, disabled));
    if alpha_on {
        sliders = sliders.child(alpha_slider(id, rgba, fmt_sig, out, show_alpha, disabled));
    }
    sliders.into()
}

fn ensure_linear_ramp(key: String, from: [f32; 4], to: [f32; 4]) -> String {
    tex_cached(key, move || {
        let w = 180u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for x in 0..w {
            let t = x as f32 / (w - 1).max(1) as f32;
            let r = ((from[0] + (to[0] - from[0]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let g = ((from[1] + (to[1] - from[1]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let b = ((from[2] + (to[2] - from[2]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            for y in 0..h {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    })
}

fn channel_byte(c: Rgba, part: &str) -> u8 {
    match part {
        "r" => c.r,
        "g" => c.g,
        _ => c.b,
    }
}

fn write_channel_byte(mut c: Rgba, part: &str, v: u8) -> Rgba {
    match part {
        "r" => c.r = v,
        "g" => c.g = v,
        _ => c.b = v,
    }
    c
}

fn channel_track(
    id: &str,
    part: &str,
    from: [f32; 4],
    to: [f32; 4],
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let ch = use_signal(format!("{id}__cp_ch_{part}").as_str(), || {
        channel_byte(rgba.peek(), part) as f32
    });
    let last = use_signal(format!("{id}__cp_ch_{part}_last").as_str(), || ch.peek());
    let v = ch.get();
    let c = rgba.get();
    if (v - last.peek()).abs() > 0.5 {
        last.set(v);
        let mut next = write_channel_byte(c, part, v.round().clamp(0.0, 255.0) as u8);
        if !show_alpha {
            next.a = 1.0;
        }
        rgba.set(next);
        apply_sv_from_rgb(next, hue, sat, val);
        sync_string_from_rgba(out, next, fmt_sig.peek());
    } else {
        let want = channel_byte(c, part) as f32;
        if (want - ch.peek()).abs() > 0.5 {
            ch.set(want);
            last.set(want);
        }
    }
    let ramp_key = ensure_linear_ramp(format!("rui_cp_ramp:{id}:{part}"), from, to);
    slider(format!("{id}:{part}"))
        .min(0.0)
        .max(255.0)
        .step(1.0)
        .value_signal(ch)
        .track_texture(ramp_key)
        .disabled(disabled)
        .into()
}

fn rgb_sliders(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let c = rgba.peek();
    let rf = c.r as f32 / 255.0;
    let gf = c.g as f32 / 255.0;
    let bf = c.b as f32 / 255.0;
    let mut row = vstack()
        .gap("0.5rem")
        .w("100%")
        .child(channel_track(
            id,
            "r",
            [0.0, gf, bf, 1.0],
            [1.0, gf, bf, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(channel_track(
            id,
            "g",
            [rf, 0.0, bf, 1.0],
            [rf, 1.0, bf, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(channel_track(
            id,
            "b",
            [rf, gf, 0.0, 1.0],
            [rf, gf, 1.0, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ));
    if show_alpha && fmt_sig.peek().supports_alpha() {
        row = row.child(alpha_slider(id, rgba, fmt_sig, out, show_alpha, disabled));
    }
    row.into()
}

fn format_value_row(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
    out: Signal<String>,
    disabled: bool,
) -> Element {
    let selected = ColorFormat::parse(&fmt_str.get());
    if selected != fmt_sig.peek() {
        fmt_sig.set(selected);
        sync_string_from_rgba(out, rgba.peek(), selected);
    }
    let fmt = fmt_sig.peek();

    let mut sel = select(format!("{id}:fmt"))
        .value_signal(fmt_str)
        .placeholder("HEX")
        .match_trigger(false)
        .trigger_width("5.75rem")
        .menu_width("6.5rem")
        .max_height("16rem")
        .scroll_selected("center");
    for (value, label) in FORMAT_OPTIONS {
        sel = sel.item(SelectItemData::new(value.as_str(), *label));
    }
    let sel: Element = sel.into();
    // Input chrome is h-9 (2.25rem); SelectTrigger defaults to h-8 (2rem).
    let sel = sel
        .w("5.75rem")
        .h("2.25rem")
        .min_h("2.25rem")
        .flex_shrink(0.0);

    let hex_id = format!("{id}__hex");
    let mut hex_in: Element = input(&hex_id)
        .placeholder(match fmt {
            ColorFormat::Hex => "#RRGGBB",
            ColorFormat::Hex8 => "#RRGGBBAA",
            ColorFormat::Rgb => "rgb()",
            ColorFormat::Rgba => "rgba()",
            ColorFormat::Hsl => "hsl()",
            ColorFormat::Hsla => "hsla()",
        })
        .disabled(disabled)
        .flex_grow(1.0)
        .value_signal(out)
        .into();
    hex_in = hex_in.on_key_down({
        let out_s = out;
        let rgba_s = rgba;
        let hue_s = hue;
        let sat_s = sat;
        let val_s = val;
        let fmt_s = fmt_sig;
        move |ev| {
            if matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
                if let Some(parsed) = parse_color_str(&out_s.peek()) {
                    apply_parsed(parsed, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                }
                return true;
            }
            false
        }
    });

    hstack()
        .gap("0.5rem")
        .w("100%")
        .items_center()
        .min_w("0")
        .child(sel)
        .child(hex_in)
        .into()
}

fn channel_fields(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    disabled: bool,
) -> Element {
    let c = rgba.get();
    let fields = [
        ("r", "R", c.r.to_string()),
        ("g", "G", c.g.to_string()),
        ("b", "B", c.b.to_string()),
        ("a", "A", format!("{:.0}", c.a * 100.0)),
    ];
    let mut row = hstack().gap("0.375rem").w("100%");
    for (key, label, text) in fields {
        let fid = format!("{id}__ch_{key}");
        let sig = use_signal(input_value_signal_key(&fid).as_str(), String::new);
        sig.set(text);
        let mut field: Element = input(&fid)
            .placeholder(label)
            .disabled(disabled)
            .flex_grow(1.0)
            .h("2rem")
            .into();
        field = field.on_key_down({
            let sig = sig;
            let rgba_s = rgba;
            let hue_s = hue;
            let sat_s = sat;
            let val_s = val;
            let out_s = out;
            let fmt_s = fmt_sig;
            let key = key.to_string();
            move |ev| {
                if !matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
                    return false;
                }
                let n: f32 = sig.peek().trim().parse().unwrap_or(f32::NAN);
                if !n.is_finite() {
                    return true;
                }
                let mut cur = rgba_s.peek();
                match key.as_str() {
                    "r" => cur.r = n.round().clamp(0.0, 255.0) as u8,
                    "g" => cur.g = n.round().clamp(0.0, 255.0) as u8,
                    "b" => cur.b = n.round().clamp(0.0, 255.0) as u8,
                    "a" => cur.a = (n / 100.0).clamp(0.0, 1.0),
                    _ => {}
                }
                apply_parsed(cur, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                true
            }
        });
        let col = vstack()
            .gap("0.125rem")
            .flex_grow(1.0)
            .min_w("0")
            .child(trigger_label(label))
            .child(field);
        row = row.child(col);
    }
    row.into()
}

fn preview_bar(rgba: Signal<Rgba>, fmt_sig: Signal<ColorFormat>) -> Element {
    let c = rgba.get();
    let mut bar: Element = r#box().w("100%").h("3.25rem").rounded("0.5rem").into();
    bar = bar.bg(c.to_f32()).border(tokens::border(), 1.0).relative();
    bar = bar.child(
        hstack()
            .w("100%")
            .h("100%")
            .items_center()
            .justify_center()
            .pointer_events("none")
            .child(trigger_label(&format_color(c, fmt_sig.get()))),
    );
    bar
}

fn swatch_grid(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    open: Signal<bool>,
    round: &str,
) -> Element {
    let cols = variant.swatch_columns();
    let mut swatches: Element = r#box()
        .w("100%")
        .grid_cols_equal(cols)
        .gap("0.375rem")
        .into();
    let circle = variant.circle_swatches();
    let radius = if circle { "9999px" } else { round };
    for (i, sw) in variant.swatches().iter().enumerate() {
        let parsed = parse_color_str(sw).unwrap_or(Rgba::opaque(59, 130, 246));
        let mut chip: Element = r#box().w("100%").rounded(radius).into();
        chip = chip
            .aspect_ratio(1.0)
            .bg(parsed.to_f32())
            .border(tokens::border(), 1.0);
        let rgba_s = rgba;
        let hue_s = hue;
        let sat_s = sat;
        let val_s = val;
        let out_s = out;
        let fmt_s = fmt_sig;
        let open_s = open;
        let close = variant.close_on_swatch();
        let mut btn: Element = button(format!("{id}:sw{i}"))
            .variant("ghost")
            .size("sm")
            .p("0")
            .w("100%")
            .h("auto")
            .on_click(move || {
                apply_parsed(parsed, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                if close {
                    open_s.set(false);
                }
            })
            .child(chip)
            .into();
        btn = btn.min_h(0.0);
        swatches = swatches.child(btn);
    }
    swatches
}

fn panel_shell(width: &str, body: Element) -> Element {
    let mut panel: Element = vstack()
        .gap("0.5rem")
        .p("0.75rem")
        .w(width)
        .child(body)
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

fn build_panel(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
    show_alpha: bool,
    disabled: bool,
    out: Signal<String>,
    open: Signal<bool>,
) -> Element {
    let width = variant.panel_width();
    let alpha_on = show_alpha && fmt_sig.peek().supports_alpha();
    let sliders = || {
        hsv_sliders(
            id, variant, rgba, hue, sat, val, fmt_sig, out, alpha_on, disabled,
        )
    };
    let controls = || format_value_row(id, rgba, hue, sat, val, fmt_sig, fmt_str, out, disabled);
    let swatches =
        |round: &str| swatch_grid(id, variant, rgba, hue, sat, val, fmt_sig, out, open, round);

    let body: Element = match variant {
        ColorPickerVariant::Twitter => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(controls())
            .into(),
        ColorPickerVariant::Block => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(controls())
            .into(),
        ColorPickerVariant::Circle => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("9999px"))
            .into(),
        ColorPickerVariant::Slider => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(rgb_sliders(
                id, rgba, hue, sat, val, fmt_sig, out, show_alpha, disabled,
            ))
            .child(controls())
            .into(),
        ColorPickerVariant::Sketch => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(preview_bar(rgba, fmt_sig))
            .child(sliders())
            .child(channel_fields(
                id, rgba, hue, sat, val, fmt_sig, out, disabled,
            ))
            .child(controls())
            .into(),
        ColorPickerVariant::Github => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("9999px"))
            .child(Element::from(separator()))
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Material => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(Element::from(separator()))
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Chrome => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .child(trigger_label("Chrome 预设"))
            .child(swatches("0.25rem"))
            .into(),
        ColorPickerVariant::Compact => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Default | ColorPickerVariant::Inline => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .child(trigger_label("预设"))
            .child(swatches("0.25rem"))
            .into(),
    };
    panel_shell(width, body)
}

fn build_color_picker_view(
    props: ColorPickerRenderProps,
    open: Signal<bool>,
    out: Signal<String>,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
) -> Element {
    let ColorPickerRenderProps {
        id,
        variant,
        format: _,
        show_alpha,
        disabled,
    } = props;
    let _ = open.get();
    let selected = ColorFormat::parse(&fmt_str.get());
    if selected != fmt_sig.peek() {
        fmt_sig.set(selected);
        sync_string_from_rgba(out, rgba.peek(), selected);
    }

    let mut swatch: Element = r#box().w("1.25rem").h("1.25rem").rounded("0.25rem").into();
    swatch = swatch.border(tokens::border(), 1.0);
    let swatch_id = format!("{id}__cp_trig_swatch");
    swatch = bind_style(swatch, swatch_id, {
        let rgba = rgba;
        move || StylePatch {
            bg: Some(rgba.get().to_f32()),
            ..Default::default()
        }
    });
    let main = bind_text(format!("{id}__cp_trig_label"), {
        let rgba = rgba;
        let fmt_sig = fmt_sig;
        move || format_color(rgba.get(), fmt_sig.peek())
    })
    .text_size(0.875 * 16.0)
    .text_color(parse_text_color("foreground"))
    .min_w(0.0)
    .flex_grow(1.0)
    .whitespace_nowrap()
    .text_overflow_ellipsis();

    if variant == ColorPickerVariant::Inline {
        let panel = build_panel(
            &id, variant, rgba, hue, sat, val, fmt_sig, fmt_str, show_alpha, disabled, out, open,
        );
        let view: Element = vstack()
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
        return with_trigger_preview_sync(view, id, rgba, fmt_sig);
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
        return with_trigger_preview_sync(view, id, rgba, fmt_sig);
    }
    if let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) {
        if open.peek() {
            let mut panel = build_panel(
                &id, variant, rgba, hue, sat, val, fmt_sig, fmt_str, show_alpha, disabled, out,
                open,
            );
            panel = panel.absolute();
            mark_floating(
                &mut panel,
                &id,
                &trigger_focus_id(&id),
                PopupPlacement::BottomStart,
            );
            panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
            panel.set_attr(ATTR_FLOATING_W, variant.panel_width());
            panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
            apply_stored_floating_place(&mut panel, &id);
            panel = mount_anchored_overlay(panel, oz.panel);
            view = view.child(panel);
        }
    }
    with_trigger_preview_sync(view, id, rgba, fmt_sig)
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
        let sat = use_signal(format!("{id}__cp_sat").as_str(), move || s0);
        let val = use_signal(format!("{id}__cp_val").as_str(), move || v0);
        let fmt_sig = use_signal(format!("{id}__cp_fmt").as_str(), || format);
        let fmt0 = format.as_str().to_string();
        let fmt_str = use_signal(format!("{id}__cp_fmt_str").as_str(), move || fmt0.clone());
        let open = use_signal(format!("{id}__cp_open").as_str(), || false);
        follow_hue_into_rgba(rgba, hue, sat, val, out, fmt_sig);
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
        reactive_component_live(view_id, props, move |p| {
            build_color_picker_view(p, open, out, rgba, hue, sat, val, fmt_sig, fmt_str)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_color_picker_defers_sv_texture_until_panel() {
        use crate::core::reactivity::create_root;
        let before = TEX_KEYS.with(|s| {
            s.borrow()
                .iter()
                .filter(|k| k.starts_with("rui_cp_sv:"))
                .count()
        });
        let (_h, _el) = create_root(|| {
            let _: Element = color_picker("defer-sv-closed").into();
        });
        let after = TEX_KEYS.with(|s| {
            s.borrow()
                .iter()
                .filter(|k| k.starts_with("rui_cp_sv:"))
                .count()
        });
        assert_eq!(
            before, after,
            "closed ColorPicker must not generate SV texture at mount"
        );
    }

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

    #[test]
    fn hex8_roundtrip() {
        let c = parse_color_str("#3b82f680").unwrap();
        assert!((c.a - 0.5).abs() < 0.01);
        let s = format_color(c, ColorFormat::Hex8);
        assert!(s.starts_with("#3b82f6"));
    }

    #[test]
    fn variant_parse_solid_color_family() {
        assert_eq!(
            ColorPickerVariant::parse("sketch"),
            ColorPickerVariant::Sketch
        );
        assert_eq!(
            ColorPickerVariant::parse("twitter"),
            ColorPickerVariant::Twitter
        );
        assert_eq!(
            ColorPickerVariant::parse("github"),
            ColorPickerVariant::Github
        );
        assert_eq!(
            ColorPickerVariant::parse("slider"),
            ColorPickerVariant::Slider
        );
        assert_eq!(ColorPickerVariant::parse("default").as_str(), "default");
        assert_eq!(ColorPickerVariant::Default.swatch_columns(), 6);
    }

    #[test]
    fn sync_trigger_preview_writes_swatch_and_label() {
        let mut swatch = Element::new();
        swatch.template_id = Some("t__cp_trig_swatch".into());
        let mut label = Element::new().text("#000000");
        label.template_id = Some("t__cp_trig_label".into());
        let mut root = Element::new().child(swatch).child(label);
        let c = Rgba {
            r: 0x1b,
            g: 0x63,
            b: 0xda,
            a: 1.0,
        };
        let impact = sync_trigger_preview(&mut root, "t", c, ColorFormat::Hex);
        assert_eq!(impact, crate::core::tree::LayoutStatus::StyleChanged);
        assert_eq!(root.children[1].text.as_deref(), Some("#1b63da"));
        assert!((root.children[0].style.bg_color[0] - 27.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn opaque_formats_do_not_support_alpha() {
        assert!(!ColorFormat::Hex.supports_alpha());
        assert!(!ColorFormat::Rgb.supports_alpha());
        assert!(!ColorFormat::Hsl.supports_alpha());
        assert!(ColorFormat::Hex8.supports_alpha());
        assert!(ColorFormat::Rgba.supports_alpha());
        assert!(ColorFormat::Hsla.supports_alpha());
    }

    #[test]
    fn sv_texture_node_has_opaque_bed() {
        let (_handle, _) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("sv_bed_rgba", || Rgba {
                r: 158,
                g: 83,
                b: 83,
                a: 1.0,
            });
            let hue = use_signal("sv_bed_hue", || 0.0f32);
            let (_, s0, v0) = rgb_to_hsv(158, 83, 83);
            let sat = use_signal("sv_bed_sat", || s0);
            let val = use_signal("sv_bed_val", || v0);
            let fmt = use_signal("sv_bed_fmt", || ColorFormat::Hex);
            let out = use_signal("sv_bed_out", || "#9e5353".into());
            let el = sv_area("sv_bed", "9rem", rgba, hue, sat, val, fmt, out, false, true);
            assert!(
                el.style.bg_color[3] > 0.5,
                "SV area bed alpha={}",
                el.style.bg_color[3]
            );
            let tex = el
                .children
                .iter()
                .find(|c| c.attrs.get(ATTR_CP_SLOT).map(|s| s.as_str()) == Some(SLOT_SV_TEX))
                .expect("sv tex");
            assert!(
                tex.style.bg_color[3] > 0.5,
                "SV tex bed alpha={}",
                tex.style.bg_color[3]
            );
        });
    }

    #[test]
    fn format_row_select_matches_input_height() {
        let (_handle, _) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("fmt_h_rgba", || Rgba {
                r: 158,
                g: 83,
                b: 83,
                a: 1.0,
            });
            let hue = use_signal("fmt_h_hue", || 0.0f32);
            let sat = use_signal("fmt_h_sat", || 0.0f32);
            let val = use_signal("fmt_h_val", || 1.0f32);
            let fmt_sig = use_signal("fmt_h_fmt", || ColorFormat::Hex);
            let fmt_str = use_signal("fmt_h_fmt_str", || "hex".to_string());
            let out = use_signal("fmt_h_out", || "#9e5353".into());
            let row = format_value_row("fmt_h", rgba, hue, sat, val, fmt_sig, fmt_str, out, false);
            assert!(row.children.len() >= 2);
            let sel_h = row.children[0].style.size.height;
            let in_h = row.children[1].style.size.height;
            assert_eq!(sel_h, in_h, "Select height {sel_h:?} vs Input {in_h:?}");
        });
    }

    #[test]
    fn sv_knob_insets_match_hsv_before_layout() {
        let (_handle, _) = crate::core::reactivity::create_root(|| {
            let c = Rgba {
                r: 158,
                g: 83,
                b: 83,
                a: 1.0,
            };
            let rgba = use_signal("sv_knob_rgba", || c);
            let hue = use_signal("sv_knob_hue", || 0.0f32);
            let (_, s0, v0) = rgb_to_hsv(c.r, c.g, c.b);
            let sat = use_signal("sv_knob_sat", || s0);
            let val = use_signal("sv_knob_val", || v0);
            let fmt = use_signal("sv_knob_fmt", || ColorFormat::Hex);
            let out = use_signal("sv_knob_out", || "#9e5353".into());
            let el = sv_area(
                "sv_knob", "9rem", rgba, hue, sat, val, fmt, out, false, true,
            );
            let knob = el
                .children
                .iter()
                .find(|ch| ch.attrs.get(ATTR_CP_SLOT).map(|s| s.as_str()) == Some(SLOT_SV_KNOB))
                .expect("sv knob");
            let (left, top) = sv_knob_frac(s0, v0);
            match crate::layout::lpa_kind(knob.style.inset.left) {
                crate::layout::DimKind::Percent(p) => {
                    assert!((p - left).abs() < 0.0005, "left percent {p} vs {left}")
                }
                other => panic!("expected percent left, got {other:?}"),
            }
            match crate::layout::lpa_kind(knob.style.inset.top) {
                crate::layout::DimKind::Percent(p) => {
                    assert!((p - top).abs() < 0.0005, "top percent {p} vs {top}")
                }
                other => panic!("expected percent top, got {other:?}"),
            }
            assert_eq!(knob.style.translate, [-SV_KNOB_HALF_PX, -SV_KNOB_HALF_PX]);
            assert!(
                left > 0.2 && top > 0.2,
                "fixture should not sit on the default (S=0,V=1) corner, got left={left} top={top}"
            );
        });
    }

    fn quantized_sv_pair(hue: f32) -> (f32, f32, f32, f32, Rgba) {
        let mut first_of: std::collections::HashMap<(u8, u8, u8), (f32, f32)> =
            std::collections::HashMap::new();
        for yi in 0..=80 {
            let val = yi as f32 / 80.0;
            for xi in 0..=80 {
                let sat = xi as f32 / 80.0;
                let rgb = hsv_to_rgb(hue, sat, val);
                if let Some(&(s0, v0)) = first_of.get(&rgb) {
                    if (sat - s0).abs() + (val - v0).abs() > 0.05 {
                        return (
                            s0,
                            v0,
                            sat,
                            val,
                            Rgba {
                                r: rgb.0,
                                g: rgb.1,
                                b: rgb.2,
                                a: 1.0,
                            },
                        );
                    }
                } else {
                    first_of.insert(rgb, (sat, val));
                }
            }
        }
        panic!("expected an 8-bit RGB collision on the SV plane at hue={hue}");
    }

    #[test]
    fn sv_knob_keeps_pointer_sv_when_rgb_quantizes() {
        let (s0, v0, s1, v1, c) = quantized_sv_pair(210.0);
        let (_, inv_s, inv_v) = rgb_to_hsv(c.r, c.g, c.b);
        let mut k0 = Element::new();
        let mut k1 = Element::new();
        let _ = place_sv_knob(&mut k0, None, c, s0, v0);
        let _ = place_sv_knob(&mut k1, None, c, s1, v1);
        let left = |k: &Element| match crate::layout::lpa_kind(k.style.inset.left) {
            crate::layout::DimKind::Percent(p) => p,
            other => panic!("{other:?}"),
        };
        let top = |k: &Element| match crate::layout::lpa_kind(k.style.inset.top) {
            crate::layout::DimKind::Percent(p) => p,
            other => panic!("{other:?}"),
        };
        assert!((left(&k0) - s0).abs() < 0.0005);
        assert!((left(&k1) - s1).abs() < 0.0005);
        let (t0, t1) = (sv_knob_frac(s0, v0).1, sv_knob_frac(s1, v1).1);
        assert!((top(&k0) - t0).abs() < 0.0005);
        assert!((top(&k1) - t1).abs() < 0.0005);
        assert!((left(&k0) - left(&k1)).abs() + (top(&k0) - top(&k1)).abs() > 0.04);
        // rgb_to_hsv would collapse both samples onto one inverse point.
        let (inv_l, inv_t) = sv_knob_frac(inv_s, inv_v);
        assert!(
            (left(&k1) - inv_l).abs() + (top(&k1) - inv_t).abs() > 0.01,
            "second sample must not snap to rgb inverse ({inv_s}, {inv_v})"
        );
    }

    #[test]
    fn sv_pointer_uses_window_space_bounds() {
        let (_handle, _) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("sv_ptr_rgba", || Rgba::opaque(255, 255, 255));
            let hue = use_signal("sv_ptr_hue", || 210.0f32);
            let sat = use_signal("sv_ptr_sat", || 0.0f32);
            let val = use_signal("sv_ptr_val", || 1.0f32);
            let out = use_signal("sv_ptr_out", || "#ffffff".into());
            let fmt_sig = use_signal("sv_ptr_fmt", || ColorFormat::Hex);
            let visual = [20.0, 200.0, 100.0, 80.0];
            let drag = ColorDrag {
                kind: ColorDragKind::Sv,
                bounds: Rc::new(Cell::new(visual)),
                rgba,
                hue,
                sat,
                val,
                show_alpha: false,
                out,
                fmt_sig,
                sv_id: Some("sv_ptr".into()),
            };
            apply_color_pointer(&drag, 70.0, 240.0);
            assert!((sat.peek() - 0.5).abs() < 0.02, "sat={}", sat.peek());
            assert!(
                (val.peek() - 0.5).abs() < 0.02,
                "val={} (ty must use visual y, not scrolled layout y)",
                val.peek()
            );

            let document_y = visual[1] + 800.0;
            apply_color_pointer(
                &ColorDrag {
                    bounds: Rc::new(Cell::new([visual[0], document_y, visual[2], visual[3]])),
                    ..drag
                },
                70.0,
                240.0,
            );
            assert!(
                (val.peek() - 1.0).abs() < 0.02,
                "document-space y must clamp ty→0 / V→1; got {}",
                val.peek()
            );
        });
    }
}
