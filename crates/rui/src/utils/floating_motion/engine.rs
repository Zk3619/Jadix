//! Shared motion for anchored overlays (Popover / Menu / Tooltip / Select): presence + CSS keyframes.
//!
//! - [`FloatingMotionKind::Fade`]: Dialog single-panel `fade-in-0` / `fade-out-0` (300 / 200ms)
//! - [`FloatingMotionKind::Expand`]: Accordion `accordion-down` / `accordion-up` (200ms)
//!
//! Portal stack uses [`crate::app::portal_overlay::portal_sync_presence`]: Esc clears logical `open`,
//! `mounted` stays alive through exit. Do not reuse [`crate::utils::modal_motion`] (dual surface + Dialog zoom).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::style::{
    animation_run_active, keyframes::KeyframeStop, keyframes_anim_entry, register_keyframes,
    AnimationFillMode, TimingFunction,
};
use crate::utils::collapse_panel::{
    apply_collapse_height, CollapseHeightEffect, CollapseHeightOpts, CollapseKeyframes,
};
use crate::utils::motion::clear_motion;
use crate::utils::panel_presence::{
    sync_panel_presence, tick_panel_cells, use_panel_presence, PanelPresence, PanelPresenceCell,
    SyncEvent, SyncPolicy,
};
use crate::utils::popup::{
    clamp_animated_max_h_to_place, peek_floating_place_max_h, ATTR_FLOATING_CONTENT_H,
    ATTR_FLOATING_EXITING, ATTR_FLOATING_EXPAND_CLIP_X, ATTR_FLOATING_EXPAND_FROM_H,
    ATTR_FLOATING_EXPAND_FROM_W, ATTR_FLOATING_EXPAND_MEASURE,
};
use crate::utils::presence::PresencePhase;
use taffy::style::{Dimension, FlexWrap, Overflow};

/// Dialog-aligned fade enter.
const FADE_ENTER_MS: f32 = 300.0;
/// Dialog-aligned fade exit.
const FADE_EXIT_MS: f32 = 200.0;
const EXPAND_KEYS: CollapseKeyframes = CollapseKeyframes::ACCORDION;
/// Accordion-aligned expand (`CollapseKeyframes::ACCORDION`).
const EXPAND_MS: f32 = EXPAND_KEYS.duration_ms;
const EXIT_DURATION: Duration = Duration::from_millis(FADE_EXIT_MS as u64 + 40);
const EXPAND_EXIT_DURATION: Duration = Duration::from_millis(EXPAND_MS as u64 + 40);

thread_local! {
    static EXPAND_OFFSET_KEYFRAMES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

fn floating_expand_from_h(el: &Element) -> Option<f32> {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_FROM_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 0.5)
}

fn floating_expand_from_w(el: &Element) -> Option<f32> {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_FROM_W)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|w| *w > 0.5)
}

fn expand_clip_x(el: &Element) -> bool {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_CLIP_X)
        .is_some_and(|s| s != "0")
}

fn expand_anim_progress(el: &Element) -> f32 {
    let Some(entry) = el.style.animations.first() else {
        return 1.0;
    };
    let elapsed = el
        .animation_snapshot
        .runs
        .first()
        .map(|r| r.elapsed_ms)
        .unwrap_or(0.0);
    let dur = entry.duration_ms.max(1.0);
    ((elapsed - entry.delay_ms).max(0.0) / dur).clamp(0.0, 1.0)
}

fn expand_width_clip_active(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| is_expand_down_anim(&a.name) || is_expand_up_anim(&a.name))
}

/// Clip extra row content with `max-width` on the same 200ms ease as accordion height.
fn apply_expand_width_clip(panel: &mut Element, presence: &FloatingPresenceHandle, exiting: bool) {
    let Some(from_w) = floating_expand_from_w(panel) else {
        return;
    };
    panel.style.overflow.x = Overflow::Hidden;
    if !expand_clip_x(panel) {
        let measuring = presence.cell.borrow().measuring;
        if !measuring && !expand_width_clip_active(panel) {
            return;
        }
        panel.style.max_size.width = Dimension::length(from_w);
        presence.cell.borrow_mut().clip_width = Some(from_w);
        return;
    }
    if !expand_width_clip_active(panel) {
        return;
    }
    panel.style.min_size.width = Dimension::length(0.0);
    panel.style.flex_wrap = FlexWrap::NoWrap;
    let to_w = panel
        .used_layout
        .anim_width()
        .max(panel.layout_bounds[2])
        .max(from_w)
        .max(1.0);
    let t = expand_anim_progress(panel);
    let e = if exiting {
        TimingFunction::EaseIn.eval(t)
    } else {
        TimingFunction::EaseOut.eval(t)
    };
    let w = if exiting {
        from_w + (to_w - from_w) * (1.0 - e)
    } else {
        from_w + (to_w - from_w) * e
    };
    panel.style.max_size.width = Dimension::length(w);
    presence.cell.borrow_mut().clip_width = Some(w);
}

fn is_expand_down_anim(name: &str) -> bool {
    name == EXPAND_KEYS.down || name.starts_with("rui-expand-down-")
}

fn is_expand_up_anim(name: &str) -> bool {
    name == EXPAND_KEYS.up || name.starts_with("rui-expand-up-")
}

fn ensure_expand_down_from_keyframe(from_h: f32) -> String {
    let from_h = from_h.max(0.0);
    let key = format!("rui-expand-down-{}", from_h.round().max(1.0) as u32);
    EXPAND_OFFSET_KEYFRAMES.with(|cache| {
        let mut set = cache.borrow_mut();
        if !set.contains(&key) {
            register_keyframes(&key, vec![KeyframeStop::at(0.0).max_height(from_h)]);
            set.insert(key.clone());
        }
    });
    key
}

/// Accordion-down is single-ended (`0%` → 0, `100%` → used intrinsic). Select year
/// lists are thousands of px; interpolating to that and clamping each frame lets the
/// finishing tick write unconstrained height, then Idle/placement snap to the cap
/// (last-frame shrink-then-recover). Pin the `100%` stop to the painted open height.
fn ensure_expand_down_to_keyframe(to_h: f32) -> String {
    let to_h = to_h.max(0.0);
    let key = format!("rui-expand-down-to-{}", to_h.round().max(1.0) as u32);
    EXPAND_OFFSET_KEYFRAMES.with(|cache| {
        let mut set = cache.borrow_mut();
        if !set.contains(&key) {
            register_keyframes(
                &key,
                vec![
                    KeyframeStop::at(0.0).max_height(0.0),
                    KeyframeStop::at(1.0).max_height(to_h),
                ],
            );
            set.insert(key.clone());
        }
    });
    key
}

fn ensure_expand_up_to_keyframe(to_h: f32) -> String {
    let to_h = to_h.max(0.0);
    let key = format!("rui-expand-up-{}", to_h.round().max(1.0) as u32);
    EXPAND_OFFSET_KEYFRAMES.with(|cache| {
        let mut set = cache.borrow_mut();
        if !set.contains(&key) {
            register_keyframes(&key, vec![KeyframeStop::at(1.0).max_height(to_h)]);
            set.insert(key.clone());
        }
    });
    key
}

pub use crate::utils::floating_kind::FloatingMotionKind;

fn panel_tid(overlay_id: &str) -> String {
    format!("{overlay_id}__floating_panel")
}

#[derive(Clone)]
struct FloatingShellState {
    overlay_id: String,
    kind: FloatingMotionKind,
    phase: PresencePhase,
    overlay_kind: OverlayKind,
    overlay_mode: OverlayMode,
    /// Logical open (Esc dismisses this); drain uses it with mounted.
    open: Signal<bool>,
    /// Expand: user max-height cap (logical px); `None` = uncapped.
    max_height_cap: Option<f32>,
}

#[derive(Clone)]
pub(crate) struct FloatingExtra {
    shell: Option<FloatingShellState>,
    /// Survives `shell = None` after exit so orphan prune can tell a live
    /// (closed) overlay from a disposed owner.
    logical_open: Option<Signal<bool>>,
    /// Last accordion clip height (interpolated max-height). Rebuild/placement must not
    /// flash the viewport cap before flush re-seeds.
    clip_height: Option<f32>,
    /// Last `max-width` clip when [`ATTR_FLOATING_EXPAND_FROM_W`] is set.
    clip_width: Option<f32>,
    /// Expand: waiting for an unconstrained layout before seeding accordion-down.
    measuring: bool,
    /// Seed already ticked dt=0 this present. Skip `frame_dt` once so enter
    /// does not EaseOut from 0 to ~one Select row on the first painted frame.
    hold_enter_css_dt: bool,
}

type FloatingPresenceCell = PanelPresenceCell<FloatingExtra>;

/// Per-instance presence + motion ([`use_floating_presence`]).
pub(crate) type FloatingPresenceHandle = PanelPresence<FloatingExtra>;

impl FloatingPresenceHandle {
    /// Enter seed / measure / exit — Command overlay must not focus the filter until this is false.
    #[allow(dead_code)]
    pub(crate) fn is_motion_busy(&self) -> bool {
        let c = self.cell.borrow();
        c.needs_enter || c.enter_active || c.measuring || c.gate.phase() == PresencePhase::Exiting
    }
}

thread_local! {
    static PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<FloatingPresenceCell>>>> =
        const { RefCell::new(Vec::new()) };
    static PENDING_UNMOUNTS: RefCell<
        Vec<(
            Signal<bool>,
            Signal<bool>,
            String,
            OverlayKind,
            OverlayMode,
        )>,
    > = const { RefCell::new(Vec::new()) };
    /// Post-hoist CSS still sampling. Same role as `MODAL_CSS_NEEDS_FRAME` — a live
    /// query, not a one-shot. Parent Fade settling must not swallow a nested Expand.
    static FLOATING_CSS_NEEDS_FRAME: Cell<bool> = const { Cell::new(false) };
    /// Expand max-height / width-clip wrote Taffy geometry this frame.
    /// Fade is paint-only (opacity) — must not force a second full-tree collect.
    static FLOATING_NEEDS_RELAYOUT: Cell<bool> = const { Cell::new(false) };
}

/// True while a stacked Popover/Menu is measuring, accordion-ing, fading, or exiting.
///
/// `about_to_wait` / caret `WaitUntil` must Poll on this the same way they Poll on
/// [`crate::utils::modal_motion::any_modal_css_needs_frame`]. A one-shot armed at
/// `Enter` is wrong: ColorPicker Fade consumes it, then the nested format Select
/// Expand's follow-up frame is coalesced and the menu stays opacity-0 until the
/// next pointer event.
pub(crate) fn any_floating_motion_needs_frame() -> bool {
    if FLOATING_CSS_NEEDS_FRAME.with(|c| c.get()) {
        return true;
    }
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow().iter().any(|cell| {
            let c = cell.borrow();
            c.gate.is_mounted()
                && (c.needs_enter
                    || c.enter_active
                    || c.measuring
                    || c.needs_exit_anim
                    || c.gate.phase() == PresencePhase::Exiting)
        })
    })
}

/// True when Expand wrote layout geometry (max-height clip / measure) this frame.
///
/// Fade enter/exit only samples opacity. A second `build_and_layout_fast_ex` of the
/// whole app tree on every opacity tick is the gallery hitch with a Popover open.
pub(crate) fn floating_motion_needs_relayout() -> bool {
    FLOATING_NEEDS_RELAYOUT.with(|c| c.get())
}

pub(crate) fn use_floating_presence(
    inst_id: &str,
    kind: FloatingMotionKind,
) -> FloatingPresenceHandle {
    let key = format!("__rui_floating_presence:{inst_id}");
    let mounted_key = format!("__rui_floating_mounted:{inst_id}");
    let exit_dur = match kind {
        FloatingMotionKind::Fade => EXIT_DURATION,
        FloatingMotionKind::Expand => EXPAND_EXIT_DURATION,
    };
    let handle = use_panel_presence(
        key.as_str(),
        mounted_key.as_str(),
        exit_dur,
        || FloatingExtra {
            shell: None,
            logical_open: None,
            clip_height: None,
            clip_width: None,
            measuring: false,
            hold_enter_css_dt: false,
        },
        |rc| {
            register_floating_presence_cell(&rc);
        },
        |rc| {
            // Owner teardown (TabsContent, page switch) must not drop a live
            // overlay from the registry: flush still has to seed the exit.
            // Nested-island rebuilds while open hit the same path.
            if rc.borrow().gate.is_mounted() {
                return;
            }
            PRESENCE_REGISTRY.with(|r| {
                r.borrow_mut().retain(|c| !Rc::ptr_eq(c, &rc));
            });
        },
    );
    take_over_stale_floating_presence(&handle);
    handle
}

/// Flush / CSS tick walk this registry. Closed overlays have no `shell` yet, so
/// [`prune_finished_orphaned_floating`] would drop them on the first idle tick
/// unless we keep `logical_open` and re-bind here after that prune.
fn register_floating_presence_cell(rc: &Rc<RefCell<FloatingPresenceCell>>) {
    PRESENCE_REGISTRY.with(|r| {
        let mut v = r.borrow_mut();
        if !v.iter().any(|c| Rc::ptr_eq(c, rc)) {
            v.push(Rc::clone(rc));
        }
    });
}

fn take_over_stale_floating_presence(handle: &FloatingPresenceHandle) {
    let stale = {
        let c = handle.cell.borrow();
        if !c.gate.is_mounted() {
            return;
        }
        let sig = c.logical_open.or_else(|| c.shell.as_ref().map(|s| s.open));
        match sig {
            Some(s) => s.try_peek().is_none(),
            None => false,
        }
    };
    if !stale {
        return;
    }
    // Named presence outlives TabsContent. A new owner must not inherit the
    // previous instance's Exiting gate (flash open → immediate fade-out).
    handle.force_absent_flags();
    {
        let mut c = handle.cell.borrow_mut();
        c.shell = None;
        c.logical_open = None;
        c.clip_height = None;
        c.clip_width = None;
        c.measuring = false;
    }
    let mounted = floating_mounted_signal(handle);
    if mounted.try_peek() == Some(true) {
        mounted.set(false);
    }
}

pub(crate) fn floating_mounted_signal(handle: &FloatingPresenceHandle) -> Signal<bool> {
    handle.cell.borrow().mounted
}

/// Skip unflushed enter initial state (for tests that assert final paint / layout).
#[cfg(test)]
pub(crate) fn clear_pending_floating_enter(handle: &FloatingPresenceHandle) {
    let mut c = handle.cell.borrow_mut();
    c.needs_enter = false;
}

/// Sync logical `open`; returns phase. If something cleared `mounted` (mutex kick), become Absent immediately.
pub(crate) fn sync_floating_presence(
    handle: &FloatingPresenceHandle,
    want_open: bool,
) -> PresencePhase {
    sync_panel_presence(handle, want_open, SyncPolicy::FLOATING, |ev, c| match ev {
        SyncEvent::Enter => {
            c.clip_height = None;
            c.clip_width = None;
            c.measuring = false;
        }
        SyncEvent::MutexAbsent => {
            c.shell = None;
            c.clip_height = None;
            c.clip_width = None;
            c.measuring = false;
        }
        SyncEvent::ExitArm => {}
    })
}

fn floating_kind_has_exit(kind: FloatingMotionKind) -> bool {
    match kind {
        FloatingMotionKind::Fade | FloatingMotionKind::Expand => true,
    }
}

fn should_arm_orphan_floating_exit(c: &FloatingPresenceCell) -> bool {
    if !c.gate.is_mounted() || c.shell.is_none() {
        return false;
    }
    let Some(sig) = c.logical_open.or_else(|| c.shell.as_ref().map(|s| s.open)) else {
        return false;
    };
    // Live `open=false` is handled by the overlay island. Only owner-dispose
    // (signal gone) should be armed from the registry.
    sig.try_peek().is_none()
}

/// Owner dispose / exclusive close may never re-enter the overlay island.
/// Arm the configured exit from the registry so flush can seed keyframes.
fn arm_orphaned_floating_exits() -> bool {
    let mut armed = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let handle = FloatingPresenceHandle {
                cell: Rc::clone(cell_rc),
            };
            let has_exit = {
                let c = handle.cell.borrow();
                if !should_arm_orphan_floating_exit(&c) {
                    continue;
                }
                c.shell
                    .as_ref()
                    .is_some_and(|s| floating_kind_has_exit(s.kind))
            };
            if !has_exit {
                handle.force_absent_flags();
                let mounted_sig = floating_mounted_signal(&handle);
                if mounted_sig.try_peek() == Some(true) {
                    mounted_sig.set(false);
                }
                continue;
            }
            let phase = sync_floating_presence(&handle, false);
            if let Some(s) = handle.cell.borrow_mut().shell.as_mut() {
                s.phase = phase;
            }
            if phase == PresencePhase::Exiting {
                armed = true;
            }
        }
    });
    if armed {
        crate::app::runtime::request_redraw();
    }
    armed
}

fn prune_finished_orphaned_floating() {
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow_mut().retain(|cell| {
            let c = cell.borrow();
            if c.gate.is_mounted() {
                return true;
            }
            c.logical_open
                .or_else(|| c.shell.as_ref().map(|s| s.open))
                .is_some_and(|s| s.try_peek().is_some())
        });
    });
}

/// Event loop: CSS exit ends on flush; wall clock is fallback only (do not unmount on tick while exit_armed).
pub(crate) fn tick_floating_presence() -> bool {
    drain_pending_floating_unmounts();
    let orphaned = arm_orphaned_floating_exits();
    let now = Instant::now();
    let outcome = PRESENCE_REGISTRY.with(|reg| {
        tick_panel_cells(
            reg.borrow().as_slice(),
            now,
            |c| c.exit_armed,
            |c| {
                c.shell = None;
            },
            true,
        )
    });
    for sig in outcome.unmount {
        sig.set(false);
    }
    prune_finished_orphaned_floating();
    outcome.needs_frame || orphaned
}

fn find_template_id_mut<'a>(el: &'a mut Element, want: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_by_template_id_mut(el, want)
}

fn fade_has_exit_anim(el: &Element) -> bool {
    el.style.animations.iter().any(|a| a.name == "fade-out-0")
}

fn expand_has_exit_anim(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| is_expand_up_anim(&a.name))
}

fn reset_anim_snapshot(el: &mut Element) {
    el.animation_snapshot = Default::default();
}

fn seed_fade_enter(el: &mut Element) {
    clear_motion(el);
    el.style.opacity = 0.0;
    el.style.animations = vec![keyframes_anim_entry(
        "fade-in-0",
        FADE_ENTER_MS,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
}

fn seed_fade_exit(el: &mut Element) {
    clear_motion(el);
    el.style.opacity = 1.0;
    el.style.animations = vec![keyframes_anim_entry(
        "fade-out-0",
        FADE_EXIT_MS,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
}

fn seed_expand_enter(el: &mut Element, content_height: f32) {
    if let Some(from_h) = floating_expand_from_h(el) {
        clear_motion(el);
        el.style.max_size.height = Dimension::auto();
        el.style.animations = vec![keyframes_anim_entry(
            ensure_expand_down_from_keyframe(from_h.min(content_height)),
            EXPAND_KEYS.duration_ms,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        )];
        reset_anim_snapshot(el);
    } else {
        clear_motion(el);
        el.style.max_size.height = Dimension::auto();
        el.style.animations = vec![keyframes_anim_entry(
            ensure_expand_down_to_keyframe(content_height),
            EXPAND_KEYS.duration_ms,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        )];
        reset_anim_snapshot(el);
    }
    el.style.opacity = 1.0;
    // dt=0: apply fill-mode backwards only (max-height:0); avoid a large first-frame dt skipping enter.
    let _ = el.tick_css_animations(0.0);
    el.set_attr(
        ATTR_FLOATING_CONTENT_H,
        format!("{}", content_height.max(1.0)),
    );
    el.attrs.remove(ATTR_FLOATING_EXPAND_MEASURE);
}

fn remember_clip_height(presence: &FloatingPresenceHandle, el: &Element) {
    let clip = crate::layout::dim_length_px(el.style.max_size.height).filter(|v| *v >= 0.0);
    let painted = el
        .attrs
        .get(ATTR_FLOATING_CONTENT_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 0.5)
        .or_else(|| (el.layout_bounds[3] > 0.5).then_some(el.layout_bounds[3]));
    let h = match (clip, painted) {
        (Some(c), Some(p)) => c.min(p),
        (Some(c), None) => c,
        (None, Some(p)) => p,
        (None, None) => el.layout_bounds[3],
    };
    presence.cell.borrow_mut().clip_height = Some(h);
}

/// Hide the collapsing panel when accordion-up finished.
/// Must run **before** the post-tick layout/collect; opacity 0 after collect still paints.
/// Mid-exit must keep opacity so the clip can finish (`from_h` cover overlays land on the
/// trigger; menus without `from_h` run to 0). Cutting off at ~16px looked like a row snap.
fn hide_expand_exit_sliver(
    panel: &mut Element,
    presence: &FloatingPresenceHandle,
    finish: bool,
) -> bool {
    let done = finish || !animation_run_active(&panel.style, &panel.animation_snapshot);
    if !done {
        return false;
    }
    let mut changed = false;
    if panel.style.opacity > 0.0 {
        panel.style.opacity = 0.0;
        changed = true;
    }
    if !panel.style.animations.is_empty() {
        panel.style.animations.clear();
        changed = true;
    }
    if crate::layout::dim_length_px(panel.style.max_size.height).is_none_or(|v| v > 0.5) {
        panel.style.max_size.height = Dimension::length(0.0);
        changed = true;
    }
    finish_floating_exit(presence);
    changed
}

fn seed_expand_exit(el: &mut Element, content_height: f32) {
    clear_motion(el);
    // If already mid-collapse (rebuild dropped the animation list), resume from current visible height — do not jump to full height.
    let from = crate::layout::dim_length_px(el.style.max_size.height)
        .filter(|h| *h > 1.0)
        .unwrap_or(content_height)
        .min(content_height.max(0.0))
        .max(1.0);
    el.style.max_size.height = Dimension::length(from);
    // Covered triggers (Bubble +N / AvatarGroup +N) set `from_h` so enter *and* exit
    // clip against the pill still in flow. Accordion-up to 0 shrinks through that
    // pill. Select menus do not set `from_h` and still use accordion-up → 0.
    let anim = match floating_expand_from_h(el).filter(|h| *h > 0.5) {
        Some(to_h) => ensure_expand_up_to_keyframe(to_h.min(from)),
        None => EXPAND_KEYS.up.to_string(),
    };
    el.style.animations = vec![keyframes_anim_entry(
        anim,
        EXPAND_KEYS.duration_ms,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
    let _ = el.tick_css_animations(0.0);
    el.set_attr(
        ATTR_FLOATING_CONTENT_H,
        format!("{}", content_height.max(1.0)),
    );
}

fn max_height_is_auto(el: &Element) -> bool {
    matches!(
        crate::layout::dim_kind(el.style.max_size.height),
        crate::layout::DimKind::Auto
    )
}

fn take_measured_height(panel: &Element, max_height_cap: Option<f32>) -> f32 {
    let h = panel.used_layout.anim_height().max(panel.layout_bounds[3]);
    if h <= 1.0 {
        return 0.0;
    }
    match max_height_cap {
        Some(max) if max > 0.0 => h.min(max),
        _ => h,
    }
}

fn prepare_expand_measure(panel: &mut Element, max_height_cap: Option<f32>) {
    panel.set_attr(ATTR_FLOATING_EXPAND_MEASURE, "1");
    panel.style.opacity = 0.0;
    panel.style.overflow.y = Overflow::Hidden;
    if floating_expand_from_w(panel).is_some() {
        panel.style.overflow.x = Overflow::Hidden;
        if expand_clip_x(panel) {
            panel.style.max_size.width = Dimension::auto();
        } else if let Some(from_w) = floating_expand_from_w(panel) {
            panel.style.max_size.width = Dimension::length(from_w);
        }
    }
    if let Some(cap) = max_height_cap.filter(|h| *h > 0.0) {
        panel.style.max_size.height = Dimension::length(cap);
    } else if !max_height_is_auto(panel) {
        panel.style.max_size.height = Dimension::auto();
    }
}

fn apply_idle_expand_max_h(panel: &mut Element, presence: &FloatingPresenceHandle) {
    let overlay_id = presence
        .cell
        .borrow()
        .shell
        .as_ref()
        .map(|s| s.overlay_id.clone());
    if let Some(id) = overlay_id {
        if let Some(cap) = peek_floating_place_max_h(&id).filter(|h| *h > 0.0) {
            // Place cap is an overflow limit (16rem / viewport). Writing it as
            // max-height when the list is shorter stretches ScrollArea `h:100%`
            // and reverse-glue treats the cap as painted height (jump up on exit).
            let painted = panel
                .attrs
                .get(ATTR_FLOATING_CONTENT_H)
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|h| *h > 0.5)
                .or_else(|| {
                    crate::layout::dim_length_px(panel.style.max_size.height).filter(|h| *h > 0.5)
                })
                .unwrap_or(cap);
            panel.style.max_size.height = Dimension::length(cap.min(painted));
        }
        // No place cap: keep the last accordion sample. Writing `auto` here lets
        // Taffy min-content the unpinned ScrollArea to one row for a frame.
    }
    if floating_expand_from_w(panel).is_some() {
        panel.style.max_size.width = Dimension::auto();
        presence.cell.borrow_mut().clip_width = None;
    }
}

fn node_is_scroll_viewport(el: &Element) -> bool {
    el.focus_id.as_deref().is_some_and(|id| {
        id.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
    })
}

fn node_wraps_scroll_viewport(el: &Element) -> bool {
    el.children
        .iter()
        .any(|c| node_is_scroll_viewport(c) || node_wraps_scroll_viewport(c))
}

fn expand_node_is_pinned(el: &Element) -> bool {
    (el.style.flex_shrink < 0.5
        && crate::layout::dim_length_px(el.style.min_size.height).is_some_and(|h| h > 1.0))
        || el.children.iter().any(expand_node_is_pinned)
}

fn expand_contents_are_pinned(panel: &Element) -> bool {
    panel.children.iter().any(expand_node_is_pinned)
}

fn pin_expand_overflow_node(el: &mut Element, inner: f32) {
    if node_is_scroll_viewport(el) {
        // Viewport must shrink with the accordion so overflow:auto clips a short
        // list (ColorPicker 6 formats). Pin the rows, not the scrollport.
        for child in &mut el.children {
            child.style.flex_shrink = 0.0;
            child.style.min_size.height = Dimension::length(inner);
        }
        return;
    }
    if node_wraps_scroll_viewport(el) {
        for child in &mut el.children {
            if node_is_scroll_viewport(child) || node_wraps_scroll_viewport(child) {
                pin_expand_overflow_node(child, inner);
            }
        }
        return;
    }
    el.style.flex_shrink = 0.0;
    el.style.min_size.height = Dimension::length(inner);
}

fn restore_expand_overflow_node(el: &mut Element) {
    if node_is_scroll_viewport(el) {
        for child in &mut el.children {
            child.style.flex_shrink = 1.0;
            child.style.min_size.height = Dimension::length(0.0);
        }
        return;
    }
    if node_wraps_scroll_viewport(el) {
        for child in &mut el.children {
            if node_is_scroll_viewport(child) || node_wraps_scroll_viewport(child) {
                restore_expand_overflow_node(child);
            }
        }
        return;
    }
    el.style.flex_shrink = 1.0;
    el.style.min_size.height = Dimension::length(0.0);
}

fn panel_content_height(el: &Element, cap: Option<f32>) -> f32 {
    take_measured_height(el, cap).max(el.layout_bounds[3].max(0.0))
}

fn expand_open_content_h(el: &Element, cap: Option<f32>) -> f32 {
    let raw = el
        .attrs
        .get(ATTR_FLOATING_CONTENT_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 1.0)
        .unwrap_or_else(|| panel_content_height(el, cap));
    match cap.filter(|c| *c > 0.0) {
        Some(c) => raw.min(c),
        None => raw,
    }
}

/// Accordion `max-height` must overflow-clip the open list. Select's ScrollArea is
/// `h:100%; min-h:0; flex-shrink:1` so it can scroll when idle — under a shrinking
/// clip that relayouts the viewport to one row (month menu becoming a "March" chip).
/// Pin the *rows inside* the scroll viewport to the measured open height; do not pin
/// the ScrollArea wrapper (that makes viewport==content so a short list never clips).
fn pin_expand_overflow_contents(panel: &mut Element, max_height_cap: Option<f32>) {
    if floating_expand_from_h(panel).is_some() {
        return;
    }
    if !panel
        .style
        .animations
        .iter()
        .any(|a| is_expand_down_anim(&a.name) || is_expand_up_anim(&a.name))
    {
        return;
    }
    let h = expand_open_content_h(panel, max_height_cap);
    if h <= 1.0 {
        return;
    }
    // Accordion `max-height` is on the padded chrome. Pinning children to the same
    // px as the cap makes Taffy used height = cap + padding for the last frames;
    // Idle then unpins and the panel snaps down by that padding, then placement
    // restores the cap.
    let pad_y = crate::layout::lp_length_px(panel.style.padding.top).unwrap_or(0.0)
        + crate::layout::lp_length_px(panel.style.padding.bottom).unwrap_or(0.0);
    let inner = (h - pad_y).max(1.0);
    for child in &mut panel.children {
        pin_expand_overflow_node(child, inner);
    }
}

/// Idle / uncapped chrome: ScrollArea must shrink again (`min-h:0`) so overflow can scroll.
fn restore_expand_overflow_contents(panel: &mut Element) {
    if floating_expand_from_h(panel).is_some() {
        return;
    }
    for child in &mut panel.children {
        restore_expand_overflow_node(child);
    }
}

/// Grow used-layout intrinsic; never shrink to the current viewport clip.
fn absorb_expand_intrinsic(panel: &mut Element, max_height_cap: Option<f32>) {
    let h = panel.layout_bounds[3];
    if h > 1.0 {
        let capped = match max_height_cap {
            Some(max) if max > 0.0 => h.min(max),
            _ => h,
        };
        panel.used_layout.absorb_child_height(capped);
        let stored = match max_height_cap {
            Some(max) if max > 0.0 => panel.used_layout.anim_height().min(max),
            _ => panel.used_layout.anim_height(),
        };
        panel.set_attr(ATTR_FLOATING_CONTENT_H, format!("{}", stored));
    }
    let w = panel.layout_bounds[2];
    if w > 1.0 {
        panel.used_layout.absorb_child_width(w);
    }
}

fn finish_floating_exit(presence: &FloatingPresenceHandle) {
    {
        let c = presence.cell.borrow();
        if c.shell.is_none() && !c.exit_armed && !c.gate.is_mounted() {
            return;
        }
    }
    let mounted_sig = floating_mounted_signal(presence);
    let meta = {
        let mut c = presence.cell.borrow_mut();
        c.gate.force_absent();
        c.exit_armed = false;
        c.needs_exit_anim = false;
        c.enter_active = false;
        let meta = c
            .shell
            .as_ref()
            .map(|s| (s.overlay_id.clone(), s.open, s.overlay_kind, s.overlay_mode));
        c.shell = None;
        c.clip_height = None;
        c.clip_width = None;
        c.measuring = false;
        meta
    };
    if let Some((overlay_id, open, overlay_kind, overlay_mode)) = meta {
        PENDING_UNMOUNTS.with(|q| {
            q.borrow_mut()
                .push((mounted_sig, open, overlay_id, overlay_kind, overlay_mode));
        });
    } else {
        PENDING_UNMOUNTS.with(|q| {
            q.borrow_mut().push((
                mounted_sig,
                mounted_sig,
                String::new(),
                OverlayKind::Menu,
                OverlayMode::Stacked,
            ));
        });
    }
    crate::app::runtime::request_redraw();
}

/// Drain CSS exit unmounts. Call only when AppState is not borrowed.
pub(crate) fn drain_pending_floating_unmounts() {
    let pending = PENDING_UNMOUNTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if pending.is_empty() {
        return;
    }
    use crate::app::portal_overlay::portal_sync_presence;
    for (mounted_sig, open, overlay_id, overlay_kind, overlay_mode) in pending {
        mounted_sig.set(false);
        if !overlay_id.is_empty() {
            let _ =
                portal_sync_presence(&overlay_id, open, mounted_sig, overlay_kind, overlay_mode);
        }
    }
    crate::app::runtime::try_hoist_portals_now();
}

fn apply_fade_motion(
    panel: &mut Element,
    phase: PresencePhase,
    presence: &FloatingPresenceHandle,
) -> bool {
    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;

    let (needs_enter, needs_exit_anim) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        if needs_enter && open_visual {
            c.needs_enter = false;
        }
        if needs_exit_anim && exiting {
            c.needs_exit_anim = false;
        }
        (needs_enter, needs_exit_anim)
    };

    if exiting && (needs_exit_anim || !fade_has_exit_anim(panel)) {
        seed_fade_exit(panel);
        return true;
    }
    if exiting {
        if animation_run_active(&panel.style, &panel.animation_snapshot) {
            if panel.style.opacity < 0.08 {
                panel.set_attr(
                    crate::core::tree::ATTR_POINTER_EVENTS,
                    crate::core::tree::PointerEvents::None.as_str(),
                );
            }
            return true;
        }
        panel.style.animations.clear();
        finish_floating_exit(presence);
        return false;
    }
    if open_visual && needs_enter {
        seed_fade_enter(panel);
        return true;
    }
    if open_visual {
        let animating = animation_run_active(&panel.style, &panel.animation_snapshot);
        if !animating {
            panel.style.opacity = 1.0;
            panel.style.animations.clear();
            presence.cell.borrow_mut().enter_active = false;
        }
        return animating;
    }
    false
}

fn apply_expand_motion(
    panel: &mut Element,
    phase: PresencePhase,
    presence: &FloatingPresenceHandle,
    max_height_cap: Option<f32>,
) -> bool {
    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;

    if panel.style.overflow.y != Overflow::Hidden {
        panel.style.overflow.y = Overflow::Hidden;
    }
    if floating_expand_from_w(panel).is_some() && panel.style.overflow.x != Overflow::Hidden {
        panel.style.overflow.x = Overflow::Hidden;
    }

    let from_h = floating_expand_from_h(panel);
    let from_w = floating_expand_from_w(panel);

    // Default height path: shared collapse seed + presence. Offset / width clip stay here.
    if from_h.is_none() {
        let measuring = presence.cell.borrow().measuring;
        if open_visual && presence.needs_enter() && !measuring {
            presence.cell.borrow_mut().measuring = true;
            presence.cell.borrow_mut().enter_active = true;
            presence.cell.borrow_mut().clip_width = None;
            panel.layout_bounds[2] = 0.0;
            panel.layout_bounds[3] = 0.0;
            prepare_expand_measure(panel, max_height_cap);
            apply_expand_width_clip(panel, presence, false);
            return true;
        }
        let effect = apply_collapse_height(
            panel,
            presence,
            EXPAND_KEYS,
            CollapseHeightOpts {
                pin_exit_height: true,
                keep_open_max_height: true,
            },
        );
        if from_w.is_some() {
            apply_expand_width_clip(panel, presence, exiting);
        }
        match effect {
            CollapseHeightEffect::SeededEnter => {
                presence.cell.borrow_mut().measuring = false;
                let open_h = expand_open_content_h(panel, max_height_cap);
                if open_h > 1.0 {
                    panel.style.animations = vec![keyframes_anim_entry(
                        ensure_expand_down_to_keyframe(open_h),
                        EXPAND_KEYS.duration_ms,
                        TimingFunction::EaseOut,
                        AnimationFillMode::Both,
                    )];
                    reset_anim_snapshot(panel);
                }
                // fill-mode backwards: first painted frame is max-height≈0, not auto flash.
                let _ = panel.tick_css_animations(0.0);
                panel.attrs.remove(ATTR_FLOATING_EXPAND_MEASURE);
                remember_clip_height(presence, panel);
                if from_w.is_some() {
                    apply_expand_width_clip(panel, presence, false);
                }
                absorb_expand_intrinsic(panel, max_height_cap);
                pin_expand_overflow_contents(panel, max_height_cap);
                presence.cell.borrow_mut().hold_enter_css_dt = true;
                return true;
            }
            CollapseHeightEffect::SeededExit | CollapseHeightEffect::Animating => {
                remember_clip_height(presence, panel);
                pin_expand_overflow_contents(panel, max_height_cap);
                if exiting {
                    let _ = hide_expand_exit_sliver(panel, presence, false);
                    return true;
                }
                absorb_expand_intrinsic(panel, max_height_cap);
                return true;
            }
            CollapseHeightEffect::FinishedExit => {
                hide_expand_exit_sliver(panel, presence, true);
                return true;
            }
            CollapseHeightEffect::Measuring => {
                prepare_expand_measure(panel, max_height_cap);
                return true;
            }
            CollapseHeightEffect::Idle => {
                let before_h = crate::layout::dim_length_px(panel.style.max_size.height);
                let was_pinned = expand_contents_are_pinned(panel);
                apply_idle_expand_max_h(panel, presence);
                restore_expand_overflow_contents(panel);
                reset_anim_snapshot(panel);
                remember_clip_height(presence, panel);
                absorb_expand_intrinsic(panel, max_height_cap);
                let after_h = crate::layout::dim_length_px(panel.style.max_size.height);
                let unpinned = was_pinned && !expand_contents_are_pinned(panel);
                if unpinned || before_h != after_h {
                    return true;
                }
            }
            CollapseHeightEffect::Hidden => {}
        }
        if !open_visual {
            return false;
        }
    } else {
        let (needs_enter, needs_exit_anim, enter_active) = {
            let mut c = presence.cell.borrow_mut();
            let needs_enter = c.needs_enter;
            let needs_exit_anim = c.needs_exit_anim;
            let enter_active = c.enter_active;
            if needs_exit_anim && exiting {
                c.needs_exit_anim = false;
            }
            (needs_enter, needs_exit_anim, enter_active)
        };

        let exit_armed = presence.cell.borrow().exit_armed;
        if exiting && (needs_exit_anim || (!expand_has_exit_anim(panel) && exit_armed)) {
            let h = panel_content_height(panel, max_height_cap);
            seed_expand_exit(panel, h);
            remember_clip_height(presence, panel);
            apply_expand_width_clip(panel, presence, true);
            return true;
        }
        if exiting {
            if animation_run_active(&panel.style, &panel.animation_snapshot) {
                remember_clip_height(presence, panel);
                apply_expand_width_clip(panel, presence, true);
                let _ = hide_expand_exit_sliver(panel, presence, false);
                return true;
            }
            hide_expand_exit_sliver(panel, presence, true);
            return true;
        }

        if open_visual && (needs_enter || enter_active) {
            let has_enter = panel
                .style
                .animations
                .iter()
                .any(|a| is_expand_down_anim(&a.name));
            if has_enter {
                if animation_run_active(&panel.style, &panel.animation_snapshot) {
                    remember_clip_height(presence, panel);
                    apply_expand_width_clip(panel, presence, false);
                    return true;
                }
                panel.style.animations.clear();
                apply_idle_expand_max_h(panel, presence);
                reset_anim_snapshot(panel);
                {
                    let mut c = presence.cell.borrow_mut();
                    c.enter_active = false;
                    c.needs_enter = false;
                    c.clip_height = None;
                    c.clip_width = None;
                    c.measuring = false;
                }
                return false;
            }

            let measuring = presence.cell.borrow().measuring;
            if needs_enter && !measuring {
                {
                    let mut c = presence.cell.borrow_mut();
                    c.measuring = true;
                    c.enter_active = true;
                    c.clip_width = None;
                }
                panel.layout_bounds[2] = 0.0;
                panel.layout_bounds[3] = 0.0;
                prepare_expand_measure(panel, max_height_cap);
                apply_expand_width_clip(panel, presence, false);
                return true;
            }

            let measured = take_measured_height(panel, max_height_cap);
            if measured < 1.0 {
                prepare_expand_measure(panel, max_height_cap);
                apply_expand_width_clip(panel, presence, false);
                let mut c = presence.cell.borrow_mut();
                c.enter_active = true;
                c.measuring = true;
                return true;
            }
            {
                let mut c = presence.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = true;
                c.measuring = false;
            }
            seed_expand_enter(panel, measured);
            remember_clip_height(presence, panel);
            apply_expand_width_clip(panel, presence, false);
            presence.cell.borrow_mut().hold_enter_css_dt = true;
            return true;
        }
    }

    if open_visual {
        absorb_expand_intrinsic(panel, max_height_cap);
    }
    false
}

fn apply_floating_motion_inner(
    panel: &mut Element,
    shell: &FloatingShellState,
    presence: &FloatingPresenceHandle,
) -> bool {
    let phase = presence.phase();
    match shell.kind {
        FloatingMotionKind::Fade => apply_fade_motion(panel, phase, presence),
        FloatingMotionKind::Expand => {
            apply_expand_motion(panel, phase, presence, shell.max_height_cap)
        }
    }
}

/// Build-time register: write `template_id` + shell; Fade enter pre-writes opacity=0.
pub(crate) fn apply_floating_panel_motion(
    overlay_id: &str,
    panel: &mut Element,
    kind: FloatingMotionKind,
    phase: PresencePhase,
    presence: &FloatingPresenceHandle,
    open: Signal<bool>,
    overlay_kind: OverlayKind,
    overlay_mode: OverlayMode,
    max_height_cap: Option<f32>,
) -> bool {
    register_floating_presence_cell(&presence.cell);
    let tid = panel_tid(overlay_id);
    panel.template_id = Some(tid);
    if matches!(phase, PresencePhase::Exiting) {
        panel.set_attr(ATTR_FLOATING_EXITING, "1");
    } else {
        panel.attrs.remove(ATTR_FLOATING_EXITING);
    }
    let needs_frame = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        c.logical_open = Some(open);
        c.shell = Some(FloatingShellState {
            overlay_id: overlay_id.to_string(),
            kind,
            phase,
            overlay_kind,
            overlay_mode,
            open,
            max_height_cap,
        });
        if needs_enter
            && matches!(phase, PresencePhase::Present)
            && kind == FloatingMotionKind::Fade
        {
            panel.style.opacity = 0.0;
        }
        let measuring = c.measuring;
        if kind == FloatingMotionKind::Expand
            && !measuring
            && panel.used_layout.anim_height() >= 1.0
            && panel
                .attrs
                .get(ATTR_FLOATING_CONTENT_H)
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|h| *h > 0.5)
                .is_none()
        {
            // Do not overwrite a measured list height with the idle place-cap box
            // (ScrollArea `h:100%` under max-h-16rem).
            panel.set_attr(
                ATTR_FLOATING_CONTENT_H,
                format!("{}", panel.used_layout.anim_height()),
            );
        }
        if kind == FloatingMotionKind::Expand
            && matches!(phase, PresencePhase::Present)
            && (needs_enter || measuring)
        {
            panel.style.overflow.y = Overflow::Hidden;
            panel.set_attr(ATTR_FLOATING_EXPAND_MEASURE, "1");
            panel.style.opacity = 0.0;
            if floating_expand_from_w(panel).is_some() {
                panel.style.overflow.x = Overflow::Hidden;
                if expand_clip_x(panel) {
                    panel.style.max_size.width = Dimension::auto();
                } else if let Some(from_w) = floating_expand_from_w(panel) {
                    panel.style.max_size.width = Dimension::length(from_w);
                }
            }
            if let Some(cap) = max_height_cap.filter(|h| *h > 0.0) {
                panel.style.max_size.height = Dimension::length(cap);
            } else {
                panel.style.max_size.height = Dimension::auto();
            }
        } else if kind == FloatingMotionKind::Expand
            && !needs_enter
            && !measuring
            && matches!(phase, PresencePhase::Present | PresencePhase::Exiting)
        {
            panel.style.overflow.y = Overflow::Hidden;
            if floating_expand_from_w(panel).is_some() {
                panel.style.overflow.x = Overflow::Hidden;
            }
            if let Some(h) = c.clip_height.filter(|h| *h > 1.0) {
                // SeededEnter remembers max-height≈0. Writing that back on every
                // island rebuild zeroes the accordion before layout — the menu
                // stays 0-tall on PartialLoad, then snaps to the place cap (no
                // animation; flip-above jumps to a leftover Y). CSS owns clip
                // while enter/exit keyframes are running.
                panel.style.max_size.height = Dimension::length(h);
                if c.enter_active || matches!(phase, PresencePhase::Exiting) {
                    panel.style.opacity = 1.0;
                }
            }
            if let Some(w) = c.clip_width.filter(|w| *w > 0.0) {
                panel.style.max_size.width = Dimension::length(w);
                if expand_clip_x(panel) {
                    panel.style.min_size.width = Dimension::length(0.0);
                    panel.style.flex_wrap = FlexWrap::NoWrap;
                }
            }
        }
        matches!(phase, PresencePhase::Present | PresencePhase::Exiting) || needs_enter
    };
    needs_frame
}

/// Sync presence + portal; returns `None` when not mounted.
pub(crate) fn floating_portal_phase(
    overlay_id: &str,
    open: Signal<bool>,
    presence: &FloatingPresenceHandle,
    overlay_kind: OverlayKind,
    overlay_mode: OverlayMode,
    scroll_freeze: crate::app::portal_overlay::ScrollFreeze,
) -> Option<(PresencePhase, crate::app::portal_overlay::OverlayZ)> {
    use crate::app::portal_overlay::portal_sync_presence_lock;
    // Closed overlay islands never assemble a shell. Bind `open` anyway so an
    // idle presence tick cannot prune the cell before the first trigger click.
    presence.cell.borrow_mut().logical_open = Some(open);
    register_floating_presence_cell(&presence.cell);
    let want = open.try_get().unwrap_or(false);
    let phase = sync_floating_presence(presence, want);
    let mounted = floating_mounted_signal(presence);
    if !mounted.try_get().unwrap_or(false) {
        let _ = portal_sync_presence_lock(
            overlay_id,
            open,
            mounted,
            overlay_kind,
            overlay_mode,
            scroll_freeze,
        );
        return None;
    }
    let oz = portal_sync_presence_lock(
        overlay_id,
        open,
        mounted,
        overlay_kind,
        overlay_mode,
        scroll_freeze,
    )?;
    Some((phase, oz))
}

/// After portal hoist: register seed keyframes from presence.
pub(crate) fn flush_floating_motion_subtree(root: &mut Element) -> bool {
    FLOATING_NEEDS_RELAYOUT.with(|c| c.set(false));
    let mut needs_frame = arm_orphaned_floating_exits();
    let mut needs_layout = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let (shell, handle) = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                (
                    shell,
                    crate::utils::panel_presence::PanelPresence {
                        cell: Rc::clone(cell_rc),
                    },
                )
            };
            let tid = panel_tid(&shell.overlay_id);
            let Some(panel) = find_template_id_mut(root, &tid) else {
                needs_frame = true;
                continue;
            };
            if apply_floating_motion_inner(panel, &shell, &handle) {
                needs_frame = true;
                if shell.kind == FloatingMotionKind::Expand {
                    needs_layout = true;
                }
            }
        }
    });
    FLOATING_NEEDS_RELAYOUT.with(|c| c.set(needs_layout));
    needs_frame
}

/// Keep a hoisted portal whose overlay owner is gone but presence is still mounted
/// (exit animation in flight, including TabsContent dispose).
pub(crate) fn hoisted_should_retain(el: &Element) -> bool {
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow().iter().any(|cell| {
            let c = cell.borrow();
            if !c.gate.is_mounted() {
                return false;
            }
            let Some(id) = c.shell.as_ref().map(|s| s.overlay_id.as_str()) else {
                return false;
            };
            subtree_belongs_to_floating(el, id)
        })
    })
}

fn subtree_belongs_to_floating(el: &Element, overlay_id: &str) -> bool {
    let tid = panel_tid(overlay_id);
    let mut stack: Vec<&Element> = vec![el];
    while let Some(n) = stack.pop() {
        if n.template_id.as_deref() == Some(tid.as_str()) {
            return true;
        }
        if n.attrs.get("rui_floating").map(String::as_str) == Some(overlay_id) {
            return true;
        }
        for c in n.children.iter().rev() {
            stack.push(c);
        }
    }
    false
}

fn tick_element_css(el: &mut Element, dt: f32) -> bool {
    if !el.style.animations.is_empty()
        || crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot)
    {
        el.tick_css_animations(dt)
    } else {
        false
    }
}

/// Advance CSS on hoisted floating panels（post-layout stepper）。
pub(crate) fn tick_hoisted_floating_css(root: &mut Element) -> bool {
    let dt = crate::style::utility::frame_dt();
    let mut any = false;
    let mut needs_layout = FLOATING_NEEDS_RELAYOUT.with(|c| c.get());
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let (shell, handle) = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                (
                    shell,
                    crate::utils::panel_presence::PanelPresence {
                        cell: Rc::clone(cell_rc),
                    },
                )
            };
            let tid = panel_tid(&shell.overlay_id);
            let Some(panel) = find_template_id_mut(root, &tid) else {
                any = true;
                continue;
            };
            if tick_element_css(panel, {
                let hold = {
                    let mut c = handle.cell.borrow_mut();
                    let hold = c.hold_enter_css_dt;
                    c.hold_enter_css_dt = false;
                    hold
                };
                if hold {
                    0.0
                } else {
                    dt
                }
            }) {
                any = true;
                if shell.kind == FloatingMotionKind::Expand {
                    // Accordion max-height / width-clip are Taffy geometry.
                    needs_layout = true;
                }
                // Fade/expand ticks used to skip named Overlay dirty → Load stacked
                // translucent fill + box-shadow into retain (calendar smear).
                // Expand measure is opacity-0 unconstrained layout — it is not a
                // paint stamp (see `hoisted_portal_paint_box`).
                if panel
                    .attrs
                    .get(crate::utils::popup::ATTR_FLOATING_EXPAND_MEASURE)
                    .map(String::as_str)
                    != Some("1")
                {
                    crate::app::damage::push_frame_paint_element(panel, [0.0, 0.0]);
                }
            }
            if shell.kind == FloatingMotionKind::Expand {
                apply_expand_width_clip(
                    panel,
                    &handle,
                    matches!(shell.phase, PresencePhase::Exiting),
                );
                pin_expand_overflow_contents(panel, shell.max_height_cap);
            }
            if clamp_animated_max_h_to_place(panel, &shell.overlay_id) {
                any = true;
                needs_layout = true;
            }
            if shell.kind == FloatingMotionKind::Expand
                && matches!(shell.phase, PresencePhase::Exiting)
                && hide_expand_exit_sliver(panel, &handle, false)
            {
                any = true;
                needs_layout = true;
            }
        }
    });
    FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(any));
    FLOATING_NEEDS_RELAYOUT.with(|c| c.set(needs_layout));
    any
}

/// After Expand max-height changes, mark the panel dirty for a same-frame second layout.
///
/// Fade panels are paint-only; dirtying them (and every other mounted overlay) on a
/// sibling opacity tick used to recompute the whole gallery every Popover frame.
pub(crate) fn mark_floating_panels_layout_dirty(
    root: &Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
) {
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let tid = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.as_ref() else {
                    continue;
                };
                if shell.kind != FloatingMotionKind::Expand {
                    continue;
                }
                panel_tid(&shell.overlay_id)
            };
            if let Some(panel) = find_template_id(root, &tid) {
                layout_tree.mark_element_layout_dirty(panel);
            }
        }
    });
}

fn find_template_id<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
    crate::core::tree::find_by_template_id(el, want)
}

#[cfg(test)]
pub(crate) fn debug_floating_registry(root: &Element) -> String {
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow()
            .iter()
            .map(|cell| {
                let c = cell.borrow();
                let id = c
                    .shell
                    .as_ref()
                    .map(|s| s.overlay_id.clone())
                    .unwrap_or_else(|| "<no-shell>".into());
                let tid = if id == "<no-shell>" {
                    String::new()
                } else {
                    panel_tid(&id)
                };
                let found = !tid.is_empty() && find_template_id(root, &tid).is_some();
                format!(
                    "id={id} mounted={} phase={:?} needs_enter={} measuring={} enter_active={} found={found} kind={:?}",
                    c.gate.is_mounted(),
                    c.gate.phase(),
                    c.needs_enter,
                    c.measuring,
                    c.enter_active,
                    c.shell.as_ref().map(|s| s.kind),
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

/// Skip pre-hoist CSS tick for live floating panels (parity with modal_shell_defer_css_tick).
pub(crate) fn floating_panel_defer_css_tick(tid: Option<&str>) -> bool {
    let Some(t) = tid else {
        return false;
    };
    let Some(overlay_id) = t.strip_suffix("__floating_panel") else {
        return false;
    };
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow().iter().any(|cell| {
            let c = cell.borrow();
            c.gate.is_mounted() && c.shell.as_ref().is_some_and(|s| s.overlay_id == overlay_id)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn nested_child_owner_reregisters_after_parent_effect_rerun() {
        use crate::core::reactivity::{create_effect, with_child_owner};
        create_root(|| {
            let tick = Signal::new(0u32);
            create_effect(move || {
                let _ = tick.get();
                with_child_owner(|| {
                    let h = use_floating_presence("rebind-fmt", FloatingMotionKind::Expand);
                    sync_floating_presence(&h, true);
                });
            });
            assert!(
                any_floating_motion_needs_frame(),
                "first child-owner mount must register Expand presence"
            );
            tick.set(1);
            assert!(
                any_floating_motion_needs_frame(),
                "parent overlay rebuild disposes the Select owner; named use_signal must rebind the flush registry"
            );
        });
    }

    #[test]
    fn idle_presence_tick_must_not_drop_closed_overlay_before_first_open() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let open = Signal::new(false);
            let handle = use_floating_presence("idle-first-open", FloatingMotionKind::Fade);
            let _ = floating_portal_phase(
                "idle-first-open",
                open,
                &handle,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                crate::app::portal_overlay::ScrollFreeze::None,
            );
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();

            open.set(true);
            let phase = sync_floating_presence(&handle, true);
            assert!(
                apply_floating_panel_motion(
                    "idle-first-open",
                    &mut panel,
                    FloatingMotionKind::Fade,
                    phase,
                    &handle,
                    open,
                    OverlayKind::Popover,
                    OverlayMode::Stacked,
                    None,
                ),
                "first open after idle ticks must still register Fade enter"
            );
            root.children.push(panel);
            assert!(
                flush_floating_motion_subtree(&mut root),
                "flush must see the overlay after idle prune"
            );
            let panel = find_template_id_mut(&mut root, "idle-first-open__floating_panel").unwrap();
            assert!(
                panel.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "first open after idle ticks must seed fade-in-0, not stay opacity 0"
            );
        });
    }

    #[test]
    fn fade_enter_seeds_once() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let open = Signal::new(true);
            let handle = use_floating_presence("fade-enter", FloatingMotionKind::Fade);
            sync_floating_presence(&handle, true);
            assert!(apply_floating_panel_motion(
                "fade-enter",
                &mut panel,
                FloatingMotionKind::Fade,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            ));
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "fade-enter__floating_panel").unwrap();
            assert!(
                panel.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "must seed fade-in-0"
            );
            assert!(panel.style.opacity < 0.05);
            let elapsed_before = panel
                .animation_snapshot
                .runs
                .first()
                .map(|r| r.elapsed_ms)
                .unwrap_or(0.0);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "fade-enter__floating_panel").unwrap();
            let elapsed_after = panel
                .animation_snapshot
                .runs
                .first()
                .map(|r| r.elapsed_ms)
                .unwrap_or(0.0);
            assert!(
                (elapsed_after - elapsed_before).abs() < 1.0,
                "second flush must not restart fade-in"
            );
        });
    }

    #[test]
    fn expand_open_stays_busy_across_queries_unlike_one_shot_wake() {
        create_root(|| {
            FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
            let fade = use_floating_presence("fade-parent", FloatingMotionKind::Fade);
            sync_floating_presence(&fade, true);
            assert!(
                any_floating_motion_needs_frame(),
                "parent Fade open must be visible to about_to_wait before flush"
            );
            // One-shot take() would return false here and starve the nested Expand.
            assert!(
                any_floating_motion_needs_frame(),
                "must be a live query, not a consumed latch"
            );
            let nested = use_floating_presence("nested-fmt", FloatingMotionKind::Expand);
            sync_floating_presence(&nested, true);
            assert!(
                any_floating_motion_needs_frame(),
                "nested Expand open must still pump after the parent Fade already entered"
            );
        });
    }

    #[test]
    fn fade_css_tick_does_not_request_second_layout() {
        create_root(|| {
            FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
            let mut root = Element::new();
            let mut panel = Element::new();
            let open = Signal::new(true);
            let handle = use_floating_presence("fade-norel", FloatingMotionKind::Fade);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "fade-norel",
                &mut panel,
                FloatingMotionKind::Fade,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(
                flush_floating_motion_subtree(&mut root),
                "Fade enter still needs paint frames"
            );
            assert!(
                !floating_motion_needs_relayout(),
                "Fade seed must not dirty Taffy / second collect"
            );
            crate::style::utility::set_frame_dt(0.016);
            assert!(tick_hoisted_floating_css(&mut root));
            crate::style::utility::set_frame_dt(0.0);
            assert!(
                !floating_motion_needs_relayout(),
                "Fade opacity tick is paint-only"
            );
        });
    }

    #[test]
    fn expand_flush_requests_second_layout_for_measure() {
        create_root(|| {
            FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
            let mut root = Element::new();
            let mut panel = Element::new();
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-relayout", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-relayout",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            assert!(
                floating_motion_needs_relayout(),
                "Expand measure / max-height seed must same-frame relayout"
            );
        });
    }

    #[test]
    fn fade_exit_seeds_and_defers_unmount() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let open = Signal::new(true);
            let handle = use_floating_presence("fade-exit", FloatingMotionKind::Fade);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "fade-exit",
                &mut panel,
                FloatingMotionKind::Fade,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            // Settle enter.
            for _ in 0..40 {
                let p = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
                let _ = p.tick_css_animations(0.016);
            }
            let _ = flush_floating_motion_subtree(&mut root);

            open.set(false);
            sync_floating_presence(&handle, false);
            assert_eq!(handle.cell.borrow().gate.phase(), PresencePhase::Exiting);
            assert!(floating_mounted_signal(&handle).peek());
            // Re-apply shell with exiting phase.
            let panel = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
            apply_floating_panel_motion(
                "fade-exit",
                panel,
                FloatingMotionKind::Fade,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
            assert!(panel
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"));
        });
    }

    #[test]
    fn expand_waits_for_measured_height() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-wait", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-wait",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-wait__floating_panel").unwrap();
            assert!(
                panel.style.animations.is_empty(),
                "must not seed before measured height"
            );
            panel.layout_bounds[3] = 120.0;
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-wait__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| is_expand_down_anim(&a.name)),
                "must seed expand-down after measure"
            );
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(999.0);
            assert!(
                mh < 2.0,
                "seed+tick(0) must start at max-height≈0, got {mh}"
            );
            assert!(
                (panel.style.opacity - 1.0).abs() < 0.05,
                "enter seed must restore opacity"
            );
        });
    }

    #[test]
    fn expand_enter_same_frame_css_tick_must_stay_at_zero() {
        create_root(|| {
            crate::style::utility::set_frame_dt(0.016);
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-seed-dt", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-seed-dt",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-seed-dt__floating_panel").unwrap();
            panel.layout_bounds[3] = 192.0;
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
            assert!(flush_floating_motion_subtree(&mut root));
            let _ = tick_hoisted_floating_css(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-seed-dt__floating_panel").unwrap();
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(999.0);
            assert!(
                mh < 2.0,
                "same-present CSS tick must not EaseOut the seed frame off 0, got {mh}"
            );
            crate::style::utility::set_frame_dt(0.0);
        });
    }

    #[test]
    fn expand_enter_rebuild_must_not_zero_remembered_clip() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-clip0", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-clip0",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
            panel.layout_bounds[3] = 120.0;
            let bounds = panel.layout_bounds;
            panel.used_layout.stamp_border_box(bounds, false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
                for _ in 0..4 {
                    let _ = panel.tick_css_animations(0.016);
                }
            }
            let mh = {
                let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
                crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0)
            };
            assert!(mh > 16.0, "accordion-down must have grown, got {mh}");
            {
                let mut c = handle.cell.borrow_mut();
                c.clip_height = Some(0.0);
                c.needs_enter = false;
                c.measuring = false;
                c.enter_active = true;
            }
            let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
            apply_floating_panel_motion(
                "exp-clip0",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
            let mh2 = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                mh2 > 16.0,
                "island rebuild must not write remembered 0 over accordion clip, got {mh2}"
            );
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| is_expand_down_anim(&a.name)),
                "enter keyframes must survive rebuild"
            );
        });
    }

    #[test]
    fn expand_with_from_h_starts_at_baseline_not_zero() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-from", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-from",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from__floating_panel").unwrap();
            assert!(
                panel.style.animations.is_empty(),
                "must not seed before measured height"
            );
            panel.layout_bounds[3] = 120.0;
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-expand-down-")),
                "offset expand must use rui-expand-down-* keyframes"
            );
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                mh > 20.0 && mh < 40.0,
                "seed+tick(0) must start near collapsed bar height, got {mh}"
            );
        });
    }

    #[test]
    fn expand_from_h_exit_collapses_to_chip_not_zero() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-from-exit", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-from-exit",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
            panel.layout_bounds[3] = 120.0;
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
                for _ in 0..20 {
                    let _ = panel.tick_css_animations(0.016);
                }
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                handle.cell.borrow_mut().enter_active = false;
                handle.cell.borrow_mut().needs_enter = false;
                handle.cell.borrow_mut().measuring = false;
                panel.style.max_size.height = Dimension::length(120.0);
                panel
                    .used_layout
                    .stamp_border_box([0.0, 0.0, 100.0, 120.0], false);
            }

            open.set(false);
            sync_floating_presence(&handle, false);
            let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
            apply_floating_panel_motion(
                "exp-from-exit",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-expand-up-")),
                "from_h exit must clip to the covered trigger, not accordion-up to 0; anims={:?}",
                panel
                    .style
                    .animations
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
            );
            assert!(
                !panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == EXPAND_KEYS.up),
                "cover expand must not accordion-up through the pill"
            );
            for _ in 0..8 {
                let _ = panel.tick_css_animations(0.016);
            }
            let mid = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                mid + 0.5 >= 28.0,
                "visible exit must not shrink below the reaction/chip height, got {mid}"
            );
            assert!(mid < 120.0, "exit must actually be collapsing, got {mid}");
            for _ in 0..16 {
                let _ = panel.tick_css_animations(0.016);
            }
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                (mh - 28.0).abs() < 3.0,
                "from_h exit must land on the covered trigger height, got {mh}"
            );
        });
    }

    #[test]
    fn expand_with_from_w_clips_width_on_enter() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "32");
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, "80");
            panel.set_attr(ATTR_FLOATING_EXPAND_CLIP_X, "1");
            panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-from-w", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-from-w",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from-w__floating_panel").unwrap();
            panel.layout_bounds[2] = 240.0;
            panel.layout_bounds[3] = 48.0;
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-from-w__floating_panel").unwrap();
            let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
            assert!(
                (mw - 80.0).abs() < 2.0,
                "seed+tick(0) must start near collapsed row width, got {mw}"
            );
            assert_eq!(
                panel.style.overflow.x,
                Overflow::Hidden,
                "row expand must clip overflow-x"
            );
        });
    }

    #[test]
    fn expand_from_w_without_clip_x_pins_width_for_wrap() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
            panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, "90");
            panel.style.flex_wrap = FlexWrap::Wrap;
            panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-pin-w", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-pin-w",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-pin-w__floating_panel").unwrap();
            let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
            assert!(
                (mw - 90.0).abs() < 2.0,
                "wrap expand must pin max-width to collapsed bar during measure, got {mw}"
            );
            assert_eq!(
                panel.style.flex_wrap,
                FlexWrap::Wrap,
                "wrap expand must not force nowrap"
            );
            panel.layout_bounds[2] = 90.0;
            panel.layout_bounds[3] = 56.0;
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-pin-w__floating_panel").unwrap();
            let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
            assert!(
                (mw - 90.0).abs() < 2.0,
                "wrap expand must stay pinned after seed, got {mw}"
            );
            assert_eq!(panel.style.flex_wrap, FlexWrap::Wrap);
        });
    }

    #[test]
    fn expand_does_not_seed_from_viewport_capped_first_layout() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.style.max_size.height = Dimension::length(80.0);
            panel.layout_bounds = [0.0, 0.0, 100.0, 80.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-cap", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-cap",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-cap__floating_panel").unwrap();
            assert!(
                panel.style.animations.is_empty(),
                "must not seed accordion from the overflow-capped first layout"
            );
            assert!(
                panel
                    .attrs
                    .get(ATTR_FLOATING_EXPAND_MEASURE)
                    .map(|s| s.as_str())
                    == Some("1"),
                "measure attr must stay until unconstrained height is known"
            );
            panel.layout_bounds[3] = 200.0;
            panel.style.max_size.height = Dimension::auto();
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-cap__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| is_expand_down_anim(&a.name)),
                "must seed after unconstrained measure"
            );
            let content = panel
                .attrs
                .get(ATTR_FLOATING_CONTENT_H)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            assert!(
                (content - 200.0).abs() < 0.5,
                "content height must be unconstrained 200, got {content}"
            );
        });
    }

    #[test]
    fn expand_idle_does_not_shrink_measured_height_to_clip() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-idle", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-idle",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-idle__floating_panel").unwrap();
            panel.layout_bounds[3] = 200.0;
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel = find_template_id_mut(&mut root, "exp-idle__floating_panel").unwrap();
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                handle.cell.borrow_mut().enter_active = false;
                handle.cell.borrow_mut().needs_enter = false;
                handle.cell.borrow_mut().measuring = false;
                handle.cell.borrow_mut().clip_height = None;
                panel.layout_bounds[3] = 80.0;
            }
            let _ = flush_floating_motion_subtree(&mut root);
            let stored = find_template_id_mut(&mut root, "exp-idle__floating_panel")
                .unwrap()
                .used_layout
                .anim_height();
            assert!(
                (stored - 200.0).abs() < 0.5,
                "idle clip 80 must not overwrite content measure 200, got {stored}"
            );
        });
    }

    #[test]
    fn expand_idle_keeps_max_height_and_relayouts_unpin() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let mut child = Element::new();
            child.style.flex_shrink = 1.0;
            child.style.min_size.height = Dimension::length(0.0);
            panel.children.push(child);
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-idle-keep", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-idle-keep",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
            panel.layout_bounds[3] = 192.0;
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
                assert!(
                    panel.children[0].style.flex_shrink < 0.5,
                    "enter must pin overflow contents"
                );
                for _ in 0..40 {
                    let _ = panel.tick_css_animations(0.016);
                }
            }
            let idle = flush_floating_motion_subtree(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
            assert!(
                crate::layout::dim_length_px(panel.style.max_size.height).is_some(),
                "Expand Idle must not snap max-height to auto"
            );
            assert!(
                (panel.children[0].style.flex_shrink - 1.0).abs() < 0.01,
                "Idle must unpin ScrollArea"
            );
            assert!(
                idle,
                "first Idle after unpin must request a same-frame layout"
            );
            let mh_before_tick = crate::layout::dim_length_px(panel.style.max_size.height);
            let _ = tick_hoisted_floating_css(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
            let mh_after_tick = crate::layout::dim_length_px(panel.style.max_size.height);
            assert_eq!(
                mh_before_tick, mh_after_tick,
                "tick after Idle must not restore seed max-height auto"
            );
        });
    }

    #[test]
    fn expand_enter_last_frame_stays_at_capped_height() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let mut child = Element::new();
            child.style.flex_shrink = 1.0;
            child.style.min_size.height = Dimension::length(0.0);
            panel.children.push(child);
            panel.style.padding.top = taffy::style::LengthPercentage::length(4.0);
            panel.style.padding.bottom = taffy::style::LengthPercentage::length(4.0);
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-cap-end", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-cap-end",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                Some(256.0),
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
            panel.layout_bounds[3] = 2000.0;
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 2000.0], false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
                let pinned = crate::layout::dim_length_px(panel.children[0].style.min_size.height)
                    .unwrap_or(0.0);
                assert!(
                    (pinned - 248.0).abs() < 1.0,
                    "overflow pin must be cap minus chrome padding, got {pinned}"
                );
                for _ in 0..40 {
                    let _ = panel.tick_css_animations(0.016);
                }
            }
            let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
            let last_anim =
                crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                (last_anim - 256.0).abs() < 1.5,
                "finishing tick must land on the painted cap, not unconstrained intrinsic, got {last_anim}"
            );
            let _ = flush_floating_motion_subtree(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
            let idle = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                (idle - last_anim).abs() < 1.0,
                "Idle must not dip below the last expand frame, last={last_anim} idle={idle}"
            );
            assert!(
                crate::layout::dim_length_px(panel.style.size.height).is_none(),
                "must not force a definite used height as a last-frame fallback"
            );
        });
    }

    #[test]
    fn expand_exit_reseed_continues_from_mid_height() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-exit-mid", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-exit-mid",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            // Pretend mid-exit after a rebuild wiped animations but kept mid max-height.
            let panel = find_template_id_mut(&mut root, "exp-exit-mid__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            panel.style.max_size.height = Dimension::length(48.0);
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 120.0], false);

            open.set(false);
            sync_floating_presence(&handle, false);
            apply_floating_panel_motion(
                "exp-exit-mid",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-exit-mid__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == EXPAND_KEYS.up),
                "menu expand exit must accordion-up; anims={:?}",
                panel
                    .style
                    .animations
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
            );
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                mh <= 50.0,
                "reseed must not jump back to full 120px, got {mh}"
            );
        });
    }

    #[test]
    fn expand_exit_hides_sliver_on_same_frame_as_finish() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-exit-hide", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-exit-hide",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
            panel.layout_bounds[3] = 120.0;
            let bounds = panel.layout_bounds;
            panel.used_layout.stamp_border_box(bounds, false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                handle.cell.borrow_mut().enter_active = false;
                handle.cell.borrow_mut().needs_enter = false;
                handle.cell.borrow_mut().measuring = false;
                panel.style.max_size.height = Dimension::length(120.0);
                panel.style.opacity = 1.0;
            }

            open.set(false);
            sync_floating_presence(&handle, false);
            let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
            apply_floating_panel_motion(
                "exp-exit-hide",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
                assert!(
                    panel
                        .style
                        .animations
                        .iter()
                        .any(|a| a.name == EXPAND_KEYS.up),
                    "menu exit must accordion-up"
                );
                for _ in 0..20 {
                    let _ = panel.tick_css_animations(0.016);
                }
                assert!(
                    !animation_run_active(&panel.style, &panel.animation_snapshot),
                    "accordion-up must finish"
                );
                assert!(
                    panel.style.opacity > 0.5,
                    "finished keyframe must not be the paint hide; got {}",
                    panel.style.opacity
                );
            }
            assert!(
                tick_hoisted_floating_css(&mut root),
                "post-tick hide must request the same-frame collect"
            );
            let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
            assert!(
                panel.style.opacity < 0.01,
                "sliver must be hidden before collect, got {}",
                panel.style.opacity
            );
        });
    }

    #[test]
    fn expand_exit_keeps_opacity_until_accordion_up_finishes() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-exit-full", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-exit-full",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
            panel.layout_bounds[3] = 120.0;
            let bounds = panel.layout_bounds;
            panel.used_layout.stamp_border_box(bounds, false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                handle.cell.borrow_mut().enter_active = false;
                handle.cell.borrow_mut().needs_enter = false;
                handle.cell.borrow_mut().measuring = false;
                panel.style.max_size.height = Dimension::length(120.0);
                panel.style.opacity = 1.0;
            }

            open.set(false);
            sync_floating_presence(&handle, false);
            let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
            apply_floating_panel_motion(
                "exp-exit-full",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
                for _ in 0..4 {
                    let _ = panel.tick_css_animations(0.016);
                }
                let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
                assert!(
                    animation_run_active(&panel.style, &panel.animation_snapshot),
                    "accordion-up must still be running"
                );
                assert!(
                    mh > 16.0 && panel.style.opacity > 0.5,
                    "mid-exit must stay visible so accordion-up can finish; mh={mh} opacity={}",
                    panel.style.opacity
                );
            }
            let _ = tick_hoisted_floating_css(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                panel.style.opacity > 0.5 && mh > 16.0,
                "hide-on-sliver must not snap opacity at ~16px; mh={mh} opacity={}",
                panel.style.opacity
            );
        });
    }

    #[test]
    fn expand_exit_pins_child_min_height_so_list_overflows_instead_of_relayout() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let mut child = Element::new();
            child.style.flex_shrink = 1.0;
            child.style.min_size.height = Dimension::length(0.0);
            child.layout_bounds = [0.0, 0.0, 100.0, 240.0];
            panel.children.push(child);
            panel.layout_bounds = [0.0, 0.0, 100.0, 240.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-exit-clip", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-exit-clip",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
            panel.layout_bounds[3] = 240.0;
            let bounds = panel.layout_bounds;
            panel.used_layout.stamp_border_box(bounds, false);
            assert!(flush_floating_motion_subtree(&mut root));
            {
                let panel =
                    find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                handle.cell.borrow_mut().enter_active = false;
                handle.cell.borrow_mut().needs_enter = false;
                handle.cell.borrow_mut().measuring = false;
                panel.style.max_size.height = Dimension::length(240.0);
                panel.style.opacity = 1.0;
                if let Some(child) = panel.children.first_mut() {
                    child.style.flex_shrink = 1.0;
                    child.style.min_size.height = Dimension::length(0.0);
                }
            }
            open.set(false);
            sync_floating_presence(&handle, false);
            let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
            apply_floating_panel_motion(
                "exp-exit-clip",
                panel,
                FloatingMotionKind::Expand,
                PresencePhase::Exiting,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == EXPAND_KEYS.up),
                "exit must stay accordion-up, not scale"
            );
            for _ in 0..8 {
                let _ = panel.tick_css_animations(0.016);
            }
            let _ = flush_floating_motion_subtree(&mut root);
            let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(mh < 200.0, "accordion-up must shrink the clip, got {mh}");
            let child = panel.children.first().expect("list child");
            assert!(
                child.style.flex_shrink < 0.01,
                "list must not flex-shrink with the clip"
            );
            let child_min =
                crate::layout::dim_length_px(child.style.min_size.height).unwrap_or(0.0);
            assert!(
                child_min > 200.0,
                "open list min-height must survive the clip so overflow hides rows, got {child_min}"
            );
        });
    }

    #[test]
    fn expand_pin_targets_scroll_viewport_rows_not_the_wrapper() {
        create_root(|| {
            let mut root = Element::new();
            let mut panel = Element::new();
            let mut rows = Element::new();
            rows.style.flex_shrink = 1.0;
            rows.style.min_size.height = Dimension::length(0.0);
            let mut viewport = Element::new().focus_id(
                crate::core::scroll_viewport::focus_id_for_scroll_viewport("fmt:menu_scroll"),
            );
            viewport.style.flex_shrink = 1.0;
            viewport.style.min_size.height = Dimension::length(0.0);
            viewport.children.push(rows);
            let mut outer = Element::new();
            outer.style.flex_shrink = 1.0;
            outer.style.min_size.height = Dimension::length(0.0);
            outer.children.push(viewport);
            panel.children.push(outer);
            panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let open = Signal::new(true);
            let handle = use_floating_presence("exp-pin-sa", FloatingMotionKind::Expand);
            sync_floating_presence(&handle, true);
            apply_floating_panel_motion(
                "exp-pin-sa",
                &mut panel,
                FloatingMotionKind::Expand,
                PresencePhase::Present,
                &handle,
                open,
                OverlayKind::Menu,
                OverlayMode::Stacked,
                None,
            );
            root.children.push(panel);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-pin-sa__floating_panel").unwrap();
            panel.layout_bounds[3] = 192.0;
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
            assert!(flush_floating_motion_subtree(&mut root));
            let panel = find_template_id_mut(&mut root, "exp-pin-sa__floating_panel").unwrap();
            let outer = panel.children.first().expect("ScrollArea wrapper");
            let viewport = outer.children.first().expect("scroll viewport");
            let rows = viewport.children.first().expect("rows");
            assert!(
                (outer.style.flex_shrink - 1.0).abs() < 0.01,
                "wrapper must shrink with the accordion so overflow:auto can clip a short list"
            );
            assert!(
                (viewport.style.flex_shrink - 1.0).abs() < 0.01,
                "scroll viewport must shrink with the accordion"
            );
            assert!(
                rows.style.flex_shrink < 0.01,
                "rows must not collapse into a one-line chip"
            );
            let row_min = crate::layout::dim_length_px(rows.style.min_size.height).unwrap_or(0.0);
            assert!(
                row_min > 100.0,
                "pinned rows min-height must be the open list, got {row_min}"
            );
        });
    }
}
