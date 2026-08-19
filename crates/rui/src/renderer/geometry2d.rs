//! GPU 2D path backend: flatten curves → tessellate quads → [`InstanceInput`].
//!
//! CPU coverage raster (`paint_path_premul`) remains the default product path and
//! the golden/headless fallback. Enable GPU quads with `RUI_GPU_2D=1`.

use crate::primitives::path::{tessellate_path, PathCommand, PathQuad, PathStyle};
use crate::renderer::InstanceInput;

/// `RUI_GPU_2D=1` opts Chart/Svg/Canvas path leaves into tessellated GPU quads.
pub fn gpu2d_enabled() -> bool {
    static CELL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CELL.get_or_init(|| {
        std::env::var("RUI_GPU_2D")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Flatten + tessellate a path into SDF-rect instances (same pipeline as UI chrome).
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
                transform: crate::style::transform::mat2d_rotate(q.rotate_deg.to_radians()),
                clip_pos,
                clip_size,
                border_extra: 0.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::path::{polyline, PathStyle};

    #[test]
    fn stroke_polyline_emits_instances() {
        let cmds = polyline(&[(0.0, 0.0), (40.0, 0.0), (40.0, 20.0)]);
        let style = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 2.0);
        let inst = path_to_instances(&cmds, &style, [10.0, 5.0], None, 1.0);
        assert!(!inst.is_empty(), "tessellated stroke must emit quads");
        assert!(inst.iter().all(|i| i.rect_pos[0] >= 10.0 - 4.0));
    }
}
