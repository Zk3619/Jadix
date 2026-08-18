//! Dedicated GPU for windows that need compositor alpha.
//!
//! wgpu 22 GL reports only `Opaque`; DX12 maps every alpha mode to
//! `DXGI_ALPHA_MODE_IGNORE`. Vulkan on Windows exposes `PreMultiplied`, which
//! DWM can composite over the desktop when the HWND has `WS_EX_NOREDIRECTIONBITMAP`.

use std::sync::Arc;

use winit::window::Window;

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

pub(super) fn try_acquire(window: &Arc<Window>) -> Option<CompositorGpu> {
    pollster::block_on(try_acquire_async(window))
}

async fn try_acquire_async(window: &Arc<Window>) -> Option<CompositorGpu> {
    for (backends, name) in backend_candidates() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let Ok(surface) = instance.create_surface(window.clone()) else {
            log::debug!("glass gpu {name}: no surface");
            continue;
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await;
        let Some(adapter) = adapter else {
            log::debug!("glass gpu {name}: no adapter");
            continue;
        };
        let caps = surface.get_capabilities(&adapter);
        if !compositor_alpha_available(&caps.alpha_modes) {
            log::debug!(
                "glass gpu {name}: no premul alpha (modes={:?})",
                caps.alpha_modes
            );
            continue;
        }
        let adapter_limits = adapter.limits();
        let mut required_limits = wgpu::Limits::downlevel_defaults();
        required_limits.max_texture_dimension_1d = adapter_limits.max_texture_dimension_1d;
        required_limits.max_texture_dimension_2d = adapter_limits.max_texture_dimension_2d;
        required_limits.max_texture_dimension_3d = adapter_limits
            .max_texture_dimension_3d
            .min(required_limits.max_texture_dimension_3d.max(2048));
        let (device, queue) = match adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    label: Some("rui glass"),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
        {
            Ok(dq) => dq,
            Err(e) => {
                log::debug!("glass gpu {name}: device failed: {e}");
                continue;
            }
        };
        let alpha_mode =
            crate::renderer::pick_composite_alpha(&caps.alpha_modes, true);
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
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);
        log::info!(
            "glass gpu: {name} ({}) alpha_mode={alpha_mode:?} format={format:?}",
            adapter.get_info().name
        );
        return Some(CompositorGpu {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            config,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::compositor_alpha_available;

    #[test]
    fn premul_counts_as_available() {
        assert!(compositor_alpha_available(&[
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::PreMultiplied,
        ]));
        assert!(!compositor_alpha_available(&[
            wgpu::CompositeAlphaMode::Opaque
        ]));
    }
}
