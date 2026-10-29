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
    if std::env::var("JADIX_GPU_BACKEND").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "dx11" | "d3d11" | "dx11-native"
        )
    }) {
        if cfg!(all(windows, feature = "backend-dx11-native")) {
            return vec![(wgpu::Backends::DX11, "dx11-native")];
        } else {
            panic!("native dx11 requires Windows and feature backend-dx11-native");
        }
    }
    if std::env::var("JADIX_GPU_BACKEND")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("dx11-angle"))
    {
        if !cfg!(all(windows, feature = "backend-dx11-angle")) {
            panic!("dx11-angle requires Windows and feature backend-dx11-angle");
        }
        // A comparison must never silently fall back to WGL/DX12/software.
        return vec![(wgpu::Backends::GL, "dx11-angle")];
    }
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
#[path = "tests/gpu.rs"]
mod tests;
