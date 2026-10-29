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
