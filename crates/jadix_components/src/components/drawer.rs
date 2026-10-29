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
//! ├── DrawerFooter // mt-auto; also available Button + open.set(false)
//!     └── DrawerClose         // 任意子节点点击关闭
//! ```
//!
//! Modal-shell overlay / Portal z: see [`crate::utils::modal_shell`];
//! 面板表面在本模块内实现（默认自底部滑入、顶圆角 + 拖柄）。`ui!` 中 `<Drawer>` /
//! `<DrawerContent>` is expanded by macro.
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
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
//! // B · Builder (no Trigger; open/close via external Signal)
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
use std::time::Instant;

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::{release_pointer_capture, runtime::request_redraw, set_pointer_capture};
use crate::components::button::button;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{r#box, vstack};
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
    viewport_modal_is_mounted, viewport_modal_island, viewport_modal_layer,
    viewport_modal_motion_busy, ModalSurfaceMotion, ViewportModalLayerOpts,
};

// ---------------------------------------------------------------------------
// Direction（对齐 base-ui `swipeDirection`）
// ---------------------------------------------------------------------------

/// 抽屉滑入方向（`down` = 自底向上，shadcn 默认）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawerDirection {
    /// Bottom-up (`swipeDirection="down"`).
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

    fn is_vertical(self) -> bool {
        matches!(self, Self::Down | Self::Up)
    }

    pub(crate) fn to_surface_motion(self) -> ModalSurfaceMotion {
        match self {
            Self::Down => ModalSurfaceMotion::SlideFromBottom,
            Self::Up => ModalSurfaceMotion::SlideFromTop,
            Self::Left => ModalSurfaceMotion::SlideFromLeft,
            Self::Right => ModalSurfaceMotion::SlideFromRight,
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

/// Root context: `open` + stable instance `id`.
#[derive(Clone)]
pub struct DrawerCtx {
    pub open: Signal<bool>,
    pub id: String,
}

#[derive(Clone)]
struct DrawerSwipeConfig {
    direction: DrawerDirection,
    snap_points: Vec<DrawerSnapPoint>,
    sequential: bool,
}

fn drawer_swipe_context() -> Context<DrawerSwipeConfig> {
    static CTX: OnceLock<Context<DrawerSwipeConfig>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DrawerSwipeConfig>)
}

fn drawer_context() -> Context<DrawerCtx> {
    static CTX: OnceLock<Context<DrawerCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DrawerCtx>)
}

/// Read the most recent [`Drawer`] context.
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

/// Bottom sliding Drawer entry (double B; please use `<Drawer>` for RSX).
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

    /// Aligns with Vaul `shouldScaleBackground` / shadcn `scaleBackground`.
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
            panel: Default::default(),
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

/// shadcn `cn-drawer-handle`: Slender capsule strip (`w-[100px] h-2` / left and right `w-2 h-[100px]`).
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
    el = el.rounded_full().cursor(if is_drawer_dragging(drawer_id) {
        "grabbing"
    } else {
        "grab"
    });
    bind_drawer_drag_region(
        el,
        drawer_id,
        &format!("{drawer_id}__drag"),
        open,
        direction,
        snap_points,
        snap_to_sequential,
        true,
    )
}

// ---------------------------------------------------------------------------
// Modal shell
// ---------------------------------------------------------------------------

/// 对齐 shadcn `cn-drawer-popup`：底滑默认顶圆角、`max-h: calc(100dvh - 6rem)`。
/// `6rem` inset at 16px root ≈ 96px.
fn drawer_y_max_h(vh: f32) -> f32 {
    (vh - 6.0 * crate::layout::dimension::css_root_font_size_px())
        .max(120.0)
        .min(vh.max(1.0))
}

fn drawer_panel_axis(
    direction: DrawerDirection,
    vw: f32,
    vh: f32,
    width: Option<&str>,
    height: Option<&str>,
) -> f32 {
    use crate::layout::dimension::{css_root_font_size_px, parse_css_dim, PercentAxis, UnitCtx};
    let ctx =
        UnitCtx::new(vw, vh, css_root_font_size_px()).with_containing_block(Some(vw), Some(vh));
    let (value, axis, max, default) = if direction.is_vertical() {
        (
            height,
            PercentAxis::Block,
            drawer_y_max_h(vh),
            drawer_y_max_h(vh),
        )
    } else {
        (
            width,
            PercentAxis::Inline,
            (24.0 * ctx.root_font_size).min(vw),
            vw * 0.75,
        )
    };
    value
        .and_then(parse_css_dim)
        .and_then(|v| v.resolve_px(&ctx, axis))
        .unwrap_or(default)
        .clamp(1.0, max.max(1.0))
}

/// Visible height for a Y-axis snap or drag (frac × max-h). Swipe follows this
/// box size rather than laying out at max-h and translating the overflow away.
fn drawer_y_visible_h(frac: f32, vh: f32) -> f32 {
    (frac.clamp(0.0, 1.0) * drawer_y_max_h(vh))
        .max(64.0)
        .min(drawer_y_max_h(vh))
}

/// `(visible_px, extra_translate)` for a vertical drawer.
///
/// Height tracks `frac` at rest **and** while dragging. Extra translate stays 0 so
/// the layout box is the visible panel, not a max-h sheet slid off-screen.
fn drawer_y_shell_metrics(frac: f32, vh: f32) -> (f32, [f32; 2]) {
    (drawer_y_visible_h(frac, vh), [0.0, 0.0])
}

/// 对齐 shadcn `cn-drawer-popup`：底滑默认顶圆角、`max-h: calc(100dvh - 6rem)`。
fn apply_drawer_shell_surface(
    mut panel: Element,
    direction: DrawerDirection,
    width: Option<&str>,
    height: Option<&str>,
    visible_px: Option<f32>,
) -> Element {
    let border = tokens::border();
    let radius = 8.0;
    panel = panel
        .bg(tokens::background())
        .shadow(tokens::shadow_rgba(tokens::Elevation::Xl), 16.0)
        .gap("1rem");

    match direction {
        DrawerDirection::Down => {
            panel = panel
                .absolute()
                .bottom(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .min_h(0.0)
                .overflow_y("hidden")
                // Visible snap height already includes the top inset. A second
                // calc cap can collapse the whole panel in a very short viewport.
                .max_h(if visible_px.is_some() {
                    "100dvh"
                } else {
                    "calc(100dvh - 6rem)"
                })
                .border_top(border, 1.0)
                .border_top_left_radius(radius)
                .border_top_right_radius(radius);
            panel = if let Some(px) = visible_px {
                panel.h(px)
            } else {
                panel.h(height.unwrap_or("auto"))
            };
        }
        DrawerDirection::Up => {
            panel = panel
                .absolute()
                .top(0.0)
                .left(0.0)
                .right(0.0)
                .w("100%")
                .min_h(0.0)
                .overflow_y("hidden")
                .max_h(if visible_px.is_some() {
                    "100dvh"
                } else {
                    "calc(100dvh - 6rem)"
                })
                .border_bottom(border, 1.0)
                .border_bottom_left_radius(radius)
                .border_bottom_right_radius(radius);
            panel = if let Some(px) = visible_px {
                panel.h(px)
            } else {
                panel.h(height.unwrap_or("auto"))
            };
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
                .overflow_x("hidden")
                .border_left(border, 1.0)
                .border_top_left_radius(radius)
                .border_bottom_left_radius(radius);
            panel = crate::authoring::widgets::classes::with_default_class_list(
                panel,
                "max-sm:max-w-full",
            );
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
                .overflow_x("hidden")
                .border_right(border, 1.0)
                .border_top_right_radius(radius)
                .border_bottom_right_radius(radius);
            panel = crate::authoring::widgets::classes::with_default_class_list(
                panel,
                "max-sm:max-w-full",
            );
        }
    }
    panel
}

fn drawer_shell_panel(
    modal_id: &str,
    open: Signal<bool>,
    inner: Element,
    direction: DrawerDirection,
    width: Option<&str>,
    height: Option<&str>,
    show_swipe_handle: bool,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
    scale_background: bool,
    chrome_close: Option<Element>,
    panel_chrome: crate::components::base::SlotChrome,
    background_lifetime: &std::rc::Rc<()>,
) -> Element {
    let (vw, vh) = drawer_viewport_size();
    let axis = drawer_panel_axis(direction, vw, vh, width, height);
    DRAWER_PANEL_AXIS.with(|m| {
        m.borrow_mut().insert(modal_id.to_string(), axis);
    });
    let snap_fracs = resolve_snap_fractions(snap_points, axis);
    cache_snap_fracs(modal_id, &snap_fracs);

    let _ = drawer_layout_bump(modal_id).get();
    let frac = drawer_open_fraction_tracked(modal_id);
    let dragging = is_drawer_dragging(modal_id);
    let visible_px = if direction.is_vertical() {
        Some((frac.clamp(0.0, 1.0) * axis).max(1.0))
    } else {
        None
    };

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
            vstack()
                .w("100%")
                .h("100%")
                .min_h("0")
                .overflow_y("hidden")
                .children(kids)
                .into()
        }
        DrawerDirection::Up => {
            let mut kids = vec![inner];
            if show_swipe_handle {
                kids.push(handle(direction));
            }
            vstack()
                .w("100%")
                .h("100%")
                .min_h("0")
                .overflow_y("hidden")
                .children(kids)
                .into()
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

    panel = apply_drawer_shell_surface(panel, direction, width, height, visible_px);
    panel.set_attr("data-slot", "drawer-content");
    panel = panel_chrome.apply(panel);
    // Keep `position: absolute` from the shell surface — `relative()` would put the
    // panel in stage flow (flex-start / opposite edge) so enter looked like it
    // crossed the viewport to the dock. Absolute is already a containing block
    // for the chrome close button.
    panel = swallow_clicks(panel);
    if let Some(mut close) = chrome_close {
        close = close.absolute().top("1rem").right("1rem").z_index(1);
        panel = panel.child(close);
    }

    panel = bind_drawer_drag_region(
        panel,
        modal_id,
        &format!("{modal_id}__panel_drag"),
        open,
        direction,
        snap_points,
        snap_to_sequential,
        false,
    );

    // Scale only at rest after enter/snap — live swipe, enter, and exit would
    // zoom the page under the mask (Vaul applies this to a single wrapper).
    let rest_frac = drawer_frac_signal(modal_id).peek();
    let motion_busy = viewport_modal_motion_busy(modal_id);
    register_drawer_scale_background(
        modal_id,
        scale_background && !motion_busy && !dragging,
        rest_frac,
        background_lifetime,
    );
    panel
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
DrawerTriggerProps [];
/// shadcn `DrawerTrigger`：子节点为组件时挂载打开；纯文本则默认 [`Label`]。
#[allow(non_snake_case)]
pub fn DrawerTrigger(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_drawer("DrawerTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__trigger", ctx.id)
    } else {
        id
    };

    match children {
        None => interactive(
            label(trigger_id.as_str())
                .text(crate::tr_default_zh("jadix.drawer.open", "Open", "打开"))
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
DrawerCloseProps [];
/// shadcn `DrawerClose`：点击子树关闭最近的 Drawer。
#[allow(non_snake_case)]
pub fn DrawerClose(
plain [] children: Option<Element>,
) -> Element {
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
}

crate::__component_contract! {
DrawerSwipeHandleProps [];
/// Top/bottom/left/right drag handle (`cn-drawer-handle`); usually inserted by [`DrawerContent`] during `show_swipe_handle`.
#[allow(non_snake_case)]
pub fn DrawerSwipeHandle(
plain [] _children: Option<Element>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let ctx = require_drawer("DrawerSwipeHandle");
    let config = use_context(drawer_swipe_context()).unwrap_or(DrawerSwipeConfig {
        direction: DrawerDirection::Down,
        snap_points: vec![DrawerSnapPoint::Fraction(1.0)],
        sequential: false,
    });
    drawer_swipe_handle_element(
        &ctx.id,
        ctx.open,
        config.direction,
        &config.snap_points,
        config.sequential,
    )
}
}

/// `DrawerContent` 遮罩 / 方向 / chrome 选项（`ui!` / [`drawer_content`] 共用）。
#[derive(Clone)]
pub struct DrawerContentOpts {
    /// 滑入方向；默认 [`DrawerDirection::Down`]（自底向上）。
    pub direction: DrawerDirection,
    /// 右上角 ghost 关闭钮；Drawer 默认 `false`（靠拖柄 / [`DrawerClose`]）。
    pub show_close: bool,
    /// 顶/底/左/右是否在面板开放边插入拖柄；默认 `false`（对齐 Vaul 可选 Handle）。
    pub show_swipe_handle: bool,
    /// 是否模态（`false` 时不渲染遮罩）；默认 `true`。
    pub modal: bool,
    /// Overlay color (semantic token / hex); `None` → theme `background` RGB + modal default alpha.
    pub overlay_color: Option<String>,
    /// 遮罩不透明度 0…1；`None` → 模态默认（见 [`modal_shell`]）。
    pub overlay_opacity: Option<f32>,
    /// 点击遮罩是否关闭；默认 `true`（仅 `modal=true` 时生效）。
    pub close_on_overlay_click: bool,
    /// 遮罩 backdrop 模糊半径（逻辑 px）；`None` → 模态默认。
    pub overlay_blur: Option<f32>,
    /// Left / right width; `None` → `75%` (`max-w: 24rem`).
    pub width: Option<String>,
    /// Top / bottom height; `None` → `auto` (`max-h: calc(100dvh - 6rem)`).
    pub height: Option<String>,
    /// 吸附点（视口比例或像素）；默认 `[1.0]`。
    pub snap_points: Vec<DrawerSnapPoint>,
    /// 对齐 Vaul `snapToSequentialPoint`。
    pub snap_to_sequential_point: bool,
    /// Aligns with Vaul `shouldScaleBackground` / shadcn `scaleBackground`.
    pub scale_background: bool,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
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
            panel: Default::default(),
        }
    }
}

impl DrawerContentOpts {
    crate::impl_content_opts_chrome!();
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
    let panel_chrome = opts.panel;
    let had_mounted_key = format!("{id}__drawer_had_mounted");
    let had_mounted = use_signal(had_mounted_key.as_str(), || false);

    let background_lifetime = std::rc::Rc::new(());
    viewport_modal_island(view_id, move || {
        let owner = format!("drawer:{id}");
        let (vw, vh) = drawer_viewport_size();
        let axis = drawer_panel_axis(direction, vw, vh, width.as_deref(), height.as_deref());
        let frac = drawer_open_fraction_tracked(&id);
        DRAWER_SCRIM_ALPHA.with(|m| {
            m.borrow_mut().insert(id.clone(), overlay.color[3]);
        });
        let mut live_overlay = overlay.clone();
        live_overlay.color[3] *= frac.clamp(0.0, 1.0);
        let dragging = is_drawer_dragging(&id);
        let extra = if direction.is_vertical() {
            drawer_y_shell_metrics(frac, vh).1
        } else {
            drawer_drag_translate(direction, frac, axis)
        };
        let mut layer_opts = ViewportModalLayerOpts::slide(direction.to_surface_motion(), axis);
        layer_opts.extra_translate = extra;
        layer_opts.dragging = dragging;
        layer_opts.overlay = modal;
        layer_opts.stage_overflow_hidden = true;
        let mut layer = viewport_modal_layer(&id, open, &live_overlay, layer_opts, |phase| {
            let Some(phase) = phase else {
                had_mounted.set(false);
                clear_drawer_scale_background(&id);
                clear_drawer_y_settling(&id);
                clear_drawer_live_frac(&id);
                sync_focus_trap(&owner, false, "");
                return None;
            };
            let m = viewport_modal_is_mounted(&id, open);
            if m && !had_mounted.peek() {
                reset_drawer_open_fraction(&id, &snap_points, axis);
            }
            had_mounted.set(m);

            provide_context(
                drawer_swipe_context(),
                DrawerSwipeConfig {
                    direction,
                    snap_points: snap_points.clone(),
                    sequential: snap_to_sequential,
                },
            );
            let mut inner = body();
            inner = inner
                .w("100%")
                .h("100%")
                .min_h("0")
                .flex_col()
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .overflow_y("hidden")
                .gap("0");

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

            Some(drawer_shell_panel(
                &id,
                open,
                inner,
                direction,
                width.as_deref(),
                height.as_deref(),
                show_swipe_handle,
                &snap_points,
                snap_to_sequential,
                scale_background,
                chrome_close,
                panel_chrome.clone(),
                &background_lifetime,
            ))
        });
        let snap_id = id.clone();
        crate::authoring::deadline::attach(
            &mut layer,
            crate::authoring::deadline::Visibility::Always,
            move |context| {
                let changed = poll_drawer_snap(&snap_id, context.now);
                drain_drawer_layout_bump(&snap_id);
                let active = DRAWER_Y_SETTLING.with(|snaps| snaps.borrow().contains_key(&snap_id));
                crate::authoring::deadline::Poll {
                    redraw: changed,
                    next_wake: active.then_some(context.now + std::time::Duration::from_millis(16)),
                }
            },
        );
        attach_drawer_background(&mut layer, &id, &background_lifetime);
        layer
    })
}

fn attach_drawer_background(layer: &mut Element, id: &str, background_lifetime: &std::rc::Rc<()>) {
    let cleanup_id = id.to_string();
    let cleanup_lifetime = background_lifetime.clone();
    crate::authoring::presentation::attach_tree_with_cleanup(
        layer,
        crate::authoring::presentation::Phase::BeforeLayout,
        |root, _, _| {
            sync_drawer_background_scale(root);
            LayoutStatus::Clean
        },
        move |root, _, _| {
            let owns_background = DRAWER_SCALE_BG.with(|state| {
                state.borrow().as_ref().is_some_and(|state| {
                    state
                        .lifetime
                        .ptr_eq(&std::rc::Rc::downgrade(&cleanup_lifetime))
                })
            });
            if owns_background {
                clear_drawer_scale_background(&cleanup_id);
            }
            sync_drawer_background_scale(root);
            LayoutStatus::StyleChanged
        },
    );
}

fn collect_drawer_focus_ids(el: &Element, out: &mut Vec<String>) {
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
DrawerHeaderProps [];
/// 固定顶栏；对齐 `cn-drawer-header` / `p-4 gap-1.5`。
#[allow(non_snake_case)]
pub fn DrawerHeader(
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
DrawerTitleProps [];
/// Title.
#[allow(non_snake_case)]
pub fn DrawerTitle(
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
DrawerDescriptionProps [];
/// Description copy.
#[allow(non_snake_case)]
pub fn DrawerDescription(
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
DrawerBodyProps [];
/// Header 与 Footer 之间的滚动区。
#[allow(non_snake_case)]
pub fn DrawerBody(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_drawer("DrawerBody");
    let child = children.unwrap_or_else(Element::new);
    let scroll_id = format!("{}__body", ctx.id);
    // `scroll_area` defaults to `h: 100%`. As a flex sibling of Header/Footer that
    // preferred size plus `min-h: 0` shrinks the body to 0 on a half-open rest
    // height — Header+Footer remain, content cannot scroll.
    let mut body: Element = scroll_area(scroll_id.as_str())
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
        .into();
    body = body.flex_basis(0.0);
    body
}
}

crate::__component_contract! {
DrawerFooterProps [];
/// Sticky footer (`mt-auto flex flex-col gap-2 p-4`). Pins to the **visible**
/// bottom of the panel — on a half-open snap that is the drawer edge, not the
/// end of the scrolling copy. Put actions in [`DrawerBody`] when they should scroll.
#[allow(non_snake_case)]
pub fn DrawerFooter(
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
    DrawerTriggerBuilder,
    drawer_trigger,
    DrawerTrigger,
    DrawerTriggerProps
);
impl_simple_slot_builder!(
    DrawerCloseBuilder,
    drawer_close_part,
    DrawerClose,
    DrawerCloseProps
);
impl_simple_slot_builder!(
    DrawerHeaderBuilder,
    drawer_header,
    DrawerHeader,
    DrawerHeaderProps
);
impl_simple_slot_builder!(
    DrawerTitleBuilder,
    drawer_title,
    DrawerTitle,
    DrawerTitleProps
);
impl_simple_slot_builder!(
    DrawerDescriptionBuilder,
    drawer_description,
    DrawerDescription,
    DrawerDescriptionProps
);
impl_simple_slot_builder!(DrawerBodyBuilder, drawer_body, DrawerBody, DrawerBodyProps);
impl_simple_slot_builder!(
    DrawerFooterBuilder,
    drawer_footer,
    DrawerFooter,
    DrawerFooterProps
);

pub struct DrawerContentSlotBuilder {
    opts: DrawerContentOpts,
}

pub fn drawer_content_slot() -> DrawerContentSlotBuilder {
    DrawerContentSlotBuilder {
        opts: DrawerContentOpts::default(),
    }
}

impl DrawerContentSlotBuilder {
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
    pub fn direction(mut self, direction: impl Into<DrawerDirection>) -> Self {
        self.opts.direction = direction.into();
        self
    }
    pub fn show_close(mut self, v: bool) -> Self {
        self.opts.show_close = v;
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        drawer_content(self.opts, body)
    }
}

// ---------------------------------------------------------------------------
// 滑动手势 / snapPoints / scaleBackground（Vaul 对齐）
// ---------------------------------------------------------------------------

const DRAWER_CLOSE_THRESHOLD: f32 = 0.28;
/// Vaul-ish dismiss velocity along the close axis (logical px / s).
const DRAWER_CLOSE_VELOCITY: f32 = 900.0;
const SCALE_BG_MAX_SHRINK: f32 = 0.04;
const DRAWER_DRAG_COMMIT_PX: f32 = 2.0;

crate::authoring::window::window_local! {
    static DRAWER_DRAG: RefCell<Option<DrawerDrag>> = const { RefCell::new(None) };
    static DRAWER_LIVE_FRAC: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    static DRAWER_SCALE_BG: RefCell<Option<DrawerScaleBg>> = const { RefCell::new(None) };
    static DRAWER_SNAP_FRACS: RefCell<HashMap<String, Vec<f32>>> = RefCell::new(HashMap::new());
    static DRAWER_PANEL_AXIS: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    static DRAWER_SCRIM_ALPHA: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
    static DRAWER_SCALE_BG_APPLIED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static DRAWER_Y_SETTLING: RefCell<HashMap<String, DrawerSettle>> = RefCell::new(HashMap::new());
    /// `tick_drawer_drag` runs while `AppState` is borrowed. Queue shell layout
    /// bumps and drain after the RefCell drop (same contract as ColorPicker SV).
    static PENDING_DRAWER_LAYOUT_BUMP: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct DrawerScaleBg {
    lifetime: std::rc::Weak<()>,
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
    last_x: f32,
    last_y: f32,
    last_t: Instant,
    /// Positive = closing along swipe direction (px/s).
    velocity: f32,
    capture_id: String,
    /// Handle starts committed; panel waits for [`DRAWER_DRAG_COMMIT_PX`].
    committed: bool,
    captured: bool,
}

/// 逻辑视口由 [`crate::utils::modal_surface`] 持有；Drawer 只转发。
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn set_drawer_viewport_size(w: f32, h: f32) {
    crate::utils::modal_surface::set_modal_viewport_size(w, h);
}

pub(crate) fn drawer_viewport_size() -> (f32, f32) {
    crate::utils::modal_surface::modal_viewport_size()
}

fn drawer_frac_signal(id: &str) -> Signal<f32> {
    use_signal(&format!("__jadix_drawer_open_frac:{id}"), || 1.0_f32)
}

fn drawer_layout_bump(id: &str) -> Signal<u32> {
    use_signal(&format!("__jadix_drawer_layout_bump:{id}"), || 0_u32)
}

fn bump_drawer_shell_layout(id: &str) {
    drawer_layout_bump(id).update(|n| *n = n.wrapping_add(1));
}

fn queue_drawer_layout_bump(id: &str) {
    PENDING_DRAWER_LAYOUT_BUMP.with(|q| {
        let mut q = q.borrow_mut();
        if !q.iter().any(|s| s == id) {
            q.push(id.to_string());
        }
    });
}

/// Apply queued vertical-drag shell layout bumps. Call only when `AppState` is not borrowed.
pub(crate) fn drain_drawer_layout_bumps() {
    let ids = PENDING_DRAWER_LAYOUT_BUMP.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for id in ids {
        bump_drawer_shell_layout(&id);
    }
}

fn clear_drawer_y_settling(id: &str) {
    DRAWER_Y_SETTLING.with(|s| {
        s.borrow_mut().remove(id);
    });
}

#[derive(Clone)]
struct DrawerSettle {
    from: f32,
    target: f32,
    started: Instant,
}

/// Run outside the live tree borrow, alongside the existing modal frame pump.
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn tick_drawer_snaps() -> bool {
    tick_drawer_snaps_at(Instant::now())
}

fn poll_drawer_snap(id: &str, now: Instant) -> bool {
    let Some(snap) = DRAWER_Y_SETTLING.with(|snaps| snaps.borrow().get(id).cloned()) else {
        return false;
    };
    let time = (now.saturating_duration_since(snap.started).as_secs_f32() / 0.25).min(1.0);
    let eased = 1.0 - (1.0 - time).powi(3);
    set_drawer_live_frac(id, snap.from + (snap.target - snap.from) * eased);
    if time >= 1.0 {
        clear_drawer_y_settling(id);
        clear_drawer_live_frac(id);
    }
    queue_drawer_layout_bump(id);
    let (width, height) = drawer_viewport_size();
    crate::app::damage::push_frame_paint_damage([0.0, 0.0, width, height]);
    true
}

#[cfg(all(test, feature = "product-tests"))]
fn tick_drawer_snaps_at(now: Instant) -> bool {
    let ids = DRAWER_Y_SETTLING.with(|snaps| snaps.borrow().keys().cloned().collect::<Vec<_>>());
    for id in ids {
        poll_drawer_snap(&id, now);
    }
    DRAWER_Y_SETTLING.with(|snaps| !snaps.borrow().is_empty())
}

fn drain_drawer_layout_bump(id: &str) {
    let pending = PENDING_DRAWER_LAYOUT_BUMP.with(|pending| {
        let mut pending = pending.borrow_mut();
        let queued = pending.iter().any(|entry| entry == id);
        pending.retain(|entry| entry != id);
        queued
    });
    if pending {
        bump_drawer_shell_layout(id);
    }
}

fn drawer_snap_index_signal(id: &str) -> Signal<Option<usize>> {
    use_signal(&format!("__jadix_drawer_snap_idx:{id}"), || None::<usize>)
}

fn normalize_snap_points(points: &[DrawerSnapPoint]) -> Vec<DrawerSnapPoint> {
    if points.is_empty() {
        return vec![DrawerSnapPoint::Fraction(1.0)];
    }
    points.to_vec()
}

fn resolve_snap_fractions(points: &[DrawerSnapPoint], axis_px: f32) -> Vec<f32> {
    let mut out: Vec<f32> = points
        .iter()
        .map(|p| p.to_open_fraction(axis_px))
        .filter(|p| p.is_finite())
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    if out.is_empty() {
        out.push(1.0);
    }
    out
}

fn reset_drawer_open_fraction(id: &str, snap_points: &[DrawerSnapPoint], axis_px: f32) {
    clear_drawer_live_frac(id);
    clear_drawer_y_settling(id);
    let fracs = resolve_snap_fractions(snap_points, axis_px);
    let max_idx = fracs.len().saturating_sub(1);
    drawer_frac_signal(id).set(fracs[max_idx]);
    drawer_snap_index_signal(id).set(Some(max_idx));
}

fn drawer_open_fraction(id: &str) -> f32 {
    if let Some(f) = DRAWER_LIVE_FRAC.with(|m| m.borrow().get(id).copied()) {
        return f;
    }
    drawer_frac_signal(id).peek()
}

/// Subscribe so `apply_snap_index` rebuilds rest extra (peek-only left the last drag pose).
fn drawer_open_fraction_tracked(id: &str) -> f32 {
    // Retain the subscription even while a live swipe temporarily owns the pose.
    let rest = drawer_frac_signal(id).get();
    DRAWER_LIVE_FRAC
        .with(|m| m.borrow().get(id).copied())
        .unwrap_or(rest)
}

fn set_drawer_live_frac(id: &str, frac: f32) {
    DRAWER_LIVE_FRAC.with(|m| {
        m.borrow_mut().insert(id.to_string(), frac);
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
    DRAWER_DRAG.with(|d| {
        d.borrow()
            .as_ref()
            .is_some_and(|drag| drag.id == id && drag.committed)
    })
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
    capture_id: String,
    exclusive: bool,
) {
    let (vw, vh) = drawer_viewport_size();
    let axis_px = if direction.is_vertical() {
        drawer_y_max_h(vh)
    } else {
        direction.axis_viewport(vw, vh)
    };
    let axis_px = DRAWER_PANEL_AXIS
        .with(|m| m.borrow().get(&id).copied())
        .unwrap_or(axis_px);
    let snap_fracs = resolve_snap_fractions(snap_points, axis_px);
    let start_frac = drawer_open_fraction(&id);
    if exclusive {
        clear_drawer_y_settling(&id);
    }
    let now = Instant::now();
    if exclusive {
        set_drawer_live_frac(&id, start_frac);
        set_pointer_capture(capture_id.clone());
    }
    // Y-axis swipe resizes height from `frac`; extra stays 0. Horizontal still translates.
    let extra = if direction.is_vertical() {
        [0.0, 0.0]
    } else {
        drawer_drag_translate(direction, start_frac, axis_px)
    };
    if exclusive {
        crate::utils::modal_motion::note_modal_shell_drag(&id, extra, true, start_frac);
    }
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
            last_x: x,
            last_y: y,
            last_t: now,
            velocity: 0.0,
            capture_id,
            committed: exclusive,
            captured: exclusive,
        });
    });
    crate::utils::drag::routing::register(
        "drawer",
        crate::utils::drag::routing::DragHandlers::new(
            dispatch_drawer_drag,
            clear_drawer_drag_if_any,
        )
        .paint_sampling(tick_drawer_drag, drain_drawer_layout_bumps),
    );
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

fn sequential_snap_target(snap_fracs: &[f32], start_frac: f32, delta_sign: f32) -> f32 {
    // Select by neighboring midpoints, then limit travel to one gear.
    let idx = nearest_snap_index(snap_fracs, start_frac);
    let nearest = nearest_snap_index(snap_fracs, start_frac + delta_sign);
    snap_fracs[nearest.clamp(idx.saturating_sub(1), (idx + 1).min(snap_fracs.len() - 1))]
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
            clear_drawer_y_settling(drawer_id);
            clear_drawer_live_frac(drawer_id);
            drawer_frac_signal(drawer_id).set(target);
            drawer_snap_index_signal(drawer_id).set(Some(index));
            request_redraw();
        }
    }
}

/// Switch to the next gear (further open).
pub fn drawer_snap_to_next(drawer_id: &str) {
    let idx = drawer_active_snap_point(drawer_id).unwrap_or(0);
    drawer_set_active_snap_point(drawer_id, idx + 1);
}

/// Switch to the previous gear (more off).
pub fn drawer_snap_to_prev(drawer_id: &str) {
    let idx = drawer_active_snap_point(drawer_id).unwrap_or(0);
    drawer_set_active_snap_point(drawer_id, idx.saturating_sub(1));
}

fn register_drawer_scale_background(
    id: &str,
    enabled: bool,
    open_frac: f32,
    lifetime: &std::rc::Rc<()>,
) {
    if !enabled {
        clear_drawer_scale_background(id);
        return;
    }
    DRAWER_SCALE_BG.with(|slot| {
        *slot.borrow_mut() = Some(DrawerScaleBg {
            lifetime: std::rc::Rc::downgrade(lifetime),
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

/// Vaul `shouldScaleBackground`: fully open shrinks the page; closed is identity.
/// Do **not** drive this from the live swipe frac — that zooms the blurred underlay.
fn drawer_background_scale(open_frac: f32) -> f32 {
    1.0 - open_frac.clamp(0.0, 1.0) * SCALE_BG_MAX_SHRINK
}

fn reset_non_portal_child_scale(root: &mut Element) {
    use crate::layout::layout_tree::PORTAL_LAYER_ID;
    for child in &mut root.children {
        if child.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        if let Some(saved) = child.attrs.remove("jadix_drawer_background_scale") {
            let values: Vec<f32> = saved
                .split(',')
                .filter_map(|value| value.parse().ok())
                .collect();
            if values.len() == 4 && child.style.scale == [values[2], values[3]] {
                child.style.scale = [values[0], values[1]];
            }
        }
    }
}

/// 对齐 Vaul `shouldScaleBackground`：缩放页面层（非 Portal 子树）。
pub(crate) fn sync_drawer_background_scale(root: &mut Element) {
    use crate::layout::layout_tree::PORTAL_LAYER_ID;
    let Some(state) = DRAWER_SCALE_BG.with(|s| s.borrow().clone()) else {
        if DRAWER_SCALE_BG_APPLIED.get() {
            reset_non_portal_child_scale(root);
            DRAWER_SCALE_BG_APPLIED.set(false);
        }
        return;
    };
    reset_non_portal_child_scale(root);
    let scale = drawer_background_scale(state.open_frac);
    for child in &mut root.children {
        if child.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
            continue;
        }
        let base = child.style.scale;
        let applied = [base[0] * scale, base[1] * scale];
        child.set_attr(
            "jadix_drawer_background_scale",
            format!("{},{},{},{}", base[0], base[1], applied[0], applied[1]),
        );
        child.style.scale = applied;
    }
    DRAWER_SCALE_BG_APPLIED.set(true);
}

fn dispatch_drawer_drag(_x: f32, _y: f32) -> bool {
    drain_drawer_layout_bumps();
    DRAWER_DRAG.with(|drag| drag.borrow().is_some())
}

pub(crate) fn tick_drawer_drag(root: &mut Element, x: f32, y: f32) -> bool {
    let Some(mut drag) = DRAWER_DRAG.with(|d| d.borrow().clone()) else {
        return false;
    };
    if x == drag.last_x && y == drag.last_y {
        return false;
    }
    let now = Instant::now();
    // Sub-frame dt (1ms) explodes px/s and false-triggers flick-dismiss on the next gesture.
    let dt = now
        .duration_since(drag.last_t)
        .as_secs_f32()
        .max(1.0 / 60.0);
    let close_delta = match drag.direction {
        DrawerDirection::Down => y - drag.last_y,
        DrawerDirection::Up => drag.last_y - y,
        DrawerDirection::Right => x - drag.last_x,
        DrawerDirection::Left => drag.last_x - x,
    };
    drag.velocity = close_delta / dt;
    drag.last_x = x;
    drag.last_y = y;
    drag.last_t = now;

    let delta = axis_delta(drag.direction, &drag, x, y);
    if !drag.committed {
        if delta.abs() < DRAWER_DRAG_COMMIT_PX {
            DRAWER_DRAG.with(|d| *d.borrow_mut() = Some(drag));
            return false;
        }
        drag.committed = true;
        clear_drawer_y_settling(&drag.id);
        if !drag.captured {
            set_pointer_capture(drag.capture_id.clone());
            drag.captured = true;
        }
    }

    let next = (drag.start_frac - delta / drag.axis_px).clamp(0.0, 1.15);
    set_drawer_live_frac(&drag.id, next);
    let overlay_tid = format!("{}__modal_overlay", drag.id);
    if let Some(overlay) = crate::core::tree::find_in_tree_mut(root, |n| {
        n.template_id.as_deref() == Some(overlay_tid.as_str())
    }) {
        let alpha = DRAWER_SCRIM_ALPHA
            .with(|m| m.borrow().get(&drag.id).copied())
            .unwrap_or(0.25);
        overlay.style.bg_color[3] = alpha * next.clamp(0.0, 1.0);
    }
    if drag.direction.is_vertical() {
        crate::utils::modal_motion::note_modal_shell_drag(&drag.id, [0.0, 0.0], true, next);
        crate::utils::modal_motion::patch_modal_overlay_opacity(root, &drag.id, 1.0);
        queue_drawer_layout_bump(&drag.id);
        request_redraw();
    } else {
        let extra = drawer_drag_translate(drag.direction, next, drag.axis_px);
        crate::utils::modal_motion::note_modal_shell_drag(&drag.id, extra, true, next);
        crate::utils::modal_motion::patch_modal_panel_transform(root, &drag.id, extra);
        crate::utils::modal_motion::patch_modal_overlay_opacity(root, &drag.id, 1.0);
    }
    DRAWER_DRAG.with(|d| *d.borrow_mut() = Some(drag));
    true
}

pub(crate) fn clear_drawer_drag_if_any() {
    crate::utils::drag::routing::unregister("drawer");
    let drag = DRAWER_DRAG.with(|d| d.borrow().clone());
    let Some(drag) = drag else {
        return;
    };
    DRAWER_DRAG.with(|d| *d.borrow_mut() = None);
    if drag.captured {
        release_pointer_capture();
    }

    let frac = DRAWER_LIVE_FRAC
        .with(|m| m.borrow().get(&drag.id).copied())
        .unwrap_or_else(|| drawer_frac_signal(&drag.id).peek());
    if !drag.committed {
        return;
    }
    clear_drawer_live_frac(&drag.id);
    // Release restores the full modal scrim. Invalidate its entire silhouette,
    // not only the resized panel, or later Body scroll leaves opacity bands.
    let (vw, vh) = drawer_viewport_size();
    crate::app::damage::push_frame_paint_damage([0.0, 0.0, vw, vh]);

    let lowest = drag.snap_fracs.first().copied().unwrap_or(1.0);
    let dismiss = should_dismiss_drawer(frac, drag.velocity, lowest);
    if dismiss {
        // Vertical drag already resized the panel. Keep that live height for exit;
        // a second full-axis translate or restoring the full height would jump.
        let extra = if drag.direction.is_vertical() {
            set_drawer_live_frac(&drag.id, frac);
            [0.0, 0.0]
        } else {
            drawer_drag_translate(drag.direction, frac, drag.axis_px)
        };
        crate::utils::modal_motion::note_modal_shell_drag(&drag.id, extra, false, frac);
        drag.open.set(false);
        request_redraw();
        return;
    }
    let delta_sign = frac - drag.start_frac;
    let target = if drag.snap_to_sequential {
        sequential_snap_target(&drag.snap_fracs, drag.start_frac, delta_sign)
    } else {
        nearest_snap(&drag.snap_fracs, frac)
    };
    if drag.direction.is_vertical() && (target - frac).abs() > 0.001 {
        set_drawer_live_frac(&drag.id, frac);
        DRAWER_Y_SETTLING.with(|s| {
            s.borrow_mut().insert(
                drag.id.clone(),
                DrawerSettle {
                    from: frac,
                    target,
                    started: Instant::now(),
                },
            );
        });
    }
    apply_snap_index(&drag.id, &drag.snap_fracs, target);
    if drag.direction.is_vertical() {
        crate::utils::modal_motion::note_modal_shell_drag(&drag.id, [0.0, 0.0], false, target);
        bump_drawer_shell_layout(&drag.id);
    } else {
        let from = drawer_drag_translate(drag.direction, frac, drag.axis_px);
        let to = drawer_drag_translate(drag.direction, target, drag.axis_px);
        crate::utils::modal_motion::note_modal_shell_drag(&drag.id, to, false, target);
        crate::utils::modal_motion::arm_modal_slide_settle(&drag.id, from);
    }
    request_redraw();
}

fn should_dismiss_drawer(frac: f32, velocity: f32, lowest_snap: f32) -> bool {
    // A configured low snap is a valid rest pose, not an implicit dismissal.
    let threshold = DRAWER_CLOSE_THRESHOLD.min(lowest_snap * 0.56);
    if frac < threshold {
        return true;
    }
    // Flick-close only when already well below the lowest rest snap — not a
    // high instantaneous px/s while parked on 0.5.
    velocity > DRAWER_CLOSE_VELOCITY && frac < lowest_snap && frac < threshold + lowest_snap * 0.16
}

fn bind_drawer_drag_region(
    el: Element,
    drawer_id: &str,
    capture_id: &str,
    open: Signal<bool>,
    direction: DrawerDirection,
    snap_points: &[DrawerSnapPoint],
    snap_to_sequential: bool,
    exclusive: bool,
) -> Element {
    let id = drawer_id.to_string();
    let caps = snap_points.to_vec();
    let cap = capture_id.to_string();
    el.focus_id(cap.clone())
        .on_pointer_down(move |e: &PointerEvent| {
            if e.button != winit::event::MouseButton::Left {
                return;
            }
            if exclusive {
                // Handle: bubble + capture so the gesture is exclusive.
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
                    cap.clone(),
                    true,
                );
                e.stop_propagation();
                return;
            }
            // Nested controls and Body scrolling own their pointer gesture.
            if e.target_focus_id
                .as_deref()
                .is_some_and(|target| target != cap)
            {
                return;
            }
            // Panel: start on capture (plain content) or target (blank panel).
            if e.phase == EventPhase::Bubble {
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
                cap.clone(),
                false,
            );
        })
        .on_pointer_up(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            clear_drawer_drag_if_any();
            release_pointer_capture();
        })
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/drawer.rs"]
mod tests;
