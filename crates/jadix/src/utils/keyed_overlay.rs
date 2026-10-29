//! KeyedOverlay — portal + 同 key 调和（含 shadow/transform mid 保留）。
//!
//! 自定义浮层/通知应：
//! 1. 用 [`item_template_id`] 给每条实例稳定 `template_id`
//! 2. 用 [`viewport_portal_stage`] / [`mount_reactive_anchor`] 挂 portal
//! 3. hoist / reconcile 时走 [`adopt_keyed_overlay`] / [`preserve_keyed_motion_on_reconcile`]
//!
//! 底层 hoist 仍由 [`crate::layout::layout_tree::hoist_portal_subtrees`] 驱动；
//! 本模块提供 **一等 API**，避免每个组件抄 toast 特例。
//! 产品文件不要调用 [`crate::app::portal_overlay::portal_sync`]：Pinned 不进 Esc 栈，
//! 开合走 `viewport_modal` / `anchored_overlay`。

use std::collections::HashMap;
use std::fmt::Display;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::style::{css_animation_needs_frame, css_transition_needs_frame};
use crate::utils::popup::{
    flow_hidden_portal_host, portal_panel, ATTR_FLOATING_ANCHOR_EDGE_Y, ATTR_FLOATING_CONTENT_H,
    ATTR_FLOATING_EXPAND_FROM_H, ATTR_FLOATING_EXPAND_MEASURE,
};

/// 通用 keyed 项前缀（新组件推荐 `ko:{scope}`）。
pub const KEYED_OVERLAY_PREFIX: &str = "ko:";

/// Toast 历史 scope（`toast-card-{id}`）；新代码可用任意 `{scope}-{key}`。
pub const TOAST_CARD_SCOPE: &str = "toast-card";

/// 稳定 `template_id`：同 key 在 reactive 重建 / portal hoist 时调和子树。
pub fn item_template_id(scope: &str, key: impl Display) -> String {
    format!("{scope}-{key}")
}

/// 是否应按 keyed overlay 保留 CSS transition mid（translate/opacity/scale/shadow）。
pub fn is_keyed_motion_item(tid: Option<&str>) -> bool {
    tid.is_some_and(|t| t.starts_with(KEYED_OVERLAY_PREFIX) || t.starts_with("toast-card-"))
}

/// Dialog / Sheet / Drawer 模态壳节点（[`apply_modal_shell_motion`] 每帧写入目标）。
pub(crate) fn is_modal_shell_motion_item(tid: Option<&str>) -> bool {
    tid.is_some_and(|t| {
        t.ends_with("__modal_overlay")
            || t.ends_with("__modal_panel")
            || t.ends_with("__modal_zoom")
    })
}

/// 锚定浮层面板（[`floating_motion`]：`{id}__floating_panel`）。
pub(crate) fn is_floating_panel_motion_item(tid: Option<&str>) -> bool {
    tid.is_some_and(|t| t.ends_with("__floating_panel"))
}

fn modal_shell_should_preserve_motion(prev: &Element) -> bool {
    prev.transition_snapshot.has_active_tweens()
        || css_transition_needs_frame(&prev.style, &prev.transition_snapshot)
        || css_animation_needs_frame(&prev.style, &prev.animation_snapshot)
}

fn floating_panel_should_preserve_motion(prev: &Element) -> bool {
    modal_shell_should_preserve_motion(prev) || !prev.style.animations.is_empty()
}

fn preserve_modal_shell_motion(fresh: &mut Element, prev: &Element) {
    let is_zoom = fresh
        .template_id
        .as_deref()
        .is_some_and(|t| t.ends_with("__modal_zoom"));
    // Fresh already seeded exit; do not replace it with leftover enter.
    if fresh
        .style
        .animations
        .iter()
        .any(|a| a.name == "fade-out-0" || a.name.starts_with("jadix-modal-slide-out-"))
    {
        return;
    }
    let enter_mid_rebuild = prev.style.opacity + 0.02 < fresh.style.opacity
        || (is_zoom && (prev.style.scale[0] + 0.02 < fresh.style.scale[0]));
    let exit_kf = prev.style.animations.iter().any(|a| {
        a.name == "fade-out-0"
            || a.name == "zoom-out-90"
            || a.name.starts_with("jadix-modal-slide-out-")
    });
    if !modal_shell_should_preserve_motion(prev) && !enter_mid_rebuild && !exit_kf {
        return;
    }
    copy_keyed_motion_state(fresh, prev);
    fresh.style.opacity = prev.style.opacity;
    fresh.style.translate = prev.style.translate;
    fresh.style.translate_pct = prev.style.translate_pct;
    if fresh.style.transition.is_empty() && !prev.style.transition.is_empty() {
        fresh.style.transition = prev.style.transition.clone();
    }
    if fresh.style.animations.is_empty() && !prev.style.animations.is_empty() {
        fresh.style.animations = prev.style.animations.clone();
    }
}

/// Fade / Expand 入场中途：reactive 重建会把 opacity/max-height 打回终态，须吞回 mid。
fn preserve_floating_panel_motion(fresh: &mut Element, prev: &Element) {
    if fresh
        .attrs
        .get(ATTR_FLOATING_EXPAND_MEASURE)
        .is_some_and(|s| s == "1")
    {
        // Unconstrained measure frame: do not copy the previous viewport-capped max-height
        // or clipped layout_bounds — that would freeze Expand at overflow height.
        return;
    }
    let fade_enter_mid = prev.style.opacity + 0.02 < fresh.style.opacity;
    let expand_mid = !matches!(
        crate::layout::dim_kind(prev.style.max_size.height),
        crate::layout::DimKind::Auto
    ) && matches!(
        crate::layout::dim_kind(fresh.style.max_size.height),
        crate::layout::DimKind::Auto
    );
    if !floating_panel_should_preserve_motion(prev) && !fade_enter_mid && !expand_mid {
        return;
    }
    copy_keyed_motion_state(fresh, prev);
    // Fade 动画拥有 opacity；copy_keyed 只抄 transition / snapshot 属性。
    fresh.style.opacity = prev.style.opacity;
    // Expand 动画拥有 max-height；copy_keyed 默认只在 transition 时抄。
    fresh.style.max_size.height = prev.style.max_size.height;
    fresh.style.min_size.height = prev.style.min_size.height;
    fresh.style.overflow.y = prev.style.overflow.y;
    // layout 在 flush 之后；跨 reactive 重建保留测高，避免 Expand 永远等不到 height。
    if prev.layout_bounds[3] > 1.0 {
        fresh.layout_bounds = prev.layout_bounds;
    }
    if let Some(h) = prev.attrs.get(ATTR_FLOATING_CONTENT_H) {
        fresh.set_attr(ATTR_FLOATING_CONTENT_H, h.clone());
    }
    if let Some(edge) = prev.attrs.get(ATTR_FLOATING_ANCHOR_EDGE_Y) {
        fresh.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, edge.clone());
    }
    if let Some(from_h) = prev.attrs.get(ATTR_FLOATING_EXPAND_FROM_H) {
        fresh.set_attr(ATTR_FLOATING_EXPAND_FROM_H, from_h.clone());
    }
    if fresh.style.transition.is_empty() && !prev.style.transition.is_empty() {
        fresh.style.transition = prev.style.transition.clone();
    }
    if fresh.style.animations.is_empty() && !prev.style.animations.is_empty() {
        fresh.style.animations = prev.style.animations.clone();
    }
}

/// 视口级 portal 舞台：铺满层 + 稳定 portal key + z（Toast / 轻量 Notify 宿主）。
pub fn viewport_portal_stage(portal_key: impl Into<String>, panel: Element, z: i32) -> Element {
    let key = portal_key.into();
    let mut stage: Element = Element::new()
        .absolute()
        .inset(0.0)
        .w("100%")
        .h("100%")
        .pointer_events_box_none()
        .child(panel);
    stage.set_attr(ATTR_PORTAL_KEY, key.as_str());
    portal_panel(stage, z)
}

/// 0×0 reactive 锚点外壳。反应根 **不得** 带 `jadix_portal`，否则 hoist 后调和会丢壳。
/// Leftover host is out of flow so a parent wrap/`gap` row does not reserve a slot.
pub fn mount_reactive_anchor(view_id: &str, paint: impl Fn() -> Element + 'static) -> Element {
    reactive_anchor(view_id, paint);
    flow_hidden_portal_host(
        Element::new()
            .child(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()),
    )
}

/// 同 `template_id` 的 reconcile：保留 transition mid + 阴影（非 hoist 路径）。
pub fn preserve_keyed_motion_on_reconcile(fresh: &mut Element, prev: &Element) -> bool {
    if is_modal_shell_motion_item(fresh.template_id.as_deref()) {
        let preserve = modal_shell_should_preserve_motion(prev);
        if preserve {
            preserve_modal_shell_motion(fresh, prev);
        }
        return preserve;
    }
    if is_floating_panel_motion_item(fresh.template_id.as_deref()) {
        let preserve = floating_panel_should_preserve_motion(prev)
            || prev.style.opacity + 0.02 < fresh.style.opacity
            || (!matches!(
                crate::layout::dim_kind(prev.style.max_size.height),
                crate::layout::DimKind::Auto
            ) && matches!(
                crate::layout::dim_kind(fresh.style.max_size.height),
                crate::layout::DimKind::Auto
            ));
        if preserve {
            preserve_floating_panel_motion(fresh, prev);
        }
        return preserve;
    }
    let preserve = !fresh.style.transition.is_empty()
        && (is_keyed_motion_item(fresh.template_id.as_deref())
            || crate::utils::presence::is_presence_wrapper_tid(fresh.template_id.as_deref())
            || prev.transition_snapshot.has_active_tweens()
            || prev.transition_snapshot.has_any_tweens()
            || !prev.style.transition.is_empty()
            || css_transition_needs_frame(&prev.style, &prev.transition_snapshot));
    if preserve {
        preserve_generic_keyed_motion(fresh, prev);
    }
    preserve
}

/// Keep used values for enter / mid-tween rebuilds. On first exit (`opacity` dropped,
/// no active tween yet) keep the specified target and seed a snapshot from `prev`
/// so CSS can retarget — `copy_transition_used_values` would clobber opacity 0→1.
fn preserve_generic_keyed_motion(fresh: &mut Element, prev: &Element) {
    let fresh_exit_motion = fresh.transition_snapshot.has_active_tweens()
        || fresh.style.opacity + 0.05 < prev.style.opacity;
    if fresh_exit_motion && !prev.transition_snapshot.has_active_tweens() {
        seed_exit_transition_from_prev(fresh, prev);
        return;
    }
    copy_keyed_motion_state(fresh, prev);
}

fn seed_exit_transition_from_prev(fresh: &mut Element, prev: &Element) {
    fresh.transition_snapshot = prev.transition_snapshot.clone();
    fresh.animation_snapshot = prev.animation_snapshot.clone();
    if fresh.style.transition.is_empty() && !prev.style.transition.is_empty() {
        fresh.style.transition = prev.style.transition.clone();
    }
    if !fresh.transition_snapshot.has_any_tweens() {
        crate::style::snap_element_transitions_to_current(
            &prev.style,
            &mut fresh.transition_snapshot,
        );
    }
}

/// hoist 后同 portal key 调和：按 `template_id` 递归合并，保留 mid。
///
/// **Do not** adopt `layout_node` — Taffy SlotMap topology would become inconsistent.
pub fn adopt_keyed_overlay(fresh: &mut Element, prev: &mut Element) {
    adopt_overlay_node(fresh, prev);
}

fn adopt_overlay_node(fresh: &mut Element, prev: &mut Element) {
    if fresh.template_id == prev.template_id {
        fresh.layout_bounds = prev.layout_bounds;
        fresh.paint_scroll_acc = prev.paint_scroll_acc;
        fresh.paint_overlay_stamp = prev.paint_overlay_stamp;
        fresh.paint_ancestor_scale = prev.paint_ancestor_scale;
        fresh.paint_ancestor_linear = prev.paint_ancestor_linear;
        fresh.sticky_visual_offset = prev.sticky_visual_offset;
    }
    let tid_match = fresh.template_id.is_some() && fresh.template_id == prev.template_id;
    if tid_match {
        if fresh.on_unmount.is_none() {
            fresh.on_unmount = prev.on_unmount.clone();
        }
        preserve_keyed_motion_on_hoist(fresh, prev);
    }

    let fresh_keyed =
        !fresh.children.is_empty() && fresh.children.iter().all(|c| c.template_id.is_some());
    let prev_keyed =
        !prev.children.is_empty() && prev.children.iter().all(|c| c.template_id.is_some());

    if fresh_keyed && prev_keyed {
        let mut prev_map: HashMap<String, Element> = HashMap::new();
        for child in prev.children.drain(..) {
            if let Some(tid) = child.template_id.clone() {
                prev_map.insert(tid, child);
            }
        }
        for child in &mut fresh.children {
            if let Some(tid) = child.template_id.clone() {
                if let Some(mut old) = prev_map.remove(&tid) {
                    adopt_overlay_node(child, &mut old);
                }
            }
        }
        return;
    }

    if fresh.children.len() == prev.children.len() {
        for (f, p) in fresh.children.iter_mut().zip(prev.children.iter_mut()) {
            adopt_overlay_node(f, p);
        }
        return;
    }

    let mut prev_by_tid: HashMap<String, Element> = HashMap::new();
    take_templated_descendants(prev, &mut prev_by_tid);
    adopt_templated_into(fresh, &mut prev_by_tid);
}

fn preserve_keyed_motion_on_hoist(fresh: &mut Element, prev: &Element) {
    if is_modal_shell_motion_item(fresh.template_id.as_deref()) {
        preserve_modal_shell_motion(fresh, prev);
        return;
    }
    if is_floating_panel_motion_item(fresh.template_id.as_deref()) {
        preserve_floating_panel_motion(fresh, prev);
        return;
    }
    let keyed = is_keyed_motion_item(fresh.template_id.as_deref());
    if !keyed
        && fresh.style.transition.is_empty()
        && prev.style.transition.is_empty()
        && !prev.transition_snapshot.has_active_tweens()
    {
        return;
    }
    // Exit: sync may already set opacity→0 and seed the tween before hoist. Do not overwrite with idle mid.
    let fresh_exit_motion = fresh.transition_snapshot.has_active_tweens()
        || fresh.style.opacity + 0.05 < prev.style.opacity;
    if fresh_exit_motion {
        if fresh.style.transition.is_empty() && !prev.style.transition.is_empty() {
            fresh.style.transition = prev.style.transition.clone();
        }
        return;
    }
    if keyed
        || !prev.style.transition.is_empty()
        || prev.transition_snapshot.has_active_tweens()
        || !fresh.style.transition.is_empty()
    {
        copy_keyed_motion_state(fresh, prev);
        if fresh.style.transition.is_empty() && !prev.style.transition.is_empty() {
            fresh.style.transition = prev.style.transition.clone();
        }
    }
}

fn copy_keyed_motion_state(fresh: &mut Element, prev: &Element) {
    fresh.transition_snapshot = prev.transition_snapshot.clone();
    fresh.animation_snapshot = prev.animation_snapshot.clone();
    crate::style::copy_transition_used_values(
        &mut fresh.style,
        &prev.style,
        &prev.transition_snapshot,
    );
}

fn take_templated_descendants(node: &mut Element, out: &mut HashMap<String, Element>) {
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].template_id.is_some() {
            let child = node.children.remove(i);
            if let Some(tid) = child.template_id.clone() {
                out.insert(tid, child);
            }
        } else {
            take_templated_descendants(&mut node.children[i], out);
            i += 1;
        }
    }
}

fn adopt_templated_into(fresh: &mut Element, prev_by_tid: &mut HashMap<String, Element>) {
    for child in &mut fresh.children {
        if let Some(tid) = child.template_id.clone() {
            if let Some(mut old) = prev_by_tid.remove(&tid) {
                adopt_overlay_node(child, &mut old);
            }
        } else {
            adopt_templated_into(child, prev_by_tid);
        }
    }
}

#[cfg(test)]
#[path = "tests/keyed_overlay.rs"]
mod tests;
