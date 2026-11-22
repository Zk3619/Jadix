//! Declarative visual motion; durations and delays use milliseconds.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewSpring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}
impl Default for ViewSpring {
    fn default() -> Self {
        Self {
            stiffness: 100.,
            damping: 10.,
            mass: 1.,
        }
    }
}
impl ViewSpring {
    pub fn is_valid(&self) -> bool {
        [self.stiffness, self.damping, self.mass]
            .into_iter()
            .all(|x| x.is_finite() && (0.001..=1_000_000.).contains(&x))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewTransition {
    /// Bitmask of COLORS, OPACITY, TRANSFORM, SHADOW and SHAPE.
    pub properties: u8,
    pub duration: f32,
    pub delay: f32,
    /// Cubic Bezier control points; x coordinates must be within 0..=1.
    pub easing: [f32; 4],
    /// Physics replaces duration. Interrupted transitions restart from the visible values at rest.
    pub spring: Option<ViewSpring>,
}
impl ViewTransition {
    pub const COLORS: u8 = 1;
    pub const OPACITY: u8 = 2;
    pub const TRANSFORM: u8 = 4;
    pub const SHADOW: u8 = 8;
    pub const SHAPE: u8 = 16;
    pub const ALL: u8 = 31;
    pub fn is_valid(&self) -> bool {
        self.properties & !Self::ALL == 0
            && time(self.duration)
            && time(self.delay)
            && easing(self.easing)
            && self.spring.as_ref().is_none_or(ViewSpring::is_valid)
    }
}
impl Default for ViewTransition {
    fn default() -> Self {
        Self {
            properties: Self::ALL,
            duration: 0.,
            delay: 0.,
            easing: [0.25, 0.1, 0.25, 1.],
            spring: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKeyframes {
    #[default]
    Spin,
    Ping,
    Pulse,
    Bounce,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewAnimation {
    pub keyframes: ViewKeyframes,
    pub duration: f32,
    pub delay: f32,
    pub easing: [f32; 4],
    /// None repeats indefinitely; zero iterations disables playback.
    pub iterations: Option<u32>,
    /// Maximum frame rate; omitted uses the host's animation cadence.
    pub frame_rate: Option<f32>,
}
impl Default for ViewAnimation {
    fn default() -> Self {
        Self {
            keyframes: ViewKeyframes::Spin,
            duration: 1000.,
            delay: 0.,
            easing: [0., 0., 1., 1.],
            iterations: None,
            frame_rate: None,
        }
    }
}
impl ViewAnimation {
    pub fn is_valid(&self) -> bool {
        time(self.duration)
            && self.duration > 0.
            && time(self.delay)
            && easing(self.easing)
            && self
                .frame_rate
                .is_none_or(|x| x.is_finite() && (1.0..=240.).contains(&x))
    }
}
fn time(x: f32) -> bool {
    x.is_finite() && (0.0..=86_400_000.).contains(&x)
}
fn easing(v: [f32; 4]) -> bool {
    v.into_iter().all(|x| x.is_finite() && x.abs() <= 16.)
        && (0.0..=1.).contains(&v[0])
        && (0.0..=1.).contains(&v[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_contract_rejects_unbounded_or_nonsettling_values() {
        let mut style = super::super::ViewAppearance {
            transition: Some(ViewTransition {
                duration: 250.,
                spring: Some(ViewSpring::default()),
                ..Default::default()
            }),
            animation: Some(ViewAnimation {
                keyframes: ViewKeyframes::Bounce,
                iterations: Some(3),
                frame_rate: Some(30.),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(style.is_valid());
        let value = serde_json::to_value(&style).unwrap();
        assert_eq!(value["transition"]["properties"], 31);
        assert_eq!(value["animation"]["keyframes"], "bounce");
        assert_eq!(
            serde_json::from_value::<super::super::ViewAppearance>(value).unwrap(),
            style
        );
        style.transition.as_mut().unwrap().properties = 32;
        assert!(!style.is_valid());
        style.transition.as_mut().unwrap().properties = ViewTransition::ALL;
        style
            .transition
            .as_mut()
            .unwrap()
            .spring
            .as_mut()
            .unwrap()
            .damping = 0.;
        assert!(!style.is_valid());
        style.transition = None;
        style.animation.as_mut().unwrap().duration = f32::INFINITY;
        assert!(!style.is_valid());
        style.animation.as_mut().unwrap().duration = 1000.;
        style.animation.as_mut().unwrap().easing[2] = -0.1;
        assert!(!style.is_valid());
        style.animation.as_mut().unwrap().easing[2] = 1.;
        style.animation.as_mut().unwrap().frame_rate = Some(f32::NAN);
        assert!(!style.is_valid());
        assert!(serde_json::from_str::<ViewTransition>(r#"{"duraton":200}"#).is_err());
    }
}
