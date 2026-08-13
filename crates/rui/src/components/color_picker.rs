//! # ColorPicker
//!
//! 颜色选择 MVP。对应 HTML `input type="color"`。
//! 触发器显示色块 + `#RRGGBB`；打开面板可选预设色板或点选写入值。
//!
//! ## 用法
//! ```ignore
//! <ColorPicker id="accent" value="#3b82f6" />
//! ```

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_component_live;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, CursorKind, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    apply_stored_floating_place, mark_floating, panel_block_clicks, PopupPlacement,
};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

fn trigger_focus_id(id: &str) -> String {
    format!("{id}:trigger")
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

/// ColorPicker 触发器壳（样式本文件内维护，勿再抽公共 stub）。
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

const PRESET_SWATCHES: &[&str] = &[
    "#000000", "#ffffff", "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6",
    "#8b5cf6", "#ec4899", "#64748b", "#0f172a",
];

fn parse_hex_swatch(value: &str) -> [f32; 4] {
    let s = value.trim().trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
        }
    }
    tokens::primary()
}

fn normalize_hex(raw: &str) -> String {
    let s = raw.trim().trim_start_matches('#');
    if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{}", s.to_ascii_lowercase())
    } else {
        "#3b82f6".into()
    }
}

/// HTML `input type="color"`：色块 + 十六进制 + 可打开色板。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ColorPicker(
    id: String,
    value: Option<String>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    color_picker(&id)
        .default_value(value.unwrap_or_else(|| "#3b82f6".into()))
        .disabled(disabled)
        .into()
}

pub struct ColorPickerBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    disabled: bool,
}

pub fn color_picker(id: impl Into<String>) -> ColorPickerBuilder {
    ColorPickerBuilder {
        id: id.into(),
        value: None,
        default_value: "#3b82f6".into(),
        disabled: false,
    }
}

impl ColorPickerBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = normalize_hex(&v.into());
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
    disabled: bool,
}

fn build_color_picker_view(
    props: ColorPickerRenderProps,
    open: Signal<bool>,
    val: Signal<String>,
) -> Element {
    let ColorPickerRenderProps { id, disabled } = props;
    let _ = open.get();
    let hex = normalize_hex(&val.get());
    let swatch_color = parse_hex_swatch(&hex);
    let mut swatch: Element = r#box().w("1.25rem").h("1.25rem").rounded("0.25rem").into();
    swatch = swatch.bg(swatch_color);
    swatch.style.border_width = 1.0;
    swatch.style.border_color = parse_color("border");
    let main = trigger_label(&hex);
    let mut trigger = picker_trigger(&id, swatch, main, disabled);
    if !disabled {
        let open_t = open;
        trigger = trigger.on_click(move || open_t.set(!open_t.peek()));
    }

    let mut view: Element = vstack().gap("0").relative(true).child(trigger).into();
    if disabled {
        return view;
    }

    if let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) {
        if open.peek() {
            let mut grid: Element = hstack().w("100%").gap("0.375rem").wrap("wrap").into();
            for (i, sw) in PRESET_SWATCHES.iter().enumerate() {
                let v_sig = val;
                let o_sig = open;
                let hex_v = (*sw).to_string();
                let color = parse_hex_swatch(sw);
                let mut cell: Element = r#box().w("1.5rem").h("1.5rem").rounded("0.25rem").into();
                cell = cell.bg(color);
                cell.style.border_width = 1.0;
                cell.style.border_color = parse_color("border");
                let bid = format!("{id}:sw{i}");
                let chip: Element = button(&bid)
                    .variant("ghost")
                    .size("sm")
                    .on_click(move || {
                        v_sig.set(hex_v.clone());
                        o_sig.set(false);
                    })
                    .child(cell)
                    .into();
                grid = grid.child(chip);
            }

            let hex_label = build_text_node(
                &hex,
                TextNodeConfig {
                    size_px: 12.0,
                    weight: 500,
                    ..Default::default()
                },
            );

            // HSV-ish quick tones: darken / lighten of current via simple presets around primary
            let mut tones: Element = hstack().gap("0.25rem").wrap("wrap").into();
            for (i, t) in ["#1e3a8a", "#2563eb", "#60a5fa", "#bfdbfe"]
                .iter()
                .enumerate()
            {
                let v_sig = val;
                let o_sig = open;
                let hex_v = (*t).to_string();
                let bid = format!("{id}:tone{i}");
                let row: Element = button(&bid)
                    .variant("outline")
                    .size("sm")
                    .on_click(move || {
                        v_sig.set(hex_v.clone());
                        o_sig.set(false);
                    })
                    .text((*t).to_string())
                    .into();
                tones = tones.child(row);
            }

            let mut panel: Element = vstack()
                .gap("0.5rem")
                .p("0.75rem")
                .w("14rem")
                .child(hex_label)
                .child(grid)
                .child(tones)
                .into();
            panel = panel
                .bg(tokens::background())
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border())
                .shadow([0.0, 0.0, 0.0, 0.12], 8.0)
                .overflow_x("hidden")
                .overflow_y("auto")
                .absolute();
            mark_floating(
                &mut panel,
                &id,
                &trigger_focus_id(&id),
                PopupPlacement::BottomStart,
            );
            panel.set_attr("rui_floating_match_trigger", "0");
            panel.set_attr("rui_floating_w", "14rem");
            panel.set_attr("rui_floating_max_h", "20rem");
            apply_stored_floating_place(&mut panel, &id);
            panel = panel_block_clicks(panel, oz.panel);
            view = view.child(panel);
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
            disabled,
        } = builder;
        let open = use_signal(format!("{id}__cp_open").as_str(), || false);
        let initial = default_value;
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__cp_value").as_str(), move || initial.clone())
        });
        let view_id = format!("{id}__cp_view");
        let props = ColorPickerRenderProps { id, disabled };
        reactive_component_live(view_id, props, move |p| {
            build_color_picker_view(p, open, val)
        })
    }
}
