use super::*;

#[test]
fn round_caps_and_markers_obey_pixel_error_budget() {
    for radius in [1.0, 4.0, 16.0, 64.0, 128.0] {
        let mut tris = Vec::new();
        disc_fan((0.0, 0.0), radius, [1.0; 4], &mut tris);
        for tri in tris {
            let midpoint = ((tri.b.0 + tri.c.0) * 0.5, (tri.b.1 + tri.c.1) * 0.5);
            let error = radius - midpoint.0.hypot(midpoint.1);
            assert!(
                error <= 0.1251,
                "round cap radius {radius} has visible polygon facets: chord error {error}"
            );
        }
    }
}

#[test]
fn polyline_and_pie_encode() {
    let cmds = polyline(&[(0.0, 0.0), (1.0, 2.0)]);
    assert_eq!(cmds.len(), 2);
    let pie = pie_sector(0.0, 0.0, 10.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
    assert!(pie.len() >= 4);
    assert!(
        pie.iter().any(|c| matches!(c, PathCommand::Arc { .. })),
        "pie_sector must keep Arc until flatten, got {pie:?}"
    );
    let ring = pie_ring(0.0, 0.0, 10.0, 4.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
    assert!(
        ring.len() > pie.len(),
        "donut ring should have outer+inner arc, ring={} pie={}",
        ring.len(),
        pie.len()
    );
    let s = encode_points(&[(1.5, 2.5)]);
    assert_eq!(s, "1.500,2.500");
    let c = filled_circle(10.0, 20.0, 5.0, 16);
    assert!(
        c.iter().any(|cmd| matches!(cmd, PathCommand::Arc { .. })),
        "filled_circle must keep Arc until flatten, got {c:?}"
    );
    let ell = filled_ellipse(0.0, 0.0, 10.0, 4.0, 8);
    assert!(
        ell.iter().any(|cmd| matches!(
            cmd,
            PathCommand::Arc {
                radii: (rx, ry),
                ..
            } if (*rx - 10.0).abs() < 1e-3 && (*ry - 4.0).abs() < 1e-3
        )),
        "filled_ellipse must keep rx/ry Arc, got {ell:?}"
    );
    let mut pts = points_from_commands(&c);
    pts.pop(); // the closing arc repeats the first point
    let n = pts.len().max(1) as f32;
    let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
    assert!(
        (cx - 10.0).abs() < 0.2 && (cy - 20.0).abs() < 0.2,
        "centroid=({cx},{cy})"
    );
}

#[test]
fn circle_stays_round_when_viewbox_is_stretched_wide() {
    let cmds = filled_circle(160.0, 70.0, 8.0, 20);
    let scaled = scale_commands_preserve_circles(&cmds, [0.0, 0.0, 320.0, 140.0], [640.0, 140.0]);
    let mut pts = points_from_commands(&scaled);
    pts.pop(); // the closing arc repeats the first point
    let n = pts.len().max(1) as f32;
    let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
    let mut r_min = f32::MAX;
    let mut r_max = 0.0f32;
    for &(x, y) in &pts {
        let r = (x - cx).hypot(y - cy);
        r_min = r_min.min(r);
        r_max = r_max.max(r);
    }
    assert!(
        (r_max - r_min) < 1.0,
        "dot must stay circular under anisotropic stretch, r=[{r_min},{r_max}]"
    );
    assert!(
        (cx - 320.0).abs() < 1.0 && (cy - 70.0).abs() < 1.0,
        "centroid should map by sx/sy, got ({cx},{cy})"
    );
}

#[test]
fn path_stroke_contains_mid_segment_and_square_cap() {
    let cmds = polyline(&[(10.0, 10.0), (30.0, 10.0)]);
    let butt = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 4.0).linecap(StrokeLinecap::Butt);
    assert!(path_stroke_contains(&cmds, &butt, (20.0, 10.0)));
    assert!(!path_stroke_contains(&cmds, &butt, (20.0, 16.0)));
    let corner = (32.0, 12.0);
    assert!(
        !path_stroke_contains(&cmds, &butt, corner),
        "butt must miss the square-cap corner"
    );
    let square = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 4.0).linecap(StrokeLinecap::Square);
    assert!(
        path_stroke_contains(&cmds, &square, corner),
        "square cap must cover the outer rectangle corner"
    );
}

#[test]
fn polyline_still_stretches_anisotropically() {
    let cmds = polyline(&[(0.0, 0.0), (320.0, 140.0)]);
    let scaled = scale_commands_preserve_circles(&cmds, [0.0, 0.0, 320.0, 140.0], [640.0, 140.0]);
    let pts = points_from_commands(&scaled);
    assert_eq!(pts.len(), 2);
    assert!((pts[1].0 - 640.0).abs() < 0.5 && (pts[1].1 - 140.0).abs() < 0.5);
}

#[test]
fn pie_sector_is_not_treated_as_circle() {
    let cmds = pie_sector(50.0, 50.0, 40.0, 0.0, std::f32::consts::FRAC_PI_2, 12);
    assert!(
        detect_regular_circle(&cmds).is_none(),
        "wedge with a center vertex must keep anisotropic scale"
    );
}

#[test]
fn path_d_roundtrip_mlz() {
    let cmds = vec![
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((10.0, 0.0)),
        PathCommand::LineTo((10.0, 8.0)),
        PathCommand::Close,
    ];
    let d = encode_path_d(&cmds);
    let back = decode_path_d(&d);
    assert_eq!(back, cmds);
}

#[test]
fn loader_circle_arc_flattens_smoothly() {
    let cmds = decode_path_d("M21 12a9 9 0 1 1-6.219-8.56");
    let pts = points_from_commands(&cmds);
    for segment in pts.windows(2) {
        let midpoint = (
            (segment[0].0 + segment[1].0) * 0.5,
            (segment[0].1 + segment[1].1) * 0.5,
        );
        let error = 9.0 - (midpoint.0 - 12.0).hypot(midpoint.1 - 12.0);
        assert!(
            error.abs() <= 0.1251,
            "loader-circle arc must have a smooth silhouette: chord error {error}"
        );
    }
}

#[test]
fn path_d_preserves_cubic_and_arc() {
    let cubic = decode_path_d("M0,0 C10,0 10,10 0,10");
    assert_eq!(cubic.len(), 2, "cubic must stay one CubicTo, got {cubic:?}");
    assert!(matches!(cubic[0], PathCommand::MoveTo((0.0, 0.0))));
    assert!(matches!(
        cubic[1],
        PathCommand::CubicTo {
            to: (x, y),
            ..
        } if (x - 0.0).abs() < 1e-3 && (y - 10.0).abs() < 0.2
    ));
    let flat = flatten_path(&cubic);
    assert!(
        flat.len() > 4,
        "flatten_path should emit multiple LineTo, got {}",
        flat.len()
    );
    let last = points_from_commands(&cubic).last().copied().unwrap();
    assert!((last.0 - 0.0).abs() < 0.2 && (last.1 - 10.0).abs() < 0.2);

    let tiny = flatten_path(&[
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::CubicTo {
            c1: (0.2, 0.0),
            c2: (0.4, 0.0),
            to: (0.5, 0.0),
        },
    ]);
    assert!(
        tiny.len() <= 4,
        "near-linear 0.5px cubic must not force 12 segments, got {}",
        tiny.len()
    );

    let arc = decode_path_d("M10,0 A10,10 0 0 1 0,10");
    assert!(
        matches!(arc.last(), Some(PathCommand::Arc { .. })),
        "arc must stay Arc, got {arc:?}"
    );
    let end = points_from_commands(&arc).last().copied().unwrap();
    assert!((end.0 - 0.0).abs() < 0.5 && (end.1 - 10.0).abs() < 0.5);
}

#[test]
fn path_d_relative_and_hv() {
    let cmds = decode_path_d("M10 10 h20 v-5 l-5 5 Z");
    let pts = points_from_commands(&cmds);
    assert!(pts.len() >= 4);
    assert!((pts[0].0 - 10.0).abs() < 1e-3 && (pts[0].1 - 10.0).abs() < 1e-3);
    assert!((pts[1].0 - 30.0).abs() < 1e-3);
    assert!(cmds.iter().any(|c| matches!(c, PathCommand::Close)));
}

#[test]
fn tessellate_stroke_emits_rotated_quads() {
    let cmds = polyline(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0)]);
    let quads = tessellate_path(&cmds, &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 2.0));
    assert!(
        quads.iter().any(|q| q.rotate_deg.abs() > 1.0),
        "vertical segment should rotate, quads={quads:?}"
    );
    assert!(quads.iter().any(|q| q.rotate_deg.abs() < 1.0));
    assert!(quads.len() >= 3);
}

#[test]
fn tessellate_stroke_butt_is_exact_length() {
    let cmds = polyline(&[(0.0, 10.0), (40.0, 10.0)]);
    let style = PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 4.0).linecap(StrokeLinecap::Butt);
    let quads = tessellate_path(&cmds, &style);
    let seg = quads
        .iter()
        .find(|q| q.rotate_deg.abs() < 1.0 && q.size.0 > 20.0)
        .expect("horizontal segment");
    assert!(
        (seg.size.0 - 40.0).abs() < 0.6,
        "butt must not extend len+w, got {}",
        seg.size.0
    );
}

#[test]
fn tessellate_stroke_miter_covers_outer_corner() {
    let cmds = polyline(&[(0.0, 20.0), (40.0, 20.0), (40.0, 60.0)]);
    let style = PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 4.0)
        .linecap(StrokeLinecap::Butt)
        .linejoin(StrokeLinejoin::Miter);
    let tris = tessellate_path_mesh(&cmds, &style, &[], 0.0);
    let has_miter = tris
        .iter()
        .any(|t| [t.a, t.b, t.c].iter().any(|p| p.0 >= 41.4 && p.1 <= 18.6));
    assert!(
        has_miter,
        "90° miter should reach ~ (42, 18), tris={tris:?}"
    );
}

#[test]
fn tessellate_path_mesh_stroke_is_triangles() {
    let cmds = polyline(&[(10.0, 10.0), (50.0, 10.0), (50.0, 40.0)]);
    let style = PathStyle::stroke([1.0, 0.2, 0.2, 1.0], 4.0);
    let tris = tessellate_path_mesh(&cmds, &style, &[], 0.0);
    assert!(
        tris.len() >= 4,
        "stroke mesh must be triangles, got {}",
        tris.len()
    );
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for t in &tris {
        for p in [t.a, t.b, t.c] {
            min_x = min_x.min(p.0);
            max_x = max_x.max(p.0);
            min_y = min_y.min(p.1);
            max_y = max_y.max(p.1);
        }
    }
    assert!(
        min_x <= 10.0 + 1.0 && max_x >= 50.0 - 1.0 && max_y >= 40.0 - 1.0,
        "mesh bbox [{min_x},{min_y}]-[{max_x},{max_y}] must cover the polyline"
    );
    let cpu = rasterize_path_rgba(&cmds, &style, &[], 64, 48, 2);
    let opaque = cpu.chunks(4).filter(|p| p[3] > 8).count();
    assert!(
        opaque > 40,
        "CPU golden stroke should paint, opaque={opaque}"
    );
}

#[test]
fn path_gpu_queues_mesh_job_not_sdf_instances() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    let _ = take_path_jobs();
    let el = attach_path(
        Element::new().w(80.0).h(60.0),
        &polyline(&[(4.0, 30.0), (40.0, 8.0), (76.0, 50.0)]),
        &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
    );
    let (key, w, h) = path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0)
        .expect("stroke polyline must queue PathPipeline job");
    assert!(key.starts_with("jadix_path:"));
    assert!(w > 40.0 && h > 20.0);
    let jobs = take_path_jobs();
    assert!(!jobs.is_empty());
    let ops = jobs[0].ops.clone().into_ops();
    let n = ops
        .iter()
        .map(|op| match op {
            PathGpuOp::Mesh(m) => m.tris.len(),
            PathGpuOp::Fill(_) => 0,
        })
        .sum::<usize>();
    assert!(n >= 4, "stroke mesh must be triangles, got {n}");
}

#[test]
fn path_gpu_queues_stencil_fill() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    let _ = take_path_jobs();
    let el = attach_path(
        Element::new().w(80.0).h(60.0),
        &pie_sector(40.0, 30.0, 24.0, 0.0, 1.8, 8),
        &PathStyle::fill([0.2, 0.4, 0.9, 1.0]),
    );
    let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0)
        .expect("solid fill must queue stencil-cover");
    assert!(key.starts_with("jadix_path:"));
    let jobs = take_path_jobs();
    let ops = jobs[0].ops.clone().into_ops();
    let fill = ops.iter().find_map(|op| match op {
        PathGpuOp::Fill(f) => Some(f),
        PathGpuOp::Mesh(_) => None,
    });
    let fill = fill.expect("fill op");
    assert!(!fill.even_odd);
    assert!(fill.stencil.len() >= 3);
    assert!(fill.grad.is_none());
}

#[test]
fn path_gpu_evenodd_hole_is_one_fill() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    let _ = take_path_jobs();
    let mut cmds = filled_circle(40.0, 30.0, 22.0, 16);
    cmds.extend(filled_circle(40.0, 30.0, 10.0, 12));
    let mut style = PathStyle::fill([0.9, 0.3, 0.2, 1.0]);
    style.fill_rule = FillRule::EvenOdd;
    let el = attach_path(Element::new().w(80.0).h(60.0), &cmds, &style);
    path_gpu_from_element_attrs(&el.attrs, 80.0, 60.0).expect("evenodd hole must stay on GPU");
    let jobs = take_path_jobs();
    let ops = jobs[0].ops.clone().into_ops();
    let fill = ops
        .iter()
        .find_map(|op| match op {
            PathGpuOp::Fill(f) => Some(f),
            _ => None,
        })
        .expect("fill");
    assert!(fill.even_odd);
    assert!(
        fill.stencil.len() >= 20,
        "outer+inner fans, got {}",
        fill.stencil.len()
    );
}

#[test]
fn path_gpu_linear_gradient_packs() {
    let mut style = PathStyle::fill([1.0, 1.0, 1.0, 1.0]);
    style.fill_gradient = Some(crate::style::Gradient::Linear(
        crate::style::LinearGradient::two_stop([0.1, 0.2, 0.9, 1.0], [0.9, 0.2, 0.1, 1.0], 90.0),
    ));
    let fill = build_fill_op(&filled_rect(8.0, 8.0, 60.0, 40.0), &style).expect("fill");
    let g = fill.grad.expect("packed linear");
    assert_eq!(g.kind, 1);
    assert_eq!(g.stop_n, 2);
}

#[test]
fn path_gpu_conic_falls_back_to_cpu() {
    let mut style = PathStyle::fill([1.0, 1.0, 1.0, 1.0]);
    style.fill_gradient = Some(crate::style::Gradient::Conic(
        crate::style::ConicGradient::default(),
    ));
    assert!(pack_gpu_gradient(style.fill_gradient.as_ref().unwrap()).is_none());
    assert!(mesh_needs_cpu(&[(
        filled_rect(8.0, 8.0, 60.0, 40.0),
        style
    )]));
}

#[test]
fn tessellate_fill_pie_has_coverage() {
    let cmds = pie_sector(50.0, 50.0, 40.0, 0.0, std::f32::consts::FRAC_PI_2, 8);
    let quads = tessellate_path(&cmds, &PathStyle::fill([0.2, 0.4, 0.9, 1.0]));
    assert!(
        quads.len() >= 4,
        "pie fill should emit wedge quads, got {}",
        quads.len()
    );
    assert!(
        quads.iter().all(|q| q.rotate_deg.abs() < 0.01),
        "fill must be axis-aligned scanlines, not rotated wedges"
    );
    let area: f32 = quads.iter().map(|q| q.size.0 * q.size.1).sum();
    assert!(area > 200.0, "pie fill area too small: {area}");
}

#[test]
fn viewbox_commas_parse_like_spaces() {
    let a = parse_path_viewbox("0 0 100 80").unwrap();
    let b = parse_path_viewbox("0,0,100,80").unwrap();
    assert_eq!(a, b);
}

#[test]
fn fit_leaf_viewbox_follows_both_axes() {
    let mut el = Element::new();
    el.set_attr(ATTR_PATH_VIEWBOX, "0 0 100 100");
    el.set_attr(ATTR_PATH_FIT_LEAF, "1");
    let target = path_target_leaf_size(&el.attrs, [160.0, 120.0]);
    assert!(
        (target[0] - 160.0).abs() < 0.5 && (target[1] - 120.0).abs() < 0.5,
        "svg fit-leaf must follow the leaf, got {target:?}"
    );
}

#[test]
fn pixel_viewbox_height_locks_target_not_stretched_leaf() {
    let el = attach_path(
        Element::new().w(200.0).h(400.0),
        &polyline(&[(0.0, 40.0), (1.0, 10.0)]),
        &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
    );
    let mut attrs = el.attrs.clone();
    attrs.insert(ATTR_PATH_VIEWBOX.into(), "0 0 1 80".into());
    let target = path_target_leaf_size(&attrs, [200.0, 400.0]);
    assert!(
        (target[0] - 200.0).abs() < 0.5 && (target[1] - 80.0).abs() < 0.5,
        "normalized x → leaf w, pixel y → viewBox h, got {target:?}"
    );
    let (key, dw, dh) = path_raster_from_element_attrs(&attrs, 200.0, 400.0).expect("raster");
    assert!(key.starts_with("jadix_path:"));
    assert!((dw - 200.0).abs() < 0.5 && (dh - 80.0).abs() < 0.5);
}

#[test]
fn viewbox_scales_commands_to_leaf() {
    let el = attach_path(
        Element::new().w(100.0).h(50.0),
        &polyline(&[(0.0, 0.0), (1.0, 1.0)]),
        &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 2.0),
    );
    let mut attrs = el.attrs.clone();
    attrs.insert(ATTR_PATH_VIEWBOX.into(), "0 0 1 1".into());
    let quads = path_quads_from_element_attrs(&attrs, [0.0, 0.0], [200.0, 100.0]).unwrap();
    assert!(
        quads.iter().any(|q| q.size.0 > 150.0),
        "viewBox 0..1 should stretch to leaf 200px, quads={quads:?}"
    );
}

#[test]
fn attach_path_writes_shared_d_attr() {
    let el = attach_path(
        Element::new().w(10.0).h(10.0),
        &polyline(&[(0.0, 0.0), (4.0, 5.0)]),
        &PathStyle::stroke([0.1, 0.2, 0.3, 1.0], 2.0),
    );
    let d = el.get_attr(ATTR_PATH_D).expect("d");
    assert!(d.contains('M') && d.contains('L'), "{d}");
    assert!(el.get_attr(ATTR_PATH_STROKE).is_some());
    let quads = path_quads_from_element_attrs(&el.attrs, [100.0, 50.0], [10.0, 10.0]).unwrap();
    assert!(quads.iter().all(|q| q.pos.0 >= 100.0 - 4.0));
}

#[test]
fn raster_stroke_is_continuous_at_joint() {
    let cmds = polyline(&[(8.0, 40.0), (40.0, 8.0), (72.0, 36.0)]);
    let rgba = rasterize_path_rgba(
        &cmds,
        &PathStyle::stroke([1.0, 0.0, 0.0, 1.0], 5.0),
        &[],
        80,
        48,
        1,
    );
    let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
    assert!(
        alpha(40, 8) > 180,
        "joint center should be covered, a={}",
        alpha(40, 8)
    );
    assert!(
        alpha(24, 24) > 120,
        "first segment mid should be covered, a={}",
        alpha(24, 24)
    );
    assert!(
        alpha(56, 22) > 80,
        "second segment should be covered, a={}",
        alpha(56, 22)
    );
}

#[test]
fn stroke_long_scribble_stays_under_interactive_budget() {
    let pts: Vec<Point2> = (0..400)
        .map(|i| {
            let t = i as f32;
            (4.0 + t * 0.75, 100.0 + (t * 0.11).sin() * 70.0)
        })
        .collect();
    let cmds = polyline(&pts);
    let t0 = std::time::Instant::now();
    let rgba = rasterize_path_rgba(
        &cmds,
        &PathStyle::stroke([0.1, 0.1, 0.1, 1.0], 2.5),
        &[],
        320,
        200,
        1,
    );
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(rgba.len(), 320 * 200 * 4);
    assert!(
        ms < 40.0,
        "per-segment stroke of 400 pts on 320x200 should stay interactive, took {ms:.1}ms"
    );
    assert!(
        rgba.chunks(4).any(|p| p[3] > 40),
        "scribble should leave visible stroke coverage"
    );
}

#[test]
fn raster_pie_has_antialiased_rim() {
    let cmds = pie_sector(40.0, 40.0, 30.0, 0.0, std::f32::consts::FRAC_PI_2, 24);
    let rgba = rasterize_path_rgba(
        &cmds,
        &PathStyle::fill([0.2, 0.4, 0.9, 1.0]),
        &[],
        80,
        80,
        2,
    );
    let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
    assert!(alpha(55, 55) > 200, "inside wedge, a={}", alpha(55, 55));
    assert!(alpha(78, 40) < 40, "outside rim, a={}", alpha(78, 40));
    let fringe = (0..80u32)
        .flat_map(|y| (0..80u32).map(move |x| alpha(x, y)))
        .filter(|&a| a > 20 && a < 230)
        .count();
    assert!(
        fringe > 8,
        "rim should have anti-aliased fringe pixels, got {fringe}"
    );
}

#[test]
fn legacy_points_attr_still_decodes() {
    let mut el = Element::new();
    el.set_attr(ATTR_PATH_POINTS, "0,0;8,0;8,6");
    el.set_attr(ATTR_PATH_KIND, "polyline");
    let cmds = commands_from_attrs(&el.attrs);
    assert_eq!(cmds.len(), 3);
    match cmds[0] {
        PathCommand::MoveTo((x, y)) => {
            assert!((x - 0.0).abs() < 1e-3 && (y - 0.0).abs() < 1e-3);
        }
        _ => panic!("expected MoveTo"),
    }
}

#[test]
fn flip_rgba_vertical_swaps_first_and_last_rows() {
    let mut rgba = vec![
        1, 0, 0, 255, 2, 0, 0, 255, // row 0
        0, 3, 0, 255, 0, 4, 0, 255, // row 1
        0, 0, 5, 255, 0, 0, 6, 255, // row 2
    ];
    flip_rgba_vertical(&mut rgba, 2, 3);
    assert_eq!(&rgba[0..8], &[0, 0, 5, 255, 0, 0, 6, 255]);
    assert_eq!(&rgba[8..16], &[0, 3, 0, 255, 0, 4, 0, 255]);
    assert_eq!(&rgba[16..24], &[1, 0, 0, 255, 2, 0, 0, 255]);
}
