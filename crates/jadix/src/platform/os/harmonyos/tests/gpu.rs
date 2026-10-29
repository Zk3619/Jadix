use super::*;

#[test]
fn ohos_has_gles_fallback() {
    let names: Vec<_> = gpu_backend_order(false).iter().map(|(_, n)| *n).collect();
    assert!(names.contains(&"gl"));
    if cfg!(feature = "backend-vulkan") {
        assert_eq!(names.first().copied(), Some("vulkan"));
    }
}
