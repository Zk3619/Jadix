//! 占位选择器共用的触发器壳（非公开 API）。
//! 各业务选择器实现完整后可逐步弃用本模块。

use crate::core::tree::{parse_color, parse_text_color, CursorKind, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};

pub(crate) fn stub_label(text: &str, muted: bool) -> Element {
    let color = if muted {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 0.875 * 16.0,
            weight: 400,
            color,
            ..Default::default()
        },
    )
}

fn stub_badge(text: &str) -> Element {
    let mut el: Element = r#box()
        .px("0.375rem")
        .py("0.125rem")
        .rounded("0.25rem")
        .into();
    el = el.bg(tokens::muted());
    el.child(build_text_node(
        text,
        TextNodeConfig {
            size_px: 0.625 * 16.0,
            weight: 500,
            color: parse_text_color("muted-foreground"),
            ..Default::default()
        },
    ))
}

/// 触发器 `focus_id`（与 Select `{id}:trigger` 对齐，供 `mark_floating` 锚定）。
pub(crate) fn stub_trigger_focus_id(id: &str) -> String {
    format!("{id}:trigger")
}

/// 与 Input 同高的触发器壳。
pub(crate) fn stub_trigger(id: &str, leading: Element, main: Element, disabled: bool) -> Element {
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
    row = row
        .child(leading)
        .child(Element::from(
            hstack()
                .flex_grow(1.0)
                .min_w("0")
                .items_center()
                .child(main),
        ))
        .child(stub_badge("WIP"));
    let cursor = if disabled {
        row.cursor_not_allowed()
    } else {
        row.cursor_kind(CursorKind::Pointer)
    };
    let trigger_fid = stub_trigger_focus_id(id);
    let mut out: Element = interactive(cursor)
        .focus_id(trigger_fid.as_str())
        .focusable(!disabled)
        .into();
    out.template_id = Some(trigger_fid);
    out
}

pub(crate) fn lucide(hex: u32) -> Element {
    let glyph = char::from_u32(hex).unwrap_or('?').to_string();
    build_icon_text_node(glyph)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"))
}
