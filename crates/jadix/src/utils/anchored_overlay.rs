//! Headless anchored overlay shell ([`docs/components/overlay_shells.md`](../../../../docs/components/overlay_shells.md)).
//!
//! Three placement modes share one portal / presence / motion recipe:
//! [`Placement::Trigger`], [`Placement::Point`], [`Placement::Unanchored`].
//! Not in `prelude` — `use jadix::utils::anchored_overlay::{…}`.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized, retained_reactive_anchor};
use crate::utils::floating_motion::{
    apply_floating_panel_motion, floating_portal_phase, use_floating_presence,
    FloatingPresenceHandle,
};

use crate::core::reactivity::{
    create_context, isolate_reads, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::Element;
#[cfg(test)]
use crate::utils::floating_motion::floating_mounted_signal;
use crate::utils::popup::{
    apply_stored_floating_place, flow_hidden_portal_host, mark_floating_at_point,
    mark_floating_with_gap, mount_anchored_overlay, PopupPlacement, ATTR_FLOATING,
    ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_W, ATTR_FLOATING_POINT,
    ATTR_FLOATING_STRATEGY, ATTR_FLOATING_TRIGGER, ATTR_FLOATING_W, FLOATING_TRIGGER_GAP,
};

pub use crate::app::portal_overlay::{
    page_scroll_locked, set_page_scroll_locked, set_scroll_freeze, ScrollFreeze,
};
pub use crate::utils::floating_kind::FloatingMotionKind;

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// Flip with the viewport (default) or pin to the preferred trigger side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatingStrategy {
    #[default]
    Auto,
    Fixed,
}

impl FloatingStrategy {
    pub fn from_attr(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("fixed") {
            Self::Fixed
        } else {
            Self::Auto
        }
    }

    fn as_attr(self) -> Option<&'static str> {
        match self {
            Self::Fixed => Some("fixed"),
            Self::Auto => None,
        }
    }
}

/// Where an anchored overlay is positioned.
///
/// Point coordinates must be a [`Signal`] so drag / click-to-place updates
/// re-run the island (captured `f32` would freeze after the first register).
///
/// [`Self::Unanchored`] is **not** [`Self::Point`] at `(0, 0)`: it skips
/// `mark_floating` / `sync_floating_placement` and lays out against a viewport
/// stage (`fixed` / inset / flex). Folding the two would drop `%` / flex centering.
#[derive(Clone)]
pub enum Placement {
    Trigger {
        focus_id: String,
        placement: PopupPlacement,
        gap: f32,
        strategy: FloatingStrategy,
    },
    Point {
        pos: Signal<[f32; 2]>,
        gap: f32,
    },
    Unanchored,
}

impl Placement {
    pub fn trigger(
        focus_id: impl Into<String>,
        placement: PopupPlacement,
        gap: f32,
        strategy: FloatingStrategy,
    ) -> Self {
        Self::Trigger {
            focus_id: focus_id.into(),
            placement,
            gap,
            strategy,
        }
    }

    pub fn point(pos: Signal<[f32; 2]>, gap: f32) -> Self {
        Self::Point { pos, gap }
    }

    /// RSX `x`/`y` without `pos`: per-overlay signal, updated when Content rebuilds.
    pub fn point_xy(overlay_id: &str, x: f32, y: f32, gap: f32) -> Self {
        let key = format!("{overlay_id}__ao_point");
        let pos = use_signal(key.as_str(), || [x, y]);
        if pos.peek() != [x, y] {
            pos.set([x, y]);
        }
        Self::Point { pos, gap }
    }
}

pub fn parse_overlay_kind(s: &str) -> OverlayKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "menu" => OverlayKind::Menu,
        "tooltip" => OverlayKind::Tooltip,
        "modal" => OverlayKind::Modal,
        _ => OverlayKind::Popover,
    }
}

pub fn parse_floating_motion(s: &str) -> FloatingMotionKind {
    if s.trim().eq_ignore_ascii_case("expand") {
        FloatingMotionKind::Expand
    } else {
        FloatingMotionKind::Fade
    }
}

pub fn parse_overlay_mode(s: &str) -> OverlayMode {
    if s.trim().eq_ignore_ascii_case("pinned") {
        OverlayMode::Pinned
    } else {
        OverlayMode::Stacked
    }
}

/// Build [`Placement`] from RSX attrs. Call from [`anchored_overlay_content`] (has overlay id).
pub fn placement_from_rsx(
    overlay_id: &str,
    mode: &str,
    side: &str,
    align: &str,
    side_offset: f32,
    focus_id: &str,
    strategy: &str,
    pos: Option<Signal<[f32; 2]>>,
    x: f32,
    y: f32,
    gap: f32,
) -> Placement {
    match mode.trim().to_ascii_lowercase().as_str() {
        "point" => {
            if let Some(pos) = pos {
                Placement::Point { pos, gap }
            } else {
                Placement::point_xy(overlay_id, x, y, gap)
            }
        }
        "unanchored" => Placement::Unanchored,
        _ => Placement::Trigger {
            focus_id: focus_id.to_string(),
            placement: PopupPlacement::from_side_align(side, align),
            gap: side_offset,
            strategy: FloatingStrategy::from_attr(strategy),
        },
    }
}

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AnchoredOverlayCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub trigger_fid: Signal<String>,
}

fn overlay_context() -> Context<AnchoredOverlayCtx> {
    static CTX: OnceLock<Context<AnchoredOverlayCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AnchoredOverlayCtx>)
}

fn retained_overlay_context() -> Context<bool> {
    static CTX: OnceLock<Context<bool>> = OnceLock::new();
    *CTX.get_or_init(create_context::<bool>)
}

pub fn use_anchored_overlay() -> Option<AnchoredOverlayCtx> {
    use_context(overlay_context())
}

/// Provide anchored-overlay context inside an existing scope (Popover / Tooltip / HoverCard).
pub fn provide_anchored_overlay_context(
    open: Signal<bool>,
    id: String,
    trigger_fid: Signal<String>,
) {
    provide_context(
        overlay_context(),
        AnchoredOverlayCtx {
            open,
            id,
            trigger_fid,
        },
    );
    provide_context(retained_overlay_context(), false);
}

fn require_anchored_overlay(part: &str) -> AnchoredOverlayCtx {
    use_anchored_overlay().unwrap_or_else(|| {
        panic!("{part} must be used within <AnchoredOverlay> / anchored_overlay_scope");
    })
}

pub fn anchored_overlay_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    anchored_overlay_scope_impl(id, open, body, false)
}

pub fn retained_anchored_overlay_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    anchored_overlay_scope_impl(id, open, body, true)
}

fn anchored_overlay_scope_impl(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
    retained: bool,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "anchored_overlay");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__ao_open").as_str(), || false));
    let trigger_fid = use_signal(format!("{id}__ao_trigger_fid").as_str(), || {
        format!("{id}__ao_trigger")
    });
    let mut root = with_child_owner(|| {
        provide_context(
            overlay_context(),
            AnchoredOverlayCtx {
                open,
                id: id.clone(),
                trigger_fid,
            },
        );
        provide_context(retained_overlay_context(), retained);
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__anchored_overlay"));
    }
    root
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct AnchoredOverlayBuilder {
    id: String,
    open: Option<Signal<bool>>,
    placement: Option<Placement>,
    kind: Option<OverlayKind>,
    motion: Option<FloatingMotionKind>,
    mode: OverlayMode,
    retained: bool,
    match_trigger_width: bool,
    max_h: Option<String>,
    width: Option<String>,
    /// Toggle / open button `focus_id`. Used by Unanchored for outside-press
    /// exemption (not placement). Trigger mode prefers [`Placement::Trigger::focus_id`].
    trigger_id: String,
    /// `None` = Menu/Modal freeze the page, Popover/Tooltip do not.
    lock_page_scroll: Option<bool>,
    /// Explicit freeze scope; wins over [`Self::lock_page_scroll`].
    scroll_freeze: Option<crate::app::portal_overlay::ScrollFreeze>,
}

pub fn anchored_overlay(id: impl Into<String>) -> AnchoredOverlayBuilder {
    AnchoredOverlayBuilder {
        id: id.into(),
        open: None,
        placement: None,
        kind: None,
        motion: None,
        mode: OverlayMode::Stacked,
        retained: false,
        match_trigger_width: false,
        max_h: None,
        width: None,
        trigger_id: String::new(),
        lock_page_scroll: None,
        scroll_freeze: None,
    }
}

impl AnchoredOverlayBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }

    pub fn kind(mut self, kind: OverlayKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn motion(mut self, motion: FloatingMotionKind) -> Self {
        self.motion = Some(motion);
        self
    }

    /// Resident surface: outside presses and Esc do not dismiss it. `open`
    /// still controls mounting and lets author close buttons start the exit.
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.mode = if pinned {
            OverlayMode::Pinned
        } else {
            OverlayMode::Stacked
        };
        self
    }

    pub fn retained(mut self) -> Self {
        self.retained = true;
        self
    }

    pub fn match_trigger_width(mut self, yes: bool) -> Self {
        self.match_trigger_width = yes;
        self
    }

    pub fn max_h(mut self, h: impl Into<String>) -> Self {
        self.max_h = Some(h.into());
        self
    }

    pub fn width(mut self, w: impl Into<String>) -> Self {
        self.width = Some(w.into());
        self
    }

    /// Register a toggle button `focus_id` for outside-press (Unanchored / Point).
    ///
    /// Trigger placement still uses [`Placement::Trigger::focus_id`]. Empty falls
    /// back to `{overlay_id}__ao_trigger`.
    pub fn trigger_id(mut self, id: impl Into<String>) -> Self {
        self.trigger_id = id.into();
        self
    }

    /// Freeze page wheel / pan / fling while this overlay is open.
    ///
    /// Default is off for popovers; menu-kind overlays freeze the **page** unless this is
    /// `false`. Overlay ScrollAreas still move. For a different scope use [`Self::scroll_freeze`].
    /// For hover-region locks that are not tied to an open overlay, use
    /// [`crate::app::portal_overlay::set_page_scroll_locked`].
    pub fn lock_page_scroll(mut self, yes: bool) -> Self {
        self.lock_page_scroll = Some(yes);
        self
    }

    /// Freeze wheel / pan / fling for `page`, `overlay`, or the whole program.
    pub fn scroll_freeze(mut self, scope: crate::app::portal_overlay::ScrollFreeze) -> Self {
        self.scroll_freeze = Some(scope);
        self
    }

    /// Panel body when [`Self::placement`] is set; otherwise compound Trigger/Content tree.
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        if let Some(placement) = self.placement {
            let opts = AnchoredOverlayContentOpts {
                placement: Some(placement),
                placement_mode: String::new(),
                side: String::new(),
                align: String::new(),
                side_offset: FLOATING_TRIGGER_GAP,
                focus_id: self.trigger_id,
                strategy: String::new(),
                pos: None,
                x: 0.0,
                y: 0.0,
                gap: 0.0,
                kind: self.kind.unwrap_or(OverlayKind::Popover),
                motion: self.motion.unwrap_or(FloatingMotionKind::Fade),
                mode: self.mode,
                match_trigger_width: self.match_trigger_width,
                max_h: self.max_h,
                width: self.width,
                lock_page_scroll: self.lock_page_scroll,
                scroll_freeze: self.scroll_freeze,
            };
            let scope = if self.retained {
                retained_anchored_overlay_scope
            } else {
                anchored_overlay_scope
            };
            scope(self.id, self.open, move || {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![anchored_overlay_content(opts, body)])
            })
        } else {
            let scope = if self.retained {
                retained_anchored_overlay_scope
            } else {
                anchored_overlay_scope
            };
            scope(self.id, self.open, body)
        }
    }

    /// Alias of [`Self::child`] for the one-shot panel API.
    pub fn on_panel(self, body: impl Fn() -> Element + 'static) -> Element {
        self.child(body)
    }
}

// ---------------------------------------------------------------------------
// Content opts
// ---------------------------------------------------------------------------

/// Shared by Builder [`.placement`](AnchoredOverlayBuilder::placement) and RSX `<AnchoredOverlayContent>`.
#[derive(Clone)]
pub struct AnchoredOverlayContentOpts {
    pub placement: Option<Placement>,
    pub placement_mode: String,
    pub side: String,
    pub align: String,
    pub side_offset: f32,
    pub focus_id: String,
    pub strategy: String,
    pub pos: Option<Signal<[f32; 2]>>,
    pub x: f32,
    pub y: f32,
    pub gap: f32,
    pub kind: OverlayKind,
    pub motion: FloatingMotionKind,
    pub mode: OverlayMode,
    pub match_trigger_width: bool,
    pub max_h: Option<String>,
    pub width: Option<String>,
    /// `None` = [`crate::app::portal_overlay::default_scroll_freeze`] for [`Self::kind`].
    /// Popovers default off; menus / modals default to page-only freeze.
    pub lock_page_scroll: Option<bool>,
    /// Explicit freeze scope; wins over [`Self::lock_page_scroll`] when set.
    pub scroll_freeze: Option<crate::app::portal_overlay::ScrollFreeze>,
}

impl Default for AnchoredOverlayContentOpts {
    fn default() -> Self {
        Self {
            placement: None,
            placement_mode: "trigger".into(),
            side: "bottom".into(),
            align: "start".into(),
            side_offset: FLOATING_TRIGGER_GAP,
            focus_id: String::new(),
            strategy: String::new(),
            pos: None,
            x: 0.0,
            y: 0.0,
            gap: 0.0,
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Fade,
            mode: OverlayMode::Stacked,
            match_trigger_width: false,
            max_h: None,
            width: None,
            lock_page_scroll: None,
            scroll_freeze: None,
        }
    }
}

impl AnchoredOverlayContentOpts {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }

    pub fn kind(mut self, kind: OverlayKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn motion(mut self, motion: FloatingMotionKind) -> Self {
        self.motion = motion;
        self
    }

    pub fn mode(mut self, mode: OverlayMode) -> Self {
        self.mode = mode;
        self
    }

    /// Resident surface; only the author's `open` updates close it.
    pub fn pinned(self, pinned: bool) -> Self {
        self.mode(if pinned {
            OverlayMode::Pinned
        } else {
            OverlayMode::Stacked
        })
    }

    pub fn match_trigger_width(mut self, yes: bool) -> Self {
        self.match_trigger_width = yes;
        self
    }

    pub fn max_h(mut self, h: impl Into<String>) -> Self {
        self.max_h = Some(h.into());
        self
    }

    pub fn width(mut self, w: impl Into<String>) -> Self {
        self.width = Some(w.into());
        self
    }

    /// Freeze page scroll while this floating layer is open.
    ///
    /// Default is off for popovers; menu-kind overlays freeze the page unless this is
    /// `false`. Overlay ScrollAreas still move. For overlay-only or whole-program freeze,
    /// use [`Self::scroll_freeze`].
    /// For hover-region locks not tied to an overlay, use [`set_page_scroll_locked`].
    pub fn lock_page_scroll(mut self, yes: bool) -> Self {
        self.lock_page_scroll = Some(yes);
        self
    }

    /// Freeze wheel / pan / fling for page, overlay, or the whole program.
    pub fn scroll_freeze(mut self, scope: crate::app::portal_overlay::ScrollFreeze) -> Self {
        self.scroll_freeze = Some(scope);
        self
    }

    fn resolved_scroll_freeze(&self) -> crate::app::portal_overlay::ScrollFreeze {
        crate::app::portal_overlay::resolve_scroll_freeze(
            self.kind,
            self.scroll_freeze,
            self.lock_page_scroll,
        )
    }

    fn resolve(&self, overlay_id: &str) -> Placement {
        if let Some(p) = self.placement.clone() {
            return p;
        }
        placement_from_rsx(
            overlay_id,
            &self.placement_mode,
            &self.side,
            &self.align,
            self.side_offset,
            &self.focus_id,
            &self.strategy,
            self.pos,
            self.x,
            self.y,
            self.gap,
        )
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

pub fn anchored_overlay_content(
    opts: AnchoredOverlayContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_anchored_overlay("AnchoredOverlayContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let trigger_fid = ctx.trigger_fid;
    let retained = use_context(retained_overlay_context()).unwrap_or(false);
    let view_id = format!("{id}__view");
    let presence = use_floating_presence(&id, opts.motion);
    let render = move || {
        if open.try_peek().is_none() {
            let _ = floating_portal_phase(
                &id,
                open,
                &presence,
                opts.kind,
                opts.mode,
                opts.resolved_scroll_freeze(),
            );
            return flow_hidden_portal_host(Element::new());
        }
        let Some((phase, oz)) = floating_portal_phase(
            &id,
            open,
            &presence,
            opts.kind,
            opts.mode,
            opts.resolved_scroll_freeze(),
        ) else {
            return flow_hidden_portal_host(Element::new());
        };
        assemble_mounted_panel(
            &id,
            open,
            trigger_fid,
            &presence,
            &opts,
            isolate_reads(|| body()),
            phase,
            oz,
        )
    };
    if retained {
        retained_reactive_anchor(view_id.clone(), render);
    } else {
        reactive_anchor(view_id.clone(), render);
    }
    flow_hidden_portal_host(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none())
}

/// RSX `<AnchoredOverlayContent>` entry (same implementation as [`anchored_overlay_content`]).
pub fn anchored_overlay_content_from_rsx(
    opts: AnchoredOverlayContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    anchored_overlay_content(opts, body)
}

#[cfg(test)]
fn assemble_panel(
    id: &str,
    open: Signal<bool>,
    trigger_fid: Signal<String>,
    presence: &FloatingPresenceHandle,
    opts: &AnchoredOverlayContentOpts,
    panel: Element,
) -> Element {
    let Some((phase, oz)) = floating_portal_phase(
        id,
        open,
        presence,
        opts.kind,
        opts.mode,
        opts.resolved_scroll_freeze(),
    ) else {
        return flow_hidden_portal_host(Element::new());
    };
    assemble_mounted_panel(id, open, trigger_fid, presence, opts, panel, phase, oz)
}

fn assemble_mounted_panel(
    id: &str,
    open: Signal<bool>,
    trigger_fid: Signal<String>,
    presence: &FloatingPresenceHandle,
    opts: &AnchoredOverlayContentOpts,
    mut panel: Element,
    phase: crate::utils::presence::PresencePhase,
    oz: crate::app::portal_overlay::OverlayZ,
) -> Element {
    let placement = opts.resolve(id);
    apply_placement(
        &mut panel,
        id,
        &placement,
        trigger_fid,
        opts.focus_id.as_str(),
        opts.match_trigger_width,
        opts.max_h.as_deref(),
        opts.width.as_deref(),
    );
    // Stamp EXITING before apply_stored so a reverse-expand edge is restored, not
    // cleared. Otherwise the first sync after dismiss flips the menu below the
    // trigger (nested ColorPicker format Select exit jump).
    if matches!(phase, crate::utils::presence::PresencePhase::Exiting) {
        panel.set_attr(crate::utils::popup::ATTR_FLOATING_EXITING, "1");
    }
    match &placement {
        Placement::Trigger { .. } | Placement::Point { .. } => {
            apply_stored_floating_place(&mut panel, id);
        }
        Placement::Unanchored => {}
    }
    if apply_floating_panel_motion(
        id,
        &mut panel,
        opts.motion,
        phase,
        presence,
        open,
        opts.kind,
        opts.mode,
        opts.max_h.as_deref().and_then(parse_max_h_cap_px),
    ) {
        crate::app::runtime::request_redraw();
    }
    // Same 0×0 portal mount as Trigger/Point. Unanchored must not wrap a viewport
    // stage: author `fixed` / inset already resolve against the window.
    mount_anchored_overlay(panel, oz.panel)
}

/// Smoke: current portal phase. `None` when not mounted.
#[cfg(test)]
pub(crate) fn anchored_overlay_phase(
    id: &str,
    open: Signal<bool>,
    opts: &AnchoredOverlayContentOpts,
) -> Option<crate::utils::presence::PresencePhase> {
    let presence = use_floating_presence(id, opts.motion);
    floating_portal_phase(
        id,
        open,
        &presence,
        opts.kind,
        opts.mode,
        opts.resolved_scroll_freeze(),
    )
    .map(|(phase, _)| phase)
}

/// Smoke: exit animation still mounted after `open` is false.
#[cfg(test)]
pub(crate) fn anchored_overlay_is_mounted(id: &str, opts: &AnchoredOverlayContentOpts) -> bool {
    let presence = use_floating_presence(id, opts.motion);
    floating_mounted_signal(&presence).peek()
}

/// Smoke: apply panel motion without exposing the presence handle.
#[cfg(test)]
#[cfg(feature = "animation-base")]
pub(crate) fn apply_anchored_overlay_panel_motion(
    id: &str,
    panel: &mut Element,
    opts: &AnchoredOverlayContentOpts,
    open: Signal<bool>,
) -> bool {
    let presence = use_floating_presence(id, opts.motion);
    let Some((phase, _)) = floating_portal_phase(
        id,
        open,
        &presence,
        opts.kind,
        opts.mode,
        opts.resolved_scroll_freeze(),
    ) else {
        return false;
    };
    apply_floating_panel_motion(
        id,
        panel,
        opts.motion,
        phase,
        &presence,
        open,
        opts.kind,
        opts.mode,
        opts.max_h.as_deref().and_then(parse_max_h_cap_px),
    )
}

fn apply_placement(
    panel: &mut Element,
    overlay_id: &str,
    placement: &Placement,
    trigger_fid: Signal<String>,
    content_focus_id: &str,
    match_trigger_width: bool,
    max_h: Option<&str>,
    width: Option<&str>,
) {
    match placement {
        Placement::Trigger {
            focus_id,
            placement,
            gap,
            strategy,
        } => {
            let fid = if focus_id.trim().is_empty() {
                trigger_fid.peek()
            } else {
                focus_id.clone()
            };
            mark_floating_with_gap(panel, overlay_id, &fid, *placement, *gap);
            if let Some(s) = strategy.as_attr() {
                panel.set_attr(ATTR_FLOATING_STRATEGY, s);
            }
            apply_size_attrs(panel, match_trigger_width, max_h, width);
            take_absolute(panel);
        }
        Placement::Point { pos, gap } => {
            // Peek: `.get()` subscribed the content island, remounting the panel on
            // every title-bar tick (Gallery Point 拖拽卡顿 / 不跟手).
            let [x, y] = pos.peek();
            mark_floating_at_point(panel, overlay_id, "", x, y, *gap);
            panel.attrs.remove(ATTR_FLOATING_TRIGGER);
            apply_size_attrs(panel, match_trigger_width, max_h, width);
            take_absolute(panel);
        }
        Placement::Unanchored => {
            // Identity only: portal key + outside-press. No PLACEMENT / POINT so
            // sync_floating_placement does not move author `fixed` / inset.
            panel.set_attr(ATTR_FLOATING, overlay_id);
            let fid = if !content_focus_id.trim().is_empty() {
                content_focus_id.trim().to_string()
            } else {
                trigger_fid.peek()
            };
            if !fid.trim().is_empty() {
                panel.set_attr(ATTR_FLOATING_TRIGGER, fid);
            }
        }
    }
}

fn take_absolute(panel: &mut Element) {
    let mut taken = Element::new();
    std::mem::swap(panel, &mut taken);
    *panel = taken.absolute();
}

fn parse_max_h_cap_px(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("rem") {
        n.trim().parse::<f32>().ok().map(|v| v * 16.0)
    } else if let Some(n) = s.strip_suffix("px") {
        n.trim().parse().ok()
    } else {
        s.parse().ok()
    }
}

fn apply_size_attrs(
    panel: &mut Element,
    match_trigger_width: bool,
    max_h: Option<&str>,
    width: Option<&str>,
) {
    panel.set_attr(
        ATTR_FLOATING_MATCH_TRIGGER,
        if match_trigger_width { "1" } else { "0" },
    );
    panel.set_attr(ATTR_FLOATING_MIN_W, "0");
    if let Some(w) = width.map(str::trim).filter(|s| !s.is_empty()) {
        // `"auto"` means content-sized. Writing it as `jadix_floating_w` makes sync
        // treat it as an explicit px (parse fallback = trigger width) and lock
        // AvatarGroup / similar expand panels to ~two faces.
        if !w.eq_ignore_ascii_case("auto") {
            panel.set_attr(ATTR_FLOATING_W, w);
        }
    } else if !match_trigger_width {
        // Author shell already set a definite width (e.g. gallery `.w("16rem")`).
        // Without stamping `jadix_floating_w`, `match_trigger=0` coerces `width: auto`
        // and reflows wrap height for a frame (intermittent stretch on open).
        if let Some(px) = crate::layout::dim_length_px(panel.style.size.width).filter(|w| *w > 0.5)
        {
            panel.set_attr(ATTR_FLOATING_W, format!("{px}px"));
        }
    }
    if let Some(h) = max_h.map(str::trim).filter(|s| !s.is_empty()) {
        panel.set_attr(ATTR_FLOATING_MAX_H, h);
    }
}

// ---------------------------------------------------------------------------
// Point-mode title-bar drag (WP-OVL.3)
// ---------------------------------------------------------------------------

struct FloatingPanelDrag {
    id: String,
    pos: Signal<[f32; 2]>,
    origin: [f32; 2],
    start_pointer: [f32; 2],
    /// Visual left/top at pointer-down. Filled on the first live-tree sync.
    panel_origin: Cell<Option<[f32; 2]>>,
    stamp_key: Cell<u64>,
    panel_size: Cell<[f32; 2]>,
    last_visual: Cell<Option<[f32; 2]>>,
    last_pointer: Cell<[f32; 2]>,
}

thread_local! {
    static FLOATING_DRAG: RefCell<Option<FloatingPanelDrag>> = const { RefCell::new(None) };
    static ENDED_FLOATING_DRAG: Cell<bool> = const { Cell::new(false) };
    /// Mouseup: commit left/top on the next hoist (Taffy still at grab origin until dirty).
    static PENDING_POINT_COMMIT: RefCell<Option<(String, [f32; 2])>> = const { RefCell::new(None) };
    /// Fast-path Taffy must recompute the committed panel or it jumps back to the grab box.
    static POINT_COMMIT_DIRTY: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn begin_floating_panel_drag(id: impl Into<String>, pos: Signal<[f32; 2]>, x: f32, y: f32) {
    let id = id.into();
    ENDED_FLOATING_DRAG.set(false);
    // Mouseup commit is applied on the next hoist. Starting another grab before
    // that hoist used to `take()` it, so Taffy stayed at the first grab origin
    // (release snap-back + gap that grows on every drag).
    let pending_xy = PENDING_POINT_COMMIT.with(|p| {
        let mut g = p.borrow_mut();
        if g.as_ref().is_some_and(|(i, _)| i == &id) {
            g.take().map(|(_, xy)| xy)
        } else {
            None
        }
    });
    if pending_xy.is_some() {
        POINT_COMMIT_DIRTY.with(|d| {
            *d.borrow_mut() = Some(id.clone());
        });
    }
    crate::utils::drag::pointer::begin(
        crate::utils::drag::pointer::DragKind::FloatingPanel,
        id.clone(),
        x,
        y,
    );
    FLOATING_DRAG.with(|d| {
        *d.borrow_mut() = Some(FloatingPanelDrag {
            id,
            pos,
            origin: pos.peek(),
            start_pointer: [x, y],
            panel_origin: Cell::new(pending_xy),
            stamp_key: Cell::new(0),
            panel_size: Cell::new([0.0, 0.0]),
            last_visual: Cell::new(pending_xy),
            last_pointer: Cell::new([x, y]),
        });
    });
}

pub fn is_floating_panel_dragging() -> bool {
    FLOATING_DRAG.with(|d| d.borrow().is_some())
}

/// Overlay id of the live Point-mode title-bar drag, if any.
pub(crate) fn floating_panel_drag_id() -> Option<String> {
    FLOATING_DRAG.with(|d| d.borrow().as_ref().map(|drag| drag.id.clone()))
}

pub fn tick_floating_panel_drag(x: f32, y: f32) -> bool {
    FLOATING_DRAG.with(|d| {
        let (pos, next) = {
            let guard = d.borrow();
            let Some(drag) = guard.as_ref() else {
                return false;
            };
            drag.last_pointer.set([x, y]);
            (
                drag.pos,
                [
                    drag.origin[0] + x - drag.start_pointer[0],
                    drag.origin[1] + y - drag.start_pointer[1],
                ],
            )
        };
        if pos.peek() != next {
            // Silent: the island peeks `pos`; notifying remounted the panel.
            pos.set_silent(next);
            note_point_overlay_tick_damage();
        }
        true
    })
}

pub fn clear_floating_panel_drag() {
    let ended = FLOATING_DRAG.with(|d| d.borrow_mut().take());
    if let Some(drag) = ended {
        let pointer = drag.last_pointer.get();
        let visual = match drag.panel_origin.get() {
            Some(origin) => [
                origin[0] + pointer[0] - drag.start_pointer[0],
                origin[1] + pointer[1] - drag.start_pointer[1],
            ],
            None => drag.last_visual.get().unwrap_or_else(|| drag.pos.peek()),
        };
        PENDING_POINT_COMMIT.with(|p| {
            *p.borrow_mut() = Some((drag.id.clone(), visual));
        });
        ENDED_FLOATING_DRAG.set(true);
        crate::utils::drag::pointer::clear();
        crate::app::release_pointer_capture();
        crate::app::runtime::request_redraw();
    }
}

/// True if a Point-mode title-bar drag ended since the last take (mouseup dismiss).
pub(crate) fn take_ended_floating_panel_drag() -> bool {
    ENDED_FLOATING_DRAG.replace(false)
}

/// Title-bar (or other) handle: pointer-down starts a Point-mode drag of `pos`.
pub fn drag_handle(id: impl Into<String>, pos: Signal<[f32; 2]>, mut el: Element) -> Element {
    let id = id.into();
    let fid = format!("{id}__ao_drag");
    if el.focus_id.is_none() {
        el = el.focus_id(fid.as_str());
    }
    let capture = el.focus_id.clone().unwrap_or(fid);
    el = el.cursor("grab");
    el.on_pointer_down(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        begin_floating_panel_drag(id.clone(), pos, e.x, e.y);
        crate::app::set_pointer_capture(capture.clone());
        e.stop_propagation();
    })
}

fn read_panel_xy(el: &Element) -> Option<[f32; 2]> {
    let left = crate::core::tree::lpa_length_px(el.style.inset.left)?;
    let top = crate::core::tree::lpa_length_px(el.style.inset.top)?;
    Some([left, top])
}

fn grab_panel_xy(el: &Element) -> [f32; 2] {
    // Once per drag (`panel_origin` Cell). Insets are the Taffy grab; if they
    // are still `auto`, last `layout_bounds` is the previous paint origin.
    read_panel_xy(el).unwrap_or([el.layout_bounds[0], el.layout_bounds[1]])
}

fn write_panel_xy(el: &mut Element, xy: [f32; 2]) {
    el.style.inset.left = Element::into_margin_auto(xy[0]);
    el.style.inset.top = Element::into_margin_auto(xy[1]);
}

/// Pin Taffy left/top at the grab origin. Collect patches `abs` to the live visual.
pub(crate) fn pin_overlay_follow_insets(el: &mut Element, base: [f32; 2]) {
    if read_panel_xy(el) != Some(base) {
        write_panel_xy(el, base);
    }
    el.style.translate = [0.0, 0.0];
}

fn note_point_overlay_follow_damage(el: &Element, visual: [f32; 2]) {
    crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
    let w = el.layout_bounds[2];
    let h = el.layout_bounds[3];
    let key = crate::layout::layout_tree::overlay_portal_stamp_key(el);
    FLOATING_DRAG.with(|d| {
        if let Some(drag) = d.borrow().as_ref() {
            if point_overlay_matches_drag(el, drag.id.as_str()) {
                drag.stamp_key.set(key);
                if w > 0.5 && h > 0.5 {
                    drag.panel_size.set([w, h]);
                }
                drag.last_visual.set(Some(visual));
            }
        }
    });
    if w > 0.5 && h > 0.5 {
        crate::app::damage::push_frame_overlay_damage(key, [visual[0], visual[1], w, h]);
    }
}

/// CursorMoved runs before RedrawRequested: queue dest-reset + new box so this
/// frame's `take_frame_paint_nodes` already has Overlay dirty (vsync follow).
fn note_point_overlay_tick_damage() {
    FLOATING_DRAG.with(|d| {
        let drag = d.borrow();
        let Some(drag) = drag.as_ref() else {
            return;
        };
        let key = drag.stamp_key.get();
        let size = drag.panel_size.get();
        if key == 0 || size[0] < 0.5 || size[1] < 0.5 {
            return;
        }
        let Some(origin) = drag.panel_origin.get() else {
            return;
        };
        let pointer = drag.last_pointer.get();
        let visual = [
            origin[0] + pointer[0] - drag.start_pointer[0],
            origin[1] + pointer[1] - drag.start_pointer[1],
        ];
        crate::app::damage::queue_overlay_follow_tick_damage(
            key,
            size,
            drag.last_visual.get(),
            visual,
        );
        drag.last_visual.set(Some(visual));
    });
}

fn point_overlay_matches_drag(el: &Element, id: &str) -> bool {
    el.attrs.get(ATTR_FLOATING).map(String::as_str) == Some(id)
        && el.attrs.get(ATTR_FLOATING_POINT).is_some()
}

/// Fingerprint skip would freeze the panel at last Taffy origin (geom_key is Taffy abs).
pub(crate) fn point_overlay_drag_collect_live(el: &Element) -> bool {
    let Some(id) = floating_panel_drag_id() else {
        return false;
    };
    point_overlay_matches_drag(el, &id)
}

fn point_overlay_drag_origin(el: &Element) -> Option<[f32; 2]> {
    FLOATING_DRAG.with(|d| {
        let drag = d.borrow();
        let drag = drag.as_ref()?;
        if !point_overlay_matches_drag(el, drag.id.as_str()) {
            return None;
        }
        let panel_origin = drag.panel_origin.get().unwrap_or_else(|| grab_panel_xy(el));
        if drag.panel_origin.get().is_none() {
            drag.panel_origin.set(Some(panel_origin));
        }
        let pointer = drag.last_pointer.get();
        Some([
            panel_origin[0] + pointer[0] - drag.start_pointer[0],
            panel_origin[1] + pointer[1] - drag.start_pointer[1],
        ])
    })
}

fn point_overlay_drag_base(el: &Element) -> Option<[f32; 2]> {
    FLOATING_DRAG.with(|d| {
        let drag = d.borrow();
        let drag = drag.as_ref()?;
        if !point_overlay_matches_drag(el, drag.id.as_str()) {
            return None;
        }
        Some(drag.panel_origin.get().unwrap_or_else(|| grab_panel_xy(el)))
    })
}

fn walk_mut_first(root: &mut Element, mut visit: impl FnMut(&mut Element) -> bool) {
    let mut stack: Vec<*mut Element> = vec![root as *mut Element];
    while let Some(ptr) = stack.pop() {
        let el = unsafe { &mut *ptr };
        if visit(el) {
            return;
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
}

/// Paint-only follow: Taffy stays at grab origin; collect patches abs the same frame.
pub(crate) fn sync_point_overlay_drag_insets(root: &mut Element) {
    if is_floating_panel_dragging() {
        walk_mut_first(root, |el| {
            let Some(visual) = point_overlay_drag_origin(el) else {
                return false;
            };
            let Some(base) = point_overlay_drag_base(el) else {
                return false;
            };
            let pos = FLOATING_DRAG.with(|d| {
                d.borrow()
                    .as_ref()
                    .map(|drag| drag.pos.peek())
                    .unwrap_or(visual)
            });
            el.set_attr(ATTR_FLOATING_POINT, format!("{},{}", pos[0], pos[1]));
            pin_overlay_follow_insets(el, base);
            note_point_overlay_follow_damage(el, visual);
            true
        });
        return;
    }
    let pending = PENDING_POINT_COMMIT.with(|p| p.borrow_mut().take());
    let Some((id, xy)) = pending else {
        return;
    };
    walk_mut_first(root, |el| {
        if !point_overlay_matches_drag(el, id.as_str()) {
            return false;
        }
        write_panel_xy(el, xy);
        el.style.translate = [0.0, 0.0];
        el.set_attr(ATTR_FLOATING_POINT, format!("{},{}", xy[0], xy[1]));
        POINT_COMMIT_DIRTY.with(|d| {
            *d.borrow_mut() = Some(id.clone());
        });
        true
    });
}

/// Fast-path Taffy id to dirty after mouseup commit (grab-origin cache → visual inset).
pub(crate) fn take_point_overlay_commit_dirty() -> Option<String> {
    POINT_COMMIT_DIRTY.with(|d| d.borrow_mut().take())
}

/// Same-frame paint follow. Returns the visual origin so collect patches `abs`.
pub(crate) fn adjust_point_overlay_collect_origin(el: &mut Element) -> Option<[f32; 2]> {
    let visual = point_overlay_drag_origin(el)?;
    let base = point_overlay_drag_base(el)?;
    let pos = FLOATING_DRAG.with(|d| {
        d.borrow()
            .as_ref()
            .map(|drag| drag.pos.peek())
            .unwrap_or(visual)
    });
    el.set_attr(ATTR_FLOATING_POINT, format!("{},{}", pos[0], pos[1]));
    pin_overlay_follow_insets(el, base);
    Some(visual)
}

#[cfg(test)]
#[path = "tests/anchored_overlay.rs"]
mod tests;
