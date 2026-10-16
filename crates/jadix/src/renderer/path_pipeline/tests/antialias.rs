use super::*;
use crate::core::tree::Element;
use crate::primitives::path::{
    attach_path, path_gpu_from_element_attrs, polyline, request_path_rt_readback,
    take_path_rt_readback, PathStyle, StrokeLinecap,
};
use crate::renderer::offscreen::OffscreenGpu;

fn setup() -> (OffscreenGpu, TexturedQuadPipeline) {
    let gpu = OffscreenGpu::new().expect("headless GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 96,
        height: 96,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    (gpu, textured)
}

fn render(
    gpu: &OffscreenGpu,
    textured: &TexturedQuadPipeline,
    path: &mut PathGpu,
    el: &Element,
    dpi: f32,
) -> crate::renderer::offscreen::RgbaImage {
    let _ = take_path_jobs();
    crate::style::set_gradient_raster_dpi(dpi);
    let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    crate::style::set_gradient_raster_dpi(1.0);
    request_path_rt_readback(&key);
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, textured, &mut images));
    let (width, height, pixels) = take_path_rt_readback(&key).expect("path pixels");
    crate::renderer::offscreen::RgbaImage {
        width,
        height,
        pixels,
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn resident_path_recollect_does_not_tessellate_again() {
    use crate::primitives::path::gpu_op_build_count;
    let (gpu, textured) = setup();
    let el = attach_path(
        Element::new(),
        &crate::primitives::path::decode_path_d("M 12 72 C 22 -14 76 118 84 19"),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 2.5),
    );
    let mut path = PathGpu::try_new(&gpu.device, 1).unwrap();
    assert!(path.stencil_eo.is_none() && path.stencil_nz.is_none() && path.cover.is_none());
    let mut images = HashMap::new();
    let _ = take_path_jobs();
    crate::style::set_gradient_raster_dpi(1.0);
    let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let original = take_path_rt_readback(&key).unwrap();
    assert!(
        path.stencil_eo.is_none() && path.stencil_nz.is_none() && path.cover.is_none(),
        "stroke jobs must not compile fill programs"
    );
    let built = gpu_op_build_count();
    for _ in 0..4 {
        path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
        assert!(!path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    }
    assert_eq!(
        gpu_op_build_count(),
        built,
        "resident recollect must skip tessellation"
    );
    images.remove(&key);
    path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 1,
        "eviction must rebuild geometry"
    );
    assert_eq!(
        take_path_rt_readback(&key).unwrap(),
        original,
        "eviction must preserve pixels"
    );

    path_gpu_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    let mut forced = take_path_jobs().pop().unwrap();
    forced.force_redraw = true;
    crate::primitives::path::queue_path_job(forced);
    request_path_rt_readback(&key);
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 2,
        "explicit redraw must rebuild"
    );
    assert_eq!(take_path_rt_readback(&key).unwrap(), original);

    let (resized, _, _) = path_gpu_from_element_attrs(&el.attrs, 80.0, 64.0).unwrap();
    assert_ne!(
        resized, key,
        "dimensions must remain part of texture identity"
    );
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    assert_eq!(
        gpu_op_build_count(),
        built + 3,
        "new dimensions must rebuild"
    );
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_edges_have_subpixel_coverage() {
    let (gpu, textured) = setup();
    let triangle = [
        crate::primitives::path::PathCommand::MoveTo((11.2, 13.4)),
        crate::primitives::path::PathCommand::LineTo((83.7, 29.8)),
        crate::primitives::path::PathCommand::LineTo((24.6, 82.1)),
        crate::primitives::path::PathCommand::Close,
    ];
    let fill = attach_path(
        Element::new(),
        &triangle,
        &PathStyle::fill([0.2, 0.5, 0.8, 1.0]),
    );
    let line = attach_path(
        Element::new(),
        &polyline(&[(9.3, 16.2), (82.4, 72.8)]),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 1.25).linecap(StrokeLinecap::Butt),
    );
    let curve = attach_path(
        Element::new(),
        &crate::primitives::path::decode_path_d("M 12.3 72.4 C 22.7 -14.6 76.1 118.2 83.6 18.7"),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 2.5),
    );
    let caps = attach_path(
        Element::new(),
        &polyline(&[(26.3, 34.6), (69.2, 61.8)]),
        &PathStyle::stroke([0.2, 0.5, 0.8, 1.0], 24.0),
    );
    for samples in [1, 4] {
        let mut path = PathGpu::try_new(&gpu.device, samples).expect("path pipeline");
        for dpi in [1.0, 1.5, 2.0] {
            for (name, el) in [
                ("fill", &fill),
                ("thin-stroke", &line),
                ("curve", &curve),
                ("round-caps", &caps),
            ] {
                let img = render(&gpu, &textured, &mut path, el, dpi);
                let partial = img
                    .pixels
                    .chunks_exact(4)
                    .filter(|p| p[3] > 0 && p[3] < 255)
                    .count();
                assert!(partial > 50, "{name} at {dpi}, {samples} samples has aliased edges: {partial} partial pixels");
                if name == "fill" {
                    let p = img.pixel((40.0 * dpi) as u32, (40.0 * dpi) as u32).unwrap();
                    assert_eq!(p[3], 255, "antialiasing must keep interiors opaque");
                    for (channel, expected) in p[..3].iter().zip([51, 128, 204]) {
                        assert!(
                            (*channel as i32 - expected).abs() <= 2,
                            "path must preserve display sRGB color, got {p:?}"
                        );
                    }
                }
                if let Ok(dir) = std::env::var("JADIX_AA_CAPTURE_DIR") {
                    image::save_buffer(
                        std::path::Path::new(&dir).join(format!("{name}-{samples}-{dpi}.png")),
                        &img.pixels,
                        img.width,
                        img.height,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_lazy_gradients_match_general_cover() {
    use crate::primitives::path::{filled_rect, register_path_scene, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    for samples in [1, 4] {
        let begin = std::time::Instant::now();
        let mut path = PathGpu::try_new(&gpu.device, samples).expect("path pipeline");
        eprintln!(
            "path init samples={samples}: {:.3}ms",
            begin.elapsed().as_secs_f64() * 1000.0
        );
        assert!(path.gradient_cover.is_none());
        let mut reference = PathGpu::try_new(&gpu.device, samples).unwrap();
        let face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::NotEqual,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        reference.cover = Some(make_pipeline(
            &gpu.device,
            &reference.layout,
            &reference.shader,
            samples,
            wgpu::ColorWrites::ALL,
            face,
            face,
            "jadix-path-cover",
        ));
        for gradient in [
            None,
            Some("linear-gradient(35deg, #ff000080, #0044ff)"),
            Some("radial-gradient(circle, #ffffff, #00000080)"),
            None,
        ] {
            let mut style = PathStyle::fill([0.3, 0.6, 0.8, 0.7]);
            style.fill_gradient = gradient.map(|css| crate::style::parse_gradient(css).unwrap());
            let scene = register_path_scene(
                vec![(filled_rect(9.3, 12.4, 70.5, 66.2), style)],
                Vec::new(),
                0.0,
                96.0,
                96.0,
            );
            let mut el = Element::new();
            el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
            for dpi in [1.0, 1.5, 2.0] {
                let actual = render(&gpu, &textured, &mut path, &el, dpi);
                let expected = render(&gpu, &textured, &mut reference, &el, dpi);
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "gradient={gradient:?} samples={samples} dpi={dpi}"
                );
            }
            assert!(path.stencil_nz.is_some());
            assert!(
                path.stencil_eo.is_none(),
                "nonzero fills must not compile even-odd"
            );
            if gradient.is_some() {
                assert!(path.gradient_cover.is_some());
            }
        }
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_translucent_layers_preserve_color() {
    use crate::primitives::path::{filled_rect, register_path_scene, ATTR_PATH_SCENE};
    let (gpu, textured) = setup();
    let layers = vec![
        (
            filled_rect(10.0, 10.0, 70.0, 70.0),
            PathStyle::fill([1.0, 0.0, 0.0, 0.5]),
        ),
        (
            filled_rect(30.0, 30.0, 60.0, 60.0),
            PathStyle::fill([0.0, 0.0, 1.0, 0.5]),
        ),
    ];
    let scene = register_path_scene(layers, Vec::new(), 0.0, 96.0, 96.0);
    let mut el = Element::new();
    el.set_attr(ATTR_PATH_SCENE, format!("{scene:016x}"));
    let mut path = PathGpu::try_new(&gpu.device, 1).expect("path pipeline");
    let img = render(&gpu, &textured, &mut path, &el, 1.0);
    let p = img.pixel(50, 50).unwrap();
    let linear = |c: u8| {
        let s = c as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    assert!(
        (p[3] as f32 / 255.0 - 0.75).abs() < 0.01,
        "source-over alpha: {p:?}"
    );
    assert!(
        (linear(p[0]) - 1.0 / 3.0).abs() < 0.02 && (linear(p[2]) - 2.0 / 3.0).abs() < 0.02,
        "path textures must export straight color after premultiplied compositing: {p:?}"
    );
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn shared_gpu_canvas_aa_keeps_clipping_and_cached_paint() {
    use crate::core::tree::LayoutStatus;
    use crate::primitives::canvas::{canvas, canvas_raster_from_element_attrs};
    let (gpu, textured) = setup();
    let el: Element = canvas()
        .w(96.0)
        .h(96.0)
        .paint(|ctx, _| {
            ctx.clip_rect(12.0, 12.0, 72.0, 72.0);
            ctx.set_fill("#336699");
            ctx.fill_path(&crate::primitives::path::filled_circle(
                48.0, 48.0, 39.0, 16,
            ));
            LayoutStatus::Clean
        })
        .into();
    let _ = take_path_jobs();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 96.0, 96.0).unwrap();
    request_path_rt_readback(&key);
    let mut path = PathGpu::try_new(&gpu.device, 1).expect("path pipeline");
    let mut images = HashMap::new();
    assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
    let (w, h, rgba) = take_path_rt_readback(&key).unwrap();
    assert_eq!(
        [w, h],
        [96, 96],
        "SSAA must preserve output texture dimensions"
    );
    let partial = rgba
        .chunks_exact(4)
        .filter(|p| p[3] > 0 && p[3] < 255)
        .count();
    assert!(partial > 50, "Canvas curves must also receive subpixel AA");
    for y in 0..h {
        for x in 0..w {
            if !(12..84).contains(&x) || !(12..84).contains(&y) {
                assert_eq!(
                    rgba[((y * w + x) * 4 + 3) as usize],
                    0,
                    "supersampling must keep Canvas scissor clipping at ({x}, {y})"
                );
            }
        }
    }
    assert_eq!(
        canvas_raster_from_element_attrs(&el.attrs, 96.0, 96.0)
            .unwrap()
            .0,
        key
    );
    assert!(
        !path.flush(&gpu.device, &gpu.queue, &textured, &mut images),
        "static Canvas paint must reuse its resolved texture"
    );
}
