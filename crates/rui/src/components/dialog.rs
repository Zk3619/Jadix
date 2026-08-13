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
//!     └── DialogFooter       // 也可用 Button + open.set(false)
//! ```
//!
//! 模态壳（Portal / Overlay / 面板表面）在本模块内实现；确认框直接组合 Header/Footer，
//! 勿再维护平行 AlertDialog。`ui!` 中 `<Dialog>` / `<DialogContent>` 由宏展开。
//!
//! ## 用法
//! ```ignore
//! ui! {
//!     <Dialog id="edit" open={open}>
//!         <DialogTrigger>
//!             <Button>Dialog</Button>
//!         </DialogTrigger>
//!         <DialogContent
//!             overlay_blur={8.0}
//!             close_on_overlay_click={true}
//!         >
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

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode, OverlayZ};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::primitives::interactive::interactive;
use crate::primitives::overlay::overlay;
use crate::primitives::r#box::{r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::popup::{portal_panel, swallow_clicks};
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context / Builder
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id`。
#[derive(Clone)]
pub struct DialogCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn dialog_context() -> Context<DialogCtx> {
    static CTX: OnceLock<Context<DialogCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DialogCtx>)
}

/// 读取最近的 [`Dialog`] 上下文。
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

    /// 在上下文内构建 Trigger / Content 等子树。
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        dialog_scope(self.id, self.open, body)
    }
}

fn require_dialog(part: &str) -> DialogCtx {
    use_dialog().unwrap_or_else(|| {
        panic!("{part} must be used within <Dialog> / dialog_scope");
    })
}

/// `ui!` 多子节点会先打成匿名 VStack；Header/Footer 再拆开接到自己的布局上。
fn take_packed_children(mut el: Element) -> Vec<Element> {
    let bare = el.text.as_ref().is_none_or(|t| t.is_empty());
    if bare && el.children.len() > 1 {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
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
// Modal shell（原 internal/dialog_shell）
// ---------------------------------------------------------------------------

struct ModalShellZs {
    underlay: i32,
    panel: i32,
}

/// 遮罩外观与交互（产品默认：opacity 0.5，点击关闭）。
#[derive(Clone, Debug)]
struct ShellOverlayOpts {
    color: [f32; 4],
    close_on_click: bool,
    backdrop_blur: f32,
}

impl ShellOverlayOpts {
    fn from_parts(
        color: Option<&str>,
        opacity: Option<f32>,
        close_on_click: bool,
        backdrop_blur: f32,
    ) -> Self {
        Self {
            color: default_overlay_color(color, opacity),
            close_on_click,
            backdrop_blur: backdrop_blur.max(0.0),
        }
    }
}

/// `color`：语义色 / hex；缺省用 `background` 的 RGB。  
/// `opacity`：遮罩 alpha，缺省 `0.5`。
fn default_overlay_color(color: Option<&str>, opacity: Option<f32>) -> [f32; 4] {
    let mut c = match color {
        Some(s) if !s.is_empty() => parse_color(s),
        _ => tokens::background(),
    };
    c[3] = opacity.unwrap_or(0.5).clamp(0.0, 1.0);
    c
}

fn modal_sync(id: &str, open: Signal<bool>) -> Option<ModalShellZs> {
    let oz: OverlayZ = portal_sync(id, open, OverlayKind::Modal, OverlayMode::Stacked)?;
    Some(ModalShellZs {
        underlay: oz.underlay,
        panel: oz.panel,
    })
}

fn dialog_overlay(open: Signal<bool>, underlay_z: i32, opts: &ShellOverlayOpts) -> Element {
    let mut b = overlay(underlay_z)
        .color(opts.color)
        .backdrop_blur(opts.backdrop_blur);
    if opts.close_on_click {
        b = b.close_on_click(open);
    }
    b.into()
}

fn apply_dialog_shell_surface(mut panel: Element) -> Element {
    panel = panel
        .bg(tokens::background())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
        .p("1.5rem")
        .max_w("85dvw")
        .w("100%")
        .max_h("85dvh")
        .overflow_y("hidden")
        .overflow_x("hidden");
    panel
}

/// Portal + Overlay + Content 壳；`inner` 为面板内整棵子树。
fn dialog_shell_frame(
    open: Signal<bool>,
    zs: &ModalShellZs,
    inner: Element,
    overlay_opts: &ShellOverlayOpts,
    chrome_close: Option<Element>,
) -> Element {
    let overlay_el = dialog_overlay(open, zs.underlay, overlay_opts);

    // Swallow clicks only — `panel_block_clicks` would nest a second `rui_portal` under stage.
    let mut panel: Element = vstack().w("100%").min_h("0").child(inner).into();
    panel = apply_dialog_shell_surface(panel);
    panel = swallow_clicks(panel.relative());
    if let Some(mut close) = chrome_close {
        close = close.absolute().top("1rem").right("1rem").z_index(1);
        panel = panel.child(close);
    }

    let mut stage: Element = r#box()
        .absolute(true)
        .inset(0.0)
        .w("100%")
        .h("100%")
        .items_center()
        .justify_center()
        .child(panel)
        .into();
    stage.set_attr(ATTR_PORTAL_KEY, format!("panel:{}", zs.panel));
    stage = portal_panel(stage, zs.panel);
    stage = stage.pointer_events_box_none();

    Element::new()
        .child(overlay_el)
        .child(stage)
        .pointer_events_box_none()
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
    }
}

/// `DialogContent` 遮罩与 chrome 选项（`ui!` / [`dialog_content`] 共用）。
#[derive(Clone, Debug)]
pub struct DialogContentOpts {
    /// 右上角仅图标 ghost 关闭钮；默认 `true`（对齐 solid-ui / shadcn）。
    pub show_close: bool,
    /// 遮罩色（语义 token / hex）；`None` → theme background RGB + 默认 alpha。
    pub overlay_color: Option<String>,
    /// 遮罩不透明度 0…1；`None` → `0.5`。
    pub overlay_opacity: Option<f32>,
    /// 点击遮罩是否关闭；默认 `true`。
    pub close_on_overlay_click: bool,
    /// 遮罩 backdrop 模糊半径（逻辑 px）；`0` / `None` → 关闭。
    pub overlay_blur: Option<f32>,
}

impl Default for DialogContentOpts {
    fn default() -> Self {
        Self {
            show_close: true,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
        }
    }
}

/// solid-ui `DialogContent`：Portal + Overlay + 面板壳。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建，同 Popover）。
pub fn dialog_content(opts: DialogContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_dialog("DialogContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let view_id = format!("{id}__dialog_view");
    let overlay = ShellOverlayOpts::from_parts(
        opts.overlay_color.as_deref(),
        opts.overlay_opacity,
        opts.close_on_overlay_click,
        opts.overlay_blur.unwrap_or(0.0),
    );
    let show_close = opts.show_close;

    reactive_view(view_id, move || {
        let is_open = open.get();
        let Some(zs) = modal_sync(&id, open) else {
            return Element::new();
        };
        if !is_open {
            return Element::new();
        }

        let mut inner = body();
        inner = inner.w("100%").min_h("0").flex_grow(1.0).gap("1rem");
        // Chrome close is absolute on the panel shell only — never inject padding
        // into Header / Body / Footer (those parts own their own layout).
        let chrome_close = if show_close {
            let o = open;
            let glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
            let icon = build_icon_text_node(glyph).text_size(16.0);
            Some(
                button(format!("{id}:chrome_close"))
                    .variant("ghost")
                    .size("icon-sm")
                    .on_click(move || o.set(false))
                    .child(icon)
                    .into(),
            )
        } else {
            None
        };
        dialog_shell_frame(open, &zs, inner, &overlay, chrome_close)
    })
}

/// 固定顶栏（不滚动）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.375rem")
            .flex_shrink(0.0)
            .children(kids),
    )
}

/// 标题。
///
/// 必须显式设 `foreground`：[`crate::style::TextStyle`] 默认色是白，叠在浅色面板上会“消失”。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("foreground");
    match children {
        Some(c) => c.text_size(18.0).font_weight(600.0).text_color(fg),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)),
    }
}

/// 说明文案。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
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

/// 固定底栏（不滚动）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DialogFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    Element::from(
        crate::primitives::r#box::hstack()
            .w("100%")
            .gap("0.5rem")
            .justify("end")
            .flex_shrink(0.0)
            .children(kids),
    )
}
