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

pub(crate) use drag::{
    clear_color_drag_if_any, drain_color_drag_apply, is_color_dragging, queue_color_drag_apply,
    snapshot_color_drag_bounds, tick_color_drag_with_bounds,
};
pub use model::{
    format_color, hsl_to_rgb, hsv_to_rgb, parse_color_str, rgb_to_hsl, rgb_to_hsv, ColorFormat,
    ColorPickerVariant, Rgba,
};
pub(crate) use view::flush_color_picker_textures;
use view::{build_color_picker_view, follow_fmt_str, follow_hue_into_rgba, ColorPickerRenderProps};

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

#[cfg(test)]
mod tests {
    use super::drag::{
        apply_color_pointer, begin_color_drag, drain_color_drag_apply, end_color_drag,
        queue_color_drag_apply, tick_color_drag, tick_color_drag_with_bounds, ColorDrag,
        ColorDragKind,
    };
    use super::model::SWATCHES_GITHUB;
    use super::view::{
        build_color_picker_view, format_value_row, place_sv_knob, sv_area, sv_knob_frac,
        sv_texture_mid_column_luma, swatch_grid, sync_trigger_preview, tex_sv_cpu_count,
        ColorPickerRenderProps, ATTR_CP_SLOT, SLOT_SV_KNOB, SLOT_SV_TEX, SV_KNOB_HALF_PX,
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
    fn switching_to_alpha_format_mounts_alpha_slider() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, fmt_str) = create_root(|| {
            let fmt_str = use_signal("cp-afmt__cp_fmt_str", || "hex".to_string());
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("cp-afmt").default_value("#3b82f6").into();
                *live_e.borrow_mut() = Element::new().w(800.0).h(600.0).child(picker);
            });
            fmt_str
        });
        drain_deferred_view_remounts();

        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn present_pass(root: &mut Element) {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
        }
        fn event_loop_pass(root: &mut Element) {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            present_pass(root);
        }

        let click = {
            let tree = live.borrow();
            find_click(&tree, "cp-afmt:trigger").expect("trigger")
        };
        click.borrow_mut()();
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "cp-afmt__floating_panel").is_some(),
                "popover must be open"
            );
            assert!(
                find_tid(&tree, "cp-afmt:alpha").is_none(),
                "HEX format must not mount the alpha slider"
            );
        }

        fmt_str.set("hex8".into());
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "cp-afmt:alpha").is_some(),
                "switching HEX → HEX8 must mount the alpha slider on the open popover"
            );
        }

        fmt_str.set("hex".into());
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "cp-afmt:alpha").is_none(),
                "switching HEX8 → HEX must unmount the alpha slider"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
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
    fn newer_sv_pointer_supersedes_queued_render_sample() {
        crate::core::reactivity::create_root(|| {
            let sat = Signal::new(0.0f32);
            let val = Signal::new(1.0f32);
            begin_color_drag(ColorDrag {
                kind: ColorDragKind::Sv,
                bounds: Rc::new(Cell::new([0.0, 0.0, 100.0, 100.0])),
                rgba: Signal::new(Rgba::opaque(255, 255, 255)),
                hue: Signal::new(0.0),
                sat,
                val,
                show_alpha: false,
                out: Signal::new("#ffffff".into()),
                fmt_sig: Signal::new(ColorFormat::Hex),
                sv_id: Some("sv-newer-sample".into()),
            });
            queue_color_drag_apply(20.0, 20.0, None);
            tick_color_drag_with_bounds(80.0, 70.0, None);
            drain_color_drag_apply();
            let result = [sat.peek(), val.peek()];
            end_color_drag();
            assert!(
                (result[0] - 0.8).abs() < 0.001 && (result[1] - 0.3).abs() < 0.001,
                "queued stale pointer replaced latest sample: {result:?}"
            );
        });
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

    /// Frozen press-time window box + leftover smooth-scroll: the Fixed overlay
    /// (and SV chart) move while the pointer tracks the same S/V. Stale bounds
    /// map the new pointer to V→1; live bounds keep V≈0.5 (跟手).
    #[test]
    fn tick_color_drag_uses_live_bounds_when_chart_moves() {
        let (_handle, _) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("sv_live_rgba", || Rgba::opaque(255, 255, 255));
            let hue = use_signal("sv_live_hue", || 210.0f32);
            let sat = use_signal("sv_live_sat", || 0.0f32);
            let val = use_signal("sv_live_val", || 1.0f32);
            let out = use_signal("sv_live_out", || "#ffffff".into());
            let fmt_sig = use_signal("sv_live_fmt", || ColorFormat::Hex);
            let frozen = [20.0, 200.0, 100.0, 80.0];
            let live = [20.0, 120.0, 100.0, 80.0];
            begin_color_drag(ColorDrag {
                kind: ColorDragKind::Sv,
                bounds: Rc::new(Cell::new(frozen)),
                rgba,
                hue,
                sat,
                val,
                show_alpha: false,
                out,
                fmt_sig,
                sv_id: Some("sv_live".into()),
            });

            let px = 70.0;
            // Pointer stays on the live chart center (user tracking the same S/V
            // while the overlay moved up 80px).
            let py_live_center = 160.0;
            tick_color_drag(px, py_live_center);
            assert!(
                (val.peek() - 1.0).abs() < 0.02,
                "frozen y=200 maps live-center y=160 to ty→0 / V→1; got {}",
                val.peek()
            );

            assert!(tick_color_drag_with_bounds(px, py_live_center, Some(live)));
            assert!((sat.peek() - 0.5).abs() < 0.02, "sat={}", sat.peek());
            assert!(
                (val.peek() - 0.5).abs() < 0.02,
                "live bounds must keep V≈0.5 when the pointer tracks the chart; got {}",
                val.peek()
            );

            // Pointer held at the press-time center (y=240). The live chart has
            // moved up, so that window point is now the SV bottom → V=0.
            tick_color_drag_with_bounds(px, 240.0, Some(frozen));
            assert!((val.peek() - 0.5).abs() < 0.02);
            tick_color_drag_with_bounds(px, 240.0, Some(live));
            assert!(
                val.peek() < 0.02,
                "pointer at frozen center is the live chart bottom (V→0); got {}",
                val.peek()
            );

            end_color_drag();
        });
    }

    #[test]
    fn color_picker_drag_sets_after_state_drop() {
        use crate::app::reactive::{bind_prop, drain_deferred_view_remounts};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};

        crate::app::reactive::reset_registered_anchors();
        clear_test_reconcile_root();
        let tree = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&tree));
        let (_handle, sat) = crate::core::reactivity::create_root({
            let tree = Rc::clone(&tree);
            move || {
                let rgba = use_signal("sv_drop_rgba", || Rgba::opaque(255, 255, 255));
                let hue = use_signal("sv_drop_hue", || 210.0f32);
                let sat = use_signal("sv_drop_sat", || 0.0f32);
                let val = use_signal("sv_drop_val", || 1.0f32);
                let out = use_signal("sv_drop_out", || "#ffffff".into());
                let fmt_sig = use_signal("sv_drop_fmt", || ColorFormat::Hex);
                create_effect({
                    let tree = Rc::clone(&tree);
                    move || {
                        let leaf = bind_prop(
                            Element::new().template_id("sv-drop-leaf"),
                            "sv-drop-leaf",
                            move || format!("{:.3}", sat.get()),
                            |el, next| {
                                el.text = Some(next);
                                LayoutStatus::StyleChanged
                            },
                        );
                        *tree.borrow_mut() = leaf;
                    }
                });
                begin_color_drag(ColorDrag {
                    kind: ColorDragKind::Sv,
                    bounds: Rc::new(Cell::new([0.0, 0.0, 100.0, 100.0])),
                    rgba,
                    hue,
                    sat,
                    val,
                    show_alpha: false,
                    out,
                    fmt_sig,
                    sv_id: Some("sv_drop".into()),
                });
                sat
            }
        });
        drain_deferred_view_remounts();
        assert_eq!(tree.borrow().text.as_deref(), Some("0.000"));

        let live_bounds = Some([0.0, 0.0, 100.0, 100.0]);
        {
            let held = tree.borrow_mut();
            queue_color_drag_apply(80.0, 20.0, live_bounds);
            assert_eq!(sat.peek(), 0.0, "queue under borrow must not Signal::set");
            assert_eq!(
                held.text.as_deref(),
                Some("0.000"),
                "bind_prop must not patch while AppState is borrowed"
            );
        }
        drain_color_drag_apply();
        assert!(
            (sat.peek() - 0.8).abs() < 0.02,
            "apply after drop must write sat; got {}",
            sat.peek()
        );
        drain_deferred_view_remounts();
        assert_eq!(
            tree.borrow().text.as_deref(),
            Some("0.800"),
            "leaf bind must land once try_borrow_mut succeeds"
        );

        end_color_drag();
        clear_test_reconcile_root();
        crate::app::reactive::reset_registered_anchors();
    }

    fn find_cp_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_CP_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_cp_slot(c, slot))
    }

    /// Percent `top` must resolve against the SV height. If Taffy treats it as auto
    /// (indefinite CB) the absolute knob falls to static position — after the
    /// 100%-tall texture — i.e. glued to the bottom of the SV chart.
    #[test]
    fn sv_knob_layout_mid_sv_is_not_glued_to_bottom() {
        let (_handle, mut root) = crate::core::reactivity::create_root(|| {
            let rgba = use_signal("sv_lay_rgba", || Rgba::opaque(255, 0, 0));
            let hue = use_signal("sv_lay_hue", || 0.0f32);
            let sat = use_signal("sv_lay_sat", || 0.5f32);
            let val = use_signal("sv_lay_val", || 0.5f32);
            let fmt = use_signal("sv_lay_fmt", || ColorFormat::Hex);
            let out = use_signal("sv_lay_out", || "#ff0000".into());
            Element::new().w(280.0).h(200.0).child(sv_area(
                "sv_lay", "9rem", rgba, hue, sat, val, fmt, out, false, true,
            ))
        });
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 280.0, 200.0, [0.0, 0.0]);
        let area = root.children.first().expect("sv area");
        let knob = find_cp_slot(&root, SLOT_SV_KNOB).expect("sv knob");
        let ab = area.layout_bounds;
        let kb = knob.layout_bounds;
        assert!(
            ab[3] > 80.0,
            "SV area must have a definite height; got {ab:?}"
        );
        let frac_left = (kb[0] - ab[0]) / ab[2].max(1.0);
        let frac_top = (kb[1] - ab[1]) / ab[3].max(1.0);
        assert!(
            (frac_left - 0.5).abs() < 0.08,
            "sat=0.5 → left frac {frac_left}; area={ab:?} knob={kb:?}"
        );
        assert!(
            (frac_top - 0.5).abs() < 0.08,
            "val=0.5 → top frac {frac_top} (must not glue to SV bottom); area={ab:?} knob={kb:?}"
        );
    }

    /// Hit-path visual box after page scroll must map the pointer to the same
    /// S/V as an unscrolled chart. Document-space `layout_bounds` would push
    /// V→0 (knob forced to the SV bottom) once offset_y exceeds the chart height.
    #[test]
    fn sv_hit_path_after_scroll_maps_visual_center() {
        use crate::app::pointer_events::hit_path;
        use crate::components::scroll_area;
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;
        use crate::core::tree::LayoutStatus;

        let rest = "sv-scroll-hit";
        let (_handle, mut root) = crate::core::reactivity::create_root(|| {
            use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).set_silent(240.0);
            let rgba = use_signal("sv_scr_rgba", || Rgba::opaque(255, 255, 255));
            let hue = use_signal("sv_scr_hue", || 210.0f32);
            let sat = use_signal("sv_scr_sat", || 0.0f32);
            let val = use_signal("sv_scr_val", || 1.0f32);
            let fmt = use_signal("sv_scr_fmt", || ColorFormat::Hex);
            let out = use_signal("sv_scr_out", || "#ffffff".into());
            let page = Element::new()
                .w(280.0)
                .h(800.0)
                .flex_col()
                .child(Element::new().h(240.0).w("100%"))
                .child(sv_area(
                    "sv_scr", "9rem", rgba, hue, sat, val, fmt, out, false, false,
                ));
            scroll_area(rest)
                .w(280.0)
                .h(200.0)
                .overflow_y("scroll")
                .child(page)
                .into()
        });
        let mut tree = crate::layout::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        let _ = tree.build_and_layout(&mut root, 280.0, 200.0, [0.0, 0.0]);
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut tree,
            &mut impact,
            280.0,
            200.0,
            [0.0, 0.0],
        );
        let _ = tree.build_and_layout_fast(&mut root, 280.0, 200.0, [0.0, 0.0], true, false);

        let knob = find_cp_slot(&root, SLOT_SV_KNOB).expect("sv knob");
        // After 240px scroll the spacer is gone; SV sits at the viewport top.
        let path = hit_path(&root, 140.0, 72.0);
        let leaf = path.last().expect("hit sv after scroll");
        let b = leaf.bounds;
        assert!(
            b[3] > 80.0 && b[3] < 200.0,
            "SV hit bounds height should be the chart, not the scrolled page; {b:?}"
        );
        let rgba = use_signal("sv_scr_rgba", || Rgba::opaque(255, 255, 255));
        let hue = use_signal("sv_scr_hue", || 210.0f32);
        let sat = use_signal("sv_scr_sat", || 0.0f32);
        let val = use_signal("sv_scr_val", || 1.0f32);
        let out = use_signal("sv_scr_out", || "#ffffff".into());
        let fmt_sig = use_signal("sv_scr_fmt", || ColorFormat::Hex);
        let drag = ColorDrag {
            kind: ColorDragKind::Sv,
            bounds: std::rc::Rc::new(std::cell::Cell::new(b)),
            rgba,
            hue,
            sat,
            val,
            show_alpha: false,
            out,
            fmt_sig,
            sv_id: Some("sv_scr".into()),
        };
        let px = b[0] + b[2] * 0.5;
        let py = b[1] + b[3] * 0.5;
        apply_color_pointer(&drag, px, py);
        assert!(
            (sat.peek() - 0.5).abs() < 0.08,
            "scrolled visual center must map S≈0.5, sat={} bounds={b:?}",
            sat.peek()
        );
        assert!(
            (val.peek() - 0.5).abs() < 0.08,
            "scrolled visual center must map V≈0.5 (not glued to bottom V=0), val={} bounds={b:?} knob={:?}",
            val.peek(),
            knob.layout_bounds
        );
    }

    #[test]
    fn builder_trigger_style_reaches_trigger_slot() {
        use crate::core::reactivity::create_root;
        let (_h, el): (_, Element) = create_root(|| {
            let id = "cp-trig-chrome";
            let rgba = use_signal("cp-trig-chrome__cp_rgba", || Rgba::opaque(59, 130, 246));
            let hue = use_signal("cp-trig-chrome__cp_hue", || 210.0f32);
            let sat = use_signal("cp-trig-chrome__cp_sat", || 0.76f32);
            let val = use_signal("cp-trig-chrome__cp_val", || 0.96f32);
            let fmt_sig = use_signal("cp-trig-chrome__cp_fmt", || ColorFormat::Hex);
            let fmt_str = use_signal("cp-trig-chrome__cp_fmt_str", || "hex".to_string());
            let out = use_signal("cp-trig-chrome__value", || "#3b82f6".to_string());
            let open = use_signal("cp-trig-chrome__open", || false);
            let slots =
                SlotMap::default().slot("color-picker-trigger", |c| c.style(|b| b.text_size(20.0)));
            build_color_picker_view(
                ColorPickerRenderProps {
                    id: id.into(),
                    variant: ColorPickerVariant::Default,
                    format: ColorFormat::Hex,
                    show_alpha: true,
                    disabled: false,
                },
                open,
                out,
                rgba,
                hue,
                sat,
                val,
                fmt_sig,
                fmt_str,
                &slots,
            )
        });
        fn find_trigger_size(el: &Element, want: f32) -> bool {
            let is_trig =
                el.attrs.get(ATTR_CP_SLOT).map(String::as_str) == Some("color-picker-trigger");
            (is_trig
                && (el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                    || (el.text_style.size_resolved() - want).abs() < 0.01))
                || el.children.iter().any(|c| find_trigger_size(c, want))
        }
        assert!(
            find_trigger_size(&el, 20.0),
            "color_picker .style must land on data-slot=color-picker-trigger"
        );
    }

    #[test]
    fn color_picker_trigger_uses_outline_button_hover_classes() {
        use crate::core::reactivity::create_root;
        use crate::style::ClassSet;
        let (_h, el): (_, Element) = create_root(|| {
            let id = "cp-trig-hover";
            let rgba = use_signal("cp-trig-hover__cp_rgba", || Rgba::opaque(59, 130, 246));
            let hue = use_signal("cp-trig-hover__cp_hue", || 210.0f32);
            let sat = use_signal("cp-trig-hover__cp_sat", || 0.76f32);
            let val = use_signal("cp-trig-hover__cp_val", || 0.96f32);
            let fmt_sig = use_signal("cp-trig-hover__cp_fmt", || ColorFormat::Hex);
            let fmt_str = use_signal("cp-trig-hover__cp_fmt_str", || "hex".to_string());
            let out = use_signal("cp-trig-hover__value", || "#3b82f6".to_string());
            let open = use_signal("cp-trig-hover__open", || false);
            build_color_picker_view(
                ColorPickerRenderProps {
                    id: id.into(),
                    variant: ColorPickerVariant::Default,
                    format: ColorFormat::Hex,
                    show_alpha: true,
                    disabled: false,
                },
                open,
                out,
                rgba,
                hue,
                sat,
                val,
                fmt_sig,
                fmt_str,
                &SlotMap::default(),
            )
        });
        fn find_outline_trigger(el: &Element) -> Option<(bool, Option<String>)> {
            let is_trig =
                el.attrs.get(ATTR_CP_SLOT).map(String::as_str) == Some("color-picker-trigger");
            if is_trig {
                let has_hover = el.class_set.as_ref().is_some_and(ClassSet::has_hover_color);
                return Some((has_hover, el.focus_id.clone()));
            }
            el.children.iter().find_map(find_outline_trigger)
        }
        let Some((has_hover, fid)) = find_outline_trigger(&el) else {
            panic!("missing data-slot=color-picker-trigger");
        };
        assert!(
            has_hover && fid.as_deref() == Some("cp-trig-hover:trigger"),
            "ColorPicker trigger must be Button outline with hover color classes, not a static interactive row\n\
             has_hover={has_hover} focus_id={fid:?}"
        );
    }

    #[test]
    fn builder_content_class_reaches_inline_panel() {
        use crate::core::reactivity::create_root;
        let (_h, el): (_, Element) = create_root(|| {
            let id = "cp-content-chrome";
            let rgba = use_signal("cp-content-chrome__cp_rgba", || Rgba::opaque(59, 130, 246));
            let hue = use_signal("cp-content-chrome__cp_hue", || 210.0f32);
            let sat = use_signal("cp-content-chrome__cp_sat", || 0.76f32);
            let val = use_signal("cp-content-chrome__cp_val", || 0.96f32);
            let fmt_sig = use_signal("cp-content-chrome__cp_fmt", || ColorFormat::Hex);
            let fmt_str = use_signal("cp-content-chrome__cp_fmt_str", || "hex".to_string());
            let out = use_signal("cp-content-chrome__value", || "#3b82f6".to_string());
            let open = use_signal("cp-content-chrome__open", || true);
            let slots = SlotMap::default().slot("color-picker-content", |c| c.class("p-4"));
            build_color_picker_view(
                ColorPickerRenderProps {
                    id: id.into(),
                    variant: ColorPickerVariant::Inline,
                    format: ColorFormat::Hex,
                    show_alpha: true,
                    disabled: false,
                },
                open,
                out,
                rgba,
                hue,
                sat,
                val,
                fmt_sig,
                fmt_str,
                &slots,
            )
        });
        fn find_content_class(el: &Element, needle: &str) -> bool {
            let is_content =
                el.attrs.get(ATTR_CP_SLOT).map(String::as_str) == Some("color-picker-content");
            (is_content
                && (el
                    .class_set
                    .as_ref()
                    .is_some_and(|cs| cs.base.iter().any(|t| t.contains(needle)))
                    || el
                        .attrs
                        .get("rui_utility_class")
                        .is_some_and(|c| c.contains(needle))))
                || el.children.iter().any(|c| find_content_class(c, needle))
        }
        assert!(
            find_content_class(&el, "p-4"),
            "color_picker .slot(color-picker-content) class must land on data-slot=color-picker-content"
        );
    }

    #[test]
    fn click_default_color_picker_hoists_fade_panel_and_encodes_its_band() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::damage::{
            overlay_band_encode_flags, overlay_membership_paint_stamps, push_frame_paint_element,
            take_frame_paint_nodes, PaintDirtyKind,
        };
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, overlay_portal_stamp_key, LayoutTree, ATTR_HOISTED_PORTAL,
        };
        use crate::utils::floating_motion::{
            flush_floating_motion_subtree, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let _ = take_frame_paint_nodes();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("gal-color").default_value("#3b82f6").into();
                *live_e.borrow_mut() = Element::new().w(800.0).h(600.0).child(picker);
            });
        });

        drain_deferred_view_remounts();

        fn find_trigger_click(
            el: &Element,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some("gal-color:trigger") {
                return el.on_click.clone();
            }
            el.children.iter().find_map(find_trigger_click)
        }
        let click = {
            let tree = live.borrow();
            find_trigger_click(&tree).expect("ColorPicker trigger click")
        };
        click.borrow_mut()();
        drain_deferred_view_remounts();

        fn find_hoisted_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                && el
                    .template_id
                    .as_deref()
                    .is_some_and(|t| t.contains("gal-color") && t.ends_with("__floating_panel"))
            {
                return Some(el);
            }
            el.children.iter().find_map(find_hoisted_panel)
        }
        fn any_hoisted(el: &Element) -> bool {
            el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                || el.children.iter().any(any_hoisted)
        }

        let (
            overlay_start,
            inst_len,
            textured_start,
            textured_len,
            band_keys,
            panel_key,
            panel_bounds,
            panel_opacity,
            panel_tid,
            panel_portal_key,
            overlay_dirties,
        ) = {
            let mut root = live.borrow_mut();
            let _ = hoist_portal_subtrees(&mut root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(&mut root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    &mut root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let _ = flush_floating_motion_subtree(&mut root);
            let _ = tick_hoisted_floating_css(&mut root);
            let out = tree.build_and_layout_fast_ex(
                &mut root,
                800.0,
                600.0,
                [0.0, 0.0],
                false,
                false,
                None,
            );
            let overlay_start = out.overlay_split.instance_start;
            let inst_len = out.instances.len();
            let textured_start = out.overlay_split.textured_start;
            let textured_len = out.textured_draws.len();
            let band_keys: Vec<u64> = out
                .overlay_split
                .band_ranges(
                    out.instances.len(),
                    out.textured_draws.len(),
                    out.text_nodes.len(),
                    0,
                    0,
                )
                .iter()
                .map(|b| b.stamp_key)
                .collect();
            let has_any = any_hoisted(&root);
            let panel = find_hoisted_panel(&root);
            let panel_bounds = panel.map(|p| p.layout_bounds).unwrap_or([0.0; 4]);
            let panel_opacity = panel.map(|p| p.style.opacity).unwrap_or(0.0);
            let panel_tid = panel.and_then(|p| p.template_id.clone());
            let panel_portal_key = panel.and_then(|p| p.attrs.get("rui_portal_key").cloned());
            let panel_key = panel.map(overlay_portal_stamp_key).unwrap_or(0);
            let _ = take_frame_paint_nodes();
            if let Some(p) = panel {
                push_frame_paint_element(p, [0.0, 0.0]);
            }
            let overlay_dirties: Vec<u64> = take_frame_paint_nodes()
                .into_iter()
                .filter(|d| d.kind == PaintDirtyKind::Overlay)
                .map(|d| d.key)
                .collect();
            // Exercise paint-only Fade frames with retained sibling caches enabled.
            tree.set_collect_skip_allowed(true);
            crate::style::utility::set_frame_dt(0.016);
            for frame in 0..24 {
                let _ = flush_floating_motion_subtree(&mut root);
                let _ = tick_hoisted_floating_css(&mut root);
                let painted = tree.collect_animation_frame(&mut root, [0.0, 0.0], false);
                let opacity = find_hoisted_panel(&root).unwrap().style.opacity;
                let sv = painted
                    .textured_draws
                    .iter()
                    .find(|draw| draw.texture_key.starts_with("rui_cp_sv:gal-color:"))
                    .expect("SV texture must be collected during Fade");
                assert!(
                    (sv.opacity - opacity).abs() < 0.001,
                    "SV opacity lagged behind panel at frame {frame}: {} vs {opacity}",
                    sv.opacity
                );
                if frame == 23 {
                    assert!(
                        sv.opacity > 0.999,
                        "SV must not retain a translucent Fade sample"
                    );
                }
            }
            crate::style::utility::set_frame_dt(0.0);
            assert!(
                has_any,
                "clicking #3b82f6 trigger must hoist ColorPicker portal; overlay_start={overlay_start} inst={inst_len} overlay_depth={}",
                crate::app::portal_overlay::overlay_stack_depth_for_test(),
            );
            (
                overlay_start,
                inst_len,
                textured_start,
                textured_len,
                band_keys,
                panel_key,
                panel_bounds,
                panel_opacity,
                panel_tid,
                panel_portal_key,
                overlay_dirties,
            )
        };

        assert!(
            panel_bounds[2] > 8.0 && panel_bounds[3] > 8.0,
            "ColorPicker popover must have a box; bounds={panel_bounds:?} opacity={panel_opacity} tid={panel_tid:?} key={panel_portal_key:?}"
        );
        assert!(
            overlay_start < inst_len || textured_start < textured_len,
            "ColorPicker overlay suffix must contain instances"
        );
        assert_eq!(
            overlay_dirties,
            vec![panel_key],
            "Fade Overlay dirty must be the portal stamp; panel_key={panel_key}"
        );
        assert!(
            band_keys.iter().any(|k| *k == panel_key),
            "portal-layer band must use the same stamp as Fade dirty; panel_key={panel_key} bands={band_keys:?}"
        );
        let flags = overlay_band_encode_flags(&band_keys, &overlay_dirties, false, &[panel_key]);
        assert!(
            flags.iter().any(|e| *e),
            "PartialLoad must submit ColorPicker; bands={band_keys:?} flags={flags:?}"
        );

        const DP_KEY: u64 = 1;
        let leftover = [64.0, 344.0, 312.0, 352.0];
        let joined =
            overlay_membership_paint_stamps(&[(DP_KEY, leftover)], &[(panel_key, panel_bounds)]);
        assert_eq!(
            joined.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![panel_key],
            "leftover DatePicker occupancy must not become the ColorPicker Overlay dirty"
        );
        let leftover_bands = {
            let mut k = vec![DP_KEY, 0];
            k.extend(band_keys.iter().copied());
            k
        };
        let leftover_flags =
            overlay_band_encode_flags(&leftover_bands, &overlay_dirties, false, &[panel_key]);
        assert!(
            leftover_flags.iter().any(|e| *e),
            "ColorPicker must still encode above a leftover DatePicker band; flags={leftover_flags:?} bands={leftover_bands:?}"
        );

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn color_picker_open_clear_leaves_the_title_prefix_blank() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::damage::{
            overlay_membership_paint_stamps, overlay_replay_restore_bounds, take_frame_paint_nodes,
            PaintDirty, PaintDirtyKind,
        };
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::{sync_reactive_elements_subtree_ex, LayoutStatus, ReactiveSyncCollect};
        use crate::layout::layout_tree::{hoisted_portal_stamps, LayoutTree};
        use crate::primitives::r#box::{hstack, vstack};
        use crate::renderer::font_loader;
        use crate::text::install_shared_font_system;
        use crate::utils::floating_motion::{flush_floating_motion_subtree, tick_hoisted_floating_css};
        use crate::utils::popup::sync_floating_placement;

        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let _ = take_frame_paint_nodes();

        const TITLE: &str = "2c) Inline Calendar · ColorPicker stays under the popover";
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("open-diag").default_value("#3b82f6").into();
                let page = Element::new().w(800.0).h(600.0).overflow("hidden").child(
                    vstack()
                        .w(720.0)
                        .gap("0.5rem")
                        .child(hstack().w("100%").child(Element::new().flex_grow(1.0)).child(picker))
                        .child(Element::new().w("100%").text(TITLE).text_size(16.0)),
                );
                *live_e.borrow_mut() = page;
            });
        });
        drain_deferred_view_remounts();

        let mut tree = LayoutTree::new();
        {
            let mut root = live.borrow_mut();
            let mut impact = LayoutStatus::Clean;
            let mut collect = ReactiveSyncCollect::default();
            sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
                false,
                &mut collect,
            );
            let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        }
        let _ = take_frame_paint_nodes();
        let before_anchor = {
            let root = live.borrow();
            fn find_view<'a>(el: &'a Element) -> Option<&'a Element> {
                if el
                    .template_id
                    .as_deref()
                    .is_some_and(|t| t.contains("open-diag") && t.ends_with("__view"))
                {
                    return Some(el);
                }
                el.children.iter().find_map(find_view)
            }
            find_view(&root).map(|el| {
                (
                    el.layout_bounds,
                    crate::layout::layout_tree::subtree_is_portal_only(el),
                    crate::layout::layout_tree::subtree_declares_portal(el),
                )
            })
        };

        fn find_click(el: &Element) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some("open-diag:trigger") {
                return el.on_click.clone();
            }
            el.children.iter().find_map(find_click)
        }
        fn find_trigger<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.focus_id.as_deref() == Some("open-diag:trigger") {
                return Some(el);
            }
            el.children.iter().find_map(find_trigger)
        }
        let click = {
            let tree = live.borrow();
            find_click(&tree).expect("trigger")
        };
        click.borrow_mut()();
        drain_deferred_view_remounts();

        let mut impact = LayoutStatus::Clean;
        let mut collect = ReactiveSyncCollect::default();
        let pre = {
            let mut root = live.borrow_mut();
            sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
                false,
                &mut collect,
            );
            let mouse = find_trigger(&root).map(|el| {
                let b = el.layout_bounds;
                [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
            });
            if let Some(mouse) = mouse {
                crate::style::sync_class_variants_subtree(
                    &mut root,
                    mouse,
                    "open-diag:trigger",
                    false,
                    800.0,
                    600.0,
                    &mut tree,
                    &mut impact,
                );
            }
            take_frame_paint_nodes()
        };

        let (post, stamps, paints, texts, title_box, panel, trigger_box) = {
            let mut root = live.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut root);
            let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(&mut root, 800.0, 600.0) {
                let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            }
            let _ = flush_floating_motion_subtree(&mut root);
            let _ = tick_hoisted_floating_css(&mut root);
            let painted = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            let post = take_frame_paint_nodes();
            let stamps = hoisted_portal_stamps(&root);
            let title = painted
                .text_nodes
                .iter()
                .find(|n| n.text == TITLE)
                .map(|n| [n.x, n.y, n.width, n.height])
                .unwrap_or([0.0; 4]);
            let panel = stamps.first().map(|(_, b)| *b).unwrap_or([0.0; 4]);
            let trigger_box = find_trigger(&root).map(|el| el.layout_bounds).unwrap_or([0.0; 4]);
            (
                post,
                stamps,
                painted.node_paints,
                painted.text_nodes,
                title,
                panel,
                trigger_box,
            )
        };

        let mut dirties: Vec<PaintDirty> = pre;
        dirties.extend(post);
        for (key, bounds) in overlay_membership_paint_stamps(&[], &stamps) {
            dirties.push(PaintDirty {
                key,
                bounds,
                kind: PaintDirtyKind::Overlay,
            });
        }
        let overlay_dirties: Vec<_> = dirties
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::Overlay)
            .copied()
            .collect();
        for d in &overlay_dirties {
            if let Some(bounds) =
                overlay_replay_restore_bounds(d.key, d.bounds, &stamps, Some(panel))
            {
                dirties.push(PaintDirty {
                    key: d.key,
                    bounds,
                    kind: PaintDirtyKind::OverlayRestore,
                });
            }
        }
        crate::app::draw_cull::prepare_node_present_dirties_ex(
            &mut dirties,
            &paints,
            &[],
            &[],
            &[],
            &[],
            1.0,
            Some(panel),
            &stamps,
        );

        let mut clears = Vec::new();
        for dirty in &dirties {
            let pieces = match dirty.kind {
                PaintDirtyKind::Node | PaintDirtyKind::Region => {
                    crate::app::damage::page_scissors_excluding_portal_boxes(dirty.bounds, &stamps)
                }
                PaintDirtyKind::OverlayRestore => {
                    crate::app::draw_cull::scissors_keeping_stable_page_captions(
                        vec![dirty.bounds],
                        &paints,
                        &stamps,
                        Some(panel),
                    )
                }
                PaintDirtyKind::Overlay => continue,
            };
            for piece in pieces {
                clears.push((dirty.kind, piece));
            }
        }
        let (replay, _) = crate::app::draw_cull::overlay_grazing_caption_replay(
            &dirties,
            &paints,
            &stamps,
            Some(panel),
            &texts,
            1.0,
        );
        let hits = |r: [f32; 4], x: f32, y: f32| {
            x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
        };
        let title_y = title_box[1] + title_box[3] * 0.5;
        let prefix_x = (panel[0] - 80.0).max(title_box[0] + 4.0);
        let page = crate::app::draw_cull::page_runs_excluding_caption_replay(texts.clone(), &replay);
        let covers = |c: [f32; 4], x: f32, y: f32| hits(c, x, y);
        let prefix_kept = page.iter().any(|n| {
            n.text == TITLE
                && covers(
                    n.clip_bounds
                        .unwrap_or([n.x, n.y, n.width.max(0.0), n.height.max(0.0)]),
                    prefix_x,
                    title_y,
                )
        });
        let button_cx = trigger_box[0] + trigger_box[2] * 0.5;
        let button_cy = trigger_box[1] + trigger_box[3] * 0.5;
        let swatch_x = trigger_box[0] + 8.0;
        let punched = |x: f32| {
            clears.iter().any(|(kind, r)| {
                *kind == PaintDirtyKind::OverlayRestore && hits(*r, x, button_cy)
            })
        };
        let report = format!(
            "before_anchor={before_anchor:?}\n\
             title={title_box:?} panel={panel:?} trigger={trigger_box:?}\n\
             dirties={:?}\nclears={clears:?}\nreplay={:?}\npage={:?}\n\
             prefix=({prefix_x:.1},{title_y:.1}) kept={prefix_kept} center_punched={} swatch_punched={}",
            dirties
                .iter()
                .map(|d| format!("{:?} {}", d.kind, format!("[{:.0},{:.0} {:.0}x{:.0}]", d.bounds[0], d.bounds[1], d.bounds[2], d.bounds[3])))
                .collect::<Vec<_>>(),
            replay.iter().map(|n| (n.text.as_str(), n.clip_bounds)).collect::<Vec<_>>(),
            page.iter().filter(|n| n.text == TITLE).map(|n| n.clip_bounds).collect::<Vec<_>>(),
            punched(button_cx),
            punched(swatch_x),
        );
        assert!(
            title_box[2] > 8.0 && panel[2] > 8.0,
            "scene did not lay out\n{report}"
        );
        assert!(
            replay
                .iter()
                .all(|n| n.clip_bounds.is_none_or(|c| c[3] <= title_box[3] + 2.0)),
            "caption replay must stay within the title line, not the full viewport\n{report}"
        );
        assert!(
            prefix_kept,
            "title prefix left of the popover must stay in the page mesh\n{report}"
        );
        assert!(
            !punched(button_cx) && !punched(swatch_x),
            "overlay restore punches the trigger\n{report}"
        );
        let hairline = clears.iter().any(|(kind, r)| {
            *kind == PaintDirtyKind::OverlayRestore
                && (r[2] < 8.0 || r[3] < 8.0)
                && r[2].max(r[3]) > 32.0
        });
        assert!(!hairline, "overlay restore leaves a hairline clear\n{report}");

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn nested_format_select_first_open_seeds_enter_and_encodes() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::damage::{overlay_band_encode_flags, take_frame_paint_nodes};
        use crate::app::portal_overlay::{clear_overlay_stack, try_dismiss_outside_press};
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::find_by_template_id;
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, overlay_portal_stamp_key, LayoutTree, ATTR_HOISTED_PORTAL,
        };
        use crate::utils::floating_motion::{
            debug_floating_registry, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_hoisted_floating_css,
        };
        use crate::utils::popup::{
            arm_underlay_close, sync_floating_placement, ATTR_FLOATING_EXPAND_MEASURE,
        };

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let _ = take_frame_paint_nodes();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("gal-color").default_value("#3b82f6").into();
                *live_e.borrow_mut() = Element::new().w(800.0).h(600.0).child(picker);
            });
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn find_fmt_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t == "gal-color:fmt__floating_panel")
            {
                return Some(el);
            }
            el.children.iter().find_map(find_fmt_panel)
        }
        fn count_fmt_tids(el: &Element) -> usize {
            let here =
                el.template_id
                    .as_deref()
                    .is_some_and(|t| t == "gal-color:fmt__floating_panel") as usize;
            here + el.children.iter().map(count_fmt_tids).sum::<usize>()
        }
        fn dump_fmt(el: &Element) -> String {
            let Some(p) = find_fmt_panel(el) else {
                return format!(
                    "missing fmt panel; n_tid={} find_by_tid={}",
                    count_fmt_tids(el),
                    find_by_template_id(el, "gal-color:fmt__floating_panel").is_some()
                );
            };
            let anims: Vec<_> = p.style.animations.iter().map(|a| a.name.as_str()).collect();
            format!(
                "tid={:?} hoisted={} opacity={} bounds={:?} measuring={} anims={:?} n_tid={}",
                p.template_id,
                p.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1"),
                p.style.opacity,
                p.layout_bounds,
                p.attrs
                    .get(ATTR_FLOATING_EXPAND_MEASURE)
                    .map(String::as_str)
                    == Some("1"),
                anims,
                count_fmt_tids(el),
            )
        }
        fn present_pass(root: &mut Element) -> crate::layout::layout_tree::RenderResult {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let mut out = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            out
        }

        let picker_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:trigger").expect("picker trigger")
        };
        picker_click.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
        }

        let fmt_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:fmt:trigger").expect("format Select trigger")
        };
        fmt_click.borrow_mut()();
        drain_deferred_view_remounts();

        let (band_keys, nested_key, nested_in_stamps, encode_flags) = {
            let mut root = live.borrow_mut();
            // Two present passes: measure then seed. Event-loop Poll is not this seam.
            let _ = present_pass(&mut root);
            let _ = present_pass(&mut root);
            let dump = format!(
                "{} registry={}",
                dump_fmt(&root),
                debug_floating_registry(&root)
            );
            let panel = find_fmt_panel(&root)
                .unwrap_or_else(|| panic!("nested format menu must exist; {dump}"));
            let measuring = panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_MEASURE)
                .map(String::as_str)
                == Some("1");
            let has_enter = panel.style.animations.iter().any(|a| {
                a.name.contains("accordion-down") || a.name.starts_with("rui-expand-down-")
            });
            assert!(
                !measuring && (panel.style.opacity > 0.5 || has_enter),
                "nested format Select first present must seed enter (not stay opacity-0 measure); {dump}"
            );
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(999.0);
            assert!(
                mh < 2.0,
                "seeded enter must start at max-height≈0, not one item row; mh={mh}; {dump}"
            );
            crate::style::utility::set_frame_dt(0.016);
            let out = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.0);
            let dump = format!(
                "{} registry={}",
                dump_fmt(&root),
                debug_floating_registry(&root)
            );
            let panel = find_fmt_panel(&root)
                .unwrap_or_else(|| panic!("nested format menu must exist after grow; {dump}"));
            let nested_key = overlay_portal_stamp_key(panel);
            let stamps = crate::layout::layout_tree::hoisted_portal_stamps(&root);
            let nested_in_stamps = stamps.iter().any(|(k, _)| *k == nested_key);
            let band_keys: Vec<u64> = out
                .overlay_split
                .band_ranges(
                    out.instances.len(),
                    out.textured_draws.len(),
                    out.text_nodes.len(),
                    0,
                    0,
                )
                .iter()
                .map(|b| b.stamp_key)
                .collect();
            let remaining: Vec<u64> = stamps.iter().map(|(k, _)| *k).collect();
            let flags = overlay_band_encode_flags(&band_keys, &[nested_key], false, &remaining);
            (band_keys, nested_key, nested_in_stamps, flags)
        };
        assert!(
            nested_in_stamps,
            "seeded nested menu must be a paint stamp; key={nested_key} bands={band_keys:?}"
        );
        assert!(
            encode_flags.iter().any(|e| *e),
            "PartialLoad dirty={nested_key} must submit the nested menu; bands={band_keys:?} flags={encode_flags:?}"
        );

        arm_underlay_close();
        assert!(
            try_dismiss_outside_press(),
            "outside press must dismiss the nested format Select"
        );
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
            let dump = dump_fmt(&root);
            if let Some(panel) = find_fmt_panel(&root) {
                let has_exit = panel.style.animations.iter().any(|a| {
                    a.name.contains("accordion-up") || a.name.starts_with("rui-expand-up-")
                });
                assert!(
                    panel.style.opacity > 0.5 || has_exit,
                    "dismiss must not be the first time the menu becomes visible; {dump}"
                );
            }
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Accordion-up already shrank the format-Select chrome; HEX/RGB labels must not
    /// keep painting below that clip (leftover ink over the swatch grid).
    #[test]
    fn nested_format_select_exit_clips_labels_to_accordion() {
        check_nested_format_select_exit(false, false);
    }

    #[test]
    fn inline_format_select_close_leaves_no_painted_menu() {
        check_inline_format_select(false);
    }

    #[test]
    fn inline_format_select_upward_stays_next_to_trigger() {
        check_inline_format_select(true);
    }

    /// 6 format rows fit in `max-h-16rem`. Opening above with room must not treat the
    /// Expand accordion clip as a scroll viewport (that recenters HSLA and leaves a
    /// leftover offset — items look "scrolled" after settle).
    #[test]
    fn inline_format_select_upward_must_not_scroll_when_content_fits() {
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::app::scroll_support::{
            apply_pending_scroll_align, cancel_scroll_item_into_view,
        };
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            floating_motion_needs_relayout, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;
        use std::{cell::RefCell, rc::Rc};

        fn click(root: &Element, id: &str) -> Rc<RefCell<dyn FnMut()>> {
            let mut callback = None;
            crate::core::tree::walk_elements(root, |el| {
                if el.focus_id.as_deref() == Some(id) {
                    callback = el.on_click.clone();
                }
            });
            callback.expect("click target")
        }
        fn frame(root: &mut Element, tree: &mut LayoutTree) {
            let height = crate::layout::dim_length_px(root.style.size.height).unwrap();
            hoist_portal_subtrees(root);
            // Mirror AppState present: layout (sync inside) → flush idle hug →
            // relayout/sync again when Expand dirty. Old harness order (sync then
            // flush) hid the place-CAP rewrite that undoes content hug.
            let mut out = if tree.root_node.is_none() {
                tree.build_and_layout(root, 800.0, height, [-1.0, -1.0])
            } else {
                tree.build_and_layout_fast_ex(root, 800.0, height, [-1.0, -1.0], false, false, None)
            };
            flush_floating_motion_subtree(root);
            tick_hoisted_floating_css(root);
            if floating_motion_needs_relayout() || sync_floating_placement(root, 800.0, height) {
                mark_floating_panels_layout_dirty(root, tree);
                out = tree.relayout_overlay_follow_and_collect(
                    root,
                    800.0,
                    height,
                    [-1.0, -1.0],
                    false,
                    false,
                );
                flush_floating_motion_subtree(root);
            }
            let _ = out;
            // Mirror AppState: apply Select `scroll_selected` against live metrics.
            let rest = "inline-noscroll:fmt:menu_scroll";
            let mut vp = None;
            let mut content_h = 0.0_f32;
            crate::core::tree::walk_elements(root, |el| {
                if el.focus_id.as_deref()
                    == Some(
                        crate::core::scroll_viewport::focus_id_for_scroll_viewport(rest).as_str(),
                    )
                {
                    vp = Some(el.layout_bounds);
                }
                if el
                    .template_id
                    .as_deref()
                    .is_some_and(|t| t == "inline-noscroll:fmt__floating_panel")
                {
                    // Rows live under the scroll viewport; prefer CONTENT_H when set.
                    if let Some(h) = el
                        .attrs
                        .get(crate::utils::popup::ATTR_FLOATING_CONTENT_H)
                        .and_then(|s| s.parse::<f32>().ok())
                        .filter(|h| *h > 0.5)
                    {
                        content_h = h;
                    }
                }
            });
            if let Some(bounds) = vp {
                if content_h < 0.5 {
                    content_h = bounds[3].max(1.0);
                }
                let _ = apply_pending_scroll_align(rest, bounds, bounds[2], content_h, None);
            }
        }

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        cancel_scroll_item_into_view("inline-noscroll:fmt:menu_scroll");
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, ()) = create_root(|| {
            let live = Rc::clone(&live);
            create_effect(move || {
                // HSLA = last row; formerly scroll_selected(center) recentered wrongly
                // under accordion / place-CAP stretch — keep HSLA to catch regressions.
                let picker: Element = color_picker("inline-noscroll")
                    .variant(ColorPickerVariant::Inline)
                    .format(ColorFormat::Hsla)
                    .into();
                *live.borrow_mut() = Element::new().w(800.0).h(600.0).pt(200.0).child(picker);
            });
        });
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        let mut tree = LayoutTree::new();
        tree.set_collect_skip_allowed(true);
        frame(&mut live.borrow_mut(), &mut tree);
        let open_menu = click(&live.borrow(), "inline-noscroll:fmt:trigger");
        open_menu.borrow_mut()();
        drain_deferred_view_remounts();
        for _ in 0..24 {
            frame(&mut live.borrow_mut(), &mut tree);
            let sy = crate::core::reactivity::use_signal(
                &scroll_offset_y_signal_key("inline-noscroll:fmt:menu_scroll"),
                || 0.0_f32,
            )
            .peek();
            assert!(
                sy.abs() <= 1.0,
                "upward format menu must not scroll when 6 rows fit the place cap; sy={sy}"
            );
        }
        // Settled: place CAP (16rem) must not stretch ScrollArea into a scrollable
        // empty band — that is the inline-upward "items can scroll" bug vs floating.
        let mut panel_h = None;
        let mut content_attr = None;
        let mut vp_h = None;
        let mut scroll_content_h = None;
        let rest = "inline-noscroll:fmt:menu_scroll";
        let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(rest);
        crate::core::tree::walk_elements(&live.borrow(), |el| {
            if el.template_id.as_deref() == Some("inline-noscroll:fmt__floating_panel") {
                panel_h = Some(el.layout_bounds[3]);
                content_attr = el
                    .attrs
                    .get(crate::utils::popup::ATTR_FLOATING_CONTENT_H)
                    .and_then(|s| s.parse::<f32>().ok());
            }
            if el.focus_id.as_deref() == Some(vp_fid.as_str()) {
                vp_h = Some(el.layout_bounds[3]);
                // Scroll content is the viewport's laid-out children extent.
                scroll_content_h = Some(
                    el.children
                        .iter()
                        .map(|c| c.layout_bounds[3])
                        .fold(0.0_f32, f32::max),
                );
            }
        });
        let panel_h = panel_h.expect("format panel");
        let content_attr = content_attr.expect("CONTENT_H");
        let vp_h = vp_h.expect("menu scroll viewport");
        let scroll_content_h = scroll_content_h.unwrap_or(0.0);
        assert!(
            content_attr < 240.0,
            "6 format rows must not absorb the 16rem place CAP as CONTENT_H; content_h={content_attr}"
        );
        assert!(
            (panel_h - content_attr).abs() <= 12.0,
            "idle panel must hug content, not the place CAP: panel_h={panel_h} content_h={content_attr}"
        );
        let max_scroll = (scroll_content_h - vp_h).max(0.0);
        assert!(
            max_scroll <= 2.0,
            "settled upward format menu must not be wheel-scrollable; max_scroll={max_scroll} vp={vp_h} scroll_content={scroll_content_h} panel={panel_h}"
        );
        crate::style::utility::set_frame_dt(0.0);
        cancel_scroll_item_into_view("inline-noscroll:fmt:menu_scroll");
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    fn check_inline_format_select(upward: bool) {
        use crate::app::portal_overlay::{clear_overlay_stack, try_dismiss_outside_press};
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, hoisted_portal_stamps, LayoutTree,
        };
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, floating_motion_needs_relayout,
            flush_floating_motion_subtree, mark_floating_panels_layout_dirty,
            tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::{arm_underlay_close, sync_floating_placement};
        use std::{cell::RefCell, rc::Rc};

        fn click(root: &Element, id: &str) -> Rc<RefCell<dyn FnMut()>> {
            let mut callback = None;
            crate::core::tree::walk_elements(root, |el| {
                if el.focus_id.as_deref() == Some(id) {
                    callback = el.on_click.clone();
                }
            });
            callback.expect("click target")
        }
        fn frame(
            root: &mut Element,
            tree: &mut LayoutTree,
        ) -> crate::layout::layout_tree::RenderResult {
            let height = crate::layout::dim_length_px(root.style.size.height).unwrap();
            hoist_portal_subtrees(root);

            let mut out = if tree.root_node.is_none() {
                tree.build_and_layout(root, 800.0, height, [-1.0, -1.0])
            } else {
                tree.build_and_layout_fast_ex(root, 800.0, height, [-1.0, -1.0], false, false, None)
            };
            if sync_floating_placement(root, 800.0, height) {
                out = tree.relayout_overlay_follow_and_collect(
                    root,
                    800.0,
                    height,
                    [-1.0, -1.0],
                    false,
                    false,
                );
            }
            flush_floating_motion_subtree(root);
            tick_hoisted_floating_css(root);
            if floating_motion_needs_relayout() {
                mark_floating_panels_layout_dirty(root, tree);
                out = tree.relayout_overlay_follow_and_collect(
                    root,
                    800.0,
                    height,
                    [-1.0, -1.0],
                    false,
                    false,
                );
            }
            out
        }

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, ()) = create_root(|| {
            let live = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("inline-residue")
                    .variant(ColorPickerVariant::Inline)
                    .format(ColorFormat::Hsla)
                    .into();
                *live.borrow_mut() = Element::new()
                    .w(800.0)
                    .h(600.0)
                    .pt(if upward { 200.0 } else { 0.0 })
                    .child(picker);
            });
        });
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        let mut tree = LayoutTree::new();
        tree.set_collect_skip_allowed(true);
        let baseline = frame(&mut live.borrow_mut(), &mut tree);
        assert!(hoisted_portal_stamps(&live.borrow()).is_empty());
        for cycle in 0..2 {
            let open_menu = click(&live.borrow(), "inline-residue:fmt:trigger");
            open_menu.borrow_mut()();
            drain_deferred_view_remounts();
            for frame_i in 0..20 {
                frame(&mut live.borrow_mut(), &mut tree);
                if upward {
                    let mut trigger = None;
                    let mut panel = None;
                    crate::core::tree::walk_elements(&live.borrow(), |el| {
                        if el.focus_id.as_deref() == Some("inline-residue:fmt:trigger") {
                            trigger = Some(el.layout_bounds);
                        }
                        if el.template_id.as_deref() == Some("inline-residue:fmt__floating_panel") {
                            let mh = crate::layout::dim_length_px(el.style.max_size.height)
                                .unwrap_or(el.layout_bounds[3]);
                            panel = Some((el.layout_bounds, mh, el.attrs.clone()));
                        }
                    });
                    let Some(trigger) = trigger else { continue };
                    let Some((bounds, mh, attrs)) = panel else {
                        continue;
                    };
                    if mh < 0.5 {
                        continue;
                    }
                    assert!(
                        attrs.contains_key(crate::utils::popup::ATTR_FLOATING_ANCHOR_EDGE_Y),
                        "upward enter must seed anchor edge frame={frame_i}"
                    );
                    let gap = trigger[1] - bounds[1] - bounds[3].min(mh);
                    assert!(
                        (gap - crate::utils::popup::FLOATING_TRIGGER_GAP).abs() <= 1.5,
                        "upward enter must stay glued every frame (no content-height gap then snap): frame={frame_i} gap={gap} mh={mh} trigger={trigger:?} panel={bounds:?}"
                    );
                }
            }
            assert!(
                !hoisted_portal_stamps(&live.borrow()).is_empty(),
                "menu must open"
            );
            if upward {
                let mut trigger = None;
                let mut panel = None;
                crate::core::tree::walk_elements(&live.borrow(), |el| {
                    if el.focus_id.as_deref() == Some("inline-residue:fmt:trigger") {
                        trigger = Some(el.layout_bounds);
                    }
                    if el.template_id.as_deref() == Some("inline-residue:fmt__floating_panel") {
                        panel = Some((el.layout_bounds, el.attrs.clone()));
                    }
                });
                let trigger = trigger.expect("format trigger");
                let (bounds, attrs) = panel.expect("format menu");
                assert!(attrs.contains_key(crate::utils::popup::ATTR_FLOATING_ANCHOR_EDGE_Y));
                let gap = trigger[1] - bounds[1] - bounds[3];
                assert!((gap - crate::utils::popup::FLOATING_TRIGGER_GAP).abs() <= 1.0,
                    "upward menu must stay next to trigger: gap={gap} trigger={trigger:?} panel={bounds:?} attrs={attrs:?}");
            }
            arm_underlay_close();
            assert!(try_dismiss_outside_press());
            drain_deferred_view_remounts();
            for _ in 0..30 {
                tick_floating_presence();
                drain_pending_floating_unmounts();
                drain_deferred_view_remounts();
                let out = frame(&mut live.borrow_mut(), &mut tree);
                if hoisted_portal_stamps(&live.borrow()).is_empty() {
                    let start = out.overlay_split.instance_start.min(out.instances.len());
                    assert!(
                        out.instances[start..].iter().all(|i| i.color[3] < 0.01
                            && i.shadow_color[3] < 0.01
                            && i.border_color[3] < 0.01),
                        "hidden menu still paints cached chrome after its stamp disappeared"
                    );
                }
            }
            let closed = frame(&mut live.borrow_mut(), &mut tree);
            assert!(
                hoisted_portal_stamps(&live.borrow()).is_empty(),
                "closed menu leaves an overlay stamp, cycle={cycle}"
            );
            assert_eq!(
                closed.instances.len(),
                baseline.instances.len(),
                "closed menu leaves painted chrome, cycle={cycle}"
            );
        }
        crate::style::utility::set_frame_dt(0.0);
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn nested_format_select_upward_exit_keeps_bottom_anchored() {
        check_nested_format_select_exit(true, false);
    }

    #[test]
    fn nested_format_select_alpha_change_exit_tracks_trigger() {
        check_nested_format_select_exit(true, true);
    }

    #[test]
    fn nested_format_select_downward_alpha_change_exit_tracks_trigger() {
        check_nested_format_select_exit(false, true);
    }

    fn check_nested_format_select_exit(upward: bool, change_format: bool) {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::damage::take_frame_paint_nodes;
        use crate::app::portal_overlay::{clear_overlay_stack, try_dismiss_outside_press};
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, overlay_portal_stamp_key, LayoutTree, ATTR_HOISTED_PORTAL,
        };
        use crate::utils::floating_motion::{
            floating_motion_needs_relayout, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_hoisted_floating_css,
        };
        use crate::utils::popup::{
            arm_underlay_close, sync_floating_placement, ATTR_FLOATING_EXPAND_MEASURE,
        };

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let _ = take_frame_paint_nodes();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("gal-color").default_value("#3b82f6").into();
                *live_e.borrow_mut() = Element::new()
                    .w(800.0)
                    .h(if upward { 360.0 } else { 600.0 })
                    .child(picker);
            });
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn find_fmt_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t == "gal-color:fmt__floating_panel")
            {
                return Some(el);
            }
            el.children.iter().find_map(find_fmt_panel)
        }
        fn find_fmt_panel_mut<'a>(el: &'a mut Element) -> Option<&'a mut Element> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t == "gal-color:fmt__floating_panel")
            {
                return Some(el);
            }
            el.children.iter_mut().find_map(find_fmt_panel_mut)
        }
        fn present_pass(root: &mut Element) -> crate::layout::layout_tree::RenderResult {
            let height = crate::layout::dim_length_px(root.style.size.height).unwrap();
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let mut out = tree.build_and_layout(root, 800.0, height, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, height) {
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    height,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let _ = flush_floating_motion_subtree(root);
            let _ = tick_hoisted_floating_css(root);
            // Match AppState: Expand accordion geometry relayouts; Fade opacity does not.
            if floating_motion_needs_relayout() {
                mark_floating_panels_layout_dirty(root, &mut tree);
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    height,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            out
        }

        let picker_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:trigger").expect("picker trigger")
        };
        picker_click.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
        }

        let fmt_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:fmt:trigger").expect("format Select trigger")
        };
        fmt_click.borrow_mut()();
        drain_deferred_view_remounts();

        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
            let _ = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.016);
            for _ in 0..16 {
                let _ = present_pass(&mut root);
            }
            crate::style::utility::set_frame_dt(0.0);
            let panel = find_fmt_panel(&root).expect("format menu after enter");
            let measuring = panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_MEASURE)
                .map(String::as_str)
                == Some("1");
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                !measuring && mh > 80.0,
                "enter must settle past one item row before exit; mh={mh} bounds={:?} hoisted={}",
                panel.layout_bounds,
                panel.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1"),
            );
        }

        let before_exit = find_fmt_panel(&live.borrow()).unwrap().layout_bounds;
        if upward {
            assert!(
                find_fmt_panel(&live.borrow())
                    .unwrap()
                    .attrs
                    .contains_key(crate::utils::popup::ATTR_FLOATING_ANCHOR_EDGE_Y),
                "fixture must open upward: {before_exit:?}"
            );
        } else {
            assert!(
                !find_fmt_panel(&live.borrow())
                    .unwrap()
                    .attrs
                    .contains_key(crate::utils::popup::ATTR_FLOATING_ANCHOR_EDGE_Y),
                "fixture must open downward: {before_exit:?}"
            );
        }
        if change_format {
            let click = find_click(&live.borrow(), "gal-color:fmt:opt:hex8").unwrap();
            click.borrow_mut()();
        } else {
            arm_underlay_close();
            assert!(
                try_dismiss_outside_press(),
                "outside press must dismiss the nested format Select"
            );
        }
        drain_deferred_view_remounts();

        let (out, clip_bottom, dump) = {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
            if upward || change_format {
                let mut previous = before_exit;
                for frame in 0..10 {
                    let b = find_fmt_panel(&root).unwrap().layout_bounds;
                    let mut trigger_bounds = [0.0_f32; 4];
                    let mut found_trigger = false;
                    crate::core::tree::walk_elements(&root, |el| {
                        if el.focus_id.as_deref() == Some("gal-color:fmt:trigger") {
                            trigger_bounds = el.layout_bounds;
                            found_trigger = true;
                        }
                    });
                    let expect_edge = if upward {
                        let mut edge = before_exit[1] + before_exit[3];
                        if change_format && found_trigger {
                            edge = trigger_bounds[1] - crate::utils::popup::FLOATING_TRIGGER_GAP;
                        }
                        edge
                    } else {
                        // Downward: panel top sticks to trigger bottom + gap.
                        assert!(
                            found_trigger,
                            "format trigger must stay mounted during exit"
                        );
                        assert!(
                            !find_fmt_panel(&root)
                                .unwrap()
                                .attrs
                                .contains_key(crate::utils::popup::ATTR_FLOATING_ANCHOR_EDGE_Y),
                            "downward exit must not flip above"
                        );
                        trigger_bounds[1]
                            + trigger_bounds[3]
                            + crate::utils::popup::FLOATING_TRIGGER_GAP
                    };
                    if upward {
                        assert!(
                            (b[1] + b[3] - expect_edge).abs() < 1.0,
                            "exit bottom jumped frame={frame}: before={before_exit:?} now={b:?} edge={expect_edge}"
                        );
                    } else {
                        assert!(
                            (b[1] - expect_edge).abs() < 1.0,
                            "exit top stuck at pre-alpha position frame={frame}: before={before_exit:?} now={b:?} expect_top={expect_edge} trigger={trigger_bounds:?}"
                        );
                    }
                    assert!(
                        (change_format || !upward || b[1] >= previous[1] - 0.5)
                            && b[3] <= previous[3] + 0.5,
                        "exit reversed frame={frame}: previous={previous:?} now={b:?}"
                    );
                    previous = b;
                    crate::style::utility::set_frame_dt(0.016);
                    let _ = present_pass(&mut root);
                }
                crate::style::utility::set_frame_dt(0.0);
            }
            if !upward && !change_format {
                let panel = find_fmt_panel_mut(&mut root).expect("format menu after dismiss");
                for _ in 0..10 {
                    let _ = panel.tick_css_animations(0.016);
                }
            }
            crate::style::utility::set_frame_dt(0.016);
            let out = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.0);
            let panel = find_fmt_panel(&root).expect("format menu during accordion-up");
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            let has_exit =
                panel.style.animations.iter().any(|a| {
                    a.name.contains("accordion-up") || a.name.starts_with("rui-expand-up-")
                });
            assert!(
                has_exit && mh > 0.5 && mh < 80.0,
                "mid-exit must still be accordion-up with a short clip; mh={mh} bounds={:?} anims={:?}",
                panel.layout_bounds,
                panel
                    .style
                    .animations
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>(),
            );
            let nested_key = overlay_portal_stamp_key(panel);
            let stamps = crate::layout::layout_tree::hoisted_portal_stamps(&root);
            let stamp = stamps
                .iter()
                .find(|(k, _)| *k == nested_key)
                .map(|(_, b)| *b)
                .unwrap_or(panel.layout_bounds);
            let clip_bottom = stamp[1] + stamp[3];
            let labels = ["HEX", "HEX8", "RGB", "RGBA", "HSL", "HSLA"];
            let texts: Vec<String> = out
                .text_nodes
                .iter()
                .filter(|t| labels.iter().any(|l| t.text == *l))
                .map(|t| {
                    let pb = crate::app::draw_cull::text_paint_bounds(t);
                    format!(
                        "{} y={:.1} h={:.1} clip={:?} paint={:?}",
                        t.text, t.y, t.height, t.clip_bounds, pb
                    )
                })
                .collect();
            let dump = format!(
                "mh={mh:.1} bounds={:?} stamp={:?} clip_bottom={clip_bottom:.1} texts=[{}]",
                panel.layout_bounds,
                stamp,
                texts.join("; ")
            );
            (out, clip_bottom, dump)
        };

        let labels = ["HEX", "HEX8", "RGB", "RGBA", "HSL", "HSLA"];
        let mut saw_format = false;
        for t in &out.text_nodes {
            if t.template_id.as_deref() == Some("gal-color:fmt:value")
                || !labels.iter().any(|l| t.text == *l)
            {
                continue;
            }
            saw_format = true;
            let pb = crate::app::draw_cull::text_paint_bounds(t);
            if pb[2] <= 0.0 || pb[3] <= 0.0 {
                continue;
            }
            assert!(
                pb[1] + pb[3] <= clip_bottom + 2.0,
                "format label {:?} must not paint below accordion chrome\n{dump}",
                t.text
            );
        }
        assert!(
            saw_format,
            "mid-exit must still collect the visible format row\n{dump}"
        );

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Selecting a format remounts ColorPicker (fmt_str.get). The nested Select must
    /// leave the overlay stack, keep the popover on-screen, and open again.
    #[test]
    fn nested_format_select_item_click_must_not_orphan_menu() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::damage::take_frame_paint_nodes;
        use crate::app::portal_overlay::{clear_overlay_stack, overlay_stack_depth_for_test};
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::{sync_floating_placement, ATTR_FLOATING_EXPAND_MEASURE};

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        let _ = take_frame_paint_nodes();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let picker: Element = color_picker("gal-color").default_value("#3b82f6").into();
                *live_e.borrow_mut() = Element::new().w(800.0).h(600.0).child(picker);
            });
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn find_fmt_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t == "gal-color:fmt__floating_panel")
            {
                return Some(el);
            }
            el.children.iter().find_map(find_fmt_panel)
        }
        fn find_picker_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                && el
                    .template_id
                    .as_deref()
                    .is_some_and(|t| t == "gal-color__floating_panel")
            {
                return Some(el);
            }
            el.children.iter().find_map(find_picker_panel)
        }
        fn present_pass(root: &mut Element) -> crate::layout::layout_tree::RenderResult {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let mut out = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                out = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            out
        }
        fn fmt_label_paint(
            out: &crate::layout::layout_tree::RenderResult,
        ) -> Vec<(String, [f32; 4])> {
            let labels = ["HEX", "HEX8", "RGB", "RGBA", "HSL", "HSLA"];
            out.text_nodes
                .iter()
                .filter(|t| labels.iter().any(|l| t.text == *l))
                .map(|t| (t.text.clone(), crate::app::draw_cull::text_paint_bounds(t)))
                .filter(|(_, b)| b[2] > 0.0 && b[3] > 0.0)
                .collect()
        }

        let picker_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:trigger").expect("picker trigger")
        };
        picker_click.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
        }
        let depth_picker = overlay_stack_depth_for_test();
        assert!(
            depth_picker >= 1,
            "ColorPicker popover must be on the overlay stack; depth={depth_picker}"
        );

        let fmt_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:fmt:trigger").expect("format Select trigger")
        };
        fmt_click.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
            let _ = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.016);
            for _ in 0..16 {
                let _ = present_pass(&mut root);
            }
            crate::style::utility::set_frame_dt(0.0);
        }
        let depth_open = overlay_stack_depth_for_test();
        assert!(
            depth_open > depth_picker,
            "format Select must stack above ColorPicker; picker={depth_picker} open={depth_open}"
        );

        let rgb_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:fmt:opt:rgb").expect("RGB format item")
        };
        rgb_click.borrow_mut()();
        drain_deferred_view_remounts();

        crate::style::utility::set_frame_dt(0.016);
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
        }
        for _ in 0..20 {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        let _ = tick_floating_presence();
        drain_pending_floating_unmounts();
        drain_deferred_view_remounts();
        let (out, picker_bounds, fmt_alive, dump) = {
            let mut root = live.borrow_mut();
            let out = present_pass(&mut root);
            let picker = find_picker_panel(&root).expect("ColorPicker popover after format click");
            let fmt = find_fmt_panel(&root);
            let fmt_alive = fmt.is_some_and(|p| {
                p.style.opacity > 0.05
                    && crate::layout::dim_length_px(p.style.max_size.height)
                        .unwrap_or(p.layout_bounds[3])
                        > 8.0
                    && p.attrs
                        .get(ATTR_FLOATING_EXPAND_MEASURE)
                        .map(String::as_str)
                        != Some("1")
            });
            let labels = fmt_label_paint(&out);
            let dump = format!(
                "picker={:?} fmt={:?} depth={} labels={:?} measuring={}",
                picker.layout_bounds,
                fmt.map(|p| (p.layout_bounds, p.style.opacity)),
                overlay_stack_depth_for_test(),
                labels,
                fmt.and_then(|p| p
                    .attrs
                    .get(ATTR_FLOATING_EXPAND_MEASURE)
                    .map(String::as_str))
                    == Some("1"),
            );
            (out, picker.layout_bounds, fmt_alive, dump)
        };

        assert!(
            picker_bounds[2] > 80.0
                && picker_bounds[3] > 80.0
                && picker_bounds[0] >= -1.0
                && picker_bounds[1] >= -1.0
                && picker_bounds[0] + picker_bounds[2] <= 801.0
                && picker_bounds[1] + picker_bounds[3] <= 601.0,
            "ColorPicker popover must stay on-screen after picking RGB; {dump}"
        );
        assert!(
            !fmt_alive,
            "format menu must finish exit after picking an item (not linger as a detached list); {dump}"
        );
        let depth_after = overlay_stack_depth_for_test();
        assert_eq!(
            depth_after, depth_picker,
            "nested Select must leave the overlay stack after pick; {dump}"
        );
        for (text, b) in fmt_label_paint(&out) {
            if text == "HEX" || text == "RGB" {
                continue;
            }
            let below_picker = b[1] > picker_bounds[1] + picker_bounds[3] + 8.0;
            assert!(
                !below_picker,
                "format label {text:?} must not float below the ColorPicker chrome; paint={b:?}; {dump}"
            );
        }

        let reopen = {
            let tree = live.borrow();
            find_click(&tree, "gal-color:fmt:trigger").expect("format trigger after pick")
        };
        reopen.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            let _ = present_pass(&mut root);
            let _ = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.016);
            let _ = present_pass(&mut root);
            crate::style::utility::set_frame_dt(0.0);
            let panel = find_fmt_panel(&root);
            let dump = format!(
                "reopen depth={} panel={:?} opacity={:?}",
                overlay_stack_depth_for_test(),
                panel.map(|p| p.layout_bounds),
                panel.map(|p| p.style.opacity),
            );
            assert!(
                panel.is_some_and(|p| p.style.opacity > 0.5 || {
                    p.style.animations.iter().any(|a| {
                        a.name.contains("accordion-down") || a.name.starts_with("rui-expand-down-")
                    })
                }),
                "format Select must open again after picking RGB; {dump}"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn color_picker_popover_blocks_hover_on_datepicker_trigger_below() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        use crate::app::pointer_events::hit_path;
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::app::tree_hit::{clear_hover_tracking, update_hovers};
        use crate::components::button::button;
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL};
        use crate::primitives::interactive::interactive;
        use crate::utils::floating_motion::{
            flush_floating_motion_subtree, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();
        clear_hover_tracking();

        let below_hover = Rc::new(Cell::new(false));
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            let below_hover = Rc::clone(&below_hover);
            create_effect(move || {
                let picker: Element = color_picker("gal-color").default_value("#3b82f6").into();
                let below: Element = interactive(
                    button("gal-date:trigger")
                        .variant("outline")
                        .w(800.0)
                        .h(600.0)
                        .text("Pick a date")
                        .into(),
                )
                .on_hover({
                    let below_hover = Rc::clone(&below_hover);
                    move |h| below_hover.set(h)
                })
                .into();
                *live_e.borrow_mut() = Element::new().w(800.0).h(600.0).child(below).child(picker);
            });
        });

        drain_deferred_view_remounts();
        fn find_trigger_click(
            el: &Element,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some("gal-color:trigger") {
                return el.on_click.clone();
            }
            el.children.iter().find_map(find_trigger_click)
        }
        let click = {
            let tree = live.borrow();
            find_trigger_click(&tree).expect("ColorPicker trigger click")
        };
        click.borrow_mut()();
        drain_deferred_view_remounts();

        fn find_hoisted_panel<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                && el
                    .template_id
                    .as_deref()
                    .is_some_and(|t| t.contains("gal-color") && t.ends_with("__floating_panel"))
            {
                return Some(el);
            }
            el.children.iter().find_map(find_hoisted_panel)
        }

        let (px, py, panel_bounds, pe) = {
            let mut root = live.borrow_mut();
            let _ = hoist_portal_subtrees(&mut root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(&mut root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    &mut root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let _ = flush_floating_motion_subtree(&mut root);
            let _ = tick_hoisted_floating_css(&mut root);
            let _ = tree.build_and_layout_fast_ex(
                &mut root,
                800.0,
                600.0,
                [0.0, 0.0],
                false,
                false,
                None,
            );
            let panel = find_hoisted_panel(&root).expect("hoisted ColorPicker panel");
            let b = panel.layout_bounds;
            assert!(
                b[2] > 8.0 && b[3] > 8.0,
                "popover must have a box; bounds={b:?}"
            );
            let px = b[0] + b[2] * 0.5;
            let py = b[1] + b[3] * 0.5;
            let pe = panel.pointer_events_mode();
            let path = hit_path(&root, px, py);
            assert!(
                path.iter()
                    .all(|e| e.focus_id.as_deref() != Some("gal-date:trigger")),
                "hit_path must not reach DatePicker under the popover; path={:?}",
                path.iter()
                    .map(|e| (e.focus_id.clone(), e.portal_key.clone()))
                    .collect::<Vec<_>>()
            );
            let mut cbs = Vec::new();
            update_hovers(&root, px, py, [0.0, 0.0], &mut cbs);
            for (cb, is_hover) in cbs {
                (*cb.borrow_mut())(is_hover);
            }
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut root,
                [px, py],
                "",
                false,
                800.0,
                600.0,
                &mut tree,
                &mut impact,
            );
            fn find_date_trigger(el: &Element) -> Option<&Element> {
                if el.focus_id.as_deref() == Some("gal-date:trigger") {
                    return Some(el);
                }
                el.children.iter().find_map(find_date_trigger)
            }
            let date = find_date_trigger(&root).expect("date trigger");
            assert_ne!(
                date.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
                Some(true),
                "DatePicker outline hover:bg must not apply under ColorPicker popover at ({px:.1},{py:.1})"
            );
            (px, py, b, pe)
        };

        assert!(
            !below_hover.get(),
            "ColorPicker popover must absorb hover at ({px:.1},{py:.1}) panel={panel_bounds:?} pe={pe:?}; DatePicker trigger underneath must not hover"
        );

        clear_hover_tracking();
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Gallery ColorPicker variants Tabs: switching unmounts the open popover's
    /// owner. Overlay `body()` used to run (and peek `fmt_sig`) before `open`.
    #[test]
    fn color_picker_tab_unmount_with_open_overlay_must_not_peek_disposed() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::{clear_overlay_stack, try_dismiss_outside_press};
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::components::tabs::{tabs_content, tabs_scope};
        use crate::core::reactivity::{component_root, create_effect, create_root, use_signal};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::{arm_underlay_close, sync_floating_placement};

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, tab) = create_root(|| {
            let tab = use_signal("gal-cp-variants__tabs", || "chrome".to_string());
            let live_e = Rc::clone(&live);
            let tab_e = tab;
            create_effect(move || {
                *live_e.borrow_mut() = tabs_scope(
                    "gal-cp-variants",
                    Some(tab_e),
                    "chrome",
                    "horizontal",
                    false,
                    || {
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag = frag.children(vec![
                            tabs_content("chrome", || {
                                // Gallery `<ColorPicker>` is `#[component]` → component_root.
                                // Builder `.into()` in the TabsContent effect left `out` alive
                                // after hide; the hex Input still painted the disposed Signal.
                                component_root(|| {
                                    color_picker("gal-cp-chrome")
                                        .variant(ColorPickerVariant::Chrome)
                                        .default_value("#0693E3")
                                        .into()
                                })
                            }),
                            tabs_content("github", || {
                                component_root(|| {
                                    color_picker("gal-cp-github")
                                        .variant(ColorPickerVariant::Github)
                                        .default_value("#1273DE")
                                        .into()
                                })
                            }),
                        ]);
                        frag
                    },
                );
            });
            tab
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn present_pass(root: &mut Element) {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
        }

        {
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        let chrome_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-cp-chrome:trigger").expect("chrome ColorPicker trigger")
        };
        chrome_click.borrow_mut()();
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            present_pass(&mut root);
            present_pass(&mut root);
        }

        // Gallery chip click: pointer-down outside-dismiss, then TabsTrigger set.
        arm_underlay_close();
        let _ = try_dismiss_outside_press();
        tab.set("github".into());
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..8 {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);

        {
            let tree = live.borrow();
            assert!(
                find_click(&tree, "gal-cp-github:trigger").is_some(),
                "github variant must mount after tab switch"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Regression: orphan-exit pump must not treat a live `open=true` overlay as
    /// disposed. Gallery symptom: trigger click does nothing; tab away/back
    /// flashes the leftover panel then immediately fades it out.
    #[test]
    fn color_picker_tabs_trigger_open_stays_open_after_presence_tick() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::components::tabs::{tabs_content, tabs_scope};
        use crate::core::reactivity::{component_root, create_effect, create_root, use_signal};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, tab) = create_root(|| {
            let tab = use_signal("gal-cp-openstay__tabs", || "chrome".to_string());
            let live_e = Rc::clone(&live);
            let tab_e = tab;
            create_effect(move || {
                *live_e.borrow_mut() = tabs_scope(
                    "gal-cp-openstay",
                    Some(tab_e),
                    "chrome",
                    "horizontal",
                    false,
                    || {
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag = frag.children(vec![
                            tabs_content("chrome", || {
                                component_root(|| {
                                    color_picker("gal-cp-openstay-chrome")
                                        .variant(ColorPickerVariant::Chrome)
                                        .default_value("#0693E3")
                                        .into()
                                })
                            }),
                            tabs_content("github", || {
                                component_root(|| {
                                    color_picker("gal-cp-openstay-github")
                                        .variant(ColorPickerVariant::Github)
                                        .default_value("#1273DE")
                                        .into()
                                })
                            }),
                        ]);
                        frag
                    },
                );
            });
            tab
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        fn has_anim(el: &Element, name: &str) -> bool {
            el.style.animations.iter().any(|a| a.name == name)
                || el.children.iter().any(|c| has_anim(c, name))
        }
        fn present_pass(root: &mut Element) {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
        }
        fn event_loop_pass(root: &mut Element) {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            present_pass(root);
        }

        {
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        let chrome_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-cp-openstay-chrome:trigger").expect("chrome trigger")
        };
        chrome_click.borrow_mut()();
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        {
            let tree = live.borrow();
            let panel = find_tid(&tree, "gal-cp-openstay-chrome__floating_panel")
                .expect("trigger click must hoist the ColorPicker panel");
            assert!(
                !has_anim(panel, "fade-out-0"),
                "live open overlay must not be orphan-exited on presence tick"
            );
            assert!(
                panel.style.opacity > 0.05,
                "open panel must not stay stuck at opacity 0 (got {})",
                panel.style.opacity
            );
        }

        // Switch away and back immediately — named presence is still Exiting.
        tab.set("github".into());
        drain_deferred_view_remounts();
        {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        tab.set("chrome".into());
        drain_deferred_view_remounts();
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "gal-cp-openstay-chrome__floating_panel").is_none(),
                "returning to the tab must not auto-open a leftover overlay"
            );
        }

        let chrome_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-cp-openstay-chrome:trigger")
                .expect("chrome trigger after return")
        };
        chrome_click.borrow_mut()();
        drain_deferred_view_remounts();
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            let tree = live.borrow();
            let panel = find_tid(&tree, "gal-cp-openstay-chrome__floating_panel")
                .expect("trigger click after tab roundtrip must open the panel");
            assert!(
                !has_anim(panel, "fade-out-0"),
                "reopened overlay must stay open, not immediately fade out"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Gallery forms: enter the page, click a Tabs-hosted ColorPicker variant
    /// trigger (`#[component]` + `tabs_content`, no tab switch). Overlay must open.
    #[test]
    fn color_picker_first_trigger_click_opens_before_tab_switch() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::components::tabs::{tabs_content, tabs_scope};
        use crate::core::reactivity::{create_effect, create_root, isolate_reads};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, ()) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                *live_e.borrow_mut() = isolate_reads(|| {
                    tabs_scope(
                        "gal-cp-variants",
                        None,
                        "sketch",
                        "horizontal",
                        false,
                        || {
                            let mut frag = Element::new();
                            frag.set_attr("rui_fragment", "1");
                            frag = frag.children(vec![
                                tabs_content("sketch", || {
                                    ColorPicker(ColorPickerProps {
                                        id: "gal-cp-sketch".into(),
                                        value: Some("#f47373".into()),
                                        variant: Some("sketch".into()),
                                        ..Default::default()
                                    })
                                }),
                                tabs_content("chrome", || {
                                    ColorPicker(ColorPickerProps {
                                        id: "gal-cp-chrome".into(),
                                        value: Some("#0693E3".into()),
                                        variant: Some("chrome".into()),
                                        ..Default::default()
                                    })
                                }),
                            ]);
                            frag
                        },
                    )
                });
            });
        });
        drain_deferred_view_remounts();

        fn find_click(
            el: &Element,
            fid: &str,
        ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>> {
            if el.focus_id.as_deref() == Some(fid) {
                return el.on_click.clone();
            }
            el.children.iter().find_map(|c| find_click(c, fid))
        }
        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        fn has_anim(el: &Element, name: &str) -> bool {
            el.style.animations.iter().any(|a| a.name == name)
                || el.children.iter().any(|c| has_anim(c, name))
        }
        fn present_pass(root: &mut Element) {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
        }
        fn event_loop_pass(root: &mut Element) {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            present_pass(root);
        }

        {
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        for _ in 0..4 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "gal-cp-sketch__floating_panel").is_none(),
                "idle ticks before click must not auto-open the overlay"
            );
        }

        let sketch_click = {
            let tree = live.borrow();
            find_click(&tree, "gal-cp-sketch:trigger").expect("sketch trigger")
        };
        sketch_click.borrow_mut()();
        drain_deferred_view_remounts();

        crate::style::utility::set_frame_dt(0.016);
        for _ in 0..6 {
            let mut root = live.borrow_mut();
            event_loop_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            use crate::utils::floating_motion::debug_floating_registry;
            let tree = live.borrow();
            let panel = find_tid(&tree, "gal-cp-sketch__floating_panel")
                .expect("clicking a Tabs-hosted ColorPicker variant trigger must hoist the panel");
            let anims: Vec<&str> = panel
                .style
                .animations
                .iter()
                .map(|a| a.name.as_str())
                .collect();
            let dump = debug_floating_registry(&tree);
            assert!(
                !has_anim(panel, "fade-out-0"),
                "live open overlay must not play fade-out on first open; anims={anims:?} registry={dump}"
            );
            assert!(
                panel.style.opacity > 0.05,
                "open panel must not stay stuck at opacity 0 (got {}, anims={anims:?}, registry={dump})",
                panel.style.opacity
            );
            assert!(
                panel.layout_bounds[2] > 20.0 && panel.layout_bounds[3] > 20.0,
                "open panel must have a visible box, got {:?}",
                panel.layout_bounds
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}
