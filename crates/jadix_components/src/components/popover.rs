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
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX（camelCase 宏别名：sideOffset / openSignal）
//! ui! {
//!     <Popover id="demo" open={pop_open}>
//!         <PopoverTrigger>
//!             <Button variant="outline" size="sm">Open</Button>
//!         </PopoverTrigger>
//!         <PopoverContent side="bottom" align="center" sideOffset={4} class="w-80">
//!             <PopoverHeader>
//!                 <PopoverTitle>Dimensions</PopoverTitle>
//!                 <PopoverDescription>Set the dimensions for the layer.</PopoverDescription>
//!             </PopoverHeader>
//!         </PopoverContent>
//!     </Popover>
//! }
//!
//! // B · Builder (snake_case only)
//! popover("demo")
//!     .open_signal(pop_open)
//!     .child(|| {
//!         let mut frag = Element::new();
//!         frag.set_attr("jadix_fragment", "1");
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

use crate::app::portal_overlay::{OverlayKind, OverlayMode, ScrollFreeze};
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay, anchored_overlay_content, provide_anchored_overlay_context,
    AnchoredOverlayContentOpts, FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::popup::{disarm_underlay_close, PopupPlacement, FLOATING_TRIGGER_GAP};
use crate::utils::take_packed;

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root context: `open` + stable instance `id` + trigger `focus_id`.
#[derive(Clone)]
pub struct PopoverCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// Trigger focus_id (Content anchor); written when Trigger mounts.
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
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
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

    /// Build Trigger / Content (and related) subtrees inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        popover_scope(self.id, self.open, body)
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

crate::__component_contract! {
PopoverTriggerProps [];
/// ShadCN `PopoverTrigger`：点击切换；子节点为触发器。
#[allow(non_snake_case)]
pub fn PopoverTrigger(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
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
            let mut el: Element = interactive(
                label(trigger_id.as_str())
                    .text(crate::tr_default_zh("jadix.popover.open", "Open", "打开"))
                    .into(),
            )
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

    if trigger.authoring_state.get::<crate::components::button::ButtonHoverPaint>().is_none() && trigger.input_paint_sync.is_none() {
        trigger.set_focusable(true);
    }
    let fid_focus = trigger
        .focus_id
        .clone()
        .unwrap_or_else(|| trigger_id.clone());
    ctx.trigger_fid.set(fid_focus);
    trigger
}
}

/// `PopoverContent` 选项（`ui!` / [`popover_content`] 共用）。
#[derive(Clone)]
pub struct PopoverContentOpts {
    /// `top` / `bottom` / `left` / `right`（默认 `bottom`）。
    pub side: String,
    /// `start` / `center` / `end`（默认 `center`）。
    pub align: String,
    /// Gap between trigger and panel (logical px); default **4** (ShadCN `sideOffset`).
    pub side_offset: f32,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
    /// `flip`（默认）或 `fixed`：钉在触发器首选侧，滚动不夹高度。
    pub strategy: Option<String>,
    /// 面板宽；默认 `18rem`（`w-72`）。空 / `auto` → 内容宽。
    pub width: Option<String>,
    /// Freeze page scroll while the popover is open. Default `false`.
    /// Overlay ScrollAreas still move; use [`Self::scroll_freeze`] for overlay-only or all.
    pub lock_page_scroll: bool,
    /// Explicit freeze scope; wins over [`Self::lock_page_scroll`] when set.
    pub scroll_freeze: Option<ScrollFreeze>,
}

impl Default for PopoverContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            align: "center".into(),
            side_offset: FLOATING_TRIGGER_GAP,
            panel: Default::default(),
            strategy: None,
            width: None,
            lock_page_scroll: false,
            scroll_freeze: None,
        }
    }
}

impl PopoverContentOpts {
    crate::impl_content_opts_chrome!();

    /// Freeze page scroll while this popover is open. Default `false`.
    pub fn lock_page_scroll(mut self, yes: bool) -> Self {
        self.lock_page_scroll = yes;
        self
    }

    /// Freeze wheel / pan / fling for page, overlay, or the whole program.
    pub fn scroll_freeze(mut self, scope: ScrollFreeze) -> Self {
        self.scroll_freeze = Some(scope);
        self
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
        .popup_shadow();
    if is_text_leaf(&body) {
        let t = body.text.clone().unwrap_or_default();
        body = text(t)
            .size(14.0)
            .color(parse_text_color("popover-foreground"))
            .into();
    }
    panel.set_attr("data-slot", "popover-content");
    crate::style::apply_stylesheet_rules(panel.child(body))
}

/// ShadCN `PopoverContent`：Portal + floating；仅在 open 时构建。
/// `ui!` 中由宏编成此函数。
pub fn popover_content(opts: PopoverContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let chrome = opts.panel;
    let width = opts.width.clone();
    let strategy = opts.strategy.clone().unwrap_or_default();
    let ao_opts = AnchoredOverlayContentOpts {
        side: opts.side.clone(),
        align: opts.align.clone(),
        side_offset: opts.side_offset,
        strategy,
        kind: OverlayKind::Popover,
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Stacked,
        width: width.clone().or_else(|| Some("18rem".into())),
        lock_page_scroll: if opts.lock_page_scroll {
            Some(true)
        } else {
            None
        },
        scroll_freeze: opts.scroll_freeze,
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        let panel = popover_surface(body(), width.as_deref());
        chrome.apply(panel)
    })
}

crate::__component_contract! {
PopoverHeaderProps [];
/// ShadCN `PopoverHeader`。
#[allow(non_snake_case)]
pub fn PopoverHeader(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .gap("0.25rem")
            .flex_shrink(0.0)
            .children(kids),
    )
}
}

crate::__component_contract! {
PopoverTitleProps [];
/// ShadCN `PopoverTitle`（`font-medium` / `text-sm`）。
#[allow(non_snake_case)]
pub fn PopoverTitle(
plain [] children: Option<Element>,
) -> Element {
    let fg = parse_text_color("popover-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(500.0).text_color(fg),
        None => Element::from(text("").size(14.0).weight(500.0).color(fg)),
    }
}
}

crate::__component_contract! {
PopoverDescriptionProps [];
/// ShadCN `PopoverDescription`（`text-sm` / `muted-foreground`）。
#[allow(non_snake_case)]
pub fn PopoverDescription(
plain [] children: Option<Element>,
) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c.text_size(14.0).font_weight(400.0).text_color(muted),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)),
    }
}
}

impl_simple_slot_builder!(
    PopoverTriggerBuilder,
    popover_trigger,
    PopoverTrigger,
    PopoverTriggerProps
);
impl_simple_slot_builder!(
    PopoverHeaderBuilder,
    popover_header,
    PopoverHeader,
    PopoverHeaderProps
);
impl_simple_slot_builder!(
    PopoverTitleBuilder,
    popover_title,
    PopoverTitle,
    PopoverTitleProps
);
impl_simple_slot_builder!(
    PopoverDescriptionBuilder,
    popover_description,
    PopoverDescription,
    PopoverDescriptionProps
);

pub struct PopoverContentSlotBuilder {
    opts: PopoverContentOpts,
}

pub fn popover_content_slot() -> PopoverContentSlotBuilder {
    PopoverContentSlotBuilder {
        opts: PopoverContentOpts::default(),
    }
}

impl PopoverContentSlotBuilder {
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
    pub fn side(mut self, side: impl Into<String>) -> Self {
        self.opts.side = side.into();
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        popover_content(self.opts, body)
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
        side_offset: FLOATING_TRIGGER_GAP,
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
            anchor_focus_id,
            placement,
            side_offset,
            strategy,
            ..
        } = builder;
        let id = id.unwrap_or_else(|| "popover".into());
        let Some(build) = body else {
            return Element::new();
        };
        let strat = if strategy.trim().eq_ignore_ascii_case("fixed") {
            FloatingStrategy::Fixed
        } else {
            FloatingStrategy::Auto
        };
        if let Some(anchor) = anchor_focus_id {
            anchored_overlay(&id)
                .open(open)
                .placement(Placement::Trigger {
                    focus_id: anchor,
                    placement,
                    gap: side_offset,
                    strategy: strat,
                })
                .kind(OverlayKind::Popover)
                .width("18rem")
                .child(move || popover_surface(build(), None))
        } else {
            anchored_overlay(&id)
                .open(open)
                .placement(Placement::Unanchored)
                .kind(OverlayKind::Popover)
                .width("18rem")
                .child(move || {
                    popover_surface(build(), None)
                        .fixed()
                        .top("1rem")
                        .right("1rem")
                })
        }
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/popover.rs"]
mod tests;
