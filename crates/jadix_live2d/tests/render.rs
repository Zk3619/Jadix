#![cfg(feature = "gpu-tests")]
use jadix_live2d::{
    assets::DecodedTexture,
    core::Matrix44,
    moc3::{Moc3DrawableMesh, Moc3DrawableVertex},
    render::wgpu::{GpuModel, WgpuLive2dRenderer},
};

fn quad(
    left: f32,
    right: f32,
    texture: i32,
    flags: u8,
    opacity: f32,
    masks: Vec<i32>,
) -> Moc3DrawableMesh {
    Moc3DrawableMesh::from_parts(
        texture,
        flags,
        opacity,
        texture as f32,
        [[left, -1.0], [right, -1.0], [right, 1.0], [left, 1.0]]
            .map(|position| Moc3DrawableVertex::new(position, [0.5, 0.5]))
            .to_vec(),
        vec![0, 1, 2, 0, 2, 3],
        masks,
    )
}

#[test]
fn gpu_masks_blends_deformation_and_validation() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("GPU adapter required for gpu-tests");
    eprintln!("Live2D test adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let renderer = WgpuLive2dRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let textures = [
        DecodedTexture::new(1, 1, vec![255, 0, 0, 255]),
        DecodedTexture::new(1, 1, vec![0, 0, 255, 255]),
    ];
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("live2d-pixel-test"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let render = |model: &GpuModel, clear| {
        let mut encoder = device.create_command_encoder(&Default::default());
        model
            .encode(&renderer, &mut encoder, &view, Some(clear))
            .unwrap();
        queue.submit([encoder.finish()]);
        read_pixels(&device, &queue, &target)
    };
    for (flags, expected) in [(0, [128, 128, 0]), (1, [128, 255, 0]), (2, [0, 128, 0])] {
        let mut model = GpuModel::from_drawables(
            &renderer,
            &device,
            &queue,
            &[quad(-1.0, 1.0, 0, flags, 0.5, vec![])],
            &textures,
        )
        .unwrap();
        let pixels = render(&model, wgpu::Color::GREEN);
        let pixel = &pixels[(32 * 64 + 32) * 4..][..4];
        for (actual, expected) in pixel[..3].iter().zip(expected) {
            assert!(
                (i32::from(*actual) - expected).abs() <= 1,
                "blend {flags}: {pixel:?}"
            );
        }
        model
            .update(
                &renderer,
                &device,
                &queue,
                &[quad(0.0, 1.0, 0, flags, 0.5, vec![])],
                &Matrix44::identity(),
            )
            .unwrap();
        let moved = render(&model, wgpu::Color::TRANSPARENT);
        assert_eq!(&moved[(32 * 64 + 16) * 4..][..4], &[0, 0, 0, 0]);
        assert!(model
            .update(
                &renderer,
                &device,
                &queue,
                &[quad(0.0, 1.0, 20, flags, 0.5, vec![])],
                &Matrix44::identity()
            )
            .is_err());
    }
    for inverted in [false, true] {
        let meshes = [
            quad(-1.0, 0.0, 0, 0, 1.0, vec![]),
            quad(-1.0, 1.0, 1, if inverted { 8 } else { 0 }, 1.0, vec![0]),
        ];
        let model =
            GpuModel::from_drawables(&renderer, &device, &queue, &meshes, &textures).unwrap();
        let pixels = render(&model, wgpu::Color::TRANSPARENT);
        let left = &pixels[(32 * 64 + 16) * 4..][..4];
        let right = &pixels[(32 * 64 + 48) * 4..][..4];
        assert_eq!(
            left,
            if inverted {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            }
        );
        assert_eq!(
            right,
            if inverted {
                &[0, 0, 255, 255]
            } else {
                &[0, 0, 0, 0]
            }
        );
    }
    assert!(renderer
        .create_rgba8_texture(
            &device,
            &queue,
            device.limits().max_texture_dimension_2d + 1,
            1,
            &[]
        )
        .is_err());
    assert!(GpuModel::from_drawables(
        &renderer,
        &device,
        &queue,
        &[quad(-1.0, 1.0, 0, 0, 1.0, vec![9])],
        &textures
    )
    .is_err());
    assert!(pollster::block_on(errors.pop()).is_none());
}

fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    pixels
}
