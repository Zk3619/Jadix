//! Taffy 布局、文本测量与将树展开GPU 实例 / 文本排版信息
//! 命名说明：本模块**不是** [`crate::renderer`]（wgpu/glyphon），只做 **布局与遍*
//! 因此使用 `LayoutTree` / `layout_tree`，避免与「渲染器」混淆
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
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
    scroll_viewport_axis_offset, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
    SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::{Element, UnitCtx};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::TextStyle;

/// Heap DFS so dirty/measure walks do not map element-tree depth onto the OS stack.
fn for_each_element<'a>(root: &'a Element, visit: impl FnMut(&'a Element)) {
    crate::core::tree::walk_elements(root, visit);
}

/// Test-only stack watermark for Windows PE ~1 MiB overflow diagnosis.
/// `mark_*` samples a local; consecutive DFS entries approximate one frame.
#[cfg(test)]
pub(crate) mod stack_probe {
    use std::cell::Cell;

    thread_local! {
        static BASE: Cell<usize> = const { Cell::new(0) };
        static MIN: Cell<usize> = const { Cell::new(usize::MAX) };
        static COLLECT: Cell<(u32, usize, usize)> = const { Cell::new((0, 0, 0)) };
        static BUILD: Cell<(u32, usize, usize)> = const { Cell::new((0, 0, 0)) };
        static CASCADE: Cell<(u32, usize, usize)> = const { Cell::new((0, 0, 0)) };
    }

    #[inline(never)]
    fn sp() -> usize {
        let x = 0u8;
        &x as *const u8 as usize
    }

    fn note(slot: &'static std::thread::LocalKey<Cell<(u32, usize, usize)>>) {
        let p = sp();
        BASE.with(|b| {
            if b.get() == 0 {
                b.set(p);
            }
        });
        MIN.with(|m| m.set(m.get().min(p)));
        slot.with(|c| {
            let (n, last, max_d) = c.get();
            let d = if n > 0 && last > p { last - p } else { 0 };
            c.set((n + 1, p, max_d.max(d)));
        });
    }

    pub fn reset() {
        BASE.with(|c| c.set(0));
        MIN.with(|c| c.set(usize::MAX));
        COLLECT.with(|c| c.set((0, 0, 0)));
        BUILD.with(|c| c.set((0, 0, 0)));
        CASCADE.with(|c| c.set((0, 0, 0)));
    }

    pub fn mark_collect() {
        note(&COLLECT);
    }
    pub fn mark_build() {
        note(&BUILD);
    }
    pub fn mark_cascade() {
        note(&CASCADE);
    }

    pub fn used() -> usize {
        let base = BASE.with(|c| c.get());
        let min = MIN.with(|c| c.get());
        if base == 0 || min == usize::MAX || base < min {
            0
        } else {
            base - min
        }
    }

    pub fn collect_max_frame() -> usize {
        COLLECT.with(|c| c.get().2)
    }
    pub fn build_max_frame() -> usize {
        BUILD.with(|c| c.get().2)
    }
    pub fn cascade_max_frame() -> usize {
        CASCADE.with(|c| c.get().2)
    }
    pub fn collect_marks() -> u32 {
        COLLECT.with(|c| c.get().0)
    }
}

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
pub(crate) const ATTR_HOISTED_MOUNT: &str = "rui_portal_hoisted_mount";

/// Stamped with `data-stacking-context` when this crate opened a local overlay stack
/// on a host that did not already opt in. Cleared when the last local overlay leaves
/// so `collect_skip_live` does not keep the whole gallery column unskippable.
const ATTR_LOCAL_OVERLAY_STACK: &str = "data-rui-local-overlay-stack";

/// Opt out of ancestor overflow clip (Command / Dialog stages that must paint over the sidebar).
pub const ATTR_ESCAPE_OVERFLOW_CLIP: &str = "rui_escape_overflow_clip";

/// Body-layer / `position:fixed` / explicit escape skip ancestor overflow + scroll.
/// Local `mount=` hoisted nodes stay in the host: clipped and scrolled with it.
pub(crate) fn escapes_ancestor_overflow(el: &Element) -> bool {
    if el.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return true;
    }
    // Host-mounted portals must follow the host even if a prior body-mount frame
    // left `position_fixed` set (keyed overlay adopt copies style flags).
    let local_mount = el
        .attrs
        .get(ATTR_HOISTED_MOUNT)
        .is_some_and(|s| !s.trim().is_empty());
    if local_mount {
        return false;
    }
    if el.style.position_fixed {
        return true;
    }
    if el.attrs.get(ATTR_ESCAPE_OVERFLOW_CLIP).map(|s| s.as_str()) == Some("1") {
        return true;
    }
    el.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1")
}

/// Portal stacking shells that intentionally pass hits to content underneath.
///
/// True when the node opts into [`PointerEvents::BoxNone`], or is the root
/// [`PORTAL_LAYER_ID`] mount. Hoisted overlays (menus / dialogs / underlays) keep
/// `auto` by default so their silhouette blocks click-through; only explicit
/// `box-none` / `none` (viewport stage chrome, empty anchors, Toast host) pass.
#[inline]
pub fn portal_hit_passthrough(el: &Element) -> bool {
    use crate::core::tree::PointerEvents;
    el.pointer_events_mode() == PointerEvents::BoxNone
        || el.template_id.as_deref() == Some(PORTAL_LAYER_ID)
}

/// Island that hosts (or hosted) a portal: live `rui_portal`, keep-alive slot, or hoisted root.
///
/// Overlay open/close reconciles these 0×0 placeholders. That is not page structure — GPU
/// FullClear of the gallery is wrong; callers scissor to [`hoisted_portal_damage`] instead.
pub(crate) fn subtree_declares_portal(el: &Element) -> bool {
    crate::core::tree::any_in_tree(el, |n| {
        n.attrs.get("rui_portal").map(String::as_str) == Some("1")
            || n.attrs.contains_key(ATTR_PORTAL_SLOT)
            || n.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
    })
}

/// Union of laid-out hoisted overlay boxes (logical px `[x,y,w,h]`).
///
/// Used as PartialLoad scissor when a popover/tooltip/menu joins or leaves the portal layer.
/// A viewport-filling modal underlay still covers the window via [`crate::app::damage::damage_covers_full`].
pub(crate) fn hoisted_portal_damage(root: &Element) -> Option<[f32; 4]> {
    let boxes = hoisted_portal_boxes(root);
    let first = *boxes.first()?;
    Some(boxes.iter().copied().skip(1).fold(first, |e, b| {
        let x = e[0].min(b[0]);
        let y = e[1].min(b[1]);
        let x2 = (e[0] + e[2]).max(b[0] + b[2]);
        let y2 = (e[1] + e[3]).max(b[1] + b[3]);
        [x, y, x2 - x, y2 - y]
    }))
}

/// Each hoisted overlay child's **painted** box (no union — node present submits separately).
///
/// Expand measure (`rui_floating_expand_measure`) lays out unconstrained content at
/// opacity 0. Recording that box as occupancy / `last_overlay_boxes` makes the first
/// accordion clip look like a shrink — OverlayRestore dest-resets window white around
/// the growing menu. Skip it. Animated `max-height` is the overflow clip; Taffy may
/// still report content height, so clamp to the specified max.
pub(crate) fn hoisted_portal_boxes(root: &Element) -> Vec<[f32; 4]> {
    hoisted_portal_stamps(root)
        .into_iter()
        .map(|(_, b)| b)
        .collect()
}

/// Painted hoisted overlay boxes keyed like [`crate::app::damage::paint_element_key`].
/// Keyframe replace dest-resets this overlay's previous stamp, not a sibling portal.
pub(crate) fn hoisted_portal_stamps(root: &Element) -> Vec<(u64, [f32; 4])> {
    let mut out = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) != Some("1") {
            return;
        }
        if let Some(b) = hoisted_portal_paint_box(el) {
            out.push((hoisted_portal_stamp_key(el), b));
        }
    });
    out
}

fn hoisted_portal_stamp_key(el: &Element) -> u64 {
    overlay_portal_stamp_key(el)
}

/// Stable GPU stamp identity: `rui_portal_key` (not Taffy `NodeId`, not AABB).
pub fn overlay_portal_stamp_key(el: &Element) -> u64 {
    let mut h = DefaultHasher::new();
    portal_key_of(el).hash(&mut h);
    h.finish()
}

/// Last-collect overlay membership for paint dirty classification.
/// Direct hoisted hosts use their own stamp; descendants inherit the host key.
fn stamp_element_overlay_paint(
    el: &mut Element,
    overlay_marked: bool,
    inherited_stamp: Option<u64>,
) {
    el.paint_overlay_stamp = if !overlay_marked {
        None
    } else if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1") {
        let k = overlay_portal_stamp_key(el);
        (k != 0).then_some(k)
    } else {
        inherited_stamp.filter(|k| *k != 0)
    };
}

/// True while an expand panel is on its unconstrained measure frame.
pub(crate) fn hoisted_expand_measuring(root: &Element) -> bool {
    let mut yes = false;
    crate::core::tree::walk_elements(root, |el| {
        if yes {
            return;
        }
        if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) != Some("1") {
            return;
        }
        if el
            .attrs
            .get("rui_floating_expand_measure")
            .map(String::as_str)
            == Some("1")
        {
            yes = true;
        }
    });
    yes
}

fn hoisted_portal_own_ink(el: &Element) -> bool {
    el.style.bg_color[3] > 0.0
        || el.style.bg_gradient.is_some()
        || el.style.shadow_color[3] > 0.0
        || el.style.border_width > 0.0
        || el.style.backdrop_blur > 0.5
}

fn hoisted_portal_child_paint_union(el: &Element) -> Option<[f32; 4]> {
    let mut acc: Option<[f32; 4]> = None;
    for child in &el.children {
        let Some(c) = hoisted_portal_paint_box(child) else {
            continue;
        };
        if c[2] <= 0.0 || c[3] <= 0.0 {
            continue;
        }
        acc = Some(match acc {
            Some(p) => {
                let x = p[0].min(c[0]);
                let y = p[1].min(c[1]);
                let x2 = (p[0] + p[2]).max(c[0] + c[2]);
                let y2 = (p[1] + p[3]).max(c[1] + c[3]);
                [x, y, x2 - x, y2 - y]
            }
            None => c,
        });
    }
    acc
}

fn hoisted_portal_paint_box(el: &Element) -> Option<[f32; 4]> {
    if el
        .attrs
        .get("rui_floating_expand_measure")
        .map(String::as_str)
        == Some("1")
    {
        return None;
    }
    // Visual box (CSS translate / collect abs patch). `layout_bounds` is Taffy
    // origin — Point title-bar drag used translate follow and occupancy stayed
    // on the grab box, so PartialLoad scissored content to a slice.
    let mut b = crate::core::tree::visual_paint_aabb(el, [0.0, 0.0]);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return None;
    }
    // Dialog/Sheet centering stage is `inset 0` / 100% with no fill. Recording that
    // empty host as occupancy dest-resets the sibling scrim on a zoom/slide CSS tick.
    // Occupancy is GPU ink, not the flex host.
    if !hoisted_portal_own_ink(el) {
        if let Some(kids) = hoisted_portal_child_paint_union(el) {
            b = kids;
        }
    }
    if let Some(h) = crate::layout::dim_length_px(el.style.max_size.height) {
        if h >= 0.0 {
            b[3] = b[3].min(h);
        }
    }
    if let Some(w) = crate::layout::dim_length_px(el.style.max_size.width) {
        if w >= 0.0 {
            b[2] = b[2].min(w);
        }
    }
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return None;
    }
    Some(b)
}

fn subtree_has_drag_overlay(el: &Element) -> bool {
    if el.attrs.contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY) {
        return true;
    }
    el.children.iter().any(subtree_has_drag_overlay)
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
/// - Returns a non-zero “structure dirty” count when the portal tree changed.
///   Callers should attach unbuilt Taffy nodes and sync the portal layer's
///   children — not recycle the whole gallery tree.
pub fn hoist_portal_subtrees(root: &mut Element) -> usize {
    // Drag ghost follow is Point paint chrome: pin Taffy at grab origin before
    // layout so collect can patch `abs` the same frame. Live left/top churn
    // disabled collect-skip and lagged the Gallery ghost.
    if crate::utils::drag::is_active() {
        crate::utils::drag::overlay::sync_overlay_ghost_insets(root);
    }
    crate::utils::anchored_overlay::sync_point_overlay_drag_insets(root);
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
            // Same portal key was already live: adopt CSS transition mid from the previous
            // hoisted tree (not layout_node — that breaks Taffy SlotMap topology).
            // Drag overlay ghosts are paint clones: grafting last frame's chip onto a
            // closed empty host leaves a frozen ghost on the portal layer.
            if let Some(mut prev_node) = prev.remove(&key) {
                if !subtree_has_drag_overlay(&node) && !subtree_has_drag_overlay(&prev_node) {
                    crate::utils::keyed_overlay::adopt_keyed_overlay(&mut node, &mut prev_node);
                }
            }
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
            if fresh_keys.contains(&key) {
                continue;
            }
            if !wanted.contains(&key) && !overlay_presence_retains_hoisted(&node) {
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
            root_bucket.sort_by_key(|c| c.style.z_index);
            layer.children.append(&mut root_bucket);
        }

        for (path, mut kids) in host_buckets {
            let host = element_mut_by_path(root, &path);
            // Local mount + in-page siblings share a parent paint band unless this host
            // is a stacking context. Nested gallery cards already open `page_stack_depth`,
            // so z-index on a child is not enough — glyphs of earlier siblings paint
            // *after* every rect in that band (scrim under, sharp text on top).
            stamp_local_overlay_host_stack(host);
            host.children.append(&mut kids);
        }

        // Layer membership was rewritten — always force a Taffy rebuild this frame.
        structure_dirty = pending.max(1).max(dirty);
    }

    // Dialog already on the layer + Select opens inside it: main-tree pending is 0, but a
    // fresh `rui_portal` still sits under the hoisted stage — promote it now. Same portal
    // key replaces the keep-alive copy (nested overlay nest), it does not append a second tree.
    let nested = hoist_nested_portals_from_hoisted(root, &mut wanted);
    unstamp_idle_local_overlay_hosts(root);
    structure_dirty + nested
}

fn count_rui_portal_nodes(node: &Element) -> usize {
    let mut n = 0;
    let mut stack: Vec<&Element> = vec![node];
    while let Some(node) = stack.pop() {
        if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        for c in node.children.iter().rev() {
            if c.attrs.get(ATTR_PORTAL_SLOT).is_some() {
                continue;
            }
            if c.attrs.get("rui_portal").map(|s| s.as_str()) == Some("1") {
                n += 1;
            }
            stack.push(c);
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
            keep_hoisted_portal(c, wanted)
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
    let mut dropped = 0usize;
    let mut stack: Vec<*mut Element> = vec![node as *mut Element];
    while let Some(ptr) = stack.pop() {
        let node = unsafe { &mut *ptr };
        if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        let mut i = 0;
        while i < node.children.len() {
            if node.children[i]
                .attrs
                .get(ATTR_HOISTED_PORTAL)
                .map(|s| s.as_str())
                == Some("1")
            {
                if wanted.contains(&portal_key_of(&node.children[i]))
                    || overlay_presence_retains_hoisted(&node.children[i])
                {
                    i += 1;
                } else {
                    node.children.remove(i);
                    dropped += 1;
                }
            } else {
                stack.push(std::ptr::addr_of_mut!(node.children[i]));
                i += 1;
            }
        }
    }
    dropped
}

fn keep_hoisted_portal(el: &Element, wanted: &std::collections::HashSet<String>) -> bool {
    wanted.contains(&portal_key_of(el)) || overlay_presence_retains_hoisted(el)
}

fn overlay_presence_retains_hoisted(el: &Element) -> bool {
    crate::utils::floating_motion::hoisted_should_retain(el)
        || crate::utils::modal_motion::hoisted_should_retain(el)
}

/// Nested overlay rebuild (Select inside ColorPicker / Dialog) must not stack two
/// copies of the same portal key. Keep-alive exit already occupies the layer;
/// appending a fresh `rui_portal` paints unclipped labels over the parent.
fn replace_or_push_hoisted(kids: &mut Vec<Element>, node: Element) {
    let key = portal_key_of(&node);
    if let Some(i) = kids.iter().position(|c| {
        c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1") && portal_key_of(c) == key
    }) {
        kids[i] = node;
        return;
    }
    kids.push(node);
}

fn append_hoisted_replacing(dest: &mut Vec<Element>, incoming: Vec<Element>) {
    for node in incoming {
        replace_or_push_hoisted(dest, node);
    }
}

fn place_hoisted(
    root: &Element,
    node: Element,
    mount: &str,
    root_bucket: &mut Vec<Element>,
    host_buckets: &mut Vec<(Vec<usize>, Vec<Element>)>,
) {
    if mount.is_empty() {
        replace_or_push_hoisted(root_bucket, node);
        return;
    }
    let mut path = Vec::new();
    if find_template_id_path(root, mount, &mut path) && !path.is_empty() {
        if let Some((_, kids)) = host_buckets.iter_mut().find(|(p, _)| *p == path) {
            replace_or_push_hoisted(kids, node);
        } else {
            host_buckets.push((path, vec![node]));
        }
    } else {
        replace_or_push_hoisted(root_bucket, node);
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
        node.style.position_fixed = false;
        if node.style.position != taffy::style::Position::Absolute {
            node.style.position = taffy::style::Position::Absolute;
        }
    } else {
        if node.style.position != taffy::style::Position::Absolute {
            node.style.position = taffy::style::Position::Absolute;
        }
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
    crate::core::tree::walk_elements(node, |n| {
        if let Some(k) = n.attrs.get(ATTR_PORTAL_SLOT) {
            let t = k.trim();
            if !t.is_empty() {
                out.insert(t.to_string());
            }
        }
    });
}

/// Promote `rui_portal=1` nodes that were inserted under already-hoisted overlays.
///
/// [`hoist_portal_rec_with_slots`] skips [`PORTAL_LAYER_ID`], and
/// [`count_rui_portal_nodes`] does too — so a Select opened inside a live Dialog never
/// enters the main lift path. Extract here and append as siblings on the same mount.
///
/// A rebuild of that Select (exit presence) inserts a **fresh** `rui_portal` under the
/// parent overlay while the previous hoisted copy is still retained. Replace + adopt
/// the same portal key — appending a second copy paints leftover labels over the parent
/// (ColorPicker format menu: flash then snap-unmount instead of accordion-up).
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
        if let Some(mut prev) = take_hoisted_by_key(root, &key) {
            if !subtree_has_drag_overlay(&node) && !subtree_has_drag_overlay(&prev) {
                crate::utils::keyed_overlay::adopt_keyed_overlay(&mut node, &mut prev);
            }
        }
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
        append_hoisted_replacing(&mut layer.children, root_bucket);
    }
    for (path, kids) in host_buckets {
        let host = element_mut_by_path(root, &path);
        stamp_local_overlay_host_stack(host);
        append_hoisted_replacing(&mut host.children, kids);
    }
    n
}

/// Pull the live hoisted copy for `key` so a nested rebuild can adopt it.
fn take_hoisted_by_key(root: &mut Element, key: &str) -> Option<Element> {
    if let Some(layer) = root
        .children
        .iter_mut()
        .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
    {
        if let Some(i) = layer.children.iter().position(|c| {
            c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1")
                && portal_key_of(c) == key
        }) {
            return Some(layer.children.remove(i));
        }
    }
    take_hoisted_by_key_from_hosts(root, key)
}

fn take_hoisted_by_key_from_hosts(node: &mut Element, key: &str) -> Option<Element> {
    if node.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return None;
    }
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            i += 1;
            continue;
        }
        if node.children[i]
            .attrs
            .get(ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1")
            && portal_key_of(&node.children[i]) == key
        {
            return Some(node.children.remove(i));
        }
        if let Some(found) = take_hoisted_by_key_from_hosts(&mut node.children[i], key) {
            return Some(found);
        }
        i += 1;
    }
    None
}

fn stamp_local_overlay_host_stack(host: &mut Element) {
    if host.attrs.get("data-stacking-context").map(String::as_str) == Some("1") {
        return;
    }
    host.set_attr("data-stacking-context", "1");
    host.set_attr(ATTR_LOCAL_OVERLAY_STACK, "1");
}

fn host_has_local_hoisted_overlay(host: &Element) -> bool {
    host.children.iter().any(|c| {
        c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
            && c.attrs
                .get(ATTR_HOISTED_MOUNT)
                .is_some_and(|s| !s.trim().is_empty())
    })
}

fn unstamp_idle_local_overlay_hosts(root: &mut Element) {
    crate::core::tree::walk_elements_mut(root, |node| {
        if node.attrs.get(ATTR_LOCAL_OVERLAY_STACK).map(String::as_str) != Some("1") {
            return;
        }
        if host_has_local_hoisted_overlay(node) {
            return;
        }
        node.attrs.remove("data-stacking-context");
        node.attrs.remove(ATTR_LOCAL_OVERLAY_STACK);
    });
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
    struct Frame {
        ptr: *const Element,
        index: usize,
        pop_path: bool,
    }
    let mut stack = vec![Frame {
        ptr: node as *const Element,
        index: 0,
        pop_path: false,
    }];
    while let Some(mut frame) = stack.pop() {
        let n = unsafe { &*frame.ptr };
        if frame.index >= n.children.len() {
            if frame.pop_path {
                path.pop();
            }
            continue;
        }
        let i = frame.index;
        frame.index += 1;
        let c = &n.children[i];
        if c.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            stack.push(frame);
            continue;
        }
        path.push(i);
        if c.template_id.as_deref() == Some(id) {
            return true;
        }
        stack.push(frame);
        stack.push(Frame {
            ptr: c as *const Element,
            index: 0,
            pop_path: true,
        });
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
    enum Phase {
        Enter,
        AfterChild,
    }
    struct Frame {
        ptr: *mut Element,
        index: usize,
        phase: Phase,
    }
    let mut stack = vec![Frame {
        ptr: node as *mut Element,
        index: 0,
        phase: Phase::Enter,
    }];
    while let Some(frame) = stack.pop() {
        // SAFETY: AfterChild runs only after the child's subtree frame has finished.
        let node = unsafe { &mut *frame.ptr };
        match frame.phase {
            Phase::Enter => {
                let i = frame.index;
                if i >= node.children.len() {
                    continue;
                }
                if node.children[i].template_id.as_deref() == Some(PORTAL_LAYER_ID)
                    || node.children[i].attrs.get(ATTR_PORTAL_SLOT).is_some()
                {
                    stack.push(Frame {
                        ptr: frame.ptr,
                        index: i + 1,
                        phase: Phase::Enter,
                    });
                    continue;
                }
                let child_ptr = std::ptr::addr_of_mut!(node.children[i]);
                stack.push(Frame {
                    ptr: frame.ptr,
                    index: i,
                    phase: Phase::AfterChild,
                });
                stack.push(Frame {
                    ptr: child_ptr,
                    index: 0,
                    phase: Phase::Enter,
                });
            }
            Phase::AfterChild => {
                let i = frame.index;
                if node.children[i].attrs.get("rui_portal").map(|s| s.as_str()) == Some("1") {
                    let n = node.children.remove(i);
                    let key = portal_key_of(&n);
                    node.children.insert(i, make_portal_slot(&key));
                    lifted.push(n);
                }
                stack.push(Frame {
                    ptr: frame.ptr,
                    index: i + 1,
                    phase: Phase::Enter,
                });
            }
        }
    }
}

/// CSS 3D border: mix toward black so a dark `primary` still has a visible bevel.
fn css_bevel_dark(c: [f32; 4]) -> [f32; 4] {
    [c[0] * 0.32, c[1] * 0.32, c[2] * 0.32, c[3]]
}

/// Mix toward white (not `rgb * 1.4`, which clips and stays dark on near-black).
fn css_bevel_light(c: [f32; 4]) -> [f32; 4] {
    [
        c[0] + (1.0 - c[0]) * 0.58,
        c[1] + (1.0 - c[1]) * 0.58,
        c[2] + (1.0 - c[2]) * 0.58,
        c[3],
    ]
}

/// CSS inset lighting. `side`: 0=top 1=right 2=bottom 3=left.
fn css_inset_tone(c: [f32; 4], side: usize) -> [f32; 4] {
    if side == 0 || side == 3 {
        css_bevel_dark(c)
    } else {
        css_bevel_light(c)
    }
}

fn css_outset_tone(c: [f32; 4], side: usize) -> [f32; 4] {
    if side == 0 || side == 3 {
        css_bevel_light(c)
    } else {
        css_bevel_dark(c)
    }
}

/// Uniform `border_color` is often left at transparent when only per-side colors
/// were set. SDF stroke is single-colored, so pick the first opaque side.
fn representative_border_color(style: &crate::style::Style) -> [f32; 4] {
    if style.border_color[3] > 0.001 {
        return style.border_color;
    }
    for c in style.resolved_border_colors() {
        if c[3] > 0.001 {
            return c;
        }
    }
    style.border_color
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

    /// Push a single side strip with style-aware rendering.
    /// `horizontal`: true for top/bottom edges (span full width), false for left/right.
    /// `side`: 0=top 1=right 2=bottom 3=left (CSS 3D lighting).
    fn push_styled_side(
        instances: &mut Vec<InstanceInput>,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        style: LineStyle,
        horizontal: bool,
        side: usize,
        opacity: f32,
        transform: [f32; 6],
        parent_clip: Option<[f32; 4]>,
    ) {
        if size[0] <= 0.0 || size[1] <= 0.0 || color[3] <= 0.0 || style.is_invisible() {
            return;
        }
        let bw = if horizontal { size[1] } else { size[0] };
        let span = if horizontal { size[0] } else { size[1] };

        let push_one =
            |instances: &mut Vec<InstanceInput>, p: [f32; 2], s: [f32; 2], c: [f32; 4]| {
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
                        backdrop_blur: 0.0,
                    }
                    .with_clip(parent_clip),
                );
            };

        match style {
            LineStyle::Solid => {
                push_one(instances, pos, size, color);
            }
            LineStyle::Dotted => {
                let d = bw.max(2.0);
                let step = d * 2.0;
                let mut t = 0.0;
                while t + d <= span + 0.05 {
                    let (p, s) = if horizontal {
                        ([pos[0] + t, pos[1]], [d, d])
                    } else {
                        ([pos[0], pos[1] + t], [d, d])
                    };
                    instances.push(
                        InstanceInput {
                            rect_pos: p,
                            rect_size: s,
                            color: mul_color_alpha(color, opacity),
                            border_color: [0.0; 4],
                            radii: [d * 0.5; 4],
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
                            backdrop_blur: 0.0,
                        }
                        .with_clip(parent_clip),
                    );
                    t += step;
                }
            }
            LineStyle::Dashed => {
                let dash = bw * 3.0;
                let gap = bw * 1.5;
                let step = dash + gap;
                let mut t = 0.0;
                while t + dash <= span + 0.05 {
                    let (p, s) = if horizontal {
                        ([pos[0] + t, pos[1]], [dash, size[1]])
                    } else {
                        ([pos[0], pos[1] + t], [size[0], dash])
                    };
                    push_one(instances, p, s, color);
                    t += step;
                }
            }
            LineStyle::Double => {
                let third = bw / 3.0;
                if horizontal {
                    push_one(instances, pos, [size[0], third], color);
                    push_one(
                        instances,
                        [pos[0], pos[1] + bw - third],
                        [size[0], third],
                        color,
                    );
                } else {
                    push_one(instances, pos, [third, size[1]], color);
                    push_one(
                        instances,
                        [pos[0] + bw - third, pos[1]],
                        [third, size[1]],
                        color,
                    );
                }
            }
            LineStyle::Groove => {
                // Outer band = inset lighting, inner band = outset (carved).
                let half = bw / 2.0;
                let outer_c = css_inset_tone(color, side);
                let inner_c = css_outset_tone(color, side);
                match side {
                    0 => {
                        push_one(instances, pos, [size[0], half], outer_c);
                        push_one(instances, [pos[0], pos[1] + half], [size[0], half], inner_c);
                    }
                    2 => {
                        push_one(instances, pos, [size[0], half], inner_c);
                        push_one(instances, [pos[0], pos[1] + half], [size[0], half], outer_c);
                    }
                    3 => {
                        push_one(instances, pos, [half, size[1]], outer_c);
                        push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], inner_c);
                    }
                    _ => {
                        push_one(instances, pos, [half, size[1]], inner_c);
                        push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], outer_c);
                    }
                }
            }
            LineStyle::Ridge => {
                let half = bw / 2.0;
                let outer_c = css_outset_tone(color, side);
                let inner_c = css_inset_tone(color, side);
                match side {
                    0 => {
                        push_one(instances, pos, [size[0], half], outer_c);
                        push_one(instances, [pos[0], pos[1] + half], [size[0], half], inner_c);
                    }
                    2 => {
                        push_one(instances, pos, [size[0], half], inner_c);
                        push_one(instances, [pos[0], pos[1] + half], [size[0], half], outer_c);
                    }
                    3 => {
                        push_one(instances, pos, [half, size[1]], outer_c);
                        push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], inner_c);
                    }
                    _ => {
                        push_one(instances, pos, [half, size[1]], inner_c);
                        push_one(instances, [pos[0] + half, pos[1]], [half, size[1]], outer_c);
                    }
                }
            }
            LineStyle::Inset => {
                push_one(instances, pos, size, css_inset_tone(color, side));
            }
            LineStyle::Outset => {
                push_one(instances, pos, size, css_outset_tone(color, side));
            }
            LineStyle::None | LineStyle::Hidden => {}
        }
    }

    // Top (full width)
    push_styled_side(
        instances,
        [x, y],
        [w, bt],
        colors[0],
        styles[0],
        true,
        0,
        opacity,
        transform,
        parent_clip,
    );
    // Bottom (full width)
    push_styled_side(
        instances,
        [x, y + (h - bb).max(0.0)],
        [w, bb],
        colors[2],
        styles[2],
        true,
        2,
        opacity,
        transform,
        parent_clip,
    );
    let mid_h = (h - bt - bb).max(0.0);
    // Left
    push_styled_side(
        instances,
        [x, y + bt],
        [bl, mid_h],
        colors[3],
        styles[3],
        false,
        3,
        opacity,
        transform,
        parent_clip,
    );
    // Right
    push_styled_side(
        instances,
        [x + (w - br).max(0.0), y + bt],
        [br, mid_h],
        colors[1],
        styles[1],
        false,
        1,
        opacity,
        transform,
        parent_clip,
    );
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
        let sub_u = if rect[2] > 0.0 {
            (draw[0] - rect[0]) / rect[2]
        } else {
            0.0
        };
        let sub_v = if rect[3] > 0.0 {
            (draw[1] - rect[1]) / rect[3]
        } else {
            0.0
        };
        let sub_uw = if rect[2] > 0.0 {
            draw[2] / rect[2]
        } else {
            1.0
        };
        let sub_vh = if rect[3] > 0.0 {
            draw[3] / rect[3]
        } else {
            1.0
        };
        // Map to source UV (rect top → smaller v, matching textured_rect vertices).
        let final_u = uv_x + sub_u * uv_w;
        let final_v = uv_y + sub_v * uv_h;
        textured_draws.push(TexturedDraw {
            rect: draw,
            uv_offset: [final_u, final_v],
            uv_scale: [sub_uw * uv_w, sub_vh * uv_h],
            texture_key: tex_key.clone(),
            opacity,
            tint_rgb: [1.0, 1.0, 1.0],
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: TexturedDraw::NO_CLIP_SIZE,
        });
    };

    // Nine regions (CSS border-image nine-slice):
    // Corners (always stretch)
    push_slice([bx, by, wl, wt], 0.0, 0.0, sl, st); // top-left
    push_slice([bx + bw - wr, by, wr, wt], 1.0 - sr, 0.0, sr, st); // top-right
    push_slice([bx, by + bh - wb, wl, wb], 0.0, 1.0 - sb, sl, sb); // bottom-left
    push_slice(
        [bx + bw - wr, by + bh - wb, wr, wb],
        1.0 - sr,
        1.0 - sb,
        sr,
        sb,
    ); // bottom-right

    // Edges (stretch for now; repeat/round/space would need multiple draws)
    let ew = bw - wl - wr;
    let eh = bh - wt - wb;
    let eu_w = (1.0 - sl - sr).max(0.0);
    let eu_h = (1.0 - st - sb).max(0.0);
    push_slice([bx + wl, by, ew, wt], sl, 0.0, eu_w, st); // top edge
    push_slice([bx + wl, by + bh - wb, ew, wb], sl, 1.0 - sb, eu_w, sb); // bottom edge
    push_slice([bx, by + wt, wl, eh], 0.0, st, sl, eu_h); // left edge
    push_slice([bx + bw - wr, by + wt, wr, eh], 1.0 - sr, st, sr, eu_h); // right edge

    // Center (only if fill)
    if style.border_image_slice_fill {
        push_slice([bx + wl, by + wt, ew, eh], sl, st, eu_w, eu_h);
    }
}

/// Minimum cross-axis size for an `auto` scrollbar at spring≈0 so track/thumb stay hittable; must match `app::state`.
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
fn apply_inheritable_text_fields(
    node: &mut Element,
    parent: &TextStyle,
    parent_wm: crate::style::WritingMode,
    parent_dir: crate::style::TextDirection,
) {
    if !node.style.writing_mode_specified {
        node.style.writing_mode = parent_wm;
    }
    if !node.style.direction_specified {
        node.style.direction = parent_dir;
    }
    let inherited_dir = node.style.direction;
    let ts = &mut node.text_style;
    // Keep Auto when the ancestor did not specify a paragraph direction so
    // `prepare_text` does not inject bidi marks on ordinary LTR copy.
    if ts.direction == crate::style::TextDirection::Auto
        && inherited_dir != crate::style::TextDirection::Auto
    {
        ts.direction = inherited_dir;
    }
    if ts.color.is_none() {
        ts.color = parent.color;
    }
    if ts.size.is_none() {
        ts.size = parent.size;
    }
    if ts.weight.is_none() {
        ts.weight = parent.weight;
    }
    if ts.font_family.is_none() {
        ts.font_family = parent.font_family.clone();
    }
    match ts.align {
        None => ts.align = parent.align,
        Some(crate::style::TextAlign::MatchParent) => {
            ts.align = Some(crate::style::TextAlign::resolve_match_parent(
                parent.align_resolved(),
                parent.direction,
            ));
        }
        Some(_) => {}
    }
    if ts.underline.is_none() {
        ts.underline = parent.underline;
    }
    if ts.overline.is_none() {
        ts.overline = parent.overline;
    }
    if ts.line_through.is_none() {
        ts.line_through = parent.line_through;
    }
    if ts.decoration_style.is_none() {
        ts.decoration_style = parent.decoration_style;
    }
    if !ts.decoration_color_specified {
        ts.decoration_color = parent.decoration_color;
    }
    if ts.decoration_thickness.is_none() {
        ts.decoration_thickness = parent.decoration_thickness;
    }
    if ts.underline_offset.is_none() {
        ts.underline_offset = parent.underline_offset;
    }
    if !ts.line_height_specified {
        ts.line_height_px = parent.line_height_px;
        ts.line_height_scale = parent.line_height_scale;
    }
    if !ts.letter_spacing_specified {
        ts.letter_spacing = parent.letter_spacing;
    }
}

fn cascade_inheritable_text_style(root: &mut Element, parent_style: Option<TextStyle>) {
    #[cfg(test)]
    stack_probe::mark_cascade();
    let parent = parent_style.unwrap_or_else(crate::style::TextStyle::cascade_root);
    // Heap worklist: style inherit must not map tree depth onto the OS call stack.
    let mut stack: Vec<(
        *mut Element,
        TextStyle,
        crate::style::WritingMode,
        crate::style::TextDirection,
    )> = Vec::new();
    stack.push((
        root as *mut Element,
        parent,
        crate::style::WritingMode::HorizontalTb,
        // Auto, not Ltr: CSS initial `ltr` is physical LTR, but feeding Ltr into
        // `text_style.direction` used to prepend U+200E on every run (visible junk).
        crate::style::TextDirection::Auto,
    ));
    while let Some((ptr, parent, parent_wm, parent_dir)) = stack.pop() {
        // SAFETY: pointers are into `root`'s exclusive subtree for this pass.
        let node = unsafe { &mut *ptr };
        apply_inheritable_text_fields(node, &parent, parent_wm, parent_dir);
        let inherit_attrs: Vec<(String, String)> = crate::renderer::SHIMMER_INHERIT_ATTRS
            .iter()
            .filter_map(|key| {
                node.attrs
                    .get(*key)
                    .map(|v| ((*key).to_string(), v.clone()))
            })
            .collect();
        let child_parent = node.text_style.effective_ancestry(&parent);
        let child_wm = node.style.writing_mode;
        let child_dir = node.style.direction;
        for child in node.children.iter_mut().rev() {
            for (key, value) in &inherit_attrs {
                child
                    .attrs
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            stack.push((
                child as *mut Element,
                child_parent.clone(),
                child_wm,
                child_dir,
            ));
        }
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
}

fn peek_scroll_area_meta(rest: &str) -> ScrollAreaSignalsCached {
    // Raw metrics only: half-pixel snapping vw/vh/cw/ch independently can erase a tiny
    // (ch - vh), making max_scroll 0 and snapping the thumb to the track start after idle.
    // Post-render already gates signal writes with a 0.1px threshold (window_events).
    ScrollAreaSignalsCached {
        vw: use_signal(&scroll_viewport_w_signal_key(rest), || 0.0_f32).peek(),
        vh: use_signal(&scroll_viewport_h_signal_key(rest), || 0.0_f32).peek(),
        cw: use_signal(&scroll_content_w_signal_key(rest), || 0.0_f32).peek(),
        ch: use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).peek(),
        sx: use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek(),
        sy: use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek(),
        overflow_x: use_signal(&scroll_overflow_x_signal_key(rest), || "auto".to_string()).peek(),
        overflow_y: use_signal(&scroll_overflow_y_signal_key(rest), || "auto".to_string()).peek(),
        scrollbar_vis_x: use_signal(&scrollbar_visibility_signal_key(rest, "x"), || {
            "auto".to_string()
        })
        .peek(),
        scrollbar_vis_y: use_signal(&scrollbar_visibility_signal_key(rest, "y"), || {
            "auto".to_string()
        })
        .peek(),
    }
}

fn scroll_area_signals_cached<'a>(
    rest: &str,
    cache: &'a mut HashMap<String, ScrollAreaSignalsCached>,
) -> &'a ScrollAreaSignalsCached {
    match cache.entry(rest.to_string()) {
        Entry::Occupied(o) => o.into_mut(),
        Entry::Vacant(v) => v.insert(peek_scroll_area_meta(rest)),
    }
}

/// Viewport collect runs before its overlay-track sibling. Write this frame's Taffy
/// metrics so overlay `can_scroll` does not keep last-frame overflow (short sidebar
/// + maximize, or a nested MessageScroller leftover) and paint a tall rail track.
fn seed_scroll_signals_from_collect(
    cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    rest: &str,
    vw: f32,
    vh: f32,
    cw: f32,
    ch: f32,
) {
    let Some(cache) = cache.as_mut() else {
        return;
    };
    match cache.entry(rest.to_string()) {
        Entry::Occupied(mut o) => {
            let e = o.get_mut();
            e.vw = vw;
            e.vh = vh;
            e.cw = cw;
            e.ch = ch;
        }
        Entry::Vacant(v) => {
            let mut sig = peek_scroll_area_meta(rest);
            sig.vw = vw;
            sig.vh = vh;
            sig.cw = cw;
            sig.ch = ch;
            v.insert(sig);
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
    clip_polys: Vec<crate::style::GpuClipPoly>,
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
            clip_polys: Vec::new(),
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
        self.clip_polys.clear();
    }
}

/// Flex sibling wholly outside an ancestor overflow clip during `scroll_only` collect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollSiblingBandAction {
    Visit,
    SkipBefore,
    StopAfter,
}

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
///
/// Non-visible overflow boxes contribute only their border box to an ancestor scroller
/// (CSS scrollable overflow). A stale inner extent here made the outer thumb jump as
/// nested Table/Tree/Masonry left the visit band (gallery data page).
#[inline]
fn skipped_sibling_extent(child: &Element, child_abs: [f32; 2], child_size: [f32; 2]) -> [f32; 2] {
    let box_ext = scroll_sibling_extent(child_abs, child_size);
    let contain_x = overflow_contains_axis(child.style.overflow.x);
    let contain_y = overflow_contains_axis(child.style.overflow.y);
    if contain_x && contain_y {
        return box_ext;
    }
    let mut ext = if child.collect_extent_valid {
        child.collect_extent
    } else {
        box_ext
    };
    if contain_x {
        ext[0] = box_ext[0];
    }
    if contain_y {
        ext[1] = box_ext[1];
    }
    ext
}

#[inline]
fn overflow_contains_axis(overflow: taffy::style::Overflow) -> bool {
    !matches!(overflow, taffy::style::Overflow::Visible)
}

fn child_is_nested_scroll_viewport(child: &Element) -> bool {
    child
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX))
}

/// Scrollable overflow relative to `origin` (viewport content-box top-left).
///
/// Independent of paint clip/band cull, so `content_w`/`content_h` stay stable while
/// scrolling. Includes abspos (chart tick labels). Stops at nested `scroll:` viewports
/// and non-visible overflow boxes — those contribute only their border box.
///
/// Iterative: this runs from inside `collect_instances_mut`, whose frames are already
/// large; a recursive walk of the gallery data/basics trees overflowed the default stack.
fn taffy_scrollable_overflow_extent(
    taffy: &TaffyTree,
    element: &Element,
    node: NodeId,
    origin: [f32; 2],
) -> [f32; 2] {
    let mut r = origin[0];
    let mut b = origin[1];
    // (element, taffy node, origin, accumulate x, accumulate y)
    let mut stack: Vec<(&Element, NodeId, [f32; 2], bool, bool)> = Vec::new();
    stack.push((element, node, origin, true, true));
    while let Some((el, nid, org, allow_x, allow_y)) = stack.pop() {
        let Ok(child_nodes) = taffy.children(nid) else {
            continue;
        };
        let n = el.children.len().min(child_nodes.len());
        for i in 0..n {
            let child = &el.children[i];
            if child.style.position_fixed {
                continue;
            }
            let Ok(cl) = taffy.layout(child_nodes[i]) else {
                continue;
            };
            let child_origin = [org[0] + cl.location.x, org[1] + cl.location.y];
            if allow_x {
                r = r.max(child_origin[0] + cl.size.width);
            }
            if allow_y {
                b = b.max(child_origin[1] + cl.size.height);
            }
            if child_is_nested_scroll_viewport(child) {
                continue;
            }
            let next_x = allow_x && !overflow_contains_axis(child.style.overflow.x);
            let next_y = allow_y && !overflow_contains_axis(child.style.overflow.y);
            if next_x || next_y {
                stack.push((child, child_nodes[i], child_origin, next_x, next_y));
            }
        }
    }
    [r, b]
}

fn scroll_viewport_taffy_content_size(
    taffy: &TaffyTree,
    element: &Element,
    node: NodeId,
    layout_size: taffy::geometry::Size<f32>,
) -> [f32; 2] {
    let pad_b = crate::layout::lp_length_px(element.style.padding.bottom)
        .unwrap_or(0.0)
        .max(0.0);
    let ovf = taffy_scrollable_overflow_extent(taffy, element, node, [0.0, 0.0]);
    [
        ovf[0].max(layout_size.width),
        (ovf[1] + pad_b).max(layout_size.height),
    ]
}

/// Authored block size is `max-content` / `min-content` / `fit-content`.
///
/// A scroll viewport's intrinsic-height column is taller than the clip; sibling
/// band cull of its children drops the live end edge. Detect the column by CSS
/// dim, not a product `data-slot`.
fn element_authored_block_is_intrinsic(el: &Element) -> bool {
    el.style.unit_specs.iter().any(|(slot, dim)| {
        matches!(
            *slot,
            crate::style::UnitSlot::Height | crate::style::UnitSlot::BlockSize
        ) && dim.is_content_keyword()
    })
}

/// Tight-cap Taffy `content_h` to in-flow children of an intrinsic column.
///
/// Inflated `min-h-full` / leftover extent would otherwise publish a tail past
/// the last row; post-render then clamps a follow jump. Product slots are not
/// consulted — any max-content (etc.) column inside a scroll viewport qualifies.
fn cap_intrinsic_scroll_column_content_h(
    layout_tree: &LayoutTree,
    viewport_el: &Element,
    viewport_nid: NodeId,
    content_h: f32,
    viewport_h: f32,
) -> f32 {
    let child_nodes = match layout_tree.taffy.children(viewport_nid) {
        Ok(n) => n,
        Err(_) => return content_h,
    };
    for (i, child) in viewport_el.children.iter().enumerate() {
        if !element_authored_block_is_intrinsic(child) {
            continue;
        }
        let Some(&content_nid) = child_nodes.get(i) else {
            return content_h;
        };
        let Ok(content_lay) = layout_tree.taffy.layout(content_nid) else {
            return content_h;
        };
        let origin_y = content_lay.location.y;
        let mut found_item = false;
        let mut incomplete = false;
        let mut max_bottom = origin_y;
        walk_intrinsic_column_child_bottoms(
            child,
            layout_tree,
            origin_y,
            &mut found_item,
            &mut incomplete,
            &mut max_bottom,
        );
        if incomplete {
            // A newly grafted row has no Taffy node yet. Do not min() content_h down to
            // the previous items — post-render would clamp a follow jump back to the old max.
            return content_h;
        }
        if !found_item {
            // scroll_only / band-cull may miss item nodes. Publishing `viewport_h` makes
            // max_scroll=0 and post-render clamps a live nested scroller to the top —
            // wheel then chains to the parent while glyphs stay at the previous offset.
            if let Some(rest) = viewport_el
                .focus_id
                .as_deref()
                .and_then(|f| f.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
            {
                let anchored = use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).peek();
                if anchored > viewport_h + 0.5 {
                    return content_h.max(anchored);
                }
            }
            return content_h.max(viewport_h);
        }
        let pad_b = crate::layout::lp_length_px(child.style.padding.bottom)
            .unwrap_or(0.0)
            .max(0.0);
        let capped = (max_bottom + pad_b).max(viewport_h);
        if capped <= viewport_h + 0.5 && content_h > viewport_h + 0.5 {
            return content_h;
        }
        return content_h.min(capped);
    }
    content_h
}

fn walk_intrinsic_column_child_bottoms(
    column: &Element,
    layout_tree: &LayoutTree,
    origin_y: f32,
    found_item: &mut bool,
    incomplete: &mut bool,
    max_bottom: &mut f32,
) {
    for item in &column.children {
        if item.style.position_fixed || item.style.position == Position::Absolute {
            continue;
        }
        *found_item = true;
        match item
            .layout_node
            .and_then(|nid| layout_tree.taffy.layout(nid).ok())
        {
            Some(lay) => {
                *max_bottom = max_bottom.max(origin_y + lay.location.y + lay.size.height);
            }
            None => {
                *incomplete = true;
            }
        }
    }
}

/// Layout-backed scroll metrics for `reactive_sync` (signal `content_h` can lag paint by a frame).
pub(crate) fn scroll_viewport_content_metrics(
    layout_tree: &LayoutTree,
    viewport_el: &Element,
) -> Option<(f32, f32)> {
    let nid = viewport_el.layout_node?;
    let layout = layout_tree.taffy.layout(nid).ok()?;
    if layout.size.height <= 1.0 {
        return None;
    }
    let [_, mut ch] =
        scroll_viewport_taffy_content_size(&layout_tree.taffy, viewport_el, nid, layout.size);
    ch = cap_intrinsic_scroll_column_content_h(
        layout_tree,
        viewport_el,
        nid,
        ch,
        layout.size.height,
    );
    if ch <= 1.0 {
        return None;
    }
    Some((ch, layout.size.height))
}

fn textarea_scrollport_content_size(
    element: &Element,
    abs_x: f32,
    abs_y: f32,
    layout_size: taffy::geometry::Size<f32>,
) -> [f32; 2] {
    [
        crate::utils::text_field::textarea_scroll_content_w(element, abs_x, layout_size.width),
        crate::utils::text_field::textarea_scroll_content_h(element, abs_y, layout_size.height),
    ]
}

#[cfg(feature = "devtools")]
fn next_scroll_acc_for_children(element: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let mut next = scroll_acc;
    let is_scroll_viewport = element
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
    if is_scroll_viewport || matches!(element.style.overflow.x, taffy::style::Overflow::Scroll) {
        next[0] += element.scroll_offset[0];
    }
    if is_scroll_viewport || matches!(element.style.overflow.y, taffy::style::Overflow::Scroll) {
        next[1] += element.scroll_offset[1];
    }
    next
}

/// CPU breakdown (ms) for one layout+collect pass; see `AppState` debug `render profile`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutProfileMs {
    /// `build_and_layout`: build/prune Taffy subtree (cascade, clear); `0` on the `fast` path.
    pub rebuild_ms: f32,
    /// `build_and_layout_fast`: root size, cascade, `update_taffy_node_style(root)`; `0` on full-rebuild path.
    pub setup_ms: f32,
    /// `compute_layout_with_measure`; `0` when skipped by `scroll_only`.
    pub taffy_compute_ms: f32,
    /// `stamp_subtree_paint_hashes` + `collect_instances_mut`.
    pub collect_ms: f32,
    /// Paint-hash walk inside [`Self::collect_ms`].
    pub collect_stamp_ms: f32,
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
///
/// [`Self::page_stack_bands`] applies the same band model to **in-page** stacking contexts
/// (e.g. CarouselStack cards with `z-index`): without it, page glyphs from lower cards
/// paint after every card rect and bleed through the top card.
///
/// Bands are recorded when the parent opts in via `data-stacking-context=1`, **or** when any
/// direct child has a non-zero `z-index`. The **outermost** such parent splits every child
/// (in-flow page + overlay scrollbar). A **nested** stacking-context (DragProvider under a
/// Gallery ScrollArea) only opens a band for children that themselves stack — z-index,
/// `data-stacking-context`, or absolute/fixed. Idle in-flow rows must share one band:
/// one band per row reshapes every label while scrolling (`slow render text=150ms+`).
/// Inside an already-marked portal overlay (Toast), the same z-stack appends to
/// [`Self::bands`] instead — `page_stack_bands` end at the portal split and cannot cover
/// overlay glyphs.
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
    /// In-page z-index stacking contexts: one band per z-ordered child (same draw model
    /// as [`Self::bands`], but within the page layer before the portal suffix).
    pub page_stack_bands: Vec<OverlayPaintBand>,
    marked: bool,
    /// Nested overlay scrollbars (`z-index: 1000`) must not append to the same global
    /// [`Self::page_stack_bands`] list — that mis-assigns glyphs vs rects once a nested
    /// scroller (MessageScroller, table, …) enters the visit band.
    page_stack_depth: u32,
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
    /// Underlay 有效 opacity（祖先 × 自身），用于 backdrop blur 与遮罩同步淡入淡出。
    pub underlay_opacity: f32,
    /// [`overlay_portal_stamp_key`] of this portal-layer child. `0` = synthetic / page-stack.
    pub stamp_key: u64,
}

fn subtree_has_backdrop_blur(el: &Element) -> bool {
    if el.style.backdrop_blur > 0.5 {
        return true;
    }
    el.children.iter().any(subtree_has_backdrop_blur)
}

/// Nested stacking-context children that form their own CSS stacking level.
/// In-flow `z-index: auto` siblings share the parent's band (sortable rows).
fn child_opens_page_stack_band(el: &Element) -> bool {
    if el.style.z_index != 0
        || el.style.position == taffy::style::Position::Absolute
        || el.style.position_fixed
    {
        return true;
    }
    el.attrs.get("data-stacking-context").map(|s| s.as_str()) == Some("1")
}

/// Resolved `[start, end)` ranges for drawing one overlay band.
#[derive(Clone, Copy, Debug)]
pub struct OverlayBandRange {
    pub instance: (usize, usize),
    pub textured: (usize, usize),
    pub text: (usize, usize),
    pub chrome: (usize, usize),
    pub paint_ops: (usize, usize),
    pub backdrop_blur: f32,
    pub underlay_opacity: f32,
    /// [`OverlayPaintBand::stamp_key`] copied through [`OverlayPaintSplit::band_ranges`].
    pub stamp_key: u64,
}

/// Collect slots belonging to one Element (Taffy node). Present draws these
/// ranges under that node's scissor — never a union AABB of neighbors.
#[derive(Clone, Copy, Debug)]
pub struct NodePaint {
    pub key: u64,
    pub bounds: [f32; 4],
    pub overlay: bool,
    pub instance: (usize, usize),
    pub textured: (usize, usize),
    pub text: (usize, usize),
    pub chrome: (usize, usize),
    pub paint_ops: (usize, usize),
    /// Ancestor overflow clip in **paint** space (ScrollArea / `overflow:hidden`).
    /// Identity overflow stays the layout border (Wave 18 title-bar leak).
    /// Rotated overflow is the AABB of the transformed local box — dest-reset
    /// and node-present scissor must cover diamond tips, not the unrotated AABB.
    pub overflow_clip: Option<[f32; 4]>,
    /// Absolute / z-index / stacking-context slot that opened a later page-stack
    /// band. Dest-reset of this slot composites over in-flow content: do not
    /// shrink the wipe away from overlapping underlay, and restore that underlay
    /// in the hole (in-flow Node dest-reset stays membership-only).
    pub layered: bool,
    /// Untransformed Taffy box in paint space (no CSS scale / translate).
    /// Sibling occupancy for dest-reset uses this so ping/bounce growth cannot
    /// treat a caption above as an already-overlapping earlier sibling.
    pub rest: Option<[f32; 4]>,
}

/// Dest-reset / scissor clip for this overflow box.
///
/// scale¹ bakes CSS `scale` into `vis` and leaves the affine identity. The rest-sized
/// layout border then AABB-crops hover:scale Twemoji (screenshot chords) while the
/// unscaled capsule stays intact. Rotated overflow still uses the transformed AABB.
fn overflow_dest_aabb(
    layout_border: [f32; 4],
    vis: [f32; 4],
    transform: &crate::style::Mat2d,
) -> [f32; 4] {
    let _ = layout_border;
    if crate::style::mat2d_is_identity(transform) {
        vis
    } else {
        crate::style::aabb_of_centered_transform(vis, transform)
    }
}

thread_local! {
    static NODE_PAINTS: std::cell::RefCell<Vec<NodePaint>> = std::cell::RefCell::new(Vec::new());
}

fn begin_node_paint_collect() {
    NODE_PAINTS.with(|s| s.borrow_mut().clear());
}

fn take_node_paints() -> Vec<NodePaint> {
    NODE_PAINTS.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

fn record_node_paint(
    node: NodeId,
    el: &Element,
    start: crate::layout::collect_cache::DlCursor,
    instances: &[InstanceInput],
    textured: &[TexturedDraw],
    text: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    paint_ops: &[PaintOp],
    overlay: bool,
    scroll_acc: [f32; 2],
    overflow_clip: Option<[f32; 4]>,
    layered: bool,
) {
    // Same space as collect instances / chrome dirties: visual, not Taffy document.
    let paint_scroll = if escapes_ancestor_overflow(el) {
        [0.0_f32, 0.0]
    } else {
        scroll_acc
    };
    let b = crate::core::tree::visual_paint_aabb(el, paint_scroll);
    if b[2] <= 0.0 && b[3] <= 0.0 {
        return;
    }
    let end = crate::layout::collect_cache::DlCursor::capture(
        instances,
        textured,
        text,
        chrome,
        &[],
        paint_ops,
    );
    NODE_PAINTS.with(|s| {
        s.borrow_mut().push(NodePaint {
            key: u64::from(node),
            bounds: b,
            overlay,
            instance: (start.instances, end.instances),
            textured: (start.textured, end.textured),
            text: (start.text, end.text),
            chrome: (start.chrome, end.chrome),
            paint_ops: (start.paint_ops, end.paint_ops),
            overflow_clip,
            layered,
            rest: {
                let b = el.layout_bounds;
                let r = [
                    b[0] - paint_scroll[0],
                    b[1] - paint_scroll[1],
                    b[2].max(0.0),
                    b[3].max(0.0),
                ];
                (r[2] > 0.5 && r[3] > 0.5).then_some(r)
            },
        });
    });
}

impl OverlayPaintSplit {
    /// True once collect entered [`PORTAL_LAYER_ID`]. Fingerprint skip of an ancestor
    /// must not leave this false while overlay rects still sit in the DL.
    pub fn is_marked(&self) -> bool {
        self.marked
    }

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
                    underlay_opacity: 1.0,
                    stamp_key: 0,
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
                backdrop_blur: b.backdrop_blur,
                underlay_opacity: b.underlay_opacity,
                stamp_key: b.stamp_key,
            });
        }
        out
    }

    /// Expand [`Self::page_stack_bands`] into `[start, end)` ranges ending at the portal
    /// overlay split ([`Self::instance_start`] etc.). Prepends a synthetic band from buffer
    /// origin when the first stack child is not at index 0 (page chrome before the context).
    pub fn page_stack_band_ranges(&self) -> Vec<OverlayBandRange> {
        if self.page_stack_bands.is_empty() {
            return Vec::new();
        }
        let page_end = OverlayPaintBand {
            instance_start: self.instance_start,
            textured_start: self.textured_start,
            text_start: self.text_start,
            chrome_start: self.chrome_start,
            paint_op_start: self.paint_op_start,
            backdrop_blur: 0.0,
            underlay_opacity: 1.0,
            stamp_key: 0,
        };
        let mut starts = Vec::with_capacity(self.page_stack_bands.len() + 1);
        let first = self.page_stack_bands[0];
        if first.instance_start > 0
            || first.textured_start > 0
            || first.text_start > 0
            || first.chrome_start > 0
            || first.paint_op_start > 0
        {
            starts.push(OverlayPaintBand::default());
        }
        starts.extend_from_slice(&self.page_stack_bands);
        let mut out = Vec::with_capacity(starts.len());
        for (i, b) in starts.iter().enumerate() {
            let next = starts.get(i + 1).copied().unwrap_or(page_end);
            out.push(OverlayBandRange {
                instance: (b.instance_start, next.instance_start),
                textured: (b.textured_start, next.textured_start),
                text: (b.text_start, next.text_start),
                chrome: (b.chrome_start, next.chrome_start),
                paint_ops: (b.paint_op_start, next.paint_op_start),
                backdrop_blur: b.backdrop_blur,
                underlay_opacity: b.underlay_opacity,
                stamp_key: b.stamp_key,
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
    /// Per-SDF-instance polygon clip (even-odd). Length matches [`Self::instances`].
    pub clip_polys: Vec<crate::style::GpuClipPoly>,
    /// Per-element collect slots for node present (no cross-node AABB cull).
    pub node_paints: Vec<NodePaint>,
}

/// Post-layout geometry/style snapshot for one text run (consumed by glyphon).
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
    /// shadcn `shimmer` sweep (present when [`Self::glyph_fx`] is `shimmer`).
    pub shimmer: Option<crate::renderer::ShimmerParams>,
    /// CSS/`ancestor_scale` paint scale for glyphon `TextArea.scale` (default 1).
    /// Keep [`Self::style`] font size at layout size so drag/coverflow does not reshape every frame.
    pub paint_scale: f32,
    /// World 2×2 linear transform for glyphon (`[a,b,c,d,0,0]`). Identity when axis-aligned.
    pub paint_transform: crate::style::Mat2d,
    /// Pivot for [`Self::paint_transform`] in screen/logical px (typically text box center).
    pub paint_origin: [f32; 2],
    /// Coverage / Bake / Msdf — see [`crate::renderer::GlyphRasterMode`].
    pub glyph_raster: crate::renderer::GlyphRasterMode,
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
    /// Test spy: Taffy measure-function invocations in the last compute (text leaves).
    #[cfg(test)]
    pub(crate) text_measure_calls: std::cell::Cell<u32>,
    /// Test spy: [`Self::build_and_layout`] invocations (full tree walk).
    #[cfg(test)]
    pub(crate) full_rebuilds: u32,
    /// Frame-local collect output; `std::mem::take` hands buffers to [`RenderResult`].
    collect_scratch: CollectScratch,
    /// Cross-frame fingerprint DL for paint-only sibling skip.
    collect_skip: crate::layout::collect_cache::CollectSkipState,
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

    /// Paint-only (`Clean` / `StyleChanged`) frames may replay fingerprint DL for clean siblings.
    /// Layout/structure, live resize, and text settle must pass `false`. Smooth scroll stays
    /// `true`: fully-visible scrolled subtrees translate last-frame ops; idle siblings replay.
    pub fn set_collect_skip_allowed(&mut self, allowed: bool) {
        self.collect_skip.allowed = allowed;
    }

    /// First frame of a CSS transform tween under an idle wrapper: ViewportsOnly
    /// `reuse_idle` would keep the rest paint-hash and skip-replay the rest Bake.
    pub(crate) fn note_collect_skip_hash_bust_if_idle(&mut self, already_live: bool) {
        if !already_live {
            self.collect_skip.hash_bust = true;
        }
    }

    /// Drop fingerprint DLs (maximize/restore, full Taffy rebuild).
    pub fn invalidate_collect_skip(&mut self) {
        self.collect_skip.invalidate();
    }

    /// `(skipped_siblings, patched_dl)` from the last collect.
    pub fn last_collect_skip_counts(&self) -> (u32, u32) {
        (self.collect_skip.skipped, self.collect_skip.patched)
    }

    /// Taffy ids collect skipped as fully outside ancestor overflow (this frame).
    pub fn last_clip_culled_paint_keys(&self) -> &HashSet<u64> {
        &self.collect_skip.clip_culled
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
    /// Native `<input>` / Combobox: crop + caret pan. Placeholder/value glyphs must
    /// not become flex max-content (gallery wrap row reflowed after typing).
    caret_scroll: bool,
    writing_mode: crate::style::WritingMode,
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

/// Extra clip height so CSS underline-offset / overline can paint past the glyph box.
fn text_decoration_clip_outset(style: &crate::style::TextStyle) -> f32 {
    if !style.underline_resolved() && !style.overline_resolved() {
        return 0.0;
    }
    let thickness = {
        let t = style.decoration_thickness_resolved();
        if t > 0.0 {
            t
        } else {
            (style.size_resolved() * 0.07).max(1.0)
        }
    };
    style.underline_offset_resolved().max(0.0) + thickness + 1.0
}

fn text_leaf_clip_rect(render_rect: [f32; 4], style: &crate::style::TextStyle) -> [f32; 4] {
    let deco = text_decoration_clip_outset(style);
    let mut r = render_rect;
    // `text-overflow: clip|ellipsis` scissors the inline axis. Glyph ink may
    // overflow the line box (`leading-none` + CJK / descenders) — same as CSS
    // `overflow-y: visible`. Do not use the 1em strut as a vertical scissor.
    if matches!(
        style.overflow,
        crate::style::TextOverflow::Clip | crate::style::TextOverflow::Ellipsis
    ) {
        let ink = style.size_resolved().max(deco);
        r[1] -= ink;
        r[3] += ink * 2.0;
        return r;
    }
    if deco <= 0.0 {
        return render_rect;
    }
    if style.underline_resolved() {
        r[3] += deco;
    }
    if style.overline_resolved() {
        r[1] -= deco;
        r[3] += deco;
    }
    r
}

fn text_measure_meta(element: &Element, text: String, is_icon: bool) -> TextMeasureMeta {
    TextMeasureMeta {
        size: element.text_style.size_resolved(),
        text,
        shape_suffix: element.text_shape_suffix.clone(),
        is_icon,
        line_height_px: element.text_style.line_height_px,
        line_height_scale: element.text_style.line_height_scale,
        weight: element.text_style.weight_resolved(),
        letter_spacing: element.text_style.letter_spacing,
        spans: element.text_spans.clone(),
        nowrap: element.text_style.white_space == crate::style::WhiteSpace::Nowrap,
        clip_to_box: element_text_clips_to_box(element),
        caret_scroll: element.text_style.overflow.allows_caret_scroll(),
        writing_mode: element.style.writing_mode,
    }
}

/// Per-`compute_layout` memo for `measured_text_leaf_size` (Taffy probes the same leaf many times).
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

fn element_tree_has_deferred_units(root: &Element) -> bool {
    let mut stack = vec![root];
    while let Some(el) = stack.pop() {
        if el.style.has_deferred_units() {
            return true;
        }
        stack.extend(el.children.iter());
    }
    false
}

#[inline]
fn pack_avail_space(d: taffy::style::AvailableSpace) -> u64 {
    match d {
        taffy::style::AvailableSpace::Definite(v) => {
            // High 2 bits = tag; low 62 bits = f32::to_bits (huge exponents are rare in layout).
            (3u64 << 62) | ((v.to_bits() as u64) & ((1u64 << 62) - 1))
        }
        taffy::style::AvailableSpace::MinContent => 1u64 << 62,
        taffy::style::AvailableSpace::MaxContent => 2u64 << 62,
    }
}

/// Taffy 0.13's flex/grid/block algorithms recurse through `compute_child_layout`.
/// That is algorithmic (parent needs child sizes mid-pass), not a tree walk we can
/// turn into a heap queue without forking Taffy. Keep the deep-chain stack test at
/// 128 wrappers until an upstream iterative compute exists. Do not grow the PE stack.
#[inline(never)]
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
    if m.writing_mode.is_vertical() {
        let inner = measured_text_leaf_size_horizontal(
            m,
            taffy::geometry::Size {
                width: known_dimensions.height,
                height: known_dimensions.width,
            },
            taffy::geometry::Size {
                width: available_space.height,
                height: available_space.width,
            },
        );
        return taffy::geometry::Size {
            width: inner.height,
            height: inner.width,
        };
    }
    measured_text_leaf_size_horizontal(m, known_dimensions, available_space)
}

fn measured_text_leaf_size_horizontal(
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
        let intrinsic_w = crate::text::snap_content_text_width(intrinsic_w);
        let min_content = if m.caret_scroll {
            0.0
        } else if m.nowrap {
            intrinsic_w
        } else {
            crate::text::snap_content_text_width(prepared.max_segment_width.min(intrinsic_w))
        };
        let width = if m.clip_to_box {
            known_dimensions
                .width
                .or(avail_width)
                .unwrap_or(if m.caret_scroll { 0.0 } else { intrinsic_w })
                .max(if m.caret_scroll { 0.0 } else { 1.0 })
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
                .unwrap_or(if m.caret_scroll { 0.0 } else { measured_w })
                .max(if m.caret_scroll { 0.0 } else { 1.0 })
        } else if m.caret_scroll {
            known_dimensions.width.or(avail_width).unwrap_or(0.0)
        } else {
            measured_w
        };
        (width, height)
    };
    // Never raise height from known_dimensions: flex Stretch would inflate text leaves to
    // the ScrollArea/Textarea viewport, so leaf (and soft_h.max(scaled_h)) tracked chrome
    // resize instead of content — gallery "text render height grows with drag".
    // Definite `style.size_resolved().height` is applied by Taffy via node_size, not this max().
    let _ = known_dimensions.height;
    taffy::geometry::Size { width, height }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Geometry/paint snapshot returned by the non-recursive paint helper.
/// Kept small so the recursive child walk does not inherit the paint function's
/// MSVC-debug frame (Windows PE default stack is ~1 MiB).
struct CollectPainted {
    abs_x: f32,
    abs_y: f32,
    lay_w: f32,
    lay_h: f32,
    opacity: f32,
    clip: Option<[f32; 4]>,
    parent_clip: Option<[f32; 4]>,
    /// Ancestor overflow in paint space (AABB of transformed local box).
    dest_clip: Option<[f32; 4]>,
    clip_poly: crate::style::GpuClipPoly,
    render_rect: [f32; 4],
    visible_rect: [f32; 4],
    paint_sx: f32,
    paint_sy: f32,
    world_linear: crate::style::Mat2d,
    local_linear: crate::style::Mat2d,
    /// CSS `transform-origin` in layout-box px (from top-left). Child orbit uses this.
    origin_layout: [f32; 2],
    transform: crate::style::Mat2d,
    /// CSS `transform` list translation (not origin wrap). Child `parent_tl` adds this.
    list_t: [f32; 2],
    filter_capture: bool,
    filter_cap_inst0: usize,
    filter_cap_tex0: usize,
    /// Dialog `__modal_zoom` CSS scale: capture at rest, blit scaled. `None` when idle.
    scale_layer: Option<[f32; 2]>,
    scale_cap_inst0: usize,
    scale_cap_tex0: usize,
    scale_cap_text0: usize,
    /// Rest layout border box (not CSS-scaled `render_rect`) for zoom capture dest.
    layout_border: [f32; 4],
    is_body_portal_layer: bool,
    thumb_rect_for_child: Option<[f32; 4]>,
    layout_size: taffy::geometry::Size<f32>,
    scroll_acc: [f32; 2],
}

enum CollectPaintOutcome {
    Done([f32; 2]),
    Ready(CollectPainted),
}

/// Heap-stack state for walking children after a node has been painted.
struct CollectWalk {
    painted: CollectPainted,
    child_nodes: Vec<NodeId>,
    cursor: usize,
    z_order: Option<Vec<usize>>,
    max_right: f32,
    max_bottom: f32,
    next_scroll_acc: [f32; 2],
    pass_to_children: Option<String>,
    ancestor_scroll_viewport_rest: Option<String>,
    record_page_stack_bands: bool,
    /// Nested stacking-context: only `child_opens_page_stack_band` children get a band.
    stack_band_only_layered: bool,
    cap_children: bool,
    cap_inst0: usize,
    cap_tex0: usize,
    cap_text0: usize,
    parent_tl: [f32; 2],
    child_ancestor_scale: [f32; 2],
    child_ancestor_linear: crate::style::Mat2d,
    child_sticky_xform: bool,
    parent_scales: bool,
    parent_rotates: bool,
    parent_translates: bool,
    is_carousel_track: bool,
    skip_scroll_band_cull: bool,
    any_stacking: bool,
    opacity: f32,
    clip: Option<[f32; 4]>,
    /// Dest-reset / scissor clip passed to children (paint AABB, not GPU layout).
    dest_clip: Option<[f32; 4]>,
    /// Clip passed to children. CSS `filter` SourceGraphic must not inherit ancestor
    /// overflow — only this node's own overflow (if any).
    child_clip_poly: crate::style::GpuClipPoly,
    stopped: bool,
}

struct CollectChildJob {
    index: usize,
    node: NodeId,
    abs_offset: [f32; 2],
    thumb_ov: Option<[f32; 4]>,
}

enum CollectWalkStart {
    Done([f32; 2]),
    Ready(CollectWalk),
}

struct CollectFrame {
    element: *mut Element,
    node: NodeId,
    abs_offset: [f32; 2],
    scroll_acc: [f32; 2],
    parent_clip: Option<[f32; 4]>,
    parent_dest_clip: Option<[f32; 4]>,
    parent_clip_poly: crate::style::GpuClipPoly,
    ancestor_scroll_viewport_rest: Option<String>,
    ancestor_opacity: f32,
    ancestor_scale: [f32; 2],
    ancestor_linear: crate::style::Mat2d,
    ancestor_sticky_xform: bool,
    scrollbar_overlay_collect: bool,
    thumb_geometry_override: Option<[f32; 4]>,
    walk: Option<CollectWalk>,
    cache_start: crate::layout::collect_cache::DlCursor,
    cache_geom: crate::layout::collect_cache::GeomKey,
    /// This node (or an ancestor) opened a later page-stack band.
    layered: bool,
    /// Host portal stamp while walking the overlay suffix. Direct portal-layer
    /// children get their own key; nested descendants inherit it.
    overlay_stamp: Option<u64>,
}

fn collect_commit_extent(
    stack: &mut Vec<CollectFrame>,
    extent: [f32; 2],
    root_extent: &mut [f32; 2],
) {
    if let Some(parent) = stack.last_mut() {
        if let Some(walk) = parent.walk.as_mut() {
            walk.max_right = walk.max_right.max(extent[0]);
            walk.max_bottom = walk.max_bottom.max(extent[1]);
        }
    } else {
        *root_extent = extent;
    }
}

impl CollectWalk {
    fn child_pass_offset_for(&self, clx: f32, cly: f32, cw: f32, ch: f32) -> [f32; 2] {
        if !self.parent_scales && !self.parent_rotates {
            return self.parent_tl;
        }
        // Scale and rotate share CSS origin (vis bake + VS txty). Center origin
        // is the old "about layout center" path.
        let ox = self.painted.origin_layout[0];
        let oy = self.painted.origin_layout[1];
        let mut dx = (clx + cw * 0.5 - ox) * self.painted.paint_sx;
        let mut dy = (cly + ch * 0.5 - oy) * self.painted.paint_sy;
        if self.parent_rotates {
            let (rx, ry) = crate::style::mat2d_transform_vec(&self.painted.local_linear, dx, dy);
            dx = rx;
            dy = ry;
        }
        let pox = self.parent_tl[0] + ox;
        let poy = self.parent_tl[1] + oy;
        [pox + dx - cw * 0.5 - clx, poy + dy - ch * 0.5 - cly]
    }
}

impl LayoutTree {
    /// Shared Taffy measure callback wrapper to avoid repeating closure
    /// monomorphizations at multiple compute callsites.
    ///
    /// Taffy's `compute_layout_with_measure` still recurses with tree depth
    /// (flex/grid/block). Rounding is disabled here; `round_layout` is not on
    /// this path. Deep-chain tests stay at 128 wrappers so one ~1 MiB segment
    /// covers Taffy's frames; [`with_ui_build_stack`] only grows if the PE
    /// thread is already inside the 64 KiB red zone (GPUI `stacksafe` analog).
    #[inline(never)]
    fn compute_layout_cached_measure(
        &mut self,
        node: NodeId,
        available_space: Size<AvailableSpace>,
    ) -> Result<(), taffy::TaffyError> {
        crate::core::reactivity::with_ui_build_stack(|| {
            let tm = &self.text_measurements;
            let mcache = &self.measure_layout_cache;
            #[cfg(test)]
            self.text_measure_calls.set(0);
            #[cfg(test)]
            let measure_calls = &self.text_measure_calls;
            self.taffy.compute_layout_with_measure(
                node,
                available_space,
                |known_dimensions, available_space, node_id, _context, _style| {
                    #[cfg(test)]
                    if tm.contains_key(&node_id) {
                        measure_calls.set(measure_calls.get().saturating_add(1));
                    }
                    measure_leaf_cached(mcache, tm, node_id, known_dimensions, available_space)
                },
            )
        })
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
            #[cfg(test)]
            text_measure_calls: std::cell::Cell::new(0),
            #[cfg(test)]
            full_rebuilds: 0,
            collect_scratch: CollectScratch::default(),
            collect_skip: crate::layout::collect_cache::CollectSkipState::default(),
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
        UnitCtx::new(
            self.viewport[0],
            self.viewport[1],
            element.text_style.size_resolved(),
        )
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

    /// Parent already has a Taffy id, a child does not — fast path cannot
    /// measure that child (`sync_taffy_children_from_elements` skips `None`).
    fn subtree_has_unbuilt_taffy_nodes(root: &Element) -> bool {
        let mut stack = vec![root];
        while let Some(el) = stack.pop() {
            if el.layout_node.is_some() && el.children.iter().any(|c| c.layout_node.is_none()) {
                return true;
            }
            for c in el.children.iter().rev() {
                stack.push(c);
            }
        }
        false
    }

    /// Create Taffy nodes only for children that have no `layout_node` yet.
    fn attach_unbuilt_taffy_nodes(&mut self, root: &mut Element) {
        let mut stack: Vec<*mut Element> = vec![root as *mut Element];
        while let Some(ptr) = stack.pop() {
            let el = unsafe { &mut *ptr };
            if el.layout_node.is_some() {
                let ctx = self.unit_ctx_from_parent_layout(el);
                let (child_cb_w, child_cb_h) = el.style.estimate_content_box(&ctx);
                let child_parent_dir = matches!(el.style.display, Display::Flex).then_some(
                    el.style
                        .writing_mode
                        .map_flex_direction(el.style.direction, el.style.flex_direction),
                );
                for i in 0..el.children.len() {
                    if el.children[i].layout_node.is_none() {
                        self.build_taffy_node_mut(
                            &mut el.children[i],
                            child_cb_w,
                            child_cb_h,
                            child_parent_dir,
                        );
                    }
                }
            }
            for c in el.children.iter_mut().rev() {
                stack.push(c as *mut Element);
            }
        }
    }

    /// Keep Taffy child order in lockstep with `element.children`.
    ///
    /// Keyed list reorder reuses the same nodes (`LayoutChanged`, not
    /// `StructureChanged`). [`Self::build_and_layout_fast_ex`] never rebuilds the
    /// Taffy tree, so without this walk `compute_layout` packs the previous order.
    ///
    /// Cross-parent moves (kanban / sortable) keep the child's `layout_node`. A
    /// parent-local subtree drop would free that SlotMap key before the
    /// destination `set_children` — Taffy panics: `invalid SlotMap key used`.
    /// Reparent every live id first, then drop nodes the element tree no longer
    /// references.
    pub(crate) fn sync_taffy_children_from_elements(&mut self, root: &Element) {
        let mut live = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(el) = stack.pop() {
            if let Some(nid) = el.layout_node {
                if self.total_nodes.contains(&nid) {
                    live.insert(nid);
                }
            }
            for c in el.children.iter().rev() {
                stack.push(c);
            }
        }

        stack = vec![root];
        while let Some(el) = stack.pop() {
            if let Some(nid) = el.layout_node {
                if self.total_nodes.contains(&nid) {
                    let child_nodes: Vec<NodeId> = el
                        .children
                        .iter()
                        .filter_map(|c| c.layout_node.filter(|id| live.contains(id)))
                        .collect();
                    let kids_changed = self
                        .taffy
                        .children(nid)
                        .ok()
                        .map_or(true, |old| old.as_slice() != child_nodes.as_slice());
                    if kids_changed {
                        let _ = self.taffy.set_children(nid, &child_nodes);
                    }
                }
            }
            for c in el.children.iter().rev() {
                stack.push(c);
            }
        }

        let orphans: Vec<NodeId> = self
            .total_nodes
            .iter()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        for id in orphans {
            if !self.total_nodes.contains(&id) {
                continue;
            }
            // Do not recurse. `new_with_children` / hoist can leave a live id in
            // an orphan's child list; subtree-delete would free that SlotMap key
            // while another parent still walks it (`invalid SlotMap key used`).
            // Taffy `remove` unparents remaining children without deleting them.
            self.collect_skip.evict(id);
            let _ = self.taffy.remove(id);
            self.total_nodes.remove(&id);
            self.text_measurements.remove(&id);
            self.visited_nodes.remove(&id);
        }
    }

    pub fn update_taffy_node_style(&mut self, element: &Element) {
        if let Some(node_id) = element.layout_node {
            let style = element.style.to_taffy_style_with_parent(
                &self.unit_ctx_from_parent_layout(element),
                self.parent_flex_direction(element),
            );
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

    /// Mouseup Point-drag commit: Taffy still caches the grab-origin location.
    fn dirty_point_overlay_commit(&mut self, root: &Element) {
        let Some(id) = crate::utils::anchored_overlay::take_point_overlay_commit_dirty() else {
            return;
        };
        let mut ptrs: Vec<*const Element> = Vec::new();
        crate::core::tree::walk_elements(root, |el| {
            if el
                .attrs
                .get(crate::utils::popup::ATTR_FLOATING)
                .map(String::as_str)
                == Some(id.as_str())
            {
                ptrs.push(el as *const Element);
            }
        });
        for p in ptrs {
            // SAFETY: `root` outlives this loop; walk already finished.
            self.mark_element_layout_dirty(unsafe { &*p });
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
        for_each_element(root, |el| {
            if let Some(n) = el.layout_node {
                if dirty.contains(&n) {
                    self.mark_element_layout_dirty(el);
                }
            }
        });
    }

    /// Mark every text leaf dirty so `compute_layout` re-runs measure (FIT_CACHE / settle).
    ///
    /// Needed when available space is unchanged but soft-wrap height must grow after
    /// `glyphon_fit` (paint spill → `note_fit_needs_relayout`, or post-resize settle).
    /// Without this, Taffy keeps the previous one-line cache and labels stay clipped / stacked.
    pub fn mark_all_text_leaves_dirty(&mut self, root: &Element) {
        for_each_element(root, |el| {
            if el.text.is_some() && el.children.is_empty() && el.layout_node.is_some() {
                self.mark_element_layout_dirty(el);
            }
        });
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
        let _ = crate::utils::anchored_overlay::take_point_overlay_commit_dirty();
        // Full rebuild may recycle Taffy `NodeId`s; fingerprint keys would alias.
        self.collect_skip.invalidate();
        #[cfg(test)]
        {
            self.full_rebuilds = self.full_rebuilds.saturating_add(1);
        }

        let t_rebuild_begin = Instant::now();
        self.viewport = [width.max(0.0), height.max(0.0)];
        root.style.size.width = Dimension::length(width);
        root.style.size.height = Dimension::length(height);
        cascade_inheritable_text_style(root, None);

        // Keep prior text-measure metas. Clearing here forced every portal hoist
        // (Dialog open) to remasure the whole gallery. Unvisited nodes are pruned below.
        self.visited_nodes.clear();
        // DO NOT clear taffy anymore; we rely on reconciliation and selective removal.
        let root_node = self.build_taffy_node_mut(root, Some(width), Some(height), None);
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

        self.apply_floating_placement_before_collect(root, width, height);

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
    /// — avoids O(nested) full layout each frame when many `ScrollArea`s are nested.
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

    /// Taffy node of the root [`PORTAL_LAYER_ID`] layer, if present.
    pub fn portal_layer_layout_node(root: &Element) -> Option<NodeId> {
        root.children.iter().find_map(|c| {
            if c.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
                c.layout_node
            } else {
                None
            }
        })
    }

    /// Hoist + attach newly inserted Taffy nodes + sync child order.
    ///
    /// A fresh `rui_portal` used to return `>0` and force [`Self::build_and_layout`]
    /// of the whole gallery (bubble +N during scroll). Layer membership is a
    /// child-list change on the portal layer — [`Self::sync_taffy_children_from_elements`]
    /// reparents those ids without recycling page `NodeId`s.
    fn reconcile_portals_incremental(&mut self, root: &mut Element) -> usize {
        let portal_dirty = hoist_portal_subtrees(root);
        self.dirty_point_overlay_commit(root);
        let attached = Self::subtree_has_unbuilt_taffy_nodes(root);
        if attached {
            self.attach_unbuilt_taffy_nodes(root);
        }
        if portal_dirty > 0 || attached {
            self.sync_taffy_children_from_elements(root);
        }
        portal_dirty
    }

    fn portal_layer_available_space(
        &self,
        overlay_root: NodeId,
        root_node: NodeId,
        width: f32,
        height: f32,
    ) -> (NodeId, Size<AvailableSpace>) {
        if overlay_root != root_node {
            if let Ok(layout) = self.taffy.layout(overlay_root) {
                return (
                    overlay_root,
                    Size {
                        width: AvailableSpace::Definite(layout.size.width),
                        height: AvailableSpace::Definite(layout.size.height),
                    },
                );
            }
        }
        (
            root_node,
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
        )
    }

    /// Compute the portal layer (or app root if the layer has no node) without
    /// clearing the page measure cache.
    fn compute_portal_layer(&mut self, root: &mut Element, width: f32, height: f32) {
        let Some(root_node) = self.root_node else {
            return;
        };
        let overlay_root = Self::portal_layer_layout_node(root).unwrap_or(root_node);
        let (compute_node, available_space) =
            self.portal_layer_available_space(overlay_root, root_node, width, height);
        #[cfg(test)]
        {
            self.last_compute_root = Some(compute_node);
        }
        if let Err(err) = self.compute_layout_cached_measure(compute_node, available_space) {
            log::error!("taffy compute_layout_with_measure failed: {err:?}");
        }
        let _ = self.relayout_deferred_units_if_needed(root, compute_node, available_space);
    }

    /// `sync_floating_placement` writes left/top from trigger visual bounds.
    /// Must run after Taffy (layout_bounds exist) and **before** collect so a
    /// scroll frame with +N open does not collect the gallery twice.
    fn apply_floating_placement_before_collect(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
    ) {
        if !crate::utils::popup::sync_floating_placement(root, width, height) {
            return;
        }
        crate::utils::popup::visit_floating_panels(root, |el| {
            self.mark_element_layout_dirty(el);
        });
        self.compute_portal_layer(root, width, height);
    }

    /// After `sync_floating_placement` writes left/top: recompute only the portal
    /// layer and recollect. Must not restyle the app root or clear the page
    /// measure cache — that remasures every gallery text leaf each scroll frame
    /// while a +N / DatePicker panel follows its trigger.
    pub fn relayout_overlay_follow_and_collect(
        &mut self,
        root: &mut Element,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        collect_scroll_only: bool,
        scrollbar_overlay_collect: bool,
    ) -> RenderResult {
        if !self.element_tree_is_bound(root) {
            return self.build_and_layout(root, width, height, mouse_pos);
        }
        let _ = self.reconcile_portals_incremental(root);

        let root_node = self
            .root_node
            .expect("overlay follow requires existing root_node");
        let overlay_root = Self::portal_layer_layout_node(root).unwrap_or(root_node);

        let t_setup_begin = Instant::now();
        let next_viewport = [width.max(0.0), height.max(0.0)];
        let viewport_changed = self.viewport != next_viewport;
        if viewport_changed {
            self.collect_skip.invalidate();
            self.viewport = next_viewport;
            root.style.size.width = Dimension::length(width);
            root.style.size.height = Dimension::length(height);
            cascade_inheritable_text_style(root, None);
            self.update_taffy_node_style(root);
        }
        let setup_ms = t_setup_begin.elapsed().as_secs_f32() * 1000.0;

        let (compute_node, available_space) =
            self.portal_layer_available_space(overlay_root, root_node, width, height);

        let t_compute_begin = Instant::now();
        // Keep the page measure memo. Overlay follow only dirtied portal panels.
        #[cfg(test)]
        {
            self.last_compute_root = Some(compute_node);
        }
        if let Err(err) = self.compute_layout_cached_measure(compute_node, available_space) {
            log::error!("taffy compute_layout_with_measure failed: {err:?}");
        }
        let relayout_ms =
            self.relayout_deferred_units_if_needed(root, compute_node, available_space);
        let taffy_compute_ms = t_compute_begin.elapsed().as_secs_f32() * 1000.0 + relayout_ms;

        let mut scroll_signal_cache: HashMap<String, ScrollAreaSignalsCached> = HashMap::new();
        let mut scroll_cache_slot: Option<&mut HashMap<String, ScrollAreaSignalsCached>> =
            if collect_scroll_only || scrollbar_overlay_collect {
                Some(&mut scroll_signal_cache)
            } else {
                None
            };

        self.finish_collect(
            root,
            root_node,
            mouse_pos,
            collect_scroll_only,
            scrollbar_overlay_collect,
            &mut scroll_cache_slot,
            0.0,
            setup_ms,
            taffy_compute_ms,
        )
    }

    /// True when `root` is the same Taffy tree `self.root_node` currently owns.
    ///
    /// `new_bootstrap` layouts a private shell, then `rebuild_ui` swaps in the
    /// real app tree (`layout_node = None`). Incremental sync would treat every
    /// bootstrap id as an orphan, free them, then `compute_layout` panics
    /// `invalid SlotMap key used`.
    fn element_tree_is_bound(&self, root: &Element) -> bool {
        let Some(rid) = self.root_node else {
            return false;
        };
        matches!(root.layout_node, Some(eid) if eid == rid && self.total_nodes.contains(&rid))
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
        if !self.element_tree_is_bound(root) {
            return self.build_and_layout(root, width, height, mouse_pos);
        }
        // Fresh `rui_portal` (bubble +N) rewrites layer children. Sync those ids
        // onto the existing Taffy tree — a full `build_and_layout` remasures the
        // gallery on every scroll-frame open.
        let portal_dirty = self.reconcile_portals_incremental(root);

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
        if viewport_changed {
            // Cached DLs store absolute rects. Replaying them after maximize/restore
            // leaves glyphs at the old Y (shimmer label ghosting onto the next section)
            // and freezes time-sampled overlays until a scroll_only frame.
            self.collect_skip.invalidate();
        }
        if !scroll_only || viewport_changed {
            self.viewport = next_viewport;
            root.style.size.width = Dimension::length(width);
            root.style.size.height = Dimension::length(height);
            cascade_inheritable_text_style(root, None);
            self.update_taffy_node_style(root);
        }
        let setup_ms = t_setup_begin.elapsed().as_secs_f32() * 1000.0;

        // Full rebuild walks `build_taffy_node_mut` and `set_children` when the
        // element child order changes. The fast path only `compute_layout`s the
        // existing Taffy tree — keyed reorder is LayoutChanged, not
        // StructureChanged, so without this sync flex items keep their old slots.
        if !scroll_only || portal_dirty > 0 {
            self.sync_taffy_children_from_elements(root);
        }

        // Live maximize/minimize is often `Clean` + `scroll_only` (ViewportsOnly prepass).
        // Skipping compute here leaves Taffy at the pre-resize locations: a flex Footer
        // stays at the old Y while the swapchain already matches the new window.
        // Portal-only membership ( +N open while scrolling ) computes the overlay
        // layer without clearing the page measure cache.
        let taffy_compute_ms = if portal_dirty > 0 && scroll_only && !viewport_changed {
            let t_compute_begin = Instant::now();
            self.compute_portal_layer(root, width, height);
            t_compute_begin.elapsed().as_secs_f32() * 1000.0
        } else if !scroll_only || viewport_changed {
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

        self.apply_floating_placement_before_collect(root, width, height);

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
        begin_node_paint_collect();
        let mut hover_cursor = None;
        let mut overlay_split = OverlayPaintSplit::default();
        let mut paint_rec = PaintRecorder::default();

        // Taffy has assigned `layout_node`s. Jump follow *before* collect so this frame
        // paints the new live edge. AppState used to run this after collect; the offset
        // signal moved but paint stayed on the old offset, and `request_redraw` is a
        // no-op while `AppState` is mutably borrowed for render.
        crate::components::message_scroller::sync_message_scroller_follow_after_layout(root, self);
        // Wheel / autoscroll chase offset after the vsync drag `tick`. Hidden
        // clip shells must follow the live parent before this collect, or the
        // ghost paints in the pre-scroll restrict box (突破父容器).
        if crate::utils::drag::is_active() {
            crate::utils::drag::session::resync_overlay_follow_after_layout(root);
            crate::utils::drag::overlay::sync_overlay_ghost_insets(root);
        }

        let t_collect_begin = Instant::now();
        #[cfg(feature = "devtools")]
        {
            crate::app::devtools::begin_highlight_collect();
            crate::app::devtools::set_highlight_collect_root(root);
        }
        let mut skip = std::mem::take(&mut self.collect_skip);
        skip.begin_frame();
        let t_stamp = Instant::now();
        let reuse_idle = scroll_only && !std::mem::take(&mut skip.hash_bust);
        crate::layout::collect_cache::stamp_subtree_paint_hashes(root, reuse_idle);
        let collect_stamp_ms = t_stamp.elapsed().as_secs_f32() * 1000.0;
        self.collect_instances_mut(
            root,
            root_node,
            [0.0, 0.0],
            [0.0, 0.0],
            None,
            None,
            mouse_pos,
            1.0,
            [1.0, 1.0],
            crate::style::MAT2D_IDENTITY,
            false,
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
            &mut scratch.clip_polys,
            &mut skip,
        );
        skip.end_frame();
        crate::app::frame_stats::note_collect_skip(skip.skipped, skip.patched);
        self.collect_skip = skip;
        while scratch.clip_polys.len() < scratch.instances.len() {
            scratch.clip_polys.push(crate::style::GpuClipPoly::NONE);
        }
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

        if crate::utils::drag::debug_drag_hits_enabled() {
            let frames = crate::utils::drag::debug_frames_for_tree(root);
            if !frames.is_empty() {
                paint_rec.split_here();
                let i0 = scratch.instances.len();
                for frame in &frames {
                    let stroke = frame.stroke();
                    let style = if stroke.dashed { 3.0 } else { 1.0 };
                    scratch.instances.push(InstanceInput {
                        rect_pos: [frame.rect[0], frame.rect[1]],
                        rect_size: [frame.rect[2], frame.rect[3]],
                        color: stroke.fill,
                        border_color: stroke.border,
                        radii: [0.0; 4],
                        border_width: stroke.width,
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_spread: 0.0,
                        shadow_inset: 0.0,
                        transform: crate::style::MAT2D_IDENTITY,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: InstanceInput::pack_border_extra(style, 0.0, 0.0),
                        backdrop_blur: 0.0,
                    });
                }
                paint_rec.push_rects(i0, scratch.instances.len() - i0);
                while scratch.clip_polys.len() < scratch.instances.len() {
                    scratch.clip_polys.push(crate::style::GpuClipPoly::NONE);
                }
            }
        }

        if let Some(g) = crate::utils::text_sel_move::ghost_paint() {
            let n_lines = g.text.lines().count().max(1) as f32;
            let lh = g.line_height.max(g.size);
            let prepared = crate::text::prepare(&g.text, g.size, g.weight, lh);
            let w = crate::text::intrinsic_width(&prepared).max(8.0);
            let h = lh * n_lines;
            let mut style = TextStyle::default();
            style.size = Some(g.size);
            style.weight = Some(g.weight);
            style.color = Some(g.color);
            style.line_height_px = Some(lh);
            scratch.text_nodes.push(TextLayoutInfo {
                text: g.text,
                shape_suffix: None,
                spans: None,
                template_id: Some("__rui_text_sel_move_ghost".into()),
                x: g.x,
                y: g.y,
                width: w,
                height: h,
                clip_bounds: None,
                style,
                prepared: Some(prepared),
                glyph_fx: None,
                shimmer: None,
                paint_scale: 1.0,
                paint_transform: crate::style::transform::MAT2D_IDENTITY,
                paint_origin: [g.x + w * 0.5, g.y + h * 0.5],
                glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
            });
        }

        scratch.tex_keys.clear();
        scratch
            .tex_keys
            .extend(scratch.textured_draws.iter().map(|d| d.texture_key.clone()));
        let mut render_graph =
            crate::renderer::RenderGraph::standard_composite(scratch.tex_keys.clone());
        crate::renderer::enrich_render_graph(&mut render_graph);
        crate::renderer::enrich_filter_graph(&mut render_graph);
        crate::primitives::shader::enrich_shader_graph(&mut render_graph);
        crate::primitives::path::enrich_path_graph(&mut render_graph);
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
            clip_polys: std::mem::take(&mut scratch.clip_polys),
            node_paints: take_node_paints(),
            prof: LayoutProfileMs {
                rebuild_ms,
                setup_ms,
                taffy_compute_ms,
                collect_ms,
                collect_stamp_ms,
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

    fn relayout_deferred_units_walk(&mut self, root: &Element, dirty: &mut bool) {
        for_each_element(root, |element| {
            if !element.style.has_deferred_units() {
                return;
            }
            let Some(node_id) = element.layout_node else {
                return;
            };
            let ctx = self.unit_ctx_from_parent_layout(element);
            let new_style = element
                .style
                .to_taffy_style_with_parent(&ctx, self.parent_flex_direction(element));
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
        });
    }

    fn parent_flex_direction(&self, element: &Element) -> Option<FlexDirection> {
        let id = element.layout_node?;
        let pid = self.taffy.parent(id)?;
        let style = self.taffy.style(pid).ok()?;
        matches!(style.display, Display::Flex).then_some(style.flex_direction)
    }

    fn build_taffy_node_mut(
        &mut self,
        element: &mut Element,
        cb_w: Option<f32>,
        cb_h: Option<f32>,
        parent_flex_direction: Option<FlexDirection>,
    ) -> NodeId {
        enum Phase {
            Enter,
            Leave,
        }
        struct Frame {
            element: *mut Element,
            cb_w: Option<f32>,
            cb_h: Option<f32>,
            parent_flex_direction: Option<FlexDirection>,
            phase: Phase,
        }

        let mut stack = vec![Frame {
            element: element as *mut Element,
            cb_w,
            cb_h,
            parent_flex_direction,
            phase: Phase::Enter,
        }];
        let mut root_id: Option<NodeId> = None;

        while let Some(mut frame) = stack.pop() {
            // SAFETY: exclusive subtree walk; Leave runs after all descendants have nodes.
            let element = unsafe { &mut *frame.element };
            match frame.phase {
                Phase::Enter => {
                    #[cfg(test)]
                    stack_probe::mark_build();
                    let ctx = self.unit_ctx_with_cb(element, frame.cb_w, frame.cb_h);
                    let (child_cb_w, child_cb_h) = element.style.estimate_content_box(&ctx);
                    let child_parent_dir = matches!(element.style.display, Display::Flex)
                        .then_some(element.style.writing_mode.map_flex_direction(
                            element.style.direction,
                            element.style.flex_direction,
                        ));
                    frame.phase = Phase::Leave;
                    stack.push(frame);
                    for child in element.children.iter_mut().rev() {
                        stack.push(Frame {
                            element: child as *mut Element,
                            cb_w: child_cb_w,
                            cb_h: child_cb_h,
                            parent_flex_direction: child_parent_dir,
                            phase: Phase::Enter,
                        });
                    }
                }
                Phase::Leave => {
                    let child_nodes: Vec<NodeId> = element
                        .children
                        .iter()
                        .filter_map(|c| c.layout_node)
                        .collect();
                    let ctx = self.unit_ctx_with_cb(element, frame.cb_w, frame.cb_h);
                    let style = element
                        .style
                        .to_taffy_style_with_parent(&ctx, frame.parent_flex_direction);
                    let is_text_leaf = element.text.is_some() && child_nodes.is_empty();

                    let node_id = if let Some(existing_id) = element.layout_node {
                        // SlotMap ids restart at 0 on a new `TaffyTree`. A stale
                        // `layout_node` from a dropped tree can numerically match a
                        // node just created for a *different* element; reusing it
                        // parents two Element nodes onto one Taffy id and
                        // `compute_layout` recurses until the stack dies (Gallery
                        // `card_chip` ghost after `LayoutTree::new()`).
                        if self.total_nodes.contains(&existing_id)
                            && !self.visited_nodes.contains(&existing_id)
                        {
                            // set_style / set_children dirty the node even when the value is
                            // identical. Portal hoist (Dialog open) rewrites only the overlay
                            // layer — the page walk must not remasure every gallery text leaf.
                            let style_changed = self
                                .taffy
                                .style(existing_id)
                                .ok()
                                .map_or(true, |old| old != &style);
                            if style_changed {
                                self.taffy.set_style(existing_id, style).unwrap();
                            }
                            let kids_changed = self
                                .taffy
                                .children(existing_id)
                                .ok()
                                .map_or(true, |old| old.as_slice() != child_nodes.as_slice());
                            if kids_changed {
                                self.taffy.set_children(existing_id, &child_nodes).unwrap();
                            }
                            existing_id
                        } else if is_text_leaf {
                            let id = self.taffy.new_leaf_with_context(style, ()).unwrap();
                            self.total_nodes.insert(id);
                            id
                        } else {
                            self.new_taffy_parent(style, &child_nodes)
                        }
                    } else if is_text_leaf {
                        let id = self.taffy.new_leaf_with_context(style, ()).unwrap();
                        self.total_nodes.insert(id);
                        id
                    } else {
                        self.new_taffy_parent(style, &child_nodes)
                    };

                    element.layout_node = Some(node_id);
                    self.visited_nodes.insert(node_id);

                    if let Some(text) = &element.text {
                        if child_nodes.is_empty() {
                            let text_clone = text.clone();
                            let is_icon =
                                element.text_style.font_family.as_deref() == Some("lucide");
                            self.text_measurements
                                .insert(node_id, text_measure_meta(element, text_clone, is_icon));
                        }
                    }

                    if stack.is_empty() {
                        root_id = Some(node_id);
                    }
                }
            }
        }

        root_id.expect("build_taffy_node_mut produced a root")
    }

    /// `TaffyTree::new_with_children` overwrites `parents[child]` without splicing
    /// the child out of the previous parent. Incremental attach (new wrapper, live
    /// child) then leaves the old parent walking a key that orphan-drop frees.
    fn new_taffy_parent(&mut self, style: taffy::style::Style, child_nodes: &[NodeId]) -> NodeId {
        let id = self.taffy.new_leaf(style).unwrap();
        self.total_nodes.insert(id);
        if !child_nodes.is_empty() {
            self.taffy.set_children(id, child_nodes).unwrap();
        }
        id
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
        let mut stack: Vec<(*mut Element, NodeId, [f32; 2], [f32; 2], [f32; 2])> =
            vec![(element as *mut Element, node, abs, size, scroll_acc)];
        while let Some((ptr, node, abs, size, scroll_acc)) = stack.pop() {
            // SAFETY: exclusive subtree rooted at the original `element`.
            let element = unsafe { &mut *ptr };
            crate::app::devtools::note_element_paint_visual_from_abs(
                element, abs, size, scroll_acc,
            );
            let Ok(child_nodes) = self.taffy.children(node) else {
                continue;
            };
            let child_scroll = next_scroll_acc_for_children(element, scroll_acc);
            let pass = [
                abs[0] + element.style.translate[0] + size[0] * element.style.translate_pct[0],
                abs[1] + element.style.translate[1] + size[1] * element.style.translate_pct[1],
            ];
            let n = element.children.len().min(child_nodes.len());
            for i in (0..n).rev() {
                let Ok(cl) = self.taffy.layout(child_nodes[i]) else {
                    continue;
                };
                let child_abs = [pass[0] + cl.location.x, pass[1] + cl.location.y];
                let child_size = [cl.size.width, cl.size.height];
                stack.push((
                    std::ptr::addr_of_mut!(element.children[i]),
                    child_nodes[i],
                    child_abs,
                    child_size,
                    child_scroll,
                ));
            }
        }
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

    /// Stamp `layout_bounds` for clip-culled descendants from Taffy so max-scroll tests and
    /// scroll-into-view still see the last row (collect skips paint, not geometry).
    fn stamp_descendants_layout_bounds_from_taffy(
        &self,
        element: &mut Element,
        node: NodeId,
        abs: [f32; 2],
        size: [f32; 2],
        overlay_collect: bool,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    ) {
        if self.try_stamp_overlay_scrollbar_hit(
            element,
            abs,
            size,
            overlay_collect,
            scroll_signals_cache,
        ) {
            return;
        }
        let mut stack: Vec<(*mut Element, NodeId, [f32; 2], [f32; 2])> =
            vec![(element as *mut Element, node, abs, size)];
        while let Some((ptr, node, abs, size)) = stack.pop() {
            // SAFETY: pointers are into the exclusive subtree rooted at `element`.
            let element = unsafe { &mut *ptr };
            let Ok(child_nodes) = self.taffy.children(node) else {
                continue;
            };
            let pass = [
                abs[0] + element.style.translate[0] + size[0] * element.style.translate_pct[0],
                abs[1] + element.style.translate[1] + size[1] * element.style.translate_pct[1],
            ];
            let n = element.children.len().min(child_nodes.len());
            for i in (0..n).rev() {
                let Ok(cl) = self.taffy.layout(child_nodes[i]) else {
                    continue;
                };
                let child_abs = [pass[0] + cl.location.x, pass[1] + cl.location.y];
                let child_size = [cl.size.width, cl.size.height];
                if self.try_stamp_overlay_scrollbar_hit(
                    &mut element.children[i],
                    child_abs,
                    child_size,
                    overlay_collect,
                    scroll_signals_cache,
                ) {
                    continue;
                }
                element.children[i].layout_bounds =
                    [child_abs[0], child_abs[1], child_size[0], child_size[1]];
                crate::style::stamp_element_used_layout(&mut element.children[i]);
                stack.push((
                    &mut element.children[i] as *mut Element,
                    child_nodes[i],
                    child_abs,
                    child_size,
                ));
            }
        }
    }

    /// Band-culled siblings keep last-painted pixels out of this frame, but wheel hit-testing
    /// still reads `layout_bounds`. Nested scroll viewports inside a skipped card must keep
    /// fresh content-space bounds or the parent steals the wheel (glyphs freeze, page moves).
    fn stamp_skipped_sibling_hit_bounds(
        &self,
        child: &mut Element,
        node: NodeId,
        abs: [f32; 2],
        size: [f32; 2],
        overlay_collect: bool,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    ) {
        if self.try_stamp_overlay_scrollbar_hit(
            child,
            abs,
            size,
            overlay_collect,
            scroll_signals_cache,
        ) {
            return;
        }
        child.layout_bounds = [abs[0], abs[1], size[0], size[1]];
        crate::style::stamp_element_used_layout(child);
        self.stamp_descendants_layout_bounds_from_taffy(
            child,
            node,
            abs,
            size,
            overlay_collect,
            scroll_signals_cache,
        );
    }

    /// Band-cull / clip-cull never `record_node_paint` descendants. Last-present of
    /// those keys still sits at the last collected visual box — dest-reset expand
    /// may wipe leftover retain, but only for ids recorded here.
    fn note_clip_culled_subtree(taffy: &TaffyTree<()>, node: NodeId, out: &mut HashSet<u64>) {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if !out.insert(u64::from(n)) {
                continue;
            }
            if let Ok(children) = taffy.children(n) {
                stack.extend(children);
            }
        }
    }

    /// Overlay tracks/thumbs paint from `scrollbar_track_overlay_abs`. Skip replay must
    /// stamp the same rects — Taffy thumb size is the Full-prepass leftover (min cap at
    /// the unscrolled top) and is not the hit box.
    fn try_stamp_overlay_scrollbar_hit(
        &self,
        element: &mut Element,
        base_abs: [f32; 2],
        layout_size: [f32; 2],
        overlay_collect: bool,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
    ) -> bool {
        let Some(fid) = element.focus_id.as_deref() else {
            return false;
        };
        let Some(rest_axis) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) else {
            return false;
        };
        let is_overlay = overlay_collect
            || element
                .attrs
                .get("rui_scrollbar_overlay")
                .map(String::as_str)
                == Some("1");
        if !is_overlay {
            return false;
        }
        let (rest, axis) = rest_axis.rsplit_once(':').unwrap_or((rest_axis, "y"));
        let (t_rect, th_rect) = if let Some(cache) = scroll_signals_cache.as_mut() {
            let sig = scroll_area_signals_cached(rest, cache);
            scrollbar_track_overlay_abs(
                element,
                base_abs[0],
                base_abs[1],
                layout_size[0],
                layout_size[1],
                axis,
                sig,
            )
        } else {
            let sig = peek_scroll_area_meta(rest);
            scrollbar_track_overlay_abs(
                element,
                base_abs[0],
                base_abs[1],
                layout_size[0],
                layout_size[1],
                axis,
                &sig,
            )
        };
        element.layout_bounds = t_rect;
        crate::style::stamp_element_used_layout(element);
        if let Some(thumb) = element.children.first_mut() {
            thumb.layout_bounds = th_rect;
            crate::style::stamp_element_used_layout(thumb);
        }
        true
    }

    fn collect_skip_geom(&self, frame: &CollectFrame) -> crate::layout::collect_cache::GeomKey {
        // Paint origin is `abs_offset + taffy.location` (see collect_paint_node).
        // Keying only parent origin + size let a flex Footer replay at the pre-maximize
        // Y: the box is still 48px tall, the parent still sits at (0,0), but location.y
        // moved with the column.
        let (origin, size) = self
            .taffy
            .layout(frame.node)
            .map(|cl| {
                (
                    [
                        frame.abs_offset[0] + cl.location.x,
                        frame.abs_offset[1] + cl.location.y,
                    ],
                    [cl.size.width, cl.size.height],
                )
            })
            .unwrap_or((frame.abs_offset, [0.0, 0.0]));
        // Own `scroll_offset` is not in `scroll_acc` (that's ancestors). Folding it in
        // busts exact-replay when a viewport scrolls its contents without moving its box.
        let scroll = unsafe {
            let el = &*frame.element;
            [
                frame.scroll_acc[0] + el.scroll_offset[0],
                frame.scroll_acc[1] + el.scroll_offset[1],
            ]
        };
        crate::layout::collect_cache::geom_key(
            origin,
            size,
            scroll,
            frame.parent_clip,
            frame.ancestor_opacity,
            frame.ancestor_scale,
            frame.ancestor_linear,
        )
    }

    /// `ancestor_scroll_viewport_rest`: nearest ancestor `scroll:{id}` viewport id for this subtree.
    /// Used so nested ScrollAreas do not inflate parent content extents (matches
    /// `scroll_support::collect_descendant_extent`); fixes thumb math for middle layers in 3+ nesting.
    #[inline(never)]
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
        ancestor_scale: [f32; 2],
        ancestor_linear: crate::style::Mat2d,
        ancestor_sticky_xform: bool,
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
        clip_polys: &mut Vec<crate::style::GpuClipPoly>,
        skip: &mut crate::layout::collect_cache::CollectSkipState,
    ) -> [f32; 2] {
        crate::style::begin_named_clip_pass();
        let mut stack = vec![CollectFrame {
            element: element as *mut Element,
            node,
            abs_offset,
            scroll_acc,
            parent_clip,
            parent_dest_clip: None,
            parent_clip_poly: crate::style::GpuClipPoly::NONE,
            ancestor_scroll_viewport_rest: ancestor_scroll_viewport_rest.map(str::to_string),
            ancestor_opacity,
            ancestor_scale,
            ancestor_linear,
            ancestor_sticky_xform,
            scrollbar_overlay_collect,
            thumb_geometry_override,
            walk: None,
            cache_start: crate::layout::collect_cache::DlCursor::default(),
            cache_geom: crate::layout::collect_cache::GeomKey::default(),
            layered: child_opens_page_stack_band(element),
            overlay_stamp: None,
        }];
        let mut root_extent = [0.0_f32, 0.0];

        while let Some(mut frame) = stack.pop() {
            // SAFETY: exclusive collect walk of the tree passed to this function.
            let element = unsafe { &mut *frame.element };
            if frame.walk.is_none() {
                #[cfg(test)]
                stack_probe::mark_collect();
                stamp_element_overlay_paint(element, overlay_split.marked, frame.overlay_stamp);
                frame.cache_geom = self.collect_skip_geom(&frame);
                let replay_start = crate::layout::collect_cache::DlCursor::capture(
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    clip_polys,
                    &paint_rec.ops,
                );
                if let Some(extent) = skip.try_replay(
                    frame.node,
                    element,
                    frame.cache_geom,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    clip_polys,
                    &mut paint_rec.ops,
                ) {
                    element.paint_scroll_acc = frame.scroll_acc;
                    record_node_paint(
                        frame.node,
                        element,
                        replay_start,
                        instances,
                        textured_draws,
                        text_nodes,
                        input_chrome_instances,
                        &paint_rec.ops,
                        overlay_split.marked,
                        frame.scroll_acc,
                        frame.parent_dest_clip,
                        frame.layered,
                    );
                    if let Ok(cl) = self.taffy.layout(frame.node) {
                        let paint_abs = [
                            frame.abs_offset[0] + cl.location.x,
                            frame.abs_offset[1] + cl.location.y,
                        ];
                        self.stamp_skipped_sibling_hit_bounds(
                            element,
                            frame.node,
                            paint_abs,
                            [cl.size.width, cl.size.height],
                            frame.scrollbar_overlay_collect,
                            scroll_signals_cache,
                        );
                    }
                    let [lx, ly, lw, lh] = element.layout_bounds;
                    if mouse_pos[0] >= lx
                        && mouse_pos[0] <= lx + lw
                        && mouse_pos[1] >= ly
                        && mouse_pos[1] <= ly + lh
                    {
                        if element.cursor.is_some() {
                            *hover_cursor = element.cursor.clone();
                        }
                    }
                    collect_commit_extent(&mut stack, extent, &mut root_extent);
                    continue;
                }
                frame.cache_start = crate::layout::collect_cache::DlCursor::capture(
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    clip_polys,
                    &paint_rec.ops,
                );
                match self.collect_paint_node(
                    element,
                    frame.node,
                    frame.abs_offset,
                    frame.scroll_acc,
                    frame.parent_clip,
                    frame.parent_dest_clip,
                    frame.parent_clip_poly,
                    frame.ancestor_scroll_viewport_rest.as_deref(),
                    mouse_pos,
                    frame.ancestor_opacity,
                    frame.ancestor_scale,
                    frame.ancestor_linear,
                    frame.ancestor_sticky_xform,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    scroll_metrics,
                    hover_cursor,
                    _scroll_only,
                    frame.scrollbar_overlay_collect,
                    frame.thumb_geometry_override,
                    scroll_signals_cache,
                    overlay_split,
                    paint_rec,
                    clip_polys,
                    &mut skip.clip_culled,
                ) {
                    CollectPaintOutcome::Done(extent) => {
                        skip.store(
                            frame.node,
                            element,
                            frame.cache_geom,
                            frame.cache_start,
                            instances,
                            textured_draws,
                            text_nodes,
                            input_chrome_instances,
                            clip_polys,
                            &paint_rec.ops,
                            extent,
                        );
                        record_node_paint(
                            frame.node,
                            element,
                            frame.cache_start,
                            instances,
                            textured_draws,
                            text_nodes,
                            input_chrome_instances,
                            &paint_rec.ops,
                            overlay_split.marked,
                            frame.scroll_acc,
                            frame.parent_dest_clip,
                            frame.layered,
                        );
                        collect_commit_extent(&mut stack, extent, &mut root_extent);
                        continue;
                    }
                    CollectPaintOutcome::Ready(painted) => {
                        match self.collect_begin_walk(
                            element,
                            frame.node,
                            frame.ancestor_scroll_viewport_rest.as_deref(),
                            frame.ancestor_linear,
                            frame.ancestor_sticky_xform,
                            instances,
                            textured_draws,
                            overlay_split,
                            painted,
                        ) {
                            CollectWalkStart::Done(extent) => {
                                skip.store(
                                    frame.node,
                                    element,
                                    frame.cache_geom,
                                    frame.cache_start,
                                    instances,
                                    textured_draws,
                                    text_nodes,
                                    input_chrome_instances,
                                    clip_polys,
                                    &paint_rec.ops,
                                    extent,
                                );
                                record_node_paint(
                                    frame.node,
                                    element,
                                    frame.cache_start,
                                    instances,
                                    textured_draws,
                                    text_nodes,
                                    input_chrome_instances,
                                    &paint_rec.ops,
                                    overlay_split.marked,
                                    frame.scroll_acc,
                                    frame.parent_dest_clip,
                                    frame.layered,
                                );
                                collect_commit_extent(&mut stack, extent, &mut root_extent);
                                continue;
                            }
                            CollectWalkStart::Ready(walk) => {
                                frame.walk = Some(walk);
                            }
                        }
                    }
                }
            }

            let job = {
                let walk = frame.walk.as_mut().expect("walk ready");
                self.collect_take_child(
                    element,
                    walk,
                    _scroll_only || skip.allowed,
                    frame.scrollbar_overlay_collect,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    overlay_split,
                    paint_rec,
                    scroll_signals_cache,
                    &mut skip.clip_culled,
                )
            };
            if let Some(job) = job {
                let walk = frame.walk.as_ref().expect("walk ready");
                let child_frame = CollectFrame {
                    element: std::ptr::addr_of_mut!(element.children[job.index]),
                    node: job.node,
                    abs_offset: job.abs_offset,
                    scroll_acc: walk.next_scroll_acc,
                    parent_clip: walk.clip,
                    parent_dest_clip: walk.dest_clip,
                    parent_clip_poly: walk.child_clip_poly,
                    ancestor_scroll_viewport_rest: walk.pass_to_children.clone(),
                    ancestor_opacity: walk.opacity,
                    ancestor_scale: walk.child_ancestor_scale,
                    ancestor_linear: walk.child_ancestor_linear,
                    ancestor_sticky_xform: walk.child_sticky_xform,
                    scrollbar_overlay_collect: frame.scrollbar_overlay_collect,
                    thumb_geometry_override: job.thumb_ov,
                    walk: None,
                    cache_start: crate::layout::collect_cache::DlCursor::default(),
                    cache_geom: crate::layout::collect_cache::GeomKey::default(),
                    layered: frame.layered
                        || child_opens_page_stack_band(&element.children[job.index]),
                    overlay_stamp: if walk.painted.is_body_portal_layer {
                        let k = overlay_portal_stamp_key(&element.children[job.index]);
                        (k != 0).then_some(k)
                    } else {
                        frame.overlay_stamp
                    },
                };
                stack.push(frame);
                stack.push(child_frame);
            } else {
                let walk = frame.walk.take().expect("walk ready");
                let extent = self.collect_finish_walk(
                    element,
                    frame.node,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    scroll_metrics,
                    overlay_split,
                    paint_rec,
                    clip_polys,
                    scroll_signals_cache,
                    walk,
                );
                skip.store(
                    frame.node,
                    element,
                    frame.cache_geom,
                    frame.cache_start,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    clip_polys,
                    &paint_rec.ops,
                    extent,
                );
                record_node_paint(
                    frame.node,
                    element,
                    frame.cache_start,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    &paint_rec.ops,
                    overlay_split.marked,
                    frame.scroll_acc,
                    frame.parent_dest_clip,
                    frame.layered,
                );
                collect_commit_extent(&mut stack, extent, &mut root_extent);
            }
        }
        root_extent
    }

    /// Paint this node only. `#[inline(never)]` so MSVC debug keeps this frame
    /// off the heap-stack collect driver.
    #[inline(never)]
    fn collect_paint_node(
        &self,
        element: &mut Element,
        node: NodeId,
        abs_offset: [f32; 2],
        scroll_acc: [f32; 2],
        parent_clip: Option<[f32; 4]>,
        parent_dest_clip: Option<[f32; 4]>,
        parent_clip_poly: crate::style::GpuClipPoly,
        _ancestor_scroll_viewport_rest: Option<&str>,
        mouse_pos: [f32; 2],
        ancestor_opacity: f32,
        // Parent CSS `scale` product (CarouselStack shell → children). Default `[1,1]`.
        ancestor_scale: [f32; 2],
        // Parent 2×2 linear cascade (rotate/skew). Default identity. Composed with local
        // `mat2d_linear(compute_combined_transform)` for SDF / text / textured draws.
        ancestor_linear: crate::style::Mat2d,
        // Sticky Bake/Msdf for text under transform/transition (even at 0°).
        ancestor_sticky_xform: bool,
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
        clip_polys: &mut Vec<crate::style::GpuClipPoly>,
        clip_culled: &mut HashSet<u64>,
    ) -> CollectPaintOutcome {
        // Solid `<Portal>` → document.body: content is outside cropping / scrolled layouts.
        // Reset clip + scroll accumulation when entering the root portal layer so hoisted
        // overlays never inherit ScrollArea (or other overflow) clips from page ancestors.
        // `position: fixed` / `rui_escape_overflow_clip` / already-hoisted portals: same
        // (Command overlay left edge was flush-cut by the gallery sidebar ScrollArea).
        let is_body_portal_layer = element.template_id.as_deref() == Some(PORTAL_LAYER_ID);
        let escape_overflow_clip = escapes_ancestor_overflow(element);
        let (mut parent_clip, parent_dest_clip, scroll_acc) = if escape_overflow_clip {
            (None, None, [0.0_f32, 0.0_f32])
        } else {
            (parent_clip, parent_dest_clip, scroll_acc)
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
                stamp_clip_polys(clip_polys, instances.len(), parent_clip_poly);
                return CollectPaintOutcome::Done([abs_offset[0], abs_offset[1]]);
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

        let (abs_x, abs_y) = if crate::utils::drag::is_active() {
            if let Some(origin) =
                crate::utils::drag::overlay::adjust_overlay_collect_origin(element, [lay_w, lay_h])
            {
                (origin[0], origin[1])
            } else {
                (abs_x, abs_y)
            }
        } else if crate::utils::anchored_overlay::is_floating_panel_dragging() {
            if let Some(origin) =
                crate::utils::anchored_overlay::adjust_point_overlay_collect_origin(element)
            {
                (origin[0], origin[1])
            } else {
                (abs_x, abs_y)
            }
        } else {
            (abs_x, abs_y)
        };

        element.layout_bounds = [abs_x, abs_y, lay_w, lay_h];

        #[cfg(feature = "devtools")]
        if element.template_id.as_deref() == Some(crate::app::devtools::HIGHLIGHT_SLOT_ID) {
            let _ = crate::app::devtools::refresh_highlight_slot(element);
        }

        let zoom_layer_scale = modal_zoom_compositor_scale(element);
        let fade = (ancestor_opacity * element.style.opacity).clamp(0.0, 1.0);
        // Own chrome (rounded border / shadow) paints with the fade. Capture children
        // stay opaque; fade lives on the inner blit so the RT is content-stable.
        // Auto scrollbar cubic is paint-only on this node — do not fold it into
        // `fade` passed to children (track × thumb would square the alpha).
        let bar_alpha = crate::components::scrollbar::auto_scrollbar_live_paint_alpha(element);
        let opacity = match bar_alpha {
            Some(v) => (fade * v).clamp(0.0, 1.0),
            None => fade,
        };
        // Stamped Inspector highlight rects are already in paint/visual space — never
        // subtract ancestor scroll again (that shifts the overlay upward).
        // Local `mount=` portals must keep ancestor scroll even if a prior body-mount
        // frame left `position_fixed` set (`escapes_ancestor_overflow` already restored
        // `scroll_acc`). Using `position_fixed` here un-scrolled the scrim for a frame.
        let scroll_for_render = if escape_overflow_clip || devtools_highlight_rect.is_some() {
            [0.0_f32, 0.0_f32]
        } else {
            scroll_acc
        };
        element.paint_scroll_acc = scroll_for_render;
        if crate::components::scrollbar::auto_scrollbar_needs_node_present(element) {
            crate::app::damage::push_frame_paint_element(element, scroll_for_render);
        }
        let paint_nudge = crate::components::button_group::button_group_paint_nudge(element);
        let offset_motion = crate::style::offset_path::sample(&element.style, [lay_w, lay_h]);
        // 布局原点（文字 / 子节点 / overflow clip）。连体叠边只扩 chrome，不平移整框。
        let mut render_x = abs_x - scroll_for_render[0]
            + element.style.translate[0]
            + lay_w * element.style.translate_pct[0]
            + offset_motion.translate[0];
        let mut render_y = abs_y - scroll_for_render[1]
            + element.style.translate[1]
            + lay_h * element.style.translate_pct[1]
            + offset_motion.translate[1];

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
        // Include ancestor scale (CarouselStack: scale lives on the shell; children stay at 1).
        let paint_sx = (ancestor_scale[0] * scale_x).clamp(0.01, 8.0);
        let paint_sy = (ancestor_scale[1] * scale_y).clamp(0.01, 8.0);
        // ButtonGroup：向共享边加宽 1px，远侧边仍落在布局盒上（避免中间项整框平移后
        // 与 Separator / 下一项并成 2px 线）。
        let chrome_x = render_x + paint_nudge[0];
        let chrome_y = render_y + paint_nudge[1];
        let chrome_w = (lay_w - paint_nudge[0]).max(0.0);
        let chrome_h = (lay_h - paint_nudge[1]).max(0.0);
        // Bake CSS independent `scale` into the instance rect about `transform-origin`
        // so placement matches `visual_paint_aabb` (hover / leave). Center origin is
        // the old `(size-scaled)*0.5` inset. Passing `style.scale` again in the SDF
        // affine painted ~scale² while hit-testing stayed at scale¹.
        //
        // Overflow-visible emoji: Taffy `leading-none` strut is 1em; Twemoji ink is
        // 1.10em. Scale that strut and hover:scale-125 dest is a 15px-tall AABB of a
        // 16.5px face (screenshot chords). Match `paint_vis` / dest draw box.
        let paint_box = crate::renderer::emoji_layout::layout_bounds_with_visible_emoji_ink(
            [render_x, render_y, lay_w, lay_h],
            element.text.as_deref(),
            element.text_style.size_resolved(),
            element.style.overflow.x == taffy::style::Overflow::Visible,
            element.style.overflow.y == taffy::style::Overflow::Visible,
        );
        let origin_layout =
            crate::style::origin_px(&element.style.transform_origin, paint_box[2], paint_box[3]);
        let (vis_w, vis_h, vis_x, vis_y) =
            if (paint_sx - 1.0).abs() > f32::EPSILON || (paint_sy - 1.0).abs() > f32::EPSILON {
                let vis = crate::style::scale_box_about_origin(
                    paint_box,
                    origin_layout,
                    [paint_sx, paint_sy],
                );
                (vis[2], vis[3], vis[0], vis[1])
            } else if (paint_box[2] - lay_w).abs() > 1e-3 || (paint_box[3] - lay_h).abs() > 1e-3 {
                (paint_box[2], paint_box[3], paint_box[0], paint_box[1])
            } else {
                (chrome_w, chrome_h, chrome_x, chrome_y)
            };
        let render_rect = [vis_x, vis_y, vis_w, vis_h];
        #[cfg(feature = "devtools")]
        crate::app::devtools::note_element_paint_visual(element, render_rect);
        // `render_x/y`（及 vis_*）已含 CSS translate。VS 绕 rect **中心** 再加 tx/ty。
        // `compute_combined_transform` 的 origin wrap 是 top-left 空间，不能直接抄上实例
        //（会绕角转、壳与文分离）。2×2 用 linear；tx/ty = (I-R)*(origin-center)，壳 / 贴图 /
        // 子树 `child_pass_offset_for` 共用同一 origin。默认 center → tx/ty = 0。
        let origin_in_vis = [
            origin_layout[0] - (vis_x - paint_box[0]),
            origin_layout[1] - (vis_y - paint_box[1]),
        ];
        let local_unwrapped = crate::style::rotate_and_list_mat2d(
            element.style.rotate_deg + offset_motion.rotate_deg,
            element.style.transform.as_ref(),
            vis_w,
            vis_h,
        );
        let local_linear = crate::style::mat2d_linear(&local_unwrapped);
        let list_t = [local_unwrapped[4], local_unwrapped[5]];
        let world_linear = crate::style::mat2d_multiply(&ancestor_linear, &local_linear);
        let transform = crate::style::mat2d_world_center_pivot_with_shift(
            &ancestor_linear,
            &local_linear,
            origin_in_vis,
            [vis_w, vis_h],
            list_t,
        );
        element.paint_ancestor_scale = ancestor_scale;
        element.paint_ancestor_linear = ancestor_linear;
        let text_world_linear = world_linear;
        let self_inst0 = instances.len();
        let self_tex0 = textured_draws.len();

        // Clipping & Visibility
        // Overflow clips the padding box. Unscaled scrollports keep the Taffy border
        // so hover:scale tiles do not grow the *viewport* clip every tween frame.
        // scale¹ dest already includes CSS scale (affine identity): clipping children
        // to the rest-sized border AABB-crops hover:scale Twemoji (screenshot chords).
        let layout_border = [render_x, render_y, lay_w, lay_h];
        let overflow_border = overflow_clip_layout_border(element, layout_border);
        let mut clip = parent_clip;
        let mut dest_clip = parent_dest_clip;
        let (clip_x, clip_y) = crate::core::tree::overflow_clip_axes(element);
        let overflow_clips = clip_x || clip_y;
        let scaled_paint_box = (paint_sx - 1.0).abs() > 1e-4 || (paint_sy - 1.0).abs() > 1e-4;
        if overflow_clips {
            let descendant_overflow = if scaled_paint_box {
                render_rect
            } else {
                overflow_border
            };
            clip = intersect_overflow_axes(clip, descendant_overflow, clip_x, clip_y);
            dest_clip = intersect_overflow_axes(
                dest_clip,
                overflow_dest_aabb(overflow_border, render_rect, &transform),
                clip_x,
                clip_y,
            );
        }
        if !element.style.clip_path.is_none() {
            let paint_clip = !crate::style::mat2d_is_identity(&transform);
            if paint_clip {
                let pts = element.style.clip_path.to_world_polygon(layout_border);
                let painted: Vec<[f32; 2]> = pts
                    .iter()
                    .map(|p| {
                        crate::core::tree::layout_local_to_paint_world(
                            element,
                            [p[0] - layout_border[0], p[1] - layout_border[1]],
                            scroll_for_render,
                            ancestor_linear,
                            ancestor_scale,
                        )
                    })
                    .collect();
                if let Some(aabb) = aabb_from_points(&painted) {
                    parent_clip = Some(intersect_rect(parent_clip, aabb));
                    clip = Some(intersect_rect(clip, aabb));
                    dest_clip = Some(intersect_rect(dest_clip, aabb));
                }
            } else if let Some(aabb) = element.style.clip_path.aabb(layout_border) {
                parent_clip = Some(intersect_rect(parent_clip, aabb));
                clip = Some(intersect_rect(clip, aabb));
                dest_clip = Some(intersect_rect(dest_clip, aabb));
            }
        }
        if let Some(name) = element.style.clip_path_name.as_deref() {
            crate::style::register_clip_path(name, element.style.clip_path.clone());
        }
        let inherited_clip_poly = if escape_overflow_clip {
            crate::style::GpuClipPoly::NONE
        } else {
            parent_clip_poly
        };
        let clip_path_poly = if element.style.clip_path.is_none() {
            crate::style::GpuClipPoly::NONE
        } else if !crate::style::mat2d_is_identity(&transform) {
            let pts = element.style.clip_path.to_world_polygon(layout_border);
            let painted: Vec<[f32; 2]> = pts
                .iter()
                .map(|p| {
                    crate::core::tree::layout_local_to_paint_world(
                        element,
                        [p[0] - layout_border[0], p[1] - layout_border[1]],
                        scroll_for_render,
                        ancestor_linear,
                        ancestor_scale,
                    )
                })
                .collect();
            crate::style::GpuClipPoly::from_world_points(
                &painted,
                element.style.clip_path.fill_rule(),
            )
        } else {
            element.style.clip_path.to_gpu(layout_border)
        };
        // Own overflow always stamps a **world** clip (AABB or transformed quad),
        // including `rui_escape_overflow_clip` shells (DragOverlay restrict box)
        // and zero-radius boxes. `escape` only drops *ancestor* overflow.
        let overflow_clip_poly = if overflow_clips && clip_x && clip_y {
            // scale¹ keeps the affine identity; a rest-sized poly then GPU-clips
            // hover:scale Twemoji to the Taffy strut (screenshot AABB chords).
            // Unscaled scrollports keep the layout border so tiles do not grow the
            // viewport clip every tween frame.
            let poly_box = if scaled_paint_box {
                render_rect
            } else {
                overflow_border
            };
            crate::style::GpuClipPoly::from_overflow_box(poly_box, element.style.radii, &transform)
        } else {
            crate::style::GpuClipPoly::NONE
        };
        // Hidden DragOverlay: portal + escape drops page overflow. Re-AND the
        // restrict target's ancestor clip so rotation cannot paint in the header
        // or the padding beside the dashed parent after the page scrolls.
        let (clip, dest_clip, overflow_clip_poly) = if element
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_OVERLAY_CLIP)
            .is_some()
        {
            if let Some(anc) = crate::utils::drag::session::peek_restrict_ancestor_clip()
                .filter(|b| b[2] > 0.5 && b[3] > 0.5)
            {
                (
                    Some(intersect_rect(clip, anc)),
                    Some(intersect_rect(dest_clip, anc)),
                    crate::style::GpuClipPoly::and_layers(
                        overflow_clip_poly,
                        crate::style::GpuClipPoly::from_axis_rect(anc),
                    ),
                )
            } else {
                (clip, dest_clip, overflow_clip_poly)
            }
        } else {
            (clip, dest_clip, overflow_clip_poly)
        };
        // Descendants: overflow + clip-path. Own chrome: clip-path only —
        // overflow must not scissor outset box-shadow (Dialog/Drawer/Sheet).
        let descendant_local = if clip_path_poly.is_active() {
            clip_path_poly
        } else {
            overflow_clip_poly
        };
        let gpu_poly = crate::style::GpuClipPoly::and_layers(inherited_clip_poly, descendant_local);
        let own_clip_poly =
            crate::style::GpuClipPoly::and_layers(inherited_clip_poly, clip_path_poly);
        let visible_rect = intersect_rect(clip, render_rect);

        // Clip-cull: skip paint + descendants when this box (plus shadow/transform outset)
        // is fully outside the ancestor clip. Parent scroll metrics still use this node's
        // Taffy size so content height does not collapse to the viewport.
        if parent_clip.is_some() && !escape_overflow_clip {
            let pad = paint_outset_pad(&element.style, lay_w, lay_h);
            let vis = intersect_rect(parent_clip, inflate_rect(render_rect, pad, pad));
            if vis[2] <= 0.0 || vis[3] <= 0.0 {
                let focused =
                    crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                        .peek();
                if !crate::core::tree::subtree_holds_live_text_edit(element, focused.as_str()) {
                    if let Some(fid) = element.focus_id.as_deref() {
                        if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                            // Do not publish max_scroll=0 for off-screen nested viewports —
                            // post-render would clamp their offset to 0 (table/tree/masonry
                            // snap to top while the page scrolls).
                            let content = if rest.ends_with("__ta_scroll") {
                                textarea_scrollport_content_size(element, abs_x, abs_y, layout.size)
                            } else {
                                let mut content = scroll_viewport_taffy_content_size(
                                    &self.taffy,
                                    element,
                                    node,
                                    layout.size,
                                );
                                content[1] = cap_intrinsic_scroll_column_content_h(
                                    self,
                                    element,
                                    node,
                                    content[1],
                                    layout.size.height,
                                );
                                content
                            };
                            scroll_metrics.push((
                                rest.to_string(),
                                [abs_x, abs_y, layout.size.width, layout.size.height],
                                [
                                    (content[0] - layout.size.width).max(0.0),
                                    (content[1] - layout.size.height).max(0.0),
                                ],
                                content,
                            ));
                            seed_scroll_signals_from_collect(
                                scroll_signals_cache,
                                rest,
                                layout.size.width,
                                layout.size.height,
                                content[0],
                                content[1],
                            );
                        }
                    }
                    let own = [abs_x + lay_w, abs_y + lay_h];
                    let contain_x = overflow_contains_axis(element.style.overflow.x);
                    let contain_y = overflow_contains_axis(element.style.overflow.y);
                    // Taffy overflow (incl. abspos) so clip-cull does not drop chart tick labels
                    // from the parent scroller. Non-visible overflow contributes the border box.
                    let mut extent = own;
                    if !contain_x || !contain_y {
                        let ovf = taffy_scrollable_overflow_extent(
                            &self.taffy,
                            element,
                            node,
                            [abs_x, abs_y],
                        );
                        if !contain_x {
                            extent[0] = extent[0].max(ovf[0]);
                        }
                        if !contain_y {
                            extent[1] = extent[1].max(ovf[1]);
                        }
                    }
                    element.collect_extent = extent;
                    element.collect_extent_valid = true;
                    self.stamp_descendants_layout_bounds_from_taffy(
                        element,
                        node,
                        [abs_x, abs_y],
                        [lay_w, lay_h],
                        scrollbar_overlay_collect,
                        scroll_signals_cache,
                    );
                    Self::note_clip_culled_subtree(&self.taffy, node, clip_culled);
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
                    stamp_clip_polys(clip_polys, instances.len(), gpu_poly);
                    return CollectPaintOutcome::Done(extent);
                }
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

        // CSS `filter` must capture this node's own paint (bg/border/shadow) + descendants.
        // Record buffer indices *before* own paint; `rui_fx` still captures children only.
        let filter_capture = element.style.filter.as_ref().is_some_and(|l| !l.is_empty());
        let scale_layer_capture = zoom_layer_scale.is_some() && !filter_capture;
        // Filter SourceGraphic includes this node's own paint. Zoom capture must
        // **not**: own rounded chrome stays live SDF (the blit dest was the extra
        // rectangular stroke). Record zoom indices after own paint.
        let filter_cap_inst0 = if filter_capture { instances.len() } else { 0 };
        let filter_cap_tex0 = if filter_capture {
            textured_draws.len()
        } else {
            0
        };
        // Filter SourceGraphic is the unclipped subtree (plus this node's own overflow).
        // Ancestor overflow clips the *output* after the filter runs.
        let paint_parent_clip = if filter_capture { None } else { parent_clip };
        let paint_clip = if filter_capture { None } else { clip };

        // Collect main instance (solid / gradient strips + shadows).
        // Geometry stays full `render_rect`. CSS overflow clips *descendants* only — the
        // element's own background / border / outset shadow must use `parent_clip`, otherwise
        // a scrolled panel's soft rounded shadow is sliced into a sharp axis-aligned box.
        // Baking `visible_rect` into pos/size made scrolled borders stick to the viewport edge.
        let border_widths = element.style.resolved_border_widths();
        let border_w = border_widths.into_iter().fold(0.0_f32, f32::max);
        let use_border_strips = border_w > 0.0
            && !element.style.border_is_uniform()
            // 有圆角时 strip 路径把 radii 钉成 0，ButtonGroup 端段会丢圆角；改走 SDF
            //（零宽侧仍会画一笔，叠在前一段共享边上，观感与 ShadCN border-l-0 一致）。
            && !element.style.radii.iter().any(|r| *r > 0.5);
        let paint_bg = crate::components::scrollbar::auto_scrollbar_restore_base_alpha(
            element,
            element.style.bg_color,
        );
        let paint_border = crate::components::scrollbar::auto_scrollbar_restore_base_alpha(
            element,
            representative_border_color(&element.style),
        );
        // ShadCN `border-transparent` keeps a 1px width for layout. Width-only
        // `has_border` still pushed an SDF stroke (gallery Item outline→default
        // left a 1px ring and darkened title AA against that instance).
        let visible_border = paint_border[3] > 0.001;
        let sdf_border_w = if use_border_strips || !visible_border {
            0.0
        } else {
            border_w
        };
        let sdf_border_color = if use_border_strips {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            mul_color_alpha(paint_border, opacity)
        };
        let sdf_border_extra = {
            let bs = if use_border_strips || !visible_border {
                0.0
            } else {
                element.style.border_style.to_shader_f32()
            };
            InstanceInput::pack_border_extra(
                bs,
                element.style.corner_shape.to_shader_f32(),
                element.style.corner_shape.shader_param(),
            )
        };
        let has_bg = paint_bg[3] > 0.0 || element.style.bg_gradient.is_some();
        let has_border = border_w > 0.0 && visible_border;
        let has_shadow = element.style.shadow_color[3] > 0.0;
        // Page-stack `backdrop-filter` is sampled on this element's SDF instance
        // (modal underlays keep the fullscreen blit and must not double-sample).
        let instance_backdrop_blur = if overlay_split.page_stack_depth > 0 {
            element.style.backdrop_blur.max(0.0)
        } else {
            0.0
        };
        let has_backdrop = instance_backdrop_blur > 0.5;
        let paint_visible = intersect_rect(parent_clip, render_rect);
        // Hidden `auto` bars keep layout_bounds for hit, but must not emit even
        // alpha-0 quads (maximize tests treat thin rects as a painted rail).
        let auto_bar_hidden = bar_alpha.is_some_and(|v| v <= 1e-4);
        // Same not-ink gate as glyphs: opacity≈0 chrome is not in the draw list.
        // Layered overlays (Latest) then dest-reset last-present instead of
        // skipping wipe because a faded fill was still collected.
        if (has_bg || has_border || has_shadow || has_backdrop)
            && !auto_bar_hidden
            && opacity >= 0.01
            && paint_visible[2] > 0.0
            && paint_visible[3] > 0.0
        {
            let resolved_radii =
                resolve_corner_radii(element.style.radii, render_rect[2], render_rect[3]);
            let need_fill_instance =
                has_bg || has_shadow || (has_border && !use_border_strips) || has_backdrop;
            if let Some(ref grad) = element.style.bg_gradient {
                use crate::style::{
                    ensure_gradient_texture, gradient_texture_radii, gradient_texture_size,
                };

                // Always texture-paint: strip approximation left seams with corner radii
                // and diagonal/radial/conic cannot be represented as axis-aligned strips.
                // Size textures in physical pixels so hard edges stay sharp under DPI scale.
                let (tw, th) = gradient_texture_size(render_rect[2], render_rect[3]);
                let tex_radii = gradient_texture_radii(resolved_radii);
                let key = ensure_gradient_texture(grad, tw, th, tex_radii);
                if let Some(mut draw) =
                    TexturedDraw::clipped(render_rect, paint_parent_clip, key, opacity)
                {
                    let [px, py] = element.style.bg_position;
                    if px.abs() > 1e-6 || py.abs() > 1e-6 {
                        draw.uv_offset[0] = (draw.uv_offset[0] + px).rem_euclid(1.0);
                        draw.uv_offset[1] = (draw.uv_offset[1] + py).rem_euclid(1.0);
                    }
                    textured_draws.push(draw);
                }
                if has_shadow || (has_border && !use_border_strips) || has_backdrop {
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
                            backdrop_blur: instance_backdrop_blur,
                        }
                        .with_clip(paint_parent_clip),
                    );
                }
            } else if need_fill_instance {
                instances.push(
                    InstanceInput {
                        rect_pos: [render_rect[0], render_rect[1]],
                        rect_size: [render_rect[2], render_rect[3]],
                        color: mul_color_alpha(paint_bg, opacity),
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
                        backdrop_blur: instance_backdrop_blur,
                    }
                    .with_clip(paint_parent_clip),
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
                    paint_parent_clip,
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
                    paint_parent_clip,
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
                        backdrop_blur: 0.0,
                    }
                    .with_clip(paint_parent_clip),
                );
            }
        }

        // WP-3.2 / 3.11 / 3.12: GPU PathPipeline（描边 mesh + stencil-cover 填充）或 CPU golden → rui_path:。
        // Canvas：GpuCanvas → PathPipeline `rui_canvas:`（或 CPU pixmap）；再 shader/scene3d，再 path。
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
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                {
                    textured_draws.push(draw.with_transform(transform));
                    path_tex_drawn = true;
                }
            } else if let Some((tex_key, dw, dh)) =
                crate::primitives::shader::shader_tex_from_element_attrs(
                    &element.attrs,
                    vis_w,
                    vis_h,
                )
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                {
                    textured_draws.push(draw.with_transform(transform));
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
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                {
                    textured_draws.push(draw.with_transform(transform));
                    path_tex_drawn = true;
                }
            } else if crate::renderer::gpu2d_enabled() {
                if let Some((tex_key, dw, dh)) =
                    crate::primitives::path::path_gpu_from_element_attrs(
                        &element.attrs,
                        vis_w,
                        vis_h,
                    )
                {
                    if let Some(draw) =
                        TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                    {
                        textured_draws.push(draw.with_transform(transform));
                        path_tex_drawn = true;
                    }
                } else if let Some((tex_key, dw, dh)) =
                    crate::primitives::svg_raster_from_element_attrs(&element.attrs, vis_w, vis_h)
                {
                    if let Some(draw) =
                        TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                    {
                        textured_draws.push(draw.with_transform(transform));
                        path_tex_drawn = true;
                    }
                } else if let Some((tex_key, dw, dh)) =
                    crate::primitives::path::path_raster_from_element_attrs(
                        &element.attrs,
                        vis_w,
                        vis_h,
                    )
                {
                    if let Some(draw) =
                        TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                    {
                        textured_draws.push(draw.with_transform(transform));
                        path_tex_drawn = true;
                    }
                }
            } else if let Some((tex_key, dw, dh)) =
                crate::primitives::svg_raster_from_element_attrs(&element.attrs, vis_w, vis_h)
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                {
                    textured_draws.push(draw.with_transform(transform));
                    path_tex_drawn = true;
                }
            } else if let Some((tex_key, dw, dh)) =
                crate::primitives::path::path_raster_from_element_attrs(
                    &element.attrs,
                    vis_w,
                    vis_h,
                )
            {
                if let Some(draw) =
                    TexturedDraw::clipped([vis_x, vis_y, dw, dh], paint_clip, tex_key, opacity)
                {
                    textured_draws.push(draw.with_transform(transform));
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
                    let q_xf = if q.rotate_deg.abs() > 1e-3 {
                        crate::style::transform::mat2d_multiply(
                            &transform,
                            &crate::style::transform::mat2d_rotate(q.rotate_deg.to_radians()),
                        )
                    } else {
                        transform
                    };
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
                            transform: q_xf,
                            clip_pos: [0.0, 0.0],
                            clip_size: InstanceInput::NO_CLIP_SIZE,
                            border_extra: 0.0,
                            backdrop_blur: 0.0,
                        }
                        .with_clip(paint_parent_clip),
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
                            backdrop_blur: 0.0,
                        }
                        .with_clip(paint_parent_clip),
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
        if outline_w > 0.0 && lay_w > 0.0 && lay_h > 0.0 && opacity >= 0.01 {
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
                        backdrop_blur: 0.0,
                    }
                    .with_clip(paint_parent_clip),
                );
            }
        }

        if !path_tex_drawn {
            if let Some(tex_key) = element.image_texture.as_ref() {
                if !tex_key.is_empty() {
                    if let Some(mut draw) = TexturedDraw::clipped(
                        [vis_x, vis_y, vis_w, vis_h],
                        paint_parent_clip,
                        tex_key.clone(),
                        opacity,
                    ) {
                        if element.attrs.contains_key(crate::utils::ATTR_ICON_SRC) {
                            draw = draw.with_transform(transform);
                        }
                        textured_draws.push(draw);
                    }
                }
            }
        }

        let mut pending_caret_scroll: Option<(f32, Option<String>)> = None;
        'text_emit: {
            let Some(text) = &element.text else {
                break 'text_emit;
            };
            // Fully transparent text is not ink. A later page-stack glyphon band
            // that keeps those runs will skip `render()` on empty/unchanged and
            // PartialLoad retains the last GPU mesh.
            if opacity < 0.01 {
                break 'text_emit;
            }
            let mut text_style = element.text_style.as_ref().clone();
            if opacity < 1.0 - f32::EPSILON {
                let mut c = text_style.color_resolved();
                let a = ((f32::from(c[3]) * opacity).round() as u16).min(255) as u8;
                c[3] = a;
                text_style.color = Some(c);
            }
            // CSS `scale` paints like a transform: shape at layout box width (soft-wrap
            // follows element `white-space`, not visual scale), place at scaled top-left,
            // and apply glyphon `TextArea.scale` — no per-frame font-size reshape (that
            // lagged behind continuous rect scale during CarouselStack drag).
            let text_scale =
                if (paint_sx - 1.0).abs() > f32::EPSILON || (paint_sy - 1.0).abs() > f32::EPSILON {
                    paint_sx.min(paint_sy)
                } else {
                    1.0
                };
            let scaled_paint = (text_scale - 1.0).abs() > f32::EPSILON;
            let is_icon = text_style.font_family.as_deref() == Some("lucide");
            let wm = element.style.writing_mode;
            let wm_rot = if wm.is_vertical() && !is_icon {
                wm.glyph_rotation_rad()
            } else {
                0.0
            };
            let (text_x0, text_y0, text_w, text_h) = {
                let (x, y, w, h) = if scaled_paint {
                    // vis_* = layout box scaled about center; width/height stay layout for shaping.
                    (vis_x, vis_y, lay_w, lay_h)
                } else {
                    (render_x, render_y, lay_w, lay_h)
                };
                if wm_rot.abs() > 1e-6 {
                    let cx = x + w * 0.5;
                    let cy = y + h * 0.5;
                    let sw = h;
                    let sh = w;
                    (cx - sw * 0.5, cy - sh * 0.5, sw, sh)
                } else {
                    (x, y, w, h)
                }
            };
            let text_paint_linear = if wm_rot.abs() > 1e-6 {
                crate::style::mat2d_multiply(
                    &text_world_linear,
                    &crate::style::mat2d_rotate(wm_rot),
                )
            } else {
                text_world_linear
            };
            // wrap=off textarea pans via viewport `scroll_acc` (signal sx). Do not also
            // subtract `text_style.scroll_x` (single-line Hidden input only).
            let viewport_nowrap_edit = text_style.white_space == crate::style::WhiteSpace::Nowrap
                && !text_style.overflow.allows_caret_scroll();
            let mut text_x = if viewport_nowrap_edit {
                text_x0
            } else {
                text_x0 - text_style.scroll_x.max(0.0) * text_scale
            };
            let is_animated_icon = element.attrs.contains_key(crate::utils::ATTR_ANIMATED_ICON);
            #[cfg(feature = "svg")]
            let is_morph_icon = element.attrs.contains_key(crate::utils::ATTR_MORPH_ICON);
            #[cfg(not(feature = "svg"))]
            let is_morph_icon = false;
            // Glyphon cannot rotate. Lucide + spin/rotate/transform-transition → cached
            // square texture + element transform (same path at 0° and 180°, so accordion
            // chevrons don't pop from Coverage→Linear mid-spin). lucide-animated always
            // uses the same TexturedDraw path (frame bake).
            let icon_needs_xform = is_animated_icon
                || is_morph_icon
                || (is_icon
                    && (crate::renderer::element_wants_sticky_xform(element)
                        || element.style.rotate_deg.abs() > 0.5
                        || element
                            .style
                            .animations
                            .iter()
                            .any(|e| e.name == "spin" && e.is_named())
                        || element
                            .style
                            .transform
                            .as_ref()
                            .is_some_and(|t| !t.is_empty())));
            if icon_needs_xform {
                // Paint a font-size square centered in the layout box. Using
                // max(lay/vis) stretched a 16px chevron to the flex parent (thicker
                // after rotate) and shifted the pivot away from the glyph.
                let side = (text_style.size_resolved() * text_scale).max(1.0);
                let cx = vis_x + vis_w * 0.5;
                let cy = vis_y + vis_h * 0.5;
                let full = [cx - side * 0.5, cy - side * 0.5, side, side];
                // Same VS affine as the SDF shell: linear about rect center + origin txty.
                // Recomputing origin wrap on `side` and copying tx/ty made the icon orbit a corner.
                let icon_xf = transform;
                if element.attrs.contains_key(crate::utils::ATTR_ANIMATED_ICON) {
                    if let Some((tex_key, _)) =
                        crate::utils::lucide_animated::ensure_animated_icon_texture(
                            element,
                            side,
                            text_style.color_resolved(),
                            2.0,
                        )
                    {
                        textured_draws.push(
                            crate::renderer::TexturedDraw {
                                rect: full,
                                uv_offset: [0.0, 0.0],
                                uv_scale: [1.0, 1.0],
                                texture_key: tex_key,
                                opacity,
                                tint_rgb: [1.0, 1.0, 1.0],
                                transform: icon_xf,
                                clip_pos: [0.0, 0.0],
                                clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
                            }
                            .with_overflow_clip(clip),
                        );
                    }
                } else if is_morph_icon {
                    #[cfg(feature = "svg")]
                    if let Some((tex_key, _)) = crate::utils::morph_icon::ensure_morph_icon_texture(
                        element,
                        side,
                        text_style.color_resolved(),
                        2.0,
                    ) {
                        textured_draws.push(
                            crate::renderer::TexturedDraw {
                                rect: full,
                                uv_offset: [0.0, 0.0],
                                uv_scale: [1.0, 1.0],
                                texture_key: tex_key,
                                opacity,
                                tint_rgb: [1.0, 1.0, 1.0],
                                transform: icon_xf,
                                clip_pos: [0.0, 0.0],
                                clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
                            }
                            .with_overflow_clip(clip),
                        );
                    }
                } else if let Some(glyph) = element.text.as_deref() {
                    let icon_name = element
                        .attrs
                        .get(crate::utils::ATTR_ICON_NAME)
                        .map(|s| s.as_str());
                    // 2× supersample for crisp spin; draw a font-size square (not the layout box).
                    if let Some((tex_key, _)) =
                        crate::utils::icon_texture::ensure_lucide_icon_texture(
                            icon_name,
                            glyph,
                            side,
                            text_style.color_resolved(),
                            2.0,
                        )
                    {
                        textured_draws.push(
                            crate::renderer::TexturedDraw {
                                rect: full,
                                uv_offset: [0.0, 0.0],
                                uv_scale: [1.0, 1.0],
                                texture_key: tex_key,
                                opacity,
                                tint_rgb: [1.0, 1.0, 1.0],
                                transform: icon_xf,
                                clip_pos: [0.0, 0.0],
                                clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
                            }
                            .with_overflow_clip(clip),
                        );
                    }
                }
            } else {
                // Named Lucide icons: raster SVG/path to texture (bypasses PUA + fontdb drift).
                let mut lucide_static_textured = false;
                if is_icon {
                    if let (Some(icon_name), Some(glyph)) = (
                        element
                            .attrs
                            .get(crate::utils::ATTR_ICON_NAME)
                            .map(|s| s.as_str())
                            .filter(|n| !n.is_empty()),
                        element.text.as_deref(),
                    ) {
                        if crate::utils::lucide_svg_inner(icon_name).is_some() {
                            let side = (text_style.size_resolved() * text_scale).max(1.0);
                            if let Some((tex_key, _)) =
                                crate::utils::icon_texture::ensure_lucide_icon_texture(
                                    Some(icon_name),
                                    glyph,
                                    side,
                                    text_style.color_resolved(),
                                    2.0,
                                )
                            {
                                let cx = vis_x + vis_w * 0.5;
                                let cy = vis_y + vis_h * 0.5;
                                textured_draws.push(
                                    crate::renderer::TexturedDraw {
                                        rect: [cx - side * 0.5, cy - side * 0.5, side, side],
                                        uv_offset: [0.0, 0.0],
                                        uv_scale: [1.0, 1.0],
                                        texture_key: tex_key,
                                        opacity,
                                        tint_rgb: [1.0, 1.0, 1.0],
                                        transform,
                                        clip_pos: [0.0, 0.0],
                                        clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
                                    }
                                    .with_overflow_clip(clip),
                                );
                                lucide_static_textured = true;
                            }
                        }
                    }
                }
                if !lucide_static_textured {
                    // Sticky Bake / Msdf: supersampled texture + matrix (same world_linear as shell).
                    // Coverage (glyphon) for idle axis-aligned UI text only.
                    // scale¹ keeps world_linear identity, so Coverage is the default.
                    // Twemoji under CSS scale must Bake: glyphon TextBounds of the
                    // unscaled strut AABB-crops the face (screenshot chords).
                    let emoji_scaled =
                        scaled_paint && crate::renderer::emoji_detect::text_contains_emoji(text);
                    let sticky_xform = ancestor_sticky_xform
                        || crate::renderer::element_wants_sticky_xform(element)
                        || wm_rot.abs() > 1e-6
                        || emoji_scaled;
                    let glyph_raster = crate::renderer::resolve_glyph_raster(
                        sticky_xform,
                        &text_paint_linear,
                        text_scale,
                    );
                    let shimmer_active = element
                        .attrs
                        .get(crate::renderer::ATTR_GLYPH_FX)
                        .is_some_and(|s| {
                            crate::renderer::GlyphFxKind::parse(s)
                                == crate::renderer::GlyphFxKind::Shimmer
                        });
                    let use_bake = matches!(
                        glyph_raster,
                        crate::renderer::GlyphRasterMode::Bake
                            | crate::renderer::GlyphRasterMode::Msdf
                    ) && !text.trim().is_empty()
                        && text_w >= 1.0
                        && text_h >= 1.0
                        && !shimmer_active;

                    let mut text_baked = false;
                    if use_bake {
                        let align = match text_style.align_resolved() {
                            crate::style::TextAlign::Left
                            | crate::style::TextAlign::Start
                            | crate::style::TextAlign::MatchParent => {
                                glyphon::cosmic_text::Align::Left
                            }
                            crate::style::TextAlign::Center => glyphon::cosmic_text::Align::Center,
                            crate::style::TextAlign::Right | crate::style::TextAlign::End => {
                                glyphon::cosmic_text::Align::Right
                            }
                            crate::style::TextAlign::Justify
                            | crate::style::TextAlign::JustifyAll => {
                                glyphon::cosmic_text::Align::Justified
                            }
                        };
                        let as_sdf = matches!(glyph_raster, crate::renderer::GlyphRasterMode::Msdf)
                            && !crate::renderer::emoji_detect::text_contains_emoji(text);
                        // Sticky hover:scale / rotate: raster at rest and let the draw quad
                        // carry CSS scale. Re-shaping Twemoji into a leading-none strut every
                        // tween frame crops the face (square cut mid hover:scale-125).
                        // Non-sticky zoom uses the identity band (Dialog zoom-in-95).
                        let raster_scale = if sticky_xform {
                            1.0
                        } else {
                            crate::renderer::bake_source_scale(text_scale)
                        };
                        let dest_sx = text_scale / raster_scale.max(1e-6);
                        let dpr = if as_sdf {
                            (3.5 * text_scale).clamp(3.5, 8.0)
                        } else if crate::renderer::emoji_detect::text_contains_emoji(text)
                            && raster_scale > 1.25
                        {
                            (crate::renderer::BAKE_OVERSAMPLE * raster_scale).clamp(3.0, 6.0)
                        } else {
                            crate::renderer::BAKE_OVERSAMPLE
                        };
                        let mut src_w = (text_w * raster_scale).max(1.0);
                        let mut src_h = (text_h * raster_scale).max(1.0);
                        let (draw_w, draw_h) =
                            if crate::renderer::emoji_detect::text_contains_emoji(text) {
                                let (pw, ph) =
                                    crate::renderer::emoji_layout::emoji_visible_paint_size(
                                        text_w,
                                        text_h,
                                        text_style.size_resolved(),
                                    );
                                src_w = (pw * raster_scale).max(1.0);
                                src_h = (ph * raster_scale).max(1.0);
                                (pw * text_scale, ph * text_scale)
                            } else {
                                (text_w * text_scale, text_h * text_scale)
                            };
                        let bake_font_size = text_style.size_resolved() * raster_scale;
                        // Expand to line-height + ink pad (+ text-shadow extent); draw the padded
                        // quad so glyphs/shadow aren't clipped. Intersect only with ancestor scroll
                        // clip — not the text leaf's overflow:hidden (that would crop the pad).
                        let user_shadow = match text_style.shadow_color {
                            Some(sc)
                                if sc[3] > 0
                                    && (text_style.shadow_blur > 0.0
                                        || text_style.shadow_offset[0].abs() > 0.0
                                        || text_style.shadow_offset[1].abs() > 0.0) =>
                            {
                                Some((
                                    sc,
                                    [
                                        text_style.shadow_offset[0] * raster_scale,
                                        text_style.shadow_offset[1] * raster_scale,
                                    ],
                                    text_style.shadow_blur * raster_scale,
                                ))
                            }
                            _ => None,
                        };
                        let bake_shadow = crate::utils::text_texture::text_xform_effective_shadow(
                            user_shadow,
                            text_style.weight_resolved(),
                            text_style.color_resolved(),
                            raster_scale,
                            is_icon,
                            text_style.font_family.as_deref(),
                        );
                        let shadow_extra = bake_shadow
                            .map(|(_, so, sb)| {
                                crate::utils::text_texture::text_xform_shadow_pad(so, sb)
                            })
                            .unwrap_or(0.0);
                        let nowrap = text_style.white_space == crate::style::WhiteSpace::Nowrap;
                        let is_input_text = text_style.input_caret.is_some();
                        let line_h = crate::renderer::prepare_text::coverage_line_height(
                            &text_style,
                            raster_scale,
                            is_input_text,
                        );
                        let (bake_w, bake_h, pad) =
                            crate::utils::text_texture::text_xform_bake_box_ex(
                                src_w,
                                src_h,
                                bake_font_size,
                                shadow_extra,
                                line_h,
                            );
                        let hard_lines_n = text.split('\n').count().max(1);
                        let glyph_dy = crate::utils::text_texture::bake_glyph_layout_dy(
                            src_h,
                            line_h,
                            hard_lines_n,
                            nowrap,
                            is_input_text,
                        );
                        let bake_result = if let Some(spans) =
                            element.text_spans.as_ref().filter(|s| !s.is_empty())
                        {
                            // Rich spans: shadow still expands the quad; glyph shadow bake is
                            // Coverage-path only for now (span colors complicate soft stamps).
                            let _ = bake_shadow;
                            crate::utils::text_texture::ensure_text_xform_texture_spans(
                                spans,
                                bake_font_size,
                                text_style.weight_resolved(),
                                text_style.color_resolved(),
                                src_w,
                                src_h,
                                bake_w,
                                bake_h,
                                pad,
                                align,
                                dpr,
                                as_sdf,
                                glyph_dy,
                                line_h,
                            )
                        } else {
                            crate::utils::text_texture::ensure_text_xform_texture_with_shadow(
                                text,
                                bake_font_size,
                                text_style.weight_resolved(),
                                text_style.color_resolved(),
                                src_w,
                                src_h,
                                bake_w,
                                bake_h,
                                pad,
                                align,
                                dpr,
                                as_sdf,
                                bake_shadow,
                                glyph_dy,
                                line_h,
                            )
                        };
                        if let Some((tex_key, _)) = bake_result {
                            let full = crate::utils::text_texture::bake_draw_rect_for_layout(
                                text_x,
                                text_y0,
                                draw_w,
                                draw_h,
                                bake_w * dest_sx,
                                bake_h * dest_sx,
                            );
                            let xform = crate::style::mat2d_set_translation(
                                &text_paint_linear,
                                transform[4],
                                transform[5],
                            );
                            let rotated =
                                !crate::style::transform::mat2d_is_axis_aligned(&text_paint_linear);
                            // Rotated Bake (writing-mode / CSS rotate): the quad is the
                            // pre-rotation box. AABB-intersecting it with overflow squeezes
                            // vertical text into the clip edge. Keep the full quad and GPU-clip
                            // after the vertex transform.
                            // Overflow-visible emoji: never UV-crop the scaled dest against a
                            // rest-sized ancestor AABB (`clipped`) — that is the screenshot
                            // square cut. Still GPU-clip to ancestor `overflow:hidden`
                            // (`dest_clip`): the chip is overflow-visible, so dest_clip is the
                            // padded pill / accordion max-height, not the rest strut.
                            // Skipping that clip leaves wrap-row Twemoji hanging below a
                            // shrinking +N capsule. Hover:scale-125 fits in the 6px pad.
                            let emoji_overflow_visible =
                                crate::renderer::emoji_detect::text_contains_emoji(text)
                                    && element.style.overflow.x == taffy::style::Overflow::Visible
                                    && element.style.overflow.y == taffy::style::Overflow::Visible;
                            let cpu_clip = if rotated || emoji_overflow_visible {
                                None
                            } else {
                                parent_clip
                            };
                            let skip = rotated
                                && parent_clip.is_some_and(|c| {
                                    crate::renderer::aabbs_disjoint(
                                        crate::renderer::aabb_of_centered_transform(full, &xform),
                                        c,
                                    )
                                });
                            if !skip {
                                if let Some(mut draw) =
                                    TexturedDraw::clipped(full, cpu_clip, tex_key, opacity)
                                {
                                    draw = draw.with_transform(xform);
                                    if rotated || emoji_overflow_visible {
                                        draw = draw.with_overflow_clip(dest_clip.or(parent_clip));
                                    }
                                    if as_sdf {
                                        let c = text_style.color_resolved();
                                        draw = draw.with_tint_rgb([
                                            c[0] as f32 / 255.0,
                                            c[1] as f32 / 255.0,
                                            c[2] as f32 / 255.0,
                                        ]);
                                    }
                                    // Emoji Bake is RGBA Twemoji — never apply text-color tint.
                                    textured_draws.push(draw);
                                }
                            }
                            text_baked = true;
                        } else if sticky_xform
                            || !crate::style::transform::mat2d_is_axis_aligned(&text_paint_linear)
                        {
                            log::warn!(
                                target: "rui::text_xform",
                                "text xform bake failed for {:?}; skipping (no glyphon transform fallback)",
                                text
                            );
                            text_baked = true;
                        }
                    }
                    if !text_baked {
                        let prepared = if is_icon {
                            None
                        } else {
                            let line_height = if let Some(lh) = text_style.line_height_px {
                                lh
                            } else if let Some(scale) = text_style.line_height_scale {
                                text_style.size_resolved() * scale
                            } else {
                                text_style.size_resolved()
                                    * crate::style::tailwind_default_leading_scale(
                                        text_style.size_resolved(),
                                    )
                            };
                            if let Some(spans) =
                                element.text_spans.as_ref().filter(|s| !s.is_empty())
                            {
                                Some(crate::text::prepare_rich(
                                    spans,
                                    text_style.size_resolved(),
                                    text_style.weight_resolved(),
                                    line_height,
                                ))
                            } else {
                                let blob: std::borrow::Cow<'_, str> =
                                    match element.text_shape_suffix.as_deref() {
                                        Some(suf) if !suf.is_empty() => {
                                            std::borrow::Cow::Owned(format!("{}{}", text, suf))
                                        }
                                        _ => std::borrow::Cow::Borrowed(text.as_str()),
                                    };
                                Some(crate::text::prepare(
                                    blob.as_ref(),
                                    text_style.size_resolved(),
                                    text_style.weight_resolved(),
                                    line_height,
                                ))
                            }
                        };
                        if !viewport_nowrap_edit && text_style.overflow.allows_caret_scroll() {
                            if let Some(p) = prepared.as_ref() {
                                let content_w = crate::text::intrinsic_width(p);
                                let sx = crate::utils::text_field::clamp_caret_scroll_x(
                                    content_w,
                                    text_w,
                                    text_style.scroll_x,
                                );
                                if (sx - text_style.scroll_x).abs() > 0.05 {
                                    text_style.scroll_x = sx;
                                    let rest = element.template_id.as_deref().and_then(
                                        crate::utils::text_field::input_rest_from_text_template_id,
                                    );
                                    pending_caret_scroll = Some((sx, rest.map(str::to_string)));
                                }
                                text_x = text_x0 - sx.max(0.0) * text_scale;
                            }
                        }
                        // Taffy `leading-none` strut is one line; glyphon still wraps.
                        // Dest-reset hit-tests this box — a first-line height left
                        // Latest enter/exit CLEARing wrap lines that never recaptured.
                        let wrap_ink_h = if !is_icon
                            && !viewport_nowrap_edit
                            && !text_style.overflow.allows_caret_scroll()
                            && text_style.white_space != crate::style::WhiteSpace::Nowrap
                        {
                            prepared
                                .as_ref()
                                .map(|p| crate::text::layout(p, text_w.max(1.0)).height)
                                .filter(|h| h.is_finite() && *h > 0.0)
                                .unwrap_or(text_h)
                        } else {
                            text_h
                        };
                        text_nodes.push(TextLayoutInfo {
                            text: text.clone(),
                            shape_suffix: element.text_shape_suffix.clone(),
                            spans: element.text_spans.clone(),
                            template_id: element.template_id.clone(),
                            x: text_x,
                            y: text_y0,
                            width: text_w,
                            height: text_h.max(wrap_ink_h),
                            // Clip/hidden/ellipsis nowrap must scissor to the field. Growing the leaf to
                            // intrinsic (button nowrap) used to make `clip == leaf`, so glyphs and
                            // selection painted past the input border.
                            clip_bounds: if text_style.overflow.allows_caret_scroll() {
                                hidden_input_text_clip(parent_clip, render_rect)
                            } else if element_text_clips_to_box(element)
                                && !crate::renderer::emoji_detect::text_contains_emoji(text)
                            {
                                let leaf_clip = if scaled_paint {
                                    [
                                        text_x,
                                        text_y0,
                                        (text_w * text_scale).max(1.0),
                                        (text_h * text_scale).max(1.0),
                                    ]
                                } else {
                                    text_leaf_clip_rect(render_rect, &text_style)
                                };
                                Some(intersect_rect(clip.or(parent_clip), leaf_clip))
                            } else {
                                // Ancestor overflow only. Emoji / wrapping ink may overflow the
                                // line box (`leading-none`); clipping to the strut is the
                                // hover:scale Twemoji square crop.
                                clip
                            },
                            style: text_style.clone(),
                            prepared,
                            glyph_fx: element.attrs.get(crate::renderer::ATTR_GLYPH_FX).cloned(),
                            shimmer: crate::renderer::ShimmerParams::from_attrs(&element.attrs),
                            paint_scale: text_scale,
                            paint_transform: text_paint_linear,
                            paint_origin: [
                                text_x + text_w * text_scale * 0.5,
                                text_y0 + text_h * text_scale * 0.5,
                            ],
                            glyph_raster,
                        });
                        let chrome_clip = if text_style.overflow.allows_caret_scroll() {
                            hidden_input_text_clip(parent_clip, render_rect)
                        } else if element_text_clips_to_box(element)
                            && !crate::renderer::emoji_detect::text_contains_emoji(text)
                        {
                            let leaf_clip = if scaled_paint {
                                [
                                    text_x,
                                    text_y0,
                                    (text_w * text_scale).max(1.0),
                                    (text_h * text_scale).max(1.0),
                                ]
                            } else {
                                text_leaf_clip_rect(render_rect, &text_style)
                            };
                            Some(intersect_rect(clip.or(parent_clip), leaf_clip))
                        } else {
                            clip
                        };
                        push_input_chrome_instances(
                            instances,
                            input_chrome_instances,
                            text,
                            &text_style,
                            text_x,
                            text_y0,
                            text_w,
                            text_h,
                            chrome_clip,
                            opacity,
                        );
                    } // !text_baked
                } // !lucide_static_textured
            } // else !icon_spin_paint
        }

        if let Some((sx, rest)) = pending_caret_scroll {
            element.text_style.scroll_x = sx;
            if let Some(rest) = rest {
                crate::core::reactivity::use_signal(
                    &crate::utils::text_field::input_scroll_x_signal_key(&rest),
                    || 0.0_f32,
                )
                .set_silent(sx);
            }
        }

        stamp_clip_polys(clip_polys, instances.len(), own_clip_poly);

        paint_rec.push_self(self_inst0, instances.len(), self_tex0, textured_draws.len());

        let scale_cap_inst0 = if scale_layer_capture {
            instances.len()
        } else {
            0
        };
        let scale_cap_tex0 = if scale_layer_capture {
            textured_draws.len()
        } else {
            0
        };
        let scale_cap_text0 = if scale_layer_capture {
            text_nodes.len()
        } else {
            0
        };

        CollectPaintOutcome::Ready(CollectPainted {
            abs_x,
            abs_y,
            lay_w,
            lay_h,
            opacity: if zoom_layer_scale.is_some() {
                ancestor_opacity.clamp(0.0, 1.0)
            } else {
                fade
            },
            clip,
            parent_clip,
            dest_clip,
            clip_poly: gpu_poly,
            render_rect,
            visible_rect,
            paint_sx,
            paint_sy,
            world_linear,
            local_linear,
            origin_layout,
            transform,
            list_t,
            filter_capture,
            filter_cap_inst0,
            filter_cap_tex0,
            scale_layer: zoom_layer_scale,
            scale_cap_inst0,
            scale_cap_tex0,
            scale_cap_text0,
            layout_border,
            is_body_portal_layer,
            thumb_rect_for_child,
            layout_size: layout.size,
            scroll_acc,
        })
    }

    fn collect_begin_walk(
        &self,
        element: &mut Element,
        node: NodeId,
        ancestor_scroll_viewport_rest: Option<&str>,
        ancestor_linear: crate::style::Mat2d,
        ancestor_sticky_xform: bool,
        instances: &Vec<InstanceInput>,
        textured_draws: &Vec<TexturedDraw>,
        overlay_split: &mut OverlayPaintSplit,
        painted: CollectPainted,
    ) -> CollectWalkStart {
        let abs_x = painted.abs_x;
        let abs_y = painted.abs_y;
        let lay_w = painted.lay_w;
        let lay_h = painted.lay_h;
        let mut next_scroll_acc = painted.scroll_acc;
        let is_scroll_viewport = element
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
        let scroll_x = is_scroll_viewport
            || matches!(element.style.overflow.x, taffy::style::Overflow::Scroll);
        let scroll_y = is_scroll_viewport
            || matches!(element.style.overflow.y, taffy::style::Overflow::Scroll);
        if scroll_x {
            next_scroll_acc[0] += scroll_viewport_axis_offset(element, is_scroll_viewport, true);
        }
        if scroll_y {
            next_scroll_acc[1] += scroll_viewport_axis_offset(element, is_scroll_viewport, false);
        }

        let pass_to_children = element
            .focus_id
            .as_deref()
            .and_then(|fid| fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
            .or(ancestor_scroll_viewport_rest)
            .map(str::to_string);

        let child_nodes = match self.taffy.children(node) {
            Ok(c) => c,
            Err(err) => {
                log::error!("taffy.children({node:?}) failed: {err:?}");
                let own = [abs_x + lay_w, abs_y + lay_h];
                element.collect_extent = own;
                element.collect_extent_valid = true;
                return CollectWalkStart::Done(own);
            }
        };
        let any_z = element.children.iter().any(|c| c.style.z_index != 0);
        let stacking_attr = element
            .attrs
            .get("data-stacking-context")
            .map(|s| s.as_str())
            == Some("1");
        let any_stacking = stacking_attr || any_z;
        // `backdrop-filter` must split even inside an already-open page_stack
        // (gallery sidebar / ScrollArea). Nested z-index alone must not
        // (MessageScroller). Only walk the subtree when this node already
        // z-orders children — O(children of stacking parents), not O(n²).
        let nested_backdrop_split = overlay_split.page_stack_depth > 0
            && !stacking_attr
            && any_z
            && subtree_has_backdrop_blur(element);
        let record_page_stack_bands = !overlay_split.marked
            && ((overlay_split.page_stack_depth == 0 && any_stacking)
                || (overlay_split.page_stack_depth > 0 && stacking_attr)
                || nested_backdrop_split);
        // Outermost stack parent bands every child (page + overlay bar). Nested
        // `data-stacking-context` (DragProvider under ScrollArea) must not turn
        // each in-flow row into its own glyphon band.
        let stack_band_only_layered = record_page_stack_bands && overlay_split.page_stack_depth > 0;
        if record_page_stack_bands {
            overlay_split.page_stack_depth = overlay_split.page_stack_depth.saturating_add(1);
        }

        let fx_capture_children = element
            .attrs
            .get(crate::renderer::ATTR_EFFECT_CAPTURE_CHILDREN)
            .map(|s| s.as_str())
            == Some("1");
        let fx_cap_inst0 = if fx_capture_children && !painted.filter_capture {
            instances.len()
        } else {
            0
        };
        let fx_cap_tex0 = if fx_capture_children && !painted.filter_capture {
            textured_draws.len()
        } else {
            0
        };
        let (cap_inst0, cap_tex0, cap_text0, cap_children) = if painted.filter_capture {
            (painted.filter_cap_inst0, painted.filter_cap_tex0, 0, true)
        } else if painted.scale_layer.is_some() {
            (
                painted.scale_cap_inst0,
                painted.scale_cap_tex0,
                painted.scale_cap_text0,
                true,
            )
        } else if fx_capture_children {
            (fx_cap_inst0, fx_cap_tex0, 0, true)
        } else {
            (0, 0, 0, false)
        };

        let (child_clip, child_clip_poly) =
            if painted.filter_capture || painted.scale_layer.is_some() {
                filter_source_overflow_clip(&element.style, abs_x, abs_y, lay_w, lay_h)
            } else {
                (painted.clip, painted.clip_poly)
            };
        let dest_clip = if painted.filter_capture || painted.scale_layer.is_some() {
            let (clip_x, clip_y) = crate::core::tree::overflow_clip_axes(element);
            if clip_x || clip_y {
                intersect_overflow_axes(
                    None,
                    overflow_dest_aabb(
                        painted.layout_border,
                        painted.render_rect,
                        &painted.transform,
                    ),
                    clip_x,
                    clip_y,
                )
            } else {
                None
            }
        } else {
            painted.dest_clip
        };

        let parent_tl = [
            abs_x
                + element.style.translate[0]
                + lay_w * element.style.translate_pct[0]
                + painted.list_t[0],
            abs_y
                + element.style.translate[1]
                + lay_h * element.style.translate_pct[1]
                + painted.list_t[1],
        ];
        let parent_scales = painted.scale_layer.is_none()
            && ((painted.paint_sx - 1.0).abs() > 1e-4 || (painted.paint_sy - 1.0).abs() > 1e-4);
        let parent_rotates = !crate::style::mat2d_is_axis_aligned(&painted.local_linear)
            || !crate::style::mat2d_is_axis_aligned(&ancestor_linear);
        let parent_translates = element.style.translate[0].abs() > 0.05
            || element.style.translate[1].abs() > 0.05
            || element.style.translate_pct[0].abs() > 0.001
            || element.style.translate_pct[1].abs() > 0.001
            || painted.list_t[0].abs() > 0.05
            || painted.list_t[1].abs() > 0.05
            || parent_scales
            || parent_rotates;
        let z_order = if any_z {
            let mut order: Vec<usize> = (0..element.children.len()).collect();
            order.sort_by(|&a, &b| {
                element.children[a]
                    .style
                    .z_index
                    .cmp(&element.children[b].style.z_index)
                    .then(a.cmp(&b))
            });
            Some(order)
        } else {
            None
        };

        CollectWalkStart::Ready(CollectWalk {
            child_ancestor_scale: if painted.scale_layer.is_some() {
                [1.0, 1.0]
            } else {
                [painted.paint_sx, painted.paint_sy]
            },
            child_ancestor_linear: painted.world_linear,
            child_sticky_xform: ancestor_sticky_xform
                || crate::renderer::element_wants_sticky_xform(element),
            opacity: painted.opacity,
            clip: child_clip,
            dest_clip,
            child_clip_poly,
            parent_tl,
            parent_scales,
            parent_rotates,
            parent_translates,
            is_carousel_track: element.attrs.get("data-carousel-track").map(|s| s.as_str())
                == Some("1"),
            skip_scroll_band_cull: element_authored_block_is_intrinsic(element),
            any_stacking,
            z_order,
            max_right: abs_x + lay_w,
            max_bottom: abs_y + lay_h,
            next_scroll_acc,
            pass_to_children,
            ancestor_scroll_viewport_rest: ancestor_scroll_viewport_rest.map(str::to_string),
            record_page_stack_bands,
            stack_band_only_layered,
            cap_children,
            cap_inst0,
            cap_tex0,
            cap_text0,
            child_nodes,
            cursor: 0,
            stopped: false,
            painted,
        })
    }

    fn collect_push_portal_band(
        overlay_split: &mut OverlayPaintSplit,
        instances: &Vec<InstanceInput>,
        textured_draws: &Vec<TexturedDraw>,
        text_nodes: &Vec<TextLayoutInfo>,
        input_chrome_instances: &Vec<InstanceInput>,
        paint_ops_len: usize,
        band_root: &Element,
        ancestor_opacity: f32,
        is_body_portal_layer: bool,
        any_stacking: bool,
        record_page_stack_bands: bool,
    ) {
        let band = OverlayPaintBand {
            instance_start: instances.len(),
            textured_start: textured_draws.len(),
            text_start: text_nodes.len(),
            chrome_start: input_chrome_instances.len(),
            paint_op_start: paint_ops_len,
            backdrop_blur: band_root.style.backdrop_blur.max(0.0),
            underlay_opacity: (ancestor_opacity * band_root.style.opacity).clamp(0.0, 1.0),
            // Identity is the portal-layer child. Nested Select/stacking inside
            // DatePicker stays in that child's band so calendar glyphs and
            // ColorPicker internals are not split onto a 0-key suffix that
            // PartialLoad then skips.
            stamp_key: if is_body_portal_layer {
                overlay_portal_stamp_key(band_root)
            } else {
                0
            },
        };
        if is_body_portal_layer {
            overlay_split.bands.push(band);
        } else if overlay_split.marked && any_stacking {
            overlay_split.bands.push(band);
        } else if record_page_stack_bands {
            overlay_split.page_stack_bands.push(band);
        }
    }

    fn collect_emit_child_job(
        &self,
        element: &mut Element,
        walk: &mut CollectWalk,
        i: usize,
        instances: &Vec<InstanceInput>,
        textured_draws: &Vec<TexturedDraw>,
        text_nodes: &Vec<TextLayoutInfo>,
        input_chrome_instances: &Vec<InstanceInput>,
        overlay_split: &mut OverlayPaintSplit,
        paint_rec: &mut PaintRecorder,
    ) -> CollectChildJob {
        paint_rec.split_here();
        let record_page_stack = walk.record_page_stack_bands
            && (!walk.stack_band_only_layered || child_opens_page_stack_band(&element.children[i]));
        Self::collect_push_portal_band(
            overlay_split,
            instances,
            textured_draws,
            text_nodes,
            input_chrome_instances,
            paint_rec.ops.len(),
            &element.children[i],
            walk.opacity,
            walk.painted.is_body_portal_layer,
            walk.any_stacking,
            record_page_stack,
        );
        let thumb_ov = if walk.painted.thumb_rect_for_child.is_some() {
            if element.children[i]
                .focus_id
                .as_deref()
                .is_some_and(|f| f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX))
            {
                walk.painted.thumb_rect_for_child.take()
            } else {
                None
            }
        } else {
            None
        };
        let abs_offset = if let Ok(cl) = self.taffy.layout(walk.child_nodes[i]) {
            walk.child_pass_offset_for(cl.location.x, cl.location.y, cl.size.width, cl.size.height)
        } else {
            walk.parent_tl
        };
        CollectChildJob {
            index: i,
            node: walk.child_nodes[i],
            abs_offset,
            thumb_ov,
        }
    }

    fn collect_take_child(
        &self,
        element: &mut Element,
        walk: &mut CollectWalk,
        _scroll_only: bool,
        scrollbar_overlay_collect: bool,
        instances: &mut Vec<InstanceInput>,
        textured_draws: &mut Vec<TexturedDraw>,
        text_nodes: &mut Vec<TextLayoutInfo>,
        input_chrome_instances: &mut Vec<InstanceInput>,
        overlay_split: &mut OverlayPaintSplit,
        paint_rec: &mut PaintRecorder,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
        clip_culled: &mut HashSet<u64>,
    ) -> Option<CollectChildJob> {
        if walk.stopped {
            return None;
        }
        if let Some(order) = walk.z_order.clone() {
            while walk.cursor < order.len() {
                let i = order[walk.cursor];
                walk.cursor += 1;
                if i >= walk.child_nodes.len() {
                    log::error!(
                        "layout/taffy child desync (z-ordered): element has {} children, taffy node has {}",
                        element.children.len(),
                        walk.child_nodes.len()
                    );
                    continue;
                }
                return Some(self.collect_emit_child_job(
                    element,
                    walk,
                    i,
                    instances,
                    textured_draws,
                    text_nodes,
                    input_chrome_instances,
                    overlay_split,
                    paint_rec,
                ));
            }
            return None;
        }

        let n = element.children.len();
        while walk.cursor < n {
            let i = walk.cursor;
            if i >= walk.child_nodes.len() {
                log::error!(
                    "layout/taffy child desync: element has {} children, taffy node {:?} has {} — skip (portal hoist must full-rebuild)",
                    element.children.len(),
                    element.layout_node,
                    walk.child_nodes.len()
                );
                walk.stopped = true;
                return None;
            }
            if _scroll_only
                && !walk.skip_scroll_band_cull
                && !walk.parent_translates
                && !walk.is_carousel_track
            {
                if let Some(clip_rect) = walk.clip {
                    if !element.children[i].style.position_fixed
                        && element.children[i].style.position != Position::Absolute
                    {
                        if let Ok(cl) = self.taffy.layout(walk.child_nodes[i]) {
                            let child_abs = [
                                walk.parent_tl[0] + cl.location.x,
                                walk.parent_tl[1] + cl.location.y,
                            ];
                            let child_size = [cl.size.width, cl.size.height];
                            let mut band = scroll_sibling_band_action(
                                child_abs,
                                child_size,
                                walk.next_scroll_acc,
                                clip_rect,
                                element.style.flex_direction,
                                element.style.flex_wrap,
                            );
                            if !matches!(band, ScrollSiblingBandAction::Visit) {
                                let focused = crate::core::reactivity::use_signal(
                                    "__rui_button_focus_id",
                                    String::new,
                                )
                                .peek();
                                let keep = match band {
                                    ScrollSiblingBandAction::SkipBefore => {
                                        crate::core::tree::subtree_holds_live_text_edit(
                                            &element.children[i],
                                            focused.as_str(),
                                        )
                                    }
                                    ScrollSiblingBandAction::StopAfter => {
                                        element.children[i..].iter().any(|c| {
                                            crate::core::tree::subtree_holds_live_text_edit(
                                                c,
                                                focused.as_str(),
                                            )
                                        })
                                    }
                                    ScrollSiblingBandAction::Visit => false,
                                };
                                if keep {
                                    band = ScrollSiblingBandAction::Visit;
                                }
                            }
                            match band {
                                ScrollSiblingBandAction::SkipBefore => {
                                    #[cfg(feature = "devtools")]
                                    {
                                        self.sync_devtools_subtree_from_taffy(
                                            &mut element.children[i],
                                            walk.child_nodes[i],
                                            child_abs,
                                            child_size,
                                            walk.next_scroll_acc,
                                        );
                                    }
                                    self.stamp_skipped_sibling_hit_bounds(
                                        &mut element.children[i],
                                        walk.child_nodes[i],
                                        child_abs,
                                        child_size,
                                        scrollbar_overlay_collect,
                                        scroll_signals_cache,
                                    );
                                    Self::note_clip_culled_subtree(
                                        &self.taffy,
                                        walk.child_nodes[i],
                                        clip_culled,
                                    );
                                    let extent = skipped_sibling_extent(
                                        &element.children[i],
                                        child_abs,
                                        child_size,
                                    );
                                    walk.max_right = walk.max_right.max(extent[0]);
                                    walk.max_bottom = walk.max_bottom.max(extent[1]);
                                    walk.cursor += 1;
                                    continue;
                                }
                                ScrollSiblingBandAction::StopAfter => {
                                    for j in i..n.min(walk.child_nodes.len()) {
                                        if let Ok(jl) = self.taffy.layout(walk.child_nodes[j]) {
                                            let j_abs = [
                                                walk.parent_tl[0] + jl.location.x,
                                                walk.parent_tl[1] + jl.location.y,
                                            ];
                                            let j_size = [jl.size.width, jl.size.height];
                                            #[cfg(feature = "devtools")]
                                            {
                                                self.sync_devtools_subtree_from_taffy(
                                                    &mut element.children[j],
                                                    walk.child_nodes[j],
                                                    j_abs,
                                                    j_size,
                                                    walk.next_scroll_acc,
                                                );
                                            }
                                            self.stamp_skipped_sibling_hit_bounds(
                                                &mut element.children[j],
                                                walk.child_nodes[j],
                                                j_abs,
                                                j_size,
                                                scrollbar_overlay_collect,
                                                scroll_signals_cache,
                                            );
                                            Self::note_clip_culled_subtree(
                                                &self.taffy,
                                                walk.child_nodes[j],
                                                clip_culled,
                                            );
                                            let extent = skipped_sibling_extent(
                                                &element.children[j],
                                                j_abs,
                                                j_size,
                                            );
                                            walk.max_right = walk.max_right.max(extent[0]);
                                            walk.max_bottom = walk.max_bottom.max(extent[1]);
                                        }
                                    }
                                    walk.stopped = true;
                                    return None;
                                }
                                ScrollSiblingBandAction::Visit => {}
                            }
                        }
                    }
                }
            }
            walk.cursor += 1;
            return Some(self.collect_emit_child_job(
                element,
                walk,
                i,
                instances,
                textured_draws,
                text_nodes,
                input_chrome_instances,
                overlay_split,
                paint_rec,
            ));
        }
        None
    }

    fn collect_finish_walk(
        &self,
        element: &mut Element,
        node: NodeId,
        instances: &mut Vec<InstanceInput>,
        textured_draws: &mut Vec<TexturedDraw>,
        text_nodes: &mut Vec<TextLayoutInfo>,
        input_chrome_instances: &mut Vec<InstanceInput>,
        scroll_metrics: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
        overlay_split: &mut OverlayPaintSplit,
        paint_rec: &mut PaintRecorder,
        clip_polys: &mut Vec<crate::style::GpuClipPoly>,
        scroll_signals_cache: &mut Option<&mut HashMap<String, ScrollAreaSignalsCached>>,
        walk: CollectWalk,
    ) -> [f32; 2] {
        let CollectWalk {
            painted,
            max_right,
            max_bottom,
            record_page_stack_bands,
            cap_children,
            cap_inst0,
            cap_tex0,
            cap_text0,
            ancestor_scroll_viewport_rest,
            ..
        } = walk;
        let CollectPainted {
            abs_x,
            abs_y,
            opacity,
            parent_clip,
            render_rect,
            visible_rect,
            transform,
            filter_capture,
            scale_layer,
            layout_border,
            layout_size,
            clip,
            ..
        } = painted;

        if record_page_stack_bands {
            overlay_split.page_stack_depth = overlay_split.page_stack_depth.saturating_sub(1);
        }

        let has_cap_payload = instances.len() > cap_inst0
            || textured_draws.len() > cap_tex0
            || text_nodes.len() > cap_text0;
        if cap_children && has_cap_payload {
            let filter_outset = if filter_capture {
                element
                    .style
                    .filter
                    .as_ref()
                    .map(|list| {
                        crate::style::filter_list_paint_outset(list, render_rect[2], render_rect[3])
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let (capture_bounds, draw_rect) = if filter_capture {
                let inflated = inflate_rect(render_rect, filter_outset, filter_outset);
                let b = [
                    inflated[0],
                    inflated[1],
                    inflated[2].max(1.0),
                    inflated[3].max(1.0),
                ];
                (b, b)
            } else if let Some([sx, sy]) = scale_layer {
                // Inner content only — own rounded chrome is live SDF in the main pass.
                // Dest is the scaled padding box (not a shadow-outset AABB around the card).
                // That AABB was the extra rectangular stroke that vanished at rest.
                let [bt, br, bb, bl] = element.style.resolved_border_widths();
                let capture = [
                    layout_border[0] + bl,
                    layout_border[1] + bt,
                    (layout_border[2] - bl - br).max(1.0),
                    (layout_border[3] - bt - bb).max(1.0),
                ];
                let cx = layout_border[0] + layout_border[2] * 0.5;
                let cy = layout_border[1] + layout_border[3] * 0.5;
                (capture, scale_rect_about(capture, sx, sy, cx, cy))
            } else {
                (visible_rect, visible_rect)
            };
            let output_vis = intersect_rect(parent_clip, draw_rect);
            if output_vis[2] <= 0.0 || output_vis[3] <= 0.0 {
                // Fully outside ancestor clip (including filter halo) — drop captured draws.
                instances.truncate(cap_inst0);
                textured_draws.truncate(cap_tex0);
                text_nodes.truncate(cap_text0);
                clip_polys.truncate(cap_inst0);
                paint_rec.truncate_from(cap_inst0, cap_tex0);
            } else if let Some([sx, sy]) = scale_layer {
                // Hairline SDF (Select trigger border, inner cards) stays live, scaled
                // about the card center. A 1px stroke in the blit dest goes thick/dark
                // under linear minification and snaps thin at Coverage.
                let child_tex: Vec<TexturedDraw> = textured_draws.drain(cap_tex0..).collect();
                let mut child_text: Vec<TextLayoutInfo> = text_nodes.drain(cap_text0..).collect();
                // Transparent RT: translucent glyphs (old muted_from_accent alpha, faded icons)
                // composite to gray. Flatten over the live card fill so enter matches settle.
                flatten_zoom_capture_text(&mut child_text, element.style.bg_color);
                textured_draws.truncate(cap_tex0);
                paint_rec.truncate_from(instances.len(), cap_tex0);
                let fade = element.style.opacity.clamp(0.0, 1.0);
                let cx = layout_border[0] + layout_border[2] * 0.5;
                let cy = layout_border[1] + layout_border[3] * 0.5;
                for inst in &mut instances[cap_inst0..] {
                    scale_zoom_child_instance(inst, sx, sy, cx, cy, fade);
                }
                for poly in clip_polys[cap_inst0..].iter_mut() {
                    poly.scale_about(sx, sy, cx, cy);
                }
                let out_key = if !child_tex.is_empty() || !child_text.is_empty() {
                    Some(crate::renderer::queue_zoom_layer_wrap(
                        capture_bounds,
                        Vec::new(),
                        child_tex,
                        child_text,
                    ))
                } else {
                    None
                };
                if let Some(out_key) = out_key {
                    let blit_clip = clip;
                    let blit_fade = (opacity * fade).clamp(0.0, 1.0);
                    if let Some(draw) =
                        TexturedDraw::clipped(draw_rect, blit_clip, out_key, blit_fade)
                    {
                        let tex_i = textured_draws.len();
                        textured_draws.push(draw);
                        paint_rec.push_textured(tex_i, 1);
                    }
                }
            } else {
                let child_insts = instances[cap_inst0..].to_vec();
                let child_tex = textured_draws[cap_tex0..].to_vec();
                instances.truncate(cap_inst0);
                textured_draws.truncate(cap_tex0);
                clip_polys.truncate(cap_inst0);
                paint_rec.truncate_from(cap_inst0, cap_tex0);
                let out_key = if filter_capture {
                    element.style.filter.as_ref().and_then(|list| {
                        crate::renderer::queue_css_filter_wrap(
                            list,
                            capture_bounds,
                            child_insts,
                            child_tex,
                        )
                    })
                } else {
                    crate::renderer::queue_subtree_wrap(
                        &element.attrs,
                        visible_rect,
                        child_insts,
                        child_tex,
                    )
                };
                if let Some(out_key) = out_key {
                    if let Some(draw) = TexturedDraw::clipped(draw_rect, clip, out_key, opacity) {
                        let tex_i = textured_draws.len();
                        textured_draws.push(draw);
                        paint_rec.push_textured(tex_i, 1);
                    }
                }
            }
        }

        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                let [content_w, content_h] = if rest.ends_with("__ta_scroll") {
                    textarea_scrollport_content_size(element, abs_x, abs_y, layout_size)
                } else {
                    let mut content =
                        scroll_viewport_taffy_content_size(&self.taffy, element, node, layout_size);
                    content[1] = cap_intrinsic_scroll_column_content_h(
                        self,
                        element,
                        node,
                        content[1],
                        layout_size.height,
                    );
                    content
                };
                scroll_metrics.push((
                    rest.to_string(),
                    [abs_x, abs_y, layout_size.width, layout_size.height],
                    [
                        (content_w - layout_size.width).max(0.0),
                        (content_h - layout_size.height).max(0.0),
                    ],
                    [content_w, content_h],
                ));
                seed_scroll_signals_from_collect(
                    scroll_signals_cache,
                    rest,
                    layout_size.width,
                    layout_size.height,
                    content_w,
                    content_h,
                );
            }
        }

        let mut extent_r = max_right;
        let mut extent_b = max_bottom;
        let mut nested_scroll_viewport = false;
        if let Some(fid) = element.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                if ancestor_scroll_viewport_rest
                    .as_deref()
                    .is_some_and(|a| a != rest)
                {
                    nested_scroll_viewport = true;
                }
            }
        }
        if nested_scroll_viewport || overflow_contains_axis(element.style.overflow.x) {
            extent_r = abs_x + layout_size.width;
        }
        if nested_scroll_viewport || overflow_contains_axis(element.style.overflow.y) {
            extent_b = abs_y + layout_size.height;
        }

        push_shimmer_box_overlay(
            input_chrome_instances,
            element,
            render_rect,
            parent_clip,
            transform,
            opacity,
        );

        element.collect_extent = [extent_r, extent_b];
        element.collect_extent_valid = true;
        crate::style::stamp_element_used_layout(element);
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
    layout_w: f32,
    layout_h: f32,
    axis: &str,
    sig: &ScrollAreaSignalsCached,
) -> ([f32; 4], [f32; 4]) {
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
    // Overflow vs the *viewport*, not the overlay track box. Absolute left/right
    // (and top/bottom) inset the track by edge-pad + the orthogonal bar, so a
    // 100% row is ~10–16px “wider” than the track and would paint a phantom
    // H-bar after Gallery Match page switches (ViewportsOnly overlay collect).
    let mut can_scroll_x = allow_x && cw > vw + 0.5;
    let mut can_scroll_y = allow_y && ch > vh + 0.5;
    // Maximize / leftover signals: if the live overlay box already covers content,
    // hide even when last-frame vw/vh still look overflowed.
    if layout_w > 1.0 && cw <= layout_w + 0.5 {
        can_scroll_x = false;
    }
    if layout_h > 1.0 && ch <= layout_h + 0.5 {
        can_scroll_y = false;
    }
    let vw_vis = if vw > 1.0 { vw } else { layout_w.max(1.0) };
    let vh_vis = if vh > 1.0 { vh } else { layout_h.max(1.0) };
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

    let inset = 1.0_f32;
    let cross = SCROLLBAR_HIT_SLAB_CROSS.max(base_thickness).max(2.0);
    let thumb_cross = (cross - inset * 2.0).max(1.0);

    // Prefer Taffy's placed box: overlay insets (or 100% flex lanes) already encode
    // the VS Code overlap / outside L-join. Fall back to viewport metrics on first frame.
    let (track_w, track_h) = if layout_w > 1.0 && layout_h > 1.0 {
        (layout_w, layout_h)
    } else {
        match axis {
            "x" => (vw.max(1.0), cross),
            _ => (cross, vh.max(1.0)),
        }
    };
    let track_rect = [base_abs_x, base_abs_y, track_w, track_h];

    let content_inset = scrollbar_track_content_inset(track_el);
    let thumb_rect = match axis {
        "x" => {
            let track_inner = (track_w - inset * 2.0).max(1.0);
            let (tw, ml) = if scrollable {
                let ratio = (vw_vis / cw.max(vw_vis)).clamp(0.0, 1.0);
                let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                let tw = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                let max_offset = (cw - vw_vis).max(0.0);
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
                base_abs_y + content_inset + inset,
                tw,
                thumb_cross,
            ]
        }
        _ => {
            let track_inner = (track_h - inset * 2.0).max(1.0);
            let (th, mt) = if scrollable {
                let ratio = (vh_vis / ch.max(vh_vis)).clamp(0.0, 1.0);
                let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                let th = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                let max_offset = (ch - vh_vis).max(0.0);
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
                base_abs_x + content_inset + inset,
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

fn inflate_rect(rect: [f32; 4], dx: f32, dy: f32) -> [f32; 4] {
    [
        rect[0] - dx,
        rect[1] - dy,
        (rect[2] + dx * 2.0).max(0.0),
        (rect[3] + dy * 2.0).max(0.0),
    ]
}

fn aabb_from_points(pts: &[[f32; 2]]) -> Option<[f32; 4]> {
    if pts.is_empty() {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for [x, y] in pts {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    Some([
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
    ])
}

fn scale_rect_about(rect: [f32; 4], sx: f32, sy: f32, cx: f32, cy: f32) -> [f32; 4] {
    [
        cx + (rect[0] - cx) * sx,
        cy + (rect[1] - cy) * sy,
        (rect[2] * sx).max(1.0),
        (rect[3] * sy).max(1.0),
    ]
}

/// Scale a zoom-layer descendant SDF about the **card** center. `border_width` stays
/// 1px (CSS scale would be a bitmap hairline that reads thick/dark then snaps thin).
fn scale_zoom_child_instance(
    inst: &mut InstanceInput,
    sx: f32,
    sy: f32,
    cx: f32,
    cy: f32,
    fade: f32,
) {
    let scaled = scale_rect_about(
        [
            inst.rect_pos[0],
            inst.rect_pos[1],
            inst.rect_size[0],
            inst.rect_size[1],
        ],
        sx,
        sy,
        cx,
        cy,
    );
    inst.rect_pos = [scaled[0], scaled[1]];
    inst.rect_size = [scaled[2], scaled[3]];
    inst.color[3] *= fade;
    inst.border_color[3] *= fade;
    inst.shadow_color[3] *= fade;
    let s = sx.abs().min(sy.abs());
    inst.radii = inst.radii.map(|r| r * s);
    inst.shadow_offset[0] *= sx;
    inst.shadow_offset[1] *= sy;
    inst.shadow_blur *= s;
    inst.shadow_spread *= s;
    if inst.clip_size[0] >= 0.0 && inst.clip_size[1] >= 0.0 {
        let clip = scale_rect_about(
            [
                inst.clip_pos[0],
                inst.clip_pos[1],
                inst.clip_size[0],
                inst.clip_size[1],
            ],
            sx,
            sy,
            cx,
            cy,
        );
        inst.clip_pos = [clip[0], clip[1]];
        inst.clip_size = [clip[2], clip[3]];
    }
}

/// Composite coverage glyphs over an opaque surface for `__modal_zoom` capture.
///
/// The zoom RT clears to transparent; live Coverage instead blends over the card fill.
/// Leaving glyph alpha < 1 in the blit made AlertDialog descriptions look like theme
/// `muted-foreground` until the compositor dropped the capture at rest.
fn flatten_zoom_capture_text(nodes: &mut [TextLayoutInfo], bg: [f32; 4]) {
    let bg = if bg[3] > 0.01 {
        bg
    } else {
        crate::style::parse_color("card")
    };
    for n in nodes {
        n.style.color = Some(flatten_glyph_over_bg(n.style.color_resolved(), bg));
        if let Some(spans) = n.spans.as_mut() {
            for span in spans {
                if let Some(c) = span.color {
                    span.color = Some(flatten_glyph_over_bg(c, bg));
                }
            }
        }
    }
}

fn flatten_glyph_over_bg(fg: [u8; 4], bg: [f32; 4]) -> [u8; 4] {
    if fg[3] >= 255 {
        return fg;
    }
    let a = fg[3] as f32 / 255.0;
    let mixed = crate::style::color_mix_over_surface(
        [
            fg[0] as f32 / 255.0,
            fg[1] as f32 / 255.0,
            fg[2] as f32 / 255.0,
            1.0,
        ],
        bg,
        a,
    );
    [
        (mixed[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (mixed[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (mixed[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        255,
    ]
}

/// Dialog `__modal_zoom` during `zoom-in-90` / `zoom-out-90`: capture **text/icons** at rest,
/// GPU-scale one inner blit. Own rounded chrome and descendant SDF (Select 1px border)
/// stay live — a capture dest AABB, or a minified 1px stroke in that blit, was the extra
/// rectangular / thick-dark frame that vanished at rest.
///
/// Live CSS scale on each descendant bakes about *that node's* center, so body rows
/// crawl toward/away from the card origin while glyphon `paint_scale` churns the atlas.
/// Children inherit scale `[1,1]` into the RT; the blit matches CSS `transform-origin`.
fn modal_zoom_compositor_scale(el: &Element) -> Option<[f32; 2]> {
    let tid = el.template_id.as_deref()?;
    if !tid.ends_with("__modal_zoom") {
        return None;
    }
    let sx = el.style.scale[0];
    let sy = el.style.scale[1];
    if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
        return None;
    }
    let zoom_kf = el.style.animations.iter().any(|a| {
        matches!(
            a.name.as_str(),
            "zoom-in-90" | "zoom-out-90" | "zoom-in-95" | "zoom-out-95"
        )
    });
    // Drain only after keyframes are cleared. Fill-mode can hold scale=1 while
    // `zoom-in-90` is still listed — dropping then popped dest AABB + bitmap rows.
    if (sx - 1.0).abs() <= 1e-5 && (sy - 1.0).abs() <= 1e-5 && !zoom_kf {
        return None;
    }
    Some([sx, sy])
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
    if let Some(list) = style.filter.as_ref() {
        pad = pad.max(crate::style::filter_list_paint_outset(list, lay_w, lay_h));
    }
    pad
}

/// Clip applied to descendants while capturing a CSS `filter` SourceGraphic.
/// Ancestor overflow is *not* inherited — it clips the filter output after the effect.
fn filter_source_overflow_clip(
    style: &crate::style::Style,
    abs_x: f32,
    abs_y: f32,
    lay_w: f32,
    lay_h: f32,
) -> (Option<[f32; 4]>, crate::style::GpuClipPoly) {
    let (clip_x, clip_y) =
        crate::core::tree::overflow_clip_axes_from(style.overflow.x, style.overflow.y, false);
    if !clip_x && !clip_y {
        return (None, crate::style::GpuClipPoly::NONE);
    }
    let box_r = [abs_x, abs_y, lay_w, lay_h];
    let poly = if clip_x && clip_y && crate::style::radii_affect_clip(style.radii) {
        crate::style::GpuClipPoly::from_rounded_rect(box_r, style.radii)
    } else {
        crate::style::GpuClipPoly::NONE
    };
    (intersect_overflow_axes(None, box_r, clip_x, clip_y), poly)
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

/// Soft white column sheen for `class="shimmer-box"`. Pushed into the input
/// chrome band so it overlays this node's contents (including glyphs). Thin
/// columns, zero radii (rounded slabs read as pills). Text-clip sweep is
/// `class="shimmer"` and must not share this overlay.
fn push_shimmer_box_overlay(
    chrome: &mut Vec<InstanceInput>,
    element: &Element,
    render_rect: [f32; 4],
    parent_clip: Option<[f32; 4]>,
    transform: crate::style::Mat2d,
    opacity: f32,
) {
    let Some(params) = crate::renderer::ShimmerParams::box_from_attrs(&element.attrs) else {
        return;
    };
    let now = crate::style::global_time_s();
    if params.finished(now) {
        return;
    }
    let box_clip = intersect_rect(parent_clip, render_rect);
    if box_clip[2] <= 0.5 || box_clip[3] <= 0.5 {
        return;
    }
    let highlight = crate::renderer::shimmer_box_highlight_rgba(&params, element.style.bg_color);
    let [bx, by, bw, bh] = render_rect;
    // Same quadratic falloff as glyph shimmer — thin columns, not 3 rounded slabs.
    const COLS: usize = 28;
    let col_w = (bw / COLS as f32).max(1.0);
    for i in 0..COLS {
        let u = (i as f32 + 0.5) / COLS as f32;
        let mix = crate::renderer::shimmer_box_mix_at(&params, now, u, bw);
        if mix < 0.02 {
            continue;
        }
        let mut color = highlight;
        color[3] = (highlight[3] * mix * opacity).clamp(0.0, 1.0);
        if color[3] < 0.01 {
            continue;
        }
        let x = bx + i as f32 * col_w;
        chrome.push(
            InstanceInput {
                rect_pos: [x - 0.5, by],
                rect_size: [col_w + 1.0, bh],
                color,
                border_color: [0.0, 0.0, 0.0, 0.0],
                radii: [0.0; 4],
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
                backdrop_blur: 0.0,
            }
            .with_clip(Some(box_clip)),
        );
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

/// Animated `max-height` is the overflow clip. Taffy may still report content
/// height (Expand pin / short Select lists), so occupancy and Overlay dirty
/// clamp to max-size — collect must use the same box or ellipsis labels survive
/// accordion-up after the chrome has shrunk.
fn overflow_clip_layout_border(element: &Element, mut layout_border: [f32; 4]) -> [f32; 4] {
    if let Some(h) =
        crate::layout::dim_length_px(element.style.max_size.height).filter(|h| *h >= 0.0)
    {
        layout_border[3] = layout_border[3].min(h);
    }
    if let Some(w) =
        crate::layout::dim_length_px(element.style.max_size.width).filter(|w| *w >= 0.0)
    {
        layout_border[2] = layout_border[2].min(w);
    }
    layout_border
}

/// Intersect `parent` with `layout` only on axes that clip.
/// `overflow-x: hidden; overflow-y: visible` (static `text-overflow`) must not
/// scissor glyph ink to the line box. Scrollports clip both axes.
fn intersect_overflow_axes(
    parent: Option<[f32; 4]>,
    layout: [f32; 4],
    clip_x: bool,
    clip_y: bool,
) -> Option<[f32; 4]> {
    if !clip_x && !clip_y {
        return parent;
    }
    const UNBOUNDED: [f32; 4] = [-1.0e7, -1.0e7, 2.0e7, 2.0e7];
    let mut r = parent.unwrap_or(UNBOUNDED);
    if clip_x {
        let x0 = layout[0].max(r[0]);
        let x1 = (layout[0] + layout[2]).min(r[0] + r[2]);
        r[2] = (x1 - x0).max(0.0);
        r[0] = x0;
    }
    if clip_y {
        let y0 = layout[1].max(r[1]);
        let y1 = (layout[1] + layout[3]).min(r[1] + r[3]);
        r[3] = (y1 - y0).max(0.0);
        r[1] = y0;
    }
    Some(r)
}

fn stamp_clip_polys(
    clip_polys: &mut Vec<crate::style::GpuClipPoly>,
    instance_count: usize,
    poly: crate::style::GpuClipPoly,
) {
    while clip_polys.len() < instance_count {
        clip_polys.push(poly);
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
    let fill = [x, y, w, h];
    let visible = intersect_rect(clip, fill);
    if visible[2] <= 0.0 || visible[3] <= 0.0 {
        return;
    }
    // Keep full caret/selection geometry in `rect_*`; GPU clip via with_clip.
    // Baking intersect into rect_size made a short scroll/viewport clip shrink
    // the caret into a flat stub that matched the squashed glyph band.
    //
    // SDF vertex AA is +2px. A 2px I-beam's entire body is that fringe, which
    // paints serif caps and src-overs the glyph under the caret (Search「S」叠影).
    // Clip to the visible fill so hairline chrome ink matches dest-reset.
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
            backdrop_blur: 0.0,
        }
        .with_clip(Some(visible)),
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
        style.size_resolved() * scale
    } else {
        style.size_resolved() * crate::style::tailwind_default_leading_scale(style.size_resolved())
    };
    let soft_wrap = style.white_space == crate::style::WhiteSpace::Normal
        && !style.overflow.allows_caret_scroll();
    // wrap=off textarea: viewport-pan like native `<input>` — do not clamp chrome to
    // the content leaf width (selection/caret must track signal `sx`).
    let viewport_nowrap_edit = style.white_space == crate::style::WhiteSpace::Nowrap
        && !style.overflow.allows_caret_scroll();
    let pan = style.overflow.allows_caret_scroll() || viewport_nowrap_edit;
    // Soft-wrap: visual rows from measure layout (matches glyphon soft paint).
    // Nowrap / single-line Hidden: hard-`\n` rows only.
    let soft_ranges: Vec<(usize, usize)> = if soft_wrap {
        crate::text::soft_visual_line_byte_ranges(
            text,
            style.size_resolved(),
            style.weight_resolved(),
            line_height.max(1.0),
            box_w.max(1.0),
            style.wrap_mode.to_line_break_source(),
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
    let caret_h =
        crate::utils::text_field::text_edit_caret_height_px(line_height, style.size_resolved());
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
            hard_lines.get(line_i).copied().unwrap_or((0, ""))
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

    let mut sel_ranges: Vec<(usize, usize)> = Vec::new();
    if let Some(r) = range_opt {
        sel_ranges.push(r);
    }
    for &(sa, sb) in &caret.extra_selections {
        let mut s = sa.min(sb).min(text.len());
        let mut e = sa.max(sb).min(text.len());
        s = text.floor_char_boundary(s);
        e = text.floor_char_boundary(e);
        if s < e {
            sel_ranges.push((s, e));
        }
    }

    for (s, e) in sel_ranges {
        let bg = caret.selection_bg;
        let color = [
            bg[0] as f32 / 255.0,
            bg[1] as f32 / 255.0,
            bg[2] as f32 / 255.0,
            (bg[3] as f32 / 255.0) * opacity,
        ];
        // BiDi / WJ: LTR prefix-width boxes sit in the wrong visual place, so
        // `selection-foreground` paints light glyphs on the card (ink "vanishes").
        let mut painted_visual = false;
        if crate::text::needs_logical_prewrap(text) {
            let source = if soft_wrap {
                style.wrap_mode.to_line_break_source()
            } else {
                crate::text::LineBreakSource::None
            };
            let slabs = crate::text::visual_selection_highlights(
                text,
                style.size_resolved(),
                style.weight_resolved(),
                line_height,
                box_w.max(1.0),
                source,
                s,
                e,
            );
            if !slabs.is_empty() {
                painted_visual = true;
                for (line_i, hx, hw, line_w) in slabs {
                    let align_dx = style.align_inline_offset(line_w, box_w);
                    let sel_w = if pan {
                        hw.max(0.0)
                    } else {
                        hw.min((box_w - hx).max(0.0)).max(0.0)
                    };
                    if sel_w > 0.0 {
                        push_chrome_rect(
                            selection_out,
                            clip,
                            if pan {
                                x + align_dx + hx
                            } else {
                                x + align_dx + hx.max(0.0)
                            },
                            line_top_y(line_i.min(line_count.saturating_sub(1))),
                            sel_w,
                            sel_h,
                            color,
                        );
                    }
                }
            }
        }
        if !painted_visual {
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
                let prefix_w = prefix_width_logical(
                    line_body,
                    from,
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let end_w = prefix_width_logical(
                    line_body,
                    to,
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let line_w = prefix_width_logical(
                    line_body,
                    line_body.len(),
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let align_dx = style.align_inline_offset(line_w, box_w);
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
                            x + align_dx + prefix_w
                        } else {
                            x + align_dx + prefix_w.min(box_w)
                        },
                        line_top_y(line_i),
                        sel_w,
                        sel_h,
                        color,
                    );
                }
            }
        }
    }

    let mut preedit_uls = Vec::with_capacity(
        usize::from(caret.preedit_underline.is_some()) + caret.extra_preedit_underlines.len(),
    );
    if let Some(r) = caret.preedit_underline {
        preedit_uls.push(r);
    }
    preedit_uls.extend_from_slice(&caret.extra_preedit_underlines);
    for (sa, sb) in preedit_uls {
        let mut s = sa.min(sb).min(text.len());
        let mut e = sa.max(sb).min(text.len());
        s = text.floor_char_boundary(s);
        e = text.floor_char_boundary(e);
        if s < e {
            let c = style.color_resolved();
            let color = [
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                (c[3] as f32 / 255.0) * opacity,
            ];
            // Gap under glyphs so the underline is not flush with ink (IME composing).
            const PREEDIT_UL_GAP: f32 = 3.0;
            const PREEDIT_UL_H: f32 = 1.25;
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
                if from >= to {
                    continue;
                }
                let prefix_w = prefix_width_logical(
                    line_body,
                    from,
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let end_w = prefix_width_logical(
                    line_body,
                    to,
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let line_w = prefix_width_logical(
                    line_body,
                    line_body.len(),
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                );
                let align_dx = style.align_inline_offset(line_w, box_w);
                let ul_w = if pan {
                    (end_w - prefix_w).max(0.0)
                } else {
                    (end_w - prefix_w).min(box_w - prefix_w).max(0.0)
                };
                if ul_w > 0.0 {
                    let ul_y = line_origin_y(line_i) + style.size_resolved() + PREEDIT_UL_GAP;
                    push_chrome_rect(
                        selection_out,
                        clip,
                        if pan {
                            x + align_dx + prefix_w
                        } else {
                            x + align_dx + prefix_w.min(box_w)
                        },
                        ul_y,
                        ul_w,
                        PREEDIT_UL_H,
                        color,
                    );
                }
            }
        }
    }

    if !caret.drop_caret && !crate::renderer::caret_blink_visible() {
        return;
    }
    let mut caret_ixs: Vec<usize> = Vec::new();
    // Multi-cursor column select keeps I-beams on the origin column even while
    // each line has a selection; single-caret drag-select still hides the beam.
    if range_opt.is_none() || caret.drop_caret || !caret.extra_carets.is_empty() {
        caret_ixs.push(caret.caret_byte_index);
    }
    caret_ixs.extend(caret.extra_carets.iter().copied());
    for raw_ix in caret_ixs {
        let mut ix = raw_ix.min(text.len());
        ix = text.floor_char_boundary(ix);
        let (line_i, prefix_w, line_w) = if crate::text::needs_logical_prewrap(text) {
            let source = if soft_wrap {
                style.wrap_mode.to_line_break_source()
            } else {
                crate::text::LineBreakSource::None
            };
            let model = crate::text::build_visual_lines(
                text,
                style.size_resolved(),
                style.weight_resolved(),
                line_height.max(1.0),
                box_w.max(1.0),
                source,
            );
            let i = crate::text::caret_visual_line_index(&model.ranges, ix);
            (i, model.caret_x_on_line(ix), model.line_visual_width(i))
        } else {
            let (line_i, local) = line_and_local(ix);
            let (_line_start, line_body) = line_body_at(line_i);
            (
                line_i,
                prefix_width_logical(
                    line_body,
                    local,
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                ),
                prefix_width_logical(
                    line_body,
                    line_body.len(),
                    style.size_resolved(),
                    style.weight_resolved(),
                    line_height,
                ),
            )
        };
        let align_dx = style.align_inline_offset(line_w, box_w);
        let caret_w = 2.0_f32;
        let mut caret_x = x + align_dx + prefix_w;
        if soft_wrap {
            // Soft-wrap: do not clamp to `box_w - caret_w` — that parked the hairline on the
            // last glyph when a visual line filled the leaf (gallery soft-wrap caret bug).
            caret_x = caret_x.max(x);
        } else if !pan {
            caret_x = caret_x.min(x + box_w - caret_w).max(x);
        }
        let c = caret.caret_color.unwrap_or_else(|| style.color_resolved());
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
            shimmer: None,
            paint_scale: 1.0,
            paint_transform: crate::style::MAT2D_IDENTITY,
            paint_origin: [0.0, 0.0],
            glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
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
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: Some((0, text.len())),
            selection_bg: [80, 120, 200, 90],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
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

    /// RTL paragraph: selecting the trailing LTR island must highlight the visual
    /// left cluster, not a logical-prefix box that covers the Hebrew run.
    #[test]
    fn bidi_selection_rects_follow_visual_runs() {
        let mut fs = crate::renderer::font_loader::create_font_system();
        crate::renderer::font_loader::apply_default_families(fs.db_mut());
        crate::text::install_shared_font_system(&mut fs);

        let text = "עברית Hello";
        let Some(lo) = text.find("Hello") else {
            return;
        };
        let hi = lo + "Hello".len();
        let hebrew_prefix = prefix_width_logical(&text[..lo], lo, 14.0, 400, 21.0);
        let lh = 21.0_f32;
        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.white_space = crate::style::WhiteSpace::Normal;
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: hi,
            selection: Some((lo, hi)),
            selection_bg: [0, 0, 0, 255],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: Some([255, 255, 255, 255]),
        });

        let mut selection = Vec::new();
        let mut chrome = Vec::new();
        push_input_chrome_instances(
            &mut selection,
            &mut chrome,
            text,
            &style,
            0.0,
            0.0,
            280.0,
            lh + 4.0,
            None,
            1.0,
        );
        assert!(
            !selection.is_empty(),
            "BiDi selection must emit at least one visual slab"
        );
        let x = selection[0].rect_pos[0];
        assert!(
            x > hebrew_prefix + 1.0,
            "Hello glyphs are past the logical Hebrew prefix (LTR box would cover Hebrew); x={x} hebrew_prefix={hebrew_prefix}"
        );
    }

    #[test]
    fn preedit_underline_sits_below_glyphs_and_caret_still_drawn() {
        let text = "你好hao";
        let lh = 21.0_f32;
        let y = 20.0_f32;
        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0; 4],
            overwrite_highlight: None,
            preedit_underline: Some((6, 9)),
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });
        let mut underline = Vec::new();
        let mut chrome = Vec::new();
        push_input_chrome_instances(
            &mut underline,
            &mut chrome,
            text,
            &style,
            10.0,
            y,
            280.0,
            lh,
            None,
            1.0,
        );
        assert_eq!(underline.len(), 1, "one underline band for the preedit run");
        let ul = &underline[0];
        assert!(
            ul.rect_pos[1] >= y + 14.0 + 2.5,
            "underline must sit below glyphs with a gap (y={}, size=14, got {})",
            y,
            ul.rect_pos[1]
        );
        assert!(
            ul.rect_size[1] < 2.5,
            "preedit underline is a hairline, got h={}",
            ul.rect_size[1]
        );
        assert!(
            !chrome.is_empty(),
            "caret hairline must still draw while composing"
        );
    }

    #[test]
    fn extra_carets_emit_additional_hairlines() {
        let text = "hello";
        let lh = 21.0_f32;
        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: 1,
            selection: None,
            extra_carets: vec![3, 5],
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            selection_bg: [0; 4],
            overwrite_highlight: None,
            preedit_underline: None,
            drop_caret: true,
            caret_color: None,
            selection_foreground: None,
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
            280.0,
            lh,
            None,
            1.0,
        );
        assert_eq!(
            chrome.len(),
            3,
            "primary + two extra carets, got {}",
            chrome.len()
        );
    }

    #[test]
    fn soft_wrap_caret_sits_on_visual_line_not_box_center() {
        // One hard line soft-wrapped into several visual rows: caret must track the
        // visual line (not vertically center in the tall leaf — gallery symptom).
        let text = "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px";
        let lh = 21.0_f32;
        let box_w = 160.0_f32;
        let ranges = crate::text::soft_visual_line_byte_ranges(
            text,
            14.0,
            400,
            lh,
            box_w,
            crate::text::LineBreakSource::SoftWord,
        );
        assert!(
            ranges.len() >= 2,
            "fixture must soft-wrap under {box_w}px, got {} lines",
            ranges.len()
        );
        let caret_byte = text.len();
        let visual_i = crate::text::caret_visual_line_index(&ranges, caret_byte);
        let expected_y = visual_i as f32 * lh; // multi-line origin (no half-box center)

        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.white_space = crate::style::WhiteSpace::Normal;
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: caret_byte,
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
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
    fn wrap_char_caret_x_uses_soft_char_not_soft_word() {
        // Gallery wrap=char sample: SoftChar mid-breaks the long token; SoftWord does not.
        // Chrome must use wrap_mode so the caret before 😃 lands on the SoftChar visual line.
        let text = "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😃 —";
        let lh = 21.0_f32;
        let box_w = 280.0_f32;
        let size = 14.0_f32;
        let emoji = text.find('😃').expect("emoji");
        let soft_char = crate::text::soft_visual_line_byte_ranges(
            text,
            size,
            400,
            lh,
            box_w,
            crate::text::LineBreakSource::SoftChar,
        );
        let soft_word = crate::text::soft_visual_line_byte_ranges(
            text,
            size,
            400,
            lh,
            box_w,
            crate::text::LineBreakSource::SoftWord,
        );
        assert_ne!(
            soft_char, soft_word,
            "fixture requires SoftChar ≠ SoftWord ranges"
        );
        let ci = crate::text::caret_visual_line_index(&soft_char, emoji);
        let (cs, _) = soft_char[ci];
        let expected_x = prefix_width_logical(&text[cs..emoji], emoji - cs, size, 400, lh);

        let mut style = TextStyle::default();
        style.size = Some(size);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.white_space = crate::style::WhiteSpace::Normal;
        style.wrap_mode = crate::text::WrapMode::Char;
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: emoji,
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });

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
                lh * soft_char.len() as f32 + 8.0,
                None,
                1.0,
            );
            if !chrome.is_empty() {
                saw = true;
                break;
            }
        }
        assert!(saw, "wrap=char caret must draw within blink window");
        let caret_x = chrome[0].rect_pos[0];
        let caret_y = chrome[0].rect_pos[1];
        let expected_y = ci as f32 * lh + (lh - chrome[0].rect_size[1]) * 0.5;
        assert!(
            (caret_y - expected_y).abs() < lh * 0.6,
            "wrap=char caret y={caret_y} should sit on SoftChar visual line {ci} (≈{expected_y})"
        );
        assert!(
            (caret_x - expected_x).abs() < 3.0,
            "wrap=char caret x={caret_x} must match SoftChar prefix≈{expected_x} (not SoftWord)"
        );
    }

    #[test]
    fn multiline_caret_slightly_taller_than_line_and_centered() {
        let text = "asd\nadj";
        let lh = 21.0_f32;
        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
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
    fn nowrap_textarea_selection_highlight_follows_viewport_scroll() {
        let text = "abcdefghijklmnop";
        let lh = 21.0_f32;
        let prefix_mid = prefix_width_logical(text, 4, 14.0, 400, lh);
        let prefix_end = prefix_width_logical(text, 8, 14.0, 400, lh);
        let scroll_x = 80.0_f32;
        let layout_x = 20.0_f32;
        let text_x = layout_x - scroll_x;

        let mut style = TextStyle::default();
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.white_space = crate::style::WhiteSpace::Nowrap;
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: 8,
            selection: Some((0, 8)),
            selection_bg: [80, 120, 200, 90],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });

        let mut selection = Vec::new();
        let mut chrome = Vec::new();
        push_input_chrome_instances(
            &mut selection,
            &mut chrome,
            text,
            &style,
            text_x,
            0.0,
            800.0,
            lh,
            None,
            1.0,
        );
        assert_eq!(selection.len(), 1);
        assert!(
            (selection[0].rect_pos[0] - text_x).abs() < 1.5,
            "selection start must track panned text origin"
        );
        assert!(
            (selection[0].rect_size[0] - prefix_end).abs() < 2.0,
            "selection width must stay glyph-tight"
        );

        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: 8,
            selection: Some((4, 8)),
            selection_bg: [80, 120, 200, 90],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });
        selection.clear();
        push_input_chrome_instances(
            &mut selection,
            &mut chrome,
            text,
            &style,
            text_x,
            0.0,
            800.0,
            lh,
            None,
            1.0,
        );
        assert_eq!(selection.len(), 1);
        assert!(
            (selection[0].rect_pos[0] - (text_x + prefix_mid)).abs() < 1.5,
            "mid-line selection must follow sx (x={} expected={})",
            selection[0].rect_pos[0],
            text_x + prefix_mid
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
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.overflow = TextOverflow::Hidden;
        style.scroll_x = scroll_x;
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
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
    fn center_align_caret_offsets_into_box() {
        let text = "42";
        let lh = 21.0_f32;
        let size = 14.0_f32;
        let line_w = prefix_width_logical(text, text.len(), size, 400, lh);
        let box_w = 80.0_f32;
        assert!(
            line_w + 8.0 < box_w,
            "fixture must fit so center offset applies (line_w={line_w})"
        );
        let mut style = TextStyle::default();
        style.size = Some(size);
        style.weight = Some(400);
        style.line_height_px = Some(lh);
        style.color = Some([0, 0, 0, 255]);
        style.overflow = TextOverflow::Hidden;
        style.align = Some(crate::style::TextAlign::Center);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: text.len(),
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });
        let origin = 10.0_f32;
        let want_dx = style.align_inline_offset(line_w, box_w);
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
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        assert!(saw, "center-align caret missing during blink ON");
        let caret_x = chrome[0].rect_pos[0];
        let expected = origin + want_dx + line_w;
        assert!(
            (caret_x - expected).abs() < 1.5,
            "centered caret should sit after ink (x={caret_x}, expected={expected}, dx={want_dx})"
        );
        assert!(
            want_dx > 4.0,
            "center offset should be material for this fixture (dx={want_dx})"
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
            caret_scroll: false,
            writing_mode: crate::style::WritingMode::HorizontalTb,
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
    fn caret_scroll_max_content_ignores_placeholder_intrinsic() {
        let text = "Placeholder…";
        let size = 14.0;
        let lh = 21.0;
        let prepared = crate::text::prepare(text, size, 400, lh);
        let intrinsic = crate::text::intrinsic_width(&prepared);
        assert!(intrinsic > 40.0, "fixture intrinsic={intrinsic}");
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
            caret_scroll: true,
            writing_mode: crate::style::WritingMode::HorizontalTb,
        };
        let known = taffy::geometry::Size {
            width: None,
            height: None,
        };
        let avail = taffy::geometry::Size {
            width: taffy::style::AvailableSpace::MaxContent,
            height: taffy::style::AvailableSpace::MaxContent,
        };
        let out = measured_text_leaf_size(&m, known, avail);
        assert!(
            out.width < 1.0,
            "native <input> max-content must not follow placeholder glyphs ({intrinsic}), got {}",
            out.width
        );
        let min_avail = taffy::geometry::Size {
            width: taffy::style::AvailableSpace::MinContent,
            height: taffy::style::AvailableSpace::MinContent,
        };
        let min_out = measured_text_leaf_size(&m, known, min_avail);
        assert!(
            min_out.width < 1.0,
            "native <input> min-content must not follow placeholder glyphs, got {}",
            min_out.width
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
            caret_scroll: false,
            writing_mode: crate::style::WritingMode::HorizontalTb,
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
        style.size = Some(14.0);
        style.weight = Some(400);
        style.line_height_px = Some(21.0);
        style.color = Some([0, 0, 0, 255]);
        style.input_caret = Some(InputCaretDraw {
            caret_byte_index: caret_byte,
            selection: None,
            selection_bg: [0, 0, 0, 0],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
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
    fn source_slot_after_hoist_is_a_portal_island() {
        let mut panel = Element::new().w(80.0).h(32.0);
        panel.set_attr("rui_portal", "1");
        panel.set_attr(ATTR_PORTAL_KEY, "rx-expand");
        let island = Element::new().w(0.0).h(0.0).child(panel);
        assert!(subtree_declares_portal(&island));

        let mut root = Element::new().w(800.0).h(600.0).child(island);
        assert!(hoist_portal_subtrees(&mut root) >= 1);
        let source = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() != Some(PORTAL_LAYER_ID))
            .expect("source island");
        assert!(
            subtree_declares_portal(source),
            "keep-alive slot must still count as a portal island so close skips FullClear"
        );
        assert!(!subtree_declares_portal(&Element::new().text("page")));
    }

    #[test]
    fn hoisted_popover_damage_is_local_modal_underlay_covers_viewport() {
        let page = Element::new()
            .w(800.0)
            .h(600.0)
            .text("gallery page")
            .template_id("page");

        let mut popover = Element::new().w(72.0).h(36.0).template_id("rx-panel");
        popover.set_attr("rui_portal", "1");
        popover.set_attr(ATTR_PORTAL_KEY, "rx");
        popover.layout_bounds = [120.0, 80.0, 72.0, 36.0];

        let mut root = Element::new().w(800.0).h(600.0).child(page).child(popover);
        assert!(hoist_portal_subtrees(&mut root) >= 1);
        // Hoist does not run Taffy; stamp the laid-out box onto the lifted node.
        if let Some(layer) = root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        {
            for c in &mut layer.children {
                if c.template_id.as_deref() == Some("rx-panel") {
                    c.layout_bounds = [120.0, 80.0, 72.0, 36.0];
                }
            }
        }
        let d = hoisted_portal_damage(&root).expect("popover box");
        assert!((d[2] - 72.0).abs() < 0.5 && (d[3] - 36.0).abs() < 0.5);
        assert!(!crate::app::damage::damage_covers_full(
            [0.0, 0.0, 800.0, 600.0],
            d
        ));

        let mut underlay = Element::new().w(800.0).h(600.0).template_id("dlg-underlay");
        underlay.set_attr("rui_portal", "1");
        underlay.set_attr(ATTR_PORTAL_KEY, "dlg");
        let mut modal_root = Element::new().w(800.0).h(600.0).child(underlay);
        assert!(hoist_portal_subtrees(&mut modal_root) >= 1);
        if let Some(layer) = modal_root
            .children
            .iter_mut()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
        {
            for c in &mut layer.children {
                c.layout_bounds = [0.0, 0.0, 800.0, 600.0];
            }
        }
        let full = hoisted_portal_damage(&modal_root).expect("underlay");
        assert!(crate::app::damage::damage_covers_full(
            [0.0, 0.0, 800.0, 600.0],
            full
        ));
    }

    #[test]
    fn expand_measure_portal_is_not_a_paint_box() {
        let mut panel = Element::new();
        panel.set_attr(ATTR_HOISTED_PORTAL, "1");
        panel.set_attr("rui_floating_expand_measure", "1");
        panel.layout_bounds = [90.0, 140.0, 40.0, 200.0];
        let root = Element::new().child(panel);
        assert!(hoisted_expand_measuring(&root));
        assert!(
            hoisted_portal_boxes(&root).is_empty(),
            "unconstrained measure must not enter last_overlay_boxes"
        );
    }

    #[test]
    fn expand_clip_portal_box_follows_max_height() {
        let mut panel = Element::new();
        panel.set_attr(ATTR_HOISTED_PORTAL, "1");
        panel.layout_bounds = [90.0, 140.0, 40.0, 200.0];
        panel.style.max_size.height = taffy::style::Dimension::length(40.0);
        let root = Element::new().child(panel);
        let boxes = hoisted_portal_boxes(&root);
        assert_eq!(boxes.len(), 1);
        assert!(
            (boxes[0][3] - 40.0).abs() < 0.01,
            "paint box must be the overflow clip, got {}",
            boxes[0][3]
        );
        assert!(!hoisted_expand_measuring(&root));
    }

    #[test]
    fn hoisted_portal_without_handlers_keeps_auto_and_occludes() {
        use crate::app::pointer_events::hit_path;
        use crate::core::tree::PointerEvents;
        use crate::utils::popup::portal_panel;

        // Raw portal surface (no swallow_clicks / no floating attrs): must still
        // claim its box after hoist — never auto-coerce to box-none.
        let mut panel = Element::new()
            .w(200.0)
            .h(200.0)
            .bg([0.0, 1.0, 0.0, 1.0])
            .absolute()
            .left(100.0)
            .top(100.0);
        panel.set_attr(ATTR_PORTAL_KEY, "panel:plain");
        panel = portal_panel(panel, 1001);

        let under = Element::new()
            .w(400.0)
            .h(400.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .focus_id("under-btn")
            .on_click(|| {});

        let mut root = Element::new().w(400.0).h(400.0).child(under).child(panel);
        assert!(hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

        let hoisted = &layer_children(&root)[0];
        assert_eq!(
            hoisted.pointer_events_mode(),
            PointerEvents::Auto,
            "plain portal panel must stay auto after hoist"
        );
        let path = hit_path(&root, 200.0, 200.0);
        assert!(
            path.iter()
                .any(|e| e.portal_key.as_deref() == Some("panel:plain")),
            "hit must land on the hoisted panel; path={:?}",
            path.iter()
                .map(|e| (e.focus_id.clone(), e.portal_key.clone()))
                .collect::<Vec<_>>()
        );
        assert!(
            path.iter()
                .all(|e| e.focus_id.as_deref() != Some("under-btn")),
            "page control under the panel must not receive the hit"
        );
    }

    #[test]
    fn viewport_stage_box_none_still_passes_empty_chrome() {
        use crate::app::pointer_events::hit_path;
        use crate::core::reactivity::Signal;
        use crate::core::tree::PointerEvents;
        use crate::utils::popup::{
            mount_modal_layer, mount_viewport_portal_stage, swallow_clicks, underlay_close_on_click,
        };

        // Special-purpose pass-through: empty stage chrome → underlay, not page.
        let panel = swallow_clicks(
            Element::new()
                .w(120.0)
                .h(80.0)
                .bg([0.0, 1.0, 0.0, 1.0])
                .focus_id("dlg-panel"),
        );
        let stage = Element::new()
            .w("100%")
            .h("100%")
            .items_center()
            .justify_center()
            .child(panel);
        let underlay = underlay_close_on_click(Signal::new(true), 1000);
        let mut root = Element::new().w(400.0).h(400.0).child(mount_modal_layer(
            underlay,
            mount_viewport_portal_stage(stage, 1001),
        ));
        assert!(hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

        let stage_el = layer_children(&root)
            .iter()
            .find(|c| c.attrs.get(ATTR_PORTAL_KEY).map(|s| s.as_str()) == Some("panel:1001"))
            .expect("stage on portal layer");
        assert_eq!(
            stage_el.pointer_events_mode(),
            PointerEvents::BoxNone,
            "viewport stage must keep explicit box-none"
        );

        // Corner of the fullscreen stage (outside the centered panel).
        let path = hit_path(&root, 8.0, 8.0);
        assert!(
            path.iter().any(|e| {
                e.portal_key
                    .as_deref()
                    .is_some_and(|k| k.starts_with("underlay:"))
            }),
            "empty stage chrome must hit underlay; path={:?}",
            path.iter()
                .map(|e| e.portal_key.clone())
                .collect::<Vec<_>>()
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
    fn page_stack_bands_interleave_rects_and_text() {
        let lower = Element::new()
            .bg([1.0, 0.0, 0.0, 1.0])
            .w(120.0)
            .h(80.0)
            .absolute()
            .left(10.0)
            .top(10.0)
            .z_index(1)
            .child(Element::new().text("stack-low"));
        let upper = Element::new()
            .bg([0.0, 0.0, 1.0, 1.0])
            .w(120.0)
            .h(80.0)
            .absolute()
            .left(40.0)
            .top(30.0)
            .z_index(2)
            .child(Element::new().text("stack-high"));
        let mut stage = Element::new()
            .w(400.0)
            .h(200.0)
            .relative()
            .child(lower)
            .child(upper);
        stage
            .attrs
            .insert("data-stacking-context".to_string(), "1".to_string());
        let mut root = Element::new().w(400.0).h(200.0).child(stage);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.page_stack_bands.len() >= 2,
            "expected page_stack_bands, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        let ranges = result.overlay_split.page_stack_band_ranges();
        assert!(ranges.len() >= 2);
        // Highest z is last band; its text must not be in an earlier band.
        let last = ranges.last().unwrap();
        assert!(
            result.text_nodes[last.text.0..last.text.1]
                .iter()
                .any(|t| t.text == "stack-high"),
            "top card text belongs to last page stack band"
        );
        let earlier_has_high = ranges[..ranges.len() - 1].iter().any(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "stack-high")
        });
        assert!(!earlier_has_high);
    }

    #[test]
    fn z_index_children_auto_page_stack_bands_without_attr() {
        // Toast-style pile: z-ordered absolute cards, no data-stacking-context.
        // Without auto bands, all glyphs paint after all rects → text bleed.
        let lower = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(120.0)
            .h(60.0)
            .absolute()
            .left(0.0)
            .top(0.0)
            .z_index(1)
            .child(Element::new().text("toast-low"));
        let upper = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(120.0)
            .h(60.0)
            .absolute()
            .left(0.0)
            .top(20.0)
            .z_index(2)
            .child(Element::new().text("toast-high"));
        let pile = Element::new()
            .w(200.0)
            .h(120.0)
            .relative()
            .child(lower)
            .child(upper);
        let mut root = Element::new().w(400.0).h(200.0).child(pile);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.page_stack_bands.len() >= 2,
            "z-index children must auto-enable page_stack_bands, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        let ranges = result.overlay_split.page_stack_band_ranges();
        let last = ranges.last().unwrap();
        assert!(
            result.text_nodes[last.text.0..last.text.1]
                .iter()
                .any(|t| t.text == "toast-high"),
            "higher z text in last band"
        );
    }

    #[test]
    fn nested_wrapper_z_index_with_stacking_context_keeps_glyphs_in_band() {
        // Drag free-move: relative arena (stacking context) → vstack shell (z) →
        // absolute pill + text. Without bands, both glyphs paint after both shells.
        let pill = |label: &str, x: f32| {
            Element::new()
                .bg([0.2, 0.4, 0.9, 1.0])
                .w(120.0)
                .h(40.0)
                .absolute()
                .left(x)
                .top(20.0)
                .child(Element::new().text(label))
        };
        let shell = |z: i32, pill: Element| Element::new().z_index(z).child(pill);
        let mut arena = Element::new()
            .w(400.0)
            .h(120.0)
            .relative()
            .child(shell(1, pill("drag-low", 10.0)))
            .child(shell(2, pill("drag-high", 50.0)));
        arena.set_attr("data-stacking-context", "1");
        let mut root = Element::new().w(400.0).h(200.0).child(arena);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.page_stack_bands.len() >= 2,
            "expected page_stack_bands for overlapping drag shells, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        let ranges = result.overlay_split.page_stack_band_ranges();
        let last = ranges.last().expect("band");
        assert!(
            result.text_nodes[last.text.0..last.text.1]
                .iter()
                .any(|t| t.text == "drag-high"),
            "front shell glyphs must ride with its rect band"
        );
        let earlier_has_high = ranges[..ranges.len() - 1].iter().any(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "drag-high")
        });
        assert!(
            !earlier_has_high,
            "front glyphs must not sit in an earlier band (text bleed)"
        );
    }

    #[test]
    fn gallery_wrapped_drag_shells_inside_outer_page_stack_occlude_glyphs() {
        // Gallery Arbitrary-move: ScrollArea scrollbar (z=1000) already owns
        // page_stack_depth. DragProvider is a stacking-context whose only child
        // is a relative arena; item z-index lives on nested shells. Without a
        // stacking-context on each shell, both pills share one band and the
        // lower glyph paints after the upper fill.
        let pill = |label: &str, x: f32, y: f32, color: [f32; 4]| {
            Element::new()
                .bg(color)
                .w(176.0)
                .h(92.0)
                .absolute()
                .left(x)
                .top(y)
                .child(Element::new().text(label))
        };
        let shell = |z: i32, pill: Element| {
            let mut s = Element::new().z_index(z).child(pill);
            s.set_attr("data-stacking-context", "1");
            s
        };
        let arena = Element::new()
            .w(400.0)
            .h(200.0)
            .relative()
            .child(shell(1, pill("drag-low", 16.0, 20.0, [0.1, 0.2, 0.9, 1.0])))
            .child(shell(
                2,
                pill("drag-high", 88.0, 52.0, [0.9, 0.1, 0.1, 1.0]),
            ));
        let mut provider = Element::new().child(arena);
        provider.set_attr("data-stacking-context", "1");
        let bar = Element::new()
            .w(8.0)
            .h(80.0)
            .absolute()
            .right(0.0)
            .z_index(1000)
            .bg([0.0, 0.0, 0.0, 0.4]);
        let mut page = Element::new()
            .w(400.0)
            .h(240.0)
            .relative()
            .child(provider)
            .child(bar);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut page, 400.0, 240.0, [0.0, 0.0]);
        let ranges = result.overlay_split.page_stack_band_ranges();
        let low_band = ranges.iter().position(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "drag-low")
        });
        let high_band = ranges.iter().rposition(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "drag-high")
        });
        assert!(
            high_band.is_some_and(|h| low_band.is_some_and(|l| h > l)),
            "upper pill glyphs must paint in a later band than lower glyphs (high={high_band:?} low={low_band:?} bands={})",
            ranges.len()
        );
        let same_band = low_band.zip(high_band).is_some_and(|(l, h)| l == h);
        assert!(
            !same_band,
            "sharing a band paints both fills then both glyphs (text bleed)"
        );
    }

    #[test]
    fn nested_overlay_z_index_does_not_append_page_stack_bands() {
        // Gallery: outer ScrollArea overlay bar (z=1000) already owns page_stack_bands.
        // A nested scroller with its own overlay bar must not push more splits — that
        // mis-assigns glyphs vs rects (background scrolls, text stays).
        let nested_bar = Element::new()
            .w(8.0)
            .h(40.0)
            .absolute()
            .right(0.0)
            .z_index(1000)
            .bg([0.0, 0.0, 0.0, 0.4]);
        let nested = Element::new()
            .w(160.0)
            .h(80.0)
            .relative()
            .child(Element::new().text("inner-glyph"))
            .child(nested_bar);
        let outer_bar = Element::new()
            .w(8.0)
            .h(80.0)
            .absolute()
            .right(0.0)
            .z_index(1000)
            .bg([0.0, 0.0, 0.0, 0.4]);
        let mut outer = Element::new()
            .w(200.0)
            .h(120.0)
            .relative()
            .child(nested)
            .child(outer_bar);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut outer, 200.0, 120.0, [0.0, 0.0]);
        assert_eq!(
            result.overlay_split.page_stack_bands.len(),
            2,
            "nested overlay z-index must not append extra page_stack_bands, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        assert!(
            result.text_nodes.iter().any(|t| t.text == "inner-glyph"),
            "nested content glyphs must still collect"
        );
    }

    #[test]
    fn nested_stacking_context_does_not_band_each_in_flow_child() {
        // DragProvider is a stacking-context whose body is often the sortable
        // vstack itself. Under Gallery's ScrollArea (already page_stack_depth),
        // banding every in-flow row reshapes labels on scroll.
        const N: usize = 12;
        let mut list = Element::new().w(200.0).relative();
        list.set_attr("data-stacking-context", "1");
        for i in 0..N {
            list = list.child(Element::new().text(format!("row-{i}")).w(160.0).h(20.0));
        }
        let bar = Element::new()
            .w(8.0)
            .h(80.0)
            .absolute()
            .right(0.0)
            .z_index(1000)
            .bg([0.0, 0.0, 0.0, 0.4]);
        let mut page = Element::new()
            .w(240.0)
            .h(280.0)
            .relative()
            .child(list)
            .child(bar);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut page, 240.0, 280.0, [0.0, 0.0]);
        let bands = result.overlay_split.page_stack_bands.len();
        assert!(
            bands <= 4,
            "nested stacking-context must not open one band per in-flow row; bands={bands} rows={N}"
        );
        assert!(
            (0..N).all(|i| result
                .text_nodes
                .iter()
                .any(|t| t.text == format!("row-{i}"))),
            "in-flow row glyphs must still collect"
        );
    }

    #[test]
    fn nested_stacking_context_can_split_inside_outer_page_bands() {
        // Jump button inside a gallery ScrollArea: explicit stacking-context must
        // subdivide the current band so later-z chrome paints after message glyphs.
        let jump = Element::new()
            .w(80.0)
            .h(28.0)
            .absolute()
            .bottom(8.0)
            .z_index(20)
            .bg([1.0, 1.0, 1.0, 1.0])
            .child(Element::new().text("Latest"));
        let mut nested = Element::new()
            .w(160.0)
            .h(80.0)
            .relative()
            .child(Element::new().text("inner-glyph"))
            .child(jump);
        nested
            .attrs
            .insert("data-stacking-context".to_string(), "1".to_string());
        let outer_bar = Element::new()
            .w(8.0)
            .h(80.0)
            .absolute()
            .right(0.0)
            .z_index(1000)
            .bg([0.0, 0.0, 0.0, 0.4]);
        let mut outer = Element::new()
            .w(200.0)
            .h(120.0)
            .relative()
            .child(nested)
            .child(outer_bar);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut outer, 200.0, 120.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.page_stack_bands.len() > 2,
            "nested stacking-context must add splits, got {}",
            result.overlay_split.page_stack_bands.len()
        );
        let ranges = result.overlay_split.page_stack_band_ranges();
        let latest_band = ranges.iter().rposition(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "Latest")
        });
        let glyph_band = ranges.iter().position(|r| {
            result.text_nodes[r.text.0..r.text.1]
                .iter()
                .any(|t| t.text == "inner-glyph")
        });
        assert!(
            latest_band.is_some_and(|j| glyph_band.is_some_and(|g| j > g)),
            "Latest must paint in a later band than inner-glyph (latest={latest_band:?} glyph={glyph_band:?})"
        );
    }

    #[test]
    fn portal_nested_z_stack_extends_overlay_bands_not_page_stack() {
        // Toast lives under `__rui_portal_layer`. Nested z-cards must become *portal*
        // bands; page_stack_bands end at the portal split and would drop overlay glyphs.
        let lower = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(120.0)
            .h(60.0)
            .absolute()
            .left(0.0)
            .top(0.0)
            .z_index(1)
            .child(Element::new().text("portal-low"));
        let upper = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(120.0)
            .h(60.0)
            .absolute()
            .left(0.0)
            .top(24.0)
            .z_index(2)
            .child(Element::new().text("portal-high"));
        let pile = Element::new()
            .w(200.0)
            .h(120.0)
            .relative()
            .child(lower)
            .child(upper);
        let host = Element::new().w(400.0).h(300.0).child(pile);
        let layer = Element::new()
            .w(400.0)
            .h(300.0)
            .template_id(PORTAL_LAYER_ID)
            .child(host);
        let mut root = Element::new().w(400.0).h(300.0).child(layer);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.marked,
            "portal layer must mark overlay split"
        );
        assert!(
            result.overlay_split.page_stack_bands.is_empty(),
            "nested portal z-stack must not use page_stack_bands (got {})",
            result.overlay_split.page_stack_bands.len()
        );
        assert!(
            result.overlay_split.bands.len() >= 3,
            "expect host band + 2 z-card bands, got {}",
            result.overlay_split.bands.len()
        );
        let ranges = result.overlay_split.band_ranges(
            result.instances.len(),
            result.textured_draws.len(),
            result.text_nodes.len(),
            result.input_chrome_instances.len(),
            result.paint_ops.len(),
        );
        let last = ranges.last().unwrap();
        assert!(
            result.text_nodes[last.text.0..last.text.1]
                .iter()
                .any(|t| t.text == "portal-high"),
            "highest z toast text must be in the last overlay band"
        );
    }

    #[test]
    fn translated_opaque_shell_keeps_child_text_aligned() {
        // Shell translate must not be applied twice (rect_pos + matrix tx/ty).
        let mut shell = Element::new()
            .bg([1.0, 1.0, 1.0, 1.0])
            .w(160.0)
            .h(48.0)
            .absolute()
            .left(10.0)
            .top(10.0)
            .child(Element::new().text("shell-label").w(140.0).h(20.0));
        shell.style.translate = [0.0, 40.0];
        let mut root = Element::new().w(400.0).h(200.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        let shell_inst = result
            .instances
            .iter()
            .find(|i| i.color[0] > 0.9 && i.rect_size[1] > 40.0)
            .expect("opaque shell instance");
        let label = result
            .text_nodes
            .iter()
            .find(|t| t.text == "shell-label")
            .expect("label text");
        // Text top should sit inside the translated shell, not ~40px above it.
        assert!(
            (label.y - shell_inst.rect_pos[1]).abs() < 24.0,
            "text y={:.1} drifted from shell y={:.1} (double-translate?)",
            label.y,
            shell_inst.rect_pos[1]
        );
        // Matrix tx/ty must stay ~0; translation lives in rect_pos only.
        let tx = shell_inst.transform[4];
        let ty = shell_inst.transform[5];
        assert!(
            tx.abs() < 0.01 && ty.abs() < 0.01,
            "transform tx/ty should be 0 when translate is baked into rect_pos, got [{tx}, {ty}]"
        );
    }

    #[test]
    fn rotate_origin_left_top_moves_shader_pivot() {
        fn collect_shell(origin: &str) -> crate::style::Mat2d {
            let mut shell = Element::new()
                .bg([0.9, 0.1, 0.1, 1.0])
                .w(100.0)
                .h(100.0)
                .absolute()
                .left(0.0)
                .top(0.0);
            shell.style.rotate_deg = 90.0;
            shell.style.transform_origin = crate::style::parse_transform_origin(origin);
            let mut root = Element::new().w(200.0).h(200.0).child(shell);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
            result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.8 && i.rect_size[0] > 80.0)
                .map(|i| i.transform)
                .expect("shell instance")
        }
        let center = collect_shell("center");
        let left_top = collect_shell("left top");
        assert!(
            center[4].abs() < 0.05 && center[5].abs() < 0.05,
            "default origin must keep shader tx/ty at 0, got {center:?}"
        );
        assert!(
            (left_top[4] + 100.0).abs() < 1.0 && left_top[5].abs() < 1.0,
            "left-top 90° on 100×100 must put origin into VS tx/ty (-100, 0), got {left_top:?}"
        );
        for i in 0..4 {
            assert!(
                (center[i] - left_top[i]).abs() < 1e-3,
                "origin must not change the 2×2 linear part"
            );
        }
    }

    #[test]
    fn rotate_origin_left_top_keeps_shell_and_child_text_together() {
        fn collect(origin: &str) -> (crate::style::Mat2d, [f32; 4], crate::style::Mat2d) {
            let mut shell = Element::new()
                .bg([0.2, 0.4, 0.9, 1.0])
                .w(120.0)
                .h(48.0)
                .absolute()
                .left(40.0)
                .top(40.0)
                .child(Element::new().text("shell-label").w(100.0).h(24.0));
            shell.style.rotate_deg = 45.0;
            shell.style.transform_origin = crate::style::parse_transform_origin(origin);
            let mut root = Element::new().w(400.0).h(300.0).child(shell);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
            let xf = result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.15 && i.rect_size[1] > 40.0)
                .map(|i| i.transform)
                .expect("opaque shell");
            let bake = result
                .textured_draws
                .iter()
                .find(|d| {
                    d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                        || d.texture_key
                            .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
                })
                .expect("baked child text");
            (xf, bake.rect, bake.transform)
        }
        let (center_xf, center_rect, center_bake_xf) = collect("center");
        let (lt_xf, lt_rect, lt_bake_xf) = collect("left top");
        assert!(
            center_xf[4].abs() < 0.05 && center_xf[5].abs() < 0.05,
            "center origin shell txty must stay 0, got {center_xf:?}"
        );
        assert!(
            lt_xf[4].abs() > 1.0 || lt_xf[5].abs() > 1.0,
            "left-top origin must move the shell shader pivot, got {lt_xf:?}"
        );
        let dcx = (center_rect[0] + center_rect[2] * 0.5) - (lt_rect[0] + lt_rect[2] * 0.5);
        let dcy = (center_rect[1] + center_rect[3] * 0.5) - (lt_rect[1] + lt_rect[3] * 0.5);
        assert!(
            dcx.abs() > 1.0 || dcy.abs() > 1.0,
            "child bake must orbit CSS origin, not stay at the center-pivot rect; center={center_rect:?} left-top={lt_rect:?}"
        );
        // Child itself has default origin (center): its VS txty stays 0; orbit is rect_pos.
        assert!(
            center_bake_xf[4].abs() < 0.05 && center_bake_xf[5].abs() < 0.05,
            "child bake txty at default origin must stay 0, got {center_bake_xf:?}"
        );
        assert!(
            lt_bake_xf[4].abs() < 0.05 && lt_bake_xf[5].abs() < 0.05,
            "child bake txty must stay 0 (orbit via rect); parent txty is on the shell, got {lt_bake_xf:?}"
        );
    }

    #[test]
    fn scale_origin_left_top_moves_child_rect() {
        fn child_x(origin: &str) -> f32 {
            let mut shell = Element::new()
                .bg([0.2, 0.4, 0.9, 1.0])
                .w(100.0)
                .h(40.0)
                .absolute()
                .left(20.0)
                .top(20.0)
                .child(
                    Element::new()
                        .bg([0.9, 0.2, 0.2, 1.0])
                        .w(20.0)
                        .h(20.0)
                        .absolute()
                        .left(10.0)
                        .top(10.0),
                );
            shell.style.scale = [2.0, 1.0];
            shell.style.transform_origin = crate::style::parse_transform_origin(origin);
            let mut root = Element::new().w(400.0).h(200.0).child(shell);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
            result
                .instances
                .iter()
                .find(|i| i.color[0] > 0.8 && i.color[1] < 0.35)
                .map(|i| i.rect_pos[0])
                .unwrap_or_else(|| {
                    panic!(
                        "child fill missing; instances={:?}",
                        result
                            .instances
                            .iter()
                            .map(|i| (i.color, i.rect_pos, i.rect_size))
                            .collect::<Vec<_>>()
                    )
                })
        }
        let center = child_x("center");
        let left_top = child_x("left top");
        assert!(
            (center - left_top).abs() > 1.0,
            "child under scaled parent must follow origin, center_x={center} left_top_x={left_top}"
        );
    }

    #[test]
    fn rotated_parent_propagates_paint_transform_to_child_text() {
        let mut shell = Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(120.0)
            .h(48.0)
            .absolute()
            .left(40.0)
            .top(40.0)
            .child(Element::new().text("rotate-me").w(100.0).h(24.0));
        shell.style.rotate_deg = 45.0;
        let mut root = Element::new().w(400.0).h(300.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
        let cos45 = 45.0f32.to_radians().cos();
        // Sticky Bake: rotated text is a textured quad, not a glyphon TextLayoutInfo.
        assert!(
            result.text_nodes.iter().all(|t| t.text != "rotate-me"),
            "Bake path must not push glyphon text_nodes for rotated labels"
        );
        let bake = result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .expect("baked text textured draw");
        assert!(
            !crate::style::mat2d_is_axis_aligned(&bake.transform),
            "child bake must inherit parent rotate via textured transform, got {:?}",
            bake.transform
        );
        assert!(
            (bake.transform[0] - cos45).abs() < 0.05,
            "expected ~45° linear a={}, got {}",
            cos45,
            bake.transform[0]
        );
        // Draw height must include leading + pad so glyphs aren't vertically clipped.
        let min_h = 24.0 * crate::style::tailwind_default_leading_scale(16.0);
        // Child text "rotate-me" uses default ~16px; pad expands beyond layout 24px height.
        assert!(
            bake.rect[3] + 0.5 >= 24.0,
            "bake draw h={} should be at least layout text h",
            bake.rect[3]
        );
        let _ = min_h;
    }

    #[test]
    fn rotated_medium_label_bake_pads_synthetic_bold() {
        // DragOverlay rotate (gallery 四角) bakes the Label. font-medium 500 on
        // static Segoe uses Regular + synth halo — the bake quad must grow so the
        // halo is not clipped (Coverage already had the 0.18/0.32 stamp).
        fn bake_h(weight: u16) -> f32 {
            let mut t = crate::utils::text_node::apply_label_defaults(
                Element::new().text("拖到四角").w(120.0).h(24.0),
            );
            t.text_style.weight = Some(weight);
            let mut shell = Element::new()
                .w(160.0)
                .h(48.0)
                .absolute()
                .left(10.0)
                .top(10.0)
                .child(t);
            shell.style.rotate_deg = 15.0;
            let mut root = Element::new().w(400.0).h(200.0).child(shell);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
            result
                .textured_draws
                .iter()
                .find(|d| {
                    d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                        || d.texture_key
                            .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
                })
                .map(|d| d.rect[3])
                .expect("baked label")
        }
        let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
            let family = crate::renderer::text_hit::best_font_family(fs);
            crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
        });
        if !needs_synth {
            return;
        }
        let h400 = bake_h(400);
        let h500 = bake_h(500);
        assert!(
            h500 > h400 + 1.0,
            "synth-bold bake must expand shadow pad, 400={h400} 500={h500}"
        );
    }

    #[test]
    fn rotated_label_bake_height_uses_leading_none_not_text_sm() {
        // DragOverlay 四角 ghost clones the Label (`leading-none` / 1em). Bake
        // must not grow the quad to Tailwind text-sm leading (~1.43em) — that
        // extra line box half-leads the glyphs ~1px down vs Coverage.
        let mut t = crate::utils::text_node::apply_label_defaults(
            Element::new().text("拖到四角").white_space("nowrap"),
        );
        t.style.rotate_deg = 15.0;
        // Shrink-wrap: a stretched 200px leaf would hide the 1em vs 1.43em delta.
        t = t.absolute().left(10.0).top(10.0);
        let mut root = Element::new().w(400.0).h(200.0).child(t);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
        let bake = result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .expect("baked label");
        let font = 14.0_f32;
        let ink_pad = crate::utils::text_texture::text_xform_pad_px(font);
        let needs_synth = crate::text::measure::with_layout_font_system(|fs| {
            let family = crate::renderer::text_hit::best_font_family(fs);
            crate::utils::font::resolve_face_weight(fs.db(), family, 500).synthetic_bold
        });
        let extra = if needs_synth {
            let (_, off, blur) = crate::utils::font::synthetic_bold_shadow([20, 20, 20, 255], 1.0);
            crate::utils::text_texture::text_xform_shadow_pad(off, blur)
        } else {
            0.0
        };
        let pad = ink_pad + extra;
        let none_h = font + pad * 2.0;
        let sm_h = font * crate::style::tailwind_default_leading_scale(font) + pad * 2.0;
        assert!(
            (bake.rect[3] - none_h).abs() < 1.0,
            "leading-none bake h={} must match 1em+pad ({none_h}), not text-sm ({sm_h})",
            bake.rect[3]
        );
        assert!(
            bake.rect[3] + 1.0 < sm_h,
            "must not use text-sm leading for Label ghost (h={} sm={sm_h})",
            bake.rect[3]
        );
    }

    #[test]
    fn rotated_flex_centered_caption_bake_pivot_matches_layout_box() {
        let mut shell = Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(120.0)
            .h(48.0)
            .items_center()
            .justify_center()
            .absolute()
            .left(40.0)
            .top(40.0)
            .child({
                let mut t = Element::new().text("倾斜按钮").w(90.0).h(28.0);
                t.text_style.size = Some(14.0);
                t.text_style.align = Some(crate::style::TextAlign::Center);
                t
            });
        shell.style.rotate_deg = -15.0;
        let mut root = Element::new().w(400.0).h(300.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [0.0, 0.0]);
        let caption = root
            .children
            .first()
            .and_then(|s| s.children.first())
            .expect("caption");
        let [lx, ly, lw, lh] = caption.layout_bounds;
        let layout_cx = lx + lw * 0.5;
        let layout_cy = ly + lh * 0.5;
        let bake = result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .expect("baked caption");
        let bake_cx = bake.rect[0] + bake.rect[2] * 0.5;
        let bake_cy = bake.rect[1] + bake.rect[3] * 0.5;
        assert!(
            (bake_cx - layout_cx).abs() < 1.5 && (bake_cy - layout_cy).abs() < 1.5,
            "bake pivot ({bake_cx:.1},{bake_cy:.1}) must match layout box center ({layout_cx:.1},{layout_cy:.1})"
        );
    }

    #[test]
    fn bake_draw_taller_than_tight_font_box() {
        let mut shell = Element::new()
            .bg([0.2, 0.4, 0.9, 1.0])
            .w(100.0)
            .h(40.0)
            .absolute()
            .left(10.0)
            .top(10.0)
            .child({
                let mut t = Element::new().text("45°").w(40.0).h(12.0);
                t.text_style.size = Some(12.0);
                t
            });
        shell.style.rotate_deg = 45.0;
        let mut root = Element::new().w(200.0).h(200.0).child(shell);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
        let bake = result
            .textured_draws
            .iter()
            .find(|d| {
                d.texture_key
                    .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
                    || d.texture_key
                        .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
            })
            .expect("bake draw");
        let leading = 12.0 * crate::style::tailwind_default_leading_scale(12.0);
        let pad = crate::utils::text_texture::text_xform_pad_px(12.0);
        assert!(
            bake.rect[3] + 0.01 >= leading + pad * 2.0,
            "bake h={} must cover leading+pad ({}), got clip risk",
            bake.rect[3],
            leading + pad * 2.0
        );
    }

    #[cfg(feature = "bundled-lucide")]
    #[test]
    fn rotated_lucide_chevron_uses_square_texture_not_glyphon() {
        let glyph = crate::utils::lucide_glyph("chevron-down").expect("glyph");
        let mut icon = crate::utils::build_named_icon_text_node("chevron-down", glyph)
            .text_size(16.0)
            .w(16.0)
            .h(16.0)
            .flex_shrink(0.0)
            .text_color([128, 128, 128, 255]);
        icon.style.rotate_deg = 180.0;
        let mut root = Element::new().w(64.0).h(64.0).child(icon);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 64.0, 64.0, [0.0, 0.0]);
        let draw = result
            .textured_draws
            .iter()
            .find(|d| d.texture_key.starts_with("rui_lucide_"))
            .expect("rotated lucide must use icon texture");
        assert!(
            (draw.rect[2] - draw.rect[3]).abs() < 0.01,
            "icon quad must be square, got {}x{}",
            draw.rect[2],
            draw.rect[3]
        );
        assert!(
            (draw.rect[2] - 16.0).abs() < 0.5,
            "draw side must match text size, got {}",
            draw.rect[2]
        );
        assert!(
            (draw.transform[0] + 1.0).abs() < 0.05 && (draw.transform[3] + 1.0).abs() < 0.05,
            "180° linear should be -I, got {:?}",
            draw.transform
        );
        assert!(
            result
                .text_nodes
                .iter()
                .all(|t| t.style.font_family.as_deref() != Some("lucide")),
            "must not also emit glyphon lucide (double-draw thickens strokes)"
        );
    }

    /// Scaled parent under scroll must pass pre-scroll `abs_offset` to children.
    /// Using post-scroll `render_*` double-subtracted scroll and culled CarouselStack sides.
    #[test]
    fn scaled_parent_under_scroll_keeps_child_paint_aligned() {
        let mut child = Element::new()
            .w(100.0)
            .h(100.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .template_id("scaled-child");
        child = child.child(Element::new().text("scale-me"));
        let mut shell = Element::new()
            .w(100.0)
            .h(100.0)
            .bg([0.0, 0.0, 0.0, 0.0])
            .child(child);
        shell.style.scale = [0.8, 0.8];
        // `overflow: scroll` (not hidden) contributes `scroll_offset` into child `scroll_acc`.
        let mut root = Element::new()
            .w(200.0)
            .h(200.0)
            .overflow_y("scroll")
            .overflow_x("scroll")
            .child(shell);
        root.scroll_offset = [0.0, 50.0];

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);

        let red = result
            .instances
            .iter()
            .find(|i| i.color[0] > 0.9 && i.color[1] < 0.1 && i.rect_size[0] > 1.0)
            .expect("scaled red child instance");
        // Layout top 0 − scroll 50 = −50; scale 0.8 about center → visual top −50 + 10 = −40.
        assert!(
            (red.rect_pos[1] - (-40.0)).abs() < 2.0,
            "child visual top must be ~-40 after one scroll subtract (not -90); got {}",
            red.rect_pos[1]
        );
        assert!(
            (red.rect_size[0] - 80.0).abs() < 2.0,
            "child width must bake ancestor scale 0.8 → 80; got {}",
            red.rect_size[0]
        );
        assert!(
            result.text_nodes.iter().any(|t| t.text == "scale-me"),
            "text under scaled+scrolled parent must still collect"
        );
        let label = result
            .text_nodes
            .iter()
            .find(|t| t.text == "scale-me")
            .expect("scale-me text");
        assert!(
            (label.paint_scale - 0.8).abs() < 0.02,
            "CSS scale must go to paint_scale (not font size); got {}",
            label.paint_scale
        );
        // Default text size is not multiplied by 0.8 (that was the drag-lag path).
        assert!(
            label.style.size_resolved() > 10.0,
            "layout font size must stay unscaled for stable reshape; got {}",
            label.style.size_resolved()
        );
    }

    /// `__modal_zoom` at 0.90: live rounded chrome + one inner blit of rest children.
    /// Fade lives on chrome and the blit; the RT stays content-stable.
    #[test]
    fn modal_zoom_layer_paints_as_one_scaled_capture() {
        let _ = crate::renderer::take_capture_jobs();
        let title = Element::new().text("Dialog title");
        let mut card = Element::new()
            .w(240.0)
            .h(120.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .child(title);
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        card.style.opacity = 0.5;
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");
        assert!(
            (cap.bounds[2] - 240.0).abs() < 0.5,
            "capture is rest layout (no shadow pad); got {}",
            cap.bounds[2]
        );
        let live = result
            .instances
            .iter()
            .find(|i| i.rect_size[0] > 100.0)
            .expect("live chrome in main pass");
        assert!(
            (live.rect_size[0] - 216.0).abs() < 1.0,
            "live chrome bakes CSS scale; got {}",
            live.rect_size[0]
        );
        assert!(
            (live.color[3] - 0.5).abs() < 0.05,
            "fade lives on live chrome; a={}",
            live.color[3]
        );
        let blit = result
            .textured_draws
            .iter()
            .find(|d| d.texture_key.starts_with("rui_zoom_cap:"))
            .expect("zoom blit dest");
        assert!(
            (blit.rect[2] - 216.0).abs() < 0.5,
            "dest w should be layout×0.90 about card center; got {}",
            blit.rect[2]
        );
        assert!(
            (blit.opacity - 0.5).abs() < 1e-3,
            "fade lives on the blit; got {}",
            blit.opacity
        );
        let title = cap
            .text_nodes
            .iter()
            .find(|t| t.text == "Dialog title")
            .expect("title in capture at rest");
        assert!(
            (title.paint_scale - 1.0).abs() < 1e-4,
            "capture freezes CSS scale; got {}",
            title.paint_scale
        );
        assert!(
            result.text_nodes.iter().all(|t| t.text != "Dialog title"),
            "title must not also paint in the main pass"
        );

        root.children[0].style.scale = [0.995, 0.995];
        let _ = crate::renderer::take_capture_jobs();
        let near_one = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        assert!(
            crate::renderer::take_capture_jobs()
                .iter()
                .any(|c| c.key.starts_with("rui_zoom_cap:")),
            "scale 0.995 must stay on the compositor (1e-3 used to drain early)"
        );
        assert!(near_one
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("rui_zoom_cap:")));

        root.children[0].style.scale = [1.0, 1.0];
        root.children[0].style.animations = vec![crate::style::keyframes_anim_entry(
            "zoom-in-90",
            200.0,
            crate::style::TimingFunction::Ease,
            crate::style::AnimationFillMode::Both,
        )];
        let _ = crate::renderer::take_capture_jobs();
        let at_one = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        assert!(
            crate::renderer::take_capture_jobs()
                .iter()
                .any(|c| c.key.starts_with("rui_zoom_cap:")),
            "fill-mode scale=1 must keep the 1:1 blit until zoom-in-90 is cleared"
        );
        assert!(at_one
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("rui_zoom_cap:")));
        assert!(
            at_one.text_nodes.iter().all(|t| t.text != "Dialog title"),
            "1:1 blit must not also emit live Coverage (that pop is the row jitter)"
        );

        root.children[0].style.animations.clear();
        root.children[0].style.opacity = 1.0;
        let settled = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let idle = settled
            .text_nodes
            .iter()
            .find(|t| t.text == "Dialog title")
            .expect("idle Coverage");
        assert!((idle.paint_scale - 1.0).abs() < 1e-4);
        assert!(
            crate::renderer::take_capture_jobs()
                .iter()
                .all(|c| !c.key.starts_with("rui_zoom_cap:")),
            "scale ≈ 1 must not capture"
        );
    }

    #[test]
    fn modal_zoom_capture_keeps_semantic_description_color() {
        use crate::style::{muted_from_accent, parse_color, parse_text_color};

        let _ = crate::renderer::take_capture_jobs();
        let muted = muted_from_accent(parse_text_color("green-600"));
        let card_bg = parse_color("card");
        let mut desc = Element::new().text("You can change this later in Settings.");
        desc.text_style.color = Some(muted);
        let mut card = Element::new().w(240.0).h(120.0).bg(card_bg).child(desc);
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");
        let desc = cap
            .text_nodes
            .iter()
            .find(|t| t.text.contains("You can change"))
            .expect("description in capture");
        let got = desc.style.color_resolved();
        assert_eq!(
            got[3], 255,
            "zoom capture must not leave translucent glyphs on a transparent RT"
        );
        assert_eq!(
            got, muted,
            "success description must keep the variant muted hue during enter, not theme muted-foreground"
        );
        assert_ne!(got, parse_text_color("muted-foreground"));
        assert_ne!(got, parse_text_color("foreground"));
    }

    #[test]
    fn modal_zoom_capture_flattens_translucent_glyphs_over_card() {
        use crate::style::{parse_color, parse_text_color};

        let _ = crate::renderer::take_capture_jobs();
        let mut faded = parse_text_color("green-600");
        faded[3] = 199; // old muted_from_accent alpha (~0.78)
        let card_bg = parse_color("card");
        let mut desc = Element::new().text("You can change this later in Settings.");
        desc.text_style.color = Some(faded);
        let mut card = Element::new().w(240.0).h(120.0).bg(card_bg).child(desc);
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");
        let desc = cap
            .text_nodes
            .iter()
            .find(|t| t.text.contains("You can change"))
            .expect("description in capture");
        let got = desc.style.color_resolved();
        assert_eq!(got[3], 255);
        assert_ne!(
            got, faded,
            "translucent glyphs must composite over the card, not sit on a transparent RT"
        );
        assert_ne!(got, parse_text_color("muted-foreground"));
        assert_ne!(got, parse_text_color("foreground"));
    }

    /// Dialog `shadow-lg` + rounded overflow: chrome stays live SDF in the main pass
    /// (rounded border + halo). Capture is the inner padding box, not a dest AABB
    /// around the shadow that reads as a rectangular stroke.
    #[test]
    fn modal_zoom_capture_includes_box_shadow_outset() {
        let _ = crate::renderer::take_capture_jobs();
        let mut card = Element::new()
            .w(240.0)
            .h(120.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .rounded(8.0)
            .border_style("solid")
            .border_width(1.0)
            .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
            .overflow_y("hidden")
            .child(Element::new().text("Dialog title"));
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");
        assert!(
            (cap.bounds[2] - 238.0).abs() < 0.5,
            "capture is padding box (240−2×1px border); got {}",
            cap.bounds[2]
        );
        let blit = result
            .textured_draws
            .iter()
            .find(|d| d.texture_key.starts_with("rui_zoom_cap:"))
            .expect("zoom blit dest");
        assert!(
            (blit.rect[2] - 214.2).abs() < 0.5,
            "dest w should be padding×0.90; got {}",
            blit.rect[2]
        );
        assert!(
            cap.instances
                .iter()
                .all(|i| i.shadow_blur < 15.0 || i.rect_size[0] < 1.0),
            "shadow chrome must not enter the inner capture"
        );
        let chrome = result
            .instances
            .iter()
            .find(|i| i.shadow_blur > 15.0 && i.rect_size[0] > 1.0)
            .expect("dialog chrome live in main pass");
        assert!(
            chrome.radii.iter().all(|r| *r > 7.0),
            "live chrome must stay rounded SDF; got {:?}",
            chrome.radii
        );
        assert!(
            (chrome.border_width - 1.0).abs() < 0.15,
            "1px border stays on the live SDF instance; got {}",
            chrome.border_width
        );
        assert!(
            chrome.clip_size == InstanceInput::NO_CLIP_SIZE || chrome.clip_size[0] < 0.0,
            "own overflow must not AABB-clip the shadow; clip_size={:?}",
            chrome.clip_size
        );
        assert!(
            (chrome.rect_size[0] - 216.0).abs() < 1.0,
            "live chrome bakes CSS scale 0.90; got {}",
            chrome.rect_size[0]
        );
    }

    /// Select-style 1px outline must stay live SDF (not a minified blit hairline).
    #[test]
    fn modal_zoom_inner_border_stays_live_one_px() {
        let _ = crate::renderer::take_capture_jobs();
        let trigger = Element::new()
            .w(200.0)
            .h(32.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .rounded(8.0)
            .border_style("solid")
            .border_width(1.0)
            .border_color([0.7, 0.7, 0.7, 1.0])
            .child(Element::new().text("Pick an option"));
        let mut card = Element::new()
            .w(240.0)
            .h(120.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .rounded(8.0)
            .child(trigger);
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");
        assert!(
            cap.instances
                .iter()
                .all(|i| (i.border_width - 1.0).abs() > 0.5 || i.rect_size[1] > 40.0),
            "1px trigger chrome must not enter the blit"
        );
        let trigger = result
            .instances
            .iter()
            .find(|i| i.border_width > 0.5 && i.rect_size[1] < 40.0)
            .expect("live 1px trigger in main pass");
        assert!(
            (trigger.border_width - 1.0).abs() < 0.15,
            "hairline stays 1px SDF; got {}",
            trigger.border_width
        );
        assert!(
            (trigger.rect_size[0] - 180.0).abs() < 2.0,
            "trigger scales about the card center (200×0.90); got {}",
            trigger.rect_size[0]
        );
        assert!(
            trigger.radii.iter().all(|r| *r > 6.0 && *r < 8.0),
            "radii scale with the card; got {:?}",
            trigger.radii
        );
    }

    fn text_geom(info: &TextLayoutInfo) -> (f32, f32, f32, f32) {
        (info.x, info.y, info.width, info.height)
    }

    /// Gallery Dialog shell at enter: live rounded chrome in the main pass;
    /// 「行 1–6」 stay at rest inside the inner capture (no per-leaf crawl).
    #[test]
    fn modal_zoom_dialog_enter_captures_rows_at_rest() {
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::components::base::SlotChrome;
        use crate::components::dialog::dialog_shell_frame;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::{create_root, Signal};
        use crate::utils::modal_motion::flush_modal_shell_motion_subtree;
        use crate::utils::modal_shell::{default_modal_overlay_color, ShellOverlayOpts};

        let _ = crate::renderer::take_capture_jobs();
        clear_overlay_stack();
        let modal_id = "gal-dialog";
        let (_owner, mut host) = create_root(|| {
            let open = Signal::new(true);
            let overlay_opts = ShellOverlayOpts {
                color: default_modal_overlay_color(None, Some(0.5)),
                close_on_click: true,
                backdrop_blur: 0.0,
            };
            let mut rows = Element::new().w("100%").flex_col().gap(8.0);
            for i in 1..=6 {
                rows = rows.child(Element::new().text(format!("行 {i}")).text_size(14.0));
            }
            let body_id = format!("{modal_id}__body");
            let scroll: Element = scroll_area(body_id.as_str())
                .overflow_x("hidden")
                .overflow_y("auto")
                .scrollbar_visibility("auto")
                .w("100%")
                .h("auto")
                .min_h("0")
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .child(rows)
                .into();
            let inner = Element::new()
                .w("100%")
                .flex_col()
                .child(
                    Element::new()
                        .text("Gallery Dialog")
                        .text_size(18.0)
                        .font_weight(600.0),
                )
                .child(scroll);
            let shell = dialog_shell_frame(
                modal_id,
                open,
                inner,
                &overlay_opts,
                None,
                false,
                None,
                None,
                None,
                None,
                SlotChrome::default(),
            );
            Element::new().w(800.0).h(600.0).child(shell)
        });
        assert!(hoist_portal_subtrees(&mut host) >= 2);
        let _ = flush_modal_shell_motion_subtree(&mut host);

        let mut tree = LayoutTree::new();
        let zooming = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("enter must capture __modal_zoom");
        assert!(
            zooming
                .textured_draws
                .iter()
                .any(|d| d.texture_key.starts_with("rui_zoom_cap:")),
            "main pass blits the capture"
        );
        let chrome = zooming
            .instances
            .iter()
            .find(|i| i.shadow_blur > 15.0 && i.radii[0] > 1.0)
            .expect("rounded dialog chrome live in main pass");
        assert!(
            chrome.radii.iter().all(|r| *r > 7.0),
            "live chrome must stay rounded SDF; got {:?}",
            chrome.radii
        );
        assert!(
            (chrome.border_width - 1.0).abs() < 0.15,
            "1px border stays on live chrome; got {}",
            chrome.border_width
        );
        for i in 1..=6 {
            let needle = format!("行 {i}");
            let node = cap
                .text_nodes
                .iter()
                .find(|t| t.text == needle)
                .unwrap_or_else(|| panic!("missing capture {needle}"));
            assert!(
                (node.paint_scale - 1.0).abs() < 1e-4,
                "{needle} must stay rest-size in the RT; paint_scale={}",
                node.paint_scale
            );
            assert!(
                zooming.text_nodes.iter().all(|t| t.text != needle),
                "{needle} must not also paint as live Coverage (per-leaf crawl)"
            );
        }
    }

    fn find_text<'a>(nodes: &'a [TextLayoutInfo], needle: &str) -> &'a TextLayoutInfo {
        nodes
            .iter()
            .find(|t| t.text == needle)
            .unwrap_or_else(|| panic!("missing text {needle:?}"))
    }

    /// Capture text is rest layout (same boxes as idle). The GPU scales the whole
    /// layer about the card center — rows must not crawl toward/away from the title.
    #[test]
    fn modal_zoom_capture_text_geometry_matches_idle() {
        let _ = crate::renderer::take_capture_jobs();
        let chevron = Element::new()
            .font_family("lucide")
            .text("v")
            .text_size(16.0)
            .w(16.0)
            .h(16.0);
        let trigger = Element::new()
            .w(220.0)
            .h(32.0)
            .flex_row()
            .items_center()
            .child(
                Element::new()
                    .text("Pick an option")
                    .text_size(14.0)
                    .white_space("nowrap"),
            )
            .child(chevron);
        let mut body = Element::new().w(240.0).flex_col().child(trigger);
        for i in 1..=6 {
            body = body.child(
                Element::new()
                    .text(format!("行 {i}"))
                    .text_size(14.0)
                    .text_leading_none(),
            );
        }
        let mut card = Element::new()
            .w(240.0)
            .h(280.0)
            .bg([1.0, 1.0, 1.0, 1.0])
            .child(body);
        card.template_id = Some("dlg__modal_zoom".into());
        card.style.scale = [0.90, 0.90];
        let mut root = Element::new().w(400.0).h(400.0).child(card);

        let mut tree = LayoutTree::new();
        let _zooming = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let caps = crate::renderer::take_capture_jobs();
        let cap = caps
            .iter()
            .find(|c| c.key.starts_with("rui_zoom_cap:"))
            .expect("zoom capture");

        root.children[0].style.scale = [1.0, 1.0];
        let idle = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        let _ = crate::renderer::take_capture_jobs();

        for needle in [
            "Pick an option",
            "行 1",
            "行 2",
            "行 3",
            "行 4",
            "行 5",
            "行 6",
        ] {
            let z = find_text(&cap.text_nodes, needle);
            let i = find_text(&idle.text_nodes, needle);
            let (zx, zy, zw, zh) = text_geom(z);
            let (ix, iy, iw, ih) = text_geom(i);
            assert!(
                (zx - ix).abs() < 1.0 && (zy - iy).abs() < 1.0,
                "{needle}: capture ({zx:.2},{zy:.2}) must match idle rest ({ix:.2},{iy:.2})"
            );
            assert!(
                (zw - iw).abs() < 0.5 && (zh - ih).abs() < 0.5,
                "{needle} layout size jumped: capture {zw:.2}x{zh:.2} idle {iw:.2}x{ih:.2}"
            );
            assert!((z.paint_scale - 1.0).abs() < 1e-4);
            assert!((i.paint_scale - 1.0).abs() < 1e-4);
        }

        let z_icon = cap
            .text_nodes
            .iter()
            .find(|t| t.style.font_family.as_deref() == Some("lucide"))
            .expect("capture lucide");
        let i_icon = idle
            .text_nodes
            .iter()
            .find(|t| t.style.font_family.as_deref() == Some("lucide"))
            .expect("idle lucide");
        assert!((z_icon.style.size_resolved() - i_icon.style.size_resolved()).abs() < 1e-3);
    }

    /// Dialog `zoom-in-95` keeps `scale` < 1 during entrance; soft-wrapping description
    /// must still use layout width (not forced nowrap) so lines match the stable frame.
    #[test]
    fn css_scale_preserves_soft_wrap_during_paint() {
        let long = "desktop_smoke #7: Body 内含 Select; 先开下拉再 Esc, 应只关顶层浮层。";
        let child = Element::new().w(200.0).h(60.0).text(long);
        let mut shell = Element::new().w(200.0).h(60.0).child(child);
        shell.style.scale = [0.95, 0.95];
        let mut root = Element::new().w(400.0).h(400.0).child(shell);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

        let label = result
            .text_nodes
            .iter()
            .find(|t| t.text.contains("desktop_smoke"))
            .expect("scaled description text");
        assert!(
            label.style.white_space == crate::style::WhiteSpace::Normal,
            "CSS scale must not force nowrap during paint; got {:?}",
            label.style.white_space
        );
        assert!(
            (label.paint_scale - 0.95).abs() < 0.02,
            "paint_scale must carry CSS scale; got {}",
            label.paint_scale
        );
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

        let panel_idx = result
            .instances
            .iter()
            .position(|i| {
                i.shadow_blur > 0.0
                    && i.color[3] > 0.5
                    && (i.rect_size[0] - 120.0).abs() < 1.0
                    && (i.rect_size[1] - 80.0).abs() < 1.0
            })
            .expect("panel chrome with shadow");
        let panel_inst = &result.instances[panel_idx];
        assert!(
            panel_inst.clip_size == InstanceInput::NO_CLIP_SIZE || panel_inst.clip_size[0] < 0.0,
            "own overflow must not clip outset shadow; clip_size={:?}",
            panel_inst.clip_size
        );
        assert!(
            result
                .clip_polys
                .get(panel_idx)
                .is_none_or(|p| !p.is_active()),
            "rounded overflow clip-poly on own chrome also kills the halo (Dialog/Drawer)"
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

    #[test]
    fn local_backdrop_blur_stamps_on_mask_instance() {
        // Gallery LocalOverlayShowcase: host stacking + 75% rounded scrim with blur.
        // Blur binds to that SDF instance (radii + overflow clip), not a parallel AABB.
        let host = Element::new()
            .relative()
            .w(220.0)
            .h(140.0)
            .template_id("card-host");
        let scrim = Element::new()
            .w(165.0)
            .h(105.0)
            .rounded(10.0)
            .bg([15.0, 23.0, 42.0, 140.0])
            .backdrop_blur(12.0)
            .template_id("local-scrim");
        let mut wrapper = Element::new()
            .w("100%")
            .h("100%")
            .absolute()
            .inset(0.0)
            .z_index(2)
            .child(scrim);
        wrapper.set_attr(ATTR_HOISTED_MOUNT, "card-host");
        let mut root = Element::new().children([host, wrapper]);
        assert_eq!(hoist_portal_subtrees(&mut root), 1);

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
        let blur_insts: Vec<_> = result
            .instances
            .iter()
            .filter(|i| i.backdrop_blur > 0.5)
            .collect();
        assert_eq!(
            blur_insts.len(),
            1,
            "only the 75% mask instance samples backdrop; got {:?}",
            blur_insts
                .iter()
                .map(|i| (i.backdrop_blur, i.rect_size, i.radii))
                .collect::<Vec<_>>()
        );
        let inst = blur_insts[0];
        assert!(
            (inst.backdrop_blur - 12.0).abs() < 0.01,
            "backdrop_blur={}",
            inst.backdrop_blur
        );
        assert!(
            (inst.rect_size[0] - 165.0).abs() < 2.0 && (inst.rect_size[1] - 105.0).abs() < 2.0,
            "mask size {:?}",
            inst.rect_size
        );
        assert!(inst.radii[0] > 5.0, "mask radii={:?}", inst.radii);
    }

    #[test]
    fn local_backdrop_blur_splits_from_host_text_inside_nested_stack() {
        // Gallery「浮层 / 反馈」sits under a page stacking context (sidebar / ScrollArea).
        // Nested z-index must still split so the scrim band is *after* host glyphs —
        // otherwise capture prefix is empty and the SDF sample covers the labels.
        let mut page = Element::new().w(400.0).h(300.0);
        page.set_attr("data-stacking-context", "1");
        page.style.z_index = 0;

        let labels = Element::new()
            .absolute()
            .inset(0.0)
            .z_index(0)
            .text("YELLOW 高对比 BLUR");
        let scrim = Element::new()
            .w(165.0)
            .h(105.0)
            .rounded(10.0)
            .bg([1.0, 1.0, 1.0, 0.55])
            .backdrop_blur(12.0)
            .template_id("local-scrim");
        let mut overlay = Element::new()
            .w("100%")
            .h("100%")
            .absolute()
            .inset(0.0)
            .z_index(2)
            .child(scrim);
        overlay.set_attr("rui_portal", "1");
        overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
        overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
        let host = Element::new()
            .relative()
            .w(220.0)
            .h(140.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .template_id("card-host")
            .children([labels, overlay]);
        let mut root = Element::new().child(page.child(host));
        assert!(
            hoist_portal_subtrees(&mut root) >= 1,
            "local mount portal must lift onto the host"
        );

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
        assert!(
            result.text_nodes.iter().any(|t| t.text.contains("YELLOW")),
            "host contrast labels must still be collected"
        );
        let bands = result.overlay_split.page_stack_band_ranges();
        let blur_at = result
            .instances
            .iter()
            .position(|i| i.backdrop_blur > 0.5)
            .expect("scrim instance must stamp backdrop_blur");
        let text_at = result
            .text_nodes
            .iter()
            .position(|t| t.text.contains("YELLOW"))
            .unwrap();
        let blur_band = bands
            .iter()
            .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
            .expect("blur instance in a page_stack band");
        let text_band = bands
            .iter()
            .position(|b| b.text.0 <= text_at && text_at < b.text.1)
            .expect("label in a page_stack band");
        assert!(
            text_band < blur_band,
            "labels must paint in an earlier band than the scrim so capture includes them; text_band={text_band} blur_band={blur_band} bands={}",
            bands.len()
        );
        let blur = &result.instances[blur_at];
        assert!(
            blur.clip_size[0] > 0.5 && blur.clip_size[1] > 0.5,
            "local scrim must inherit host overflow clip so backdrop capture can scissor to the host; clip={:?}",
            (blur.clip_pos, blur.clip_size)
        );
    }

    #[test]
    fn local_mount_scrim_follows_scroll_with_stale_position_fixed() {
        // Keyed overlay adopt can copy `position_fixed` onto a local mount. Scroll
        // must still subtract ancestor `scroll_acc` or the 75% mask stays put while
        // the host moves — off-host glyphs then occupy the mask for a frame.
        let labels = Element::new()
            .absolute()
            .inset(0.0)
            .z_index(0)
            .text("YELLOW 高对比 BLUR");
        let scrim = Element::new()
            .w(165.0)
            .h(105.0)
            .rounded(10.0)
            .bg([1.0, 1.0, 1.0, 0.55])
            .backdrop_blur(12.0)
            .template_id("local-scrim");
        let mut overlay = Element::new()
            .w("100%")
            .h("100%")
            .absolute()
            .inset(0.0)
            .z_index(2)
            .child(scrim);
        overlay.set_attr("rui_portal", "1");
        overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
        overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
        let host = Element::new()
            .relative()
            .w(220.0)
            .h(140.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .template_id("card-host")
            .children([labels, overlay]);
        let spacer = Element::new().w(220.0).h(80.0).flex_shrink(0.0);
        let mut viewport = Element::new()
            .w(240.0)
            .h(200.0)
            .overflow_y("scroll")
            .flex_col()
            .children([spacer, host]);
        assert!(hoist_portal_subtrees(&mut viewport) >= 1);

        let mut tree = LayoutTree::new();
        let before = tree.build_and_layout(&mut viewport, 240.0, 200.0, [-1.0, -1.0]);
        let y0 = before
            .instances
            .iter()
            .find(|i| i.backdrop_blur > 0.5)
            .map(|i| i.rect_pos[1])
            .expect("scrim");

        crate::core::tree::walk_elements_mut(&mut viewport, |el| {
            if el.attrs.get(ATTR_HOISTED_MOUNT).is_some() {
                el.style.position_fixed = true;
            }
        });
        viewport.scroll_offset = [0.0, 40.0];
        let after =
            tree.build_and_layout_fast(&mut viewport, 240.0, 200.0, [-1.0, -1.0], true, false);
        let y1 = after
            .instances
            .iter()
            .find(|i| i.backdrop_blur > 0.5)
            .map(|i| i.rect_pos[1])
            .expect("scrim after scroll");
        assert!(
            (y0 - y1 - 40.0).abs() < 1.5,
            "local mount scrim must follow host scroll even with leftover position_fixed; y0={y0} y1={y1}"
        );
    }

    #[test]
    fn local_backdrop_blur_keeps_label_band_before_scrim_on_scroll() {
        let mut page = Element::new().w(400.0).h(800.0).flex_col();
        page.set_attr("data-stacking-context", "1");
        page.style.z_index = 0;
        page = page.child(
            Element::new()
                .w(400.0)
                .h(120.0)
                .flex_shrink(0.0)
                .text("OUTSIDE ABOVE"),
        );
        let labels = Element::new()
            .absolute()
            .inset(0.0)
            .z_index(0)
            .text("YELLOW 高对比 BLUR");
        let scrim = Element::new()
            .w(165.0)
            .h(105.0)
            .rounded(10.0)
            .bg([1.0, 1.0, 1.0, 0.55])
            .backdrop_blur(12.0)
            .template_id("local-scrim");
        let mut overlay = Element::new()
            .w("100%")
            .h("100%")
            .absolute()
            .inset(0.0)
            .z_index(2)
            .child(scrim);
        overlay.set_attr("rui_portal", "1");
        overlay.set_attr(ATTR_PORTAL_MOUNT, "card-host");
        overlay.set_attr(ATTR_PORTAL_KEY, "local-scrim");
        let host = Element::new()
            .relative()
            .w(220.0)
            .h(140.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .flex_shrink(0.0)
            .template_id("card-host")
            .children([labels, overlay]);
        page = page.child(host).child(
            Element::new()
                .w(400.0)
                .h(120.0)
                .flex_shrink(0.0)
                .text("OUTSIDE BELOW"),
        );
        let mut viewport = Element::new()
            .w(400.0)
            .h(200.0)
            .overflow_y("scroll")
            .child(page);
        assert!(hoist_portal_subtrees(&mut viewport) >= 1);

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut viewport, 400.0, 200.0, [-1.0, -1.0]);
        for sy in [0.0_f32, 40.0, 80.0, 120.0] {
            viewport.scroll_offset = [0.0, sy];
            let result =
                tree.build_and_layout_fast(&mut viewport, 400.0, 200.0, [-1.0, -1.0], true, false);
            let bands = result.overlay_split.page_stack_band_ranges();
            let Some(blur_at) = result.instances.iter().position(|i| i.backdrop_blur > 0.5) else {
                continue;
            };
            let Some(text_at) = result
                .text_nodes
                .iter()
                .position(|t| t.text.contains("YELLOW"))
            else {
                continue;
            };
            let blur_band = bands
                .iter()
                .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
                .expect("blur instance in a page_stack band");
            let text_band = bands
                .iter()
                .position(|b| b.text.0 <= text_at && text_at < b.text.1)
                .expect("label in a page_stack band");
            assert!(
                text_band < blur_band,
                "scroll_only sy={sy}: labels must stay in an earlier band than the scrim; text_band={text_band} blur_band={blur_band}"
            );
            let blur = &result.instances[blur_at];
            for node in &result.text_nodes {
                if !node.text.contains("OUTSIDE") {
                    continue;
                }
                let overlap_x = node.x < blur.rect_pos[0] + blur.rect_size[0]
                    && node.x + node.width > blur.rect_pos[0];
                let overlap_y = node.y < blur.rect_pos[1] + blur.rect_size[1]
                    && node.y + node.height > blur.rect_pos[1];
                assert!(
                    !(overlap_x && overlap_y),
                    "scroll_only sy={sy}: outside text {:?} must not overlap the scrim {:?} {:?}",
                    (node.text.as_str(), node.x, node.y),
                    blur.rect_pos,
                    blur.rect_size
                );
            }
        }
    }
}

#[cfg(test)]
mod text_cascade_tests {
    use super::*;

    #[test]
    fn decoration_color_none_is_current_color_not_parent_rgb() {
        // Parent root defaults to white text; child sets primary-like black via attr.
        let mut root = Element::new();
        root.text_style.color = Some([255, 255, 255, 255]);
        root.text_style.decoration_color = None;

        let mut child = Element::new().text("Link").text_color([0, 0, 0, 255]);
        // Stale pinned RGB without attr — cascade must clear back to currentColor.
        child.text_style.decoration_color = Some([255, 255, 255, 255]);
        assert!(!child.attrs.contains_key("text_decoration_color"));
        root.children.push(child);

        cascade_inheritable_text_style_for_test(&mut root);
        let leaf = &root.children[0];
        assert_eq!(leaf.text_style.color_resolved(), [0, 0, 0, 255]);
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

        let parent_text = root.text_style.as_ref().clone();
        cascade_inheritable_text_style(&mut root.inner.children[0], Some(parent_text));
        assert_eq!(
            root.children[0].text_style.decoration_color,
            Some([239, 68, 68, 255])
        );
    }
}

#[cfg(test)]
mod border_paint_tests {
    use super::representative_border_color;
    use crate::style::{LineStyle, Style};

    #[test]
    fn representative_color_uses_first_opaque_side_when_uniform_is_clear() {
        let mut style = Style::default();
        style.border_style = LineStyle::Solid;
        style.set_border_side_color(0, [1.0, 0.0, 0.0, 1.0]);
        style.set_border_side_color(1, [0.0, 1.0, 0.0, 1.0]);
        let c = representative_border_color(&style);
        assert!(
            (c[0] - 1.0).abs() < 1e-5 && c[3] > 0.5,
            "expected first opaque side, got {c:?}"
        );
    }

    #[test]
    fn representative_color_prefers_uniform_when_set() {
        let mut style = Style::default();
        style.border_style = LineStyle::Solid;
        style.border_color = [0.0, 0.0, 1.0, 1.0];
        let c = representative_border_color(&style);
        assert!((c[2] - 1.0).abs() < 1e-5);
    }

    fn lum(c: [f32; 4]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    #[test]
    fn css_bevel_tones_stay_apart_on_dark_primary() {
        let dark_teal = [0.05, 0.18, 0.16, 1.0];
        let d = super::css_bevel_dark(dark_teal);
        let l = super::css_bevel_light(dark_teal);
        assert!(
            lum(l) - lum(d) > 0.35,
            "3D bevel must stay readable on near-black primary, dark={d:?} light={l:?}"
        );
        let top = super::css_inset_tone(dark_teal, 0);
        let bottom = super::css_inset_tone(dark_teal, 2);
        assert!(lum(bottom) > lum(top) + 0.35);
        let top_out = super::css_outset_tone(dark_teal, 0);
        assert!(lum(top_out) > lum(top) + 0.35);
    }
}
