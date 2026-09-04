//! # AlertDialog
//!
//! ShadCN-style modal confirmation. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/alert-dialog.tsx` — blocking overlay,
//! explicit Cancel / Action (no corner close; overlay click off by default).
//!
//! ```text
//! AlertDialog
//! ├── AlertDialogTrigger
//! └── AlertDialogContent          // Portal + Overlay + 面板（单层壳，同 DialogContent）
//!     ├── AlertDialogHeader
//!     │   ├── AlertDialogMedia?   // optional icon / illustration
//!     │   ├── AlertDialogTitle
//!     │   └── AlertDialogDescription
//!     ├── AlertDialogBody?        // 可选滚动区（同 DialogBody）
//!     └── AlertDialogFooter
//!         ├── AlertDialogCancel   // outline · closes on click
//!         └── AlertDialogAction   // primary action slot
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! let open = use_signal("confirm_open", || false);
//! ui! {
//!     <AlertDialog id="delete" open={open}>
//!         <AlertDialogTrigger>
//!             <Button variant="destructive">"Delete account"</Button>
//!         </AlertDialogTrigger>
//!         <AlertDialogContent>
//!             <AlertDialogHeader>
//!                 <AlertDialogTitle>"Are you absolutely sure?"</AlertDialogTitle>
//!                 <AlertDialogDescription>
//!                     "This action cannot be undone."
//!                 </AlertDialogDescription>
//!             </AlertDialogHeader>
//!             <AlertDialogFooter>
//!                 <AlertDialogCancel>"Cancel"</AlertDialogCancel>
//!                 <AlertDialogAction>
//!                     <Button variant="destructive" on_click={move || open.set(false)}>
//!                         "Continue"
//!                     </Button>
//!                 </AlertDialogAction>
//!             </AlertDialogFooter>
//!         </AlertDialogContent>
//!     </AlertDialog>
//! }
//!
//! // B · Builder
//! Element::from(
//!     alert_dialog("delete")
//!         .open(open)
//!         .child(|| ui! { /* Trigger + Content */ })
//! )
//! ```
//!
//! ## Notes
//! - Reuses [`crate::components::dialog::dialog_shell_frame`] — **单层**面板壳（勿再套 `bg/border/p`）。
//! - 子树布局对齐 Dialog：`Header` / `Body` / `Footer` 并列于 Content 内。
//! - `<AlertDialog>` / `<AlertDialogContent>` are macro-expanded (`alert_dialog_scope` / `alert_dialog_content`).
//! - `<AlertDialogContent variant={sig.get()}>` binds via [`apply_bound_alert_dialog_variant`] (same as [`Alert`]).
//! - 语义变体 `default` / `destructive` / `info` / `success` / `warning`；`filled` / `chrome` 同 Alert。
//! - 语义变体时 Portal 遮罩随 [`semantic_modal_overlay_scrim`] 染色（显式 `overlay_color` 仍可覆盖）。
//! - 模态确认框：遮罩点击默认不关闭；Cancel 自动 `open.set(false)`；Action 由应用侧决定何时关闭。

use std::sync::OnceLock;

use crate::app::reactive::reactive_view;
use crate::components::alert::AlertVariant;
use crate::components::button::button;
use crate::components::dialog::dialog_shell_frame;
use crate::components::internal::modal_motion::{
    mounted_signal, sync_modal_presence, use_modal_presence,
};
use crate::components::internal::modal_shell::{
    modal_sync, sync_modal_overlay_scrim, ShellOverlayOpts,
};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    component_root, create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{
    apply_semantic_chrome_respecting_overrides, is_variant_style_overridden,
    reapply_class_color_overrides, semantic_chrome, set_variant_chrome_enabled,
    variant_chrome_enabled, SemanticChrome, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, VariantStyleField,
};
use crate::utils::presence::PresencePhase;
use crate::utils::sync_focus_trap;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_ALERT_DIALOG_VARIANT: &str = "rui_alert_dialog_variant";
const ATTR_SEMANTIC_FILLED: &str = "rui_semantic_filled";
const ATTR_MODAL_INSTANCE_ID: &str = "rui_modal_instance_id";
const ATTR_OVERLAY_COLOR: &str = "rui_modal_overlay_color";
const ATTR_OVERLAY_OPACITY: &str = "rui_modal_overlay_opacity";
const SLOT_ROOT: &str = "alert-dialog";
const SLOT_TRIGGER: &str = "alert-dialog-trigger";
const SLOT_CONTENT: &str = "alert-dialog-content";
const SLOT_HEADER: &str = "alert-dialog-header";
const SLOT_BODY: &str = "alert-dialog-body";
const SLOT_FOOTER: &str = "alert-dialog-footer";
const SLOT_MEDIA: &str = "alert-dialog-media";
const SLOT_TITLE: &str = "alert-dialog-title";
const SLOT_DESCRIPTION: &str = "alert-dialog-description";
const SLOT_ACTION: &str = "alert-dialog-action";
const SLOT_CANCEL: &str = "alert-dialog-cancel";

const WIDTH_DEFAULT: &str = "32rem";
const WIDTH_SM: &str = "24rem";

const TITLE_SIZE_PX: f32 = 18.0;
const TITLE_WEIGHT: u16 = 600;
const DESC_SIZE_PX: f32 = 14.0;
const DESC_WEIGHT: u16 = 400;

/// AlertDialog visual tone — alias of [`AlertVariant`] (shared semantic palette).
pub type AlertDialogVariant = AlertVariant;

fn alert_dialog_chrome(variant: AlertDialogVariant, filled: bool) -> SemanticChrome {
    semantic_chrome(SemanticChromeOpts {
        tone: variant.to_semantic(),
        filled,
        surface: SemanticSurface::Card,
        decor: SemanticChromeDecor::ELEVATED,
    })
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn is_title(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_TITLE)
}

fn is_description(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_DESCRIPTION)
}

fn is_icon_child(el: &Element) -> bool {
    if slot_of(el).is_some() {
        return false;
    }
    el.text_style.font_family.as_deref() == Some("lucide")
}

fn stamp_alert_dialog_text_paint(el: &mut Element, color: [u8; 4], size_px: f32, weight: u16) {
    el.text_style.color = Some(color);
    el.text_style.size = Some(size_px);
    el.text_style.weight = Some(weight);
}

fn paint_title_node_on_place(el: &mut Element, fg: [u8; 4]) {
    crate::core::tree::walk_elements_mut(el, |node| {
        node.text_style.color = Some(fg);
        if node.text.is_some() {
            stamp_alert_dialog_text_paint(node, fg, TITLE_SIZE_PX, TITLE_WEIGHT);
        }
    });
}

fn paint_description_node_on_place(el: &mut Element, fg: [u8; 4]) {
    crate::core::tree::walk_elements_mut(el, |node| {
        node.text_style.color = Some(fg);
        if node.text.is_some() {
            stamp_alert_dialog_text_paint(node, fg, DESC_SIZE_PX, DESC_WEIGHT);
        }
    });
}

fn paint_title_node(mut el: Element, fg: [u8; 4]) -> Element {
    if let Some(text) = el.text.take() {
        return build_text_node(
            text,
            TextNodeConfig {
                size_px: TITLE_SIZE_PX,
                weight: TITLE_WEIGHT,
                color: fg,
                ..Default::default()
            },
        );
    }
    paint_title_node_on_place(&mut el, fg);
    el
}

fn paint_description_node(mut el: Element, fg: [u8; 4]) -> Element {
    if let Some(text) = el.text.take() {
        return build_text_node(
            text,
            TextNodeConfig {
                size_px: DESC_SIZE_PX,
                weight: DESC_WEIGHT,
                color: fg,
                ..Default::default()
            },
        );
    }
    paint_description_node_on_place(&mut el, fg);
    el
}

fn paint_icon_child(mut el: Element, fg: [u8; 4]) -> Element {
    el = el.pointer_events_none().flex_shrink(0.0);
    if el.text.is_some() {
        el = el.text_color(fg);
    }
    el
}

fn paint_alert_dialog_child(el: Element, chrome: &SemanticChrome) -> Element {
    if is_title(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_title_node(el, chrome.fg)
        }
    } else if is_description(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_description_node(el, chrome.muted_fg)
        }
    } else if is_icon_child(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_icon_child(el, chrome.fg)
        }
    } else {
        el
    }
}

fn paint_alert_dialog_tree(el: &mut Element, chrome: &SemanticChrome) {
    let mut stack: Vec<*mut Element> = vec![el as *mut Element];
    while let Some(ptr) = stack.pop() {
        // SAFETY: exclusive subtree of `el`.
        let node = unsafe { &mut *ptr };
        if is_title(node) || is_description(node) || is_icon_child(node) {
            *node = paint_alert_dialog_child(std::mem::take(node), chrome);
            continue;
        }
        let n = node.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(node.children[i]));
        }
    }
}

fn alert_dialog_filled_from_attrs(attrs: &std::collections::HashMap<String, String>) -> bool {
    attrs
        .get(ATTR_SEMANTIC_FILLED)
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn repaint_alert_dialog_descendants(el: &mut Element, chrome: &SemanticChrome) {
    crate::core::tree::walk_elements_mut(el, |node| {
        if is_title(node) {
            if !is_variant_style_overridden(&node.attrs, VariantStyleField::Fg) {
                paint_title_node_on_place(node, chrome.fg);
            }
        } else if is_description(node) {
            if !is_variant_style_overridden(&node.attrs, VariantStyleField::Fg) {
                paint_description_node_on_place(node, chrome.muted_fg);
            }
        } else if is_icon_child(node) {
            if !is_variant_style_overridden(&node.attrs, VariantStyleField::Fg) {
                if node.text.is_some() {
                    node.text_style.color = Some(chrome.fg);
                }
                for child in &mut node.children {
                    if child.text.is_some() {
                        child.text_style.color = Some(chrome.fg);
                    }
                }
            }
        }
    });
}

fn sync_alert_dialog_variant_chrome(el: &mut Element, variant: AlertDialogVariant) {
    if !variant_chrome_enabled(&el.attrs) {
        return;
    }
    let filled = alert_dialog_filled_from_attrs(&el.attrs);
    let chrome = alert_dialog_chrome(variant, filled);
    apply_semantic_chrome_respecting_overrides(el, &chrome);
    repaint_alert_dialog_descendants(el, &chrome);
    let _ = reapply_class_color_overrides(el);
}

fn stamp_alert_dialog_shell_meta(
    el: &mut Element,
    modal_id: &str,
    variant: AlertDialogVariant,
    filled: bool,
    chrome: bool,
    overlay_color: Option<&str>,
    overlay_opacity: Option<f32>,
) {
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ALERT_DIALOG_VARIANT, variant.as_str());
    el.set_attr(ATTR_SEMANTIC_FILLED, if filled { "1" } else { "0" });
    el.set_attr(ATTR_MODAL_INSTANCE_ID, modal_id);
    if let Some(color) = overlay_color.filter(|s| !s.is_empty()) {
        el.set_attr(ATTR_OVERLAY_COLOR, color);
    }
    if let Some(opacity) = overlay_opacity {
        el.set_attr(ATTR_OVERLAY_OPACITY, opacity.to_string());
    }
    set_variant_chrome_enabled(el, chrome);
}

fn overlay_scrim_from_shell_attrs(el: &Element, variant: AlertDialogVariant) -> LayoutStatus {
    let Some(modal_id) = el.attrs.get(ATTR_MODAL_INSTANCE_ID).map(String::as_str) else {
        return LayoutStatus::Clean;
    };
    let overlay_color = el.attrs.get(ATTR_OVERLAY_COLOR).map(String::as_str);
    let overlay_opacity = el
        .attrs
        .get(ATTR_OVERLAY_OPACITY)
        .and_then(|s| s.parse::<f32>().ok());
    sync_modal_overlay_scrim(
        modal_id,
        variant.to_semantic(),
        overlay_color,
        overlay_opacity,
    )
}

fn find_alert_dialog_content_mut<'a>(el: &'a mut Element) -> Option<&'a mut Element> {
    crate::core::tree::find_in_tree_mut(el, |n| slot_of(n) == Some(SLOT_CONTENT))
}

fn find_alert_dialog_content_by_template<'a>(
    el: &'a mut Element,
    modal_id: &str,
) -> Option<&'a mut Element> {
    let tid = format!("{modal_id}__modal_zoom");
    crate::core::tree::find_in_tree_mut(el, |n| n.template_id.as_deref() == Some(tid.as_str()))
}

/// Apply variant chroming to an alert-dialog content shell and repaint title/description/icon slots.
pub fn apply_alert_dialog_variant_paint(
    el: &mut Element,
    variant: impl AsRef<str>,
) -> LayoutStatus {
    let variant = AlertDialogVariant::from(variant.as_ref());
    let prev = el
        .attrs
        .get(ATTR_ALERT_DIALOG_VARIANT)
        .map(|s| s.as_str())
        .unwrap_or("default");
    if prev == variant.as_str() {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ALERT_DIALOG_VARIANT, variant.as_str());
    sync_alert_dialog_variant_chrome(el, variant);
    LayoutStatus::StyleChanged.merge(overlay_scrim_from_shell_attrs(el, variant))
}

/// Solid-style in-place bind for `<AlertDialogContent chrome={sig.get()}>`.
pub fn apply_bound_alert_dialog_chrome(el: &mut Element, enabled: bool) -> LayoutStatus {
    let Some(shell) = find_alert_dialog_content_mut(el) else {
        return LayoutStatus::Clean;
    };
    let was = variant_chrome_enabled(&shell.attrs);
    set_variant_chrome_enabled(shell, enabled);
    if was == enabled {
        return LayoutStatus::Clean;
    }
    if enabled {
        let variant = shell
            .attrs
            .get(ATTR_ALERT_DIALOG_VARIANT)
            .or_else(|| shell.attrs.get(ATTR_VARIANT))
            .map(|s| s.as_str())
            .unwrap_or("default");
        sync_alert_dialog_variant_chrome(shell, AlertDialogVariant::from(variant));
    }
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<AlertDialogContent variant={sig.get()}>`.
pub fn apply_bound_alert_dialog_variant(
    el: &mut Element,
    variant: impl AsRef<str>,
) -> LayoutStatus {
    if let Some(shell) = find_alert_dialog_content_mut(el) {
        apply_alert_dialog_variant_paint(shell, variant)
    } else if slot_of(el) == Some(SLOT_CONTENT) {
        apply_alert_dialog_variant_paint(el, variant)
    } else {
        LayoutStatus::Clean
    }
}

// ---------------------------------------------------------------------------
// Context / Builder
// ---------------------------------------------------------------------------

/// Root context: `open` + stable instance `id`.
#[derive(Clone)]
pub struct AlertDialogCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn alert_dialog_context() -> Context<AlertDialogCtx> {
    static CTX: OnceLock<Context<AlertDialogCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AlertDialogCtx>)
}

/// Read the most recent [`AlertDialog`] context.
pub fn use_alert_dialog() -> Option<AlertDialogCtx> {
    use_context(alert_dialog_context())
}

/// Close the nearest alert dialog (no-op without context).
pub fn alert_dialog_close() {
    if let Some(ctx) = use_alert_dialog() {
        ctx.open.set(false);
    }
}

/// Open the nearest alert dialog (no-op without context).
pub fn alert_dialog_open() {
    if let Some(ctx) = use_alert_dialog() {
        ctx.open.set(true);
    }
}

/// Provide alert-dialog context and build the subtree (`ui!` / Builder).
pub fn alert_dialog_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "alert-dialog");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), || false));
    provide_context(
        alert_dialog_context(),
        AlertDialogCtx {
            open,
            id: id.clone(),
        },
    );
    let mut root = body();
    root.set_attr(ATTR_SLOT, SLOT_ROOT);
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__alert_dialog"));
    }
    root
}

pub struct AlertDialogBuilder {
    id: String,
    open: Option<Signal<bool>>,
}

/// Builder entry (ADR-9 surface B).
pub fn alert_dialog(id: impl Into<String>) -> AlertDialogBuilder {
    AlertDialogBuilder {
        id: id.into(),
        open: None,
    }
}

impl AlertDialogBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        alert_dialog_scope(self.id, self.open, body)
    }
}

fn require_alert_dialog(part: &str) -> AlertDialogCtx {
    use_alert_dialog().unwrap_or_else(|| {
        panic!("{part} must be used within <AlertDialog> / alert_dialog_scope");
    })
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn chain_click(mut el: Element, handler: impl FnMut() + 'static) -> Element {
    let prev = el.on_click.clone();
    let mut handler = handler;
    el = el.on_click(move || {
        if let Some(ref rc) = prev {
            rc.borrow_mut()();
        }
        handler();
    });
    el
}

// ---------------------------------------------------------------------------
// Content options / shell
// ---------------------------------------------------------------------------

/// Panel width preset (`data-size` on content).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertDialogSize {
    #[default]
    Default,
    Sm,
}

impl AlertDialogSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
        }
    }

    fn width(self) -> &'static str {
        match self {
            Self::Default => WIDTH_DEFAULT,
            Self::Sm => WIDTH_SM,
        }
    }
}

impl From<&str> for AlertDialogSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for AlertDialogSize {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AlertDialogSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            _ => Self::Default,
        })
    }
}

/// `AlertDialogContent` chrome (`ui!` / [`alert_dialog_content`]).
#[derive(Clone)]
pub struct AlertDialogContentOpts {
    pub overlay_color: Option<String>,
    pub overlay_opacity: Option<f32>,
    /// Default `false` — alert dialogs require explicit Cancel / Action.
    pub close_on_overlay_click: bool,
    pub overlay_blur: Option<f32>,
    pub size: AlertDialogSize,
    /// Semantic tone (`default` / `destructive` / `info` / `success` / `warning`).
    pub variant: AlertDialogVariant,
    /// Dynamic variant (`variant_signal={sig}`); overrides [`Self::variant`] each reactive pass.
    pub variant_signal: Option<Signal<String>>,
    /// Badge-style soft tint surface when `true`.
    pub filled: bool,
    /// Apply semantic chrome on the panel + title/description slots (default `true`).
    pub chrome: bool,
    /// Dynamic chrome toggle (`chrome_signal={sig}`).
    pub chrome_signal: Option<Signal<bool>>,
}

impl Default for AlertDialogContentOpts {
    fn default() -> Self {
        Self {
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: false,
            overlay_blur: None,
            size: AlertDialogSize::Default,
            variant: AlertDialogVariant::Default,
            variant_signal: None,
            filled: false,
            chrome: true,
            chrome_signal: None,
        }
    }
}

fn alert_shell_overlay_opts(
    opts: &AlertDialogContentOpts,
    variant: AlertDialogVariant,
) -> ShellOverlayOpts {
    ShellOverlayOpts::from_semantic_parts(
        variant.to_semantic(),
        opts.overlay_color.as_deref(),
        opts.overlay_opacity,
        opts.close_on_overlay_click,
        opts.overlay_blur,
    )
}

fn collect_focus_ids(el: &Element, out: &mut Vec<String>) {
    crate::core::tree::walk_elements(el, |el| {
        if el.focusable {
            if let Some(fid) = &el.focus_id {
                if !fid.is_empty() {
                    out.push(fid.clone());
                }
            }
        }
    });
}

/// Portal subtree runs in a reactive island — re-provide dialog ctx so Cancel/Action see the right `open`.
fn build_alert_dialog_body(open: Signal<bool>, id: &str, body: &dyn Fn() -> Element) -> Element {
    let id = id.to_string();
    component_root(|| {
        provide_context(alert_dialog_context(), AlertDialogCtx { open, id });
        body()
    })
}

/// Portal + overlay + centered panel; opened lazily via reactive view.
pub fn alert_dialog_content(
    opts: AlertDialogContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_alert_dialog("AlertDialogContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let view_id = format!("{id}__alert_dialog_view");
    let overlay_color = opts.overlay_color.clone();
    let overlay_opacity = opts.overlay_opacity;
    let close_on_overlay_click = opts.close_on_overlay_click;
    let overlay_blur = opts.overlay_blur;
    let size = opts.size;
    let variant = opts.variant;
    let variant_signal = opts.variant_signal;
    let filled = opts.filled;
    let chrome_enabled = opts.chrome;
    let chrome_signal = opts.chrome_signal;
    let presence = use_modal_presence(&id);
    let mounted = mounted_signal(&presence);
    let body = std::sync::Arc::new(body);

    reactive_view(view_id, move || {
        let want = open.get();
        let phase = sync_modal_presence(&presence, want);
        let owner = format!("alert-dialog:{id}");
        if !mounted.get() {
            let _ = modal_sync(&id, mounted);
            sync_focus_trap(&owner, false, "");
            return Element::new();
        }
        let Some(zs) = modal_sync(&id, mounted) else {
            sync_focus_trap(&owner, false, "");
            return Element::new();
        };

        let active_variant = variant_signal
            .map(|s| AlertDialogVariant::from(s.get().as_str()))
            .unwrap_or(variant);
        let active_chrome = chrome_signal.map(|s| s.get()).unwrap_or(chrome_enabled);

        let overlay = alert_shell_overlay_opts(
            &AlertDialogContentOpts {
                overlay_color: overlay_color.clone(),
                overlay_opacity,
                close_on_overlay_click,
                overlay_blur,
                ..Default::default()
            },
            active_variant,
        );

        let mut inner = build_alert_dialog_body(open, &id, body.as_ref());
        if active_chrome {
            let chrome = alert_dialog_chrome(active_variant, filled);
            paint_alert_dialog_tree(&mut inner, &chrome);
        }
        // 与 DialogContent 相同：Header / Body / Footer 并列，单层 shell 负责 bg/border/p。
        inner = inner.w("100%").min_h("0").flex_grow(1.0).gap("1rem");

        let mut ring = Vec::new();
        collect_focus_ids(&inner, &mut ring);
        if phase != PresencePhase::Present || ring.is_empty() {
            sync_focus_trap(&owner, false, "");
        } else {
            sync_focus_trap(&owner, true, ring.join(","));
        }

        let shell_chrome = if active_chrome {
            Some(alert_dialog_chrome(active_variant, filled))
        } else {
            None
        };

        let mut root = dialog_shell_frame(
            &id,
            open,
            &zs,
            inner,
            &overlay,
            None,
            false,
            &presence,
            phase,
            Some(size.width()),
            Some(SLOT_CONTENT),
            Some(size.as_str()),
            shell_chrome.as_ref(),
        );
        if let Some(shell) = find_alert_dialog_content_by_template(&mut root, &id) {
            stamp_alert_dialog_shell_meta(
                shell,
                &id,
                active_variant,
                filled,
                active_chrome,
                overlay_color.as_deref(),
                overlay_opacity,
            );
            if active_chrome {
                sync_alert_dialog_variant_chrome(shell, active_variant);
            }
        }
        root
    })
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Opens the alert dialog on click (component child or default label).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_alert_dialog("AlertDialogTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__trigger", ctx.id)
    } else {
        id
    };

    let mut el = match children {
        None => interactive(label(trigger_id.as_str()).text("Open").into())
            .on_click(move || open.set(true))
            .into(),
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            interactive(label(trigger_id.as_str()).text(t).into())
                .on_click(move || open.set(true))
                .into()
        }
        Some(c) => chain_click(c, move || open.set(true)),
    };
    el.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    el
}

/// Header cluster (title + description + optional media). Fixed; does not scroll.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w("0")
        .gap("0.375rem")
        .flex_shrink(0.0)
        .overflow_x("hidden")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_HEADER);
    el
}

/// Optional media / icon slot above the title.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogMedia(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_MEDIA);
    el
}

/// Alert title (`font-semibold` · variant foreground when chrome is on).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogTitle(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack().w("100%").min_w("0").children(kids).into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    el
}

/// Supporting copy (`text-sm` · semantic description tone when chrome is on).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogDescription(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = r#box()
        .w("100%")
        .min_w("0")
        .overflow_x("hidden")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    el
}

/// Optional scrollable body between header and footer (same role as [`crate::components::DialogBody`]).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogBody(children: Option<Element>) -> Element {
    let ctx = require_alert_dialog("AlertDialogBody");
    let child = children.unwrap_or_else(Element::new);
    let scroll_id = format!("{}__body", ctx.id);
    let mut el: Element = scroll_area(scroll_id.as_str())
        .overflow_x("hidden")
        .overflow_y("auto")
        .scrollbar_visibility("auto")
        .w("100%")
        .h("auto")
        .min_h("0")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .child(child)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_BODY);
    el
}

/// Footer action row (Cancel + Action). Fixed; does not scroll.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .w("100%")
        .gap("0.5rem")
        .justify("end")
        .flex_shrink(0.0)
        .wrap("wrap")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_FOOTER);
    el
}

/// Primary action slot (wrap a [`Button`] or custom control).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogAction(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .gap("0.5rem")
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ACTION);
    el
}

/// Dismiss control — chains `open.set(false)` on click. Plain text → outline [`Button`].
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDialogCancel(id: String, children: Option<Element>) -> Element {
    let children = crate::utils::children_none_if_vacuous(children);
    let ctx = require_alert_dialog("AlertDialogCancel");
    let open = ctx.open;
    let btn_id = if id.is_empty() {
        format!("{}__cancel", ctx.id)
    } else {
        id
    };

    let mut el = match children {
        None => button(btn_id.as_str())
            .variant("outline")
            .text("Cancel")
            .on_click(move || open.set(false))
            .into(),
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            button(btn_id.as_str())
                .variant("outline")
                .text(t)
                .on_click(move || open.set(false))
                .into()
        }
        Some(c) => chain_click(c, move || open.set(false)),
    };
    el.set_attr(ATTR_SLOT, SLOT_CANCEL);
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::internal::modal_shell::{
        default_modal_overlay_color, DEFAULT_MODAL_OVERLAY_BLUR, DEFAULT_MODAL_OVERLAY_OPACITY,
    };

    #[test]
    fn alert_dialog_semantic_overlay_differs_by_variant() {
        let neutral = alert_shell_overlay_opts(
            &AlertDialogContentOpts::default(),
            AlertDialogVariant::Default,
        );
        let warning = alert_shell_overlay_opts(
            &AlertDialogContentOpts::default(),
            AlertDialogVariant::Warning,
        );
        assert_ne!(neutral.color, warning.color);
    }

    #[test]
    fn alert_dialog_default_overlay_does_not_close_on_click() {
        let opts = alert_shell_overlay_opts(
            &AlertDialogContentOpts::default(),
            AlertDialogVariant::Default,
        );
        assert!(!opts.close_on_click);
        assert!(
            (opts.color[3] - DEFAULT_MODAL_OVERLAY_OPACITY).abs() < 0.001,
            "expected alpha {}",
            DEFAULT_MODAL_OVERLAY_OPACITY
        );
        assert!(
            (opts.backdrop_blur - DEFAULT_MODAL_OVERLAY_BLUR).abs() < 0.001,
            "expected blur {}",
            DEFAULT_MODAL_OVERLAY_BLUR
        );
        let _ = default_modal_overlay_color(None, None);
    }

    #[test]
    fn alert_dialog_size_widths() {
        assert_eq!(AlertDialogSize::Default.width(), WIDTH_DEFAULT);
        assert_eq!(AlertDialogSize::Sm.width(), WIDTH_SM);
        assert_eq!(AlertDialogSize::from("sm"), AlertDialogSize::Sm);
    }

    #[test]
    fn alert_dialog_title_slot_and_scope_root() {
        let title = AlertDialogTitle(AlertDialogTitleProps {
            children: Some(Element::new().text("Sure?")),
            base: None,
            ..Default::default()
        });
        assert_eq!(
            title.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_TITLE)
        );
        let root = alert_dialog_scope("test-scope", None, || Element::new());
        assert_eq!(
            root.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
    }

    #[test]
    fn alert_dialog_destructive_variant_chrome_attrs() {
        use crate::style::parse_color;

        let mut title = AlertDialogTitle(AlertDialogTitleProps {
            children: Some(Element::new().text("Sure?")),
            base: None,
            ..Default::default()
        });
        let mut desc = AlertDialogDescription(AlertDialogDescriptionProps {
            children: Some(Element::new().text("Details")),
            base: None,
            ..Default::default()
        });
        let chrome = alert_dialog_chrome(AlertDialogVariant::Destructive, false);
        paint_alert_dialog_tree(&mut title, &chrome);
        paint_alert_dialog_tree(&mut desc, &chrome);
        let mut shell = Element::new();
        stamp_alert_dialog_shell_meta(
            &mut shell,
            "test-modal",
            AlertDialogVariant::Destructive,
            false,
            true,
            None,
            None,
        );
        sync_alert_dialog_variant_chrome(&mut shell, AlertDialogVariant::Destructive);
        shell.children.push(title);
        shell.children.push(desc);
        shell.set_attr(ATTR_SLOT, SLOT_CONTENT);
        assert_eq!(
            shell
                .attrs
                .get(ATTR_ALERT_DIALOG_VARIANT)
                .map(String::as_str),
            Some("destructive")
        );
        assert_eq!(shell.style.border_color, parse_color("red-300"));
        let mut title_ok = false;
        let mut desc_ok = false;
        fn walk(
            el: &Element,
            title_fg: [u8; 4],
            desc_fg: [u8; 4],
            title_ok: &mut bool,
            desc_ok: &mut bool,
        ) {
            match el.text.as_deref() {
                Some("Sure?") => {
                    assert_eq!(el.text_style.color, Some(title_fg));
                    *title_ok = true;
                }
                Some("Details") => {
                    assert_eq!(el.text_style.color, Some(desc_fg));
                    *desc_ok = true;
                }
                _ => {}
            }
            for child in &el.children {
                walk(child, title_fg, desc_fg, title_ok, desc_ok);
            }
        }
        let title_fg = chrome.fg;
        let desc_fg = chrome.muted_fg;
        walk(&shell, title_fg, desc_fg, &mut title_ok, &mut desc_ok);
        assert!(title_ok);
        assert!(desc_ok);
    }

    #[test]
    fn apply_bound_alert_dialog_variant_updates_shell() {
        let mut shell = Element::new();
        shell.set_attr(ATTR_SLOT, SLOT_CONTENT);
        stamp_alert_dialog_shell_meta(
            &mut shell,
            "test-modal",
            AlertDialogVariant::Default,
            false,
            true,
            None,
            None,
        );
        sync_alert_dialog_variant_chrome(&mut shell, AlertDialogVariant::Default);
        assert_eq!(
            apply_bound_alert_dialog_variant(&mut shell, "warning"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            shell
                .attrs
                .get(ATTR_ALERT_DIALOG_VARIANT)
                .map(String::as_str),
            Some("warning")
        );
        assert_eq!(
            apply_bound_alert_dialog_variant(&mut shell, "warning"),
            LayoutStatus::Clean
        );
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = alert_dialog("x").child(|| Element::new()).into();
    }
}
