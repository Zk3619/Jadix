//! Explicit finite offscreen domains. A halo retains input outside the output
//! rectangle before filters run; clipping to the visible canvas happens later.
use crate::*;

#[derive(Clone, Debug)]
pub struct SceneRegion {
    pub origin: [i32; 2],
    pub size: [u32; 2],
    /// Explicit filter support budget in device pixels, not an inferred bound.
    pub halo: u32,
    pub filters: Vec<LayerFilter>,
}
impl SceneRegion {
    pub fn padded_size(&self) -> Option<[u32; 2]> {
        let padding = self.halo.checked_mul(2)?;
        let size = [
            self.size[0].checked_add(padding)?,
            self.size[1].checked_add(padding)?,
        ];
        if self.size.contains(&0)
            || size.iter().any(|v| *v > 8192)
            || size[0] as u64 * size[1] as u64 > 16_777_216
            || self.origin.iter().any(|v| v.unsigned_abs() > 1_000_000)
        {
            return None;
        }
        Some(size)
    }
    fn record(&self, scene: &Scene2D) -> Result<Scene2D, SceneError> {
        let [w, h] = self.padded_size().ok_or(SceneError::InvalidSize)?;
        let transform = [
            1.,
            0.,
            0.,
            1.,
            self.halo as f32 - self.origin[0] as f32,
            self.halo as f32 - self.origin[1] as f32,
        ];
        let mut error = None;
        let recorded = Scene2D::record([w, h], |c| {
            if let Err(e) = scene.draw(
                c,
                SceneOptions {
                    transform,
                    layer: Some(LayerOptions {
                        filters: self.filters.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ) {
                error = Some(e);
            }
        })?;
        if let Some(error) = error {
            return Err(error);
        }
        Ok(recorded)
    }
    /// Filter a compact GPU domain, register it with the renderer, and return a
    /// crop-aware image that can draw into later GPU scenes. CPU fallback is
    /// transparent; use rasterize when a CPU bitmap is required. The renderer's
    /// selected format controls whether HDR values are retained.
    #[cfg(feature = "gpu")]
    pub fn render_gpu(
        &self,
        scene: &Scene2D,
        renderer: &mut gpu::PathGpu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<RegionImage, gpu::GpuImageError> {
        let prepared = self.prepare_gpu(scene).map_err(gpu::GpuImageError::Scene)?;
        let image = renderer.render_to_image(device, queue, &prepared)?;
        Ok(RegionImage {
            image,
            source_rect: [self.halo, self.halo, self.size[0], self.size[1]],
            origin: self.origin,
        })
    }
    /// Rasterize just the requested padded domain into an immutable float image.
    pub fn rasterize(&self, scene: &Scene2D) -> Result<RegionImage, SceneError> {
        let size = self.padded_size().ok_or(SceneError::InvalidSize)?;
        let recorded = self.record(scene)?;
        let mut canvas = RasterCanvas::new_hdr(size[0], size[1]);
        recorded.draw(&mut canvas, SceneOptions::default())?;
        let pixels = canvas.into_linear_rgba32f();
        let straight = pixels
            .chunks_exact(4)
            .flat_map(|p| {
                if p[3] > 0. {
                    [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]
                } else {
                    [0.; 4]
                }
            })
            .collect::<Vec<_>>();
        let image = Image2D::from_linear_rgba32f(size[0], size[1], &straight)
            .ok_or(SceneError::InvalidOptions)?;
        Ok(RegionImage {
            image,
            source_rect: [self.halo, self.halo, self.size[0], self.size[1]],
            origin: self.origin,
        })
    }
    /// GPU rendering is confined to padded_size(), independent of the final
    /// canvas dimensions. The output's visible texel crop is `[halo,halo,size]`.
    #[cfg(feature = "gpu")]
    pub fn prepare_gpu(&self, scene: &Scene2D) -> Result<PreparedSceneGpu, SceneError> {
        PreparedSceneGpu::new(
            &self.record(scene)?,
            self.padded_size().ok_or(SceneError::InvalidSize)?,
            SceneOptions::default(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct RegionImage {
    pub image: Image2D,
    pub source_rect: [u32; 4],
    pub origin: [i32; 2],
}
impl RegionImage {
    pub fn draw<C: Canvas2D + ?Sized>(&self, c: &mut C) {
        c.draw_image(
            &CanvasImageSource::Region {
                image: &self.image,
                rect: self.source_rect,
            },
            self.origin[0] as f32,
            self.origin[1] as f32,
            self.source_rect[2] as f32,
            self.source_rect[3] as f32,
        );
    }
}

/// A tessellated scene snapshot: reusing it avoids scene replay, font work and
/// geometry tessellation. Transform changes require preparing another snapshot.
#[cfg(feature = "gpu")]
#[derive(Clone, Debug)]
pub struct PreparedSceneGpu {
    size: [u32; 2],
    ops: std::sync::Arc<Vec<PathGpuOp>>,
}
#[cfg(feature = "gpu")]
impl PreparedSceneGpu {
    pub fn new(scene: &Scene2D, size: [u32; 2], options: SceneOptions) -> Result<Self, SceneError> {
        if size.contains(&0) || size.iter().any(|v| *v > 8192) {
            return Err(SceneError::InvalidSize);
        }
        let mut canvas = GpuCanvas::new_linear(size[0], size[1], None);
        scene.draw(&mut canvas, options)?;
        if canvas.needs_cpu {
            return Err(SceneError::UnsupportedCanvas);
        }
        Ok(Self {
            size,
            ops: std::sync::Arc::new(canvas.into_ops()),
        })
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn ops(&self) -> &[PathGpuOp] {
        &self.ops
    }
}
