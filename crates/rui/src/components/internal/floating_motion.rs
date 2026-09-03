//! Shared motion for anchored overlays (Popover / Menu / Tooltip / Select): presence + CSS keyframes.
//!
//! - [`FloatingMotionKind::Fade`]: Dialog single-panel `fade-in-0` / `fade-out-0` (300 / 200ms)
//! - [`FloatingMotionKind::Expand`]: Accordion `accordion-down` / `accordion-up` (200ms)
//!
//! Portal stack uses [`crate::app::portal_overlay::portal_sync_presence`]: Esc clears logical `open`,
//! `mounted` stays alive through exit. Do not reuse [`super::modal_motion`] (dual surface + Dialog zoom).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::style::{
    animation_run_active, keyframes::KeyframeStop, keyframes_anim_entry, register_keyframes,
    tick_element_animations, AnimationFillMode, TimingFunction,
};
use crate::utils::motion::clear_motion;
use crate::utils::popup::{
    clamp_animated_max_h_to_place, peek_floating_place_max_h, ATTR_FLOATING_CONTENT_H,
    ATTR_FLOATING_EXPAND_CLIP_X, ATTR_FLOATING_EXPAND_FROM_H, ATTR_FLOATING_EXPAND_FROM_W,
    ATTR_FLOATING_EXPAND_MEASURE,
};
use crate::utils::presence::{ExitGate, PresencePhase};
use taffy::style::{Dimension, FlexWrap, Overflow};

/// Dialog-aligned fade enter.
const FADE_ENTER_MS: f32 = 300.0;
/// Dialog-aligned fade exit.
const FADE_EXIT_MS: f32 = 200.0;
/// Accordion-aligned expand.
const EXPAND_MS: f32 = 200.0;
const EXIT_DURATION: Duration = Duration::from_millis(FADE_EXIT_MS as u64 + 40);
const EXPAND_EXIT_DURATION: Duration = Duration::from_millis(EXPAND_MS as u64 + 40);

const ACCORDION_DOWN: &str = "accordion-down";
const ACCORDION_UP: &str = "accordion-up";
/// Last accordion-up frames keep chrome (padding/border/shadow) as a flat sliver.
/// Hide before collect so the exit does not stall on a 0-height capsule.
const EXPAND_EXIT_HIDE_PX: f32 = 16.0;

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
    let to_w = presence.cell.borrow().measured_width.max(from_w).max(1.0);
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
    name == ACCORDION_DOWN || name.starts_with("rui-expand-down-")
}

fn is_expand_up_anim(name: &str) -> bool {
    name == ACCORDION_UP || name.starts_with("rui-expand-up-")
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

/// Floating panel motion kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloatingMotionKind {
    Fade,
    Expand,
}

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

struct FloatingPresenceCell {
    mounted: Signal<bool>,
    gate: ExitGate,
    needs_enter: bool,
    enter_active: bool,
    exit_armed: bool,
    needs_exit_anim: bool,
    shell: Option<FloatingShellState>,
    measured_height: f32,
    measured_width: f32,
    exit_from_height: Option<f32>,
    /// Last accordion clip height (interpolated max-height). Rebuild/placement must not
    /// flash the viewport cap before flush re-seeds.
    clip_height: Option<f32>,
    /// Last `max-width` clip when [`ATTR_FLOATING_EXPAND_FROM_W`] is set.
    clip_width: Option<f32>,
    /// Expand: waiting for an unconstrained layout before seeding accordion-down.
    measuring: bool,
}

/// Per-instance presence + motion ([`use_floating_presence`]).
#[derive(Clone)]
pub(crate) struct FloatingPresenceHandle {
    cell: Rc<RefCell<FloatingPresenceCell>>,
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
    let cell = use_signal(key.as_str(), || {
        let mounted = use_signal(mounted_key.as_str(), || false);
        let rc = Rc::new(RefCell::new(FloatingPresenceCell {
            mounted,
            gate: ExitGate::new(false, exit_dur),
            needs_enter: false,
            enter_active: false,
            exit_armed: false,
            needs_exit_anim: false,
            shell: None,
            measured_height: 0.0,
            measured_width: 0.0,
            exit_from_height: None,
            clip_height: None,
            clip_width: None,
            measuring: false,
        }));
        PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(Rc::clone(&rc)));
        rc
    });
    FloatingPresenceHandle { cell: cell.peek() }
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
    let mounted_sig = floating_mounted_signal(handle);
    let (phase, mounted, set_mounted) = {
        let mut c = handle.cell.borrow_mut();
        // Peer exclusive dismiss sets mounted=false while gate may still be Exiting.
        if !want_open && !c.mounted.peek() && c.gate.phase() == PresencePhase::Exiting {
            c.gate.force_absent();
            c.exit_armed = false;
            c.needs_exit_anim = false;
            c.enter_active = false;
            c.needs_enter = false;
            c.shell = None;
            c.exit_from_height = None;
            c.clip_height = None;
            c.clip_width = None;
            c.measuring = false;
            return PresencePhase::Absent;
        }
        let was = c.gate.phase();
        let changed = c.gate.set_present(want_open, Instant::now());
        let phase = c.gate.phase();
        let mounted = c.gate.is_mounted();
        if want_open
            && matches!(phase, PresencePhase::Present)
            && (changed || was == PresencePhase::Absent)
        {
            c.needs_enter = true;
            c.enter_active = false;
            c.exit_armed = false;
            c.needs_exit_anim = false;
            c.exit_from_height = None;
            c.clip_height = None;
            c.clip_width = None;
            c.measuring = false;
            c.measured_height = 0.0;
            c.measured_width = 0.0;
        }
        if matches!(phase, PresencePhase::Exiting) && !c.exit_armed {
            c.exit_armed = true;
            c.needs_enter = false;
            c.enter_active = false;
            c.needs_exit_anim = true;
            if c.exit_from_height.is_none() {
                c.exit_from_height = Some(c.measured_height.max(1.0));
            }
        }
        let set_mounted = c.mounted.peek() != mounted;
        (phase, mounted, set_mounted)
    };
    if set_mounted {
        mounted_sig.set(mounted);
    }
    phase
}

/// Event loop: CSS exit ends on flush; wall clock is fallback only (do not unmount on tick while exit_armed).
pub(crate) fn tick_floating_presence() -> bool {
    drain_pending_floating_unmounts();
    let now = Instant::now();
    let mut needs_frame = false;
    let mut unmount: Vec<Signal<bool>> = Vec::new();
    PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            let maybe = {
                let mut c = cell.borrow_mut();
                if c.gate.phase() == PresencePhase::Exiting {
                    if c.exit_armed {
                        // Wall-clock must not tear down mid CSS exit.
                        needs_frame = true;
                        None
                    } else if c.gate.tick(now) {
                        c.exit_armed = false;
                        c.enter_active = false;
                        c.needs_exit_anim = false;
                        c.shell = None;
                        c.exit_from_height = None;
                        Some(c.mounted)
                    } else {
                        needs_frame = true;
                        None
                    }
                } else if c.enter_active && c.gate.phase() == PresencePhase::Present {
                    needs_frame = true;
                    None
                } else if c.gate.phase() == PresencePhase::Present && c.needs_enter {
                    needs_frame = true;
                    None
                } else {
                    None
                }
            };
            if let Some(sig) = maybe {
                unmount.push(sig);
            }
        }
    });
    for sig in unmount {
        sig.set(false);
    }
    needs_frame
}

fn find_template_id_mut<'a>(el: &'a mut Element, want: &str) -> Option<&'a mut Element> {
    let want = want.replace('\\', "/").to_ascii_lowercase();
    if el
        .template_id
        .as_deref()
        .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
    {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(hit) = find_template_id_mut(child, &want) {
            return Some(hit);
        }
    }
    None
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
    clear_motion(el);
    el.style.opacity = 1.0;
    // Baseline = content height; keyframes `from { max-height: 0 }` → interpolate to baseline.
    el.style.max_size.height = Dimension::length(content_height.max(0.0));
    let anim_name = if let Some(from_h) = floating_expand_from_h(el) {
        ensure_expand_down_from_keyframe(from_h.min(content_height))
    } else {
        ACCORDION_DOWN.to_string()
    };
    el.style.animations = vec![keyframes_anim_entry(
        anim_name,
        EXPAND_MS,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
    // dt=0: apply fill-mode backwards only (max-height:0); avoid a large first-frame dt skipping enter.
    let _ = tick_element_animations(&mut el.style, &mut el.animation_snapshot, 0.0);
    el.set_attr(
        ATTR_FLOATING_CONTENT_H,
        format!("{}", content_height.max(1.0)),
    );
    el.attrs.remove(ATTR_FLOATING_EXPAND_MEASURE);
}

fn expand_visual_h(el: &Element) -> f32 {
    crate::layout::dim_length_px(el.style.max_size.height)
        .filter(|h| *h >= 0.0)
        .unwrap_or(el.layout_bounds[3].max(0.0))
}

fn remember_clip_height(presence: &FloatingPresenceHandle, el: &Element) {
    let h = crate::layout::dim_length_px(el.style.max_size.height)
        .filter(|v| *v >= 0.0)
        .unwrap_or(el.layout_bounds[3]);
    presence.cell.borrow_mut().clip_height = Some(h);
}

/// Hide the collapsing panel once the clip is a sliver, or when accordion-up finished.
/// Must run **before** the post-tick layout/collect; opacity 0 after collect still paints.
fn hide_expand_exit_sliver(
    panel: &mut Element,
    presence: &FloatingPresenceHandle,
    finish: bool,
) -> bool {
    let h = expand_visual_h(panel);
    let done = finish || !animation_run_active(&panel.style, &panel.animation_snapshot);
    if !done && h >= EXPAND_EXIT_HIDE_PX {
        return false;
    }
    let mut changed = false;
    if panel.style.opacity > 0.0 {
        panel.style.opacity = 0.0;
        changed = true;
    }
    if done {
        if !panel.style.animations.is_empty() {
            panel.style.animations.clear();
            changed = true;
        }
        if crate::layout::dim_length_px(panel.style.max_size.height).is_none_or(|v| v > 0.5) {
            panel.style.max_size.height = Dimension::length(0.0);
            changed = true;
        }
        finish_floating_exit(presence);
    }
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
    let anim_name = if let Some(to_h) = floating_expand_from_h(el) {
        ensure_expand_up_to_keyframe(to_h.min(from))
    } else {
        ACCORDION_UP.to_string()
    };
    el.style.animations = vec![keyframes_anim_entry(
        anim_name,
        EXPAND_MS,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    reset_anim_snapshot(el);
    let _ = tick_element_animations(&mut el.style, &mut el.animation_snapshot, 0.0);
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

fn take_measured_height(
    presence: &FloatingPresenceHandle,
    panel: &Element,
    max_height_cap: Option<f32>,
) -> f32 {
    let mut c = presence.cell.borrow_mut();
    let w = panel.layout_bounds[2];
    if w > 1.0 && w > c.measured_width {
        c.measured_width = w;
    }
    if c.measured_height >= 1.0 {
        return c.measured_height;
    }
    // Prefer this frame's layout. User max_h is already in bounds — do not clear to auto and lose height forever.
    let h = panel.layout_bounds[3];
    if h > 1.0 {
        c.measured_height = match max_height_cap {
            Some(max) if max > 0.0 => h.min(max),
            _ => h,
        };
        return c.measured_height;
    }
    0.0
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
    let Some(id) = overlay_id else {
        panel.style.max_size.height = Dimension::auto();
        if floating_expand_from_w(panel).is_some() {
            panel.style.max_size.width = Dimension::auto();
        }
        return;
    };
    match peek_floating_place_max_h(&id) {
        Some(h) if h > 0.0 => {
            panel.style.max_size.height = Dimension::length(h);
        }
        _ => {
            panel.style.max_size.height = Dimension::auto();
        }
    }
    if floating_expand_from_w(panel).is_some() {
        panel.style.max_size.width = Dimension::auto();
        presence.cell.borrow_mut().clip_width = None;
    }
}

fn panel_content_height(presence: &FloatingPresenceHandle, el: &Element, cap: Option<f32>) -> f32 {
    let c = presence.cell.borrow();
    let h = c
        .exit_from_height
        .filter(|h| *h > 1.0)
        .or_else(|| (c.measured_height > 1.0).then_some(c.measured_height))
        .unwrap_or_else(|| el.layout_bounds[3].max(0.0));
    match cap {
        Some(max) if max > 0.0 => h.min(max),
        _ => h,
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
        c.exit_from_height = None;
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

    let (needs_enter, needs_exit_anim, enter_active) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        let enter_active = c.enter_active;
        if needs_exit_anim && exiting {
            c.needs_exit_anim = false;
        }
        if exiting && c.exit_from_height.is_none() {
            c.exit_from_height = Some(c.measured_height.max(panel.layout_bounds[3]).max(1.0));
        }
        (needs_enter, needs_exit_anim, enter_active)
    };

    let exit_armed = presence.cell.borrow().exit_armed;
    // Seed only on first exit or when accordion-up was dropped; resume from current max-height (see seed_expand_exit).
    if exiting && (needs_exit_anim || (!expand_has_exit_anim(panel) && exit_armed)) {
        let h = panel_content_height(presence, panel, max_height_cap);
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

    // Keep waiting for layout until we can seed; do not clear needs_enter early.
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
            // Never seed from this frame's bounds — they may still be the viewport-capped
            // overflow height from apply_stored / last placement. Next layout is unconstrained.
            {
                let mut c = presence.cell.borrow_mut();
                c.measuring = true;
                c.enter_active = true;
                c.measured_height = 0.0;
                c.measured_width = 0.0;
                c.clip_width = None;
            }
            panel.layout_bounds[2] = 0.0;
            panel.layout_bounds[3] = 0.0;
            prepare_expand_measure(panel, max_height_cap);
            apply_expand_width_clip(panel, presence, false);
            return true;
        }

        let measured = take_measured_height(presence, panel, max_height_cap);
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
        return true;
    }

    if open_visual {
        // Grow stored content height; never shrink to the current viewport clip.
        let h = panel.layout_bounds[3];
        if h > 1.0 {
            let capped = match max_height_cap {
                Some(max) if max > 0.0 => h.min(max),
                _ => h,
            };
            let mut c = presence.cell.borrow_mut();
            if capped > c.measured_height + 0.5 {
                c.measured_height = capped;
                panel.set_attr(ATTR_FLOATING_CONTENT_H, format!("{capped}"));
            }
        }
        let w = panel.layout_bounds[2];
        if w > 1.0 {
            let mut c = presence.cell.borrow_mut();
            if w > c.measured_width + 0.5 {
                c.measured_width = w;
            }
        }
    }
    false
}

fn apply_floating_motion_inner(
    panel: &mut Element,
    shell: &FloatingShellState,
    presence: &FloatingPresenceHandle,
) -> bool {
    match shell.kind {
        FloatingMotionKind::Fade => apply_fade_motion(panel, shell.phase, presence),
        FloatingMotionKind::Expand => {
            apply_expand_motion(panel, shell.phase, presence, shell.max_height_cap)
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
    let tid = panel_tid(overlay_id);
    panel.template_id = Some(tid);
    let needs_frame = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
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
        if kind == FloatingMotionKind::Expand && !measuring && c.measured_height >= 1.0 {
            panel.set_attr(ATTR_FLOATING_CONTENT_H, format!("{}", c.measured_height));
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
            if let Some(h) = c.clip_height {
                let apply = h > 1.0 || matches!(phase, PresencePhase::Present);
                if apply {
                    panel.style.max_size.height = Dimension::length(h.max(0.0));
                    if matches!(phase, PresencePhase::Exiting) && h < EXPAND_EXIT_HIDE_PX {
                        panel.style.opacity = 0.0;
                    } else if c.enter_active || matches!(phase, PresencePhase::Exiting) {
                        panel.style.opacity = 1.0;
                    }
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
) -> Option<(PresencePhase, crate::app::portal_overlay::OverlayZ)> {
    use crate::app::portal_overlay::portal_sync_presence;
    let want = open.get();
    let phase = sync_floating_presence(presence, want);
    let mounted = floating_mounted_signal(presence);
    if !mounted.get() {
        let _ = portal_sync_presence(overlay_id, open, mounted, overlay_kind, overlay_mode);
        return None;
    }
    let oz = portal_sync_presence(overlay_id, open, mounted, overlay_kind, overlay_mode)?;
    Some((phase, oz))
}

/// After portal hoist: register seed keyframes from presence.
pub(crate) fn flush_floating_motion_subtree(root: &mut Element) -> bool {
    let mut needs_frame = false;
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
                    FloatingPresenceHandle {
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
            }
        }
    });
    needs_frame
}

fn tick_element_css(el: &mut Element, dt: f32) -> bool {
    if !el.style.animations.is_empty()
        || crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot)
    {
        crate::style::tick_element_animations(&mut el.style, &mut el.animation_snapshot, dt)
    } else {
        false
    }
}

/// Advance CSS on hoisted floating panels（post-layout stepper）。
pub(crate) fn tick_hoisted_floating_css(root: &mut Element) -> bool {
    let dt = crate::style::utility::frame_dt();
    let mut any = false;
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
                    FloatingPresenceHandle {
                        cell: Rc::clone(cell_rc),
                    },
                )
            };
            let tid = panel_tid(&shell.overlay_id);
            let Some(panel) = find_template_id_mut(root, &tid) else {
                any = true;
                continue;
            };
            if tick_element_css(panel, dt) {
                any = true;
            }
            if shell.kind == FloatingMotionKind::Expand {
                apply_expand_width_clip(
                    panel,
                    &handle,
                    matches!(shell.phase, PresencePhase::Exiting),
                );
            }
            if clamp_animated_max_h_to_place(panel, &shell.overlay_id) {
                any = true;
            }
            if shell.kind == FloatingMotionKind::Expand
                && matches!(shell.phase, PresencePhase::Exiting)
                && hide_expand_exit_sliver(panel, &handle, false)
            {
                any = true;
            }
        }
    });
    any
}

/// After Expand max-height changes, mark the panel dirty for a same-frame second layout.
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
                panel_tid(&shell.overlay_id)
            };
            if let Some(panel) = find_template_id(root, &tid) {
                layout_tree.mark_element_layout_dirty(panel);
            }
        }
    });
}

fn find_template_id<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
    let want = want.replace('\\', "/").to_ascii_lowercase();
    if el
        .template_id
        .as_deref()
        .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
    {
        return Some(el);
    }
    for child in &el.children {
        if let Some(hit) = find_template_id(child, &want) {
            return Some(hit);
        }
    }
    None
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
    use crate::style::tick_element_animations;

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
                let _ = tick_element_animations(&mut p.style, &mut p.animation_snapshot, 0.016);
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
                    .any(|a| a.name == ACCORDION_DOWN),
                "must seed accordion-down after measure"
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
                    .any(|a| a.name == ACCORDION_DOWN),
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
            let stored = handle.cell.borrow().measured_height;
            assert!(
                (stored - 200.0).abs() < 0.5,
                "idle clip 80 must not overwrite content measure 200, got {stored}"
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
            handle.cell.borrow_mut().measured_height = 120.0;
            handle.cell.borrow_mut().exit_from_height = Some(120.0);

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
            assert!(panel
                .style
                .animations
                .iter()
                .any(|a| a.name == ACCORDION_UP));
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
            handle.cell.borrow_mut().measured_height = 120.0;
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
                for _ in 0..20 {
                    let _ = tick_element_animations(
                        &mut panel.style,
                        &mut panel.animation_snapshot,
                        0.016,
                    );
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
}
