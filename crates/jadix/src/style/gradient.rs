//! Framework gradient texture cache and upload scheduling.
pub use jadix_2d::gradient::*;

struct GradCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

thread_local! {
    static GRADIENT_CPU: std::cell::RefCell<std::collections::HashMap<String, GradCpuEntry>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// Keys referenced during the current layout/collect pass.
    static GRADIENT_NEEDED: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Window DPI / `scale_factor` used when sizing gradient textures (logical → physical).
    static GRADIENT_RASTER_DPI: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Set the DPI used by [`gradient_texture_size`] during the next layout/collect.
pub fn set_gradient_raster_dpi(dpi: f32) {
    GRADIENT_RASTER_DPI.set(dpi.clamp(1.0, 3.0));
}

pub fn gradient_raster_dpi() -> f32 {
    GRADIENT_RASTER_DPI.get().clamp(1.0, 3.0)
}

/// Logical box size → texture pixel size (DPI-aware, capped).
pub fn gradient_texture_size(logical_w: f32, logical_h: f32) -> (u32, u32) {
    let dpi = gradient_raster_dpi();
    let tw = (logical_w * dpi).round().clamp(2.0, 1024.0) as u32;
    let th = (logical_h * dpi).round().clamp(2.0, 1024.0) as u32;
    (tw, th)
}

/// Scale CSS corner radii into texture pixel space.
pub fn gradient_texture_radii(logical_radii: [f32; 4]) -> [f32; 4] {
    let dpi = gradient_raster_dpi();
    [
        logical_radii[0] * dpi,
        logical_radii[1] * dpi,
        logical_radii[2] * dpi,
        logical_radii[3] * dpi,
    ]
}

/// Rasterize (cached) and mark this key as needed for the current frame.
/// Actual GPU upload is done by [`flush_gradient_textures`] so TexturePool eviction
/// can rehydrate without relying on a one-shot pending queue.
pub fn ensure_gradient_texture(
    grad: &Gradient,
    width: u32,
    height: u32,
    radii: [f32; 4],
) -> String {
    let width = width.max(1);
    let height = height.max(1);
    let key = gradient_texture_key(grad, width, height, radii);
    GRADIENT_CPU.with(|c| {
        let mut map = c.borrow_mut();
        if !map.contains_key(&key) {
            let rgba = rasterize_gradient_with_radii(grad, width, height, radii);
            map.insert(
                key.clone(),
                GradCpuEntry {
                    width,
                    height,
                    rgba,
                },
            );
        }
    });
    GRADIENT_NEEDED.with(|n| {
        n.borrow_mut().insert(key.clone());
    });
    key
}

/// Upload any needed gradient textures that are missing from the GPU pool.
/// Returns `true` if any texture was queued/uploaded (caller should request redraw).
pub fn flush_gradient_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = GRADIENT_NEEDED
        .with(|n| std::mem::take(&mut *n.borrow_mut()))
        .into_iter()
        .collect();
    let mut any = false;
    GRADIENT_CPU.with(|c| {
        let map = c.borrow();
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = map.get(&key) {
                crate::app::pending_images::queue_image_rgba8(
                    &key,
                    entry.width,
                    entry.height,
                    entry.rgba.clone(),
                );
                any = true;
            }
        }
    });
    any
}
