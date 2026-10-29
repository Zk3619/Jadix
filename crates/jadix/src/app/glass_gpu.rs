//! Dedicated GPU for windows that need compositor alpha.
//!
//! wgpu 30 GL still reports only `Opaque`. Vulkan exposes `PreMultiplied`.
//! DX12 (opt-in `backend-dx12`) maps compositor alpha on HWND swapchains.
//!
//! Custom DComp recipes **must** present DX12 into a
//! `SurfaceTargetUnsafe::SurfaceHandle` (CreateSwapChainForCompositionSurfaceHandle).
//! HWND Vulkan/DX12 cannot coexist with `DesktopWindowTarget`, and
//! `WS_EX_NOREDIRECTIONBITMAP` plus CreateSwapChainForHwnd shows nothing.

use std::sync::Arc;

use winit::window::Window;

use crate::app::WindowOptions;

pub(super) struct CompositorGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

pub(super) fn compositor_alpha_available(modes: &[wgpu::CompositeAlphaMode]) -> bool {
    modes.iter().any(|m| {
        matches!(
            m,
            wgpu::CompositeAlphaMode::PreMultiplied | wgpu::CompositeAlphaMode::PostMultiplied
        )
    })
}

fn backend_candidates() -> Vec<(wgpu::Backends, &'static str)> {
    vec![
        (wgpu::Backends::VULKAN, "vulkan"),
        #[cfg(all(windows, feature = "backend-dx12"))]
        (wgpu::Backends::DX12, "dx12"),
    ]
}

pub(super) fn try_acquire(
    window: &Arc<dyn Window>,
    chrome: &WindowOptions,
) -> Option<CompositorGpu> {
    crate::utils::block_on::block_on(try_acquire_async(window, chrome))
}

async fn try_acquire_async(
    window: &Arc<dyn Window>,
    chrome: &WindowOptions,
) -> Option<CompositorGpu> {
    if chrome.compositor_alpha() && crate::platform::try_init_custom_glass(window.as_ref(), chrome)
    {
        if let Some(gpu) = try_acquire_dcomp(window).await {
            return Some(gpu);
        }
        let msg =
            "dcomp glass: DX12 composition surface failed; recreating without NOREDIRECTIONBITMAP";
        log::warn!("{msg}");
        eprintln!("jadix glass: {msg}");
        crate::platform::glass_detach(window.as_ref());
        return None;
    }
    if chrome.wants_dcomp_swapchain() {
        let msg = "dcomp glass: DesktopWindowTarget init failed; falling back";
        log::warn!("{msg}");
        eprintln!("jadix glass: {msg}");
        return None;
    }
    for (backends, name) in backend_candidates() {
        if let Some(gpu) = try_acquire_named(window, backends, name, None).await {
            return Some(gpu);
        }
    }
    None
}

#[cfg_attr(not(all(windows, feature = "windows-dcomp")), allow(dead_code))]
async fn try_acquire_dcomp(window: &Arc<dyn Window>) -> Option<CompositorGpu> {
    #[cfg(all(windows, feature = "windows-dcomp"))]
    {
        let handle = crate::platform::custom_glass_surface_handle(window.as_ref())?;
        try_acquire_named(window, wgpu::Backends::DX12, "dx12-dcomp", Some(handle)).await
    }
    #[cfg(not(all(windows, feature = "windows-dcomp")))]
    {
        let _ = window;
        None
    }
}

async fn try_acquire_named(
    window: &Arc<dyn Window>,
    backends: wgpu::Backends,
    name: &str,
    composition_handle: Option<*mut std::ffi::c_void>,
) -> Option<CompositorGpu> {
    let instance = crate::renderer::wgpu_compat::instance(backends);
    let surface = if let Some(handle) = composition_handle {
        #[cfg(all(windows, feature = "windows-dcomp"))]
        {
            match unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::SurfaceHandle(handle))
            } {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("glass gpu {name}: composition surface failed: {e}");
                    return None;
                }
            }
        }
        #[cfg(not(all(windows, feature = "windows-dcomp")))]
        {
            let _ = handle;
            return None;
        }
    } else {
        match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(_) => {
                log::debug!("glass gpu {name}: no surface");
                return None;
            }
        }
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        })
        .await;
    let Ok(adapter) = adapter else {
        log::warn!("glass gpu {name}: no adapter");
        if composition_handle.is_some() {
            eprintln!("jadix glass: glass gpu {name}: no adapter");
        }
        return None;
    };
    let caps = surface.get_capabilities(&adapter);
    if composition_handle.is_none() && !compositor_alpha_available(&caps.alpha_modes) {
        log::debug!(
            "glass gpu {name}: no premul alpha (modes={:?})",
            caps.alpha_modes
        );
        return None;
    }
    let required_limits = crate::renderer::limits_for(&adapter);
    let (device, queue) = match adapter
        .request_device(&crate::renderer::wgpu_compat::device_desc(
            Some("jadix glass"),
            required_limits,
            wgpu::MemoryHints::MemoryUsage,
        ))
        .await
    {
        Ok(dq) => {
            crate::renderer::wgpu_compat::install_device_error_handlers(&dq.0);
            dq
        }
        Err(e) => {
            log::debug!("glass gpu {name}: device failed: {e}");
            return None;
        }
    };
    let alpha_mode = if composition_handle.is_some() {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        crate::renderer::pick_composite_alpha(&caps.alpha_modes, true)
    };
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(caps.formats[0]);
    let present_mode = caps
        .present_modes
        .iter()
        .copied()
        .find(|m| matches!(m, wgpu::PresentMode::Fifo))
        .or_else(|| {
            caps.present_modes
                .iter()
                .copied()
                .find(|m| matches!(m, wgpu::PresentMode::FifoRelaxed))
        })
        .unwrap_or(caps.present_modes[0]);
    let size = window.surface_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        alpha_mode,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    surface.configure(&device, &config);
    log::info!(
        "glass gpu: {name} ({}) alpha_mode={alpha_mode:?} format={format:?} compute: {}",
        adapter.get_info().name,
        crate::renderer::compute_yes_no(crate::renderer::probe_gpu_caps(&adapter))
    );
    Some(CompositorGpu {
        instance,
        adapter,
        device: Arc::new(device),
        queue: Arc::new(queue),
        surface,
        config,
    })
}

#[cfg(test)]
#[path = "tests/glass_gpu.rs"]
mod tests;
