//! Collapse panel presence + max-height motion（Collapsible / Accordion 共享）。
//!
//! 产品壳只保留 exclusive / roving / `value` 列表；测高、keyframes、退场卸树在此。
//! Accordion 的 `grid-template-rows` 备选路径仍在 `components/accordion.rs`。
//!
//! **留在 `utils/`，不进 `components/internal/`**：不依赖 Collapsible / Accordion
//! 类型（keyframes 名是参数）；作者手册要求自定义组件直接消费。`internal` 是
//! `pub(crate)` 产品 chrome，不进 prelude。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use taffy::style::{Dimension, Display, Overflow};

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::{animation_run_active, keyframes_anim_entry, AnimationFillMode, TimingFunction};
use crate::utils::motion::clear_motion;
use crate::utils::panel_presence::{
    sync_panel_presence, tick_panel_cells, use_panel_presence, PanelPresence, SyncPolicy,
};
use crate::utils::presence::PresencePhase;

/// ShadCN 同形不同名：`collapsible-down/up` 与 `accordion-down/up`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollapseKeyframes {
    pub down: &'static str,
    pub up: &'static str,
    pub duration_ms: f32,
}

impl CollapseKeyframes {
    pub const COLLAPSIBLE: Self = Self {
        down: "collapsible-down",
        up: "collapsible-up",
        duration_ms: 200.0,
    };
    pub const ACCORDION: Self = Self {
        down: "accordion-down",
        up: "accordion-up",
        duration_ms: 200.0,
    };
}

const PANEL_EXIT: Duration = Duration::from_millis(200);

thread_local! {
    static PANEL_PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<PanelPresenceCell<()>>>>> =
        const { RefCell::new(Vec::new()) };
    static PENDING_UNMOUNTS: RefCell<Vec<crate::core::reactivity::Signal<bool>>> =
        const { RefCell::new(Vec::new()) };
}

use crate::utils::panel_presence::PanelPresenceCell;

pub type CollapsePanelPresence = PanelPresence<()>;

/// 按稳定 `key` 取（或建）一块面板 presence。多实例须带实例前缀。
pub fn use_collapse_panel_presence(key: &str) -> CollapsePanelPresence {
    use_panel_presence(
        &format!("__rui_collapse_panel:{key}"),
        &format!("__rui_collapse_mounted:{key}"),
        PANEL_EXIT,
        || (),
        |rc| {
            PANEL_PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(rc));
        },
        |rc| {
            PANEL_PRESENCE_REGISTRY.with(|r| {
                r.borrow_mut().retain(|c| !Rc::ptr_eq(c, &rc));
            });
        },
    )
}

impl CollapsePanelPresence {
    /// CSS 退场播完：立刻 Absent，下一帧 drain 卸树。
    pub fn finish_exit(&self) {
        let mounted_sig = self.mounted();
        self.force_absent_flags();
        PENDING_UNMOUNTS.with(|q| q.borrow_mut().push(mounted_sig));
        crate::app::runtime::request_redraw();
    }
}

pub fn sync_collapse_panel_presence(handle: &CollapsePanelPresence, want: bool) -> PresencePhase {
    sync_panel_presence(handle, want, SyncPolicy::COLLAPSE, |_, _| {})
}

/// 事件循环推进退场；CSS 退场完成在 drain 卸树。
pub fn tick_collapse_panel_presence() -> bool {
    drain_pending_unmounts();
    let now = Instant::now();
    let outcome = PANEL_PRESENCE_REGISTRY.with(|reg| {
        tick_panel_cells(
            reg.borrow().as_slice(),
            now,
            |c| c.exit_armed,
            |_| {},
            false,
        )
    });
    for sig in outcome.unmount {
        if sig.is_alive() {
            sig.set(false);
        }
    }
    outcome.needs_frame
}

fn drain_pending_unmounts() {
    let pending = PENDING_UNMOUNTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for sig in pending {
        if sig.is_alive() {
            sig.set(false);
        }
    }
}

fn panel_content_height(el: &Element) -> f32 {
    el.used_layout.anim_height().max(el.layout_bounds[3])
}

fn has_exit_anim(el: &Element, up: &str) -> bool {
    el.style.animations.iter().any(|a| a.name == up)
}

fn reset_anim_snapshot(el: &mut Element) {
    el.animation_snapshot = Default::default();
}

/// Seed `*-down` keyframes（Floating Expand 默认路径与库测共用）。
pub fn seed_collapse_enter_anim(el: &mut Element, keys: CollapseKeyframes) {
    seed_enter_anim(el, keys);
}

fn seed_enter_anim(el: &mut Element, keys: CollapseKeyframes) {
    clear_motion(el);
    el.style.max_size.height = Dimension::auto();
    el.style.animations = vec![keyframes_anim_entry(
        keys.down,
        keys.duration_ms,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
}

/// Seed `*-up` keyframes（库测 / 自定义退场入口）。
pub fn seed_collapse_exit_anim(el: &mut Element, keys: CollapseKeyframes) {
    seed_collapse_exit_anim_from(el, keys, None);
}

/// `from_h` pins current clip so a rebuild does not jump to full height (Floating Expand).
pub fn seed_collapse_exit_anim_from(
    el: &mut Element,
    keys: CollapseKeyframes,
    from_h: Option<f32>,
) {
    clear_motion(el);
    el.style.max_size.height = match from_h.filter(|h| *h > 1.0) {
        Some(h) => Dimension::length(h),
        None => Dimension::auto(),
    };
    el.style.animations = vec![keyframes_anim_entry(
        keys.up,
        keys.duration_ms,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollapseHeightEffect {
    Hidden,
    Measuring,
    SeededEnter,
    SeededExit,
    Animating,
    FinishedExit,
    Idle,
}

#[derive(Clone, Copy, Debug)]
pub struct CollapseHeightOpts {
    /// Pin exit `max-height` to the current clip (Expand). Accordion uses `auto`.
    pub pin_exit_height: bool,
    /// Expand: leave the last keyframe `max-height`. Accordion Idle resets to `auto`
    /// so in-page panels size to content. Auto on a floating Select ScrollArea is a
    /// one-row hitch (min-content) that placement then covers up.
    pub keep_open_max_height: bool,
}

impl Default for CollapseHeightOpts {
    fn default() -> Self {
        Self {
            pin_exit_height: false,
            keep_open_max_height: false,
        }
    }
}

/// Height keyframes + presence flags. Does not force `width: 100%` / `display: flex`.
pub fn apply_collapse_height<E>(
    el: &mut Element,
    presence: &PanelPresence<E>,
    keys: CollapseKeyframes,
    opts: CollapseHeightOpts,
) -> CollapseHeightEffect {
    let phase = presence.phase();
    if !phase.is_mounted() {
        return CollapseHeightEffect::Hidden;
    }
    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;
    let (needs_enter, needs_exit_anim) = presence.begin_frame_flags(open_visual, exiting);
    let exit_armed = presence.exit_armed();

    if exiting && (needs_exit_anim || (!has_exit_anim(el, keys.up) && exit_armed)) {
        let from = if opts.pin_exit_height {
            crate::layout::dim_length_px(el.style.max_size.height)
                .filter(|h| *h > 1.0)
                .or_else(|| {
                    let h = panel_content_height(el);
                    (h > 1.0).then_some(h)
                })
        } else {
            None
        };
        seed_collapse_exit_anim_from(el, keys, from);
        return CollapseHeightEffect::SeededExit;
    }
    if exiting {
        if animation_run_active(&el.style, &el.animation_snapshot) {
            return CollapseHeightEffect::Animating;
        }
        el.style.animations.clear();
        reset_anim_snapshot(el);
        el.style.max_size.height = Dimension::length(0.0);
        return CollapseHeightEffect::FinishedExit;
    }
    if open_visual && needs_enter {
        if !max_height_is_auto(el) {
            el.style.max_size.height = Dimension::auto();
        }
        let measured = panel_content_height(el);
        if measured < 1.0 {
            el.style.opacity = 0.0;
            return CollapseHeightEffect::Measuring;
        }
        presence.clear_needs_enter();
        if (el.style.opacity - 1.0).abs() > 0.001 {
            el.style.opacity = 1.0;
        }
        seed_enter_anim(el, keys);
        return CollapseHeightEffect::SeededEnter;
    }
    if open_visual {
        if animation_run_active(&el.style, &el.animation_snapshot) {
            return CollapseHeightEffect::Animating;
        }
        el.style.animations.clear();
        // Drop the keyframe snapshot here. An empty list + leftover `written`
        // makes the next CSS tick `restore_baseline` back to seed `max-height: auto`
        // (and `min-height: auto` → Taffy min-content ≈ one row).
        reset_anim_snapshot(el);
        if !opts.keep_open_max_height && el.style.max_size.height != Dimension::auto() {
            el.style.max_size.height = Dimension::auto();
        }
        presence.clear_enter_active();
    }
    CollapseHeightEffect::Idle
}

fn max_height_is_auto(el: &Element) -> bool {
    matches!(
        crate::layout::dim_kind(el.style.max_size.height),
        crate::layout::DimKind::Auto
    )
}

fn mark_layout(el: &Element, layout_tree: &mut LayoutTree) -> LayoutStatus {
    if el.layout_node.is_some() {
        layout_tree.update_taffy_node_style(el);
    }
    crate::app::damage::push_frame_paint_damage(el.layout_bounds);
    LayoutStatus::LayoutChanged
}

/// `max-height` + `@keyframes *-down/up`（默认路径）。
pub fn apply_collapse_motion(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    presence: &CollapsePanelPresence,
    keys: CollapseKeyframes,
) -> LayoutStatus {
    let phase = presence.phase();
    if !phase.is_mounted() {
        let mut layout = false;
        if el.style.display != Display::None {
            el.style.display = Display::None;
            layout = true;
        }
        if layout && el.layout_node.is_some() {
            return mark_layout(el, layout_tree);
        }
        return LayoutStatus::Clean;
    }

    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;
    let mut layout = false;
    let measuring_enter = open_visual && presence.needs_enter();

    if el.style.display != Display::Flex {
        el.style.display = Display::Flex;
        layout = true;
    }
    if el.style.overflow.y != Overflow::Hidden {
        el.style.overflow.y = Overflow::Hidden;
        layout = true;
    }
    if !measuring_enter && (el.style.opacity - 1.0).abs() > 0.001 {
        el.style.opacity = 1.0;
    }
    if el.style.scale != [1.0, 1.0] {
        el.style.scale = [1.0, 1.0];
    }

    let w = crate::layout::dim_kind(el.style.size.width);
    if !matches!(w, crate::layout::DimKind::Percent(_)) {
        el.style.size.width = Dimension::percent(1.0);
        layout = true;
    }
    if !matches!(
        crate::layout::dim_kind(el.style.size.height),
        crate::layout::DimKind::Auto
    ) {
        el.style.size.height = Dimension::auto();
        layout = true;
    }

    match apply_collapse_height(el, presence, keys, CollapseHeightOpts::default()) {
        CollapseHeightEffect::SeededEnter | CollapseHeightEffect::SeededExit => {
            mark_layout(el, layout_tree)
        }
        CollapseHeightEffect::FinishedExit => {
            presence.finish_exit();
            mark_layout(el, layout_tree)
        }
        CollapseHeightEffect::Animating => {
            if el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            LayoutStatus::LayoutChanged
        }
        CollapseHeightEffect::Measuring
        | CollapseHeightEffect::Idle
        | CollapseHeightEffect::Hidden => {
            if layout && el.layout_node.is_some() {
                return mark_layout(el, layout_tree);
            }
            if layout {
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
        let (_o, ()) = create_root(|| {
            let p = use_collapse_panel_presence("util_enter_measure");
            sync_collapse_panel_presence(&p, false);
            sync_collapse_panel_presence(&p, true);
            assert!(p.needs_enter());
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let mut tree = LayoutTree::new();
            let keys = CollapseKeyframes::COLLAPSIBLE;
            let _ = apply_collapse_motion(&mut el, &mut tree, &p, keys);
            assert!(
                p.needs_enter(),
                "must keep needs_enter until height is known"
            );
            assert!(
                !el.style.animations.iter().any(|a| a.name == keys.down),
                "must not seed down with 0 height"
            );
            el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
            let _ = apply_collapse_motion(&mut el, &mut tree, &p, keys);
            assert!(!p.needs_enter());
            assert!(
                el.style.animations.iter().any(|a| a.name == keys.down),
                "first real expand must run collapsible-down"
            );
        });
    }

    #[test]
    fn accordion_idle_resets_max_height_to_auto() {
        let (_o, ()) = create_root(|| {
            let p = use_collapse_panel_presence("idle_auto");
            sync_collapse_panel_presence(&p, false);
            sync_collapse_panel_presence(&p, true);
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
            el.used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 64.0], false);
            let keys = CollapseKeyframes::ACCORDION;
            assert!(matches!(
                apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default()),
                CollapseHeightEffect::SeededEnter
            ));
            let _ = el.tick_css_animations(0.0);
            for _ in 0..40 {
                let _ = el.tick_css_animations(0.016);
            }
            assert!(!animation_run_active(&el.style, &el.animation_snapshot));
            let effect = apply_collapse_height(&mut el, &p, keys, CollapseHeightOpts::default());
            assert!(matches!(effect, CollapseHeightEffect::Idle));
            assert!(
                matches!(
                    crate::layout::dim_kind(el.style.max_size.height),
                    crate::layout::DimKind::Auto
                ),
                "in-page accordion Idle must return to auto"
            );
        });
    }

    #[test]
    fn expand_idle_keeps_last_max_height() {
        let (_o, ()) = create_root(|| {
            let p = use_collapse_panel_presence("idle_keep");
            sync_collapse_panel_presence(&p, false);
            sync_collapse_panel_presence(&p, true);
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 192.0];
            el.used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
            let keys = CollapseKeyframes::ACCORDION;
            let opts = CollapseHeightOpts {
                pin_exit_height: true,
                keep_open_max_height: true,
            };
            assert!(matches!(
                apply_collapse_height(&mut el, &p, keys, opts),
                CollapseHeightEffect::SeededEnter
            ));
            let _ = el.tick_css_animations(0.0);
            for _ in 0..40 {
                let _ = el.tick_css_animations(0.016);
            }
            let last = crate::layout::dim_length_px(el.style.max_size.height);
            assert!(
                last.is_some_and(|h| (h - 192.0).abs() < 1.0),
                "last keyframe must reach content height, got {last:?}"
            );
            let effect = apply_collapse_height(&mut el, &p, keys, opts);
            assert!(matches!(effect, CollapseHeightEffect::Idle));
            let kept = crate::layout::dim_length_px(el.style.max_size.height);
            assert!(
                kept.is_some_and(|h| (h - last.unwrap()).abs() < 0.01),
                "Expand Idle must keep last keyframe max-height, got {kept:?} last {last:?}"
            );
            let _ = el.tick_css_animations(0.0);
            assert!(
                crate::layout::dim_length_px(el.style.max_size.height).is_some(),
                "cleared snapshot must not restore max-height auto"
            );
        });
    }
}
