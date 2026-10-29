//! iOS: Metal only (same constraint as macOS).

pub(crate) fn gpu_backend_order(_compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    vec![(wgpu::Backends::METAL, "metal")]
}

#[cfg(test)]
#[path = "tests/gpu.rs"]
mod tests;
