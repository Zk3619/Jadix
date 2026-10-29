use super::*;

fn names<'a>(order: &'a [(wgpu::Backends, &'a str)]) -> Vec<&'a str> {
    order.iter().map(|(_, n)| *n).collect()
}

#[test]
fn opaque_prefers_gl_with_native_fallback() {
    let order = gpu_backend_order(false);
    if cfg!(target_os = "macos") {
        return;
    }
    if cfg!(all(windows, feature = "backend-dx12")) {
        assert_eq!(order.first().map(|(_, n)| *n), Some("gl"));
        assert_eq!(order.get(1).map(|(_, n)| *n), Some("dx12"));
    } else {
        assert_eq!(order.first().map(|(_, n)| *n), Some("gl"));
    }
}

#[test]
fn compositor_skips_gl_when_no_vulkan_feature() {
    let order = gpu_backend_order(true);
    if cfg!(target_os = "macos") {
        return;
    }
    let names = names(&order);
    if cfg!(feature = "backend-vulkan") {
        assert_eq!(names.first(), Some(&"vulkan"));
    } else if cfg!(all(windows, feature = "backend-dx12")) {
        assert_eq!(names.first(), Some(&"dx12"));
    } else {
        assert_eq!(names, vec!["gl"]);
    }
}
