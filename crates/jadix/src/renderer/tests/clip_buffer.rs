use crate::renderer::{InstanceInput, JadixPipeline};
use crate::style::{ClipFillRule, GpuClipPoly};
use bytemuck::Zeroable;
use wgpu::util::DeviceExt;

#[test]
fn texture_clips_render_without_storage_buffers_across_texture_rows() {
    let instance = crate::renderer::wgpu_compat::instance(wgpu::Backends::DX12);
    let adapter = crate::utils::block_on::block_on(instance.request_adapter(&Default::default()))
        .expect("DX12 adapter");
    let (device, queue) = crate::utils::block_on::block_on(adapter.request_device(
        &crate::renderer::wgpu_compat::device_desc(
            None,
            wgpu::Limits::downlevel_webgl2_defaults(),
            Default::default(),
        ),
    ))
    .unwrap();
    device.on_uncaptured_error(std::sync::Arc::new(|error| {
        panic!("texture clip validation: {error}")
    }));
    let mut pipeline =
        JadixPipeline::from_format(&device, wgpu::TextureFormat::Rgba8UnormSrgb, 32, 32);
    let mut instances = vec![
        InstanceInput {
            rect_size: [32.0; 2],
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 1.0,
            ..InstanceInput::zeroed()
        };
        66
    ];
    instances[65].color = [1.0, 0.0, 0.0, 1.0];
    let mut clip = GpuClipPoly::from_world_points(
        &[[4.0, 4.0], [28.0, 4.0], [28.0, 28.0], [4.0, 28.0]],
        ClipFillRule::NonZero,
    );
    let second = GpuClipPoly::from_world_points(
        &[[8.0, 8.0], [24.0, 8.0], [24.0, 24.0], [8.0, 24.0]],
        ClipFillRule::EvenOdd,
    );
    clip.count2 = second.count;
    clip.fill2 = second.fill;
    clip.pts2 = second.pts;
    clip.round_rect = [10.0, 10.0, 12.0, 12.0];
    clip.round_radii = [2.0; 4];
    clip.round_rect2 = [0.0, 0.0, 32.0, 32.0];
    clip.round_radii2 = [1.0; 4];
    let mut clips = vec![GpuClipPoly::NONE; 66];
    clips[65] = clip;
    pipeline.ensure_clip_polys(&device, &queue, &clips);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("texture clip instances"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("texture clip output"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group_clip_poly, &[]);
        pass.set_bind_group(1, pipeline.dummy_backdrop(), &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, 0..66);
    }
    let image =
        crate::renderer::offscreen::readback_texture(&device, &queue, &texture, 32, 32, encoder)
            .unwrap();
    assert_eq!(image.pixel(16, 16), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(6, 16), Some([255; 4]));
    assert_eq!(image.pixel(9, 16), Some([255; 4]));
    assert_eq!(image.pixel(10, 10), Some([255; 4]));
}
