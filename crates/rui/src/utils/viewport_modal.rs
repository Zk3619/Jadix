//! Headless viewport modal shell ([`docs/components/overlay_shells.md`](../../../../docs/components/overlay_shells.md)).
//!
//! Underlay + viewport stage + presence / motion. Not in `prelude` —
//! `use rui::utils::viewport_modal::{…}`.

use std::sync::OnceLock;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::{
    create_computed, create_context, isolate_reads, provide_context, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::modal_motion::{
    apply_modal_shell_motion, default_slide_axis_px, mounted_signal, sync_modal_presence,
    use_modal_presence, ModalPresenceHandle,
};
use crate::utils::modal_shell::ModalShellZs;
use crate::utils::modal_shell::{
    build_modal_overlay, modal_sync_presence, ShellOverlayOpts as ModalOverlayOpts,
};
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
            frag.set_attr("rui_fragment", "1");
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
            frag.set_attr("rui_fragment", "1");
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
        assemble_modal(
            &id,
            open,
            &presence,
            &overlay,
            opts,
            isolate_reads(|| body()),
        )
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
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
    let presence = presence.clone();
    let id = id.to_string();
    create_computed(move || {
        let _ = sync_modal_presence(&presence, open.get());
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
    mount_modal_layer(overlay_el, mount_viewport_portal_stage(stage, zs.panel))
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

#[cfg(test)]
pub(crate) fn viewport_modal_clear_pending_enter(id: &str) {
    crate::utils::modal_motion::clear_pending_modal_enter(&use_modal_presence(id));
}

#[cfg(test)]
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
        return Element::new();
    };
    let Some(portal) = portal else {
        return Element::new();
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
        mount_viewport_portal_stage(stage, portal.zs.panel)
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
    let Some(portal) = viewport_modal_portal_if_mounted(id, open, presence) else {
        return Element::new().w(0.0).h(0.0);
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
mod tests {
    use super::*;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::{ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT};

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

    fn has_portal(el: &Element) -> bool {
        el.attrs.get("rui_portal").map(String::as_str) == Some("1")
            || el.children.iter().any(has_portal)
    }

    fn portal_nests_portal(el: &Element) -> bool {
        let self_p = el.attrs.get("rui_portal").map(String::as_str) == Some("1");
        if self_p && el.children.iter().any(has_portal) {
            return true;
        }
        el.children.iter().any(portal_nests_portal)
    }

    fn find_view(el: &Element, suffix: &str) -> bool {
        el.template_id
            .as_deref()
            .is_some_and(|t| t.ends_with(suffix))
            || el.children.iter().any(|c| find_view(c, suffix))
    }

    #[test]
    fn builder_and_opts_share_scope_template() {
        let (_o, (a, b)) = create_root(|| {
            let open = use_signal("vm-dual-open", || false);
            let a = viewport_modal("vm-dual")
                .open(open)
                .child(|| Element::new().text("a"));
            let b = viewport_modal_scope(
                "vm-dual-opts",
                Some(open),
                ShellOverlayOpts::default(),
                || {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![viewport_modal_content(
                        ViewportModalContentOpts::default(),
                        || Element::new().text("b"),
                    )])
                },
            );
            (a, b)
        });
        assert_eq!(a.template_id.as_deref(), Some("vm-dual__viewport_modal"));
        assert_eq!(
            b.template_id.as_deref(),
            Some("vm-dual-opts__viewport_modal")
        );
        assert!(find_view(&a, "__view"));
        assert!(find_view(&b, "__view"));
        assert!(is_zero_len(&a) || a.children.iter().any(is_zero_len));
    }

    #[test]
    fn open_modal_uses_viewport_stage_not_nested_portal() {
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-open");
            let mounted = assemble_modal(
                "vm-open",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default(),
                Element::new().text("card").w("16rem").h("8rem"),
            );
            assert!(has_portal(&mounted), "underlay + stage must portal");
            assert!(
                !portal_nests_portal(&mounted),
                "stage must not wrap panel_block_clicks"
            );
            assert!(
                find_attr(&mounted, ATTR_PORTAL_KEY).is_some(),
                "viewport stage needs a portal key"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn layer_callback_sees_phase_not_handle() {
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ModalOverlayOpts::from_content_parts(None, None, true, None);
            let mut saw_phase = false;
            let mounted = viewport_modal_layer(
                "vm-layer",
                open,
                &overlay,
                ViewportModalLayerOpts::dialog_fade(),
                |phase| {
                    saw_phase = phase.is_some();
                    Some(Element::new().text("card").w("16rem").h("8rem"))
                },
            );
            assert!(
                saw_phase,
                "open layer must pass PresencePhase, not a handle"
            );
            assert!(has_portal(&mounted), "layer must portal overlay + stage");
            assert!(!portal_nests_portal(&mounted));
        });
        clear_overlay_stack();
    }

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }

    #[test]
    fn dialog_fade_seeds_overlay_fade_in_on_flush() {
        use crate::utils::modal_motion::flush_modal_shell_motion_subtree;
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-fade");
            let mut mounted = assemble_modal(
                "vm-fade",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::DialogFade),
                Element::new().text("card").w("16rem").h("8rem"),
            );
            assert!(
                find_tid(&mounted, "vm-fade__modal_zoom").is_some(),
                "DialogFade flush requires __modal_zoom or overlay stays at opacity 0"
            );
            assert!(flush_modal_shell_motion_subtree(&mut mounted));
            let ov = find_tid(&mounted, "vm-fade__modal_overlay").expect("overlay identity");
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "overlay must seed fade-in-0; otherwise first paint is opacity 0 and close gets stuck in Exiting"
            );

            open.set(false);
            let mut exiting = assemble_modal(
                "vm-fade",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::DialogFade),
                Element::new().text("card").w("16rem").h("8rem"),
            );
            assert!(flush_modal_shell_motion_subtree(&mut exiting));
            let ov = find_tid(&exiting, "vm-fade__modal_overlay").expect("overlay identity");
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "exit must seed fade-out-0 so ExitGate can finish; otherwise close is stuck"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn slide_from_right_seeds_offscreen_translate_on_flush() {
        use crate::utils::modal_motion::flush_modal_shell_motion_subtree;
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-slide");
            let mut mounted = assemble_modal(
                "vm-slide",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::SlideFromRight),
                Element::new().text("sheet").w("18rem").h("100%"),
            );
            assert!(flush_modal_shell_motion_subtree(&mut mounted));
            let panel = find_tid(&mounted, "vm-slide__modal_panel").expect("panel identity");
            assert!(
                panel.style.translate_pct[0] > 0.9,
                "SlideFromRight must start off-screen at 100%; got pct={}",
                panel.style.translate_pct[0]
            );
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-in-")),
                "panel enter must use slide-in keyframes"
            );
            let ov = find_tid(&mounted, "vm-slide__modal_overlay").expect("overlay identity");
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "slide overlay enter must seed fade-in-0 with the panel slide"
            );

            open.set(false);
            let mut exiting = assemble_modal(
                "vm-slide",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::SlideFromRight),
                Element::new().text("sheet").w("18rem").h("100%"),
            );
            assert!(flush_modal_shell_motion_subtree(&mut exiting));
            let ov = find_tid(&exiting, "vm-slide__modal_overlay").expect("overlay identity");
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "slide overlay exit must fade with the panel, not vanish a frame early"
            );
            let panel = find_tid(&exiting, "vm-slide__modal_panel").expect("panel identity");
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-out-")),
                "panel exit must slide via keyframes, not sit until ExitGate unmounts"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn slide_exit_seeds_before_and_after_portal_hoist() {
        use crate::layout::layout_tree::{hoist_portal_subtrees, PORTAL_LAYER_ID};
        use crate::utils::modal_motion::flush_modal_shell_motion_subtree;
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-hoist-slide");
            let mut root = Element::new().w(800.0).h(600.0).child(assemble_modal(
                "vm-hoist-slide",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::SlideFromRight),
                Element::new().text("sheet").w("18rem").h("100%"),
            ));
            assert!(hoist_portal_subtrees(&mut root) >= 1);
            let _ = flush_modal_shell_motion_subtree(&mut root);

            open.set(false);
            let exiting = assemble_modal(
                "vm-hoist-slide",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::SlideFromRight),
                Element::new().text("sheet").w("18rem").h("100%"),
            );
            root.children.retain(|c| {
                c.template_id.as_deref() == Some(PORTAL_LAYER_ID)
                    || c.attrs.get("rui_portal").is_none()
            });
            root.children.insert(0, exiting);
            assert!(hoist_portal_subtrees(&mut root) >= 1);
            assert!(flush_modal_shell_motion_subtree(&mut root));

            let panel = find_tid(&root, "vm-hoist-slide__modal_panel").expect("hoisted panel");
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-out-")),
                "hoist+rebuild must keep panel slide-out keyframes, otherwise the sheet sticks at the dock"
            );
            let ov = find_tid(&root, "vm-hoist-slide__modal_overlay").expect("hoisted overlay");
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "overlay fade-out must survive hoist"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn phase_api_hides_presence_handle() {
        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            assert_eq!(
                viewport_modal_phase("vm-phase", open),
                Some(PresencePhase::Present)
            );
            assert!(viewport_modal_is_mounted("vm-phase", open));
            open.set(false);
            assert_eq!(
                viewport_modal_phase("vm-phase", open),
                Some(PresencePhase::Exiting)
            );
            assert!(viewport_modal_is_mounted("vm-phase", open));
        });
        clear_overlay_stack();
    }

    #[test]
    fn parse_motion_sheet_and_dialog() {
        assert_eq!(
            parse_modal_motion("dialog-fade"),
            ModalSurfaceMotion::DialogFade
        );
        assert_eq!(
            parse_modal_motion("slide-right"),
            ModalSurfaceMotion::SlideFromRight
        );
        assert_eq!(
            parse_modal_motion("sheet-slide"),
            ModalSurfaceMotion::SlideFromRight
        );
    }

    #[test]
    fn open_dialog_fade_keeps_page_glyphs_under_overlay_chrome() {
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::modal_motion::{
            flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
        };

        clear_overlay_stack();
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts {
                opacity: Some(0.28),
                ..Default::default()
            };
            let presence = use_modal_presence("gal-vm-dlg-rsx");
            let panel = Element::new()
                .w(320.0)
                .h(160.0)
                .bg(crate::tokens::card())
                .rounded(12.0)
                .child(Element::new().text("ViewportModal · RSX"));
            let mounted = assemble_modal(
                "gal-vm-dlg-rsx",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::DialogFade),
                panel,
            );
            Element::new()
                .w(1280.0)
                .h(800.0)
                .bg([0.96, 0.96, 0.96, 1.0])
                .child(Element::new().text("before"))
                .child(mounted)
                .child(Element::new().text("SheetSlide · Builder"))
        });

        assert!(
            hoist_portal_subtrees(&mut root) >= 1,
            "underlay + stage must hoist onto the portal layer"
        );
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..30 {
            let _ = tick_hoisted_modal_css(&mut root);
        }

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.instance_start < result.instances.len(),
            "open ViewportModal must collect overlay rects after the page split"
        );

        let split_t = result.overlay_split.text_start.min(result.text_nodes.len());
        let split_i = result
            .overlay_split
            .instance_start
            .min(result.instances.len());
        let split_tex = result
            .overlay_split
            .textured_start
            .min(result.textured_draws.len());
        let texts: Vec<&str> = result.text_nodes.iter().map(|t| t.text.as_str()).collect();
        assert!(
            result.text_nodes[..split_t]
                .iter()
                .any(|t| t.text.contains("SheetSlide")),
            "page label after the modal must collect before the overlay split (otherwise it paints on the dialog); split={split_t} texts={texts:?}"
        );

        let overlay_inst = &result.instances[split_i..];
        let sizes: Vec<([f32; 2], f32)> = overlay_inst
            .iter()
            .map(|i| (i.rect_size, i.color[3]))
            .collect();
        let overlay_tex: Vec<&str> = result.textured_draws[split_tex..]
            .iter()
            .map(|d| d.texture_key.as_str())
            .collect();
        let title_in_overlay_text = result.text_nodes[split_t..]
            .iter()
            .any(|t| t.text.contains("ViewportModal"));
        let title_in_zoom_blit = overlay_tex.iter().any(|k| k.starts_with("rui_zoom_cap:"));
        assert!(
            title_in_overlay_text || title_in_zoom_blit,
            "dialog title must be overlay text or a zoom blit; split_t={split_t} texts={texts:?} overlay_tex={overlay_tex:?}"
        );
        assert!(
            overlay_inst
                .iter()
                .any(|i| i.rect_size[0] > 1000.0 && i.color[3] > 0.05),
            "fullscreen scrim missing or alpha≈0; overlay instances={sizes:?}"
        );
        assert!(
            overlay_inst
                .iter()
                .any(|i| { i.rect_size[0] > 200.0 && i.rect_size[0] < 400.0 && i.color[3] > 0.5 }),
            "opaque panel fill missing in overlay suffix; overlay instances={sizes:?}"
        );
        let overlay_i0 = split_i as u32;
        for op in &result.paint_ops[..result.overlay_split.paint_op_start] {
            if let crate::layout::layout_tree::PaintOp::Rects { start, count } = *op {
                assert!(
                    start + count <= overlay_i0,
                    "page Rects swallowed overlay panel fills (page glyphs would paint on the dialog); op={start}+{count} overlay_i0={overlay_i0}"
                );
            }
        }

        let panel_idx = overlay_inst
            .iter()
            .position(|i| i.rect_size[0] > 200.0 && i.rect_size[0] < 400.0 && i.color[3] > 0.5);
        let panel_i = (split_i + panel_idx.expect("panel fill idx")) as u32;
        let overlay_ops = &result.paint_ops[result.overlay_split.paint_op_start..];
        let panel_in_ops = overlay_ops.iter().any(|op| match *op {
            crate::layout::layout_tree::PaintOp::Rects { start, count } => {
                start <= panel_i && panel_i < start + count
            }
            _ => false,
        });
        assert!(
            panel_in_ops,
            "panel fill is in overlay instances but dropped from overlay paint_ops (node-present Overlay draws ops only); ops={overlay_ops:?} panel_i={panel_i}"
        );

        let boxes = crate::layout::layout_tree::hoisted_portal_boxes(&root);
        let vp = [0.0_f32, 0.0, 1280.0, 800.0];
        assert!(
            boxes
                .iter()
                .any(|b| crate::app::damage::damage_covers_full(vp, *b)),
            "modal underlay/stage must cover the viewport so present FullClears and can frost; boxes={boxes:?}"
        );
        let bands = result.overlay_split.band_ranges(
            result.instances.len(),
            result.textured_draws.len(),
            result.text_nodes.len(),
            result.input_chrome_instances.len(),
            result.paint_ops.len(),
        );
        assert!(
            bands.iter().any(|b| b.backdrop_blur > 0.5),
            "underlay band must stamp backdrop_blur or FullClear frost is skipped; bands blur={:?}",
            bands.iter().map(|b| b.backdrop_blur).collect::<Vec<_>>()
        );
        let overlay_nodes: Vec<_> = boxes
            .iter()
            .copied()
            .enumerate()
            .map(|(i, bounds)| crate::app::damage::PaintDirty {
                key: 0xE0E0_0000 | i as u64,
                bounds,
                kind: crate::app::damage::PaintDirtyKind::Overlay,
            })
            .collect();
        assert_eq!(
            crate::app::damage::frame_clear_policy_for_nodes(true, false, &overlay_nodes, vp),
            crate::app::damage::FrameClearPolicy::FullClear,
            "open ViewportModal must FullClear so backdrop-blur frost runs; PartialLoad leaves a white wash and sharp page glyphs"
        );

        tree.set_collect_skip_allowed(true);
        let _ = tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [0.0, 0.0], false, false);
        let skipped =
            tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [0.0, 0.0], false, false);
        assert!(
            skipped.overlay_split.instance_start > 0
                && skipped.overlay_split.instance_start < skipped.instances.len(),
            "idle paint-only collect must keep overlay_split after the page; start={} n={}",
            skipped.overlay_split.instance_start,
            skipped.instances.len()
        );
        let split_i = skipped
            .overlay_split
            .instance_start
            .min(skipped.instances.len());
        let overlay_inst = &skipped.instances[split_i..];
        assert!(
            overlay_inst
                .iter()
                .any(|i| i.rect_size[0] > 1000.0 && i.color[3] > 0.05),
            "skip replay must keep the fullscreen scrim in the overlay suffix"
        );
        assert!(
            overlay_inst
                .iter()
                .any(|i| i.rect_size[0] > 200.0 && i.rect_size[0] < 400.0 && i.color[3] > 0.5),
            "skip replay must keep the opaque panel fill in the overlay suffix"
        );
        let bands = skipped.overlay_split.band_ranges(
            skipped.instances.len(),
            skipped.textured_draws.len(),
            skipped.text_nodes.len(),
            skipped.input_chrome_instances.len(),
            skipped.paint_ops.len(),
        );
        assert!(
            bands.iter().any(|b| b.backdrop_blur > 0.5),
            "skip replay must restamp underlay backdrop_blur; bands blur={:?}",
            bands.iter().map(|b| b.backdrop_blur).collect::<Vec<_>>()
        );

        clear_overlay_stack();
    }

    #[test]
    fn open_dialog_keeps_local_scrim_instance_blur() {
        use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
        use crate::utils::modal_motion::{
            flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
        };

        clear_overlay_stack();
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let (_owner, mut root) = create_root(|| {
            let mut local = Element::new()
                .w(200.0)
                .h(120.0)
                .bg([1.0, 1.0, 1.0, 0.55])
                .backdrop_blur(16.0);
            local.set_attr("rui_portal", "1");
            local.set_attr(ATTR_PORTAL_MOUNT, "local-host");
            local.set_attr(ATTR_PORTAL_KEY, "local-scrim");
            let host = Element::new()
                .relative()
                .w(220.0)
                .h(140.0)
                .overflow_x("hidden")
                .overflow_y("hidden")
                .bg([0.15, 0.39, 0.92, 1.0])
                .template_id("local-host")
                .child(Element::new().text("YELLOW 高对比 BLUR"))
                .child(local);
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts {
                opacity: Some(0.28),
                ..Default::default()
            };
            let presence = use_modal_presence("vm-over-local");
            let panel = Element::new()
                .w(320.0)
                .h(160.0)
                .bg(crate::tokens::card())
                .child(Element::new().text("ViewportModal · RSX"));
            let mounted = assemble_modal(
                "vm-over-local",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default().motion(ModalSurfaceMotion::DialogFade),
                panel,
            );
            Element::new().w(1280.0).h(800.0).child(host).child(mounted)
        });

        assert!(hoist_portal_subtrees(&mut root) >= 1);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..30 {
            let _ = tick_hoisted_modal_css(&mut root);
        }

        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [0.0, 0.0]);
        let split_i = result
            .overlay_split
            .instance_start
            .min(result.instances.len());
        assert!(
            result.instances[..split_i]
                .iter()
                .any(|i| i.backdrop_blur > 0.5),
            "local mount scrim must still stamp instance backdrop_blur under an open modal"
        );
        let bands = result.overlay_split.band_ranges(
            result.instances.len(),
            result.textured_draws.len(),
            result.text_nodes.len(),
            result.input_chrome_instances.len(),
            result.paint_ops.len(),
        );
        assert!(
            bands.iter().any(|b| b.backdrop_blur > 0.5),
            "modal underlay must still stamp overlay-band blur so frost stacks on the local scrim; bands={:?}",
            bands.iter().map(|b| b.backdrop_blur).collect::<Vec<_>>()
        );
        clear_overlay_stack();
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
            let el = viewport_modal("vm-w2-root")
                .open(open)
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
        use crate::utils::modal_motion::finish_modal_exit_for_test;

        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-w2-exit");
            let panel = || Element::new().text("card").w(320.0).h(200.0);
            let mounted = assemble_modal(
                "vm-w2-exit",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default(),
                panel(),
            );
            assert!(has_portal(&mounted), "open must portal overlay + stage");
            assert_eq!(
                viewport_modal_phase("vm-w2-exit", open),
                Some(PresencePhase::Present)
            );

            open.set(false);
            assert_eq!(
                viewport_modal_phase("vm-w2-exit", open),
                Some(PresencePhase::Exiting)
            );
            assert!(viewport_modal_is_mounted("vm-w2-exit", open));
            let exiting = assemble_modal(
                "vm-w2-exit",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default(),
                panel(),
            );
            assert!(
                has_portal(&exiting),
                "exit must keep the portal until the gate completes"
            );

            finish_modal_exit_for_test(&presence);
            assert!(!mounted_signal(&presence).get());
            let gone = assemble_modal(
                "vm-w2-exit",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default(),
                panel(),
            );
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
            let overlay = ShellOverlayOpts::default();
            let presence = use_modal_presence("vm-w2-first");
            let mounted = assemble_modal(
                "vm-w2-first",
                open,
                &presence,
                &overlay,
                ViewportModalContentOpts::default(),
                Element::new().text("wave2-panel").w(320.0).h(200.0),
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
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        assert!(
            result.overlay_split.instance_start < result.instances.len(),
            "first open must layout overlay rects, not skip as paint-only"
        );
        let split = result
            .overlay_split
            .instance_start
            .min(result.instances.len());
        assert!(
            result.instances[split..]
                .iter()
                .any(|i| i.rect_size[0] > 1.0 && i.rect_size[1] > 1.0),
            "portal panel must have non-zero laid-out bounds on first open"
        );
        clear_overlay_stack();
    }
}
