//! # Dialog
//!
//! 对齐 solid-ui / shadcn **复合 Dialog**：
//!
//! ```text
//! Dialog
//! ├── DialogTrigger          // 可选；无组件子节点时默认 Label
//! └── DialogContent          // Portal + Overlay + 面板；默认可调遮罩 / 关闭钮
//!     ├── DialogHeader / DialogTitle / DialogDescription
//!     ├── DialogBody
//! └── DialogFooter // Also available Button + open.set(false)
//! ```
//!
//! Modal-shell overlay / Portal z: see [`crate::utils::modal_shell`];
//! 面板表面在本模块内实现。确认框直接组合 Header/Footer，
//! Confirmation flows use [`crate::components::alert_dialog`]. `<Dialog>` / `<DialogContent>` in `ui!` are expanded by macros.
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <Dialog id="edit" open={open}>
//!         <DialogTrigger>
//!             <Button>Dialog</Button>
//!         </DialogTrigger>
//!         <DialogContent close_on_overlay_click={true}>
//!             <DialogHeader>
//!                 <DialogTitle>Edit Profile</DialogTitle>
//!                 <DialogDescription>Make changes…</DialogDescription>
//!             </DialogHeader>
//!             <DialogBody>{/* … */}</DialogBody>
//!             <DialogFooter>
//!                 <Button variant="outline" on_click={move || open.set(false)}>
//!                     Cancel
//!                 </Button>
//!                 <Button>Save</Button>
//!             </DialogFooter>
//!         </DialogContent>
//!     </Dialog>
//! }
//! ```

use std::sync::OnceLock;

use crate::components::base::SlotChrome;
use crate::components::button::button;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::impl_content_opts_chrome;
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::style::{apply_semantic_chrome_respecting_overrides, parse_text_color, SemanticChrome};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::modal_shell::ShellOverlayOpts;
use crate::utils::popup::swallow_clicks;
use crate::utils::presence::PresencePhase;
use crate::utils::sync_focus_trap;
use crate::utils::take_packed;
use crate::utils::text_node::build_named_icon_text_node;
use crate::utils::viewport_modal::{
    viewport_modal_island, viewport_modal_layer, ViewportModalLayerOpts,
};
use jadix_macros::component;

// ---------------------------------------------------------------------------
// Context / Builder
// ---------------------------------------------------------------------------

/// Root context: `open` + stable instance `id`.
#[derive(Clone)]
pub struct DialogCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn dialog_context() -> Context<DialogCtx> {
    static CTX: OnceLock<Context<DialogCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DialogCtx>)
}

/// Read the most recent [`Dialog`] context.
pub fn use_dialog() -> Option<DialogCtx> {
    use_context(dialog_context())
}

/// 关闭最近的 Dialog（无上下文时为 no-op）。
pub fn dialog_close() {
    if let Some(ctx) = use_dialog() {
        ctx.open.set(false);
    }
}

/// 打开最近的 Dialog（无上下文时为 no-op）。
pub fn dialog_open() {
    if let Some(ctx) = use_dialog() {
        ctx.open.set(true);
    }
}

/// 提供 Dialog 上下文并构建子树（`ui!` `<Dialog>` / Builder 共用）。
pub fn dialog_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "dialog");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), || false));
    provide_context(
        dialog_context(),
        DialogCtx {
            open,
            id: id.clone(),
        },
    );
    let mut root = body();
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__dialog"));
    }
    root
}

/// Builder：`.open(sig).child(|| …)` → [`dialog_scope`]。
pub struct DialogBuilder {
    id: String,
    open: Option<Signal<bool>>,
}

pub fn dialog(id: impl Into<String>) -> DialogBuilder {
    DialogBuilder {
        id: id.into(),
        open: None,
    }
}

impl DialogBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    /// Build Trigger / Content (and related) subtrees inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        dialog_scope(self.id, self.open, body)
    }
}

fn require_dialog(part: &str) -> DialogCtx {
    use_dialog().unwrap_or_else(|| {
        panic!("{part} must be used within <Dialog> / dialog_scope");
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
// Modal shell（面板表面；overlay / z 见 utils::modal_shell）
// ---------------------------------------------------------------------------

fn apply_dialog_shell_surface(mut panel: Element, hug_content: bool) -> Element {
    panel = panel
        .bg(tokens::background())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow(tokens::shadow_rgba(tokens::Elevation::Xl), 16.0)
        .max_h("85dvh");
    if hug_content {
        // CommandDialog：`p-0`，无 footer。卡片只负责圆角裁剪；高度上限在 hug
        // panel（`max_h:85dvh`）上变成确定高度后，Input（flex_shrink:0）之下的
        // CommandList 才能 flex 缩短。Taffy 在「auto 高 + overflow:hidden」上会把
        // 子项当滚动盒内容，列表仍按 max-h 300 排，多出的部分被卡片底边裁掉。
        panel = panel
            .p(0.0)
            .w("32rem")
            .max_w("100%")
            .flex_shrink(1.0)
            .min_h(0.0)
            .overflow_x("hidden")
            .overflow_y("hidden");
        panel.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    } else {
        panel = panel
            .p("1rem")
            .min_w(0.0)
            .max_w("100%")
            .w("100%")
            .overflow_y("hidden")
            .overflow_x("hidden");
        panel = crate::style::utility::with_default_class_list(panel, "sm:p-6");
    }
    panel
}

fn apply_semantic_dialog_shell_surface(
    mut panel: Element,
    hug_content: bool,
    chrome: &SemanticChrome,
) -> Element {
    apply_semantic_chrome_respecting_overrides(&mut panel, chrome);
    panel = panel.rounded("0.5rem").max_h("85dvh");
    if chrome.shadow.is_none() {
        panel = panel.shadow(tokens::shadow_rgba(tokens::Elevation::Xl), 16.0);
    }
    if hug_content {
        panel = panel
            .p(0.0)
            .w("32rem")
            .max_w("100%")
            .flex_shrink(1.0)
            .min_h(0.0)
            .overflow_x("hidden")
            .overflow_y("hidden");
        panel.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    } else {
        panel = panel
            .p("1rem")
            .min_w(0.0)
            .max_w("100%")
            .w("100%")
            .overflow_y("hidden")
            .overflow_x("hidden");
        panel = crate::style::utility::with_default_class_list(panel, "sm:p-6");
    }
    panel
}

/// Chromed motion panel (no portal / presence). Shell mounts via [`viewport_modal_layer`].
pub(crate) fn dialog_shell_panel(
    modal_id: &str,
    inner: Element,
    chrome_close: Option<Element>,
    hug_content: bool,
    panel_width: Option<&str>,
    content_slot: Option<&str>,
    content_size: Option<&str>,
    semantic_shell: Option<&SemanticChrome>,
    panel_chrome: SlotChrome,
) -> Element {
    let panel_w = panel_width.unwrap_or("32rem");

    let zoom_body: Element = if hug_content {
        // min_h:0 + shrink: CommandList (not the input) must absorb card `max_h: 85dvh`.
        vstack()
            .w("100%")
            .min_h(0.0)
            .flex_shrink(1.0)
            .child(inner)
            .into()
    } else {
        vstack().w("100%").min_h("0").child(inner).into()
    };
    // shadcn animates `DialogContent` itself — the card carrying bg / border / shadow.
    // Putting the animated layer *inside* the surface instead only scales the content, so the
    // card stays full size while its contents shrink: the "缩小态渲染异常" artifact, and on
    // exit you plainly watch the inner content animate inside a static panel.
    let mut card: Element = if hug_content {
        vstack()
            .w("100%")
            .min_h(0.0)
            .flex_shrink(1.0)
            .child(zoom_body)
            .into()
    } else {
        vstack().w("100%").min_h("0").child(zoom_body).into()
    };
    card = if let Some(chrome) = semantic_shell {
        apply_semantic_dialog_shell_surface(card, hug_content, chrome)
    } else {
        apply_dialog_shell_surface(card, hug_content)
    };
    card = panel_chrome.apply(card);
    let slot_name = content_slot.unwrap_or("dialog-content");
    card.set_attr("data-slot", slot_name);
    card = card.accesskit_role(if slot_name.contains("alert") {
        "alertdialog"
    } else {
        "dialog"
    });
    if let Some(size) = content_size {
        card.set_attr("data-size", size);
    }
    card.template_id = Some(format!("{modal_id}__modal_zoom"));
    // Close sits on the padded surface (`absolute top-4 right-4`), not in the header flow.
    if let Some(mut close) = chrome_close {
        close = close.absolute().top("1rem").right("1rem").z_index(2);
        card = card.relative().child(close);
    }
    // Swallow on the visible card only — `panel_block_clicks` would nest a second
    // `jadix_portal` under stage. The outer panel must shrink-wrap so the viewport
    // stage stays `box-none` outside the dialog (underlay close + centered layout).
    card = swallow_clicks(card);

    let panel: Element = if hug_content {
        // Stage is definite (100% viewport). Panel `max_h` makes the used height
        // `min(content, 85dvh)` so the list — not a fake footer — absorbs leftover space.
        vstack()
            .w(panel_w)
            .max_w("calc(100dvw - 2rem)")
            .max_h("85dvh")
            .min_h(0.0)
            .flex_shrink(0.0)
            .child(card)
            .into()
    } else {
        vstack()
            .w(panel_w)
            .max_w("calc(100dvw - 2rem)")
            .max_h("85dvh")
            .flex_shrink(0.0)
            .min_h("0")
            .child(card)
            .into()
    };

    panel
}

/// Test / Alert helper: chrome panel + [`viewport_modal_layer`].
#[cfg(test)]
pub(crate) fn dialog_shell_frame(
    modal_id: &str,
    open: Signal<bool>,
    inner: Element,
    overlay_opts: &ShellOverlayOpts,
    chrome_close: Option<Element>,
    hug_content: bool,
    panel_width: Option<&str>,
    content_slot: Option<&str>,
    content_size: Option<&str>,
    semantic_shell: Option<&SemanticChrome>,
    panel_chrome: SlotChrome,
) -> Element {
    viewport_modal_layer(
        modal_id,
        open,
        overlay_opts,
        ViewportModalLayerOpts::dialog_fade(),
        |_| {
            Some(dialog_shell_panel(
                modal_id,
                inner,
                chrome_close,
                hug_content,
                panel_width,
                content_slot,
                content_size,
                semantic_shell,
                panel_chrome,
            ))
        },
    )
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

/// solid-ui `DialogTrigger`：子节点为组件时挂载打开；纯文本则默认 [`Label`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_dialog("DialogTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__trigger", ctx.id)
    } else {
        id
    };

    match children {
        None => interactive(
            label(trigger_id.as_str())
                .text(crate::tr_default_zh("jadix.dialog.open", "Open", "打开"))
                .into(),
        )
        .on_click(move || open.set(true))
        .into(),
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            interactive(label(trigger_id.as_str()).text(t).into())
                .on_click(move || open.set(true))
                .into()
        }
        Some(c) => chain_click(c, move || open.set(true)),
    }
}

/// `DialogContent` 遮罩与 chrome 选项（`ui!` / [`dialog_content`] 共用）。
#[derive(Clone)]
pub struct DialogContentOpts {
    /// 右上角仅图标 ghost 关闭钮；默认 `true`（对齐 solid-ui / shadcn）。
    pub show_close: bool,
    /// 遮罩色（语义 token / hex）；`None` → theme background RGB + 默认 alpha。
    pub overlay_color: Option<String>,
    /// Overlay opacity 0…1; `None` → [`DEFAULT_MODAL_OVERLAY_OPACITY`]（`0.25`，与 Sheet / Drawer / CommandDialog 相同）。
    pub overlay_opacity: Option<f32>,
    /// Whether clicking the overlay closes; default `true`.
    pub close_on_overlay_click: bool,
    /// Overlay backdrop blur radius (logical px); `None` → [`DEFAULT_MODAL_OVERLAY_BLUR`]（`2.0`，与 Sheet / Drawer / CommandDialog 相同）。
    pub overlay_blur: Option<f32>,
    /// CommandDialog：按内容撑开；顶/左右 `p-0`；列表在 `max_h` 下吃掉 Input 之后的剩余高度。
    pub hug_content: bool,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: SlotChrome,
}

impl Default for DialogContentOpts {
    fn default() -> Self {
        Self {
            show_close: true,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            hug_content: false,
            panel: Default::default(),
        }
    }
}

impl DialogContentOpts {
    impl_content_opts_chrome!();
}

/// 遮罩选项：与 Sheet / Drawer / CommandDialog 共用 [`modal_shell`] 默认（alpha `0.25`、blur `2`）。
fn dialog_shell_overlay_opts(opts: &DialogContentOpts) -> ShellOverlayOpts {
    ShellOverlayOpts::from_content_parts(
        opts.overlay_color.as_deref(),
        opts.overlay_opacity,
        opts.close_on_overlay_click,
        opts.overlay_blur,
    )
}

/// solid-ui `DialogContent`：Portal + Overlay + 面板壳。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建，同 Popover）。
pub fn dialog_content(opts: DialogContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_dialog("DialogContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let view_id = format!("{id}__dialog_view");
    let overlay = dialog_shell_overlay_opts(&opts);
    let show_close = opts.show_close;
    let hug_content = opts.hug_content;
    let panel_chrome = opts.panel;

    viewport_modal_island(view_id, move || {
        let owner = format!("dialog:{id}");
        viewport_modal_layer(
            &id,
            open,
            &overlay,
            ViewportModalLayerOpts::dialog_fade(),
            |phase| {
                let Some(phase) = phase else {
                    sync_focus_trap(&owner, false, "");
                    return None;
                };

                let mut inner = body();
                inner = if hug_content {
                    inner.w("100%").min_h(0.0).flex_shrink(1.0)
                } else {
                    inner.w("100%").min_h("0").flex_grow(1.0).gap("1rem")
                };
                // Chrome close is absolute on the panel shell only — never inject padding
                // into Header / Body / Footer (those parts own their own layout).
                let chrome_close = if show_close {
                    let o = open;
                    let glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
                    let icon = build_named_icon_text_node("x", glyph).text_size(16.0);
                    let mut close: Element = button(format!("{id}:chrome_close"))
                        .variant("ghost")
                        .size("icon-sm")
                        .on_click(move || o.set(false))
                        .child(icon)
                        .into();
                    close = close.aria_label(crate::tr_default_zh(
                        "jadix.dialog.close",
                        "Close",
                        "关闭",
                    ));
                    Some(close)
                } else {
                    None
                };
                let mut ring = Vec::new();
                collect_dialog_focus_ids(&inner, &mut ring);
                if show_close {
                    ring.push(format!("{id}:chrome_close"));
                }
                if phase != PresencePhase::Present || ring.is_empty() {
                    sync_focus_trap(&owner, false, "");
                    let focus = use_signal("__jadix_button_focus_id", String::new);
                    if focus.peek().starts_with(&format!("{id}:")) {
                        focus.set(String::new());
                    }
                } else {
                    sync_focus_trap(&owner, true, ring.join(","));
                }
                Some(dialog_shell_panel(
                    &id,
                    inner,
                    chrome_close,
                    hug_content,
                    None,
                    None,
                    None,
                    None,
                    panel_chrome.clone(),
                ))
            },
        )
    })
}

fn collect_dialog_focus_ids(el: &Element, out: &mut Vec<String>) {
    crate::core::tree::walk_elements(el, |n| {
        if n.focusable {
            if let Some(fid) = &n.focus_id {
                if !fid.is_empty() {
                    out.push(fid.clone());
                }
            }
        }
    });
}

/// 固定顶栏（不滚动）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .min_w("0")
            .gap("0.375rem")
            .flex_shrink(0.0)
            .overflow_x("hidden")
            // Reserve chrome close (`absolute top-4 right-4` on DialogContent).
            .pr("2.5rem")
            .children(kids),
    )
}

/// Title.
///
/// 必须显式设 `foreground`：[`crate::style::TextStyle`] 默认色是白，叠在浅色面板上会“消失”。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("foreground");
    let mut el = match children {
        Some(c) => c
            .w("100%")
            .min_w("0")
            .text_size(18.0)
            .font_weight(600.0)
            .text_color(fg),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg))
            .w("100%")
            .min_w("0"),
    };
    el.set_attr("data-slot", "dialog-title");
    el.accesskit_role("heading")
}

/// Description copy.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    let inner = match children {
        Some(c) => c
            .w("100%")
            .min_w("0")
            .text_size(14.0)
            .font_weight(400.0)
            .text_leading_normal()
            .text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted))
            .w("100%")
            .min_w("0"),
    };
    // Block wrapper: portal 首帧 flex 宽度未定时，文本叶按 intrinsic 单行测量会导致
    // 打开后换行 layout shift。固定为 header 可用宽度（100% − close 预留）。
    Element::from(
        crate::primitives::r#box::r#box()
            .w("100%")
            .min_w("0")
            .overflow_x("hidden")
            .child(inner),
    )
}

/// Header 与 Footer 之间的**唯一滚动区**。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogBody(children: Option<Element>) -> Element {
    let ctx = require_dialog("DialogBody");
    let child = children.unwrap_or_else(Element::new);
    let scroll_id = format!("{}__body", ctx.id);
    // Prefer flex sizing over the ScrollArea default `h: 100%` — percentage height
    // against an unresolved portal stage can leave body metrics at "no overflow"
    // until a later dirty (e.g. opening a nested Select).
    scroll_area(scroll_id.as_str())
        .overflow_x("hidden")
        .overflow_y("auto")
        .scrollbar_visibility("auto")
        .w("100%")
        .h("auto")
        .min_h("0")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .child(child)
        .into()
}

/// 固定底栏：窄窗口纵向排列，`sm` 起横向排列并允许换行。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    crate::style::utility::with_default_class_list(
        Element::from(
            crate::primitives::r#box::vstack()
                .w("100%")
                .min_w(0.0)
                .gap("0.5rem")
                .items("stretch")
                .flex_shrink(0.0)
                .wrap("wrap")
                .children(kids),
        ),
        "sm:flex-row sm:justify-end sm:items-center",
    )
}

impl_simple_slot_builder!(
    DialogTriggerBuilder,
    dialog_trigger,
    DialogTrigger,
    DialogTriggerProps
);
impl_simple_slot_builder!(
    DialogHeaderBuilder,
    dialog_header,
    DialogHeader,
    DialogHeaderProps
);
impl_simple_slot_builder!(
    DialogTitleBuilder,
    dialog_title,
    DialogTitle,
    DialogTitleProps
);
impl_simple_slot_builder!(
    DialogDescriptionBuilder,
    dialog_description,
    DialogDescription,
    DialogDescriptionProps
);
impl_simple_slot_builder!(DialogBodyBuilder, dialog_body, DialogBody, DialogBodyProps);
impl_simple_slot_builder!(
    DialogFooterBuilder,
    dialog_footer,
    DialogFooter,
    DialogFooterProps
);

impl DialogHeaderBuilder {
    pub fn title(self, text: impl Into<String>) -> Self {
        self.child(dialog_title().text(text))
    }
    pub fn title_with(self, f: impl FnOnce(DialogTitleBuilder) -> DialogTitleBuilder) -> Self {
        self.child(f(dialog_title()))
    }
    pub fn description(self, text: impl Into<String>) -> Self {
        self.child(dialog_description().text(text))
    }
    pub fn description_with(
        self,
        f: impl FnOnce(DialogDescriptionBuilder) -> DialogDescriptionBuilder,
    ) -> Self {
        self.child(f(dialog_description()))
    }
}

/// Builder for [`dialog_content`]: `.class()` / `.style()` land on the panel card.
pub struct DialogContentSlotBuilder {
    opts: DialogContentOpts,
}

pub fn dialog_content_slot() -> DialogContentSlotBuilder {
    DialogContentSlotBuilder {
        opts: DialogContentOpts::default(),
    }
}

impl DialogContentSlotBuilder {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.opts = self.opts.class(classes);
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.opts = self.opts.style(f);
        self
    }
    pub fn show_close(mut self, v: bool) -> Self {
        self.opts.show_close = v;
        self
    }
    pub fn hug_content(mut self, v: bool) -> Self {
        self.opts.hug_content = v;
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        dialog_content(self.opts, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::core::reactivity::Signal;
    use crate::core::tree::PointerEvents;
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree, PORTAL_LAYER_ID};
    use crate::utils::modal_shell::{
        default_modal_overlay_color, ShellOverlayOpts, DEFAULT_MODAL_OVERLAY_BLUR,
        DEFAULT_MODAL_OVERLAY_OPACITY,
    };

    #[test]
    fn responsive_modal_actions_fit_narrow_windows_and_restore_desktop_rows() {
        use crate::style::{resolve_class_variants, VariantContext};
        use taffy::style::FlexDirection;

        crate::core::reactivity::create_root(|| {
            let actions = || {
                let mut pack = Element::new();
                pack.set_attr("jadix_fragment", "1");
                pack.children(vec![
                    button("responsive-cancel").text("Cancel changes").into(),
                    button("responsive-confirm").text("Save changes").into(),
                ])
            };
            for mut footer in [
                DialogFooter(DialogFooterProps {
                    children: Some(actions()),
                    base: None,
                }),
                crate::components::alert_dialog::AlertDialogFooter(
                    crate::components::alert_dialog::AlertDialogFooterProps {
                        children: Some(actions()),
                        base: None,
                    },
                ),
            ] {
                footer = footer.class("gap-3");
                let mut root = Element::new().child(footer);
                let mut tree = LayoutTree::new();
                for width in [320.0, 639.5, 640.0, 1024.0, 320.0] {
                    resolve_class_variants(
                        &mut root.children[0],
                        &VariantContext {
                            viewport_w: width,
                            ..Default::default()
                        },
                    );
                    assert_eq!(
                        root.children[0].style.flex_direction,
                        if width < 640.0 {
                            FlexDirection::Column
                        } else {
                            FlexDirection::Row
                        }
                    );
                    root = root.w(width - 64.0);
                    tree.build_and_layout(&mut root, width, 800.0, [-1.0, -1.0]);
                    let row = &root.children[0];
                    for action in &row.children {
                        assert!(action.layout_bounds[2] > 0.0);
                        assert!(action.layout_bounds[0] >= row.layout_bounds[0] - 0.5);
                        assert!(
                            action.layout_bounds[0] + action.layout_bounds[2]
                                <= row.layout_bounds[0] + row.layout_bounds[2] + 0.5,
                            "action must stay reachable at {width}px: {:?}",
                            action.layout_bounds
                        );
                    }
                    if width < 640.0 {
                        assert!(
                            row.children[1].layout_bounds[1] > row.children[0].layout_bounds[1]
                        );
                    } else {
                        assert!(
                            (row.children[1].layout_bounds[1] - row.children[0].layout_bounds[1])
                                .abs()
                                < 0.5
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn responsive_dialog_panel_keeps_viewport_gutters() {
        for width in [320.0, 375.0, 639.5, 640.0, 768.0, 1280.0] {
            let panel = dialog_shell_panel(
                "responsive-panel",
                Element::new().h(80.0),
                None,
                false,
                None,
                None,
                None,
                None,
                SlotChrome::default(),
            );
            let mut root = Element::new().w(width).child(panel);
            LayoutTree::new().build_and_layout(&mut root, width, 800.0, [-1.0, -1.0]);
            let panel = &root.children[0];
            assert!(
                (panel.layout_bounds[2] - (width - 32.0).min(512.0)).abs() < 0.5,
                "width={width}: {:?}",
                panel.layout_bounds
            );
            assert!((panel.children[0].layout_bounds[2] - panel.layout_bounds[2]).abs() < 0.5);
        }
    }

    #[test]
    fn dialog_shell_shadow_matches_elevation_xl() {
        let el = apply_dialog_shell_surface(Element::new(), false);
        assert_eq!(
            el.style.shadow_color,
            crate::tokens::shadow_rgba(crate::tokens::Elevation::Xl)
        );
        assert!((el.style.shadow_blur - 16.0).abs() < 0.01);
    }

    #[test]
    fn dialog_default_overlay_matches_modal_shell() {
        let opts = dialog_shell_overlay_opts(&DialogContentOpts::default());
        assert!(
            (opts.color[3] - DEFAULT_MODAL_OVERLAY_OPACITY).abs() < 0.001,
            "expected alpha {}, got {}",
            DEFAULT_MODAL_OVERLAY_OPACITY,
            opts.color[3]
        );
        assert!(
            (opts.backdrop_blur - DEFAULT_MODAL_OVERLAY_BLUR).abs() < 0.001,
            "expected blur {}, got {}",
            DEFAULT_MODAL_OVERLAY_BLUR,
            opts.backdrop_blur
        );
    }

    #[test]
    fn dialog_panel_is_viewport_centered_and_empty_stage_hits_underlay() {
        use crate::app::pointer_events::hit_path;
        use crate::layout::layout_tree::ATTR_PORTAL_KEY;

        clear_overlay_stack();
        let open = Signal::new(true);
        let overlay_opts = ShellOverlayOpts {
            color: default_modal_overlay_color(None, Some(0.5)),
            close_on_click: true,
            backdrop_blur: 0.0,
        };
        let inner = vstack().w("100%").child(Element::new().text("Body")).into();
        let shell = dialog_shell_frame(
            "dlg-center",
            open,
            inner,
            &overlay_opts,
            None,
            false,
            None,
            None,
            None,
            None,
            SlotChrome::default(),
        );

        let mut root = Element::new().w(800.0).h(600.0).child(shell);
        assert!(hoist_portal_subtrees(&mut root) >= 2);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

        let layer = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let panel = layer
            .children
            .iter()
            .find(|c| {
                c.attrs
                    .get(ATTR_PORTAL_KEY)
                    .map(|s| s.as_str())
                    .is_some_and(|k| k.starts_with("panel:"))
            })
            .and_then(|stage| {
                stage
                    .children
                    .iter()
                    .find(|c| c.template_id.as_deref() == Some("dlg-center__modal_panel"))
            })
            .expect("dialog panel on stage");
        let [px, _, pw, _] = panel.layout_bounds;
        let panel_center = px + pw * 0.5;
        assert!(
            (panel_center - 400.0).abs() < 2.0,
            "panel must be horizontally centered in 800px viewport, center={panel_center}"
        );
        assert!(
            pw < 600.0,
            "panel must shrink-wrap dialog chrome, not span viewport; w={pw}"
        );

        let corner = hit_path(&root, 790.0, 10.0);
        assert!(
            corner.iter().any(|e| {
                e.portal_key
                    .as_deref()
                    .is_some_and(|k| k.starts_with("underlay:"))
            }),
            "clicks outside shrink-wrapped panel must reach underlay"
        );
        assert_eq!(
            panel.pointer_events_mode(),
            PointerEvents::Auto,
            "panel/card swallow must stay auto"
        );
        clear_overlay_stack();
    }

    #[test]
    fn dialog_open_builds_content_body_once() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_root, use_signal};
        use crate::utils::modal_motion::{
            any_modal_css_needs_frame, set_modal_css_needs_frame_for_test,
        };

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let builds = Rc::new(Cell::new(0u32));
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, open) = create_root(|| {
            let open = use_signal("dlg_open_once__open", || false);
            let builds = Rc::clone(&builds);
            let live_e = Rc::clone(&live);
            crate::core::reactivity::create_effect(move || {
                let builds_once = Rc::clone(&builds);
                let el = dialog("dlg_open_once").open(open).child(move || {
                    dialog_content(DialogContentOpts::default(), move || {
                        builds_once.set(builds_once.get() + 1);
                        Element::new().text("body")
                    })
                });
                *live_e.borrow_mut() = el;
            });
            open
        });

        drain_deferred_view_remounts();
        assert_eq!(
            builds.get(),
            0,
            "closed DialogContent must not run the body"
        );

        set_modal_css_needs_frame_for_test(false);
        open.set(true);
        let first = builds.get();
        assert_eq!(
            first, 1,
            "first open must build DialogContent body once (not a mounted.set re-flush), got {first}"
        );
        assert!(
            any_modal_css_needs_frame(),
            "first open.set must arm about_to_wait before the first tick, or caret WaitUntil (~520ms) sleeps through the first present"
        );

        fn has_tid(el: &Element, id: &str) -> bool {
            if el.template_id.as_deref() == Some(id) {
                return true;
            }
            el.children.iter().any(|c| has_tid(c, id))
        }
        assert!(
            has_tid(&live.borrow(), "dlg_open_once__modal_overlay"),
            "first open.set must hoist overlay in the same turn, not wait for idle retry"
        );

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn dialog_close_does_not_rebuild_content_body() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_root, use_signal};
        use crate::utils::modal_motion::flush_modal_shell_motion_subtree;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let builds = Rc::new(Cell::new(0u32));
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, open) = create_root(|| {
            let open = use_signal("dlg_close_once__open", || false);
            let builds = Rc::clone(&builds);
            let live_e = Rc::clone(&live);
            crate::core::reactivity::create_effect(move || {
                let builds_once = Rc::clone(&builds);
                let el = dialog("dlg_close_once").open(open).child(move || {
                    dialog_content(DialogContentOpts::default(), move || {
                        builds_once.set(builds_once.get() + 1);
                        Element::new().text("body")
                    })
                });
                *live_e.borrow_mut() = el;
            });
            open
        });

        drain_deferred_view_remounts();
        open.set(true);
        drain_deferred_view_remounts();
        assert_eq!(builds.get(), 1, "open must build body once");

        open.set(false);
        drain_deferred_view_remounts();
        assert_eq!(
            builds.get(),
            1,
            "close must keep the live DialogContent tree through Exiting; rebuilding overlay/zoom is the exit hitch"
        );

        {
            let mut tree = live.borrow_mut();
            assert!(
                flush_modal_shell_motion_subtree(&mut tree),
                "close must seed exit keyframes on the live tree"
            );
            fn has_fade_out(el: &Element) -> bool {
                el.style.animations.iter().any(|a| a.name == "fade-out-0")
                    || el.children.iter().any(has_fade_out)
            }
            assert!(
                has_fade_out(&tree),
                "exit keyframes must land on the existing overlay/zoom, not a rebuilt island"
            );
        }

        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    #[test]
    fn dialog_open_first_paint_is_already_fading() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_root, use_signal};
        use crate::utils::modal_motion::{
            flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
        };

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (owner, open) = create_root(|| {
            let open = use_signal("dlg_first_paint__open", || false);
            let live_e = Rc::clone(&live);
            crate::core::reactivity::create_effect(move || {
                let el = dialog("dlg_first_paint").open(open).child(move || {
                    dialog_content(DialogContentOpts::default(), || Element::new().text("body"))
                });
                *live_e.borrow_mut() = el;
            });
            open
        });

        drain_deferred_view_remounts();
        crate::style::utility::set_frame_dt(2.0);
        open.set(true);

        let opacity = {
            let mut tree = live.borrow_mut();
            let _ = hoist_portal_subtrees(&mut tree);
            assert!(
                flush_modal_shell_motion_subtree(&mut tree),
                "first paint must seed enter keyframes"
            );
            assert!(
                tick_hoisted_modal_css(&mut tree),
                "first paint must step CSS"
            );
            fn overlay_opacity(el: &Element) -> Option<f32> {
                if el.template_id.as_deref() == Some("dlg_first_paint__modal_overlay") {
                    return Some(el.style.opacity);
                }
                el.children.iter().find_map(overlay_opacity)
            }
            overlay_opacity(&tree).expect("overlay on first paint")
        };
        assert!(
            opacity < 0.35,
            "click → first RedrawRequested must present a fading dialog, not a skipped enter, got {opacity}"
        );

        crate::style::utility::set_frame_dt(1.0 / 60.0);
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}
