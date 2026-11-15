use jadix_2d::*;
fn sample(scene: &Scene2D, n: u32) -> Image2D {
    scene
        .sampled(
            [4, 4],
            Default::default(),
            SceneSampling {
                axis_samples: n,
                ..Default::default()
            },
        )
        .unwrap()
        .rasterize()
        .unwrap()
}
#[test]
fn adjacent_opaque_draws_resolve_after_composition() {
    let scene = Scene2D::record([4, 4], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 1.25, 4.);
        c.set_fill("#0000ff");
        c.fill_rect(1.25, 0., 2.75, 4.);
    })
    .unwrap();
    let image = sample(&scene, 4);
    let p = image.linear_rgba32f().unwrap();
    for px in p.chunks_exact(4) {
        assert_eq!(px[3], 1.);
    }
    assert_eq!(&p[4..8], &[0.25, 0., 0.75, 1.]);
    let mut old = RasterCanvas::new_hdr(4, 4);
    old.set_area_antialiasing(true);
    scene.draw(&mut old, Default::default()).unwrap();
    assert!(old.into_linear_rgba32f()[7] < 1.);
}
#[test]
fn repeated_path_and_layer_clips_do_not_square_coverage() {
    for layer in [false, true] {
        let scene = Scene2D::record([4, 4], |c| {
            let clip = filled_rect(0., 0., 1.25, 4.);
            c.clip_path(&clip, FillRule::NonZero);
            c.clip_path(&clip, FillRule::NonZero);
            if layer {
                c.push_layer_with_options(LayerOptions {
                    clip: Some([0., 0., 1.25, 4.]),
                    ..Default::default()
                });
            }
            c.set_fill("white");
            c.fill_rect(0., 0., 4., 4.);
            if layer {
                c.pop_layer();
            }
        })
        .unwrap();
        for n in [4, 8] {
            let image = sample(&scene, n);
            let p = image.linear_rgba32f().unwrap();
            assert_eq!(p[7], 0.25);
            assert_eq!(p[3], 1.);
            assert_eq!(p[11], 0.);
        }
    }
}
#[test]
fn sampled_filters_use_output_pixel_distances_and_reject_budget_overflow() {
    let child = Scene2D::record([4, 4], |c| {
        c.set_fill("white");
        c.fill_rect(0., 0., 1., 1.);
    })
    .unwrap();
    let scene = Scene2D::record([4, 4], |c| {
        c.draw_scene(
            &child,
            SceneOptions {
                layer: Some(LayerOptions {
                    filters: vec![LayerFilter::Offset { offset: [1., 2.] }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .unwrap();
    for n in [1, 2, 4, 8] {
        let image = sample(&scene, n);
        let p = image.linear_rgba32f().unwrap();
        assert_eq!(p[(2 * 4 + 1) * 4 + 3], 1.);
        assert_eq!(p[3], 0.);
    }
    assert_eq!(
        scene
            .sampled(
                [1024, 1024],
                Default::default(),
                SceneSampling {
                    axis_samples: 8,
                    max_surface_bytes: 1024
                }
            )
            .unwrap_err(),
        SceneError::ImageBudget
    );
    assert_eq!(
        scene
            .sampled([u32::MAX, 4], Default::default(), SceneSampling::default())
            .unwrap_err(),
        SceneError::InvalidSize
    );
    assert_eq!(
        scene
            .sampled(
                [4, 4],
                Default::default(),
                SceneSampling {
                    axis_samples: 3,
                    ..Default::default()
                }
            )
            .unwrap_err(),
        SceneError::InvalidOptions
    );
}
#[test]
fn spatial_samples_keep_hdr_premultiplication_and_child_transforms() {
    let image = Image2D::from_linear_rgba32f(1, 1, &[3., -0.25, 1.2, 0.5]).unwrap();
    let child = Scene2D::record([4, 4], |c| {
        c.reset_transform();
        c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 1.25, 4.);
    })
    .unwrap();
    let scene = Scene2D::record([4, 4], |c| {
        c.translate(1., 0.);
        c.draw_scene(&child, Default::default()).unwrap();
    })
    .unwrap();
    let output = sample(&scene, 4);
    let p = output.linear_rgba32f().unwrap();
    assert_eq!(&p[8..12], &[3., -0.25, 1.2, 0.125]);
    assert_eq!(p[3], 0.);
}

#[test]
fn rectangular_grids_cover_eight_samples_and_validate_budgets() {
    let scene = Scene2D::record([4, 4], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0.2, 0.2, 0.5, 0.5);
    })
    .unwrap();
    for grid in [[1, 2], [2, 1], [2, 4], [4, 2], [4, 8], [8, 4]] {
        let sampled = scene
            .sampled_grid([4, 4], Default::default(), grid, 1024 * 1024)
            .unwrap();
        assert_eq!(sampled.sample_grid(), grid);
        assert_eq!(sampled.sample_count(), grid[0] * grid[1]);
        assert_eq!(sampled.sample_size(), [4 * grid[0], 4 * grid[1]]);
        let nx = (0..grid[0])
            .filter(|x| {
                let p = (*x as f64 + 0.5) / grid[0] as f64;
                (0.2..0.7).contains(&p)
            })
            .count();
        let ny = (0..grid[1])
            .filter(|y| {
                let p = (*y as f64 + 0.5) / grid[1] as f64;
                (0.2..0.7).contains(&p)
            })
            .count();
        let image = sampled.rasterize().unwrap();
        let p = image.linear_rgba32f().unwrap();
        assert_eq!(p[3], (nx * ny) as f32 / (grid[0] * grid[1]) as f32);
        assert!(p[4..].chunks_exact(4).all(|p| p[3] == 0.));
    }
    assert_eq!(
        scene
            .sampled_grid([4, 4], Default::default(), [2, 3], 1024 * 1024)
            .unwrap_err(),
        SceneError::InvalidOptions
    );
    assert_eq!(
        scene
            .sampled_grid([4, 4], Default::default(), [2, 4], 1)
            .unwrap_err(),
        SceneError::ImageBudget
    );
    assert_eq!(
        scene
            .sampled_grid([u32::MAX, 4], Default::default(), [2, 4], 1024 * 1024)
            .unwrap_err(),
        SceneError::InvalidSize
    );
}
#[test]
fn rectangular_grid_scales_nested_filters_in_each_axis() {
    let child = Scene2D::record([4, 4], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 1., 1.);
    })
    .unwrap();
    let scene = Scene2D::record([4, 4], |c| {
        c.draw_scene(
            &child,
            SceneOptions {
                layer: Some(LayerOptions {
                    filters: vec![LayerFilter::Offset { offset: [1., 2.] }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .unwrap();
    for grid in [[1, 2], [2, 4], [4, 2], [8, 4]] {
        let im = scene
            .sampled_grid([4, 4], Default::default(), grid, 1024 * 1024)
            .unwrap()
            .rasterize()
            .unwrap();
        let p = im.linear_rgba32f().unwrap();
        assert_eq!(p[(2 * 4 + 1) * 4 + 3], 1.);
        assert_eq!(p[3], 0.);
    }
}
