use jadix_2d::{gradient::parse_gradient, *};

fn pixel(canvas: &dyn Canvas2D, x: u32, y: u32) -> [u8; 4] {
    canvas
        .get_image_data(x, y, 1, 1)
        .unwrap()
        .try_into()
        .unwrap()
}

#[test]
fn compound_fill_obeys_winding_and_parity_without_double_blending() {
    let contours = decode_path_d("M1 1 H19 V19 H1 Z M5 5 H15 V15 H5 Z");
    let mut canvas = RasterCanvas::new(20, 20);
    canvas.set_fill("#ff000080");
    canvas.fill_path(&contours);
    assert_eq!(pixel(&canvas, 10, 10), [255, 0, 0, 128]);
    canvas.clear("transparent");
    canvas.set_fill_rule(FillRule::EvenOdd);
    canvas.fill_path(&contours);
    assert_eq!(pixel(&canvas, 10, 10), [0; 4]);
    assert!(!canvas.is_point_in_path(&contours, 10.0, 10.0));
    let reverse = decode_path_d("M1 1 H19 V19 H1 Z M5 5 V15 H15 V5 Z");
    canvas.set_fill_rule(FillRule::NonZero);
    canvas.fill_path(&reverse);
    assert_eq!(pixel(&canvas, 10, 10), [0; 4]);
    assert!(!path_contains_point(
        &reverse,
        (10.0, 10.0),
        FillRule::NonZero
    ));
}

#[test]
fn scanline_matches_point_coverage_for_self_intersections_and_open_contours() {
    for source in [
        "M1.2 1.4 L22.1 21.7 L1.8 21.2 L22.3 1.3 Z",
        "M2 2 H20 V20 H2 M6 6 H16 V16 H6 Z",
        "M-10 -5 L10 22 L30 -5 Z M4 4 H18 V18 H4 Z",
    ] {
        let path = decode_path_d(source);
        for rule in [FillRule::NonZero, FillRule::EvenOdd] {
            let mut style = PathStyle::fill([1.0; 4]);
            style.fill_rule = rule;
            let image = rasterize_path_rgba(&path, &style, &[], 24, 24, 2);
            for y in 0..24 {
                for x in 0..24 {
                    let mut inside = 0;
                    for dy in [0.25, 0.75] {
                        for dx in [0.25, 0.75] {
                            inside += u32::from(path_contains_point(
                                &path,
                                (x as f32 + dx, y as f32 + dy),
                                rule,
                            ));
                        }
                    }
                    assert_eq!(
                        image[(y * 24 + x) * 4 + 3],
                        (inside as f32 * 255.0 / 4.0).round() as u8,
                        "{source} {rule:?} ({x},{y})"
                    );
                }
            }
            assert!(build_fill_op(&path, &style).is_some());
        }
    }
}

#[test]
fn canvas_gradient_alpha_and_rule_survive_save_restore() {
    let mut canvas = GpuCanvas::new(32, 32, None);
    canvas.set_fill_gradient(parse_gradient("conic-gradient(red, blue, red)").unwrap());
    canvas.set_global_alpha(0.5);
    canvas.save();
    canvas.set_fill("white");
    canvas.set_global_alpha(1.0);
    canvas.restore();
    canvas.fill_rect(0.0, 0.0, 32.0, 32.0);
    assert!(!canvas.needs_cpu);
    assert_eq!(pixel(&canvas, 24, 16)[3], 128);
    assert!(matches!(&canvas.into_ops()[0], PathGpuOp::Fill(fill) if fill.grad.unwrap().kind==3));
}

#[test]
fn full_circle_and_nonuniform_affine_transform_keep_their_shape() {
    let mut path = CanvasPath2D::new();
    path.arc(0.0, 0.0, 4.0, 0.0, std::f32::consts::TAU);
    path.close_path();
    let mut canvas = RasterCanvas::new(32, 24);
    canvas.set_transform([2.0, 0.0, 0.0, 1.0, 16.0, 12.0]);
    canvas.set_fill("#ff0000");
    canvas.fill_path(path.commands());
    assert_eq!(pixel(&canvas, 16, 12), [255, 0, 0, 255]);
    assert_eq!(pixel(&canvas, 22, 12)[3], 255);
    assert_eq!(pixel(&canvas, 16, 18)[3], 0);
    canvas.reset_transform();
    canvas.fill_rect(0.0, 0.0, 2.0, 2.0);
    assert_eq!(pixel(&canvas, 0, 0)[3], 255);
}

#[test]
fn image_obeys_transform_clip_alpha_and_software_continuation_order() {
    let mut canvas = GpuCanvas::new(20, 12, None);
    canvas.set_fill("#0000ff");
    canvas.fill_rect(0.0, 0.0, 20.0, 12.0);
    canvas.translate(4.0, 2.0);
    canvas.clip_rect(0.0, 0.0, 2.0, 4.0);
    canvas.set_global_alpha(0.5);
    canvas.draw_image(
        &CanvasImageSource::Rgba {
            width: 1,
            height: 1,
            rgba: &[255, 0, 0, 255],
        },
        0.0,
        0.0,
        4.0,
        4.0,
    );
    assert!(!canvas.needs_cpu);
    assert_eq!(pixel(&canvas, 4, 2), [128, 0, 128, 255]);
    assert_eq!(pixel(&canvas, 7, 2), [0, 0, 255, 255]);
    canvas.set_global_alpha(1.0);
    canvas.set_fill("green");
    canvas.fill_rect(0.0, 0.0, 1.0, 1.0);
    assert_eq!(pixel(&canvas, 4, 2), [0, 128, 0, 255]);
    canvas.clear("transparent");
    assert_eq!(pixel(&canvas, 4, 2), [0; 4]);
}

#[test]
fn exact_path_and_rotated_rectangle_clips_restore_correctly() {
    let mut canvas = GpuCanvas::new(32, 32, None);
    canvas.save();
    canvas.translate(16.0, 4.0);
    canvas.rotate(45.0);
    canvas.clip_rect(0.0, 0.0, 12.0, 12.0);
    canvas.reset_transform();
    canvas.set_fill("#ff0000");
    canvas.fill_rect(0.0, 0.0, 32.0, 32.0);
    assert_eq!(pixel(&canvas, 8, 5)[3], 0); // Inside AABB, outside diamond.
    assert_eq!(pixel(&canvas, 16, 12)[3], 255);
    canvas.restore();
    canvas.clip_path(&decode_path_d("M0 0 H4 V4 H0 Z"), FillRule::NonZero);
    canvas.set_fill("#0000ff");
    canvas.fill_rect(0.0, 0.0, 32.0, 32.0);
    assert_eq!(pixel(&canvas, 1, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(&canvas, 16, 12), [255, 0, 0, 255]);
}

#[test]
fn layer_opacity_is_applied_once_to_overlapping_children() {
    for mut canvas in [
        Box::new(RasterCanvas::new(16, 16)) as Box<dyn Canvas2D>,
        Box::new(GpuCanvas::new(16, 16, None)),
    ] {
        canvas.clear("white");
        canvas.push_layer(0.5, BlendMode::Normal);
        canvas.set_fill("#ff0000");
        canvas.fill_rect(0.0, 0.0, 12.0, 12.0);
        canvas.set_fill("#00ff00");
        canvas.fill_rect(4.0, 4.0, 12.0, 12.0);
        canvas.pop_layer();
        assert_eq!(pixel(&*canvas, 6, 6), [128, 255, 128, 255]);
        canvas.push_layer(1.0, BlendMode::Multiply);
        canvas.set_fill("#0000ff");
        canvas.fill_rect(0.0, 0.0, 16.0, 16.0);
        canvas.pop_layer();
        assert_eq!(pixel(&*canvas, 6, 6), [0, 0, 128, 255]);
    }
}

#[test]
fn repeating_gpu_gradients_resolve_implicit_stops_without_sorting_them() {
    for css in [
        "repeating-linear-gradient(red 20%, blue, green 60%)",
        "repeating-radial-gradient(red, blue 30%)",
        "repeating-conic-gradient(red, blue 90deg)",
    ] {
        let gradient = parse_gradient(css).unwrap();
        let packed = pack_gpu_gradient(&gradient).unwrap();
        assert!(packed.kind >= 5);
        assert!(!mesh_needs_cpu(&[(
            filled_rect(0.0, 0.0, 16.0, 16.0),
            PathStyle {
                fill_gradient: Some(gradient),
                ..PathStyle::fill([1.0; 4])
            }
        )]));
    }
}

#[test]
fn css_hex_gradients_accept_geometry_and_angular_stops() {
    for css in [
        "linear-gradient(to right, #ff0000, #0000ff)",
        "conic-gradient(from 20deg, #ff0000, #0000ff, #ff0000)",
        "repeating-linear-gradient(to right, #ff0000 10%, #0000ff 40%)",
        "repeating-radial-gradient(#ff0000 10%, #0000ff 40%)",
        "repeating-conic-gradient(#ff0000 20deg, #0000ff 120deg)",
    ] {
        assert!(parse_gradient(css).is_some(), "{css}");
    }
}

#[test]
fn radial_extent_reaches_the_corner_and_conic_keeps_a_full_turn() {
    let gradient =
        parse_gradient("radial-gradient(ellipse farthest-corner, #000000, #ffffff)").unwrap();
    let packed = pack_gpu_gradient(&gradient).unwrap();
    assert!((packed.radii[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    let side = jadix_2d::gradient::sample_gradient(&gradient, 1.0, 0.5);
    assert!(side[0] > 0.85 && side[0] < 0.87, "{side:?}");
    let gradient = parse_gradient("conic-gradient(#ff0000 0deg, #0000ff 360deg)").unwrap();
    assert_eq!(pack_gpu_gradient(&gradient).unwrap().stop_t[1], 1.0);
}

#[test]
fn zoomed_sheared_arc_has_device_space_chord_error_below_one_eighth_pixel() {
    let mut source = CanvasPath2D::new();
    source.arc(0.0, 0.0, 1.0, 0.0, std::f32::consts::TAU);
    source.close_path();
    for scale in [1.0, 32.0, 1024.0] {
        let matrix = [scale, 0.0, scale * 0.4, scale * 0.7, 0.0, 0.0];
        let mut canvas = GpuCanvas::new(8, 8, None);
        canvas.set_transform(matrix);
        canvas.set_fill("#ffffff");
        canvas.fill_path(source.commands());
        let ops = canvas.into_ops();
        let PathGpuOp::Fill(fill) = &ops[0] else {
            panic!("fill")
        };
        let mut edges = std::collections::HashMap::new();
        for tri in &fill.stencil {
            for (a, b) in [(tri.a, tri.b), (tri.b, tri.c), (tri.c, tri.a)] {
                let a = (a.0.to_bits(), a.1.to_bits());
                let b = (b.0.to_bits(), b.1.to_bits());
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_insert(0usize) += 1;
            }
        }
        for ((a, b), count) in edges {
            if count != 1 {
                continue;
            }
            let edge_a = (f32::from_bits(a.0), f32::from_bits(a.1));
            let edge_b = (f32::from_bits(b.0), f32::from_bits(b.1));
            // Only boundary edges are arc chords after winding tessellation. Recover the original
            // circle coordinates, then compare its projected midpoint with the arc.
            let original = |p: Point2| {
                let y = p.1 / (scale * 0.7);
                (p.0 / scale - y * 0.4, y)
            };
            let b = original(edge_a);
            let c = original(edge_b);
            let midpoint = ((b.0 + c.0) * 0.5, (b.1 + c.1) * 0.5);
            let length = midpoint.0.hypot(midpoint.1);
            let project = |p: Point2| (matrix[0] * p.0 + matrix[2] * p.1, matrix[3] * p.1);
            let chord = project(midpoint);
            let curve = project((midpoint.0 / length, midpoint.1 / length));
            assert!(
                (chord.0 - curve.0).hypot(chord.1 - curve.1) < 0.126,
                "scale={scale}"
            );
        }
    }
}

#[test]
fn every_exposed_blend_mode_matches_opaque_reference_colors() {
    for (mode, expected) in [
        (BlendMode::Normal, [64, 192, 128]),
        (BlendMode::Multiply, [32, 48, 96]),
        (BlendMode::Screen, [160, 208, 224]),
        (BlendMode::Overlay, [65, 96, 192]),
        (BlendMode::Darken, [64, 64, 128]),
        (BlendMode::Lighten, [128, 192, 192]),
        (BlendMode::Difference, [64, 128, 64]),
        (BlendMode::Exclusion, [128, 160, 127]),
    ] {
        let mut canvas = RasterCanvas::new(2, 2);
        canvas.clear("#8040c0");
        canvas.push_layer(1.0, mode);
        canvas.set_fill("#40c080");
        canvas.fill_rect(0.0, 0.0, 2.0, 2.0);
        canvas.pop_layer();
        let actual = pixel(&canvas, 0, 0);
        assert_eq!(actual[3], 255);
        for c in 0..3 {
            assert!(
                (actual[c] as i32 - expected[c]).abs() <= 1,
                "{mode:?}: {actual:?}"
            );
        }
    }
}

#[test]
fn immutable_images_filter_transparency_and_keep_sampling_state() {
    assert!(Image2D::from_rgba8(0, 1, &[]).is_none());
    assert!(Image2D::from_rgba8(u32::MAX, u32::MAX, &[]).is_none());
    assert!(Image2D::from_rgba8(2, 1, &[0; 4]).is_none());
    let image = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0]).unwrap();
    assert_eq!(image, image.clone());
    for mut c in [
        Box::new(RasterCanvas::new(8, 4)) as Box<dyn Canvas2D>,
        Box::new(GpuCanvas::new(8, 4, None)),
    ] {
        c.set_image_sampling(ImageSampling::Bilinear);
        c.save();
        c.set_image_sampling(ImageSampling::Nearest);
        c.restore();
        c.draw_image(&CanvasImageSource::Image(&image), 0.0, 0.0, 8.0, 4.0);
        let p = pixel(&*c, 4, 2);
        assert!(
            p[0] >= 254 && p[2] == 0 && (p[3] as i32 - 96).abs() <= 1,
            "{p:?}"
        );
    }
    // Opaque red/blue midpoint in linear light is about 188, not 128.
    let image = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    let mut c = RasterCanvas::new(1, 1);
    c.set_image_sampling(ImageSampling::Bilinear);
    c.draw_image(&CanvasImageSource::Image(&image), 0.0, 0.0, 1.0, 1.0);
    assert_eq!(pixel(&c, 0, 0), [188, 0, 188, 255]);
}

#[test]
fn gpu_recording_masks_survive_restore_and_later_gpu_layers() {
    let mask = Image2D::from_rgba8(8, 8, &[255u8; 8 * 8 * 4]).unwrap();
    let mut c = GpuCanvas::new(8, 8, None);
    c.clip_image_mask(&mask);
    c.save();
    c.clip_path(&filled_rect(0.0, 0.0, 4.0, 8.0), FillRule::NonZero);
    c.set_fill("#ff0000");
    c.fill_rect(0.0, 0.0, 8.0, 8.0);
    c.restore();
    c.set_fill("#0000ff");
    c.fill_rect(6.0, 0.0, 2.0, 8.0);
    assert!(!c.needs_cpu);
    assert_eq!(pixel(&c, 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&c, 5, 1), [0; 4]);
    assert_eq!(pixel(&c, 7, 1), [0, 0, 255, 255]);
    c.push_layer(0.5, BlendMode::Normal);
    c.set_fill("#00ff00");
    c.fill_rect(0.0, 0.0, 8.0, 8.0);
    c.pop_layer();
    assert!(!c.needs_cpu);
    assert_eq!(pixel(&c, 1, 1), [128, 128, 0, 255]);
    assert_eq!(pixel(&c, 5, 1), [0, 255, 0, 128]);
}

#[test]
fn direct_fills_tessellate_self_intersections_and_compound_contours() {
    for (path, direct) in [
        (filled_rect(1.0, 1.0, 6.0, 4.0), true),
        (decode_path_d("M0 0 L8 8 L0 8 L8 0 Z"), true),
        (decode_path_d("M0 0 H8 V8 H0 Z M2 2 H6 V6 H2 Z"), true),
        (decode_path_d("M0 3 L8 3 L1 8 L4 0 L7 8 Z"), true),
    ] {
        assert_eq!(
            build_fill_op(&path, &PathStyle::fill([1.0; 4]))
                .unwrap()
                .direct,
            direct
        );
    }
}

#[test]
fn drawing_after_close_restarts_at_the_closed_subpath_origin() {
    for tail in [
        "L10 18 L6 18 Z",
        "Q10 18 6 18 L10 18 Z",
        "C10 14 6 14 6 18 L10 18 Z",
    ] {
        let prefix = "M10 10 L14 10 L14 14 Z";
        let continued = decode_path_d(&format!("{prefix} {tail}"));
        let explicit = decode_path_d(&format!("{prefix} M10 10 {tail}"));
        let style = PathStyle::fill([1.; 4]);
        assert_eq!(
            rasterize_path_rgba(&continued, &style, &[], 24, 24, 2),
            rasterize_path_rgba(&explicit, &style, &[], 24, 24, 2)
        );
        let a = build_fill_op(&continued, &style).unwrap();
        let b = build_fill_op(&explicit, &style).unwrap();
        assert!(a.direct && b.direct);
        let positions = |tris: &[PathTri]| tris.iter().map(|t| [t.a, t.b, t.c]).collect::<Vec<_>>();
        assert_eq!(positions(&a.stencil), positions(&b.stencil));
    }
}

#[test]
fn nested_groups_autoclose_and_software_continuation_preserves_group_order() {
    let mut c = GpuCanvas::new(8, 8, None);
    c.pop_layer();
    c.set_fill("#0000ff");
    c.fill_rect(0., 0., 8., 8.);
    c.push_layer(0.5, BlendMode::Normal);
    c.set_fill("#ff0000");
    c.fill_rect(0., 0., 4., 8.);
    c.push_layer(1., BlendMode::Normal);
    c.set_fill("#00ff00");
    c.fill_rect(4., 0., 4., 8.);
    assert!(!c.needs_cpu);
    assert!(
        matches!(&c.into_ops()[1],PathGpuOp::Layer(l) if l.ops.len()==2 && matches!(l.ops[1],PathGpuOp::Layer(_)))
    );
    let mut c = GpuCanvas::new(8, 8, None);
    c.set_fill("#0000ff");
    c.fill_rect(0., 0., 8., 8.);
    c.push_layer(0.5, BlendMode::Normal);
    c.set_fill("#ff0000");
    c.fill_rect(0., 0., 8., 8.);
    c.put_image_data(0, 0, 1, 1, &[0, 255, 0, 255]);
    assert!(c.needs_cpu);
    c.pop_layer();
    assert_eq!(pixel(&c, 1, 1), [128, 0, 128, 255]);
    assert_eq!(pixel(&c, 0, 0), [0, 128, 128, 255]);
    let mut c = GpuCanvas::new(8, 8, None);
    c.set_fill("#0000ff");
    c.fill_rect(0., 0., 8., 8.);
    c.push_layer(0.5, BlendMode::Normal);
    c.clear("#ff0000");
    c.pop_layer();
    assert_eq!(pixel(&c, 1, 1), [128, 0, 128, 255]);
}
