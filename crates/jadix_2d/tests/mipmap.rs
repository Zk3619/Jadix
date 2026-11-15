use jadix_2d::*;
#[test]
fn npot_prefilter_preserves_linear_premultiplied_mass_and_hdr() {
    let pixels = (0..35)
        .flat_map(|i| [i as f32 / 4., -0.5, 2., if i % 3 == 0 { 0. } else { 0.5 }])
        .collect::<Vec<_>>();
    let source = Image2D::from_linear_rgba32f(7, 5, &pixels).unwrap();
    let image = source.with_mipmaps(Default::default()).unwrap();
    assert_eq!(image.mip_level_count(), 3);
    assert_eq!(image.mip_level_size(1), Some([3, 2]));
    assert_eq!(image.mip_level_size(2), Some([1, 1]));
    let last = image.mip_level_premultiplied(2).unwrap();
    for k in 0..4 {
        let expected = pixels
            .chunks_exact(4)
            .map(|p| if k == 3 { p[3] } else { p[k] * p[3] })
            .sum::<f32>()
            / 35.;
        assert!(
            (last[k] - expected).abs() < 1e-6,
            "{k}: {} {expected}",
            last[k]
        );
    }
    assert_eq!(source.mip_level_count(), 1);
    assert!(image.byte_len() > source.byte_len());
    assert!(source
        .with_mipmaps(MipmapLimits {
            max_bytes: source.byte_len(),
            ..Default::default()
        })
        .is_err());
}
#[test]
fn mipmapped_crop_and_snapshot_updates_have_independent_domains() {
    let p = (0..64)
        .flat_map(|x| {
            if x < 32 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 128]
            }
        })
        .collect::<Vec<_>>();
    let source = Image2D::from_rgba8(64, 1, &p).unwrap();
    let crop = source
        .mipmapped_region([32, 0, 32, 1], Default::default())
        .unwrap();
    let value = crop.sample_linear_with_footprint(
        [16., 0.5],
        [32., 0.],
        [0., 1.],
        ImageSampling::Trilinear,
        ExtendMode::Pad,
        ExtendMode::Pad,
    );
    assert!((value[1] - 128. / 255.).abs() < 1e-6);
    assert_eq!(value[0], 0.);
    let updated = crop.updated_rgba8([0, 0, 1, 1], &[255; 4]).unwrap();
    assert_eq!(updated.mip_level_count(), 1);
    assert_eq!(crop.mip_level_count(), 6);
}
#[test]
fn strong_minification_uses_available_mips_and_anisotropy_retains_minor_axis_detail() {
    let pixels = (0..128 * 128)
        .flat_map(|i| {
            let x = i % 128;
            let y = i / 128;
            let v = if x % 2 == 0 { 1. } else { 0. };
            [v, if y < 64 { 1. } else { 0. }, -0.25, 1.]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(128, 128, &pixels)
        .unwrap()
        .with_mipmaps(Default::default())
        .unwrap();
    let iso = image.sample_linear_with_footprint(
        [32.5, 32.5],
        [32., 0.],
        [0., 1.],
        ImageSampling::Trilinear,
        ExtendMode::Pad,
        ExtendMode::Pad,
    );
    let aniso = image.sample_linear_with_footprint(
        [32.5, 32.5],
        [32., 0.],
        [0., 1.],
        ImageSampling::Anisotropic,
        ExtendMode::Pad,
        ExtendMode::Pad,
    );
    assert!((aniso[0] - 0.5).abs() < 1e-6);
    assert!(aniso[1] >= iso[1]);
    assert_eq!(aniso[2], -0.25);
    for quality in [ImageSampling::Trilinear, ImageSampling::Anisotropic] {
        let p = image.sample_linear_with_footprint(
            [63.4, 57.1],
            [128., 0.],
            [0., 128.],
            quality,
            ExtendMode::Repeat,
            ExtendMode::Reflect,
        );
        assert!((p[0] - 0.5).abs() < 1e-6);
        assert!((p[1] - 0.5).abs() < 1e-6);
    }
}
#[test]
fn canvas_images_and_image_brushes_use_minification_footprints() {
    let p = (0..64 * 64)
        .flat_map(|i| {
            let v = if (i % 64 + i / 64) % 2 == 0 { 255 } else { 0 };
            [v, v, v, 255]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_rgba8(64, 64, &p)
        .unwrap()
        .with_mipmaps(Default::default())
        .unwrap();
    for quality in [
        ImageSampling::Trilinear,
        ImageSampling::Anisotropic,
        ImageSampling::Ewa,
    ] {
        for route in 0..2 {
            let mut c = RasterCanvas::new_hdr(4, 4);
            c.set_image_sampling(quality);
            if route == 0 {
                c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 4., 4.);
            } else {
                c.set_fill_brush(
                    Brush2D::image(image.clone())
                        .with_sampling(quality)
                        .with_transform([1. / 16., 0., 0., 1. / 16., 0., 0.]),
                );
                c.fill_rect(0., 0., 4., 4.);
            }
            assert!(c.buf.chunks_exact(4).all(|p| (p[0] - 0.5).abs() < 1e-6));
        }
    }
}

#[test]
fn elliptical_gaussian_matches_independent_quadratic_form_and_preserves_hdr() {
    let pixels = (0..17 * 9)
        .flat_map(|i| {
            [
                if i % 3 == 0 { 3. } else { -0.25 },
                i as f32 / 153.,
                1.2,
                (i % 5) as f32 / 4.,
            ]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(17, 9, &pixels).unwrap();
    // Orthogonal axes (4,2),(-1,2): lengths sqrt(20),sqrt(5).
    // Missing mips intentionally cap LOD to zero without changing this ellipse.
    for point in [[7.31, 4.17], [0.11, 0.27], [16.81, 8.73]] {
        let actual = image.sample_linear_with_footprint(
            point,
            [4., 2.],
            [-1., 2.],
            ImageSampling::Ewa,
            ExtendMode::Decal,
            ExtendMode::Decal,
        );
        let mut sum = [0f64; 4];
        let mut total = 0.;
        for y in -10..20 {
            for x in -10..30 {
                let dx = x as f64 + 0.5 - point[0] as f64;
                let dy = y as f64 + 0.5 - point[1] as f64;
                // C = (4,2)(4,2)^T + (-1,2)(-1,2)^T = [[17,6],[6,8]].
                let r = (8. * dx * dx - 12. * dx * dy + 17. * dy * dy) / 100.;
                if r >= 1. {
                    continue;
                }
                let weight = (-2. * r).exp() - (-2f64).exp();
                total += weight;
                if (0..17).contains(&x) && (0..9).contains(&y) {
                    let p = &pixels[((y * 17 + x) * 4) as usize..];
                    for k in 0..4 {
                        sum[k] += if k == 3 {
                            p[3] as f64 * weight
                        } else {
                            (p[k] * p[3]) as f64 * weight
                        };
                    }
                }
            }
        }
        for k in 0..4 {
            assert!(
                (actual[k] - (sum[k] / total) as f32).abs() < 0.000003,
                "{point:?} {actual:?} {sum:?}"
            );
        }
    }
    let constant = Image2D::from_linear_rgba32f(17, 9, &[3., -0.25, 1.2, 0.5].repeat(153))
        .unwrap()
        .with_mipmaps(Default::default())
        .unwrap();
    for dx in [[1., 0.], [8., 4.], [10000., -1000.]] {
        let p = constant.sample_linear_with_footprint(
            [8.5, 4.5],
            dx,
            [0.1, 2.],
            ImageSampling::Ewa,
            ExtendMode::Reflect,
            ExtendMode::Repeat,
        );
        assert!(p
            .iter()
            .zip([1.5, -0.125, 0.6, 0.5])
            .all(|(a, b)| (a - b).abs() < 1e-5));
    }
}
