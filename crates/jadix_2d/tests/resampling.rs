use jadix_2d::*;
const QUALITIES: [ImageSampling; 4] = [
    ImageSampling::Nearest,
    ImageSampling::Bilinear,
    ImageSampling::Bicubic,
    ImageSampling::Lanczos3,
];
#[test]
fn crop_taps_never_read_atlas_neighbors() {
    let image =
        Image2D::from_rgba8(3, 1, &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255]).unwrap();
    for quality in QUALITIES {
        for x in [-100., -0.5, 0., 0.17, 0.5, 0.91, 1., 10.] {
            let p = image
                .sample_region_linear([1, 0, 1, 1], [x, 0.5], quality)
                .unwrap();
            assert!(p[0].abs() < 1e-6 && p[2].abs() < 1e-6, "{quality:?} {p:?}");
            assert!((p[1] - 128. / 255.).abs() < 1e-6);
            assert!((p[3] - 128. / 255.).abs() < 1e-6);
        }
        let mut c = RasterCanvas::new_hdr(16, 16);
        c.set_image_sampling(quality);
        c.draw_image(
            &CanvasImageSource::Region {
                image: &image,
                rect: [1, 0, 1, 1],
            },
            0.,
            0.,
            16.,
            16.,
        );
        assert!(c
            .buf
            .chunks_exact(4)
            .all(|p| p[0].abs() < 1e-6 && p[2].abs() < 1e-6));
    }
    assert!(image
        .sample_region_linear([3, 0, 1, 1], [0., 0.], ImageSampling::Bicubic)
        .is_none());
}
#[test]
fn transparent_rgb_and_hdr_negative_channels_preserve_premultiplication() {
    let image = Image2D::from_linear_rgba32f(
        4,
        1,
        &[
            99., 44., 22., 0., 4., -0.5, 2., 0.5, 4., -0.5, 2., 1., 99., 44., 22., 0.,
        ],
    )
    .unwrap();
    for quality in QUALITIES {
        for i in -40..120 {
            let p = image.sample_linear(
                [i as f32 / 20., 0.5],
                quality,
                ExtendMode::Decal,
                ExtendMode::Pad,
            );
            assert!(p.iter().all(|x| x.is_finite()));
            assert!((0.0..=1.0).contains(&p[3]));
            for (v, scale) in [(p[0], 4.), (p[1], -0.5), (p[2], 2.)] {
                assert!((v - scale * p[3]).abs() < 3e-5, "{quality:?} {i}: {p:?}");
            }
        }
    }
}
// Independent f64 definition of Mitchell / Lanczos; no production kernel calls.
fn kernel(x: f64, q: ImageSampling) -> f64 {
    let x = x.abs();
    if q == ImageSampling::Bicubic {
        let b = 1. / 3.;
        let c = 1. / 3.;
        if x < 1. {
            ((12. - 9. * b - 6. * c) * x.powi(3) + (-18. + 12. * b + 6. * c) * x * x + 6. - 2. * b)
                / 6.
        } else if x < 2. {
            ((-b - 6. * c) * x.powi(3)
                + (6. * b + 30. * c) * x * x
                + (-12. * b - 48. * c) * x
                + 8. * b
                + 24. * c)
                / 6.
        } else {
            0.
        }
    } else if x < 1e-12 {
        1.
    } else if x >= 3. {
        0.
    } else {
        let p = std::f64::consts::PI * x;
        p.sin() / p * (p / 3.).sin() / (p / 3.)
    }
}
#[test]
fn reconstruction_matches_independent_f64_oracle_and_extends_each_tap() {
    let values = [0.1, 0.3, 0.9, 0.2, 0.6, 0.45, 0.8];
    let rgba = values
        .iter()
        .flat_map(|v| [*v, 0.25, 0.5, 1.])
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(7, 1, &rgba).unwrap();
    for q in [ImageSampling::Bicubic, ImageSampling::Lanczos3] {
        for mode in [ExtendMode::Pad, ExtendMode::Repeat, ExtendMode::Reflect] {
            for i in -60..220 {
                let at = i as f32 / 19.;
                let p = at as f64 - 0.5;
                let base = p.floor() as i32;
                let mut sum = 0.;
                let mut weight = 0.;
                let mut low = 1f64;
                let mut high = 0f64;
                for j in -2..=3 {
                    let index = base + j;
                    let w = kernel(index as f64 - p, q);
                    if w.abs() < 1e-14 {
                        continue;
                    }
                    let n = match mode {
                        ExtendMode::Pad => index.clamp(0, 6),
                        ExtendMode::Repeat => index.rem_euclid(7),
                        ExtendMode::Reflect => {
                            let v = index.rem_euclid(14);
                            if v < 7 {
                                v
                            } else {
                                13 - v
                            }
                        }
                        _ => unreachable!(),
                    } as usize;
                    let v = values[n] as f64;
                    sum += v * w;
                    weight += w;
                    low = low.min(v);
                    high = high.max(v);
                }
                let expected = (sum / weight).clamp(low, high);
                let got = image.sample_linear([at, 0.5], q, mode, ExtendMode::Pad);
                assert!(
                    (got[0] as f64 - expected).abs() < 3e-6,
                    "{q:?} {mode:?} {at}: {} {expected}",
                    got[0]
                );
                assert!((got[3] - 1.).abs() < 1e-6);
            }
        }
    }
}
#[test]
fn scene_retains_new_sampling_modes_and_region_validation() {
    let image =
        Image2D::from_rgba8(3, 1, &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255]).unwrap();
    for quality in [ImageSampling::Bicubic, ImageSampling::Lanczos3] {
        let draw = |c: &mut dyn Canvas2D| {
            c.set_image_sampling(quality);
            c.draw_image(
                &CanvasImageSource::Region {
                    image: &image,
                    rect: [1, 0, 1, 1],
                },
                0.,
                0.,
                16.,
                16.,
            );
        };
        let scene = Scene2D::record([16, 16], draw).unwrap();
        let mut c = RasterCanvas::new_hdr(16, 16);
        scene.draw(&mut c, Default::default()).unwrap();
        assert!(c
            .buf
            .chunks_exact(4)
            .all(|p| p[0].abs() < 1e-6 && p[2].abs() < 1e-6));
    }
}
