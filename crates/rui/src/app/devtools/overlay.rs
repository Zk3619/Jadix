//! Absolute-position highlight boxes + Chrome-style box-model diagram.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::components::label;
use crate::core::tree::Element;
use crate::primitives::r#box;
use crate::style::{parse_color, parse_text_color, LineStyle};

use super::snapshot::{
    box_model_snapshot, find_element_by_path, find_element_by_template, find_element_by_template_mut,
    is_inspector_chrome, BoxModelSnapshot,
};
use super::state::{DevToolsState, HighlightLayer};

pub const HIGHLIGHT_SLOT_ID: &str = "__rui_devtools_highlight";
pub const HL_POSITION: &str = "__rui_devtools_hl_position";
pub const HL_MARGIN: &str = "__rui_devtools_hl_margin";
pub const HL_BORDER: &str = "__rui_devtools_hl_border";
pub const HL_PADDING: &str = "__rui_devtools_hl_padding";
pub const HL_CONTENT: &str = "__rui_devtools_hl_content";

const POSITION_C: [f32; 4] = [0.820, 0.820, 0.859, 1.0];
const MARGIN_C: [f32; 4] = [0.976, 0.800, 0.616, 1.0];
const BORDER_C: [f32; 4] = [0.988, 0.886, 0.608, 1.0];
const PADDING_C: [f32; 4] = [0.769, 0.875, 0.722, 1.0];
const CONTENT_C: [f32; 4] = [0.639, 0.773, 0.961, 1.0];

const BM_RING_POSITION: &str = "dt-bm-ring-position";
const BM_RING_MARGIN: &str = "dt-bm-ring-margin";
const BM_RING_BORDER: &str = "dt-bm-ring-border";
const BM_RING_PADDING: &str = "dt-bm-ring-padding";
const BM_RING_CONTENT: &str = "dt-bm-ring-content";

thread_local! {
    static BM_PATCHED_LAYER: Cell<u8> = const { Cell::new(u8::MAX) };
    static BM_PATCHED_GEN: Cell<u32> = const { Cell::new(u32::MAX) };
}

thread_local! {
    /// Set for the duration of layout collect so highlight sync can resolve paths.
    static HIGHLIGHT_COLLECT_ROOT: Cell<*const Element> = const { Cell::new(std::ptr::null()) };
    /// Per-collect stamped geometry — paint reads this, not stale `Element.layout_bounds`.
    static HIGHLIGHT_COLLECT_CACHE: RefCell<HighlightCollectCache> =
        RefCell::new(HighlightCollectCache::default());
}

#[derive(Clone, Debug, Default)]
struct HighlightCollectCache {
    selection_key: String,
    layers: HashMap<String, [f32; 4]>,
}

impl HighlightCollectCache {
    fn clear(&mut self) {
        self.selection_key.clear();
        self.layers.clear();
    }
}

/// Clear cached highlight geometry at the start of each layout collect pass.
pub(crate) fn begin_highlight_collect() {
    HIGHLIGHT_COLLECT_CACHE.with(|c| c.borrow_mut().clear());
}

/// Set for the duration of layout collect so highlight sync can resolve paths.
pub(crate) fn set_highlight_collect_root(root: &Element) {
    HIGHLIGHT_COLLECT_ROOT.set(root as *const Element);
}

pub(crate) fn clear_highlight_collect_root() {
    HIGHLIGHT_COLLECT_ROOT.set(std::ptr::null());
    begin_highlight_collect();
}

fn highlight_collect_root() -> Option<&'static Element> {
    let ptr = HIGHLIGHT_COLLECT_ROOT.get();
    if ptr.is_null() {
        None
    } else {
        // SAFETY: pointer set for one collect pass on the UI thread; cleared before return.
        Some(unsafe { &*ptr })
    }
}

/// Persistent highlight rects (mutated after layout so they follow resize).
pub fn highlight_slot() -> Element {
    let mut layer = Element::new()
        .absolute()
        .inset(0.0)
        .w("100%")
        .h("100%")
        .pointer_events("none")
        .template_id(HIGHLIGHT_SLOT_ID);
    layer = layer.child(empty_rect(HL_POSITION));
    layer = layer.child(empty_rect(HL_MARGIN));
    layer = layer.child(empty_rect(HL_BORDER));
    layer = layer.child(empty_rect(HL_PADDING));
    layer = layer.child(empty_rect(HL_CONTENT));
    layer
}

fn empty_rect(id: &str) -> Element {
    Element::new()
        .absolute()
        .left(0.0)
        .top(0.0)
        .w(0.0)
        .h(0.0)
        .pointer_events("none")
        .template_id(id)
}

/// Stamp highlight rects from the selected node's box model (fallback / tests).
///
/// Also fills [`HIGHLIGHT_COLLECT_CACHE`] so paint via [`stamped_highlight_abs_rect`]
/// works even when this is called outside layout collect (e.g. pick hover).
pub fn refresh_highlight(root: &mut Element) -> bool {
    let selection_key = DevToolsState::hover_path_key();
    let bm = if DevToolsState::is_open() && !selection_key.is_empty() {
        let path = DevToolsState::highlight_path();
        find_element_by_path(root, &path)
            .filter(|n| !is_inspector_chrome(n))
            .map(box_model_snapshot)
    } else {
        None
    };
    let Some(slot) = find_element_by_template_mut(root, HIGHLIGHT_SLOT_ID) else {
        return false;
    };
    let origin = [slot.layout_bounds[0], slot.layout_bounds[1]];
    match bm {
        Some(bm) if bm.width > 0.0 || bm.height > 0.0 => {
            HIGHLIGHT_COLLECT_CACHE.with(|c| {
                let mut cache = c.borrow_mut();
                cache.selection_key = selection_key;
                cache.layers.clear();
            });
            apply_layers(slot, &bm, origin, DevToolsState::highlight_layer())
        }
        _ => clear_rects(slot),
    }
}

/// During layout collect: stamp highlight **before** highlight children are painted.
pub fn refresh_highlight_slot(slot: &mut Element) -> bool {
    let Some(tree_root) = highlight_collect_root() else {
        return false;
    };
    refresh_highlight_slot_with_root(tree_root, slot)
}

fn refresh_highlight_slot_with_root(tree_root: &Element, slot: &mut Element) -> bool {
    let origin = [slot.layout_bounds[0], slot.layout_bounds[1]];
    let selection_key = DevToolsState::hover_path_key();
    let bm = if DevToolsState::is_open() && !selection_key.is_empty() {
        let path = DevToolsState::highlight_path();
        find_element_by_path(tree_root, &path)
            .filter(|n| !is_inspector_chrome(n))
            .map(box_model_snapshot)
    } else {
        None
    };
    HIGHLIGHT_COLLECT_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.selection_key = selection_key;
        cache.layers.clear();
    });
    match bm {
        Some(bm) if bm.width > 0.0 || bm.height > 0.0 => {
            apply_layers(slot, &bm, origin, DevToolsState::highlight_layer())
        }
        _ => clear_rects(slot),
    }
}

fn clear_rects(slot: &mut Element) -> bool {
    HIGHLIGHT_COLLECT_CACHE.with(|c| c.borrow_mut().clear());
    let mut changed = false;
    for child in &mut slot.children {
        if child.layout_bounds[2] > 0.0 || child.layout_bounds[3] > 0.0 {
            changed = true;
        }
        stamp_rect(child, 0.0, 0.0, 0.0, 0.0, [0.0, 0.0], [0.0, 0.0, 0.0, 0.0]);
        child.attrs.remove("data-rui-devtools-hl-stamped");
    }
    changed
}

fn apply_layers(
    slot: &mut Element,
    bm: &BoxModelSnapshot,
    origin: [f32; 2],
    layer: HighlightLayer,
) -> bool {
    let margin = [
        bm.x - bm.margin_left,
        bm.y - bm.margin_top,
        (bm.width + bm.margin_left + bm.margin_right).max(0.0),
        (bm.height + bm.margin_top + bm.margin_bottom).max(0.0),
    ];
    let position = [
        margin[0] - bm.position_left,
        margin[1] - bm.position_top,
        (margin[2] + bm.position_left + bm.position_right).max(0.0),
        (margin[3] + bm.position_top + bm.position_bottom).max(0.0),
    ];
    let border = [bm.x, bm.y, bm.width.max(0.0), bm.height.max(0.0)];
    let padding = [
        bm.x + bm.border_left,
        bm.y + bm.border_top,
        (bm.width - bm.border_left - bm.border_right).max(0.0),
        (bm.height - bm.border_top - bm.border_bottom).max(0.0),
    ];
    let content = [
        bm.x + bm.border_left + bm.padding_left,
        bm.y + bm.border_top + bm.padding_top,
        (bm.width - bm.border_left - bm.border_right - bm.padding_left - bm.padding_right).max(0.0),
        (bm.height - bm.border_top - bm.border_bottom - bm.padding_top - bm.padding_bottom)
            .max(0.0),
    ];
    let mut changed = false;
    for child in &mut slot.children {
        let id = child.template_id.as_deref().unwrap_or("");
        let (rect, color, this_layer, dashed, dotted) = match id {
            HL_POSITION => (
                position,
                POSITION_C,
                HighlightLayer::Position,
                false,
                true,
            ),
            HL_MARGIN => (margin, MARGIN_C, HighlightLayer::Margin, true, false),
            HL_BORDER => (border, BORDER_C, HighlightLayer::Border, false, false),
            HL_PADDING => (padding, PADDING_C, HighlightLayer::Padding, true, false),
            HL_CONTENT => (content, CONTENT_C, HighlightLayer::Content, false, false),
            _ => continue,
        };
        let layer_id = id.to_string();
        let show = layer == HighlightLayer::All || layer == this_layer;
        let a = if show {
            if layer == HighlightLayer::All {
                0.18
            } else {
                0.28
            }
        } else {
            0.0
        };
        let fill = [color[0], color[1], color[2], a];
        let bw = if show && this_layer == HighlightLayer::Content {
            2.0
        } else if show {
            1.0
        } else {
            0.0
        };
        changed |= stamp_rect(child, rect[0], rect[1], rect[2], rect[3], origin, fill);
        HIGHLIGHT_COLLECT_CACHE.with(|c| {
            c.borrow_mut()
                .layers
                .insert(layer_id, [rect[0], rect[1], rect[2], rect[3]]);
        });
        child.style.border_width = bw;
        child.style.border_color = if show {
            color
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };
        child.style.border_style = LineStyle::Solid;
        child.set_attr(
            "data-rui-devtools-hl-style",
            if dotted {
                "dotted"
            } else if dashed {
                "dashed"
            } else {
                "solid"
            },
        );
    }
    changed
}

fn stamp_rect(
    el: &mut Element,
    abs_x: f32,
    abs_y: f32,
    w: f32,
    h: f32,
    origin: [f32; 2],
    fill: [f32; 4],
) -> bool {
    let rel_x = abs_x - origin[0];
    let rel_y = abs_y - origin[1];
    let changed = (el.layout_bounds[0] - abs_x).abs() > 0.5
        || (el.layout_bounds[1] - abs_y).abs() > 0.5
        || (el.layout_bounds[2] - w).abs() > 0.5
        || (el.layout_bounds[3] - h).abs() > 0.5;
    el.style.position = taffy::style::Position::Absolute;
    el.style.set_css_dim(
        crate::style::UnitSlot::InsetL,
        crate::layout::dimension::CssDim::Px(rel_x),
    );
    el.style.set_css_dim(
        crate::style::UnitSlot::InsetT,
        crate::layout::dimension::CssDim::Px(rel_y),
    );
    el.style.set_css_dim(
        crate::style::UnitSlot::Width,
        crate::layout::dimension::CssDim::Px(w.max(0.0)),
    );
    el.style.set_css_dim(
        crate::style::UnitSlot::Height,
        crate::layout::dimension::CssDim::Px(h.max(0.0)),
    );
    el.style.bg_color = fill;
    el.layout_bounds = [abs_x, abs_y, w.max(0.0), h.max(0.0)];
    el.set_attr("data-rui-devtools-hl-stamped", "1");
    changed
}

/// Inspector highlight rects are stamped during collect; paint must use the collect cache —
/// retained `Element.layout_bounds` / attrs can still describe the previous selection.
pub(crate) fn stamped_highlight_abs_rect(el: &Element) -> Option<[f32; 4]> {
    let id = el.template_id.as_deref()?;
    if !id.starts_with("__rui_devtools_hl_") {
        return None;
    }
    let want_key = DevToolsState::hover_path_key();
    HIGHLIGHT_COLLECT_CACHE.with(|c| {
        let cache = c.borrow();
        if cache.selection_key != want_key {
            return None;
        }
        cache.layers.get(id).copied()
    })
}

thread_local! {
    static LAST_HOVER_ROW_KEY: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Patch tree-row background colours when the hover key changes, without rebuilding the panel.
pub fn sync_tree_row_hovers(root: &mut Element) -> bool {
    if !DevToolsState::is_open() {
        return false;
    }
    let hover_key = DevToolsState::hover_path_key();
    let already = LAST_HOVER_ROW_KEY.with(|c| c.borrow().clone());
    if hover_key == already {
        return false;
    }
    LAST_HOVER_ROW_KEY.with(|c| *c.borrow_mut() = hover_key.clone());
    // Use the raw PATH key — `path_to_key(selected_path())` collapses “none” and root to the same value.
    let selected_key = DevToolsState::selected_path_key();
    let muted = parse_color("muted");
    let accent = parse_color("accent");
    let transparent = [0.0_f32; 4];
    patch_row_bgs(root, &hover_key, &selected_key, muted, accent, transparent)
}

pub fn reset_tree_row_hover_cache() {
    LAST_HOVER_ROW_KEY.with(|c| *c.borrow_mut() = "\x00".to_string());
}

fn patch_row_bgs(
    el: &mut Element,
    hover_key: &str,
    selected_key: &str,
    muted: [f32; 4],
    accent: [f32; 4],
    transparent: [f32; 4],
) -> bool {
    let mut changed = false;
    if let Some(id) = el.template_id.as_deref() {
        if let Some(row_key) = id.strip_prefix("dt-row-") {
            let want = if !selected_key.is_empty() && row_key == selected_key {
                accent
            } else if !hover_key.is_empty() && row_key == hover_key {
                muted
            } else {
                transparent
            };
            if el.style.bg_color != want {
                el.style.bg_color = want;
                changed = true;
            }
        }
    }
    for child in &mut el.children {
        changed |= patch_row_bgs(child, hover_key, selected_key, muted, accent, transparent);
    }
    changed
}

/// Scroll id for the Elements-tree card (`inspector_card("Elements")`).
pub(crate) const ELEMENTS_TREE_SCROLL_ID: &str = "devtools-card-elements";

/// Target scroll offset that centers `[item_top, item_top+item_h)` in the viewport.
pub(crate) fn scroll_offset_to_center_rect(
    item_top: f32,
    item_h: f32,
    viewport_h: f32,
    content_h: f32,
) -> f32 {
    let max_sy = (content_h - viewport_h).max(0.0);
    let center = item_top + item_h * 0.5;
    (center - viewport_h * 0.5).clamp(0.0, max_sy)
}

/// Scroll the Elements tree so the row for `path_key` sits near the viewport center.
///
/// Uses laid-out bounds + scroll metrics signals. Returns `true` when the scroll
/// target changed (caller should request a paint frame).
pub(crate) fn scroll_elements_tree_row_into_center(
    root: &Element,
    path_key: &str,
) -> bool {
    if path_key.is_empty() {
        return false;
    }
    let row_id = format!("dt-row-{path_key}");
    let Some(row) = find_element_by_template(root, &row_id) else {
        return false;
    };
    let focus = crate::core::scroll_viewport::focus_id_for_scroll_viewport(ELEMENTS_TREE_SCROLL_ID);
    let Some(viewport) = find_element_by_focus_id(root, &focus) else {
        return false;
    };

    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(ELEMENTS_TREE_SCROLL_ID);
    let vh_sig = crate::core::reactivity::use_signal(keys.viewport_h.as_str(), || 0.0_f32).peek();
    let ch_sig = crate::core::reactivity::use_signal(keys.content_h.as_str(), || 0.0_f32).peek();
    let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32).peek();
    let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32).peek();

    let viewport_h = if vh_sig > 1.0 {
        vh_sig
    } else {
        viewport.layout_bounds[3]
    };
    let content_h = if ch_sig > 1.0 {
        ch_sig
    } else {
        // Fallback before metrics settle: at least cover the hovered row.
        (row.layout_bounds[1] - viewport.layout_bounds[1] + row.layout_bounds[3] + viewport_h)
            .max(viewport_h)
    };
    if viewport_h <= 1.0 {
        return false;
    }

    // layout_bounds are unscrolled absolute; content Y is relative to the viewport top.
    let item_top = row.layout_bounds[1] - viewport.layout_bounds[1];
    let item_h = row.layout_bounds[3].max(1.0);
    let target = scroll_offset_to_center_rect(item_top, item_h, viewport_h, content_h);
    if (sy - target).abs() < 1.0 {
        return false;
    }
    // Immediate jump so pick hover can track the pointer without lagging smooth scroll.
    crate::app::scroll_support::set_scroll_offset_with_behavior(
        ELEMENTS_TREE_SCROLL_ID,
        sx,
        target,
        true,
    );
    true
}

fn find_element_by_focus_id<'a>(root: &'a Element, id: &str) -> Option<&'a Element> {
    if root.focus_id.as_deref() == Some(id) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_element_by_focus_id(child, id) {
            return Some(found);
        }
    }
    None
}

/// Invalidate cached box-model ring fills after the inspector panel rebuilds.
pub fn reset_box_model_visual_cache() {
    BM_PATCHED_LAYER.set(u8::MAX);
}

/// Patch box-model ring fills when [`HighlightLayer`] changes without rebuilding the panel.
pub fn sync_box_model_visuals(root: &mut Element) -> bool {
    if !DevToolsState::is_open() {
        return false;
    }
    let gen = DevToolsState::generation();
    if BM_PATCHED_GEN.get() != gen {
        BM_PATCHED_GEN.set(gen);
        BM_PATCHED_LAYER.set(u8::MAX);
    }
    let active = DevToolsState::highlight_layer();
    let v = active as u8;
    if BM_PATCHED_LAYER.get() == v {
        return false;
    }
    let rings = [
        ("position", HighlightLayer::Position, POSITION_C),
        ("margin", HighlightLayer::Margin, MARGIN_C),
        ("border", HighlightLayer::Border, BORDER_C),
        ("padding", HighlightLayer::Padding, PADDING_C),
    ];
    let mut changed = false;
    for (caption, layer, color) in rings {
        changed |= sync_ring_strip_fills(root, caption, layer, color, active);
    }
    if let Some(el) = find_element_by_template_mut(root, BM_RING_CONTENT) {
        let fill = ring_fill(CONTENT_C, active, HighlightLayer::Content);
        if el.style.bg_color != fill {
            el.style.bg_color = fill;
            changed = true;
        }
    }
    BM_PATCHED_LAYER.set(v);
    changed
}

fn sync_ring_strip_fills(
    root: &mut Element,
    caption: &str,
    layer: HighlightLayer,
    color: [f32; 4],
    active: HighlightLayer,
) -> bool {
    let fill = ring_fill(color, active, layer);
    let mut changed = false;
    for suffix in ["t", "b", "l", "r"] {
        let id = format!("dt-bm-fill-{caption}-{suffix}");
        let Some(el) = find_element_by_template_mut(root, &id) else {
            continue;
        };
        if el.style.bg_color != fill {
            el.style.bg_color = fill;
            changed = true;
        }
    }
    changed
}

fn content_size(bm: &BoxModelSnapshot) -> (f32, f32) {
    let w = (bm.width
        - bm.border_left
        - bm.border_right
        - bm.padding_left
        - bm.padding_right)
        .max(0.0);
    let h = (bm.height
        - bm.border_top
        - bm.border_bottom
        - bm.padding_top
        - bm.padding_bottom)
        .max(0.0);
    (w, h)
}

/// Fixed visual ring thickness — enough room for the edge-value labels inside each band.
fn diagram_pad(sides: [f32; 4]) -> [f32; 4] {
    sides.map(|v| if v > 0.0 { 18.0 } else { 14.0 })
}

fn ring_border_style(layer: HighlightLayer) -> &'static str {
    match layer {
        HighlightLayer::Position | HighlightLayer::Margin | HighlightLayer::Padding => "dashed",
        _ => "solid",
    }
}

/// Nested position → margin → border → padding → content diagram (Chrome Computed tab).
pub fn box_model_diagram(bm: &BoxModelSnapshot, active: HighlightLayer) -> Element {
    let content = content_box(bm, active);
    let padding = ring_shell(
        HighlightLayer::Padding,
        active,
        PADDING_C,
        "padding",
        BM_RING_PADDING,
        [
            bm.padding_top,
            bm.padding_right,
            bm.padding_bottom,
            bm.padding_left,
        ],
        content,
    );
    let border = ring_shell(
        HighlightLayer::Border,
        active,
        BORDER_C,
        "border",
        BM_RING_BORDER,
        [
            bm.border_top,
            bm.border_right,
            bm.border_bottom,
            bm.border_left,
        ],
        padding,
    );
    let margin = ring_shell(
        HighlightLayer::Margin,
        active,
        MARGIN_C,
        "margin",
        BM_RING_MARGIN,
        [
            bm.margin_top,
            bm.margin_right,
            bm.margin_bottom,
            bm.margin_left,
        ],
        border,
    );
    let position = ring_shell(
        HighlightLayer::Position,
        active,
        POSITION_C,
        "position",
        BM_RING_POSITION,
        [
            bm.position_top,
            bm.position_right,
            bm.position_bottom,
            bm.position_left,
        ],
        margin,
    );

    let diagram = r#box()
        .id("dt-bm-diagram")
        .w("100%")
        .px("0.5rem")
        .py("0.35rem")
        .child(r#box().w("100%").child(position));

    crate::primitives::interactive::interactive(diagram.into())
        .on_hover(|on| {
            if !on {
                DevToolsState::set_highlight_layer(HighlightLayer::All);
                let selected = DevToolsState::selected_path();
                if !selected.is_empty() {
                    DevToolsState::clear_hover_highlight_if_path(&selected);
                }
            }
        })
        .into()
}

fn ring_visible(active: HighlightLayer, layer: HighlightLayer) -> bool {
    active == HighlightLayer::All || active == layer
}

fn ring_fill(color: [f32; 4], active: HighlightLayer, layer: HighlightLayer) -> [f32; 4] {
    if !ring_visible(active, layer) {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let a = if active == HighlightLayer::All {
        0.15
    } else {
        0.32
    };
    [color[0], color[1], color[2], a]
}

fn edge_label(id: &str, text: String) -> Element {
    label(id)
        .text(text)
        .text_size("0.625rem")
        .color(parse_text_color("foreground"))
        .into()
}

fn content_box(bm: &BoxModelSnapshot, active: HighlightLayer) -> Element {
    let (cw, ch) = content_size(bm);
    let border_c = parse_color("primary");
    let shell = r#box()
        .id(BM_RING_CONTENT)
        .relative(true)
        .w("100%")
        .min_h("1.5rem")
        .items("center")
        .justify("center")
        .bg(ring_fill(CONTENT_C, active, HighlightLayer::Content))
        .border_width(1.0)
        .border_style("solid")
        .border_color(border_c)
        .child(
            label("dt-bm-content-size")
                .text(format!("{:.0} × {:.0}", cw, ch))
                .text_size("0.6875rem")
                .font_weight("600")
                .text_center(),
        );
    wrap_ring_interactive(shell.into(), HighlightLayer::Content)
}

fn ring_fill_strip(
    caption: &str,
    suffix: &str,
    fill: [f32; 4],
    side: &str,
    value: f32,
    pad: [f32; 4],
) -> Element {
    let [pt, pr, pb, pl] = pad;
    let id = format!("dt-bm-fill-{caption}-{suffix}");
    let label_text = format!("{:.0}", value);
    let mut strip = r#box()
        .id(id)
        .absolute(true)
        .bg(fill)
        .pointer_events("none")
        .items("center")
        .justify("center");
    strip = match side {
        "t" => strip.left(0.0).top(0.0).w("100%").h(pt),
        "b" => strip.left(0.0).bottom(0.0).w("100%").h(pb),
        "l" => strip.left(0.0).top(pt).bottom(pb).w(pl),
        "r" => strip.right(0.0).top(pt).bottom(pb).w(pr),
        _ => strip,
    };
    strip.child(edge_label(&format!("dt-bm-{suffix}-{caption}"), label_text))
        .into()
}

fn ring_shell(
    layer: HighlightLayer,
    active: HighlightLayer,
    color: [f32; 4],
    caption: &str,
    template_id: &str,
    sides: [f32; 4],
    inner: Element,
) -> Element {
    let pad = diagram_pad(sides);
    let [pt, pr, pb, pl] = pad;
    let fill = ring_fill(color, active, layer);
    let border_c = parse_color("primary");

    let cap: Element = label(format!("dt-bm-cap-{caption}"))
        .text(caption)
        .text_size("0.625rem")
        .color(parse_text_color("foreground"))
        .into();

    // Normal flow: padding creates the ring gap, inner nests naturally.
    // Absolute-positioned fill strips paint the ring band without colouring inner area.
    let shell = r#box()
        .id(template_id)
        .relative(true)
        .w("100%")
        .pt(pt)
        .pr(pr)
        .pb(pb)
        .pl(pl)
        .border_width(1.0)
        .border_color(border_c)
        .border_style(ring_border_style(layer))
        .children([
            ring_fill_strip(caption, "t", fill, "t", sides[0], pad),
            ring_fill_strip(caption, "b", fill, "b", sides[2], pad),
            ring_fill_strip(caption, "l", fill, "l", sides[3], pad),
            ring_fill_strip(caption, "r", fill, "r", sides[1], pad),
            Element::from(
                r#box()
                    .absolute(true)
                    .left("0.125rem")
                    .top("0.0625rem")
                    .pointer_events("none")
                    .child(cap),
            ),
            inner,
        ]);

    wrap_ring_interactive(shell.into(), layer)
}

fn wrap_ring_interactive(el: Element, layer: HighlightLayer) -> Element {
    crate::primitives::interactive::interactive(el)
        .on_click(move || {
            DevToolsState::set_highlight_layer(layer);
            let selected = DevToolsState::selected_path();
            if !selected.is_empty() {
                DevToolsState::set_hover_highlight_path(selected);
            }
        })
        .on_hover(move |on| {
            if on {
                DevToolsState::set_highlight_layer(layer);
                let selected = DevToolsState::selected_path();
                if !selected.is_empty() {
                    DevToolsState::set_hover_highlight_path(selected);
                }
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::devtools::state::DevToolsState;

    fn test_tree() -> Element {
        let mut target_a = Element::new().template_id("node-a");
        target_a.layout_bounds = [10.0, 20.0, 100.0, 30.0];
        let mut target_b = Element::new().template_id("node-b");
        target_b.layout_bounds = [10.0, 80.0, 100.0, 30.0];
        let mut app = Element::new().children(vec![target_a, target_b]);
        app.template_id = Some("app".into());
        let mut host = Element::new().children(vec![app, highlight_slot()]);
        host.template_id = Some("__rui_app_host".into());
        Element::new().child(host)
    }

    #[test]
    fn highlight_collect_cache_tracks_selection_change() {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut root = test_tree();
        // Make sure that if `highlight_path` is empty we *don't* accidentally highlight root.
        root.layout_bounds = [0.0, 0.0, 800.0, 600.0];
        begin_highlight_collect();
        set_highlight_collect_root(&root);
        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 800.0, 600.0];

        DevToolsState::set_hover_highlight_path(vec![0, 0, 0]);
        assert!(refresh_highlight_slot(slot));
        let a = stamped_highlight_abs_rect(&slot.children[4]).expect("content layer");
        assert!((a[1] - 20.0).abs() < 0.5, "first hover y={}", a[1]);

        DevToolsState::set_hover_highlight_path(vec![0, 0, 1]);
        assert!(refresh_highlight_slot(slot));
        let b = stamped_highlight_abs_rect(&slot.children[4]).expect("content layer");
        assert!((b[1] - 80.0).abs() < 0.5, "hover highlight y={}", b[1]);
        assert!((a[1] - b[1]).abs() > 10.0);

        DevToolsState::clear_hover_highlight_if_path(&vec![0, 0, 1]);
        assert!(!DevToolsState::has_hover_highlight());
        assert!(DevToolsState::highlight_path().is_empty());
        assert!(refresh_highlight_slot(slot));
        assert!(stamped_highlight_abs_rect(&slot.children[4]).is_none());

        clear_highlight_collect_root();
    }

    #[test]
    fn sync_tree_row_hovers_does_not_stick_root_when_none_selected() {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::clear_hover_highlight();
        // Non-root selection so selected_path_key is not empty and not ROOT_PATH_KEY.
        DevToolsState::set_selected_path(vec![0]);
        reset_tree_row_hover_cache();

        let muted = parse_color("muted");
        let accent = parse_color("accent");
        let transparent = [0.0_f32; 4];
        let root_id = format!("dt-row-{}", crate::app::devtools::snapshot::ROOT_PATH_KEY);
        let mut root = Element::new().children(vec![
            Element::new().template_id(root_id.as_str()),
            Element::new().template_id("dt-row-0,1"),
        ]);

        // Empty selected_key must never match the root row (the original bug).
        assert!(patch_row_bgs(
            &mut root,
            "0,1",
            "",
            muted,
            accent,
            transparent
        ));
        assert_eq!(root.children[0].style.bg_color, transparent);
        assert_eq!(root.children[1].style.bg_color, muted);

        // Leave hover via state sync: sticky root must clear; only the hovered child stays muted.
        root.children[0].style.bg_color = muted;
        root.children[1].style.bg_color = muted;
        reset_tree_row_hover_cache();
        DevToolsState::set_hover_highlight_path(vec![0, 1]);
        let _ = sync_tree_row_hovers(&mut root);
        assert_eq!(root.children[0].style.bg_color, transparent);
        assert_eq!(root.children[1].style.bg_color, muted);

        DevToolsState::clear_hover_highlight();
        assert!(sync_tree_row_hovers(&mut root));
        assert_eq!(root.children[0].style.bg_color, transparent);
        assert_eq!(root.children[1].style.bg_color, transparent);
    }

    #[test]
    fn scroll_offset_centers_row_and_clamps() {
        // Viewport 100, content 400, row at 200..220 → center at 210 → scroll 160.
        assert!((scroll_offset_to_center_rect(200.0, 20.0, 100.0, 400.0) - 160.0).abs() < 0.01);
        // Near top clamps to 0.
        assert_eq!(scroll_offset_to_center_rect(0.0, 20.0, 100.0, 400.0), 0.0);
        // Near bottom clamps to max.
        let max = 300.0;
        assert!((scroll_offset_to_center_rect(380.0, 20.0, 100.0, 400.0) - max).abs() < 0.01);
    }
}
