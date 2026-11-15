//! Matched input/phase comparison, using the unmodified published Vello renderer.
use super::*;
use vello::peniko::{
    Blob, Extend, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
};
fn image(
    input: &std::path::Path,
    name: &str,
    w: u32,
    h: u32,
    high: bool,
) -> Result<ImageBrush, Box<dyn Error>> {
    Ok(ImageBrush::new(ImageData {
        data: Blob::new(std::sync::Arc::new(std::fs::read(input.join(name))?)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: w,
        height: h,
    })
    .with_quality(if high {
        ImageQuality::High
    } else {
        ImageQuality::Medium
    }))
}
fn prepare(case: &str, input: &std::path::Path, frame: usize) -> Result<Scene, Box<dyn Error>> {
    let mut s = Scene::new();
    match case {
        "solid" => s.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &Rect::new(0., 0., 32., 32.),
        ),
        "bilinear" | "high" => {
            let brush = image(input, "white.rgba", 8, 8, case == "high")?;
            s.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                brush.as_ref(),
                Some(Affine::scale(4.)),
                &Rect::new(0., 0., 32., 32.),
            );
        }
        "npot-lifecycle" => {
            let brush = image(
                input,
                if frame.is_multiple_of(2) {
                    "npot-a.rgba"
                } else {
                    "npot-b.rgba"
                },
                31,
                17,
                true,
            )?;
            s.draw_image(
                brush.as_ref(),
                Affine::scale_non_uniform(32. / 31., 32. / 17.),
            );
        }
        "compound" => {
            let mut path = BezPath::new();
            for y in 0..50 {
                for x in 0..64 {
                    let a = x as f32 * 4. + 0.17;
                    let b = y as f32 * 4. + 0.23;
                    let r = a + 2.71;
                    let t = b + 2.63;
                    path.move_to((a as f64, b as f64));
                    path.line_to((r as f64, b as f64));
                    path.line_to((r as f64, t as f64));
                    path.line_to((a as f64, t as f64));
                    path.close_path();
                }
            }
            s.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(255, 0, 0, 128),
                None,
                &path,
            );
        }
        "oblique" => {
            let brush = image(input, "oblique.rgba", 127, 65, true)?
                .with_x_extend(Extend::Repeat)
                .with_y_extend(Extend::Reflect);
            s.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                brush.as_ref(),
                Some(Affine::new([
                    0.13f32 as f64,
                    0.07f32 as f64,
                    -0.11f32 as f64,
                    0.4f32 as f64,
                    3.,
                    4.,
                ])),
                &Rect::new(0., 0., 32., 32.),
            );
        }
        _ => unreachable!(),
    }
    Ok(s)
}
pub(super) fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    output: &std::path::Path,
    backend: &str,
    info: &wgpu::AdapterInfo,
    device_ms: f64,
) -> Result<(), Box<dyn Error>> {
    let input = output.parent().unwrap().parent().unwrap().join("inputs");
    let mut rows = Vec::new();
    for iteration in 0..2 {
        let start = Instant::now();
        let mut renderer = Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: std::num::NonZeroUsize::new(1),
                ..Default::default()
            },
        )?;
        let constructor_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut phases = Vec::new();
        for case in [
            "solid",
            "bilinear",
            "high",
            "npot-lifecycle",
            "compound",
            "oblique",
        ] {
            let size = if case == "compound" { 256 } else { 32 };
            let target = device.create_texture(&wgpu::TextureDescriptor {
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
            let view = target.create_view(&Default::default());
            let count = if case == "npot-lifecycle" { 24 } else { 6 };
            let mut times = Vec::new();
            let mut prepare_times = Vec::new();
            for frame in 0..count {
                let start = Instant::now();
                let scene = prepare(case, &input, frame)?;
                prepare_times.push(start.elapsed().as_secs_f64() * 1000.);
                let start = Instant::now();
                renderer.render_to_texture(
                    device,
                    queue,
                    &scene,
                    &view,
                    &RenderParams {
                        base_color: Color::TRANSPARENT,
                        width: size,
                        height: size,
                        antialiasing_method: AaConfig::Area,
                    },
                )?;
                device.poll(wgpu::PollType::wait_indefinitely())?;
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let start = Instant::now();
            let rgba = readback(device, queue, &target, size)?;
            let readback_ms = start.elapsed().as_secs_f64() * 1000.;
            if case == "solid" {
                assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
            }
            if case == "bilinear" || case == "high" {
                assert_eq!(&rgba[..4], &[255; 4]);
            }
            image::save_buffer(
                output.join(format!("{case}-{iteration}.png")),
                &rgba,
                size,
                size,
                image::ColorType::Rgba8,
            )?;
            phases.push(json!({"case":case,"size":size,"frames":count,"prepare_ms":prepare_times,"render_wait_ms":times,"readback_ms":readback_ms}));
        }
        rows.push(json!({"iteration":iteration,"constructor_ms":constructor_ms,"phases":phases}));
    }
    std::fs::write(
        output.join("matched.json"),
        serde_json::to_vec_pretty(
            &json!({"renderer":"vello","version":"0.11.0","backend":backend,"adapter":info.name,"driver":info.driver,"device_ms":device_ms,"aa":"Area","shader_compiler":"FXC on DX12","aa_support":"Area only","init_threads":1,"rows":rows,"output":"Rgba8Unorm encoded straight sRGB","image_contract":"Published Medium vs High; no EWA, GPU-only FP16 mip or sampled-scene phase is claimed equivalent."}),
        )?,
    )?;
    Ok(())
}
