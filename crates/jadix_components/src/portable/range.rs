//! Range controls share arithmetic and semantic input across native hosts.
use std::{cell::Cell, rc::Rc};
use jadix::portable::{Element, Signal, range::Range};
use jadix::presentation::{Rect, accessibility::{AccessibleAction, AccessibleValue, SemanticRole},
    interaction::{InteractionKind, InteractionPhase, InteractionPolicy, PanAxis},
    keyboard::{KeyPhase, KeyboardPolicy}};

pub struct SliderBuilder { id: String, value: Signal<f64>, range: Range, vertical: bool, disabled: bool }
pub fn slider(id: impl Into<String>, value: Signal<f64>) -> SliderBuilder {
    SliderBuilder { id: id.into(), value, range: Range::new(0., 1., 0.01).unwrap(), vertical: false, disabled: false }
}
impl SliderBuilder {
    pub fn range(mut self, min: f64, max: f64, step: f64) -> Result<Self, &'static str> { self.range = Range::new(min, max, step)?; Ok(self) }
    pub fn vertical(mut self, vertical: bool) -> Self { self.vertical = vertical; self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn build(self) -> Element {
        let Self { id, value, range, vertical, disabled } = self;
        let bounds = Rc::new(Cell::new(Rect::default())); let layout = bounds.clone();
        let mut root = Element::new().id(&id).on_layout(move |frame| { layout.set(frame); false });
        if vertical { root = root.w(32.).h(160.).flex_col(); } else { root = root.w(160.).h(32.).flex_row(); }
        root.descriptor.accessible_name = Some(id.clone());
        root.descriptor.accessibility.role = Some(SemanticRole::Adjustable);
        root.descriptor.accessibility.disabled = disabled;
        root.descriptor.keyboard = KeyboardPolicy { focusable: !disabled, key_down: !disabled, ..Default::default() };
        if !disabled {
            root.descriptor.interaction = InteractionPolicy { press: true, pan: Some(if vertical { PanAxis::Vertical } else { PanAxis::Horizontal }), ..Default::default() };
            root.descriptor.accessibility.actions = ["increment", "decrement", "setValue"].into_iter()
                .map(|id| AccessibleAction { id: id.into(), label: id.into() }).collect();
        }
        root.descriptor_binding = Some(Rc::new(move |descriptor| {
            descriptor.accessibility.value = Some(AccessibleValue { min: range.min(), max: range.max(), now: range.clamp(value.get()).unwrap_or(range.min()), text: None });
        }));
        // The two flex segments keep geometry independent from the numeric range.
        // Vertical controls grow upward; horizontal controls grow to the right.
        for filled in if vertical { [false, true] } else { [true, false] } {
            let mut segment = Element::new().bg(if filled { [44, 108, 224, 255] } else { [145, 150, 160, 255] });
            segment.descriptor_binding = Some(Rc::new(move |descriptor| {
                let fraction = range.fraction(value.get()).unwrap_or(0.) as f32;
                descriptor.style.grow = if filled { fraction } else { 1. - fraction };
                if vertical { descriptor.style.height = Some(0.); } else { descriptor.style.width = Some(0.); }
            }));
            root.children.push(segment);
        }
        root.on_interaction(move |event| {
            if disabled || event.phase == InteractionPhase::Cancelled { return; }
            // Press-up is deliberately ignored: a pan may have cancelled it.
            if !((event.kind == InteractionKind::Press && event.phase == InteractionPhase::Started)
                || event.kind == InteractionKind::Pan) { return; }
            let frame = bounds.get();
            let extent = if vertical { frame.height } else { frame.width };
            if extent <= 0. { return; }
            let local = event.local_position.unwrap_or([event.position[0] - frame.x, event.position[1] - frame.y]);
            let fraction = if vertical { 1. - local[1] / extent } else { local[0] / extent };
            if let Some(next) = range.at_fraction(fraction) { value.set(next); }
        }).on_key(move |event| {
            if disabled || event.phase != KeyPhase::Down || event.modifiers.alt || event.modifiers.control || event.modifiers.meta { return; }
            let next = match event.key.as_str() {
                "Home" => Some(range.min()), "End" => Some(range.max()),
                "ArrowRight" | "ArrowUp" => range.advance(value.peek(), 1.),
                "ArrowLeft" | "ArrowDown" => range.advance(value.peek(), -1.),
                "PageUp" => range.advance(value.peek(), 10.), "PageDown" => range.advance(value.peek(), -10.), _ => None,
            };
            if let Some(next) = next { value.set(next); }
        }).on_accessibility_action(move |action, next| {
            if disabled { return; }
            let next = match action {
                "increment" => range.advance(value.peek(), 1.), "decrement" => range.advance(value.peek(), -1.),
                "setValue" => next.and_then(|next| range.snap(next)), _ => None,
            };
            if let Some(next) = next { value.set(next); }
        })
    }
}
impl From<SliderBuilder> for Element { fn from(builder: SliderBuilder) -> Self { builder.build() } }

pub struct ProgressBuilder { id: String, value: Signal<f64>, range: Range }
pub fn progress(id: impl Into<String>, value: Signal<f64>) -> ProgressBuilder {
    ProgressBuilder { id: id.into(), value, range: Range::new(0., 1., 0.).unwrap() }
}
impl ProgressBuilder {
    pub fn range(mut self, min: f64, max: f64) -> Result<Self, &'static str> { self.range = Range::new(min, max, 0.)?; Ok(self) }
    pub fn build(self) -> Element {
        let mut root = slider(self.id, self.value).range(self.range.min(), self.range.max(), 0.).unwrap().disabled(true).build();
        root.descriptor.style.height = Some(8.);
        root.descriptor.accessibility.role = Some(SemanticRole::Progressbar);
        root.descriptor.accessibility.disabled = false;
        root.descriptor.accessibility.read_only = true;
        root
    }
}
impl From<ProgressBuilder> for Element { fn from(builder: ProgressBuilder) -> Self { builder.build() } }
