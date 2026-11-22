//! Hover surfaces share core deadlines and the retained popover contract.
use super::{popover, popover_content, use_popover, Element, Signal, PopoverContentOpts};
use jadix::portable::{hover_intent::*, core::reactivity::*, use_scoped_signal};
use jadix::presentation::{accessibility::SemanticRole, interaction::{InteractionKind, InteractionPhase}, keyboard::KeyPhase};
use jadix::portable::authoring::SlotChrome;

#[derive(Clone)]
pub struct HoverSurfaceCtx {
    pub id: String,
    pub open: Signal<bool>,
    pub trigger_fid: Signal<String>,
    pub open_delay_ms: Signal<u64>,
    pub close_delay_ms: Signal<u64>,
    trigger_hovered: Signal<bool>,
    content_hovered: Signal<bool>,
    keyboard_focused: Signal<bool>,
    dismissed: Signal<bool>,
    timer_id: String,
    tooltip: bool,
}
pub type TooltipCtx = HoverSurfaceCtx;
pub type HoverCardCtx = HoverSurfaceCtx;
thread_local! {
    static TOOLTIP: Context<HoverSurfaceCtx> = create_context();
    static HOVER_CARD: Context<HoverSurfaceCtx> = create_context();
}
pub fn use_tooltip() -> Option<TooltipCtx> { TOOLTIP.with(|ctx| use_context(*ctx)) }
pub fn use_hover_card() -> Option<HoverCardCtx> { HOVER_CARD.with(|ctx| use_context(*ctx)) }
impl HoverSurfaceCtx {
    fn dismiss(&self) {
        self.dismissed.set(true);
        self.keyboard_focused.set(false);
        self.trigger_hovered.set(false);
        self.content_hovered.set(false);
        clear(&self.timer_id);
        clear_pointer_tooltip(&self.timer_id);
        end_pointer_tooltip_grace();
        self.open.set(false);
    }
}

pub struct HoverSurfaceBuilder { id: String, open: Option<Signal<bool>>, open_delay: Option<u64>, close_delay: Option<u64>, tooltip: bool }
pub type TooltipBuilder = HoverSurfaceBuilder;
pub type HoverCardBuilder = HoverSurfaceBuilder;
pub fn tooltip(id: impl Into<String>) -> TooltipBuilder { HoverSurfaceBuilder { id: id.into(), open: None, open_delay: None, close_delay: None, tooltip: true } }
pub fn hover_card(id: impl Into<String>) -> HoverCardBuilder { HoverSurfaceBuilder { tooltip: false, ..tooltip(id) } }
impl HoverSurfaceBuilder {
    pub fn open(mut self, value: Signal<bool>) -> Self { self.open = Some(value); self }
    pub fn open_delay(mut self, value: u64) -> Self { self.open_delay = Some(value); self }
    pub fn close_delay(mut self, value: u64) -> Self { self.close_delay = Some(value); self }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        let mut builder = popover(self.id.clone());
        if let Some(open) = self.open { builder = builder.open(open); }
        builder.child(|| {
            let popover = use_popover().unwrap();
            let open = popover.open;
            let signal = |suffix: &str| use_scoped_signal(&format!("{}__hover_{suffix}", self.id), || false);
            let context = HoverSurfaceCtx {
                id: self.id.clone(), open, trigger_fid: popover.trigger_fid,
                open_delay_ms: use_scoped_signal(&format!("{}__hover_open_delay", self.id), || self.open_delay.unwrap_or(300)),
                close_delay_ms: use_scoped_signal(&format!("{}__hover_close_delay", self.id), || self.close_delay.unwrap_or(if self.tooltip { 0 } else { 100 })),
                trigger_hovered: signal("trigger"), content_hovered: signal("content"), keyboard_focused: signal("focus"), dismissed: signal("dismissed"),
                timer_id: scoped_template_id(format!("{}__hover_timer", self.id)), tooltip: self.tooltip,
            };
            let demand = signal("content_or_focus");
            let content = context.content_hovered;
            let focused = context.keyboard_focused;
            create_effect(move || set_hovered(demand, content.get() || focused.get()));
            bind_hover_intent_effect(&context.timer_id, HoverIntent { open, trigger_hovered: context.trigger_hovered, content_hovered: demand, open_delay_ms: context.open_delay_ms, close_delay_ms: context.close_delay_ms });
            let cleanup = context.timer_id.clone();
            on_cleanup(move || clear_pointer_tooltip(&cleanup));
            if self.tooltip { TOOLTIP.with(|ctx| provide_context(*ctx, context)); }
            else { HOVER_CARD.with(|ctx| provide_context(*ctx, context)); }
            body()
        })
    }
}
pub fn tooltip_scope(id: impl Into<String>, open: Option<Signal<bool>>, body: impl FnOnce() -> Element) -> Element { tooltip_scope_with_delays(id, open, None, None, body) }
pub fn tooltip_scope_with_delays(id: impl Into<String>, open: Option<Signal<bool>>, open_delay: Option<u64>, close_delay: Option<u64>, body: impl FnOnce() -> Element) -> Element {
    HoverSurfaceBuilder { open, open_delay, close_delay, ..tooltip(id) }.child(body)
}
pub fn hover_card_scope(id: impl Into<String>, open: Option<Signal<bool>>, body: impl FnOnce() -> Element) -> Element { hover_card_scope_with_delays(id, open, None, None, body) }
pub fn hover_card_scope_with_delays(id: impl Into<String>, open: Option<Signal<bool>>, open_delay: Option<u64>, close_delay: Option<u64>, body: impl FnOnce() -> Element) -> Element {
    HoverSurfaceBuilder { open, open_delay, close_delay, ..hover_card(id) }.child(body)
}

#[derive(Default)]
pub struct TooltipTriggerProps { pub id: String, pub open_delay: Option<u64>, pub close_delay: Option<u64>, pub children: Option<Element> }
pub type HoverCardTriggerProps = TooltipTriggerProps;
#[allow(non_snake_case)]
pub fn TooltipTrigger(props: TooltipTriggerProps) -> Element { trigger(use_tooltip().expect("TooltipTrigger requires Tooltip"), props) }
#[allow(non_snake_case)]
pub fn HoverCardTrigger(props: HoverCardTriggerProps) -> Element { trigger(use_hover_card().expect("HoverCardTrigger requires HoverCard"), props) }
fn trigger(ctx: HoverSurfaceCtx, props: TooltipTriggerProps) -> Element {
    if let Some(delay) = props.open_delay { ctx.open_delay_ms.set(delay); }
    if let Some(delay) = props.close_delay { ctx.close_delay_ms.set(delay); }
    let mut element = props.children.unwrap_or_else(|| Element::new().text("Hover"));
    if !props.id.is_empty() { element.template_id = Some(props.id); }
    let id = element.template_id.clone().unwrap_or_else(|| format!("{}__hover_trigger", ctx.id));
    element.template_id = Some(id.clone()); ctx.trigger_fid.set(id);
    element.descriptor.keyboard.focusable = true;
    element.descriptor.keyboard.key_down = true;
    element.descriptor.interaction.press = true;
    let previous = element.on_hover.clone(); let hover = ctx.clone();
    element = element.on_hover(move |inside| {
        if let Some(previous) = &previous { (previous.borrow_mut())(inside); }
        if inside {
            if !hover.trigger_hovered.peek() && !hover.dismissed.peek() {
                if hover.tooltip && hover.open_delay_ms.peek() != NEVER_HOVER_DELAY_MS
                    && pointer_tooltip_enter(&hover.timer_id, hover.open) { hover.open.set(true); }
                set_hovered(hover.trigger_hovered, true);
            }
        } else {
            if hover.tooltip { pointer_tooltip_leave(&hover.timer_id); }
            set_hovered(hover.trigger_hovered, false);
            hover.dismissed.set(false);
        }
    });
    let focus = ctx.clone();
    element = element.and_on_focus_change(move |focused, visible| {
        if focused && visible {
            focus.dismissed.set(false); focus.keyboard_focused.set(true); clear(&focus.timer_id); end_pointer_tooltip_grace(); focus.open.set(true);
        } else if !focused { focus.dismiss(); }
    });
    let press = ctx.clone();
    element = element.and_on_interaction(move |event| {
        if event.kind == InteractionKind::Press && event.phase == InteractionPhase::Started { press.dismiss(); }
    });
    element.and_on_key(move |event| {
        if event.phase == KeyPhase::Down {
            end_pointer_tooltip_grace();
            if matches!(event.key.as_str(), "Escape" | "Enter" | " ") { ctx.dismiss(); }
        }
    })
}

pub struct TooltipContentOpts { pub side: String, pub panel: SlotChrome }
impl Default for TooltipContentOpts { fn default() -> Self { Self { side: "bottom".into(), panel: Default::default() } } }
pub struct HoverCardContentOpts { pub side: String, pub align: String, pub side_offset: f32, pub panel: SlotChrome, pub width: Option<String> }
impl Default for HoverCardContentOpts { fn default() -> Self { Self { side: "bottom".into(), align: "center".into(), side_offset: 4.0, panel: Default::default(), width: None } } }
pub fn tooltip_content(opts: TooltipContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    content(use_tooltip().expect("TooltipContent requires Tooltip"), PopoverContentOpts { side: opts.side, panel: opts.panel, ..Default::default() }, body)
}
pub fn hover_card_content(opts: HoverCardContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    content(use_hover_card().expect("HoverCardContent requires HoverCard"), PopoverContentOpts { side: opts.side, align: opts.align, side_offset: opts.side_offset, panel: opts.panel, width: opts.width, ..Default::default() }, body)
}
fn content(ctx: HoverSurfaceCtx, opts: PopoverContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let hover = ctx.clone();
    let mut element = popover_content(opts, move || {
        let hover = hover.clone();
        Element::new().id(format!("{}__hover_surface", hover.id)).child(body())
            .on_hover(move |inside| set_hovered(hover.content_hovered, inside && !hover.dismissed.peek()))
    });
    // External open signals and the hover owner are the single source of truth.
    element.popover.as_mut().unwrap().open = ctx.open;
    element.descriptor.style.popover_focus_on_open = Some(false);
    element.descriptor.accessibility.role = Some(if ctx.tooltip { SemanticRole::Text } else { SemanticRole::Region });
    element.on_popover_dismiss(move |_| ctx.dismiss())
}
