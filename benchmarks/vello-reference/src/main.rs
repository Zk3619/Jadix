//! Fixed upstream Vello GPU reference; see README for comparison boundaries.
use serde_json::json;
use std::{error::Error, path::PathBuf, time::Instant};
use vello_gpu::peniko::{
    color::ColorSpaceTag,
    kurbo::{Affine, BezPath, Cap, Join, Rect, Stroke},
    Color, Fill, Gradient,
};
use vello_gpu::{
    ClearSettings, RenderSize, RenderTargetConfig, Renderer, Scene, TargetInit, TextureBindings,
};

fn scene(case: &str, frame: usize, size: u32) -> Scene {
    let mut scene = Scene::new(size as u16, size as u16);
    scene.set_transform(Affine::scale(size as f64 / 256.0));
    match case {
        "blurred-rrect" => {
            scene.set_paint(Color::from_rgba8(68,170,255,255));
            let sigma=3.+(frame as f32*0.05).sin();
            for y in 0..4 {for x in 0..4 {
                let (x,y)=(24.+x as f64*56.,24.+y as f64*56.);
                scene.fill_blurred_rounded_rect(&Rect::new(x,y,x+32.,y+24.),8.,sigma,false);
            }}
        }
        "compound" => {
            scene.set_fill_rule(Fill::EvenOdd);
            scene.set_paint(Color::from_rgba8(68, 170, 255, 255));
            let path=BezPath::from_svg("M238 128 A110 110 0 0 1 18 128 A110 110 0 0 1 238 128 Z M178 128 A50 50 0 0 1 78 128 A50 50 0 0 1 178 128 Z").unwrap();
            scene.fill_path(&path);
        }
        "gradient-grid" => {
            for y in 0..8 {
                for x in 0..8 {
                    let x = 4.0 + x as f64 * 31.0;
                    let y = 4.0 + y as f64 * 31.0;
                    let grad = Gradient::new_sweep((0.0, 0.0), 0.0, std::f32::consts::TAU)
                        .with_interpolation_cs(ColorSpaceTag::LinearSrgb)
                        .with_stops(
                            [
                                Color::from_rgba8(255, 0, 0, 255),
                                Color::from_rgba8(0, 0, 255, 255),
                                Color::from_rgba8(255, 0, 0, 255),
                            ]
                            .as_slice(),
                        );
                    scene.set_paint_transform(
                        Affine::translate((x + 12.5, y + 12.5))
                            * Affine::rotate(-std::f64::consts::FRAC_PI_2),
                    );
                    scene.set_paint(grad);
                    scene.fill_rect(&Rect::new(x, y, x + 25.0, y + 25.0));
                }
            }
        }
        "dynamic-curves" => {
            scene.set_paint(Color::from_rgba8(68, 170, 255, 255));
            scene.set_stroke(
                Stroke::new(1.5)
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
            );
            for row in 0..32 {
                let y = 8.0 + row as f64 * 7.5;
                let shift = ((frame as f32 * 0.07).sin() * 12.0) as f64;
                let mut path = BezPath::new();
                path.move_to((8.0, y));
                path.curve_to(
                    (70.0, y - 20.0 + shift),
                    (180.0, y + 20.0 - shift),
                    (248.0, y),
                );
                scene.stroke_path(&path);
            }
        }
        _ => unreachable!(),
    }
    scene
}
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: u32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let stride = (size * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("acceptance-readback"),
        size: u64::from(stride) * u64::from(size),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size),
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let data = buffer.slice(..).get_mapped_range()?;
    let mut rgba = Vec::with_capacity(size as usize * size as usize * 4);
    for row in data.chunks_exact(stride as usize) {
        rgba.extend_from_slice(&row[..size as usize * 4]);
    }
    drop(data);
    buffer.unmap();
    Ok(rgba)
}

fn percentiles(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    let at = |p: f64| values[((values.len() - 1) as f64 * p).ceil() as usize];
    json!({"p50_ms":at(0.5),"p95_ms":at(0.95),"max_ms":at(1.0)})
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let backend = args.get(1).map(String::as_str).unwrap_or("dx12");
    let output = PathBuf::from(args.get(2).map(String::as_str).unwrap_or("evidence/vello"));
    let frames = args
        .get(3)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(80);
    if !(2..=10000).contains(&frames) {
        return Err("frames out of range".into());
    }
    let backends = match backend {
        "dx12" => wgpu::Backends::DX12,
        "vulkan" => wgpu::Backends::VULKAN,
        "gles" => wgpu::Backends::GL,
        _ => return Err("backend must be dx12, vulkan or gles".into()),
    };
    std::fs::create_dir_all(&output)?;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    // Match the framework's production flags and FXC compiler policy.
    desc.flags = wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL.with_env();
    #[cfg(windows)]
    {
        desc.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    }
    let start = Instant::now();
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vello-reference"),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))?;
    let device_init_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut results = Vec::new();
    for size in [256u32, 512] {
        for case in ["compound", "gradient-grid", "dynamic-curves", "blurred-rrect"] {
            if case=="blurred-rrect" && args.get(4).map(String::as_str)!=Some("include-blur") {continue;}

            let start = Instant::now();
            let (mut renderer, mut resources) = Renderer::new(
                &device,
                &RenderTargetConfig {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    width: size as u16,
                    height: size as u16,
                },
            );
            let pipeline_init_ms = start.elapsed().as_secs_f64() * 1000.0;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(case),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let render_size = RenderSize {
                width: size as u16,
                height: size as u16,
            };
            let bindings = TextureBindings::new();
            let mut prepare = Vec::new();
            let mut submit = Vec::new();
            let mut total = Vec::new();
            let mut cold = 0.0;
            let mut draw = |frame| -> Result<(f64, f64, f64), Box<dyn Error>> {
                let start = Instant::now();
                let scene = scene(case, frame, size);
                let prep = start.elapsed().as_secs_f64() * 1000.0;
                let encode = Instant::now();
                let mut encoder = device.create_command_encoder(&Default::default());
                renderer.render(
                    &scene,
                    &mut resources,
                    &device,
                    &queue,
                    &mut encoder,
                    &render_size,
                    &view,
                    None,
                    &bindings,
                    TargetInit::Clear(ClearSettings::default()),
                )?;
                queue.submit(Some(encoder.finish()));
                device.poll(wgpu::PollType::wait_indefinitely())?;
                Ok((
                    prep,
                    encode.elapsed().as_secs_f64() * 1000.0,
                    start.elapsed().as_secs_f64() * 1000.0,
                ))
            };
            for frame in 0..frames + 5 {
                let (p, s, t) = draw(frame)?;
                if frame == 0 {
                    cold = t;
                }
                if frame >= 5 {
                    prepare.push(p);
                    submit.push(s);
                    total.push(t);
                }
            }
            draw(0)?;
            let rgba = readback(&device, &queue, &texture, size)?;
            assert!(
                rgba.chunks_exact(4).any(|p| p[3] > 0),
                "empty reference image"
            );
            image::save_buffer(
                output.join(format!("{case}-{size}-gpu.png")),
                &rgba,
                size,
                size,
                image::ColorType::Rgba8,
            )?;
            results.push(json!({"case":case,"size":size,"frames":frames,"cold_frame_ms":cold,"pipeline_init_ms":pipeline_init_ms,"cpu_prepare":percentiles(prepare),"encode_submit_wait":percentiles(submit),"total_frame":percentiles(total)}));
        }
    }
    let report = json!({"schema":1,"status":"pass","renderer":"vello_gpu 0.3.0","wgpu":"upstream 30.0.0","backend":backend,"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"adapter":info.name,"driver":info.driver,"driver_info":info.driver_info,"device_init_ms":device_init_ms,"output":"display-sRGB premultiplied RGBA, Rgba8Unorm target per Vello GPU encoding",
            "timing":"new scene per frame; five warmups; serialized CPU preparation + encode/submit/GPU wait; dev opt-level=1","results":results});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Vello reference failed: {e}");
        std::process::exit(1);
    }
}
