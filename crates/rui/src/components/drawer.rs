//! # Drawer
//!
//! 对齐 shadcn / base-ui **复合 Drawer**（移动端底滑抽屉为主；亦支持四向）：
//!
//! ```text
//! Drawer
//! ├── DrawerTrigger           // 可选；无组件子节点时默认 Label
//! └── DrawerContent           // Portal + Overlay + 面板；direction / 拖柄 / 遮罩可调
//!     ├── DrawerSwipeHandle   // 可选；`show_swipe_handle` 为 true 时 Content 也会自动插入
//!     ├── DrawerHeader / DrawerTitle / DrawerDescription
//!     ├── DrawerBody          // 可选滚动区（Header↔Footer）
//!     ├── DrawerFooter        // mt-auto；也可用 Button + open.set(false)
//!     └── DrawerClose         // 任意子节点点击关闭
//! ```
//!
//! 模态壳 overlay / Portal z 见 [`crate::components::internal::modal_shell`]；
//! 面板表面在本模块内实现（默认自底部滑入、顶圆角 + 拖柄）。`ui!` 中 `<Drawer>` /
//! `<DrawerContent>` 由宏展开。
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
//! ui! {
//!     <Drawer id="share" open={open}>
//!         <DrawerTrigger>
//!             <Button variant="outline">Open Drawer</Button>
//!         </DrawerTrigger>
//!         <DrawerContent direction="down" show_swipe_handle={true}>
//!             <DrawerHeader>
//!                 <DrawerTitle>Share link</DrawerTitle>
//!                 <DrawerDescription>Anyone with the link can view.</DrawerDescription>
//!             </DrawerHeader>
//!             <DrawerBody>{/* … */}</DrawerBody>
//!             <DrawerFooter>
//!                 <Button>Submit</Button>
//!                 <DrawerClose>
//!                     <Button variant="outline">Cancel</Button>
//!                 </DrawerClose>
//!             </DrawerFooter>
//!         </DrawerContent>
//!     </Drawer>
//! }
//!
//! // B · Builder（无 Trigger；由外部 Signal 开合）
//! Element::from(
//!     drawer("share")
//!         .open_signal(open)
//!         .direction(DrawerDirection::Down)
//!         .child(|| { /* Header / Body / Footer */ }),
//! )
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::reactive::reactive_view;
use crate::app::{release_pointer_capture, runtime::request_redraw, set_pointer_capture};
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
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;

// ---------------------------------------------------------------------------
// Direction（对齐 base-ui `swipeDirection`）
// ---------------------------------------------------------------------------

/// 抽屉滑入方向（`down` = 自底向上，shadcn 默认）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawerDirection {
    /// 自底向上（`swipeDirection="down"`）。
    #[default]
    Down,
    Up,
    Left,
    Right,
}

impl DrawerDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn axis_viewport(self, vw: f32, vh: f32) -> f32 {
        match self {
            Self::Down | Self::Up => vh.max(1.0),
            Self::Left | Self::Right => vw.max(1.0),
        }
    }
}

impl FromStr for DrawerDirection {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "up" => Self::Up,
            "left" => Self::Left,
            "right" => Self::Right,
            "down" | "bottom" | "" => Self::Down,
            _ => return Err(()),
        })
    }
}

impl From<&str> for DrawerDirection {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Down)
    }
}

impl From<String> for DrawerDirection {
    fn from(s: String) -> Self {
        DrawerDirection::from(s.as_str())
    }
}

// ---------------------------------------------------------------------------
// Snap points（对齐 Vaul：`number` = 视口比例，`"148px"` = 固定像素）
// ---------------------------------------------------------------------------

/// 吸附点：视口主轴 **比例**（`0.35`）或 **像素**（`"148px"`）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawerSnapPoint {
    /// 可见高度/宽度占视口比例 0…1（Vaul `snapPoints={[0.35, 1]}`）。
    Fraction(f32),
    /// 固定主轴像素（Vaul `snapPoints={['148px', 1]}` 中的 px 项）。
    Pixels(f32),
}

impl DrawerSnapPoint {
    /// 转为开度比例 0…1（相对视口主轴）。
    pub fn to_open_fraction(self, axis_viewport_px: f32) -> f32 {
        let axis = axis_viewport_px.max(1.0);
        match self {
            Self::Fraction(f) => f.clamp(0.05, 1.0),
            Self::Pixels(px) => (px / axis).clamp(0.05, 1.0),
        }
    }
}

impl From<f32> for DrawerSnapPoint {
    fn from(v: f32) -> Self {
        Self::Fraction(v)
    }
}

/// 解析单个 snap token（`0.35` / `"148px"`）。
pub fn parse_snap_point_token(s: &str) -> Option<DrawerSnapPoint> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("px").or_else(|| s.strip_suffix("PX")) {
        num.trim().parse::<f32>().ok().map(DrawerSnapPoint::Pixels)
    } else {
        s.parse::<f32>().ok().map(DrawerSnapPoint::Fraction)
    }
}

/// 解析 RSX 字符串 `snap_points="0.35,148px,1"`。
pub fn parse_snap_points(value: impl AsRef<str>) -> Vec<DrawerSnapPoint> {
    normalize_snap_points(
        &value
            .as_ref()
            .split([',', ' '])
            .filter_map(|s| parse_snap_point_token(s))
            .collect::<Vec<_>>(),
    )
}

/// 宏 / Builder：数值序列视为视口比例。
pub fn coerce_snap_points(points: impl Into<Vec<f32>>) -> Vec<DrawerSnapPoint> {
    normalize_snap_points(
        &points
            .into()
            .into_iter()
            .map(DrawerSnapPoint::Fraction)
            .collect::<Vec<_>>(),
    )
}

/// 宏：`Vec<DrawerSnapPoint>` 或混合数组。
pub fn coerce_snap_point_list(points: Vec<DrawerSnapPoint>) -> Vec<DrawerSnapPoint> {
    normalize_snap_points(&points)
}

// ---------------------------------------------------------------------------
// Context / Builder
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id`。
#[derive(Clone)]
pub struct DrawerCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn drawer_context() -> Context<DrawerCtx> {
    static CTX: OnceLock<Context<DrawerCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DrawerCtx>)
}

/// 读取最近的 [`Drawer`] 上下文。
pub fn use_drawer() -> Option<DrawerCtx> {
    use_context(drawer_context())
}

/// 关闭最近的 Drawer（无上下文时为 no-op）。
pub fn drawer_close() {
    if let Some(ctx) = use_drawer() {
        ctx.open.set(false);
    }
}

/// 打开最近的 Drawer（无上下文时为 no-op）。
pub fn drawer_open() {
    if let Some(ctx) = use_drawer() {
        ctx.open.set(true);
    }
}

/// 提供 Drawer 上下文并构建子树（`ui!` `<Drawer>` / Builder 共用）。
pub fn drawer_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "drawer");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), || false));
    provide_context(
        drawer_context(),
        DrawerCtx {
            open,
            id: id.clone(),
        },
    );
    let mut root = body();
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__drawer"));
    }
    root
}

/// Builder：`.open(sig).direction(…).child(|| …)` → [`drawer_scope`] + [`drawer_content`]。
pub struct DrawerBuilder {
    id: String,
    open: Option<Signal<bool>>,
    direction: DrawerDirection,
    width: Option<String>,
    height: Option<String>,
    show_close: bool,
    show_swipe_handle: bool,
    modal: bool,
    snap_points: Vec<DrawerSnapPoint>,
    snap_to_sequential_point: bool,
    scale_background: bool,
    body: Option<Box<dyn Fn() -> Element>>,
}

/// 底滑 Drawer 入口（双写 B；RSX 请用 `<Drawer>`）。
pub fn drawer(id: impl Into<String>) -> DrawerBuilder {
    DrawerBuilder {
        id: id.into(),
        open: None,
        direction: DrawerDirection::Down,
        width: None,
        height: None,
        show_close: false,
        show_swipe_handle: false,
        modal: true,
        snap_points: vec![DrawerSnapPoint::Fraction(1.0)],
        snap_to_sequential_point: false,
        scale_background: false,
        body: None,
    }
}

impl DrawerBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    pub fn direction(mut self, direction: impl Into<DrawerDirection>) -> Self {
        self.direction = direction.into();
        self
    }

    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.width = Some(w.into());
        self
    }

    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.height = Some(h.into());
        self
    }

    pub fn show_close(mut self, show: bool) -> Self {
        self.show_close = show;
        self
    }

    pub fn show_swipe_handle(mut self, show: bool) -> Self {
        self.show_swipe_handle = show;
        self
    }

    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// 吸附点（视口比例或 `"148px"`）；默认全开 `[1.0]`。
    pub fn snap_points(mut self, points: impl Into<Vec<DrawerSnapPoint>>) -> Self {
        self.snap_points = points.into();
        self
    }

    /// 对齐 Vaul `snapToSequentialPoint`：高速滑动也只吸附相邻档位。
    pub fn snap_to_sequential_point(mut self, enabled: bool) -> Self {
        self.snap_to_sequential_point = enabled;
        self
    }

    /// 对齐 Vaul `shouldScaleBackground` / shadcn `scaleBackground`。
    pub fn scale_background(mut self, enabled: bool) -> Self {
        self.scale_background = enabled;
        self
    }

    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }
}

impl From<DrawerBuilder> for Element {
    fn from(builder: DrawerBuilder) -> Self {
        let DrawerBuilder {
            id,
            open,
            direction,
            width,
            height,
            show_close,
            show_swipe_handle,
            modal,
            snap_points,
            snap_to_sequential_point,
            scale_background,
            body,
        } = builder;
        let opts = DrawerContentOpts {
            direction,
            show_close,
            show_swipe_handle,
            modal,
            snap_points: normalize_snap_points(&snap_points),
            snap_to_sequential_point,
            scale_background,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            width,
            height,
        };
        drawer_scope(id, open, move || {
            drawer_content(opts, move || {
                if let Some(build) = &body {
                    build()
                } else {
                    Element::new()
                }
            })
        })
    }
}

impl crate::core::tree::IntoElementList for DrawerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

fn require_drawer(part: &str) -> DrawerCtx {
    use_drawer().unwrap_or_else(|| {
        panic!("{part} must be used within <Drawer> / drawer_scope");
    })
}

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

/// shadcn `cn-drawer-handle`：细长胶囊条（`w-[100px] h-2` / 左右为 `w-2 h-[100px]`）。
fn drawer_swipe_handle_element(
    drawer_id: &str,
    open: Signal<bool>,
    direction: DrawerDirection,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
) -> Element {
    let (w, h) = match direction {
        DrawerDirection::Down | DrawerDirection::Up => ("6.25rem", "0.5rem"),
        DrawerDirection::Right | DrawerDirection::Left => ("0.5rem", "6.25rem"),
    };
    let mut el: Element = match direction {
        DrawerDirection::Down => r#box()
            .w(w)
            .h(h)
            .bg(tokens::border())
            .mx("auto")
            .mt("0.75rem")
            .mb("0.5rem")
            .flex_shrink(0.0)
            .into(),
        DrawerDirection::Up => r#box()
            .w(w)
            .h(h)
            .bg(tokens::border())
            .mx("auto")
            .mt("0.5rem")
            .mb("0.75rem")
            .flex_shrink(0.0)
            .into(),
        DrawerDirection::Right => r#box()
            .w(w)
            .h(h)
            .bg(tokens::border())
            .my("auto")
            .ml("0.75rem")
            .mr("0.5rem")
            .flex_shrink(0.0)
            .into(),
        DrawerDirection::Left => r#box()
            .w(w)
            .h(h)
            .bg(tokens::border())
            .my("auto")
            .ml("0.5rem")
            .mr("0.75rem")
            .flex_shrink(0.0)
            .into(),
    };
    el = el.rounded_full();
    bind_drawer_drag_region(
        el,
        drawer_id,
        &format!("{drawer_id}__drag"),
        open,
        direction,
        snap_points,
        snap_to_sequential,
    )
}

// ---------------------------------------------------------------------------
// Modal shell
// ---------------------------------------------------------------------------

/// 对齐 shadcn `cn-drawer-popup`：底滑默认顶圆角、`max-h: calc(100dvh - 6rem)`。
fn apply_drawer_shell_surface(
    mut panel: Element,
    direction: DrawerDirection,
    width: Option<&str>,
    height: Option<&str>,
) -> Element {
    let border = tokens::border();
    let radius = 8.0;
    panel = panel
        .bg(tokens::background())
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
        .gap("1rem");

    match direction {
        DrawerDirection::Down => {
            let h = height.unwrap_or("auto");
            panel = panel
                .absolute()
                .bottom(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .h(h)
                .max_h("calc(100dvh - 6rem)")
                .border_top(border, 1.0)
                .border_top_left_radius(radius)
                .border_top_right_radius(radius);
        }
        DrawerDirection::Up => {
            let h = height.unwrap_or("auto");
            panel = panel
                .absolute()
                .top(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .h(h)
                .max_h("calc(100dvh - 6rem)")
                .border_bottom(border, 1.0)
                .border_bottom_left_radius(radius)
                .border_bottom_right_radius(radius);
        }
        DrawerDirection::Right => {
            let w = width.unwrap_or("75%");
            panel = panel
                .absolute()
                .top(0.0)
                .bottom(0.0)
                .right(0.0)
                .h("100%")
                .w(w)
                .max_w("24rem")
                .border_left(border, 1.0)
                .border_top_left_radius(radius)
                .border_bottom_left_radius(radius);
        }
        DrawerDirection::Left => {
            let w = width.unwrap_or("75%");
            panel = panel
                .absolute()
                .top(0.0)
                .bottom(0.0)
                .left(0.0)
                .h("100%")
                .w(w)
                .max_w("24rem")
                .border_right(border, 1.0)
                .border_top_right_radius(radius)
                .border_bottom_right_radius(radius);
        }
    }
    panel
}

fn drawer_shell_frame(
    modal_id: &str,
    open: Signal<bool>,
    zs: &ModalShellZs,
    inner: Element,
    overlay_opts: &ShellOverlayOpts,
    direction: DrawerDirection,
    width: Option<&str>,
    height: Option<&str>,
    show_swipe_handle: bool,
    modal: bool,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
    scale_background: bool,
    chrome_close: Option<Element>,
    presence: &ModalPresenceHandle,
    phase: PresencePhase,
) -> Element {
    let (vw, vh) = drawer_viewport_size();
    let axis = direction.axis_viewport(vw, vh);
    let snap_fracs = resolve_snap_fractions(snap_points, axis);
    cache_snap_fracs(modal_id, &snap_fracs);

    let handle = |dir: DrawerDirection| {
        drawer_swipe_handle_element(modal_id, open, dir, snap_points, snap_to_sequential)
    };

    let mut panel: Element = match direction {
        DrawerDirection::Down => {
            let mut kids = Vec::new();
            if show_swipe_handle {
                kids.push(handle(direction));
            }
            kids.push(inner);
            vstack().w("100%").min_h("0").children(kids).into()
        }
        DrawerDirection::Up => {
            let mut kids = vec![inner];
            if show_swipe_handle {
                kids.push(handle(direction));
            }
            vstack().w("100%").min_h("0").children(kids).into()
        }
        DrawerDirection::Right => {
            let mut row: Vec<Element> = Vec::new();
            if show_swipe_handle {
                row.push(handle(direction));
            }
            row.push(inner);
            crate::primitives::r#box::hstack()
                .w("100%")
                .h("100%")
                .min_h("0")
                .items("stretch")
                .children(row)
                .into()
        }
        DrawerDirection::Left => {
            let mut row: Vec<Element> = vec![inner];
            if show_swipe_handle {
                row.push(handle(direction));
            }
            crate::primitives::r#box::hstack()
                .w("100%")
                .h("100%")
                .min_h("0")
                .items("stretch")
                .children(row)
                .into()
        }
    };

    panel = apply_drawer_shell_surface(panel, direction, width, height);
    panel = swallow_clicks(panel.relative());
    if let Some(mut close) = chrome_close {
        close = close.absolute().top("1rem").right("1rem").z_index(1);
        panel = panel.child(close);
    }

    let frac = drawer_open_fraction(modal_id);
    let extra = drawer_drag_translate(direction, frac, axis);
    let dragging = is_drawer_dragging(modal_id);

    register_drawer_scale_background(modal_id, scale_background, frac);

    if modal {
        let mut overlay_el = build_modal_overlay(open, zs.underlay, overlay_opts);
        if apply_modal_shell_motion(
            modal_id,
            &mut overlay_el,
            &mut panel,
            ModalSurfaceMotion::from_drawer_direction(direction),
            phase,
            presence,
            extra,
            dragging,
            axis,
        ) {
            request_redraw();
        }
        let stage: Element = r#box().inset(0.0).w("100%").h("100%").child(panel).into();
        let stage = mount_viewport_portal_stage(stage, zs.panel);
        mount_modal_layer(overlay_el, stage)
    } else {
        let mut dummy_overlay = Element::new();
        if apply_modal_shell_motion(
            modal_id,
            &mut dummy_overlay,
            &mut panel,
            ModalSurfaceMotion::from_drawer_direction(direction),
            phase,
            presence,
            extra,
            dragging,
            axis,
        ) {
            request_redraw();
        }
        let stage: Element = r#box().inset(0.0).w("100%").h("100%").child(panel).into();
        mount_viewport_portal_stage(stage, zs.panel)
    }
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

/// shadcn `DrawerTrigger`：子节点为组件时挂载打开；纯文本则默认 [`Label`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_drawer("DrawerTrigger");
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

/// shadcn `DrawerClose`：点击子树关闭最近的 Drawer。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerClose(children: Option<Element>) -> Element {
    let ctx = require_drawer("DrawerClose");
    let open = ctx.open;
    match children {
        None => {
            let glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
            let icon = build_icon_text_node(glyph).text_size(16.0);
            button(format!("{}:drawer_close", ctx.id))
                .variant("ghost")
                .size("icon-sm")
                .on_click(move || open.set(false))
                .child(icon)
                .into()
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            interactive(
                label(format!("{}:drawer_close", ctx.id).as_str())
                    .text(t)
                    .into(),
            )
            .on_click(move || open.set(false))
            .into()
        }
        Some(c) => chain_click(c, move || open.set(false)),
    }
}

/// 顶/底/左/右拖柄（`cn-drawer-handle`）；通常由 [`DrawerContent`] 在 `show_swipe_handle` 时插入。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerSwipeHandle(_children: Option<Element>) -> Element {
    let ctx = require_drawer("DrawerSwipeHandle");
    drawer_swipe_handle_element(
        &ctx.id,
        ctx.open,
        DrawerDirection::Down,
        &[DrawerSnapPoint::Fraction(1.0)],
        false,
    )
}

/// `DrawerContent` 遮罩 / 方向 / chrome 选项（`ui!` / [`drawer_content`] 共用）。
#[derive(Clone, Debug)]
pub struct DrawerContentOpts {
    /// 滑入方向；默认 [`DrawerDirection::Down`]（自底向上）。
    pub direction: DrawerDirection,
    /// 右上角 ghost 关闭钮；Drawer 默认 `false`（靠拖柄 / [`DrawerClose`]）。
    pub show_close: bool,
    /// 顶/底/左/右是否在面板开放边插入拖柄；默认 `false`（对齐 Vaul 可选 Handle）。
    pub show_swipe_handle: bool,
    /// 是否模态（`false` 时不渲染遮罩）；默认 `true`。
    pub modal: bool,
    /// 遮罩色（语义 token / hex）；`None` → theme `background` RGB + 模态默认 alpha。
    pub overlay_color: Option<String>,
    /// 遮罩不透明度 0…1；`None` → 模态默认（见 [`modal_shell`]）。
    pub overlay_opacity: Option<f32>,
    /// 点击遮罩是否关闭；默认 `true`（仅 `modal=true` 时生效）。
    pub close_on_overlay_click: bool,
    /// 遮罩 backdrop 模糊半径（逻辑 px）；`None` → 模态默认。
    pub overlay_blur: Option<f32>,
    /// 左 / 右侧宽；`None` → `75%`（`max-w: 24rem`）。
    pub width: Option<String>,
    /// 顶 / 底高；`None` → `auto`（`max-h: calc(100dvh - 6rem)`）。
    pub height: Option<String>,
    /// 吸附点（视口比例或像素）；默认 `[1.0]`。
    pub snap_points: Vec<DrawerSnapPoint>,
    /// 对齐 Vaul `snapToSequentialPoint`。
    pub snap_to_sequential_point: bool,
    /// 对齐 Vaul `shouldScaleBackground` / shadcn `scaleBackground`。
    pub scale_background: bool,
}

impl Default for DrawerContentOpts {
    fn default() -> Self {
        Self {
            direction: DrawerDirection::Down,
            show_close: false,
            show_swipe_handle: false,
            modal: true,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            width: None,
            height: None,
            snap_points: vec![DrawerSnapPoint::Fraction(1.0)],
            snap_to_sequential_point: false,
            scale_background: false,
        }
    }
}

/// shadcn `DrawerContent`：Portal + Overlay + 贴边/底滑面板壳。
pub fn drawer_content(opts: DrawerContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_drawer("DrawerContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let view_id = format!("{id}__drawer_view");
    let overlay = ShellOverlayOpts::from_content_parts(
        opts.overlay_color.as_deref(),
        opts.overlay_opacity,
        opts.close_on_overlay_click,
        opts.overlay_blur,
    );
    let show_close = opts.show_close;
    let show_swipe_handle = opts.show_swipe_handle;
    let modal = opts.modal;
    let direction = opts.direction;
    let width = opts.width;
    let height = opts.height;
    let snap_points = normalize_snap_points(&opts.snap_points);
    let snap_to_sequential = opts.snap_to_sequential_point;
    let scale_background = opts.scale_background;
    let presence = use_modal_presence(&id);
    let mounted = mounted_signal(&presence);
    let had_mounted_key = format!("{id}__drawer_had_mounted");
    let had_mounted = use_signal(had_mounted_key.as_str(), || false);

    reactive_view(view_id, move || {
        let want = open.get();
        let phase = sync_modal_presence(&presence, want);
        let owner = format!("drawer:{id}");
        let m = mounted.get();
        if m && !had_mounted.peek() {
            let (vw, vh) = drawer_viewport_size();
            let axis = direction.axis_viewport(vw, vh);
            reset_drawer_open_fraction(&id, &snap_points, axis);
        }
        had_mounted.set(m);
        if !m {
            clear_drawer_scale_background(&id);
            sync_focus_trap(&owner, false, "");
            return Element::new();
        }
        let Some(zs) = modal_sync(&id, mounted) else {
            sync_focus_trap(&owner, false, "");
            return Element::new();
        };

        let mut inner = body();
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
        collect_drawer_focus_ids(&inner, &mut ring);
        if show_close {
            ring.push(format!("{id}:chrome_close"));
        }
        if phase != PresencePhase::Present || ring.is_empty() {
            sync_focus_trap(&owner, false, "");
        } else {
            sync_focus_trap(&owner, true, ring.join(","));
        }

        drawer_shell_frame(
            &id,
            open,
            &zs,
            inner,
            &overlay,
            direction,
            width.as_deref(),
            height.as_deref(),
            show_swipe_handle,
            modal,
            &snap_points,
            snap_to_sequential,
            scale_background,
            chrome_close,
            &presence,
            phase,
        )
    })
}

fn collect_drawer_focus_ids(el: &Element, out: &mut Vec<String>) {
    if el.focusable {
        if let Some(fid) = &el.focus_id {
            if !fid.is_empty() {
                out.push(fid.clone());
            }
        }
    }
    for c in &el.children {
        collect_drawer_focus_ids(c, out);
    }
}

/// 固定顶栏；对齐 `cn-drawer-header` / `p-4 gap-1.5`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.375rem")
            .p("1rem")
            .flex_shrink(0.0)
            .children(kids),
    )
}

/// 标题。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("foreground");
    match children {
        Some(c) => c.text_size(18.0).font_weight(600.0).text_color(fg),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)),
    }
}

/// 说明文案。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
}

/// Header 与 Footer 之间的滚动区。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerBody(children: Option<Element>) -> Element {
    let ctx = require_drawer("DrawerBody");
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

/// 固定底栏（`mt-auto flex flex-col gap-2 p-4`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DrawerFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
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

// ---------------------------------------------------------------------------
// 滑动手势 / snapPoints / scaleBackground（Vaul 对齐）
// ---------------------------------------------------------------------------

const DRAWER_CLOSE_THRESHOLD: f32 = 0.28;
const SCALE_BG_MAX_SHRINK: f32 = 0.04;

thread_local! {
    static DRAWER_VIEWPORT: RefCell<(f32, f32)> = const { RefCell::new((800.0, 600.0)) };
    static DRAWER_DRAG: RefCell<Option<DrawerDrag>> = const { RefCell::new(None) };
    static DRAWER_LIVE_FRAC: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    static DRAWER_SCALE_BG: RefCell<Option<DrawerScaleBg>> = const { RefCell::new(None) };
    static DRAWER_SNAP_FRACS: RefCell<HashMap<String, Vec<f32>>> = RefCell::new(HashMap::new());
}

#[derive(Clone)]
struct DrawerScaleBg {
    id: String,
    open_frac: f32,
}

#[derive(Clone)]
struct DrawerDrag {
    id: String,
    direction: DrawerDirection,
    start_x: f32,
    start_y: f32,
    start_frac: f32,
    axis_px: f32,
    snap_fracs: Vec<f32>,
    snap_to_sequential: bool,
    open: Signal<bool>,
}

/// 由窗口事件循环每帧写入逻辑视口（供吸附 travel 计算）。
pub(crate) fn set_drawer_viewport_size(w: f32, h: f32) {
    DRAWER_VIEWPORT.with(|v| *v.borrow_mut() = (w.max(1.0), h.max(1.0)));
}

pub(crate) fn drawer_viewport_size() -> (f32, f32) {
    DRAWER_VIEWPORT.with(|v| *v.borrow())
}

fn drawer_frac_signal(id: &str) -> Signal<f32> {
    use_signal(&format!("__rui_drawer_open_frac:{id}"), || 1.0_f32)
}

fn drawer_snap_index_signal(id: &str) -> Signal<Option<usize>> {
    use_signal(&format!("__rui_drawer_snap_idx:{id}"), || None::<usize>)
}

fn normalize_snap_points(points: &[DrawerSnapPoint]) -> Vec<DrawerSnapPoint> {
    if points.is_empty() {
        return vec![DrawerSnapPoint::Fraction(1.0)];
    }
    points.to_vec()
}

fn resolve_snap_fractions(points: &[DrawerSnapPoint], axis_px: f32) -> Vec<f32> {
    let mut out: Vec<f32> = points.iter().map(|p| p.to_open_fraction(axis_px)).collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    if out.is_empty() {
        out.push(1.0);
    }
    out
}

fn reset_drawer_open_fraction(id: &str, snap_points: &[DrawerSnapPoint], axis_px: f32) {
    let fracs = resolve_snap_fractions(snap_points, axis_px);
    let max_idx = fracs.len().saturating_sub(1);
    drawer_frac_signal(id).set(fracs[max_idx]);
    drawer_snap_index_signal(id).set(Some(max_idx));
}

fn drawer_open_fraction(id: &str) -> f32 {
    if is_drawer_dragging(id) {
        if let Some(f) = DRAWER_LIVE_FRAC.with(|m| m.borrow().get(id).copied()) {
            return f;
        }
    }
    drawer_frac_signal(id).peek()
}

fn set_drawer_live_frac(id: &str, frac: f32) {
    DRAWER_LIVE_FRAC.with(|m| {
        m.borrow_mut().insert(id.to_string(), frac);
    });
    DRAWER_SCALE_BG.with(|slot| {
        if let Some(bg) = slot.borrow_mut().as_mut() {
            if bg.id == id {
                bg.open_frac = frac;
            }
        }
    });
}

fn clear_drawer_live_frac(id: &str) {
    DRAWER_LIVE_FRAC.with(|m| {
        m.borrow_mut().remove(id);
    });
}

fn drawer_drag_translate(direction: DrawerDirection, open_frac: f32, axis_px: f32) -> [f32; 2] {
    let off = (1.0 - open_frac.clamp(0.0, 1.0)) * axis_px.max(1.0);
    match direction {
        DrawerDirection::Down => [0.0, off],
        DrawerDirection::Up => [0.0, -off],
        DrawerDirection::Right => [off, 0.0],
        DrawerDirection::Left => [-off, 0.0],
    }
}

fn is_drawer_dragging(id: &str) -> bool {
    DRAWER_DRAG.with(|d| d.borrow().as_ref().is_some_and(|drag| drag.id == id))
}

fn axis_delta(direction: DrawerDirection, drag: &DrawerDrag, x: f32, y: f32) -> f32 {
    match direction {
        DrawerDirection::Down => y - drag.start_y,
        DrawerDirection::Up => drag.start_y - y,
        DrawerDirection::Right => x - drag.start_x,
        DrawerDirection::Left => drag.start_x - x,
    }
}

fn begin_drawer_drag(
    id: String,
    direction: DrawerDirection,
    x: f32,
    y: f32,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
    open: Signal<bool>,
) {
    let (vw, vh) = drawer_viewport_size();
    let axis_px = direction.axis_viewport(vw, vh);
    let snap_fracs = resolve_snap_fractions(snap_points, axis_px);
    let start_frac = drawer_frac_signal(&id).peek();
    DRAWER_DRAG.with(|slot| {
        *slot.borrow_mut() = Some(DrawerDrag {
            id,
            direction,
            start_x: x,
            start_y: y,
            start_frac,
            axis_px,
            snap_fracs,
            snap_to_sequential,
            open,
        });
    });
}

fn nearest_snap_index(snap_fracs: &[f32], frac: f32) -> usize {
    snap_fracs
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - frac)
                .abs()
                .partial_cmp(&(*b - frac).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn nearest_snap(snap_fracs: &[f32], frac: f32) -> f32 {
    snap_fracs
        .get(nearest_snap_index(snap_fracs, frac))
        .copied()
        .unwrap_or(1.0)
}

fn sequential_snap_target(snap_fracs: &[f32], current: f32, delta_sign: f32) -> f32 {
    let idx = nearest_snap_index(snap_fracs, current);
    if delta_sign < -0.5 {
        snap_fracs
            .get(idx.saturating_sub(1))
            .copied()
            .unwrap_or(0.0)
    } else if delta_sign > 0.5 {
        snap_fracs
            .get((idx + 1).min(snap_fracs.len().saturating_sub(1)))
            .copied()
            .unwrap_or(1.0)
    } else {
        snap_fracs[idx]
    }
}

fn apply_snap_index(id: &str, snap_fracs: &[f32], target: f32) {
    let idx = nearest_snap_index(snap_fracs, target);
    drawer_frac_signal(id).set(snap_fracs[idx]);
    drawer_snap_index_signal(id).set(Some(idx));
}

fn cache_snap_fracs(id: &str, fracs: &[f32]) {
    DRAWER_SNAP_FRACS.with(|m| {
        m.borrow_mut().insert(id.to_string(), fracs.to_vec());
    });
}

/// 当前吸附档位索引（`None` = 未初始化）。
pub fn drawer_active_snap_point(drawer_id: &str) -> Option<usize> {
    drawer_snap_index_signal(drawer_id).peek()
}

/// 程序化切换到指定 snap 档位（对齐 Vaul `setActiveSnapPoint`）。
pub fn drawer_set_active_snap_point(drawer_id: &str, index: usize) {
    let fracs = DRAWER_SNAP_FRACS.with(|m| m.borrow().get(drawer_id).cloned());
    if let Some(fracs) = fracs {
        if let Some(&target) = fracs.get(index) {
            drawer_frac_signal(drawer_id).set(target);
            drawer_snap_index_signal(drawer_id).set(Some(index));
            request_redraw();
        }
    }
}

/// 切到下一档（更开）。
pub fn drawer_snap_to_next(drawer_id: &str) {
    let idx = drawer_active_snap_point(drawer_id).unwrap_or(0);
    drawer_set_active_snap_point(drawer_id, idx + 1);
}

/// 切到上一档（更关）。
pub fn drawer_snap_to_prev(drawer_id: &str) {
    let idx = drawer_active_snap_point(drawer_id).unwrap_or(0);
    drawer_set_active_snap_point(drawer_id, idx.saturating_sub(1));
}

fn register_drawer_scale_background(id: &str, enabled: bool, open_frac: f32) {
    if !enabled {
        clear_drawer_scale_background(id);
        return;
    }
    DRAWER_SCALE_BG.with(|slot| {
        *slot.borrow_mut() = Some(DrawerScaleBg {
            id: id.to_string(),
            open_frac,
        });
    });
}

fn clear_drawer_scale_background(id: &str) {
    DRAWER_SCALE_BG.with(|slot| {
        if slot.borrow().as_ref().is_some_and(|s| s.id == id) {
            *slot.borrow_mut() = None;
        }
    });
}

/// 对齐 Vaul `shouldScaleBackground`：缩放页面层（非 Portal 子树）。
pub(crate) fn sync_drawer_background_scale(root: &mut Element) {
    use crate::layout::layout_tree::PORTAL_LAYER_ID;
    let Some(state) = DRAWER_SCALE_BG.with(|s| s.borrow().clone()) else {
        return;
    };
    let progress = 1.0 - state.open_frac.clamp(0.0, 1.0);
    let scale = 1.0 - progress * SCALE_BG_MAX_SHRINK;
    let radius = progress * 12.0;
    for child in &mut root.children {
        if child.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        child.style.scale = [scale, scale];
        if radius > 0.1 {
            child.style.radii = [radius, radius, radius, radius];
        }
    }
}

pub(crate) fn tick_drawer_drag(root: &mut Element, x: f32, y: f32) -> bool {
    let Some(drag) = DRAWER_DRAG.with(|d| d.borrow().clone()) else {
        return false;
    };
    let delta = axis_delta(drag.direction, &drag, x, y);
    let next = (drag.start_frac - delta / drag.axis_px).clamp(0.0, 1.15);
    set_drawer_live_frac(&drag.id, next);
    let extra = drawer_drag_translate(drag.direction, next, drag.axis_px);
    crate::components::internal::modal_motion::patch_modal_panel_transform(root, &drag.id, extra);
    sync_drawer_background_scale(root);
    true
}

pub(crate) fn clear_drawer_drag_if_any() {
    let drag = DRAWER_DRAG.with(|d| d.borrow().clone());
    let Some(drag) = drag else {
        return;
    };
    DRAWER_DRAG.with(|d| *d.borrow_mut() = None);

    let frac = DRAWER_LIVE_FRAC
        .with(|m| m.borrow().get(&drag.id).copied())
        .unwrap_or_else(|| drawer_frac_signal(&drag.id).peek());
    clear_drawer_live_frac(&drag.id);
    if frac < DRAWER_CLOSE_THRESHOLD {
        drag.open.set(false);
        return;
    }
    let delta_sign = frac - drag.start_frac;
    let target = if drag.snap_to_sequential {
        sequential_snap_target(&drag.snap_fracs, frac, delta_sign)
    } else {
        nearest_snap(&drag.snap_fracs, frac)
    };
    apply_snap_index(&drag.id, &drag.snap_fracs, target);
    if target < DRAWER_CLOSE_THRESHOLD {
        drag.open.set(false);
    }
    request_redraw();
}

fn bind_drawer_drag_region(
    el: Element,
    drawer_id: &str,
    capture_id: &str,
    open: Signal<bool>,
    direction: DrawerDirection,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
) -> Element {
    let id = drawer_id.to_string();
    let caps = snap_points.to_vec();
    let cap = capture_id.to_string();
    el.focus_id(cap.clone())
        .on_pointer_down(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            begin_drawer_drag(
                id.clone(),
                direction,
                e.x,
                e.y,
                &caps,
                snap_to_sequential,
                open,
            );
            set_pointer_capture(cap.clone());
            e.stop_propagation();
        })
        .on_pointer_up(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            clear_drawer_drag_if_any();
            release_pointer_capture();
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_direction_parses_shadcn_tokens() {
        assert_eq!(DrawerDirection::from("down"), DrawerDirection::Down);
        assert_eq!(DrawerDirection::from("bottom"), DrawerDirection::Down);
        assert_eq!(DrawerDirection::from("UP"), DrawerDirection::Up);
        assert_eq!(DrawerDirection::from("left"), DrawerDirection::Left);
        assert_eq!(DrawerDirection::from("right"), DrawerDirection::Right);
    }

    #[test]
    fn snap_points_normalize_and_nearest() {
        let pts = resolve_snap_fractions(
            &[
                DrawerSnapPoint::Fraction(0.35),
                DrawerSnapPoint::Fraction(1.0),
            ],
            800.0,
        );
        assert!((nearest_snap(&pts, 0.4) - 0.35).abs() < 0.01);
    }

    #[test]
    fn snap_point_pixels_convert_to_fraction() {
        let pt = DrawerSnapPoint::Pixels(400.0);
        assert!((pt.to_open_fraction(800.0) - 0.5).abs() < 0.01);
    }
}
