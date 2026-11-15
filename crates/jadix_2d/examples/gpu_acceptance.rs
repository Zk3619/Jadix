//! Cross-platform native acceptance and serialized-frame benchmark.
//! Args: BACKEND OUTPUT [FRAMES] [straight|premultiplied] [balanced|high|reference]
//! [reference-scenes]. No backend fallback.
use jadix_2d::{gpu::PathGpu, wgpu, *};
use serde_json::json;
use std::{error::Error, path::Path, time::Instant};
#[cfg(test)]
#[path = "gpu_acceptance/color.rs"]
mod color_tests;
#[cfg(test)]
#[path = "gpu_acceptance/filters.rs"]
mod filter_tests;
#[cfg(test)]
#[path = "gpu_acceptance/scene.rs"]
mod scene_tests;
#[cfg(all(test, feature = "text-layout"))]
#[path = "gpu_acceptance/text.rs"]
mod text_tests;
#[cfg(test)]
#[path = "gpu_acceptance/vector.rs"]
mod vector_tests;

#[cfg(test)]
#[path = "gpu_acceptance/advanced.rs"]
mod advanced_tests;
#[cfg(test)]
#[path = "gpu_acceptance/imaging.rs"]
mod imaging_tests;

#[cfg(test)]
#[path = "gpu_acceptance/interop.rs"]
mod interop_tests;

#[cfg(test)]
struct TestGpu {
    _instance: wgpu::Instance,
    backend: wgpu::Backends,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}
#[cfg(test)]
fn test_gpu() -> TestGpu {
    test_gpu_with_features(Default::default())
}
#[cfg(test)]
fn test_gpu_with_features(features: wgpu::Features) -> TestGpu {
    let backend = match std::env::var("JADIX_TEST_BACKEND").as_deref() {
        Ok("gles") => wgpu::Backends::GL,
        Ok("vulkan") => wgpu::Backends::VULKAN,
        _ if cfg!(windows) => wgpu::Backends::DX12,
        _ => wgpu::Backends::VULKAN,
    };
    let instance = jadix_renderer::wgpu_compat::instance(backend);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: std::env::var_os("JADIX_FORCE_SOFTWARE").is_some(),
        ..Default::default()
    }))
    .expect("headless GPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        ..jadix_renderer::wgpu_compat::device_desc(
            None,
            jadix_renderer::gpu_caps::limits_for(&adapter),
            Default::default(),
        )
    }))
    .expect("headless GPU device");
    device.on_uncaptured_error(std::sync::Arc::new(|err| panic!("GPU validation: {err}")));
    TestGpu {
        _instance: instance,
        backend,
        adapter,
        device,
        queue,
    }
}

fn scene(
    case: &str,
    frame: usize,
    size: u32,
    image: &Image2D,
    mask: &Image2D,
    glyphs: &GlyphRun,
) -> GpuCanvas {
    let mut c = GpuCanvas::new_linear(size, size, None);
    let s = size as f32 / 256.0;
    c.scale(s, s);
    match case {
        "glyph-unhinted" | "glyph-hinted" | "glyph-emboldened" => {
            static FONT: std::sync::OnceLock<Font2D> = std::sync::OnceLock::new();
            let font = FONT.get_or_init(|| {
                Font2D::new(
                    include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf").as_slice(),
                    0,
                )
                .unwrap()
            });
            let hinted = case == "glyph-hinted";
            let factor = if hinted {
                c.scale(1. / s, 1. / s);
                s
            } else {
                1.
            };
            c.set_fill("#44aaff");
            for row in 0..6 {
                let size = (11.3 + row as f32 * 2.) * factor;
                let glyphs = (0..12)
                    .map(|i| PositionedGlyph {
                        id: font.glyph_id('H').unwrap(),
                        x: (8. + i as f32 * 20.) * factor,
                        y: (30. + row as f32 * 38.) * factor,
                    })
                    .collect::<Vec<_>>();
                let run = GlyphRun::with_options(
                    font,
                    size,
                    &glyphs,
                    &GlyphRunOptions {
                        hint: hinted,
                        embolden: FontEmbolden {
                            amount: if case == "glyph-emboldened" {
                                [0.4, 0.6]
                            } else {
                                [0.; 2]
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
                c.fill_glyph_run(&run);
            }
        }
        "winding-overlap" => {
            c.set_fill("#44aaff");
            let mut path = Vec::new();
            for row in 0..4 {
                for col in 0..8 {
                    let rect =
                        filled_rect(8.2 + col as f32 * 30., 8.25 + row as f32 * 55., 0.35, 35.4);
                    path.extend(rect.clone());
                    path.extend(rect);
                }
            }
            c.fill_path(&path);
        }
        "large-grid" => {
            c.set_fill("#44aaff");
            let shift = (frame as f32 * 0.07).sin() * 0.2;
            for i in 0..30000 {
                c.fill_rect(
                    (i % 200) as f32 * 1.28 + shift,
                    (i / 200) as f32 * 1.28,
                    0.8,
                    0.8,
                );
            }
        }
        "vector-gradients" => {
            let stops = GradientStops::new(
                &(0..33)
                    .map(|i| {
                        let t = i as f32 / 32.;
                        (
                            t,
                            [
                                t,
                                if i % 2 == 0 { 0.8 } else { 0.25 },
                                1. - t,
                                0.3 + 0.7 * t,
                            ],
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap()
            .with_midpoints(&[Some(0.3); 32])
            .unwrap();
            let drift = (frame as f32 * 0.05).sin() * 4.;
            for y in 0..3 {
                for x in 0..3 {
                    c.save();
                    c.translate(x as f32 * 84., y as f32 * 84.);
                    let g = match y {
                        0 => VectorGradient::linear([12. + drift, 16.], [64., 60.], stops.clone()),
                        1 => VectorGradient::radial(
                            [28. + drift, 32.],
                            2.,
                            [40., 40.],
                            28.,
                            stops.clone(),
                        ),
                        _ => VectorGradient::sweep(
                            [40., 40.],
                            -30. + drift,
                            240. + drift,
                            stops.clone(),
                        ),
                    }
                    .unwrap()
                    .with_extend([ExtendMode::Pad, ExtendMode::Reflect, ExtendMode::Decal][x]);
                    c.set_fill_brush(Brush2D::vector(g));
                    c.fill_path(&filled_circle(40., 40., 36., 48));
                    c.restore();
                }
            }
        }
        "morphology-chain" => {
            for row in 0..3 {
                c.push_layer_with_options(LayerOptions {
                    filters: vec![
                        LayerFilter::Morphology {
                            radius: [2 + row, 3],
                            operator: if row == 1 {
                                Morphology::Erode
                            } else {
                                Morphology::Dilate
                            },
                        },
                        LayerFilter::DropShadowOnly {
                            sigma: [2., 4.],
                            offset: [4.5, 3.],
                            color: [1., 0.3, 0.1, 0.8],
                        },
                        LayerFilter::Offset {
                            offset: [(frame as f32 * 0.05).sin(), -0.5],
                        },
                    ],
                    ..Default::default()
                });
                c.set_fill("#ffffffc0");
                for col in 0..4 {
                    c.fill_path(&filled_circle(
                        36. + col as f32 * 60.,
                        40. + row as f32 * 80.,
                        16.,
                        32,
                    ));
                }
                c.pop_layer();
            }
        }
        "rounded-shadows" => {
            for row in 0..3 {
                for col in 0..3 {
                    c.draw_rounded_rect_shadow(
                        [18. + col as f32 * 78., 18. + row as f32 * 78., 58., 50.],
                        [0., 22., 8., 16.],
                        2. + (frame as f32 * 0.05).sin(),
                        [0.2, 0.6, 1., 0.8],
                        row % 2 == 1,
                    );
                }
            }
        }
        "filter-chain" => {
            c.push_layer_with_options(LayerOptions {
                filters: vec![
                    LayerFilter::Gaussian { sigma: [1., 0.5] },
                    LayerFilter::DropShadow {
                        sigma: [3., 5.],
                        offset: [4. + (frame as f32 * 0.05).sin(), 6.],
                        color: [0.8, 0.1, 0.2, 0.7],
                    },
                ],
                ..Default::default()
            });
            for row in 0..4 {
                c.set_fill("#2080ffa0");
                c.fill_rect(18., 18. + row as f32 * 58., 160., 28.);
                c.set_fill("#ff802080");
                c.fill_rect(100., 26. + row as f32 * 58., 132., 28.);
            }
            c.pop_layer();
        }
        "glyph-runs" => {
            c.set_fill_brush(Brush2D::gradient(
                gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
                [0., 0., 256., 256.],
            ));
            for row in 0..4 {
                c.save();
                c.translate(12., row as f32 * 52. + 4.);
                c.rotate((frame as f32 * 0.03).sin() * 3.);
                c.fill_glyph_run(glyphs);
                c.restore();
            }
        }
        "blurred-rrect" => {
            let sigma = 3. + (frame as f32 * 0.05).sin();
            for y in 0..4 {
                for x in 0..4 {
                    c.draw_blurred_rounded_rect(
                        [24. + x as f32 * 56., 24. + y as f32 * 56., 32., 24.],
                        8.,
                        sigma,
                        [68. / 255., 170. / 255., 1., 1.],
                    );
                }
            }
        }
        "luminance-mask" => {
            c.set_fill_brush(Brush2D::gradient(
                gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
                [0., 0., 256., 256.],
            ));
            c.fill_rect(0., 0., 256., 256.);
            c.push_luminance_layer(0.9, false);
            c.draw_blurred_rounded_rect([32., 32., 192., 192.], 32., 8., [1.; 4]);
            c.set_fill("#000000");
            c.fill_rect(0., 112., 256., 32.);
            c.pop_layer();
        }
        "compound" => {
            c.set_fill("#44aaff");
            c.set_fill_rule(FillRule::EvenOdd);
            let mut path = filled_circle(128.0, 128.0, 110.0, 64);
            path.extend(filled_circle(128.0, 128.0, 50.0, 64));
            c.fill_path(&path);
        }
        "gradient-grid" => {
            for y in 0..8 {
                for x in 0..8 {
                    c.set_fill_gradient(
                        gradient::parse_gradient("conic-gradient(#ff0000,#0000ff,#ff0000)")
                            .unwrap(),
                    );
                    c.fill_rect(4.0 + x as f32 * 31.0, 4.0 + y as f32 * 31.0, 25.0, 25.0);
                }
            }
        }
        "dynamic-curves" | "dashed-curves" | "gradient-strokes" => {
            if case == "dashed-curves" {
                c.set_line_dash(&[7., 3., 2.]);
                c.set_line_dash_offset(frame as f32 * 0.2);
            }
            c.set_stroke("#44aaff");
            if case == "gradient-strokes" {
                c.set_stroke_gradient(
                    gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
                );
            }
            c.set_line_width(1.5);
            for row in 0..32 {
                let y = 8.0 + row as f32 * 7.5;
                let shift = (frame as f32 * 0.07).sin() * 12.0;
                c.stroke_path(&[
                    PathCommand::MoveTo((8.0, y)),
                    PathCommand::CubicTo {
                        c1: (70.0, y - 20.0 + shift),
                        c2: (180.0, y + 20.0 - shift),
                        to: (248.0, y),
                    },
                ]);
            }
        }
        "isolated-layers" => {
            c.set_fill("#0000ff");
            c.fill_rect(0., 0., 256., 256.);
            for row in 0..8 {
                c.push_layer(
                    1.,
                    if row % 2 == 0 {
                        BlendMode::Multiply
                    } else {
                        BlendMode::Screen
                    },
                );
                c.push_layer(1., BlendMode::Normal);
                c.set_fill("#ff0000");
                c.fill_rect(4., row as f32 * 32. + 4., 180., 24.);
                c.set_fill("#00ff00");
                c.fill_rect(96., row as f32 * 32. + 4., 156., 24.);
                c.pop_layer();
                c.pop_layer();
            }
        }
        "porter-duff" => {
            c.set_fill("#8040c0a0");
            c.fill_rect(0., 0., 256., 256.);
            let modes = [
                CompositeMode::Clear,
                CompositeMode::Copy,
                CompositeMode::Destination,
                CompositeMode::SourceOver,
                CompositeMode::DestinationOver,
                CompositeMode::SourceIn,
                CompositeMode::DestinationIn,
                CompositeMode::SourceOut,
                CompositeMode::DestinationOut,
                CompositeMode::SourceAtop,
                CompositeMode::DestinationAtop,
                CompositeMode::Xor,
                CompositeMode::Plus,
                CompositeMode::PlusLighter,
            ];
            for (i, compose) in modes.into_iter().enumerate() {
                let x = 4. + (i % 7) as f32 * 36.;
                let y = 4. + (i / 7) as f32 * 124.;
                c.push_layer_with_options(LayerOptions {
                    opacity: 0.7,
                    compose,
                    blend: if i % 2 == 0 {
                        BlendMode::Hue
                    } else {
                        BlendMode::SoftLight
                    },
                    clip: Some([x * s, y * s, 32. * s, 116. * s]),
                    mask: Some(mask.clone()),
                    filters: Vec::new(),
                });
                c.set_fill_gradient(
                    gradient::parse_gradient("linear-gradient(#20c080c0,#e0604080)").unwrap(),
                );
                c.fill_rect(x, y, 24., 116.);
                c.pop_layer();
            }
        }
        "explicit-gradient" => {
            let a = (frame as f32 * 0.04).sin() * 0.3;
            let (sn, cs) = a.sin_cos();
            let brush = Brush2D::gradient(
                gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
                [0., 0., 256., 256.],
            )
            .with_transform([
                cs,
                sn,
                -sn,
                cs,
                128. - cs * 128. + sn * 128.,
                128. - sn * 128. - cs * 128.,
            ]);
            c.set_fill_brush(brush.clone());
            for y in 0..8 {
                for x in 0..8 {
                    c.fill_rect(4. + x as f32 * 31., 4. + y as f32 * 31., 25., 25.);
                }
            }
            c.set_stroke_brush(brush);
            c.set_line_width(4.);
            c.stroke_path(&polyline(&[(4., 252.), (252., 4.)]));
        }
        "pattern-grid" => {
            let modes = [
                ExtendMode::Pad,
                ExtendMode::Repeat,
                ExtendMode::Reflect,
                ExtendMode::Decal,
            ];
            for y in 0..4 {
                for x in 0..4 {
                    c.set_fill_brush(
                        Brush2D::image(image.clone())
                            .with_sampling(ImageSampling::Bilinear)
                            .with_extend(modes[x], modes[y])
                            .with_transform([
                                8.,
                                0.,
                                0.,
                                8.,
                                16. + x as f32 * 64. + (frame as f32 * 0.05).sin() * 4.,
                                16. + y as f32 * 64.,
                            ]),
                    );
                    c.fill_rect(4. + x as f32 * 64., 4. + y as f32 * 64., 56., 56.);
                }
            }
        }
        "image-nearest" | "image-bilinear" => {
            c.set_image_sampling(if case == "image-nearest" {
                ImageSampling::Nearest
            } else {
                ImageSampling::Bilinear
            });
            for y in 0..8 {
                for x in 0..8 {
                    c.save();
                    c.translate(17.0 + x as f32 * 31.0, 17.0 + y as f32 * 31.0);
                    c.rotate((frame as f32 * 0.03).sin() * 12.0 + 7.0);
                    c.set_global_alpha(0.8);
                    c.draw_image(&CanvasImageSource::Image(image), -12.0, -12.0, 24.0, 24.0);
                    c.restore();
                }
            }
        }
        "masked-gradients" | "path-clips" => {
            c.save();
            if case == "masked-gradients" {
                c.clip_image_mask(mask);
            } else {
                c.clip_path(&filled_circle(128.0, 128.0, 110.0, 64), FillRule::NonZero);
                c.clip_path(
                    &decode_path_d("M0 0 H256 V256 H0 Z M80 80 H176 V176 H80 Z"),
                    FillRule::EvenOdd,
                );
            }
            c.set_fill_gradient(
                gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
            );
            c.fill_rect(0.0, 0.0, 256.0, 256.0);
            c.set_stroke("white");
            c.set_line_width(2.0);
            c.stroke_path(&polyline(&[(0.0, 0.0), (256.0, 256.0)]));
            c.restore();
            c.set_fill("#00ff00");
            c.fill_rect(2.0, 2.0, 4.0, 4.0);
        }
        _ => unreachable!(),
    }
    c
}

fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: u32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    readback_rect(device, queue, texture, size, size)
}
fn readback_rect(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let stride = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("acceptance-readback"),
        size: u64::from(stride) * u64::from(h),
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
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
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
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for row in data.chunks_exact(stride as usize) {
        rgba.extend_from_slice(&row[..w as usize * 4]);
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
    let output = std::path::PathBuf::from(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("target/2d-acceptance"),
    );
    let frames = args
        .get(3)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(40);
    let premultiplied = match args.get(4).map(String::as_str) {
        None | Some("straight") => false,
        Some("premultiplied") => true,
        _ => return Err("output must be straight or premultiplied".into()),
    };
    if !(2..=10000).contains(&frames) {
        return Err("FRAMES must be in 2..=10000".into());
    }
    let backends = match backend {
        "dx12" => wgpu::Backends::DX12,
        "vulkan" => wgpu::Backends::VULKAN,
        "gles" => wgpu::Backends::GL,
        "metal" => wgpu::Backends::METAL,
        _ => return Err("unsupported backend name".into()),
    };
    std::fs::create_dir_all(&output)?;
    if !wgpu::Instance::enabled_backend_features().contains(backends) {
        return Err(format!("{backend} not compiled; enable backend-{backend}").into());
    }
    let start = Instant::now();
    let instance = jadix_renderer::wgpu_compat::instance(backends);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: std::env::var_os("JADIX_FORCE_SOFTWARE").is_some(),
        ..Default::default()
    }))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(
        &jadix_renderer::wgpu_compat::device_desc(
            Some("2d-acceptance"),
            wgpu::Limits::downlevel_defaults(),
            Default::default(),
        ),
    ))?;
    device.on_uncaptured_error(std::sync::Arc::new(|err| panic!("GPU validation: {err}")));
    let device_init_ms = start.elapsed().as_secs_f64() * 1000.0;
    let image = Image2D::from_rgba8(
        4,
        4,
        &[
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255,
            0, 255, 255, 0, 255, 0, 0, 255, 255, 255, 128, 0, 0, 0, 128, 255, 255, 255, 255, 255,
            0, 0, 128, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255,
            0,
        ],
    )
    .unwrap();
    let font = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )?;
    let mut x = 0.;
    let glyph_positions: Vec<_> = "Jadix 2D"
        .chars()
        .map(|ch| {
            let id = font.glyph_id(ch).unwrap();
            let glyph = PositionedGlyph { id, x, y: 40. };
            x += font.advance(id, 36.).unwrap();
            glyph
        })
        .collect();
    let glyphs = GlyphRun::new(&font, 36., &glyph_positions)?;
    let mut results = Vec::new();
    for size in [256u32, 512] {
        let mut mask_canvas = RasterCanvas::new(size, size);
        mask_canvas.scale(size as f32 / 256.0, size as f32 / 256.0);
        mask_canvas.set_fill("white");
        mask_canvas.fill_path(&filled_circle(128.0, 128.0, 110.0, 64));
        let mask = Image2D::from_rgba8(
            size,
            size,
            &mask_canvas.get_image_data(0, 0, size, size).unwrap(),
        )
        .unwrap();
        for case in [
            "glyph-unhinted",
            "glyph-hinted",
            "glyph-emboldened",
            "large-grid",
            "winding-overlap",
            "compound",
            "gradient-grid",
            "explicit-gradient",
            "pattern-grid",
            "glyph-runs",
            "rounded-shadows",
            "filter-chain",
            "vector-gradients",
            "morphology-chain",
            "blurred-rrect",
            "luminance-mask",
            "dynamic-curves",
            "dashed-curves",
            "isolated-layers",
            "gradient-strokes",
            "porter-duff",
            "image-nearest",
            "image-bilinear",
            "masked-gradients",
            "path-clips",
        ] {
            let reference_mode = args.get(6).map(String::as_str);
            if case == "winding-overlap" && reference_mode != Some("feature-parity") {
                continue;
            }
            if reference_mode == Some("feature-parity") && case == "large-grid" {
                continue;
            }
            if case == "large-grid" && reference_mode != Some("reference-scenes-large") {
                continue;
            }
            if case.starts_with("glyph-")
                && case != "glyph-runs"
                && !matches!(
                    reference_mode,
                    Some("reference-glyph-controls" | "feature-parity")
                )
            {
                continue;
            }
            let selected = match reference_mode {
                Some("feature-parity") => matches!(
                    case,
                    "glyph-unhinted"
                        | "glyph-hinted"
                        | "glyph-emboldened"
                        | "winding-overlap"
                        | "compound"
                        | "gradient-grid"
                        | "dynamic-curves"
                        | "blurred-rrect"
                ),
                Some("reference-glyph-controls") => {
                    matches!(case, "glyph-unhinted" | "glyph-hinted" | "glyph-emboldened")
                }
                Some("reference-scenes") => {
                    matches!(case, "compound" | "gradient-grid" | "dynamic-curves")
                }
                Some("reference-scenes-blur") => matches!(
                    case,
                    "compound" | "gradient-grid" | "dynamic-curves" | "blurred-rrect"
                ),
                Some("reference-scenes-large") => matches!(
                    case,
                    "compound"
                        | "gradient-grid"
                        | "dynamic-curves"
                        | "blurred-rrect"
                        | "large-grid"
                ),
                _ => true,
            };
            if !selected
                || (args.get(7).map(String::as_str) == Some("stress-only") && case != "large-grid")
            {
                continue;
            }

            let start = Instant::now();
            let samples = jadix_2d::gpu::path_msaa_sample_count(&adapter);
            let mut renderer =
                PathGpu::try_new(&device, samples).ok_or("pipeline initialization failed")?;
            let quality = match args.get(5).map(String::as_str) {
                None | Some("balanced") => gpu::AaQuality::Balanced,
                Some("high") => gpu::AaQuality::High,
                Some("reference") => gpu::AaQuality::Reference,
                Some("area") => gpu::AaQuality::Area,
                _ => return Err("quality must be balanced, high, reference or area".into()),
            };
            renderer.set_aa_quality(quality);
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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let render = |renderer: &mut PathGpu, ops: &[PathGpuOp]| {
                if premultiplied {
                    renderer.render_premultiplied(&device, &queue, &view, [size, size], ops)
                } else {
                    renderer.render(&device, &queue, &view, [size, size], ops)
                }
            };
            let mut prepare_times = Vec::new();
            let mut submit_times = Vec::new();
            let mut frame_times = Vec::new();
            let mut warm_allocations = 0u64;
            let mut warm_textures = 0u64;
            let mut upload_bytes = 0u64;
            let mut cold_ms = 0.0;
            let mut cold_allocations = (0u32, 0u32, 0u32);
            // Five warmup frames separate initialization from sustained work.
            for frame in 0..frames + 5 {
                let start = Instant::now();
                let recording = scene(case, frame, size, &image, &mask, &glyphs);
                assert!(!recording.needs_cpu, "{case} must stay on GPU");
                let ops = recording.into_ops();
                let prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
                let submit = Instant::now();
                if !render(&mut renderer, &ops) {
                    return Err("render failed".into());
                }
                // Both benchmark programs serialize completion before stopping
                // the timer. Submission alone is asynchronous on native GPUs.
                device.poll(wgpu::PollType::wait_indefinitely())?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if frame == 0 {
                    cold_ms = elapsed;
                    let stats = renderer.last_stats();
                    cold_allocations = (
                        stats.buffer_allocations,
                        stats.bind_group_allocations,
                        stats.texture_allocations,
                    );
                }
                if frame >= 5 {
                    prepare_times.push(prepare_ms);
                    submit_times.push(submit.elapsed().as_secs_f64() * 1000.0);
                    frame_times.push(elapsed);
                    let stats = renderer.last_stats();
                    warm_allocations +=
                        u64::from(stats.buffer_allocations + stats.bind_group_allocations);
                    warm_textures += u64::from(stats.texture_allocations);
                    upload_bytes += stats.uploaded_bytes;
                }
            }
            if case != "dynamic-curves"
                && case != "gradient-strokes"
                && case != "path-clips"
                && (warm_allocations != 0 || warm_textures != 0)
            {
                return Err(format!("{case}: GPU allocations after warmup").into());
            }
            // Rerender fixed frame zero for comparable CPU/GPU images across devices.
            let ops = scene(case, 0, size, &image, &mask, &glyphs).into_ops();
            if !render(&mut renderer, &ops) {
                return Err("capture failed".into());
            }
            let actual = readback(&device, &queue, &texture, size)?;
            let recording = scene(case, 0, size, &image, &mask, &glyphs);
            let cpu_pixels = if quality == gpu::AaQuality::Area {
                recording.into_raster_area()?
            } else {
                recording.into_raster()
            };
            let mut expected = premul_to_straight_rgba(&cpu_pixels, size, size);
            if premultiplied {
                for p in expected.chunks_exact_mut(4) {
                    let a = p[3] as f32 / 255.0;
                    for c in &mut p[..3] {
                        *c = (jadix_theme::color_space::encode_linear_srgb_component(
                            jadix_theme::color_space::linearize_display_srgb_component(
                                *c as f32 / 255.0,
                            ) * a,
                        ) * 255.0)
                            .round() as u8;
                    }
                }
            }
            let mae = actual
                .iter()
                .zip(&expected)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).unsigned_abs() as f64)
                .sum::<f64>()
                / actual.len() as f64;
            let alpha_mae = actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .map(|(a, b)| (f64::from(a[3]) - f64::from(b[3])).abs())
                .sum::<f64>()
                / (size * size) as f64;
            let visible = actual.chunks_exact(4).filter(|p| p[3] > 0).count();
            if visible == 0 {
                return Err(format!("{case}: empty GPU capture").into());
            }
            // Finite scene-specific envelopes; these do not claim global renderer equality.
            let max_mae = if case.ends_with("curves") || case == "gradient-strokes" {
                32.0
            } else {
                8.0
            };
            let decode = jadix_theme::color_space::linearize_display_srgb_component;
            let premul_mae = actual
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .map(|(a, b)| {
                    (0..3)
                        .map(|i| {
                            let (av, bv) = (decode(a[i] as f32 / 255.), decode(b[i] as f32 / 255.));
                            if premultiplied {
                                (av - bv).abs() * 255.
                            } else {
                                (av * a[3] as f32 - bv * b[3] as f32).abs()
                            }
                        })
                        .sum::<f32>()
                        + f32::from(a[3].abs_diff(b[3]))
                })
                .map(f64::from)
                .sum::<f64>()
                / actual.len() as f64;
            image::save_buffer(
                output.join(format!("{case}-{size}-gpu.png")),
                &actual,
                size,
                size,
                image::ColorType::Rgba8,
            )?;
            image::save_buffer(
                output.join(format!("{case}-{size}-cpu.png")),
                &expected,
                size,
                size,
                image::ColorType::Rgba8,
            )?;
            if (if matches!(
                case,
                "blurred-rrect"
                    | "luminance-mask"
                    | "rounded-shadows"
                    | "filter-chain"
                    | "vector-gradients"
                    | "morphology-chain"
            ) {
                premul_mae
            } else {
                mae
            }) > max_mae
                || alpha_mae > 12.0
            {
                return Err(format!("{case}/{size}: CPU/GPU error {mae}/{alpha_mae}").into());
            }
            let retained = renderer.last_stats();
            results.push(json!({"case":case,"size":size,"samples":samples,"area_ignores_msaa":quality==gpu::AaQuality::Area,"frames":frames,"cold_frame_ms":cold_ms,"pipeline_init_ms":pipeline_init_ms,
                "vertex_capacity_bytes":retained.vertex_capacity_bytes,"index_capacity_bytes":retained.index_capacity_bytes,
                "unique_vertices":retained.unique_vertices,"indices":retained.indices,"queue_submissions":retained.queue_submissions,"gradient_slots":retained.gradient_slots,
                "gradient_records":retained.gradient_records,"gradient_stops":retained.gradient_stops,"gradient_table_bytes":retained.gradient_table_bytes,
                "area_primitives":retained.area_primitives,"area_triangles":retained.area_triangles,"layer_source_pixels":retained.layer_source_pixels,"layer_full_canvas_pixels":retained.layer_full_canvas_pixels,
                "draw_calls":retained.draw_calls,"uniform_uploads":retained.uniform_uploads,"cached_images":retained.cached_images,
                "cold_buffers":cold_allocations.0,"cold_bindings":cold_allocations.1,"cold_textures":cold_allocations.2,
                "cpu_prepare":percentiles(prepare_times),"encode_submit_wait":percentiles(submit_times),"total_frame":percentiles(frame_times),
                "warm_buffer_and_binding_allocations":warm_allocations,"warm_texture_allocations":warm_textures,"warm_uploaded_bytes":upload_bytes,
                "rgba_mean_absolute_error":mae,"linear_premultiplied_mae":premul_mae,"alpha_mean_absolute_error":alpha_mae,"visible_pixels":visible,
                "gpu_byte_sum":actual.iter().map(|&v|u64::from(v)).sum::<u64>()}));
        }
    }
    let report = json!({"quality":args.get(5).map(String::as_str).unwrap_or("balanced"),"schema":3,"cpu_compositing":"linear-srgb","status":"pass","platform":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "backend":backend,"adapter":info.name,"device_type":format!("{:?}",info.device_type),"driver":info.driver,"driver_info":info.driver_info,
        "device_init_ms":device_init_ms,"timing":"serialized CPU preparation + encode/submit/GPU wait; not GPU timestamp queries or window FPS",
        "output_alpha":if premultiplied {"linear-premultiplied"} else {"straight"},"vello_comparison":"not run","results":results});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("2D acceptance failed: {error}");
        // A requested unavailable backend is an explicit failure, never a green skip.
        let args: Vec<_> = std::env::args().collect();
        let output = Path::new(
            args.get(2)
                .map(String::as_str)
                .unwrap_or("target/2d-acceptance"),
        );
        let _ = std::fs::create_dir_all(output);
        let _ = std::fs::write(
            output.join("report.json"),
            json!({"schema":1,"status":"failed","error":error.to_string()}).to_string(),
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a native headless GPU adapter"]
    fn glyph_blur_and_luminance_matrix() -> Result<(), Box<dyn Error>> {
        let TestGpu {
            backend,
            adapter,
            device,
            queue,
            _instance,
        } = test_gpu();

        let font = Font2D::new(
            &include_bytes!("../../jadix_cosmic_text/fonts/InterVariable.ttf")[..],
            0,
        )?;
        let f = font.with_variations(&[(*b"wght", 700.)])?;
        let glyphs: Vec<_> = "Jadix"
            .chars()
            .enumerate()
            .map(|(i, ch)| PositionedGlyph {
                id: f.glyph_id(ch).unwrap(),
                x: 2. + i as f32 * 12.,
                y: 28.,
            })
            .collect();
        let run = GlyphRun::new(&f, 20., &glyphs)?;
        let mask = Image2D::from_rgba8(
            64,
            64,
            &(0..64 * 64)
                .flat_map(|i| [255, 255, 255, if i % 64 < 32 { 255 } else { 128 }])
                .collect::<Vec<_>>(),
        )
        .unwrap();
        for samples in [1, 4] {
            let mut renderer = PathGpu::try_new(&device, samples).unwrap();
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
            let mut records = Vec::new();
            let mut sheet = vec![0u8; 64 * 6 * 64 * 6 * 4];
            let mut cell = 0;
            let mut check = |label: String,
                             paint: &dyn Fn(&mut dyn Canvas2D),
                             max_mae: f64,
                             max_ink_alpha: f64|
             -> Result<(), Box<dyn Error>> {
                let quality = if label.starts_with("glyph/") {
                    gpu::AaQuality::Reference
                } else {
                    gpu::AaQuality::Balanced
                };
                renderer.set_aa_quality(quality);
                let mut cpu = RasterCanvas::new_linear(64, 64);
                paint(&mut cpu);
                let expected = cpu.get_image_data(0, 0, 64, 64).unwrap();
                let mut c = GpuCanvas::new_linear(64, 64, None);
                paint(&mut c);
                assert!(!c.needs_cpu);
                let ops = c.into_ops();
                assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                let actual = readback(&device, &queue, &texture, 64)?;
                let mae = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| f64::from(a.abs_diff(*b)))
                    .sum::<f64>()
                    / actual.len() as f64;
                let mut ink = 0;
                let mut alpha_error = 0.;
                let mut premul_error = 0f32;
                let mut premul_sum = 0f64;
                for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
                    if a[3] > 0 || b[3] > 0 {
                        ink += 1;
                        alpha_error += f64::from(a[3].abs_diff(b[3]));
                    }
                    for i in 0..3 {
                        let decode = jadix_theme::color_space::linearize_display_srgb_component;
                        let error = (decode(a[i] as f32 / 255.) * a[3] as f32
                            - decode(b[i] as f32 / 255.) * b[3] as f32)
                            .abs();
                        premul_error = premul_error.max(error);
                        premul_sum += f64::from(error);
                    }
                }
                let premul_mae = (premul_sum + alpha_error) / actual.len() as f64;
                let ink_alpha = alpha_error / (ink as f64).max(1.);
                assert!(premul_mae<=max_mae&&ink_alpha<=max_ink_alpha,"{label} samples={samples} rgba_mae={mae} premul_mae={premul_mae} ink_alpha_mae={ink_alpha} premul_max={premul_error}");
                assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                let stats = renderer.last_stats();
                assert_eq!(
                    (
                        stats.buffer_allocations,
                        stats.texture_allocations,
                        stats.bind_group_allocations
                    ),
                    (0, 0, 0),
                    "warm allocation {label}"
                );
                assert_eq!(stats.queue_submissions, 1);
                if cell < 36 {
                    let (cx, cy) = (cell % 6, cell / 6);
                    for y in 0..64 {
                        for x in 0..64 {
                            let from = (y * 64 + x) * 4;
                            let to = ((cy * 64 + y) * 384 + cx * 64 + x) * 4;
                            sheet[to..to + 4].copy_from_slice(&actual[from..from + 4]);
                        }
                    }
                    cell += 1;
                }
                records.push(json!({"case":label,"quality":format!("{quality:?}"),"rgba_mae":mae,"linear_premultiplied_mae":premul_mae,"ink_alpha_mae":ink_alpha,"max_linear_premultiplied_error":premul_error,"premultiplied_mae_threshold":max_mae,"ink_alpha_threshold":max_ink_alpha}));
                Ok(())
            };
            for radius in [0., 8., 16.] {
                for sigma in [0., 0.25, 2., 12.] {
                    for t in 0..3 {
                        let paint = |c: &mut dyn Canvas2D| {
                            if t == 1 {
                                c.translate(14., -3.);
                                c.rotate(18.);
                                c.clip_rect(0., 0., 52., 52.);
                            }
                            if t == 2 {
                                c.set_transform([1., 0.15, 0.3, 0.7, -8., 5.]);
                                c.clip_image_mask(&mask);
                                c.push_layer(0.7, BlendMode::Screen);
                            }
                            c.draw_blurred_rounded_rect(
                                [16., 16., 32., 24.],
                                radius,
                                sigma,
                                [0.2, 0.5, 0.9, 0.8],
                            );
                            if t == 2 {
                                c.pop_layer();
                            }
                        };
                        check(
                            format!("blur/r={radius}/sigma={sigma}/transform={t}"),
                            &paint,
                            1.,
                            4.,
                        )?;
                    }
                }
            }
            for color in ["#ff000080", "#00ff00", "#0000ff", "#000000", "#ffffff"] {
                for invert in [false, true] {
                    for empty in [false, true] {
                        let paint = |c: &mut dyn Canvas2D| {
                            c.clear("#8040c0a0");
                            c.push_layer_with_options(LayerOptions {
                                opacity: 0.7,
                                compose: if invert {
                                    CompositeMode::LuminanceOut
                                } else {
                                    CompositeMode::LuminanceIn
                                },
                                clip: Some([4.5, 8., 55., 48.]),
                                mask: Some(mask.clone()),
                                ..Default::default()
                            });
                            if !empty {
                                c.set_fill(color);
                                c.fill_rect(0., 0., 64., 64.);
                            }
                            c.pop_layer();
                        };
                        check(
                            format!("luminance/{color}/invert={invert}/empty={empty}"),
                            &paint,
                            1.,
                            1.,
                        )?;
                    }
                }
            }
            for variant in 0..3 {
                let paint = |c: &mut dyn Canvas2D| {
                    c.set_fill_brush(Brush2D::gradient(
                        gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)")
                            .unwrap(),
                        [0., 0., 64., 64.],
                    ));
                    if variant == 1 {
                        c.set_transform([1., 0.1, 0.2, 0.8, -3., 5.]);
                        c.clip_image_mask(&mask);
                    }
                    c.fill_glyph_run(&run);
                    c.translate(0., 30.);
                    c.set_stroke("#20c080");
                    c.set_line_width(1.);
                    c.stroke_glyph_run(&run);
                    if variant == 2 {
                        c.push_luminance_layer(0.8, false);
                        c.set_fill("#ffffff");
                        c.fill_glyph_run(&run);
                        c.pop_layer();
                    }
                };
                check(format!("glyph/variant={variant}"), &paint, 2., 12.)?;
            }
            #[cfg(feature = "test-hooks")]
            {
                drop(check);
                renderer.use_general_cover_for_test(&device);
                let mut c = GpuCanvas::new_linear(64, 64, None);
                c.draw_blurred_rounded_rect([8., 12., 44., 32.], 8., 4., [1., 0., 0., 0.7]);
                let expected = c.get_image_data(0, 0, 64, 64).unwrap();
                let ops = c.into_ops();
                assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                let actual = readback(&device, &queue, &texture, 64)?;
                let alpha_mae = actual
                    .chunks_exact(4)
                    .zip(expected.chunks_exact(4))
                    .map(|(a, b)| f64::from(a[3].abs_diff(b[3])))
                    .sum::<f64>()
                    / 4096.;
                assert!(alpha_mae < 1.);
                records.push(json!({"case":"blur/general-cover","alpha_mae":alpha_mae}));
            }
            if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir)?;
                image::save_buffer(
                    dir.join(format!("content-matrix-{samples}.png")),
                    &sheet,
                    384,
                    384,
                    image::ColorType::Rgba8,
                )?;
                std::fs::write(
                    dir.join(format!("content-matrix-{samples}.json")),
                    serde_json::to_vec_pretty(
                        &json!({"status":"pass","samples":samples,"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,"cases":records,"reference":"CPU analytic/12-point Gaussian convolution and real font outlines; finite CPU/GPU AA envelopes"}),
                    )?,
                )?;
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires a native headless GPU adapter"]
    fn explicit_brushes_cross_backend_matrix() -> Result<(), Box<dyn Error>> {
        let TestGpu {
            backend,
            adapter,
            device,
            queue,
            _instance,
        } = test_gpu();
        let image = Image2D::from_rgba8(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 128, 255, 255, 0, 255,
            ],
        )
        .unwrap();
        let mask = Image2D::from_rgba8(8, 8, &[192; 256]).unwrap();
        let modes = [
            ExtendMode::Pad,
            ExtendMode::Repeat,
            ExtendMode::Reflect,
            ExtendMode::Decal,
        ];
        for samples in [1, 4] {
            let mut renderer = PathGpu::try_new(&device, samples).unwrap();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
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
            let mut records = Vec::new();
            let mut sheet = vec![0u8; 128 * 96 * 4];
            for (row, sampling) in [ImageSampling::Nearest, ImageSampling::Bilinear]
                .into_iter()
                .enumerate()
            {
                for (xm, x) in modes.into_iter().enumerate() {
                    for (ym, y) in modes.into_iter().enumerate() {
                        for (t, matrix) in [
                            [2., 0., 0., 2., 2., 2.],
                            [0., 2., -2., 0., 6., 2.],
                            [-2., 0., 0., 2., 6., 2.],
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            for variant in 0..3 {
                                let brush = Brush2D::image(image.clone())
                                    .with_extend(x, y)
                                    .with_sampling(sampling)
                                    .with_transform(matrix);
                                let paint = |c: &mut dyn Canvas2D| {
                                    if variant == 2 {
                                        c.clip_rect(1., 1., 6., 6.);
                                        c.clip_image_mask(&mask);
                                        c.push_layer(0.75, BlendMode::Screen);
                                    }
                                    c.set_global_alpha(0.8);
                                    if variant == 1 {
                                        c.set_stroke_brush(brush.clone());
                                        c.set_line_width(4.);
                                        c.stroke_path(&polyline(&[(0., 4.), (8., 4.)]));
                                    } else {
                                        c.set_fill_brush(brush.clone());
                                        c.set_fill_rule(FillRule::EvenOdd);
                                        c.fill_path(&decode_path_d(
                                            "M0 0 H8 V8 H0 Z M2 2 H6 V6 H2 Z",
                                        ));
                                    }
                                    if variant == 2 {
                                        c.pop_layer();
                                    }
                                };
                                let mut cpu = RasterCanvas::new_linear(8, 8);
                                paint(&mut cpu);
                                let expected = cpu.get_image_data(0, 0, 8, 8).unwrap();
                                let mut c = GpuCanvas::new_linear(8, 8, None);
                                paint(&mut c);
                                assert!(!c.needs_cpu);
                                let ops = c.into_ops();
                                assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                                let actual = readback(&device, &queue, &texture, 8)?;
                                let delta = actual
                                    .iter()
                                    .zip(&expected)
                                    .map(|(a, b)| a.abs_diff(*b))
                                    .max()
                                    .unwrap();
                                let premul_delta=actual.chunks_exact(4).zip(expected.chunks_exact(4)).flat_map(|(a,b)| {
                                    let decode=jadix_theme::color_space::linearize_display_srgb_component;
                                    (0..4).map(move |i| if i==3 {f32::from(a[3].abs_diff(b[3]))} else {
                                        (decode(a[i] as f32/255.)*a[3] as f32-decode(b[i] as f32/255.)*b[3] as f32).abs()
                                    })
                                }).fold(0f32,f32::max);
                                assert!(premul_delta<=3.,"brush {samples} {x:?}/{y:?} {sampling:?} transform={t} variant={variant}: linear premul delta={premul_delta}, straight={delta}, pixels={:?}", actual.chunks_exact(4).zip(expected.chunks_exact(4)).enumerate().filter(|(_, (a,b))| a.iter().zip(*b).any(|(a,b)| a.abs_diff(*b)>10)).take(8).collect::<Vec<_>>());
                                if variant != 2 {
                                    assert!(delta <= 6, "ungrouped brush straight delta={delta}");
                                }
                                assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                                let stats = renderer.last_stats();
                                assert_eq!(
                                    (
                                        stats.image_uploads,
                                        stats.texture_allocations,
                                        stats.buffer_allocations,
                                        stats.bind_group_allocations
                                    ),
                                    (0, 0, 0, 0)
                                );
                                if variant == 0 {
                                    for py in 0..8 {
                                        for px in 0..8 {
                                            let from = (py * 8 + px) * 4;
                                            let to = (((row * 3 + t) * 16 + py) * 128
                                                + (xm * 4 + ym) * 8
                                                + px)
                                                * 4;
                                            sheet[to..to + 4]
                                                .copy_from_slice(&actual[from..from + 4]);
                                        }
                                    }
                                }
                                records.push(json!({"x":format!("{x:?}"),"y":format!("{y:?}"),"sampling":format!("{sampling:?}"),"transform":t,"variant":variant,"max_rgba_delta":delta,"max_linear_premultiplied_delta":premul_delta}));
                            }
                        }
                    }
                }
            }
            // Adjacent patterns share one texture draw, even with different mappings/modes.
            let mut c = GpuCanvas::new_linear(8, 8, None);
            for x in 0..8 {
                c.set_fill_brush(
                    Brush2D::image(image.clone())
                        .with_extend(modes[x % 4], modes[(x + 1) % 4])
                        .with_sampling(if x % 2 == 0 {
                            ImageSampling::Nearest
                        } else {
                            ImageSampling::Bilinear
                        })
                        .with_transform([2., 0., 0., 2., x as f32, 0.]),
                );
                c.fill_rect(x as f32, 0., 1., 8.);
            }
            let expected = c.get_image_data(0, 0, 8, 8).unwrap();
            let ops = c.into_ops();
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            let actual = readback(&device, &queue, &texture, 8)?;
            assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 6));
            assert_eq!(
                renderer.last_stats().draw_calls,
                2,
                "8 pattern operations batch to one geometry draw + export"
            );
            records.push(json!({"batch_operations":8,"geometry_draws":1,"queue_submissions":renderer.last_stats().queue_submissions}));
            // Independent gradient brush geometry uses both table batches and the general cover.
            for general in [false, true] {
                #[cfg(feature = "test-hooks")]
                if general {
                    renderer.use_general_cover_for_test(&device);
                }
                #[cfg(not(feature = "test-hooks"))]
                if general {
                    continue;
                }
                for kind in [
                    "linear-gradient(to right,red,blue)",
                    "radial-gradient(red,blue)",
                    "conic-gradient(red,blue,red)",
                ] {
                    let brush = Brush2D::gradient(
                        gradient::parse_gradient(kind).unwrap(),
                        [0., 0., 8., 8.],
                    )
                    .with_transform([0., 1., -1., 0., 8., 0.]);
                    let paint = |c: &mut dyn Canvas2D| {
                        c.set_fill_brush(brush.clone());
                        c.fill_rect(0., 0., 4., 8.);
                        c.fill_rect(4., 0., 4., 8.);
                        c.set_stroke_brush(
                            Brush2D::image(image.clone()).with_transform([2., 0., 0., 2., 0., 0.]),
                        );
                        c.set_line_width(2.);
                        c.stroke_path(&polyline(&[(0., 4.), (8., 4.)]));
                    };
                    let mut c = GpuCanvas::new_linear(8, 8, None);
                    paint(&mut c);
                    let expected = c.get_image_data(0, 0, 8, 8).unwrap();
                    assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                    let actual = readback(&device, &queue, &texture, 8)?;
                    // Radial/conic center sampling is not equal to supersampled filtering.
                    // Assert exact linear mapping and bound radial/conic mean error separately.
                    let max = actual
                        .iter()
                        .zip(&expected)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    let mae = actual
                        .iter()
                        .zip(&expected)
                        .map(|(a, b)| f64::from(a.abs_diff(*b)))
                        .sum::<f64>()
                        / 256.;
                    if kind.starts_with("linear") {
                        assert!(max <= 3, "gradient mapping {general}: {max}");
                    } else {
                        assert!(mae <= 5., "gradient filtering {kind} {general}: {mae}");
                    }
                    records.push(json!({"gradient":kind,"general_cover":general,"max_rgba_delta":max,"rgba_mae":mae}));
                }
            }
            if let Some(directory) = std::env::var_os("JADIX_2D_EVIDENCE") {
                let dir = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&dir)?;
                image::save_buffer(
                    dir.join(format!("brush-matrix-{samples}.png")),
                    &sheet,
                    128,
                    96,
                    image::ColorType::Rgba8,
                )?;
                std::fs::write(
                    dir.join(format!("brush-matrix-{samples}.json")),
                    serde_json::to_vec_pretty(&json!({
                        "status":"pass","samples":samples,"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,
                        "cases":records,"ungrouped_pattern_straight_threshold":6,"pattern_linear_premultiplied_threshold":3,"linear_gradient_max_threshold":3,"nonlinear_gradient_mae_threshold":5,
                        "space":"linear-srgb","reference":"CPU independent texel extension; GPU MSAA/SSAA vs CPU center brush sample"
                    }))?,
                )?;
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires a native headless GPU adapter"]
    fn images_masks_reuse_and_resource_replacement() -> Result<(), Box<dyn Error>> {
        let TestGpu {
            backend,
            adapter,
            device,
            queue,
            _instance,
        } = test_gpu();
        for samples in [1, 4] {
            let mut renderer = PathGpu::try_new(&device, samples).unwrap();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
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
            let image = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0]).unwrap();
            let mut c = GpuCanvas::new(8, 8, None);
            c.set_image_sampling(ImageSampling::Bilinear);
            c.draw_image(&CanvasImageSource::Image(&image), 0.0, 0.0, 8.0, 8.0);
            let ops = c.into_ops();
            for frame in 0..3 {
                assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                let rgba = readback(&device, &queue, &texture, 8)?;
                let p = &rgba[(2 * 8 + 4) * 4..][..4];
                assert!(
                    p[0] >= 254 && p[2] == 0 && p[3].abs_diff(96) <= 1,
                    "samples={samples} {p:?}"
                );
                if frame > 0 {
                    let s = renderer.last_stats();
                    assert_eq!(
                        (
                            s.image_uploads,
                            s.texture_allocations,
                            s.buffer_allocations,
                            s.bind_group_allocations
                        ),
                        (0, 0, 0, 0)
                    );
                }
            }
            let mut alpha = vec![0u8; 8 * 8 * 4];
            for y in 0..8 {
                for x in 0..4 {
                    alpha[(y * 8 + x) * 4 + 3] = 255;
                }
            }
            let mask = Image2D::from_rgba8(8, 8, &alpha).unwrap();
            let replacement = Image2D::from_rgba8(1, 1, &[0, 255, 0, 255]).unwrap();
            let mut c = GpuCanvas::new(8, 8, None);
            c.clip_image_mask(&mask);
            c.draw_image(&CanvasImageSource::Image(&replacement), 0.0, 0.0, 8.0, 8.0);
            let ops = c.into_ops();
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert_eq!(&rgba[(2 * 8 + 1) * 4..][..4], &[0, 255, 0, 255]);
            assert_eq!(&rgba[(2 * 8 + 6) * 4..][..4], &[0, 0, 0, 0]);
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            assert_eq!(renderer.last_stats().image_uploads, 0);
            renderer.clear_image_cache();
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            assert_eq!(renderer.last_stats().image_uploads, 3); // white + mask + bitmap
            assert!(renderer.render(&device, &queue, &view, [8, 8], &[]));
            assert!(readback(&device, &queue, &texture, 8)?
                .iter()
                .all(|v| *v == 0));
            for frame in 0..4 {
                let mut c = GpuCanvas::new(8, 8, None);
                let left = if frame % 2 == 0 { "#ff0000" } else { "#00ff00" };
                c.set_fill_gradient(
                    gradient::parse_gradient(&format!("linear-gradient({left},{left})")).unwrap(),
                );
                c.fill_rect(0.0, 0.0, 4.0, 8.0);
                c.set_fill_gradient(
                    gradient::parse_gradient("linear-gradient(#0000ff,#0000ff)").unwrap(),
                );
                c.fill_rect(4.0, 0.0, 4.0, 8.0);
                let ops = c.into_ops();
                assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                assert_eq!(
                    renderer.last_stats().draw_calls,
                    2,
                    "two independent brushes share one draw plus resolve"
                );
                let rgba = readback(&device, &queue, &texture, 8)?;
                assert_eq!(
                    &rgba[(2 * 8 + 1) * 4..][..4],
                    if frame % 2 == 0 {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 255, 0, 255]
                    }
                );
                assert_eq!(&rgba[(2 * 8 + 6) * 4..][..4], &[0, 0, 255, 255]);
            }
            // Equal positions with different colors must remain distinct in
            // the index buffer, and adjacent batching must preserve SrcOver.
            let mut c = GpuCanvas::new_linear(8, 8, None);
            c.set_fill("#0000ff80");
            c.fill_rect(0., 0., 8., 8.);
            c.set_fill("#ff000080");
            c.fill_rect(0., 0., 8., 8.);
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert!(rgba[(3 * 8 + 3) * 4..][..4]
                .iter()
                .zip([213u8, 0, 156, 192])
                .all(|(a, b)| a.abs_diff(b) <= 2));
            let stats = renderer.last_stats();
            assert!(stats.unique_vertices < stats.indices);
            assert_eq!(stats.queue_submissions, 1);
            // Isolated groups apply opacity once, preserve ordering and keep
            // both straight and premultiplied output contracts.
            for blend in [
                BlendMode::Normal,
                BlendMode::Multiply,
                BlendMode::Screen,
                BlendMode::Overlay,
                BlendMode::Darken,
                BlendMode::Lighten,
                BlendMode::Difference,
                BlendMode::Exclusion,
            ] {
                let mut c = GpuCanvas::new(8, 8, None);
                c.set_fill("#0000ff");
                c.fill_rect(0., 0., 8., 8.);
                c.push_layer(0.5, blend);
                c.push_layer(1., BlendMode::Normal);
                c.set_fill("#ff0000");
                c.fill_rect(0., 0., 6., 8.);
                c.set_fill("#00ff00");
                c.fill_rect(2., 0., 4., 8.);
                c.pop_layer();
                c.pop_layer();
                c.set_fill("#ffffff");
                c.fill_rect(7., 0., 1., 8.);
                assert!(!c.needs_cpu);
                let ops = c.into_ops();
                for frame in 0..3 {
                    assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                    let rgba = readback(&device, &queue, &texture, 8)?;
                    let p = &rgba[(3 * 8 + 3) * 4..][..4];
                    let expected = match blend {
                        BlendMode::Multiply | BlendMode::Darken => [0, 0, 188, 255],
                        BlendMode::Screen
                        | BlendMode::Lighten
                        | BlendMode::Difference
                        | BlendMode::Exclusion => [0, 188, 255, 255],
                        BlendMode::Overlay => [0, 0, 255, 255],
                        BlendMode::Normal => [0, 188, 188, 255],
                        _ => unreachable!("legacy primary-color subset"),
                    };
                    assert!(
                        p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
                        "{samples} {blend:?}: {p:?} vs {expected:?}"
                    );
                    assert_eq!(&rgba[(3 * 8 + 7) * 4..][..4], &[255; 4]);
                    if frame > 0 {
                        let stats = renderer.last_stats();
                        assert_eq!(
                            (
                                stats.buffer_allocations,
                                stats.bind_group_allocations,
                                stats.texture_allocations
                            ),
                            (0, 0, 0)
                        );
                    }
                }
            }
            let mut c = GpuCanvas::new(8, 8, None);
            c.set_stroke("#ff000080");
            c.set_line_width(3.);
            c.stroke_path(&decode_path_d("M1 1 L7 7 M7 1 L1 7"));
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert!(
                rgba.chunks_exact(4).all(|p| p[3] <= 128),
                "stroke intersections blend once"
            );
            let mut c = GpuCanvas::new(8, 8, None);
            c.set_stroke("#ff000080");
            c.set_line_cap(LineCap::Butt);
            c.set_line_width(2.);
            c.set_line_dash(&[1., 1.]);
            c.scale(2., 1.);
            c.stroke_path(&polyline(&[(0., 4.), (4., 4.)]));
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert_eq!(&rgba[(4 * 8) * 4..][..4], &[255, 0, 0, 128]);
            assert_eq!(&rgba[(4 * 8 + 2) * 4..][..4], &[0; 4]);
            assert_eq!(&rgba[(6 * 8) * 4..][..4], &[0; 4]);
            let mut c = GpuCanvas::new(8, 8, None);
            c.push_layer(0.5, BlendMode::Screen);
            c.push_layer(0.5, BlendMode::Multiply);
            c.set_fill("#ff0000");
            c.fill_rect(0., 0., 8., 8.);
            c.pop_layer();
            c.pop_layer();
            let ops = c.into_ops();
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert!(
                rgba[0] >= 253 && rgba[3].abs_diff(64) <= 1,
                "nested opacity on transparent: {:?}",
                &rgba[..4]
            );
            for blend in [
                BlendMode::Normal,
                BlendMode::Multiply,
                BlendMode::Screen,
                BlendMode::Overlay,
                BlendMode::Darken,
                BlendMode::Lighten,
                BlendMode::Difference,
                BlendMode::Exclusion,
            ] {
                let paint = |c: &mut dyn Canvas2D| {
                    c.clear("#8040c0a0");
                    c.push_layer(0.65, blend);
                    c.set_fill("#40c080b0");
                    c.fill_rect(0., 0., 8., 8.);
                    c.set_fill_gradient(
                        gradient::parse_gradient("linear-gradient(#e0402080,#20a0e080)").unwrap(),
                    );
                    c.fill_rect(0., 0., 8., 8.);
                    c.pop_layer();
                };
                let mut cpu = RasterCanvas::new_linear(8, 8);
                paint(&mut cpu);
                let mut c = GpuCanvas::new_linear(8, 8, None);
                paint(&mut c);
                assert_eq!(cpu.get_image_data(3, 3, 1, 1), c.get_image_data(3, 3, 1, 1));
                let expected = cpu.get_image_data(0, 0, 8, 8).unwrap();
                assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                assert_eq!(renderer.last_stats().queue_submissions, 1);
                let rgba = readback(&device, &queue, &texture, 8)?;
                let error = rgba
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    error <= 3,
                    "linear parity {samples} {blend:?}: delta {error}"
                );
            }
            for frame in 0..8 {
                let mut c = GpuCanvas::new_linear(8, 8, None);
                if frame % 3 == 0 {
                    c.push_layer(0., BlendMode::Multiply);
                    c.set_fill("#ff0000");
                    c.fill_rect(0., 0., 8., 8.);
                    c.pop_layer();
                } else {
                    if frame % 3 == 1 {
                        c.set_fill("#0000ff");
                        c.fill_rect(0., 0., 8., 8.);
                    }
                    c.push_layer(0.5, BlendMode::Normal);
                    c.push_layer(0.5, BlendMode::Screen);
                    c.set_fill("#ff0000");
                    c.fill_rect(0., 0., 8., 8.);
                    c.pop_layer();
                    c.pop_layer();
                }
                let expected = c.get_image_data(0, 0, 8, 8).unwrap();
                assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                let rgba = readback(&device, &queue, &texture, 8)?;
                assert!(
                    rgba.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                    "structure change {frame}"
                );
                assert_eq!(renderer.last_stats().queue_submissions, 1);
            }
            // Gradient stroke brushes must use the GPU fill brush path, including CTM/dashes.
            let mut c = GpuCanvas::new_linear(8, 8, None);
            c.set_line_width(2.);
            c.set_line_cap(LineCap::Butt);
            c.set_line_dash(&[2., 2.]);
            c.set_stroke_gradient(
                gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap(),
            );
            c.stroke_path(&polyline(&[(0., 4.), (8., 4.)]));
            let expected = c.get_image_data(0, 0, 8, 8).unwrap();
            assert!(!c.needs_cpu);
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            let actual = readback(&device, &queue, &texture, 8)?;
            assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3));
            // Transient layer masks must expire, recycle and never retain stale bindings.
            for frame in 0..12 {
                let mask =
                    Image2D::from_rgba8(8, 8, &vec![if frame % 2 == 0 { 255 } else { 128 }; 256])
                        .unwrap();
                let mut c = GpuCanvas::new_linear(8, 8, None);
                c.push_layer_with_options(LayerOptions {
                    compose: CompositeMode::Copy,
                    mask: Some(mask),
                    ..Default::default()
                });
                c.set_fill("#40a0c0");
                c.fill_rect(0., 0., 8., 8.);
                c.pop_layer();
                let expected = c.get_image_data(0, 0, 8, 8).unwrap();
                assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                let actual = readback(&device, &queue, &texture, 8)?;
                assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3));
                assert!(renderer.last_stats().cached_images < 10);
            }
            // Default outer groups may fold; nested mask/composition must remain available.
            let mut c = GpuCanvas::new_linear(8, 8, None);
            c.push_layer(0.8, BlendMode::Screen);
            c.push_layer(0.9, BlendMode::Multiply);
            c.push_layer_with_options(LayerOptions {
                compose: CompositeMode::Copy,
                mask: Some(image.clone()),
                ..Default::default()
            });
            c.set_fill("#c08040");
            c.fill_rect(0., 0., 8., 8.);
            c.pop_layer();
            c.pop_layer();
            c.pop_layer();
            // Wrong-size masks define an empty final clip rather than widening access.
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            assert!(readback(&device, &queue, &texture, 8)?
                .iter()
                .all(|v| *v == 0));
            let full_mask = Image2D::from_rgba8(8, 8, &[128; 256]).unwrap();
            let mut c = GpuCanvas::new_linear(8, 8, None);
            c.push_layer(0.8, BlendMode::Screen);
            c.push_layer(0.9, BlendMode::Multiply);
            c.push_layer_with_options(LayerOptions {
                compose: CompositeMode::Copy,
                mask: Some(full_mask),
                ..Default::default()
            });
            c.set_fill("#c08040");
            c.fill_rect(0., 0., 8., 8.);
            c.pop_layer();
            c.pop_layer();
            c.pop_layer();
            let expected = c.get_image_data(0, 0, 8, 8).unwrap();
            assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
            assert!(readback(&device, &queue, &texture, 8)?
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 4));
            let mixes = [
                BlendMode::Normal,
                BlendMode::Multiply,
                BlendMode::Screen,
                BlendMode::Overlay,
                BlendMode::Darken,
                BlendMode::Lighten,
                BlendMode::Difference,
                BlendMode::Exclusion,
                BlendMode::ColorDodge,
                BlendMode::ColorBurn,
                BlendMode::HardLight,
                BlendMode::SoftLight,
                BlendMode::Hue,
                BlendMode::Saturation,
                BlendMode::Color,
                BlendMode::Luminosity,
            ];
            for blend in [
                BlendMode::ColorDodge,
                BlendMode::ColorBurn,
                BlendMode::Hue,
                BlendMode::Saturation,
                BlendMode::Color,
                BlendMode::Luminosity,
            ] {
                for (back, source) in [
                    ("#000000", "#ffffff"),
                    ("#ffffff", "#000000"),
                    ("#808080", "#808080"),
                ] {
                    let mut c = GpuCanvas::new_linear(8, 8, None);
                    c.clear(back);
                    c.push_layer(1., blend);
                    c.set_fill(source);
                    c.fill_rect(0., 0., 8., 8.);
                    c.pop_layer();
                    let expected = c.get_image_data(0, 0, 8, 8).unwrap();
                    assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                    let actual = readback(&device, &queue, &texture, 8)?;
                    assert!(
                        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                        "singular {blend:?} {back} {source}"
                    );
                }
            }
            let modes = [
                CompositeMode::Clear,
                CompositeMode::Copy,
                CompositeMode::Destination,
                CompositeMode::SourceOver,
                CompositeMode::DestinationOver,
                CompositeMode::SourceIn,
                CompositeMode::DestinationIn,
                CompositeMode::SourceOut,
                CompositeMode::DestinationOut,
                CompositeMode::SourceAtop,
                CompositeMode::DestinationAtop,
                CompositeMode::Xor,
                CompositeMode::Plus,
                CompositeMode::PlusLighter,
            ];
            let mask_pixels: Vec<_> = (0..64)
                .flat_map(|i| [255, 255, 255, (i % 8 * 32) as u8])
                .collect();
            let layer_mask = Image2D::from_rgba8(8, 8, &mask_pixels).unwrap();
            let mut sheet = vec![0u8; 16 * 8 * 14 * 8 * 4];
            let mut measurements = Vec::new();
            for (row, compose) in modes.into_iter().enumerate() {
                for (column, blend) in mixes.into_iter().enumerate() {
                    for variant in 0..3 {
                        let options = LayerOptions {
                            opacity: if variant == 2 { 0. } else { 0.65 },
                            blend,
                            compose,
                            clip: Some([1.5, 1., 5., 6.]),
                            mask: Some(layer_mask.clone()),
                            filters: Vec::new(),
                        };
                        let paint = |c: &mut dyn Canvas2D| {
                            c.set_fill("#8040c0a0");
                            c.fill_rect(0., 0., 8., 8.);
                            c.push_layer_with_options(options.clone());
                            if variant != 1 {
                                c.set_fill("#40c080b0");
                                c.fill_rect(0., 0., 6., 8.);
                                c.set_fill("#e0402080");
                                c.fill_rect(4., 2., 4., 4.);
                            }
                            c.pop_layer();
                        };
                        let mut c = GpuCanvas::new_linear(8, 8, None);
                        paint(&mut c);
                        let expected = c.get_image_data(0, 0, 8, 8).unwrap();
                        assert!(!c.needs_cpu);
                        let ops = c.into_ops();
                        assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                        let actual = readback(&device, &queue, &texture, 8)?;
                        let delta = actual
                            .iter()
                            .zip(&expected)
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        assert!(delta<=6,"matrix {samples} {blend:?} {compose:?} variant={variant} delta={delta}");
                        assert_eq!(renderer.last_stats().queue_submissions, 1);
                        // Repeated identical groups must not reallocate resources or reuse stale masks.
                        assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
                        let stats = renderer.last_stats();
                        assert_eq!(
                            (
                                stats.buffer_allocations,
                                stats.bind_group_allocations,
                                stats.texture_allocations
                            ),
                            (0, 0, 0)
                        );
                        let mut premul_delta = 0;
                        if variant == 0 {
                            assert!(renderer.render_premultiplied(
                                &device,
                                &queue,
                                &view,
                                [8, 8],
                                &ops
                            ));
                            let premul = readback(&device, &queue, &texture, 8)?;
                            let mut reference = expected.clone();
                            for p in reference.chunks_exact_mut(4) {
                                let a = p[3] as f32 / 255.;
                                for c in &mut p[..3] {
                                    *c = (jadix_theme::color_space::encode_linear_srgb_component(
                                        jadix_theme::color_space::linearize_display_srgb_component(
                                            *c as f32 / 255.,
                                        ) * a,
                                    ) * 255.)
                                        .round() as u8;
                                }
                            }
                            premul_delta = premul
                                .iter()
                                .zip(reference)
                                .map(|(a, b)| a.abs_diff(b))
                                .max()
                                .unwrap();
                            assert!(
                                premul_delta <= 4,
                                "premultiplied {samples} {blend:?} {compose:?}: {premul_delta}"
                            );
                            for y in 0..8 {
                                for x in 0..8 {
                                    let from = (y * 8 + x) * 4;
                                    let to = ((row * 8 + y) * 128 + column * 8 + x) * 4;
                                    sheet[to..to + 4].copy_from_slice(&actual[from..from + 4]);
                                }
                            }
                        }
                        measurements.push(json!({"mix":format!("{blend:?}"),"compose":format!("{compose:?}"),"variant":variant,"max_rgba_delta":delta,"premultiplied_delta":premul_delta}));
                    }
                }
            }
            if let Some(directory) = std::env::var_os("JADIX_2D_EVIDENCE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory)?;
                image::save_buffer(
                    directory.join(format!("layer-matrix-{samples}.png")),
                    &sheet,
                    128,
                    112,
                    image::ColorType::Rgba8,
                )?;
                std::fs::write(
                    directory.join(format!("layer-matrix-{samples}.json")),
                    serde_json::to_vec_pretty(
                        &json!({"status":"pass","samples":samples,"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,"cases":measurements,"threshold":6,"space":"linear-srgb","limits":"LDR RGBA8 groups; CPU float reference, not Vello pixel identity"}),
                    )?,
                )?;
            }
            for quality in [
                gpu::AaQuality::High,
                gpu::AaQuality::Reference,
                gpu::AaQuality::Balanced,
            ] {
                renderer.set_aa_quality(quality);
                let mut c = GpuCanvas::new(8, 8, None);
                c.set_fill("white");
                c.fill_rect(0., 0., 8., 8.);
                assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
                assert!(readback(&device, &queue, &texture, 8)?
                    .iter()
                    .all(|v| *v == 255));
            }
            let mut c = GpuCanvas::new(8, 8, None);
            c.set_fill("#ff000080");
            c.fill_rect(0.0, 0.0, 8.0, 8.0);
            let ops = c.into_ops();
            assert!(renderer.render_premultiplied(&device, &queue, &view, [8, 8], &ops));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert!(
                rgba[0].abs_diff(188) <= 1 && rgba[3] == 128,
                "linear premultiplied export: {:?}",
                &rgba[..4]
            );
            assert_eq!(
                renderer.last_stats().draw_calls,
                if samples == 4 { 1 } else { 2 }
            );
            assert!(renderer.render(&device, &queue, &view, [8, 8], &ops));
            let rgba = readback(&device, &queue, &texture, 8)?;
            assert_eq!(
                &rgba[..4],
                &[255, 0, 0, 128],
                "switch back to straight alpha"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "gpu_acceptance/area.rs"]
mod area_tests;

#[cfg(test)]
#[path = "gpu_acceptance/geometry_imaging.rs"]
mod geometry_imaging_tests;

#[cfg(test)]
#[path = "gpu_acceptance/sampling.rs"]
mod sampling_tests;

#[cfg(test)]
#[path = "gpu_acceptance/gpu_mips.rs"]
mod gpu_mip_tests;

#[cfg(test)]
#[path = "gpu_acceptance/initialization.rs"]
mod initialization_tests;

#[cfg(all(test, feature = "compute"))]
#[path = "gpu_acceptance/compute_effects.rs"]
mod compute_effects;

#[cfg(all(test, feature = "compute"))]
#[path = "gpu_acceptance/recording.rs"]
mod recording_tests;

#[cfg(test)]
#[path = "gpu_acceptance/matched.rs"]
mod matched_tests;
