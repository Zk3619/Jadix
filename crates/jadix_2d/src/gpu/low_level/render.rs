//! Staged scene preparation and retained GPU output, separate from generic compute.
use super::*;
use crate::gpu::{PathGpu, RenderStats, RendererOptions, SceneMemoryEstimate};
use crate::{PreparedSceneGpu, Scene2D, SceneError, SceneOptions};

/// Retained CPU-prepared geometry and GPU fine drawing state. This accepts Jadix
/// scenes, not Vello Encoding, Resolver, bump buffers or shader IDs.
pub struct Render {
    prepared: PreparedSceneGpu,
    renderer: PathGpu,
    output: wgpu::Texture,
    capture: Option<CapturedScene>,
}
#[derive(Clone, Debug)]
pub struct CapturedScene {
    pub prepared: PreparedSceneGpu,
    pub estimate: SceneMemoryEstimate,
}
impl Render {
    /// Replay/flatten/tessellate once, allocate a retained FP16 output, and defer
    /// leaf PSO compilation until record_fine. CPU-only scene operations fail.
    pub fn new(
        device: &wgpu::Device,
        scene: &Scene2D,
        size: [u32; 2],
        options: SceneOptions,
        renderer_options: RendererOptions,
        capture: bool,
    ) -> Result<Self, SceneError> {
        let prepared = PreparedSceneGpu::new(scene, size, options)?;
        let estimate = prepared.bump_estimate()?;
        let renderer = PathGpu::try_new_with_options(
            device,
            1,
            wgpu::TextureFormat::Rgba16Float,
            renderer_options,
        )
        .ok_or(SceneError::UnsupportedCanvas)?;
        if size
            .iter()
            .any(|n| *n > device.limits().max_texture_dimension_2d)
        {
            return Err(SceneError::InvalidSize);
        }
        let output =
            jadix_renderer::wgpu_compat::try_gpu_init(device, "staged scene output", || {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("staged-scene-output"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            })
            .ok_or(SceneError::ImageBudget)?;
        let capture = capture.then(|| CapturedScene {
            prepared: prepared.clone(),
            estimate,
        });
        Ok(Self {
            prepared,
            renderer,
            output,
            capture,
        })
    }
    pub fn out_image(&self) -> &wgpu::Texture {
        &self.output
    }
    pub fn prepared(&self) -> &PreparedSceneGpu {
        &self.prepared
    }
    pub fn bump_estimate(&self) -> Result<SceneMemoryEstimate, SceneError> {
        self.prepared.bump_estimate()
    }
    pub fn take_captured_scene(&mut self) -> Option<CapturedScene> {
        self.capture.take()
    }
    pub fn renderer_mut(&mut self) -> &mut PathGpu {
        &mut self.renderer
    }
    pub fn last_stats(&self) -> RenderStats {
        self.renderer.last_stats()
    }
    /// Encode once, then submit the encoder before encoding another frame on this
    /// Render. Its retained uniforms/uploads must not be overwritten in flight.
    /// The caller may encode later compute/render operations reading out_image.
    pub fn record_fine(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        premultiplied: bool,
    ) -> Result<(), SceneError> {
        let view = self.output.create_view(&Default::default());
        if self.renderer.encode_impl(
            device,
            queue,
            encoder,
            &view,
            self.prepared.size(),
            self.prepared.ops(),
            premultiplied,
        ) {
            Ok(())
        } else {
            Err(SceneError::UnsupportedCanvas)
        }
    }
}
