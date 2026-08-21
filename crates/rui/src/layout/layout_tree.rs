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
    scrollbar_min_thumb_along_track, scrollbar_track_content_inset,
    scrollbar_visibility_signal_key, SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX,
};
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
    scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
    scroll_scrollbar_x_side_signal_key, scroll_scrollbar_y_side_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::{Element, UnitCtx};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::TextStyle;

/// SolidJS-style portal mount: default overlays live on a dedicated absolute layer at the
/// app root (analogous to `document.body`), never as flex/grid siblings of the app shell.
pub const PORTAL_LAYER_ID: &str = "__rui_portal_layer";

/// Attr on nodes placed by [`hoist_portal_subtrees`] (root layer or a `mount` host).
pub const ATTR_HOISTED_PORTAL: &str = "rui_hoisted_portal";

/// Optional Solid-style `mount` target: host [`Element::template_id`] (see [`Portal`] mount prop).
pub const ATTR_PORTAL_MOUNT: &str = "rui_portal_mount";

/// Stable identity for a portal surface (underlay / panel). Required for cross-frame retention.
pub const ATTR_PORTAL_KEY: &str = "rui_portal_key";

/// Zero-size marker left in the source tree after a portal node is lifted (Solid keep-alive).
pub const ATTR_PORTAL_SLOT: &str = "rui_portal_slot";

/// Remembers `rui_portal_mount` on a hoisted node so remount can target the same host.
const ATTR_HOISTED_MOUNT: &str = "rui_portal_hoisted_mount";

/// Opt out of ancestor overflow clip (Command / Dialog stages that must paint over the sidebar).
pub const ATTR_ESCAPE_OVERFLOW_CLIP: &str = "rui_escape_overflow_clip";

/// Portal stacking shells must not steal pointer / wheel hits from content underneath.
///
/// True when the node opts into [`PointerEvents::BoxNone`], or is the root
/// [`PORTAL_LAYER_ID`] mount. Interactive hoisted surfaces (`portal_panel` underlays)
/// stay `auto` so they still receive clicks.
#[inline]
pub fn portal_hit_passthrough(el: &Element) -> bool {
    use crate::core::tree::PointerEvents;
    el.pointer_events_mode() == PointerEvents::BoxNone
        || el.template_id.as_deref() == Some(PORTAL_LAYER_ID)
}

/// Lift `rui_portal=1` subtrees onto their mount host (Solid `<Portal mount={…}>`).
///
/// Aligns with Solid’s Portal model: **move** content to a mount and **keep it alive** while
/// the portal is still declared — not destroy-and-recreate every frame.
///
/// - Default / missing host → stable root layer [`PORTAL_LAYER_ID`] (`document.body`).
///   Body mount is viewport-fixed and **escapes ancestor overflow cropping** (Solid’s
///   “outside of an cropping layout”).
/// - `rui_portal_mount=<template_id>` → that host's children (local overlay stack; still
///   subject to that host’s ancestors’ overflow, matching Solid `mount={el}`).
/// - Unknown mount id → falls back to the root layer.
/// - Fresh lifts replace in-source nodes with [`ATTR_PORTAL_SLOT`] markers so the reactive
///   tree still expresses “this portal is wanted” after the move.
/// - Stable frames (no fresh `rui_portal=1`): **do not** tear down the portal layer — only
///   prune keys that are no longer wanted. Rewriting the layer every frame while skipping
///   Taffy rebuild desyncs `element.children` vs `taffy.children` (OOB panic in collect).
/// - Nested portals that appear **inside** an already-hoisted overlay (e.g. Select menu
///   opened inside a Dialog stage on the portal layer) are lifted in a second pass so they
///   become viewport-fixed siblings — otherwise `absolute` + viewport `left`/`top` double-
///   offsets the menu relative to the local containing block.
/// - Returns a non-zero “structure dirty” count when the portal tree changed (caller must
///   full-rebuild Taffy).
pub fn hoist_portal_subtrees(root: &mut Element) -> usize {
    let dirty = ensure_portal_layer(root);

    let mut wanted: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_portal_slot_keys(root, &mut wanted);

    let pending = count_rui_portal_nodes(root);
    let mut structure_dirty = dirty;
    if pending == 0 {
        // Retention path: leave live hoisted nodes in place; drop closed keys only.
        structure_dirty += prune_unwanted_hoisted(root, &wanted);
    } else {
        let mut prev = take_all_hoisted(root);

        let mut lifted = Vec::new();
        hoist_portal_rec_with_slots(root, &mut lifted);
        debug_assert_eq!(lifted.len(), pending);

        let mut fresh_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut root_bucket = Vec::new();
        let mut host_buckets: Vec<(Vec<usize>, Vec<Element>)> = Vec::new();

        for mut node in lifted {
            let mount = node
                .attrs
                .get(ATTR_PORTAL_MOUNT)
                .cloned()
                .unwrap_or_default();
            let key = portal_key_of(&node);
            wanted.insert(key.clone());
            fresh_keys.insert(key.clone());
            prev.remove(&key);
            prepare_hoisted_node(&mut node, &key);
            place_hoisted(
                root,
                node,
                mount.trim(),
                &mut root_bucket,
                &mut host_buckets,
            );
        }

        for (key, node) in prev {
            if !wanted.contains(&key) || fresh_keys.contains(&key) {
                continue;
            }
            let mount = node
                .attrs
                .get(ATTR_HOISTED_MOUNT)
                .cloned()
                .unwrap_or_default();
            place_hoisted(
                root,
                node,
                mount.trim(),
                &mut root_bucket,
                &mut host_buckets,
            );
        }

        let layer_idx = root
            .children
            .iter()
            .position(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        {
            let layer = &mut root.children[layer_idx];
            layer.style.position = taffy::style::Position::Absolute;
            layer.style.position_fixed = true;
            layer.style.overflow.x = taffy::style::Overflow::Visible;
            layer.style.overflow.y = taffy::style::Overflow::Visible;
            if layer.pointer_events_mode() == crate::core::tree::PointerEvents::Auto {
                layer.set_attr(
                    crate::core::tree::ATTR_POINTER_EVENTS,
                    crate::core::tree::PointerEvents::BoxNone.as_str(),
                );
            }
            layer.children.clear();
            layer.children.append(&mut root_bucket);
        }

        for (path, mut kids) in host_buckets {
            let host = element_mut_by_path(root, &path);
            host.children.append(&mut kids);
        }

        // Layer membership was rewritten — always force a Taffy rebuild this frame.
        structure_dirty = pending.max(1).max(dirty);
    }

    // Dialog already on the layer + Select opens inside it: main-tree pending is 0, but a
    // fresh `rui_portal` still sits under the hoisted stage — promote it now.
    structure_dirty + hoist_nested_portals_from_hoisted(root, &mut wanted)
}

fn count_rui_portal_nodes(node: &Element) -> usize {
    if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return 0;
    }
    let mut n = 0;
    for c in &node.children {
        if c.attrs.get(ATTR_PORTAL_SLOT).is_some() {
            continue;
        }
        // Match `hoist_portal_rec_with_slots`: nested `rui_portal` nodes are lifted
        // out of their parent before the parent itself is lifted.
        if c.attrs.get("rui_portal").map(|s| s.as_str()) == Some("1") {
            n += 1 + count_rui_portal_nodes(c);
        } else {
            n += count_rui_portal_nodes(c);
        }
    }
    n
}

/// Drop hoisted nodes whose portal key is no longer declared by a slot / open overlay.
/// Returns how many nodes were removed (non-zero → Taffy rebuild).
fn prune_unwanted_hoisted(root: &mut Element, wanted: &std::collections::HashSet<String>) -> usize {
    let mut dropped = 0usize;
    if let Some(layer) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        let before = layer.children.len();
        layer.children.retain(|c| {
            if c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) != Some("1") {
                return true;
            }
            wanted.contains(&portal_key_of(c))
        });
        dropped += before.saturating_sub(layer.children.len());
    }
    dropped += prune_hoisted_from_hosts(root, wanted);
    dropped
}

fn prune_hoisted_from_hosts(
    node: &mut Element,
    wanted: &std::collections::HashSet<String>,
) -> usize {
    if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return 0;
    }
    let mut dropped = 0usize;
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i]
            .attrs
            .get(ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1")
        {
            if wanted.contains(&portal_key_of(&node.children[i])) {
                i += 1;
            } else {
                node.children.remove(i);
                dropped += 1;
            }
        } else {
            dropped += prune_hoisted_from_hosts(&mut node.children[i], wanted);
            i += 1;
        }
    }
    dropped
}

fn place_hoisted(
    root: &Element,
    node: Element,
    mount: &str,
    root_bucket: &mut Vec<Element>,
    host_buckets: &mut Vec<(Vec<usize>, Vec<Element>)>,
) {
    if mount.is_empty() {
        root_bucket.push(node);
        return;
    }
    let mut path = Vec::new();
    if find_template_id_path(root, mount, &mut path) && !path.is_empty() {
        if let Some((_, kids)) = host_buckets.iter_mut().find(|(p, _)| *p == path) {
            kids.push(node);
        } else {
            host_buckets.push((path, vec![node]));
        }
    } else {
        root_bucket.push(node);
    }
}

fn portal_key_of(el: &Element) -> String {
    if let Some(k) = el.attrs.get(ATTR_PORTAL_KEY) {
        let t = k.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    if let Some(k) = el
        .attrs
        .get("rui_floating")
        .or_else(|| el.attrs.get("rui_select_menu"))
    {
        return format!("floating:{k}");
    }
    if let Some(tid) = el.template_id.as_deref() {
        return format!("tid:{tid}");
    }
    format!("z:{}", el.style.z_index)
}

fn prepare_hoisted_node(node: &mut Element, key: &str) {
    let mount = node.attrs.remove(ATTR_PORTAL_MOUNT).unwrap_or_default();
    node.attrs.remove("rui_portal");
    node.set_attr(ATTR_HOISTED_PORTAL, "1");
    node.set_attr(ATTR_PORTAL_KEY, key);
    if !mount.trim().is_empty() {
        node.set_attr(ATTR_HOISTED_MOUNT, mount.trim());
    }
    if node.pointer_events_mode() == crate::core::tree::PointerEvents::Auto
        && portal_root_defaults_to_box_none(node)
    {
        node.set_attr(
            crate::core::tree::ATTR_POINTER_EVENTS,
            crate::core::tree::PointerEvents::BoxNone.as_str(),
        );
    }
    if node.style.position != taffy::style::Position::Absolute {
        node.style.position = taffy::style::Position::Absolute;
    }
    // Default body mount (Solid → document.body): viewport-fixed so scrolled
    // ancestors cannot shift or crop the overlay. Local `mount=` hosts keep
    // absolute positioning inside that host's containing block.
    if mount.trim().is_empty() {
        node.style.position_fixed = true;
    }
}

fn make_portal_slot(key: &str) -> Element {
    let mut slot = Element::new()
        .absolute()
        .w(0.0)
        .h(0.0)
        .pointer_events_none();
    slot.set_attr(ATTR_PORTAL_SLOT, key);
    slot.set_attr(ATTR_PORTAL_KEY, key);
    slot
}

fn collect_portal_slot_keys(node: &Element, out: &mut std::collections::HashSet<String>) {
    // Walk the portal layer too: nested menus leave slots *inside* already-hoisted
    // Dialog/Popover stages; skipping the layer would prune those menus next frame.
    if let Some(k) = node.attrs.get(ATTR_PORTAL_SLOT) {
        let t = k.trim();
        if !t.is_empty() {
            out.insert(t.to_string());
        }
    }
    for c in &node.children {
        collect_portal_slot_keys(c, out);
    }
}

/// Promote `rui_portal=1` nodes that were inserted under already-hoisted overlays.
///
/// [`hoist_portal_rec_with_slots`] skips [`PORTAL_LAYER_ID`], and
/// [`count_rui_portal_nodes`] does too — so a Select opened inside a live Dialog never
/// enters the main lift path. Extract here and append as siblings on the same mount.
fn hoist_nested_portals_from_hoisted(
    root: &mut Element,
    wanted: &mut std::collections::HashSet<String>,
) -> usize {
    let mut lifted = Vec::new();
    if let Some(layer) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        for child in &mut layer.children {
            hoist_portal_rec_with_slots(child, &mut lifted);
        }
    }
    extract_nested_portals_from_host_hoisted(root, &mut lifted);
    if lifted.is_empty() {
        return 0;
    }

    let n = lifted.len();
    let mut root_bucket = Vec::new();
    let mut host_buckets: Vec<(Vec<usize>, Vec<Element>)> = Vec::new();
    for mut node in lifted {
        let mount = node
            .attrs
            .get(ATTR_PORTAL_MOUNT)
            .cloned()
            .unwrap_or_default();
        let key = portal_key_of(&node);
        wanted.insert(key.clone());
        prepare_hoisted_node(&mut node, &key);
        place_hoisted(
            root,
            node,
            mount.trim(),
            &mut root_bucket,
            &mut host_buckets,
        );
    }

    if !root_bucket.is_empty() {
        let layer = root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        layer.children.append(&mut root_bucket);
    }
    for (path, mut kids) in host_buckets {
        let host = element_mut_by_path(root, &path);
        host.children.append(&mut kids);
    }
    n
}

fn extract_nested_portals_from_host_hoisted(node: &mut Element, lifted: &mut Vec<Element>) {
    if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return;
    }
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i]
            .attrs
            .get(ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1")
        {
            hoist_portal_rec_with_slots(&mut node.children[i], lifted);
            i += 1;
        } else {
            extract_nested_portals_from_host_hoisted(&mut node.children[i], lifted);
            i += 1;
        }
    }
}

fn take_all_hoisted(root: &mut Element) -> HashMap<String, Element> {
    let mut out = HashMap::new();
    if let Some(layer) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        for kid in std::mem::take(&mut layer.children) {
            if kid.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1") {
                let key = portal_key_of(&kid);
                out.insert(key, kid);
            }
        }
    }
    take_hoisted_from_hosts(root, &mut out);
    out
}

fn take_hoisted_from_hosts(node: &mut Element, out: &mut HashMap<String, Element>) {
    if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return;
    }
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i]
            .attrs
            .get(ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1")
        {
            let kid = node.children.remove(i);
            out.insert(portal_key_of(&kid), kid);
        } else {
            take_hoisted_from_hosts(&mut node.children[i], out);
            i += 1;
        }
    }
}

fn portal_root_defaults_to_box_none(el: &Element) -> bool {
    el.on_click.is_none()
        && el.on_mousedown.is_none()
        && el.on_mouseup.is_none()
        && el.on_context_menu.is_none()
        && el.on_double_click.is_none()
}

fn ensure_portal_layer(root: &mut Element) -> usize {
    if let Some(idx) = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        // Keep the layer last so collect paint splits as a contiguous overlay suffix.
        // Moving without a Taffy rebuild desyncs children → white blocks / missing text.
        let mut dirty = 0usize;
        if idx + 1 != root.children.len() {
            let layer = root.children.remove(idx);
            root.children.push(layer);
            dirty = 1;
        }
        let layer = root
            .children
            .last_mut()
            .expect("portal layer present after ensure");
        // Solid body mount: viewport-fixed, never inherit ancestor overflow cropping.
        layer.style.position = taffy::style::Position::Absolute;
        layer.style.position_fixed = true;
        layer.style.overflow.x = taffy::style::Overflow::Visible;
        layer.style.overflow.y = taffy::style::Overflow::Visible;
        return dirty;
    }
    root.children.push(
        // Absolute fill via inset only — avoid w/h 100% on a flex-parent child (over-constrained
        // absolute sizing has crushed sidebar/main width in the gallery shell).
        Element::new()
            .fixed()
            .inset(0.0)
            .overflow_x("visible")
            .overflow_y("visible")
            .pointer_events_box_none()
            .template_id(PORTAL_LAYER_ID),
    );
    1
}

fn find_template_id_path(node: &Element, id: &str, path: &mut Vec<usize>) -> bool {
    if node.template_id.as_deref() == Some(id) {
        return true;
    }
    for (i, c) in node.children.iter().enumerate() {
        // Do not search inside the root portal layer — hosts live in the app tree.
        if c.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        path.push(i);
        if find_template_id_path(c, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn element_mut_by_path<'a>(root: &'a mut Element, path: &[usize]) -> &'a mut Element {
    let mut cur = root;
    for &i in path {
        cur = &mut cur.children[i];
    }
    cur
}

fn hoist_portal_rec_with_slots(node: &mut Element, lifted: &mut Vec<Element>) {
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            i += 1;
            continue;
        }
        if node.children[i].attrs.get(ATTR_PORTAL_SLOT).is_some() {
            i += 1;
            continue;
        }
        hoist_portal_rec_with_slots(&mut node.children[i], lifted);
        if node.children[i].attrs.get("rui_portal").map(|s| s.as_str()) == Some("1") {
            let n = node.children.remove(i);
            let key = portal_key_of(&n);
            node.children.insert(i, make_portal_slot(&key));
            lifted.push(n);
            i += 1;
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

/// Non-uniform border paint: four axis-aligned strips (corners belong to top/bottom).
/// SDF `border_width` is single-valued, so per-side width/color cannot use the rounded stroke.
fn push_per_side_border_strips(
    instances: &mut Vec<InstanceInput>,
    render_rect: [f32; 4],
    widths: [f32; 4],
    colors: [[f32; 4]; 4],
    styles: [crate::style::LineStyle; 4],
    opacity: f32,
    transform: [f32; 6],
    parent_clip: Option<[f32; 4]>,
) {
    use crate::style::LineStyle;
    let [x, y, w, h] = render_rect;
    let [bt, br, bb, bl] = widths;

    fn darken(c: [f32; 4]) -> [f32; 4] {
        [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6, c[3]]
    }
    fn lighten(c: [f32; 4]) -> [f32; 4] {
        [(c[0] * 1.4).min(1.0), (c[1] * 1.4).min(1.0), (c[2] * 1.4).min(1.0), c[3]]
    }

    /// Push a single side strip with style-aware rendering.
    /// `horizontal`: true for top/bottom edges (span full width), false for left/right.
    fn push_styled_side(
        instances: &mut Vec<InstanceInput>,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        style: LineStyle,
        horizontal: bool,
        opacity: f32,
        transform: [f32; 6],
        parent_clip: Option<[f32; 4]>,
    ) {
        if size[0] <= 0.0 || size[1] <= 0.0 || color[3] <= 0.0 || style.is_invisible() {
            return;
        }
        let bw = if horizontal { size[1] } else { size[0] };
        let span = if horizontal { size[0] } else { size[1] };

        let push_one = |instances: &mut Vec<InstanceInput>, p: [f32; 2], s: [f32; 2], c: [f32; 4]| {
            if s[0] <= 0.0 || s[1] <= 0.0 || c[3] <= 0.0 {
                return;
            }
            instances.push(
                InstanceInput {
                    rect_pos: p,
                    rect_size: s,
                    color: mul_color_alpha(c, opacity),
                    border_color: [0.0; 4],
                    radii: [0.0; 4],
                    border_width: 0.0,
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    shadow_inset: 0.0,
                    transform,
                    clip_pos: [0.0, 0.0],
                    clip_size: InstanceInput::NO_CLIP_SIZE,
                    border_extra: 0.0,
                }
                .with_clip(parent_clip),
            );
        };

        match style {
            LineStyle::Solid => {
                push_one(instances, pos, size, color);
            }
            LineStyle::Dotted => {
                let dot_size = bw;
                let step = dot_size * 2.0;
                let mut t = 0.0;
                while t < span {
                    let seg = dot_size.min(span - t);
                    let (p, s) = if horizontal {
                        ([pos[0] + t, pos[1]], [seg, size[1]])
                    } else {
                        ([pos[0], pos[1] + t], [size[0], seg])
                    };
                    push_one(instances, p, s, color);
                    t += step;
                }
            }
            LineStyle::Dashed => {
                let dash = bw * 3.0;
                let gap = bw * 1.5;
                let step = dash + gap;
                let mut t = 0.0;
                while t < span {
                    let seg = dash.min(span - t);
                    let (p, s) = if horizontal {
                        ([pos[0] + t, pos[1]], [seg, size[1]])
                    } else {
                        ([pos[0], pos[1] + t], [size[0], seg])
                    };
                    push_one(instances, p, s, color);
                    t += step;
                }
            }
            LineStyle::Double => {
                let third = bw / 3.0;
                if horizontal {
                    push_one(instances, pos, [size[0], third], color);
                    push_one(instances, [pos[0], pos[1] + bw - third], [size[0], third], color);
                } else {
                    push_one(instances, pos, [third, size[1]], color);
                    push_one(instances, [pos[0] + bw - third, pos[1]], [third, size[1]], color);
                }
            }
            LineStyle::Groove => {
                let half = bw / 2.0;
                if horizontal {
                    push_one(instances, pos, [size[0], half], darken(color));
                    push_one(instances, [pos[0], pos[1] + half], [size[0], half], lighten(color));
                } else {
                    push_one(instances, pos, [half, size[1]], darken(color));
                    push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], lighten(color));
                }
            }
            LineStyle::Ridge => {
                let half = bw / 2.0;
                if horizontal {
                    push_one(instances, pos, [size[0], half], lighten(color));
                    push_one(instances, [pos[0], pos[1] + half], [size[0], half], darken(color));
                } else {
                    push_one(instances, pos, [half, size[1]], lighten(color));
                    push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], darken(color));
                }
            }
            LineStyle::Inset => {
                push_one(instances, pos, size, darken(color));
            }
            LineStyle::Outset => {
                push_one(instances, pos, size, lighten(color));
            }
            LineStyle::None | LineStyle::Hidden => {}
        }
    }

    // Top (full width)
    push_styled_side(instances, [x, y], [w, bt], colors[0], styles[0], true, opacity, transform, parent_clip);
    // Bottom (full width)
    push_styled_side(instances, [x, y + (h - bb).max(0.0)], [w, bb], colors[2], styles[2], true, opacity, transform, parent_clip);
    let mid_h = (h - bt - bb).max(0.0);
    // Left
    push_styled_side(instances, [x, y + bt], [bl, mid_h], colors[3], styles[3], false, opacity, transform, parent_clip);
    // Right
    push_styled_side(instances, [x + (w - br).max(0.0), y + bt], [br, mid_h], colors[1], styles[1], false, opacity, transform, parent_clip);
}

/// Generate nine-slice `TexturedDraw` instances for CSS `border-image`.
fn push_border_image_draws(
    textured_draws: &mut Vec<TexturedDraw>,
    render_rect: [f32; 4],
    border_widths: [f32; 4],
    style: &crate::style::Style,
    opacity: f32,
    parent_clip: Option<[f32; 4]>,
) {
    let tex_key = match style.border_image_source.as_ref() {
        Some(k) if !k.is_empty() => k.clone(),
        _ => return,
    };

    let [rx, ry, rw, rh] = render_rect;
    let outset = style.border_image_outset;
    // Expand render rect by outset
    let bx = rx - outset[3];
    let by = ry - outset[0];
    let bw = rw + outset[3] + outset[1];
    let bh = rh + outset[0] + outset[2];

    // Border image widths (fall back to border-width)
    let biw = style.border_image_width.unwrap_or(border_widths);
    let [wt, wr, wb, wl] = biw;

    // Slice values as fractions of image (default 100% = 1.0 for UV)
    let slice = style.border_image_slice;
    let st = (slice[0] / 100.0).clamp(0.0, 1.0);
    let sr = (slice[1] / 100.0).clamp(0.0, 1.0);
    let sb = (slice[2] / 100.0).clamp(0.0, 1.0);
    let sl = (slice[3] / 100.0).clamp(0.0, 1.0);

    // Helper: create a TexturedDraw with explicit UV rect
    let mut push_slice = |rect: [f32; 4], uv_x: f32, uv_y: f32, uv_w: f32, uv_h: f32| {
        if rect[2] <= 0.0 || rect[3] <= 0.0 || uv_w <= 0.0 || uv_h <= 0.0 {
            return;
        }
        // Apply clipping
        let draw = intersect_rect(parent_clip, rect);
        if draw[2] <= 0.0 || draw[3] <= 0.0 {
            return;
        }
        // Compute UV for the clipped portion relative to the full rect
        let sub_u = if rect[2] > 0.0 { (draw[0] - rect[0]) / rect[2] } else { 0.0 };
        let sub_v = if rect[3] > 0.0 { (draw[1] - rect[1]) / rect[3] } else { 0.0 };
        let sub_uw = if rect[2] > 0.0 { draw[2] / rect[2] } else { 1.0 };
        let sub_vh = if rect[3] > 0.0 { draw[3] / rect[3] } else { 1.0 };
        // Map to source UV (Y flipped: top of rect → uv_y + uv_h)
        let final_u = uv_x + sub_u * uv_w;
        let final_v = (uv_y + uv_h) - sub_v * uv_h - sub_vh * uv_h;
        textured_draws.push(TexturedDraw {
            rect: draw,
            uv_offset: [final_u, final_v],
            uv_scale: [sub_uw * uv_w, sub_vh * uv_h],
            texture_key: tex_key.clone(),
            opacity,
        });
    };

    // Nine regions (CSS border-image nine-slice):
    // Corners (always stretch)
    push_slice([bx, by, wl, wt], 0.0, 0.0, sl, st);                                    // top-left
    push_slice([bx + bw - wr, by, wr, wt], 1.0 - sr, 0.0, sr, st);                     // top-right
    push_slice([bx, by + bh - wb, wl, wb], 0.0, 1.0 - sb, sl, sb);                      // bottom-left
    push_slice([bx + bw - wr, by + bh - wb, wr, wb], 1.0 - sr, 1.0 - sb, sr, sb);       // bottom-right

    // Edges (stretch for now; repeat/round/space would need multiple draws)
    let ew = bw - wl - wr;
    let eh = bh - wt - wb;
    let eu_w = (1.0 - sl - sr).max(0.0);
    let eu_h = (1.0 - st - sb).max(0.0);
    push_slice([bx + wl, by, ew, wt], sl, 0.0, eu_w, st);              // top edge
    push_slice([bx + wl, by + bh - wb, ew, wb], sl, 1.0 - sb, eu_w, sb); // bottom edge
    push_slice([bx, by + wt, wl, eh], 0.0, st, sl, eu_h);              // left edge
    push_slice([bx + bw - wr, by + wt, wr, eh], 1.0 - sr, st, sr, eu_h); // right edge

    // Center (only if fill)
    if style.border_image_slice_fill {
        push_slice([bx + wl, by + wt, ew, eh], sl, st, eu_w, eu_h);
    }
}

/// Minimum cross-axis size for an `auto` scrollbar at spring?; must match `app::state`.
const SCROLLBAR_HIT_SLAB_CROSS: f32 = 8.0;
/// Default track cross size when `max_w`/`max_h` is missing; matches `ScrollbarBuilder::thickness`.
const SCROLLBAR_DEFAULT_CROSS_PX: f32 = 10.0;

/// Cascades inheritable text style fields from parent to descendants.
/// Child explicit declarations (tracked in `attrs`) always win over inherited values.
///
/// `text_style.decoration_color`:
/// - `None` = CSS `currentColor` **keyword** — resolved at draw time to **this** node's
///   `color` (`prepare_text`: `decoration_color.unwrap_or(color)`).
/// - `Some(rgb)` = explicit decoration color; inherits as an absolute RGB.
///
/// We intentionally keep `None` as a keyword through inheritance (do **not** snapshot
/// `parent.color` into `Some(...)`). Snapshotting would pin underlines to the parent's
/// RGB when a child overrides `color` (e.g. Button `link` / `text-primary` under a
/// white-default root), which is the desync authors hit in practice.
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
            // Clears stale `Some` from a prior paint when the child never opted in via attr.
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

/// Test hook: run the same inherit pass as `build_and_layout*_fast`.
#[cfg(test)]
pub(crate) fn cascade_inheritable_text_style_for_test(root: &mut Element) {
    cascade_inheritable_text_style(root, None);
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

/// Reused collect buffers — cleared each frame, capacity retained (Wave P P.3.5).
struct CollectScratch {
    instances: Vec<InstanceInput>,
    textured_draws: Vec<TexturedDraw>,
    text_nodes: Vec<TextLayoutInfo>,
    input_chrome_instances: Vec<InstanceInput>,
    scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
    tex_keys: Vec<String>,
}

impl Default for CollectScratch {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            textured_draws: Vec::new(),
            text_nodes: Vec::new(),
            input_chrome_instances: Vec::new(),
            scroll_metrics: Vec::new(),
            tex_keys: Vec::new(),
        }
    }
}

impl CollectScratch {
    fn clear_for_collect(&mut self) {
        self.instances.clear();
        self.textured_draws.clear();
        self.text_nodes.clear();
        self.input_chrome_instances.clear();
        self.scroll_metrics.clear();
    }
}

/// Flex sibling wholly outside an ancestor overflow clip during `scroll_only` collect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollSiblingBandAction {
    Visit,
    SkipBefore,
    StopAfter,
}

#[inline]
fn scroll_sibling_band_action(
    child_abs: [f32; 2],
    child_size: [f32; 2],
    scroll_acc: [f32; 2],
    clip: [f32; 4],
    flex_direction: FlexDirection,
    flex_wrap: FlexWrap,
) -> ScrollSiblingBandAction {
    if flex_wrap != FlexWrap::NoWrap {
        return ScrollSiblingBandAction::Visit;
    }
    const PAD: f32 = 64.0;
    let sx = child_abs[0] - scroll_acc[0];
    let sy = child_abs[1] - scroll_acc[1];
    let sw = child_size[0];
    let sh = child_size[1];
    let clip_r = clip[0] + clip[2];
    let clip_b = clip[1] + clip[3];

    match flex_direction {
        FlexDirection::Column => {
            if sy + sh < clip[1] - PAD {
                return ScrollSiblingBandAction::SkipBefore;
            }
            if sy > clip_b + PAD {
                return ScrollSiblingBandAction::StopAfter;
            }
        }
        FlexDirection::Row => {
            if sx + sw < clip[0] - PAD {
                return ScrollSiblingBandAction::SkipBefore;
            }
            if sx > clip_r + PAD {
                return ScrollSiblingBandAction::StopAfter;
            }
        }
        // Reverse directions reorder visual stacking; band cull is unsafe.
        FlexDirection::ColumnReverse | FlexDirection::RowReverse => {}
    }
    ScrollSiblingBandAction::Visit
}

#[inline]
fn scroll_sibling_extent(child_abs: [f32; 2], child_size: [f32; 2]) -> [f32; 2] {
    [child_abs[0] + child_size[0], child_abs[1] + child_size[1]]
}

/// Prefer last full-collect recursive extent so overflowing descendants do not drop out of
/// `content_h` when this sibling is band-culled (scrollbar thumb jitter).
#[inline]
fn skipped_sibling_extent(child: &Element, child_abs: [f32; 2], child_size: [f32; 2]) -> [f32; 2] {
    if child.collect_extent_valid {
        child.collect_extent
    } else {
        scroll_sibling_extent(child_abs, child_size)
    }
}

#[cfg(feature = "devtools")]
fn next_scroll_acc_for_children(element: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let mut next = scroll_acc;
    let is_scroll_viewport = element
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
    if is_scroll_viewport
        || matches!(element.style.overflow.x, taffy::style::Overflow::Scroll)
    {
        next[0] += element.scroll_offset[0];
    }
    if is_scroll_viewport
        || matches!(element.style.overflow.y, taffy::style::Overflow::Scroll)
    {
        next[1] += element.scroll_offset[1];
    }
    next
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

/// Split points for Solid/CSS stacking: page paint, then portal overlay paint.
///
/// Global “all rects then all text” would draw page glyphs *over* popover fills
/// (looks like a missing/transparent panel background). Portal content is collected
/// as a contiguous suffix once [`PORTAL_LAYER_ID`] is entered (layer is last on root).
///
/// Within the overlay suffix, each direct child of the portal layer is a **paint band**
/// (underlay / panel). Bands are drawn as `rects → text → next band` so a higher-z
/// panel’s background correctly occludes lower panels’ glyphs.
#[derive(Clone, Debug, Default)]
pub struct OverlayPaintSplit {
    pub instance_start: usize,
    pub textured_start: usize,
    pub text_start: usize,
    pub chrome_start: usize,
    pub paint_op_start: usize,
    /// Starts of each portal-layer child paint band (z / tree order). Empty → treat the
    /// whole overlay suffix as a single band.
    pub bands: Vec<OverlayPaintBand>,
    marked: bool,
}

/// Start indices (into the layout collect buffers) for one portal-layer child.
#[derive(Clone, Copy, Debug, Default)]
pub struct OverlayPaintBand {
    pub instance_start: usize,
    pub textured_start: usize,
    pub text_start: usize,
    pub chrome_start: usize,
    pub paint_op_start: usize,
    /// Max [`crate::style::Style::backdrop_blur`] on this band's root (logical px).
    pub backdrop_blur: f32,
}

/// Resolved `[start, end)` ranges for drawing one overlay band.
#[derive(Clone, Copy, Debug)]
pub struct OverlayBandRange {
    pub instance: (usize, usize),
    pub textured: (usize, usize),
    pub text: (usize, usize),
    pub chrome: (usize, usize),
    pub paint_ops: (usize, usize),
}

impl OverlayPaintSplit {
    /// Expand [`Self::bands`] (or a single synthetic band) into end-exclusive ranges.
    pub fn band_ranges(
        &self,
        instance_len: usize,
        textured_len: usize,
        text_len: usize,
        chrome_len: usize,
        paint_op_len: usize,
    ) -> Vec<OverlayBandRange> {
        let starts: Vec<OverlayPaintBand> = if self.bands.is_empty() {
            if self.instance_start < instance_len
                || self.textured_start < textured_len
                || self.text_start < text_len
                || self.chrome_start < chrome_len
                || self.paint_op_start < paint_op_len
            {
                vec![OverlayPaintBand {
                    instance_start: self.instance_start.min(instance_len),
                    textured_start: self.textured_start.min(textured_len),
                    text_start: self.text_start.min(text_len),
                    chrome_start: self.chrome_start.min(chrome_len),
                    paint_op_start: self.paint_op_start.min(paint_op_len),
                    backdrop_blur: 0.0,
                }]
            } else {
                Vec::new()
            }
        } else {
            self.bands.clone()
        };
        let mut out = Vec::with_capacity(starts.len());
        for (i, b) in starts.iter().enumerate() {
            let next = starts.get(i + 1);
            out.push(OverlayBandRange {
                instance: (
                    b.instance_start.min(instance_len),
                    next.map(|n| n.instance_start.min(instance_len))
                        .unwrap_or(instance_len),
                ),
                textured: (
                    b.textured_start.min(textured_len),
                    next.map(|n| n.textured_start.min(textured_len))
                        .unwrap_or(textured_len),
                ),
                text: (
                    b.text_start.min(text_len),
                    next.map(|n| n.text_start.min(text_len)).unwrap_or(text_len),
                ),
                chrome: (
                    b.chrome_start.min(chrome_len),
                    next.map(|n| n.chrome_start.min(chrome_len))
                        .unwrap_or(chrome_len),
                ),
                paint_ops: (
                    b.paint_op_start.min(paint_op_len),
                    next.map(|n| n.paint_op_start.min(paint_op_len))
                        .unwrap_or(paint_op_len),
                ),
            });
        }
        out
    }
}

/// Tree-order paint: a node's SDF rects, then its `image_texture`, then children.
/// Keeps slider thumbs / SV knobs above track ramps (textured quads used to draw in a
/// later pass that covered every sibling rect).
#[derive(Clone, Copy, Debug)]
pub enum PaintOp {
    Rects { start: u32, count: u32 },
    Textured { start: u32, count: u32 },
}

#[derive(Default)]
struct PaintRecorder {
    ops: Vec<PaintOp>,
    /// Next push must start a new op even if instance indices are contiguous.
    /// Prevents page `Rects` from swallowing the first overlay panel fill (page glyphs
    /// would then paint *on top* of the popover chrome).
    split_before_next: bool,
}

impl PaintRecorder {
    fn split_here(&mut self) {
        self.split_before_next = true;
    }

    fn push_rects(&mut self, start: usize, count: usize) {
        if count == 0 {
            return;
        }
        let start = start as u32;
        let count = count as u32;
        if !self.split_before_next {
            if let Some(PaintOp::Rects { start: s, count: c }) = self.ops.last_mut() {
                if *s + *c == start {
                    *c += count;
                    return;
                }
            }
        }
        self.split_before_next = false;
        self.ops.push(PaintOp::Rects { start, count });
    }

    fn push_textured(&mut self, start: usize, count: usize) {
        if count == 0 {
            return;
        }
        let start = start as u32;
        let count = count as u32;
        if !self.split_before_next {
            if let Some(PaintOp::Textured { start: s, count: c }) = self.ops.last_mut() {
                if *s + *c == start {
                    *c += count;
                    return;
                }
            }
        }
        self.split_before_next = false;
        self.ops.push(PaintOp::Textured { start, count });
    }

    fn push_self(&mut self, i0: usize, i1: usize, t0: usize, t1: usize) {
        self.push_rects(i0, i1.saturating_sub(i0));
        self.push_textured(t0, t1.saturating_sub(t0));
    }

    /// Drop draw ops appended after subtree-capture slice points.
    fn truncate_from(&mut self, inst_len: usize, tex_len: usize) {
        self.ops.retain_mut(|op| match op {
            PaintOp::Rects { start, count } => {
                if *start >= inst_len as u32 {
                    return false;
                }
                let end = *start + *count;
                if end > inst_len as u32 {
                    *count = inst_len as u32 - *start;
                }
                *count > 0
            }
            PaintOp::Textured { start, count } => {
                if *start >= tex_len as u32 {
                    return false;
                }
                let end = *start + *count;
                if end > tex_len as u32 {
                    *count = tex_len as u32 - *start;
                }
                *count > 0
            }
        });
    }
}

/// 一次布局+收集的产出.
pub struct RenderResult {
    pub instances: Vec<InstanceInput>,
    pub textured_draws: Vec<TexturedDraw>,
    /// Document-order rect/texture runs (see [`PaintOp`]).
    pub paint_ops: Vec<PaintOp>,
    pub render_graph: crate::renderer::RenderGraph,
    pub text_nodes: Vec<TextLayoutInfo>,
    /// Input/Label caret + selection chrome in logical px (drawn after glyphon text).
    pub input_chrome_instances: Vec<InstanceInput>,
    /// Index splits: `[0..start)` = page, `[start..)` = portal overlay stack.
    pub overlay_split: OverlayPaintSplit,
    pub scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>, // (rest, bounds, max_scroll, content_size)
    pub hover_cursor: Option<crate::core::tree::CursorValue>,
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
    /// Layout-time [`crate::text::PreparedText`] when the leaf was measured via `prepare`.
    /// Render may use it for wrap hints; measure + primary/secondary shaping share one FontSystem.
    pub prepared: Option<crate::text::PreparedText>,
    /// B17 glyph/cluster motion (`rui_glyph_fx` attr).
    pub glyph_fx: Option<String>,
}

/// 驱动 Taffy 并对 `Element` 树做 layout 与实例收集.
pub struct LayoutTree {
    pub taffy: TaffyTree<()>,
    pub(crate) root_node: Option<NodeId>,
    /// Current layout viewport (window / compute available size) for `vw`/`vh`/`dvh`.
    viewport: [f32; 2],
    text_measurements: HashMap<taffy::NodeId, TextMeasureMeta>,
    /// Per-compute_layout text leaf measure memo (cleared each compute).
    measure_layout_cache: RefCell<HashMap<TextMeasureCacheKey, taffy::geometry::Size<f32>>>,
    /// Track nodes visited in the current build pass for pruning.
    visited_nodes: std::collections::HashSet<NodeId>,
    /// Track all nodes currently in the taffy tree.
    total_nodes: std::collections::HashSet<NodeId>,
    /// Test spy: how many times [`Self::mark_element_layout_dirty`] called `taffy.mark_dirty`.
    #[cfg(test)]
    pub(crate) mark_dirty_calls: u32,
    /// Test spy: last node passed to `compute_layout_with_measure` (app root or subtree parent).
    #[cfg(test)]
    pub(crate) last_compute_root: Option<NodeId>,
    /// Frame-local collect output; `std::mem::take` hands buffers to [`RenderResult`].
    collect_scratch: CollectScratch,
}

/// Layout-side resident structures (Taffy + measure metadata).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutTreeStats {
    pub taffy_nodes: usize,
    pub text_measurements: usize,
    pub has_root: bool,
}

impl LayoutTree {
    /// Wave P P.2.2: count Taffy nodes and text-measure metadata retained between frames.
    pub fn stats(&self) -> LayoutTreeStats {
        LayoutTreeStats {
            taffy_nodes: self.total_nodes.len(),
            text_measurements: self.text_measurements.len(),
            has_root: self.root_node.is_some(),
        }
    }
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
    /// When set, leaf measure uses [`crate::text::prepare_rich`] instead of concat-only prepare.
    spans: Option<Vec<crate::style::TextSpan>>,
    /// `whitespace-nowrap`: ignore definite avail shrink; min/max content = intrinsic.
    nowrap: bool,
    /// `text-overflow: clip|hidden|ellipsis`: honor definite/avail width instead of
    /// growing the leaf to the full line (buttons still use `nowrap` without this).
    clip_to_box: bool,
}

fn element_text_clips_to_box(element: &Element) -> bool {
    match element.text_style.overflow {
        crate::style::TextOverflow::Hidden | crate::style::TextOverflow::Ellipsis => true,
        crate::style::TextOverflow::Clip => {
            element.text_style.white_space == crate::style::WhiteSpace::Nowrap
                && (element.style.overflow.x != taffy::style::Overflow::Visible
                    || element.style.overflow.y != taffy::style::Overflow::Visible)
        }
        crate::style::TextOverflow::Newline => false,
    }
}

fn text_measure_meta(element: &Element, text: String, is_icon: bool) -> TextMeasureMeta {
    TextMeasureMeta {
        size: element.text_style.size,
        text,
        shape_suffix: element.text_shape_suffix.clone(),
        is_icon,
        line_height_px: element.text_style.line_height_px,
        line_height_scale: element.text_style.line_height_scale,
        weight: element.text_style.weight,
        letter_spacing: element.text_style.letter_spacing,
        spans: element.text_spans.clone(),
        nowrap: element.text_style.white_space == crate::style::WhiteSpace::Nowrap,
        clip_to_box: element_text_clips_to_box(element),
    }
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
fn dimension_is_content_sized(d: Dimension) -> bool {
    d.is_auto()
}

fn element_tree_has_deferred_units(el: &Element) -> bool {
    el.style.has_deferred_units() || el.children.iter().any(element_tree_has_deferred_units)
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

    // `whitespace-nowrap` (ShadCN Button): ignore definite shrink — min/max content are the
    // full line, matching CSS inline-flex + nowrap (browser grows the button instead of wrapping).
    // Distinguish MinContent vs MaxContent: both used to fall through to `intrinsic_w`,
    // so soft-wrap labels reported min-content == full line and flex parents could not
    // squeeze them (gallery bidi/RTL overflow past the card).
    let avail_width = if m.nowrap && !m.clip_to_box {
        None
    } else {
        match available_space.width {
            taffy::style::AvailableSpace::Definite(w) => Some(w.max(1.0)),
            taffy::style::AvailableSpace::MinContent => Some(0.0),
            taffy::style::AvailableSpace::MaxContent => None,
        }
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

    let (width, height) = if let Some(spans) = m.spans.as_ref().filter(|s| !s.is_empty()) {
        let prepared = crate::text::prepare_rich(spans, size, m.weight, single_line_height);
        let mut intrinsic_w = crate::text::intrinsic_width(&prepared);
        if m.letter_spacing.abs() > f32::EPSILON {
            let gaps = blob.chars().count().saturating_sub(1) as f32;
            intrinsic_w = (intrinsic_w + gaps * m.letter_spacing).max(0.0);
        }
        let min_content = if m.nowrap {
            intrinsic_w.max(1.0)
        } else {
            prepared.max_segment_width.min(intrinsic_w).max(1.0)
        };
        let width = if m.clip_to_box {
            known_dimensions
                .width
                .or(avail_width)
                .unwrap_or(intrinsic_w)
                .max(1.0)
        } else if m.nowrap {
            known_dimensions
                .width
                .map(|w| w.max(intrinsic_w).max(1.0))
                .unwrap_or(intrinsic_w)
        } else if let Some(w) = known_dimensions.width {
            w.max(1.0)
        } else if let Some(w) = avail_width {
            intrinsic_w.min(w.max(1.0)).max(min_content)
        } else {
            intrinsic_w
        };
        let layout_w = if m.nowrap {
            intrinsic_w.max(1.0)
        } else {
            width.max(1.0)
        };
        let metrics = crate::text::layout(&prepared, layout_w);
        let height = crate::text::bidi_soft_wrap_height(
            blob.as_ref(),
            size,
            m.weight,
            single_line_height,
            layout_w,
            prepared.max_segment_width,
            &metrics,
            m.nowrap,
        );
        (width, height)
    } else {
        let (measured_w, height) = crate::text::measure_leaf(
            blob.as_ref(),
            size,
            m.weight,
            single_line_height,
            known_dimensions.width,
            avail_width,
            m.letter_spacing,
            m.nowrap,
        );
        let width = if m.clip_to_box {
            known_dimensions
                .width
                .or(avail_width)
                .unwrap_or(measured_w)
                .max(1.0)
        } else {
            measured_w
        };
        (width, height)
    };
    // Never raise height from known_dimensions: flex Stretch would inflate text leaves to
    // the ScrollArea/Textarea viewport, so leaf (and soft_h.max(scaled_h)) tracked chrome
    // resize instead of content — gallery "text render height grows with drag".
    // Definite `style.size.height` is applied by Taffy via node_size, not this max().
    let _ = known_dimensions.height;
    taffy::geometry::Size { width, height }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTree {
    /// Shared Taffy measure callback wrapper to avoid repeating closure
    /// monomorphizations at multiple compute callsites.
    #[inline(never)]
    fn compute_layout_cached_measure(
        &mut self,
        node: NodeId,
        available_space: Size<AvailableSpace>,
    ) -> Result<(), taffy::TaffyError> {
        let tm = &self.text_measurements;
        let mcache = &self.measure_layout_cache;
        self.taffy.compute_layout_with_measure(
            node,
            available_space,
            |known_dimensions, available_space, node_id, _context, _style| {
                measure_leaf_cached(mcache, tm, node_id, known_dimensions, available_space)
            },
        )
    }

    pub fn new() -> Self {
        let mut taffy = TaffyTree::new();
        // Taffy defaults to integer pixel rounding. That floors glyph widths
        // (e.g. Ghost 36.415 → 36), and glyphon then mid-word wraps the last letter.
        // Subpixel layout is required for text measure/render to stay consistent.
        taffy.disable_rounding();
        Self {
            taffy,
            root_node: None,
            viewport: [0.0, 0.0],
            text_measurements: HashMap::new(),
            measure_layout_cache: RefCell::new(HashMap::new()),
            visited_nodes: std::collections::HashSet::new(),
            total_nodes: std::collections::HashSet::new(),
            #[cfg(test)]
            mark_dirty_calls: 0,
            #[cfg(test)]
            last_compute_root: None,
            collect_scratch: CollectScratch::default(),
        }
    }

    /// If every dirty node shares the same parent, return that parent for subtree `compute_layout`.
    /// Divergent parents (or empty / root-level dirty) → `None` (caller uses app root).
    ///
    /// Does **not** apply size-overflow promotion — see [`Self::resolve_layout_compute_root`].
    ///
    /// **Not used on the live LayoutChanged path:** computing from a non-root node zeros that
    /// node's `location` (Taffy treats it as the layout root), collapsing absolute positions.
    /// Prefer app-root `compute_layout` after [`Self::mark_dirty_nodes_in_tree`].
    pub fn pick_subtree_compute_root(
        &self,
        dirty: &std::collections::HashSet<NodeId>,
    ) -> Option<NodeId> {
        if dirty.is_empty() {
            return None;
        }
        let app_root = self.root_node?;
        let mut common_parent: Option<NodeId> = None;
        for &n in dirty {
            if n == app_root {
                return None;
            }
            let parent = self.taffy.parent(n)?;
            match common_parent {
                None => common_parent = Some(parent),
                Some(p) if p != parent => return None,
                Some(_) => {}
            }
        }
        let parent = common_parent?;
        if parent == app_root {
            // Parent is the app root: prefer viewport-sized root compute in the caller.
            return None;
        }
        // Need a prior layout size to treat `parent` as a local compute root.
        let Ok(layout) = self.taffy.layout(parent) else {
            return None;
        };
        if layout.size.width <= 0.0 || layout.size.height <= 0.0 {
            return None;
        }
        Some(parent)
    }

    /// True when `node`'s style size on either axis is content-dependent (`Auto`), so a
    /// descendant size change can grow/shrink this node (WP-2.3「上溢」).
    pub fn node_size_depends_on_descendants(&self, node: NodeId) -> bool {
        let Ok(style) = self.taffy.style(node) else {
            return true;
        };
        dimension_is_content_sized(style.size.width)
            || dimension_is_content_sized(style.size.height)
    }

    /// Walk from `candidate` toward the app root while ancestors are content-sized.
    ///
    /// Returns the highest ancestor that still has a usable prior layout size and is **not**
    /// the app root; `None` means promote all the way to the app root (safe default).
    pub fn promote_compute_root_for_size_overflow(&self, candidate: NodeId) -> Option<NodeId> {
        let app_root = self.root_node?;
        if candidate == app_root {
            return None;
        }
        let mut cur = candidate;
        // While the candidate itself is content-sized, a child size change can change its
        // outer size — outer siblings need a higher compute root.
        while self.node_size_depends_on_descendants(cur) {
            let parent = self.taffy.parent(cur)?;
            if parent == app_root {
                return None;
            }
            cur = parent;
        }
        let Ok(layout) = self.taffy.layout(cur) else {
            return None;
        };
        if layout.size.width <= 0.0 || layout.size.height <= 0.0 {
            return None;
        }
        Some(cur)
    }

    /// Pick a common parent for `dirty`, then promote past content-sized ancestors
    /// ([`Self::promote_compute_root_for_size_overflow`]).
    ///
    /// Live [`crate::app::state`] LayoutChanged still passes `subtree_root: None` (app root)
    /// because Taffy zeros non-root `location`. This API documents / tests the overflow rule
    /// and is the gate for any future safe subtree compute.
    pub fn resolve_layout_compute_root(
        &self,
        dirty: &std::collections::HashSet<NodeId>,
    ) -> Option<NodeId> {
        let picked = self.pick_subtree_compute_root(dirty)?;
        self.promote_compute_root_for_size_overflow(picked)
    }

    #[inline]
    fn unit_ctx_for(&self, element: &Element) -> UnitCtx {
        UnitCtx::new(self.viewport[0], self.viewport[1], element.text_style.size)
    }

    fn unit_ctx_with_cb(&self, element: &Element, cb_w: Option<f32>, cb_h: Option<f32>) -> UnitCtx {
        self.unit_ctx_for(element).with_containing_block(cb_w, cb_h)
    }

    /// Containing block = parent **content box** (layout.size − padding − border), or viewport for root.
    fn unit_ctx_from_parent_layout(&self, element: &Element) -> UnitCtx {
        let ctx = self.unit_ctx_for(element);
        let Some(id) = element.layout_node else {
            return ctx.with_containing_block(Some(self.viewport[0]), Some(self.viewport[1]));
        };
        match self.taffy.parent(id) {
            None => ctx.with_containing_block(Some(self.viewport[0]), Some(self.viewport[1])),
            Some(pid) => match self.taffy.layout(pid) {
                Ok(layout) => {
                    let cw = (layout.size.width
                        - layout.padding.left
                        - layout.padding.right
                        - layout.border.left
                        - layout.border.right)
                        .max(0.0);
                    let ch = (layout.size.height
                        - layout.padding.top
                        - layout.padding.bottom
                        - layout.border.top
                        - layout.border.bottom)
                        .max(0.0);
                    ctx.with_containing_block(Some(cw), Some(ch))
                }
                Err(_) => ctx,
            },
        }
    }

    pub fn update_taffy_node_style(&mut self, element: &Element) {
        if let Some(node_id) = element.layout_node {
            let style = element
                .style
                .to_taffy_style_with(&self.unit_ctx_from_parent_layout(element));
            self.taffy.set_style(node_id, style).unwrap();
        }
    }

    /// Push `element` style + text measure into Taffy and mark that node dirty.
    ///
    /// Taffy's `set_style` already calls `mark_dirty` (node + ancestors). We still call
    /// `mark_dirty` explicitly so LayoutChanged paths that only refresh text measure are covered,
    /// and so tests can count dirty marks.
    ///
    /// Note: live LayoutChanged always `compute_layout` from the app root after mark_dirty so
    /// parent `location` stays valid. Clean sibling subtrees keep their Taffy cache.
    pub fn mark_element_layout_dirty(&mut self, element: &Element) {
        let Some(node_id) = element.layout_node else {
            return;
        };
        self.update_taffy_node_style(element);
        self.sync_text_measure(element);
        let _ = self.taffy.mark_dirty(node_id);
        #[cfg(test)]
        {
            self.mark_dirty_calls = self.mark_dirty_calls.saturating_add(1);
        }
    }

    /// Refresh the leaf text measure entry used by `compute_layout_with_measure`.
    pub fn sync_text_measure(&mut self, element: &Element) {
        let Some(node_id) = element.layout_node else {
            return;
        };
        if let Some(text) = &element.text {
            if element.children.is_empty() {
                let is_icon = element.text_style.font_family.as_deref() == Some("lucide");
                self.text_measurements
                    .insert(node_id, text_measure_meta(element, text.clone(), is_icon));
                return;
            }
        }
        self.text_measurements.remove(&node_id);
    }

    /// Whether taffy's cache for `node` is empty (needs recompute).
    pub fn is_node_dirty(&self, node: NodeId) -> bool {
        self.taffy.dirty(node).unwrap_or(false)
    }

    /// Walk `root` and call [`Self::mark_element_layout_dirty`] for each node in `dirty`.
    pub fn mark_dirty_nodes_in_tree(
        &mut self,
        root: &Element,
        dirty: &std::collections::HashSet<NodeId>,
    ) {
        if dirty.is_empty() {
            return;
        }
        fn walk(tree: &mut LayoutTree, el: &Element, dirty: &std::collections::HashSet<NodeId>) {
            if let Some(n) = el.layout_node {
                if dirty.contains(&n) {
                    tree.mark_element_layout_dirty(el);
                }
            }
            for child in &el.children {
                walk(tree, child, dirty);
            }
        }
        walk(self, root, dirty);
    }

    /// Mark every text leaf dirty so `compute_layout` re-runs measure (FIT_CACHE / settle).
    ///
    /// Needed when available space is unchanged but soft-wrap height must grow after
    /// `glyphon_fit` (paint spill → `note_fit_needs_relayout`, or post-resize settle).
    /// Without this, Taffy keeps the previous one-line cache and labels stay clipped / stacked.
    pub fn mark_all_text_leaves_dirty(&mut self, root: &Element) {
        fn walk(tree: &mut LayoutTree, el: &Element) {
            if el.text.is_some() && el.children.is_empty() && el.layout_node.is_some() {
                tree.mark_element_layout_dirty(el);
            }
            for child in &el.children {
                walk(tree, child);
            }
        }
        walk(self, root);
    }

    pub fn build_and_layout(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
    ) -> RenderResult {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!(
            "layout_tree.build_and_layout",
            width,
            height,
            children = root.children.len()
        )
        .entered();

        // Invariant: never layout/paint unhoisted `rui_portal` nodes (Solid keep-alive + body mount).
        let _ = hoist_portal_subtrees(root);

        let t_rebuild_begin = Instant::now();
        self.viewport = [width.max(0.0), height.max(0.0)];
        root.style.size.width = Dimension::length(width);
        root.style.size.height = Dimension::length(height);
        cascade_inheritable_text_style(root, None);

        self.text_measurements.clear();
        self.visited_nodes.clear();
        // DO NOT clear taffy anymore; we rely on reconciliation and selective removal.
        let root_node = self.build_taffy_node_mut(root, Some(width), Some(height));
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
        {
            #[cfg(feature = "tracing")]
            let _taffy_span = tracing::trace_span!("layout_tree.taffy_compute").entered();
            self.compute_layout_cached_measure(root_node, available_space)
                .unwrap();
        }
        let relayout_ms = self.relayout_deferred_units_if_needed(root, root_node, available_space);
        let taffy_compute_ms = t_compute_begin.elapsed().as_secs_f32() * 1000.0 + relayout_ms;

        self.finish_collect(
            root,
            root_node,
            mouse_pos,
            false,
            false,
            &mut None,
            rebuild_ms,
            0.0,
            taffy_compute_ms,
        )
    }

    /// When `scroll_only` is true, skip [`TaffyTree::compute_layout`]: `Element.scroll_offset` does
    /// not participate in flex/grid sizing (only affects collect-time drawing). Use after
    /// `update_scrollbar_elements` synced only `scroll:` offsets and no scrollbar styles changed
    /// ?avoids O(nested) full layout each frame when many `ScrollArea`s are nested.
    ///
    /// When `scrollbar_overlay_collect` is true, track/thumb draw rects use live scroll signals
    /// (same contract as a `ViewportsOnly` prepass without Full). Use whenever that prepass did not
    /// push geometry into Taffy, even if chrome sync forces `!scroll_only` layout for other nodes.
    ///
    /// LayoutChanged path: callers should [`Self::mark_element_layout_dirty`] (or
    /// [`Self::mark_dirty_nodes_in_tree`]) first. Prefer `subtree_root: None` (app root): a non-root
    /// compute node gets `location = (0,0)` in Taffy and collapses absolute positions.
    pub fn build_and_layout_fast(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        scroll_only: bool,
        scrollbar_overlay_collect: bool,
    ) -> RenderResult {
        self.build_and_layout_fast_ex(
            root,
            width,
            height,
            mouse_pos,
            scroll_only,
            scrollbar_overlay_collect,
            None,
        )
    }

    /// Like [`Self::build_and_layout_fast`], with an optional subtree compute root.
    pub fn build_and_layout_fast_ex(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        scroll_only: bool,
        scrollbar_overlay_collect: bool,
        subtree_root: Option<NodeId>,
    ) -> RenderResult {
        // Solid invariant: portals must be on their mount before any layout/collect.
        let portal_dirty = hoist_portal_subtrees(root);
        if portal_dirty > 0 || self.root_node.is_none() {
            return self.build_and_layout(root, width, height, mouse_pos);
        }

        let root_node = self
            .root_node
            .expect("fast path requires existing root_node");

        // Must match `build_and_layout`: the root's Element + Taffy style must follow the current
        // viewport. Otherwise `compute_layout` still sees stale definite sizes from the last full
        // build (e.g. after maximize / restore while status is `Clean`) and the UI stays a small
        // island in the corner while the swapchain is full-window.
        let t_setup_begin = Instant::now();
        let next_viewport = [width.max(0.0), height.max(0.0)];
        // `taffy.set_style` marks the node dirty. Doing that every scroll-only frame
        // invalidates the whole tree for no geometric change. Inherit cascade is likewise
        // unchanged when only `scroll_offset` moved.
        let viewport_changed = self.viewport != next_viewport;
        if !scroll_only || viewport_changed {
            self.viewport = next_viewport;
            root.style.size.width = Dimension::length(width);
            root.style.size.height = Dimension::length(height);
            cascade_inheritable_text_style(root, None);
            self.update_taffy_node_style(root);
        }
        let setup_ms = t_setup_begin.elapsed().as_secs_f32() * 1000.0;

        let taffy_compute_ms = if !scroll_only {
            let (compute_node, available_space) = if let Some(sub) = subtree_root {
                if let Ok(layout) = self.taffy.layout(sub) {
                    (
                        sub,
                        Size {
                            width: AvailableSpace::Definite(layout.size.width),
                            height: AvailableSpace::Definite(layout.size.height),
                        },
                    )
                } else {
                    (
                        root_node,
                        Size {
                            width: AvailableSpace::Definite(width),
                            height: AvailableSpace::Definite(height),
                        },
                    )
                }
            } else {
                (
                    root_node,
                    Size {
                        width: AvailableSpace::Definite(width),
                        height: AvailableSpace::Definite(height),
                    },
                )
            };

            let t_compute_begin = Instant::now();
            self.measure_layout_cache.borrow_mut().clear();
            #[cfg(test)]
            {
                self.last_compute_root = Some(compute_node);
            }
            // Note: We assume the caller has updated any necessary styles (like scrollbar thumbs)
            // using update_taffy_node_style before calling this.
            if let Err(err) = self.compute_layout_cached_measure(compute_node, available_space) {
                log::error!("taffy compute_layout_with_measure failed: {err:?}");
            }
            let relayout_ms =
                self.relayout_deferred_units_if_needed(root, compute_node, available_space);
            t_compute_begin.elapsed().as_secs_f32() * 1000.0 + relayout_ms
        } else {
            0.0
        };

        let mut scroll_signal_cache: HashMap<String, ScrollAreaSignalsCached> = HashMap::new();
        let mut scroll_cache_slot: Option<&mut HashMap<String, ScrollAreaSignalsCached>> =
            if scroll_only || scrollbar_overlay_collect {
                Some(&mut scroll_signal_cache)
            } else {
                None
            };

        self.finish_collect(
            root,
            root_node,
            mouse_pos,
            scroll_only,
            scrollbar_overlay_collect,
            &mut scroll_cache_slot,
            0.0,
            setup_ms,
            taffy_compute_ms,
        )
    }

    fn finish_collect(
        &mut self,
        root: &mut Element,
        root_node: NodeId,
        mouse_pos: [f32; 2],
        scroll_only: bool,
        scrollbar_overlay_collect: bool,
        scroll_cache_slot: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
        rebuild_ms: f32,
        setup_ms: f32,
        taffy_compute_ms: f32,
    ) -> RenderResult {
        let mut scratch = std::mem::take(&mut self.collect_scratch);
        scratch.clear_for_collect();
        let mut hover_cursor = None;
        let mut overlay_split = OverlayPaintSplit::default();
        let mut paint_rec = PaintRecorder::default();

        let t_collect_begin = Instant::now();
        #[cfg(feature = "devtools")]
        {
            crate::app::devtools::begin_highlight_collect();
            crate::app::devtools::set_highlight_collect_root(root);
        }
        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            1.0,
            &mut scratch.instances,
            &mut scratch.textured_draws,
            &mut scratch.text_nodes,
            &mut scratch.input_chrome_instances,
            &mut scratch.scroll_metrics,
            &mut hover_cursor,
            scroll_only,
            scrollbar_overlay_collect,
            None,
            scroll_cache_slot,
            &mut overlay_split,
            &mut paint_rec,
        );
        #[cfg(feature = "devtools")]
        {
            crate::app::devtools::finalize_highlight_after_collect(root);
            crate::app::devtools::clear_highlight_collect_root();
        }
        let collect_ms = t_collect_begin.elapsed().as_secs_f32() * 1000.0;
        if !overlay_split.marked {
            overlay_split.instance_start = scratch.instances.len();
            overlay_split.textured_start = scratch.textured_draws.len();
            overlay_split.text_start = scratch.text_nodes.len();
            overlay_split.chrome_start = scratch.input_chrome_instances.len();
            overlay_split.paint_op_start = paint_rec.ops.len();
        }

        scratch.tex_keys.clear();
        scratch
            .tex_keys
            .extend(scratch.textured_draws.iter().map(|d| d.texture_key.clone()));
        let mut render_graph =
            crate::renderer::RenderGraph::standard_composite(scratch.tex_keys.clone());
        crate::renderer::enrich_render_graph(&mut render_graph);
        let result = RenderResult {
            instances: std::mem::take(&mut scratch.instances),
            textured_draws: std::mem::take(&mut scratch.textured_draws),
            paint_ops: paint_rec.ops,
            render_graph,
            text_nodes: std::mem::take(&mut scratch.text_nodes),
            input_chrome_instances: std::mem::take(&mut scratch.input_chrome_instances),
            overlay_split,
            scroll_metrics: std::mem::take(&mut scratch.scroll_metrics),
            hover_cursor,
            prof: LayoutProfileMs {
                rebuild_ms,
                setup_ms,
                taffy_compute_ms,
                collect_ms,
            },
        };
        self.collect_scratch = scratch;
        result
    }

    /// Return collect output vecs so the next frame can reuse capacity (WP-P collect 增量).
    pub(crate) fn recycle_collect_buffers(
        &mut self,
        mut instances: Vec<InstanceInput>,
        mut textured_draws: Vec<TexturedDraw>,
        mut input_chrome: Vec<InstanceInput>,
    ) {
        instances.clear();
        textured_draws.clear();
        input_chrome.clear();
        if instances.capacity() > self.collect_scratch.instances.capacity() {
            self.collect_scratch.instances = instances;
        }
        if textured_draws.capacity() > self.collect_scratch.textured_draws.capacity() {
            self.collect_scratch.textured_draws = textured_draws;
        }
        if input_chrome.capacity() > self.collect_scratch.input_chrome_instances.capacity() {
            self.collect_scratch.input_chrome_instances = input_chrome;
        }
    }

    /// Re-bake `vw`/`vh`/`em`/`calc` after viewport or containing-block changes.
    /// Resize uses the fast path (root style only); without this, baked Lengths stay stale.
    fn relayout_deferred_units_if_needed(
        &mut self,
        root: &Element,
        compute_node: NodeId,
        available_space: Size<AvailableSpace>,
    ) -> f32 {
        if !element_tree_has_deferred_units(root) {
            return 0.0;
        }
        let mut dirty = false;
        self.relayout_deferred_units_walk(root, &mut dirty);
        if !dirty {
            return 0.0;
        }
        let t0 = Instant::now();
        self.measure_layout_cache.borrow_mut().clear();
        if let Err(err) = self.compute_layout_cached_measure(compute_node, available_space) {
            log::error!("taffy deferred-unit relayout failed: {err:?}");
        }
        t0.elapsed().as_secs_f32() * 1000.0
    }

    fn relayout_deferred_units_walk(&mut self, element: &Element, dirty: &mut bool) {
        if element.style.has_deferred_units() {
            if let Some(node_id) = element.layout_node {
                let ctx = self.unit_ctx_from_parent_layout(element);
                let new_style = element.style.to_taffy_style_with(&ctx);
                let changed = self.taffy.style(node_id).ok().map_or(true, |old| {
                    old.size != new_style.size
                        || old.min_size != new_style.min_size
                        || old.max_size != new_style.max_size
                        || old.flex_basis != new_style.flex_basis
                        || old.padding != new_style.padding
                        || old.margin != new_style.margin
                        || old.gap != new_style.gap
                        || old.inset != new_style.inset
                });
                if changed {
                    let _ = self.taffy.set_style(node_id, new_style);
                    *dirty = true;
                }
            }
        }
        for child in &element.children {
            self.relayout_deferred_units_walk(child, dirty);
        }
    }

    fn build_taffy_node_mut(
        &mut self,
        element: &mut Element,
        cb_w: Option<f32>,
        cb_h: Option<f32>,
    ) -> NodeId {
        let ctx = self.unit_ctx_with_cb(element, cb_w, cb_h);
        let (child_cb_w, child_cb_h) = element.style.estimate_content_box(&ctx);

        let mut child_nodes = Vec::new();
        for child in &mut element.children {
            child_nodes.push(self.build_taffy_node_mut(child, child_cb_w, child_cb_h));
        }

        let style = element.style.to_taffy_style_with(&ctx);
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
                let text_clone = text.clone();
                let is_icon = element.text_style.font_family.as_deref() == Some("lucide");
                self.text_measurements
                    .insert(node_id, text_measure_meta(element, text_clone, is_icon));
            }
        }

        node_id
    }

    /// Refresh abs + paint-visual for a collect-skipped subtree from live Taffy geometry.
    ///
    /// Band-cull / clip-cull used to stamp descendants from retained `layout_bounds`. After a
    /// narrow wrap reflow those coords are the *previous* flex line; combining them with the
    /// current `scroll_acc` mixed old and new layout in the Inspector highlight.
    #[cfg(feature = "devtools")]
    fn sync_devtools_subtree_from_taffy(
        &self,
        element: &mut Element,
        node: NodeId,
        abs: [f32; 2],
        size: [f32; 2],
        scroll_acc: [f32; 2],
    ) {
        crate::app::devtools::note_element_paint_visual_from_abs(element, abs, size, scroll_acc);
        self.sync_devtools_descendants_from_taffy(element, node, abs, size, scroll_acc);
    }

    #[cfg(feature = "devtools")]
    fn sync_devtools_descendants_from_taffy(
        &self,
        element: &mut Element,
        node: NodeId,
        abs: [f32; 2],
        size: [f32; 2],
        scroll_acc: [f32; 2],
    ) {
        let Ok(child_nodes) = self.taffy.children(node) else {
            return;
        };
        let child_scroll = next_scroll_acc_for_children(element, scroll_acc);
        let pass = [
            abs[0] + element.style.translate[0] + size[0] * element.style.translate_pct[0],
            abs[1] + element.style.translate[1] + size[1] * element.style.translate_pct[1],
        ];
        let n = element.children.len().min(child_nodes.len());
        for i in 0..n {
            let Ok(cl) = self.taffy.layout(child_nodes[i]) else {
                continue;
            };
            let child_abs = [pass[0] + cl.location.x, pass[1] + cl.location.y];
            let child_size = [cl.size.width, cl.size.height];
            self.sync_devtools_subtree_from_taffy(
                &mut element.children[i],
                child_nodes[i],
                child_abs,
                child_size,
                child_scroll,
            );
        }
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
        input_chrome_instances: &mut Vec<InstanceInput>,
        scroll_metrics: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
        hover_cursor: &mut Option<crate::core::tree::CursorValue>,
        _scroll_only: bool,
        scrollbar_overlay_collect: bool,
        thumb_geometry_override: Option<[f32; 4]>,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
        overlay_split: &mut OverlayPaintSplit,
        paint_rec: &mut PaintRecorder,
    ) -> [f32; 2] {
        // Solid `<Portal>` → document.body: content is outside cropping / scrolled layouts.
        // Reset clip + scroll accumulation when entering the root portal layer so hoisted
        // overlays never inherit ScrollArea (or other overflow) clips from page ancestors.
        // `position: fixed` / `rui_escape_overflow_clip` / already-hoisted portals: same
        // (Command overlay left edge was flush-cut by the gallery sidebar ScrollArea).
        let is_body_portal_layer = element.template_id.as_deref() == Some(PORTAL_LAYER_ID);
        let escape_overflow_clip = is_body_portal_layer
            || element.style.position_fixed
            || element.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1")
            || element
                .attrs
                .get(ATTR_ESCAPE_OVERFLOW_CLIP)
                .map(|s| s.as_str())
                == Some("1");
        let (parent_clip, scroll_acc) = if escape_overflow_clip {
            (None, [0.0_f32, 0.0_f32])
        } else {
            (parent_clip, scroll_acc)
        };

        if is_body_portal_layer && !overlay_split.marked {
            overlay_split.instance_start = instances.len();
            overlay_split.textured_start = textured_draws.len();
            overlay_split.text_start = text_nodes.len();
            overlay_split.chrome_start = input_chrome_instances.len();
            overlay_split.paint_op_start = paint_rec.ops.len();
            overlay_split.marked = true;
            paint_rec.split_here();
        }
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

        #[cfg(feature = "devtools")]
        let devtools_highlight_rect = crate::app::devtools::stamped_highlight_abs_rect(element);
        #[cfg(not(feature = "devtools"))]
        let devtools_highlight_rect: Option<[f32; 4]> = None;

        let (abs_x, abs_y, lay_w, lay_h) = if let Some(r) = thumb_geometry_override {
            (r[0], r[1], r[2], r[3])
        } else if let Some(b) = devtools_highlight_rect {
            (b[0], b[1], b[2], b[3])
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

        #[cfg(feature = "devtools")]
        if element.template_id.as_deref()
            == Some(crate::app::devtools::HIGHLIGHT_SLOT_ID)
        {
            let _ = crate::app::devtools::refresh_highlight_slot(element);
        }

        let opacity = (ancestor_opacity * element.style.opacity).clamp(0.0, 1.0);
        // Stamped Inspector highlight rects are already in paint/visual space — never
        // subtract ancestor scroll again (that shifts the overlay upward).
        let scroll_for_render = if element.style.position_fixed || devtools_highlight_rect.is_some()
        {
            [0.0_f32, 0.0_f32]
        } else {
            scroll_acc
        };
        let mut render_x = abs_x - scroll_for_render[0] + element.style.translate[0] + lay_w * element.style.translate_pct[0];
        let mut render_y = abs_y - scroll_for_render[1] + element.style.translate[1] + lay_h * element.style.translate_pct[1];

        // CSS `position: sticky` — clamp render position within the nearest scroll ancestor clip.
        if element.style.position_sticky {
            let orig_rx = render_x;
            let orig_ry = render_y;
            if let Some(clip) = parent_clip {
                let clip_top = clip[1];
                let clip_left = clip[0];
                let clip_bottom = clip[1] + clip[3];
                let clip_right = clip[0] + clip[2];

                let inset_top = lpa_to_px(element.style.inset.top);
                let inset_bottom = lpa_to_px(element.style.inset.bottom);
                let inset_left = lpa_to_px(element.style.inset.left);
                let inset_right = lpa_to_px(element.style.inset.right);

                if let Some(t) = inset_top {
                    render_y = render_y.max(clip_top + t);
                }
                if let Some(b) = inset_bottom {
                    render_y = render_y.min(clip_bottom - lay_h - b);
                }
                if let Some(l) = inset_left {
                    render_x = render_x.max(clip_left + l);
                }
                if let Some(r) = inset_right {
                    render_x = render_x.min(clip_right - lay_w - r);
                }
            }
            element.sticky_visual_offset = [render_x - orig_rx, render_y - orig_ry];
        } else {
            element.sticky_visual_offset = [0.0, 0.0];
        }
        let scale_x = if element.style.scale[0].is_finite() && element.style.scale[0] > 0.0 {
            element.style.scale[0]
        } else {
            1.0
        };
        let scale_y = if element.style.scale[1].is_finite() && element.style.scale[1] > 0.0 {
            element.style.scale[1]
        } else {
            1.0
        };
        let (vis_w, vis_h, vis_x, vis_y) =
            if (scale_x - 1.0).abs() > f32::EPSILON || (scale_y - 1.0).abs() > f32::EPSILON {
                let sw = lay_w * scale_x;
                let sh = lay_h * scale_y;
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
        #[cfg(feature = "devtools")]
        crate::app::devtools::note_element_paint_visual(element, render_rect);
        let resolved_translate = [
            element.style.translate[0] + lay_w * element.style.translate_pct[0],
            element.style.translate[1] + lay_h * element.style.translate_pct[1],
        ];
        let transform = crate::style::compute_combined_transform(
            &resolved_translate,
            element.style.rotate_deg,
            &element.style.scale,
            element.style.transform.as_ref(),
            &element.style.transform_origin,
            vis_w,
            vis_h,
        );
        let self_inst0 = instances.len();
        let self_tex0 = textured_draws.len();

        // Clipping & Visibility
        let mut clip = parent_clip;
        if element.style.overflow.x != taffy::style::Overflow::Visible
            || element.style.overflow.y != taffy::style::Overflow::Visible
        {
            clip = Some(intersect_rect(clip, render_rect));
        }
        let visible_rect = intersect_rect(clip, render_rect);

        // Clip-cull: skip paint + descendants when this box (plus shadow/transform outset)
        // is fully outside the ancestor clip. Parent scroll metrics still use this node's
        // Taffy size so content height does not collapse to the viewport.
        if parent_clip.is_some() && !escape_overflow_clip {
            let pad = paint_outset_pad(&element.style, lay_w, lay_h);
            let vis = intersect_rect(parent_clip, inflate_rect(render_rect, pad, pad));
            if vis[2] <= 0.0 || vis[3] <= 0.0 {
                if let Some(fid) = element.focus_id.as_deref() {
                    if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                        scroll_metrics.push((
                            rest.to_string(),
                            [abs_x, abs_y, layout.size.width, layout.size.height],
                            [0.0, 0.0],
                            [layout.size.width.max(0.0), layout.size.height.max(0.0)],
                        ));
                    }
                }
                let own = [abs_x + lay_w, abs_y + lay_h];
                // scroll_only: keep last full-collect overflow extent so band-culled cards
                // with overflow:visible children do not shrink content_h (thumb jitter).
                // Full collect after layout/resize: never max with a stale taller extent —
                // wrap reflow shortens cards; keeping the old max left blank scroll range
                // at the bottom (gallery text page: scroll then widen window).
                let extent = if _scroll_only && element.collect_extent_valid {
                    [
                        element.collect_extent[0].max(own[0]),
                        element.collect_extent[1].max(own[1]),
                    ]
                } else {
                    own
                };
                element.collect_extent = extent;
                element.collect_extent_valid = true;
                // Parent already has fresh layout_bounds + paint_visual; refresh descendants
                // from Taffy so narrow wrap + later scroll/highlight do not mix old abs.
                #[cfg(feature = "devtools")]
                self.sync_devtools_descendants_from_taffy(
                    element,
                    node,
                    [abs_x, abs_y],
                    [lay_w, lay_h],
                    scroll_for_render,
                );
                return extent;
            }
        }

        // Hit Testing (Update cursor if mouse is over visible part)
        if mouse_pos[0] >= visible_rect[0]
            && mouse_pos[0] <= visible_rect[0] + visible_rect[2]
            && mouse_pos[1] >= visible_rect[1]
            && mouse_pos[1] <= visible_rect[1] + visible_rect[3]
        {
            if let Some(c) = element.cursor.clone() {
                *hover_cursor = Some(c);
            }
        }

        // Collect main instance (solid / gradient strips + shadows).
        // Geometry stays full `render_rect`. CSS overflow clips *descendants* only — the
        // element's own background / border / outset shadow must use `parent_clip`, otherwise
        // a scrolled panel's soft rounded shadow is sliced into a sharp axis-aligned box.
        // Baking `visible_rect` into pos/size made scrolled borders stick to the viewport edge.
        let border_widths = element.style.resolved_border_widths();
        let border_w = border_widths.into_iter().fold(0.0_f32, f32::max);
        let use_border_strips = border_w > 0.0 && !element.style.border_is_uniform();
        let sdf_border_w = if use_border_strips { 0.0 } else { border_w };
        let sdf_border_color = if use_border_strips {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            mul_color_alpha(element.style.border_color, opacity)
        };
        let sdf_border_extra = {
            let bs = if use_border_strips { 0.0 } else { element.style.border_style.to_shader_f32() };
            InstanceInput::pack_border_extra(
                bs,
                element.style.corner_shape.to_shader_f32(),
                element.style.corner_shape.shader_param(),
            )
        };
        let has_bg = element.style.bg_color[3] > 0.0 || element.style.bg_gradient.is_some();
        let has_border = border_w > 0.0;
        let has_shadow = element.style.shadow_color[3] > 0.0;
        let paint_visible = intersect_rect(parent_clip, render_rect);
        if (has_bg || has_border || has_shadow) && paint_visible[2] > 0.0 && paint_visible[3] > 0.0
        {
            let resolved_radii =
                resolve_corner_radii(element.style.radii, render_rect[2], render_rect[3]);
            let need_fill_instance = has_bg || has_shadow || (has_border && !use_border_strips);
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
                        let sw = render_rect[2] / STRIPS as f32;
                        (
                            [render_rect[0] + i as f32 * sw, render_rect[1]],
                            [sw.max(0.5), render_rect[3]],
                        )
                    } else {
                        let sh = render_rect[3] / STRIPS as f32;
                        (
                            [render_rect[0], render_rect[1] + i as f32 * sh],
                            [render_rect[2], sh.max(0.5)],
                        )
                    };
                    instances.push(
                        InstanceInput {
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
                            transform,
                            clip_pos: [0.0, 0.0],
                            clip_size: InstanceInput::NO_CLIP_SIZE,
                            border_extra: sdf_border_extra,
                        }
                        .with_clip(parent_clip),
                    );
                }
                if has_shadow || (has_border && !use_border_strips) {
                    instances.push(
                        InstanceInput {
                            rect_pos: [render_rect[0], render_rect[1]],
                            rect_size: [render_rect[2], render_rect[3]],
                            color: [0.0, 0.0, 0.0, 0.0],
                            border_color: sdf_border_color,
                            radii: resolved_radii,
                            border_width: sdf_border_w,
                            shadow_color: mul_color_alpha(element.style.shadow_color, opacity),
                            shadow_offset: element.style.shadow_offset,
                            shadow_blur: element.style.shadow_blur,
                            shadow_spread: element.style.shadow_spread,
                            shadow_inset: if element.style.shadow_inset { 1.0 } else { 0.0 },
                            transform,
                            clip_pos: [0.0, 0.0],
                            clip_size: InstanceInput::NO_CLIP_SIZE,
                            border_extra: sdf_border_extra,
                        }
                        .with_clip(parent_clip),
                    );
                }
            } else if need_fill_instance {
                instances.push(
                    InstanceInput {
                        rect_pos: [render_rect[0], render_rect[1]],
                        rect_size: [render_rect[2], render_rect[3]],
                        color: mul_color_alpha(element.style.bg_color, opacity),
                        border_color: sdf_border_color,
                        radii: resolved_radii,
                        border_width: sdf_border_w,
                        shadow_color: mul_color_alpha(element.style.shadow_color, opacity),
                        shadow_offset: element.style.shadow_offset,
                        shadow_blur: element.style.shadow_blur,
                        shadow_spread: element.style.shadow_spread,
                        shadow_inset: if element.style.shadow_inset { 1.0 } else { 0.0 },
                        transform,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: sdf_border_extra,
                    }
                    .with_clip(parent_clip),
                );
            }
            if use_border_strips {
                push_per_side_border_strips(
                    instances,
                    render_rect,
                    border_widths,
                    element.style.resolved_border_colors(),
                    element.style.resolved_border_styles(),
                    opacity,
                    transform,
                    parent_clip,
                );
            }
            // border-image overrides normal border rendering when present
            if element.style.border_image_source.is_some() {
                push_border_image_draws(
                    textured_draws,
                    render_rect,
                    border_widths,
                    &element.style,
                    opacity,
                    parent_clip,
                );
            }
            for sh in &element.style.extra_shadows {
                if sh.color[3] <= 0.0 {
                    continue;
                }
                instances.push(
                    InstanceInput {
                        rect_pos: [render_rect[0], render_rect[1]],
                        rect_size: [render_rect[2], render_rect[3]],
                        color: [0.0, 0.0, 0.0, 0.0],
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        radii: resolved_radii,
                        border_width: 0.0,
                        shadow_color: mul_color_alpha(sh.color, opacity),
                        shadow_offset: sh.offset,
                        shadow_blur: sh.blur,
                        shadow_spread: sh.spread,
                        shadow_inset: if sh.inset { 1.0 } else { 0.0 },
                        transform,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: 0.0,
                    }
                    .with_clip(parent_clip),
                );
            }
        }

        // WP-3.2 / 3.11 / 3.12: CPU coverage raster → texture; tessellate fallback.
        // Canvas paint 与 path/svg 互斥：先 canvas 贴图，再 path scene，最后 tessellate。
        // Chart 多层 scene / 单层 `d` 已在此出图时，勿再画 `image_texture`（否则低分拉伸 + 默认黑描边叠一层）。
        let mut path_tex_drawn = false;
        if visible_rect[2] > 0.0 && visible_rect[3] > 0.0 {
            if let Some((tex_key, dw, dh)) =
                crate::primitives::canvas::canvas_raster_from_element_attrs(
                    &element.attrs,
                    vis_w,
                    vis_h,
                )
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], clip, tex_key, opacity)
                {
                    textured_draws.push(draw);
                    path_tex_drawn = true;
                }
            } else if let Some((tex_key, dw, dh)) =
                crate::primitives::scene3d::scene3d_tex_from_element_attrs(
                    &element.attrs,
                    vis_w,
                    vis_h,
                )
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], clip, tex_key, opacity)
                {
                    textured_draws.push(draw);
                    path_tex_drawn = true;
                }
            } else if crate::renderer::gpu2d_enabled() {
                if let Some(quads) = crate::primitives::path::path_quads_from_element_attrs(
                    &element.attrs,
                    [vis_x, vis_y],
                    [vis_w, vis_h],
                ) {
                    for q in quads {
                        if q.color[3] <= 0.0 || q.size.0 <= 0.0 || q.size.1 <= 0.0 {
                            continue;
                        }
                        instances.push(
                            InstanceInput {
                                rect_pos: [q.pos.0, q.pos.1],
                                rect_size: [q.size.0, q.size.1],
                                color: mul_color_alpha(q.color, opacity),
                                border_color: [0.0, 0.0, 0.0, 0.0],
                                radii: q.radii,
                                border_width: 0.0,
                                shadow_color: [0.0, 0.0, 0.0, 0.0],
                                shadow_offset: [0.0, 0.0],
                                shadow_blur: 0.0,
                                shadow_spread: 0.0,
                                shadow_inset: 0.0,
                                transform: crate::style::transform::mat2d_rotate(q.rotate_deg.to_radians()),
                                clip_pos: [0.0, 0.0],
                                clip_size: InstanceInput::NO_CLIP_SIZE,
                                border_extra: 0.0,
                            }
                            .with_clip(clip),
                        );
                    }
                    path_tex_drawn = true;
                } else if let Some((tex_key, dw, dh)) =
                    crate::primitives::path::path_raster_from_element_attrs(
                        &element.attrs,
                        vis_w,
                        vis_h,
                    )
                {
                    if let Some(draw) =
                        TexturedDraw::clipped([vis_x, vis_y, dw, dh], clip, tex_key, opacity)
                    {
                        textured_draws.push(draw);
                        path_tex_drawn = true;
                    }
                }
            } else if let Some((tex_key, dw, dh)) =
                crate::primitives::path::path_raster_from_element_attrs(
                    &element.attrs,
                    vis_w,
                    vis_h,
                )
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], clip, tex_key, opacity)
                {
                    textured_draws.push(draw);
                    path_tex_drawn = true;
                }
            } else if let Some(quads) = crate::primitives::path::path_quads_from_element_attrs(
                &element.attrs,
                [vis_x, vis_y],
                [vis_w, vis_h],
            ) {
                for q in quads {
                    if q.color[3] <= 0.0 || q.size.0 <= 0.0 || q.size.1 <= 0.0 {
                        continue;
                    }
                    instances.push(
                        InstanceInput {
                            rect_pos: [q.pos.0, q.pos.1],
                            rect_size: [q.size.0, q.size.1],
                            color: mul_color_alpha(q.color, opacity),
                            border_color: [0.0, 0.0, 0.0, 0.0],
                            radii: q.radii,
                            border_width: 0.0,
                            shadow_color: [0.0, 0.0, 0.0, 0.0],
                            shadow_offset: [0.0, 0.0],
                            shadow_blur: 0.0,
                            shadow_spread: 0.0,
                            shadow_inset: 0.0,
                            transform: crate::style::transform::mat2d_rotate(q.rotate_deg.to_radians()),
                            clip_pos: [0.0, 0.0],
                            clip_size: InstanceInput::NO_CLIP_SIZE,
                            border_extra: 0.0,
                        }
                        .with_clip(parent_clip),
                    );
                }
            }
        }
        if path_tex_drawn {
            if let Some(last) = textured_draws.last_mut() {
                if let Some(out_key) = crate::renderer::queue_effect_from_attrs(
                    &element.attrs,
                    [last.rect[0], last.rect[1], last.rect[2], last.rect[3]],
                    Some(last.texture_key.as_str()),
                ) {
                    last.texture_key = out_key;
                }
            }
        }

        // Tabs line variant: paint-only underline / side bar (no absolute child node).
        if let Some(ind) = element
            .attrs
            .get("rui_tabs_line_indicator")
            .map(|s| s.as_str())
        {
            let line_w = 2.0_f32;
            let color = crate::style::parse_color("foreground");
            let (pos, size) = match ind {
                "h" if render_rect[2] > 0.0 && render_rect[3] > 0.0 => (
                    [render_rect[0], render_rect[1] + render_rect[3] - line_w],
                    [render_rect[2], line_w],
                ),
                "v" if render_rect[2] > 0.0 && render_rect[3] > 0.0 => (
                    [render_rect[0] + render_rect[2] - line_w, render_rect[1]],
                    [line_w, render_rect[3]],
                ),
                _ => ([0.0, 0.0], [0.0, 0.0]),
            };
            if size[0] > 0.0 && size[1] > 0.0 {
                let bar_rect = [pos[0], pos[1], size[0], size[1]];
                let bar_visible = intersect_rect(parent_clip, bar_rect);
                if bar_visible[2] > 0.0 && bar_visible[3] > 0.0 {
                    instances.push(
                        InstanceInput {
                            rect_pos: pos,
                            rect_size: size,
                            color: mul_color_alpha(color, opacity),
                            border_color: [0.0, 0.0, 0.0, 0.0],
                            radii: [0.0, 0.0, 0.0, 0.0],
                            border_width: 0.0,
                            shadow_color: [0.0, 0.0, 0.0, 0.0],
                            shadow_offset: [0.0, 0.0],
                            shadow_blur: 0.0,
                            shadow_spread: 0.0,
                            shadow_inset: 0.0,
                            transform,
                            clip_pos: [0.0, 0.0],
                            clip_size: InstanceInput::NO_CLIP_SIZE,
                            border_extra: 0.0,
                        }
                        .with_clip(parent_clip),
                    );
                }
            }
        }

        // Collect outline as an overlay (drawn outside; does not affect layout).
        let outline_w = if element.style.outline_style.is_invisible() {
            0.0
        } else {
            element.style.outline_width
        };
        if outline_w > 0.0 && lay_w > 0.0 && lay_h > 0.0 {
            // outline-offset: gap between the border edge and the outline.
            let oo = element.style.outline_offset;
            let outline_rect = [
                render_x - outline_w - oo,
                render_y - outline_w - oo,
                lay_w + (outline_w + oo) * 2.0,
                lay_h + (outline_w + oo) * 2.0,
            ];
            let outline_visible = intersect_rect(parent_clip, outline_rect);
            if outline_visible[2] > 0.0 && outline_visible[3] > 0.0 {
                let mut outline_radii = element.style.radii;
                for r in &mut outline_radii {
                    if *r >= 0.0 {
                        *r += outline_w + oo;
                    }
                }
                let resolved_radii =
                    resolve_corner_radii(outline_radii, outline_rect[2], outline_rect[3]);
                let outline_extra = InstanceInput::pack_border_extra(
                    element.style.outline_style.to_shader_f32(),
                    element.style.corner_shape.to_shader_f32(),
                    element.style.corner_shape.shader_param(),
                );
                instances.push(
                    InstanceInput {
                        rect_pos: [outline_rect[0], outline_rect[1]],
                        rect_size: [outline_rect[2], outline_rect[3]],
                        color: [0.0, 0.0, 0.0, 0.0],
                        border_color: mul_color_alpha(element.style.outline_color, opacity),
                        radii: resolved_radii,
                        border_width: outline_w,
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_spread: 0.0,
                        shadow_inset: 0.0,
                        transform,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: outline_extra,
                    }
                    .with_clip(parent_clip),
                );
            }
        }

        if !path_tex_drawn {
            if let Some(tex_key) = element.image_texture.as_ref() {
                if !tex_key.is_empty() {
                    if let Some(draw) = TexturedDraw::clipped(
                        [vis_x, vis_y, vis_w, vis_h],
                        parent_clip,
                        tex_key.clone(),
                        opacity,
                    ) {
                        textured_draws.push(draw);
                    }
                }
            }
        }

        if let Some(text) = &element.text {
            let mut text_style = element.text_style.clone();
            if opacity < 1.0 - f32::EPSILON {
                let a = ((f32::from(text_style.color[3]) * opacity).round() as u16).min(255) as u8;
                text_style.color[3] = a;
            }
            let is_icon = text_style.font_family.as_deref() == Some("lucide");
            let prepared = if is_icon {
                None
            } else {
                let line_height = if let Some(lh) = text_style.line_height_px {
                    lh
                } else if let Some(scale) = text_style.line_height_scale {
                    text_style.size * scale
                } else {
                    text_style.size * crate::style::tailwind_default_leading_scale(text_style.size)
                };
                if let Some(spans) = element.text_spans.as_ref().filter(|s| !s.is_empty()) {
                    Some(crate::text::prepare_rich(
                        spans,
                        text_style.size,
                        text_style.weight,
                        line_height,
                    ))
                } else {
                    let blob: std::borrow::Cow<'_, str> = match element.text_shape_suffix.as_deref()
                    {
                        Some(suf) if !suf.is_empty() => {
                            std::borrow::Cow::Owned(format!("{}{}", text, suf))
                        }
                        _ => std::borrow::Cow::Borrowed(text.as_str()),
                    };
                    Some(crate::text::prepare(
                        blob.as_ref(),
                        text_style.size,
                        text_style.weight,
                        line_height,
                    ))
                }
            };
            text_nodes.push(TextLayoutInfo {
                text: text.clone(),
                shape_suffix: element.text_shape_suffix.clone(),
                spans: element.text_spans.clone(),
                template_id: element.template_id.clone(),
                x: render_x - text_style.scroll_x.max(0.0),
                y: render_y,
                width: lay_w,
                height: lay_h,
                // Clip/hidden/ellipsis nowrap must scissor to the field. Growing the leaf to
                // intrinsic (button nowrap) used to make `clip == leaf`, so glyphs and
                // selection painted past the input border.
                clip_bounds: if text_style.overflow.allows_caret_scroll() {
                    hidden_input_text_clip(parent_clip, render_rect)
                } else if element_text_clips_to_box(element) {
                    Some(intersect_rect(parent_clip, render_rect))
                } else {
                    // Ancestor overflow only for wrapping text: leaf scissor hides spilled
                    // bidi/emoji glyphs when measure under-counts soft-wrap lines.
                    clip
                },
                style: text_style.clone(),
                prepared,
                glyph_fx: element.attrs.get(crate::renderer::ATTR_GLYPH_FX).cloned(),
            });
            let chrome_clip = if text_style.overflow.allows_caret_scroll() {
                hidden_input_text_clip(parent_clip, render_rect)
            } else if element_text_clips_to_box(element) {
                Some(intersect_rect(parent_clip, render_rect))
            } else {
                clip
            };
            push_input_chrome_instances(
                instances,
                input_chrome_instances,
                text,
                &text_style,
                render_x - text_style.scroll_x.max(0.0),
                render_y,
                lay_w,
                lay_h,
                chrome_clip,
                opacity,
            );
        }

        paint_rec.push_self(self_inst0, instances.len(), self_tex0, textured_draws.len());

        // Recurse children
        let mut next_scroll_acc = scroll_acc;
        // `scroll:` viewports always contribute offset (even if overflow was cleared to Visible).
        // `overflow: hidden`/`clip` clip but do not scroll — match hit-test `next_hit_scroll_acc`.
        let is_scroll_viewport = element
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
        let scroll_x = is_scroll_viewport
            || matches!(element.style.overflow.x, taffy::style::Overflow::Scroll);
        let scroll_y = is_scroll_viewport
            || matches!(element.style.overflow.y, taffy::style::Overflow::Scroll);
        if scroll_x {
            next_scroll_acc[0] += element.scroll_offset[0];
        }
        if scroll_y {
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
                let own = [abs_x + lay_w, abs_y + lay_h];
                element.collect_extent = own;
                element.collect_extent_valid = true;
                return own;
            }
        };
        let any_z = element.children.iter().any(|c| c.style.z_index != 0);
        let mut max_right = abs_x + lay_w;
        let mut max_bottom = abs_y + lay_h;

        let fx_capture_children = element
            .attrs
            .get(crate::renderer::ATTR_EFFECT_CAPTURE_CHILDREN)
            .map(|s| s.as_str())
            == Some("1");
        let cap_inst0 = if fx_capture_children {
            instances.len()
        } else {
            0
        };
        let cap_tex0 = if fx_capture_children {
            textured_draws.len()
        } else {
            0
        };

        let push_portal_band = |overlay_split: &mut OverlayPaintSplit,
                                instances: &Vec<InstanceInput>,
                                textured_draws: &Vec<TexturedDraw>,
                                text_nodes: &Vec<TextLayoutInfo>,
                                input_chrome_instances: &Vec<InstanceInput>,
                                paint_ops_len: usize,
                                band_root: &Element| {
            if is_body_portal_layer {
                overlay_split.bands.push(OverlayPaintBand {
                    instance_start: instances.len(),
                    textured_start: textured_draws.len(),
                    text_start: text_nodes.len(),
                    chrome_start: input_chrome_instances.len(),
                    paint_op_start: paint_ops_len,
                    backdrop_blur: band_root.style.backdrop_blur.max(0.0),
                });
            }
        };

        if !any_z {
            let n = element.children.len();
            let mut i = 0;
            while i < n {
                if i >= child_nodes.len() {
                    log::error!(
                        "layout/taffy child desync: element has {} children, taffy node {:?} has {} — skip (portal hoist must full-rebuild)",
                        element.children.len(),
                        node,
                        child_nodes.len()
                    );
                    break;
                }
                let child_pass_offset = [
                    abs_x + element.style.translate[0] + lay_w * element.style.translate_pct[0],
                    abs_y + element.style.translate[1] + lay_h * element.style.translate_pct[1],
                ];
                if _scroll_only {
                    if let Some(clip_rect) = clip {
                        if !element.children[i].style.position_fixed
                            && element.children[i].style.position != Position::Absolute
                        {
                            if let Ok(cl) = self.taffy.layout(child_nodes[i]) {
                                let child_abs = [
                                    child_pass_offset[0] + cl.location.x,
                                    child_pass_offset[1] + cl.location.y,
                                ];
                                let child_size = [cl.size.width, cl.size.height];
                                match scroll_sibling_band_action(
                                    child_abs,
                                    child_size,
                                    next_scroll_acc,
                                    clip_rect,
                                    element.style.flex_direction,
                                    element.style.flex_wrap,
                                ) {
                                    ScrollSiblingBandAction::SkipBefore => {
                                        #[cfg(feature = "devtools")]
                                        {
                                            self.sync_devtools_subtree_from_taffy(
                                                &mut element.children[i],
                                                child_nodes[i],
                                                child_abs,
                                                child_size,
                                                next_scroll_acc,
                                            );
                                        }
                                        let extent = skipped_sibling_extent(
                                            &element.children[i],
                                            child_abs,
                                            child_size,
                                        );
                                        max_right = max_right.max(extent[0]);
                                        max_bottom = max_bottom.max(extent[1]);
                                        i += 1;
                                        continue;
                                    }
                                    ScrollSiblingBandAction::StopAfter => {
                                        for j in i..n.min(child_nodes.len()) {
                                            if let Ok(jl) = self.taffy.layout(child_nodes[j]) {
                                                let j_abs = [
                                                    child_pass_offset[0] + jl.location.x,
                                                    child_pass_offset[1] + jl.location.y,
                                                ];
                                                let j_size = [jl.size.width, jl.size.height];
                                                #[cfg(feature = "devtools")]
                                                {
                                                    self.sync_devtools_subtree_from_taffy(
                                                        &mut element.children[j],
                                                        child_nodes[j],
                                                        j_abs,
                                                        j_size,
                                                        next_scroll_acc,
                                                    );
                                                }
                                                let extent = skipped_sibling_extent(
                                                    &element.children[j],
                                                    j_abs,
                                                    j_size,
                                                );
                                                max_right = max_right.max(extent[0]);
                                                max_bottom = max_bottom.max(extent[1]);
                                            }
                                        }
                                        break;
                                    }
                                    ScrollSiblingBandAction::Visit => {}
                                }
                            }
                        }
                    }
                }
                let child = &mut element.children[i];
                paint_rec.split_here();
                push_portal_band(
                    overlay_split,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    paint_rec.ops.len(),
                    child,
                );
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
                    child_pass_offset,
                    next_scroll_acc,
                    clip,
                    pass_to_children,
                    mouse_pos,
                    opacity,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    scroll_metrics,
                    hover_cursor,
                    _scroll_only,
                    scrollbar_overlay_collect,
                    thumb_ov,
                    scroll_signals_cache,
                    overlay_split,
                    paint_rec,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
                i += 1;
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
                if i >= child_nodes.len() {
                    log::error!(
                        "layout/taffy child desync (z-ordered): element has {} children, taffy node {:?} has {}",
                        element.children.len(),
                        node,
                        child_nodes.len()
                    );
                    continue;
                }
                paint_rec.split_here();
                push_portal_band(
                    overlay_split,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    paint_rec.ops.len(),
                    &element.children[i],
                );
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
                    [
                        abs_x + element.style.translate[0] + lay_w * element.style.translate_pct[0],
                        abs_y + element.style.translate[1] + lay_h * element.style.translate_pct[1],
                    ],
                    next_scroll_acc,
                    clip,
                    pass_to_children,
                    mouse_pos,
                    opacity,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    scroll_metrics,
                    hover_cursor,
                    _scroll_only,
                    scrollbar_overlay_collect,
                    thumb_ov,
                    scroll_signals_cache,
                    overlay_split,
                    paint_rec,
                );
                max_right = max_right.max(cr);
                max_bottom = max_bottom.max(cb);
            }
        }

        if fx_capture_children
            && visible_rect[2] > 0.0
            && visible_rect[3] > 0.0
            && (instances.len() > cap_inst0 || textured_draws.len() > cap_tex0)
        {
            let child_insts = instances[cap_inst0..].to_vec();
            let child_tex = textured_draws[cap_tex0..].to_vec();
            instances.truncate(cap_inst0);
            textured_draws.truncate(cap_tex0);
            paint_rec.truncate_from(cap_inst0, cap_tex0);
            if let Some(out_key) = crate::renderer::queue_subtree_wrap(
                &element.attrs,
                visible_rect,
                child_insts,
                child_tex,
            ) {
                if let Some(draw) = TexturedDraw::clipped(visible_rect, clip, out_key, opacity) {
                    textured_draws.push(draw);
                }
            }
        }

        // If this is a ScrollArea, record its metrics (content size vs viewport size)
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                let content_w = (max_right - abs_x).max(layout.size.width);
                let mut content_h = (max_bottom - abs_y).max(layout.size.height);
                // Textarea: floor content_h by explicit Length children + padding. A flex-sized
                // leaf can make max_bottom≈viewport (max_scroll≈padding) while style.height
                // still holds the real field-sizing-content height.
                if rest.ends_with("__ta_scroll") {
                    content_h = crate::utils::text_field::textarea_scroll_content_h(
                        element,
                        abs_y,
                        layout.size.height,
                    );
                }
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

        element.collect_extent = [extent_r, extent_b];
        element.collect_extent_valid = true;
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
    let inset_len_px = |v: &taffy::style::LengthPercentageAuto| {
        crate::layout::lpa_length_px(*v)
            .filter(|px| px.is_finite())
            .unwrap_or(0.0)
            .max(0.0)
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
        "x" => crate::layout::dim_length_px(track_el.style.max_size.height)
            .filter(|px| px.is_finite() && *px > 1.0)
            .unwrap_or(SCROLLBAR_DEFAULT_CROSS_PX),
        _ => crate::layout::dim_length_px(track_el.style.max_size.width)
            .filter(|px| px.is_finite() && *px > 1.0)
            .unwrap_or(SCROLLBAR_DEFAULT_CROSS_PX),
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

    // Overlay path bypasses Taffy for the thumb and must match the track **content** box
    // (border-box origin + layout border; track has no padding so the 1px border is the
    // only inset — padding+border double-counted and sat the thumb 1px down-right).
    let content_inset = scrollbar_track_content_inset(track_el);
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
            [
                base_abs_x + content_inset + ml,
                base_abs_y + content_inset,
                tw,
                thumb_cross,
            ]
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
            [
                base_abs_x + content_inset,
                base_abs_y + content_inset + mt,
                thumb_cross,
                th,
            ]
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

fn inflate_rect(rect: [f32; 4], dx: f32, dy: f32) -> [f32; 4] {
    [
        rect[0] - dx,
        rect[1] - dy,
        (rect[2] + dx * 2.0).max(0.0),
        (rect[3] + dy * 2.0).max(0.0),
    ]
}

/// Outset past `layout_bounds` that can still paint into the clip (shadow / outline / transform).
fn paint_outset_pad(style: &crate::style::Style, lay_w: f32, lay_h: f32) -> f32 {
    let outset = |blur: f32, spread: f32, ox: f32, oy: f32, inset: bool| {
        if inset {
            0.0
        } else {
            blur.max(0.0) + spread.abs() + ox.abs().max(oy.abs())
        }
    };
    let mut pad = 8.0_f32;
    pad = pad.max(outset(
        style.shadow_blur,
        style.shadow_spread,
        style.shadow_offset[0],
        style.shadow_offset[1],
        style.shadow_inset,
    ));
    for sh in &style.extra_shadows {
        pad = pad.max(outset(
            sh.blur,
            sh.spread,
            sh.offset[0],
            sh.offset[1],
            sh.inset,
        ));
    }
    pad = pad.max(style.outline_width.max(0.0));
    let sx = if style.scale[0].is_finite() {
        style.scale[0].abs()
    } else {
        1.0
    };
    let sy = if style.scale[1].is_finite() {
        style.scale[1].abs()
    } else {
        1.0
    };
    if (sx - 1.0).abs() > 0.01 || (sy - 1.0).abs() > 0.01 || style.rotate_deg.abs() > 0.5 {
        pad += 0.5 * (lay_w * sx.max(1.0)).hypot(lay_h * sy.max(1.0));
    }
    pad += style.translate[0].abs().max(style.translate[1].abs());
    pad
}

/// Native `<input>`: clip at the padding box. The text leaf is the content box —
/// inflate horizontally so the caret and CJK bearings at either end stay visible,
/// then cap to the ancestor overflow (padded chrome).
fn hidden_input_text_clip(parent_clip: Option<[f32; 4]>, leaf: [f32; 4]) -> Option<[f32; 4]> {
    const INK: f32 = 2.0;
    Some(intersect_rect(parent_clip, inflate_rect(leaf, INK, 0.0)))
}

/// Extract a definite px value from `LengthPercentageAuto`, returning `None` for `Auto`.
#[inline]
fn lpa_to_px(v: taffy::style::LengthPercentageAuto) -> Option<f32> {
    use crate::layout::dimension::{lpa_kind, DimKind};
    match lpa_kind(v) {
        DimKind::Length(px) => Some(px),
        DimKind::Percent(_) | DimKind::Auto => None,
    }
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

fn prefix_width_logical(
    text: &str,
    byte_ix: usize,
    size: f32,
    weight: u16,
    line_height: f32,
) -> f32 {
    let ix = text.floor_char_boundary(byte_ix.min(text.len()));
    if ix == 0 {
        return 0.0;
    }
    // Prefer glyphon advance (matches paint + hit-test). Segment `prepare` under-counts
    // vs shaped ink and left soft-wrap carets sitting on the last glyph.
    crate::text::measure_prefix_advance(&text[..ix], size, weight, line_height.max(1.0))
}

/// Hard-`\n` lines as `(byte_start, line_body)` — trailing empty line after final `\n` included.
fn hard_line_slices(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push((start, &text[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &text[start..]));
    out
}

use crate::utils::text_field::caret_line_and_local;

fn push_chrome_rect(
    out: &mut Vec<InstanceInput>,
    clip: Option<[f32; 4]>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
) {
    // Keep full caret/selection geometry; GPU clip via with_clip. Baking intersect into
    // rect_size made a short scroll/viewport clip shrink the caret into a flat stub that
    // matched the squashed glyph band.
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let visible = intersect_rect(clip, [x, y, w, h]);
    if visible[2] <= 0.0 || visible[3] <= 0.0 {
        return;
    }
    out.push(
        InstanceInput {
            rect_pos: [x, y],
            rect_size: [w, h],
            color,
            border_color: [0.0, 0.0, 0.0, 0.0],
            radii: [0.0, 0.0, 0.0, 0.0],
            border_width: 0.0,
            shadow_color: [0.0, 0.0, 0.0, 0.0],
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            shadow_inset: 0.0,
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 0.0,
        }
        .with_clip(clip),
    );
}

fn push_input_chrome_instances(
    selection_out: &mut Vec<InstanceInput>,
    caret_out: &mut Vec<InstanceInput>,
    text: &str,
    style: &TextStyle,
    x: f32,
    y: f32,
    box_w: f32,
    box_h: f32,
    clip: Option<[f32; 4]>,
    opacity: f32,
) {
    let Some(caret) = style.input_caret.as_ref() else {
        return;
    };
    let line_height = if let Some(lh) = style.line_height_px {
        lh
    } else if let Some(scale) = style.line_height_scale {
        style.size * scale
    } else {
        style.size * crate::style::tailwind_default_leading_scale(style.size)
    };
    let soft_wrap = style.white_space == crate::style::WhiteSpace::Normal
        && !style.overflow.allows_caret_scroll();
    // Soft-wrap: visual rows from measure layout (matches glyphon soft paint).
    // Nowrap / single-line Hidden: hard-`\n` rows only.
    let soft_ranges: Vec<(usize, usize)> = if soft_wrap {
        crate::text::soft_visual_line_byte_ranges(
            text,
            style.size,
            style.weight,
            line_height.max(1.0),
            box_w.max(1.0),
        )
    } else {
        Vec::new()
    };
    let hard_lines = hard_line_slices(text);
    let line_count = if soft_wrap {
        soft_ranges.len().max(1)
    } else {
        hard_lines.len().max(1)
    };
    // Soft-wrap of one hard line still stacks visual rows — never center in the tall box.
    let multi_line_layout = line_count > 1;
    let caret_h = crate::utils::text_field::text_edit_caret_height_px(line_height, style.size);
    let sel_h = if multi_line_layout {
        line_height.max(1.0)
    } else {
        caret_h.min(line_height).max(1.0)
    };
    let line_origin_y = |line_i: usize| -> f32 {
        if multi_line_layout {
            y + line_i as f32 * line_height
        } else {
            y + (box_h - line_height).max(0.0) * 0.5
        }
    };
    let line_top_y = |line_i: usize| -> f32 { line_origin_y(line_i) };
    let caret_y = |line_i: usize| -> f32 { line_origin_y(line_i) + (line_height - caret_h) * 0.5 };

    let line_body_at = |line_i: usize| -> (usize, &str) {
        if soft_wrap {
            soft_ranges
                .get(line_i)
                .map(|&(s, e)| (s, &text[s..e]))
                .unwrap_or((0, ""))
        } else {
            hard_lines
                .get(line_i)
                .copied()
                .unwrap_or((0, ""))
        }
    };
    let line_and_local = |byte: usize| -> (usize, usize) {
        if soft_wrap {
            let i = crate::text::caret_visual_line_index(&soft_ranges, byte);
            let (s, _) = soft_ranges.get(i).copied().unwrap_or((0, 0));
            (i, byte.saturating_sub(s))
        } else {
            caret_line_and_local(text, byte)
        }
    };

    let range_opt: Option<(usize, usize)> = if let Some((sa, sb)) = caret.selection {
        let mut s = sa.min(sb).min(text.len());
        let mut e = sa.max(sb).min(text.len());
        s = text.floor_char_boundary(s);
        e = text.floor_char_boundary(e);
        if s < e {
            Some((s, e))
        } else {
            None
        }
    } else if let Some((a, b)) = caret.overwrite_highlight {
        if !crate::renderer::caret_blink_visible() {
            None
        } else {
            let mut s = a.min(b).min(text.len());
            let mut e = a.max(b).min(text.len());
            s = text.floor_char_boundary(s);
            e = text.floor_char_boundary(e);
            if s < e {
                Some((s, e))
            } else {
                None
            }
        }
    } else {
        None
    };

    if let Some((s, e)) = range_opt {
        let bg = caret.selection_bg;
        let color = [
            bg[0] as f32 / 255.0,
            bg[1] as f32 / 255.0,
            bg[2] as f32 / 255.0,
            (bg[3] as f32 / 255.0) * opacity,
        ];
        let (start_line, start_local) = line_and_local(s);
        let (end_line, end_local) = line_and_local(e);
        for line_i in start_line..=end_line.min(line_count.saturating_sub(1)) {
            let (_line_start, line_body) = line_body_at(line_i);
            let from = if line_i == start_line { start_local } else { 0 };
            let to = if line_i == end_line {
                end_local.min(line_body.len())
            } else {
                line_body.len()
            };
            if from >= to && !(from == 0 && to == 0 && line_i < end_line) {
                // Empty middle/end lines still get a thin caret-width highlight when selected through.
                if from == to && line_body.is_empty() && start_line != end_line {
                    push_chrome_rect(
                        selection_out,
                        clip,
                        x,
                        line_top_y(line_i),
                        4.0_f32.min(box_w),
                        sel_h,
                        color,
                    );
                }
                continue;
            }
            let prefix_w =
                prefix_width_logical(line_body, from, style.size, style.weight, line_height);
            let end_w = prefix_width_logical(line_body, to, style.size, style.weight, line_height);
            let pan = style.overflow.allows_caret_scroll();
            let sel_w = if pan {
                (end_w - prefix_w).max(0.0)
            } else {
                (end_w - prefix_w).min(box_w - prefix_w).max(0.0)
            };
            if sel_w > 0.0 {
                push_chrome_rect(
                    selection_out,
                    clip,
                    if pan {
                        x + prefix_w
                    } else {
                        x + prefix_w.min(box_w)
                    },
                    line_top_y(line_i),
                    sel_w,
                    sel_h,
                    color,
                );
            }
        }
        return;
    }

    if !crate::renderer::caret_blink_visible() {
        return;
    }
    let mut ix = caret.caret_byte_index.min(text.len());
    ix = text.floor_char_boundary(ix);
    let (line_i, local) = line_and_local(ix);
    let (_line_start, line_body) = line_body_at(line_i);
    let prefix_w = prefix_width_logical(line_body, local, style.size, style.weight, line_height);
    let caret_w = 2.0_f32;
    let mut caret_x = x + prefix_w;
    if soft_wrap {
        // Soft-wrap: do not clamp to `box_w - caret_w` — that parked the hairline on the
        // last glyph when a visual line filled the leaf (gallery soft-wrap caret bug).
        caret_x = caret_x.max(x);
    } else if !style.overflow.allows_caret_scroll() {
        caret_x = caret_x.min(x + box_w - caret_w).max(x);
    }
    let c = style.color;
    let color = [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        (c[3] as f32 / 255.0) * opacity,
    ];
    push_chrome_rect(
        caret_out,
        clip,
        caret_x,
        caret_y(line_i.min(line_count.saturating_sub(1))),
        caret_w,
        caret_h,
        color,
    );
}

#[cfg(test)]
mod prepared_text_attach_tests {
    use super::*;
    use crate::style::{InputCaretDraw, TextOverflow, TextStyle};

    #[test]
    fn prepared_text_on_text_layout_info_roundtrips() {
        let prepared = crate::text::prepare("hello prepared attach", 14.0, 400, 21.0);
        let info = TextLayoutInfo {
            text: "hello prepared attach".into(),
            shape_suffix: None,
            spans: None,
            template_id: Some("t-prepared".into()),
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 42.0,
            clip_bounds: None,
            style: TextStyle::default(),
            prepared: Some(prepared.clone()),
            glyph_fx: None,
        };
        let attached = info.prepared.as_ref().expect("prepared attached");
        assert_eq!(attached.segment_count(), prepared.segment_count());
        assert!((attached.max_segment_width - prepared.max_segment_width).abs() < f32::EPSILON);
        let metrics = crate::text::layout(attached, info.width);
        assert!(metrics.line_count >= 1);
        let lines = crate::text::layout_lines(attached, info.width);
        assert_eq!(lines.len() as u32, metrics.line_count);
    }

    #[test]
    fn hard_line_slices_keeps_trailing_empty_line() {
        let lines = hard_line_slices("a\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], (2, ""));
    }

    #[test]
    fn latin_cjk_prefix_width_sums_segments_not_max() {
        let mut fs = crate::renderer::font_loader::create_font_system();
        crate::renderer::font_loader::apply_default_families(fs.db_mut());
        crate::text::install_shared_font_system(&mut fs);

        let size = 14.0;
        let lh = 21.0;
        let weight = 400u16;
        let mixed = "1是";
        let after_cjk = mixed.len();
        let w_mixed = prefix_width_logical(mixed, after_cjk, size, weight, lh);
        let w_latin = prefix_width_logical("1", 1, size, weight, lh);
        let w_cjk = prefix_width_logical("是", "是".len(), size, weight, lh);
        // Bug used max(segment) ≈ w_cjk, placing caret mid-"是".
        assert!(
            w_mixed > w_cjk + w_latin * 0.5,
            "prefix of \"1是\" must be ~sum, got mixed={w_mixed} latin={w_latin} cjk={w_cjk}"
        );
        assert!(
            (w_mixed - (w_latin + w_cjk)).abs() < 1.0,
            "expected ~latin+cjk, got mixed={w_mixed} vs {w_latin}+{w_cjk}"
        );
    }

    /// Multi-line selection highlight is glyph-tight per line (no EOL/box fill).
    #[test]
    fn multiline_selection_rects_are_glyph_tight_per_line() {
        let text = "啊\n啊\n啊";
        let lh = 21.0_f32;
        let box_w = 280.0_f32;
        let glyph_w = prefix_width_logical("啊", "啊".len(), 14.0, 400, lh);
        let mut style = TextStyle::default();
        style.size = 14.0;
        style.weight = 400;
        style.line_height_px = Some(lh);
        style.color = [0, 0, 0, 255];
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: Some((0, text.len())),
            selection_bg: [80, 120, 200, 90],
            overwrite_highlight: None,
        });

        let mut selection = Vec::new();
        let mut chrome = Vec::new();
        push_input_chrome_instances(
            &mut selection,
            &mut chrome,
            text,
            &style,
            10.0,
            20.0,
            box_w,
            lh * 3.0 + 4.0,
            None,
            1.0,
        );
        assert_eq!(selection.len(), 3);
        for r in &selection {
            assert!(
                (r.rect_size[0] - glyph_w).abs() < 2.0,
                "selected line must stay glyph-tight (no EOL fill), got w={} glyph≈{glyph_w}",
                r.rect_size[0]
            );
        }
    }

    #[test]
    fn soft_wrap_caret_sits_on_visual_line_not_box_center() {
        // One hard line soft-wrapped into several visual rows: caret must track the
        // visual line (not vertically center in the tall leaf — gallery symptom).
        let text =
            "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px";
        let lh = 21.0_f32;
        let box_w = 160.0_f32;
        let ranges = crate::text::soft_visual_line_byte_ranges(text, 14.0, 400, lh, box_w);
        assert!(
            ranges.len() >= 2,
            "fixture must soft-wrap under {box_w}px, got {} lines",
            ranges.len()
        );
        let caret_byte = text.len();
        let visual_i = crate::text::caret_visual_line_index(&ranges, caret_byte);
        let expected_y = visual_i as f32 * lh; // multi-line origin (no half-box center)

        let mut style = TextStyle::default();
        style.size = 14.0;
        style.weight = 400;
        style.line_height_px = Some(lh);
        style.white_space = crate::style::WhiteSpace::Normal;
        style.color = [0, 0, 0, 255];
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: caret_byte,
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
        });

        let tall_h = lh * ranges.len() as f32 + 8.0;
        let mut chrome = Vec::new();
        let mut unused = Vec::new();
        let mut saw = false;
        for _ in 0..8 {
            chrome.clear();
            push_input_chrome_instances(
                &mut unused,
                &mut chrome,
                text,
                &style,
                0.0,
                0.0,
                box_w,
                tall_h,
                None,
                1.0,
            );
            if !chrome.is_empty() {
                saw = true;
                break;
            }
        }
        assert!(saw, "caret must draw within blink window");
        let caret_y = chrome[0].rect_pos[1];
        let caret_x = chrome[0].rect_pos[0];
        let centered = (tall_h - lh).max(0.0) * 0.5;
        assert!(
            (caret_y - expected_y).abs() < lh * 0.6,
            "soft-wrap caret y={caret_y} should sit near visual line {visual_i} (≈{expected_y}), \
             not box-center {centered}"
        );
        assert!(
            (caret_y - centered).abs() > lh * 0.4 || visual_i == ranges.len() / 2,
            "must not use single-line box centering for soft-wrapped tall leaf \
             (y={caret_y} center={centered})"
        );
        let (s, e) = ranges[visual_i];
        let body = &text[s..e];
        let full_w = prefix_width_logical(body, body.len(), 14.0, 400, lh);
        assert!(
            (caret_x - full_w).abs() < 3.0,
            "caret x={caret_x} should sit at end of visual line (prefix≈{full_w}), not mid-glyph"
        );
    }

    #[test]
    fn multiline_caret_slightly_taller_than_line_and_centered() {
        let text = "asd\nadj";
        let lh = 21.0_f32;
        let mut style = TextStyle::default();
        style.size = 14.0;
        style.weight = 400;
        style.line_height_px = Some(lh);
        style.color = [0, 0, 0, 255];
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
        });

        let mut chrome = Vec::new();
        let mut unused = Vec::new();
        let mut saw = false;
        for _ in 0..6 {
            chrome.clear();
            push_input_chrome_instances(
                &mut unused,
                &mut chrome,
                text,
                &style,
                0.0,
                0.0,
                200.0,
                lh * 2.0,
                None,
                1.0,
            );
            if !chrome.is_empty() {
                saw = true;
                break;
            }
        }
        assert!(saw, "multiline caret chrome missing during blink ON");
        let caret_h = chrome[0].rect_size[1];
        let caret_y = chrome[0].rect_pos[1];
        assert!(
            caret_h > lh && caret_h <= lh + 2.0,
            "caret should be slightly taller than line height (h={caret_h}, lh={lh})"
        );
        let expected_y = lh + (lh - caret_h) * 0.5;
        assert!(
            (caret_y - expected_y).abs() < 0.5,
            "caret should be vertically centered on line 1 (y={caret_y}, expected={expected_y})"
        );
    }

    #[test]
    fn hidden_overflow_caret_follows_scroll_not_box_clamp() {
        let text = "abcdefghijklmnop";
        let lh = 21.0_f32;
        let prefix = prefix_width_logical(text, text.len(), 14.0, 400, lh);
        let box_w = 40.0_f32;
        assert!(
            prefix > box_w + 8.0,
            "fixture must overflow the box (prefix={prefix})"
        );
        let scroll_x = (prefix - box_w + 2.0).max(0.0);
        let mut style = TextStyle::default();
        style.size = 14.0;
        style.weight = 400;
        style.line_height_px = Some(lh);
        style.color = [0, 0, 0, 255];
        style.overflow = TextOverflow::Hidden;
        style.scroll_x = scroll_x;
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
        });

        let origin = 10.0 - scroll_x;
        let mut chrome = Vec::new();
        let mut unused = Vec::new();
        let mut saw = false;
        for _ in 0..8 {
            chrome.clear();
            push_input_chrome_instances(
                &mut unused,
                &mut chrome,
                text,
                &style,
                origin,
                0.0,
                box_w,
                lh,
                None,
                1.0,
            );
            if !chrome.is_empty() {
                saw = true;
                break;
            }
        }
        assert!(saw, "hidden-overflow caret missing during blink ON");
        let caret_x = chrome[0].rect_pos[0];
        assert!(
            (caret_x - (origin + prefix)).abs() < 1.5,
            "caret must sit at content end minus scroll (x={caret_x} expected={})",
            origin + prefix
        );
        assert!(
            (caret_x - (10.0 + box_w - 2.0)).abs() < 2.0,
            "scrolled caret should land near the visible right edge, got {caret_x}"
        );
    }

    #[test]
    fn clip_to_box_nowrap_does_not_grow_past_known_width() {
        let text = "abcdefghijklmnopabcdefghijklmnop";
        let size = 14.0;
        let lh = 21.0;
        let prepared = crate::text::prepare(text, size, 400, lh);
        let intrinsic = crate::text::intrinsic_width(&prepared);
        assert!(intrinsic > 80.0, "fixture intrinsic={intrinsic}");
        let m = TextMeasureMeta {
            size,
            text: text.to_string(),
            shape_suffix: None,
            is_icon: false,
            line_height_px: Some(lh),
            line_height_scale: None,
            weight: 400,
            letter_spacing: 0.0,
            spans: None,
            nowrap: true,
            clip_to_box: true,
        };
        let known = taffy::geometry::Size {
            width: Some(64.0),
            height: None,
        };
        let avail = taffy::geometry::Size {
            width: taffy::style::AvailableSpace::Definite(64.0),
            height: taffy::style::AvailableSpace::MaxContent,
        };
        let out = measured_text_leaf_size(&m, known, avail);
        assert!(
            (out.width - 64.0).abs() < 0.5,
            "clip-to-box leaf must stay at known width, got {}",
            out.width
        );
        assert!(
            (out.height - lh).abs() < 1.0,
            "must stay one line h={}",
            out.height
        );
    }

    #[test]
    fn nowrap_without_clip_to_box_still_grows_to_intrinsic() {
        let text = "abcdefghijklmnopabcdefghijklmnop";
        let size = 14.0;
        let lh = 21.0;
        let prepared = crate::text::prepare(text, size, 400, lh);
        let intrinsic = crate::text::intrinsic_width(&prepared);
        let m = TextMeasureMeta {
            size,
            text: text.to_string(),
            shape_suffix: None,
            is_icon: false,
            line_height_px: Some(lh),
            line_height_scale: None,
            weight: 400,
            letter_spacing: 0.0,
            spans: None,
            nowrap: true,
            clip_to_box: false,
        };
        let known = taffy::geometry::Size {
            width: Some(64.0),
            height: None,
        };
        let avail = taffy::geometry::Size {
            width: taffy::style::AvailableSpace::Definite(64.0),
            height: taffy::style::AvailableSpace::MaxContent,
        };
        let out = measured_text_leaf_size(&m, known, avail);
        assert!(
            (out.width - intrinsic).abs() < 0.5,
            "button nowrap should still grow to intrinsic {intrinsic}, got {}",
            out.width
        );
    }

    #[test]
    fn hidden_input_clip_inflates_into_padding_inside_chrome() {
        let leaf = [20.0, 10.0, 80.0, 20.0];
        let chrome = Some([10.0, 0.0, 120.0, 40.0]);
        let c = hidden_input_text_clip(chrome, leaf).unwrap();
        assert!(
            (c[0] - 18.0).abs() < 0.01 && (c[2] - 84.0).abs() < 0.01,
            "content box + 2px ink, capped to chrome, got {c:?}"
        );
        let tight = Some([20.0, 10.0, 80.0, 20.0]);
        let inner = hidden_input_text_clip(tight, leaf).unwrap();
        assert!(
            (inner[2] - 80.0).abs() < 0.01,
            "cannot inflate past a content-sized parent clip, got {inner:?}"
        );
    }

    #[test]
    fn caret_after_cjk_in_latin_cjk_latin_is_past_glyph() {
        let mut fs = crate::renderer::font_loader::create_font_system();
        crate::renderer::font_loader::apply_default_families(fs.db_mut());
        crate::text::install_shared_font_system(&mut fs);

        let text = "1是1";
        // Caret after "是" (IME replace middle of "1111").
        let caret_byte = "1是".len();
        let mut style = TextStyle::default();
        style.size = 14.0;
        style.weight = 400;
        style.line_height_px = Some(21.0);
        style.color = [0, 0, 0, 255];
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: caret_byte,
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
        });

        let mut chrome = Vec::new();
        let mut unused = Vec::new();
        // Force blink-ON by retrying a few phases if needed.
        let mut saw = false;
        for _ in 0..6 {
            chrome.clear();
            push_input_chrome_instances(
                &mut unused,
                &mut chrome,
                text,
                &style,
                0.0,
                0.0,
                200.0,
                32.0,
                None,
                1.0,
            );
            if !chrome.is_empty() {
                saw = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(280));
        }
        assert!(saw, "caret chrome missing during blink ON");
        let caret_x = chrome[0].rect_pos[0];
        let after_cjk = prefix_width_logical(text, caret_byte, 14.0, 400, 21.0);
        let only_cjk = prefix_width_logical("是", "是".len(), 14.0, 400, 21.0);
        assert!(
            (caret_x - after_cjk).abs() < 0.5,
            "caret x={caret_x} should match intrinsic prefix={after_cjk}"
        );
        assert!(
            caret_x > only_cjk + 1.0,
            "caret must sit past mid-CJK (only_cjk={only_cjk}, caret_x={caret_x})"
        );
    }
}

#[cfg(test)]
mod portal_hoist_tests {
    use super::*;
    use crate::utils::popup::panel_block_clicks;

    fn layer_children(root: &Element) -> &[Element] {
        root.children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .map(|c| c.children.as_slice())
            .unwrap_or(&[])
    }

    #[test]
    fn portal_content_survives_second_hoist_pass() {
        // Solid keep-alive: after lift, source keeps a slot; next hoist must retain the panel.
        let mut panel = Element::new().bg([1.0, 1.0, 1.0, 1.0]).w(160.0).h(80.0);
        panel.set_attr(ATTR_PORTAL_KEY, "panel:test");
        panel = panel_block_clicks(panel, 1001);

        let mut root = Element::new().w(800.0).h(600.0).child(panel);
        let moved1 = hoist_portal_subtrees(&mut root);
        assert_eq!(moved1, 1, "first pass lifts the panel");
        assert_eq!(layer_children(&root).len(), 1);
        let bg = layer_children(&root)[0].style.bg_color;
        assert!(bg[3] > 0.0, "panel chrome must survive hoist");

        assert!(
            root.children
                .iter()
                .any(|c| c.attrs.get(ATTR_PORTAL_SLOT).is_some()),
            "slot marker must remain in source tree"
        );

        let moved2 = hoist_portal_subtrees(&mut root);
        assert_eq!(moved2, 0, "retention path must not rewrite the layer");
        assert_eq!(
            layer_children(&root).len(),
            1,
            "hoisted panel must persist while slot is wanted"
        );
        let bg2 = layer_children(&root)[0].style.bg_color;
        assert_eq!(bg2, bg, "background style must not be stripped on retain");
        // Identity retained: same node object still has opaque chrome.
        assert!(
            layer_children(&root)[0].style.bg_color[3] > 0.99,
            "panel background must stay opaque"
        );
    }

    #[test]
    fn portal_overlay_paint_split_after_page_content() {
        let mut page = Element::new().w(200.0).h(40.0).bg([0.9, 0.9, 0.9, 1.0]);
        page = page.child(Element::new().text("page"));

        let mut panel = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(120.0)
            .h(60.0)
            .absolute()
            .left(20.0)
            .top(20.0);
        panel.set_attr(ATTR_PORTAL_KEY, "panel:split");
        panel = panel_block_clicks(panel, 1001);
        panel = panel.child(Element::new().text("overlay"));

        let mut root = Element::new().w(800.0).h(600.0).child(page).child(panel);
        assert!(hoist_portal_subtrees(&mut root) >= 1);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.marked,
            "portal layer must mark the overlay paint split"
        );
        assert!(
            result.overlay_split.instance_start < result.instances.len(),
            "overlay must contribute rect instances"
        );
        assert!(
            result.overlay_split.text_start < result.text_nodes.len(),
            "overlay must contribute text nodes after the split"
        );
        // Page glyphs are collected before the portal suffix.
        assert!(result.text_nodes[..result.overlay_split.text_start]
            .iter()
            .any(|t| t.text == "page"));
        assert!(result.text_nodes[result.overlay_split.text_start..]
            .iter()
            .any(|t| t.text == "overlay"));
        assert!(
            !result.overlay_split.bands.is_empty(),
            "each portal child should open a paint band"
        );
        let overlay_i0 = result.overlay_split.instance_start as u32;
        for op in &result.paint_ops[..result.overlay_split.paint_op_start] {
            if let PaintOp::Rects { start, count } = *op {
                assert!(
                    start + count <= overlay_i0,
                    "page Rects must not swallow overlay panel fills (would paint page glyphs on the popover); op={start}+{count} overlay_i0={overlay_i0}"
                );
            }
        }
    }

    #[test]
    fn stacked_portal_panels_get_separate_paint_bands() {
        // Two overlapping panels: band1's instances must start after band0's texts would
        // otherwise paint over band1's fill if all overlay rects were drawn before all text.
        let page = Element::new().w(100.0).h(40.0).bg([0.95, 0.95, 0.95, 1.0]);

        let mut lower = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(160.0)
            .h(120.0)
            .absolute()
            .left(10.0)
            .top(50.0)
            .z_index(1000);
        lower.set_attr(ATTR_PORTAL_KEY, "panel:lower");
        lower = panel_block_clicks(lower, 1000);
        lower = lower.child(Element::new().text("lower-glyph"));

        let mut upper = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(160.0)
            .h(120.0)
            .absolute()
            .left(80.0)
            .top(80.0)
            .z_index(1010);
        upper.set_attr(ATTR_PORTAL_KEY, "panel:upper");
        upper = panel_block_clicks(upper, 1010);
        upper = upper.child(Element::new().text("upper-glyph"));

        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(page)
            .child(lower)
            .child(upper);
        assert!(hoist_portal_subtrees(&mut root) >= 1);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.bands.len() >= 2,
            "expected ≥2 portal bands, got {}",
            result.overlay_split.bands.len()
        );
        let ranges = result.overlay_split.band_ranges(
            result.instances.len(),
            result.textured_draws.len(),
            result.text_nodes.len(),
            result.input_chrome_instances.len(),
            result.paint_ops.len(),
        );
        assert!(ranges.len() >= 2);
        let a = ranges[ranges.len() - 2];
        let b = ranges[ranges.len() - 1];
        assert!(a.text.0 < a.text.1, "lower band must include text");
        assert!(
            b.instance.0 >= a.instance.1 || b.instance.0 > a.instance.0,
            "upper band instances follow lower band"
        );
        assert!(
            b.text.0 >= a.text.1,
            "upper band text follows lower band text"
        );
        assert!(result.text_nodes[a.text.0..a.text.1]
            .iter()
            .any(|t| t.text == "lower-glyph"));
        assert!(result.text_nodes[b.text.0..b.text.1]
            .iter()
            .any(|t| t.text == "upper-glyph"));
    }

    #[test]
    fn paint_ops_texture_then_sibling_rect() {
        let mut tex = Element::new().w(40.0).h(8.0);
        tex.image_texture = Some("ramp".into());
        let thumb = Element::new().w(20.0).h(20.0).bg([1.0, 1.0, 1.0, 1.0]);
        let mut root = Element::new()
            .w(200.0)
            .h(40.0)
            .relative()
            .child(tex)
            .child(thumb);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 40.0, [0.0, 0.0]);
        let kinds: Vec<&'static str> = result
            .paint_ops
            .iter()
            .map(|op| match op {
                PaintOp::Rects { .. } => "r",
                PaintOp::Textured { .. } => "t",
            })
            .collect();
        let tex_at = kinds.iter().position(|k| *k == "t");
        let rect_after = kinds.iter().rposition(|k| *k == "r");
        assert!(tex_at.is_some() && rect_after.is_some());
        assert!(
            tex_at.unwrap() < rect_after.unwrap(),
            "textured track must paint before sibling thumb rects, ops={kinds:?}"
        );
    }

    #[test]
    fn portal_drops_when_slot_removed() {
        let mut panel = Element::new().w(100.0).h(40.0);
        panel.set_attr(ATTR_PORTAL_KEY, "panel:close");
        let mut root = Element::new().child(panel_block_clicks(panel, 1001));
        assert_eq!(hoist_portal_subtrees(&mut root), 1);
        assert_eq!(layer_children(&root).len(), 1);

        // Overlay closed: reactive tree no longer declares the portal/slot.
        root.children.retain(|c| {
            c.template_id.as_deref() == Some(PORTAL_LAYER_ID)
                || c.attrs.get(ATTR_PORTAL_SLOT).is_none()
        });

        let _ = hoist_portal_subtrees(&mut root);
        assert!(
            layer_children(&root).is_empty(),
            "closing must drop hoisted content when no longer wanted"
        );
    }

    #[test]
    fn overflow_scroll_does_not_clip_own_outset_shadow() {
        // Regression: pairing overflow with box-shadow used to scissor the element's own
        // paint to its border box, turning a soft rounded shadow into a sharp 90° corner.
        let mut panel = Element::new()
            .w(120.0)
            .h(80.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .rounded(8.0)
            .shadow([0.0, 0.0, 0.0, 0.25], 8.0)
            .overflow_y("auto")
            .absolute()
            .left(40.0)
            .top(40.0);
        panel = panel.child(Element::new().w(100.0).h(200.0).bg([0.5, 0.5, 0.5, 1.0]));

        let mut root = Element::new().w(800.0).h(600.0).child(panel);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

        let panel_inst = result
            .instances
            .iter()
            .find(|i| {
                i.shadow_blur > 0.0
                    && i.color[3] > 0.5
                    && (i.rect_size[0] - 120.0).abs() < 1.0
                    && (i.rect_size[1] - 80.0).abs() < 1.0
            })
            .expect("panel chrome with shadow");
        assert!(
            panel_inst.clip_size == InstanceInput::NO_CLIP_SIZE || panel_inst.clip_size[0] < 0.0,
            "own overflow must not clip outset shadow; clip_size={:?}",
            panel_inst.clip_size
        );

        let child_clipped = result.instances.iter().any(|i| {
            i.color[0] > 0.4
                && i.color[0] < 0.6
                && i.clip_size[0] > 0.0
                && i.clip_size[1] > 0.0
                && (i.clip_size[0] - 120.0).abs() < 2.0
                && (i.clip_size[1] - 80.0).abs() < 2.0
        });
        assert!(
            child_clipped,
            "overflow must still clip descendants to the panel box"
        );
    }
}

#[cfg(test)]
mod text_cascade_tests {
    use super::*;

    #[test]
    fn decoration_color_none_is_current_color_not_parent_rgb() {
        // Parent root defaults to white text; child sets primary-like black via attr.
        let mut root = Element::new();
        root.text_style.color = [255, 255, 255, 255];
        root.text_style.decoration_color = None;

        let mut child = Element::new().text("Link").text_color([0, 0, 0, 255]);
        // Stale pinned RGB without attr — cascade must clear back to currentColor.
        child.text_style.decoration_color = Some([255, 255, 255, 255]);
        assert!(!child.attrs.contains_key("text_decoration_color"));
        root.children.push(child);

        cascade_inheritable_text_style_for_test(&mut root);
        let leaf = &root.children[0];
        assert_eq!(leaf.text_style.color, [0, 0, 0, 255]);
        assert_eq!(
            leaf.text_style.decoration_color, None,
            "None = currentColor at draw; must not keep parent's white RGB"
        );
    }

    #[test]
    fn decoration_color_explicit_still_inherits() {
        let mut root = Element::new();
        root.text_style.decoration_color = Some([239, 68, 68, 255]); // red-500-ish
        let child = Element::new().text("x").text_color([0, 0, 0, 255]);
        root.children.push(child);

        cascade_inheritable_text_style(&mut root.children[0], Some(root.text_style.clone()));
        assert_eq!(
            root.children[0].text_style.decoration_color,
            Some([239, 68, 68, 255])
        );
    }
}
