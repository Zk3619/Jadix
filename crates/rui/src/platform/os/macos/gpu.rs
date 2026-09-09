//! macOS: Metal only (wgpu has no other viable desktop backend here).

pub(crate) fn gpu_backend_order(_compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    vec![(wgpu::Backends::METAL, "metal")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_only_metal() {
        let order = gpu_backend_order(false);
        assert_eq!(
            order.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            vec!["metal"]
        );
        assert_eq!(gpu_backend_order(true), order);
    }
}
