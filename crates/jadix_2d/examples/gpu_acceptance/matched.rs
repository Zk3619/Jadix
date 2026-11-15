use super::*;
fn prepare(case: &str, input: &Path, frame: usize) -> Result<Vec<PathGpuOp>, Box<dyn Error>> {
    let size = if case == "compound" { 256 } else { 32 };
    let mut c = GpuCanvas::new_linear(size, size, None);
    match case {
        "solid" => {
            c.set_fill("#ff0000");
            c.fill_rect(0., 0., 32., 32.);
        }
        "bilinear" | "high" => {
            let image = Image2D::from_rgba8(8, 8, &std::fs::read(input.join("white.rgba"))?)
                .unwrap()
                .with_mipmaps(Default::default())?;
            c.set_fill_brush(
                Brush2D::image(image)
                    .with_transform([4., 0., 0., 4., 0., 0.])
                    .with_sampling(if case == "high" {
                        ImageSampling::Ewa
                    } else {
                        ImageSampling::Bilinear
                    }),
            );
            c.fill_rect(0., 0., 32., 32.);
        }
        "npot-lifecycle" => {
            let image = Image2D::from_rgba8(
                31,
                17,
                &std::fs::read(input.join(if frame % 2 == 0 {
                    "npot-a.rgba"
                } else {
                    "npot-b.rgba"
                }))?,
            )
            .unwrap()
            .with_mipmaps(Default::default())?;
            c.set_image_sampling(ImageSampling::Ewa);
            c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 32., 32.);
        }
        "compound" => {
            let mut path = Vec::new();
            for y in 0..50 {
                for x in 0..64 {
                    path.extend(filled_rect(
                        x as f32 * 4. + 0.17,
                        y as f32 * 4. + 0.23,
                        2.71,
                        2.63,
                    ));
                }
            }
            c.set_fill("#ff000080");
            c.fill_path(&path);
        }
        "oblique" => {
            let image = Image2D::from_rgba8(127, 65, &std::fs::read(input.join("oblique.rgba"))?)
                .unwrap()
                .with_mipmaps(Default::default())?;
            c.set_fill_brush(
                Brush2D::image(image)
                    .with_extend(ExtendMode::Repeat, ExtendMode::Reflect)
                    .with_transform([0.13, 0.07, -0.11, 0.4, 3., 4.])
                    .with_sampling(ImageSampling::Ewa),
            );
            c.fill_rect(0., 0., 32., 32.);
        }
        _ => unreachable!(),
    }
    Ok(c.into_ops())
}
#[test]
#[ignore = "matched native Vello comparison"]
fn matched_workloads() -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let TestGpu {
        device,
        queue,
        adapter,
        ..
    } = test_gpu();
    let device_ms = start.elapsed().as_secs_f64() * 1000.;
    let output = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap();
    std::fs::create_dir_all(&output)?;
    let input = output.parent().unwrap().parent().unwrap().join("inputs");
    let mut rows = Vec::new();
    for iteration in 0..2 {
        let start = Instant::now();
        let mut renderer = PathGpu::try_new_with_options(
            &device,
            1,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            gpu::RendererOptions {
                antialiasing: gpu::AaQuality::Area,
                num_init_threads: std::num::NonZeroUsize::new(1),
                ..Default::default()
            },
        )
        .unwrap();
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
            let target = scene_tests::texture(&device, size);
            let view = target.create_view(&Default::default());
            let count = if case == "npot-lifecycle" { 24 } else { 6 };
            let mut times = Vec::new();
            let mut prepares = Vec::new();
            for frame in 0..count {
                let start = Instant::now();
                let ops = prepare(case, &input, frame)?;
                prepares.push(start.elapsed().as_secs_f64() * 1000.);
                let start = Instant::now();
                assert!(renderer.render(&device, &queue, &view, [size, size], &ops));
                device.poll(wgpu::PollType::wait_indefinitely())?;
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let start = Instant::now();
            let rgba = readback(&device, &queue, &target, size)?;
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
            phases.push(json!({"case":case,"size":size,"frames":count,"prepare_ms":prepares,"render_wait_ms":times,"readback_ms":readback_ms}));
        }
        rows.push(json!({"iteration":iteration,"constructor_ms":constructor_ms,"phases":phases}));
    }
    let info = adapter.get_info();
    std::fs::write(
        output.join("matched.json"),
        serde_json::to_vec_pretty(
            &json!({"renderer":"jadix","backend":format!("{:?}",info.backend),"adapter":info.name,"driver":info.driver,"device_ms":device_ms,"aa":"Area","shader_compiler":"FXC on DX12","init_threads":1,"rows":rows,"output":"Rgba8UnormSrgb straight sRGB","image_contract":"Bilinear vs EWA with explicit CPU-prepared mips in scene preparation; different from Vello Medium/High, not filter-equivalent."}),
        )?,
    )?;
    Ok(())
}
