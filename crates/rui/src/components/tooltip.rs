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
//! ## 用法
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

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::popup::{
    mark_floating_with_gap, panel_block_clicks, PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER,
    ATTR_FLOATING_MIN_W,
};
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id` + 触发器 `focus_id`。
#[derive(Clone)]
pub struct TooltipCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// 触发器 focus_id（Content 锚点）；Trigger 挂载时写入。
    pub trigger_fid: Signal<String>,
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
    let trigger_fid =
        use_signal(format!("{id}__tooltip_trigger_fid").as_str(), || format!("{id}__tooltip_trigger"));
    provide_context(
        tooltip_context(),
        TooltipCtx {
            open,
            id: id.clone(),
            trigger_fid,
        },
    );
    let mut root = body();
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__tooltip"));
    }
    root
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn chain_hover(mut el: Element, open: Signal<bool>) -> Element {
    let prev = el.on_hover.clone();
    el = el.on_hover(move |hovered| {
        if let Some(ref rc) = prev {
            rc.borrow_mut()(hovered);
        }
        open.set(hovered);
    });
    el
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
pub fn TooltipTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_tooltip("TooltipTrigger");
    let open = ctx.open;
    let trigger_id = if id.is_empty() {
        format!("{}__tooltip_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    match children {
        None => {
            let mut el: Element = interactive(label(trigger_id.as_str()).text("Hover").into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_hover(el, open);
            el
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = interactive(label(trigger_id.as_str()).text(t).into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_hover(el, open);
            el
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id);
            ctx.trigger_fid.set(fid);
            chain_hover(c, open)
        }
    }
}

/// `TooltipContent` 选项（`ui!` / [`tooltip_content`] 共用）。
#[derive(Clone, Debug)]
pub struct TooltipContentOpts {
    /// `top` / `bottom` / `top-start` / …（默认 `bottom`，贴触发器下方）。
    pub side: String,
    /// 额外 class（utility）。
    pub class: Option<String>,
}

impl Default for TooltipContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            class: None,
        }
    }
}

/// solid-ui / shadcn `TooltipContent`：Portal + floating 面板。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建）。
pub fn tooltip_content(opts: TooltipContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_tooltip("TooltipContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let trigger_fid = ctx.trigger_fid;
    let placement = tooltip_side_to_placement(&opts.side);
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{id}__tooltip_view");

    reactive_view(view_id, move || {
        let is_open = open.get();
        let Some(oz) = portal_sync(&id, open, OverlayKind::Tooltip, OverlayMode::Pinned) else {
            return Element::new();
        };
        if !is_open {
            return Element::new();
        }
        let mut panel: Element = r#box()
            .px("0.75rem")
            .py("0.375rem")
            .rounded("0.375rem")
            .max_w("20rem")
            .into();
        // solid-ui: bg-popover；轻阴影（避免压过触发器）。
        panel = panel
            .bg(tokens::popover())
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .shadow([0.0, 0.0, 0.0, 0.06], 3.0);
        let mut inner = body();
        // 纯文本叶子：套上 sm muted/foreground 字号。
        if is_text_leaf(&inner) {
            let t = inner.text.clone().unwrap_or_default();
            inner = text(t)
                .size(12.0)
                .color(parse_text_color("popover-foreground"))
                .into();
        }
        panel = panel.child(inner);
        if let Some(ref c) = class {
            panel = crate::style::with_class_list(panel, c);
        }
        let anchor = trigger_fid.peek();
        // 默认 Bottom；`compute_anchored_placement` 在下方空间不足时翻到上方。
        // gutter/sideOffset ≈ 4；w-fit（不匹配触发器宽）。
        mark_floating_with_gap(&mut panel, &id, &anchor, placement, 4.0);
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        panel = panel.absolute();
        panel_block_clicks(panel, oz.panel)
    })
}

// ---------------------------------------------------------------------------
// Builder（兼容旧 API）
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
                children: Some(trigger),
                base: None,
            });
            let tip = tooltip_content(
                TooltipContentOpts {
                    side,
                    class: None,
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
        assert_eq!(
            tooltip_side_to_placement("top"),
            PopupPlacement::TopCenter
        );
        assert_eq!(
            tooltip_side_to_placement("bottom"),
            PopupPlacement::BottomCenter
        );
        assert_eq!(
            tooltip_side_to_placement("bottom-start"),
            PopupPlacement::BottomStart
        );
    }
}
