//! wgpu 30 constructors and surface-acquire mapping.

/// wgpu 30 dropped `SurfaceError`; map [`wgpu::CurrentSurfaceTexture`] back to a Result
/// so existing Lost/Outdated/Timeout recovery in the window loop stays intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAcquireError {
    Timeout,
    Outdated,
    Lost,
    Occluded,
    Validation,
}

pub fn instance(backends: wgpu::Backends) -> wgpu::Instance {
    #[cfg(all(windows, feature = "backend-dx11-native"))]
    if backends == wgpu::Backends::DX11 {
        return unsafe {
            wgpu::Instance::from_hal::<wgpu::hal::dx11::Api>(
                wgpu::hal::dx11::Instance::new().expect("native D3D11 instance"),
            )
        };
    }
    #[allow(unused_mut)] // D3D12 compiler policy is Windows-only.
    let mut descriptor = wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    };
    // Use wgpu's production flags in every build. Native SDK validation and
    // unoptimized shader debugging are explicit WGPU_VALIDATION/WGPU_DEBUG
    // opt-ins; wgpu-core and indirect-call validation remain enabled.
    descriptor.flags = wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL.with_env();
    #[cfg(all(windows, feature = "backend-dx11-angle"))]
    {
        // D3D11 ANGLE exposes its storage-buffer path with an explicit ES 3.1
        // request. Automatic picks ES 3.0, below Jadix's polygon-clip needs.
        descriptor.backend_options.gl.gles_minor_version = wgpu::Gles3MinorVersion::Version1;
    }
    #[cfg(all(windows, feature = "backend-dx12"))]
    {
        // Jadix needs SM5, not a PATH-discovered DXC/DXIL toolchain. The latter can
        // compile successfully but fail PSO creation on the installed driver.
        // Use the Windows compiler by default; an explicit wgpu override wins.
        descriptor.backend_options.dx12.shader_compiler =
            wgpu::Dx12Compiler::from_env().unwrap_or(wgpu::Dx12Compiler::Fxc);
    }
    wgpu::Instance::new(descriptor)
}

pub fn device_desc<'a>(
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

pub fn acquire_surface(
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
pub fn wait_error_scope(
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
pub fn try_shader_module(
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

/// Create a compute pipeline without panicking on validation failure.
#[cfg_attr(not(feature = "compute"), allow(dead_code))]
pub fn try_compute_pipeline(
    device: &wgpu::Device,
    desc: &wgpu::ComputePipelineDescriptor<'_>,
) -> Option<wgpu::ComputePipeline> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        device.create_compute_pipeline(desc)
    })) {
        Ok(p) => p,
        Err(_) => {
            let _ = wait_error_scope(device, scope);
            return None;
        }
    };
    if wait_error_scope(device, scope).is_some() {
        return None;
    }
    Some(pipeline)
}

/// Log wgpu errors instead of the default panic (`error_handling.md` E1).
pub fn install_device_error_handlers(device: &wgpu::Device) {
    device.on_uncaptured_error(std::sync::Arc::new(|err| {
        log::error!("wgpu uncaptured error: {err}");
    }));
    device.set_device_lost_callback(|reason, msg| {
        log::error!("wgpu device lost ({reason:?}): {msg}");
    });
}

/// Isolate bundled / optional GPU init. Returns `None` if the closure panics or the
/// validation scope captures an error (`error_handling.md` E2).
pub fn try_gpu_init<T>(device: &wgpu::Device, warn: &str, f: impl FnOnce() -> T) -> Option<T> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let scoped = wait_error_scope(device, scope);
    match (built, scoped) {
        (Ok(v), None) => Some(v),
        _ => {
            log::warn!("{warn}");
            None
        }
    }
}
