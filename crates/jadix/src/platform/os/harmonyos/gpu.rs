//! OpenHarmony: Vulkan first (device GPU), GLES fallback.

#![cfg_attr(not(target_env = "ohos"), allow(dead_code))]

pub(crate) fn gpu_backend_order(compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    let mut order = Vec::new();
    if cfg!(feature = "backend-vulkan") {
        order.push((wgpu::Backends::VULKAN, "vulkan"));
    }
    let _ = compositor_alpha;
    order.push((wgpu::Backends::GL, "gl"));
    order
}

#[cfg(test)]
#[path = "tests/gpu.rs"]
mod tests;
