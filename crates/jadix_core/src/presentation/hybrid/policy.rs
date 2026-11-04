use super::{HybridGeometry, Presentation, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationIntent {
    Default,
    PreferNative,
    RequireNative,
    RequireGpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub presentation: Presentation,
    pub fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionMode {
    Overlay,
    Snapshot,
    GpuIsland,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionError {
    UnsupportedPresenter,
    UnsupportedMode,
    UnsupportedClip,
    MissingSemantics,
    InteractiveSnapshot,
    OverlapOrder,
    InvalidGeometry,
    AdapterFailure,
    Disposed,
    Revision,
}

pub fn select(
    intent: PresentationIntent,
    default: Presentation,
    gpu: bool,
    native: bool,
) -> Result<Selection, CompositionError> {
    let desired = match intent {
        PresentationIntent::Default => default,
        PresentationIntent::PreferNative | PresentationIntent::RequireNative => {
            Presentation::Native
        }
        PresentationIntent::RequireGpu => Presentation::Gpu,
    };
    if match desired {
        Presentation::Gpu => gpu,
        Presentation::Native => native,
    } {
        return Ok(Selection {
            presentation: desired,
            fallback: false,
        });
    }
    if intent == PresentationIntent::PreferNative && gpu {
        Ok(Selection {
            presentation: Presentation::Gpu,
            fallback: true,
        })
    } else {
        Err(CompositionError::UnsupportedPresenter)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmbeddingCapabilities {
    pub overlay: bool,
    pub snapshot: bool,
    pub gpu_island: bool,
    pub rectangular_clip: bool,
    pub platform_semantics: bool,
}

impl EmbeddingCapabilities {
    pub fn validate(
        &self,
        mode: CompositionMode,
        geometry: &[HybridGeometry],
    ) -> Result<(), CompositionError> {
        let supported = match mode {
            CompositionMode::Overlay => self.overlay,
            CompositionMode::Snapshot => self.snapshot,
            CompositionMode::GpuIsland => self.gpu_island,
        };
        if !supported {
            return Err(CompositionError::UnsupportedMode);
        }
        for entry in geometry {
            if !entry.is_valid() {
                return Err(CompositionError::InvalidGeometry);
            }
            if entry.presentation != Presentation::Native {
                continue;
            }
            if !entry.clips.is_empty() && !self.rectangular_clip {
                return Err(CompositionError::UnsupportedClip);
            }
            if entry.visible && entry.interactive {
                if mode == CompositionMode::Snapshot {
                    return Err(CompositionError::InteractiveSnapshot);
                }
                if !self.platform_semantics {
                    return Err(CompositionError::MissingSemantics);
                }
            }
        }
        if mode == CompositionMode::Overlay {
            for (native_index, native) in geometry
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.visible && entry.presentation == Presentation::Native)
            {
                for (gpu_index, gpu) in geometry
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.visible && entry.presentation == Presentation::Gpu)
                {
                    if (gpu.z_index, gpu_index) > (native.z_index, native_index)
                        && intersects(gpu.frame, native.frame)
                    {
                        return Err(CompositionError::OverlapOrder);
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn intersects(first: Rect, second: Rect) -> bool {
    first.width > 0.0
        && first.height > 0.0
        && second.width > 0.0
        && second.height > 0.0
        && first.x < second.x + second.width
        && second.x < first.x + first.width
        && first.y < second.y + second.height
        && second.y < first.y + first.height
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportTransform {
    pub scale: f64,
    pub offset: [f64; 2],
    pub safe_area: [f64; 4],
}

impl ViewportTransform {
    pub fn physical(&self, frame: Rect) -> Result<Rect, CompositionError> {
        if !frame.is_valid()
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || self.offset.iter().any(|value| !value.is_finite())
            || self
                .safe_area
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(CompositionError::InvalidGeometry);
        }
        let frame = Rect {
            x: (frame.x + self.offset[0] + self.safe_area[3]) * self.scale,
            y: (frame.y + self.offset[1] + self.safe_area[0]) * self.scale,
            width: frame.width * self.scale,
            height: frame.height * self.scale,
        };
        if frame.is_valid() {
            Ok(frame)
        } else {
            Err(CompositionError::InvalidGeometry)
        }
    }
}
