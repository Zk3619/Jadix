//! Browser: WebGPU first, WebGL (GLES) fallback.

pub(crate) fn gpu_backend_order(_compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    vec![
        (wgpu::Backends::BROWSER_WEBGPU, "webgpu"),
        (wgpu::Backends::GL, "webgl"),
    ]
}

#[cfg(test)]
#[path = "tests/gpu.rs"]
mod tests;
