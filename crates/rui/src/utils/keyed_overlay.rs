//! KeyedOverlay — portal + 同 key 调和（含 shadow/transform mid 保留）。
//!
//! 自定义浮层/通知应：
//! 1. 用 [`item_template_id`] 给每条实例稳定 `template_id`
//! 2. 用 [`viewport_portal_stage`] / [`mount_reactive_anchor`] 挂 portal
//! 3. hoist / reconcile 时走 [`adopt_keyed_overlay`] / [`preserve_keyed_motion_on_reconcile`]
//!
//! 底层 hoist 仍由 [`crate::layout::layout_tree::hoist_portal_subtrees`] 驱动；
//! 本模块提供 **一等 API**，避免每个组件抄 toast 特例。

use std::collections::HashMap;
use std::fmt::Display;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::style::{css_animation_needs_frame, css_transition_needs_frame};
use crate::utils::popup::portal_panel;

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

fn modal_shell_should_preserve_motion(prev: &Element) -> bool {
    prev.transition_snapshot.has_active_tweens()
        || css_transition_needs_frame(&prev.style, &prev.transition_snapshot)
        || css_animation_needs_frame(&prev.style, &prev.animation_snapshot)
}

fn preserve_modal_shell_motion(fresh: &mut Element, prev: &Element) {
    let is_zoom = fresh
        .template_id
        .as_deref()
        .is_some_and(|t| t.ends_with("__modal_zoom"));
    let enter_mid_rebuild = prev.style.opacity + 0.02 < fresh.style.opacity
        || (is_zoom && (prev.style.scale[0] + 0.02 < fresh.style.scale[0]));
    if !modal_shell_should_preserve_motion(prev) && !enter_mid_rebuild {
        return;
    }
    copy_keyed_motion_state(fresh, prev);
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

/// 0×0 reactive 锚点外壳。反应根 **不得** 带 `rui_portal`，否则 hoist 后调和会丢壳。
pub fn mount_reactive_anchor(view_id: &str, paint: impl Fn() -> Element + 'static) -> Element {
    reactive_anchor(view_id, paint);
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none())
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
    let preserve = !fresh.style.transition.is_empty()
        && (is_keyed_motion_item(fresh.template_id.as_deref())
            || prev.transition_snapshot.has_active_tweens()
            || css_transition_needs_frame(&prev.style, &prev.transition_snapshot));
    if preserve {
        copy_keyed_motion_state(fresh, prev);
    }
    preserve
}

/// hoist 后同 portal key 调和：按 `template_id` 递归合并，保留 mid。
///
/// **勿** adopt `layout_node` — Taffy SlotMap 拓扑会不一致。
pub fn adopt_keyed_overlay(fresh: &mut Element, prev: &mut Element) {
    adopt_overlay_node(fresh, prev);
}

fn adopt_overlay_node(fresh: &mut Element, prev: &mut Element) {
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
    let keyed = is_keyed_motion_item(fresh.template_id.as_deref());
    if !keyed
        && fresh.style.transition.is_empty()
        && prev.style.transition.is_empty()
        && !prev.transition_snapshot.has_active_tweens()
    {
        return;
    }
    // 退场：sync 可能已在 hoist 前把 opacity→0 并 seed tween。勿用 idle mid 盖掉。
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
    fresh.style.translate = prev.style.translate;
    fresh.style.opacity = prev.style.opacity;
    fresh.style.scale = prev.style.scale;
    fresh.style.rotate_deg = prev.style.rotate_deg;
    fresh.style.shadow_color = prev.style.shadow_color;
    if prev
        .style
        .transition
        .iter()
        .any(|e| e.property == crate::style::TransitionProperty::MaxHeight)
        || prev.transition_snapshot.has_active_tweens()
    {
        fresh.style.max_size.height = prev.style.max_size.height;
    }
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
mod tests {
    use super::*;
    use crate::style::{TransitionEntry, TransitionProperty, TransitionSnapshot};

    #[test]
    fn item_template_id_format() {
        assert_eq!(item_template_id("toast-card", 42), "toast-card-42");
        assert_eq!(
            item_template_id(&format!("{KEYED_OVERLAY_PREFIX}notify"), "x"),
            "ko:notify-x"
        );
    }

    #[test]
    fn is_keyed_motion_item_scopes() {
        assert!(is_keyed_motion_item(Some("toast-card-1")));
        assert!(is_keyed_motion_item(Some("ko:tooltip-abc")));
        assert!(!is_keyed_motion_item(Some("random")));
    }

    #[test]
    fn adopt_keyed_overlay_preserves_mid_on_same_template_id() {
        let mut prev = Element::new().template_id("toast-card-1");
        prev.style.translate = [10.0, 20.0];
        prev.style.opacity = 0.5;
        prev.transition_snapshot = TransitionSnapshot::default();

        let mut fresh = Element::new().template_id("toast-card-1");
        fresh.style.translate = [0.0, 100.0];
        fresh.style.opacity = 1.0;
        fresh.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 300.0,
            delay_ms: 0.0,
            timing: crate::style::TimingFunction::Ease,
        });

        adopt_keyed_overlay(&mut fresh, &mut prev);
        assert!((fresh.style.translate[0] - 10.0).abs() < f32::EPSILON);
        assert!((fresh.style.opacity - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn viewport_portal_stage_sets_portal_key() {
        let stage = viewport_portal_stage("notify-host", Element::new(), 9000);
        assert_eq!(
            stage.attrs.get(ATTR_PORTAL_KEY).map(String::as_str),
            Some("notify-host")
        );
        assert_eq!(stage.attrs.get("rui_portal").map(String::as_str), Some("1"));
    }

    #[test]
    fn preserve_keyed_motion_on_reconcile_copies_mid() {
        let mut prev = Element::new().template_id("toast-card-9");
        prev.style.scale = [0.9, 0.9];
        prev.style.shadow_color = [0.0, 0.0, 0.0, 0.4];
        prev.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: crate::style::TimingFunction::Ease,
        });

        let mut fresh = Element::new().template_id("toast-card-9");
        fresh.style.scale = [1.0, 1.0];
        fresh.style.transition = prev.style.transition.clone();

        assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
        assert!((fresh.style.scale[0] - 0.9).abs() < f32::EPSILON);
        assert!((fresh.style.shadow_color[3] - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn modal_shell_motion_skips_keyed_preserve_on_reconcile_when_idle() {
        let mut prev = Element::new().template_id("dlg__modal_panel");
        prev.style.opacity = 1.0;
        prev.style.translate = [0.0, 0.0];

        let mut fresh = Element::new().template_id("dlg__modal_panel");
        fresh.style.opacity = 1.0;
        fresh.style.translate = [0.0, 520.0];

        assert!(!preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
        assert!((fresh.style.translate[1] - 520.0).abs() < f32::EPSILON);
    }

    #[test]
    fn modal_shell_motion_preserves_enter_mid_on_hoist() {
        use crate::style::{
            set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
            snap_element_transitions_to_current, tick_element_transitions, TimingFunction,
        };

        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);

        let mut prev = Element::new().template_id("dlg__modal_panel");
        prev.style.scale = [0.96, 0.96];
        prev.style.opacity = 1.0;
        prev.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 250.0,
            delay_ms: 0.0,
            timing: TimingFunction::EaseOut,
        });
        snap_element_transitions_to_current(&prev.style, &mut prev.transition_snapshot);
        prev.style.scale = [1.0, 1.0];
        let _ = tick_element_transitions(&mut prev.style, &mut prev.transition_snapshot, 0.0);
        set_css_transition_timeline_ms(120.0);
        let _ = tick_element_transitions(&mut prev.style, &mut prev.transition_snapshot, 0.0);
        assert!(
            prev.style.scale[0] > 0.97 && prev.style.scale[0] < 0.995,
            "sanity: prev should be mid-enter scale, got {}",
            prev.style.scale[0]
        );

        let mut fresh = Element::new().template_id("dlg__modal_panel");
        fresh.style.scale = [1.0, 1.0];
        fresh.style.opacity = 1.0;
        fresh.style.transition = prev.style.transition.clone();

        adopt_keyed_overlay(&mut fresh, &mut prev);
        assert!(
            (fresh.style.scale[0] - prev.style.scale[0]).abs() < f32::EPSILON,
            "enter scale mid must survive hoist adopt, fresh={} prev={}",
            fresh.style.scale[0],
            prev.style.scale[0]
        );
        assert!(
            (fresh.style.opacity - 1.0).abs() < f32::EPSILON,
            "panel must stay opaque during enter"
        );

        set_css_transition_virtual_timeline(false);
    }
}
