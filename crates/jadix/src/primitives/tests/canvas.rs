use super::*;
use crate::primitives::path::polyline;

fn sample_px(buf: &[f32], w: u32, x: u32, y: u32) -> [f32; 4] {
    let i = (y as usize * w as usize + x as usize) * 4;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

#[test]
fn fill_rect_covers_expected_pixels() {
    let el: Element = canvas()
        .w(32.0)
        .h(16.0)
        .paint(|ctx, _| {
            ctx.clear("#00000000");
            ctx.set_fill("#ff0000");
            ctx.fill_rect(2.0, 2.0, 8.0, 6.0);
            LayoutStatus::Clean
        })
        .into();
    let (buf, w, h) = rasterize_canvas_premul_for_test(&el.attrs, 32.0, 16.0).expect("raster");
    assert_eq!((w, h), (32, 16));
    let p = sample_px(&buf, w, 5, 4);
    assert!(p[0] > 0.8 && p[3] > 0.8, "fill inside rect, got {p:?}");
    let outside = sample_px(&buf, w, 20, 4);
    assert!(
        outside[3] < 0.05,
        "outside rect must stay empty, {outside:?}"
    );
}

#[test]
fn translate_shifts_fill() {
    let el: Element = canvas()
        .w(24.0)
        .h(16.0)
        .paint(|ctx, _| {
            ctx.set_fill("#00ff00");
            ctx.translate(10.0, 0.0);
            ctx.fill_rect(0.0, 2.0, 4.0, 4.0);
            LayoutStatus::Clean
        })
        .into();
    let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 24.0, 16.0).expect("raster");
    let shifted = sample_px(&buf, w, 12, 4);
    assert!(shifted[1] > 0.8, "translated fill, {shifted:?}");
    let origin = sample_px(&buf, w, 1, 4);
    assert!(origin[3] < 0.05, "unshifted origin empty, {origin:?}");
}

#[test]
fn clip_rect_hides_pixels_outside() {
    let el: Element = canvas()
        .w(24.0)
        .h(16.0)
        .paint(|ctx, _| {
            ctx.clip_rect(0.0, 0.0, 6.0, 16.0);
            ctx.set_fill("#0000ff");
            ctx.fill_rect(0.0, 0.0, 24.0, 16.0);
            LayoutStatus::Clean
        })
        .into();
    let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 24.0, 16.0).expect("raster");
    let inside = sample_px(&buf, w, 2, 8);
    assert!(inside[2] > 0.8, "inside clip, {inside:?}");
    let outside = sample_px(&buf, w, 16, 8);
    assert!(outside[3] < 0.05, "outside clip empty, {outside:?}");
}

#[test]
fn stroke_path_uses_path_command_kernel() {
    let el: Element = canvas()
        .w(40.0)
        .h(20.0)
        .paint(|ctx, _| {
            ctx.set_stroke("#ff00ff");
            ctx.set_line_width(3.0);
            ctx.stroke_path(&polyline(&[(2.0, 10.0), (38.0, 10.0)]));
            LayoutStatus::Clean
        })
        .into();
    let (buf, w, _) = rasterize_canvas_premul_for_test(&el.attrs, 40.0, 20.0).expect("raster");
    let mid = sample_px(&buf, w, 20, 10);
    assert!(mid[0] > 0.4 && mid[2] > 0.4, "stroke coverage, {mid:?}");
}

#[test]
fn leaf_has_no_taffy_children() {
    let el: Element = canvas()
        .w(40.0)
        .h(20.0)
        .paint(|ctx, _| {
            ctx.clear("secondary");
            LayoutStatus::Clean
        })
        .child(Element::new().text("nope"))
        .into();
    assert!(el.children.is_empty());
    assert!(el.get_attr(ATTR_CANVAS_PAINT).is_some());
}

#[test]
fn clean_paint_is_cached_until_dirty() {
    use std::cell::Cell;
    use std::rc::Rc;
    let runs = Rc::new(Cell::new(0u32));
    let el: Element = canvas()
        .w(24.0)
        .h(16.0)
        .paint({
            let runs = Rc::clone(&runs);
            move |ctx, _| {
                runs.set(runs.get() + 1);
                ctx.clear("secondary");
                LayoutStatus::Clean
            }
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("first");
    canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("cached");
    assert_eq!(runs.get(), 1, "Clean paint must not rerun on every collect");
    mark_canvas_dirty_hex(el.get_attr(ATTR_CANVAS_PAINT).unwrap());
    canvas_raster_from_element_attrs(&el.attrs, 24.0, 16.0).expect("dirty");
    assert_eq!(runs.get(), 2, "mark_canvas_dirty_hex must force a repaint");
}

#[test]
fn mark_canvas_dirty_sets_redraw_flag() {
    let _ = take_canvas_wants_redraw();
    let el: Element = canvas()
        .w(8.0)
        .h(8.0)
        .paint(|ctx, _| {
            ctx.clear("secondary");
            LayoutStatus::Clean
        })
        .into();
    let hex = el.get_attr(ATTR_CANVAS_PAINT).expect("paint id");
    let _ = take_canvas_wants_redraw();
    mark_canvas_dirty_hex(hex);
    assert!(
        take_canvas_wants_redraw(),
        "pointer dirty must wake a frame; CursorMoved does not redraw by default"
    );
    assert!(!take_canvas_wants_redraw(), "flag is edge-triggered");
}

#[test]
fn request_frame_keeps_dirty_and_wakes_redraw() {
    use std::cell::Cell;
    use std::rc::Rc;
    let runs = Rc::new(Cell::new(0u32));
    let _ = take_canvas_wants_redraw();
    let el: Element = canvas()
        .w(8.0)
        .h(8.0)
        .paint({
            let runs = Rc::clone(&runs);
            move |ctx, _| {
                runs.set(runs.get() + 1);
                ctx.request_frame();
                LayoutStatus::Clean
            }
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("first");
    assert!(
        take_canvas_wants_redraw(),
        "request_frame must wake the window, not only keep PAINT_DIRTY"
    );
    canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("second");
    assert_eq!(
        runs.get(),
        2,
        "request_frame must keep the leaf dirty so the next collect repaints"
    );
    assert!(
        take_canvas_wants_redraw(),
        "the window event loop must take this *after* collect, not before"
    );
}

#[test]
fn canvas_gpu_queues_path_job_not_cpu_mesh() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    use crate::primitives::path::take_path_jobs;
    let _ = take_path_jobs();
    let el: Element = canvas()
        .w(40.0)
        .h(20.0)
        .paint(|ctx, _| {
            ctx.clear("black");
            ctx.set_stroke("white");
            ctx.set_line_width(3.0);
            ctx.stroke_path(&polyline(&[(2.0, 10.0), (38.0, 10.0)]));
            LayoutStatus::Clean
        })
        .into();
    let (key, _, _) = canvas_raster_from_element_attrs(&el.attrs, 40.0, 20.0).expect("gpu canvas");
    assert!(key.starts_with("jadix_canvas:"));
    let jobs = take_path_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].key, key);
    assert!(jobs[0].force_redraw);
    assert_eq!(jobs[0].width, 40);
    assert_eq!(jobs[0].height, 20);
    let ops = jobs[0].ops.clone().into_ops();
    let mesh_n = ops
        .iter()
        .map(|op| match op {
            PathGpuOp::Mesh(m) => m.tris.len(),
            PathGpuOp::Fill(_) => 0,
        })
        .sum::<usize>();
    assert!(mesh_n >= 2, "stroke must tessellate, got {mesh_n}");
    let has_clear = ops.iter().any(|op| matches!(op, PathGpuOp::Fill(_)));
    assert!(has_clear, "clear() must queue a fill of the leaf");
}

#[test]
fn canvas_gpu_clip_is_scissor_on_fill() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    use crate::primitives::path::take_path_jobs;
    let _ = take_path_jobs();
    let el: Element = canvas()
        .w(40.0)
        .h(20.0)
        .paint(|ctx, size| {
            ctx.clear("black");
            ctx.clip_rect(size.w * 0.5, 0.0, size.w * 0.5, size.h);
            ctx.set_fill("white");
            ctx.fill_rect(0.0, 0.0, size.w, size.h);
            LayoutStatus::Clean
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 40.0, 20.0).expect("gpu canvas");
    let jobs = take_path_jobs();
    let ops = jobs[0].ops.clone().into_ops();
    let clipped = ops.iter().find_map(|op| match op {
        PathGpuOp::Fill(f) if f.clip.is_some() => f.clip,
        _ => None,
    });
    let clip = clipped.expect("clipped fill must carry scissor");
    assert!(
        clip[0] >= 18.0 && clip[2] >= 18.0,
        "right-half clip, got {clip:?}"
    );
}

#[test]
fn canvas_gpu_request_frame_requeues_same_key() {
    if !crate::renderer::gpu2d_enabled() {
        return;
    }
    use crate::primitives::path::take_path_jobs;
    let _ = take_path_jobs();
    let el: Element = canvas()
        .w(8.0)
        .h(8.0)
        .paint(|ctx, _| {
            ctx.request_frame();
            LayoutStatus::Clean
        })
        .into();
    let (k0, _, _) = canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("first");
    let j0 = take_path_jobs();
    assert_eq!(j0.len(), 1);
    assert!(j0[0].force_redraw);
    let (k1, _, _) = canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("second");
    let j1 = take_path_jobs();
    assert_eq!(k0, k1, "canvas key is id-stable");
    assert_eq!(
        j1.len(),
        1,
        "request_frame must requeue, not skip as hash reuse"
    );
    assert!(j1[0].force_redraw);
}

#[test]
fn get_image_data_samples_recorded_ops_on_gpu_canvas() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let px = Rc::new(RefCell::new(None::<Vec<u8>>));
    let el: Element = canvas()
        .w(8.0)
        .h(8.0)
        .paint({
            let px = Rc::clone(&px);
            move |ctx, _| {
                ctx.clear("#ff0000");
                *px.borrow_mut() = ctx.get_image_data(2, 2, 1, 1);
                LayoutStatus::Clean
            }
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 8.0, 8.0).expect("paint");
    let got = px
        .borrow()
        .clone()
        .expect("get_image_data must return pixels");
    assert!(
        got[0] > 200 && got[3] > 200,
        "clear red should be readable, got {got:?}"
    );
    if crate::renderer::gpu2d_enabled() {
        let key = el
            .get_attr(ATTR_CANVAS_PAINT)
            .map(|hex| {
                format!(
                    "jadix_canvas:{:016x}",
                    u64::from_str_radix(hex, 16).unwrap()
                )
            })
            .expect("id");
        assert!(
            crate::primitives::path::path_rt_readback_is_pending(&key),
            "get_image_data must request GPU RT copy after flush"
        );
    }
}

#[test]
fn is_point_in_path_follows_ctm() {
    use std::cell::Cell;
    use std::rc::Rc;
    let inside = Rc::new(Cell::new(false));
    let outside = Rc::new(Cell::new(true));
    let el: Element = canvas()
        .w(32.0)
        .h(16.0)
        .paint({
            let inside = Rc::clone(&inside);
            let outside = Rc::clone(&outside);
            move |ctx, _| {
                ctx.translate(10.0, 0.0);
                let p = polygon(&[(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)]);
                inside.set(ctx.is_point_in_path(&p, 14.0, 4.0));
                outside.set(ctx.is_point_in_path(&p, 4.0, 4.0));
                LayoutStatus::Clean
            }
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 32.0, 16.0).expect("paint");
    assert!(inside.get(), "translated square should contain (14,4)");
    assert!(!outside.get(), "unshifted (4,4) must miss");
}

#[test]
fn is_point_in_stroke_follows_ctm() {
    use std::cell::Cell;
    use std::rc::Rc;
    let hit = Rc::new(Cell::new(false));
    let miss = Rc::new(Cell::new(true));
    let el: Element = canvas()
        .w(48.0)
        .h(24.0)
        .paint({
            let hit = Rc::clone(&hit);
            let miss = Rc::clone(&miss);
            move |ctx, _| {
                ctx.set_line_width(8.0);
                ctx.translate(10.0, 0.0);
                let p = crate::primitives::path::polyline(&[(0.0, 8.0), (20.0, 8.0)]);
                hit.set(ctx.is_point_in_stroke(&p, 20.0, 8.0));
                miss.set(ctx.is_point_in_stroke(&p, 4.0, 8.0));
                LayoutStatus::Clean
            }
        })
        .into();
    canvas_raster_from_element_attrs(&el.attrs, 48.0, 24.0).expect("paint");
    assert!(hit.get(), "translated stroke should contain (20,8)");
    assert!(!miss.get(), "unshifted (4,8) must miss");
}
