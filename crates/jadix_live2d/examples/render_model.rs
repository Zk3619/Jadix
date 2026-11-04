use jadix_live2d::{
    render::wgpu::{GpuModel, WgpuLive2dRenderer},
    Player, View,
};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        println!("usage: render_model avatar.model3.json output.ppm [seconds] [motion-group] [motion-index]");
        return Ok(());
    };
    let output = args.next().ok_or("output.ppm path required")?;
    let seconds: f32 = args.next().unwrap_or_else(|| "1".into()).parse()?;
    if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
        return Err("seconds must be between 0 and 60".into());
    }
    let mut player = Player::load(path)?;
    if let Some(group) = args.next() {
        let index = args.next().unwrap_or_else(|| "0".into()).parse()?;
        player.play_motion(&group, index, true)?;
    }
    let frames = (seconds * 60.0).ceil().max(1.0) as usize;
    for _ in 0..frames {
        player.update(seconds / frames as f32)?;
    }
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for vertex in player
        .runtime()
        .meshes()
        .iter()
        .flat_map(|mesh| mesh.vertices())
    {
        let position = vertex.position();
        bounds[0] = bounds[0].min(position[0]);
        bounds[1] = bounds[1].min(position[1]);
        bounds[2] = bounds[2].max(position[0]);
        bounds[3] = bounds[3].max(position[1]);
    }
    let view = View::fit(bounds, [512, 512], 0.05)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let renderer = WgpuLive2dRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut model = GpuModel::new(&renderer, &device, &queue, player.model())?;
    model.update(
        &renderer,
        &device,
        &queue,
        player.runtime().meshes(),
        view.matrix(),
    )?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Live2D preview"),
        size: wgpu::Extent3d {
            width: 512,
            height: 512,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 512 * 512 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let draws = model.encode(
        &renderer,
        &mut encoder,
        &target.create_view(&Default::default()),
        Some(wgpu::Color {
            r: 0.08,
            g: 0.08,
            b: 0.08,
            a: 1.0,
        }),
    )?;
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512 * 4),
                rows_per_image: Some(512),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    receiver.recv()??;
    let pixels = buffer.slice(..).get_mapped_range()?;
    let mut file = std::io::BufWriter::new(std::fs::File::create(&output)?);
    file.write_all(b"P6\n512 512\n255\n")?;
    for pixel in pixels.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    file.flush()?;
    drop(pixels);
    buffer.unmap();
    if let Some(error) = pollster::block_on(errors.pop()) {
        return Err(error.to_string().into());
    }
    println!(
        "rendered {draws} drawables at {seconds}s to {output} on {}",
        adapter.get_info().name
    );
    Ok(())
}
