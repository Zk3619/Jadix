use glyphon::{
    Attrs, Buffer, Cache, Color, ContentType, CustomGlyph, Family, FontSystem, Metrics,
    RasterizedCustomGlyph, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer, Viewport,
};
#[cfg(feature = "baseline")]
use glyphon_base as glyphon;
#[cfg(feature = "patched")]
use glyphon_patch as glyphon;
use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake},
    time::Instant,
};

fn block_on<F: Future>(future: F) -> F::Output {
    struct Waker;
    impl Wake for Waker {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Arc::new(Waker).into();
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn hash_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> u64 {
    let size = 1024 * 1024 * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
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
                bytes_per_row: Some(4096),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: 1024,
            height: 1024,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0),
        "blank render"
    );
    pixels.iter().fold(14695981039346656037u64, |hash, &byte| {
        (hash ^ byte as u64).wrapping_mul(1099511628211)
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let samples: usize = args[3].parse().unwrap();
    let backends = if args[1] == "dx12" {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::GL
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = if args.get(4).is_some_and(|name| !name.is_empty()) {
        block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .find(|adapter| {
                adapter
                    .get_info()
                    .name
                    .to_lowercase()
                    .contains(&args[4].to_lowercase())
            })
            .expect("requested adapter")
    } else {
        block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("adapter")
    };
    eprintln!(
        "adapter={:?}; metrics={}; samples={samples}",
        adapter.get_info(),
        cfg!(feature = "metrics")
    );
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    }))
    .unwrap();
    let mut db = glyphon::fontdb::Database::new();
    db.load_font_file(&args[2]).unwrap();
    db.set_sans_serif_family("Noto Sans CJK SC");
    let mut fonts = FontSystem::new_with_locale_and_db("zh-CN".into(), db);
    let cache = Cache::new(&device);
    let mut viewport = Viewport::new(&device, &cache);
    viewport.update(
        &queue,
        Resolution {
            width: 1024,
            height: 1024,
        },
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1024,
            height: 1024,
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
    println!("scenario,sample,prepare_us,complete_us,uploads,upload_bytes,mirror_bytes,gpu_atlas_bytes,mask_size,color_size,raster_calls,pixel_hash");
    for scenario in [
        "latin_small_cold",
        "latin_cold",
        "cjk_cold",
        "cjk_cached",
        "cjk_scroll",
        "mask_cold",
        "color_cold",
        "color_cached",
        "color_scroll",
        "color_single",
        "color_sparse",
        "color_growth",
    ] {
        if args.get(5).is_some_and(|filter| {
            !filter.is_empty() && !filter.split(',').any(|name| name == scenario)
        }) {
            continue;
        }
        let custom = scenario.starts_with("mask") || scenario.starts_with("color");
        let color = scenario.starts_with("color");
        let cold = scenario.ends_with("cold");
        let mut buffers = Vec::new();
        let pages = if scenario == "cjk_scroll" { 32 } else { 1 };
        for page in 0..pages {
            let text = if custom {
                String::new()
            } else if scenario == "latin_small_cold" {
                "Dialog Save Cancel".into()
            } else if scenario == "latin_cold" {
                (33..127).filter_map(char::from_u32).collect()
            } else {
                let count = if pages > 1 { 128 } else { 1024 };
                (0..count)
                    .map(|i| char::from_u32(0x4e00 + page * 128 + i).unwrap())
                    .collect()
            };
            let mut buffer = Buffer::new(&mut fonts, Metrics::new(20.0, 25.0));
            buffer.set_size(Some(1000.0), Some(1000.0));
            buffer.set_text(
                &text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut fonts, false);
            buffers.push(buffer);
        }
        let mut atlas = TextAtlas::new(&device, &queue, &cache, wgpu::TextureFormat::Rgba8Unorm);
        let mut renderer = TextRenderer::new(&mut atlas, &device, Default::default(), None);
        let mut swash = SwashCache::new();
        // Warm the GPU and driver; cold scenarios still get a fresh atlas and SwashCache per sample.
        for iteration in 0..samples + 10 {
            if cold || (scenario == "color_growth" && iteration % 16 == 0) {
                atlas = TextAtlas::new(&device, &queue, &cache, wgpu::TextureFormat::Rgba8Unorm);
                renderer = TextRenderer::new(&mut atlas, &device, Default::default(), None);
                swash = SwashCache::new();
            }
            atlas.trim();
            let count = if scenario == "color_scroll" {
                64
            } else if scenario == "color_growth" {
                64 + (iteration % 16) * 64
            } else {
                1024
            };
            let custom_glyphs: Vec<CustomGlyph> = if custom {
                (0..count)
                    .map(|i| {
                        let id = if scenario == "color_scroll" {
                            iteration * 64 + i
                        } else if scenario == "color_single" && i == iteration % 1024 {
                            1024 + iteration
                        } else if scenario == "color_sparse" && i % 16 == 0 {
                            1024 + iteration * 64 + i / 16
                        } else {
                            i
                        };
                        CustomGlyph {
                            id: id.try_into().unwrap(),
                            left: (i % 32) as f32 * 30.0,
                            top: (i / 32) as f32 * 30.0,
                            width: 24.0,
                            height: 24.0,
                            snap_to_physical_pixel: true,
                            ..Default::default()
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let areas = [TextArea {
                buffer: &buffers[iteration % buffers.len()],
                left: 0.0,
                top: 0.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: 1024,
                    bottom: 1024,
                },
                default_color: Color::rgb(240, 220, 180),
                custom_glyphs: &custom_glyphs,
            }];
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            #[cfg(feature = "metrics")]
            glyphon::bench_metrics::take();
            let mut raster_calls = 0;
            let start = Instant::now();
            renderer
                .prepare_with_custom(
                    &device,
                    &queue,
                    &mut fonts,
                    &mut atlas,
                    &viewport,
                    areas,
                    &mut swash,
                    |request| {
                        raster_calls += 1;
                        let channels = if color { 4 } else { 1 };
                        let mut data =
                            vec![0; request.width as usize * request.height as usize * channels];
                        for (index, pixel) in data.chunks_exact_mut(channels).enumerate() {
                            let value = 80 + ((index + request.id as usize) % 176) as u8;
                            pixel[0] = value;
                            if color {
                                pixel[1] = 200;
                                pixel[2] = 100;
                                pixel[3] = 255;
                            }
                        }
                        Some(RasterizedCustomGlyph {
                            data,
                            content_type: if color {
                                ContentType::Color
                            } else {
                                ContentType::Mask
                            },
                        })
                    },
                )
                .unwrap();
            let prepare_us = start.elapsed().as_secs_f64() * 1e6;
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                renderer.render(&atlas, &viewport, &mut pass).unwrap();
            }
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let complete_us = start.elapsed().as_secs_f64() * 1e6;
            #[cfg(feature = "metrics")]
            let (uploads, upload_bytes) = glyphon::bench_metrics::take();
            #[cfg(not(feature = "metrics"))]
            let (uploads, upload_bytes) = (0, 0);
            #[cfg(feature = "metrics")]
            let (mirror, gpu_bytes, mask_size, color_size) = atlas.bench_memory();
            #[cfg(not(feature = "metrics"))]
            let (mirror, gpu_bytes, mask_size, color_size) = (0, 0, 0, 0);
            let hash = if iteration == samples + 9 {
                hash_pixels(&device, &queue, &texture)
            } else {
                0
            };
            if iteration >= 10 {
                println!("{scenario},{},{prepare_us:.3},{complete_us:.3},{uploads},{upload_bytes},{mirror},{gpu_bytes},{mask_size},{color_size},{raster_calls},{hash}", iteration - 10);
            }
        }
    }
}
