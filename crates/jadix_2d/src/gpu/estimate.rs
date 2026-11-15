//! Checked estimates for prepared Jadix payloads, not Vello compute bump buffers.
use super::*;
use crate::{PathGpuFill, PreparedSceneGpu, Scene2D, SceneError, SceneOptions};
/// Geometry/resource planning counters after CPU replay and tessellation. Byte
/// counts describe payloads, not allocator/driver high water. Layer surfaces are
/// conservative full-target FP16 domains; MSAA/SSAA expansion is reported separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneMemoryEstimate {
    pub operations: u64,
    pub triangles: u64,
    /// CPU command/triangulation payload estimate (including temporary image quads).
    pub geometry_bytes: u64,
    /// Upper bound for unindexed standard-renderer vertex uploads.
    pub vertex_upload_bytes: u64,
    pub area_triangle_references: u64,
    pub area_geometry_bytes: u64,
    pub area_pixel_triangle_work_upper_bound: u64,
    pub image_bytes: u64,
    pub layer_surface_bytes: u64,
    pub layers: u64,
    pub masks: u64,
}
impl SceneMemoryEstimate {
    /// Sum of the listed CPU/GPU payload estimates; excludes pipeline/driver,
    /// font caches, source texture imports, staging copies and allocator capacity.
    pub fn payload_bytes(&self) -> Option<u64> {
        self.geometry_bytes
            .checked_add(self.vertex_upload_bytes)?
            .checked_add(self.area_geometry_bytes)?
            .checked_add(self.image_bytes)?
            .checked_add(self.layer_surface_bytes)
    }
    /// Additional conservative target/coverage/depth payload at selected quality.
    pub fn target_bytes(size: [u32; 2], quality: AaQuality, sample_count: u32) -> Option<u64> {
        if size.contains(&0) || ![1, 4].contains(&sample_count) {
            return None;
        }
        let scale = match quality {
            AaQuality::Area | AaQuality::Aliased => 1,
            AaQuality::Balanced => {
                if sample_count == 1 {
                    2
                } else {
                    1
                }
            }
            AaQuality::High => 2,
            AaQuality::Reference => 4,
        };
        let pixels = (size[0] as u64)
            .checked_mul(size[1] as u64)?
            .checked_mul(scale * scale)?;
        // FP16 coverage + multisample color and depth/stencil, plus final FP16 output.
        pixels
            .checked_mul(8 + 12 * sample_count as u64)?
            .checked_add(
                (size[0] as u64)
                    .checked_mul(size[1] as u64)?
                    .checked_mul(8)?,
            )
    }
}
impl Scene2D {
    /// CPU-only preflight/replay/tessellation with an explicit target and instance
    /// transform. No GPU allocations; unlike Vello this does not estimate its bump ABI.
    pub fn bump_estimate(
        &self,
        size: [u32; 2],
        options: SceneOptions,
    ) -> Result<SceneMemoryEstimate, SceneError> {
        PreparedSceneGpu::new(self, size, options)?.bump_estimate()
    }
}
impl PreparedSceneGpu {
    pub fn bump_estimate(&self) -> Result<SceneMemoryEstimate, SceneError> {
        let mut result = SceneMemoryEstimate::default();
        let mut images = std::collections::HashSet::new();
        estimate(self.ops(), self.size(), &mut result, &mut images)?;
        Ok(result)
    }
}
fn add(slot: &mut u64, value: u64) -> Result<(), SceneError> {
    *slot = slot.checked_add(value).ok_or(SceneError::ExpandedBudget)?;
    Ok(())
}
fn image(
    image: &Image2D,
    mask: bool,
    result: &mut SceneMemoryEstimate,
    images: &mut std::collections::HashSet<(u64, bool)>,
) -> Result<(), SceneError> {
    if images.insert((image.0.id, mask)) {
        for level in 0..if mask { 1 } else { image.mip_level_count() } {
            let size = image.mip_level_size(level).unwrap();
            add(
                &mut result.image_bytes,
                size[0] as u64 * size[1] as u64 * if mask { 1 } else { 8 },
            )?;
        }
    }
    Ok(())
}
fn fill(
    f: &PathGpuFill,
    size: [u32; 2],
    result: &mut SceneMemoryEstimate,
) -> Result<(), SceneError> {
    triangles(&f.stencil, size, f.clip, result)?;
    if !f.direct {
        add(
            &mut result.vertex_upload_bytes,
            2 * 3 * std::mem::size_of::<PathVert>() as u64,
        )?;
    }
    Ok(())
}
fn triangles(
    tris: &[crate::PathTri],
    size: [u32; 2],
    clip: Option<[f32; 4]>,
    result: &mut SceneMemoryEstimate,
) -> Result<(), SceneError> {
    add(&mut result.triangles, tris.len() as u64)?;
    add(
        &mut result.vertex_upload_bytes,
        tris.len() as u64 * 3 * std::mem::size_of::<PathVert>() as u64,
    )?;
    add(
        &mut result.geometry_bytes,
        tris.len() as u64 * std::mem::size_of::<crate::PathTri>() as u64,
    )?;
    let clip = clip.unwrap_or([0., 0., size[0] as f32, size[1] as f32]);
    for t in tris {
        let points = [t.a, t.b, t.c];
        if points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
            return Err(SceneError::InvalidTransform);
        }
        let x0 = points
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .max(clip[0])
            .max(0.);
        let y0 = points
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .max(clip[1])
            .max(0.);
        let x1 = points
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip[0] + clip[2])
            .min(size[0] as f32);
        let y1 = points
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip[1] + clip[3])
            .min(size[1] as f32);
        if x1 <= x0 || y1 <= y0 {
            add(&mut result.area_triangle_references, 1)?;
            continue;
        }
        let tiles = ((x1 / 16.).ceil() as u64 - (x0 / 16.).floor() as u64)
            * ((y1 / 16.).ceil() as u64 - (y0 / 16.).floor() as u64);
        add(&mut result.area_triangle_references, tiles.max(1))?;
    }
    add(
        &mut result.area_pixel_triangle_work_upper_bound,
        (size[0] as u64 * size[1] as u64)
            .checked_mul(tris.len() as u64)
            .ok_or(SceneError::ExpandedBudget)?,
    )?;
    result.area_geometry_bytes = result
        .area_triangle_references
        .checked_mul(48)
        .ok_or(SceneError::ExpandedBudget)?;
    Ok(())
}
fn estimate(
    ops: &[PathGpuOp],
    size: [u32; 2],
    r: &mut SceneMemoryEstimate,
    images: &mut std::collections::HashSet<(u64, bool)>,
) -> Result<(), SceneError> {
    for op in ops {
        add(&mut r.operations, 1)?;
        add(
            &mut r.geometry_bytes,
            std::mem::size_of::<PathGpuOp>() as u64,
        )?;
        if let Some(mask) = op.mask() {
            add(&mut r.masks, 1)?;
            image(mask, true, r, images)?;
        }
        match op {
            PathGpuOp::Fill(f) => fill(f, size, r)?,
            PathGpuOp::Mesh(m) => triangles(&m.tris, size, m.clip, r)?,
            PathGpuOp::Pattern(p) => {
                fill(&p.fill, size, r)?;
                image(&p.image, false, r, images)?;
            }
            PathGpuOp::Image(i) => {
                let tri = |a, b, c| crate::PathTri {
                    a: i.corners[a],
                    b: i.corners[b],
                    c: i.corners[c],
                    color: [1.; 4],
                };
                triangles(&[tri(0, 1, 2), tri(0, 2, 3)], size, i.clip, r)?;
                image(&i.image, false, r, images)?;
            }
            PathGpuOp::Layer(l) => {
                add(&mut r.layers, 1)?;
                add(
                    &mut r.layer_surface_bytes,
                    size[0] as u64
                        * size[1] as u64
                        * 8
                        * (3 + if l.filters.is_empty() { 0 } else { 3 }),
                )?;
                if let Some(mask) = &l.mask {
                    add(&mut r.masks, 1)?;
                    image(mask, true, r, images)?;
                }
                estimate(&l.ops, size, r, images)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_estimate_checks_overflow_and_invalid_shapes() {
        assert_eq!(
            SceneMemoryEstimate::target_bytes([0, 1], AaQuality::Area, 1),
            None
        );
        assert_eq!(
            SceneMemoryEstimate::target_bytes([1, 1], AaQuality::Area, 2),
            None
        );
        assert_eq!(
            SceneMemoryEstimate::target_bytes([u32::MAX; 2], AaQuality::Area, 1),
            None
        );
        assert_eq!(
            SceneMemoryEstimate::target_bytes([u32::MAX; 2], AaQuality::Reference, 4),
            None
        );
        assert_eq!(
            SceneMemoryEstimate::target_bytes([10, 20], AaQuality::Area, 1),
            Some(5600)
        );
        let estimate = SceneMemoryEstimate {
            geometry_bytes: u64::MAX,
            vertex_upload_bytes: 1,
            ..Default::default()
        };
        assert_eq!(estimate.payload_bytes(), None);
    }
    #[test]
    fn repeated_image_resource_is_counted_once() {
        let image = Image2D::from_rgba8(4, 4, &[255; 64])
            .unwrap()
            .with_mipmaps(Default::default())
            .unwrap();
        let scene = Scene2D::record([32, 32], |c| {
            c.draw_image(&crate::CanvasImageSource::Image(&image), 0., 0., 4., 4.);
            c.draw_image(&crate::CanvasImageSource::Image(&image), 8., 8., 4., 4.);
        })
        .unwrap();
        let estimate = scene
            .bump_estimate([32, 32], SceneOptions::default())
            .unwrap();
        assert_eq!(estimate.image_bytes, (16 + 4 + 1) * 8);
        assert_eq!(estimate.triangles, 4);
        assert!(estimate.vertex_upload_bytes > 0);
        assert!(estimate.area_triangle_references >= estimate.triangles);
    }
}
