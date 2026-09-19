//! Browser: WebGPU first, WebGL (GLES) fallback.

pub(crate) fn gpu_backend_order(_compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    vec![
        (wgpu::Backends::BROWSER_WEBGPU, "webgpu"),
        (wgpu::Backends::GL, "webgl"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_prefers_webgpu() {
        assert_eq!(
            gpu_backend_order(false).first().map(|(_, n)| *n),
            Some("webgpu")
        );
    }
}
