//! P1: live Style patches for Inspector whitelist fields.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::tree::{Element, LayoutStatus};
use crate::style::parse_color;

use super::snapshot::{find_element_by_path_mut, ElementPath};

#[derive(Clone, Debug, PartialEq)]
pub enum StylePatch {
    PaddingUniform(f32),
    MarginUniform(f32),
    WidthPx(f32),
    HeightPx(f32),
    BackgroundToken(String),
    Opacity(f32),
    BorderWidth(f32),
    FlexGrow(f32),
    FlexShrink(f32),
}

/// Queued from Apply CSS; drained in `AppState::render` while the tree is already
/// mutably borrowed — avoids silent `try_borrow_mut` failures mid-click.
#[derive(Clone, Debug)]
pub(crate) struct PendingInspectorCssom {
    pub path: Vec<usize>,
    pub css: String,
}

thread_local! {
    static PENDING_INSPECTOR_EDITS: RefCell<Vec<PendingInspectorCssom>> =
        const { RefCell::new(Vec::new()) };
    /// Session-lived Inspector edits, re-applied after chrome sync / rebuild so hover
    /// paint and class-variant restore cannot wipe live CSSOM Apply changes.
    static COMMITTED_INSPECTOR_EDITS: RefCell<HashMap<String, Vec<CommittedInspectorOp>>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone, Debug)]
enum CommittedInspectorOp {
    Cssom(String),
}

pub(crate) fn queue_inspector_cssom(path: Vec<usize>, css: String) {
    PENDING_INSPECTOR_EDITS.with(|q| {
        q.borrow_mut().push(PendingInspectorCssom { path, css });
    });
    crate::app::runtime::request_redraw();
}

pub(crate) fn take_pending_inspector_edits() -> Vec<PendingInspectorCssom> {
    PENDING_INSPECTOR_EDITS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

fn path_key(path: &[usize]) -> String {
    super::snapshot::path_to_key(path)
}

pub(crate) fn commit_inspector_cssom(path: &[usize], css: String) {
    let key = path_key(path);
    COMMITTED_INSPECTOR_EDITS.with(|m| {
        m.borrow_mut()
            .insert(key, vec![CommittedInspectorOp::Cssom(css)]);
    });
}

/// Last CSSOM text applied via Inspector for `path`, if any this session.
pub(crate) fn committed_inspector_cssom(path: &[usize]) -> Option<String> {
    let key = path_key(path);
    COMMITTED_INSPECTOR_EDITS.with(|m| {
        m.borrow().get(&key).and_then(|ops| {
            ops.iter().rev().find_map(|op| match op {
                CommittedInspectorOp::Cssom(css) => Some(css.clone()),
            })
        })
    })
}

/// Re-apply session Inspector edits after chrome sync or `rebuild_ui`.
pub(crate) fn reapply_inspector_overrides(
    root: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
) -> LayoutStatus {
    let committed = COMMITTED_INSPECTOR_EDITS.with(|m| m.borrow().clone());
    if committed.is_empty() {
        return LayoutStatus::Clean;
    }
    let mut impact = LayoutStatus::Clean;
    for (key, ops) in committed {
        let path = super::snapshot::key_to_path(&key);
        let Some(el) = find_element_by_path_mut(root, &path) else {
            continue;
        };
        for op in ops {
            match op {
                CommittedInspectorOp::Cssom(css) => {
                    impact = impact.merge(apply_cssom_text(el, &css));
                }
            }
        }
        if el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
        }
    }
    impact
}

#[cfg(test)]
pub(crate) fn clear_committed_inspector_edits_for_test() {
    COMMITTED_INSPECTOR_EDITS.with(|m| m.borrow_mut().clear());
    let _ = take_pending_inspector_edits();
}

pub fn apply_style_patch(el: &mut Element, patch: &StylePatch) -> LayoutStatus {
    use taffy::prelude::*;
    let status = match patch {
        StylePatch::PaddingUniform(v) => {
            let lp = LengthPercentage::length(*v);
            el.style.padding = Rect {
                top: lp,
                right: lp,
                bottom: lp,
                left: lp,
            };
            LayoutStatus::LayoutChanged
        }
        StylePatch::MarginUniform(v) => {
            let lp = LengthPercentageAuto::length(*v);
            el.style.margin = Rect {
                top: lp,
                right: lp,
                bottom: lp,
                left: lp,
            };
            LayoutStatus::LayoutChanged
        }
        StylePatch::WidthPx(v) => {
            el.style.size.width = Dimension::length(*v);
            LayoutStatus::LayoutChanged
        }
        StylePatch::HeightPx(v) => {
            el.style.size.height = Dimension::length(*v);
            LayoutStatus::LayoutChanged
        }
        StylePatch::BackgroundToken(token) => {
            el.style.bg_color = parse_color(token.trim());
            LayoutStatus::StyleChanged
        }
        StylePatch::Opacity(v) => {
            el.style.opacity = v.clamp(0.0, 1.0);
            LayoutStatus::StyleChanged
        }
        StylePatch::BorderWidth(v) => {
            el.style.border_width = v.max(0.0);
            if el.style.border_style.is_invisible() && *v > 0.0 {
                el.style.border_style = crate::style::LineStyle::Solid;
            }
            LayoutStatus::LayoutChanged
        }
        StylePatch::FlexGrow(v) => {
            el.style.flex_grow = *v;
            LayoutStatus::LayoutChanged
        }
        StylePatch::FlexShrink(v) => {
            el.style.flex_shrink = *v;
            LayoutStatus::LayoutChanged
        }
    };
    sync_class_set_base_from_live(el);
    status
}

/// Keep utility-class base snapshots aligned with live style after Inspector edits.
fn sync_class_set_base_from_live(el: &mut Element) {
    if let Some(cs) = el.class_set.as_mut() {
        if cs.base_style.is_some() {
            cs.base_style = Some(el.style.clone());
        }
        if cs.base_text_style.is_some() {
            cs.base_text_style = Some(el.text_style.clone());
        }
    }
}

pub fn patch_element_at_path(
    root: &mut Element,
    path: &ElementPath,
    patch: StylePatch,
) -> Option<LayoutStatus> {
    let el = find_element_by_path_mut(root, path)?;
    Some(apply_style_patch(el, &patch))
}

/// Parse a CSS-like declaration list and write matching [`Style`] fields (P2).
///
/// Supported properties: padding/margin (1 or 4 values), width/height/min-width/min-height,
/// background, opacity, border-width, border-color, flex-grow/flex-shrink, z-index, gap,
/// display, flex-direction, overflow/overflow-x/overflow-y, position, class (utility list).
///
/// When the node has a [`ClassSet`](crate::style::ClassSet), also refreshes
/// `base_style` / `base_text_style` so the next `resolve_class_variants` pass and
/// Inspector authored dump keep the applied values (otherwise live `el.style`
/// edits are wiped or hidden behind the stale base snapshot).
pub fn apply_cssom_text(el: &mut Element, css: &str) -> LayoutStatus {
    let mut impact = LayoutStatus::Clean;
    for (prop, value) in parse_declarations(css) {
        impact = impact.merge(apply_cssom_decl(el, &prop, &value));
    }
    sync_class_set_base_from_live(el);
    impact
}

fn parse_declarations(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stripped = String::new();
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(d) = chars.next() {
                if d == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for d in chars.by_ref() {
                if d == '\n' {
                    stripped.push('\n');
                    break;
                }
            }
            continue;
        }
        stripped.push(c);
    }
    for chunk in stripped.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((k, v)) = chunk.split_once(':') else {
            continue;
        };
        let prop = k.trim().to_ascii_lowercase();
        let value = v.trim().to_string();
        if !prop.is_empty() && !value.is_empty() {
            out.push((prop, value));
        }
    }
    out
}

fn apply_cssom_decl(el: &mut Element, prop: &str, value: &str) -> LayoutStatus {
    match prop {
        "padding" => apply_padding(el, value),
        "margin" => apply_margin(el, value),
        "width" => dim_to_size(value)
            .map(|d| {
                el.style.size.width = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "height" => dim_to_size(value)
            .map(|d| {
                el.style.size.height = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "min-width" | "min_width" => dim_to_size(value)
            .map(|d| {
                el.style.min_size.width = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "min-height" | "min_height" => dim_to_size(value)
            .map(|d| {
                el.style.min_size.height = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "background" | "background-color" | "bg" => {
            el.style.bg_color = parse_cssom_color(value);
            LayoutStatus::StyleChanged
        }
        "opacity" => {
            if let Ok(v) = parse_f32(value) {
                el.style.opacity = v.clamp(0.0, 1.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border" => apply_border_shorthand(el, value, None),
        "border-top" | "border_top" => apply_border_shorthand(el, value, Some(0)),
        "border-right" | "border_right" => apply_border_shorthand(el, value, Some(1)),
        "border-bottom" | "border_bottom" => apply_border_shorthand(el, value, Some(2)),
        "border-left" | "border_left" => apply_border_shorthand(el, value, Some(3)),
        "border-width" | "border_width" => apply_border_width_shorthand(el, value),
        "border-top-width" | "border_top_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(0, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-right-width" | "border_right_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(1, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-bottom-width" | "border_bottom_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(2, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-left-width" | "border_left_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(3, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-style" | "border_style" => apply_border_style_shorthand(el, value),
        "border-top-style" | "border_top_style" => {
            el.style
                .set_border_side_style(0, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-right-style" | "border_right_style" => {
            el.style
                .set_border_side_style(1, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-bottom-style" | "border_bottom_style" => {
            el.style
                .set_border_side_style(2, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-left-style" | "border_left_style" => {
            el.style
                .set_border_side_style(3, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-color" | "border_color" => apply_border_color_shorthand(el, value),
        "border-top-color" | "border_top_color" => {
            el.style.set_border_side_color(0, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-right-color" | "border_right_color" => {
            el.style.set_border_side_color(1, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-bottom-color" | "border_bottom_color" => {
            el.style.set_border_side_color(2, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-left-color" | "border_left_color" => {
            el.style.set_border_side_color(3, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-radius" | "border_radius" => apply_border_radius(el, value),
        "border-top-left-radius" | "border_top_left_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[2] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-top-right-radius" | "border_top_right_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[0] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-bottom-right-radius" | "border_bottom_right_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[1] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-bottom-left-radius" | "border_bottom_left_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[3] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        // Logical radius properties (LTR mapping)
        "border-start-start-radius" | "border_start_start_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[2] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-start-end-radius" | "border_start_end_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[0] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-end-end-radius" | "border_end_end_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[1] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-end-start-radius" | "border_end_start_radius" => {
            if let Ok(v) = parse_f32(value) {
                el.style.radii[3] = v.max(0.0);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        // Logical border properties (LTR: inline-start=left, inline-end=right, block-start=top, block-end=bottom)
        "border-block" | "border_block" => {
            let r = apply_border_shorthand(el, value, Some(0));
            apply_border_shorthand(el, value, Some(2));
            r
        }
        "border-inline" | "border_inline" => {
            let r = apply_border_shorthand(el, value, Some(3));
            apply_border_shorthand(el, value, Some(1));
            r
        }
        "border-block-start" | "border_block_start" => apply_border_shorthand(el, value, Some(0)),
        "border-block-end" | "border_block_end" => apply_border_shorthand(el, value, Some(2)),
        "border-inline-start" | "border_inline_start" => apply_border_shorthand(el, value, Some(3)),
        "border-inline-end" | "border_inline_end" => apply_border_shorthand(el, value, Some(1)),
        "border-block-start-width" | "border_block_start_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(0, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-block-end-width" | "border_block_end_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(2, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-block-start-color" | "border_block_start_color" => {
            el.style.set_border_side_color(0, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-block-end-color" | "border_block_end_color" => {
            el.style.set_border_side_color(2, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-block-start-style" | "border_block_start_style" => {
            el.style
                .set_border_side_style(0, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-block-end-style" | "border_block_end_style" => {
            el.style
                .set_border_side_style(2, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-inline-start-width" | "border_inline_start_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(3, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-inline-end-width" | "border_inline_end_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(1, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-inline-start-color" | "border_inline_start_color" => {
            el.style.set_border_side_color(3, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-inline-end-color" | "border_inline_end_color" => {
            el.style.set_border_side_color(1, parse_cssom_color(value));
            LayoutStatus::StyleChanged
        }
        "border-inline-start-style" | "border_inline_start_style" => {
            el.style
                .set_border_side_style(3, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-inline-end-style" | "border_inline_end_style" => {
            el.style
                .set_border_side_style(1, crate::style::LineStyle::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "border-block-width" | "border_block_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(0, v);
                el.style.set_border_side_width(2, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-block-style" | "border_block_style" => {
            let s = crate::style::LineStyle::from_css_keyword(value);
            el.style.set_border_side_style(0, s);
            el.style.set_border_side_style(2, s);
            LayoutStatus::StyleChanged
        }
        "border-block-color" | "border_block_color" => {
            let c = parse_cssom_color(value);
            el.style.set_border_side_color(0, c);
            el.style.set_border_side_color(2, c);
            LayoutStatus::StyleChanged
        }
        "border-inline-width" | "border_inline_width" => {
            if let Ok(v) = parse_f32(value) {
                el.style.set_border_side_width(3, v);
                el.style.set_border_side_width(1, v);
                ensure_border_style_visible(&mut el.style);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-inline-style" | "border_inline_style" => {
            let s = crate::style::LineStyle::from_css_keyword(value);
            el.style.set_border_side_style(3, s);
            el.style.set_border_side_style(1, s);
            LayoutStatus::StyleChanged
        }
        "border-inline-color" | "border_inline_color" => {
            let c = parse_cssom_color(value);
            el.style.set_border_side_color(3, c);
            el.style.set_border_side_color(1, c);
            LayoutStatus::StyleChanged
        }
        "border-collapse" | "border_collapse" => {
            el.style.border_collapse = value.trim().eq_ignore_ascii_case("collapse");
            LayoutStatus::LayoutChanged
        }
        "border-spacing" | "border_spacing" => {
            let parts = split_box(value);
            match parts.as_slice() {
                [a] => {
                    if let Ok(v) = parse_f32(a) {
                        el.style.border_spacing = [v, v];
                        LayoutStatus::LayoutChanged
                    } else {
                        LayoutStatus::Clean
                    }
                }
                [h, v_val] => {
                    if let (Ok(h), Ok(v)) = (parse_f32(h), parse_f32(v_val)) {
                        el.style.border_spacing = [h, v];
                        LayoutStatus::LayoutChanged
                    } else {
                        LayoutStatus::Clean
                    }
                }
                _ => LayoutStatus::Clean,
            }
        }
        "border-shape" | "border_shape" | "corner-shape" | "corner_shape" => {
            apply_border_shape_shorthand(el, value)
        }
        "corner-top-left-shape" | "corner_top_left_shape" => {
            el.style
                .set_corner_side_shape(0, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-top-right-shape" | "corner_top_right_shape" => {
            el.style
                .set_corner_side_shape(1, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-bottom-right-shape" | "corner_bottom_right_shape" => {
            el.style
                .set_corner_side_shape(2, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-bottom-left-shape" | "corner_bottom_left_shape" => {
            el.style
                .set_corner_side_shape(3, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-start-start-shape" | "corner_start_start_shape" => {
            el.style
                .set_corner_side_shape(0, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-start-end-shape" | "corner_start_end_shape" => {
            el.style
                .set_corner_side_shape(1, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-end-end-shape" | "corner_end_end_shape" => {
            el.style
                .set_corner_side_shape(2, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "corner-end-start-shape" | "corner_end_start_shape" => {
            el.style
                .set_corner_side_shape(3, crate::style::CornerShape::from_css_keyword(value));
            LayoutStatus::StyleChanged
        }
        "flex-grow" | "flex_grow" => {
            if let Ok(v) = parse_f32(value) {
                el.style.flex_grow = v;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "flex-shrink" | "flex_shrink" => {
            if let Ok(v) = parse_f32(value) {
                el.style.flex_shrink = v;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "z-index" | "z_index" => {
            if let Ok(v) = value.trim().parse::<i32>() {
                el.style.z_index = v;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "gap" => apply_gap(el, value),
        "display" => apply_display(el, value),
        "flex-direction" | "flex_direction" => apply_flex_direction(el, value),
        "overflow" => {
            let o = parse_overflow(value);
            el.style.overflow.x = o;
            el.style.overflow.y = o;
            LayoutStatus::LayoutChanged
        }
        "overflow-x" | "overflow_x" => {
            el.style.overflow.x = parse_overflow(value);
            LayoutStatus::LayoutChanged
        }
        "overflow-y" | "overflow_y" => {
            el.style.overflow.y = parse_overflow(value);
            LayoutStatus::LayoutChanged
        }
        "position" => apply_position(el, value),
        "top" => dim_to_lpa(value)
            .map(|d| {
                el.style.inset.top = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "right" => dim_to_lpa(value)
            .map(|d| {
                el.style.inset.right = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "bottom" => dim_to_lpa(value)
            .map(|d| {
                el.style.inset.bottom = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "left" => dim_to_lpa(value)
            .map(|d| {
                el.style.inset.left = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "max-width" | "max_width" => dim_to_size(value)
            .map(|d| {
                el.style.max_size.width = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "max-height" | "max_height" => dim_to_size(value)
            .map(|d| {
                el.style.max_size.height = d;
                LayoutStatus::LayoutChanged
            })
            .unwrap_or(LayoutStatus::Clean),
        "color" => {
            let c = parse_cssom_color(value);
            el.text_style.color = [
                (c[0] * 255.0).round() as u8,
                (c[1] * 255.0).round() as u8,
                (c[2] * 255.0).round() as u8,
                (c[3] * 255.0).round() as u8,
            ];
            LayoutStatus::StyleChanged
        }
        "font-size" | "font_size" => {
            if let Ok(v) = parse_f32(value) {
                el.text_style.size = v.max(1.0);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "font-weight" | "font_weight" => {
            if let Ok(v) = value.trim().parse::<u16>() {
                el.text_style.weight = v;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "class" | "class-name" | "class_name" => crate::style::apply_class_list(el, value),
        "border-image-source" | "border_image_source" => {
            let src = value.trim();
            if src.eq_ignore_ascii_case("none") || src.is_empty() {
                el.style.border_image_source = None;
            } else {
                let key = crate::app::pending_images::texture_key_for_src(src);
                crate::app::pending_images::queue_image_from_src(&key, src);
                el.style.border_image_source = Some(key);
            }
            LayoutStatus::StyleChanged
        }
        "border-image-slice" | "border_image_slice" => apply_border_image_slice(el, value),
        "border-image-width" | "border_image_width" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let vals: Vec<f32> = parts.iter().filter_map(|s| parse_f32(s).ok()).collect();
            if let Some(trbl) = expand_box_values(&vals) {
                el.style.border_image_width = Some(trbl);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-image-outset" | "border_image_outset" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let vals: Vec<f32> = parts.iter().filter_map(|s| parse_f32(s).ok()).collect();
            if let Some(trbl) = expand_box_values(&vals) {
                el.style.border_image_outset = trbl;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        "border-image-repeat" | "border_image_repeat" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let reps: Vec<crate::style::BorderImageRepeat> = parts
                .iter()
                .map(|s| crate::style::BorderImageRepeat::from_css_keyword(s))
                .collect();
            match reps.as_slice() {
                [a] => {
                    el.style.border_image_repeat = [*a, *a];
                    LayoutStatus::StyleChanged
                }
                [h, v] => {
                    el.style.border_image_repeat = [*h, *v];
                    LayoutStatus::StyleChanged
                }
                _ => LayoutStatus::Clean,
            }
        }
        "border-image" | "border_image" => apply_border_image_shorthand(el, value),
        _ => LayoutStatus::Clean,
    }
}

fn parse_f32(s: &str) -> Result<f32, ()> {
    let s = s.trim().trim_end_matches("px").trim();
    s.parse::<f32>().map_err(|_| ())
}

fn parse_cssom_color(s: &str) -> [f32; 4] {
    let s = s.trim();
    if let Some(inner) = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))
        .and_then(|t| t.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() >= 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].parse::<f32>(),
                parts[1].parse::<f32>(),
                parts[2].parse::<f32>(),
            ) {
                let a = parts
                    .get(3)
                    .and_then(|p| p.parse::<f32>().ok())
                    .unwrap_or(1.0);
                return [
                    (r / 255.0).clamp(0.0, 1.0),
                    (g / 255.0).clamp(0.0, 1.0),
                    (b / 255.0).clamp(0.0, 1.0),
                    a.clamp(0.0, 1.0),
                ];
            }
        }
    }
    parse_color(s)
}

fn parse_cssom_dim(s: &str) -> Option<crate::layout::dimension::CssDim> {
    use crate::layout::dimension::{parse_css_dim, CssDim};
    let s = s.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(CssDim::Auto);
    }
    let unitless = s
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+');
    if unitless {
        return s.parse::<f32>().ok().map(CssDim::Px);
    }
    parse_css_dim(s)
}

fn dim_to_size(s: &str) -> Option<taffy::prelude::Dimension> {
    parse_cssom_dim(s)?.preview_dimension()
}

fn dim_to_lp(s: &str) -> Option<taffy::prelude::LengthPercentage> {
    let d = dim_to_size(s)?;
    Some(crate::layout::dimension::dimension_to_lp(d))
}

fn dim_to_lpa(s: &str) -> Option<taffy::prelude::LengthPercentageAuto> {
    let d = dim_to_size(s)?;
    Some(crate::layout::dimension::dimension_to_lpa(d))
}

fn split_box(value: &str) -> Vec<String> {
    value.split_whitespace().map(|s| s.to_string()).collect()
}

fn apply_padding(el: &mut Element, value: &str) -> LayoutStatus {
    use taffy::prelude::*;
    let parts = split_box(value);
    let sides = match parts.as_slice() {
        [a] => {
            let lp = match dim_to_lp(a) {
                Some(v) => v,
                None => return LayoutStatus::Clean,
            };
            [lp, lp, lp, lp]
        }
        [tb, lr] => {
            let (Some(a), Some(b)) = (dim_to_lp(tb), dim_to_lp(lr)) else {
                return LayoutStatus::Clean;
            };
            [a, b, a, b]
        }
        [t, r, b, l] => {
            let (Some(t), Some(r), Some(b), Some(l)) =
                (dim_to_lp(t), dim_to_lp(r), dim_to_lp(b), dim_to_lp(l))
            else {
                return LayoutStatus::Clean;
            };
            [t, r, b, l]
        }
        _ => return LayoutStatus::Clean,
    };
    el.style.padding = Rect {
        top: sides[0],
        right: sides[1],
        bottom: sides[2],
        left: sides[3],
    };
    LayoutStatus::LayoutChanged
}

fn apply_margin(el: &mut Element, value: &str) -> LayoutStatus {
    use taffy::prelude::*;
    let parts = split_box(value);
    let sides = match parts.as_slice() {
        [a] => {
            let lp = match dim_to_lpa(a) {
                Some(v) => v,
                None => return LayoutStatus::Clean,
            };
            [lp, lp, lp, lp]
        }
        [tb, lr] => {
            let (Some(a), Some(b)) = (dim_to_lpa(tb), dim_to_lpa(lr)) else {
                return LayoutStatus::Clean;
            };
            [a, b, a, b]
        }
        [t, r, b, l] => {
            let (Some(t), Some(r), Some(b), Some(l)) =
                (dim_to_lpa(t), dim_to_lpa(r), dim_to_lpa(b), dim_to_lpa(l))
            else {
                return LayoutStatus::Clean;
            };
            [t, r, b, l]
        }
        _ => return LayoutStatus::Clean,
    };
    el.style.margin = Rect {
        top: sides[0],
        right: sides[1],
        bottom: sides[2],
        left: sides[3],
    };
    LayoutStatus::LayoutChanged
}

fn apply_gap(el: &mut Element, value: &str) -> LayoutStatus {
    let parts = split_box(value);
    match parts.as_slice() {
        [a] => {
            let Some(lp) = dim_to_lp(a) else {
                return LayoutStatus::Clean;
            };
            el.style.gap.width = lp;
            el.style.gap.height = lp;
        }
        [row, col] => {
            let (Some(r), Some(c)) = (dim_to_lp(row), dim_to_lp(col)) else {
                return LayoutStatus::Clean;
            };
            el.style.gap.height = r;
            el.style.gap.width = c;
        }
        _ => return LayoutStatus::Clean,
    }
    LayoutStatus::LayoutChanged
}

fn apply_display(el: &mut Element, value: &str) -> LayoutStatus {
    use taffy::prelude::*;
    el.style.display = match value.trim().to_ascii_lowercase().as_str() {
        "none" => Display::None,
        "grid" => Display::Grid,
        "flex" | "block" | "inline-flex" => Display::Flex,
        _ => return LayoutStatus::Clean,
    };
    LayoutStatus::LayoutChanged
}

fn apply_flex_direction(el: &mut Element, value: &str) -> LayoutStatus {
    use taffy::prelude::*;
    el.style.flex_direction = match value.trim().to_ascii_lowercase().as_str() {
        "row" => FlexDirection::Row,
        "column" | "col" => FlexDirection::Column,
        "row-reverse" => FlexDirection::RowReverse,
        "column-reverse" | "col-reverse" => FlexDirection::ColumnReverse,
        _ => return LayoutStatus::Clean,
    };
    LayoutStatus::LayoutChanged
}

fn parse_overflow(value: &str) -> taffy::style::Overflow {
    match value.trim().to_ascii_lowercase().as_str() {
        "hidden" => taffy::style::Overflow::Hidden,
        "clip" => taffy::style::Overflow::Clip,
        "scroll" | "auto" => taffy::style::Overflow::Scroll,
        _ => taffy::style::Overflow::Visible,
    }
}

fn apply_position(el: &mut Element, value: &str) -> LayoutStatus {
    use taffy::prelude::*;
    match value.trim().to_ascii_lowercase().as_str() {
        "absolute" => {
            el.style.position = Position::Absolute;
            el.style.position_fixed = false;
            el.style.position_sticky = false;
        }
        "fixed" => {
            el.style.position = Position::Absolute;
            el.style.position_fixed = true;
            el.style.position_sticky = false;
        }
        "sticky" => {
            el.style.position = Position::Relative;
            el.style.position_fixed = false;
            el.style.position_sticky = true;
        }
        "relative" | "static" => {
            el.style.position = Position::Relative;
            el.style.position_fixed = false;
            el.style.position_sticky = false;
        }
        _ => return LayoutStatus::Clean,
    }
    LayoutStatus::LayoutChanged
}

fn ensure_border_style_visible(style: &mut crate::style::Style) {
    if style.border_style.is_invisible() {
        style.border_style = crate::style::LineStyle::Solid;
    }
}

fn is_line_style_keyword(s: &str) -> bool {
    matches!(
        s,
        "none"
            | "hidden"
            | "solid"
            | "dotted"
            | "dashed"
            | "double"
            | "groove"
            | "ridge"
            | "inset"
            | "outset"
    )
}

/// Parse `border` shorthand: `<line-width> || <line-style> || <color>` (any order).
fn apply_border_shorthand(el: &mut Element, value: &str, side: Option<usize>) -> LayoutStatus {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let mut width: Option<f32> = None;
    let mut style: Option<crate::style::LineStyle> = None;
    let mut color: Option<[f32; 4]> = None;

    for token in &parts {
        let t = token.to_ascii_lowercase();
        if is_line_style_keyword(&t) {
            style = Some(crate::style::LineStyle::from_css_keyword(&t));
        } else if let Ok(v) = parse_f32(token) {
            width = Some(v);
        } else {
            color = Some(parse_cssom_color(token));
        }
    }

    let mut impact = LayoutStatus::Clean;

    if let Some(w) = width {
        match side {
            None => {
                el.style.set_uniform_border_width(w);
                el.style.border_width_sides = None;
            }
            Some(s) => el.style.set_border_side_width(s, w),
        }
        impact = LayoutStatus::LayoutChanged;
    }
    if let Some(s) = style {
        match side {
            None => {
                el.style.border_style = s;
                el.style.border_style_sides = None;
            }
            Some(si) => el.style.set_border_side_style(si, s),
        }
        impact = impact.merge(LayoutStatus::StyleChanged);
    }
    if let Some(c) = color {
        match side {
            None => {
                el.style.border_color = c;
                el.style.border_color_sides = None;
            }
            Some(s) => el.style.set_border_side_color(s, c),
        }
        impact = impact.merge(LayoutStatus::StyleChanged);
    }

    if width.is_some() && style.is_none() {
        ensure_border_style_visible(&mut el.style);
    }

    impact
}

fn expand_box_values<T: Clone>(parts: &[T]) -> Option<[T; 4]> {
    match parts.len() {
        1 => Some([
            parts[0].clone(),
            parts[0].clone(),
            parts[0].clone(),
            parts[0].clone(),
        ]),
        2 => Some([
            parts[0].clone(),
            parts[1].clone(),
            parts[0].clone(),
            parts[1].clone(),
        ]),
        3 => Some([
            parts[0].clone(),
            parts[1].clone(),
            parts[2].clone(),
            parts[1].clone(),
        ]),
        4 => Some([
            parts[0].clone(),
            parts[1].clone(),
            parts[2].clone(),
            parts[3].clone(),
        ]),
        _ => None,
    }
}

fn apply_border_width_shorthand(el: &mut Element, value: &str) -> LayoutStatus {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 1 {
        if let Ok(v) = parse_f32(parts[0]) {
            return apply_style_patch(el, &StylePatch::BorderWidth(v));
        }
        return LayoutStatus::Clean;
    }
    let vals: Vec<f32> = parts.iter().filter_map(|s| parse_f32(s).ok()).collect();
    if let Some(trbl) = expand_box_values(&vals) {
        el.style.set_border_widths_trbl(trbl);
        ensure_border_style_visible(&mut el.style);
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::Clean
    }
}

fn apply_border_style_shorthand(el: &mut Element, value: &str) -> LayoutStatus {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let styles: Vec<crate::style::LineStyle> = parts
        .iter()
        .map(|s| crate::style::LineStyle::from_css_keyword(s))
        .collect();
    if let Some(trbl) = expand_box_values(&styles) {
        el.style.set_border_styles_trbl(trbl);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn apply_border_color_shorthand(el: &mut Element, value: &str) -> LayoutStatus {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 1 {
        el.style.border_color = parse_cssom_color(parts[0]);
        el.style.border_color_sides = None;
        return LayoutStatus::StyleChanged;
    }
    let colors: Vec<[f32; 4]> = parts.iter().map(|s| parse_cssom_color(s)).collect();
    if let Some([t, r, b, l]) = expand_box_values(&colors) {
        el.style.set_border_side_color(0, t);
        el.style.set_border_side_color(1, r);
        el.style.set_border_side_color(2, b);
        el.style.set_border_side_color(3, l);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn apply_border_radius(el: &mut Element, value: &str) -> LayoutStatus {
    // radii order: [top-right, bottom-right, top-left, bottom-left]
    let parts: Vec<&str> = value.split_whitespace().collect();
    let vals: Vec<f32> = parts.iter().filter_map(|s| parse_f32(s).ok()).collect();
    match vals.as_slice() {
        [a] => {
            el.style.radii = [*a, *a, *a, *a];
            LayoutStatus::StyleChanged
        }
        [tb, lr] => {
            // CSS order: TL TR BR BL -> internal [TR, BR, TL, BL]
            el.style.radii = [*lr, *lr, *tb, *tb];
            LayoutStatus::StyleChanged
        }
        [tl, tr_br, bl] => {
            el.style.radii = [*tr_br, *tr_br, *tl, *bl];
            LayoutStatus::StyleChanged
        }
        [tl, tr, br, bl] => {
            el.style.radii = [*tr, *br, *tl, *bl];
            LayoutStatus::StyleChanged
        }
        _ => LayoutStatus::Clean,
    }
}

fn apply_border_image_slice(el: &mut Element, value: &str) -> LayoutStatus {
    let fill = value.to_ascii_lowercase().contains("fill");
    let clean: String = value
        .to_ascii_lowercase()
        .replace("fill", "")
        .trim()
        .to_string();
    let parts: Vec<&str> = clean.split_whitespace().collect();
    let vals: Vec<f32> = parts
        .iter()
        .filter_map(|s| {
            let s = s.trim_end_matches('%');
            s.parse::<f32>().ok()
        })
        .collect();
    if let Some(trbl) = expand_box_values(&vals) {
        el.style.border_image_slice = trbl;
        el.style.border_image_slice_fill = fill;
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn apply_border_image_shorthand(el: &mut Element, value: &str) -> LayoutStatus {
    let v = value.trim();
    if v.eq_ignore_ascii_case("none") {
        el.style.border_image_source = None;
        return LayoutStatus::StyleChanged;
    }
    // Minimal parse: look for url(...) as source, then slice/width/outset/repeat
    if let Some(url_start) = v.find("url(") {
        let rest = &v[url_start..];
        if let Some(end) = rest.find(')') {
            let url = rest[4..end].trim().trim_matches(|c| c == '"' || c == '\'');
            let key = crate::app::pending_images::texture_key_for_src(url);
            crate::app::pending_images::queue_image_from_src(&key, url);
            el.style.border_image_source = Some(key);

            let after = rest[end + 1..].trim();
            if !after.is_empty() {
                // Try to parse remaining as slice values
                apply_border_image_slice(el, after);
            }
        }
    }
    LayoutStatus::StyleChanged
}

fn apply_border_shape_shorthand(el: &mut Element, value: &str) -> LayoutStatus {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 1 {
        el.style.corner_shape = crate::style::CornerShape::from_css_keyword(parts[0]);
        el.style.corner_shape_sides = None;
        return LayoutStatus::StyleChanged;
    }
    let shapes: Vec<crate::style::CornerShape> = parts
        .iter()
        .map(|s| crate::style::CornerShape::from_css_keyword(s))
        .collect();
    if let Some([tl, tr, br, bl]) = expand_box_values(&shapes) {
        el.style.set_corner_side_shape(0, tl);
        el.style.set_corner_side_shape(1, tr);
        el.style.set_corner_side_shape(2, br);
        el.style.set_corner_side_shape(3, bl);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;
    use crate::layout::dimension::{dim_length_px, lp_length_px};

    #[test]
    fn pending_inspector_edits_queue_and_take() {
        let _ = take_pending_inspector_edits(); // clear
        queue_inspector_cssom(vec![2], "padding: 8px;".into());
        let edits = take_pending_inspector_edits();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].path, vec![2]);
        assert_eq!(edits[0].css, "padding: 8px;");
        assert!(take_pending_inspector_edits().is_empty());
    }

    #[test]
    fn committed_overrides_reapply_after_style_wipe() {
        clear_committed_inspector_edits_for_test();
        let mut child = Element::new();
        let _ = crate::style::apply_class_list(&mut child, "p-2");
        let mut root = Element::new().child(child);
        commit_inspector_cssom(&[0], "padding: 20px;".into());
        // Simulate chrome / variant restore wiping live style back to class base.
        if let Some(cs) = root.children[0].class_set.as_ref() {
            if let Some(base) = cs.base_style.as_ref() {
                root.children[0].style = base.clone();
            }
        }
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        let _ = reapply_inspector_overrides(&mut root, &mut layout);
        assert_eq!(
            crate::layout::dimension::lp_length_px(root.children[0].style.padding.top),
            Some(20.0)
        );
        clear_committed_inspector_edits_for_test();
    }

    #[test]
    fn padding_and_bg_patch() {
        let mut el = Element::new();
        apply_style_patch(&mut el, &StylePatch::PaddingUniform(8.0));
        assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
        apply_style_patch(&mut el, &StylePatch::BackgroundToken("#ff0000".into()));
        assert!((el.style.bg_color[0] - 1.0).abs() < 0.02);
        apply_style_patch(&mut el, &StylePatch::WidthPx(120.0));
        assert_eq!(dim_length_px(el.style.size.width), Some(120.0));
        apply_style_patch(&mut el, &StylePatch::Opacity(0.5));
        assert!((el.style.opacity - 0.5).abs() < 0.001);
    }

    #[test]
    fn patch_at_path() {
        let mut root = Element::new().child(Element::new().focus_id("a"));
        assert!(patch_element_at_path(&mut root, &vec![0], StylePatch::FlexGrow(2.0)).is_some());
        assert!((root.children[0].style.flex_grow - 2.0).abs() < 0.001);
    }

    #[test]
    fn cssom_updates_class_set_base_style() {
        let mut el = Element::new();
        let _ = crate::style::apply_class_list(&mut el, "p-2 bg-red-500");
        let before = el
            .class_set
            .as_ref()
            .and_then(|cs| cs.base_style.as_ref())
            .map(|s| s.bg_color)
            .expect("base_style");
        apply_cssom_text(&mut el, "background: #00ff00; padding: 20px;");
        let after = el
            .class_set
            .as_ref()
            .and_then(|cs| cs.base_style.as_ref())
            .expect("base_style after");
        assert!(
            (after.bg_color[1] - 1.0).abs() < 0.02,
            "base bg should be green"
        );
        assert_ne!(before, after.bg_color);
        assert_eq!(lp_length_px(after.padding.top), Some(20.0));
        // Authored dump must reflect the applied base (not a stale snapshot).
        let dump = crate::app::devtools::cssom_text_from_element(&el);
        assert!(dump.contains("background: #00ff00"), "dump={dump}");
    }

    #[test]
    fn cssom_text_applies_whitelist_and_layout() {
        let mut el = Element::new();
        let status = apply_cssom_text(
            &mut el,
            "padding: 8px; width: 120px; opacity: 0.5; flex-grow: 2; overflow: hidden; z-index: 4;",
        );
        assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
        assert_eq!(dim_length_px(el.style.size.width), Some(120.0));
        assert!((el.style.opacity - 0.5).abs() < 0.001);
        assert!((el.style.flex_grow - 2.0).abs() < 0.001);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(el.style.z_index, 4);
        assert_ne!(status, crate::core::tree::LayoutStatus::Clean);
    }

    #[test]
    fn cssom_unitless_is_px_not_rem() {
        let mut el = Element::new();
        apply_cssom_text(&mut el, "padding: 8; width: 40;");
        assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
        assert_eq!(dim_length_px(el.style.size.width), Some(40.0));
    }

    #[test]
    fn cssom_roundtrip_dump() {
        let mut el = Element::new();
        apply_cssom_text(&mut el, "padding: 8px; background: #ff0000; width: 100px;");
        let dump = crate::app::devtools::cssom_text_from_element(&el);
        let mut el2 = Element::new();
        apply_cssom_text(&mut el2, &dump);
        assert_eq!(lp_length_px(el2.style.padding.top), Some(8.0));
        assert_eq!(dim_length_px(el2.style.size.width), Some(100.0));
        assert!((el2.style.bg_color[0] - 1.0).abs() < 0.02);
    }
}
