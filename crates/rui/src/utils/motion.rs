//! MotionStyle — 只写 CSS transition **目标**，mid 由 [`TransitionSnapshot`] 推进。
//!
//! 自定义浮层/通知应消费本模块，而不是在 `reactive_sync` 里重复：
//! snap 基线、ensure transition、目标去重 StyleChanged、按卡要帧。
//!
//! 底层引擎：[`crate::style::tick_element_transitions`] / [`css_transition_needs_frame`].
//! 与 [`crate::style::keyframes`] / CSS `@keyframes` 路径的分工见仓库 `docs/engine/animation_and_motion.md`。

use crate::core::tree::{Element, LayoutStatus};
use crate::style::{css_transition_needs_frame, TransitionEntry, TransitionSnapshot};

/// CSS transition 目标姿态（写入 `Style`；tick 会把 style 覆写为 mid）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionTransform {
    pub translate: [f32; 2],
    /// CSS `translate` percentage of the border box (independent of pixel translate).
    pub translate_pct: [f32; 2],
    pub opacity: f32,
    pub scale: [f32; 2],
}

impl MotionTransform {
    pub fn uniform_scale(translate: [f32; 2], opacity: f32, scale: f32) -> Self {
        Self {
            translate,
            translate_pct: [0.0, 0.0],
            opacity,
            scale: [scale, scale],
        }
    }
}

/// 目标变化判定容差（与 Toast 堆叠 sync 对齐）。
#[derive(Clone, Copy, Debug)]
pub struct MotionEpsilon {
    pub translate: f32,
    pub translate_pct: f32,
    pub opacity: f32,
    pub scale: f32,
}

impl Default for MotionEpsilon {
    fn default() -> Self {
        Self {
            translate: 0.5,
            translate_pct: 0.002,
            opacity: 0.001,
            scale: 0.001,
        }
    }
}

/// 首帧 / reconcile 后如何 seed transition 基线。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionBootstrap {
    /// reconcile/hoist 后：ensure → snap（退场 seed）。
    SnapAfterEnsure,
    /// transition 为空：snap → ensure。
    EnsureAfterSnap,
    /// 仅刷新 transition 配置。
    EnsureOnly,
}

/// 跨帧记住上一帧**目标**（style 可能是 mid，不能拿 style≈target 判等）。
#[derive(Clone, Debug, Default)]
pub struct MotionStyle {
    last: Option<MotionTransform>,
}

impl MotionStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.last = None;
    }

    pub fn target_changed(&self, target: MotionTransform, eps: MotionEpsilon) -> bool {
        match self.last {
            Some(prev) => {
                (prev.translate[0] - target.translate[0]).abs() > eps.translate
                    || (prev.translate[1] - target.translate[1]).abs() > eps.translate
                    || (prev.translate_pct[0] - target.translate_pct[0]).abs() > eps.translate_pct
                    || (prev.translate_pct[1] - target.translate_pct[1]).abs() > eps.translate_pct
                    || (prev.opacity - target.opacity).abs() > eps.opacity
                    || (prev.scale[0] - target.scale[0]).abs() > eps.scale
                    || (prev.scale[1] - target.scale[1]).abs() > eps.scale
            }
            None => true,
        }
    }

    /// 写目标到 `style`；返回目标是否相对上一帧变化。
    pub fn apply(
        &mut self,
        style: &mut crate::style::Style,
        target: MotionTransform,
        eps: MotionEpsilon,
    ) -> bool {
        let changed = self.target_changed(target, eps);
        style.translate = target.translate;
        style.translate_pct = target.translate_pct;
        style.opacity = target.opacity;
        style.scale = target.scale;
        if changed {
            self.last = Some(target);
        }
        changed
    }
}

fn transition_entry_eq(a: &TransitionEntry, b: &TransitionEntry) -> bool {
    a.property == b.property
        && (a.duration_ms - b.duration_ms).abs() < 0.5
        && a.delay_ms == b.delay_ms
        && a.timing == b.timing
}

/// 仅在配置变化时重建 `style.transition`。
pub fn ensure_motion_transitions(style: &mut crate::style::Style, entries: &[TransitionEntry]) {
    if entries.len() == style.transition.len()
        && entries
            .iter()
            .zip(style.transition.iter())
            .all(|(a, b)| transition_entry_eq(a, b))
    {
        return;
    }
    style.transition.clear();
    style.transition.extend_from_slice(entries);
}

/// 拖拽等跟手路径：清 transition + snapshot。
pub fn clear_motion(el: &mut Element) {
    el.style.transition.clear();
    el.transition_snapshot = TransitionSnapshot::default();
}

/// 把当前 style 记为已 settle 基线（无缝瞬移 / exit seed）。
pub fn snap_motion_baseline(el: &mut Element) {
    el.snap_css_transition_baseline();
}

/// 本节点 CSS transition 是否仍需帧。
#[inline]
pub fn motion_is_animating(style: &crate::style::Style, snapshot: &TransitionSnapshot) -> bool {
    snapshot.has_active_tweens() || css_transition_needs_frame(style, snapshot)
}

/// `reactive_sync` 前段：拖拽清场，或 ensure/snap 基线。
pub fn prepare_motion_sync(
    el: &mut Element,
    entries: &[TransitionEntry],
    bootstrap: Option<MotionBootstrap>,
    tracker: &mut MotionStyle,
    dragging: bool,
) {
    if dragging {
        clear_motion(el);
        tracker.reset();
        return;
    }
    match bootstrap {
        Some(MotionBootstrap::SnapAfterEnsure) => {
            ensure_motion_transitions(&mut el.style, entries);
            snap_motion_baseline(el);
            tracker.reset();
        }
        Some(MotionBootstrap::EnsureAfterSnap) => {
            snap_motion_baseline(el);
            ensure_motion_transitions(&mut el.style, entries);
        }
        Some(MotionBootstrap::EnsureOnly) | None => {
            ensure_motion_transitions(&mut el.style, entries);
        }
    }
}

/// 合并 StyleChanged + 按卡 frame damage（仅本节点 CSS，勿用全局 animation 查询）。
pub fn motion_sync_status(el: &Element, style_changed: bool, dragging: bool) -> LayoutStatus {
    let animating = !dragging && motion_is_animating(&el.style, &el.transition_snapshot);
    if animating || style_changed || dragging {
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        set_css_transition_timeline_ms, set_css_transition_virtual_timeline, TimingFunction,
        TransitionProperty,
    };

    #[test]
    fn motion_style_dedupes_target_unchanged() {
        let mut m = MotionStyle::new();
        let mut style = crate::style::Style::default();
        let t = MotionTransform::uniform_scale([1.0, 2.0], 0.5, 1.0);
        assert!(m.apply(&mut style, t, MotionEpsilon::default()));
        assert!(!m.apply(&mut style, t, MotionEpsilon::default()));
        m.reset();
        assert!(m.apply(&mut style, t, MotionEpsilon::default()));
    }

    #[test]
    fn ensure_motion_transitions_idempotent() {
        let entries = [TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 300.0,
            delay_ms: 0.0,
            timing: TimingFunction::Ease,
        }];
        let mut style = crate::style::Style::default();
        ensure_motion_transitions(&mut style, &entries);
        let ptr = style.transition.as_ptr();
        ensure_motion_transitions(&mut style, &entries);
        assert_eq!(style.transition.as_ptr(), ptr);
        assert_eq!(style.transition.len(), 1);
    }

    #[test]
    fn snap_baseline_then_new_target_animates_via_motion_helpers() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut el = Element::new();
        el.style.translate = [-100.0, 0.0];
        ensure_motion_transitions(
            &mut el.style,
            &[TransitionEntry {
                property: TransitionProperty::Transform,
                duration_ms: 100.0,
                delay_ms: 0.0,
                timing: TimingFunction::Linear,
            }],
        );
        snap_motion_baseline(&mut el);
        el.style.translate = [-300.0, 0.0];
        assert!(motion_is_animating(&el.style, &el.transition_snapshot));
        let _ = el.tick_css_transitions(0.0);
        set_css_transition_timeline_ms(50.0);
        el.style.translate = [-300.0, 0.0];
        let _ = el.tick_css_transitions(0.0);
        assert!(
            (el.style.translate[0] + 200.0).abs() < 1.0,
            "mid should be between -100 and -300, got {}",
            el.style.translate[0]
        );
        set_css_transition_virtual_timeline(false);
    }
}
