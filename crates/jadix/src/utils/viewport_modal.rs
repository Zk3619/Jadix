//! Headless viewport modal shell ([`docs/components/overlay_shells.md`](../../../../docs/components/overlay_shells.md)).
//!
//! Underlay + viewport stage + presence / motion. Not in `prelude` —
//! `use jadix::utils::viewport_modal::{…}`.

use std::sync::OnceLock;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized, reactive_view_sized};
use crate::core::reactivity::{
    create_computed, create_context, isolate_reads, provide_context, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::modal_motion::{
    apply_modal_shell_motion, bind_modal_open, default_slide_axis_px, mounted_signal,
    sync_modal_presence, use_modal_presence, ModalPresenceHandle,
};
use crate::utils::modal_shell::ModalShellZs;
use crate::utils::modal_shell::{
    build_modal_overlay, modal_sync_presence, ShellOverlayOpts as ModalOverlayOpts,
};
pub(crate) use crate::utils::popup::flow_hidden_portal_host;
use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage, swallow_clicks};
use crate::utils::presence::PresencePhase;

pub use crate::utils::modal_motion::ModalSurfaceMotion;
pub use crate::utils::modal_shell::{DEFAULT_MODAL_OVERLAY_BLUR, DEFAULT_MODAL_OVERLAY_OPACITY};

// ---------------------------------------------------------------------------
// Overlay opts (author-facing; converts to internal ShellOverlayOpts)
// ---------------------------------------------------------------------------

/// Scrim appearance for [`viewport_modal`]. `None` fields use Dialog/Sheet defaults.
#[derive(Clone, Debug)]
pub struct ShellOverlayOpts {
    pub color: Option<String>,
    pub opacity: Option<f32>,
    pub backdrop_blur: Option<f32>,
    pub close_on_click: bool,
}

impl Default for ShellOverlayOpts {
    fn default() -> Self {
        Self {
            color: None,
            opacity: None,
            backdrop_blur: None,
            close_on_click: true,
        }
    }
}

impl ShellOverlayOpts {
    fn to_internal(&self) -> ModalOverlayOpts {
        ModalOverlayOpts::from_content_parts(
            self.color.as_deref(),
            self.opacity,
            self.close_on_click,
            self.backdrop_blur,
        )
    }
}

pub fn parse_modal_motion(s: &str) -> ModalSurfaceMotion {
    ModalSurfaceMotion::from_attr(s)
}

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ViewportModalCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub overlay: ShellOverlayOpts,
}

fn modal_context() -> Context<ViewportModalCtx> {
    static CTX: OnceLock<Context<ViewportModalCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ViewportModalCtx>)
}

pub fn use_viewport_modal() -> Option<ViewportModalCtx> {
    use_context(modal_context())
}

fn require_viewport_modal(part: &str) -> ViewportModalCtx {
    use_viewport_modal().unwrap_or_else(|| {
        panic!("{part} must be used within <ViewportModal> / viewport_modal_scope");
    })
}

pub fn viewport_modal_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    overlay: ShellOverlayOpts,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "viewport_modal");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__vm_open").as_str(), || false));
    let mut root = with_child_owner(|| {
        provide_context(
            modal_context(),
            ViewportModalCtx {
                open,
                id: id.clone(),
                overlay,
            },
        );
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__viewport_modal"));
    }
    root
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct ViewportModalBuilder {
    id: String,
    open: Option<Signal<bool>>,
    overlay: ShellOverlayOpts,
    motion: ModalSurfaceMotion,
}

pub fn viewport_modal(id: impl Into<String>) -> ViewportModalBuilder {
    ViewportModalBuilder {
        id: id.into(),
        open: None,
        overlay: ShellOverlayOpts::default(),
        motion: ModalSurfaceMotion::DialogFade,
    }
}

impl ViewportModalBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    pub fn overlay(mut self, overlay: ShellOverlayOpts) -> Self {
        self.overlay = overlay;
        self
    }

    pub fn overlay_color(mut self, color: impl Into<String>) -> Self {
        self.overlay.color = Some(color.into());
        self
    }

    pub fn overlay_opacity(mut self, opacity: f32) -> Self {
        self.overlay.opacity = Some(opacity);
        self
    }

    pub fn overlay_blur(mut self, blur: f32) -> Self {
        self.overlay.backdrop_blur = Some(blur);
        self
    }

    pub fn close_on_click(mut self, yes: bool) -> Self {
        self.overlay.close_on_click = yes;
        self
    }

    pub fn motion(mut self, motion: ModalSurfaceMotion) -> Self {
        self.motion = motion;
        self
    }

    /// Panel body: shell wraps a viewport stage from [`Self::motion`] and
    /// [`swallow_clicks`]. Prefer this for Dialog-style cards / Sheet docks.
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let opts = ViewportModalContentOpts {
            motion: self.motion,
            raw_stage: false,
        };
        let overlay = self.overlay;
        viewport_modal_scope(self.id, self.open, overlay, move || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![viewport_modal_content(opts, body)])
        })
    }

    /// Full viewport stage. Author owns layout (center / dock / Command top-1/3)
    /// and must [`swallow_clicks`] the panel — do not [`crate::utils::popup::panel_block_clicks`].
    pub fn stage(self, body: impl Fn() -> Element + 'static) -> Element {
        let opts = ViewportModalContentOpts {
            motion: self.motion,
            raw_stage: true,
        };
        let overlay = self.overlay;
        viewport_modal_scope(self.id, self.open, overlay, move || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![viewport_modal_content(opts, body)])
        })
    }

    /// Alias of [`Self::child`].
    pub fn on_panel(self, body: impl Fn() -> Element + 'static) -> Element {
        self.child(body)
    }
}

// ---------------------------------------------------------------------------
// Content opts
// ---------------------------------------------------------------------------

/// Shared by Builder [`.child`](ViewportModalBuilder::child) / [`.stage`](ViewportModalBuilder::stage)
/// and RSX `<ViewportModalContent>`.
#[derive(Clone, Copy)]
pub struct ViewportModalContentOpts {
    pub motion: ModalSurfaceMotion,
    /// `true`: `body` is the full viewport stage (no auto wrap / swallow).
    pub raw_stage: bool,
}

impl Default for ViewportModalContentOpts {
    fn default() -> Self {
        Self {
            motion: ModalSurfaceMotion::DialogFade,
            raw_stage: false,
        }
    }
}

impl ViewportModalContentOpts {
    pub fn motion(mut self, motion: ModalSurfaceMotion) -> Self {
        self.motion = motion;
        self
    }

    pub fn raw_stage(mut self, yes: bool) -> Self {
        self.raw_stage = yes;
        self
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// Dialog / AlertDialog / Sheet / Drawer content island.
///
/// `reactive_view` defaults to a 100%×100% in-flow placeholder. Gallery RSX unpacks
/// Trigger+Content into the parent wrap HStack (`jadix_fragment` + `IntoElementList`),
/// so that leftover is a flex item: closed `Element::new()` is a 0×0 static (double
/// `gap` to the next trigger), open `absolute` 0×0 drops it. The clicked trigger's
/// neighbor spacing jumps, dest-reset punches its glyphs through the scrim, and
/// overlay StructureChanged skips enter/exit.
pub fn viewport_modal_island(
    view_id: impl Into<String>,
    render: impl FnMut() -> Element + 'static,
) -> Element {
    flow_hidden_portal_host(reactive_view_sized(view_id, 0.0, 0.0, render))
}

pub fn viewport_modal_content(
    opts: ViewportModalContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_viewport_modal("ViewportModalContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let overlay = ctx.overlay.clone();
    let view_id = format!("{id}__view");
    let presence = use_modal_presence(&id);

    reactive_anchor(view_id.clone(), move || {
        arm_modal_open_sync(&id, open, &presence);
        assemble_modal(&id, open, &presence, &overlay, opts, isolate_reads(&body))
    });
    flow_hidden_portal_host(reactive_placeholder_sized(view_id, 0.0, 0.0))
}

/// RSX `<ViewportModalContent>` entry (same implementation as [`viewport_modal_content`]).
pub fn viewport_modal_content_from_rsx(
    opts: ViewportModalContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    viewport_modal_content(opts, body)
}

/// Sync `open` → presence + portal z-index. `None` when the shell is not mounted.
pub(crate) struct ViewportModalPortal {
    pub phase: PresencePhase,
    pub zs: ModalShellZs,
}

/// `open.get()` lives here, not in the paint island. Tracking `open` in the view
/// rebuilds overlay/panel/zoom on close (StructureChanged + zoom recapture).
fn arm_modal_open_sync(id: &str, open: Signal<bool>, presence: &ModalPresenceHandle) {
    bind_modal_open(presence, open);
    let presence = presence.clone();
    let id = id.to_string();
    create_computed(move || {
        let want = open.try_get().unwrap_or(false);
        let _ = sync_modal_presence(&presence, want);
        let mounted = mounted_signal(&presence);
        let _ = modal_sync_presence(&id, open, mounted);
    });
}

fn viewport_modal_portal_if_mounted(
    id: &str,
    open: Signal<bool>,
    presence: &ModalPresenceHandle,
) -> Option<ViewportModalPortal> {
    let mounted = mounted_signal(presence);
    if !mounted.get() {
        let _ = modal_sync_presence(id, open, mounted);
        return None;
    }
    let zs = modal_sync_presence(id, open, mounted)?;
    Some(ViewportModalPortal {
        phase: presence.phase(),
        zs,
    })
}

pub(crate) fn sync_viewport_modal_portal(
    id: &str,
    open: Signal<bool>,
    presence: &ModalPresenceHandle,
) -> Option<ViewportModalPortal> {
    let _ = sync_modal_presence(presence, open.peek());
    viewport_modal_portal_if_mounted(id, open, presence)
}

pub(crate) fn apply_viewport_modal_motion(
    modal_id: &str,
    presence: &ModalPresenceHandle,
    motion: ModalSurfaceMotion,
    phase: PresencePhase,
    overlay_el: &mut Element,
    panel: &mut Element,
    extra_translate: [f32; 2],
    dragging: bool,
    slide_axis_px: f32,
) -> bool {
    apply_modal_shell_motion(
        modal_id,
        overlay_el,
        panel,
        motion,
        phase,
        presence,
        extra_translate,
        dragging,
        slide_axis_px,
    )
}

pub(crate) fn mount_viewport_modal_layer(
    overlay_el: Element,
    stage: Element,
    zs: &ModalShellZs,
) -> Element {
    // Portals hoist out; the leftover host must not be a wrapping flex item.
    flow_hidden_portal_host(mount_modal_layer(
        overlay_el,
        mount_viewport_portal_stage(stage, zs.panel),
    ))
}

/// Product chrome → overlay / motion / stage / mount. Callers see [`PresencePhase`], not a handle.
#[derive(Clone, Copy, Debug)]
pub struct ViewportModalLayerOpts {
    pub motion: ModalSurfaceMotion,
    pub extra_translate: [f32; 2],
    pub dragging: bool,
    pub slide_axis_px: f32,
    pub overlay: bool,
    pub center_stage: bool,
    pub stage_overflow_hidden: bool,
}

impl ViewportModalLayerOpts {
    pub fn dialog_fade() -> Self {
        Self {
            motion: ModalSurfaceMotion::DialogFade,
            extra_translate: [0.0, 0.0],
            dragging: false,
            slide_axis_px: 0.0,
            overlay: true,
            center_stage: true,
            stage_overflow_hidden: false,
        }
    }

    pub fn slide(motion: ModalSurfaceMotion, slide_axis_px: f32) -> Self {
        Self {
            motion,
            extra_translate: [0.0, 0.0],
            dragging: false,
            slide_axis_px,
            overlay: true,
            center_stage: false,
            stage_overflow_hidden: false,
        }
    }
}

/// Enter / CSS settle / exit — Command focus and Drawer page-scale.
pub fn viewport_modal_motion_busy(id: &str) -> bool {
    use_modal_presence(id).is_motion_busy()
}

/// Logical `open` may already be false; `true` while the exit animation is still mounted.
pub fn viewport_modal_is_mounted(id: &str, open: Signal<bool>) -> bool {
    let presence = use_modal_presence(id);
    let _ = sync_modal_presence(&presence, open.peek());
    mounted_signal(&presence).get()
}

/// Current portal phase. `None` when the shell is not mounted.
pub fn viewport_modal_phase(id: &str, open: Signal<bool>) -> Option<PresencePhase> {
    let presence = use_modal_presence(id);
    sync_viewport_modal_portal(id, open, &presence).map(|p| p.phase)
}

/// A responsive surface returning to normal flow must release its modal stack immediately.
pub fn dismiss_viewport_modal(id: &str, open: Signal<bool>) {
    if open.try_peek() == Some(true) {
        open.set(false);
    }
    let presence = use_modal_presence(id);
    crate::utils::modal_motion::finish_modal_exit_immediate(&presence);
    let _ = modal_sync_presence(id, open, mounted_signal(&presence));
}

/// Mount existing stateful content. The caller pumps presence each frame instead of
/// installing a new reactive effect every time the content changes presentation.
pub fn retained_viewport_modal_layer(
    id: &str,
    open: Signal<bool>,
    overlay: &ModalOverlayOpts,
    opts: ViewportModalLayerOpts,
    panel: Element,
) -> Element {
    let presence = use_modal_presence(id);
    bind_modal_open(&presence, open);
    let portal = sync_viewport_modal_portal(id, open, &presence)
        .expect("retained modal content is mounted only while open");
    finish_viewport_modal_layer(id, open, overlay, opts, &presence, portal, panel)
}

#[cfg(test)]
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn viewport_modal_clear_pending_enter(id: &str) {
    crate::utils::modal_motion::clear_pending_modal_enter(&use_modal_presence(id));
}

#[cfg(test)]
#[cfg(feature = "animation-base")]
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn viewport_modal_force_unmount(id: &str) {
    mounted_signal(&use_modal_presence(id)).set(false);
}

/// Sync presence + portal, then mount product `panel` (or empty when `body` returns `None`).
///
/// `body(None)` = not mounted (clear focus trap / session). `body(Some(phase))` returns the
/// chromed motion panel; the shell owns overlay, motion, stage, and mount.
pub fn viewport_modal_layer(
    id: &str,
    open: Signal<bool>,
    overlay: &ModalOverlayOpts,
    opts: ViewportModalLayerOpts,
    body: impl FnOnce(Option<PresencePhase>) -> Option<Element>,
) -> Element {
    let presence = use_modal_presence(id);
    arm_modal_open_sync(id, open, &presence);
    let _ = sync_modal_presence(&presence, open.peek());
    let portal = viewport_modal_portal_if_mounted(id, open, &presence);
    let phase = portal.as_ref().map(|p| p.phase);
    let Some(panel) = body(phase) else {
        return flow_hidden_portal_host(Element::new());
    };
    let Some(portal) = portal else {
        return flow_hidden_portal_host(Element::new());
    };
    finish_viewport_modal_layer(id, open, overlay, opts, &presence, portal, panel)
}

fn finish_viewport_modal_layer(
    id: &str,
    open: Signal<bool>,
    overlay: &ModalOverlayOpts,
    opts: ViewportModalLayerOpts,
    presence: &ModalPresenceHandle,
    portal: ViewportModalPortal,
    mut panel: Element,
) -> Element {
    let mut overlay_el = if opts.overlay {
        build_modal_overlay(open, portal.zs.underlay, overlay)
    } else {
        Element::new()
    };
    if apply_viewport_modal_motion(
        id,
        presence,
        opts.motion,
        portal.phase,
        &mut overlay_el,
        &mut panel,
        opts.extra_translate,
        opts.dragging,
        opts.slide_axis_px,
    ) {
        crate::app::runtime::request_redraw();
    }
    let mut stage = r#box().inset(0.0).w("100%").h("100%");
    if opts.center_stage {
        stage = stage.items_center().justify_center();
    }
    if opts.stage_overflow_hidden {
        stage = stage.overflow("hidden");
    }
    let stage: Element = stage.child(panel).into();
    if opts.overlay {
        mount_viewport_modal_layer(overlay_el, stage, &portal.zs)
    } else {
        // Don't stamp `.absolute()` onto the portal node itself (`absolute()`
        // clears `position_fixed`). Host is leftover after hoist.
        flow_hidden_portal_host(
            Element::new().child(mount_viewport_portal_stage(stage, portal.zs.panel)),
        )
    }
}

fn assemble_modal(
    id: &str,
    open: Signal<bool>,
    presence: &ModalPresenceHandle,
    overlay_opts: &ShellOverlayOpts,
    opts: ViewportModalContentOpts,
    body: Element,
) -> Element {
    let _ = sync_modal_presence(presence, open.peek());
    bind_modal_open(presence, open);
    let Some(portal) = viewport_modal_portal_if_mounted(id, open, presence) else {
        return flow_hidden_portal_host(Element::new());
    };

    let mut overlay_el = build_modal_overlay(open, portal.zs.underlay, &overlay_opts.to_internal());
    let mut stage = if opts.raw_stage {
        body
    } else {
        wrap_panel_in_stage(opts.motion, body)
    };

    let mut panel = if stage.children.is_empty() {
        Element::new()
    } else {
        stage.children.remove(0)
    };
    // DialogFade flush looks up `{id}__modal_zoom` (same as Dialog). Without it
    // overlay stays at opacity 0; a later rebuild shows the scrim but ExitGate
    // never completes, so overlay click / close become no-ops.
    panel = wrap_dialog_fade_zoom(id, opts.motion, panel);
    if apply_viewport_modal_motion(
        id,
        presence,
        opts.motion,
        portal.phase,
        &mut overlay_el,
        &mut panel,
        [0.0, 0.0],
        false,
        default_slide_axis_px(opts.motion),
    ) {
        crate::app::runtime::request_redraw();
    }
    stage = stage.child(panel);
    mount_viewport_modal_layer(overlay_el, stage, &portal.zs)
}

fn wrap_dialog_fade_zoom(modal_id: &str, motion: ModalSurfaceMotion, panel: Element) -> Element {
    if motion != ModalSurfaceMotion::DialogFade {
        return panel;
    }
    let mut zoom = panel;
    zoom.template_id = Some(format!("{modal_id}__modal_zoom"));
    vstack().flex_shrink(0.0).child(zoom).into()
}

fn wrap_panel_in_stage(motion: ModalSurfaceMotion, mut panel: Element) -> Element {
    panel = dock_for_motion(motion, panel);
    panel = swallow_clicks(panel);
    let mut stage = r#box().inset(0.0).w("100%").h("100%");
    if motion == ModalSurfaceMotion::DialogFade {
        stage = stage.items_center().justify_center();
    }
    stage.child(panel).into()
}

fn dock_for_motion(motion: ModalSurfaceMotion, panel: Element) -> Element {
    match motion {
        ModalSurfaceMotion::DialogFade => panel,
        ModalSurfaceMotion::SlideFromRight => panel.absolute().top(0.0).right(0.0).bottom(0.0),
        ModalSurfaceMotion::SlideFromLeft => panel.absolute().top(0.0).left(0.0).bottom(0.0),
        ModalSurfaceMotion::SlideFromTop => panel.absolute().top(0.0).left(0.0).right(0.0),
        ModalSurfaceMotion::SlideFromBottom => panel.absolute().bottom(0.0).left(0.0).right(0.0),
    }
}

#[cfg(test)]
#[path = "tests/viewport_modal.rs"]
mod tests;
