use super::*;

#[test]
fn web_prefers_webgpu() {
    assert_eq!(
        gpu_backend_order(false).first().map(|(_, n)| *n),
        Some("webgpu")
    );
}
