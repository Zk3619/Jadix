//! GPU regression for the modal's blur + blit in a single command encoder.
use jadix::renderer::{BackdropBlurPipeline, offscreen::OffscreenGpu};

fn blur_then_blit_preserves_pass_uniforms() {
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::GL).expect("GL adapter");
    let (device, queue) = (&gpu.device, &gpu.queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut blur = BackdropBlurPipeline::new(device, format);
    blur.ensure_size(device, 128, 128, format);
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blur regression"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    // Uneven checker cells distinguish a four-tap downsample from a plain blit.
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl("@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f { var p = array<vec2f,3>(vec2f(-1,-1), vec2f(3,-1), vec2f(-1,3)); return vec4f(p[i],0,1); } @fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f { let cell = u32(p.x / 3.0) + u32(p.y / 5.0); return vec4f(vec3f(f32(cell % 2u)),1); }".into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: blur.scene_view().unwrap(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    let render = |blur: &mut BackdropBlurPipeline, split: bool| {
        let mut encoder = device.create_command_encoder(&Default::default());
        blur.blur(device, queue, &mut encoder, 12.0, None);
        if split {
            queue.submit([encoder.finish()]);
            encoder = device.create_command_encoder(&Default::default());
        }
        blur.blit_to(queue, &mut encoder, &view, 0.5, wgpu::Color::BLACK);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128 * 512,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512),
                    rows_per_image: Some(128),
                },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
        buffer.unmap();
        pixels
    };
    let separate = render(&mut blur, true);
    let combined = render(&mut blur, false);
    let changed = separate
        .iter()
        .zip(&combined)
        .filter(|(a, b)| a.abs_diff(**b) > 1)
        .count();
    assert_eq!(
        changed, 0,
        "same-encoder blur must match separate submissions"
    );
    assert!(
        (40..215).contains(&combined[(64 * 128 + 64) * 4]),
        "edge must actually be blurred"
    );
    println!("blur + blit: identical pixels in combined/separate submissions");
}

#[test]
#[ignore = "requires a GL adapter"]
fn modal_blur_and_blit_share_encoder() {
    blur_then_blit_preserves_pass_uniforms();
}

// Also runnable directly with rustc against the built release dependencies.
#[cfg(not(test))]
fn main() {
    blur_then_blit_preserves_pass_uniforms();
}
