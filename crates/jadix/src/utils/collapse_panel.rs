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
        duration_ms: 280.0,
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
    static PENDING_UNMOUNTS: RefCell<Vec<CollapsePanelPresence>> =
        const { RefCell::new(Vec::new()) };
}

use crate::utils::panel_presence::PanelPresenceCell;

pub type CollapsePanelPresence = PanelPresence<()>;

/// 按稳定 `key` 取（或建）一块面板 presence。多实例须带实例前缀。
pub fn use_collapse_panel_presence(key: &str) -> CollapsePanelPresence {
    use_panel_presence(
        &format!("__jadix_collapse_panel:{key}"),
        &format!("__jadix_collapse_mounted:{key}"),
        PANEL_EXIT,
        || (),
        |rc| {
            PANEL_PRESENCE_REGISTRY.with(|r| {
                let mut v = r.borrow_mut();
                if !v.iter().any(|c| Rc::ptr_eq(c, &rc)) {
                    v.push(rc);
                }
            });
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
        self.force_absent_flags();
        PENDING_UNMOUNTS.with(|q| q.borrow_mut().push(self.clone()));
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
    for presence in pending {
        // A new open may arrive after the closing frame, before this drain.
        // Only the still-absent panel belongs to the completed exit.
        let sig = presence.mounted();
        if presence.phase() == PresencePhase::Absent && sig.is_alive() {
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

#[derive(Clone, Copy, Debug, Default)]
pub struct CollapseHeightOpts {
    /// Pin exit `max-height` to the current clip (Expand). Accordion uses `auto`.
    pub pin_exit_height: bool,
    /// Expand: leave the last keyframe `max-height`. Accordion Idle resets to `auto`
    /// so in-page panels size to content. Auto on a floating Select ScrollArea is a
    /// one-row hitch (min-content) that placement then covers up.
    pub keep_open_max_height: bool,
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
        let reversing_enter = el.style.animations.iter().any(|a| a.name == keys.down);
        let from = if opts.pin_exit_height || reversing_enter {
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
        let reverse_from = has_exit_anim(el, keys.up)
            .then(|| crate::layout::dim_length_px(el.style.max_size.height))
            .flatten();
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
        if let Some(from) = reverse_from.filter(|h| *h > 0.0) {
            // Continue along the down curve from the currently visible clip.
            // Recover its time by inverting the easing rather than restarting at 0.
            let ratio = (from / measured).clamp(0.0, 1.0);
            let mut lo = 0.0;
            let mut hi = 1.0;
            for _ in 0..20 {
                let mid = (lo + hi) * 0.5;
                if TimingFunction::EaseOut.eval(mid) < ratio {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            el.tick_css_animations(0.0);
            if let Some(run) = el.animation_snapshot.runs.first_mut() {
                run.elapsed_ms = (lo + hi) * 0.5 * keys.duration_ms;
            }
            el.tick_css_animations(0.0);
        }
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
    crate::app::damage::push_frame_region_paint(
        crate::app::damage::paint_element_key(el),
        crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc),
    );
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

    let previous_max_height = el.style.max_size.height;
    let effect = apply_collapse_height(el, presence, keys, CollapseHeightOpts::default());
    if effect == CollapseHeightEffect::Measuring {
        el.set_attr("jadix_collapse_measuring", "1");
    } else {
        el.attrs.remove("jadix_collapse_measuring");
    }
    layout |= el.style.max_size.height != previous_max_height;
    let status = match effect {
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
            // A growing clip reveals descendants whose retained node slots did not
            // exist in the previous frame. Repair the region, including new ink.
            crate::app::damage::push_frame_region_paint(
                crate::app::damage::paint_element_key(el),
                crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc),
            );
            LayoutStatus::LayoutChanged
        }
        CollapseHeightEffect::Measuring
        | CollapseHeightEffect::Idle
        | CollapseHeightEffect::Hidden => {
            if layout {
                mark_layout(el, layout_tree)
            } else {
                LayoutStatus::Clean
            }
        }
    };
    if status == LayoutStatus::LayoutChanged {
        el.set_attr("jadix_collapse_damage", "1");
    }
    status
}

/// Queue the new clip after layout as well as the previous clip before layout.
pub(crate) fn flush_collapse_layout_damage(root: &mut Element) {
    walk_collapse_work(root, &mut |el| {
        if el.attrs.remove("jadix_collapse_damage").is_some() {
            crate::app::damage::push_frame_region_paint(
                crate::app::damage::paint_element_key(el),
                crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc),
            );
        }
    });
}

fn walk_collapse_work(root: &mut Element, visit: &mut impl FnMut(&mut Element)) {
    // Unknown/invalidated hashes must still discover newly mounted callbacks.
    if root.collect_paint_hash != 0 && !root.subtree_collapse_work {
        return;
    }
    crate::core::tree::walk_elements_mut_pruned(root, |el| {
        if el.collect_paint_hash != 0 && !el.subtree_collapse_work {
            return false;
        }
        visit(el);
        true
    });
}

/// Finish first-open measurement before presenting the unconstrained layout.
pub(crate) fn seed_measured_collapse_panels(
    root: &mut Element,
    layout_tree: &mut LayoutTree,
) -> bool {
    let mut changed = false;
    walk_collapse_work(root, &mut |el| {
        if !el.attrs.contains_key("jadix_collapse_measuring") || panel_content_height(el) < 1.0 {
            return;
        }
        if let Some(sync) = el.reactive_sync.clone() {
            let status = sync.run(el, layout_tree);
            if status != LayoutStatus::Clean && !el.attrs.contains_key("jadix_collapse_measuring") {
                // Sample the starting clip at zero elapsed time, then lay it out.
                el.tick_css_animations(0.0);
                layout_tree.update_taffy_node_style(el);
                changed = true;
            }
        }
    });
    changed
}

#[cfg(test)]
#[path = "tests/collapse_panel.rs"]
mod tests;
