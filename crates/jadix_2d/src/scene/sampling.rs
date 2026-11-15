//! Whole-scene spatial sampling, including clip and composition at each sample.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct SceneSampling {
    /// Uniform ordered grid: 1,2,4 or8 samples on each axis (up to64/pixel).
    pub axis_samples: u32,
    /// Conservative surface/mask estimate in bytes; excludes path tessellation,
    /// immutable input resources, driver and command metadata. Hard ceiling1GiB.
    pub max_surface_bytes: u64,
}
impl Default for SceneSampling {
    fn default() -> Self {
        Self {
            axis_samples: 4,
            max_surface_bytes: 512 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug)]
pub struct SampledScene2D {
    pub(crate) scene: Scene2D,
    pub(crate) options: SceneOptions,
    pub(crate) size: [u32; 2],
    pub(crate) high_size: [u32; 2],
    pub(crate) sampling: SceneSampling,
    pub(crate) grid: [u32; 2],
}
impl Scene2D {
    /// Prepare a bounded whole-scene sampling domain. Each sample retains the
    /// complete painter order/clip/layer state until a final linear resolve.
    /// Layer effects scale from output pixels to sample pixels; primitive blur
    /// already follows its local CTM and is not scaled twice.
    pub fn sampled(
        &self,
        size: [u32; 2],
        options: SceneOptions,
        sampling: SceneSampling,
    ) -> Result<SampledScene2D, SceneError> {
        self.sampled_grid(
            size,
            options,
            [sampling.axis_samples; 2],
            sampling.max_surface_bytes,
        )
    }
    /// Ordered rectangular sample grid: each axis is 1, 2, 4 or 8, allowing
    /// 1/2/4/8/16/32/64 samples per pixel. All samples retain clip/composition.
    /// This is spatial supersampling, not a native hardware/Vello MSAA pattern.
    pub fn sampled_grid(
        &self,
        size: [u32; 2],
        mut options: SceneOptions,
        grid: [u32; 2],
        max_surface_bytes: u64,
    ) -> Result<SampledScene2D, SceneError> {
        let sampling = SceneSampling {
            axis_samples: grid[0],
            max_surface_bytes,
        };
        if grid.iter().any(|n| ![1, 2, 4, 8].contains(n))
            || sampling.max_surface_bytes == 0
            || sampling.max_surface_bytes > 1024 * 1024 * 1024
        {
            return Err(SceneError::InvalidOptions);
        }
        if size.contains(&0) {
            return Err(SceneError::InvalidSize);
        }
        let high = [
            size[0]
                .checked_mul(grid[0])
                .ok_or(SceneError::InvalidSize)?,
            size[1]
                .checked_mul(grid[1])
                .ok_or(SceneError::InvalidSize)?,
        ];
        if high.iter().any(|v| *v > 1_000_000) {
            return Err(SceneError::InvalidSize);
        }
        let (mut groups, mut masks) = surface_counts(self);
        groups += u64::from(options.layer.is_some());
        masks += u64::from(
            options
                .layer
                .as_ref()
                .is_some_and(|l| l.mask.is_some() || l.clip.is_some()),
        );
        let buffers = 6u64
            .saturating_add(self.stats().nesting as u64 * 3)
            .saturating_add(groups * 4);
        let bytes = (high[0] as u64)
            .checked_mul(high[1] as u64)
            .and_then(|p| p.checked_mul(buffers * 16 + masks * 4))
            .ok_or(SceneError::ImageBudget)?;
        if bytes > sampling.max_surface_bytes {
            return Err(SceneError::ImageBudget);
        }
        let scale = Affine::scale(grid[0] as f32, grid[1] as f32);
        options.transform = scale.mul(Affine::from_array(options.transform)).array();
        if let Some(layer) = options.layer.take() {
            options.layer = Some(scale_effects(layer, grid)?);
        }
        let scene = scaled_scene(self, grid, &mut std::collections::HashMap::new())?;
        // Transform and mask preflight use a recording canvas, without drawing.
        let mut check = GpuCanvas::new_linear(high[0], high[1], None);
        check.set_point_sampling(true);
        scene.context(&check, &options)?;
        options.layer = options.layer.map(|l| map_layer(l, scale, high, true));
        Ok(SampledScene2D {
            scene,
            options,
            size,
            high_size: high,
            sampling,
            grid,
        })
    }
}
impl SampledScene2D {
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn sample_size(&self) -> [u32; 2] {
        self.high_size
    }
    /// Horizontal sample count. Equal to vertical for the original sampled API.
    pub fn axis_samples(&self) -> u32 {
        self.sampling.axis_samples
    }
    pub fn sample_grid(&self) -> [u32; 2] {
        self.grid
    }
    pub fn sample_count(&self) -> u32 {
        self.grid[0] * self.grid[1]
    }
    pub fn rasterize(&self) -> Result<Image2D, SceneError> {
        let mut canvas = RasterCanvas::new_hdr(self.high_size[0], self.high_size[1]);
        canvas.set_point_sampling(true);
        self.scene.draw(&mut canvas, self.options.clone())?;
        let [nx, ny] = self.grid;
        let mut pixels = Vec::with_capacity(self.size[0] as usize * self.size[1] as usize * 4);
        for y in 0..self.size[1] {
            for x in 0..self.size[0] {
                let mut sum = [0f64; 4];
                for sy in 0..ny {
                    for sx in 0..nx {
                        let at = (((y * ny + sy) * self.high_size[0] + x * nx + sx) * 4) as usize;
                        for (sum, value) in sum.iter_mut().zip(&canvas.buf[at..at + 4]) {
                            *sum += *value as f64 / (nx * ny) as f64;
                        }
                    }
                }
                let a = sum[3];
                if a > 0. {
                    for v in &mut sum[..3] {
                        *v /= a;
                    }
                }
                pixels.extend(sum.map(|v| v as f32));
            }
        }
        Image2D::from_linear_rgba32f(self.size[0], self.size[1], &pixels)
            .ok_or(SceneError::InvalidOptions)
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_ops(&self) -> Result<Vec<PathGpuOp>, SceneError> {
        let mut canvas = GpuCanvas::new_linear(self.high_size[0], self.high_size[1], None);
        canvas.set_point_sampling(true);
        self.scene.draw(&mut canvas, self.options.clone())?;
        if canvas.needs_cpu {
            return Err(SceneError::UnsupportedOperation);
        }
        Ok(canvas.into_ops())
    }
}
fn surface_counts(scene: &Scene2D) -> (u64, u64) {
    let mut groups = 0;
    let mut masks = 0;
    for c in &scene.0.commands {
        match c {
            Command::Push(l) => {
                groups += 1;
                if l.mask.is_some() || l.clip.is_some() {
                    masks += 1;
                }
            }
            Command::Clip(_) | Command::ClipPath(..) | Command::Mask(_) => masks += 1,
            Command::Scene(s, o) => {
                let (g, m) = surface_counts(s);
                groups += g + u64::from(o.layer.is_some());
                masks += m + u64::from(
                    o.layer
                        .as_ref()
                        .is_some_and(|v| v.clip.is_some() || v.mask.is_some()),
                );
            }
            _ => {}
        }
    }
    (groups, masks)
}
fn scale_effects(mut layer: LayerOptions, n: [u32; 2]) -> Result<LayerOptions, SceneError> {
    let s = n.map(|v| v as f32);
    for filter in &mut layer.filters {
        match filter {
            LayerFilter::Gaussian { sigma } => *sigma = [sigma[0] * s[0], sigma[1] * s[1]],
            LayerFilter::DropShadow { sigma, offset, .. }
            | LayerFilter::DropShadowOnly { sigma, offset, .. } => {
                *sigma = [sigma[0] * s[0], sigma[1] * s[1]];
                *offset = [offset[0] * s[0], offset[1] * s[1]];
            }
            LayerFilter::Offset { offset } => *offset = [offset[0] * s[0], offset[1] * s[1]],
            LayerFilter::Morphology { radius, .. } => {
                radius[0] = radius[0]
                    .checked_mul(n[0])
                    .ok_or(SceneError::InvalidOptions)?;
                radius[1] = radius[1]
                    .checked_mul(n[1])
                    .ok_or(SceneError::InvalidOptions)?;
            }
            LayerFilter::ConvolveMatrix(c) => {
                c.unit_length = [c.unit_length[0] * s[0], c.unit_length[1] * s[1]]
            }
            _ => {}
        }
        if !filter.valid() {
            return Err(SceneError::InvalidOptions);
        }
    }
    Ok(layer)
}
fn scaled_scene(
    scene: &Scene2D,
    n: [u32; 2],
    cache: &mut std::collections::HashMap<usize, Scene2D>,
) -> Result<Scene2D, SceneError> {
    let key = Arc::as_ptr(&scene.0) as usize;
    if let Some(s) = cache.get(&key) {
        return Ok(s.clone());
    }
    let mut commands = scene.0.commands.clone();
    for cmd in &mut commands {
        match cmd {
            Command::Push(layer) => *layer = scale_effects(layer.clone(), n)?,
            Command::Scene(child, options) => {
                *child = scaled_scene(child, n, cache)?;
                if let Some(l) = options.layer.take() {
                    options.layer = Some(scale_effects(l, n)?);
                }
            }
            _ => {}
        }
    }
    let result = Scene2D(Arc::new(SceneData {
        point_sample_masks: true,
        size: scene.0.size,
        commands,
        stats: scene.0.stats,
        max_replay_mask_bytes: scene.0.max_replay_mask_bytes,
    }));
    cache.insert(key, result.clone());
    Ok(result)
}
