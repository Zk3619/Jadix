//! # HoverCard
//!
//! Aligns with ShadCN **base** [`hover-card.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/hover-card.tsx):
//!
//! ```text
//! HoverCard
//! ├── HoverCardTrigger     // hover open/close; openDelay / closeDelay (PreviewCard parity)
//! └── HoverCardContent     // Portal + floating; default bottom / center / sideOffset=4
//! ```
//!
//! ## Width (vs ShadCN)
//! - **base** (this file): component does **not** hard-code width; demos override with `class="w-64"`.
//! - **radix / new-york**: component class hard-codes `w-64`.
//! Jadix follows **base**: content width by default; pass `width` / `class` when fixed width is needed (Gallery demo uses `w-64`).
//!
//! ## Open/close delays (Solid-style)
//! Same shape as Solid `createEffect` + `setTimeout` / `onCleanup`: on the **UI thread**,
//! `create_effect` watches hover intent and registers deadlines in this module's timer table; the event loop
//! [`AboutToWait`](crate::app::lifecycle) wakes via [`crate::utils::hover_intent`].
//!
//! `ui!` `<HoverCard>` / `<HoverCardContent>` are macro-expanded. Builder
//! [`hover_card`] / [`HoverCardBuilder::child`] share the same implementation.
//!
//! ## Usage (dual surface)
//! ```ignore
//! // A · RSX（camelCase：sideOffset / openSignal / openDelay / closeDelay / delay）
//! ui! {
//!     <HoverCard id="demo" open={hc_open}>
//!         <HoverCardTrigger openDelay=10 closeDelay=100>
//!             <Button variant="link" size="sm">@nextjs</Button>
//!         </HoverCardTrigger>
//!         <HoverCardContent side="bottom" class="w-64">
//!             <Label font_weight=600>"@nextjs"</Label>
//!         </HoverCardContent>
//!     </HoverCard>
//! }
//!
//! // B · Builder
//! hover_card("demo").open_signal(hc_open).child(|| { /* Trigger + content */ })
//! ```

use std::sync::OnceLock;

use crate::app::portal_overlay::OverlayMode;
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
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
};
use crate::utils::floating_motion::FloatingMotionKind;
use crate::utils::hover_intent::{
    bind_hover_intent_effect, chain_region_hover, set_hovered, HoverIntent,
};
use crate::utils::popup::{ATTR_FLOATING_W, FLOATING_TRIGGER_GAP};
use jadix_macros::component;

/// Default open delay (ms); demos often use `delay={10}` — product default stays snappy.
const DEFAULT_OPEN_DELAY_MS: u64 = 10;
/// Default close delay (ms); demos use `closeDelay={100}` so the pointer can cross the sideOffset gap.
const DEFAULT_CLOSE_DELAY_MS: u64 = 100;

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root context: `open` + instance `id` + trigger anchor + dual hover + delays.
#[derive(Clone)]
pub struct HoverCardCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// Trigger focus_id (Content anchor); written when Trigger mounts.
    pub trigger_fid: Signal<String>,
    trigger_hovered: Signal<bool>,
    content_hovered: Signal<bool>,
    open_delay_ms: Signal<u64>,
    close_delay_ms: Signal<u64>,
}

fn hover_card_context() -> Context<HoverCardCtx> {
    static CTX: OnceLock<Context<HoverCardCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<HoverCardCtx>)
}

/// Read the nearest [`HoverCard`] context.
pub fn use_hover_card() -> Option<HoverCardCtx> {
    use_context(hover_card_context())
}

fn require_hover_card(part: &str) -> HoverCardCtx {
    use_hover_card().unwrap_or_else(|| {
        panic!("{part} must be used within <HoverCard> / hover_card_scope");
    })
}

/// Provide HoverCard context and build the subtree (shared by `ui!` `<HoverCard>` / Builder).
pub fn hover_card_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "hover_card");
    let open =
        open.unwrap_or_else(|| use_signal(format!("{id}__hover_card_open").as_str(), || false));
    let trigger_fid = use_signal(format!("{id}__hover_card_trigger_fid").as_str(), || {
        format!("{id}__hover_card_trigger")
    });
    let trigger_hovered = use_signal(format!("{id}__hover_card_th").as_str(), || false);
    let content_hovered = use_signal(format!("{id}__hover_card_ch").as_str(), || false);
    let open_delay_ms = use_signal(format!("{id}__hover_card_od").as_str(), || {
        DEFAULT_OPEN_DELAY_MS
    });
    let close_delay_ms = use_signal(format!("{id}__hover_card_cd").as_str(), || {
        DEFAULT_CLOSE_DELAY_MS
    });

    let mut root = with_child_owner(|| {
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
        provide_context(
            hover_card_context(),
            HoverCardCtx {
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
        root.template_id = Some(format!("{id}__hover_card"));
    }
    root
}

/// Builder：`.open(sig).child(|| …)` → [`hover_card_scope`]。
pub struct HoverCardBuilder {
    id: String,
    open: Option<Signal<bool>>,
}

/// Composite HoverCard entry (parity with `<HoverCard>`).
pub fn hover_card(id: impl Into<String>) -> HoverCardBuilder {
    HoverCardBuilder {
        id: id.into(),
        open: None,
    }
}

impl HoverCardBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    /// Same as [`Self::open`] (RSX `openSignal` parity).
    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    /// Build Trigger / Content subtree inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        hover_card_scope(self.id, self.open, body)
    }
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn chain_hover_intent(
    el: Element,
    on_enter: impl Fn() + 'static,
    on_leave: impl Fn() + 'static,
) -> Element {
    chain_region_hover(el, on_enter, on_leave)
}

// ---------------------------------------------------------------------------
// Trigger / Content
// ---------------------------------------------------------------------------

/// ShadCN `HoverCardTrigger`: hover open/close; child is the trigger.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn HoverCardTrigger(
    id: String,
    open_delay: Option<u64>,
    close_delay: Option<u64>,
    children: Option<Element>,
) -> Element {
    let ctx = require_hover_card("HoverCardTrigger");
    if let Some(ms) = open_delay {
        ctx.open_delay_ms.set(ms);
    }
    if let Some(ms) = close_delay {
        ctx.close_delay_ms.set(ms);
    }
    let trigger_id = if id.is_empty() {
        format!("{}__hover_card_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    let enter = {
        let h = ctx.trigger_hovered;
        move || set_hovered(h, true)
    };
    let leave = {
        let h = ctx.trigger_hovered;
        move || set_hovered(h, false)
    };

    match children {
        None => {
            let mut el: Element = interactive(label(trigger_id.as_str()).text("Hover").into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_hover_intent(el, enter, leave);
            el
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = interactive(label(trigger_id.as_str()).text(t).into())
                .focus_id(trigger_id.as_str())
                .focusable(true)
                .into();
            el = chain_hover_intent(el, enter, leave);
            el
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id);
            ctx.trigger_fid.set(fid);
            chain_hover_intent(c, enter, leave)
        }
    }
}

/// `HoverCardContent` options (shared by `ui!` / [`hover_card_content`]).
#[derive(Clone)]
pub struct HoverCardContentOpts {
    /// `top` / `bottom` / `left` / `right` (default `bottom`).
    pub side: String,
    /// `start` / `center` / `end` (default `center`).
    pub align: String,
    /// Gap between trigger and panel (logical px); default **4** (ShadCN `sideOffset`).
    pub side_offset: f32,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
    /// Optional panel width. **Default none** (content width, base parity); pass `16rem` / `w-64` for demo / radix.
    pub width: Option<String>,
}

impl Default for HoverCardContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            align: "center".into(),
            side_offset: FLOATING_TRIGGER_GAP,
            panel: Default::default(),
            width: None,
        }
    }
}

impl HoverCardContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// ShadCN `cn-hover-card-content` panel surface (no Portal / no floating).
///
/// No default width (intrinsic content width); lock width only when `width` is set and not `auto`.
pub fn hover_card_surface(mut body: Element, width: Option<&str>) -> Element {
    let w = width
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"));
    let mut panel: Element = vstack().p("1rem").gap("0.25rem").rounded("0.375rem").into();
    if let Some(w) = w {
        panel = panel.w(w);
    }
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
    panel.child(body)
}

/// Write `opts.width` or a class-resolved length into [`ATTR_FLOATING_W`] so sync does not treat it as content-auto.
fn seed_hover_card_floating_w(panel: &mut Element, width: Option<&str>) {
    if let Some(w) = width {
        let w = w.trim();
        if !w.is_empty() && !w.eq_ignore_ascii_case("auto") {
            panel.set_attr(ATTR_FLOATING_W, w);
            return;
        }
    }
    if let Some(px) = crate::layout::dim_length_px(panel.style.size.width) {
        if px > 0.5 {
            panel.set_attr(ATTR_FLOATING_W, format!("{px}px"));
        }
    }
}

/// ShadCN `HoverCardContent`: Portal + floating; built only while open.
/// Macro-lowered from `ui!` into this function.
pub fn hover_card_content(
    opts: HoverCardContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_hover_card("HoverCardContent");
    let content_hovered = ctx.content_hovered;
    let chrome = opts.panel;
    let width = opts.width.clone();
    let ao_opts = AnchoredOverlayContentOpts {
        side: opts.side.clone(),
        align: opts.align.clone(),
        side_offset: opts.side_offset,
        kind: crate::app::portal_overlay::OverlayKind::Tooltip,
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Pinned,
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        let mut panel = hover_card_surface(body(), width.as_deref());
        panel = chrome.apply(panel);
        let ch = content_hovered;
        panel = chain_hover_intent(
            panel,
            move || set_hovered(ch, true),
            move || set_hovered(ch, false),
        );
        seed_hover_card_floating_w(&mut panel, width.as_deref());
        panel
    })
}

impl_simple_slot_builder!(
    HoverCardTriggerBuilder,
    hover_card_trigger,
    HoverCardTrigger,
    HoverCardTriggerProps
);

pub struct HoverCardContentSlotBuilder {
    opts: HoverCardContentOpts,
}

pub fn hover_card_content_slot() -> HoverCardContentSlotBuilder {
    HoverCardContentSlotBuilder {
        opts: HoverCardContentOpts::default(),
    }
}

impl HoverCardContentSlotBuilder {
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
        hover_card_content(self.opts, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::popover::popover_side_align_to_placement;
    use crate::core::reactivity::create_root;
    use crate::style::with_class_list;
    use crate::utils::popup::PopupPlacement;

    #[test]
    fn side_align_defaults_via_popover_helper() {
        assert_eq!(
            popover_side_align_to_placement("bottom", "center"),
            PopupPlacement::BottomCenter
        );
    }

    #[test]
    fn content_anchor_is_zero_sized() {
        let (_owner, el) = create_root(|| {
            hover_card_scope("hc-anchor", None, || {
                hover_card_content(HoverCardContentOpts::default(), || {
                    Element::new().text("hi")
                })
            })
        });
        fn find_zero(el: &Element) -> bool {
            let is_view = el.template_id.as_deref() == Some("hc-anchor__view");
            let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
                && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
            (is_view && dim0) || el.children.iter().any(find_zero)
        }
        assert!(
            find_zero(&el),
            "HoverCardContent must use a 0×0 portal anchor"
        );
    }

    #[test]
    fn scope_exposes_template() {
        let (_owner, el) = create_root(|| {
            hover_card("hc-scope")
                .open_signal(use_signal("hc-scope-open", || false))
                .child(|| Element::new().text("x"))
        });
        assert_eq!(el.template_id.as_deref(), Some("hc-scope__hover_card"));
    }

    #[test]
    fn surface_default_has_no_fixed_width() {
        let el = hover_card_surface(Element::new().text("hi"), None);
        assert!(
            crate::layout::dim_length_px(el.style.size.width).is_none(),
            "base HoverCardContent must not bake w-64; width comes from class/opts"
        );
    }

    #[test]
    fn class_width_seeds_floating_w_attr() {
        let mut panel = with_class_list(
            hover_card_surface(Element::new().text("body"), None),
            "w-64",
        );
        seed_hover_card_floating_w(&mut panel, None);
        let w = panel
            .attrs
            .get(ATTR_FLOATING_W)
            .expect("class w-64 must seed ATTR_FLOATING_W");
        let px = w.trim_end_matches("px").parse::<f32>().unwrap_or(0.0);
        assert!(
            (px - 256.0).abs() < 0.5,
            "w-64 → 256px floating width, got {w}"
        );
    }

    #[test]
    fn zero_delay_open_closes_via_intent() {
        use crate::utils::hover_intent::{arm_close, arm_open, HoverIntent};
        let (_owner, _) = create_root(|| {
            let open = use_signal("hc-zd-open", || false);
            let th = use_signal("hc-zd-th", || false);
            let ch = use_signal("hc-zd-ch", || false);
            let od = use_signal("hc-zd-od", || 0_u64);
            let cd = use_signal("hc-zd-cd", || 0_u64);
            let intent = HoverIntent {
                open,
                trigger_hovered: th,
                content_hovered: ch,
                open_delay_ms: od,
                close_delay_ms: cd,
            };
            arm_open("hc-zd", intent);
            assert!(!open.peek());
            th.set(true);
            arm_open("hc-zd", intent);
            assert!(open.peek());
            th.set(false);
            arm_close("hc-zd", intent);
            assert!(!open.peek());
            Element::new()
        });
    }
}
