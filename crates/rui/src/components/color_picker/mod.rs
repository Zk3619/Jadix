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
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::app::portal_overlay::{OverlayKind, OverlayMode, portal_sync};
use crate::app::reactive::{StylePatch, bind_style, bind_text, reactive_component_live};
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::components::select::{SelectItemData, select};
use crate::components::separator::separator;
use crate::components::slider::slider;
use crate::core::reactivity::{Signal, create_effect, use_signal};
use crate::core::tree::{CursorKind, Element, LayoutStatus, parse_color, parse_text_color};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{r#box, hstack, vstack};
use crate::primitives::interactive::interactive;
use crate::tokens;
use crate::utils::controlled::{resolve_open_signal, resolve_string_signal};
use crate::utils::popup::{
    ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_W, PopupPlacement,
    apply_stored_floating_place, mark_floating, mount_anchored_overlay,
};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_node::{TextNodeConfig, build_text_node};
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

mod drag;
mod model;
pub mod view;

pub(crate) use drag::{clear_color_drag_if_any, is_color_dragging, tick_color_drag};
pub use model::{
    ColorFormat, ColorPickerVariant, Rgba, format_color, hsl_to_rgb, hsv_to_rgb, parse_color_str,
    rgb_to_hsl, rgb_to_hsv,
};
pub(crate) use view::flush_color_picker_textures;
use view::{ColorPickerRenderProps, build_color_picker_view, follow_hue_into_rgba};

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
    use super::drag::{ColorDrag, ColorDragKind, apply_color_pointer};
    use super::model::SWATCHES_GITHUB;
    use super::view::{
        ATTR_CP_SLOT, SLOT_SV_KNOB, SLOT_SV_TEX, SV_KNOB_HALF_PX, format_value_row, place_sv_knob,
        sv_area, sv_knob_frac, sv_texture_mid_column_luma, swatch_grid, sync_trigger_preview,
        tex_sv_cpu_count,
    };
    use super::*;

    #[test]
    fn closed_color_picker_defers_sv_texture_until_panel() {
        use crate::core::reactivity::create_root;
        let before = tex_sv_cpu_count();
        let (_h, _el) = create_root(|| {
            let _: Element = color_picker("defer-sv-closed").into();
        });
        let after = tex_sv_cpu_count();
        assert_eq!(
            before, after,
            "closed ColorPicker must not generate SV texture at mount"
        );
    }

    #[test]
    fn sv_texture_top_row_is_bright_bottom_is_dark() {
        let (top_luma, bot_luma) = sv_texture_mid_column_luma("orient_sv", 0.0);
        assert!(
            top_luma > bot_luma + 200,
            "SV row0 (widget top) must be brighter than last row; top={top_luma} bot={bot_luma}"
        );
        assert!(
            bot_luma < 24,
            "bottom row should be near black, got {bot_luma}"
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
        assert_eq!(ColorPickerVariant::Github.swatch_chip_size(), "1.5rem");
        assert_eq!(ColorPickerVariant::Github.swatch_columns(), 8);
    }

    #[test]
    fn github_swatch_grid_chips_have_explicit_square_size() {
        let (_handle, grid) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("gh_sw_rgba", || Rgba::opaque(0, 77, 207));
            let hue = use_signal("gh_sw_hue", || 220.0f32);
            let sat = use_signal("gh_sw_sat", || 1.0f32);
            let val = use_signal("gh_sw_val", || 0.8f32);
            let fmt = use_signal("gh_sw_fmt", || ColorFormat::Hex);
            let out = use_signal("gh_sw_out", || "#004dcf".into());
            let open = use_signal("gh_sw_open", || true);
            swatch_grid(
                "gh_sw",
                ColorPickerVariant::Github,
                rgba,
                hue,
                sat,
                val,
                fmt,
                out,
                open,
                "0.25rem",
            )
        });
        assert_eq!(
            grid.children.len(),
            SWATCHES_GITHUB.len(),
            "github ships 16 preset chips"
        );
        for (i, btn) in grid.children.iter().enumerate() {
            let h = crate::layout::dim_length_px(btn.style.size.height);
            let w = crate::layout::dim_length_px(btn.style.size.width);
            assert!(
                h.is_some_and(|v| v >= 20.0),
                "swatch {i} height must be explicit (≥1.5rem), got {:?}",
                btn.style.size.height
            );
            assert!(
                w.is_some_and(|v| v >= 20.0),
                "swatch {i} width must be explicit (≥1.5rem), got {:?}",
                btn.style.size.width
            );
            let chip = btn.children.first().expect("chip child");
            let ch = crate::layout::dim_length_px(chip.style.size.height);
            assert!(
                ch.is_some_and(|v| v >= 20.0),
                "chip {i} must not rely on unwired aspect-ratio; h={:?}",
                chip.style.size.height
            );
        }
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
