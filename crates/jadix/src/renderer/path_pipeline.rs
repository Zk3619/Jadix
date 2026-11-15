//! Framework texture cache and submission adapter for jadix_2d.
use crate::app::image_registry::GpuImageTexture;
use crate::primitives::path::{take_path_jobs, PathGpuJob, PathGpuOp};
use crate::renderer::TexturedQuadPipeline;
pub use jadix_2d::gpu::path_msaa_sample_count;
use std::collections::HashMap;

pub struct PathGpu {
    core: jadix_2d::gpu::PathGpu,
    target_size: HashMap<String, (u32, u32)>,
}
impl PathGpu {
    pub(crate) fn retain_targets(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.target_size.retain(|key, _| keep(key));
    }

    pub fn try_new(device: &wgpu::Device, sample_count: u32) -> Option<Self> {
        let mut core = jadix_2d::gpu::PathGpu::try_new(device, sample_count)?;
        core.set_aa_quality(match std::env::var("JADIX_GPU_2D_AA").as_deref() {
            Ok("area") => jadix_2d::gpu::AaQuality::Area,
            Ok("high") => jadix_2d::gpu::AaQuality::High,
            Ok("reference") => jadix_2d::gpu::AaQuality::Reference,
            _ => jadix_2d::gpu::AaQuality::Balanced,
        });
        core.set_tight_layer_bounds(
            std::env::var("JADIX_GPU_2D_TIGHT_LAYERS").as_deref() == Ok("1"),
        );
        Some(Self {
            core,
            target_size: HashMap::new(),
        })
    }
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
    ) -> bool {
        self.flush_jobs(device, queue, textured, images, take_path_jobs())
    }

    pub(crate) fn flush_matching(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        owns: impl FnMut(&str) -> bool,
    ) -> bool {
        self.flush_jobs(
            device,
            queue,
            textured,
            images,
            crate::primitives::path::take_path_jobs_matching(owns),
        )
    }

    fn flush_jobs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        jobs: Vec<PathGpuJob>,
    ) -> bool {
        let mut last: HashMap<String, PathGpuJob> = HashMap::new();
        for job in jobs {
            last.insert(job.key.clone(), job);
        }
        let mut any = false;
        for job in last.into_values() {
            if self.render_job(device, queue, textured, images, job) {
                any = true;
            }
        }
        if any {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        any
    }

    fn render_job(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        job: PathGpuJob,
    ) -> bool {
        let always = job.force_redraw || job.key.starts_with("jadix_canvas:");
        let w = job.width.max(2);
        let h = job.height.max(2);
        if self.target_size.get(&job.key) == Some(&(w, h))
            && images.contains_key(&job.key)
            && !always
        {
            return false;
        }
        let ops = job.ops.into_ops();
        if ops.iter().all(PathGpuOp::is_empty) && !always {
            return false;
        }
        self.ensure_target(device, textured, images, &job.key, w, h);
        let view = images[&job.key]._texture.create_view(&Default::default());
        if !self.core.render(device, queue, &view, [w, h], &ops) {
            crate::renderer::geometry2d::disable_gpu2d_runtime();
            return false;
        }
        if crate::primitives::path::take_path_rt_readback_request(&job.key) {
            if let Some(gpu_img) = images.get(&job.key) {
                let enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("jadix-path-readback"),
                });
                if let Some(img) = crate::renderer::offscreen::readback_texture(
                    device,
                    queue,
                    &gpu_img._texture,
                    w,
                    h,
                    enc,
                ) {
                    crate::primitives::path::store_path_rt_readback(
                        job.key.clone(),
                        img.width,
                        img.height,
                        img.pixels,
                    );
                }
            }
        }
        true
    }

    fn ensure_target(
        &mut self,
        device: &wgpu::Device,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        key: &str,
        w: u32,
        h: u32,
    ) {
        let stale = match self.target_size.get(key) {
            Some(&(cw, ch)) => cw != w || ch != h,
            None => true,
        };
        if !stale && images.contains_key(key) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-path-color"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = textured.create_texture_bind_group(device, &view);
        images.insert(
            key.to_string(),
            GpuImageTexture {
                _texture: texture,
                bind_group,
            },
        );
        self.target_size.insert(key.to_string(), (w, h));
    }
}
#[cfg(test)]
#[path = "path_pipeline/tests/mod.rs"]
mod tests;
