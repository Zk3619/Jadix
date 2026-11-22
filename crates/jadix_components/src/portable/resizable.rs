//! Retained split panels. Content stays mounted while only flex sizes change.
use jadix::portable::{Element, Signal, resizable::resize_pair};
use jadix::presentation::{Rect, accessibility::{AccessibleAction, AccessibleValue, SemanticRole},
    interaction::{InteractionKind, InteractionPhase, InteractionPolicy, PanAxis},
    keyboard::{KeyPhase, KeyboardPolicy}};
use std::{cell::{Cell, RefCell}, rc::Rc};

pub struct ResizableBuilder {
    id: String,
    sizes: Signal<Vec<f64>>,
    limits: Option<Vec<(f64, f64)>>,
    vertical: bool,
    rtl: Option<bool>,
    on_resize: Option<Rc<dyn Fn(Vec<f64>)>>,
    on_resize_phase: Option<Rc<dyn Fn(InteractionPhase)>>,
}
pub fn resizable(id: impl Into<String>, sizes: Signal<Vec<f64>>) -> ResizableBuilder {
    ResizableBuilder { id: id.into(), sizes, limits: None, vertical: false, rtl: None, on_resize: None, on_resize_phase: None }
}
impl ResizableBuilder {
    pub fn vertical(mut self, vertical: bool) -> Self { self.vertical = vertical; self }
    /// Override the active locale's inline direction for this panel group.
    pub fn rtl(mut self, rtl: bool) -> Self { self.rtl = Some(rtl); self }
    pub fn limits(mut self, limits: Vec<(f64, f64)>) -> Self { self.limits = Some(limits); self }
    pub fn on_resize(mut self, callback: impl Fn(Vec<f64>) + 'static) -> Self {
        self.on_resize = Some(Rc::new(callback)); self
    }
    pub fn on_resize_phase(mut self, callback: impl Fn(InteractionPhase) + 'static) -> Self {
        self.on_resize_phase = Some(Rc::new(callback)); self
    }
    /// Sizes and limits use the same units; all panels together retain their sum.
    pub fn children(self, children: Vec<Element>) -> Result<Element, &'static str> {
        let Self { id, sizes, limits, vertical, rtl, on_resize, on_resize_phase } = self;
        let initial = sizes.peek();
        let total: f64 = initial.iter().sum();
        let limits = limits.unwrap_or_else(|| vec![(0., total); children.len()]);
        if children.len() < 2 || children.len() > 64 || initial.len() != children.len()
            || !total.is_finite() || total <= 0. || total > 1e6 || limits.len() != children.len()
            || initial.iter().zip(&limits).any(|(size, (min, max))| !size.is_finite() || !min.is_finite()
                || !max.is_finite() || *min < 0. || min > max || size < min || size > max) {
            return Err("invalid split panel sizes or constraints");
        }
        let limits = Rc::new(limits);
        let bounds = Rc::new(Cell::new(Rect::default())); let measured = bounds.clone();
        let mut root = Element::new().id(&id).on_layout(move |frame| { measured.set(frame); false });
        root = if vertical { root.flex_col() } else { root.flex_row() };
        let reversed = Rc::new(Cell::new(false));
        let direction = reversed.clone();
        root.descriptor_binding = Some(Rc::new(move |descriptor| {
            let rtl = rtl.unwrap_or_else(jadix::chrome_rtl);
            direction.set(!vertical && rtl);
            descriptor.style.direction = Some(if rtl { jadix::environment::LayoutDirection::RightToLeft }
                else { jadix::environment::LayoutDirection::LeftToRight });
        }));
        let count = children.len();
        for (index, child) in children.into_iter().enumerate() {
            let mut panel = Element::new().id(format!("{id}__panel_{index}")).child(child).flex_shrink(1.);
            panel.descriptor_binding = Some(Rc::new(move |descriptor| {
                let current = sizes.get();
                descriptor.style.grow = current.get(index).copied().filter(|size| size.is_finite() && *size >= 0.)
                    .unwrap_or(0.).min(1e6) as f32;
                if vertical { descriptor.style.height = Some(0.); } else { descriptor.style.width = Some(0.); }
            }));
            panel.descriptor.style.min_width = 0.; panel.descriptor.style.min_height = 0.;
            root.children.push(panel);
            if index + 1 == count { continue; }
            let mut handle = Element::new().id(format!("{id}__handle_{index}")).bg([145, 150, 160, 255]).flex_shrink(0.);
            handle = if vertical { handle.h(12.) } else { handle.w(12.) };
            handle.descriptor.keyboard = KeyboardPolicy { focusable: true, key_down: true, ..Default::default() };
            handle.descriptor.accessible_name = Some(format!("Resize panel {}", index + 1));
            handle.descriptor.accessibility.role = Some(SemanticRole::Adjustable);
            handle.descriptor.interaction = InteractionPolicy { pan: Some(if vertical { PanAxis::Vertical } else { PanAxis::Horizontal }), ..Default::default() };
            let semantics = limits.clone();
            handle.descriptor_binding = Some(Rc::new(move |descriptor| {
                let current = sizes.get();
                descriptor.keyboard.focusable = false;
                descriptor.accessibility.actions.clear();
                descriptor.accessibility.value = None;
                if let (Some(&first), Some(&second)) = (current.get(index), current.get(index + 1)) {
                    let lower = semantics[index].0.max(first + second - semantics[index + 1].1);
                    let upper = semantics[index].1.min(first + second - semantics[index + 1].0);
                    if lower.is_finite() && upper.is_finite() && lower < upper && first.is_finite() {
                        descriptor.keyboard.focusable = true;
                        descriptor.accessibility.value = Some(AccessibleValue { min: lower, max: upper, now: first.clamp(lower, upper), text: None });
                        descriptor.accessibility.actions = ["increment", "decrement", "setValue"].into_iter()
                            .map(|id| AccessibleAction { id: id.into(), label: id.into() }).collect();
                    }
                }
            }));
            let update = {
                let limits = limits.clone();
                let on_resize = on_resize.clone();
                Rc::new(move |start: &[f64], delta: f64| {
                    if start.len() != count || !delta.is_finite() { return; }
                    if let Some(pair) = resize_pair([start[index], start[index + 1]], delta,
                        [[limits[index].0, limits[index].1], [limits[index + 1].0, limits[index + 1].1]]) {
                        let mut next = start.to_vec(); next[index] = pair[0]; next[index + 1] = pair[1];
                        if sizes.peek() != next {
                            sizes.set(next.clone());
                            if let Some(callback) = &on_resize { callback(next); }
                        }
                    }
                })
            };
            let drag = Rc::new(RefCell::new(None::<Vec<f64>>));
            let cleanup_drag = drag.clone(); let cleanup_phase = on_resize_phase.clone();
            let cleanup_resize = on_resize.clone();
            jadix::portable::on_cleanup(move || {
                let start = cleanup_drag.borrow_mut().take();
                if let Some(start) = start {
                    if let Some(callback) = &cleanup_phase { callback(InteractionPhase::Cancelled); }
                    if sizes.try_get().is_some_and(|current| current != start) && sizes.try_set(start.clone()) {
                        if let Some(callback) = &cleanup_resize { callback(start); }
                    }
                }
            });
            let resize = update.clone(); let frame = bounds.clone();
            let on_resize = on_resize.clone();
            let phase = on_resize_phase.clone();
            let drag_reversed = reversed.clone();
            handle = handle.on_interaction(move |event| {
                if event.kind != InteractionKind::Pan { return; }
                match event.phase {
                    InteractionPhase::Started => {
                        if drag.borrow().is_some() { return; }
                        *drag.borrow_mut() = Some(sizes.peek());
                        if let Some(callback) = &phase { callback(InteractionPhase::Started); }
                    }
                    InteractionPhase::Cancelled => {
                        let start = drag.borrow_mut().take();
                        if let Some(start) = start {
                            if let Some(callback) = &phase { callback(InteractionPhase::Cancelled); }
                            if sizes.peek() != start {
                                sizes.set(start.clone());
                                if let Some(callback) = &on_resize { callback(start); }
                            }
                        }
                        return;
                    }
                    _ => {}
                }
                let extent = if vertical { frame.get().height } else { frame.get().width } - 12. * (count - 1) as f64;
                if extent > 0. {
                    let start = drag.borrow().clone();
                    if let (Some(start), Some(delta)) = (start, event.translation) {
                        let sign = if drag_reversed.get() { -1. } else { 1. };
                        resize(&start, sign * delta[usize::from(vertical)] / extent * start.iter().sum::<f64>());
                    }
                }
                if event.phase == InteractionPhase::Ended {
                    if drag.borrow_mut().take().is_some() {
                        if let Some(callback) = &phase { callback(InteractionPhase::Ended); }
                    }
                }
            });
            let keyboard = update.clone();
            let key_reversed = reversed.clone();
            handle = handle.on_key(move |event| {
                if event.phase != KeyPhase::Down || event.modifiers.alt || event.modifiers.control || event.modifiers.meta { return; }
                let delta = match event.key.as_str() {
                    "ArrowRight" if !vertical => if key_reversed.get() { -1. } else { 1. },
                    "ArrowLeft" if !vertical => if key_reversed.get() { 1. } else { -1. },
                    "ArrowDown" if vertical => 1., "ArrowUp" if vertical => -1.,
                    "Home" => -1e6, "End" => 1e6, _ => return,
                };
                let current = sizes.peek(); keyboard(&current, delta * total / 100.);
            });
            handle = handle.on_accessibility_action(move |action, value| {
                let current = sizes.peek();
                let Some(&first) = current.get(index) else { return; };
                let delta = match action { "increment" => total / 100., "decrement" => -total / 100.,
                    "setValue" => match value { Some(value) => value - first, None => return }, _ => return };
                update(&current, delta);
            });
            root.children.push(handle);
        }
        Ok(root)
    }
}
