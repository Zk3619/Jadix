//! GPU 2D path backend: flatten curves → tessellate mesh → [`crate::renderer::path_pipeline::PathGpu`].
//!
//! CPU coverage raster (`paint_path_premul`) remains the golden/headless fallback.
//! Product path is on by default; set `JADIX_GPU_2D=0` to force CPU raster.
//! Runtime pipeline failure also disables GPU 2D (`error_handling.md` E2).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::primitives::path::{tessellate_path, PathCommand, PathQuad, PathStyle};
use crate::renderer::InstanceInput;

static ENV_ON: OnceLock<bool> = OnceLock::new();
static RUNTIME_ON: AtomicBool = AtomicBool::new(true);

/// Product GPU 2D path (PathPipeline). `JADIX_GPU_2D=0`/`false` forces CPU golden.
pub fn gpu2d_enabled() -> bool {
    let env_on = *ENV_ON.get_or_init(|| {
        std::env::var("JADIX_GPU_2D")
            .map(|v| {
                let t = v.trim();
                !(t == "0" || t.eq_ignore_ascii_case("false") || t.eq_ignore_ascii_case("off"))
            })
            .unwrap_or(true)
    });
    env_on && RUNTIME_ON.load(Ordering::Relaxed)
}

/// Turn off GPU 2D for this process after a pipeline / stencil init failure.
pub(crate) fn disable_gpu2d_runtime() {
    RUNTIME_ON.store(false, Ordering::Relaxed);
}

/// Flatten + tessellate a path into SDF-rect instances (thin-stroke fallback / tests).
/// Product Chart/Svg leaves use [`crate::renderer::path_pipeline::PathGpu`].
pub fn path_to_instances(
    cmds: &[PathCommand],
    style: &PathStyle,
    origin: [f32; 2],
    clip: Option<[f32; 4]>,
    opacity: f32,
) -> Vec<InstanceInput> {
    let quads = tessellate_path(cmds, style);
    quads_to_instances(&quads, origin, clip, opacity)
}

pub fn quads_to_instances(
    quads: &[PathQuad],
    origin: [f32; 2],
    clip: Option<[f32; 4]>,
    opacity: f32,
) -> Vec<InstanceInput> {
    quads
        .iter()
        .map(|q| {
            let mut color = q.color;
            color[3] *= opacity;
            let (clip_pos, clip_size) = match clip {
                Some(c) => ([c[0], c[1]], [c[2], c[3]]),
                None => ([0.0, 0.0], [-1.0, -1.0]),
            };
            InstanceInput {
                rect_pos: [origin[0] + q.pos.0, origin[1] + q.pos.1],
                rect_size: [q.size.0, q.size.1],
                color,
                border_color: [0.0, 0.0, 0.0, 0.0],
                radii: q.radii,
                border_width: 0.0,
                shadow_color: [0.0, 0.0, 0.0, 0.0],
                shadow_offset: [0.0, 0.0],
                shadow_blur: 0.0,
                shadow_spread: 0.0,
                shadow_inset: 0.0,
                backdrop_fade: 0.0,
                transform: crate::style::transform::mat2d_rotate(q.rotate_deg.to_radians()),
                clip_pos,
                clip_size,
                border_extra: 0.0,
                backdrop_blur: 0.0,
                glass: [0.0; 4],
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/geometry2d.rs"]
mod tests;
