use crate::{
    assets::{self, RuntimeModel},
    expression::{self, ExpressionManager},
    json::{Expression3, Motion3},
    motion::{self, MotionPlayer},
    ModelRuntime,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Asset(#[from] assets::AssetLoadError),
    #[error("{0}")]
    Motion(#[from] motion::MotionLoadError),
    #[error("{0}")]
    Expression(#[from] expression::ExpressionLoadError),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;

/// A model, cached motion/expression assets, and deterministic frame updates.
/// Audio playback is supplied by the host. Motion group names are case sensitive.
pub struct Player {
    model: RuntimeModel,
    motions: BTreeMap<(String, usize), Motion3>,
    expressions: BTreeMap<String, Expression3>,
    motion: Option<MotionPlayer>,
    expression_manager: ExpressionManager,
    elapsed: f32,
    blink: bool,
    breath: bool,
    look: [f32; 2],
    mouth: Option<f32>,
}

fn relative_asset(directory: &Path, reference: &str) -> Result<PathBuf> {
    let path = Path::new(reference);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Error::Invalid(format!(
            "asset must be inside the model folder: {reference}"
        )));
    }
    Ok(directory.join(path))
}

impl Player {
    /// Loads model3, moc3, PNG textures, physics3, pose3, and all declared motions/expressions.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let model = assets::load_model_runtime(path)?;
        let directory = model
            .model_dir()
            .ok_or_else(|| Error::Invalid("missing model directory".into()))?;
        let mut motions = BTreeMap::new();
        for (group, entries) in model.runtime().model().motions() {
            for (index, entry) in entries.iter().enumerate() {
                motions.insert(
                    (group.clone(), index),
                    motion::load_motion(relative_asset(directory, entry.file())?)?,
                );
            }
        }
        let mut expressions = BTreeMap::new();
        for entry in model.runtime().model().expressions() {
            if expressions.contains_key(entry.name()) {
                return Err(Error::Invalid(format!(
                    "duplicate expression {}",
                    entry.name()
                )));
            }
            expressions.insert(
                entry.name().to_owned(),
                expression::load_expression(relative_asset(directory, entry.file())?)?,
            );
        }
        Ok(Self {
            model,
            motions,
            expressions,
            motion: None,
            expression_manager: ExpressionManager::new(),
            elapsed: 0.0,
            blink: true,
            breath: true,
            look: [0.0; 2],
            mouth: None,
        })
    }

    pub fn model(&self) -> &RuntimeModel {
        &self.model
    }
    pub fn runtime(&self) -> &ModelRuntime {
        self.model.runtime()
    }
    /// Custom controls should use parameter overrides to persist across updates.
    pub fn runtime_mut(&mut self) -> &mut ModelRuntime {
        self.model.runtime_mut()
    }
    pub fn motions(&self) -> impl Iterator<Item = (&str, usize)> {
        self.motions
            .keys()
            .map(|(group, index)| (group.as_str(), *index))
    }
    pub fn expressions(&self) -> impl Iterator<Item = &str> {
        self.expressions.keys().map(String::as_str)
    }
    pub fn play_motion(&mut self, group: &str, index: usize, looping: bool) -> Result<()> {
        let motion = self
            .motions
            .get(&(group.to_owned(), index))
            .ok_or_else(|| Error::Invalid(format!("unknown motion {group}[{index}]")))?
            .clone();
        self.motion = Some(MotionPlayer::with_looping(motion, looping));
        Ok(())
    }
    pub fn stop_motion(&mut self) {
        self.motion = None;
    }
    pub fn motion_finished(&self) -> bool {
        self.motion.as_ref().is_none_or(MotionPlayer::is_finished)
    }
    pub fn set_expression(&mut self, name: &str) -> Result<()> {
        let expression = self
            .expressions
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("unknown expression {name}")))?
            .clone();
        self.expression_manager.play(expression);
        Ok(())
    }
    pub fn clear_expressions(&mut self) {
        self.expression_manager.stop_all();
    }
    pub fn set_blink(&mut self, enabled: bool) {
        self.blink = enabled;
    }
    pub fn set_breath(&mut self, enabled: bool) {
        self.breath = enabled;
    }
    /// Normalized eye/head target, with y increasing upward.
    pub fn set_look_at(&mut self, x: f32, y: f32) -> Result<()> {
        if !x.is_finite() || !y.is_finite() {
            return Err(Error::Invalid("non-finite look target".into()));
        }
        self.look = [x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)];
        Ok(())
    }
    /// External lip-sync amplitude. The host can derive this from microphone/audio data.
    pub fn set_mouth(&mut self, amplitude: f32) -> Result<()> {
        if !amplitude.is_finite() {
            return Err(Error::Invalid("non-finite mouth amplitude".into()));
        }
        self.mouth = Some(amplitude.clamp(0.0, 1.0));
        Ok(())
    }
    pub fn clear_mouth(&mut self) {
        self.mouth = None;
    }
    /// Advances in bounded substeps so physics and pose fading remain stable after slow frames.
    pub fn update(&mut self, seconds: f32) -> Result<()> {
        if !seconds.is_finite() || !(0.0..=1.0).contains(&seconds) {
            return Err(Error::Invalid(
                "frame delta must be finite and between 0 and 1 seconds".into(),
            ));
        }
        let steps = (seconds * 60.0).ceil().max(1.0) as usize;
        let delta = seconds / steps as f32;
        for _ in 0..steps {
            self.elapsed = (self.elapsed + delta).rem_euclid(120.0);
            let runtime = self.model.runtime_mut();
            runtime.reset_parameters();
            if self.blink {
                let phase = self.elapsed.rem_euclid(4.0);
                let value = if phase < 0.18 {
                    (phase / 0.09 - 1.0).abs()
                } else {
                    1.0
                };
                apply_group(runtime, "EyeBlink", value);
            }
            if self.breath {
                runtime.set_parameter("ParamBreath", 0.5 + 0.5 * (self.elapsed * 1.5).sin());
            }
            if let Some(motion) = &mut self.motion {
                motion.tick(delta);
                motion.apply(runtime);
            }
            for (name, value) in [
                ("ParamAngleX", self.look[0] * 30.0),
                ("ParamAngleY", self.look[1] * 30.0),
                ("ParamEyeBallX", self.look[0]),
                ("ParamEyeBallY", self.look[1]),
            ] {
                let base = runtime.parameter_value(name).unwrap_or(0.0);
                runtime.set_parameter(name, base + value);
            }
            if let Some(mouth) = self.mouth {
                apply_group(runtime, "LipSync", mouth);
            }
            self.expression_manager.tick(delta);
            self.expression_manager.apply(runtime);
            runtime.apply_physics(delta);
            runtime.apply_pose(delta);
            runtime.apply_parameter_overrides();
            runtime
                .update_meshes()
                .ok_or_else(|| Error::Invalid("drawable deformation failed".into()))?;
        }
        Ok(())
    }
}

fn apply_group(runtime: &mut ModelRuntime, name: &str, value: f32) {
    let ids: Vec<_> = runtime
        .model()
        .groups()
        .iter()
        .filter(|group| group.target() == "Parameter" && group.name() == name)
        .flat_map(|group| group.ids().iter().cloned())
        .collect();
    for id in ids {
        runtime.set_parameter(&id, value);
    }
}
