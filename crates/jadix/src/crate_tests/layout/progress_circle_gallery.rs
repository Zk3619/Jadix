//! Gallery-shaped ProgressCircle: root effect defers anchors → reconcile → layout paint.

use crate::app::reactive::reset_registered_anchors;
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::{
    progress_circle, progress_circle_svg, ProgressCircle, ProgressCircleProps, ProgressCircleSize,
};
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::path::ATTR_PATH_SCENE;
use crate::primitives::{hstack, vstack};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn progress_circle_gpu_stroke_stays_inside_outer_circle() {
    use crate::primitives::path::{path_gpu_from_element_attrs, take_path_jobs, PathGpuOp};
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    for size in [
        ProgressCircleSize::Sm,
        ProgressCircleSize::Md,
        ProgressCircleSize::Lg,
    ] {
        for value in [0.0, 0.25, 0.6, 1.0] {
            let el = progress_circle_svg(value, size, None, None, false);
            let r = size.radius();
            let _ = take_path_jobs();
            path_gpu_from_element_attrs(&el.attrs, r * 2.0, r * 2.0).unwrap();
            for job in take_path_jobs() {
                for op in job.ops.into_ops() {
                    if let PathGpuOp::Mesh(mesh) = op {
                        for tri in mesh.tris {
                            for p in [tri.a, tri.b, tri.c] {
                                let distance = (p.0 - r).hypot(p.1 - r);
                                assert!(distance <= r + 0.05,
                                    "{size:?} at {value}: stroke vertex {p:?} extends beyond outer radius {r}: {distance}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn progress_circle_gpu_texture_matches_display_scale() {
    use crate::primitives::path::{path_gpu_from_element_attrs, take_path_jobs};
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    let el = progress_circle_svg(0.6, ProgressCircleSize::Md, None, None, false);
    for dpi in [1.0, 1.5, 2.0] {
        let _ = take_path_jobs();
        crate::style::set_gradient_raster_dpi(dpi);
        let (_, w, h) = path_gpu_from_element_attrs(&el.attrs, 64.0, 64.0).unwrap();
        let jobs = take_path_jobs();
        crate::style::set_gradient_raster_dpi(1.0);
        assert_eq!([w, h], [64.0, 64.0], "logical draw size must stay fixed");
        let physical = (64.0 * dpi).ceil() as u32;
        assert_eq!(
            [jobs[0].width, jobs[0].height],
            [physical, physical],
            "circle texture must render at display resolution for scale {dpi}"
        );
    }
}

#[test]
fn progress_circle_cpu_texture_matches_display_scale() {
    use crate::primitives::path::{flush_path_textures, path_raster_from_element_attrs};
    let el = progress_circle_svg(0.6, ProgressCircleSize::Md, None, None, false);
    for dpi in [1.0, 1.5, 2.0] {
        crate::style::set_gradient_raster_dpi(dpi);
        let (key, w, h) = path_raster_from_element_attrs(&el.attrs, 64.0, 64.0).unwrap();
        crate::style::set_gradient_raster_dpi(1.0);
        flush_path_textures(|_| false);
        let (rgba, tw, th) = crate::app::pending_images::take_pending_key(&key).unwrap();
        assert_eq!([w, h], [64.0, 64.0]);
        let physical = (64.0 * dpi).ceil() as u32;
        assert_eq!(
            [tw, th],
            [physical, physical],
            "CPU fallback must also render at display resolution for scale {dpi}"
        );
        let mid_stroke_y = (ProgressCircleSize::Md.stroke_width() * dpi * 0.5) as u32;
        assert!(
            rgba[((mid_stroke_y * tw + tw / 2) * 4 + 3) as usize] > 240,
            "scaled stroke must remain opaque at its center"
        );
        assert_eq!(
            rgba[((th / 2 * tw + tw / 2) * 4 + 3) as usize],
            0,
            "scaled circle must keep a transparent center"
        );
    }
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn progress_circle_gpu_pixels_have_smooth_round_edges() {
    use crate::primitives::path::{
        path_gpu_from_element_attrs, request_path_rt_readback, take_path_jobs,
        take_path_rt_readback,
    };
    use crate::renderer::{offscreen::OffscreenGpu, path_pipeline::PathGpu, TexturedQuadPipeline};
    let gpu = OffscreenGpu::new().expect("headless GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 320,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let textured = TexturedQuadPipeline::new(&gpu.device, &config);
    let mut path = PathGpu::try_new(&gpu.device, 1).expect("path pipeline");
    let mut images = std::collections::HashMap::new();
    for dpi in [1.0, 1.5, 2.0] {
        for size in [
            ProgressCircleSize::Sm,
            ProgressCircleSize::Md,
            ProgressCircleSize::Lg,
        ] {
            let el = progress_circle_svg(0.6, size, None, None, false);
            let d = size.radius() * 2.0;
            let _ = take_path_jobs();
            crate::style::set_gradient_raster_dpi(dpi);
            let (key, _, _) = path_gpu_from_element_attrs(&el.attrs, d, d).unwrap();
            crate::style::set_gradient_raster_dpi(1.0);
            request_path_rt_readback(&key);
            assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
            let (w, h, rgba) = take_path_rt_readback(&key).expect("GPU pixels");
            assert_eq!([w, h], [(d * dpi) as u32; 2]);
            let r = w as f32 * 0.5;
            for y in 0..h {
                for x in 0..w {
                    if (x as f32 + 0.5 - r).hypot(y as f32 + 0.5 - r) > r + 0.75 {
                        assert_eq!(
                            rgba[((y * w + x) * 4 + 3) as usize],
                            0,
                            "{size:?} at scale {dpi}: square join protrudes at ({x}, {y})"
                        );
                    }
                }
            }
            if let Ok(dir) = std::env::var("JADIX_PROGRESS_CAPTURE_DIR") {
                image::save_buffer(
                    std::path::Path::new(&dir).join(format!("circle-{size:?}-{dpi}.png")),
                    &rgba,
                    w,
                    h,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}

fn tree_has_path_scene(el: &Element) -> bool {
    el.get_attr(ATTR_PATH_SCENE).is_some() || el.children.iter().any(tree_has_path_scene)
}

fn tree_has_placeholder(el: &Element) -> bool {
    el.template_hash.as_deref() == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH)
        || el.children.iter().any(tree_has_placeholder)
}

fn count_attr(el: &Element, key: &str) -> usize {
    let here = usize::from(el.get_attr(key).is_some());
    here + el
        .children
        .iter()
        .map(|c| count_attr(c, key))
        .sum::<usize>()
}

fn find_circle_content(el: &Element) -> Option<&Element> {
    if el.get_attr("data-slot").map(String::as_str) == Some("progress-circle-content") {
        Some(el)
    } else {
        el.children.iter().find_map(find_circle_content)
    }
}

fn find_circle_child<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) {
        Some(el)
    } else {
        el.children.iter().find_map(|c| find_circle_child(c, id))
    }
}

#[test]
fn progress_circle_custom_image_fits_hole_and_keeps_ring_size() {
    for size in [
        ProgressCircleSize::Xs,
        ProgressCircleSize::Sm,
        ProgressCircleSize::Md,
        ProgressCircleSize::Lg,
        ProgressCircleSize::Xl,
    ] {
        for (radius, stroke) in [(None, None), (Some(40.0), Some(12.0))] {
            for builder in [false, true] {
                let (_owner, mut root) = create_root(|| {
                    let image = crate::components::ui_image()
                        .id("circle-content-image")
                        .texture("circle-content-image-texture")
                        .style(|b| b.w("100%").h("100%"));
                    let circle = if builder {
                        let mut b = progress_circle().size(size).default_value(0.6).child(image);
                        if let Some(r) = radius {
                            b = b.radius(r);
                        }
                        if let Some(w) = stroke {
                            b = b.stroke_width(w);
                        }
                        b.into()
                    } else {
                        ProgressCircle(ProgressCircleProps {
                            size: Some(size),
                            value: Some(0.6),
                            radius,
                            stroke_width: stroke,
                            children: Some(image.into()),
                            ..Default::default()
                        })
                    };
                    Element::new().w(300.0).h(220.0).child(circle)
                });
                let mut layout = LayoutTree::new();
                let result = layout.build_and_layout(&mut root, 300.0, 220.0, [-1.0, -1.0]);
                let circle = &root.children[0];
                let [x, y, w, h] = circle.layout_bounds;
                let outer_r = radius.unwrap_or(size.radius());
                let inner_r = outer_r - stroke.unwrap_or(size.stroke_width());
                assert!((w - outer_r * 2.0).abs() < 0.1);
                assert!((h - outer_r * 2.0).abs() < 0.1);
                let slot = find_circle_content(circle).expect("inner content slot");
                let [sx, sy, sw, sh] = slot.layout_bounds;
                assert!((sx + sw * 0.5 - (x + w * 0.5)).abs() < 0.6);
                assert!((sy + sh * 0.5 - (y + h * 0.5)).abs() < 0.6);
                assert!(
                    (sw - inner_r * 2.0).abs() < 0.6,
                    "content must fill the inner diameter"
                );
                assert!((sh - inner_r * 2.0).abs() < 0.6);
                let draw = result
                    .textured_draws
                    .iter()
                    .find(|d| d.texture_key == "circle-content-image-texture")
                    .expect("custom image must paint alongside the ring");
                for (actual, expected) in draw.rect.into_iter().zip(slot.layout_bounds) {
                    assert!(
                        (actual - expected).abs() < 0.1,
                        "100% image must fill the inner slot"
                    );
                }
                assert_eq!(draw.rounded_clips[0], slot.layout_bounds);
                assert_eq!(draw.rounded_clips[1], [inner_r; 4]);
                assert_eq!(count_attr(circle, ATTR_PATH_SCENE), 1);
            }
        }
    }
}

#[test]
fn progress_circle_content_clips_oversized_children_without_hiding_ring() {
    let (_owner, mut root) = create_root(|| {
        let image = Element::new()
            .w(200.0)
            .h(200.0)
            .flex_shrink(0.0)
            .image_texture("circle-oversized-image")
            .on_click(|| {});
        let circle: Element = progress_circle()
            .radius(40.0)
            .stroke_width(10.0)
            .default_value(0.6)
            .child(image)
            .into();
        Element::new().w(200.0).h(200.0).child(circle)
    });
    let mut layout = LayoutTree::new();
    let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let slot = find_circle_content(&root).expect("slot");
    let [x, y, w, h] = slot.layout_bounds;
    let draw = result
        .textured_draws
        .iter()
        .find(|d| d.texture_key == "circle-oversized-image")
        .expect("clipped image");
    assert_eq!(draw.rect, slot.layout_bounds);
    assert!(
        draw.uv_scale[0] < 0.35 && draw.uv_scale[1] < 0.35,
        "overflow must crop the image rather than squeeze it"
    );
    assert!(
        crate::app::tree_hit::find_click(&root, x + w / 2.0, y + h / 2.0, [0.0, 0.0]).is_some()
    );
    assert!(crate::app::tree_hit::find_click(&root, x - 2.0, y + h / 2.0, [0.0, 0.0]).is_none());
    assert!(
        crate::app::tree_hit::find_click(&root, x + 1.0, y + 1.0, [0.0, 0.0]).is_none(),
        "circular clipping must reject image corner hits"
    );
    assert_eq!(draw.rounded_clips[0], slot.layout_bounds);
    assert_eq!(draw.rounded_clips[1], [30.0; 4]);
    assert_eq!(root.children[0].layout_bounds[2..], [80.0, 80.0]);
}

#[test]
fn progress_circle_custom_percentage_updates_without_remounting_content() {
    reset_registered_anchors();
    clear_test_reconcile_root();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let mounts = Rc::new(std::cell::Cell::new(0));
    let (_owner, value) = create_root(|| {
        let value = use_signal("circle-content-value", || 0.45f32);
        mark_next_effect_as_ui_root();
        create_effect({
            let live = Rc::clone(&live);
            let mounts = Rc::clone(&mounts);
            move || {
                mounts.set(mounts.get() + 1);
                let percentage = Element::new()
                    .template_id("circle-content-percentage")
                    .text_size(20.0)
                    .text(move || format!("{:.0}%", value.get().clamp(0.0, 1.0) * 100.0));
                let circle = ProgressCircle(ProgressCircleProps {
                    id: "circle-content-reactive".into(),
                    size: Some(ProgressCircleSize::Lg),
                    value_signal: Some(value),
                    children: Some(
                        vstack()
                            .items_center()
                            .child(
                                Element::new()
                                    .template_id("circle-content-photo")
                                    .w(24.0)
                                    .h(24.0)
                                    .image_texture("circle-content-photo-texture"),
                            )
                            .child(percentage)
                            .into(),
                    ),
                    ..Default::default()
                });
                live.borrow_mut().children = vec![circle];
            }
        });
        value
    });
    let mut layout = LayoutTree::new();
    for (v, expected) in [(0.45, "45%"), (1.5, "100%"), (-0.2, "0%"), (0.7, "70%")] {
        value.set(v);
        let mut root = live.borrow_mut();
        assert_eq!(
            find_circle_child(&root, "circle-content-percentage")
                .unwrap()
                .text
                .as_deref(),
            Some(expected)
        );
        assert_eq!(
            find_circle_child(&root, "circle-content-reactive__view")
                .unwrap()
                .get_attr("jadix_progress")
                .map(String::as_str),
            Some(expected)
        );
        assert_eq!(
            find_circle_child(&root, "circle-content-photo")
                .unwrap()
                .image_texture
                .as_deref(),
            Some("circle-content-photo-texture")
        );
        let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
        assert!(result.text_nodes.iter().any(|n| n.text == expected));
        let slot = find_circle_content(&root).unwrap();
        let content = &slot.children[0];
        for axis in [0, 1] {
            assert!(
                (content.layout_bounds[axis] + content.layout_bounds[axis + 2] * 0.5
                    - slot.layout_bounds[axis]
                    - slot.layout_bounds[axis + 2] * 0.5)
                    .abs()
                    < 0.6,
                "composed content must remain centered: content={:?}, slot={:?}, value={v}",
                content.layout_bounds,
                slot.layout_bounds
            );
        }
    }
    assert_eq!(
        mounts.get(),
        1,
        "updates must patch the ring and text in place"
    );
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
#[ignore = "requires a headless GPU adapter"]
fn progress_circle_gpu_image_fills_and_clips_entire_inner_circle() {
    use crate::renderer::{
        offscreen::{readback_texture, OffscreenGpu},
        path_pipeline::PathGpu,
        scale_textured_draws, ScaledTexturedDraw, TexturedQuadPipeline,
    };
    use wgpu::util::DeviceExt;
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    for dpi in [1.0f32, 1.5, 2.0] {
        let side = (104.0 * dpi) as u32;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: side,
            height: side,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        let textured = TexturedQuadPipeline::new(&gpu.device, &config);
        let (_owner, mut root) = create_root(|| {
            let circle: Element = progress_circle()
                .size(ProgressCircleSize::Lg)
                .default_value(0.6)
                .show_animation(false)
                .child(
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .image_texture("circle-gpu-photo"),
                )
                .into();
            Element::new().w(104.0).h(104.0).child(circle)
        });
        let mut layout = LayoutTree::new();
        let _ = crate::primitives::path::take_path_jobs();
        crate::style::set_gradient_raster_dpi(dpi);
        let result = layout.build_and_layout(&mut root, 104.0, 104.0, [-1.0, -1.0]);
        crate::style::set_gradient_raster_dpi(1.0);
        let mut images = std::collections::HashMap::new();
        images.insert(
            "circle-gpu-photo".to_string(),
            crate::app::image_registry::upload_rgba8(
                &gpu.device,
                &gpu.queue,
                &textured,
                &[0, 255, 0, 255].repeat(16),
                4,
                4,
            ),
        );
        let mut path = PathGpu::try_new(&gpu.device, 1).unwrap();
        assert!(path.flush(&gpu.device, &gpu.queue, &textured, &mut images));
        let draws = scale_textured_draws(&result.textured_draws, dpi);
        let render = |draws: &[ScaledTexturedDraw]| {
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("circle content pixels"),
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let instances: Vec<_> = draws.iter().map(ScaledTexturedDraw::to_instance).collect();
            let buf = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("circle content instances"),
                    contents: bytemuck::cast_slice(&instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("circle content pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&textured.render_pipeline);
                pass.set_vertex_buffer(0, textured.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, buf.slice(..));
                pass.set_index_buffer(textured.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                for (i, draw) in draws.iter().enumerate() {
                    pass.set_bind_group(0, &images[&draw.texture_key].bind_group, &[]);
                    pass.draw_indexed(0..textured.num_indices, 0, i as u32..i as u32 + 1);
                }
            }
            readback_texture(&gpu.device, &gpu.queue, &target, side, side, encoder).unwrap()
        };
        let photo_draw = draws
            .iter()
            .find(|d| d.texture_key == "circle-gpu-photo")
            .unwrap();
        let photo = render(std::slice::from_ref(photo_draw));
        let center = 52.0 * dpi;
        let inner_r = 44.0 * dpi;
        let mut smooth_edge_pixels = 0;
        for y in 0..side {
            for x in 0..side {
                let distance = (x as f32 + 0.5 - center).hypot(y as f32 + 0.5 - center);
                let pixel = photo.pixel(x, y).unwrap();
                if distance > inner_r + 1.0 {
                    assert_eq!(pixel[3], 0, "photo outside circular clip at DPI {dpi}");
                } else if distance < inner_r - 1.5 {
                    assert_eq!(
                        pixel,
                        [0, 255, 0, 255],
                        "photo must fill the entire inner circle at DPI {dpi}"
                    );
                }
                smooth_edge_pixels += usize::from(pixel[3] > 0 && pixel[3] < 255);
            }
        }
        assert!(
            smooth_edge_pixels > 50,
            "circular image edge needs subpixel coverage"
        );
        let composite = render(&draws);
        let top = composite.pixel(center as u32, (4.0 * dpi) as u32).unwrap();
        assert!(
            top[3] > 240 && top[1] < 128,
            "photo must not cover the progress ring"
        );
        let mut nested = photo_draw.clone();
        nested.rounded_clips[2] = [8.0 * dpi, 8.0 * dpi, 88.0 * dpi, 44.0 * dpi];
        nested.rounded_clips[3] = [8.0 * dpi; 4];
        let nested = render(&[nested]);
        assert_eq!(
            nested.pixel(center as u32, (70.0 * dpi) as u32).unwrap()[3],
            0
        );
        assert_eq!(
            nested.pixel(center as u32, (30.0 * dpi) as u32).unwrap()[3],
            255
        );
        if let Ok(dir) = std::env::var("JADIX_PROGRESS_CONTENT_CAPTURE_DIR") {
            image::save_buffer(
                std::path::Path::new(&dir).join(format!("circle-content-{dpi}.png")),
                &composite.pixels,
                side,
                side,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

/// Static SVG in gallery-shaped HStack must layout and produce textured draws.
#[test]
fn static_progress_circle_in_hstack_paints() {
    let md = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, false);
    let sm = progress_circle_svg(0.45, ProgressCircleSize::Sm, None, None, false);
    let lg = progress_circle_svg(0.45, ProgressCircleSize::Lg, None, None, false);
    let mut root: Element = hstack()
        .gap("1rem")
        .items_center()
        .children([md, sm, lg])
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 600.0, 200.0, [-1.0, -1.0]);
    assert_eq!(count_attr(&root, ATTR_PATH_SCENE), 3, "three svg leaves");
    assert!(
        result.textured_draws.len() >= 3,
        "expected >=3 textured draws, got {:?}",
        result.textured_draws
    );
    for (i, child) in root.children.iter().enumerate() {
        let [_, _, w, h] = child.layout_bounds;
        assert!(
            w > 20.0 && h > 20.0,
            "child {i} must have positive layout bounds, got [{w}, {h}]"
        );
    }
}

/// Mimics lifecycle: ui root effect builds tree while anchor effects defer, then reconcile.
#[test]
fn progress_circle_ui_root_effect_mounts_svg() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _): (_, ()) = create_root(|| {
        create_effect(move || {
            let progress_val = use_signal("gal_progress_test", || 0.45f32);
            let builder_el: Element = progress_circle()
                .id("gal-pc-builder")
                .value_signal(progress_val)
                .size(ProgressCircleSize::Lg)
                .into();
            let row: Element = hstack()
                .gap("1rem")
                .items_center()
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-md".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Md),
                    ..Default::default()
                }))
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-sm".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Sm),
                    ..Default::default()
                }))
                .child(builder_el)
                .into();
            let card: Element = vstack().w(600.0).gap("0.5rem").child(row).into();
            live.borrow_mut().children = vec![card];
        });
    });

    let tree = live_check.borrow();
    assert!(
        tree_has_path_scene(&tree),
        "after deferred anchor flush, mounted tree must contain jadix_svg_scene; placeholders={} scenes={}",
        tree_has_placeholder(&tree),
        count_attr(&tree, ATTR_PATH_SCENE)
    );
    assert!(
        !tree_has_placeholder(&tree) || count_attr(&tree, ATTR_PATH_SCENE) > 0,
        "bind_prop must not leave empty placeholders without svg scenes"
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Gallery uses `switch_reactive` + `isolate_reads`; section remount must re-fill anchors.
#[test]
fn progress_circle_switch_reactive_remounts_svg() {
    use crate::app::reactive::switch_reactive;
    use crate::core::reactivity::isolate_reads;

    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let live_check = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, section): (_, _) = create_root(|| {
        let section = use_signal("gal_nav_sw", || "overlays".to_string());
        create_effect({
            let section = section.clone();
            move || {
                let sec = section.clone();
                let body = switch_reactive(
                    "test:gal:section",
                    move || sec.get(),
                    move |key| {
                        isolate_reads(|| {
                            if key == "overlays" {
                                let progress_val = use_signal("gal_progress_sw", || 0.45f32);
                                let row: Element = hstack()
                                    .gap("1rem")
                                    .items_center()
                                    .child(ProgressCircle(ProgressCircleProps {
                                        id: "gal-pc-sw".into(),
                                        value_signal: Some(progress_val),
                                        size: Some(ProgressCircleSize::Md),
                                        ..Default::default()
                                    }))
                                    .into();
                                vstack().w(600.0).child(row).into()
                            } else {
                                Element::new().w(100.0).h(40.0)
                            }
                        })
                    },
                );
                live.borrow_mut().children = vec![body];
            }
        });
        section
    });

    assert!(
        tree_has_path_scene(&live_check.borrow()),
        "first overlays mount must produce svg scene"
    );

    section.set("forms".to_string());
    section.set("overlays".to_string());

    assert!(
        tree_has_path_scene(&live_check.borrow()),
        "remount overlays after section switch must restore svg scene, scenes={}",
        count_attr(&live_check.borrow(), ATTR_PATH_SCENE)
    );

    clear_test_reconcile_root();
    reset_registered_anchors();
}

/// Full layout after ui-root mount: reproduces gallery paint path.
#[test]
fn progress_circle_ui_root_layout_has_textured_draws() {
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let live_layout = Rc::clone(&live);

    mark_next_effect_as_ui_root();
    let (_owner, _): (_, ()) = create_root(|| {
        create_effect(move || {
            let progress_val = use_signal("gal_progress_layout", || 0.45f32);
            let row: Element = hstack()
                .gap("1rem")
                .items_center()
                .child(ProgressCircle(ProgressCircleProps {
                    id: "gal-pc-layout-md".into(),
                    value_signal: Some(progress_val),
                    size: Some(ProgressCircleSize::Md),
                    ..Default::default()
                }))
                .into();
            live_layout.borrow_mut().children = vec![row];
        });
    });

    let mut root = Element::new();
    std::mem::swap(&mut root, &mut *live.borrow_mut());
    clear_test_reconcile_root();
    reset_registered_anchors();

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 120.0, [-1.0, -1.0]);
    assert!(
        tree_has_path_scene(&root),
        "layout tree must still contain svg scene attrs"
    );
    assert!(
        !result.textured_draws.is_empty(),
        "gallery-shaped reactive mount must produce textured draws, got {:?}",
        result.textured_draws
    );
}
