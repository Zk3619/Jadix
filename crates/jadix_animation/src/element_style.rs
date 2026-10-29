/// CSS `steps()` jump term (`jump-start` / `jump-end` / `jump-none` / `jump-both`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepJumpTerm {
    JumpStart,
    JumpEnd,
    JumpNone,
    JumpBoth,
}

/// CSS `transition-timing-function` / `animation-timing-function`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TimingFunction {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps {
        n: u32,
        jump: StepJumpTerm,
    },
    /// CSS `linear()` piecewise stops (intern id; table in [`super::easing`]).
    LinearStops(u16),
    /// Native spring driver (`spring()` / `spring(stiffness, damping)`). Not CSS.
    /// Preserve duration/visual-duration authoring for the native spring driver.
    SpringOptions(super::generator::SpringOptions),
    Spring {
        stiffness: f32,
        damping: f32,
        mass: f32,
    },
}

impl TimingFunction {
    pub fn eval(&self, t: f32) -> f32 {
        use super::animation::cubic_bezier_y_at_x;
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier_y_at_x(t, 0.25, 0.1, 0.25, 1.0),
            Self::EaseIn => cubic_bezier_y_at_x(t, 0.42, 0.0, 1.0, 1.0),
            Self::EaseOut => cubic_bezier_y_at_x(t, 0.0, 0.0, 0.58, 1.0),
            Self::EaseInOut => cubic_bezier_y_at_x(t, 0.42, 0.0, 0.58, 1.0),
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier_y_at_x(t, *x1, *y1, *x2, *y2),
            Self::Steps { n, jump } => super::easing::eval_steps(t, *n, *jump),
            Self::LinearStops(id) => super::easing::eval_linear_stops(*id, t),
            Self::SpringOptions(opts) => {
                let spring = super::generator::Spring::new(0.0, 1.0, 0.0, *opts);
                spring.next(t * spring.calculated_duration_ms()).value
            }
            Self::Spring {
                stiffness,
                damping,
                mass,
            } => super::generator::spring_unit_easing(t, *stiffness, *damping, *mass),
        }
    }

    pub fn is_spring(self) -> bool {
        matches!(self, Self::Spring { .. } | Self::SpringOptions(_))
    }

    /// react-spring / Framer-ish defaults (stiffness 170, damping 26, mass 1).
    pub fn default_spring() -> Self {
        Self::Spring {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
        }
    }

    pub fn from_css(s: &str) -> Self {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "linear" => Self::Linear,
            "ease" => Self::Ease,
            "ease-in" => Self::EaseIn,
            "ease-out" => Self::EaseOut,
            "ease-in-out" => Self::EaseInOut,
            _ => {
                if let Some(spring) = super::timing::parse_spring_timing(&s) {
                    return spring;
                }
                if let Some(steps) = super::easing::parse_steps_timing(&s) {
                    return steps;
                }
                if let Some(linear) = super::easing::parse_linear_timing(&s) {
                    return linear;
                }
                if let Some(inner) = s
                    .strip_prefix("cubic-bezier(")
                    .and_then(|r| r.strip_suffix(')'))
                {
                    let parts: Vec<f32> = inner
                        .split(',')
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    if parts.len() == 4 {
                        return Self::CubicBezier(parts[0], parts[1], parts[2], parts[3]);
                    }
                }
                Self::Ease
            }
        }
    }
}

/// CSS `animation-direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "reverse" => Some(Self::Reverse),
            "alternate" => Some(Self::Alternate),
            "alternate-reverse" => Some(Self::AlternateReverse),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Reverse => "reverse",
            Self::Alternate => "alternate",
            Self::AlternateReverse => "alternate-reverse",
        }
    }
}

/// CSS `animation-fill-mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "forwards" => Some(Self::Forwards),
            "backwards" => Some(Self::Backwards),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forwards => "forwards",
            Self::Backwards => "backwards",
            Self::Both => "both",
        }
    }

    pub fn apply_backwards(self) -> bool {
        matches!(self, Self::Backwards | Self::Both)
    }

    pub fn apply_forwards(self) -> bool {
        matches!(self, Self::Forwards | Self::Both)
    }
}

/// CSS `animation-play-state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Running,
    Paused,
}

impl AnimationPlayState {
    pub fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

/// CSS `animation-iteration-count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationIterationCount {
    Finite(f32),
    Infinite,
}

impl Default for AnimationIterationCount {
    fn default() -> Self {
        Self::Finite(1.0)
    }
}

impl AnimationIterationCount {
    pub fn from_css(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s == "infinite" {
            return Some(Self::Infinite);
        }
        s.parse::<f32>().ok().map(|n| Self::Finite(n.max(0.0)))
    }
}
