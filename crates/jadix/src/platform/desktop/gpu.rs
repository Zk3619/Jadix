//! Opaque desktop windows prefer GL for low startup and driver overhead.
//!
//! macOS overrides to Metal-only. `JADIX_GPU_BACKEND` is applied by the facade so an
//! unsupported pick degrades instead of aborting.

pub(crate) fn gpu_backend_order(compositor_alpha: bool) -> Vec<(wgpu::Backends, &'static str)> {
    if compositor_alpha {
        compositor_backend_order()
    } else {
        opaque_backend_order()
    }
}

pub(crate) fn with_requested_backend_front(
    mut order: Vec<(wgpu::Backends, &'static str)>,
) -> Vec<(wgpu::Backends, &'static str)> {
    if let Some(front) = parse_requested_backend() {
        let label = order
            .iter()
            .find(|(b, _)| *b == front)
            .map_or("requested", |(_, name)| *name);
        order.retain(|(b, _)| *b != front);
        order.insert(0, (front, label));
    }
    order
}

fn opaque_backend_order() -> Vec<(wgpu::Backends, &'static str)> {
    let mut order = vec![(wgpu::Backends::GL, "gl")];
    push_optional_backend(&mut order, wgpu::Backends::DX12, "dx12");
    push_optional_backend(&mut order, wgpu::Backends::VULKAN, "vulkan");
    order
}

fn compositor_backend_order() -> Vec<(wgpu::Backends, &'static str)> {
    let mut order = Vec::new();
    push_optional_backend(&mut order, wgpu::Backends::VULKAN, "vulkan");
    push_optional_backend(&mut order, wgpu::Backends::DX12, "dx12");
    order.push((wgpu::Backends::GL, "gl"));
    order
}

fn push_optional_backend(
    order: &mut Vec<(wgpu::Backends, &'static str)>,
    backend: wgpu::Backends,
    name: &'static str,
) {
    if backend_available(backend) {
        order.push((backend, name));
    }
}

fn backend_available(backend: wgpu::Backends) -> bool {
    match backend {
        wgpu::Backends::DX12 => cfg!(all(windows, feature = "backend-dx12")),
        wgpu::Backends::VULKAN => cfg!(feature = "backend-vulkan"),
        _ => true,
    }
}

fn parse_requested_backend() -> Option<wgpu::Backends> {
    let requested = std::env::var("JADIX_GPU_BACKEND").unwrap_or_default();
    match requested.trim().to_ascii_lowercase().as_str() {
        "gl" | "gles" | "opengl" => Some(wgpu::Backends::GL),
        "dx12" | "d3d12" => Some(wgpu::Backends::DX12),
        "vulkan" | "vk" => Some(wgpu::Backends::VULKAN),
        "metal" => Some(wgpu::Backends::METAL),
        "webgpu" | "browser" => Some(wgpu::Backends::BROWSER_WEBGPU),
        "webgl" => Some(wgpu::Backends::GL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
}
