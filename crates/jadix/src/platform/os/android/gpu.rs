//! Android: GLES first; optional Vulkan when `backend-vulkan` is on.

pub(crate) fn gpu_backend_order(compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    let mut order = Vec::new();
    if compositor_alpha && cfg!(feature = "backend-vulkan") {
        order.push((wgpu::Backends::VULKAN, "vulkan"));
    }
    order.push((wgpu::Backends::GL, "gl"));
    if !compositor_alpha && cfg!(feature = "backend-vulkan") {
        order.push((wgpu::Backends::VULKAN, "vulkan"));
    }
    order
}

#[cfg(test)]
#[path = "tests/gpu.rs"]
mod tests;
