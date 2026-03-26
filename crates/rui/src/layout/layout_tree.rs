//! Taffy 布局、文本测量与将树展开为 GPU 实例 / 文本排版信息。
//!
//! 命名说明：本模块**不是** [`crate::renderer`]（wgpu/glyphon），只做 **布局与遍历**；
//! 因此使用 `LayoutTree` / `layout_tree`，避免与「渲染器」混淆。

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use taffy::prelude::*;

use crate::renderer::InstanceInput;
use crate::style::TextStyle;
use crate::core::tree::Element;
use crate::components::scrollbar::{
    scrollbar_visibility_signal_key, SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::components::scroll_area::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_AREA_FOCUS_PREFIX,
};
use crate::core::reactivity::use_signal;

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
    visibility_x: String,
    visibility_y: String,
    spring_x: f32,
    spring_y: f32,
}

fn scroll_area_signals_cached<'a>(
    rest: &str,
    cache: &'a mut HashMap<String, ScrollAreaSignalsCached>,
) -> &'a ScrollAreaSignalsCached {
    match cache.entry(rest.to_string()) {
        Entry::Occupied(o) => o.into_mut(),
        Entry::Vacant(v) => {
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
            let visibility_x =
                use_signal(&scrollbar_visibility_signal_key(rest, "x"), || "auto".to_string()).peek();
            let visibility_y =
                use_signal(&scrollbar_visibility_signal_key(rest, "y"), || "auto".to_string()).peek();
            let spring_x = crate::style::use_spring(&format!("scrollbar_spring_{rest}:x"), 0.0)
                .peek()
                .clamp(0.0, 1.0);
            let spring_y = crate::style::use_spring(&format!("scrollbar_spring_{rest}:y"), 0.0)
                .peek()
                .clamp(0.0, 1.0);
            v.insert(ScrollAreaSignalsCached {
                vw,
                vh,
                cw,
                ch,
                sx,
                sy,
                overflow_x,
                overflow_y,
                visibility_x,
                visibility_y,
                spring_x,
                spring_y,
            })
        }
    }
}

/// 一次布局+收集的产出。
pub struct RenderResult {
    pub instances: Vec<InstanceInput>,
    pub text_nodes: Vec<TextLayoutInfo>,
    pub scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>, // (rest, bounds, max_scroll, content_size)
    pub hover_cursor: Option<crate::core::tree::CursorKind>,
}

/// 单段文本在布局后的几何与样式快照（供 glyphon 使用）。
#[derive(Clone)]
pub struct TextLayoutInfo {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub clip_bounds: Option<[f32; 4]>,
    pub style: TextStyle,
}

/// 驱动 Taffy 并对 `Element` 树做 layout 与实例收集。
pub struct LayoutTree {
    pub taffy: TaffyTree<()>,
    pub(crate) root_node: Option<NodeId>,
    text_measurements: HashMap<taffy::NodeId, TextMeasureMeta>,
    /// Track nodes visited in the current build pass for pruning.
    visited_nodes: std::collections::HashSet<NodeId>,
    /// Track all nodes currently in the taffy tree.
    total_nodes: std::collections::HashSet<NodeId>,
}

/// 文本叶子的测量元数据（与 [`TextStyle`] 行高一致，避免布局盒过矮裁切 glyph）。
struct TextMeasureMeta {
    size: f32,
    text: String,
    is_icon: bool,
    line_height_px: Option<f32>,
    line_height_scale: Option<f32>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            root_node: None,
            text_measurements: HashMap::new(),
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
        root.style.size.width = Dimension::Length(width);
        root.style.size.height = Dimension::Length(height);

        self.text_measurements.clear();
        self.visited_nodes.clear();
        // DO NOT clear taffy anymore; we rely on reconciliation and selective removal.
        let root_node = self.build_taffy_node_mut(root);
        self.root_node = Some(root_node);

        // Prune nodes that were not visited in this pass.
        let to_remove: Vec<NodeId> = self.total_nodes.iter()
            .filter(|id| !self.visited_nodes.contains(id))
            .cloned()
            .collect();
        for node_id in to_remove {
            let _ = self.taffy.remove(node_id);
            self.total_nodes.remove(&node_id);
            self.text_measurements.remove(&node_id);
        }

        let available_space = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };

        self.taffy
            .compute_layout_with_measure(
                root_node,
                available_space,
                |known_dimensions, _available_space, node_id, _context, _style| {
                    if let Some(m) = self.text_measurements.get(&node_id) {
                        let size = m.size;
                        let is_icon = m.is_icon;
                        let line_count = m.text.split('\n').count().max(1) as f32;
                        let width = if let Some(w) = known_dimensions.width {
                            w
                        } else if is_icon {
                            size
                        } else {
                            let max_chars = m.text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
                            max_chars * size * 0.55
                        };
                        
                        // 行高必须参与测量。若 flex 传入偏小的 definite height，旧逻辑会完全采用 h，
                        // 叶节点布局盒过矮，prepare 里 buffer.set_size 会按矮盒裁切，leading/line_height 看似「无效」。
                        let single_line_height = if is_icon {
                            size
                        } else if let Some(lh) = m.line_height_px {
                            lh
                        } else if let Some(scale) = m.line_height_scale {
                            size * scale
                        } else {
                            size * 1.2
                        };
                        let intrinsic_height = single_line_height * line_count;

                        let height = match known_dimensions.height {
                            Some(h) => intrinsic_height.max(h),
                            None => intrinsic_height,
                        };
                        taffy::geometry::Size { width, height }
                    } else {
                        taffy::geometry::Size::ZERO
                    }
                },
            )
            .unwrap();

        let mut instances = Vec::new();
        let mut text_nodes = Vec::new();
        let mut scroll_metrics = Vec::new();
        let mut hover_cursor = None;

        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            &mut instances,
            &mut text_nodes,
            &mut scroll_metrics,
            &mut hover_cursor,
            false,
            None,
            &mut None,
        );

        RenderResult {
            instances,
            text_nodes,
            scroll_metrics,
            hover_cursor,
        }
    }

    /// When `scroll_only` is true, skip [`TaffyTree::compute_layout`]: `Element.scroll_offset` does
    /// not participate in flex/grid sizing (only affects collect-time drawing). Use after
    /// `update_scrollbar_elements` synced only `scroll:` offsets and no scrollbar styles changed
    /// — avoids O(nested) full layout each frame when many `ScrollArea`s are nested.
    pub fn build_and_layout_fast(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        scroll_only: bool,
    ) -> RenderResult {
        let root_node = self.root_node.expect("fast path requires existing root_node");

        // Must match `build_and_layout`: the root's Element + Taffy style must follow the current
        // viewport. Otherwise `compute_layout` still sees stale definite sizes from the last full
        // build (e.g. after maximize / restore while status is `Clean`) and the UI stays a small
        // island in the corner while the swapchain is full-window.
        root.style.size.width = Dimension::Length(width);
        root.style.size.height = Dimension::Length(height);
        self.update_taffy_node_style(root);

        if !scroll_only {
            let available_space = Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            };

            // Note: We assume the caller has updated any necessary styles (like scrollbar thumbs)
            // using update_taffy_node_style before calling this.
            self.taffy
                .compute_layout_with_measure(
                    root_node,
                    available_space,
                    |known_dimensions, _available_space, node_id, _context, _style| {
                        if let Some(m) = self.text_measurements.get(&node_id) {
                            let size = m.size;
                            let line_count = m.text.split('\n').count().max(1) as f32;
                            let single_line_height = if m.is_icon { size } else if let Some(lh) = m.line_height_px { lh } else if let Some(scale) = m.line_height_scale { size * scale } else { size * 1.2 };
                            let intrinsic_height = single_line_height * line_count;
                            let width = if let Some(w) = known_dimensions.width { w } else if m.is_icon { size } else { (m.text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32) * size * 0.55 };
                            let height = match known_dimensions.height { Some(h) => intrinsic_height.max(h), None => intrinsic_height };
                            taffy::geometry::Size { width, height }
                        } else {
                            taffy::geometry::Size::ZERO
                        }
                    },
                )
                .unwrap();
        }

        let mut instances = Vec::new();
        let mut text_nodes = Vec::new();
        let mut scroll_metrics = Vec::new();
        let mut hover_cursor = None;

        let mut scroll_signal_cache: HashMap<String, ScrollAreaSignalsCached> = HashMap::new();
        let mut scroll_cache_slot: Option<&mut HashMap<String, ScrollAreaSignalsCached>> =
            if scroll_only {
                Some(&mut scroll_signal_cache)
            } else {
                None
            };

        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            &mut instances,
            &mut text_nodes,
            &mut scroll_metrics,
            &mut hover_cursor,
            scroll_only,
            None,
            &mut scroll_cache_slot,
        );

        RenderResult { instances, text_nodes, scroll_metrics, hover_cursor }
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
                        is_icon,
                        line_height_px: element.text_style.line_height_px,
                        line_height_scale: element.text_style.line_height_scale,
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
        instances: &mut Vec<InstanceInput>,
        text_nodes: &mut Vec<TextLayoutInfo>,
        scroll_metrics: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
        hover_cursor: &mut Option<crate::core::tree::CursorKind>,
        scroll_only: bool,
        thumb_geometry_override: Option<[f32; 4]>,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    ) -> [f32; 2] {
        element.layout_node = Some(node);
        let layout = self.taffy.layout(node).unwrap();
        let base_abs_x = abs_offset[0] + layout.location.x;
        let base_abs_y = abs_offset[1] + layout.location.y;

        let mut thumb_rect_for_child: Option<[f32; 4]> = None;

        let (abs_x, abs_y, lay_w, lay_h) = if let Some(r) = thumb_geometry_override {
            (r[0], r[1], r[2], r[3])
        } else if scroll_only {
            if let Some(cache) = scroll_signals_cache.as_mut() {
                if let Some(fid) = element.focus_id.as_deref() {
                    if let Some(rest_axis) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
                        let (rest, axis) = rest_axis.rsplit_once(':').unwrap_or((rest_axis, "y"));
                        let sig = scroll_area_signals_cached(rest, *cache);
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

        let render_x = abs_x - scroll_acc[0];
        let render_y = abs_y - scroll_acc[1];
        let render_rect = [render_x, render_y, lay_w, lay_h];
        
        // Clipping & Visibility
        let mut clip = parent_clip;
        if element.style.overflow.x != taffy::style::Overflow::Visible || element.style.overflow.y != taffy::style::Overflow::Visible {
            clip = Some(intersect_rect(clip, render_rect));
        }
        let draw_rect = intersect_rect(clip, render_rect);

        // Hit Testing (Update cursor if mouse is over visible part)
        if mouse_pos[0] >= draw_rect[0] && mouse_pos[0] <= draw_rect[0] + draw_rect[2] &&
           mouse_pos[1] >= draw_rect[1] && mouse_pos[1] <= draw_rect[1] + draw_rect[3] {
            if let Some(c) = element.cursor {
                *hover_cursor = Some(c);
            }
        }

        // Collect instance
        if (element.style.bg_color[3] > 0.0 || element.style.border_width > 0.0 || element.style.shadow_color[3] > 0.0)
            && draw_rect[2] > 0.0 && draw_rect[3] > 0.0 {
            let resolved_radii = resolve_corner_radii(element.style.radii, draw_rect[2], draw_rect[3]);
            instances.push(InstanceInput {
                rect_pos: [draw_rect[0], draw_rect[1]], rect_size: [draw_rect[2], draw_rect[3]],
                color: element.style.bg_color, border_color: element.style.border_color,
                radii: resolved_radii, border_width: element.style.border_width,
                shadow_color: element.style.shadow_color, shadow_blur: element.style.shadow_blur,
                _padding: 0.0,
            });
        }

        if let Some(text) = &element.text {
            text_nodes.push(TextLayoutInfo {
                text: text.clone(), x: render_x, y: render_y,
                width: lay_w, height: lay_h,
                clip_bounds: clip, style: element.text_style.clone(),
            });
        }

        // Recurse children
        let mut next_scroll_acc = scroll_acc;
        if element.style.overflow.x != taffy::style::Overflow::Visible { next_scroll_acc[0] += element.scroll_offset[0]; }
        if element.style.overflow.y != taffy::style::Overflow::Visible { next_scroll_acc[1] += element.scroll_offset[1]; }

        let pass_to_children = element
            .focus_id
            .as_deref()
            .and_then(|fid| fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX))
            .or(ancestor_scroll_viewport_rest);

        let child_nodes = self.taffy.children(node).unwrap();
        let any_z = element.children.iter().any(|c| c.style.z_index != 0);
        let mut max_right = abs_x + lay_w;
        let mut max_bottom = abs_y + lay_h;

        if !any_z {
            for (i, child) in element.children.iter_mut().enumerate() {
                let thumb_ov = if thumb_rect_for_child.is_some() {
                    if child
                        .focus_id
                        .as_deref()
                        .map_or(false, |f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX))
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
                    instances,
                    text_nodes,
                    scroll_metrics,
                    hover_cursor,
                    scroll_only,
                    thumb_ov,
                    scroll_signals_cache,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
            }
        } else {
            let mut order: Vec<usize> = (0..element.children.len()).collect();
            order.sort_by(|&a, &b| element.children[a].style.z_index.cmp(&element.children[b].style.z_index).then(a.cmp(&b)));
            for i in order {
                let thumb_ov = if thumb_rect_for_child.is_some() {
                    let c = &element.children[i];
                    if c
                        .focus_id
                        .as_deref()
                        .map_or(false, |f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX))
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
                    instances,
                    text_nodes,
                    scroll_metrics,
                    hover_cursor,
                    scroll_only,
                    thumb_ov,
                    scroll_signals_cache,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
            }
        }

        // If this is a ScrollArea, record its metrics (content size vs viewport size)
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX) {
                let content_w = (max_right - abs_x).max(layout.size.width);
                let content_h = (max_bottom - abs_y).max(layout.size.height);
                scroll_metrics.push((rest.to_string(), [abs_x, abs_y, layout.size.width, layout.size.height], [content_w - layout.size.width, content_h - layout.size.height], [content_w, content_h]));
            }
        }

        // Do not leak inner scrollable descendants' height/width past a nested scroll viewport box.
        let mut extent_r = max_right;
        let mut extent_b = max_bottom;
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX) {
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
/// scroll-only path can skip `compute_layout` while keeping thumb/track rects correct.
/// Signals are read once per scroll-area id per frame via [`scroll_area_signals_cached`].
fn scrollbar_track_overlay_abs(
    track_el: &Element,
    base_abs_x: f32,
    base_abs_y: f32,
    layout_w: f32,
    layout_h: f32,
    axis: &str,
    sig: &ScrollAreaSignalsCached,
) -> ([f32; 4], [f32; 4]) {
    const HIT_SLAB: f32 = 8.0;
    let vw = sig.vw;
    let vh = sig.vh;
    let cw = sig.cw;
    let ch = sig.ch;
    let sx = sig.sx;
    let sy = sig.sy;

    let allow_axis = match axis {
        "x" => {
            !sig.overflow_x.eq_ignore_ascii_case("hidden")
                && !sig.overflow_x.eq_ignore_ascii_case("clip")
                && !sig.overflow_x.eq_ignore_ascii_case("visible")
        }
        _ => {
            !sig.overflow_y.eq_ignore_ascii_case("hidden")
                && !sig.overflow_y.eq_ignore_ascii_case("clip")
                && !sig.overflow_y.eq_ignore_ascii_case("visible")
        }
    };
    let can_scroll = match axis {
        "x" => allow_axis && cw > vw + 1.0,
        _ => allow_axis && ch > vh + 1.0,
    };

    let vis = match axis {
        "x" => sig.visibility_x.as_str(),
        _ => sig.visibility_y.as_str(),
    };
    let vis = vis.to_ascii_lowercase();
    let v = if vis == "never" {
        0.0
    } else if vis == "always" {
        1.0
    } else {
        match axis {
            "x" => sig.spring_x,
            _ => sig.spring_y,
        }
    };

    let base_thickness = match axis {
        "x" => match track_el.style.max_size.height {
            Dimension::Length(px) => px,
            _ => layout_h.max(8.0),
        },
        _ => match track_el.style.max_size.width {
            Dimension::Length(px) => px,
            _ => layout_w.max(8.0),
        },
    };

    if !can_scroll {
        return (
            [base_abs_x, base_abs_y, 0.0, 0.0],
            [base_abs_x, base_abs_y, 0.0, 0.0],
        );
    }

    let inset = 1.0_f32;
    let use_hit_slab = vis == "auto" && v < 0.01;
    let cross = if use_hit_slab {
        HIT_SLAB.max(base_thickness * (0.15 + 0.85 * v)).max(2.0)
    } else {
        (base_thickness * (0.15 + 0.85 * v)).max(2.0)
    };
    let thumb_cross = (cross - inset * 2.0).max(1.0);

    let (track_w, track_h) = match axis {
        "x" => (vw.max(1.0), cross),
        _ => (cross, vh.max(1.0)),
    };

    let track_rect = [base_abs_x, base_abs_y, track_w, track_h];

    let thumb_rect = match axis {
        "x" => {
            let track = vw.max(1.0);
            let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
            let min_thumb = track.min(24.0);
            let tw = (track * ratio).clamp(min_thumb, track.max(min_thumb));
            let max_offset = (cw - vw).max(0.0);
            let t = if max_offset > 0.0 { sx / max_offset } else { 0.0 };
            let ml = (track - tw).max(0.0) * t.clamp(0.0, 1.0);
            [base_abs_x + ml, base_abs_y, tw, thumb_cross]
        }
        _ => {
            let track = vh.max(1.0);
            let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
            let min_thumb = track.min(24.0);
            let th = (track * ratio).clamp(min_thumb, track.max(min_thumb));
            let max_offset = (ch - vh).max(0.0);
            let t = if max_offset > 0.0 { sy / max_offset } else { 0.0 };
            let mt = (track - th).max(0.0) * t.clamp(0.0, 1.0);
            [base_abs_x, base_abs_y + mt, thumb_cross, th]
        }
    };

    (track_rect, thumb_rect)
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
