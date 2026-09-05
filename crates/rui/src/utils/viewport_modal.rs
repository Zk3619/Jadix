//! Headless viewport modal shell ([`docs/components/overlay_shells.md`](../../../../docs/components/overlay_shells.md)).
//!
//! Underlay + viewport stage + presence / motion. Not in `prelude` —
//! `use rui::utils::viewport_modal::{…}`.

use std::sync::OnceLock;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::internal::modal_motion::{
    apply_modal_shell_motion, default_slide_axis_px, mounted_signal, sync_modal_presence,
    use_modal_presence,
};
use crate::components::internal::modal_shell::{
    build_modal_overlay, modal_sync_presence, ShellOverlayOpts as InternalOverlayOpts,
};
use crate::core::reactivity::{
    create_context, isolate_reads, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage, swallow_clicks};

pub use crate::components::internal::modal_motion::ModalSurfaceMotion;
pub use crate::components::internal::modal_shell::{
    DEFAULT_MODAL_OVERLAY_BLUR, DEFAULT_MODAL_OVERLAY_OPACITY,
};

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
    fn to_internal(&self) -> InternalOverlayOpts {
        InternalOverlayOpts::from_content_parts(
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

fn assemble_modal(
    id: &str,
    open: Signal<bool>,
    presence: &crate::components::internal::modal_motion::ModalPresenceHandle,
    overlay_opts: &ShellOverlayOpts,
    opts: ViewportModalContentOpts,
    body: Element,
) -> Element {
    let want = open.get();
    let phase = sync_modal_presence(presence, want);
    let mounted = mounted_signal(presence);
    if !mounted.get() {
        let _ = modal_sync_presence(id, open, mounted);
        return Element::new().w(0.0).h(0.0);
    }
    let Some(zs) = modal_sync_presence(id, open, mounted) else {
        return Element::new().w(0.0).h(0.0);
    };

    let mut overlay_el = build_modal_overlay(open, zs.underlay, &overlay_opts.to_internal());
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
    if apply_modal_shell_motion(
        id,
        &mut overlay_el,
        &mut panel,
        opts.motion,
        phase,
        presence,
        [0.0, 0.0],
        false,
        default_slide_axis_px(opts.motion),
    ) {
        crate::app::runtime::request_redraw();
    }
    stage = stage.child(panel);
    mount_modal_layer(overlay_el, mount_viewport_portal_stage(stage, zs.panel))
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
    use crate::layout::layout_tree::ATTR_PORTAL_KEY;

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

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }

    #[test]
    fn dialog_fade_seeds_overlay_fade_in_on_flush() {
        use crate::components::internal::modal_motion::flush_modal_shell_motion_subtree;
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
        use crate::components::internal::modal_motion::flush_modal_shell_motion_subtree;
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
        use crate::components::internal::modal_motion::flush_modal_shell_motion_subtree;
        use crate::layout::layout_tree::{hoist_portal_subtrees, PORTAL_LAYER_ID};
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
}
