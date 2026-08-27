//! # Popover
//!
//! 对照 ShadCN **base** [`popover.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/popover.tsx)：
//!
//! ```text
//! Popover
//! ├── PopoverTrigger          // 点击切换；子节点为触发器（常为 Button）
//! └── PopoverContent          // Portal + floating；默认 bottom / center / sideOffset=4
//!     ├── PopoverHeader / PopoverTitle / PopoverDescription
//!     └── …自定义内容
//! ```
//!
//! 面板样式对位 `cn-popover-content`：`w-72` · `p-4` · `rounded-md` · `border` · `bg-popover` ·
//! `shadow-md`。`sideOffset` 默认 **4**（对位 Positioner）。
//!
//! `ui!` 中 `<Popover>` / `<PopoverContent>` 由宏展开。Builder
//! [`popover`] / [`PopoverBuilder::child`] 接到同一套实现。
//!
//! ## 用法（双写）
//! ```ignore
//! // A · RSX（camelCase 宏别名：sideOffset / openSignal）
//! ui! {
//!     <Popover id="demo" open={pop_open}>
//!         <PopoverTrigger>
//!             <Button variant="outline" size="sm">Open</Button>
//!         </PopoverTrigger>
//!         <PopoverContent side="bottom" align="center" sideOffset={4.0} class="w-80">
//!             <PopoverHeader>
//!                 <PopoverTitle>Dimensions</PopoverTitle>
//!                 <PopoverDescription>Set the dimensions for the layer.</PopoverDescription>
//!             </PopoverHeader>
//!         </PopoverContent>
//!     </Popover>
//! }
//!
//! // B · Builder（只 snake_case）
//! popover("demo")
//!     .open_signal(pop_open)
//!     .child(|| {
//!         let mut frag = Element::new();
//!         frag.set_attr("rui_fragment", "1");
//!         frag.children(vec![
//!             PopoverTrigger(PopoverTriggerProps {
//!                 id: String::new(),
//!                 children: Some(button("t").variant("outline").text("Open").into()),
//!                 base: None,
//!             }),
//!             popover_content(
//!                 PopoverContentOpts::default(),
//!                 || {/* … */},
//!             ),
//!         ])
//!     })
//! ```
//!
//! 低层面板（无 Trigger / 无锚）仍可用 [`popover_panel`]（浮层内嵌套等）。

use std::sync::OnceLock;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized, reactive_view};
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::style::{parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::popup::{
    disarm_underlay_close, mark_floating_with_gap, mount_anchored_overlay, panel_block_clicks,
    PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W, ATTR_FLOATING_STRATEGY,
    ATTR_FLOATING_W,
};
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id` + 触发器 `focus_id`。
#[derive(Clone)]
pub struct PopoverCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// 触发器 focus_id（Content 锚点）；Trigger 挂载时写入。
    pub trigger_fid: Signal<String>,
}

fn popover_context() -> Context<PopoverCtx> {
    static CTX: OnceLock<Context<PopoverCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<PopoverCtx>)
}

/// 读取最近的 [`Popover`] 上下文。
pub fn use_popover() -> Option<PopoverCtx> {
    use_context(popover_context())
}

fn require_popover(part: &str) -> PopoverCtx {
    use_popover().unwrap_or_else(|| {
        panic!("{part} must be used within <Popover> / popover_scope");
    })
}

/// 提供 Popover 上下文并构建子树（`ui!` `<Popover>` / Builder 共用）。
pub fn popover_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "popover");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__popover_open").as_str(), || false));
    let trigger_fid = use_signal(format!("{id}__popover_trigger_fid").as_str(), || {
        format!("{id}__popover_trigger")
    });
    let mut root = with_child_owner(|| {
        provide_context(
            popover_context(),
            PopoverCtx {
                open,
                id: id.clone(),
                trigger_fid,
            },
        );
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__popover"));
    }
    root
}

/// Builder：`.open(sig).child(|| …)` → [`popover_scope`]。
pub struct PopoverBuilder {
    id: String,
    open: Option<Signal<bool>>,
}

/// 复合 Popover 入口（对位 `<Popover>`）。
pub fn popover(id: impl Into<String>) -> PopoverBuilder {
    PopoverBuilder {
        id: id.into(),
        open: None,
    }
}

impl PopoverBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    /// 与 [`Self::open`] 相同（对位 RSX `openSignal`）。
    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    /// 在上下文内构建 Trigger / Content 等子树。
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        popover_scope(self.id, self.open, body)
    }
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

/// ShadCN `side` + `align` → [`PopupPlacement`]（默认 `bottom` + `center`）。
pub fn popover_side_align_to_placement(side: &str, align: &str) -> PopupPlacement {
    let side = side.trim().to_ascii_lowercase();
    let align = align.trim().to_ascii_lowercase();
    // Combined form: `bottom-start` / `top-end` / …
    if side.contains('-') {
        return PopupPlacement::from_attr(&side);
    }
    match (side.as_str(), align.as_str()) {
        ("bottom", "start") => PopupPlacement::BottomStart,
        ("bottom", "end") => PopupPlacement::BottomEnd,
        ("bottom", _) => PopupPlacement::BottomCenter,
        ("top", "start") => PopupPlacement::TopStart,
        ("top", "end") => PopupPlacement::TopEnd,
        ("top", _) => PopupPlacement::TopCenter,
        ("right", _) => PopupPlacement::RightStart,
        ("left", _) => PopupPlacement::LeftStart,
        _ => PopupPlacement::BottomCenter,
    }
}

// ---------------------------------------------------------------------------
// Trigger / Content / Header / Title / Description
// ---------------------------------------------------------------------------

/// ShadCN `PopoverTrigger`：点击切换；子节点为触发器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PopoverTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_popover("PopoverTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__popover_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    let toggle = move || {
        let next = !open.peek();
        if next {
            disarm_underlay_close();
        }
        open.set(next);
    };

    let mut trigger = match children {
        None => {
            let mut el: Element = interactive(label(trigger_id.as_str()).text("Open").into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_click(el, toggle);
            el
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = interactive(label(trigger_id.as_str()).text(t).into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_click(el, toggle);
            el
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id.clone());
            ctx.trigger_fid.set(fid);
            chain_click(c, toggle)
        }
    };

    if trigger.button_hover_paint.is_none() && trigger.input_paint_sync.is_none() {
        trigger.focusable = true;
        trigger.focus_ring = true;
    }
    let fid_focus = trigger
        .focus_id
        .clone()
        .unwrap_or_else(|| trigger_id.clone());
    ctx.trigger_fid.set(fid_focus);
    trigger
}

/// `PopoverContent` 选项（`ui!` / [`popover_content`] 共用）。
#[derive(Clone, Debug)]
pub struct PopoverContentOpts {
    /// `top` / `bottom` / `left` / `right`（默认 `bottom`）。
    pub side: String,
    /// `start` / `center` / `end`（默认 `center`）。
    pub align: String,
    /// 触发器与面板间距（逻辑 px）；默认 **4**（对位 ShadCN `sideOffset`）。
    pub side_offset: f32,
    /// 额外 class（utility）。
    pub class: Option<String>,
    /// `flip`（默认）或 `fixed`：钉在触发器首选侧，滚动不夹高度。
    pub strategy: Option<String>,
    /// 面板宽；默认 `18rem`（`w-72`）。空 / `auto` → 内容宽。
    pub width: Option<String>,
}

impl Default for PopoverContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            align: "center".into(),
            side_offset: 4.0,
            class: None,
            strategy: None,
            width: None,
        }
    }
}

/// ShadCN `cn-popover-content` 面板表面（无 Portal / 无 floating）。
pub fn popover_surface(mut body: Element, width: Option<&str>) -> Element {
    let w = width
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"))
        .unwrap_or("18rem");
    let mut panel: Element = vstack()
        .w(w)
        .p("1rem")
        .gap("1rem")
        .rounded("0.375rem")
        .into();
    panel = panel
        .bg(tokens::popover())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        // shadow-md 近似
        .shadow([0.0, 0.0, 0.0, 0.12], 10.0);
    if is_text_leaf(&body) {
        let t = body.text.clone().unwrap_or_default();
        body = text(t)
            .size(14.0)
            .color(parse_text_color("popover-foreground"))
            .into();
    }
    panel.child(body)
}

/// ShadCN `PopoverContent`：Portal + floating；仅在 open 时构建。
/// `ui!` 中由宏编成此函数。
pub fn popover_content(
    opts: PopoverContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_popover("PopoverContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let trigger_fid = ctx.trigger_fid;
    let placement = popover_side_align_to_placement(&opts.side, &opts.align);
    let side_offset = opts.side_offset;
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let strategy = opts.strategy.filter(|s| !s.trim().is_empty());
    let width = opts.width;
    let view_id = format!("{id}__popover_view");

    reactive_anchor(view_id.clone(), move || {
        // 先订 open，再 sync；关闭时 portal_sync 返回 None 也必须保留订阅。
        let is_open = open.get();
        if !is_open {
            let _ = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked);
            return Element::new().w(0.0).h(0.0);
        }
        let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) else {
            return Element::new().w(0.0).h(0.0);
        };

        let mut panel = popover_surface(body(), width.as_deref());
        if let Some(ref c) = class {
            panel = with_class_list(panel, c);
        }
        let anchor = trigger_fid.peek();
        mark_floating_with_gap(&mut panel, &id, &anchor, placement, side_offset);
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        if let Some(ref w) = width {
            let w = w.trim();
            if !w.is_empty() && !w.eq_ignore_ascii_case("auto") {
                panel.set_attr(ATTR_FLOATING_W, w);
            }
        } else {
            panel.set_attr(ATTR_FLOATING_W, "18rem");
        }
        if strategy
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("fixed"))
        {
            panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
        }
        panel = panel.absolute();
        mount_anchored_overlay(panel, oz.panel)
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
}

/// ShadCN `PopoverHeader`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PopoverHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.25rem")
            .flex_shrink(0.0)
            .children(kids),
    )
}

/// ShadCN `PopoverTitle`（`font-medium` / `text-sm`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PopoverTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("popover-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(500.0).text_color(fg),
        None => Element::from(text("").size(14.0).weight(500.0).color(fg)),
    }
}

/// ShadCN `PopoverDescription`（`text-sm` / `muted-foreground`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PopoverDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
}

// ---------------------------------------------------------------------------
// Low-level panel（兼容旧 API / 无锚浮层）
// ---------------------------------------------------------------------------

/// 低层面板 Builder：无 Trigger 上下文；可选 `anchor_focus_id`。
///
/// 新代码请优先用 [`popover`] + [`PopoverTrigger`] / [`popover_content`]。
pub struct PopoverPanelBuilder {
    open: Signal<bool>,
    body: Option<Box<dyn Fn() -> Element>>,
    id: Option<String>,
    z_override: Option<i32>,
    anchor_focus_id: Option<String>,
    placement: PopupPlacement,
    side_offset: f32,
    strategy: String,
}

/// 非模态浮层面板；`open` 为 false 时为空。见模块文档。
pub fn popover_panel(open: Signal<bool>) -> PopoverPanelBuilder {
    PopoverPanelBuilder {
        open,
        body: None,
        id: None,
        z_override: None,
        anchor_focus_id: None,
        placement: PopupPlacement::BottomCenter,
        side_offset: 4.0,
        strategy: String::new(),
    }
}

impl PopoverPanelBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_override = Some(z);
        self
    }

    pub fn anchor_focus_id(mut self, fid: impl Into<String>) -> Self {
        self.anchor_focus_id = Some(fid.into());
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    pub fn side_offset(mut self, px: f32) -> Self {
        self.side_offset = px;
        self
    }

    /// `fixed`：浮层钉在触发器下方/上方，尺寸不随滚动夹 `max_h`（可溢出视口）。
    pub fn strategy(mut self, s: impl Into<String>) -> Self {
        self.strategy = s.into();
        self
    }
}

impl From<PopoverPanelBuilder> for Element {
    fn from(builder: PopoverPanelBuilder) -> Self {
        let PopoverPanelBuilder {
            open,
            body,
            id,
            z_override,
            anchor_focus_id,
            placement,
            side_offset,
            strategy,
        } = builder;
        let id = id.unwrap_or_else(|| "popover".into());
        let view_id = format!("{id}__popover_panel_view");

        reactive_view(view_id, move || {
            let is_open = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked)
            else {
                return Element::new();
            };
            if !is_open {
                return Element::new();
            }
            let inner_body = if let Some(build) = &body {
                build()
            } else {
                Element::new()
            };
            let mut inner = popover_surface(inner_body, None);
            if let Some(anchor) = &anchor_focus_id {
                mark_floating_with_gap(&mut inner, &id, anchor, placement, side_offset);
                if strategy.eq_ignore_ascii_case("fixed") {
                    inner.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
                }
                inner.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
                inner.set_attr(ATTR_FLOATING_W, "18rem");
                inner = inner.absolute();
            }
            let z = z_override.unwrap_or(oz.panel);
            if anchor_focus_id.is_some() {
                mount_anchored_overlay(inner, z)
            } else {
                panel_block_clicks(inner, z)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn side_align_defaults_to_bottom_center() {
        assert_eq!(
            popover_side_align_to_placement("bottom", "center"),
            PopupPlacement::BottomCenter
        );
        assert_eq!(
            popover_side_align_to_placement("bottom", "start"),
            PopupPlacement::BottomStart
        );
        assert_eq!(
            popover_side_align_to_placement("top-end", ""),
            PopupPlacement::TopEnd
        );
    }

    #[test]
    fn content_anchor_is_zero_sized() {
        let (_owner, el) = create_root(|| {
            popover_scope("pop-anchor", None, || {
                popover_content(PopoverContentOpts::default(), || Element::new().text("hi"))
            })
        });
        fn find_zero(el: &Element) -> bool {
            let is_view = el.template_id.as_deref() == Some("pop-anchor__popover_view");
            let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
                && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
            (is_view && dim0) || el.children.iter().any(find_zero)
        }
        assert!(
            find_zero(&el),
            "PopoverContent must use a 0×0 portal anchor"
        );
    }

    #[test]
    fn scope_exposes_template() {
        let (_owner, el) = create_root(|| {
            popover("pop-scope")
                .open_signal(use_signal("pop-scope-open", || false))
                .child(|| Element::new().text("x"))
        });
        assert_eq!(el.template_id.as_deref(), Some("pop-scope__popover"));
    }
}
