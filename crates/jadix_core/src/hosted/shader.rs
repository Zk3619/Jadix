//! Portable shader inputs. Names and aggregate sizes are validated before any backend compiles them.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShaderUniform {
    Float(f32), Int(i32), Uint(u32), Vec2([f32; 2]), Vec3([f32; 3]), Vec4([f32; 4]),
    Mat4([f32; 16]), Vec4Array(Vec<[f32; 4]>),
}

#[cfg(test)]
mod composition_tests {
    use super::*;
    #[test]
    fn transition_progress_has_bounded_time_and_zero_duration_policy() {
        let mut composition = ShaderComposition::Transition { value: "next".into(), duration_ms: 100, fallback: ShaderFallback::Fade };
        for (elapsed, expected) in [(-1., 0.), (0., 0.), (50., 0.5), (100., 1.), (f64::INFINITY, 1.), (f64::NAN, 0.)] {
            assert_eq!(composition.transition_progress(elapsed), expected);
        }
        if let ShaderComposition::Transition { duration_ms, .. } = &mut composition { *duration_ms = 0; }
        assert_eq!(composition.transition_progress(0.), 1.);
        assert_eq!(ShaderComposition::Background.transition_progress(0.), 1.);
    }
}
impl ShaderUniform {
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Int(_) | Self::Uint(_) => true,
            Self::Float(v) => v.is_finite(),
            Self::Vec2(v) => v.iter().all(|v| v.is_finite()),
            Self::Vec3(v) => v.iter().all(|v| v.is_finite()),
            Self::Vec4(v) => v.iter().all(|v| v.is_finite()),
            Self::Mat4(v) => v.iter().all(|v| v.is_finite()),
            Self::Vec4Array(v) => !v.is_empty() && v.len() <= 64 && v.iter().flatten().all(|v| v.is_finite()),
        }
    }
}
pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128
        && value.bytes().next().is_some_and(|v| v.is_ascii_alphabetic() || v == b'_')
        && value.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'_')
}
pub(super) fn parameter_name(value: &str) -> bool {
    identifier(value) && !value.starts_with("gl_") && !value.starts_with("jadix_") && !value.starts_with("sample_")
        && !matches!(value, "uResolution" | "uTime" | "uScale" | "uProgress" | "uOverflow" | "uValues" | "aPosition" | "jadix" | "uniforms")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderLanguage { #[default] Glsl, Wgsl }
#[cfg(feature = "shader")]
pub mod compiler;

/// Compositing is shared policy; native adapters only provide frame textures.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShaderComposition {
    #[default] Background,
    Effect,
    Transition { value: String, #[serde(default = "transition_duration")] duration_ms: u32,
        #[serde(default)] fallback: ShaderFallback },
}
fn transition_duration() -> u32 { 300 }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderFallback { #[default] Fade, None }
impl ShaderComposition {
    /// Uncontrolled transitions finish immediately for zero duration. Adapters
    /// supply elapsed time so pausing, testing and frame scheduling share policy.
    pub fn transition_progress(&self, elapsed_ms: f64) -> f32 {
        match self {
            Self::Transition { duration_ms, .. } if *duration_ms > 0 =>
                (elapsed_ms.max(0.0) / f64::from(*duration_ms)).min(1.0) as f32,
            _ => 1.0,
        }
    }
    pub fn validate(&self) -> bool {
        match self {
            Self::Transition { value, duration_ms, .. } => value.len() <= 4096 && !value.contains('\0') && *duration_ms <= 60_000,
            _ => true,
        }
    }
    pub fn inputs(&self) -> &'static [&'static str] {
        match self { Self::Background => &[], Self::Effect => &["content", "backdrop"],
            Self::Transition { .. } => &["from", "to", "content", "backdrop"] }
    }
}
