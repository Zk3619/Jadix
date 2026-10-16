//! Absolute-position highlight boxes + Chrome-style box-model diagram.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::components::label;
use crate::core::tree::Element;
use crate::primitives::r#box;
use crate::style::{parse_color, parse_text_color, LineStyle};

use super::snapshot::{
    border_box_size_matches, box_model_snapshot, find_element_by_identity_near_size,
    find_element_by_path, find_element_by_size_near, find_element_by_template,
    find_element_by_template_mut, is_inspector_chrome, BoxModelSnapshot,
};
use super::state::{DevToolsState, HighlightLayer};

pub const HIGHLIGHT_SLOT_ID: &str = "__jadix_devtools_highlight";
pub const HL_POSITION: &str = "__jadix_devtools_hl_position";
pub const HL_MARGIN: &str = "__jadix_devtools_hl_margin";
pub const HL_BORDER: &str = "__jadix_devtools_hl_border";
pub const HL_PADDING: &str = "__jadix_devtools_hl_padding";
pub const HL_CONTENT: &str = "__jadix_devtools_hl_content";

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
    /// Bumped each collect; elements stamp [`Element::paint_visual_gen`] with this value.
    static HIGHLIGHT_PAINT_GEN: Cell<u32> = const { Cell::new(1) };
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
    HIGHLIGHT_PAINT_GEN.set(HIGHLIGHT_PAINT_GEN.get().wrapping_add(1).max(1));
}

/// Generation of the in-progress / last collect paint-visual stamp.
pub(crate) fn highlight_paint_gen() -> u32 {
    HIGHLIGHT_PAINT_GEN.get()
}

/// Set for the duration of layout collect so highlight sync can resolve paths.
pub(crate) fn set_highlight_collect_root(root: &Element) {
    HIGHLIGHT_COLLECT_ROOT.set(root as *const Element);
}

pub(crate) fn clear_highlight_collect_root() {
    HIGHLIGHT_COLLECT_ROOT.set(std::ptr::null());
    // Keep layer cache until the next `begin_highlight_collect` so post-collect
    // readers (tests / same-frame refresh) still see the stamped rects.
}

/// Record the paint-space border box for this node (same space as instances).
pub(crate) fn note_element_paint_visual(el: &mut Element, visual_border: [f32; 4]) {
    el.paint_visual_bounds = visual_border;
    el.paint_visual_gen = HIGHLIGHT_PAINT_GEN.get();
}

/// Synthesize paint-space bounds for one node from abs layout − `scroll_acc`.
///
/// Callers that skip a whole subtree (band-cull / clip-cull) must also refresh
/// descendants via Taffy — do **not** reuse stored `layout_bounds` after wrap reflow
/// (that mixed old abs with new scroll and stamped the highlight in the wrong place).
pub(crate) fn note_element_paint_visual_from_abs(
    el: &mut Element,
    abs: [f32; 2],
    size: [f32; 2],
    scroll_acc: [f32; 2],
) {
    let w = size[0].max(0.0);
    let h = size[1].max(0.0);
    let tx = el.style.translate[0] + w * el.style.translate_pct[0];
    let ty = el.style.translate[1] + h * el.style.translate_pct[1];
    let vx = abs[0] - scroll_acc[0] + el.sticky_visual_offset[0] + tx;
    let vy = abs[1] - scroll_acc[1] + el.sticky_visual_offset[1] + ty;
    note_element_paint_visual(el, [vx, vy, w, h]);
    // Keep abs layout in sync so size-hint matching sees the same border box.
    el.layout_bounds = [abs[0], abs[1], w, h];
}

/// After layout collect finishes, re-stamp the highlight using final paint_visual / abs−scroll.
///
/// Mid-collect `refresh_highlight_slot` may have run before a scrolled target's paint_visual was
/// corrected (band-skip synthesize, wrap reflow). One post-pass removes the narrow+scroll drift.
pub(crate) fn finalize_highlight_after_collect(root: &mut Element) {
    if !DevToolsState::is_open() || DevToolsState::overlay_path_key().is_empty() {
        return;
    }
    let _ = refresh_highlight(root);
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

fn box_model_at_visual_border(el: &Element, border: [f32; 4]) -> BoxModelSnapshot {
    let mut bm = box_model_snapshot(el);
    bm.x = border[0];
    bm.y = border[1];
    bm.width = border[2].max(0.0);
    bm.height = border[3].max(0.0);
    bm
}

fn highlight_box_model(tree_root: &Element, path: &[usize]) -> Option<BoxModelSnapshot> {
    let el = resolve_highlight_element(tree_root, path)?;
    // Always compute abs−scroll along the *live* path (identity may outlive a stale snapshot path).
    let live_path = path_of_element(tree_root, el).unwrap_or_else(|| path.to_vec());
    let acc = crate::app::devtools::snapshot::scroll_acc_along_path(tree_root, &live_path);
    let from_scroll = crate::app::devtools::snapshot::box_model_snapshot_visual(el, acc);

    if el.paint_visual_gen == HIGHLIGHT_PAINT_GEN.get() && HIGHLIGHT_PAINT_GEN.get() != 0 {
        let from_paint = box_model_at_visual_border(el, el.paint_visual_bounds);
        // Narrow+scroll repro: paint_visual sometimes equals raw abs (forgot scroll_acc).
        // Prefer abs−scroll when paint looks unscrolled but ancestors actually scrolled.
        let abs_y = el.layout_bounds[1]
            + el.style.translate[1]
            + el.layout_bounds[3] * el.style.translate_pct[1]
            + el.sticky_visual_offset[1];
        let paint_y = el.paint_visual_bounds[1];
        let scrolled = acc[0].abs() > 1.0 || acc[1].abs() > 1.0;
        let paint_forgot_scroll =
            scrolled && (paint_y - abs_y).abs() < 2.0 && (paint_y - from_scroll.y).abs() > 2.0;
        let paint_forgot_scroll_x = scrolled
            && (el.paint_visual_bounds[0]
                - (el.layout_bounds[0]
                    + el.style.translate[0]
                    + el.layout_bounds[2] * el.style.translate_pct[0]
                    + el.sticky_visual_offset[0]))
                .abs()
                < 2.0
            && (el.paint_visual_bounds[0] - from_scroll.x).abs() > 2.0;
        // Wrap reflow mix: paint size/pos from an old flex line while layout_bounds are new
        // (or the reverse). Trust abs−scroll whenever paint and layout visual disagree and
        // the node is not scaled (scale is the main reason paint may legitimately differ).
        let scaled =
            (el.style.scale[0] - 1.0).abs() > 0.01 || (el.style.scale[1] - 1.0).abs() > 0.01;
        let paint_layout_mismatch = !scaled
            && ((from_paint.x - from_scroll.x).abs() > 2.0
                || (from_paint.y - from_scroll.y).abs() > 2.0
                || (from_paint.width - from_scroll.width).abs() > 2.0
                || (from_paint.height - from_scroll.height).abs() > 2.0);
        if paint_forgot_scroll || paint_forgot_scroll_x || paint_layout_mismatch {
            return Some(from_scroll);
        }
        return Some(from_paint);
    }
    Some(from_scroll)
}

/// Resolve the live node for the page overlay.
/// Prefer identity (+ relative) with snapshot size gate; absolute path is last resort.
fn resolve_highlight_element<'a>(tree_root: &'a Element, path: &[usize]) -> Option<&'a Element> {
    let identity = DevToolsState::overlay_identity();
    let relative = DevToolsState::overlay_relative_path();
    let hint = DevToolsState::overlay_size_hint();

    if !identity.is_empty() {
        if let Some(el) =
            find_element_by_identity_near_size(tree_root, &identity, path, &relative, hint)
                .filter(|n| !is_inspector_chrome(n))
        {
            return Some(el);
        }
    }

    if let Some(el) = find_element_by_path(tree_root, path).filter(|n| !is_inspector_chrome(n)) {
        if border_box_size_matches(el, hint) {
            return Some(el);
        }
    }

    find_element_by_size_near(tree_root, hint, path).filter(|n| !is_inspector_chrome(n))
}

fn path_of_element(root: &Element, target: &Element) -> Option<Vec<usize>> {
    crate::core::tree::find_path_in_tree(root, |el| std::ptr::eq(el, target))
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
    let selection_key = DevToolsState::overlay_path_key();
    let bm = if DevToolsState::is_open() && !selection_key.is_empty() {
        let path = DevToolsState::highlight_path();
        // Prefer last-collect paint rect when still current (avoids abs−scroll drift).
        highlight_box_model(root, &path)
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
    let selection_key = DevToolsState::overlay_path_key();
    let bm = if DevToolsState::is_open() && !selection_key.is_empty() {
        let path = DevToolsState::highlight_path();
        // Prefer paint-space rect captured while collecting the target (translate/sticky/scale).
        highlight_box_model(tree_root, &path)
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
        child.attrs.remove("data-jadix-devtools-hl-stamped");
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
            HL_POSITION => (position, POSITION_C, HighlightLayer::Position, false, true),
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
        child.style.border_color = if show { color } else { [0.0, 0.0, 0.0, 0.0] };
        child.style.border_style = LineStyle::Solid;
        child.set_attr(
            "data-jadix-devtools-hl-style",
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
    el.set_attr("data-jadix-devtools-hl-stamped", "1");
    changed
}

/// Inspector highlight rects are stamped during collect; paint must use the collect cache —
/// retained `Element.layout_bounds` / attrs can still describe the previous selection.
pub(crate) fn stamped_highlight_abs_rect(el: &Element) -> Option<[f32; 4]> {
    let id = el.template_id.as_deref()?;
    if !id.starts_with("__jadix_devtools_hl_") {
        return None;
    }
    let want_key = DevToolsState::overlay_path_key();
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
    crate::core::tree::walk_elements_mut(el, |el| {
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
    });
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
pub(crate) fn scroll_elements_tree_row_into_center(root: &Element, path_key: &str) -> bool {
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
    crate::core::tree::find_in_tree(root, |el| el.focus_id.as_deref() == Some(id))
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
    let w =
        (bm.width - bm.border_left - bm.border_right - bm.padding_left - bm.padding_right).max(0.0);
    let h = (bm.height - bm.border_top - bm.border_bottom - bm.padding_top - bm.padding_bottom)
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
    strip
        .child(edge_label(&format!("dt-bm-{suffix}-{caption}"), label_text))
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
    use crate::app::devtools::snapshot::find_element_by_path_mut;
    use crate::app::devtools::state::DevToolsState;

    fn test_tree() -> Element {
        let mut target_a = Element::new().template_id("node-a");
        target_a.layout_bounds = [10.0, 20.0, 100.0, 30.0];
        let mut target_b = Element::new().template_id("node-b");
        target_b.layout_bounds = [10.0, 80.0, 100.0, 30.0];
        let mut app = Element::new().children(vec![target_a, target_b]);
        app.template_id = Some("app".into());
        let mut host = Element::new().children(vec![app, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
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
    fn highlight_slot_prefers_noted_paint_visual_over_abs_scroll() {
        use taffy::style::Overflow;

        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut target = Element::new().template_id("btn");
        target.layout_bounds = [10.0, 250.0, 80.0, 40.0];
        // Paint uses abs − scroll + translate; reconstruction historically omitted translate.
        target.style.translate = [0.0, 12.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.scroll_offset = [0.0, 100.0];
        viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        viewport = viewport.child(target);

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        let mut root = Element::new().child(host);
        root.layout_bounds = [0.0, 0.0, 400.0, 200.0];

        let path = vec![0, 0, 0];
        DevToolsState::set_hover_highlight_path(path.clone());

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        // Same space as collect `render_rect` (250 − 100 + 12).
        {
            let target_ref = find_element_by_path_mut(&mut root, &path).expect("target");
            note_element_paint_visual(target_ref, [10.0, 162.0, 80.0, 40.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border layer");
        assert!(
            (border[1] - 162.0).abs() < 0.5,
            "noted paint visual must win over abs−scroll, got y={}",
            border[1]
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn stamped_highlight_prefers_element_paint_visual_bounds() {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut target = Element::new().template_id("card");
        target.layout_bounds = [20.0, 400.0, 128.0, 302.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 180.0];
        viewport = viewport.child(target);

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.style.overflow.y = taffy::style::Overflow::Hidden;
        host.scroll_offset = [0.0, 80.0];
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];

        let mut root = Element::new().child(host);
        let path = vec![0, 0, 0];
        DevToolsState::set_hover_highlight_path(path.clone());

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let t = find_element_by_path_mut(&mut root, &path).unwrap();
            // Paint space (already scrolled); must win over abs−(viewport+host) reconstruction.
            note_element_paint_visual(t, [20.0, 220.0, 128.0, 302.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 220.0).abs() < 0.5,
            "must keep paint visual y=220, got {}",
            border[1]
        );
        // Reconstruction that wrongly includes host overflow:hidden offset would land at 140.
        assert!(
            (border[1] - 140.0).abs() > 10.0,
            "must not double-subtract host scroll"
        );
        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_resolves_by_template_id_when_child_index_path_is_stale() {
        // Gallery repro: Elements row still shows card caption/tid, but live child indices
        // shifted so the snapshotted path now points at a button (Outline-sized).
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut card = Element::new()
            .template_id("examples/gallery/src/main.rs:191:5")
            .p(16.0);
        card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

        let mut outline = Element::new()
            .template_id("outline-btn")
            .focus_id("btn-outline");
        outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

        // Live tree: viewport[outline, card] — path [0,0,0] is Outline.
        let mut viewport = Element::new().children(vec![outline, card]);
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 40.0];

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut root = Element::new().child(host);

        // Stale path → Outline; identity → 1b card (what the tree row still labels).
        let stale_path = vec![0, 0, 0];
        DevToolsState::set_hover_highlight(
            stale_path,
            "examples/gallery/src/main.rs:191:5".to_string(),
            Vec::new(),
            (164.0, 334.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
            let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
            note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 320.0).abs() < 0.5 && (border[3] - 334.0).abs() < 0.5,
            "identity must win over stale path (Outline); got {:?}",
            border
        );
        assert!(
            (border[3] - 36.0).abs() > 10.0,
            "must not stamp the Outline-sized rect"
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_resolves_anonymous_child_via_identity_relative_path() {
        // Screenshot repro: hover the description Label under 1b card (no tid). Absolute
        // path drifted to Outline; anchor tid + relative [1,0] must still hit the label.
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut title = Element::new().text("1b) Hover");
        title.layout_bounds = [28.0, 370.0, 120.0, 24.0];
        let mut desc = Element::new().text("默认 hover_easing");
        desc.layout_bounds = [28.0, 400.0, 120.0, 36.0];
        let inner = Element::new().children(vec![desc]);
        let mut card = Element::new()
            .template_id("examples/gallery/src/main.rs:191:5")
            .children(vec![title, inner]);
        card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

        let mut outline = Element::new().template_id("outline-btn");
        outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

        let mut viewport = Element::new().children(vec![outline, card]);
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 40.0];

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut root = Element::new().child(host);

        // Absolute path points at Outline; relative [1,0] under card is the description.
        DevToolsState::set_hover_highlight(
            vec![0, 0, 0],
            "examples/gallery/src/main.rs:191:5".to_string(),
            vec![1, 0],
            (120.0, 36.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
            let desc = find_element_by_path_mut(&mut root, &[0, 0, 1, 1, 0]).unwrap();
            note_element_paint_visual(desc, [28.0, 360.0, 120.0, 36.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 360.0).abs() < 0.5 && (border[3] - 36.0).abs() < 0.5,
            "must highlight description label via relative path, got {:?}",
            border
        );
        assert!(
            (border[0] - 40.0).abs() > 1.0 || (border[2] - 72.0).abs() > 1.0,
            "must not stamp Outline geometry"
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn selecting_node_replaces_stale_pick_hover_on_overlay() {
        // Repro: Styles shows the clicked 1b card (128×298) while the page overlay still
        // stamps a leftover Ghost/Outline hover from pick / a previous tree row.
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut card = Element::new()
            .template_id(r"examples\gallery\src\main.rs:191:5")
            .p(16.0);
        card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

        let mut ghost = Element::new()
            .template_id("ghost-btn")
            .focus_id("btn-ghost");
        ghost.layout_bounds = [40.0, 120.0, 72.0, 36.0];

        let mut viewport = Element::new().children(vec![ghost, card]);
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 40.0];

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut root = Element::new().child(host);

        // Leftover pick hover on Ghost (absolute path, no useful identity).
        DevToolsState::set_hover_highlight_path(vec![0, 0, 0]);
        // Click the 1b card in the Elements tree (backslash tid as on Windows `file!()`).
        DevToolsState::set_selected(
            vec![0, 0, 1],
            r"examples\gallery\src\main.rs:191:5".to_string(),
            Vec::new(),
            (164.0, 334.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let ghost = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            note_element_paint_visual(ghost, [40.0, 80.0, 72.0, 36.0]);
            let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
            note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 320.0).abs() < 0.5 && (border[3] - 334.0).abs() < 0.5,
            "selection must replace stale Ghost hover; got {:?}",
            border
        );
        assert!(
            (border[3] - 36.0).abs() > 10.0,
            "must not keep the Ghost-sized overlay"
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn identity_match_normalizes_path_separators() {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut card = Element::new()
            .template_id("examples/gallery/src/main.rs:191:5")
            .p(16.0);
        card.layout_bounds = [20.0, 360.0, 164.0, 334.0];
        let mut outline = Element::new().template_id("outline-btn");
        outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];
        let viewport = Element::new().children(vec![outline, card]);
        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut root = Element::new().child(host);

        // Snapshot / UI shows Windows separators; live tree keeps forward slashes.
        DevToolsState::set_hover_highlight(
            vec![0, 0, 0],
            r"examples\gallery\src\main.rs:191:5".to_string(),
            Vec::new(),
            (164.0, 334.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            note_element_paint_visual(outline, [40.0, 80.0, 72.0, 36.0]);
            let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
            note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
        }
        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[3] - 334.0).abs() < 0.5,
            "backslash identity must match slash tid; got {:?}",
            border
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_size_hint_rejects_stale_path_without_identity() {
        // Screenshot: Styles shows 1b card 164×334 while absolute path drifted to Outline.
        // No usable identity — size hint must still find the card.
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut card = Element::new().p(16.0);
        card.layout_bounds = [20.0, 360.0, 164.0, 334.0];
        let mut outline = Element::new().template_id("outline-btn");
        outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

        let mut viewport = Element::new().children(vec![outline, card]);
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 40.0];

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut root = Element::new().child(host);

        DevToolsState::set_hover_highlight(
            vec![0, 0, 0],
            String::new(),
            Vec::new(),
            (164.0, 334.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
            let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
            note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
        }

        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 320.0).abs() < 0.5 && (border[3] - 334.0).abs() < 0.5,
            "size hint must reject Outline path; got {:?}",
            border
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_rejects_unscrolled_paint_visual_when_ancestors_scrolled() {
        // Narrow+scroll repro: paint_visual_gen is current but bounds equal raw abs
        // (scroll_acc was 0 when noted). Stamp must use abs−scroll, not the stale paint.
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        let mut target = Element::new()
            .focus_id("btn-default")
            .template_id("btn-default");
        target.layout_bounds = [20.0, 280.0, 140.0, 36.0];

        let mut viewport = Element::new().child(target);
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = taffy::style::Overflow::Scroll;
        viewport.scroll_offset = [0.0, 120.0];
        viewport.layout_bounds = [0.0, 0.0, 400.0, 500.0];

        let mut host = Element::new().children(vec![viewport, highlight_slot()]);
        host.template_id = Some("__jadix_app_host".into());
        host.layout_bounds = [0.0, 0.0, 400.0, 500.0];
        let mut root = Element::new().child(host);

        DevToolsState::set_hover_highlight(
            vec![0, 0, 0],
            "btn-default".to_string(),
            Vec::new(),
            (140.0, 36.0),
        );

        begin_highlight_collect();
        set_highlight_collect_root(&root);
        {
            let t = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
            // Bug shape: gen current, but Y is raw abs (280) instead of visual (160).
            note_element_paint_visual(t, [20.0, 280.0, 140.0, 36.0]);
        }
        let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
        slot.layout_bounds = [0.0, 0.0, 400.0, 500.0];
        assert!(refresh_highlight_slot(slot));
        let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
        assert!(
            (border[1] - 160.0).abs() < 0.5,
            "must prefer abs−scroll (160) over unscrolled paint (280); got {:?}",
            border
        );

        clear_highlight_collect_root();
        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_matches_paint_under_narrow_wrap_and_scroll() {
        // Gallery repro (user-confirmed): docked Inspector squeezes host → HStack wrap
        // stacks buttons; ScrollArea offset > 0. Highlight of the first button must track
        // its painted instance — not drift toward lower siblings (Disabled / "Selected variant").
        use crate::app::devtools::panel::wrap_app;
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(360.0);

        create_root(|| {
            let mk_btn = |id: &str, label: &str, color: [f32; 4]| {
                Element::new()
                    .focus_id(id)
                    .template_id(id)
                    .w(140.0)
                    .h(36.0)
                    .flex_shrink(0.0)
                    .bg(color)
                    .text(label)
            };
            let row = Element::new()
                .w("100%")
                .min_w(0.0)
                .flex_row()
                .flex_wrap_kw("wrap")
                .gap(8.0)
                .children(vec![
                    mk_btn("btn-default", "Default", [0.1, 0.1, 0.1, 1.0]),
                    mk_btn("btn-secondary", "Secondary", [0.3, 0.3, 0.3, 1.0]),
                    mk_btn("btn-outline", "Outline", [0.5, 0.5, 0.5, 1.0]),
                    mk_btn("btn-ghost", "Ghost", [0.6, 0.6, 0.6, 1.0]),
                    mk_btn("btn-destructive", "Destructive", [0.8, 0.2, 0.2, 1.0]),
                    mk_btn("btn-link", "Link", [0.2, 0.4, 0.8, 1.0]),
                    mk_btn("btn-disabled", "Disabled", [0.7, 0.7, 0.7, 1.0]),
                ]);
            let label = Element::new()
                .w("100%")
                .h(20.0)
                .flex_shrink(0.0)
                .text("Selected variant: default");
            let spacer = Element::new().w("100%").h(400.0).flex_shrink(0.0);
            let col = Element::new()
                .flex_col()
                .w("100%")
                .flex_shrink(0.0)
                .gap(8.0)
                .p(16.0)
                .children(vec![row, label, spacer]);

            // Real ScrollArea (signal-backed offset) — gallery uses this, not a bare overflow node.
            let scroll: Element = crate::components::scroll_area("gallery-narrow")
                .w("100%")
                .h("100%")
                .overflow_x("auto")
                .overflow_y("auto")
                .child(col)
                .into();

            let app = Element::new().w("100%").h("100%").min_w(0.0).child(scroll);
            let mut root = wrap_app(app);
            let _ = crate::app::devtools::panel::sync_overlay(&mut root);

            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);
            let _ = layout.build_and_layout(&mut root, 520.0, 420.0, [-1.0, -1.0]);

            // Drive scroll like the runtime prepass (signal → element).
            let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key("gallery-narrow");
            crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).set(120.0);
            fn apply_sy(el: &mut Element) {
                if el.focus_id.as_deref() == Some("scroll:gallery-narrow") {
                    el.scroll_offset[1] = 120.0;
                }
                for c in &mut el.children {
                    apply_sy(c);
                }
            }
            apply_sy(&mut root);

            // Stale snapshot identity (remounted auto btn-id) + size of a button.
            DevToolsState::set_hover_highlight(
                vec![0, 0, 0, 0, 0, 0, 0],
                "btn-default".to_string(),
                Vec::new(),
                (140.0, 36.0),
            );

            let result =
                layout.build_and_layout_fast(&mut root, 520.0, 420.0, [-1.0, -1.0], true, false);

            let btn =
                crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
                    .expect("btn-default");
            let btn_path = path_of_element(&root, btn).expect("path of btn-default");
            let paint = result.instances.iter().find(|i| {
                (i.rect_size[0] - btn.layout_bounds[2]).abs() < 2.0
                    && (i.rect_size[1] - btn.layout_bounds[3]).abs() < 2.0
                    && (i.color[0] - 0.1).abs() < 0.05
            });
            let paint_y = paint.map(|i| i.rect_pos[1]);
            let paint_x = paint.map(|i| i.rect_pos[0]);

            let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
            let border = &slot.children[2];
            let got_y = border.layout_bounds[1];
            let got_x = border.layout_bounds[0];

            let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &btn_path);
            let expect_y = btn.layout_bounds[1] - acc[1];
            let expect_x = btn.layout_bounds[0] - acc[0];

            assert!(
                (got_y - expect_y).abs() < 2.0,
                "narrow+wrap+scroll: highlight y={got_y} expect abs-scroll={expect_y} abs={} acc={:?} paint_y={paint_y:?} path={btn_path:?}",
                btn.layout_bounds[1],
                acc,
            );
            if let Some(py) = paint_y {
                assert!(
                    (got_y - py).abs() < 2.0,
                    "highlight y={got_y} must match first-button paint y={py} (got_x={got_x}, paint_x={paint_x:?})"
                );
            }
            assert!(
                (got_x - expect_x).abs() < 2.0,
                "highlight x={got_x} expect {expect_x}"
            );
        });

        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn clip_cull_after_narrow_wrap_refreshes_descendant_bounds_for_highlight() {
        // User report: after content squeeze, highlight mixed new wrap layout with old abs.
        // Clip-cull used to leave nested buttons at pre-wrap layout_bounds; a later
        // scroll_only band-skip then stamped paint_visual = old_abs − new_scroll.
        use crate::app::devtools::panel::wrap_app;
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(360.0);

        create_root(|| {
            let mk_btn = |id: &str, color: [f32; 4]| {
                Element::new()
                    .focus_id(id)
                    .template_id(id)
                    .w(140.0)
                    .h(36.0)
                    .flex_shrink(0.0)
                    .bg(color)
            };
            let row = Element::new()
                .w("100%")
                .min_w(0.0)
                .flex_row()
                .flex_wrap_kw("wrap")
                .gap(8.0)
                .children(vec![
                    mk_btn("btn-default", [0.1, 0.1, 0.1, 1.0]),
                    mk_btn("btn-second", [0.3, 0.3, 0.3, 1.0]),
                ]);
            // Push the wrap row far below the short viewport so full collect clip-culls it.
            let spacer = Element::new().w("100%").h(500.0).flex_shrink(0.0);
            let col = Element::new()
                .flex_col()
                .w("100%")
                .flex_shrink(0.0)
                .gap(8.0)
                .p(8.0)
                .children(vec![spacer, row]);

            let scroll: Element = crate::components::scroll_area("gallery-cull")
                .w("100%")
                .h("100%")
                .overflow_x("auto")
                .overflow_y("auto")
                .child(col)
                .into();

            let app = Element::new().w("100%").h("100%").min_w(0.0).child(scroll);
            let mut root = wrap_app(app);
            let _ = crate::app::devtools::panel::sync_overlay(&mut root);

            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);

            let wide_second =
                crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-second")
                    .expect("btn-second wide");
            let wide_x = wide_second.layout_bounds[0];
            assert!(
                wide_x > 100.0,
                "precondition: wide layout places second button to the right (x={wide_x})"
            );

            // Squeeze + short host: row wraps AND sits below the clip → clip-cull path.
            let _ = layout.build_and_layout(&mut root, 520.0, 200.0, [-1.0, -1.0]);

            let second =
                crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-second")
                    .expect("btn-second narrow");
            let default =
                crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
                    .expect("btn-default narrow");
            assert!(
                (second.layout_bounds[0] - default.layout_bounds[0]).abs() < 4.0,
                "clip-cull after wrap must refresh descendant abs from Taffy (stacked), \
                 not keep wide-row x (default={:?} second={:?} wide_x={wide_x})",
                default.layout_bounds,
                second.layout_bounds,
            );
            assert!(
                second.layout_bounds[1] > default.layout_bounds[1] + 20.0,
                "wrapped second button must sit below default after cull sync"
            );

            // scroll_only band-skip while deep scrolled: highlight must track abs−scroll.
            let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key("gallery-cull");
            crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).set(480.0);
            fn apply_sy(el: &mut Element) {
                if el.focus_id.as_deref() == Some("scroll:gallery-cull") {
                    el.scroll_offset[1] = 480.0;
                }
                for c in &mut el.children {
                    apply_sy(c);
                }
            }
            apply_sy(&mut root);

            DevToolsState::set_hover_highlight(
                vec![0, 0, 0, 0, 0, 0, 0],
                "btn-default".to_string(),
                Vec::new(),
                (140.0, 36.0),
            );

            let _ =
                layout.build_and_layout_fast(&mut root, 520.0, 200.0, [-1.0, -1.0], true, false);

            let btn =
                crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
                    .expect("btn-default");
            let btn_path = path_of_element(&root, btn).expect("path");
            let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &btn_path);
            let expect_y = btn.layout_bounds[1] - acc[1];
            let expect_x = btn.layout_bounds[0] - acc[0];

            let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
            let border = &slot.children[2];
            assert!(
                (border.layout_bounds[1] - expect_y).abs() < 2.0,
                "highlight y={} expect abs-scroll {} (abs={} acc={:?})",
                border.layout_bounds[1],
                expect_y,
                btn.layout_bounds[1],
                acc,
            );
            assert!(
                (border.layout_bounds[0] - expect_x).abs() < 2.0,
                "highlight x={} expect {}",
                border.layout_bounds[0],
                expect_x,
            );
        });

        DevToolsState::clear_hover_highlight();
    }

    #[test]
    fn highlight_collect_matches_target_instance_under_docked_scroll() {
        use crate::app::devtools::panel::wrap_app;
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);

        create_root(|| {
            // wrap_app: host[app, highlight] + panel. App is a tall scrolled column.
            let target = Element::new()
                .w(120.0)
                .h(40.0)
                .bg([0.2, 0.4, 0.8, 1.0])
                .template_id("target-btn")
                .focus_id("target-btn");
            // Leave room above so scrolling moves the button.
            let spacer = Element::new().w(120.0).h(300.0).flex_shrink(0.0);
            let col = Element::new()
                .flex_col()
                .w(200.0)
                .flex_shrink(0.0)
                .children(vec![spacer, target]);

            let mut viewport = Element::new()
                .w("100%")
                .h("100%")
                .overflow_y("scroll")
                .focus_id("scroll:gallery-main")
                .child(col);
            viewport.scroll_offset = [0.0, 180.0];

            let app = Element::new().w("100%").h("100%").child(viewport);
            let mut root = wrap_app(app);

            // Path: host(0) → app(0) → viewport(0) → col(0) → target(1)
            let path = vec![0, 0, 0, 0, 1];
            DevToolsState::set_hover_highlight_path(path.clone());

            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 1000.0, 600.0, [-1.0, -1.0]);

            let target_el = find_element_by_path(&root, &path).expect("target");
            let target_bounds = target_el.layout_bounds;
            let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &path);
            let expect_y = target_bounds[1] - acc[1];

            // Highlight content layer should have been stamped during collect.
            let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
            let content = &slot.children[4];
            let stamped = stamped_highlight_abs_rect(content);
            // Cache is cleared after collect — read layout_bounds stamped onto the layer instead.
            let got_y = content.layout_bounds[1];
            let got_h = content.layout_bounds[3];

            // Also find a painted instance near the target color / size.
            let target_inst = result.instances.iter().find(|i| {
                (i.rect_size[0] - 120.0).abs() < 1.0
                    && (i.rect_size[1] - 40.0).abs() < 1.0
                    && i.color[2] > 0.5
            });
            let paint_y = target_inst.map(|i| i.rect_pos[1]);

            assert!(
                (got_y - expect_y).abs() < 1.5,
                "highlight y={got_y} expect visual y={expect_y} (abs={}, acc={:?}, stamped={:?}, paint_y={paint_y:?})",
                target_bounds[1],
                acc,
                stamped,
            );
            if let Some(py) = paint_y {
                assert!(
                    (got_y - py).abs() < 1.5,
                    "highlight y={got_y} must match target paint y={py} (h={got_h})"
                );
            }
        });

        DevToolsState::clear_hover_highlight();
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
