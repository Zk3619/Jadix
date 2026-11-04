use crate::*;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view_projection: Mat4,
    pub position: Vec3,
}
impl Camera {
    pub fn perspective(
        position: Vec3,
        target: Vec3,
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Self {
            position,
            view_projection: Mat4::perspective_rh(fov_y, aspect, near, far)
                * Mat4::look_at_rh(position, target, Vec3::Y),
        }
    }

    pub fn from_node(scene: &Scene, pose: &Pose, index: usize, aspect: f32) -> Result<Self> {
        let projection = scene
            .nodes
            .get(index)
            .and_then(|node| node.camera)
            .ok_or_else(|| Error("node has no camera".into()))?;
        let world = *pose
            .world
            .get(index)
            .ok_or_else(|| Error("camera pose index out of range".into()))?;
        if world.determinant().abs() < 1e-10 {
            return Err(Error("camera transform is singular".into()));
        }
        let projection = match projection {
            Projection::Perspective { fov_y, near, far } => {
                Mat4::perspective_rh(fov_y, aspect, near, far)
            }
            Projection::Orthographic {
                xmag,
                ymag,
                near,
                far,
            } => Mat4::orthographic_rh(-xmag, xmag, -ymag, ymag, near, far),
        };
        Ok(Self {
            view_projection: projection * world.inverse(),
            position: world.transform_point3(Vec3::ZERO),
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight {
    position_kind: [f32; 4],
    direction_range: [f32; 4],
    color_intensity: [f32; 4],
    cone: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewUniform {
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    ambient_count: [f32; 4],
    lights: [GpuLight; 8],
}

struct GpuPrimitive {
    indices: wgpu::Buffer,
    material: wgpu::BindGroup,
}

pub struct Renderer {
    view_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    asset: Option<Arc<Scene>>,
    primitives: Vec<Vec<GpuPrimitive>>,
    depth: Option<(u32, u32, wgpu::Texture)>,
}

impl Renderer {
    /// The target format must match every view passed to render.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-3d-view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let mut material_entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for slot in 0..5 {
            material_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1 + slot * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            material_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2 + slot * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-3d-material"),
            entries: &material_entries,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jadix-3d"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-3d"),
            bind_group_layouts: &[Some(&view_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let mut pipelines = Vec::new();
        for blend in [false, true] {
            for double_sided in [false, true] {
                pipelines.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("jadix-3d"), layout: Some(&layout),
                    vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 48, step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4] })] },
                    fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState { format, blend: blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
                    primitive: wgpu::PrimitiveState { cull_mode: if double_sided { None } else { Some(wgpu::Face::Back) }, ..Default::default() },
                    depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: Some(!blend), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
                    multisample: Default::default(), multiview_mask: None, cache: None,
                }));
            }
        }
        Self {
            view_layout,
            material_layout,
            pipelines,
            asset: None,
            primitives: Vec::new(),
            depth: None,
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Arc<Scene>,
    ) -> Result<()> {
        if self
            .asset
            .as_ref()
            .is_some_and(|asset| Arc::ptr_eq(asset, scene))
        {
            return Ok(());
        }
        let mut meshes = Vec::new();
        for mesh in &scene.meshes {
            let mut primitives = Vec::new();
            for primitive in &mesh.primitives {
                if primitive.indices.len() % 3 != 0
                    || primitive
                        .indices
                        .iter()
                        .any(|index| *index as usize >= primitive.vertices.len())
                {
                    return Err(Error("invalid triangle indices".into()));
                }
                let material = &primitive.material;
                if !material.base_color.is_finite()
                    || !material.emissive.is_finite()
                    || ![
                        material.metallic,
                        material.roughness,
                        material.normal_scale,
                        material.occlusion_strength,
                    ]
                    .iter()
                    .all(|value| value.is_finite())
                {
                    return Err(Error("material factors must be finite".into()));
                }
                let maps = [
                    (material.base_color_texture, &material.sampler, true),
                    (
                        material
                            .metallic_roughness_texture
                            .as_ref()
                            .map(|map| map.image),
                        material
                            .metallic_roughness_texture
                            .as_ref()
                            .map_or(&material.sampler, |map| &map.sampler),
                        false,
                    ),
                    (
                        material.normal_texture.as_ref().map(|map| map.image),
                        material
                            .normal_texture
                            .as_ref()
                            .map_or(&material.sampler, |map| &map.sampler),
                        false,
                    ),
                    (
                        material.occlusion_texture.as_ref().map(|map| map.image),
                        material
                            .occlusion_texture
                            .as_ref()
                            .map_or(&material.sampler, |map| &map.sampler),
                        false,
                    ),
                    (
                        material.emissive_texture.as_ref().map(|map| map.image),
                        material
                            .emissive_texture
                            .as_ref()
                            .map_or(&material.sampler, |map| &map.sampler),
                        true,
                    ),
                ];
                let mut views = Vec::new();
                let mut samplers = Vec::new();
                for (image, sampler, srgb) in maps {
                    let image = image
                        .map(|index| {
                            scene
                                .images
                                .get(index)
                                .ok_or_else(|| Error("material image out of range".into()))
                        })
                        .transpose()?;
                    let (width, height, rgba) = image
                        .map(|image| (image.width, image.height, image.rgba.as_slice()))
                        .unwrap_or((1, 1, &[255, 255, 255, 255]));
                    if width == 0
                        || height == 0
                        || width > device.limits().max_texture_dimension_2d
                        || height > device.limits().max_texture_dimension_2d
                        || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4
                    {
                        return Err(Error(
                            "material image exceeds device limits or has invalid pixels".into(),
                        ));
                    }
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("jadix-3d-material-map"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: if srgb {
                            wgpu::TextureFormat::Rgba8UnormSrgb
                        } else {
                            wgpu::TextureFormat::Rgba8Unorm
                        },
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    queue.write_texture(
                        texture.as_image_copy(),
                        rgba,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(height),
                        },
                        texture.size(),
                    );
                    views.push(texture.create_view(&Default::default()));
                    samplers.push(device.create_sampler(sampler));
                }
                let uniform = [
                    [
                        material.normal_scale,
                        material.occlusion_strength,
                        if material.normal_texture.is_some() {
                            1.0
                        } else {
                            0.0
                        },
                        0.0,
                    ],
                    material.base_color.to_array(),
                    [
                        material.emissive.x,
                        material.emissive.y,
                        material.emissive.z,
                        if material.unlit { 1.0 } else { 0.0 },
                    ],
                    [
                        material.metallic,
                        material.roughness,
                        if let AlphaMode::Mask(cutoff) = material.alpha {
                            cutoff
                        } else {
                            -1.0
                        },
                        if material.alpha == AlphaMode::Blend {
                            1.0
                        } else {
                            0.0
                        },
                    ],
                ];
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("jadix-3d-material"),
                    contents: bytemuck::cast_slice(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let mut entries = vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }];
                for (slot, (view, sampler)) in views.iter().zip(&samplers).enumerate() {
                    entries.push(wgpu::BindGroupEntry {
                        binding: 1 + slot as u32 * 2,
                        resource: wgpu::BindingResource::TextureView(view),
                    });
                    entries.push(wgpu::BindGroupEntry {
                        binding: 2 + slot as u32 * 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    });
                }
                let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("jadix-3d-material"),
                    layout: &self.material_layout,
                    entries: &entries,
                });
                let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("jadix-3d-indices"),
                    contents: bytemuck::cast_slice(&primitive.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                primitives.push(GpuPrimitive { indices, material });
            }
            meshes.push(primitives);
        }
        self.primitives = meshes;
        self.asset = Some(Arc::clone(scene));
        Ok(())
    }

    /// Draw an evaluated scene into a caller-owned texture. Creates a depth buffer matching size.
    /// Skinning/morphs run on CPU; geometry is rasterized by wgpu with depth and materials.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scene: &Arc<Scene>,
        pose: &Pose,
        camera: Camera,
        clear: [f64; 4],
    ) -> Result<()> {
        self.prepare(device, queue, scene)?;
        let [width, height] = size;
        if width == 0
            || height == 0
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(Error("invalid render size".into()));
        }
        if pose.world.len() != scene.nodes.len()
            || pose.visible.len() != scene.nodes.len()
            || pose.weights.len() != scene.nodes.len()
        {
            return Err(Error("pose does not match scene".into()));
        }
        if !camera.view_projection.is_finite()
            || !camera.position.is_finite()
            || clear.iter().any(|value| !value.is_finite())
        {
            return Err(Error("camera and clear color must be finite".into()));
        }
        let mut uniform = ViewUniform {
            view_projection: camera.view_projection.to_cols_array_2d(),
            camera: camera.position.extend(1.0).to_array(),
            ambient_count: [0.16, 0.16, 0.16, 0.0],
            lights: [GpuLight {
                position_kind: [0.0; 4],
                direction_range: [0.0; 4],
                color_intensity: [0.0; 4],
                cone: [0.0; 4],
            }; 8],
        };
        let mut light_count = 0;
        for (index, node) in scene.nodes.iter().enumerate() {
            if !pose.visible[index] {
                continue;
            }
            if let Some(light) = node.light {
                if light_count == 8 {
                    return Err(Error(
                        "renderer supports at most eight visible punctual lights".into(),
                    ));
                }
                let world = pose.world[index];
                let direction = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                let (kind, cone) = match light.kind {
                    LightKind::Directional => (0.0, [0.0; 4]),
                    LightKind::Point => (1.0, [0.0; 4]),
                    LightKind::Spot { inner, outer } => (2.0, [inner.cos(), outer.cos(), 0.0, 0.0]),
                };
                uniform.lights[light_count] = GpuLight {
                    position_kind: world.transform_point3(Vec3::ZERO).extend(kind).to_array(),
                    direction_range: direction.extend(light.range.unwrap_or(0.0)).to_array(),
                    color_intensity: light.color.extend(light.intensity).to_array(),
                    cone,
                };
                light_count += 1;
            }
        }
        if light_count == 0 {
            uniform.lights[0] = GpuLight {
                position_kind: [0.0; 4],
                direction_range: [0.3, -0.8, -0.5, 0.0],
                color_intensity: [1.0, 1.0, 1.0, 2.0],
                cone: [0.0; 4],
            };
            light_count = 1;
        }
        uniform.ambient_count[3] = light_count as f32;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jadix-3d-view"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-3d-view"),
            layout: &self.view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        if self
            .depth
            .as_ref()
            .is_none_or(|(old_width, old_height, _)| *old_width != width || *old_height != height)
        {
            self.depth = Some((
                width,
                height,
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("jadix-3d-depth"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                }),
            ));
        }
        let depth = self
            .depth
            .as_ref()
            .unwrap()
            .2
            .create_view(&Default::default());
        let mut draws = Vec::new();
        for (node_index, node) in scene.nodes.iter().enumerate() {
            if !pose.visible[node_index] {
                continue;
            }
            if let Some(mesh_index) = node.mesh {
                let mesh = scene
                    .meshes
                    .get(mesh_index)
                    .ok_or_else(|| Error("mesh index out of range".into()))?;
                for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
                    if primitive.indices.is_empty() {
                        continue;
                    }
                    let vertices = scene.deformed_vertices(pose, node_index, primitive_index)?;
                    let center = vertices
                        .iter()
                        .fold(Vec3::ZERO, |sum, vertex| sum + vertex.position)
                        / vertices.len().max(1) as f32;
                    let vertices: Vec<_> = vertices
                        .iter()
                        .map(|vertex| GpuVertex {
                            position: vertex.position.to_array(),
                            normal: vertex.normal.to_array(),
                            uv: vertex.uv.to_array(),
                            color: vertex.color.to_array(),
                        })
                        .collect();
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("jadix-3d-vertices"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let blended = primitive.material.alpha == AlphaMode::Blend;
                    let pipeline =
                        usize::from(blended) * 2 + usize::from(primitive.material.double_sided);
                    draws.push((
                        blended,
                        center.distance_squared(camera.position),
                        mesh_index,
                        primitive_index,
                        pipeline,
                        buffer,
                    ));
                }
            }
        }
        draws.sort_by(|left, right| {
            left.0.cmp(&right.0).then_with(|| {
                if left.0 {
                    right.1.total_cmp(&left.1)
                } else {
                    left.1.total_cmp(&right.1)
                }
            })
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &view_group, &[]);
            for (_, _, mesh, primitive, pipeline, vertices) in &draws {
                let gpu = &self.primitives[*mesh][*primitive];
                pass.set_pipeline(&self.pipelines[*pipeline]);
                pass.set_bind_group(1, &gpu.material, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(gpu.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(
                    0..scene.meshes[*mesh].primitives[*primitive].indices.len() as u32,
                    0,
                    0..1,
                );
            }
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }
}
