//! Taffy 布局、文本测量与将树展开GPU 实例 / 文本排版信息
//! 命名说明：本模块**不是** [`crate::renderer`]（wgpu/glyphon），只做 **布局与遍*
//! 因此使用 `LayoutTree` / `layout_tree`，避免与「渲染器」混淆
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Instant;

use taffy::prelude::*;

use crate::components::scrollbar::{
    scrollbar_min_thumb_along_track, scrollbar_visibility_signal_key, SCROLLBAR_THUMB_FOCUS_PREFIX,
    SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
    scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
    scroll_scrollbar_x_side_signal_key, scroll_scrollbar_y_side_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::Element;
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::TextStyle;

/// 将带 `rui_portal=1` 的子树从原父节点摘下并挂?`root` 末尾（绘制顺序靠后，便于浮层）?/// 返回本帧提升的子树数；非 0 时应做完?Taffy 重建（见 `AppState::render`）.
pub fn hoist_portal_subtrees(root: &mut Element) -> usize {
    let mut lifted = Vec::new();
    hoist_portal_rec(root, &mut lifted);
    let n = lifted.len();
    root.children.extend(lifted);
    n
}

fn hoist_portal_rec(node: &mut Element, lifted: &mut Vec<Element>) {
    let mut i = 0;
    while i < node.children.len() {
        hoist_portal_rec(&mut node.children[i], lifted);
        if node.children[i].attrs.get("rui_portal").map(|s| s.as_str()) == Some("1") {
            let mut n = node.children.remove(i);
            n.attrs.remove("rui_portal");
            lifted.push(n);
        } else {
            i += 1;
        }
    }
}

fn resolve_corner_radii(radii: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    let cap = (w.min(h) * 0.5).max(0.0);
    let mut out = radii;
    for r in &mut out {
        if *r < 0.0 {
            *r = cap;
        } else {
            *r = (*r).clamp(0.0, cap);
        }
    }
    out
}

/// Minimum cross-axis size for an `auto` scrollbar at spring?; must match `app::state`.
const SCROLLBAR_HIT_SLAB_CROSS: f32 = 8.0;
/// Default track cross size when `max_w`/`max_h` is missing; matches `ScrollbarBuilder::thickness`.
const SCROLLBAR_DEFAULT_CROSS_PX: f32 = 10.0;

/// Cascades inheritable text style fields from parent to descendants.
/// Child explicit declarations (tracked in `attrs`) always win over inherited values.
fn cascade_inheritable_text_style(node: &mut Element, parent_style: Option<TextStyle>) {
    if let Some(parent) = parent_style {
        if !node.attrs.contains_key("text_align") {
            node.text_style.align = parent.align;
        }
        if !node.attrs.contains_key("text_color") {
            node.text_style.color = parent.color;
        }
        if !node.attrs.contains_key("font_family") {
            node.text_style.font_family = parent.font_family.clone();
        }
        if !node.attrs.contains_key("font_weight") {
            node.text_style.weight = parent.weight;
        }
        if !node.attrs.contains_key("text_size") {
            node.text_style.size = parent.size;
        }
        if !node.attrs.contains_key("line_height") && !node.attrs.contains_key("leading") {
            node.text_style.line_height_px = parent.line_height_px;
            node.text_style.line_height_scale = parent.line_height_scale;
        }
        if !node.attrs.contains_key("text_decoration_line") {
            node.text_style.underline = parent.underline;
            node.text_style.overline = parent.overline;
            node.text_style.line_through = parent.line_through;
        }
        if !node.attrs.contains_key("text_decoration_style") {
            node.text_style.decoration_style = parent.decoration_style;
        }
        if !node.attrs.contains_key("text_decoration_color") {
            node.text_style.decoration_color = parent.decoration_color;
        }
        if !node.attrs.contains_key("text_decoration_thickness") {
            node.text_style.decoration_thickness = parent.decoration_thickness;
        }
        if !node.attrs.contains_key("text_underline_offset") {
            node.text_style.underline_offset = parent.underline_offset;
        }
    }

    let next_style = Some(node.text_style.clone());
    for child in &mut node.children {
        cascade_inheritable_text_style(child, next_style.clone());
    }
}

/// One `peek()` batch per scroll-area id per collect pass; shared by x/y scrollbar tracks on that area.
struct ScrollAreaSignalsCached {
    vw: f32,
    vh: f32,
    cw: f32,
    ch: f32,
    sx: f32,
    sy: f32,
    overflow_x: String,
    overflow_y: String,
    scrollbar_vis_x: String,
    scrollbar_vis_y: String,
    inset_x: String,
    inset_y: String,
    y_side: String,
    x_side: String,
}

fn scroll_area_signals_cached<'a>(
    rest: &str,
    cache: &'a mut HashMap<String, ScrollAreaSignalsCached>,
) -> &'a ScrollAreaSignalsCached {
    match cache.entry(rest.to_string()) {
        Entry::Occupied(o) => o.into_mut(),
        Entry::Vacant(v) => {
            // Raw metrics only: half-pixel snapping vw/vh/cw/ch independently can erase a tiny
            // (ch - vh), making max_scroll 0 and snapping the thumb to the track start after idle.
            // Post-render already gates signal writes with a 0.1px threshold (window_events).
            let vw = use_signal(&scroll_viewport_w_signal_key(rest), || 0.0_f32).peek();
            let vh = use_signal(&scroll_viewport_h_signal_key(rest), || 0.0_f32).peek();
            let cw = use_signal(&scroll_content_w_signal_key(rest), || 0.0_f32).peek();
            let ch = use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).peek();
            let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
            let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
            let overflow_x =
                use_signal(&scroll_overflow_x_signal_key(rest), || "auto".to_string()).peek();
            let overflow_y =
                use_signal(&scroll_overflow_y_signal_key(rest), || "auto".to_string()).peek();
            let scrollbar_vis_x = use_signal(&scrollbar_visibility_signal_key(rest, "x"), || {
                "auto".to_string()
            })
            .peek();
            let scrollbar_vis_y = use_signal(&scrollbar_visibility_signal_key(rest, "y"), || {
                "auto".to_string()
            })
            .peek();
            let inset_x = use_signal(&scroll_scrollbar_inset_x_signal_key(rest), || {
                "inside".to_string()
            })
            .peek();
            let inset_y = use_signal(&scroll_scrollbar_inset_y_signal_key(rest), || {
                "inside".to_string()
            })
            .peek();
            let y_side = use_signal(&scroll_scrollbar_y_side_signal_key(rest), || {
                "right".to_string()
            })
            .peek();
            let x_side = use_signal(&scroll_scrollbar_x_side_signal_key(rest), || {
                "bottom".to_string()
            })
            .peek();
            v.insert(ScrollAreaSignalsCached {
                vw,
                vh,
                cw,
                ch,
                sx,
                sy,
                overflow_x,
                overflow_y,
                scrollbar_vis_x,
                scrollbar_vis_y,
                inset_x,
                inset_y,
                y_side,
                x_side,
            })
        }
    }
}

/// 一次布局+收集?CPU 细分（毫秒）；见 `AppState` debug `render profile`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutProfileMs {
    /// `build_and_layout`：建/?Taffy 子树（含 cascade、clear）；`fast` 路径?0
    pub rebuild_ms: f32,
    /// `build_and_layout_fast`：根尺寸、cascade、`update_taffy_node_style(root)`；整树重建路径为 0
    pub setup_ms: f32,
    /// `compute_layout_with_measure`；`scroll_only` 跳过则为 0
    pub taffy_compute_ms: f32,
    /// `collect_instances_mut`（整树展开实例?`TextLayoutInfo`）
    pub collect_ms: f32,
}

/// 一次布局+收集的产出.
pub struct RenderResult {
    pub instances: Vec<InstanceInput>,
    pub textured_draws: Vec<TexturedDraw>,
    pub text_nodes: Vec<TextLayoutInfo>,
    pub scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>, // (rest, bounds, max_scroll, content_size)
    pub hover_cursor: Option<crate::core::tree::CursorKind>,
    pub prof: LayoutProfileMs,
}

/// 单段文本在布局后的几何与样式快照（?glyphon 使用）.
#[derive(Clone)]
pub struct TextLayoutInfo {
    pub text: String,
    /// Matches [`crate::core::tree::Element::text_shape_suffix`]: shapes with text, not for caret.
    pub shape_suffix: Option<String>,
    /// Rich-text spans; when set, prepare uses `set_rich_text`.
    pub spans: Option<Vec<crate::style::TextSpan>>,
    /// From [`crate::core::tree::Element::template_id`] for `RUI_TEXT_DEBUG` filters.
    pub template_id: Option<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub clip_bounds: Option<[f32; 4]>,
    pub style: TextStyle,
}

/// 驱动 Taffy 并对 `Element` 树做 layout 与实例收集.
pub struct LayoutTree {
    pub taffy: TaffyTree<()>,
    pub(crate) root_node: Option<NodeId>,
    text_measurements: HashMap<taffy::NodeId, TextMeasureMeta>,
    /// Per-compute_layout text leaf measure memo (cleared each compute).
    measure_layout_cache: RefCell<HashMap<TextMeasureCacheKey, taffy::geometry::Size<f32>>>,
    /// Track nodes visited in the current build pass for pruning.
    visited_nodes: std::collections::HashSet<NodeId>,
    /// Track all nodes currently in the taffy tree.
    total_nodes: std::collections::HashSet<NodeId>,
}

/// Text leaf measure meta (line-height aligned with TextStyle).
struct TextMeasureMeta {
    size: f32,
    text: String,
    shape_suffix: Option<String>,
    is_icon: bool,
    line_height_px: Option<f32>,
    line_height_scale: Option<f32>,
    weight: u16,
    letter_spacing: f32,
}

/// 单次 `compute_layout` 内对 `measured_text_leaf_size` ?memo（Taffy 会对同一叶多次探测）.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct TextMeasureCacheKey {
    node: NodeId,
    kw: u32,
    kh: u32,
    aw: u64,
    ah: u64,
}

impl TextMeasureCacheKey {
    fn new(
        node: NodeId,
        kd: taffy::geometry::Size<Option<f32>>,
        av: taffy::geometry::Size<taffy::style::AvailableSpace>,
    ) -> Self {
        Self {
            node,
            kw: kd.width.map(|w| w.to_bits()).unwrap_or(u32::MAX),
            kh: kd.height.map(|h| h.to_bits()).unwrap_or(u32::MAX),
            aw: pack_avail_space(av.width),
            ah: pack_avail_space(av.height),
        }
    }
}

#[inline]
fn pack_avail_space(d: taffy::style::AvailableSpace) -> u64 {
    match d {
        taffy::style::AvailableSpace::Definite(v) => {
            // ?2 bit ?tag，低 62 bit ?f32::to_bits（布局里极少用到极大指数）
            (3u64 << 62) | ((v.to_bits() as u64) & ((1u64 << 62) - 1))
        }
        taffy::style::AvailableSpace::MinContent => 1u64 << 62,
        taffy::style::AvailableSpace::MaxContent => 2u64 << 62,
    }
}

#[inline]
fn measure_leaf_cached(
    cache: &RefCell<HashMap<TextMeasureCacheKey, taffy::geometry::Size<f32>>>,
    tm: &HashMap<NodeId, TextMeasureMeta>,
    node_id: NodeId,
    known_dimensions: taffy::geometry::Size<Option<f32>>,
    available_space: taffy::geometry::Size<taffy::style::AvailableSpace>,
) -> taffy::geometry::Size<f32> {
    let key = TextMeasureCacheKey::new(node_id, known_dimensions, available_space);
    if let Some(v) = cache.borrow().get(&key) {
        return *v;
    }
    let out = if let Some(m) = tm.get(&node_id) {
        measured_text_leaf_size(m, known_dimensions, available_space)
    } else {
        taffy::geometry::Size::ZERO
    };
    cache.borrow_mut().insert(key, out);
    out
}

fn measured_text_leaf_size(
    m: &TextMeasureMeta,
    known_dimensions: taffy::geometry::Size<Option<f32>>,
    available_space: taffy::geometry::Size<taffy::style::AvailableSpace>,
) -> taffy::geometry::Size<f32> {
    let size = m.size;
    let is_icon = m.is_icon;

    let avail_width = match available_space.width {
        taffy::style::AvailableSpace::Definite(w) => Some(w.max(1.0)),
        _ => None,
    };

    let single_line_height = if is_icon {
        size
    } else if let Some(lh) = m.line_height_px {
        lh
    } else if let Some(scale) = m.line_height_scale {
        size * scale
    } else {
        size * crate::style::tailwind_default_leading_scale(size)
    };

    if is_icon {
        let width = known_dimensions.width.unwrap_or(size);
        let height = known_dimensions.height.unwrap_or(size);
        return taffy::geometry::Size { width, height };
    }

    let blob: Cow<str> = match m.shape_suffix.as_deref() {
        Some(suf) if !suf.is_empty() => Cow::Owned(format!("{}{}", m.text, suf)),
        _ => Cow::Borrowed(m.text.as_str()),
    };

    let (width, height) = crate::text::measure_leaf(
        blob.as_ref(),
        size,
        m.weight,
        single_line_height,
        known_dimensions.width,
        avail_width,
        m.letter_spacing,
    );
    let height = match known_dimensions.height {
        Some(h) => height.max(h),
        None => height,
    };
    taffy::geometry::Size { width, height }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            root_node: None,
            text_measurements: HashMap::new(),
            measure_layout_cache: RefCell::new(HashMap::new()),
            visited_nodes: std::collections::HashSet::new(),
            total_nodes: std::collections::HashSet::new(),
        }
    }

    pub fn update_taffy_node_style(&mut self, element: &Element) {
        if let Some(node_id) = element.layout_node {
            let style = element.style.to_taffy_style();
            self.taffy.set_style(node_id, style).unwrap();
        }
    }

    pub fn build_and_layout(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
    ) -> RenderResult {
        let t_rebuild_begin = Instant::now();
        root.style.size.width = Dimension::Length(width);
        root.style.size.height = Dimension::Length(height);
        cascade_inheritable_text_style(root, None);

        self.text_measurements.clear();
        self.visited_nodes.clear();
        // DO NOT clear taffy anymore; we rely on reconciliation and selective removal.
        let root_node = self.build_taffy_node_mut(root);
        self.root_node = Some(root_node);

        // Prune nodes that were not visited in this pass.
        let to_remove: Vec<NodeId> = self
            .total_nodes
            .iter()
            .filter(|id| !self.visited_nodes.contains(id))
            .cloned()
            .collect();
        for node_id in to_remove {
            let _ = self.taffy.remove(node_id);
            self.total_nodes.remove(&node_id);
            self.text_measurements.remove(&node_id);
        }
        let rebuild_ms = t_rebuild_begin.elapsed().as_secs_f32() * 1000.0;

        let available_space = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };

        let t_compute_begin = Instant::now();
        self.measure_layout_cache.borrow_mut().clear();
        let tm = &self.text_measurements;
        let mcache = &self.measure_layout_cache;
        self.taffy
            .compute_layout_with_measure(
                root_node,
                available_space,
                |known_dimensions, available_space, node_id, _context, _style| {
                    measure_leaf_cached(mcache, tm, node_id, known_dimensions, available_space)
                },
            )
            .unwrap();
        let taffy_compute_ms = t_compute_begin.elapsed().as_secs_f32() * 1000.0;

        let mut instances = Vec::new();
        let mut textured_draws = Vec::new();
        let mut text_nodes = Vec::new();
        let mut scroll_metrics = Vec::new();
        let mut hover_cursor = None;

        let t_collect_begin = Instant::now();
        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            1.0,
            &mut instances,
            &mut textured_draws,
            &mut text_nodes,
            &mut scroll_metrics,
            &mut hover_cursor,
            false,
            false,
            None,
            &mut None,
        );
        let collect_ms = t_collect_begin.elapsed().as_secs_f32() * 1000.0;

        RenderResult {
            instances,
            textured_draws,
            text_nodes,
            scroll_metrics,
            hover_cursor,
            prof: LayoutProfileMs {
                rebuild_ms,
                setup_ms: 0.0,
                taffy_compute_ms,
                collect_ms,
            },
        }
    }

    /// When `scroll_only` is true, skip [`TaffyTree::compute_layout`]: `Element.scroll_offset` does
    /// not participate in flex/grid sizing (only affects collect-time drawing). Use after
    /// `update_scrollbar_elements` synced only `scroll:` offsets and no scrollbar styles changed
    /// ?avoids O(nested) full layout each frame when many `ScrollArea`s are nested.
    ///
    /// When `scrollbar_overlay_collect` is true, track/thumb draw rects use live scroll signals
    /// (same contract as a `ViewportsOnly` prepass without Full). Use whenever that prepass did not
    /// push geometry into Taffy, even if chrome sync forces `!scroll_only` layout for other nodes.
    pub fn build_and_layout_fast(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        scroll_only: bool,
        scrollbar_overlay_collect: bool,
    ) -> RenderResult {
        let root_node = self
            .root_node
            .expect("fast path requires existing root_node");

        // Must match `build_and_layout`: the root's Element + Taffy style must follow the current
        // viewport. Otherwise `compute_layout` still sees stale definite sizes from the last full
        // build (e.g. after maximize / restore while status is `Clean`) and the UI stays a small
        // island in the corner while the swapchain is full-window.
        let t_setup_begin = Instant::now();
        root.style.size.width = Dimension::Length(width);
        root.style.size.height = Dimension::Length(height);
        cascade_inheritable_text_style(root, None);
        self.update_taffy_node_style(root);
        let setup_ms = t_setup_begin.elapsed().as_secs_f32() * 1000.0;

        let taffy_compute_ms = if !scroll_only {
            let available_space = Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            };

            let t_compute_begin = Instant::now();
            self.measure_layout_cache.borrow_mut().clear();
            let tm = &self.text_measurements;
            let mcache = &self.measure_layout_cache;
            // Note: We assume the caller has updated any necessary styles (like scrollbar thumbs)
            // using update_taffy_node_style before calling this.
            self.taffy
                .compute_layout_with_measure(
                    root_node,
                    available_space,
                    |known_dimensions, available_space, node_id, _context, _style| {
                        measure_leaf_cached(mcache, tm, node_id, known_dimensions, available_space)
                    },
                )
                .unwrap();
            t_compute_begin.elapsed().as_secs_f32() * 1000.0
        } else {
            0.0
        };

        let mut instances = Vec::new();
        let mut textured_draws = Vec::new();
        let mut text_nodes = Vec::new();
        let mut scroll_metrics = Vec::new();
        let mut hover_cursor = None;

        let mut scroll_signal_cache: HashMap<String, ScrollAreaSignalsCached> = HashMap::new();
        let mut scroll_cache_slot: Option<&mut HashMap<String, ScrollAreaSignalsCached>> =
            if scroll_only || scrollbar_overlay_collect {
                Some(&mut scroll_signal_cache)
            } else {
                None
            };

        let t_collect_begin = Instant::now();
        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            1.0,
            &mut instances,
            &mut textured_draws,
            &mut text_nodes,
            &mut scroll_metrics,
            &mut hover_cursor,
            scroll_only,
            scrollbar_overlay_collect,
            None,
            &mut scroll_cache_slot,
        );
        let collect_ms = t_collect_begin.elapsed().as_secs_f32() * 1000.0;

        RenderResult {
            instances,
            textured_draws,
            text_nodes,
            scroll_metrics,
            hover_cursor,
            prof: LayoutProfileMs {
                rebuild_ms: 0.0,
                setup_ms,
                taffy_compute_ms,
                collect_ms,
            },
        }
    }

    fn build_taffy_node_mut(&mut self, element: &mut Element) -> NodeId {
        let mut child_nodes = Vec::new();
        for child in &mut element.children {
            child_nodes.push(self.build_taffy_node_mut(child));
        }

        let style = element.style.to_taffy_style();
        let is_text_leaf = element.text.is_some() && child_nodes.is_empty();

        let node_id = if let Some(existing_id) = element.layout_node {
            if self.total_nodes.contains(&existing_id) {
                // Reuse existing node and update its children/style.
                self.taffy.set_style(existing_id, style).unwrap();
                self.taffy.set_children(existing_id, &child_nodes).unwrap();
                existing_id
            } else {
                // Node was somehow lost from taffy (should not happen if we don't clear), create new.
                if is_text_leaf {
                    let id = self.taffy.new_leaf_with_context(style, ()).unwrap();
                    self.total_nodes.insert(id);
                    id
                } else {
                    let id = self.taffy.new_with_children(style, &child_nodes).unwrap();
                    self.total_nodes.insert(id);
                    id
                }
            }
        } else {
            // Brand new element
            if is_text_leaf {
                let id = self.taffy.new_leaf_with_context(style, ()).unwrap();
                self.total_nodes.insert(id);
                id
            } else {
                let id = self.taffy.new_with_children(style, &child_nodes).unwrap();
                self.total_nodes.insert(id);
                id
            }
        };

        element.layout_node = Some(node_id);
        self.visited_nodes.insert(node_id);

        if let Some(text) = &element.text {
            if child_nodes.is_empty() {
                let size = element.text_style.size;
                let text_clone = text.clone();
                let is_icon = element.text_style.font_family.as_deref() == Some("lucide");
                self.text_measurements.insert(
                    node_id,
                    TextMeasureMeta {
                        size,
                        text: text_clone,
                        shape_suffix: element.text_shape_suffix.clone(),
                        is_icon,
                        line_height_px: element.text_style.line_height_px,
                        line_height_scale: element.text_style.line_height_scale,
                        weight: element.text_style.weight,
                        letter_spacing: element.text_style.letter_spacing,
                    },
                );
            }
        }

        node_id
    }

    /// `ancestor_scroll_viewport_rest`: nearest ancestor `scroll:{id}` viewport id for this subtree.
    /// Used so nested ScrollAreas do not inflate parent content extents (matches
    /// `scroll_support::collect_descendant_extent`); fixes thumb math for middle layers in 3+ nesting.
    fn collect_instances_mut(
        &self,
        element: &mut Element,
        node: NodeId,
        abs_offset: [f32; 2],
        scroll_acc: [f32; 2],
        parent_clip: Option<[f32; 4]>,
        ancestor_scroll_viewport_rest: Option<&str>,
        mouse_pos: [f32; 2],
        ancestor_opacity: f32,
        instances: &mut Vec<InstanceInput>,
        textured_draws: &mut Vec<TexturedDraw>,
        text_nodes: &mut Vec<TextLayoutInfo>,
        scroll_metrics: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
        hover_cursor: &mut Option<crate::core::tree::CursorKind>,
        _scroll_only: bool,
        scrollbar_overlay_collect: bool,
        thumb_geometry_override: Option<[f32; 4]>,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    ) -> [f32; 2] {
        element.layout_node = Some(node);
        let layout = match self.taffy.layout(node) {
            Ok(l) => *l,
            Err(err) => {
                log::error!("taffy.layout({node:?}) failed: {err:?}");
                return [abs_offset[0], abs_offset[1]];
            }
        };
        let base_abs_x = abs_offset[0] + layout.location.x;
        let base_abs_y = abs_offset[1] + layout.location.y;

        let mut thumb_rect_for_child: Option<[f32; 4]> = None;

        let (abs_x, abs_y, lay_w, lay_h) = if let Some(r) = thumb_geometry_override {
            (r[0], r[1], r[2], r[3])
        } else if scrollbar_overlay_collect {
            if let Some(cache) = scroll_signals_cache.as_mut() {
                if let Some(fid) = element.focus_id.as_deref() {
                    if let Some(rest_axis) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
                        let (rest, axis) = rest_axis.rsplit_once(':').unwrap_or((rest_axis, "y"));
                        let sig = scroll_area_signals_cached(rest, cache);
                        let (t_rect, th_rect) = scrollbar_track_overlay_abs(
                            element,
                            base_abs_x,
                            base_abs_y,
                            layout.size.width,
                            layout.size.height,
                            axis,
                            sig,
                        );
                        thumb_rect_for_child = Some(th_rect);
                        (t_rect[0], t_rect[1], t_rect[2], t_rect[3])
                    } else {
                        (
                            base_abs_x,
                            base_abs_y,
                            layout.size.width,
                            layout.size.height,
                        )
                    }
                } else {
                    (
                        base_abs_x,
                        base_abs_y,
                        layout.size.width,
                        layout.size.height,
                    )
                }
            } else {
                (
                    base_abs_x,
                    base_abs_y,
                    layout.size.width,
                    layout.size.height,
                )
            }
        } else {
            (
                base_abs_x,
                base_abs_y,
                layout.size.width,
                layout.size.height,
            )
        };

        element.layout_bounds = [abs_x, abs_y, lay_w, lay_h];

        let opacity = (ancestor_opacity * element.style.opacity).clamp(0.0, 1.0);
        let scroll_for_render = if element.style.position_fixed {
            [0.0_f32, 0.0_f32]
        } else {
            scroll_acc
        };
        let render_x = abs_x - scroll_for_render[0] + element.style.translate[0];
        let render_y = abs_y - scroll_for_render[1] + element.style.translate[1];
        let scale = if element.style.scale.is_finite() && element.style.scale > 0.0 {
            element.style.scale
        } else {
            1.0
        };
        let (vis_w, vis_h, vis_x, vis_y) = if (scale - 1.0).abs() > f32::EPSILON {
            let sw = lay_w * scale;
            let sh = lay_h * scale;
            (
                sw,
                sh,
                render_x + (lay_w - sw) * 0.5,
                render_y + (lay_h - sh) * 0.5,
            )
        } else {
            (lay_w, lay_h, render_x, render_y)
        };
        let render_rect = [vis_x, vis_y, vis_w, vis_h];
        let rotate_deg = element.style.rotate_deg;

        // Clipping & Visibility
        let mut clip = parent_clip;
        if element.style.overflow.x != taffy::style::Overflow::Visible
            || element.style.overflow.y != taffy::style::Overflow::Visible
        {
            clip = Some(intersect_rect(clip, render_rect));
        }
        let draw_rect = intersect_rect(clip, render_rect);

        // Hit Testing (Update cursor if mouse is over visible part)
        if mouse_pos[0] >= draw_rect[0]
            && mouse_pos[0] <= draw_rect[0] + draw_rect[2]
            && mouse_pos[1] >= draw_rect[1]
            && mouse_pos[1] <= draw_rect[1] + draw_rect[3]
        {
            if let Some(c) = element.cursor {
                *hover_cursor = Some(c);
            }
        }

        // Collect main instance (solid / gradient strips + shadows)
        let border_w = if element.style.border_style == crate::style::LineStyle::None {
            0.0
        } else {
            element.style.border_width
        };
        let has_bg = element.style.bg_color[3] > 0.0 || element.style.bg_gradient.is_some();
        let has_border = border_w > 0.0;
        let has_shadow = element.style.shadow_color[3] > 0.0;
        if (has_bg || has_border || has_shadow) && draw_rect[2] > 0.0 && draw_rect[3] > 0.0 {
            let resolved_radii =
                resolve_corner_radii(element.style.radii, draw_rect[2], draw_rect[3]);
            if let Some(grad) = element.style.bg_gradient {
                const STRIPS: usize = 24;
                let angle = grad.angle_deg.to_radians();
                let dx = angle.cos();
                let dy = angle.sin();
                let horizontal = dx.abs() >= dy.abs();
                for i in 0..STRIPS {
                    let t = (i as f32 + 0.5) / STRIPS as f32;
                    let t = if horizontal {
                        if dx >= 0.0 {
                            t
                        } else {
                            1.0 - t
                        }
                    } else if dy >= 0.0 {
                        t
                    } else {
                        1.0 - t
                    };
                    let color = mul_color_alpha(lerp_color(grad.from, grad.to, t), opacity);
                    let (pos, size) = if horizontal {
                        let sw = draw_rect[2] / STRIPS as f32;
                        (
                            [draw_rect[0] + i as f32 * sw, draw_rect[1]],
                            [sw.max(0.5), draw_rect[3]],
                        )
                    } else {
                        let sh = draw_rect[3] / STRIPS as f32;
                        (
                            [draw_rect[0], draw_rect[1] + i as f32 * sh],
                            [draw_rect[2], sh.max(0.5)],
                        )
                    };
                    instances.push(InstanceInput {
                        rect_pos: pos,
                        rect_size: size,
                        color,
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        radii: resolved_radii,
                        border_width: 0.0,
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_spread: 0.0,
                        shadow_inset: 0.0,
                        rotate_deg,
                        _padding: 0.0,
                    });
                }
                if has_border || has_shadow {
                    instances.push(InstanceInput {
                        rect_pos: [draw_rect[0], draw_rect[1]],
                        rect_size: [draw_rect[2], draw_rect[3]],
                        color: [0.0, 0.0, 0.0, 0.0],
                        border_color: mul_color_alpha(element.style.border_color, opacity),
                        radii: resolved_radii,
                        border_width: border_w,
                        shadow_color: mul_color_alpha(element.style.shadow_color, opacity),
                        shadow_offset: element.style.shadow_offset,
                        shadow_blur: element.style.shadow_blur,
                        shadow_spread: element.style.shadow_spread,
                        shadow_inset: if element.style.shadow_inset { 1.0 } else { 0.0 },
                        rotate_deg,
                        _padding: 0.0,
                    });
                }
            } else {
                instances.push(InstanceInput {
                    rect_pos: [draw_rect[0], draw_rect[1]],
                    rect_size: [draw_rect[2], draw_rect[3]],
                    color: mul_color_alpha(element.style.bg_color, opacity),
                    border_color: mul_color_alpha(element.style.border_color, opacity),
                    radii: resolved_radii,
                    border_width: border_w,
                    shadow_color: mul_color_alpha(element.style.shadow_color, opacity),
                    shadow_offset: element.style.shadow_offset,
                    shadow_blur: element.style.shadow_blur,
                    shadow_spread: element.style.shadow_spread,
                    shadow_inset: if element.style.shadow_inset { 1.0 } else { 0.0 },
                    rotate_deg,
                    _padding: 0.0,
                });
            }
            for sh in &element.style.extra_shadows {
                if sh.color[3] <= 0.0 {
                    continue;
                }
                instances.push(InstanceInput {
                    rect_pos: [draw_rect[0], draw_rect[1]],
                    rect_size: [draw_rect[2], draw_rect[3]],
                    color: [0.0, 0.0, 0.0, 0.0],
                    border_color: [0.0, 0.0, 0.0, 0.0],
                    radii: resolved_radii,
                    border_width: 0.0,
                    shadow_color: mul_color_alpha(sh.color, opacity),
                    shadow_offset: sh.offset,
                    shadow_blur: sh.blur,
                    shadow_spread: sh.spread,
                    shadow_inset: if sh.inset { 1.0 } else { 0.0 },
                    rotate_deg,
                    _padding: 0.0,
                });
            }
        }

        // Collect outline as an overlay (drawn outside; does not affect layout).
        let outline_w = if element.style.outline_style == crate::style::LineStyle::None {
            0.0
        } else {
            element.style.outline_width
        };
        if outline_w > 0.0 && lay_w > 0.0 && lay_h > 0.0 {
            // Outline should not be clipped by element's own overflow; only parent clip applies.
            let outline_rect = [
                render_x - outline_w,
                render_y - outline_w,
                lay_w + outline_w * 2.0,
                lay_h + outline_w * 2.0,
            ];
            let outline_draw = intersect_rect(parent_clip, outline_rect);
            if outline_draw[2] > 0.0 && outline_draw[3] > 0.0 {
                let mut outline_radii = element.style.radii;
                for r in &mut outline_radii {
                    if *r >= 0.0 {
                        *r += outline_w;
                    }
                }
                let resolved_radii =
                    resolve_corner_radii(outline_radii, outline_draw[2], outline_draw[3]);
                instances.push(InstanceInput {
                    rect_pos: [outline_draw[0], outline_draw[1]],
                    rect_size: [outline_draw[2], outline_draw[3]],
                    color: [0.0, 0.0, 0.0, 0.0],
                    border_color: mul_color_alpha(element.style.outline_color, opacity),
                    radii: resolved_radii,
                    border_width: outline_w,
                    shadow_color: [0.0, 0.0, 0.0, 0.0],
                    shadow_offset: [0.0, 0.0],
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    shadow_inset: 0.0,
                    rotate_deg,
                    _padding: 0.0,
                });
            }
        }

        if let Some(tex_key) = element.image_texture.as_ref() {
            if !tex_key.is_empty() && draw_rect[2] > 0.0 && draw_rect[3] > 0.0 {
                textured_draws.push(TexturedDraw {
                    rect: draw_rect,
                    texture_key: tex_key.clone(),
                    opacity,
                });
            }
        }

        if let Some(text) = &element.text {
            let mut text_style = element.text_style.clone();
            if opacity < 1.0 - f32::EPSILON {
                let a = ((f32::from(text_style.color[3]) * opacity).round() as u16).min(255) as u8;
                text_style.color[3] = a;
            }
            text_nodes.push(TextLayoutInfo {
                text: text.clone(),
                shape_suffix: element.text_shape_suffix.clone(),
                spans: element.text_spans.clone(),
                template_id: element.template_id.clone(),
                x: render_x,
                y: render_y,
                width: lay_w,
                height: lay_h,
                clip_bounds: clip,
                style: text_style,
            });
        }

        // Recurse children
        let mut next_scroll_acc = scroll_acc;
        if element.style.overflow.x != taffy::style::Overflow::Visible {
            next_scroll_acc[0] += element.scroll_offset[0];
        }
        if element.style.overflow.y != taffy::style::Overflow::Visible {
            next_scroll_acc[1] += element.scroll_offset[1];
        }

        let pass_to_children = element
            .focus_id
            .as_deref()
            .and_then(|fid| fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
            .or(ancestor_scroll_viewport_rest);

        let child_nodes = match self.taffy.children(node) {
            Ok(c) => c,
            Err(err) => {
                log::error!("taffy.children({node:?}) failed: {err:?}");
                return [abs_x + lay_w, abs_y + lay_h];
            }
        };
        let any_z = element.children.iter().any(|c| c.style.z_index != 0);
        let mut max_right = abs_x + lay_w;
        let mut max_bottom = abs_y + lay_h;

        if !any_z {
            for (i, child) in element.children.iter_mut().enumerate() {
                let thumb_ov = if thumb_rect_for_child.is_some() {
                    if child
                        .focus_id
                        .as_deref()
                        .is_some_and(|f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX))
                    {
                        thumb_rect_for_child.take()
                    } else {
                        None
                    }
                } else {
                    None
                };
                let [cr, cb] = self.collect_instances_mut(
                    child,
                    child_nodes[i],
                    [abs_x, abs_y],
                    next_scroll_acc,
                    clip,
                    pass_to_children,
                    mouse_pos,
                    opacity,
                    instances,
                    textured_draws,
                    text_nodes,
                    scroll_metrics,
                    hover_cursor,
                    _scroll_only,
                    scrollbar_overlay_collect,
                    thumb_ov,
                    scroll_signals_cache,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
            }
        } else {
            let mut order: Vec<usize> = (0..element.children.len()).collect();
            order.sort_by(|&a, &b| {
                element.children[a]
                    .style
                    .z_index
                    .cmp(&element.children[b].style.z_index)
                    .then(a.cmp(&b))
            });
            for i in order {
                let thumb_ov = if thumb_rect_for_child.is_some() {
                    let c = &element.children[i];
                    if c.focus_id
                        .as_deref()
                        .is_some_and(|f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX))
                    {
                        thumb_rect_for_child.take()
                    } else {
                        None
                    }
                } else {
                    None
                };
                let [cr, cb] = self.collect_instances_mut(
                    &mut element.children[i],
                    child_nodes[i],
                    [abs_x, abs_y],
                    next_scroll_acc,
                    clip,
                    pass_to_children,
                    mouse_pos,
                    opacity,
                    instances,
                    textured_draws,
                    text_nodes,
                    scroll_metrics,
                    hover_cursor,
                    _scroll_only,
                    scrollbar_overlay_collect,
                    thumb_ov,
                    scroll_signals_cache,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
            }
        }

        // If this is a ScrollArea, record its metrics (content size vs viewport size)
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                let content_w = (max_right - abs_x).max(layout.size.width);
                let content_h = (max_bottom - abs_y).max(layout.size.height);
                scroll_metrics.push((
                    rest.to_string(),
                    [abs_x, abs_y, layout.size.width, layout.size.height],
                    [
                        (content_w - layout.size.width).max(0.0),
                        (content_h - layout.size.height).max(0.0),
                    ],
                    [content_w, content_h],
                ));
            }
        }

        // Do not leak inner scrollable descendants' height/width past a nested scroll viewport box.
        let mut extent_r = max_right;
        let mut extent_b = max_bottom;
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                if ancestor_scroll_viewport_rest.is_some_and(|a| a != rest) {
                    extent_r = abs_x + layout.size.width;
                    extent_b = abs_y + layout.size.height;
                }
            }
        }

        [extent_r, extent_b]
    }
}

/// Mirrors [`crate::app::state::update_scrollbar_elements`] geometry using live signals so the
/// `ViewportsOnly` prepass can skip pushing track/thumb sizes into Taffy while collect still draws
/// correct rects (including when chrome sync forces a full `compute_layout` the same frame).
/// Signals are read once per scroll-area id per frame via [`scroll_area_signals_cached`].
fn scrollbar_track_overlay_abs(
    track_el: &Element,
    base_abs_x: f32,
    base_abs_y: f32,
    _layout_w: f32,
    _layout_h: f32,
    axis: &str,
    sig: &ScrollAreaSignalsCached,
) -> ([f32; 4], [f32; 4]) {
    let inset_len_px = |v: &taffy::style::LengthPercentageAuto| match *v {
        taffy::style::LengthPercentageAuto::Length(px) if px.is_finite() => px.max(0.0),
        _ => 0.0,
    };
    let vw = sig.vw;
    let vh = sig.vh;
    let cw = sig.cw;
    let ch = sig.ch;
    let sx = sig.sx;
    let sy = sig.sy;

    let allow_x = !sig.overflow_x.eq_ignore_ascii_case("hidden")
        && !sig.overflow_x.eq_ignore_ascii_case("clip")
        && !sig.overflow_x.eq_ignore_ascii_case("visible");
    let allow_y = !sig.overflow_y.eq_ignore_ascii_case("hidden")
        && !sig.overflow_y.eq_ignore_ascii_case("clip")
        && !sig.overflow_y.eq_ignore_ascii_case("visible");
    let vis_x_trim = sig.scrollbar_vis_x.trim().to_ascii_lowercase();
    let vis_y_trim = sig.scrollbar_vis_y.trim().to_ascii_lowercase();
    let show_track_x = allow_x && vis_x_trim != "never";
    let show_track_y = allow_y && vis_y_trim != "never";
    let can_scroll_x = allow_x && cw > vw + 0.5;
    let can_scroll_y = allow_y && ch > vh + 0.5;
    let reserve_for_x = show_track_x
        && match vis_x_trim.as_str() {
            "always" => true,
            "never" => false,
            _ => can_scroll_x,
        };
    let reserve_for_y = show_track_y
        && match vis_y_trim.as_str() {
            "always" => true,
            "never" => false,
            _ => can_scroll_y,
        };
    let allow_axis = if axis == "x" { allow_x } else { allow_y };
    let scrollable = if axis == "x" {
        can_scroll_x
    } else {
        can_scroll_y
    };
    let vis_trim = if axis == "x" {
        sig.scrollbar_vis_x.trim()
    } else {
        sig.scrollbar_vis_y.trim()
    };
    if !allow_axis
        || vis_trim.eq_ignore_ascii_case("never")
        || (!vis_trim.eq_ignore_ascii_case("always") && !scrollable)
    {
        return (
            [base_abs_x, base_abs_y, 0.0, 0.0],
            [base_abs_x, base_abs_y, 0.0, 0.0],
        );
    }

    let base_thickness = match axis {
        "x" => match track_el.style.max_size.height {
            Dimension::Length(px) if px.is_finite() && px > 1.0 => px,
            _ => SCROLLBAR_DEFAULT_CROSS_PX,
        },
        _ => match track_el.style.max_size.width {
            Dimension::Length(px) if px.is_finite() && px > 1.0 => px,
            _ => SCROLLBAR_DEFAULT_CROSS_PX,
        },
    };

    let x_inside = !sig.inset_x.eq_ignore_ascii_case("outside");
    let y_inside = !sig.inset_y.eq_ignore_ascii_case("outside");
    let y_is_left = sig.y_side.eq_ignore_ascii_case("left");
    let x_is_top = sig.x_side.eq_ignore_ascii_case("top");
    // Keep identical with `app::state`: reserve only opposite lane thickness.
    let corner_reserve = base_thickness;

    let mut reserve_left = 0.0_f32;
    let mut reserve_right = 0.0_f32;
    let mut reserve_top = 0.0_f32;
    let mut reserve_bottom = 0.0_f32;
    if axis == "x" && x_inside && y_inside && reserve_for_y {
        if y_is_left {
            reserve_left = corner_reserve;
        } else {
            reserve_right = corner_reserve;
        }
    }
    if axis == "y" && x_inside && y_inside && reserve_for_x {
        if x_is_top {
            reserve_top = corner_reserve;
        } else {
            reserve_bottom = corner_reserve;
        }
    }

    // Keep same track-length semantics as `app::state`: only corner reserve affects main-axis length.
    // Do not additionally subtract edge pads, otherwise inside tracks become 4px shorter.
    let edge_pad_main = 0.0_f32;

    let inset = 1.0_f32;
    let cross = SCROLLBAR_HIT_SLAB_CROSS.max(base_thickness).max(2.0);
    let thumb_cross = (cross - inset * 2.0).max(1.0);

    let main_available = match axis {
        "x" => {
            let inset_l = inset_len_px(&track_el.style.inset.left);
            let inset_r = inset_len_px(&track_el.style.inset.right);
            (vw - inset_l - inset_r).max(1.0)
        }
        _ => {
            let inset_t = inset_len_px(&track_el.style.inset.top);
            let inset_b = inset_len_px(&track_el.style.inset.bottom);
            (vh - inset_t - inset_b).max(1.0)
        }
    };

    let (track_w, track_h) = match axis {
        "x" => (
            (main_available - reserve_left - reserve_right - edge_pad_main).max(1.0),
            cross,
        ),
        _ => (
            cross,
            (main_available - reserve_top - reserve_bottom - edge_pad_main).max(1.0),
        ),
    };

    let track_rect = [base_abs_x, base_abs_y, track_w, track_h];

    // Overlay path bypasses Taffy for the thumb and must match [`components::scrollbar`] track
    // `.p("0.0625rem")` (~1px): the thumb is laid out inside the track **content** box, i.e.
    // border-box origin + `inset` on each axis. Omitting this shifts the thumb up-left vs Full mode.
    let thumb_rect = match axis {
        "x" => {
            let track = (main_available - reserve_left - reserve_right - edge_pad_main).max(1.0);
            let track_inner = (track - inset * 2.0).max(1.0);
            let (tw, ml) = if scrollable {
                let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
                let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                let tw = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                let max_offset = (cw - vw).max(0.0);
                let t = if max_offset > 1e-4 {
                    (sx / max_offset).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let ml = (track_inner - tw).max(0.0) * t;
                (tw, ml)
            } else {
                (track_inner, 0.0)
            };
            [base_abs_x + inset + ml, base_abs_y + inset, tw, thumb_cross]
        }
        _ => {
            let track = (main_available - reserve_top - reserve_bottom - edge_pad_main).max(1.0);
            let track_inner = (track - inset * 2.0).max(1.0);
            let (th, mt) = if scrollable {
                let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
                let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                let th = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                let max_offset = (ch - vh).max(0.0);
                let t = if max_offset > 1e-4 {
                    (sy / max_offset).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let mt = (track_inner - th).max(0.0) * t;
                (th, mt)
            } else {
                (track_inner, 0.0)
            };
            [base_abs_x + inset, base_abs_y + inset + mt, thumb_cross, th]
        }
    };

    (track_rect, thumb_rect)
}

#[inline]
fn mul_color_alpha(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] *= opacity;
    color
}

#[inline]
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn intersect_rect(clip: Option<[f32; 4]>, rect: [f32; 4]) -> [f32; 4] {
    if let Some(c) = clip {
        let x0 = rect[0].max(c[0]);
        let y0 = rect[1].max(c[1]);
        let x1 = (rect[0] + rect[2]).min(c[0] + c[2]);
        let y1 = (rect[1] + rect[3]).min(c[1] + c[3]);
        [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
    } else {
        rect
    }
}
