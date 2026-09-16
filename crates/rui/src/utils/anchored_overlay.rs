//! Headless anchored overlay shell ([`docs/components/overlay_shells.md`](../../../../docs/components/overlay_shells.md)).
//!
//! Three placement modes share one portal / presence / motion recipe:
//! [`Placement::Trigger`], [`Placement::Point`], [`Placement::Unanchored`].
//! Not in `prelude` — `use rui::utils::anchored_overlay::{…}`.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
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
    apply_stored_floating_place, mark_floating_at_point, mark_floating_with_gap,
    mount_anchored_overlay, PopupPlacement, ATTR_FLOATING, ATTR_FLOATING_MATCH_TRIGGER,
    ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_W, ATTR_FLOATING_STRATEGY, ATTR_FLOATING_TRIGGER,
    ATTR_FLOATING_W, FLOATING_TRIGGER_GAP,
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
                mode: OverlayMode::Stacked,
                match_trigger_width: self.match_trigger_width,
                max_h: self.max_h,
                width: self.width,
                lock_page_scroll: self.lock_page_scroll,
                scroll_freeze: self.scroll_freeze,
            };
            anchored_overlay_scope(self.id, self.open, move || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![anchored_overlay_content(opts, body)])
            })
        } else {
            anchored_overlay_scope(self.id, self.open, body)
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
    let view_id = format!("{id}__view");
    let presence = use_floating_presence(&id, opts.motion);

    reactive_anchor(view_id.clone(), move || {
        // Do not build `body()` until the portal still wants a panel. ColorPicker
        // hex Input binds a scoped `out`; TabsContent dispose kills it while a
        // leftover hoist can still paint. `open.try_peek()` alone is not enough:
        // gallery `<ColorPicker>` keeps `open` on a child owner that dies with
        // the tab, but `assemble_panel` used to evaluate `body()` first.
        if open.try_peek().is_none() {
            let _ = floating_portal_phase(
                &id,
                open,
                &presence,
                opts.kind,
                opts.mode,
                opts.resolved_scroll_freeze(),
            );
            return Element::new().w(0.0).h(0.0);
        }
        let Some((phase, oz)) = floating_portal_phase(
            &id,
            open,
            &presence,
            opts.kind,
            opts.mode,
            opts.resolved_scroll_freeze(),
        ) else {
            return Element::new().w(0.0).h(0.0);
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
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
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
        return Element::new().w(0.0).h(0.0);
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
            let [x, y] = pos.get();
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
        // `"auto"` means content-sized. Writing it as `rui_floating_w` makes sync
        // treat it as an explicit px (parse fallback = trigger width) and lock
        // AvatarGroup / similar expand panels to ~two faces.
        if !w.eq_ignore_ascii_case("auto") {
            panel.set_attr(ATTR_FLOATING_W, w);
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
    pos: Signal<[f32; 2]>,
    origin: [f32; 2],
    start_pointer: [f32; 2],
}

thread_local! {
    static FLOATING_DRAG: RefCell<Option<FloatingPanelDrag>> = const { RefCell::new(None) };
}

pub fn begin_floating_panel_drag(id: impl Into<String>, pos: Signal<[f32; 2]>, x: f32, y: f32) {
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::FloatingPanel,
        id,
        x,
        y,
    );
    FLOATING_DRAG.with(|d| {
        *d.borrow_mut() = Some(FloatingPanelDrag {
            pos,
            origin: pos.peek(),
            start_pointer: [x, y],
        });
    });
}

pub fn is_floating_panel_dragging() -> bool {
    FLOATING_DRAG.with(|d| d.borrow().is_some())
}

pub fn tick_floating_panel_drag(x: f32, y: f32) -> bool {
    FLOATING_DRAG.with(|d| {
        let (pos, next) = {
            let guard = d.borrow();
            let Some(drag) = guard.as_ref() else {
                return false;
            };
            (
                drag.pos,
                [
                    drag.origin[0] + x - drag.start_pointer[0],
                    drag.origin[1] + y - drag.start_pointer[1],
                ],
            )
        };
        if pos.peek() != next {
            pos.set(next);
        }
        true
    })
}

pub fn clear_floating_panel_drag() {
    let had = FLOATING_DRAG.with(|d| d.borrow_mut().take().is_some());
    if had {
        crate::utils::pointer_drag::clear();
        crate::app::release_pointer_capture();
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::ATTR_PORTAL_KEY;
    use crate::utils::popup::{ATTR_FLOATING, ATTR_FLOATING_POINT, ATTR_FLOATING_TRIGGER};

    fn is_zero_len(el: &Element) -> bool {
        crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0)
    }

    fn find_attr<'a>(el: &'a Element, key: &str) -> Option<&'a str> {
        el.attrs
            .get(key)
            .map(String::as_str)
            .or_else(|| el.children.iter().find_map(|c| find_attr(c, key)))
    }

    #[test]
    fn builder_and_opts_share_scope_template() {
        let (_o, (a, b)) = create_root(|| {
            let open = use_signal("ao-dual-open", || false);
            let a = anchored_overlay("ao-dual")
                .open(open)
                .placement(Placement::Unanchored)
                .child(|| Element::new().text("a"));
            let b = anchored_overlay_scope("ao-dual-opts", Some(open), || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![anchored_overlay_content(
                    AnchoredOverlayContentOpts::default().placement(Placement::Unanchored),
                    || Element::new().text("b"),
                )])
            });
            (a, b)
        });
        assert_eq!(a.template_id.as_deref(), Some("ao-dual__anchored_overlay"));
        assert_eq!(
            b.template_id.as_deref(),
            Some("ao-dual-opts__anchored_overlay")
        );
        assert!(is_zero_len(&a) || a.children.iter().any(is_zero_len));
        fn find_view(el: &Element, suffix: &str) -> bool {
            el.template_id
                .as_deref()
                .is_some_and(|t| t.ends_with(suffix))
                || el.children.iter().any(|c| find_view(c, suffix))
        }
        assert!(find_view(&a, "__view"), "one-shot must mount 0×0 view");
        assert!(find_view(&b, "__view"), "opts content must mount 0×0 view");
    }

    #[test]
    fn placement_trigger_marks_floating() {
        create_root(|| {
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-t",
                &Placement::Trigger {
                    focus_id: "btn".into(),
                    placement: PopupPlacement::BottomStart,
                    gap: FLOATING_TRIGGER_GAP,
                    strategy: FloatingStrategy::Fixed,
                },
                Signal::new(String::new()),
                "",
                false,
                Some("24rem"),
                None,
            );
            assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-t"));
            assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("btn"));
            assert_eq!(find_attr(&panel, ATTR_FLOATING_STRATEGY), Some("fixed"));
            assert_eq!(find_attr(&panel, ATTR_FLOATING_MAX_H), Some("24rem"));
            assert!(find_attr(&panel, ATTR_FLOATING_POINT).is_none());
        });
    }

    #[test]
    fn placement_width_auto_does_not_seed_explicit_w() {
        create_root(|| {
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-auto-w",
                &Placement::Trigger {
                    focus_id: "btn".into(),
                    placement: PopupPlacement::BottomStart,
                    gap: FLOATING_TRIGGER_GAP,
                    strategy: FloatingStrategy::Fixed,
                },
                Signal::new(String::new()),
                "",
                false,
                None,
                Some("auto"),
            );
            assert_eq!(find_attr(&panel, ATTR_FLOATING_MATCH_TRIGGER), Some("0"));
            assert!(
                find_attr(&panel, ATTR_FLOATING_W).is_none(),
                "width=auto must not write rui_floating_w (sync would parse it as trigger px)"
            );
        });
    }

    #[test]
    fn placement_point_drops_trigger_attr() {
        create_root(|| {
            let pos = Signal::new([120.0_f32, 80.0]);
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-p",
                &Placement::Point { pos, gap: 0.0 },
                Signal::new(String::new()),
                "",
                false,
                None,
                None,
            );
            assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-p"));
            assert_eq!(find_attr(&panel, ATTR_FLOATING_POINT), Some("120,80"));
            assert!(
                find_attr(&panel, ATTR_FLOATING_TRIGGER).is_none(),
                "Point must not look like a trigger hit-target"
            );
        });
    }

    #[test]
    fn placement_unanchored_identity_floating() {
        create_root(|| {
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-u",
                &Placement::Unanchored,
                Signal::new("hud-btn".into()),
                "",
                false,
                None,
                None,
            );
            assert_eq!(find_attr(&panel, ATTR_FLOATING), Some("ao-u"));
            assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("hud-btn"));
            assert!(find_attr(&panel, ATTR_FLOATING_POINT).is_none());
            assert!(
                find_attr(&panel, crate::utils::popup::ATTR_FLOATING_PLACEMENT).is_none(),
                "Unanchored must not set placement (sync would move author inset)"
            );
        });
    }

    #[test]
    fn placement_unanchored_prefers_content_focus_id() {
        create_root(|| {
            let mut panel = Element::new().text("p");
            apply_placement(
                &mut panel,
                "ao-u2",
                &Placement::Unanchored,
                Signal::new("default-trig".into()),
                "rsx-btn",
                false,
                None,
                None,
            );
            assert_eq!(find_attr(&panel, ATTR_FLOATING_TRIGGER), Some("rsx-btn"));
        });
    }

    #[test]
    fn open_unanchored_uses_zero_anchor_without_viewport_stage() {
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let presence = use_floating_presence("ao-open-u", FloatingMotionKind::Fade);
            let opts = AnchoredOverlayContentOpts::default()
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover);
            let mounted = assemble_panel(
                "ao-open-u",
                open,
                Signal::new("hud-btn".into()),
                &presence,
                &opts,
                Element::new().text("hud").fixed().top("1rem").right("1rem"),
            );
            assert!(
                is_zero_len(&mounted),
                "Unanchored keeps the same 0×0 document-flow mount as Trigger"
            );
            assert_eq!(
                find_attr(&mounted, ATTR_PORTAL_KEY),
                Some("panel:ao-open-u"),
                "portal key must be instance-unique, not panel:{{z}}"
            );
            assert_eq!(
                find_attr(&mounted, ATTR_FLOATING),
                Some("ao-open-u"),
                "identity ATTR_FLOATING so panel hits are not outside-press"
            );
            assert_eq!(
                find_attr(&mounted, ATTR_FLOATING_TRIGGER),
                Some("hud-btn"),
                "toggle focus_id must be registered for outside-press exemption"
            );
            assert!(
                find_attr(&mounted, crate::utils::popup::ATTR_FLOATING_PLACEMENT).is_none(),
                "must not place against the trigger"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn point_drag_updates_pos_signal() {
        create_root(|| {
            let pos = Signal::new([10.0_f32, 20.0]);
            begin_floating_panel_drag("ao-drag", pos, 10.0, 20.0);
            assert!(tick_floating_panel_drag(25.0, 40.0));
            assert_eq!(pos.peek(), [25.0, 40.0]);
            clear_floating_panel_drag();
            assert!(!is_floating_panel_dragging());
        });
    }

    #[test]
    fn smoke_phase_hides_presence_handle() {
        use crate::utils::presence::PresencePhase;
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let opts = AnchoredOverlayContentOpts::default()
                .kind(OverlayKind::Popover)
                .motion(FloatingMotionKind::Fade);
            assert_eq!(
                anchored_overlay_phase("ao-phase", open, &opts),
                Some(PresencePhase::Present)
            );
            open.set(false);
            assert_eq!(
                anchored_overlay_phase("ao-phase", open, &opts),
                Some(PresencePhase::Exiting)
            );
            assert!(anchored_overlay_is_mounted("ao-phase", &opts));
        });
        clear_overlay_stack();
    }

    fn has_portal(el: &Element) -> bool {
        el.attrs.get("rui_portal").map(String::as_str) == Some("1")
            || el.children.iter().any(has_portal)
    }

    fn find_view(el: &Element, suffix: &str) -> bool {
        el.template_id
            .as_deref()
            .is_some_and(|t| t.ends_with(suffix))
            || el.children.iter().any(|c| find_view(c, suffix))
    }

    #[test]
    fn wave2_canary_open_island_does_not_subscribe_ui_root() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::core::reactivity::{
            clear_ui_root_effect, create_effect, dispose_effect, mark_next_effect_as_ui_root,
            ui_root_effect_dep_count,
        };
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        reset_registered_anchors();
        clear_overlay_stack();
        clear_ui_root_effect();
        let open = Signal::new(false);
        let runs = Rc::new(Cell::new(0));
        let kept = Rc::new(RefCell::new(None::<Element>));
        let runs_e = Rc::clone(&runs);
        let kept_e = Rc::clone(&kept);

        mark_next_effect_as_ui_root();
        let root = create_effect(move || {
            runs_e.set(runs_e.get() + 1);
            let el = anchored_overlay("ao-w2-root")
                .open(open)
                .placement(Placement::Unanchored)
                .child(|| Element::new().text("x").w(160.0).h(80.0));
            *kept_e.borrow_mut() = Some(el);
        });

        assert_eq!(runs.get(), 1);
        assert_eq!(
            ui_root_effect_dep_count(),
            0,
            "open must subscribe the structure island, not the UI root"
        );
        {
            let mounted = kept.borrow();
            let el = mounted.as_ref().expect("mounted");
            assert!(find_view(el, "__view"));
        }

        open.set(true);
        assert_eq!(runs.get(), 1, "open must not rebuild the UI root");
        assert_eq!(ui_root_effect_dep_count(), 0);

        drop(kept.borrow_mut().take());
        dispose_effect(root);
        clear_ui_root_effect();
        reset_registered_anchors();
        clear_overlay_stack();
    }

    #[test]
    fn wave2_canary_close_keeps_portal_while_exiting() {
        use crate::utils::presence::PresencePhase;

        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let presence = use_floating_presence("ao-w2-exit", FloatingMotionKind::Fade);
            let opts = AnchoredOverlayContentOpts::default()
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover);
            let trigger = Signal::new("hud-btn".into());
            let panel = || Element::new().text("hud").w(200.0).h(80.0);
            let mounted = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
            assert!(has_portal(&mounted), "open must portal the panel");
            assert_eq!(
                anchored_overlay_phase("ao-w2-exit", open, &opts),
                Some(PresencePhase::Present)
            );

            open.set(false);
            assert_eq!(
                anchored_overlay_phase("ao-w2-exit", open, &opts),
                Some(PresencePhase::Exiting)
            );
            assert!(anchored_overlay_is_mounted("ao-w2-exit", &opts));
            let exiting = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
            assert!(
                has_portal(&exiting),
                "exit must keep the portal until the gate completes"
            );

            presence.force_absent_flags();
            floating_mounted_signal(&presence).set(false);
            assert!(!anchored_overlay_is_mounted("ao-w2-exit", &opts));
            let gone = assemble_panel("ao-w2-exit", open, trigger, &presence, &opts, panel());
            assert!(
                !has_portal(&gone),
                "Absent must drop the portal after exit completes"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn wave2_canary_first_open_layouts_portal_panel() {
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
        };

        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let presence = use_floating_presence("ao-w2-first", FloatingMotionKind::Fade);
            let opts = AnchoredOverlayContentOpts::default()
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover);
            let mounted = assemble_panel(
                "ao-w2-first",
                open,
                Signal::new("hud-btn".into()),
                &presence,
                &opts,
                Element::new()
                    .text("wave2-ao")
                    .w(200.0)
                    .h(80.0)
                    .bg([1.0, 1.0, 1.0, 1.0])
                    .fixed()
                    .top("1rem")
                    .right("1rem"),
            );
            Element::new()
                .w(800.0)
                .h(600.0)
                .child(Element::new().text("page"))
                .child(mounted)
        });

        assert!(
            hoist_portal_subtrees(&mut root) >= 1,
            "first open must hoist the shell onto the portal layer"
        );
        assert!(
            root.children
                .iter()
                .any(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID)),
            "portal layer must exist after hoist"
        );
        assert_eq!(find_attr(&root, ATTR_HOISTED_PORTAL), Some("1"));

        let mut tree = LayoutTree::new();
        let _result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        fn find_hoisted(el: &Element) -> Option<&Element> {
            if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1") {
                return Some(el);
            }
            el.children.iter().find_map(find_hoisted)
        }
        let hoisted = find_hoisted(&root).expect("hoisted portal panel");
        fn subtree_has_sized_box(el: &Element) -> bool {
            (el.layout_bounds[2] > 1.0 && el.layout_bounds[3] > 1.0)
                || el.children.iter().any(subtree_has_sized_box)
        }
        assert!(
            subtree_has_sized_box(hoisted),
            "first open must layout the portal panel, not skip as paint-only; hoisted bounds={:?}",
            hoisted.layout_bounds
        );
        clear_overlay_stack();
    }

    #[test]
    fn fade_popover_join_stamp_matches_band_above_leftover_datepicker() {
        use crate::app::damage::{
            overlay_band_encode_flags, overlay_membership_paint_stamps, push_frame_paint_element,
            take_frame_paint_nodes, PaintDirtyKind,
        };
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, overlay_portal_stamp_key, LayoutTree, ATTR_HOISTED_PORTAL,
            PORTAL_LAYER_ID,
        };
        use crate::utils::floating_motion::{
            flush_floating_motion_subtree, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        let _ = take_frame_paint_nodes();
        let (_owner, mut root) = create_root(|| {
            let dp_open = Signal::new(true);
            let dp_presence = use_floating_presence("ao-dp-left", FloatingMotionKind::Expand);
            let dp_opts = AnchoredOverlayContentOpts::default()
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover)
                .motion(FloatingMotionKind::Expand);
            let datepicker = assemble_panel(
                "ao-dp-left",
                dp_open,
                Signal::new("dp-btn".into()),
                &dp_presence,
                &dp_opts,
                Element::new()
                    .text("calendar")
                    .w(280.0)
                    .h(320.0)
                    .bg([1.0, 1.0, 1.0, 1.0])
                    .fixed()
                    .top(344.0)
                    .left(64.0),
            );

            let cp_open = Signal::new(true);
            let cp_presence = use_floating_presence("ao-cp-fade", FloatingMotionKind::Fade);
            let cp_opts = AnchoredOverlayContentOpts::default()
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover)
                .motion(FloatingMotionKind::Fade)
                .width("17.5rem");
            let mut knob = Element::new().w(12.0).h(12.0).bg([1.0, 0.0, 0.0, 1.0]);
            knob = knob.absolute();
            let colorpicker = assemble_panel(
                "ao-cp-fade",
                cp_open,
                Signal::new("cp-btn".into()),
                &cp_presence,
                &cp_opts,
                Element::new()
                    .w(280.0)
                    .h(360.0)
                    .bg([1.0, 1.0, 1.0, 1.0])
                    .child(knob)
                    .fixed()
                    .top(120.0)
                    .left(640.0),
            );

            Element::new()
                .w(800.0)
                .h(600.0)
                .child(datepicker)
                .child(colorpicker)
        });

        assert!(hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        if sync_floating_placement(&mut root, 800.0, 600.0) {
            let _ = tree.build_and_layout_fast_ex(
                &mut root,
                800.0,
                600.0,
                [0.0, 0.0],
                false,
                false,
                None,
            );
        }
        let _ = flush_floating_motion_subtree(&mut root);
        let _ = tick_hoisted_floating_css(&mut root);
        let out =
            tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);

        fn find_tid<'a>(el: &'a Element, suffix: &str) -> Option<&'a Element> {
            if el
                .template_id
                .as_deref()
                .is_some_and(|t| t.ends_with(suffix))
            {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, suffix))
        }
        let cp = find_tid(&root, "ao-cp-fade__floating_panel")
            .expect("ColorPicker Fade panel must keep __floating_panel tid after hoist");
        assert!(
            cp.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                || find_tid(&root, PORTAL_LAYER_ID).is_some(),
            "ColorPicker Fade must live on the portal layer"
        );
        assert!(
            cp.layout_bounds[2] > 8.0 && cp.layout_bounds[3] > 8.0,
            "ColorPicker Fade box must be the popover, not 0×0; bounds={:?} opacity={}",
            cp.layout_bounds,
            cp.style.opacity
        );

        let panel_key = overlay_portal_stamp_key(cp);
        let band_keys: Vec<u64> = out
            .overlay_split
            .band_ranges(
                out.instances.len(),
                out.textured_draws.len(),
                out.text_nodes.len(),
                0,
                0,
            )
            .iter()
            .map(|b| b.stamp_key)
            .collect();
        assert!(
            band_keys.iter().any(|k| *k == panel_key),
            "portal-layer band stamp must be overlay_portal_stamp_key of the Fade panel; panel_key={panel_key} bands={band_keys:?}"
        );

        let _ = take_frame_paint_nodes();
        push_frame_paint_element(cp, [0.0, 0.0]);
        let dirties = take_frame_paint_nodes();
        let overlay_dirties: Vec<u64> = dirties
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::Overlay)
            .map(|d| d.key)
            .collect();
        assert_eq!(
            overlay_dirties,
            vec![panel_key],
            "Fade Overlay dirty must be the portal stamp, not tid/NodeId; dirties={dirties:?}"
        );
        let flags = overlay_band_encode_flags(&band_keys, &overlay_dirties, false, &[panel_key]);
        assert!(
            flags.iter().any(|e| *e),
            "ColorPicker Fade band must encode; bands={band_keys:?} flags={flags:?}"
        );

        let dp = find_tid(&root, "ao-dp-left__floating_panel");
        if let Some(dp) = dp {
            let dp_key = overlay_portal_stamp_key(dp);
            let color = cp.layout_bounds;
            let leftover = dp.layout_bounds;
            let joined =
                overlay_membership_paint_stamps(&[(dp_key, leftover)], &[(panel_key, color)]);
            assert_eq!(
                joined.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                vec![panel_key],
                "opening ColorPicker must Overlay-paint ColorPicker, not leftover DatePicker"
            );
        }
        clear_overlay_stack();
    }

    /// TabsContent dispose kills the overlay owner. The hoisted panel must still
    /// play Fade exit (`fade-out-0`) instead of being pruned on the same frame.
    #[test]
    fn tab_unmount_plays_configured_floating_exit() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::components::tabs::{tabs_content, tabs_scope};
        use crate::core::reactivity::{component_root, create_effect, create_root, use_signal};
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::floating_motion::{
            drain_pending_floating_unmounts, flush_floating_motion_subtree,
            mark_floating_panels_layout_dirty, tick_floating_presence, tick_hoisted_floating_css,
        };
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, tab) = create_root(|| {
            let tab = use_signal("ao-tab-exit__tabs", || "a".to_string());
            let live_e = Rc::clone(&live);
            let tab_e = tab;
            create_effect(move || {
                *live_e.borrow_mut() =
                    tabs_scope("ao-tab-exit", Some(tab_e), "a", "horizontal", false, || {
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag = frag.children(vec![
                            tabs_content("a", || {
                                component_root(|| {
                                    anchored_overlay("ao-tab-a")
                                        .open(Signal::new(true))
                                        .placement(Placement::Unanchored)
                                        .motion(FloatingMotionKind::Fade)
                                        .child(|| {
                                            Element::new()
                                                .text("keep-me")
                                                .w(200.0)
                                                .h(80.0)
                                                .bg([1.0, 1.0, 1.0, 1.0])
                                        })
                                })
                            }),
                            tabs_content("b", || component_root(|| Element::new().text("other"))),
                        ]);
                        frag
                    });
            });
            tab
        });
        drain_deferred_view_remounts();

        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        fn has_anim(el: &Element, name: &str) -> bool {
            el.style.animations.iter().any(|a| a.name == name)
                || el.children.iter().any(|c| has_anim(c, name))
        }
        fn present_pass(root: &mut Element) {
            let _ = hoist_portal_subtrees(root);
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(root, 800.0, 600.0, [0.0, 0.0]);
            if sync_floating_placement(root, 800.0, 600.0) {
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
            let floating_needs = flush_floating_motion_subtree(root);
            let css_ticking = tick_hoisted_floating_css(root);
            if floating_needs || css_ticking {
                mark_floating_panels_layout_dirty(root, &mut tree);
                let _ = tree.build_and_layout_fast_ex(
                    root,
                    800.0,
                    600.0,
                    [0.0, 0.0],
                    false,
                    false,
                    None,
                );
            }
        }

        {
            let mut root = live.borrow_mut();
            present_pass(&mut root);
            present_pass(&mut root);
        }
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "ao-tab-a__floating_panel").is_some(),
                "open overlay must hoist the panel before the tab switch"
            );
        }

        tab.set("b".into());
        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(0.016);
        {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        {
            let tree = live.borrow();
            let panel = find_tid(&tree, "ao-tab-a__floating_panel")
                .expect("tab unmount must keep the hoisted panel for the configured exit");
            assert!(
                has_anim(panel, "fade-out-0"),
                "tab unmount must seed fade-out-0 instead of dropping the layer"
            );
        }

        for _ in 0..20 {
            let _ = tick_floating_presence();
            drain_pending_floating_unmounts();
            drain_deferred_view_remounts();
            let mut root = live.borrow_mut();
            present_pass(&mut root);
        }
        crate::style::utility::set_frame_dt(0.0);
        {
            let tree = live.borrow();
            assert!(
                find_tid(&tree, "ao-tab-a__floating_panel").is_none(),
                "panel must drop after the exit animation completes"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}
