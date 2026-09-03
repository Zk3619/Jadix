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
//! Modal-shell overlay / Portal z: see [`crate::components::internal::modal_shell`];
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

use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::internal::modal_motion::{
    apply_modal_shell_motion, mounted_signal, sync_modal_presence, use_modal_presence,
    ModalPresenceHandle, ModalSurfaceMotion,
};
use crate::components::internal::modal_shell::{
    build_modal_overlay, modal_sync, ModalShellZs, ShellOverlayOpts,
};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{r#box, vstack};
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage, swallow_clicks};
use crate::utils::presence::PresencePhase;
use crate::utils::sync_focus_trap;
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;

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
// Modal shell（贴边面板表面；overlay / z 见 internal::modal_shell）
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
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
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
                .border_left(border, 1.0);
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
                .border_right(border, 1.0);
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

/// Portal + Overlay + Content shell; `inner` is the full in-panel subtree.
fn sheet_shell_frame(
    modal_id: &str,
    open: Signal<bool>,
    zs: &ModalShellZs,
    inner: Element,
    overlay_opts: &ShellOverlayOpts,
    side: SheetSide,
    width: Option<&str>,
    height: Option<&str>,
    chrome_close: Option<Element>,
    presence: &ModalPresenceHandle,
    phase: PresencePhase,
) -> Element {
    let mut overlay_el = build_modal_overlay(open, zs.underlay, overlay_opts);

    // Swallow clicks only — nesting `panel_block_clicks` under stage nests `rui_portal`.
    let mut panel: Element = vstack().w("100%").min_h("0").child(inner).into();
    panel = apply_sheet_shell_surface(panel, side, width, height);
    // Keep `position: absolute` so the sheet docks on `side` and slides in from
    // that edge (`relative()` would lay it out at flex-start / the opposite edge).
    panel = swallow_clicks(panel);
    if let Some(mut close) = chrome_close {
        // shadcn: absolute top-4 right-4
        close = close.absolute().top("1rem").right("1rem").z_index(1);
        panel = panel.child(close);
    }

    let (vw, vh) = crate::components::drawer::drawer_viewport_size();
    let slide_axis = match side {
        SheetSide::Top | SheetSide::Bottom => vh,
        SheetSide::Left | SheetSide::Right => vw,
    };

    if apply_modal_shell_motion(
        modal_id,
        &mut overlay_el,
        &mut panel,
        ModalSurfaceMotion::from_sheet_side(side),
        phase,
        presence,
        [0.0, 0.0],
        false,
        slide_axis,
    ) {
        crate::app::runtime::request_redraw();
    }

    let stage: Element = r#box().inset(0.0).w("100%").h("100%").child(panel).into();
    let stage = mount_viewport_portal_stage(stage, zs.panel);
    mount_modal_layer(overlay_el, stage)
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

/// shadcn `SheetTrigger`：子节点为组件时挂载打开；纯文本则默认 [`Label`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_sheet("SheetTrigger");
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

/// shadcn `SheetClose`：点击子树关闭最近的 Sheet。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetClose(children: Option<Element>) -> Element {
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

/// `SheetContent` 遮罩 / 贴边 / chrome 选项（`ui!` / [`sheet_content`] 共用）。
#[derive(Clone, Debug)]
pub struct SheetContentOpts {
    /// 贴边方向；默认 [`SheetSide::Right`]。
    pub side: SheetSide,
    /// 右上角仅图标 ghost 关闭钮；默认 `true`（对齐 shadcn `showCloseButton`）。
    pub show_close: bool,
    /// Overlay color (semantic token / hex); `None` → theme `background` RGB + modal default alpha.
    pub overlay_color: Option<String>,
    /// Overlay opacity 0…1; `None` → modal default ([`crate::components::internal::modal_shell::DEFAULT_MODAL_OVERLAY_OPACITY`], `0.6`).
    pub overlay_opacity: Option<f32>,
    /// Whether clicking the overlay closes; default `true`.
    pub close_on_overlay_click: bool,
    /// Overlay backdrop blur radius (logical px); `None` → [`crate::components::internal::modal_shell::DEFAULT_MODAL_OVERLAY_BLUR`] (`4`).
    pub overlay_blur: Option<f32>,
    /// 左右侧宽；`None` → `75%`（`max-w: 24rem`）。
    pub width: Option<String>,
    /// 顶 / 底高；`None` → `auto`（`max-h: 50dvh`）。
    pub height: Option<String>,
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
        }
    }
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
    let presence = use_modal_presence(&id);
    let mounted = mounted_signal(&presence);

    reactive_view(view_id, move || {
        let want = open.get();
        let phase = sync_modal_presence(&presence, want);
        let owner = format!("sheet:{id}");
        if !mounted.get() {
            sync_focus_trap(&owner, false, "");
            return Element::new();
        }
        let Some(zs) = modal_sync(&id, mounted) else {
            sync_focus_trap(&owner, false, "");
            return Element::new();
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

        sheet_shell_frame(
            &id,
            open,
            &zs,
            inner,
            &overlay,
            side,
            width.as_deref(),
            height.as_deref(),
            chrome_close,
            &presence,
            phase,
        )
    })
}

fn collect_sheet_focus_ids(el: &Element, out: &mut Vec<String>) {
    if el.focusable {
        if let Some(fid) = &el.focus_id {
            if !fid.is_empty() {
                out.push(fid.clone());
            }
        }
    }
    for c in &el.children {
        collect_sheet_focus_ids(c, out);
    }
}

/// 固定顶栏（不滚动）；对齐 `cn-sheet-header` / `p-4 gap-1.5`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetHeader(children: Option<Element>) -> Element {
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

/// 标题（显式 `foreground`，避免默认白字叠浅色面板）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("foreground");
    match children {
        Some(c) => c.text_size(18.0).font_weight(600.0).text_color(fg),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)),
    }
}

/// 说明文案（`text-sm text-muted-foreground`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
}

/// Header 与 Footer 之间的**唯一滚动区**（可选；长列表 / 表单用）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetBody(children: Option<Element>) -> Element {
    let ctx = require_sheet("SheetBody");
    let child = children.unwrap_or_else(Element::new);
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

/// Sticky footer (`mt-auto flex flex-col gap-2 p-4`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SheetFooter(children: Option<Element>) -> Element {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::popup::{panel_block_clicks, portal_panel};

    fn has_portal_mark(el: &Element) -> bool {
        el.attrs.get("rui_portal").map(String::as_str) == Some("1")
            || el.children.iter().any(has_portal_mark)
    }

    fn portal_nests_portal(el: &Element) -> bool {
        let self_p = el.attrs.get("rui_portal").map(String::as_str) == Some("1");
        if self_p && el.children.iter().any(has_portal_mark) {
            return true;
        }
        el.children.iter().any(portal_nests_portal)
    }

    #[test]
    fn nested_portal_panel_is_the_old_sheet_bug() {
        let panel = panel_block_clicks(Element::new().text("sheet"), 1001);
        let stage = portal_panel(Element::new().child(panel), 1001);
        assert!(
            portal_nests_portal(&stage),
            "portal stage wrapping a panel_block_clicks child nests rui_portal"
        );
    }

    #[test]
    fn sheet_side_parses_shadcn_tokens() {
        assert_eq!(SheetSide::from("right"), SheetSide::Right);
        assert_eq!(SheetSide::from("LEFT"), SheetSide::Left);
        assert_eq!(SheetSide::from("top"), SheetSide::Top);
        assert_eq!(SheetSide::from("bottom"), SheetSide::Bottom);
    }

    #[test]
    fn sheet_shell_stays_absolute_on_dock_edge() {
        use taffy::style::Position;
        let right = apply_sheet_shell_surface(Element::new(), SheetSide::Right, None, None);
        assert_eq!(right.style.position, Position::Absolute);
        assert_eq!(
            swallow_clicks(right).style.position,
            Position::Absolute,
            "swallow_clicks must not reset dock positioning to relative"
        );
        let bottom = apply_sheet_shell_surface(Element::new(), SheetSide::Bottom, None, None);
        assert_eq!(bottom.style.position, Position::Absolute);
    }
}
