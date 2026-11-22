//! Opt-in gestures in window logical coordinates. Native editors retain their
//! own input handling; hosts must cancel active gestures when ownership ends.
use serde::{Deserialize, Serialize};
use super::keyboard::KeyModifiers;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanAxis { Both, Horizontal, Vertical }
impl PanAxis {
    /// Decide direction after the host's native pan threshold. Ties on a
    /// constrained axis yield to scrolling; nonfinite samples never activate.
    pub fn allows_direction(self, delta: [f64; 2]) -> bool {
        point(&delta) && match self {
            Self::Both => delta != [0.0; 2],
            Self::Horizontal => delta[0].abs() > delta[1].abs(),
            Self::Vertical => delta[1].abs() > delta[0].abs(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PanActivation {
    pub active_offset_x: Option<f64>,
    pub active_offset_y: Option<f64>,
    pub fail_offset_x: Option<f64>,
    pub fail_offset_y: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanDecision { Wait, Activate, Fail }
impl PanActivation {
    pub fn is_valid(&self) -> bool {
        [self.active_offset_x, self.active_offset_y, self.fail_offset_x, self.fail_offset_y]
            .into_iter().flatten().all(|v| v.is_finite() && (0.0..=1e6).contains(&v))
    }
    /// Failure takes precedence when a sample crosses both limits. Thresholds
    /// are logical distances in either direction; no active limits means 10pt.
    pub fn decide(&self, axis: PanAxis, delta: [f64; 2]) -> PanDecision {
        if !self.is_valid() || !point(&delta) { return PanDecision::Fail; }
        let past = |limit: Option<f64>, value: f64| limit.is_some_and(|limit| value.abs() > limit);
        if past(self.fail_offset_x, delta[0]) || past(self.fail_offset_y, delta[1]) { return PanDecision::Fail; }
        let active = if self.active_offset_x.is_some() || self.active_offset_y.is_some() {
            past(self.active_offset_x, delta[0]) || past(self.active_offset_y, delta[1])
        } else { delta[0].hypot(delta[1]) > 10.0 };
        if !active { PanDecision::Wait }
        else if axis.allows_direction(delta) { PanDecision::Activate }
        else { PanDecision::Fail }
    }
}

/// Pointer routing is independent of accessibility and layout bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PointerEvents { #[default] Auto, None, BoxNone }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct InteractionPolicy {
    pub pointer_events: PointerEvents,
    /// Logical points, top/right/bottom/left; ancestors still clip this target.
    pub hit_slop: [f64; 4],
    pub press: bool,
    /// Secondary pointer activation, independent from primary press and long press.
    pub context_menu: bool,
    /// Long press is independent from press; a recognized long press cancels click.
    pub long_press_millis: Option<u16>,
    pub pan: Option<PanAxis>,
    pub pan_activation: Option<PanActivation>,
    pub pinch: bool,
    pub rotation: bool,
    pub touch: bool,
    pub hover: bool,
    /// Receive bounded local file URI drops; this grants no filesystem access.
    pub file_drop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InteractionKind { Press, LongPress, ContextMenu, Pan, Pinch, Rotation, Touch, Hover }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InteractionPhase { Started, Changed, Ended, Cancelled }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TouchKind { Direct, Pencil, Indirect }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TouchPoint {
    pub id: u32,
    pub position: [f64; 2],
    pub force: f64,
    /// Pen inclination in degrees along the screen X/Y axes, each in [-90, 90].
    /// Hosts without inclination data report zero, including older payloads.
    #[serde(default)]
    pub tilt: [f64; 2],
    pub kind: TouchKind,
}

/// Per-listener contacts. Hosts assign identity once per native contact, before
/// routing it to nested listeners; this state never manufactures new identities.
#[derive(Default)]
pub struct TouchContacts(Vec<TouchPoint>);
impl TouchContacts {
    pub fn active(&self) -> &[TouchPoint] { &self.0 }
    pub fn apply(&mut self, phase: InteractionPhase, touch: TouchPoint) -> bool {
        if !contacts_valid(std::slice::from_ref(&touch)) { return false; }
        let index = self.0.iter().position(|old| old.id == touch.id);
        match (phase, index) {
            (InteractionPhase::Started, None) if self.0.len() < 10 => self.0.push(touch),
            (InteractionPhase::Changed, Some(index)) => self.0[index] = touch,
            (InteractionPhase::Ended | InteractionPhase::Cancelled, Some(index)) => { self.0.remove(index); }
            _ => return false,
        }
        true
    }
    pub fn cancel(&mut self) -> Vec<TouchPoint> { std::mem::take(&mut self.0) }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InteractionEvent {
    pub kind: InteractionKind,
    pub phase: InteractionPhase,
    pub position: [f64; 2],
    /// Receiver-local logical coordinates, after scrolling and inverse transforms.
    /// Older hosts may omit these and use root coordinates in `position`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_position: Option<[f64; 2]>,
    /// Monotonic seconds since host boot, not wall time.
    pub timestamp: f64,
    #[serde(default)] pub modifiers: KeyModifiers,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub translation: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub velocity: Option<[f64; 2]>,
    /// Pinch scale or rotation radians, with their derivative in rate.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub rate: Option<f64>,
    /// Changed contacts, including contacts that just ended or were cancelled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub touches: Vec<TouchPoint>,
    /// Contacts still down after this event. Identity is stable for a contact's
    /// lifetime, including observations by nested listeners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub active_touches: Vec<TouchPoint>,
}
fn point(value: &[f64; 2]) -> bool { value.iter().all(|v| v.is_finite() && v.abs() <= 1e6) }
impl InteractionPolicy {
    pub fn is_valid(&self) -> bool {
        self.hit_slop.iter().all(|v| v.is_finite() && (0.0..=1e6).contains(v))
            && self.long_press_millis.is_none_or(|ms| (100..=10000).contains(&ms))
            && self.pan_activation.as_ref().is_none_or(|v| self.pan.is_some() && v.is_valid())
    }
    pub fn is_empty(&self) -> bool { self == &Self::default() }
    pub fn accepts(&self, event: &InteractionEvent) -> bool {
        (self.pointer_events == PointerEvents::Auto || event.phase == InteractionPhase::Cancelled)
            && self.is_valid() && event.is_valid() && match event.kind {
            InteractionKind::Press => self.press,
            InteractionKind::LongPress => self.long_press_millis.is_some(),
            InteractionKind::ContextMenu => self.context_menu,
            InteractionKind::Pan => self.pan.is_some(),
            InteractionKind::Pinch => self.pinch,
            InteractionKind::Rotation => self.rotation,
            InteractionKind::Touch => self.touch,
            InteractionKind::Hover => self.hover,
        }
    }
}
fn contacts_valid(touches: &[TouchPoint]) -> bool {
    touches.len() <= 10 && touches.iter().enumerate().all(|(index, t)|
        t.id != 0 && point(&t.position) && t.force.is_finite() && (0.0..=1.0).contains(&t.force)
        && t.tilt.iter().all(|angle| angle.is_finite() && (-90.0..=90.0).contains(angle))
        && touches[..index].iter().all(|prior| prior.id != t.id))
}
impl InteractionEvent {
    pub fn is_valid(&self) -> bool {
        use InteractionKind::*;
        use InteractionPhase::*;
        point(&self.position) && self.local_position.as_ref().is_none_or(point)
            && self.timestamp.is_finite() && self.timestamp >= 0.0
            && self.translation.as_ref().is_none_or(point) && self.velocity.as_ref().is_none_or(point)
            && self.value.is_none_or(|v| v.is_finite() && v.abs() <= 1e6)
            && self.rate.is_none_or(|v| v.is_finite() && v.abs() <= 1e6)
            && contacts_valid(&self.touches) && contacts_valid(&self.active_touches)
            && (self.kind == Touch || (self.touches.is_empty() && self.active_touches.is_empty()))
            && (self.kind != Touch || self.touches.iter().all(|touch| {
                let active = self.active_touches.iter().any(|value| value.id == touch.id);
                if matches!(self.phase, Ended | Cancelled) { !active } else { active }
            }))
            && match self.kind {
                Press => self.phase != Changed,
                LongPress | ContextMenu => self.phase == Started,
                Pan => self.translation.is_some() && self.velocity.is_some(),
                Pinch => self.value.is_some_and(|v| v > 0.0) && self.rate.is_some(),
                Rotation => self.value.is_some() && self.rate.is_some(),
                Touch => !self.touches.is_empty(),
                Hover => true,
            }
            && (self.kind == Pan || (self.translation.is_none() && self.velocity.is_none()))
            && (matches!(self.kind, Pinch | Rotation) || (self.value.is_none() && self.rate.is_none()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contacts_preserve_shared_identity_and_reject_stale_or_over_capacity_samples() {
        let touch = |id| TouchPoint { id, position:[1.,2.], force:1., tilt:[0.;2], kind:TouchKind::Direct };
        let mut parent=TouchContacts::default(); let mut child=TouchContacts::default();
        assert!(!parent.apply(InteractionPhase::Changed,touch(1)));
        for id in 1..=10 { assert!(parent.apply(InteractionPhase::Started,touch(id))); }
        assert!(child.apply(InteractionPhase::Started,touch(2)));
        assert_eq!(parent.active()[1],child.active()[0]);
        assert!(!parent.apply(InteractionPhase::Started,touch(11)));
        assert!(!parent.apply(InteractionPhase::Started,touch(2)));
        assert!(parent.apply(InteractionPhase::Ended,touch(2)));
        assert!(parent.apply(InteractionPhase::Started,touch(11)));
        assert_eq!(parent.cancel().len(),10);
        assert!(parent.active().is_empty());
        assert!(!parent.apply(InteractionPhase::Ended,touch(11)));
    }
    #[test]
    fn constrained_pan_yields_cross_axis_and_invalid_samples() {
        assert!(PanAxis::Horizontal.allows_direction([10.0, 2.0]));
        assert!(!PanAxis::Horizontal.allows_direction([2.0, 10.0]));
        assert!(!PanAxis::Vertical.allows_direction([2.0, 2.0]));
        assert!(PanAxis::Vertical.allows_direction([2.0, -10.0]));
        assert!(!PanAxis::Both.allows_direction([0.0; 2]));
        assert!(!PanAxis::Both.allows_direction([f64::NAN, 1.0]));
        use PanDecision::*;
        let gate = PanActivation { active_offset_x: Some(12.5), fail_offset_y: Some(8.0), ..Default::default() };
        assert_eq!(gate.decide(PanAxis::Horizontal, [12.5, 0.0]), Wait);
        assert_eq!(gate.decide(PanAxis::Horizontal, [-13.0, 2.0]), Activate);
        assert_eq!(gate.decide(PanAxis::Horizontal, [14.0, 9.0]), Fail);
        assert_eq!(PanActivation::default().decide(PanAxis::Both, [6.0, 8.0]), Wait);
        assert!(!PanActivation { fail_offset_x: Some(-1.0), ..Default::default() }.is_valid());
        assert!(!InteractionPolicy { pan_activation: Some(gate), ..Default::default() }.is_valid());
    }
    #[test]
    fn gestures_are_bounded_and_opt_in() {
        let mut event = InteractionEvent { kind: InteractionKind::Pinch, phase: InteractionPhase::Changed,
            position: [1.0, 2.0], local_position: None, timestamp: 1.0, modifiers: Default::default(),
            translation: None, velocity: None, value: Some(1.5), rate: Some(0.4), touches: vec![], active_touches: vec![] };
        assert!(!InteractionPolicy::default().accepts(&event));
        let policy = InteractionPolicy { pinch: true, ..Default::default() };
        assert!(policy.accepts(&event));
        event.value = Some(0.0); assert!(!policy.accepts(&event));
        event.value = Some(f64::NAN); assert!(!event.is_valid());
        assert!(!InteractionPolicy { long_press_millis: Some(0), ..policy }.is_valid());
    }
    #[test]
    fn touch_identity_and_payload_shape_are_checked() {
        let touch = TouchPoint { id: 1, position: [0.0;2], force: 0.5, tilt: [30.0, -45.0], kind: TouchKind::Pencil };
        let mut event = InteractionEvent { kind: InteractionKind::Touch, phase: InteractionPhase::Started,
            position: [0.0;2], local_position: None, timestamp: 0.0, modifiers: Default::default(), translation: None,
            velocity: None, value: None, rate: None, touches: vec![touch.clone()], active_touches: vec![touch.clone()] };
        assert!(event.is_valid());
        event.touches[0].tilt[0] = 90.01; assert!(!event.is_valid());
        event.touches[0].tilt[0] = f64::NAN; assert!(!event.is_valid());
        event.touches[0].tilt[0] = 30.0;
        let legacy: TouchPoint = serde_json::from_str(r#"{"id":1,"position":[0,0],"force":0.5,"kind":"pencil"}"#).unwrap();
        assert_eq!(legacy.tilt, [0.0; 2]);
        event.touches.push(touch); assert!(!event.is_valid());
        event.touches.pop();
        event.phase = InteractionPhase::Ended; assert!(!event.is_valid());
        event.active_touches.clear(); assert!(event.is_valid());
        event.translation = Some([0.0;2]); assert!(!event.is_valid());
    }
}

/// OS file drops use logical window coordinates and retain URI identities only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileDropPhase { Entered, Moved, Left, Dropped }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileDropEvent {
    pub phase: FileDropPhase,
    pub position: [f64; 2],
    pub uris: Vec<String>,
}
impl FileDropEvent {
    pub fn is_valid(&self) -> bool {
        self.position.iter().all(|value| value.is_finite()) && self.uris.len() <= 64
            && (self.phase != FileDropPhase::Dropped || !self.uris.is_empty())
            && self.uris.iter().all(|uri| uri.len() <= 8192 && !uri.chars().any(char::is_control)
                && url::Url::parse(uri).is_ok_and(|url| url.scheme() == "file"
                    && url.host_str().is_none_or(|host| host == "localhost")
                    && url.username().is_empty() && url.password().is_none()
                    && url.query().is_none() && url.fragment().is_none()
                    && url.path().starts_with('/') && !url.path().contains("%00")))
    }
}

#[cfg(test)]
mod file_drop_tests {
    use super::*;
    #[test]
    fn file_drop_rejects_remote_malformed_and_unbounded_payloads() {
        let mut event = FileDropEvent { phase: FileDropPhase::Dropped, position: [1.0, 2.0], uris: vec!["file:///tmp/a%20b.txt".into()] };
        assert!(event.is_valid());
        for uri in ["https://example.com/a", "file://server/share/a", "file:///tmp/a#x", "file:///tmp/%00secret", "file:///tmp/a?x"] {
            event.uris = vec![uri.into()]; assert!(!event.is_valid(), "{uri}");
        }
        event.uris = vec!["file:///tmp/a".into(); 65]; assert!(!event.is_valid());
        event.uris.clear(); assert!(!event.is_valid());
        event.phase = FileDropPhase::Left; assert!(event.is_valid());
        event.position[0] = f64::NAN; assert!(!event.is_valid());
    }
}
