use super::*;
use crate::{assets::RuntimeModel, core::Matrix44, moc3::Moc3DrawableMesh};

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("{0}")]
    Texture(#[from] WgpuTextureError),
    #[error("{0}")]
    Clipping(#[from] WgpuClippingLayoutError),
    #[error("{0}")]
    Update(#[from] WgpuMeshUpdateError),
    #[error("{0}")]
    Render(#[from] WgpuRenderError),
    #[error("empty or invalid drawable mesh")]
    Empty,
    #[error("invalid drawable data or transform")]
    Invalid,
}

/// Persistent GPU resources: textures are uploaded once, vertices/masks updated per frame.
pub struct GpuModel {
    meshes: WgpuMeshBuffers,
    textures: Vec<WgpuTexture>,
    clipping: WgpuClippingResources,
    mask: WgpuMaskRenderTarget,
    transform: WgpuTransform,
}
impl GpuModel {
    pub fn new(
        renderer: &WgpuLive2dRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &RuntimeModel,
    ) -> Result<Self, GpuError> {
        Self::from_drawables(
            renderer,
            device,
            queue,
            model.runtime().meshes(),
            model.textures(),
        )
    }

    pub fn from_drawables(
        renderer: &WgpuLive2dRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        drawables: &[Moc3DrawableMesh],
        textures: &[crate::assets::DecodedTexture],
    ) -> Result<Self, GpuError> {
        validate(drawables, textures.len())?;
        let meshes = WgpuMeshBuffers::from_drawables(device, drawables).ok_or(GpuError::Empty)?;
        let textures = textures
            .iter()
            .map(|texture| {
                renderer.create_rgba8_texture(
                    device,
                    queue,
                    texture.width(),
                    texture.height(),
                    texture.rgba(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let clipping = renderer.create_clipping_resources(device, &plan(&meshes)?)?;
        Ok(Self {
            meshes,
            textures,
            clipping,
            mask: renderer.create_mask_render_target(device, 512)?,
            transform: renderer.create_transform(device, &Matrix44::identity()),
        })
    }
    pub fn update(
        &mut self,
        renderer: &WgpuLive2dRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Moc3DrawableMesh],
        matrix: &Matrix44,
    ) -> Result<(), GpuError> {
        validate(meshes, self.textures.len())?;
        if matrix.as_slice().iter().any(|value| !value.is_finite()) {
            return Err(GpuError::Invalid);
        }
        self.meshes.update_drawables(queue, meshes)?;
        let plan = plan(&self.meshes)?;
        if !renderer.update_clipping_resources(queue, &mut self.clipping, &plan)? {
            self.clipping = renderer.create_clipping_resources(device, &plan)?;
        }
        self.transform.update_matrix(queue, matrix);
        Ok(())
    }
    /// Encodes clipping masks then color; the caller submits the encoder.
    pub fn encode(
        &self,
        renderer: &WgpuLive2dRenderer,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clear: Option<wgpu::Color>,
    ) -> Result<u32, GpuError> {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("live2d.mask"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.mask.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.draw_masks_with_textures(
                &mut pass,
                &self.meshes,
                &self.clipping,
                &self.textures,
            )?;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("live2d.color"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        Ok(renderer.draw_with_textures_clipping_and_transform(
            &mut pass,
            &self.meshes,
            &self.textures,
            &self.clipping,
            &self.mask,
            &self.transform,
        )?)
    }
}
fn plan(meshes: &WgpuMeshBuffers) -> Result<WgpuClippingPlan, WgpuClippingLayoutError> {
    let mut plan = WgpuClippingPlan::from_mesh_buffers(meshes);
    plan.assign_single_texture_layouts()?;
    plan.prepare_single_texture_masks(meshes)?;
    Ok(plan)
}

fn validate(meshes: &[Moc3DrawableMesh], texture_count: usize) -> Result<(), GpuError> {
    if meshes.is_empty() {
        return Err(GpuError::Empty);
    }
    for mesh in meshes {
        if mesh.texture_index() < 0
            || mesh.texture_index() as usize >= texture_count
            || !mesh.opacity().is_finite()
            || !mesh.draw_order().is_finite()
            || mesh
                .multiply_color()
                .into_iter()
                .chain(mesh.screen_color())
                .any(|value| !value.is_finite())
            || mesh.vertices().iter().any(|vertex| {
                vertex
                    .position()
                    .into_iter()
                    .chain(vertex.uv())
                    .any(|value| !value.is_finite())
            })
            || mesh.indices().len() % 3 != 0
            || mesh
                .indices()
                .iter()
                .any(|index| *index as usize >= mesh.vertices().len())
            || mesh
                .masks()
                .iter()
                .any(|index| *index < 0 || *index as usize >= meshes.len())
        {
            return Err(GpuError::Invalid);
        }
    }
    Ok(())
}
