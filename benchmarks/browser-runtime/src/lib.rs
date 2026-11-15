#[cfg(feature = "jadix")]
use jadix_2d::{wgpu, *};
use wasm_bindgen::prelude::*;
fn error(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}

#[cfg(feature = "jadix")]
#[wasm_bindgen]
pub async fn run_jadix(backend: &str) -> Result<String, JsValue> {
    std::panic::set_hook(Box::new(|info| {
        let global = js_sys::global();
        if let Ok(console) = js_sys::Reflect::get(&global, &JsValue::from_str("console")) {
            if let Ok(log) = js_sys::Reflect::get(&console, &JsValue::from_str("error")) {
                let _ = log
                    .unchecked_into::<js_sys::Function>()
                    .call1(&console, &JsValue::from_str(&info.to_string()));
            }
        }
    }));
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = if backend == "webgl" {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::BROWSER_WEBGPU
    };
    let instance = wgpu::Instance::new(descriptor);
    let surface = if backend == "webgl" {
        Some(
            instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(
                    web_sys::OffscreenCanvas::new(16, 16)?,
                ))
                .map_err(error)?,
        )
    } else {
        None
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: surface.as_ref(),
            ..Default::default()
        })
        .await
        .map_err(error)?;
    let info = adapter.get_info();
    let mut required = wgpu::Limits::downlevel_webgl2_defaults();
    #[cfg(feature = "compute")]
    if backend != "webgl" {
        required.max_compute_workgroup_size_x = 64;
        required.max_compute_workgroup_size_y = 8;
        required.max_compute_workgroup_size_z = 1;
        required.max_compute_invocations_per_workgroup = 64;
        required.max_compute_workgroups_per_dimension = 65535;
        required.max_storage_buffers_per_shader_stage = 1;
        required.max_storage_textures_per_shader_stage = 1;
        required.max_storage_buffer_binding_size = 4096;
    }
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: required,
            ..Default::default()
        })
        .await
        .map_err(error)?;
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("browser-float"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | if backend == "webgl" {
                wgpu::TextureUsages::empty()
            } else {
                wgpu::TextureUsages::TEXTURE_BINDING
            },
        view_formats: &[],
    });
    let image =
        Image2D::from_color_space(1, 1, &[1., 0., 0., 0.5], ColorSpaceTag::DisplayP3).unwrap();
    let mut canvas = GpuCanvas::new_linear(16, 16, None);
    canvas.set_fill_brush(
        Color2D::new(ColorSpaceTag::LinearSrgb, [4., -0.25, 0.5, 1.])
            .unwrap()
            .brush()
            .unwrap(),
    );
    canvas.fill_rect(0., 0., 16., 16.);
    canvas.push_layer_with_options(LayerOptions {
        filters: vec![LayerFilter::Gaussian { sigma: [0.5; 2] }],
        ..Default::default()
    });
    canvas.draw_image(&CanvasImageSource::Image(&image), 4., 4., 8., 8.);
    canvas.pop_layer();
    let ops = canvas.into_ops();
    let mut renderer =
        gpu::PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float)
            .ok_or_else(|| error("pipeline init"))?;
    renderer.set_aa_quality(gpu::AaQuality::Area);
    renderer.set_tight_layer_bounds(true);
    if !renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [16, 16],
        &ops,
    ) {
        return Err(error("render returned false"));
    }
    // Read an external float texture through the ordinary image pipeline, with
    // no CPU readback between the producer and consumer render passes.
    let imported = renderer
        .register_texture(
            target.clone(),
            gpu::GpuImageColor::LinearSrgb,
            gpu::GpuImageAlpha::Premultiplied,
        )
        .map_err(error)?;
    let mut c = GpuCanvas::new_linear(16, 16, None);
    c.draw_image(&CanvasImageSource::Image(&imported), 0., 0., 16., 16.);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("browser-external-consumer"),
        size: target.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    if !renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [16, 16],
        &c.into_ops(),
    ) {
        return Err(error("imported render failed"));
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4096,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let reject = reject.clone();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| match result {
                Ok(()) => {
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(e) => {
                    let _ = reject.call1(&JsValue::NULL, &error(e));
                }
            });
    });
    if backend == "webgl" {
        for _ in 0..100 {
            let _ = device.poll(wgpu::PollType::Poll);
            let delay = js_sys::Promise::new(&mut |resolve, _| {
                let global = js_sys::global();
                let f = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout"))
                    .unwrap()
                    .unchecked_into::<js_sys::Function>();
                let _ = f.call2(&global, &resolve, &JsValue::from_f64(5.));
            });
            wasm_bindgen_futures::JsFuture::from(delay).await?;
        }
    }
    let _ = device.poll(wgpu::PollType::Poll);
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    let data = buffer.slice(..).get_mapped_range().map_err(error)?;
    let pixel = |x: usize, y: usize| -> Vec<u16> {
        data[y * 256 + x * 8..y * 256 + x * 8 + 8]
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect()
    };
    let corner = pixel(0, 0);
    let center = pixel(8, 8);
    // Known IEEE binary16 encodings: 4.0=0x4400, -0.25=0xb400.
    if corner[0] != 0x4400 || corner[1] != 0xb400 || center == corner {
        let validation = scope.pop().await.map(|e| format!("{e:?}"));
        return Err(error((&corner, &center, validation)));
    }
    drop(data);
    buffer.unmap();
    let sampling_cases = sampling_browser(&device, &queue, &target, backend).await?;
    let imported_mips = imported_mips_browser(&device, &queue, &target, backend).await?;
    let compute = compute_browser(&device, &queue, backend).await?;
    let recording = recording_browser(&device, &queue, backend).await?;
    let cpu_scene = cpu_scene_browser(&device, &queue, &target, backend).await?;
    let capability_cases = geometry_imaging_browser(&device, &queue, &target, backend).await?;
    if let Some(e) = scope.pop().await {
        return Err(error(e));
    }
    Ok(serde_json::json!({"status":"pass","area_mode":true,"tight_layers":true,"compute":compute,"recording":recording,"cpu_scene":cpu_scene,"geometry_imaging":capability_cases,"sampling":sampling_cases,"imported_mips":imported_mips,"renderer":"jadix_2d","backend":backend,"adapter":info.name,"driver":info.driver,"hdr_corner_f16":corner,"composited_center_f16":center,"format":"Rgba16Float","import_route":if backend=="webgl"{"COPY_SRC-only staged on GPU"}else{"TEXTURE_BINDING"},"scope":"HDR/P3 + Gaussian + external float texture GPU-to-GPU + actual GPU readback; not full browser conformance"}).to_string())
}

#[cfg(feature = "vello-reference")]
#[wasm_bindgen]
pub async fn run_vello() -> Result<String, JsValue> {
    use wgpu_upstream as w;
    let mut desc = w::InstanceDescriptor::new_without_display_handle();
    desc.backends = w::Backends::BROWSER_WEBGPU;
    let instance = w::Instance::new(desc);
    let adapter = instance
        .request_adapter(&Default::default())
        .await
        .map_err(error)?;
    let info = adapter.get_info();
    let (device, queue) = adapter
        .request_device(&w::DeviceDescriptor::default())
        .await
        .map_err(error)?;
    let target = device.create_texture(&w::TextureDescriptor {
        label: None,
        size: w::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: w::TextureDimension::D2,
        format: w::TextureFormat::Rgba8Unorm,
        usage: w::TextureUsages::STORAGE_BINDING | w::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut scene = vello::Scene::new();
    scene.fill(
        vello::peniko::Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        vello::peniko::Color::from_rgba8(255, 0, 0, 255),
        None,
        &vello::kurbo::Rect::new(0., 0., 16., 16.),
    );
    let mut renderer = vello::Renderer::new(
        &device,
        vello::RendererOptions {
            antialiasing_support: vello::AaSupport::area_only(),
            ..Default::default()
        },
    )
    .map_err(error)?;
    renderer
        .render_to_texture(
            &device,
            &queue,
            &scene,
            &target.create_view(&Default::default()),
            &vello::RenderParams {
                base_color: vello::peniko::Color::TRANSPARENT,
                width: 16,
                height: 16,
                antialiasing_method: vello::AaConfig::Area,
            },
        )
        .map_err(error)?;
    let buffer = device.create_buffer(&w::BufferDescriptor {
        label: None,
        size: 4096,
        usage: w::BufferUsages::MAP_READ | w::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        w::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: w::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let reject = reject.clone();
        buffer
            .slice(..)
            .map_async(w::MapMode::Read, move |r| match r {
                Ok(()) => {
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(e) => {
                    let _ = reject.call1(&JsValue::NULL, &error(e));
                }
            });
    });
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    let data = buffer.slice(..).get_mapped_range().map_err(error)?;
    if data[..4] != [255, 0, 0, 255] {
        return Err(error(&data[..4]));
    }
    drop(data);
    buffer.unmap();
    Ok(serde_json::json!({"status":"pass","area_mode":true,"renderer":"vello 0.11.0 compute","backend":"webgpu","adapter":info.name,"scope":"one opaque fill with actual GPU readback"}).to_string())
}

#[cfg(feature = "jadix")]
#[wasm_bindgen]
pub fn css_cases() -> String {
    let mut context = ColorContext2D {
        current_color: Color2D::parse("#4080c0").unwrap(),
        ..Default::default()
    };
    context
        .variables
        .insert("--base".into(), "currentColor".into());
    context
        .variables
        .insert("--channels".into(), "255 0 0".into());
    let cases = [
        "currentColor",
        "var(--unknown, red)",
        "rgb(var(--channels) / calc(1 / 2))",
        "color-mix(in srgb-linear, red, blue)",
        "color-mix(in srgb, red 20%, blue 20%)",
        "color-mix(in srgb, rgb(255 0 0 / 0), blue)",
        "color-mix(in oklch longer hue, oklch(.6 .1 20), oklch(.6 .1 40))",
        "rgb(from var(--base) calc(r / 2) g b / calc(alpha / 2))",
        "color(from color(display-p3 .4 .2 .1) display-p3 calc(r / 2) g b)",
        "oklch(from oklch(.5 .1 60) calc(l + .1) calc(c / 2) calc(h + 30))",
        "lab(calc(25% * 2) 0 0)",
        "rgb(clamp(0, 300, 255) min(10,20) max(1,2))",
        "light-dark(red, blue)",
    ];
    let mut cases = cases.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for space in [
        "srgb",
        "srgb-linear",
        "display-p3",
        "a98-rgb",
        "prophoto-rgb",
        "rec2020",
        "xyz-d50",
        "xyz-d65",
        "lab",
        "lch",
        "oklab",
        "oklch",
        "hsl",
        "hwb",
    ] {
        cases.push(format!("color-mix(in {space}, #307080 30%, #805030 70%)"));
    }
    for (name, value, channels) in [
        ("hsl", "hsl(30 40% 50%)", "calc(h + 30) calc(s / 2) l"),
        ("hwb", "hwb(30 20% 30%)", "calc(h + 30) w b"),
        ("lab", "lab(50 10 -10)", "calc(l + 5) a b"),
        ("lch", "lch(50 15 60)", "l c calc(h + 30)"),
        ("oklab", "oklab(.5 .05 -.05)", "calc(l + .1) a b"),
    ] {
        cases.push(format!("{name}(from {value} {channels})"));
    }
    cases.push("color-mix(in srgb, rgb(none 0 0), red)".into());
    let rows = cases
        .iter()
        .map(|css| {
            let c = context
                .resolve(css)
                .unwrap()
                .0
                .convert(ColorSpaceTag::Srgb)
                .components;
            serde_json::json!({"css":css,"expected":c.map(|v|(v.clamp(0.,1.)*255.).round() as u8)})
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&rows).unwrap()
}

#[cfg(feature = "jadix")]
async fn browser_float_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    backend: &str,
) -> Result<Vec<f32>, JsValue> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4096,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let reject = reject.clone();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| match result {
                Ok(()) => {
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(e) => {
                    let _ = reject.call1(&JsValue::NULL, &error(e));
                }
            });
    });
    if backend == "webgl" {
        for _ in 0..100 {
            let _ = device.poll(wgpu::PollType::Poll);
            let delay = js_sys::Promise::new(&mut |resolve, _| {
                let global = js_sys::global();
                let f = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout"))
                    .unwrap()
                    .unchecked_into::<js_sys::Function>();
                let _ = f.call2(&global, &resolve, &JsValue::from_f64(5.));
            });
            wasm_bindgen_futures::JsFuture::from(delay).await?;
        }
    }
    let _ = device.poll(wgpu::PollType::Poll);
    wasm_bindgen_futures::JsFuture::from(promise).await?;
    let data = buffer.slice(..).get_mapped_range().map_err(error)?;
    let values = data
        .chunks_exact(256)
        .flat_map(|row| {
            row[..128].chunks_exact(2).map(|v| {
                let bits = u16::from_le_bytes([v[0], v[1]]);
                let sign = if bits & 0x8000 != 0 { -1. } else { 1. };
                let exponent = (bits >> 10) & 31;
                let mantissa = (bits & 1023) as f32;
                sign * if exponent == 0 {
                    mantissa * 2f32.powi(-24)
                } else {
                    (1. + mantissa / 1024.) * 2f32.powi(exponent as i32 - 15)
                }
            })
        })
        .collect::<Vec<f32>>();
    drop(data);
    buffer.unmap();
    Ok(values)
}

#[cfg(feature = "jadix")]
async fn geometry_imaging_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    let image =
        Image2D::from_rgba8(3, 1, &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255]).unwrap();
    let hdr = Image2D::from_linear_rgba32f(
        4,
        1,
        &[
            2., -0.25, 0.5, 0.5, 0.2, 0.8, 2., 1., 2., -0.25, 0.5, 0., 1.7, 0.3, 0.2, 1.,
        ],
    )
    .unwrap();
    let shape = PathGeometry2D::from_path(
        &filled_rect(1.125, 10.25, 0.5, 0.5),
        FillRule::NonZero,
        Default::default(),
    )
    .map_err(error)?;
    let shape = shape
        .boolean(&shape, PathBooleanOp::Intersection, Default::default())
        .map_err(error)?;
    let mut renderer =
        gpu::PathGpu::try_new_with_format(device, 1, wgpu::TextureFormat::Rgba16Float)
            .ok_or_else(|| error("capabilities init"))?;
    renderer.set_aa_quality(gpu::AaQuality::Area);
    let mut rows = Vec::new();
    for aa in [gpu::AaQuality::Area, gpu::AaQuality::Balanced] {
        renderer.set_aa_quality(aa);
        for quality in [ImageSampling::Bicubic, ImageSampling::Lanczos3] {
            let scene = Scene2D::record([16, 16], |c| {
                c.set_image_sampling(quality);
                c.draw_image(
                    &CanvasImageSource::Region {
                        image: &image,
                        rect: [1, 0, 1, 1],
                    },
                    0.,
                    0.,
                    8.,
                    8.,
                );
                c.draw_image(
                    &CanvasImageSource::Region {
                        image: &image,
                        rect: [0, 0, 1, 1],
                    },
                    8.,
                    0.,
                    8.,
                    8.,
                );
                c.set_fill("white");
                shape.fill(c);
                c.set_fill_brush(
                    Brush2D::image(hdr.clone())
                        .with_transform([2., 0., 0., 2., 8., 8.])
                        .with_extend(ExtendMode::Reflect, ExtendMode::Reflect)
                        .with_sampling(quality),
                );
                c.fill_rect(8., 8., 8., 8.);
            })
            .map_err(error)?;
            let mut canvas = GpuCanvas::new_linear(16, 16, None);
            scene.draw(&mut canvas, Default::default()).map_err(error)?;
            if !renderer.render_premultiplied(
                device,
                queue,
                &target.create_view(&Default::default()),
                [16, 16],
                &canvas.into_ops(),
            ) {
                return Err(error("capability render"));
            }
            let actual = browser_float_pixels(device, queue, target, backend).await?;
            let mut max = 0f32;
            for y in 0..16 {
                for x in 0..16 {
                    let expected = if y < 8 {
                        if x < 8 {
                            [0., 128. / 255., 0., 128. / 255.]
                        } else {
                            [1., 0., 0., 1.]
                        }
                    } else if x >= 8 {
                        let samples = if aa == gpu::AaQuality::Area { 1 } else { 2 };
                        let mut expected = [0.; 4];
                        for sy in 0..samples {
                            for sx in 0..samples {
                                let value = hdr.sample_linear(
                                    [
                                        (x as f32 + (sx as f32 + 0.5) / samples as f32 - 8.) / 2.,
                                        (y as f32 + (sy as f32 + 0.5) / samples as f32 - 8.) / 2.,
                                    ],
                                    quality,
                                    ExtendMode::Reflect,
                                    ExtendMode::Reflect,
                                );
                                for k in 0..4 {
                                    expected[k] += value[k] / (samples * samples) as f32;
                                }
                            }
                        }
                        expected
                    } else if x == 1 && y == 10 {
                        [0.25; 4]
                    } else {
                        [0.; 4]
                    };
                    for k in 0..4 {
                        max = max.max((actual[(y * 16 + x) * 4 + k] - expected[k]).abs());
                    }
                }
            }
            if max > 0.008 {
                return Err(error((
                    "geometry/image oracle",
                    format!("{quality:?}"),
                    max,
                )));
            }
            rows.push(serde_json::json!({"sampling":format!("{quality:?}"),"aa":format!("{aa:?}"),"max_linear_error":max,"boolean_alpha":actual[(10*16+1)*4+3]}));
        }
    }
    Ok(
        serde_json::json!({"status":"pass","cases":rows,"scope":"actual FP16 readback: geometric intersection before AA, cropped image domains, signed HDR pattern reconstruction"}),
    )
}

#[cfg(feature = "jadix")]
async fn sampling_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    let pixels = (0..63 * 31)
        .flat_map(|i| [if i % 2 == 0 { 2. } else { 0. }, -0.25, 1.2, 0.5])
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(63, 31, &pixels)
        .unwrap()
        .with_mipmaps(Default::default())
        .map_err(error)?;
    let mut renderer =
        gpu::PathGpu::try_new_with_format(device, 1, wgpu::TextureFormat::Rgba16Float)
            .ok_or_else(|| error("mip pipeline"))?;
    let mut rows = vec![];
    for aa in [gpu::AaQuality::Area, gpu::AaQuality::Balanced] {
        renderer.set_aa_quality(aa);
        for quality in [
            ImageSampling::Trilinear,
            ImageSampling::Anisotropic,
            ImageSampling::Ewa,
        ] {
            let mut c = GpuCanvas::new_linear(16, 16, None);
            c.set_fill_brush(
                Brush2D::image(image.clone())
                    .with_sampling(quality)
                    .with_transform([0.2, 0., 0., 0.5, -0.3, 0.2])
                    .with_extend(ExtendMode::Repeat, ExtendMode::Reflect),
            );
            c.fill_rect(0., 0., 16., 16.);
            if !renderer.render_premultiplied(
                device,
                queue,
                &target.create_view(&Default::default()),
                [16, 16],
                &c.into_ops(),
            ) {
                return Err(error("mip render"));
            }
            let actual = browser_float_pixels(device, queue, target, backend).await?;
            let mut max = 0f32;
            let n = if aa == gpu::AaQuality::Area { 1 } else { 2 };
            for y in 0..16 {
                for x in 0..16 {
                    let mut expected = [0.; 4];
                    for sy in 0..n {
                        for sx in 0..n {
                            let p = [
                                (x as f32 + (sx as f32 + 0.5) / n as f32 + 0.3) * 5.,
                                (y as f32 + (sy as f32 + 0.5) / n as f32 - 0.2) * 2.,
                            ];
                            let value = image.sample_linear_with_footprint(
                                p,
                                [5., 0.],
                                [0., 2.],
                                quality,
                                ExtendMode::Repeat,
                                ExtendMode::Reflect,
                            );
                            for k in 0..4 {
                                expected[k] += value[k] / (n * n) as f32;
                            }
                        }
                    }
                    for k in 0..4 {
                        max = max.max((actual[(y * 16 + x) * 4 + k] - expected[k]).abs());
                    }
                }
            }
            if max > 0.01 {
                return Err(error(("mip oracle", max)));
            }
            rows.push(serde_json::json!({"kind":"mips","quality":format!("{quality:?}"),"aa":format!("{aa:?}"),"max_error":max}));
        }
    }
    for n in [2, 4, 8] {
        let scene = Scene2D::record([16, 16], |c| {
            c.set_fill("#ff0000");
            c.fill_rect(0., 0., 7.25, 16.);
            c.set_fill("#0000ff");
            c.fill_rect(7.25, 0., 8.75, 16.);
            let clip = filled_rect(3.25, 3.25, 6., 6.);
            c.clip_path(&clip, FillRule::NonZero);
            c.clip_path(&clip, FillRule::NonZero);
            c.push_layer_with_options(LayerOptions {
                filters: vec![
                    LayerFilter::Offset {
                        offset: [0.5, 0.25],
                    },
                    LayerFilter::Gaussian { sigma: [0.25, 0.5] },
                ],
                ..Default::default()
            });
            c.set_fill("#00ff00");
            c.fill_rect(2., 2., 8., 8.);
            c.pop_layer();
        })
        .map_err(error)?;
        let sampled = scene
            .sampled(
                [16, 16],
                Default::default(),
                SceneSampling {
                    axis_samples: n,
                    ..Default::default()
                },
            )
            .map_err(error)?;
        let cpu = sampled.rasterize().map_err(error)?;
        let mut gpu = sampled
            .prepare_gpu(device, wgpu::TextureFormat::Rgba16Float)
            .map_err(error)?;
        if !gpu.render(
            device,
            queue,
            &target.create_view(&Default::default()),
            true,
        ) {
            return Err(error("whole-scene render"));
        }
        let actual = browser_float_pixels(device, queue, target, backend).await?;
        let mut max = 0f32;
        for (a, e) in actual
            .chunks_exact(4)
            .zip(cpu.linear_rgba32f().unwrap().chunks_exact(4))
        {
            for k in 0..4 {
                let v = if k == 3 { e[3] } else { e[k] * e[3] };
                max = max.max((a[k] - v).abs());
            }
        }
        if max > 0.012 {
            return Err(error(("whole-scene oracle", n, max)));
        }
        rows.push(serde_json::json!({"kind":"whole-scene","axis_samples":n,"max_error":max,"submissions":gpu.last_stats().queue_submissions}));
    }
    Ok(serde_json::json!({"status":"pass","cases":rows}))
}

#[cfg(feature = "jadix")]
async fn imported_mips_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    let mut rows = vec![];
    for copy_only in [false, true] {
        let external = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("browser-atlas"),
            size: wgpu::Extent3d {
                width: 39,
                height: 23,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | if copy_only {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::TEXTURE_BINDING
                },
            view_formats: &[],
        });
        let mut raw = vec![];
        let mut crop = vec![];
        for y in 0..23 {
            for x in 0..39 {
                let inside = (3..34).contains(&x) && (2..19).contains(&y);
                let p = if inside {
                    [
                        if (x + y) % 2 == 0 { 255 } else { 0 },
                        64,
                        192,
                        if x % 3 == 0 { 0 } else { 128 },
                    ]
                } else {
                    [0, 255, 0, 255]
                };
                raw.extend(p);
                if inside {
                    crop.extend(p);
                }
            }
        }
        queue.write_texture(
            external.as_image_copy(),
            &raw,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(39 * 4),
                rows_per_image: Some(23),
            },
            external.size(),
        );
        let cpu = Image2D::from_rgba8(31, 17, &crop)
            .unwrap()
            .with_mipmaps(Default::default())
            .map_err(error)?;
        let mut renderer =
            gpu::PathGpu::try_new_with_format(device, 1, wgpu::TextureFormat::Rgba16Float)
                .ok_or_else(|| error("gpu mip init"))?;
        renderer.set_aa_quality(gpu::AaQuality::Area);
        let handle = renderer
            .register_texture_region(
                external.clone(),
                [3, 2, 31, 17],
                gpu::GpuImageColor::Srgb,
                gpu::GpuImageAlpha::Straight,
                Some(Default::default()),
            )
            .map_err(error)?;
        for quality in [
            ImageSampling::Trilinear,
            ImageSampling::Anisotropic,
            ImageSampling::Ewa,
        ] {
            let mut c = GpuCanvas::new_linear(16, 16, None);
            c.set_fill_brush(
                Brush2D::image(handle.clone())
                    .with_sampling(quality)
                    .with_transform([0.2, 0., 0., 0.4, 0., 0.])
                    .with_extend(ExtendMode::Repeat, ExtendMode::Reflect),
            );
            c.fill_rect(0., 0., 16., 16.);
            let ops = c.into_ops();
            if !renderer.render_premultiplied(
                device,
                queue,
                &target.create_view(&Default::default()),
                [16, 16],
                &ops,
            ) {
                return Err(error("gpu mip render"));
            }
            if quality == ImageSampling::Trilinear
                && renderer.last_stats().mip_generation_passes != 4
            {
                return Err(error((
                    "gpu mip passes",
                    renderer.last_stats().mip_generation_passes,
                )));
            }
            let cold_compute_dispatches = renderer.last_stats().compute_dispatches;
            let actual = browser_float_pixels(device, queue, target, backend).await?;
            let mut max = 0f32;
            for y in 0..16 {
                for x in 0..16 {
                    let e = cpu.sample_linear_with_footprint(
                        [(x as f32 + 0.5) * 5., (y as f32 + 0.5) * 2.5],
                        [5., 0.],
                        [0., 2.5],
                        quality,
                        ExtendMode::Repeat,
                        ExtendMode::Reflect,
                    );
                    for k in 0..4 {
                        max = max.max((actual[(y * 16 + x) * 4 + k] - e[k]).abs());
                    }
                }
            }
            if max > 0.012 {
                return Err(error((
                    "gpu mip oracle",
                    copy_only,
                    format!("{quality:?}"),
                    max,
                )));
            }
            if !renderer.render_premultiplied(
                device,
                queue,
                &target.create_view(&Default::default()),
                [16, 16],
                &ops,
            ) || renderer.last_stats().mip_generation_passes != 0
            {
                return Err(error("warm gpu mip cache"));
            }
            rows.push(serde_json::json!({"copy_only":copy_only,"quality":format!("{quality:?}"),"max_error":max,"cold_compute_dispatches":cold_compute_dispatches,"warm_mip_passes":renderer.last_stats().mip_generation_passes}));
        }
    }
    Ok(serde_json::json!({"status":"pass","cases":rows}))
}

#[cfg(feature = "jadix")]
async fn cpu_scene_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    let hdr = Image2D::from_linear_rgba32f(1, 1, &[2., -0.125, 1., 0.5]).unwrap();
    let scene = Scene2D::record([16, 16], |c| {
        c.push_layer_with_options(LayerOptions {
            opacity: 0.5,
            ..Default::default()
        });
        c.draw_image(&CanvasImageSource::Image(&hdr), 4., 4., 8., 8.);
        c.pop_layer();
    })
    .map_err(error)?;
    let mut renderer = gpu::SceneRenderer2D::new(
        device,
        1,
        wgpu::TextureFormat::Rgba16Float,
        gpu::RendererOptions {
            use_cpu: true,
            ..Default::default()
        },
    )
    .ok_or_else(|| error("cpu renderer init"))?;
    renderer
        .render(
            device,
            queue,
            &target.create_view(&Default::default()),
            [16, 16],
            &scene,
            SceneOptions::default(),
            true,
        )
        .map_err(error)?;
    let pixels = browser_float_pixels(device, queue, target, backend).await?;
    let center = &pixels[(8 * 16 + 8) * 4..][..4];
    let expected = [0.5, -0.03125, 0.25, 0.25];
    let max = center
        .iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    if max > 0.002 || renderer.last_stats().cpu_rasterized_pixels != 1024 {
        return Err(error(("cpu scene", center, max)));
    }
    Ok(
        serde_json::json!({"status":"pass","max_error":max,"cpu_rasterized_pixels":renderer.last_stats().cpu_rasterized_pixels,"export_draws":renderer.last_stats().draw_calls}),
    )
}
#[cfg(feature = "jadix")]
async fn compute_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    #[cfg(not(feature = "compute"))]
    {
        let _ = (device, queue, backend);
        Ok(serde_json::json!({"status":"feature-disabled"}))
    }
    #[cfg(feature = "compute")]
    {
        use wgpu::util::DeviceExt;
        if backend == "webgl" {
            if device.limits().max_compute_invocations_per_workgroup != 0 {
                return Err(error("WebGL advertised compute"));
            }
            return Ok(
                serde_json::json!({"status":"unsupported-webgl","compiled_feature":wgpu::COMPUTE_FEATURE_ENABLED}),
            );
        }
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:None,source:wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var output:texture_storage_2d<rgba16float,write>; @compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>){if(all(id.xy<vec2(16u))){textureStore(output,vec2<i32>(id.xy),vec4(f32(id.x)*0.25,-0.125,2.,0.5));}}".into())});
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let args = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &[2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0],
            usage: wgpu::BufferUsages::INDIRECT,
        });
        let mut max = 0f32;
        for indirect in [false, true] {
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &output.create_view(&Default::default()),
                    ),
                }],
            });
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                if indirect {
                    pass.dispatch_workgroups_indirect(&args, 0)
                } else {
                    pass.dispatch_workgroups(2, 2, 1)
                }
            }
            queue.submit([encoder.finish()]);
            let pixels = browser_float_pixels(device, queue, &output, backend).await?;
            for (y, row) in pixels.chunks_exact(16 * 4).enumerate() {
                for (x, p) in row.chunks_exact(4).enumerate() {
                    let _ = y;
                    for (a, b) in p.iter().zip([x as f32 * 0.25, -0.125, 2., 0.5]) {
                        max = max.max((a - b).abs());
                    }
                }
            }
        }
        if max > 0.0001 {
            return Err(error(("compute storage oracle", max)));
        }
        Ok(
            serde_json::json!({"status":"pass","dispatches":2,"readback_pixels":512,"max_error":max}),
        )
    }
}

#[cfg(feature = "jadix")]
async fn recording_browser(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backend: &str,
) -> Result<serde_json::Value, JsValue> {
    #[cfg(not(feature = "compute"))]
    {
        let _ = (device, queue, backend);
        Ok(serde_json::json!({"status":"feature-disabled"}))
    }
    #[cfg(feature = "compute")]
    {
        use jadix_2d::gpu::low_level::*;
        if backend == "webgl" {
            return Ok(
                serde_json::json!({"status":"unsupported-webgl","engine_rejected":Engine::new(device,Default::default(),None).is_err()}),
            );
        }
        let mut engine = Engine::new(device, Default::default(), None).map_err(error)?;
        let shader=engine.add_compute_shader("recorded-hdr","@group(0) @binding(0) var dst:texture_storage_2d<rgba16float,write>; @compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>){if(all(id.xy<vec2(16u))){textureStore(dst,vec2<i32>(id.xy),vec4(f32(id.x)*.25,-0.125,2.,.5));}}","main",&[BindType::Image(ImageFormat::Rgba16Float)]).map_err(error)?;
        let image = ImageProxy::new(16, 16, ImageFormat::Rgba16Float);
        let mut r = Recording::default();
        let args = r.upload("args", vec![2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0]);
        r.dispatch_indirect(shader, args, 0, [image.into()]);
        let stats = engine.execute(queue, &r).map_err(error)?.stats;
        let pixels =
            browser_float_pixels(device, queue, engine.image(image).unwrap(), backend).await?;
        let mut max = 0f32;
        for row in pixels.chunks_exact(64) {
            for (x, p) in row.chunks_exact(4).enumerate() {
                for (a, b) in p.iter().zip([x as f32 * 0.25, -0.125, 2., 0.5]) {
                    max = max.max((a - b).abs());
                }
            }
        }
        if max > 0.0001 {
            return Err(error(("recorded-compute", max)));
        }
        let mut warm = Recording::default();
        warm.dispatch(shader, [2, 2, 1], [image.into()]);
        let warm = engine.execute(queue, &warm).map_err(error)?.stats;
        let mut free = Recording::default();
        free.free_image(image);
        free.free_buffer(args);
        engine.execute(queue, &free).map_err(error)?;
        if engine.live_resources() != 0 {
            return Err(error("recorded resource release"));
        }
        Ok(
            serde_json::json!({"status":"pass","max_error":max,"dispatches":stats.dispatches,"cold_compiles":stats.compiled_pipelines,"warm_compiles":warm.compiled_pipelines,"released_resources":true}),
        )
    }
}
