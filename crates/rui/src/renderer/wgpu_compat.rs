//! wgpu 30 constructors and surface-acquire mapping.

/// wgpu 30 dropped `SurfaceError`; map [`wgpu::CurrentSurfaceTexture`] back to a Result
/// so existing Lost/Outdated/Timeout recovery in the window loop stays intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceAcquireError {
    Timeout,
    Outdated,
    Lost,
    Occluded,
    Validation,
}

pub(crate) fn instance(backends: wgpu::Backends) -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
}

pub(crate) fn device_desc<'a>(
    label: wgpu::Label<'a>,
    required_limits: wgpu::Limits,
    memory_hints: wgpu::MemoryHints,
) -> wgpu::DeviceDescriptor<'a> {
    wgpu::DeviceDescriptor {
        label,
        required_features: wgpu::Features::empty(),
        required_limits,
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints,
        trace: wgpu::Trace::Off,
    }
}

pub(crate) fn acquire_surface(
    surface: &wgpu::Surface<'_>,
) -> Result<wgpu::SurfaceTexture, SurfaceAcquireError> {
    match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
            Ok(t)
        }
        wgpu::CurrentSurfaceTexture::Timeout => Err(SurfaceAcquireError::Timeout),
        wgpu::CurrentSurfaceTexture::Occluded => Err(SurfaceAcquireError::Occluded),
        wgpu::CurrentSurfaceTexture::Outdated => Err(SurfaceAcquireError::Outdated),
        wgpu::CurrentSurfaceTexture::Lost => Err(SurfaceAcquireError::Lost),
        wgpu::CurrentSurfaceTexture::Validation => Err(SurfaceAcquireError::Validation),
    }
}
