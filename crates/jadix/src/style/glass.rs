use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GlassMaterial {
    pub refraction_index: f32,
    pub thickness: f32,
    pub dispersion: f32,
    pub highlight: f32,
}

impl Default for GlassMaterial {
    fn default() -> Self {
        Self {
            refraction_index: 1.46,
            thickness: 12.0,
            dispersion: 0.025,
            highlight: 0.6,
        }
    }
}

impl GlassMaterial {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("refraction-index", self.refraction_index, 1.0, 2.5),
            ("thickness", self.thickness, 0.0, 64.0),
            ("dispersion", self.dispersion, 0.0, 0.2),
            ("highlight", self.highlight, 0.0, 1.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!(
                    "glass.{name} must be finite and between {min} and {max}"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn gpu_params(self) -> [f32; 4] {
        [
            self.refraction_index.clamp(1.0, 2.5),
            self.thickness.clamp(0.0, 64.0),
            self.dispersion.clamp(0.0, 0.2),
            self.highlight.clamp(0.0, 1.0),
        ]
    }
}
