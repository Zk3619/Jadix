//! # Tooltip
//!
//! 对齐 solid-ui / shadcn **复合 Tooltip**：
//!
//! ```text
//! Tooltip
//! ├── TooltipTrigger     // hover 开合；子节点为组件时挂载，纯文本则默认 Label
//! └── TooltipContent     // Portal + floating；默认下方，空间不足翻上方；w-fit
//! ```
//!
//! `ui!` 中 `<Tooltip>` / `<TooltipContent>` 由宏展开。Builder
//! [`tooltip`] / [`TooltipBuilder::trigger`] 仍可用（内部接到同一套实现）。
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <Tooltip id="tip">
//!         <TooltipTrigger>
//!             <Button variant="outline" size="sm">Hover</Button>
//!         </TooltipTrigger>
//!         <TooltipContent>
//!             "Hover tip"
//!         </TooltipContent>
//!     </Tooltip>
//! }
//! ```

use std::sync::OnceLock;

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingStrategy, Placement,
};
use crate::utils::floating_motion::FloatingMotionKind;
use crate::utils::hover_intent::{
    bind_hover_intent_effect, chain_region_hover, set_hovered, HoverIntent,
};
use crate::utils::popup::{PopupPlacement, FLOATING_TRIGGER_GAP};
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root context: `open` + stable instance `id` + trigger `focus_id` + hover intent.
#[derive(Clone)]
pub struct TooltipCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// Trigger focus_id (Content anchor); written when Trigger mounts.
    pub trigger_fid: Signal<String>,
    trigger_hovered: Signal<bool>,
    content_hovered: Signal<bool>,
    open_delay_ms: Signal<u64>,
    close_delay_ms: Signal<u64>,
}

fn tooltip_context() -> Context<TooltipCtx> {
    static CTX: OnceLock<Context<TooltipCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<TooltipCtx>)
}

/// 读取最近的 [`Tooltip`] 上下文。
pub fn use_tooltip() -> Option<TooltipCtx> {
    use_context(tooltip_context())
}

fn require_tooltip(part: &str) -> TooltipCtx {
    use_tooltip().unwrap_or_else(|| {
        panic!("{part} must be used within <Tooltip> / tooltip_scope");
    })
}

/// 提供 Tooltip 上下文并构建子树（`ui!` `<Tooltip>` / Builder 共用）。
pub fn tooltip_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "tooltip");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__tooltip_open").as_str(), || false));
    let trigger_fid = use_signal(format!("{id}__tooltip_trigger_fid").as_str(), || {
        format!("{id}__tooltip_trigger")
    });
    let trigger_hovered = use_signal(format!("{id}__tooltip_th").as_str(), || false);
    let content_hovered = use_signal(format!("{id}__tooltip_ch").as_str(), || false);
    let open_delay_ms = use_signal(format!("{id}__tooltip_od").as_str(), || 0_u64);
    let close_delay_ms = use_signal(format!("{id}__tooltip_cd").as_str(), || 0_u64);
    let mut root = with_child_owner(|| {
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
        provide_context(
            tooltip_context(),
            TooltipCtx {
                open,
                id: id.clone(),
                trigger_fid,
                trigger_hovered,
                content_hovered,
                open_delay_ms,
                close_delay_ms,
            },
        );
        bind_hover_intent_effect(
            &id,
            HoverIntent {
                open,
                trigger_hovered,
                content_hovered,
                open_delay_ms,
                close_delay_ms,
            },
        );
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__tooltip"));
    }
    root
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

/// shadcn `TooltipContent` 气泡样式（无 Portal / 无 floating 锚点）。
/// Chart 选中钉与 [`tooltip_content`] 共用，避免第二套面板。
pub fn tooltip_bubble(mut body: Element) -> Element {
    let mut panel: Element = r#box()
        .px("0.75rem")
        .py("0.375rem")
        .rounded("0.375rem")
        .max_w("20rem")
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
            .size(12.0)
            .color(parse_text_color("popover-foreground"))
            .into();
    }
    panel.child(body)
}

fn chain_hover(
    el: Element,
    on_enter: impl Fn() + 'static,
    on_leave: impl Fn() + 'static,
) -> Element {
    chain_region_hover(el, on_enter, on_leave)
}

/// solid-ui / shadcn `side` → [`PopupPlacement`]（`bottom`/`top` 为水平居中，空间不足时贴边）。
pub fn tooltip_side_to_placement(side: &str) -> PopupPlacement {
    match side.trim().to_ascii_lowercase().as_str() {
        "bottom-start" => PopupPlacement::BottomStart,
        "bottom-end" => PopupPlacement::BottomEnd,
        "bottom" | "bottom-center" | "" => PopupPlacement::BottomCenter,
        "top-start" => PopupPlacement::TopStart,
        "top-end" => PopupPlacement::TopEnd,
        "top" | "top-center" | "left" | "right" => PopupPlacement::TopCenter,
        other => PopupPlacement::from_attr(other),
    }
}

// ---------------------------------------------------------------------------
// Trigger / Content
// ---------------------------------------------------------------------------

/// solid-ui `TooltipTrigger`：子节点为组件时挂载 hover；纯文本则默认 [`Label`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TooltipTrigger(
    id: String,
    open_delay: Option<u64>,
    close_delay: Option<u64>,
    children: Option<Element>,
) -> Element {
    let ctx = require_tooltip("TooltipTrigger");
    if let Some(ms) = open_delay {
        ctx.open_delay_ms.set(ms);
    }
    if let Some(ms) = close_delay {
        ctx.close_delay_ms.set(ms);
    }
    let enter = {
        let h = ctx.trigger_hovered;
        move || set_hovered(h, true)
    };
    let leave = {
        let h = ctx.trigger_hovered;
        move || set_hovered(h, false)
    };
    let trigger_id = if id.is_empty() {
        format!("{}__tooltip_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    match children {
        None => {
            let h = ctx.trigger_hovered;
            let mut el: Element = interactive(
                label(trigger_id.as_str())
                    .text(crate::tr_default_zh("rui.tooltip.hover", "Hover", "悬停"))
                    .into(),
            )
            .focus_id(trigger_id.as_str())
            .focusable(true)
            .into();
            el = chain_hover(
                el,
                move || set_hovered(h, true),
                move || set_hovered(h, false),
            );
            el
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = interactive(label(trigger_id.as_str()).text(t).into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_hover(el, enter, leave);
            el
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id);
            ctx.trigger_fid.set(fid);
            chain_hover(c, enter, leave)
        }
    }
}

/// `TooltipContent` 选项（`ui!` / [`tooltip_content`] 共用）。
#[derive(Clone)]
pub struct TooltipContentOpts {
    /// `top` / `bottom` / `top-start` / …（默认 `bottom`，贴触发器下方）。
    pub side: String,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
}

impl Default for TooltipContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            panel: Default::default(),
        }
    }
}

impl TooltipContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// solid-ui / shadcn `TooltipContent`：Portal + floating 面板。
/// `ui!` lowers to this function via the macro (subtree rebuilt lazily when open).
pub fn tooltip_content(opts: TooltipContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_tooltip("TooltipContent");
    let content_hovered = ctx.content_hovered;
    let chrome = opts.panel;
    let placement = tooltip_side_to_placement(&opts.side);
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            "",
            placement,
            FLOATING_TRIGGER_GAP,
            FloatingStrategy::Auto,
        )),
        kind: OverlayKind::Tooltip,
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Pinned,
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        let mut panel = tooltip_bubble(body());
        panel = chrome.apply(panel);
        let ch = content_hovered;
        chain_hover(
            panel,
            move || set_hovered(ch, true),
            move || set_hovered(ch, false),
        )
    })
}

impl_simple_slot_builder!(
    TooltipTriggerBuilder,
    tooltip_trigger,
    TooltipTrigger,
    TooltipTriggerProps
);

pub struct TooltipContentSlotBuilder {
    opts: TooltipContentOpts,
}

pub fn tooltip_content_slot() -> TooltipContentSlotBuilder {
    TooltipContentSlotBuilder {
        opts: TooltipContentOpts::default(),
    }
}

impl TooltipContentSlotBuilder {
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
        tooltip_content(self.opts, body)
    }
}

// ---------------------------------------------------------------------------
// Builder (legacy API compatible)
// ---------------------------------------------------------------------------

pub struct TooltipBuilder {
    id: String,
    content: String,
    open: Option<Signal<bool>>,
    placement: PopupPlacement,
}

pub fn tooltip(id: impl Into<String>, content: impl Into<String>) -> TooltipBuilder {
    TooltipBuilder {
        id: id.into(),
        content: content.into(),
        open: None,
        placement: PopupPlacement::BottomCenter,
    }
}

impl TooltipBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    /// 旧 API：`trigger(|open| …)`；内部接到 [`tooltip_scope`] + Content。
    pub fn trigger(self, build: impl FnOnce(Signal<bool>) -> Element) -> Element {
        let id = self.id.clone();
        let content = self.content.clone();
        let open = self.open;
        let side = self.placement.as_attr().to_string();
        tooltip_scope(id, open, || {
            let ctx = use_tooltip().expect("tooltip_scope");
            let open_sig = ctx.open;
            let trigger = build(open_sig);
            let trigger = TooltipTrigger(TooltipTriggerProps {
                id: String::new(),
                open_delay: None,
                close_delay: None,
                children: Some(trigger),
                base: None,
            });
            let tip = tooltip_content(
                TooltipContentOpts {
                    side,
                    ..Default::default()
                },
                move || {
                    Element::from(
                        text(content.as_str())
                            .size(12.0)
                            .color(parse_text_color("popover-foreground")),
                    )
                },
            );
            // Fragment：Trigger 可与兄弟并排放在父 HStack。
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![trigger, tip])
        })
    }
}

impl From<TooltipBuilder> for Element {
    fn from(builder: TooltipBuilder) -> Self {
        let label = builder.content.clone();
        builder.trigger(move |_open| {
            let mut el: Element = r#box().px("0.5rem").py("0.25rem").into();
            el = el.child(text(label.as_str()));
            el
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_builds_tree() {
        let _el: Element = tooltip("tip1", "Hello").into();
    }

    #[test]
    fn tooltip_side_maps() {
        assert_eq!(tooltip_side_to_placement("top"), PopupPlacement::TopCenter);
        assert_eq!(
            tooltip_side_to_placement("bottom"),
            PopupPlacement::BottomCenter
        );
        assert_eq!(
            tooltip_side_to_placement("bottom-start"),
            PopupPlacement::BottomStart
        );
    }

    #[test]
    fn tooltip_content_anchor_is_zero_sized() {
        use crate::core::reactivity::create_root;
        let (_owner, el) = create_root(|| {
            tooltip_scope("tip-anchor", None, || {
                tooltip_content(TooltipContentOpts::default(), || Element::new().text("hi"))
            })
        });
        fn find_zero(el: &Element) -> bool {
            let is_view = el.template_id.as_deref() == Some("tip-anchor__view");
            let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
                && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
            (is_view && dim0) || el.children.iter().any(find_zero)
        }
        assert!(
            find_zero(&el),
            "TooltipContent must use a 0×0 portal anchor"
        );
    }
}
