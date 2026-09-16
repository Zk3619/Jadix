//! # Resizable
//!
//! 对照 ShadCN **base** [`resizable.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/resizable.tsx)：
//!
//! ```text
//! ResizablePanelGroup     // Root：orientation → flex-row / flex-col；sizes 为百分比
//! ├── ResizablePanel      // 面板：defaultSize / minSize / maxSize（%）
//! └── ResizableHandle     // 分隔条：可选 withHandle 握柄；指针拖拽调宽
//! ```
//!
//! 样式对位 `cn-resizable-panel-group` / `cn-resizable-handle`：`w-full h-full` · handle
//! `w-px` / `h-px` · `bg-border` · 可选握柄小条。
//!
//! `ui!` 中 `<ResizablePanelGroup>` 由宏展开为 [`resizable_panel_group_scope`]。
//! Builder [`resizable`] 两栏便捷入口与复合面共用同一套实现。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX（camelCase 宏别名：withHandle / defaultSize / minSize / maxSize）
//! ui! {
//!     <ResizablePanelGroup id="split" orientation="horizontal" h="12rem">
//!         <ResizablePanel defaultSize={30.0} minSize={15.0}>
//!             <Label>"Left"</Label>
//!         </ResizablePanel>
//!         <ResizableHandle withHandle={true} />
//!         <ResizablePanel defaultSize={70.0}>
//!             <Label>"Right"</Label>
//!         </ResizablePanel>
//!     </ResizablePanelGroup>
//! }
//!
//! // B · Builder (snake_case only)
//! resizable("split")
//!     .orientation("horizontal")
//!     .height(192.0)
//!     .default_sizes([30.0, 70.0])
//!     .min_sizes([15.0, 15.0])
//!     .left(|| label("").text("Left").into())
//!     .right(|| label("").text("Right").into())
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use rui_macros::component;

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::components::base::{ensure_data_slot, SlotChrome, SlotStyleFn};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{CursorKind, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::with_class_list;
use crate::tokens;
use crate::utils::resolve_instance_id;

const ATTR_SLOT: &str = "data-slot";
const ATTR_ORIENTATION: &str = "data-orientation";
const ATTR_PANEL_INDEX: &str = "rui_resizable_panel";
const ATTR_HANDLE_INDEX: &str = "rui_resizable_handle";
const ATTR_DEFAULT_SIZE: &str = "rui_resizable_default";
const ATTR_MIN_SIZE: &str = "rui_resizable_min";
const ATTR_MAX_SIZE: &str = "rui_resizable_max";
const ATTR_WITH_HANDLE: &str = "rui_resizable_with_handle";

const SLOT_GROUP: &str = "resizable-panel-group";
const SLOT_PANEL: &str = "resizable-panel";
const SLOT_HANDLE: &str = "resizable-handle";
const SLOT_HANDLE_ICON: &str = "resizable-handle-icon";

// ── Orientation ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizableOrientation {
    Horizontal,
    Vertical,
}

impl ResizableOrientation {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vertical" | "col" | "column" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ResizableCtx {
    pub id: String,
    pub orientation: ResizableOrientation,
    pub sizes: Signal<Vec<f32>>,
}

fn resizable_context() -> Context<ResizableCtx> {
    static CTX: OnceLock<Context<ResizableCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ResizableCtx>)
}

/// 读取最近的 [`ResizablePanelGroup`] 上下文。
pub fn use_resizable() -> Option<ResizableCtx> {
    use_context(resizable_context())
}

// ── Drag ──────────────────────────────────────────────────────────────────────

struct ResizableDrag {
    sizes: Signal<Vec<f32>>,
    limits: Rc<Vec<(f32, f32)>>,
    orientation: ResizableOrientation,
    group_bounds: Rc<Cell<[f32; 4]>>,
    handle_index: usize,
    start_sizes: Vec<f32>,
    start_x: f32,
    start_y: f32,
}

thread_local! {
    static RESIZABLE_DRAG: RefCell<Option<ResizableDrag>> = const { RefCell::new(None) };
}

fn begin_resizable_drag(id: impl Into<String>, drag: ResizableDrag, x: f32, y: f32) {
    crate::utils::pointer_drag::begin(crate::utils::pointer_drag::DragKind::Resizable, id, x, y);
    RESIZABLE_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

fn end_resizable_drag() {
    crate::utils::pointer_drag::clear();
    RESIZABLE_DRAG.with(|d| *d.borrow_mut() = None);
    crate::app::release_pointer_capture();
}

pub(crate) fn is_resizable_dragging() -> bool {
    RESIZABLE_DRAG.with(|d| d.borrow().is_some())
}

/// CursorMoved：若正在拖拽分隔条，按指针位移更新百分比。
pub(crate) fn tick_resizable_drag(x: f32, y: f32) -> bool {
    RESIZABLE_DRAG.with(|d| {
        let borrow = d.borrow();
        let Some(drag) = borrow.as_ref() else {
            return false;
        };
        let bounds = drag.group_bounds.get();
        let axis_len = match drag.orientation {
            ResizableOrientation::Horizontal => bounds[2],
            ResizableOrientation::Vertical => bounds[3],
        }
        .max(1.0);
        let delta_px = match drag.orientation {
            ResizableOrientation::Horizontal => x - drag.start_x,
            ResizableOrientation::Vertical => y - drag.start_y,
        };
        let delta_pct = (delta_px / axis_len) * 100.0;
        let i = drag.handle_index;
        if i + 1 >= drag.start_sizes.len() || i + 1 >= drag.limits.len() {
            return true;
        }
        let (min_a, max_a) = drag.limits[i];
        let (min_b, max_b) = drag.limits[i + 1];
        let a0 = drag.start_sizes[i];
        let b0 = drag.start_sizes[i + 1];
        let pair = a0 + b0;
        let mut a = (a0 + delta_pct).clamp(min_a, max_a);
        let mut b = pair - a;
        if b < min_b {
            b = min_b;
            a = (pair - b).clamp(min_a, max_a);
            b = pair - a;
        } else if b > max_b {
            b = max_b;
            a = (pair - b).clamp(min_a, max_a);
            b = pair - a;
        }
        let mut next = drag.start_sizes.clone();
        next[i] = a;
        next[i + 1] = b;
        let sizes = drag.sizes;
        if sizes.peek() != next {
            drop(borrow);
            sizes.set(next);
        }
        true
    })
}

pub(crate) fn clear_resizable_drag_if_any() {
    if is_resizable_dragging() {
        end_resizable_drag();
    }
}

// ── Parse body parts ──────────────────────────────────────────────────────────

struct PanelPart {
    default_size: f32,
    min_size: f32,
    max_size: f32,
    content: Element,
    class: Option<String>,
    class_set: Option<crate::style::ClassSet>,
    text_style: Box<crate::style::TextStyle>,
    paint: Box<crate::style::Style>,
}

struct HandlePart {
    with_handle: bool,
    class: Option<String>,
    class_set: Option<crate::style::ClassSet>,
    text_style: Box<crate::style::TextStyle>,
    paint: Box<crate::style::Style>,
}

enum Part {
    Panel(PanelPart),
    Handle(HandlePart),
}

fn parse_f32_attr(el: &Element, key: &str, fallback: f32) -> f32 {
    el.attrs
        .get(key)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(fallback)
}

fn take_class_attr(el: &mut Element) -> Option<String> {
    el.attrs.remove("class").filter(|c| !c.trim().is_empty())
}

fn overlay_paint(dst: &mut crate::style::Style, src: &crate::style::Style) {
    dst.padding = src.padding;
    dst.gap = src.gap;
    dst.bg_color = src.bg_color;
    dst.bg_gradient = src.bg_gradient.clone();
    dst.border_style = src.border_style;
    dst.border_color = src.border_color;
    dst.border_width = src.border_width;
    dst.border_width_sides = src.border_width_sides;
    dst.border_color_sides = src.border_color_sides;
    dst.border_style_sides = src.border_style_sides;
    dst.radii = src.radii;
    dst.shadow_color = src.shadow_color;
    dst.shadow_offset = src.shadow_offset;
    dst.shadow_blur = src.shadow_blur;
    dst.shadow_spread = src.shadow_spread;
    dst.shadow_inset = src.shadow_inset;
    dst.extra_shadows = src.extra_shadows.clone();
    dst.opacity = src.opacity;
}

fn overlay_slot_chrome(
    dst: &mut Element,
    text_style: Box<crate::style::TextStyle>,
    paint: Box<crate::style::Style>,
) {
    *dst.text_style = *text_style;
    overlay_paint(&mut dst.style, &paint);
}

fn flatten_body_children(mut body: Element) -> Vec<Element> {
    if body.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut body.children);
    }
    if body.children.is_empty()
        && body.text.as_ref().is_none_or(|t| t.is_empty())
        && body.template_id.is_none()
        && body.attrs.get(ATTR_SLOT).is_none()
    {
        return Vec::new();
    }
    if body.attrs.get(ATTR_SLOT).is_some() {
        return vec![body];
    }
    std::mem::take(&mut body.children)
}

fn parse_parts(body: Element) -> Vec<Part> {
    let mut out = Vec::new();
    for mut el in flatten_body_children(body) {
        let slot = el.attrs.get(ATTR_SLOT).map(|s| s.as_str()).unwrap_or("");
        match slot {
            SLOT_PANEL => {
                let class = take_class_attr(&mut el);
                let class_set = el.class_set.take();
                let default_size = parse_f32_attr(&el, ATTR_DEFAULT_SIZE, 50.0);
                let min_size = parse_f32_attr(&el, ATTR_MIN_SIZE, 10.0);
                let max_size = parse_f32_attr(&el, ATTR_MAX_SIZE, 100.0);
                let text_style = el.text_style.clone();
                let paint = el.style.clone();
                // Prefer inner children as content; otherwise the element itself (text etc.).
                let content =
                    if el.children.len() == 1 && el.text.as_ref().is_none_or(|t| t.is_empty()) {
                        el.children.pop().unwrap()
                    } else if !el.children.is_empty() {
                        let kids = std::mem::take(&mut el.children);
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(kids)
                    } else {
                        let mut bare = Element::new();
                        bare.text = el.text.take();
                        bare
                    };
                out.push(Part::Panel(PanelPart {
                    default_size,
                    min_size: min_size.clamp(0.0, 100.0),
                    max_size: max_size.clamp(0.0, 100.0),
                    content,
                    class,
                    class_set,
                    text_style,
                    paint,
                }));
            }
            SLOT_HANDLE => {
                let class = take_class_attr(&mut el);
                let class_set = el.class_set.take();
                let with_handle = el
                    .attrs
                    .get(ATTR_WITH_HANDLE)
                    .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                out.push(Part::Handle(HandlePart {
                    with_handle,
                    class,
                    class_set,
                    text_style: el.text_style.clone(),
                    paint: el.style.clone(),
                }));
            }
            _ => {
                // Unknown node: treat as panel with default 50%.
                out.push(Part::Panel(PanelPart {
                    default_size: 50.0,
                    min_size: 10.0,
                    max_size: 100.0,
                    content: el,
                    class: None,
                    class_set: None,
                    text_style: Box::new(crate::style::TextStyle::default()),
                    paint: Box::new(crate::style::Style::default()),
                }));
            }
        }
    }
    out
}

fn normalize_defaults(panels: &mut [PanelPart]) {
    if panels.is_empty() {
        return;
    }
    let sum: f32 = panels.iter().map(|p| p.default_size.max(0.0)).sum();
    if sum <= 1e-3 {
        let each = 100.0 / panels.len() as f32;
        for p in panels.iter_mut() {
            p.default_size = each;
        }
        return;
    }
    if (sum - 100.0).abs() > 0.5 {
        let scale = 100.0 / sum;
        for p in panels.iter_mut() {
            p.default_size *= scale;
        }
    }
}

fn apply_panel_grows(root: &mut Element, layout_tree: &mut LayoutTree, sizes: &[f32]) -> bool {
    let mut changed = false;
    crate::core::tree::walk_elements_mut_pruned(root, |el| {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_PANEL) {
            let i = el
                .attrs
                .get(ATTR_PANEL_INDEX)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let want = sizes.get(i).copied().unwrap_or(0.0).max(0.0001);
            if (el.style.flex_grow - want).abs() > 1e-4 {
                el.style.flex_grow = want;
                if el.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(el);
                }
                changed = true;
            }
            return false;
        }
        true
    });
    changed
}

const HANDLE_HIT_PX: f32 = 10.0;
/// 发丝线两端内缩，避免与主容器外边框形成 T 字「端帽」。
const HAIRLINE_END_INSET: f32 = 1.0;

fn grip_icon(horizontal_group: bool) -> Element {
    // ShadCN new-york：`bg-border` + GripVertical；水平分隔条对位 `rotate-90` → 六点改为 3×2。
    // 手柄体 = border（与发丝线同色）；六点 = text-foreground。
    let border = tokens::border();
    let fg = crate::style::parse_color("foreground");
    let dot = || -> Element {
        r#box()
            .w(2.0)
            .h(2.0)
            .rounded(0.5)
            .bg(fg)
            .flex_shrink(0.0)
            .into()
    };
    // 竖直分隔（水平组）：GripVertical = 2 列 × 3 行。
    // 水平分隔（垂直组）：GripHorizontal = 3 列 × 2 行。
    let dots: Element = if horizontal_group {
        let col = || -> Element {
            r#box()
                .flex_col()
                .gap(2.0)
                .items_center()
                .justify_center()
                .child(dot())
                .child(dot())
                .child(dot())
                .into()
        };
        r#box()
            .flex_row()
            .gap(2.0)
            .items_center()
            .justify_center()
            .child(col())
            .child(col())
            .into()
    } else {
        let row = || -> Element {
            r#box()
                .flex_row()
                .gap(2.0)
                .items_center()
                .justify_center()
                .child(dot())
                .child(dot())
                .child(dot())
                .into()
        };
        r#box()
            .flex_col()
            .gap(2.0)
            .items_center()
            .justify_center()
            .child(row())
            .child(row())
            .into()
    };

    let mut grip: Element = if horizontal_group {
        r#box()
            .w(12.0)
            .h(16.0)
            .rounded(2.0)
            .bg(border)
            .border(border, 1.0)
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .child(dots)
            .into()
    } else {
        r#box()
            .w(16.0)
            .h(12.0)
            .rounded(2.0)
            .bg(border)
            .border(border, 1.0)
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .child(dots)
            .into()
    };
    grip.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_HANDLE_ICON.to_string());
    grip = grip.z_index(10).pointer_events_none();
    grip
}

fn build_handle(
    group_id: &str,
    orientation: ResizableOrientation,
    handle_index: usize,
    with_handle: bool,
    class: Option<String>,
    class_set: Option<crate::style::ClassSet>,
    text_style: Box<crate::style::TextStyle>,
    paint: Box<crate::style::Style>,
    sizes: Signal<Vec<f32>>,
    limits: Rc<Vec<(f32, f32)>>,
    group_bounds: Rc<Cell<[f32; 4]>>,
) -> Element {
    let is_row = matches!(orientation, ResizableOrientation::Horizontal);
    let cursor = if is_row {
        CursorKind::ColResize
    } else {
        CursorKind::RowResize
    };

    // 布局只占 1px：发丝线绝对铺满槽位（两端内缩避 T 帽）；握柄走 flex 居中（对位 ShadCN
    // `flex items-center justify-center`），勿用 absolute+translate（1px 含块上易错位）。
    let hairline: Element = if is_row {
        r#box()
            .absolute(true)
            .w(1.0)
            .left(0.0)
            .top(HAIRLINE_END_INSET)
            .bottom(HAIRLINE_END_INSET)
            .bg(tokens::border())
            .pointer_events("none")
            .into()
    } else {
        r#box()
            .absolute(true)
            .h(1.0)
            .top(0.0)
            .left(HAIRLINE_END_INSET)
            .right(HAIRLINE_END_INSET)
            .bg(tokens::border())
            .pointer_events("none")
            .into()
    };

    let hit_expand = (HANDLE_HIT_PX - 1.0) * 0.5;
    let mut hit_pad: Element = if is_row {
        r#box()
            .absolute(true)
            .w(HANDLE_HIT_PX)
            .h("100%")
            .top(0.0)
            .left(-hit_expand)
            .bg([0.0, 0.0, 0.0, 0.0])
            .into()
    } else {
        r#box()
            .absolute(true)
            .h(HANDLE_HIT_PX)
            .w("100%")
            .left(0.0)
            .top(-hit_expand)
            .bg([0.0, 0.0, 0.0, 0.0])
            .into()
    };
    hit_pad = hit_pad.cursor_kind(cursor);

    let focus_id = format!("{group_id}__rz_h{handle_index}");
    let sizes_pd = sizes;
    let limits_pd = limits;
    let bounds_pd = group_bounds;
    let ori = orientation;
    let hi = handle_index;
    let fid = focus_id.clone();

    // 命中落在扩展透明板上（槽位仅 1px，box-none 穿透到子节点）。
    hit_pad = interactive(hit_pad)
        .focus_id(focus_id.clone())
        .focusable(true)
        .focus_ring(false)
        .cursor_kind(cursor)
        .into();
    hit_pad = hit_pad.bg([0.0, 0.0, 0.0, 0.0]);
    hit_pad = hit_pad.on_pointer_down(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        use_signal("__rui_button_focus_id", String::new).set(fid.clone());
        use_signal("__rui_focus_visible", || false).set(false);
        let start = sizes_pd.peek();
        if start.len() < 2 {
            return;
        }
        let mut bounds = bounds_pd.get();
        if bounds[2] < 1.0 && bounds[3] < 1.0 {
            bounds = [e.x - 200.0, e.y - 80.0, 400.0, 160.0];
        }
        bounds_pd.set(bounds);
        begin_resizable_drag(
            fid.clone(),
            ResizableDrag {
                sizes: sizes_pd,
                limits: Rc::clone(&limits_pd),
                orientation: ori,
                group_bounds: Rc::clone(&bounds_pd),
                handle_index: hi,
                start_sizes: start,
                start_x: e.x,
                start_y: e.y,
            },
            e.x,
            e.y,
        );
        crate::app::set_pointer_capture(fid.clone());
        e.stop_propagation();
        e.prevent_default();
    });
    hit_pad = hit_pad.on_pointer_up(move |_e: &PointerEvent| {
        clear_resizable_drag_if_any();
    });

    let mut kids = vec![hairline, hit_pad];
    if with_handle {
        // 流内子项：由槽位 flex 居中，自然压在发丝线正中（可溢出 1px 槽）。
        kids.push(grip_icon(is_row));
    }

    let mut slot: Element = if is_row {
        r#box()
            .w(1.0)
            .h("100%")
            .min_h("1rem")
            .flex_shrink(0.0)
            .flex_grow(0.0)
            .flex_row()
            .items_center()
            .justify_center()
            .relative(true)
            .overflow("visible")
            .bg([0.0, 0.0, 0.0, 0.0])
            .children(kids)
            .into()
    } else {
        r#box()
            .h(1.0)
            .w("100%")
            .min_w("1rem")
            .flex_shrink(0.0)
            .flex_grow(0.0)
            .flex_row()
            .items_center()
            .justify_center()
            .relative(true)
            .overflow("visible")
            .bg([0.0, 0.0, 0.0, 0.0])
            .children(kids)
            .into()
    };

    slot.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_HANDLE.to_string());
    slot.attrs
        .insert(ATTR_HANDLE_INDEX.to_string(), handle_index.to_string());
    slot.attrs.insert(
        ATTR_ORIENTATION.to_string(),
        orientation.as_str().to_string(),
    );
    if let Some(cs) = class_set {
        slot.class_set = Some(cs);
    }
    if let Some(ref c) = class {
        slot = with_class_list(slot, c);
    }
    // 槽位自身不拦命中，交给扩展 hit_pad。
    // z-index>0：握柄溢出 1px 槽时仍画在后续面板之上，否则会被下一栏盖住一半，
    // 看起来像发丝线贴在手柄底边/侧边（布局中心其实已对齐）。
    slot = slot
        .bg([0.0, 0.0, 0.0, 0.0])
        .cursor_kind(cursor)
        .z_index(1)
        .pointer_events_box_none();
    overlay_slot_chrome(&mut slot, text_style, paint);

    slot
}

fn build_group_from_parts(
    id: String,
    orientation: ResizableOrientation,
    height: Option<f32>,
    sizes: Signal<Vec<f32>>,
    parts: Vec<Part>,
    group_bounds: Rc<Cell<[f32; 4]>>,
) -> Element {
    let mut panels = Vec::new();
    let mut handle_meta: Vec<Option<HandlePart>> = Vec::new();
    // Sequence: Panel (Handle Panel)*
    let mut expect_panel = true;
    for part in parts {
        match part {
            Part::Panel(p) => {
                if !expect_panel {
                    // missing handle between panels — insert default handle
                    handle_meta.push(Some(HandlePart {
                        with_handle: false,
                        class: None,
                        class_set: None,
                        text_style: Box::new(crate::style::TextStyle::default()),
                        paint: Box::new(crate::style::Style::default()),
                    }));
                }
                panels.push(p);
                expect_panel = false;
            }
            Part::Handle(h) => {
                if expect_panel {
                    // handle before first panel — ignore
                    continue;
                }
                handle_meta.push(Some(h));
                expect_panel = true;
            }
        }
    }
    // Trim trailing handle without following panel
    while handle_meta.len() >= panels.len() && !handle_meta.is_empty() {
        handle_meta.pop();
    }
    while handle_meta.len() + 1 < panels.len() {
        handle_meta.push(Some(HandlePart {
            with_handle: false,
            class: None,
            class_set: None,
            text_style: Box::new(crate::style::TextStyle::default()),
            paint: Box::new(crate::style::Style::default()),
        }));
    }

    if panels.is_empty() {
        panels.push(PanelPart {
            default_size: 100.0,
            min_size: 10.0,
            max_size: 100.0,
            content: r#box().w("100%").h("100%").bg(tokens::muted()).into(),
            class: None,
            class_set: None,
            text_style: Box::new(crate::style::TextStyle::default()),
            paint: Box::new(crate::style::Style::default()),
        });
    }

    normalize_defaults(&mut panels);

    let limits: Rc<Vec<(f32, f32)>> = Rc::new(
        panels
            .iter()
            .map(|p| {
                let min = p.min_size.clamp(0.0, 100.0);
                let max = p.max_size.max(min).clamp(0.0, 100.0);
                (min, max)
            })
            .collect(),
    );

    // Init sizes once if empty / length mismatch
    {
        let cur = sizes.peek();
        if cur.len() != panels.len() {
            let init: Vec<f32> = panels.iter().map(|p| p.default_size).collect();
            sizes.set(init);
        }
    }

    let group_bounds = group_bounds;
    let init_sizes = sizes.peek();

    let mut kids: Vec<Element> = Vec::new();
    for (i, panel) in panels.into_iter().enumerate() {
        let grow = init_sizes
            .get(i)
            .copied()
            .unwrap_or(panel.default_size)
            .max(0.0001);
        let mut pane: Element = r#box()
            .flex_grow(grow)
            .flex_shrink(1.0)
            .flex_basis("0%")
            .min_w(0.0)
            .min_h(0.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .w(if matches!(orientation, ResizableOrientation::Horizontal) {
                "auto"
            } else {
                "100%"
            })
            .h(if matches!(orientation, ResizableOrientation::Vertical) {
                "auto"
            } else {
                "100%"
            })
            .child(panel.content)
            .into();
        pane.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_PANEL.to_string());
        pane.attrs
            .insert(ATTR_PANEL_INDEX.to_string(), i.to_string());
        if let Some(cs) = panel.class_set {
            pane.class_set = Some(cs);
        }
        if let Some(ref c) = panel.class {
            pane = with_class_list(pane, c);
        }
        overlay_slot_chrome(&mut pane, panel.text_style, panel.paint);
        kids.push(pane);

        if i < handle_meta.len() {
            if let Some(h) = handle_meta[i].take() {
                kids.push(build_handle(
                    &id,
                    orientation,
                    i,
                    h.with_handle,
                    h.class,
                    h.class_set,
                    h.text_style,
                    h.paint,
                    sizes,
                    Rc::clone(&limits),
                    Rc::clone(&group_bounds),
                ));
            }
        }
    }

    let row: Element = match orientation {
        ResizableOrientation::Horizontal => hstack()
            .w("100%")
            .h("100%")
            .items("stretch")
            .children(kids)
            .into(),
        ResizableOrientation::Vertical => vstack()
            .w("100%")
            .h("100%")
            .items("stretch")
            .children(kids)
            .into(),
    };

    let mut root: Element = if let Some(h) = height {
        r#box().w("100%").h(h).overflow("hidden").child(row).into()
    } else {
        r#box()
            .w("100%")
            .h("100%")
            .overflow("hidden")
            .child(row)
            .into()
    };
    root.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_GROUP.to_string());
    root.attrs.insert(
        ATTR_ORIENTATION.to_string(),
        orientation.as_str().to_string(),
    );
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__resizable"));
    }

    let sizes_sync = sizes;
    let bounds_sync = Rc::clone(&group_bounds);
    root = root.reactive_sync(move |el, layout_tree| {
        bounds_sync.set(el.layout_bounds);
        let sizes_now = sizes_sync.peek();
        if apply_panel_grows(el, layout_tree, &sizes_now) {
            LayoutStatus::LayoutChanged
        } else {
            LayoutStatus::Clean
        }
    });

    root
}

/// 提供 Resizable 上下文；解析 Panel / Handle 子树并组装可拖拽布局。
pub fn resizable_panel_group_scope(
    id: impl Into<String>,
    orientation: impl AsRef<str>,
    height: Option<f32>,
    sizes: Option<Signal<Vec<f32>>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "resizable");
    let orientation = ResizableOrientation::parse(orientation.as_ref());
    let sizes =
        sizes.unwrap_or_else(|| use_signal(format!("{id}__rz_sizes").as_str(), Vec::<f32>::new));

    with_child_owner(|| {
        let body_el = body();
        let parts = parse_parts(body_el);
        let group_bounds = Rc::new(Cell::new([0.0f32; 4]));
        provide_context(
            resizable_context(),
            ResizableCtx {
                id: id.clone(),
                orientation,
                sizes,
            },
        );
        build_group_from_parts(id, orientation, height, sizes, parts, group_bounds)
    })
}

// ── Compound components ───────────────────────────────────────────────────────

/// Root：宏走 [`resizable_panel_group_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ResizablePanelGroup(
    id: String,
    orientation: Option<String>,
    h: Option<f32>,
    height: Option<f32>,
    sizes: Option<Signal<Vec<f32>>>,
    children: Option<Element>,
) -> Element {
    let ori = orientation.unwrap_or_else(|| "horizontal".into());
    let height = h.or(height);
    resizable_panel_group_scope(id, ori, height, sizes, || {
        children.unwrap_or_else(Element::new)
    })
}

/// 面板：`default_size` / `min_size` / `max_size` 为百分比（0..=100）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ResizablePanel(
    default_size: Option<f32>,
    min_size: Option<f32>,
    max_size: Option<f32>,
    children: Option<Element>,
) -> Element {
    let mut el: Element = r#box()
        .w("100%")
        .h("100%")
        .child(children.unwrap_or_else(Element::new))
        .into();
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_PANEL.to_string());
    el.attrs.insert(
        ATTR_DEFAULT_SIZE.to_string(),
        default_size.unwrap_or(50.0).to_string(),
    );
    el.attrs.insert(
        ATTR_MIN_SIZE.to_string(),
        min_size.unwrap_or(10.0).to_string(),
    );
    el.attrs.insert(
        ATTR_MAX_SIZE.to_string(),
        max_size.unwrap_or(100.0).to_string(),
    );
    el
}

/// 分隔条：`with_handle` 显示握柄（对位 `withHandle`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ResizableHandle(with_handle: bool, children: Option<Element>) -> Element {
    let _ = children;
    let mut el = Element::new();
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_HANDLE.to_string());
    el.attrs.insert(
        ATTR_WITH_HANDLE.to_string(),
        if with_handle { "1" } else { "0" }.to_string(),
    );
    el
}

pub struct ResizablePanelBuilder {
    default_size: Option<f32>,
    min_size: Option<f32>,
    max_size: Option<f32>,
    children: Vec<crate::core::tree::Element>,
    chrome: SlotChrome,
}

pub fn resizable_panel() -> ResizablePanelBuilder {
    ResizablePanelBuilder {
        default_size: None,
        min_size: None,
        max_size: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ResizablePanelBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn default_size(mut self, v: f32) -> Self {
        self.default_size = Some(v);
        self
    }
    pub fn min_size(mut self, v: f32) -> Self {
        self.min_size = Some(v);
        self
    }
    pub fn max_size(mut self, v: f32) -> Self {
        self.max_size = Some(v);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
}

impl From<ResizablePanelBuilder> for Element {
    fn from(builder: ResizablePanelBuilder) -> Self {
        let el = ResizablePanel(ResizablePanelProps {
            default_size: builder.default_size,
            min_size: builder.min_size,
            max_size: builder.max_size,
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for ResizablePanelBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct ResizableHandleBuilder {
    with_handle: bool,
    chrome: SlotChrome,
}

pub fn resizable_handle() -> ResizableHandleBuilder {
    ResizableHandleBuilder {
        with_handle: false,
        chrome: SlotChrome::default(),
    }
}

impl ResizableHandleBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn with_handle(mut self, v: bool) -> Self {
        self.with_handle = v;
        self
    }
}

impl From<ResizableHandleBuilder> for Element {
    fn from(builder: ResizableHandleBuilder) -> Self {
        let el = ResizableHandle(ResizableHandleProps {
            with_handle: Some(builder.with_handle),
            children: None,
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for ResizableHandleBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct ResizableBuilder {
    id: String,
    orientation: ResizableOrientation,
    sizes: Option<Signal<Vec<f32>>>,
    default_sizes: Vec<f32>,
    min_sizes: Vec<f32>,
    max_sizes: Vec<f32>,
    panels: Vec<Box<dyn Fn() -> Element>>,
    height: Option<f32>,
    with_handle: bool,
    class: String,
    style: Option<SlotStyleFn>,
    custom_body: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder 入口：两栏便捷 / 多栏 `panels`。
pub fn resizable(id: impl Into<String>) -> ResizableBuilder {
    ResizableBuilder {
        id: id.into(),
        orientation: ResizableOrientation::Horizontal,
        sizes: None,
        default_sizes: vec![50.0, 50.0],
        min_sizes: vec![15.0, 15.0],
        max_sizes: vec![100.0, 100.0],
        panels: Vec::new(),
        height: Some(240.0),
        with_handle: true,
        class: String::new(),
        style: None,
        custom_body: None,
    }
}

impl ResizableBuilder {
    pub fn orientation(mut self, o: impl AsRef<str>) -> Self {
        self.orientation = ResizableOrientation::parse(o.as_ref());
        self
    }

    pub fn sizes_signal(mut self, s: Signal<Vec<f32>>) -> Self {
        self.sizes = Some(s);
        self
    }

    pub fn default_sizes(mut self, sizes: impl IntoIterator<Item = f32>) -> Self {
        self.default_sizes = sizes.into_iter().collect();
        self
    }

    pub fn min_sizes(mut self, sizes: impl IntoIterator<Item = f32>) -> Self {
        self.min_sizes = sizes.into_iter().collect();
        self
    }

    pub fn max_sizes(mut self, sizes: impl IntoIterator<Item = f32>) -> Self {
        self.max_sizes = sizes.into_iter().collect();
        self
    }

    /// 兼容旧 API：左栏默认像素宽（会换算成约 40% 若未设 default_sizes）。
    pub fn default_left(mut self, w: f32) -> Self {
        let pct = if w > 100.0 {
            ((w / 600.0) * 100.0).clamp(10.0, 90.0)
        } else {
            w.clamp(10.0, 90.0)
        };
        self.default_sizes = vec![pct, 100.0 - pct];
        self
    }

    pub fn min_left(mut self, w: f32) -> Self {
        let pct = if w > 100.0 {
            ((w / 600.0) * 100.0).clamp(5.0, 50.0)
        } else {
            w.clamp(5.0, 50.0)
        };
        if self.min_sizes.is_empty() {
            self.min_sizes = vec![pct, 15.0];
        } else {
            self.min_sizes[0] = pct;
        }
        self
    }

    pub fn max_left(mut self, w: f32) -> Self {
        let pct = if w > 100.0 {
            ((w / 600.0) * 100.0).clamp(20.0, 95.0)
        } else {
            w.clamp(20.0, 95.0)
        };
        if self.max_sizes.is_empty() {
            self.max_sizes = vec![pct, 100.0];
        } else {
            self.max_sizes[0] = pct;
        }
        self
    }

    /// 已弃用：请用 [`Self::sizes_signal`]（百分比向量）。
    pub fn left_width_signal(self, _s: Signal<f32>) -> Self {
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    pub fn with_handle(mut self, yes: bool) -> Self {
        self.with_handle = yes;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
        self
    }

    /// Build Panel / Handle subtrees inside the context.
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.custom_body = Some(Box::new(body));
        self
    }

    pub fn left(mut self, f: impl Fn() -> Element + 'static) -> Self {
        if self.panels.is_empty() {
            self.panels.push(Box::new(f));
        } else {
            self.panels[0] = Box::new(f);
        }
        self
    }

    pub fn right(mut self, f: impl Fn() -> Element + 'static) -> Self {
        while self.panels.len() < 1 {
            self.panels
                .push(Box::new(|| r#box().w("100%").h("100%").into()));
        }
        if self.panels.len() == 1 {
            self.panels.push(Box::new(f));
        } else {
            self.panels[1] = Box::new(f);
        }
        self
    }

    pub fn panels(mut self, panels: Vec<Box<dyn Fn() -> Element>>) -> Self {
        self.panels = panels;
        self
    }

    pub fn panel(mut self, f: impl Fn() -> Element + 'static) -> Self {
        self.panels.push(Box::new(f));
        self
    }
}

impl From<ResizableBuilder> for Element {
    fn from(builder: ResizableBuilder) -> Self {
        let ResizableBuilder {
            id,
            orientation,
            sizes,
            default_sizes,
            min_sizes,
            max_sizes,
            panels,
            height,
            with_handle,
            class,
            style,
            custom_body,
        } = builder;

        let el = if let Some(body) = custom_body {
            resizable_panel_group_scope(id, orientation.as_str(), height, sizes, body)
        } else {
            let n = panels.len().max(2);
            let mut defaults = default_sizes;
            while defaults.len() < n {
                defaults.push(100.0 / n as f32);
            }
            defaults.truncate(n);
            let mut mins = min_sizes;
            while mins.len() < n {
                mins.push(10.0);
            }
            mins.truncate(n);
            let mut maxs = max_sizes;
            while maxs.len() < n {
                maxs.push(100.0);
            }
            maxs.truncate(n);

            let mut panel_fns = panels;
            while panel_fns.len() < n {
                let idx = panel_fns.len();
                panel_fns.push(Box::new(move || {
                    let bg = if idx % 2 == 0 {
                        tokens::muted()
                    } else {
                        tokens::background()
                    };
                    r#box().w("100%").h("100%").bg(bg).into()
                }));
            }

            resizable_panel_group_scope(id, orientation.as_str(), height, sizes, move || {
                let mut list = Vec::new();
                for (i, f) in panel_fns.into_iter().enumerate() {
                    list.push(ensure_data_slot(f(), SLOT_PANEL, |el| {
                        ResizablePanel(ResizablePanelProps {
                            default_size: Some(defaults[i]),
                            min_size: Some(mins[i]),
                            max_size: Some(maxs[i]),
                            children: Some(el),
                            base: None,
                        })
                    }));
                    if i + 1 < n {
                        list.push(ResizableHandle(ResizableHandleProps {
                            with_handle: Some(with_handle),
                            children: None,
                            base: None,
                        }));
                    }
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(list)
            })
        };
        SlotChrome { class, style }.apply(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizable_builder_builds() {
        let _el: Element = resizable("rz1")
            .default_sizes([30.0, 70.0])
            .left(|| {
                let t: Element = crate::primitives::text::text("L").into();
                r#box().child(t).into()
            })
            .right(|| {
                let t: Element = crate::primitives::text::text("R").into();
                r#box().child(t).into()
            })
            .into();
    }

    #[test]
    fn builder_panel_style() {
        use crate::core::reactivity::create_root;
        let (_o, el): (_, Element) = create_root(|| {
            resizable("rz-chrome")
                .child(|| {
                    resizable_panel()
                        .default_size(40.0)
                        .style(|b| b.text_size(20.0))
                        .child(crate::primitives::text::text("L"))
                        .into()
                })
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "resizable_panel .style(|b| b.text_size(20)) must land on the panel tree"
        );
    }

    #[test]
    fn resizable_panel_group_scope_builds() {
        let el = resizable_panel_group_scope("rz2", "horizontal", Some(120.0), None, || {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![
                ResizablePanel(ResizablePanelProps {
                    default_size: Some(40.0),
                    min_size: Some(20.0),
                    max_size: None,
                    children: Some(crate::primitives::text::text("A").into()),
                    base: None,
                }),
                ResizableHandle(ResizableHandleProps {
                    with_handle: Some(true),
                    children: None,
                    base: None,
                }),
                ResizablePanel(ResizablePanelProps {
                    default_size: Some(60.0),
                    min_size: None,
                    max_size: None,
                    children: Some(crate::primitives::text::text("B").into()),
                    base: None,
                }),
            ])
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_GROUP)
        );
    }

    #[test]
    fn orientation_parse() {
        assert_eq!(
            ResizableOrientation::parse("vertical"),
            ResizableOrientation::Vertical
        );
        assert_eq!(
            ResizableOrientation::parse("horizontal"),
            ResizableOrientation::Horizontal
        );
    }

    #[test]
    fn horizontal_handle_is_narrow_and_drag_updates_sizes() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_owner, (mut root, sizes)) = create_root(|| {
            let sizes = use_signal("rz_layout_sizes", || vec![30.0f32, 70.0]);
            let el = resizable_panel_group_scope(
                "rz_layout",
                "horizontal",
                Some(160.0),
                Some(sizes),
                || {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![
                        ResizablePanel(ResizablePanelProps {
                            default_size: Some(30.0),
                            min_size: Some(10.0),
                            max_size: None,
                            children: Some(r#box().w("100%").h("100%").bg(tokens::muted()).into()),
                            base: None,
                        }),
                        ResizableHandle(ResizableHandleProps {
                            with_handle: Some(true),
                            children: None,
                            base: None,
                        }),
                        ResizablePanel(ResizablePanelProps {
                            default_size: Some(70.0),
                            min_size: Some(10.0),
                            max_size: None,
                            children: Some(
                                r#box().w("100%").h("100%").bg(tokens::background()).into(),
                            ),
                            base: None,
                        }),
                    ])
                },
            );
            (el, sizes)
        });

        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 400.0, 160.0, [0.0, 0.0]);

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            for c in &el.children {
                if let Some(f) = find_slot(c, slot) {
                    return Some(f);
                }
            }
            None
        }
        fn find_handle<'a>(el: &'a Element) -> Option<&'a Element> {
            find_slot(el, SLOT_HANDLE)
        }
        fn panel_bounds(el: &Element, idx: usize) -> Option<[f32; 4]> {
            fn walk(el: &Element, idx: usize) -> Option<[f32; 4]> {
                if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_PANEL)
                    && el.attrs.get(ATTR_PANEL_INDEX).map(|s| s.as_str()) == Some(&idx.to_string())
                {
                    return Some(el.layout_bounds);
                }
                for c in &el.children {
                    if let Some(b) = walk(c, idx) {
                        return Some(b);
                    }
                }
                None
            }
            walk(el, idx)
        }

        let handle = find_handle(&root).expect("handle");
        let hw = handle.layout_bounds[2];
        assert!(
            hw <= 2.0,
            "handle layout width should be 1px hairline, got {hw} bounds={:?}",
            handle.layout_bounds
        );

        // 握柄布局中心须与发丝线中心重合（flex 居中；发丝线铺满 1px 槽）。
        let hairline = handle
            .children
            .first()
            .expect("hairline is first handle child");
        let grip = find_slot(&root, SLOT_HANDLE_ICON).expect("grip icon");
        let cx = |b: [f32; 4]| b[0] + b[2] * 0.5;
        let cy = |b: [f32; 4]| b[1] + b[3] * 0.5;
        let hcx = cx(hairline.layout_bounds);
        let gcx = cx(grip.layout_bounds);
        let hcy = cy(hairline.layout_bounds);
        let gcy = cy(grip.layout_bounds);
        assert!(
            (hcx - gcx).abs() < 0.75,
            "grip center x ({gcx}) must align with hairline center x ({hcx}); handle={:?} hairline={:?} grip={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds
        );
        assert!(
            (hcy - gcy).abs() < 0.75,
            "grip center y ({gcy}) must align with hairline center y ({hcy}); handle={:?} hairline={:?} grip={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds
        );

        let left = panel_bounds(&root, 0).expect("left");
        let right = panel_bounds(&root, 1).expect("right");
        assert!(
            left[2] > 80.0 && left[2] < 160.0,
            "left panel ~30% of 400, got {:?}",
            left
        );
        assert!(right[2] > 200.0, "right panel ~70% of 400, got {:?}", right);

        // Simulate drag: move pointer +40px on a 400px group → ~10%
        begin_resizable_drag(
            "rz_layout__rz_h0",
            ResizableDrag {
                sizes,
                limits: Rc::new(vec![(10.0, 100.0), (10.0, 100.0)]),
                orientation: ResizableOrientation::Horizontal,
                group_bounds: Rc::new(Cell::new([0.0, 0.0, 400.0, 160.0])),
                handle_index: 0,
                start_sizes: vec![30.0, 70.0],
                start_x: 120.0,
                start_y: 80.0,
            },
            120.0,
            80.0,
        );
        assert!(tick_resizable_drag(160.0, 80.0));
        let after = sizes.peek();
        clear_resizable_drag_if_any();
        assert!(
            (after[0] - 40.0).abs() < 0.5 && (after[1] - 60.0).abs() < 0.5,
            "expected ~[40,60] after +40px drag, got {after:?}"
        );
    }

    #[test]
    fn vertical_grip_centers_on_hairline() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_owner, mut root) = create_root(|| {
            resizable_panel_group_scope("rz_v_align", "vertical", Some(224.0), None, || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    ResizablePanel(ResizablePanelProps {
                        default_size: Some(30.0),
                        min_size: Some(10.0),
                        max_size: None,
                        children: Some(r#box().w("100%").h("100%").bg(tokens::muted()).into()),
                        base: None,
                    }),
                    ResizableHandle(ResizableHandleProps {
                        with_handle: Some(true),
                        children: None,
                        base: None,
                    }),
                    ResizablePanel(ResizablePanelProps {
                        default_size: Some(70.0),
                        min_size: Some(10.0),
                        max_size: None,
                        children: Some(r#box().w("100%").h("100%").bg(tokens::background()).into()),
                        base: None,
                    }),
                ])
            })
        });

        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 400.0, 224.0, [0.0, 0.0]);

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            for c in &el.children {
                if let Some(f) = find_slot(c, slot) {
                    return Some(f);
                }
            }
            None
        }
        fn find_panel<'a>(el: &'a Element, idx: usize) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_PANEL)
                && el.attrs.get(ATTR_PANEL_INDEX).map(|s| s.as_str()) == Some(&idx.to_string())
            {
                return Some(el);
            }
            for c in &el.children {
                if let Some(f) = find_panel(c, idx) {
                    return Some(f);
                }
            }
            None
        }

        let handle = find_slot(&root, SLOT_HANDLE).expect("handle");
        assert!(
            handle.style.z_index >= 1,
            "handle must stack above sibling panels so overflowing grip is not covered"
        );
        let grip = find_slot(&root, SLOT_HANDLE_ICON).expect("grip");
        let hairline = handle.children.first().expect("hairline");
        assert!(
            handle.layout_bounds[3] <= 2.0,
            "vertical handle height should be ~1px, got {:?}",
            handle.layout_bounds
        );
        let cy = |b: [f32; 4]| b[1] + b[3] * 0.5;
        let dy = (cy(grip.layout_bounds) - cy(hairline.layout_bounds)).abs();
        assert!(
            dy < 0.75,
            "vertical grip must sit on hairline; dy={dy} handle={:?} hair={:?} grip={:?} align={:?} justify={:?}",
            handle.layout_bounds,
            hairline.layout_bounds,
            grip.layout_bounds,
            handle.style.align_items,
            handle.style.justify_content
        );

        // 绘制序：握柄实例须晚于下一栏面板底色，否则溢出半截会被盖住。
        let grip_b = grip.layout_bounds;
        let p1 = find_panel(&root, 1).expect("panel 1").layout_bounds;
        let grip_inst = result
            .instances
            .iter()
            .position(|i| {
                (i.rect_pos[0] - grip_b[0]).abs() < 2.0
                    && (i.rect_pos[1] - grip_b[1]).abs() < 2.0
                    && (i.rect_size[0] - grip_b[2]).abs() < 2.0
                    && (i.rect_size[1] - grip_b[3]).abs() < 2.0
            })
            .expect("grip paint instance");
        let panel_inst = result
            .instances
            .iter()
            .position(|i| {
                (i.rect_pos[0] - p1[0]).abs() < 2.0
                    && (i.rect_pos[1] - p1[1]).abs() < 2.0
                    && i.rect_size[0] > p1[2] * 0.5
                    && i.color[3] > 0.5
            })
            .expect("panel1 paint instance");
        assert!(
            grip_inst > panel_inst,
            "grip instance #{grip_inst} must paint after panel1 instance #{panel_inst}"
        );
    }
}
