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
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::components::label::label;
use crate::core::reactivity::{
    create_context, create_effect, on_cleanup, provide_context, use_context, use_signal,
    with_child_owner, Context, Signal,
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
    bind_hover_intent_effect, chain_region_hover, clear_pointer_tooltip, end_pointer_tooltip_grace,
    pointer_tooltip_enter, pointer_tooltip_leave, set_hovered, HoverIntent,
    DEFAULT_HOVER_OPEN_DELAY_MS, NEVER_HOVER_DELAY_MS,
};
use crate::utils::popup::{PopupPlacement, FLOATING_TRIGGER_GAP};

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
    dismissed: Signal<bool>,
    /// Delay before pointer hover opens the tooltip, in milliseconds.
    pub open_delay_ms: Signal<u64>,
    /// Delay before pointer leave closes the tooltip, in milliseconds.
    pub close_delay_ms: Signal<u64>,
}

impl TooltipCtx {
    fn dismiss(&self) {
        self.dismissed.set(true);
        set_hovered(self.content_hovered, false);
        self.open.set(false);
        crate::utils::hover_intent::clear(&self.id);
        clear_pointer_tooltip(&self.id);
        end_pointer_tooltip_grace();
    }
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
    tooltip_scope_with_delays(id, open, None, None, body)
}

/// Provide Tooltip context with root-level pointer delays before building its children.
pub fn tooltip_scope_with_delays(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    open_delay: Option<u64>,
    close_delay: Option<u64>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "tooltip");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__tooltip_open").as_str(), || false));
    let trigger_fid = use_signal(format!("{id}__tooltip_trigger_fid").as_str(), || {
        format!("{id}__tooltip_trigger")
    });
    let trigger_hovered = use_signal(format!("{id}__tooltip_th").as_str(), || false);
    let content_hovered = use_signal(format!("{id}__tooltip_ch").as_str(), || false);
    let dismissed = use_signal(format!("{id}__tooltip_dismissed").as_str(), || false);
    let open_delay_ms = use_signal(format!("{id}__tooltip_od").as_str(), || {
        DEFAULT_HOVER_OPEN_DELAY_MS
    });
    let close_delay_ms = use_signal(format!("{id}__tooltip_cd").as_str(), || 0_u64);
    if let Some(ms) = open_delay {
        open_delay_ms.set(ms);
    }
    if let Some(ms) = close_delay {
        close_delay_ms.set(ms);
    }
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
                dismissed,
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
        let cleanup_id = id.clone();
        on_cleanup(move || clear_pointer_tooltip(&cleanup_id));
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
        body.text_style.size.get_or_insert(12.0);
        body.text_style
            .color
            .get_or_insert(parse_text_color("popover-foreground"));
    }
    panel.set_attr("data-slot", "tooltip-content");
    crate::style::apply_stylesheet_rules(panel.child(body))
}

fn chain_hover(
    el: Element,
    on_enter: impl Fn() + 'static,
    on_leave: impl Fn() + 'static,
) -> Element {
    chain_region_hover(el, on_enter, on_leave)
}

fn chain_trigger_hover(el: Element, ctx: TooltipCtx, focus_id: String) -> Element {
    let focused_id = use_signal("__jadix_button_focus_id", String::new);
    let focus_visible = use_signal("__jadix_focus_visible", || false);
    let pointer_hovered = use_signal(
        format!("{focus_id}__tooltip_pointer_hovered").as_str(),
        || false,
    );
    let keyboard_focused = use_signal(
        format!("{focus_id}__tooltip_keyboard_focused").as_str(),
        || false,
    );
    let dismissed = ctx.dismissed;
    let trigger_hovered = ctx.trigger_hovered;
    let open = ctx.open;
    create_effect(move || {
        let is_keyboard_focused = focus_visible.get()
            && focused_id.get() == focus_id
            && !crate::utils::roving::is_restored_focus(&focus_id);
        let was_keyboard_focused = keyboard_focused.peek();
        set_hovered(keyboard_focused, is_keyboard_focused);
        if is_keyboard_focused && !was_keyboard_focused {
            dismissed.set(false);
        }
        let is_dismissed = dismissed.get();
        set_hovered(
            trigger_hovered,
            (pointer_hovered.get() || is_keyboard_focused) && !is_dismissed,
        );
        if is_keyboard_focused && !is_dismissed {
            open.set(true);
        }
    });
    let enter_id = ctx.id.clone();
    let leave_id = ctx.id.clone();
    let open_delay_ms = ctx.open_delay_ms;
    let mut el = chain_hover(
        el,
        move || {
            // Repeated hover notifications after a press are not a new entry.
            if pointer_hovered.peek() {
                return;
            }
            dismissed.set(false);
            let skip_delay = open_delay_ms.peek() != NEVER_HOVER_DELAY_MS
                && pointer_tooltip_enter(&enter_id, open);
            pointer_hovered.set(true);
            if skip_delay {
                open.set(true);
            }
        },
        move || {
            pointer_tooltip_leave(&leave_id);
            pointer_hovered.set(false);
        },
    );
    let click_ctx = ctx.clone();
    let mut click = el.on_click.clone();
    el = el.on_click(move || {
        click_ctx.dismiss();
        if let Some(ref mut handler) = click {
            handler.borrow_mut()();
        }
    });
    let press_ctx = ctx.clone();
    let mut pointer_down = el.on_mousedown.clone();
    el = el.on_pointer_down(move |event| {
        press_ctx.dismiss();
        if let Some(ref mut handler) = pointer_down {
            handler.borrow_mut()(event);
        }
    });
    crate::authoring::keys::attach(&mut el, move |event| {
        if event.state == ElementState::Pressed
            && matches!(event.key, Key::Named(NamedKey::Escape))
            && (ctx.open.peek() || ctx.trigger_hovered.peek() || ctx.content_hovered.peek())
        {
            ctx.dismiss();
            return true;
        }
        false
    });
    let mut key_down = el.on_key_down.clone();
    el.on_key_down(move |event| {
        end_pointer_tooltip_grace();
        key_down
            .as_mut()
            .is_some_and(|handler| handler.borrow_mut()(event))
    })
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

crate::__component_contract! {
TooltipTriggerProps [];
/// solid-ui `TooltipTrigger`：子节点为组件时挂载 hover；纯文本则默认 [`Label`]。
#[allow(non_snake_case)]
pub fn TooltipTrigger(
identity [] id: String,
plain [] open_delay: Option<u64>,
plain [] close_delay: Option<u64>,
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_tooltip("TooltipTrigger");
    if let Some(ms) = open_delay {
        ctx.open_delay_ms.set(ms);
    }
    if let Some(ms) = close_delay {
        ctx.close_delay_ms.set(ms);
    }
    let trigger_id = if id.is_empty() {
        format!("{}__tooltip_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    match children {
        None => {
            let mut el: Element = interactive(
                label(trigger_id.as_str())
                    .text(crate::tr_default_zh("jadix.tooltip.hover", "Hover", "悬停"))
                    .into(),
            )
            .focus_id(trigger_id.as_str())
            .focusable(true)
            .into();
            chain_trigger_hover(el, ctx, trigger_id)
        }
        Some(mut c) => {
            if is_text_leaf(&c) {
                c.text_style.size.get_or_insert(14.0);
                c.text_style.weight.get_or_insert(500);
                c.text_style.color.get_or_insert(parse_text_color("foreground"));
                if !c.text_style.line_height_specified() {
                    c = c.text_leading_none();
                }
                c = c.focusable(true);
            }
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id);
            ctx.trigger_fid.set(fid);
            let fid = ctx.trigger_fid.peek();
            chain_trigger_hover(c, ctx, fid)
        }
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
    let dismissed = ctx.dismissed;
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
            move || set_hovered(ch, !dismissed.peek()),
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
    open_delay_ms: Option<u64>,
    close_delay_ms: Option<u64>,
}

pub fn tooltip(id: impl Into<String>, content: impl Into<String>) -> TooltipBuilder {
    TooltipBuilder {
        id: id.into(),
        content: content.into(),
        open: None,
        placement: PopupPlacement::BottomCenter,
        open_delay_ms: None,
        close_delay_ms: None,
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

    /// Pointer-hover delay before opening, in milliseconds. Defaults to 300; 0 opens immediately.
    /// `u64::MAX` disables hover opening; explicit state changes remain available.
    pub fn open_delay(mut self, delay: u64) -> Self {
        self.open_delay_ms = Some(delay);
        self
    }

    /// Delay after pointer leave, in milliseconds. Defaults to 0.
    /// `u64::MAX` disables hover closing.
    pub fn close_delay(mut self, delay: u64) -> Self {
        self.close_delay_ms = Some(delay);
        self
    }

    /// Compatibility alias for [`Self::open_delay`].
    #[doc(hidden)]
    pub fn open_delay_ms(self, delay: u64) -> Self {
        self.open_delay(delay)
    }

    /// Compatibility alias for [`Self::close_delay`].
    #[doc(hidden)]
    pub fn close_delay_ms(self, delay: u64) -> Self {
        self.close_delay(delay)
    }

    /// 旧 API：`trigger(|open| …)`；内部接到 [`tooltip_scope`] + Content。
    pub fn trigger(self, build: impl FnOnce(Signal<bool>) -> Element) -> Element {
        let id = self.id.clone();
        let content = self.content.clone();
        let open = self.open;
        let side = self.placement.as_attr().to_string();
        let open_delay_ms = self.open_delay_ms;
        let close_delay_ms = self.close_delay_ms;
        tooltip_scope(id, open, || {
            let ctx = use_tooltip().expect("tooltip_scope");
            let open_sig = ctx.open;
            let trigger = build(open_sig);
            let trigger = TooltipTrigger(TooltipTriggerProps {
                id: String::new(),
                open_delay: open_delay_ms,
                close_delay: close_delay_ms,
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
            frag.set_attr("jadix_fragment", "1");
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/tooltip.rs"]
mod tests;
