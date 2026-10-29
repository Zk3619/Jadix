use super::*;
use crate::primitives::path::{points_from_commands, rasterize_layers_rgba};

#[test]
fn svg_raster_pixel_size_is_at_least_2x() {
    crate::style::set_gradient_raster_dpi(1.0);
    assert_eq!(svg_raster_pixel_size([64.0, 64.0]), (128, 128));
    crate::style::set_gradient_raster_dpi(1.5);
    let (w, h) = svg_raster_pixel_size([64.0, 64.0]);
    assert!(w >= 192 && h >= 192, "1.5 dpi + 2× SSAA, got {w}×{h}");
    crate::style::set_gradient_raster_dpi(1.0);
}

fn sample_svg() -> Element {
    svg()
        .view_box("0 0 100 100")
        .w(160.0)
        .h(160.0)
        .child(
            circle()
                .cx(50.0)
                .cy(50.0)
                .r(36.0)
                .fill("primary")
                .stroke("foreground")
                .stroke_width(3.0),
        )
        .child(
            path()
                .d("M12,78 L50,88 L88,78")
                .fill("none")
                .stroke("destructive")
                .stroke_width(4.0),
        )
        .into()
}

#[test]
fn builder_leaf_has_no_taffy_children() {
    let el = sample_svg();
    assert!(
        el.children.is_empty(),
        "svg shapes must flatten into the leaf, got {} children",
        el.children.len()
    );
    assert!(el.get_attr(ATTR_PATH_SCENE).is_some());
    let d = el.get_attr(ATTR_PATH_D).expect("shared path d");
    assert!(
        d.contains('M') && (d.contains('L') || d.contains('A')),
        "{d}"
    );
    assert_eq!(
        el.get_attr(ATTR_PATH_FIT_LEAF).map(String::as_str),
        Some("1")
    );
}

#[test]
fn compile_circle_and_path_use_path_command_kernel() {
    let nodes = vec![
        node_from_element(&circle().cx(50).cy(50).r(10).fill("primary").into()).unwrap(),
        node_from_element(
            &path()
                .d("M0 0 L10 0 L10 8 Z")
                .fill("none")
                .stroke("foreground")
                .stroke_width(2)
                .into(),
        )
        .unwrap(),
    ];
    let (layers, vb) = compile_layers(&nodes, Some("0 0 100 100"));
    assert_eq!(layers.len(), 2);
    assert!((vb[2] - 100.0).abs() < 1e-3 && (vb[3] - 100.0).abs() < 1e-3);
    let circle_pts = points_from_commands(&layers[0].0);
    assert!(circle_pts.len() >= 8);
    assert!(
        layers[0]
            .0
            .iter()
            .any(|c| matches!(c, PathCommand::Arc { .. })),
        "svg circle must keep Arc until flatten, got {:?}",
        layers[0].0
    );
    assert!(layers[0].1.fill.is_some());
    assert!(layers[1].1.stroke.is_some() && layers[1].1.fill.is_none());
    match &layers[1].0[0] {
        PathCommand::MoveTo(_) => {}
        other => panic!("path must start with MoveTo, got {other:?}"),
    }
}

#[test]
fn raster_has_fill_and_stroke_coverage() {
    let nodes = vec![
        node_from_element(&circle().cx(40).cy(40).r(24).fill("#2563eb").into()).unwrap(),
        node_from_element(
            &path()
                .d("M8 72 L40 80 L72 72")
                .fill("none")
                .stroke("#dc2626")
                .stroke_width(5)
                .into(),
        )
        .unwrap(),
    ];
    let (layers, _) = compile_layers(&nodes, Some("0 0 80 80"));
    let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 80, 80, 1);
    let alpha = |x: u32, y: u32| rgba[((y * 80 + x) * 4 + 3) as usize];
    assert!(
        alpha(40, 40) > 180,
        "circle fill center, a={}",
        alpha(40, 40)
    );
    assert!(
        alpha(40, 80.min(79)) > 40 || alpha(40, 72) > 80,
        "path stroke"
    );
    assert!(
        alpha(2, 2) < 20,
        "outside should be empty, a={}",
        alpha(2, 2)
    );
}

#[test]
fn circle_stroke_ring_is_distinct_from_fill() {
    let node = node_from_element(
        &circle()
            .cx(40)
            .cy(40)
            .r(20)
            .fill("#2563eb")
            .stroke("#dc2626")
            .stroke_width(5)
            .into(),
    )
    .unwrap();
    let (layers, _) = compile_layers(&[node], Some("0 0 80 80"));
    assert!(layers[0].1.fill.is_some() && layers[0].1.stroke.is_some());
    let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 80, 80, 1);
    let px = |x: u32, y: u32| {
        let i = ((y * 80 + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
    };
    let (cr, cg, cb, ca) = px(40, 40);
    assert!(
        ca > 180 && cb > cr + 40,
        "center should be blue fill, got {cr},{cg},{cb},{ca}"
    );
    let (rr, rg, rb, ra) = px(40, 20);
    assert!(
        ra > 80 && rr > rb + 30,
        "rim should show red stroke, got {rr},{rg},{rb},{ra}"
    );
}

#[test]
fn group_translate_moves_circle() {
    let g_el: Element = g()
        .transform("translate(30 0)")
        .child(circle().cx(10).cy(10).r(4).fill("#111111"))
        .into();
    let node = node_from_element(&g_el).unwrap();
    let (layers, _) = compile_layers(&[node], Some("0 0 80 80"));
    let pts = points_from_commands(&layers[0].0);
    let n = pts.len().max(1) as f32;
    let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
    assert!(
        (cx - 40.0).abs() < 1.5,
        "translated circle centroid x should be ~40, got {cx}"
    );
}

#[test]
fn defs_gradient_clip_use_emit_document() {
    let builder = svg()
        .view_box("0 0 100 100")
        .child(
            defs()
                .child(
                    linear_gradient()
                        .id("g1")
                        .x1("0%")
                        .y1("0%")
                        .x2("100%")
                        .y2("0%")
                        .child(stop().offset("0%").stop_color("#2563eb"))
                        .child(stop().offset("100%").stop_color("#dc2626")),
                )
                .child(
                    clip_path()
                        .id("clip1")
                        .child(rect().x(10).y(10).w(80).h(80)),
                )
                .child(
                    symbol()
                        .id("icon")
                        .view_box("0 0 24 24")
                        .child(circle().cx(12).cy(12).r(10).fill("url(#g1)")),
                ),
        )
        .child(
            svg_use()
                .href("#icon")
                .x(0)
                .y(0)
                .w(100)
                .h(100)
                .clip_path("url(#clip1)"),
        );
    let nodes: Vec<SvgNode> = builder
        .el
        .children
        .iter()
        .filter_map(node_from_element)
        .collect();
    let el: Element = builder.into();
    assert!(el.get_attr(super::ATTR_SVG_SCENE).is_some());
    let out = emit_svg_document(&nodes, [0.0, 0.0, 100.0, 100.0]);
    assert!(out.contains("<linearGradient"), "{out}");
    assert!(out.contains("<clipPath"), "{out}");
    assert!(out.contains("<use"), "{out}");
    assert!(out.contains("url(#g1)"), "{out}");
}

#[test]
fn smil_rotate_bakes_transform_at_time() {
    let g_el: Element = g()
        .child(
            animate_transform()
                .attribute_name("transform")
                .transform_type("rotate")
                .from_val("0 50 50")
                .to_val("90 50 50")
                .dur("1s")
                .repeat_count("indefinite"),
        )
        .child(circle().cx(50).cy(50).r(20).fill("#111111"))
        .into();
    let node = node_from_element(&g_el).unwrap();
    assert!(crate::primitives::svg_smil::node_has_smil(&node));
    let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.25);
    let doc = emit_svg_document_at(&baked, [0.0, 0.0, 100.0, 100.0], None);
    assert!(
        doc.contains("rotate(22.5") || doc.contains("rotate(22.5 "),
        "{doc}"
    );
}

#[test]
fn smil_animate_r_bakes_on_circle() {
    let el: Element = circle()
        .cx(40)
        .cy(40)
        .r(10)
        .fill("#111111")
        .child(
            animate()
                .attribute_name("r")
                .from_val("10")
                .to_val("30")
                .dur("1s")
                .repeat_count("1"),
        )
        .into();
    let node = node_from_element(&el).unwrap();
    assert!(crate::primitives::svg_smil::node_has_smil(&node));
    let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.5);
    let (layers, _) = compile_layers(&baked, Some("0 0 80 80"));
    assert!(!layers.is_empty());
    let pts = points_from_commands(&layers[0].0);
    let n = pts.len().max(1) as f32;
    let mean_r = pts
        .iter()
        .map(|(x, y)| (x - 40.0).hypot(y - 40.0))
        .sum::<f32>()
        / n;
    assert!(
        (mean_r - 20.0).abs() < 2.0,
        "r should lerp to ~20 at t=0.5, got {mean_r}"
    );
}

#[test]
fn smil_set_opacity_bakes_on_group() {
    let el: Element = g()
        .child(set_el().attribute_name("opacity").to_val("0.25"))
        .child(circle().cx(10).cy(10).r(5).fill("#ffffff"))
        .into();
    let node = node_from_element(&el).unwrap();
    let baked = crate::primitives::svg_smil::apply_smil(&[node], 0.0);
    match &baked[0] {
        SvgNode::Group { opacity, .. } => {
            assert!((*opacity - 0.25).abs() < 1e-3, "opacity={opacity}");
        }
        other => panic!("expected group, got {other:?}"),
    }
}

#[test]
fn cpu_path_resolves_url_linear_gradient_fill() {
    let nodes = vec![
        node_from_element(
            &defs()
                .child(
                    linear_gradient()
                        .id("g")
                        .x1("0%")
                        .y1("0%")
                        .x2("100%")
                        .y2("0%")
                        .child(stop().offset("0%").stop_color("#0000ff"))
                        .child(stop().offset("100%").stop_color("#ff0000")),
                )
                .into(),
        )
        .unwrap(),
        node_from_element(&rect().x(0).y(0).w(64).h(64).fill("url(#g)").into()).unwrap(),
    ];
    let (layers, _) = compile_layers(&nodes, Some("0 0 64 64"));
    assert_eq!(layers.len(), 1);
    assert!(
        layers[0].1.fill_gradient.is_some(),
        "url(#g) must resolve to fill_gradient"
    );
    let rgba = rasterize_layers_rgba(&layers, &[], 0.0, 64, 64, 1);
    let px = |x: u32, y: u32| {
        let i = ((y * 64 + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
    };
    let (lr, _, lb, la) = px(4, 32);
    let (rr, _, _, ra) = px(60, 32);
    assert!(la > 180 && lb > lr + 40, "left blue, got {lr},{lb},{la}");
    assert!(ra > 180 && rr > lb, "right red, got {rr},{ra}");
}

#[cfg(feature = "svg")]
#[test]
fn resvg_rasterizes_linear_gradient_fill() {
    let builder = svg()
        .view_box("0 0 64 64")
        .child(
            defs().child(
                linear_gradient()
                    .id("g")
                    .x1("0%")
                    .y1("0%")
                    .x2("100%")
                    .y2("0%")
                    .child(stop().offset("0%").stop_color("#0000ff"))
                    .child(stop().offset("100%").stop_color("#ff0000")),
            ),
        )
        .child(rect().x(0).y(0).w(64).h(64).fill("url(#g)"));
    let nodes: Vec<SvgNode> = builder
        .el
        .children
        .iter()
        .filter_map(node_from_element)
        .collect();
    assert!(!nodes.is_empty(), "gradient scene must parse to SvgNode");
    let el: Element = builder.into();
    assert!(el.get_attr(ATTR_SVG_SCENE).is_some());
    let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
    assert!(doc.contains("linearGradient"), "{doc}");
    let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, 64, 64).expect("resvg");
    let px = |x: u32, y: u32| {
        let i = ((y * 64 + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
    };
    let (lr, _, lb, la) = px(4, 32);
    let (rr, _, _, ra) = px(60, 32);
    assert!(
        la > 180 && lb > lr + 40,
        "left should be blue, got {lr},{lb},{la}"
    );
    assert!(ra > 180 && rr > lb, "right should be red, got {rr},{ra}");
}

#[test]
fn elem_text_tspan_emits_font_size() {
    let nodes = vec![node_from_element(
        &svg_text()
            .attr("x", "10")
            .attr("y", "20")
            .font_size(14.0)
            .text_content("Hello ")
            .child(tspan().font_size(18.0).text_content("World"))
            .into(),
    )
    .unwrap()];
    let doc = emit_svg_document(&nodes, [0.0, 0.0, 100.0, 40.0]);
    assert!(doc.contains("<text"), "{doc}");
    assert!(doc.contains("font-size"), "{doc}");
    assert!(doc.contains("Hello "), "{doc}");
    assert!(doc.contains("<tspan"), "{doc}");
    assert!(doc.contains("World"), "{doc}");
}

#[test]
fn filter_fe_gaussian_nested_in_defs() {
    let nodes = vec![
        node_from_element(
            &filter()
                .id("blur")
                .child(
                    fe_gaussian_blur()
                        .attr("in", "SourceGraphic")
                        .attr("stdDeviation", "2"),
                )
                .into(),
        )
        .unwrap(),
        node_from_element(
            &circle()
                .cx(32)
                .cy(32)
                .r(20)
                .fill("#2563eb")
                .filter_attr("url(#blur)")
                .into(),
        )
        .unwrap(),
    ];
    assert!(is_defs_only_node(&nodes[0]));
    let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
    assert!(doc.contains("<defs>"), "{doc}");
    assert!(doc.contains("<filter"), "{doc}");
    assert!(doc.contains("feGaussianBlur"), "{doc}");
    assert!(doc.contains("filter=\"url(#blur)\""), "{doc}");
}

#[cfg(feature = "svg")]
#[test]
fn resvg_rasterizes_filter_blur_scene() {
    let builder = svg()
        .view_box("0 0 64 64")
        .child(
            filter().id("blur").child(
                fe_gaussian_blur()
                    .attr("in", "SourceGraphic")
                    .attr("stdDeviation", "1.5"),
            ),
        )
        .child(
            circle()
                .cx(32)
                .cy(32)
                .r(18)
                .fill("#2563eb")
                .filter_attr("url(#blur)"),
        );
    let nodes: Vec<SvgNode> = builder
        .el
        .children
        .iter()
        .filter_map(node_from_element)
        .collect();
    let el: Element = builder.into();
    assert!(el.get_attr(ATTR_SVG_SCENE).is_some());
    let doc = emit_svg_document(&nodes, [0.0, 0.0, 64.0, 64.0]);
    let rgba = crate::media::svg_raster::rasterize_svg_str(&doc, 64, 64).expect("resvg");
    let nonempty = rgba.chunks(4).any(|px| px[3] > 20);
    assert!(nonempty, "filter scene should rasterize non-empty pixels");
}

#[test]
fn scene_has_smil_detects_elem_animate_motion() {
    let n = node_from_element(
        &animate_motion()
            .attr("path", "M0 0 L10 10")
            .attr("dur", "1s")
            .into(),
    )
    .unwrap();
    assert!(crate::primitives::svg_smil::node_has_smil(&n));
}

#[test]
fn pointer_events_fill_hits_interior_not_box_corner() {
    use crate::app::pointer_events::hit_path;
    use crate::core::tree::PointerEvents;

    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("fill")
        .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
        .into();
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    assert_eq!(el.pointer_events_mode(), PointerEvents::Fill);
    assert!(crate::primitives::path::element_has_path_geometry(
        &el.attrs
    ));

    let center = hit_path(&el, 50.0, 50.0);
    assert!(!center.is_empty(), "fill mode must hit circle interior");
    let corner = hit_path(&el, 95.0, 95.0);
    assert!(
        corner.is_empty(),
        "fill mode must miss outside the circle (inside leaf box)"
    );
}

#[test]
fn pointer_events_fill_hits_origin_left_rotate_90_painted_center() {
    use crate::app::pointer_events::hit_path;

    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("fill")
        .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
        .into();
    el.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    el.style.rotate_deg = 90.0;
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    assert!(
        !hit_path(&el, 350.0, 250.0).is_empty(),
        "fill must hit the painted circle after origin-left 90°"
    );
    assert!(
        hit_path(&el, 450.0, 250.0).is_empty(),
        "old layout center must miss the rotated circle"
    );
}

#[test]
fn pointer_events_stroke_hits_perimeter_not_center() {
    use crate::app::pointer_events::hit_path;

    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("stroke")
        .child(
            circle()
                .cx(50.0)
                .cy(50.0)
                .r(30.0)
                .fill("none")
                .stroke("#0f0")
                .stroke_width(6.0),
        )
        .into();
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let rim = hit_path(&el, 50.0, 20.0); // top of circle
    assert!(!rim.is_empty(), "stroke mode must hit perimeter");
    let center = hit_path(&el, 50.0, 50.0);
    assert!(center.is_empty(), "stroke mode must miss hollow interior");
}

#[test]
fn pointer_events_bounding_box_hits_empty_corner() {
    use crate::app::pointer_events::hit_path;

    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("bounding-box")
        .child(circle().cx(50.0).cy(50.0).r(10.0).fill("#00f"))
        .into();
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let corner = hit_path(&el, 2.0, 2.0);
    assert!(!corner.is_empty(), "bounding-box must hit entire leaf");
}

#[test]
fn pointer_events_none_misses_everywhere() {
    use crate::app::pointer_events::hit_path;

    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("none")
        .child(circle().cx(50.0).cy(50.0).r(40.0).fill("#00f"))
        .into();
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    assert!(hit_path(&el, 50.0, 50.0).is_empty());
}

#[test]
fn pointer_events_auto_on_svg_equals_visible_painted() {
    use crate::app::pointer_events::hit_path;

    // auto on SVG leaf → visiblePainted: fill painted ⇒ interior hits; empty corner misses.
    let mut el: Element = svg()
        .view_box("0 0 100 100")
        .w(100.0)
        .h(100.0)
        .pointer_events("auto")
        .child(circle().cx(50.0).cy(50.0).r(20.0).fill("#f00"))
        .into();
    el.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    assert!(!hit_path(&el, 50.0, 50.0).is_empty());
    assert!(hit_path(&el, 95.0, 95.0).is_empty());
}
