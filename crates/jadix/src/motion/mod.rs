//! Retained interaction variants, layout projection and vector animation.
//!
//! Keep [`MotionState`] and [`LayoutMotion`] between builds, or use [`use_motion`]
//! and [`use_layout_motion`]. Bindings sample the existing window animation clock
//! without subscribing the root UI effect.

#![doc = include_str!("../../MOTION.md")]

mod layout;
mod path;
mod target;

pub(crate) use layout::LayoutRegistry;
pub use layout::{LayoutAnimation, LayoutMotion, use_layout_motion};
pub use path::{
    PathMorph, PathMorphError, PathSample, PreparedPath, bind_path_follow, bind_path_morph,
    bind_path_reveal,
};
pub use target::MotionTarget;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::style::{AnimatableValue, AnimatedNumber, PropertyId, ValueDriver};

/// Partial variants compose in this order: animate, focus, hover, tap, drag.
#[derive(Clone, Debug)]
pub struct Motion {
    target: MotionTarget,
    initial: MotionTarget,
    hover: MotionTarget,
    tap: MotionTarget,
    focus: MotionTarget,
    drag: MotionTarget,
    driver: ValueDriver,
    delay_ms: f32,
    disabled: bool,
}

impl Motion {
    pub fn new(target: MotionTarget) -> Self {
        Self {
            target,
            initial: MotionTarget::default(),
            hover: MotionTarget::default(),
            tap: MotionTarget::default(),
            focus: MotionTarget::default(),
            drag: MotionTarget::default(),
            driver: ValueDriver::spring(),
            delay_ms: 0.0,
            disabled: false,
        }
    }

    pub fn initial(mut self, target: MotionTarget) -> Self {
        self.initial = target;
        self
    }

    pub fn while_hover(mut self, target: MotionTarget) -> Self {
        self.hover = target;
        self
    }

    pub fn while_tap(mut self, target: MotionTarget) -> Self {
        self.tap = target;
        self
    }

    pub fn while_focus(mut self, target: MotionTarget) -> Self {
        self.focus = target;
        self
    }

    /// Controlled by [`MotionState::set_dragging`]; reuses the application's drag engine.
    pub fn while_drag(mut self, target: MotionTarget) -> Self {
        self.drag = target;
        self
    }

    pub fn transition(mut self, driver: ValueDriver) -> Self {
        assert!(
            !matches!(driver, ValueDriver::Inertia(_)),
            "element targets require a spring or tween"
        );
        self.driver = driver;
        self
    }

    /// Initial delay, in milliseconds. Interruptions do not repeat this delay.
    pub fn delay(mut self, milliseconds: f32) -> Self {
        assert!(milliseconds.is_finite(), "motion delay must be finite");
        self.delay_ms = milliseconds.max(0.0);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Default)]
struct MotionInner {
    motion: Option<Motion>,
    tracks: BTreeMap<PropertyId, Track>,
    hovered: bool,
    tapped: bool,
    keyboard_tapped: bool,
    dragging: bool,
}

/// One element's retained presentation. Clones share ownership.
#[derive(Clone, Default)]
pub struct MotionState(Rc<RefCell<MotionInner>>);

impl MotionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_target(&self, target: MotionTarget) {
        if let Some(motion) = &mut self.0.borrow_mut().motion {
            motion.target = target;
        }
        crate::app::invalidation::request_frame();
    }

    pub fn set_dragging(&self, dragging: bool) {
        self.0.borrow_mut().dragging = dragging;
        crate::app::invalidation::request_frame();
    }

    pub fn is_active(&self) -> bool {
        self.0.borrow().tracks.values().any(Track::is_active)
    }

    pub fn pause(&self) {
        for track in self.0.borrow().tracks.values() {
            for channel in &track.channels {
                channel.pause();
            }
        }
    }

    pub fn play(&self) {
        for track in self.0.borrow().tracks.values() {
            for channel in &track.channels {
                channel.play();
            }
        }
        crate::app::invalidation::request_frame();
    }

    pub fn stop(&self) {
        for track in self.0.borrow().tracks.values() {
            for channel in &track.channels {
                channel.stop();
            }
        }
    }

    pub fn value(&self, property: PropertyId) -> Option<AnimatableValue> {
        self.0.borrow().tracks.get(&property).map(Track::value)
    }

    fn interact(&self, update: impl FnOnce(&mut MotionInner)) {
        update(&mut self.0.borrow_mut());
        crate::app::invalidation::request_frame();
    }

    fn configure(&self, element: &Element, motion: Motion) {
        let mut state = self.0.borrow_mut();
        let first_mount = state.motion.is_none();
        for target in [
            &motion.target,
            &motion.initial,
            &motion.hover,
            &motion.tap,
            &motion.focus,
            &motion.drag,
        ] {
            for &property in target.values.keys() {
                state.tracks.entry(property).or_insert_with(|| {
                    let baseline = read_baseline(element, property);
                    let initial = if first_mount {
                        motion
                            .initial
                            .values
                            .get(&property)
                            .cloned()
                            .or_else(|| motion.target.values.get(&property).cloned())
                            .unwrap_or_else(|| baseline.clone())
                    } else {
                        baseline.clone()
                    };
                    Track::new(baseline, initial)
                });
            }
        }
        state.motion = Some(motion);
    }

    fn sync(&self, element: &mut Element) -> LayoutStatus {
        let mut state = self.0.borrow_mut();
        let Some(motion) = state.motion.clone() else {
            return LayoutStatus::Clean;
        };
        let disabled = motion.disabled
            || element.attrs.contains_key("disabled")
            || element
                .attrs
                .get("aria-disabled")
                .is_some_and(|value| value == "true")
            || element.attrs.contains_key("data-disabled");
        if disabled {
            state.hovered = false;
            state.tapped = false;
            state.keyboard_tapped = false;
            state.dragging = false;
        }
        let focused = !disabled
            && element.focus_id.as_deref().is_some_and(|id| {
                use_signal("__jadix_button_focus_id", String::new)
                    .try_peek()
                    .as_deref()
                    == Some(id)
                    && use_signal("__jadix_focus_visible", || false)
                        .try_peek()
                        .unwrap_or(false)
            });
        let mut target = motion.target.clone();
        if !focused {
            state.keyboard_tapped = false;
        }
        for (active, variant) in [
            (focused, &motion.focus),
            (state.hovered && !disabled, &motion.hover),
            (
                (state.tapped || state.keyboard_tapped) && !disabled,
                &motion.tap,
            ),
            (state.dragging && !disabled, &motion.drag),
        ] {
            if active {
                target = target.overlay(variant);
            }
        }
        let reduced = crate::style::prefers_reduced_motion();
        for &property in target.values.keys() {
            state.tracks.entry(property).or_insert_with(|| {
                let baseline = read_baseline(element, property);
                let mut track = Track::new(baseline.clone(), baseline);
                track.started = true;
                track
            });
        }
        let mut status = LayoutStatus::Clean;
        for (&property, track) in &mut state.tracks {
            let destination = target
                .values
                .get(&property)
                .unwrap_or(&track.baseline)
                .clone();
            track.retarget(destination, &motion.driver, motion.delay_ms, reduced);
            let value = track.value();
            let current = read_baseline(element, property);
            if !crate::style::animatable_values_close(&current, &value) {
                let inner = &mut element.inner;
                crate::style::write_animatable_value(
                    &mut inner.style,
                    &mut inner.text_style,
                    property,
                    &value,
                );
                status = status.merge(
                    if property.layout_impact() == crate::style::LayoutImpact::Layout {
                        LayoutStatus::LayoutChanged
                    } else {
                        LayoutStatus::StyleChanged
                    },
                );
            }
        }
        status
    }
}

pub fn use_motion(id: &str) -> MotionState {
    use_signal(id, MotionState::new).peek()
}

impl Element {
    /// Bind retained motion without replacing existing event or frame callbacks.
    pub fn motion(mut self, state: &MotionState, motion: Motion) -> Self {
        state.configure(&self, motion);
        state.sync(&mut self);
        for track in state.0.borrow_mut().tracks.values_mut() {
            track.started = true;
        }
        let previous = self.on_hover.take();
        let hovered = state.clone();
        self = self.on_hover(move |active| {
            hovered.interact(|state| {
                state.hovered = active;
                if !active {
                    state.tapped = false;
                }
            });
            if let Some(handler) = &previous {
                (handler.borrow_mut())(active);
            }
        });
        let previous = self.on_mousedown.take();
        let tapped = state.clone();
        self = self.on_mousedown(move |event| {
            if event.button == winit::event::MouseButton::Left {
                tapped.interact(|state| state.tapped = true);
            }
            if let Some(handler) = &previous {
                (handler.borrow_mut())(event);
            }
        });
        let previous = self.on_mouseup.take();
        let released = state.clone();
        self = self.on_mouseup(move |event| {
            if event.button == winit::event::MouseButton::Left {
                released.interact(|state| state.tapped = false);
            }
            if let Some(handler) = &previous {
                (handler.borrow_mut())(event);
            }
        });
        let previous = self.on_mousemove.take();
        let moved = state.clone();
        self = self.on_mousemove(move |event| {
            if event.buttons & 1 == 0 && moved.0.borrow().tapped {
                moved.interact(|state| state.tapped = false);
            }
            if let Some(handler) = &previous {
                (handler.borrow_mut())(event);
            }
        });
        let previous = self.on_key_down.take();
        let keyboard = state.clone();
        let keyboard_enabled = self.focusable && self.on_click.is_some();
        self = self.on_key_down(move |event| {
            if keyboard_enabled && crate::utils::keyboard::is_enter_or_space(&event.logical_key) {
                keyboard.interact(|state| {
                    state.keyboard_tapped = event.state == winit::event::ElementState::Pressed
                });
            }
            previous
                .as_ref()
                .is_some_and(|handler| (handler.borrow_mut())(event))
        });
        let previous = self.reactive_sync.take();
        let state = state.clone();
        self.reactive_sync(move |element, tree| {
            let status = previous
                .as_ref()
                .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree));
            status.merge(state.sync(element))
        })
    }
}

struct Track {
    baseline: AnimatableValue,
    target: AnimatableValue,
    channels: Vec<AnimatedNumber>,
    started: bool,
}

impl Track {
    fn new(baseline: AnimatableValue, initial: AnimatableValue) -> Self {
        let channels = components(&initial)
            .into_iter()
            .map(AnimatedNumber::follow)
            .collect();
        Self {
            baseline,
            target: initial,
            channels,
            started: false,
        }
    }

    fn value(&self) -> AnimatableValue {
        let values: Vec<f32> = self.channels.iter().map(AnimatedNumber::peek).collect();
        match &self.target {
            AnimatableValue::Scalar(_) if values.len() == 1 => AnimatableValue::Scalar(values[0]),
            AnimatableValue::Vec2(_) if values.len() == 2 => {
                AnimatableValue::Vec2([values[0], values[1]])
            }
            AnimatableValue::Vec4(_) if values.len() == 4 => {
                AnimatableValue::Vec4(values.try_into().unwrap())
            }
            AnimatableValue::Color(_) if values.len() == 4 => {
                let rgba: [f32; 4] = values.try_into().unwrap();
                AnimatableValue::Color(rgba.map(|value| value.clamp(0.0, 1.0)))
            }
            _ => self.target.clone(),
        }
    }

    fn retarget(
        &mut self,
        target: AnimatableValue,
        driver: &ValueDriver,
        delay: f32,
        reduced: bool,
    ) {
        if crate::style::animatable_values_close(&target, &self.target) {
            if reduced {
                for (channel, value) in self.channels.iter().zip(components(&target)) {
                    if channel.is_active() || channel.peek() != value {
                        channel.jump(value);
                    }
                }
            }
            return;
        }
        let destination = components(&target);
        if destination.len() != self.channels.len() {
            self.channels = destination
                .iter()
                .copied()
                .map(AnimatedNumber::follow)
                .collect();
        } else {
            for (channel, &value) in self.channels.iter().zip(&destination) {
                if reduced {
                    channel.jump(value);
                } else {
                    channel.restart(value, driver.clone());
                }
            }
        }
        if !self.started && !reduced && delay > 0.0 {
            for channel in &self.channels {
                let _ = channel.clone().delay(delay);
            }
        }
        self.started = true;
        self.target = target;
    }

    fn is_active(&self) -> bool {
        self.channels.iter().any(AnimatedNumber::is_active)
    }
}

fn components(value: &AnimatableValue) -> Vec<f32> {
    match value {
        AnimatableValue::Scalar(value) => vec![*value],
        AnimatableValue::Vec2(value) => value.to_vec(),
        AnimatableValue::Vec4(value) | AnimatableValue::Color(value) => value.to_vec(),
        _ => Vec::new(),
    }
}

fn read_baseline(element: &Element, property: PropertyId) -> AnimatableValue {
    use crate::layout::dimension::IntoCssDim;
    let value = crate::style::read_animatable_value(&element.style, &element.text_style, property);
    let (slot, dimension) = match property {
        PropertyId::Width => (crate::style::UnitSlot::Width, element.style.size.width),
        PropertyId::Height => (crate::style::UnitSlot::Height, element.style.size.height),
        _ => return value,
    };
    if let Some((_, dimension)) = element
        .style
        .unit_specs
        .iter()
        .find(|(key, _)| *key == slot)
    {
        return AnimatableValue::Dim(dimension.clone());
    }
    if matches!(value, AnimatableValue::Auto) {
        AnimatableValue::Dim(dimension.into_css_dim())
    } else {
        value
    }
}

pub(crate) fn presentation_translate_in_paint_space(element: &Element, ancestor_scale: [f32; 2]) -> [f32; 2] {
    let translate = presentation_translate(element);
    std::array::from_fn(|axis| translate[axis]
        + element.style.translate[axis] * (ancestor_scale[axis] - 1.0))
}

pub(crate) fn presentation_translate(element: &Element) -> [f32; 2] {
    let Some(layout) = &element.layout_motion else {
        return element.style.translate;
    };
    let origin = crate::style::origin_px(
        &element.style.transform_origin,
        element.layout_bounds[2],
        element.layout_bounds[3],
    );
    let (translate, _) = layout.projection(origin);
    std::array::from_fn(|axis| element.style.translate[axis] + translate[axis])
}

pub(crate) fn presentation_scale(element: &Element) -> [f32; 2] {
    let Some(layout) = &element.layout_motion else {
        return element.style.scale;
    };
    let (_, scale) = layout.projection([0.0; 2]);
    std::array::from_fn(|axis| element.style.scale[axis] * scale[axis])
}

#[cfg(test)]
mod tests;
