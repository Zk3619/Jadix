//! Fixed upstream Vello GPU reference; see README for comparison boundaries.
use serde_json::json;
use std::{error::Error, path::PathBuf, time::Instant};
use vello::peniko::{
    color::ColorSpaceTag,
    kurbo::{Affine, BezPath, Cap, Join, Rect, Stroke},
    Color, ColorStop, Fill, Gradient,
};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
fn scene(case: &str, frame: usize, size: u32) -> Scene {
    let mut scene = Scene::new();
    let transform = Affine::scale(size as f64 / 256.);
    let blue = Color::from_rgba8(68, 170, 255, 255);
    match case {
        "glyph-unhinted" | "glyph-hinted" | "glyph-emboldened" => {
            static FONT: std::sync::OnceLock<vello::peniko::FontData> = std::sync::OnceLock::new();
            let font = FONT.get_or_init(|| {
                vello::peniko::FontData::new(
                    vello::peniko::Blob::new(std::sync::Arc::new(
                        include_bytes!("../../../crates/jadix_cosmic_text/fonts/Inter-Regular.ttf")
                            .to_vec(),
                    )),
                    0,
                )
            });
            for row in 0..6 {
                scene
                    .draw_glyphs(font)
                    .font_size(11.3 + row as f32 * 2.)
                    .transform(transform)
                    .hint(case == "glyph-hinted")
                    .font_embolden(vello::FontEmbolden::new(vello::kurbo::Diagonal2::new(
                        if case == "glyph-emboldened" { 0.4 } else { 0. },
                        if case == "glyph-emboldened" { 0.6 } else { 0. },
                    )))
                    .brush(blue)
                    .draw(
                        Fill::NonZero,
                        (0..12).map(|i| vello::Glyph {
                            id: 161,
                            x: 8. + i as f32 * 20.,
                            y: 30. + row as f32 * 38.,
                        }),
                    );
            }
        }
        "sampling-seams" => {
            let xf = Affine::IDENTITY;
            scene.fill(
                Fill::NonZero,
                xf,
                Color::from_rgba8(255, 0, 0, 255),
                None,
                &Rect::new(0., 0., 12.25, size as f64),
            );
            scene.fill(
                Fill::NonZero,
                xf,
                Color::from_rgba8(0, 0, 255, 255),
                None,
                &Rect::new(12.25, 0., size as f64, size as f64),
            );
        }
        "sampling-oblique" => {
            use vello::peniko::{
                Blob, Extend, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
            };
            let pixels = (0..127 * 65)
                .flat_map(|i| {
                    [
                        if i % 127 % 2 == 0 { 255 } else { 0 },
                        if i / 127 < 32 { 255 } else { 0 },
                        ((i % 127) as f32 / 126. * 255.).round() as u8,
                        255,
                    ]
                })
                .collect::<Vec<u8>>();
            let image = ImageData {
                data: Blob::new(std::sync::Arc::new(pixels)),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 127,
                height: 65,
            };
            let brush = ImageBrush::new(image)
                .with_quality(ImageQuality::High)
                .with_x_extend(Extend::Repeat)
                .with_y_extend(Extend::Reflect);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                brush.as_ref(),
                Some(Affine::new([0.13, 0.07, -0.11, 0.4, 3., 4.])),
                &Rect::new(0., 0., size as f64, size as f64),
            );
        }
        "sampling-minify" => {
            use vello::peniko::{
                Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
            };
            let pixels = (0..383 * 191)
                .flat_map(|i| {
                    let v = if (i % 383 + i / 383) % 2 == 0 { 255 } else { 0 };
                    [v, v, v, 255]
                })
                .collect::<Vec<u8>>();
            let image = ImageData {
                data: Blob::new(std::sync::Arc::new(pixels)),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 383,
                height: 191,
            };
            scene.draw_image(
                ImageBrush::new(image)
                    .with_quality(ImageQuality::High)
                    .as_ref(),
                Affine::scale_non_uniform(size as f64 / 383., size as f64 / 191.),
            );
        }
        "clip-conflation" => {
            for row in 0..4 {
                for col in 0..8 {
                    let x = 8.2 + col as f64 * 30.;
                    let y = 8.25 + row as f64 * 55.;
                    let mut subject = BezPath::new();
                    subject.move_to((x, y));
                    subject.line_to((x + 0.35, y));
                    subject.line_to((x + 0.35, y + 35.4));
                    subject.line_to((x, y + 35.4));
                    subject.close_path();
                    let mut clip = BezPath::new();
                    let (lo, hi) = if col < 4 {
                        (x, x + 0.35)
                    } else {
                        (x + 0.1, x + 0.3)
                    };
                    clip.move_to((lo, y));
                    clip.line_to((hi, y));
                    clip.line_to((hi, y + 35.4));
                    clip.line_to((lo, y + 35.4));
                    clip.close_path();
                    scene.push_clip_layer(Fill::NonZero, transform, &clip);
                    scene.push_clip_layer(Fill::NonZero, transform, &clip);
                    scene.fill(Fill::NonZero, transform, blue, None, &subject);
                    scene.pop_layer();
                    scene.pop_layer();
                }
            }
        }
        "winding-overlap" => {
            let mut path = BezPath::new();
            for row in 0..4 {
                for col in 0..8 {
                    let x = 8.2 + col as f64 * 30.;
                    let y = 8.25 + row as f64 * 55.;
                    for _ in 0..2 {
                        path.move_to((x, y));
                        path.line_to((x + 0.35, y));
                        path.line_to((x + 0.35, y + 35.4));
                        path.line_to((x, y + 35.4));
                        path.close_path();
                    }
                }
            }
            scene.fill(Fill::NonZero, transform, blue, None, &path);
        }
        "large-grid" => {
            let shift = (frame as f32 * 0.07).sin() as f64 * 0.2;
            for i in 0..30000 {
                let x = (i % 200) as f64 * 1.28 + shift;
                let y = (i / 200) as f64 * 1.28;
                scene.fill(
                    Fill::NonZero,
                    transform,
                    blue,
                    None,
                    &Rect::new(x, y, x + 0.8, y + 0.8),
                );
            }
        }
        "compound" => {
            // Same two semicircle Arc commands as Jadix filled_circle; each renderer flattens independently.
            let path=BezPath::from_svg("M238 128 A110 110 0 0 1 18 128 A110 110 0 0 1 238 128 Z M178 128 A50 50 0 0 1 78 128 A50 50 0 0 1 178 128 Z").unwrap();
            scene.fill(Fill::EvenOdd, transform, blue, None, &path);
        }
        "gradient-grid" => {
            for y in 0..8 {
                for x in 0..8 {
                    let (x, y) = (4. + x as f64 * 31., 4. + y as f64 * 31.);
                    let gradient = Gradient::new_sweep((0., 0.), 0., std::f32::consts::TAU)
                        .with_interpolation_cs(ColorSpaceTag::LinearSrgb)
                        // Vello 0.11 does not consume interpolation_cs in its ramp encoder.
                        // Pre-expand the same linear-sRGB red/blue ramp to compare the same paint.
                        .with_stops(
                            (0..=512)
                                .map(|i| {
                                    let t = i as f32 / 512.;
                                    let blue = 1. - (2. * t - 1.).abs();
                                    let encode = |v: f32| {
                                        if v <= 0.0031308 {
                                            12.92 * v
                                        } else {
                                            1.055 * v.powf(1. / 2.4) - 0.055
                                        }
                                    };
                                    ColorStop {
                                        offset: t,
                                        color: Color::new([
                                            encode(1. - blue),
                                            0.,
                                            encode(blue),
                                            1.,
                                        ])
                                        .into(),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .as_slice(),
                        );
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        &gradient,
                        Some(
                            Affine::translate((x + 12.5, y + 12.5))
                                * Affine::rotate(-std::f64::consts::FRAC_PI_2),
                        ),
                        &Rect::new(x, y, x + 25., y + 25.),
                    );
                }
            }
        }
        "dynamic-curves" => {
            let stroke = Stroke::new(1.5)
                .with_caps(Cap::Round)
                .with_join(Join::Round);
            for row in 0..32 {
                let y = 8. + row as f64 * 7.5;
                let shift = ((frame as f32 * 0.07).sin() * 12.) as f64;
                let mut p = BezPath::new();
                p.move_to((8., y));
                p.curve_to((70., y - 20. + shift), (180., y + 20. - shift), (248., y));
                scene.stroke(&stroke, transform, blue, None, &p);
            }
        }
        "blurred-rrect" => {
            let sigma = 3. + (frame as f32 * 0.05).sin();
            for y in 0..4 {
                for x in 0..4 {
                    let (x, y) = (24. + x as f64 * 56., 24. + y as f64 * 56.);
                    scene.draw_blurred_rounded_rect(
                        transform,
                        Rect::new(x, y, x + 32., y + 24.),
                        blue,
                        8.,
                        sigma as f64,
                    );
                }
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

mod matched;
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
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))?;
    device.on_uncaptured_error(std::sync::Arc::new(|error| {
        eprintln!("uncaptured GPU error: {error}")
    }));
    device.set_device_lost_callback(|reason, message| {
        eprintln!("GPU device lost: {reason:?}: {message}")
    });
    let device_init_ms = start.elapsed().as_secs_f64() * 1000.0;
    if args.get(4).map(String::as_str) == Some("matched") {
        return matched::run(&device, &queue, &output, backend, &info, device_init_ms);
    }
    let mut results = Vec::new();
    let sampling = args.get(4).map(String::as_str) == Some("sampling-scenes");
    for size in if sampling { [32u32, 64] } else { [256, 512] } {
        for case in [
            "glyph-unhinted",
            "glyph-hinted",
            "glyph-emboldened",
            "compound",
            "gradient-grid",
            "dynamic-curves",
            "blurred-rrect",
            "large-grid",
            "winding-overlap",
            "clip-conflation",
            "sampling-seams",
            "sampling-minify",
            "sampling-oblique",
        ] {
            if sampling != case.starts_with("sampling-") {
                continue;
            }
            let geometry = args.get(4).map(String::as_str) == Some("geometry-clips");
            if (case == "clip-conflation") != geometry {
                continue;
            }
            let parity = args.get(4).map(String::as_str) == Some("feature-parity");
            if case == "winding-overlap" && !parity {
                continue;
            }
            if args.get(4).map(String::as_str) == Some("glyph-controls") {
                if !case.starts_with("glyph-") {
                    continue;
                }
            } else if case.starts_with("glyph-") && !parity {
                continue;
            }
            if args.get(6).map(String::as_str) == Some("stress-only") && case != "large-grid" {
                continue;
            }
            if case == "large-grid" && args.get(4).map(String::as_str) != Some("include-large") {
                continue;
            }
            if case == "blurred-rrect"
                && !matches!(
                    args.get(4).map(String::as_str),
                    Some("include-blur" | "include-large" | "feature-parity")
                )
            {
                continue;
            }

            eprintln!("case {case} size {size}");
            let start = Instant::now();
            let aa = match args.get(5).map(String::as_str) {
                Some("msaa16") => AaConfig::Msaa16,
                Some("msaa8") => AaConfig::Msaa8,
                _ => AaConfig::Area,
            };
            let mut renderer = Renderer::new(
                &device,
                RendererOptions {
                    antialiasing_support: AaSupport::all(),
                    num_init_threads: std::num::NonZeroUsize::new(1),
                    ..Default::default()
                },
            )?;
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
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut prepare = Vec::new();
            let mut submit = Vec::new();
            let mut total = Vec::new();
            let mut cold = 0.0;
            let mut draw = |frame| -> Result<(f64, f64, f64), Box<dyn Error>> {
                let start = Instant::now();
                let scene = scene(case, frame, size);
                let prep = start.elapsed().as_secs_f64() * 1000.0;
                let encode = Instant::now();
                renderer.render_to_texture(
                    &device,
                    &queue,
                    &scene,
                    &view,
                    &RenderParams {
                        base_color: Color::TRANSPARENT,
                        width: size,
                        height: size,
                        antialiasing_method: aa,
                    },
                )?;
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
    let report = json!({"schema":1,"status":"pass","renderer":"vello 0.11.0 compute","wgpu":"upstream 30.0.0","aa":args.get(5).map(String::as_str).unwrap_or("area"),"backend":backend,"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"adapter":info.name,"driver":info.driver,"driver_info":info.driver_info,"device_init_ms":device_init_ms,"output":"straight display-sRGB RGBA, Rgba8Unorm; fine.wgsl final rgba_sep unpremultiplies",
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
