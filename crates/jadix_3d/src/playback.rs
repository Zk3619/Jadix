use crate::*;

/// Editable local-space pose. Blend before evaluating hierarchy and skin matrices.
#[derive(Clone, Debug)]
pub struct LocalPose {
    pub transforms: Vec<Transform>,
    pub weights: Vec<Vec<f32>>,
}

impl LocalPose {
    pub fn blend(&self, other: &Self, weight: f32) -> Result<Self> {
        if !weight.is_finite()
            || !(0.0..=1.0).contains(&weight)
            || self.transforms.len() != other.transforms.len()
            || self.weights.len() != other.weights.len()
            || self
                .weights
                .iter()
                .zip(&other.weights)
                .any(|(left, right)| left.len() != right.len())
        {
            return Err(Error("incompatible poses or invalid blend weight".into()));
        }
        Ok(Self {
            transforms: self
                .transforms
                .iter()
                .zip(&other.transforms)
                .map(|(left, right)| Transform {
                    translation: left.translation.lerp(right.translation, weight),
                    rotation: left.rotation.slerp(right.rotation, weight).normalize(),
                    scale: left.scale.lerp(right.scale, weight),
                })
                .collect(),
            weights: self
                .weights
                .iter()
                .zip(&other.weights)
                .map(|(left, right)| {
                    left.iter()
                        .zip(right)
                        .map(|(left, right)| left + (right - left) * weight)
                        .collect()
                })
                .collect(),
        })
    }
}

impl Scene {
    pub fn local_pose(&self, clip: Option<usize>, seconds: f32, repeat: bool) -> Result<LocalPose> {
        animation::local_pose(self, clip, seconds, repeat)
    }

    pub fn evaluate_pose(&self, local: LocalPose) -> Result<Pose> {
        animation::evaluate(self, local)
    }

    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|animation| animation.name.as_deref() == Some(name))
    }
}

/// Playback clock with pause, reverse speed, seeking and interruptible pose crossfades.
#[derive(Clone, Debug)]
pub struct Animator {
    clip: Option<usize>,
    seconds: f32,
    speed: f32,
    playing: bool,
    repeat: bool,
    fade: Option<(LocalPose, f32, f32)>,
}

impl Default for Animator {
    fn default() -> Self {
        Self {
            clip: None,
            seconds: 0.0,
            speed: 1.0,
            playing: true,
            repeat: true,
            fade: None,
        }
    }
}

impl Animator {
    pub fn play(
        &mut self,
        scene: &Scene,
        clip: Option<usize>,
        fade_seconds: f32,
        repeat: bool,
    ) -> Result<()> {
        if !fade_seconds.is_finite() || fade_seconds < 0.0 {
            return Err(Error("fade duration must be finite and nonnegative".into()));
        }
        scene.local_pose(clip, 0.0, repeat)?;
        let current = self.sample_local(scene)?;
        self.clip = clip;
        self.seconds = 0.0;
        self.repeat = repeat;
        self.playing = true;
        self.fade = (fade_seconds > 0.0).then_some((current, 0.0, fade_seconds));
        Ok(())
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }
    pub fn time(&self) -> f32 {
        self.seconds
    }

    pub fn set_speed(&mut self, speed: f32) -> Result<()> {
        if !speed.is_finite() {
            return Err(Error("speed must be finite".into()));
        }
        self.speed = speed;
        Ok(())
    }

    pub fn seek(&mut self, seconds: f32) -> Result<()> {
        if !seconds.is_finite() {
            return Err(Error("time must be finite".into()));
        }
        self.seconds = seconds;
        self.fade = None;
        Ok(())
    }

    pub fn update(&mut self, delta_seconds: f32) -> Result<()> {
        if !delta_seconds.is_finite()
            || delta_seconds < 0.0
            || !(self.seconds + delta_seconds * self.speed).is_finite()
        {
            return Err(Error("invalid animation delta".into()));
        }
        if self.playing {
            self.seconds += delta_seconds * self.speed;
            if let Some((_, elapsed, duration)) = &mut self.fade {
                *elapsed = (*elapsed + delta_seconds).min(*duration);
                if *elapsed >= *duration {
                    self.fade = None;
                }
            }
        }
        Ok(())
    }

    pub fn sample_local(&self, scene: &Scene) -> Result<LocalPose> {
        let local = scene.local_pose(self.clip, self.seconds, self.repeat)?;
        match &self.fade {
            Some((source, elapsed, duration)) => source.blend(&local, elapsed / duration),
            None => Ok(local),
        }
    }

    pub fn sample(&self, scene: &Scene) -> Result<Pose> {
        scene.evaluate_pose(self.sample_local(scene)?)
    }
}
