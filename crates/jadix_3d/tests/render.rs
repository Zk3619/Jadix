#![cfg(feature = "gpu-tests")]
mod common;
use jadix_3d::*;
use std::sync::Arc;

#[test]
fn skinned_glb_renders_and_moves_pixels_on_gpu() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("GPU adapter required for gpu-tests");
    eprintln!("3D test adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let scene = Arc::new(Scene::from_slice(&common::model(None)).unwrap());
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let mut renderer = Renderer::new(&device, format);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3d-pixel-test"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let camera =
            Camera::perspective(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, 0.8, 1.0, 0.1, 100.0);
        let mut centers = Vec::new();
        for time in [0.0, 0.6] {
            renderer
                .render(
                    &device,
                    &queue,
                    &view,
                    [64, 64],
                    &scene,
                    &scene.pose(Some(0), time, false).unwrap(),
                    camera,
                    [0.0; 4],
                )
                .unwrap();
            let pixels = read_pixels(&device, &queue, &texture);
            let occupied: Vec<_> = pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, pixel)| pixel[3] > 128)
                .collect();
            assert!(occupied.len() > 150, "model must produce visible pixels");
            let red = if format == wgpu::TextureFormat::Bgra8UnormSrgb {
                2
            } else {
                0
            };
            assert!(occupied
                .iter()
                .all(|(_, pixel)| pixel[red] > 240 && pixel[1] < 8));
            centers.push(
                occupied
                    .iter()
                    .map(|(index, _)| (index % 64) as f32)
                    .sum::<f32>()
                    / occupied.len() as f32,
            );
        }
        assert!(
            centers[1] > centers[0] + 10.0,
            "bone animation must move GPU pixels: {centers:?}"
        );
    }
    for textured in [false, true] {
        let mut scene = Scene::from_slice(&common::model(None)).unwrap();
        scene.nodes[0].skin = None;
        scene.nodes[0].transform = Transform::default();
        scene.meshes.push(scene.meshes[0].clone());
        scene.nodes[1].mesh = Some(1);
        scene.nodes[1].children.clear();
        scene.nodes[1].transform.translation.z = -0.5;
        scene.meshes[1].primitives[0].material.base_color = Vec4::new(0.0, 1.0, 0.0, 1.0);
        if textured {
            scene.images.push(Image {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 255, 255],
            });
            scene.meshes[0].primitives[0].material.base_color = Vec4::ONE;
            scene.meshes[0].primitives[0].material.base_color_texture = Some(0);
        }
        let scene = Arc::new(scene);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
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
        let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let camera =
            Camera::perspective(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, 0.8, 1.0, 0.1, 100.0);
        renderer
            .render(
                &device,
                &queue,
                &view,
                [64, 64],
                &scene,
                &scene.pose(None, 0.0, false).unwrap(),
                camera,
                [0.0; 4],
            )
            .unwrap();
        let pixels = read_pixels(&device, &queue, &texture);
        let center = &pixels[(32 * 64 + 32) * 4..(32 * 64 + 33) * 4];
        assert_eq!(
            center,
            if textured {
                &[0, 0, 255, 255]
            } else {
                &[255, 0, 0, 255]
            },
            "texture and depth must both be respected"
        );
    }
    assert!(pollster::block_on(errors.pop()).is_none());
}

#[test]
fn pbr_maps_affect_pixels_and_use_correct_color_spaces() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut base = Scene::from_slice(&common::model(None)).unwrap();
    base.nodes[0].skin = None;
    base.nodes[0].transform = Transform::default();
    base.nodes[1].light = Some(Light {
        color: Vec3::ONE,
        intensity: 2.0,
        range: None,
        kind: LightKind::Directional,
    });
    let material = &mut base.meshes[0].primitives[0].material;
    material.base_color = Vec4::ONE;
    material.metallic = 0.7;
    material.roughness = 0.7;
    material.unlit = false;
    for vertex in &mut base.meshes[0].primitives[0].vertices {
        vertex.uv = vertex.position.truncate() + Vec2::splat(0.5);
    }
    let sample = |scene: Scene| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let scene = Arc::new(scene);
        let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        renderer
            .render(
                &device,
                &queue,
                &texture.create_view(&Default::default()),
                [64, 64],
                &scene,
                &scene.pose(None, 0.0, false).unwrap(),
                Camera::perspective(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, 0.8, 1.0, 0.1, 100.0),
                [0.0; 4],
            )
            .unwrap();
        let pixels = read_pixels(&device, &queue, &texture);
        <[u8; 4]>::try_from(&pixels[(32 * 64 + 32) * 4..][..4]).unwrap()
    };
    let original = sample(base.clone());
    for normal in [false, true] {
        let mut scene = base.clone();
        scene.images.push(Image {
            width: 1,
            height: 1,
            rgba: if normal {
                vec![255, 128, 128, 255]
            } else {
                vec![255, 128, 0, 255]
            },
        });
        let map = TextureMap {
            image: 0,
            sampler: Material::default().sampler,
        };
        let material = &mut scene.meshes[0].primitives[0].material;
        if normal {
            material.normal_texture = Some(map);
        } else {
            material.metallic_roughness_texture = Some(map);
        }
        let changed = sample(scene);
        assert!(
            (i32::from(changed[0]) - i32::from(original[0])).abs() > 5,
            "map must affect pixels: normal={normal}, {original:?} -> {changed:?}"
        );
    }
    base.nodes[1].light.as_mut().unwrap().intensity = 0.0;
    base.meshes[0].primitives[0].material.metallic = 0.0;
    let ambient = sample(base.clone());
    let mut occluded = base.clone();
    occluded.images.push(Image {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 255],
    });
    occluded.meshes[0].primitives[0].material.occlusion_texture = Some(TextureMap {
        image: 0,
        sampler: Material::default().sampler,
    });
    assert!(
        sample(occluded)[0] + 5 < ambient[0],
        "occlusion must attenuate ambient"
    );
    let mut emissive = base;
    let material = &mut emissive.meshes[0].primitives[0].material;
    material.base_color = Vec4::new(0.0, 0.0, 0.0, 1.0);
    material.emissive = Vec3::ONE;
    material.emissive_texture = Some(TextureMap {
        image: 0,
        sampler: Material::default().sampler,
    });
    emissive.images.push(Image {
        width: 1,
        height: 1,
        rgba: vec![128, 0, 0, 255],
    });
    let pixel = sample(emissive);
    assert!(
        (i32::from(pixel[0]) - 128).abs() <= 1 && pixel[1] == 0 && pixel[2] == 0,
        "emissive sRGB must round-trip: {pixel:?}"
    );
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
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
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
