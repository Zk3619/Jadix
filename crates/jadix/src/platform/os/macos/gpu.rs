//! macOS: Metal only (wgpu has no other viable desktop backend here).

pub(crate) fn gpu_backend_order(_compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    vec![(wgpu::Backends::METAL, "metal")]
}

#[cfg(test)]
#[path = "tests/gpu.rs"]
mod tests;
