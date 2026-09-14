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

/// Poll a wgpu validation error scope to completion (shader / pipeline compile).
pub(crate) fn wait_error_scope(
    device: &wgpu::Device,
    scope: wgpu::ErrorScopeGuard,
) -> Option<wgpu::Error> {
    use std::future::Future;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake};

    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
        fn wake_by_ref(self: &Arc<Self>) {}
    }

    let waker = Arc::new(Noop).into();
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(scope.pop());
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(err) => return err,
            Poll::Pending => {
                let _ = device.poll(wgpu::PollType::Poll);
                std::thread::yield_now();
            }
        }
    }
}

/// Create a WGSL module without panicking on validation failure.
pub(crate) fn try_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &str,
) -> Option<wgpu::ShaderModule> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        })
    })) {
        Ok(m) => m,
        Err(_) => {
            let _ = wait_error_scope(device, scope);
            return None;
        }
    };
    if wait_error_scope(device, scope).is_some() {
        return None;
    }
    Some(module)
}
