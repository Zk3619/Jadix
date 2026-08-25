//! Dropdown / Context 菜单行 chrome、文本染色与 highlight paint（WP-SI.5）。

use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::{parse_color, parse_text_color, WhiteSpace};

/// Shared item chrome. Do **not** use `min_w(0)` — that collapses wrapping labels to 1ch.
pub fn item_row_chrome(inset: bool) -> BoxBuilder {
    hstack()
        .w("auto")
        .min_w("100%")
        .items_center()
        .gap("0.5rem")
        .h("auto")
        .min_h("1.75rem")
        .flex_shrink(0.0)
        .rounded("0.25rem")
        .py("0.25rem")
        .px(if inset { "2rem" } else { "0.5rem" })
        .justify("start")
}

/// Recolor text leaves; skip `shortcut_slot` subtrees (`attr_slot` = usually `"data-slot"`).
pub fn style_item_text(el: &mut Element, color: [u8; 4], attr_slot: &str, shortcut_slot: &str) {
    if el.attrs.get(attr_slot).map(String::as_str) == Some(shortcut_slot) {
        return;
    }
    if el.text.is_some() {
        el.text_style.size = 14.0;
        el.text_style.line_height_px = Some(20.0);
        el.text_style.weight = 600;
        el.text_style.color = color;
        el.attrs.insert("font_weight".to_string(), "1".to_string());
        el.attrs.insert("text_size".to_string(), "1".to_string());
        el.attrs.insert("text_color".to_string(), "1".to_string());
        el.attrs.insert("line_height".to_string(), "1".to_string());
        el.text_style.white_space = WhiteSpace::Nowrap;
        el.attrs
            .insert("white_space".to_string(), "nowrap".to_string());
    }
    for c in &mut el.children {
        style_item_text(c, color, attr_slot, shortcut_slot);
    }
}

pub fn has_shortcut_slot(el: &Element, attr_slot: &str, shortcut_slot: &str) -> bool {
    el.attrs.get(attr_slot).map(|s| s.as_str()) == Some(shortcut_slot)
        || el
            .children
            .iter()
            .any(|c| has_shortcut_slot(c, attr_slot, shortcut_slot))
}

/// Per-menu attr / slot / active-key 配置（`rui_dd_*` vs `rui_ctx_*`）。
pub struct ItemPaintConfig {
    pub attr_slot: &'static str,
    pub shortcut_slot: &'static str,
    pub interactive_slots: &'static [&'static str],
    pub attr_disabled: &'static str,
    pub attr_destructive: &'static str,
    pub attr_ix: &'static str,
    pub attr_menu_id: &'static str,
    /// e.g. `"__dd_active"` / `"__ctx_active"`.
    pub active_key_suffix: &'static str,
    pub active_none: usize,
}

pub fn sync_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    cfg: &ItemPaintConfig,
) {
    let slot = el.attrs.get(cfg.attr_slot).map(String::as_str);
    if slot.is_some_and(|s| cfg.interactive_slots.iter().any(|want| *want == s)) {
        let disabled = el.attrs.get(cfg.attr_disabled).map(String::as_str) == Some("1");
        let destructive = el.attrs.get(cfg.attr_destructive).map(String::as_str) == Some("1");
        let ix = el
            .attrs
            .get(cfg.attr_ix)
            .and_then(|s| s.parse::<usize>().ok());
        let id = el.attrs.get(cfg.attr_menu_id).cloned();
        if let (Some(id), Some(ix)) = (id, ix) {
            let key = format!("{id}{}", cfg.active_key_suffix);
            let active = use_signal(key.as_str(), || cfg.active_none).peek();
            let on = !disabled && active == ix;
            apply_item_highlight(el, layout_tree, impact, on, destructive, cfg);
        }
    }
    for child in &mut el.children {
        sync_item_paint_subtree(child, layout_tree, impact, cfg);
    }
}

pub fn apply_item_highlight(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    on: bool,
    destructive: bool,
    cfg: &ItemPaintConfig,
) {
    let want_bg = if on {
        parse_color("accent")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let want_fg = if destructive {
        parse_text_color("destructive")
    } else if on {
        parse_text_color("accent-foreground")
    } else {
        parse_text_color("popover-foreground")
    };
    let bg_changed = el.style.bg_color != want_bg;
    if bg_changed {
        el.style.bg_color = want_bg;
        layout_tree.update_taffy_node_style(el);
        *impact = impact.merge(LayoutStatus::StyleChanged);
    }
    paint_item_fg(el, want_fg, cfg.attr_slot, cfg.shortcut_slot);
}

pub fn paint_item_fg(el: &mut Element, fg: [u8; 4], attr_slot: &str, shortcut_slot: &str) {
    if el.checkbox_sync.is_some() || el.radio_selected_sync.is_some() {
        return;
    }
    if el.attrs.get(attr_slot).map(String::as_str) == Some(shortcut_slot) {
        return;
    }
    if el.text.is_some() && el.text_style.font_family.as_deref() != Some("lucide") {
        el.text_style.color = fg;
        el.text_style.weight = 600;
        el.text_style.white_space = WhiteSpace::Nowrap;
        el.attrs
            .entry("font_weight".to_string())
            .or_insert_with(|| "1".to_string());
    }
    for child in &mut el.children {
        paint_item_fg(child, fg, attr_slot, shortcut_slot);
    }
}
