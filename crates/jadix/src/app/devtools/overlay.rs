//! Absolute-position highlight boxes + Chrome-style box-model diagram.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::controls::label;
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::r#box;
use crate::style::{parse_color, parse_text_color, LineStyle};

use super::snapshot::{
    border_box_size_matches, box_model_snapshot, find_element_by_identity_near_size,
    find_element_by_path, find_element_by_size_near, find_element_by_template_mut,
    is_inspector_chrome, BoxModelSnapshot,
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
    if crate::app::window_scope::active() != crate::app::window_scope::WindowScope::Primary {
        return;
    }
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
    // Keep abs layout in sync so size-hint matching sees the same border box.
    el.layout_bounds = [abs[0], abs[1], w, h];
    let visual = crate::core::tree::visual_paint_aabb(el, scroll_acc);
    note_element_paint_visual(el, visual);
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
    let scale_x = border[2] / el.layout_bounds[2].max(0.001);
    let scale_y = border[3] / el.layout_bounds[3].max(0.001);
    for value in [
        &mut bm.margin_top,
        &mut bm.margin_bottom,
        &mut bm.border_top,
        &mut bm.border_bottom,
        &mut bm.padding_top,
        &mut bm.padding_bottom,
        &mut bm.position_top,
        &mut bm.position_bottom,
    ] {
        *value *= scale_y;
    }
    for value in [
        &mut bm.margin_left,
        &mut bm.margin_right,
        &mut bm.border_left,
        &mut bm.border_right,
        &mut bm.padding_left,
        &mut bm.padding_right,
        &mut bm.position_left,
        &mut bm.position_right,
    ] {
        *value *= scale_x;
    }
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
        let scaled = (0..=live_path.len()).any(|depth| {
            find_element_by_path(tree_root, &live_path[..depth]).is_some_and(|ancestor| {
                (ancestor.style.scale[0] - 1.0).abs() > 0.01
                    || (ancestor.style.scale[1] - 1.0).abs() > 0.01
            })
        });
        let paint_layout_mismatch = !scaled
            && ((from_paint.x - from_scroll.x).abs() > 2.0
                || (from_paint.y - from_scroll.y).abs() > 2.0
                || (from_paint.width - from_scroll.width).abs() > 2.0
                || (from_paint.height - from_scroll.height).abs() > 2.0);
        if !scaled && (paint_forgot_scroll || paint_forgot_scroll_x || paint_layout_mismatch) {
            return Some(from_scroll);
        }
        return Some(from_paint);
    }
    let scaled = el
        .paint_ancestor_scale
        .iter()
        .any(|scale| (*scale - 1.0).abs() > 0.01)
        || el
            .style
            .scale
            .iter()
            .any(|scale| (*scale - 1.0).abs() > 0.01);
    if scaled {
        Some(box_model_at_visual_border(
            el,
            crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc),
        ))
    } else {
        Some(from_scroll)
    }
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
        .z_index(i32::MAX)
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
    let clip = highlight_clip(root);
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
            apply_layers(slot, &bm, origin, DevToolsState::highlight_layer(), clip)
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
    let clip = highlight_clip(tree_root);
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
            apply_layers(slot, &bm, origin, DevToolsState::highlight_layer(), clip)
        }
        _ => clear_rects(slot),
    }
}

fn highlight_clip(root: &Element) -> Option<[f32; 4]> {
    let host = super::snapshot::find_element_by_template(root, "__jadix_app_host")?;
    let app = host
        .children
        .first()
        .filter(|app| app.attrs.contains_key("data-jadix-device-preview"))?;
    let bounds = crate::core::tree::visual_paint_aabb(app, app.paint_scroll_acc);
    (bounds[2] > 0.0 && bounds[3] > 0.0).then_some(bounds)
}

fn clear_rects(slot: &mut Element) -> bool {
    HIGHLIGHT_COLLECT_CACHE.with(|c| c.borrow_mut().clear());
    let mut changed = false;
    for child in &mut slot.children {
        changed |= stamp_rect(child, 0.0, 0.0, 0.0, 0.0, [0.0, 0.0], [0.0, 0.0, 0.0, 0.0]);
        changed |= child.style.border_width > 0.0;
        child.style.border_width = 0.0;
        child.style.border_color = [0.0; 4];
        child.attrs.remove("data-jadix-devtools-hl-stamped");
        child.attrs.remove("data-jadix-devtools-hl-style");
    }
    changed
}

fn apply_layers(
    slot: &mut Element,
    bm: &BoxModelSnapshot,
    origin: [f32; 2],
    layer: HighlightLayer,
    clip: Option<[f32; 4]>,
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
        let rect = if let Some(clip) = clip {
            crate::app::damage::intersect_rects(rect, clip).unwrap_or([0.0; 4])
        } else {
            rect
        };
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
        || (el.layout_bounds[3] - h).abs() > 0.5
        || el.style.bg_color != fill;
    let previous = el.layout_bounds;
    let was_visible = el.style.bg_color[3] > 0.0 || el.style.border_width > 0.0;
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
    el.set_attr("jadix_devtools_composite", "1");
    if changed {
        if was_visible {
            crate::app::damage::push_frame_region_damage(previous);
        }
        if fill[3] > 0.0 {
            crate::app::damage::push_frame_region_damage(el.layout_bounds);
        }
    }
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

/// Scroll id for the Elements-tree card (`inspector_card("Element tree")`).
pub(crate) const ELEMENTS_TREE_SCROLL_ID: &str = "devtools-card-element-tree";

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

pub(super) fn tree_row_color(selected: bool) -> [f32; 4] {
    let background = parse_color("background");
    let accent = parse_color("#2563eb");
    let alpha = if selected { 0.18 } else { 0.07 };
    [
        background[0] * (1.0 - alpha) + accent[0] * alpha,
        background[1] * (1.0 - alpha) + accent[1] * alpha,
        background[2] * (1.0 - alpha) + accent[2] * alpha,
        1.0,
    ]
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
                DevToolsState::set_highlight_hover_layer(None);
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

fn sync_ring_fill(element: &mut Element, color: [f32; 4], layer: HighlightLayer) -> LayoutStatus {
    let fill = ring_fill(color, DevToolsState::highlight_layer(), layer);
    if element.style.bg_color == fill {
        LayoutStatus::Clean
    } else {
        element.style.bg_color = fill;
        element.set_attr("jadix_devtools_composite", "1");
        LayoutStatus::StyleChanged
    }
}

fn edge_label(id: &str, text: String) -> Element {
    label(id)
        .text(text)
        .text_size("0.625rem")
        .text_color(parse_text_color("foreground"))
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
        .reactive_sync(|element, _| sync_ring_fill(element, CONTENT_C, HighlightLayer::Content))
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
        .text_color(parse_text_color("foreground"))
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
            ring_fill_strip(caption, "t", fill, "t", sides[0], pad)
                .reactive_sync(move |element, _| sync_ring_fill(element, color, layer)),
            ring_fill_strip(caption, "b", fill, "b", sides[2], pad)
                .reactive_sync(move |element, _| sync_ring_fill(element, color, layer)),
            ring_fill_strip(caption, "l", fill, "l", sides[3], pad)
                .reactive_sync(move |element, _| sync_ring_fill(element, color, layer)),
            ring_fill_strip(caption, "r", fill, "r", sides[1], pad)
                .reactive_sync(move |element, _| sync_ring_fill(element, color, layer)),
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
            DevToolsState::set_highlight_hover_layer(None);
            DevToolsState::set_highlight_layer(layer);
        })
        .on_hover(move |on| {
            if on {
                let path = DevToolsState::selected_path();
                if !DevToolsState::selected_path_key().is_empty() {
                    DevToolsState::set_hover_highlight_path(path);
                }
                DevToolsState::set_highlight_hover_layer(Some(layer));
            } else {
                DevToolsState::set_highlight_hover_layer(None);
                let path = DevToolsState::selected_path();
                DevToolsState::clear_hover_highlight_if_path(&path);
            }
        })
        .into()
}

#[cfg(test)]
#[path = "tests/overlay.rs"]
mod tests;
