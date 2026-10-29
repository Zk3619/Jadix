//! # Sheet
//!
//! 对齐 shadcn / base-ui **复合 Sheet**（侧滑 / 顶底抽屉；与 Navigator
//! `Presentation::Sheet` 转场栈无关）：
//!
//! ```text
//! Sheet
//! ├── SheetTrigger           // 可选；无组件子节点时默认 Label
//! └── SheetContent           // Portal + Overlay + 面板；side / 关闭钮 / 遮罩可调
//!     ├── SheetHeader / SheetTitle / SheetDescription
//!     ├── SheetBody          // 可选滚动区（Header↔Footer）
//!     ├── SheetFooter        // mt-auto；也可用 Button + open.set(false)
//!     └── SheetClose         // 任意子节点点击关闭
//! ```
//!
//! Modal-shell overlay / Portal z: see [`crate::utils::modal_shell`];
//! 贴边面板表面在本模块内实现。`ui!` 中 `<Sheet>` /
//! `<SheetContent>` 由宏展开。
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! ui! {
//!     <Sheet id="edit" open={open}>
//!         <SheetTrigger>
//!             <Button variant="outline">Open</Button>
//!         </SheetTrigger>
//!         <SheetContent side="right" show_close={true}>
//!             <SheetHeader>
//!                 <SheetTitle>Edit profile</SheetTitle>
//!                 <SheetDescription>Make changes…</SheetDescription>
//!             </SheetHeader>
//!             <SheetBody>{/* … */}</SheetBody>
//!             <SheetFooter>
//!                 <Button>Save changes</Button>
//!                 <SheetClose>
//!                     <Button variant="outline">Close</Button>
//!                 </SheetClose>
//!             </SheetFooter>
//!         </SheetContent>
//!     </Sheet>
//! }
//!
//! // B · Builder (no Trigger; open/close via external Signal)
//! Element::from(
//!     sheet("edit")
//!         .open_signal(open)
//!         .side(SheetSide::Right)
//!         .w("24rem")
//!         .child(|| { /* Header / Body / Footer */ }),
//! )
//! ```

use std::str::FromStr;
use std::sync::OnceLock;

use crate::components::button::button;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::modal_shell::ShellOverlayOpts;
use crate::utils::popup::swallow_clicks;
use crate::utils::presence::PresencePhase;
use crate::utils::sync_focus_trap;
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::viewport_modal::{
    viewport_modal_island, viewport_modal_layer, ModalSurfaceMotion, ViewportModalLayerOpts,
};

// ---------------------------------------------------------------------------
// Side
// ---------------------------------------------------------------------------

/// 面板贴边方向（对齐 shadcn `side`：`top` | `right` | `bottom` | `left`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SheetSide {
    Top,
    #[default]
    Right,
    Bottom,
    Left,
}

impl SheetSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }

    pub(crate) fn to_surface_motion(self) -> ModalSurfaceMotion {
        match self {
            Self::Top => ModalSurfaceMotion::SlideFromTop,
            Self::Right => ModalSurfaceMotion::SlideFromRight,
            Self::Bottom => ModalSurfaceMotion::SlideFromBottom,
            Self::Left => ModalSurfaceMotion::SlideFromLeft,
        }
    }
}

impl FromStr for SheetSide {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            "right" | "" => Self::Right,
            _ => return Err(()),
        })
    }
}

impl From<&str> for SheetSide {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Right)
    }
}

impl From<String> for SheetSide {
    fn from(s: String) -> Self {
        SheetSide::from(s.as_str())
    }
}

// ---------------------------------------------------------------------------
// Context / Builder
// ---------------------------------------------------------------------------

/// Root context: `open` + stable instance `id`.
#[derive(Clone)]
pub struct SheetCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn sheet_context() -> Context<SheetCtx> {
    static CTX: OnceLock<Context<SheetCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<SheetCtx>)
}

/// 读取最近的 [`Sheet`] 上下文。
pub fn use_sheet() -> Option<SheetCtx> {
    use_context(sheet_context())
}

/// 关闭最近的 Sheet（无上下文时为 no-op）。
pub fn sheet_close() {
    if let Some(ctx) = use_sheet() {
        ctx.open.set(false);
    }
}

/// 打开最近的 Sheet（无上下文时为 no-op）。
pub fn sheet_open() {
    if let Some(ctx) = use_sheet() {
        ctx.open.set(true);
    }
}

/// 提供 Sheet 上下文并构建子树（`ui!` `<Sheet>` / Builder 共用）。
pub fn sheet_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "sheet");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), || false));
    provide_context(
        sheet_context(),
        SheetCtx {
            open,
            id: id.clone(),
        },
    );
    let mut root = body();
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__sheet"));
    }
    root
}

/// Builder：`.open(sig).side(…).w(…).child(|| …)` → [`sheet_scope`] + [`sheet_content`]。
pub struct SheetBuilder {
    id: String,
    open: Option<Signal<bool>>,
    side: SheetSide,
    width: Option<String>,
    height: Option<String>,
    show_close: bool,
    body: Option<Box<dyn Fn() -> Element>>,
}

/// 侧栏 / 抽屉 Sheet 入口（双写 B；RSX 请用 `<Sheet>`）。
pub fn sheet(id: impl Into<String>) -> SheetBuilder {
    SheetBuilder {
        id: id.into(),
        open: None,
        side: SheetSide::Right,
        width: None,
        height: None,
        show_close: true,
        body: None,
    }
}

impl SheetBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    pub fn side(mut self, side: impl Into<SheetSide>) -> Self {
        self.side = side.into();
        self
    }

    /// 左右侧面板宽（默认 `75%` + `max-w: 24rem`）。
    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.width = Some(w.into());
        self
    }

    /// 顶 / 底面板高（默认内容撑开，`max-h: 50dvh`）。
    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.height = Some(h.into());
        self
    }

    pub fn show_close(mut self, show: bool) -> Self {
        self.show_close = show;
        self
    }

    /// 在上下文内构建 Content 子树（无 Trigger；由 Signal 开合）。
    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }
}

impl From<SheetBuilder> for Element {
    fn from(builder: SheetBuilder) -> Self {
        let SheetBuilder {
            id,
            open,
            side,
            width,
            height,
            show_close,
            body,
        } = builder;
        let opts = SheetContentOpts {
            side,
            show_close,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            width,
            height,
            panel: Default::default(),
        };
        sheet_scope(id, open, move || {
            sheet_content(opts, move || {
                if let Some(build) = &body {
                    build()
                } else {
                    Element::new()
                }
            })
        })
    }
}

impl crate::core::tree::IntoElementList for SheetBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

fn require_sheet(part: &str) -> SheetCtx {
    use_sheet().unwrap_or_else(|| {
        panic!("{part} must be used within <Sheet> / sheet_scope");
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
// Modal shell（贴边面板表面；overlay / z 见 utils::modal_shell）
// ---------------------------------------------------------------------------

/// 对齐 new-york-v4 `SheetContent` 表面：gap-4、bg、shadow、贴边单侧边框。
fn apply_sheet_shell_surface(
    mut panel: Element,
    side: SheetSide,
    width: Option<&str>,
    height: Option<&str>,
) -> Element {
    let border = tokens::border();
    panel = panel
        .bg(tokens::background())
        .shadow(tokens::shadow_rgba(tokens::Elevation::Xl), 16.0)
        .gap("1rem");

    match side {
        SheetSide::Right => {
            let w = width.unwrap_or("75%");
            panel = panel
                .absolute()
                .top(0.0)
                .bottom(0.0)
                .right(0.0)
                .h("100%")
                .w(w)
                .max_w("24rem")
                .min_w(0.0)
                .border_left(border, 1.0);
            panel = crate::authoring::widgets::classes::with_default_class_list(
                panel,
                "max-sm:max-w-full",
            );
        }
        SheetSide::Left => {
            let w = width.unwrap_or("75%");
            panel = panel
                .absolute()
                .top(0.0)
                .bottom(0.0)
                .left(0.0)
                .h("100%")
                .w(w)
                .max_w("24rem")
                .min_w(0.0)
                .border_right(border, 1.0);
            panel = crate::authoring::widgets::classes::with_default_class_list(
                panel,
                "max-sm:max-w-full",
            );
        }
        SheetSide::Top => {
            let h = height.unwrap_or("auto");
            panel = panel
                .absolute()
                .top(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .h(h)
                .max_h("50dvh")
                .border_bottom(border, 1.0);
        }
        SheetSide::Bottom => {
            let h = height.unwrap_or("auto");
            panel = panel
                .absolute()
                .bottom(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .h(h)
                .max_h("50dvh")
                .border_top(border, 1.0);
        }
    }
    panel
}

/// Chromed motion panel (no portal / presence).
pub(crate) fn sheet_shell_panel(
    inner: Element,
    side: SheetSide,
    width: Option<&str>,
    height: Option<&str>,
    chrome_close: Option<Element>,
    panel_chrome: crate::components::base::SlotChrome,
) -> Element {
    // Swallow clicks only — nesting `panel_block_clicks` under stage nests `jadix_portal`.
    let mut panel: Element = vstack().w("100%").min_h("0").child(inner).into();
    panel = apply_sheet_shell_surface(panel, side, width, height);
    panel.set_attr("data-slot", "sheet-content");
    panel = panel_chrome.apply(panel);
    // Keep `position: absolute` so the sheet docks on `side` and slides in from
    // that edge (`relative()` would lay it out at flex-start / the opposite edge).
    panel = swallow_clicks(panel);
    if let Some(mut close) = chrome_close {
        // Include docking in the class baseline so hover resolution preserves it.
        close = close.z_index(1);
        crate::authoring::widgets::classes::extend_class_list(&mut close, "absolute top-4 right-4");
        panel = panel.child(close);
    }
    panel
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
SheetTriggerProps [];
/// shadcn `SheetTrigger`：子节点为组件时挂载打开；纯文本则默认 [`Label`]。
#[allow(non_snake_case)]
pub fn SheetTrigger(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_sheet("SheetTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__trigger", ctx.id)
    } else {
        id
    };

    match children {
        None => interactive(
            label(trigger_id.as_str())
                .text(crate::tr_default_zh("jadix.sheet.open", "Open", "打开"))
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
}

crate::__component_contract! {
SheetCloseProps [];
/// shadcn `SheetClose`：点击子树关闭最近的 Sheet。
#[allow(non_snake_case)]
pub fn SheetClose(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_sheet("SheetClose");
    let open = ctx.open;
    match children {
        None => {
            let glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
            let icon = build_icon_text_node(glyph).text_size(16.0);
            button(format!("{}:sheet_close", ctx.id))
                .variant("ghost")
                .size("icon-sm")
                .on_click(move || open.set(false))
                .child(icon)
                .into()
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            interactive(
                label(format!("{}:sheet_close", ctx.id).as_str())
                    .text(t)
                    .into(),
            )
            .on_click(move || open.set(false))
            .into()
        }
        Some(c) => chain_click(c, move || open.set(false)),
    }
}
}

/// `SheetContent` 遮罩 / 贴边 / chrome 选项（`ui!` / [`sheet_content`] 共用）。
#[derive(Clone)]
pub struct SheetContentOpts {
    /// 贴边方向；默认 [`SheetSide::Right`]。
    pub side: SheetSide,
    /// 右上角仅图标 ghost 关闭钮；默认 `true`（对齐 shadcn `showCloseButton`）。
    pub show_close: bool,
    /// Overlay color (semantic token / hex); `None` → theme `background` RGB + modal default alpha.
    pub overlay_color: Option<String>,
    /// Overlay opacity 0…1; `None` → modal default ([`crate::utils::modal_shell::DEFAULT_MODAL_OVERLAY_OPACITY`], `0.6`).
    pub overlay_opacity: Option<f32>,
    /// Whether clicking the overlay closes; default `true`.
    pub close_on_overlay_click: bool,
    /// Overlay backdrop blur radius (logical px); `None` → [`crate::utils::modal_shell::DEFAULT_MODAL_OVERLAY_BLUR`] (`4`).
    pub overlay_blur: Option<f32>,
    /// 左右侧宽；`None` → `75%`（`max-w: 24rem`）。
    pub width: Option<String>,
    /// 顶 / 底高；`None` → `auto`（`max-h: 50dvh`）。
    pub height: Option<String>,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
}

impl Default for SheetContentOpts {
    fn default() -> Self {
        Self {
            side: SheetSide::Right,
            show_close: true,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            width: None,
            height: None,
            panel: Default::default(),
        }
    }
}

impl SheetContentOpts {
    crate::impl_content_opts_chrome!();
}

/// shadcn `SheetContent`：Portal + Overlay + 贴边面板壳。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建，同 Dialog / Popover）。
pub fn sheet_content(opts: SheetContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_sheet("SheetContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let view_id = format!("{id}__sheet_view");
    let overlay = ShellOverlayOpts::from_content_parts(
        opts.overlay_color.as_deref(),
        opts.overlay_opacity,
        opts.close_on_overlay_click,
        opts.overlay_blur,
    );
    let show_close = opts.show_close;
    let side = opts.side;
    let width = opts.width;
    let height = opts.height;
    let panel_chrome = opts.panel;

    viewport_modal_island(view_id, move || {
        let owner = format!("sheet:{id}");
        let (vw, vh) = crate::utils::modal_surface::modal_viewport_size();
        let slide_axis = match side {
            SheetSide::Top | SheetSide::Bottom => vh,
            SheetSide::Left | SheetSide::Right => vw,
        };
        viewport_modal_layer(
            &id,
            open,
            &overlay,
            ViewportModalLayerOpts::slide(side.to_surface_motion(), slide_axis),
            |phase| {
                let Some(phase) = phase else {
                    sync_focus_trap(&owner, false, "");
                    return None;
                };

                let mut inner = body();
                // Vertical stack: Header / Body / Footer；中间区 flex-grow 把 Footer 顶到底。
                inner = inner.w("100%").min_h("0").flex_grow(1.0).gap("0");

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

                let mut ring = Vec::new();
                collect_sheet_focus_ids(&inner, &mut ring);
                if show_close {
                    ring.push(format!("{id}:chrome_close"));
                }
                if phase != PresencePhase::Present || ring.is_empty() {
                    sync_focus_trap(&owner, false, "");
                } else {
                    sync_focus_trap(&owner, true, ring.join(","));
                }

                Some(sheet_shell_panel(
                    inner,
                    side,
                    width.as_deref(),
                    height.as_deref(),
                    chrome_close,
                    panel_chrome.clone(),
                ))
            },
        )
    })
}

pub(crate) fn collect_sheet_focus_ids(el: &Element, out: &mut Vec<String>) {
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

crate::__component_contract! {
SheetHeaderProps [];
/// 固定顶栏（不滚动）；对齐 `cn-sheet-header` / `p-4 gap-1.5`。
#[allow(non_snake_case)]
pub fn SheetHeader(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.375rem")
            .p("1rem")
            .flex_shrink(0.0)
            .children(kids),
    )
}
}

crate::__component_contract! {
SheetTitleProps [];
/// 标题（显式 `foreground`，避免默认白字叠浅色面板）。
#[allow(non_snake_case)]
pub fn SheetTitle(
plain [] children: Option<Element>,
) -> Element {
    let fg = parse_text_color("foreground");
    match children {
        Some(c) => c.text_size(18.0).font_weight(600.0).text_color(fg),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)),
    }
}
}

crate::__component_contract! {
SheetDescriptionProps [];
/// 说明文案（`text-sm text-muted-foreground`）。
#[allow(non_snake_case)]
pub fn SheetDescription(
plain [] children: Option<Element>,
) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
}
}

crate::__component_contract! {
SheetBodyProps [];
/// Header 与 Footer 之间的**唯一滚动区**（可选；长列表 / 表单用）。
#[allow(non_snake_case)]
pub fn SheetBody(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_sheet("SheetBody");
    let child = children.unwrap_or_default();
    let scroll_id = format!("{}__body", ctx.id);
    scroll_area(scroll_id.as_str())
        .overflow_x("hidden")
        .overflow_y("auto")
        .scrollbar_visibility("auto")
        .w("100%")
        .h("auto")
        .min_h("0")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .px("1rem")
        .child(child)
        .into()
}
}

crate::__component_contract! {
SheetFooterProps [];
/// Sticky footer (`mt-auto flex flex-col gap-2 p-4`).
#[allow(non_snake_case)]
pub fn SheetFooter(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.5rem")
            .p("1rem")
            .mt("auto")
            .flex_shrink(0.0)
            .children(kids),
    )
}
}

impl_simple_slot_builder!(
    SheetTriggerBuilder,
    sheet_trigger,
    SheetTrigger,
    SheetTriggerProps
);
impl_simple_slot_builder!(
    SheetCloseBuilder,
    sheet_close_part,
    SheetClose,
    SheetCloseProps
);
impl_simple_slot_builder!(
    SheetHeaderBuilder,
    sheet_header,
    SheetHeader,
    SheetHeaderProps
);
impl_simple_slot_builder!(SheetTitleBuilder, sheet_title, SheetTitle, SheetTitleProps);
impl_simple_slot_builder!(
    SheetDescriptionBuilder,
    sheet_description,
    SheetDescription,
    SheetDescriptionProps
);
impl_simple_slot_builder!(SheetBodyBuilder, sheet_body, SheetBody, SheetBodyProps);
impl_simple_slot_builder!(
    SheetFooterBuilder,
    sheet_footer,
    SheetFooter,
    SheetFooterProps
);

pub struct SheetContentSlotBuilder {
    opts: SheetContentOpts,
}

pub fn sheet_content_slot() -> SheetContentSlotBuilder {
    SheetContentSlotBuilder {
        opts: SheetContentOpts::default(),
    }
}

impl SheetContentSlotBuilder {
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
    pub fn side(mut self, side: impl Into<SheetSide>) -> Self {
        self.opts.side = side.into();
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        sheet_content(self.opts, body)
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/sheet.rs"]
mod tests;
