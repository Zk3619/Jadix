use jadix_2d::*;

fn content(c: &mut dyn Canvas2D) {
    c.set_fill("#ff400080");
    c.fill_rect(0., 0., 12., 12.);
    c.save();
    c.translate(4., 4.);
    c.set_global_alpha(0.5);
    c.set_fill("#2080ff");
    c.fill_rect(0., 0., 12., 12.);
    c.restore();
    c.set_transform([1., 0., 0., 1., 16., 0.]);
    c.set_fill("#00ff00");
    c.fill_rect(0., 0., 4., 4.);
    c.reset_transform();
    c.set_stroke("#ffffff");
    c.set_line_width(1.);
    c.stroke_path(&polyline(&[(0., 15.), (16., 15.)]));
}
#[test]
fn transforms_styles_alpha_and_relative_reset_match_direct_draws() {
    let scene = Scene2D::record([24, 20], content).unwrap();
    for isolated in [false, true] {
        let paint = |c: &mut dyn Canvas2D, use_scene: bool| {
            c.set_fill("#8040ff");
            c.set_global_alpha(0.6);
            c.translate(5., 4.);
            if use_scene {
                c.draw_scene(
                    &scene,
                    SceneOptions {
                        transform: [1.5, 0., 0., 1.25, 4., 3.],
                        layer: isolated.then(|| LayerOptions::new(0.75, BlendMode::Normal)),
                        ..Default::default()
                    },
                )
                .unwrap();
            } else {
                c.save();
                if isolated {
                    c.push_layer(0.6 * 0.75, BlendMode::Normal);
                }
                c.transform([1.5, 0., 0., 1.25, 4., 3.]);
                let base = c.current_transform().unwrap();
                c.set_fill("#ff400080");
                c.set_global_alpha(if isolated { 1. } else { 0.6 });
                c.fill_rect(0., 0., 12., 12.);
                c.save();
                c.translate(4., 4.);
                c.set_global_alpha(if isolated { 0.5 } else { 0.3 });
                c.set_fill("#2080ff");
                c.fill_rect(0., 0., 12., 12.);
                c.restore();
                c.set_transform([
                    base[0],
                    base[1],
                    base[2],
                    base[3],
                    base[4] + 16. * base[0],
                    base[5] + 16. * base[1],
                ]);
                c.set_fill("#00ff00");
                c.fill_rect(0., 0., 4., 4.);
                c.set_transform(base);
                c.set_stroke("#ffffff");
                c.set_line_width(1.);
                c.stroke_path(&polyline(&[(0., 15.), (16., 15.)]));
                if isolated {
                    c.pop_layer();
                }
                c.restore();
            }
            assert_eq!(c.current_transform(), Some([1., 0., 0., 1., 5., 4.]));
            assert_eq!(c.current_global_alpha(), Some(0.6));
            c.fill_rect(48., 0., 2., 2.);
        };
        let mut expected = RasterCanvas::new_linear(64, 40);
        paint(&mut expected, false);
        let mut cpu = RasterCanvas::new_linear(64, 40);
        paint(&mut cpu, true);
        let mut gpu = GpuCanvas::new_linear(64, 40, None);
        paint(&mut gpu, true);
        assert_eq!(
            cpu.get_image_data(0, 0, 64, 40),
            expected.get_image_data(0, 0, 64, 40)
        );
        assert_eq!(
            gpu.get_image_data(0, 0, 64, 40),
            expected.get_image_data(0, 0, 64, 40)
        );
        assert!(!gpu.needs_cpu);
        cpu.put_image_data(0, 0, 1, 1, &[255; 4]);
        gpu.put_image_data(0, 0, 1, 1, &[255; 4]);
        assert_eq!(cpu.into_pixels(), gpu.into_raster());
    }
}
#[test]
fn group_opacity_applies_once_and_nested_snapshots_outlive_builder_resources() {
    let child = Scene2D::record([16, 16], |c| {
        c.set_fill("#ffffff");
        c.fill_rect(0., 0., 12., 12.);
        c.fill_rect(4., 4., 12., 12.);
    })
    .unwrap();
    let parent = Scene2D::record([40, 16], |c| {
        c.draw_scene(&child, SceneOptions::default()).unwrap();
        c.draw_scene(
            &child,
            SceneOptions {
                transform: [1., 0., 0., 1., 20., 0.],
                layer: Some(LayerOptions::default()),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .unwrap();
    drop(child);
    let mut cpu = RasterCanvas::new_linear(40, 16);
    cpu.set_global_alpha(0.5);
    parent.draw(&mut cpu, SceneOptions::default()).unwrap();
    let p = cpu.get_image_data(0, 0, 40, 16).unwrap();
    assert_eq!(p[(8 * 40 + 8) * 4 + 3], 191);
    assert_eq!(p[(8 * 40 + 28) * 4 + 3], 128);
    assert_eq!(parent.stats().commands, 2);
    assert!(parent.stats().expanded_commands > parent.stats().commands);
    let mut gpu = GpuCanvas::new_linear(40, 16, None);
    gpu.set_global_alpha(0.5);
    parent
        .clone()
        .draw(&mut gpu, SceneOptions::default())
        .unwrap();
    assert_eq!(gpu.get_image_data(0, 0, 40, 16).unwrap(), p);
}
#[test]
fn scene_device_masks_and_final_layer_clips_follow_base_not_local_ctm() {
    let mask = Image2D::from_rgba8(
        8,
        8,
        &(0..64)
            .flat_map(|i| [0, 0, 0, if i % 8 < 4 { 255 } else { 0 }])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let scene = Scene2D::record([8, 8], |c| {
        c.set_fill("#ffffff");
        c.push_layer_with_options(LayerOptions {
            clip: Some([1., 0., 6., 8.]),
            mask: Some(mask.clone()),
            ..Default::default()
        });
        c.translate(3., 0.);
        c.fill_rect(-3., 0., 8., 8.);
        c.pop_layer();
    })
    .unwrap();
    let mut c = RasterCanvas::new_linear(24, 16);
    c.set_fill("#ff0000");
    scene
        .draw(
            &mut c,
            SceneOptions {
                transform: [1., 0., 0., 1., 8., 4.],
                ..Default::default()
            },
        )
        .unwrap();
    let p = c.get_image_data(0, 0, 24, 16).unwrap();
    assert_eq!(p[(8 * 24 + 10) * 4 + 3], 255);
    assert_eq!(p[(8 * 24 + 8) * 4 + 3], 0);
    assert_eq!(p[(8 * 24 + 14) * 4 + 3], 0);
    let mut g = GpuCanvas::new_linear(24, 16, None);
    scene
        .draw(
            &mut g,
            SceneOptions {
                transform: [1., 0., 0., 1., 8., 4.],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(g.get_image_data(0, 0, 24, 16).unwrap(), p);
}
#[test]
fn balance_unsupported_ops_and_all_recording_budgets_fail_explicitly() {
    assert_eq!(
        Scene2D::record([0, 8], |_| {}).unwrap_err(),
        SceneError::InvalidSize
    );
    for f in [
        |c: &mut dyn Canvas2D| c.save(),
        |c: &mut dyn Canvas2D| c.restore(),
        |c: &mut dyn Canvas2D| c.push_layer(1., BlendMode::Normal),
        |c: &mut dyn Canvas2D| c.pop_layer(),
    ] {
        assert_eq!(
            Scene2D::record([8, 8], f).unwrap_err(),
            SceneError::UnbalancedState
        );
    }
    for f in [
        |c: &mut dyn Canvas2D| c.clear("black"),
        |c: &mut dyn Canvas2D| {
            c.get_image_data(0, 0, 1, 1);
        },
        |c: &mut dyn Canvas2D| c.put_image_data(0, 0, 1, 1, &[0; 4]),
        |c: &mut dyn Canvas2D| {
            c.is_point_in_path(&[], 0., 0.);
        },
    ] {
        assert_eq!(
            Scene2D::record([8, 8], f).unwrap_err(),
            SceneError::UnsupportedOperation
        );
    }
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_commands: 1,
                ..Default::default()
            },
            |c| {
                c.set_fill("white");
                c.fill_rect(0., 0., 1., 1.);
            }
        )
        .unwrap_err(),
        SceneError::CommandBudget
    );
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_path_commands: 2,
                ..Default::default()
            },
            |c| c.fill_rect(0., 0., 1., 1.)
        )
        .unwrap_err(),
        SceneError::PathBudget
    );
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_copied_image_bytes: 0,
                ..Default::default()
            },
            |c| c.draw_image(
                &CanvasImageSource::Rgba {
                    width: 1,
                    height: 1,
                    rgba: &[255; 4]
                },
                0.,
                0.,
                1.,
                1.
            )
        )
        .unwrap_err(),
        SceneError::ImageBudget
    );
    let mut s = Scene2D::record([8, 8], |c| c.fill_rect(0., 0., 1., 1.)).unwrap();
    for _ in 0..5 {
        s = Scene2D::record([8, 8], |c| {
            s.draw(c, SceneOptions::default()).unwrap();
            s.draw(c, SceneOptions::default()).unwrap();
        })
        .unwrap();
    }
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_expanded_commands: 10,
                ..Default::default()
            },
            |c| {
                let _ = s.draw(c, SceneOptions::default());
            }
        )
        .unwrap_err(),
        SceneError::ExpandedBudget
    );
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_nesting: 3,
                ..Default::default()
            },
            |c| {
                let _ = s.draw(c, SceneOptions::default());
            }
        )
        .unwrap_err(),
        SceneError::NestingBudget
    );
}
#[test]
fn failed_transform_is_atomic_and_request_frame_and_gradient_fallback_propagate() {
    let scene = Scene2D::record([8, 8], |c| {
        c.fill_rect(0., 0., 8., 8.);
        c.transform([1e12, 0., 0., 1e12, 0., 0.]);
        c.fill_rect(0., 0., 1., 1.);
    })
    .unwrap();
    let mut c = RasterCanvas::new_linear(16, 16);
    c.translate(2., 3.);
    assert_eq!(
        scene.draw(
            &mut c,
            SceneOptions {
                transform: [2., 0., 0., 2., 0., 0.],
                ..Default::default()
            }
        ),
        Err(SceneError::InvalidTransform)
    );
    assert!(c.buf.iter().all(|v| *v == 0.));
    assert_eq!(c.current_transform(), Some([1., 0., 0., 1., 2., 3.]));
    let scene = Scene2D::record([8, 8], |c| {
        let stops = (0..9)
            .map(|i| format!("#ff0000 {}%", i * 12))
            .collect::<Vec<_>>()
            .join(",");
        c.set_fill_gradient(
            gradient::parse_gradient(format!("linear-gradient(to right,{stops})")).unwrap(),
        );
        c.fill_rect(0., 0., 8., 8.);
        c.request_frame();
    })
    .unwrap();
    let mut g = GpuCanvas::new_linear(16, 16, None);
    scene.draw(&mut g, SceneOptions::default()).unwrap();
    assert!(g.needs_cpu && g.want_frame);
    let mut c = RasterCanvas::new_linear(16, 16);
    scene.draw(&mut c, SceneOptions::default()).unwrap();
    assert_eq!(g.into_raster(), c.into_pixels());
}

#[test]
fn replay_mask_budget_and_invalid_path_fail_before_destination_changes() {
    fn assert_shared<T: Send + Sync>() {}
    assert_shared::<Scene2D>();
    let mask = Image2D::from_rgba8(8, 8, &[255; 256]).unwrap();
    let child = Scene2D::record_with_limits(
        [8, 8],
        SceneLimits {
            max_replay_mask_bytes: 256,
            ..Default::default()
        },
        |c| {
            c.clip_image_mask(&mask);
            c.fill_rect(0., 0., 8., 8.);
        },
    )
    .unwrap();
    let parent = Scene2D::record([8, 8], |c| {
        c.fill_rect(0., 0., 8., 8.);
        child.draw(c, SceneOptions::default()).unwrap();
    })
    .unwrap();
    assert_eq!(parent.stats().mask_mappings, 1);
    let mut c = RasterCanvas::new_linear(16, 16);
    assert_eq!(
        parent.draw(&mut c, SceneOptions::default()),
        Err(SceneError::MaskBudget)
    );
    assert!(c.buf.iter().all(|v| *v == 0.));
    let mut gpu = GpuCanvas::new_linear(16, 16, None);
    assert_eq!(
        parent.draw(&mut gpu, SceneOptions::default()),
        Err(SceneError::MaskBudget)
    );
    assert!(!gpu.needs_cpu);
    assert!(gpu.into_ops().is_empty());
    assert_eq!(
        Scene2D::record([8, 8], |c| c
            .fill_path(&[PathCommand::MoveTo((f32::NAN, 0.))]))
        .unwrap_err(),
        SceneError::InvalidOptions
    );
    assert_eq!(
        Scene2D::record_with_limits(
            [8, 8],
            SceneLimits {
                max_path_commands: 3,
                ..Default::default()
            },
            |c| {
                c.set_line_dash(&[1., 2.]);
                c.set_line_dash(&[3., 4.]);
            }
        )
        .unwrap_err(),
        SceneError::PathBudget
    );
}

#[test]
fn declared_bounds_clip_input_and_allow_filtered_output_to_spread() {
    let scene = Scene2D::record([8, 8], |c| {
        c.set_fill("#ffffff");
        c.fill_rect(-4., -4., 16., 16.);
    })
    .unwrap();
    for clip in [false, true] {
        let options = SceneOptions {
            transform: [0., 1., -1., 0., 16., 8.],
            clip_to_bounds: clip,
            ..Default::default()
        };
        let mut cpu = RasterCanvas::new_linear(24, 24);
        scene.draw(&mut cpu, options.clone()).unwrap();
        let expected = cpu.get_image_data(0, 0, 24, 24).unwrap();
        assert_eq!(expected[(9 * 24 + 6) * 4 + 3], if clip { 0 } else { 255 });
        assert_eq!(expected[(9 * 24 + 9) * 4 + 3], 255);
        let mut gpu = GpuCanvas::new_linear(24, 24, None);
        scene.draw(&mut gpu, options).unwrap();
        assert_eq!(gpu.get_image_data(0, 0, 24, 24).unwrap(), expected);
    }
    let mut c = RasterCanvas::new_linear(24, 24);
    scene
        .draw(
            &mut c,
            SceneOptions {
                transform: [1., 0., 0., 1., 8., 8.],
                clip_to_bounds: true,
                layer: Some(LayerOptions {
                    filters: vec![LayerFilter::Gaussian { sigma: [1., 1.] }],
                    ..Default::default()
                }),
            },
        )
        .unwrap();
    let pixels = c.get_image_data(0, 0, 24, 24).unwrap();
    assert!(
        pixels[(11 * 24 + 7) * 4 + 3] > 0,
        "filter spreads beyond clipped input"
    );
    assert_eq!(pixels[(11 * 24 + 3) * 4 + 3], 0);
}
