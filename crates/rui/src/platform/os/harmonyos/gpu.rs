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
mod tests {
    use super::*;

    #[test]
    fn ohos_has_gles_fallback() {
        let names: Vec<_> = gpu_backend_order(false).iter().map(|(_, n)| *n).collect();
        assert!(names.contains(&"gl"));
        if cfg!(feature = "backend-vulkan") {
            assert_eq!(names.first().copied(), Some("vulkan"));
        }
    }
}
