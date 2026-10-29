use super::*;

#[test]
fn moving_single_carousel_keeps_slide_width_and_text() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::sync_reactive_elements_subtree;

    fn content(el: &Element) -> &Element {
        crate::core::tree::find_by_attr(el, ATTR_SLOT, SLOT_CONTENT).expect("carousel viewport")
    }

    create_root(|| {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        crate::style::utility::set_frame_dt(0.016);
        let id = "moving-single-regression";
        let mut builder = carousel_single(id).autoplay_ms(4000);
        for label in ["Slide A", "Slide B", "Slide C"] {
            builder = builder.slide(move || {
                r#box()
                    .w("100%")
                    .h(128.0)
                    .p(16.0)
                    .class("rounded-md")
                    .bg([0.9, 0.9, 0.9, 1.0])
                    .items("center")
                    .justify("center")
                    .child(Element::new().text(label))
                    .into()
            });
        }
        let mut root: Element = r#box().w("100%").child(builder).into();
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 600.0, 240.0, [-1.0, -1.0]);
        tree.set_collect_skip_allowed(true);
        for frame in 0..120 {
            if frame == 1 {
                tick_carousel_autoplay();
            }
            if frame == 10 || frame == 50 || frame == 90 {
                AUTOPLAY_AT.with(|at| {
                    at.borrow_mut()
                        .insert(id.into(), Instant::now() - Duration::from_millis(4000));
                });
                assert!(
                    tick_carousel_autoplay(),
                    "frame {frame}: autoplay must advance"
                );
            }
            if frame == 100 {
                let vp = content(&root).layout_bounds;
                let x = vp[0] + vp[2] * 0.75;
                let y = vp[1] + vp[3] * 0.5;
                let path = crate::app::pointer_events::hit_path(&root, x, y);
                crate::app::pointer_events::dispatch_pointer(
                    &path,
                    &mut PointerEvent::new(x, y, winit::event::MouseButton::Left, 0),
                    crate::app::pointer_events::PointerKind::Down,
                );
            }
            if (101..110).contains(&frame) {
                let vp = content(&root).layout_bounds;
                tick_carousel_drag(
                    vp[0] + vp[2] * (0.75 - (frame - 100) as f32 * 0.07),
                    vp[1] + vp[3] * 0.5,
                );
            }
            if frame == 110 {
                clear_carousel_drag_if_any();
            }
            crate::style::advance_css_transition_timeline_ms(16.0);
            begin_carousel_paint_frame();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                600.0,
                240.0,
                [0.0, 0.0],
            );
            crate::style::sync_class_variants_subtree(
                &mut root,
                [-1.0, -1.0],
                "",
                false,
                600.0,
                240.0,
                &mut tree,
                &mut impact,
            );
            let scroll_only = !matches!(
                impact,
                LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
            );
            let result = tree.build_and_layout_fast(
                &mut root,
                600.0,
                240.0,
                [-1.0, -1.0],
                scroll_only,
                false,
            );
            let vp = content(&root);
            let track = &vp.children[0];
            for item in &track.children {
                assert!(
                    (item.layout_bounds[2] - vp.layout_bounds[2]).abs() < 1.0,
                    "frame {frame}: slide width {} differs from viewport {}",
                    item.layout_bounds[2],
                    vp.layout_bounds[2]
                );
            }
            assert!(
                result
                    .text_nodes
                    .iter()
                    .any(|text| text.text.starts_with("Slide ")),
                "frame {frame}: carousel lost all slide text at offset {}",
                track.style.translate[0]
            );
        }
        crate::style::set_css_transition_virtual_timeline(false);
    });
}

#[test]
fn orientation_parse() {
    assert_eq!(
        CarouselOrientation::parse("vertical"),
        CarouselOrientation::Vertical
    );
    assert_eq!(
        CarouselOrientation::parse("horizontal"),
        CarouselOrientation::Horizontal
    );
}

#[test]
fn kind_and_end_align_parse() {
    assert_eq!(CarouselVariant::parse("multi"), CarouselVariant::Multi);
    assert_eq!(CarouselVariant::parse("stack"), CarouselVariant::Stack);
    assert!(!CarouselVariant::Multi.default_dots());
    assert!(CarouselVariant::Single.default_dots());
    assert_eq!(CarouselEndAlign::parse("pad"), CarouselEndAlign::Pad);
    assert_eq!(CarouselEndAlign::parse("flush"), CarouselEndAlign::Flush);
    assert_eq!(CarouselPageSnap::parse("peek"), CarouselPageSnap::Peek);
    assert_eq!(
        CarouselPageSnap::parse("viewport"),
        CarouselPageSnap::Viewport
    );
    assert_eq!(
        CarouselItemSizing::parse("fixed"),
        CarouselItemSizing::Fixed
    );
    assert_eq!(
        CarouselItemSizing::parse("fraction"),
        CarouselItemSizing::Fraction
    );
}

#[test]
fn controls_and_scroll_by_parse() {
    assert_eq!(CarouselControls::parse("inside"), CarouselControls::Inside);
    assert_eq!(
        CarouselControls::parse("outside"),
        CarouselControls::Outside
    );
    assert_eq!(CarouselScrollBy::parse("page"), CarouselScrollBy::Page);
    assert_eq!(CarouselScrollBy::parse("slide"), CarouselScrollBy::Slide);
}

#[test]
fn builder_builds() {
    let el: Element = carousel("t-carousel")
        .per_view(2.0)
        .autoplay_ms(0)
        .controls(CarouselControls::Outside)
        .scroll_by(CarouselScrollBy::Page)
        .variant(CarouselVariant::Multi)
        .end_align(CarouselEndAlign::Pad)
        .slide(|| r#box().w("100%").h("4rem").into())
        .slide(|| r#box().w("100%").h("4rem").into())
        .into();
    assert_ne!(el.template_id.as_deref(), Some(""));
    assert_eq!(
        el.attrs.get("data-variant").map(|s| s.as_str()),
        Some("multi")
    );
    assert_eq!(
        el.attrs.get("data-end-align").map(|s| s.as_str()),
        Some("pad")
    );
    assert_eq!(el.attrs.get("data-dots").map(|s| s.as_str()), Some("0"));
    assert_eq!(el.attrs.get("data-seamless").map(|s| s.as_str()), Some("1"));
    assert_eq!(el.attrs.get("data-snap").map(|s| s.as_str()), Some("1"));
}

#[test]
fn multi_loop_false_disables_wrap_and_clones() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let el: Element = carousel_multi("t-car-noloop")
            .loop_slides(false)
            .seamless(true) // Should be forced off by loop=false
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        assert_eq!(el.attrs.get("data-seamless").map(|s| s.as_str()), Some("0"));
        let ctx = CarouselCtx {
            id: "t-car-noloop".into(),
            index: use_signal("t-car-noloop__idx", || 0usize),
            count: use_signal("t-car-noloop__count", || 3usize),
            orientation: CarouselOrientation::Horizontal,
            loop_slides: false,
            seamless: false,
            snap: true,
            per_view: 2.2,
            autoplay_ms: 0,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Slide,
            variant: CarouselVariant::Multi,
            end_align: CarouselEndAlign::Flush,
            page_snap: CarouselPageSnap::Peek,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px: use_signal("t-car-noloop__drag", || 0.0f32),
            viewport_len: use_signal("t-car-noloop__vlen", || 400.0f32),
            paused: use_signal("t-car-noloop__paused", || false),
        };
        assert!(!wraps_nav(&ctx));
        assert!(!track_loops(&ctx));
        assert_eq!(loop_clone_count(&ctx), 0);
        assert!(!can_scroll_prev(&ctx));
        assert!(can_scroll_next(&ctx));
    });
}

#[test]
fn multi_fraction_index0_left_aligns_first_slide() {
    use crate::core::reactivity::create_root;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = format!("t-car-frac-{}", N.fetch_add(1, Ordering::Relaxed));
    let id_off = id.clone();
    create_root(move || {
        let carousel_el: Element = carousel_multi(id)
            .loop_slides(true)
            .seamless(true)
            .per_view(2.2)
            .controls(CarouselControls::Outside)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        let mut root: Element = r#box().w("100%").child(carousel_el).into();
        let mut layout = LayoutTree::new();
        for _ in 0..4 {
            let _ = layout.build_and_layout(&mut root, 600.0, 400.0, [-1.0, -1.0]);
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                600.0,
                400.0,
                [0.0, 0.0],
            );
        }
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let vp = content.layout_bounds[2];
        assert!(vp > 40.0, "viewport should be measurable, got {vp}");
        let track = content.children.first().expect("track");
        let offset = track.style.translate[0];
        let step = vp / 2.2;
        // per_view=2.2 → need_view=3 → cc=min(3,3)=3; index0 stops at -cc*step.
        let cc = 3.0f32;
        assert!(
                (offset + cc * step).abs() < 3.0,
                "index0 must park at -cc*step for left-aligned first slide; offset={offset} step={step} vp={vp}"
            );
        let tgt = offset_target(&id_off).peek();
        assert!(
            (tgt - offset).abs() < 1.0 || track.transition_snapshot.has_active_tweens(),
            "translate should match offset target; translate={offset} target={tgt}"
        );
        // First real item (skip prefix clone) left edge ≈ viewport left edge.
        let items: Vec<_> = track
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM))
            .collect();
        assert!(
            items.len() >= 6,
            "expected prefix clones + real slides, got {}",
            items.len()
        );
        let first_real = items[cc as usize];
        let left_delta = (first_real.layout_bounds[0] + offset) - content.layout_bounds[0];
        // layout_bounds may or may not include transform; both methods accept small errors.
        assert!(
                left_delta.abs() < 4.0
                    || (first_real.layout_bounds[0] - content.layout_bounds[0]).abs() < 4.0,
                "first real slide should be left-aligned; left_delta={left_delta} item_x={} content_x={} offset={offset}",
                first_real.layout_bounds[0],
                content.layout_bounds[0]
            );
    });
}

#[test]
fn snap_false_parks_at_visual_not_grid() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-free-drag";
        let index = use_signal("t-car-free-drag__idx", || 0usize);
        let count = use_signal("t-car-free-drag__count", || 5usize);
        let drag_px = use_signal("t-car-free-drag__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-free-drag__vlen", || 400.0f32);
        let paused = use_signal("t-car-free-drag__paused", || false);
        // Starting offset=0, drag -70px → visual -70; step size ≈181.8, if adsorbed, it will reach 0 or -181.8.
        snap_offset_to(id, 0.0);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: 0.0,
            index: index.clone(),
            count: count.clone(),
            per_view: 2.2,
            wraps: false,
            loop_clones: 0,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Multi,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: false,
            viewport_len: viewport_len.clone(),
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        drag_px.set(-70.0);
        end_carousel_drag();
        let parked = offset_target(id).peek();
        assert!(
            (parked + 70.0).abs() < 1.0,
            "snap=false should park at visual offset, got {parked}"
        );
        let step = 400.0 / 2.2;
        assert!(
            (parked % step).abs() > 1.0 && (parked % step + step).abs() > 1.0,
            "parked offset must not land on an integer grid; parked={parked} step={step}"
        );
        // Regression: When index→nearest is released after letting go, the track automatically syncs and must not pull the free offset back to the grid point.
        let ctx = CarouselCtx {
            id: id.into(),
            index,
            count,
            orientation: CarouselOrientation::Horizontal,
            loop_slides: false,
            seamless: false,
            snap: false,
            per_view: 2.2,
            autoplay_ms: 0,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Slide,
            variant: CarouselVariant::Multi,
            end_align: CarouselEndAlign::Flush,
            page_snap: CarouselPageSnap::Peek,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px,
            viewport_len,
            paused,
        };
        auto_sync_offset_if_snapping(&ctx);
        assert!(
            (offset_target(id).peek() + 70.0).abs() < 1.0,
            "auto sync must not re-snap free park; got {}",
            offset_target(id).peek()
        );
    });
}

#[test]
fn wrap_loop_offset_folds_clone_band() {
    let step = 100.0;
    let n = 5usize;
    let cc = 2usize;
    let start = -(cc as f32) * step; // -200
    let end = start - (n as f32) * step; // -700
    assert!((wrap_loop_offset(start, n, cc, step) - start).abs() < 0.01);
    // Entering prefix: corrected than start → fold to the end
    let into_prefix = start + 40.0;
    let w = wrap_loop_offset(into_prefix, n, cc, step);
    assert!((w - (into_prefix - n as f32 * step)).abs() < 0.01);
    assert!(w < start && w > end);
    // Enter suffix: ≤ end → fold to the first paragraph
    let into_suffix = end - 30.0;
    let w2 = wrap_loop_offset(into_suffix, n, cc, step);
    assert!((w2 - (into_suffix + n as f32 * step)).abs() < 0.01);
    assert!(w2 <= start && w2 > end);
}

#[test]
fn seamless_free_drag_end_wraps_out_of_clone_zone() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-seamless-free";
        let index = use_signal("t-car-seamless-free__idx", || 0usize);
        let count = use_signal("t-car-seamless-free__count", || 5usize);
        let drag_px = use_signal("t-car-seamless-free__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-seamless-free__vlen", || 440.0f32);
        let step = 440.0 / 2.2;
        let cc = 3usize;
        let start = -(cc as f32) * step;
        // Drag from index0 into the prefix clone area
        snap_offset_to(id, start);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: start,
            index: index.clone(),
            count: count.clone(),
            per_view: 2.2,
            wraps: true,
            loop_clones: cc,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Multi,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: false,
            viewport_len: viewport_len.clone(),
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        drag_px.set(55.0); // visual = start+55 → prefix
        end_carousel_drag();
        let parked = offset_target(id).peek();
        assert!(
            parked <= start + 0.01,
            "must wrap out of prefix; parked={parked} start={start}"
        );
        let period = 5.0 * step;
        assert!(
            (parked - (start + 55.0 - period)).abs() < 1.0,
            "expected start+55-period; parked={parked}"
        );
    });
}

#[test]
fn page_pad_peek_carries_cropped_item_to_next_page_start() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-page-peek";
        let index = use_signal("t-car-page-peek__idx", || 0usize);
        let count = use_signal("t-car-page-peek__count", || 5usize);
        let drag_px = use_signal("t-car-page-peek__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-page-peek__vlen", || 440.0f32);
        let paused = use_signal("t-car-page-peek__paused", || false);
        viewport_len.set(440.0);
        snap_offset_to(id, 0.0);
        let ctx = CarouselCtx {
            id: id.into(),
            index: index.clone(),
            count: count.clone(),
            orientation: CarouselOrientation::Horizontal,
            loop_slides: false,
            seamless: false,
            snap: true,
            per_view: 2.2,
            autoplay_ms: 0,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Page,
            variant: CarouselVariant::Multi,
            end_align: CarouselEndAlign::Pad,
            page_snap: CarouselPageSnap::Peek,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px: drag_px.clone(),
            viewport_len: viewport_len.clone(),
            paused: paused.clone(),
        };
        let vp = 440.0f32;
        let step = vp / 2.2;
        // perView=2.2: Crop the last item on the first screen index=2; after peeking, the item becomes the first item on the next page → offset=-2*step.
        scroll_next(&ctx);
        let after_btn = offset_target(id).peek();
        assert!(
            (after_btn + 2.0 * step).abs() < 1.0,
            "peek page next must start at cropped item; got {after_btn}, want {}",
            -2.0 * step
        );
        assert!(
            (after_btn + vp).abs() > 10.0,
            "peek must not advance a full viewport; got {after_btn}"
        );

        snap_offset_to(id, 0.0);
        index.set(0);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: 0.0,
            index: index.clone(),
            count: count.clone(),
            per_view: 2.2,
            wraps: false,
            loop_clones: 0,
            end_align: CarouselEndAlign::Pad,
            variant: CarouselVariant::Multi,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Page,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len: viewport_len.clone(),
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        // Only follow the next page after half a page (page=2*step); no longer use "small displacements are also forced to ±1".
        let page = 2.0 * step;
        drag_px.set(-(page * 0.55));
        end_carousel_drag();
        let after_drag = offset_target(id).peek();
        assert!(
            (after_drag + 2.0 * step).abs() < 1.0,
            "peek drag snap must park at -2*step; got {after_drag}"
        );
    });
}

#[test]
fn page_navigation_and_drag_keep_partial_final_page_flush() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-page-vp";
        let index = use_signal("t-car-page-vp__idx", || 0usize);
        let count = use_signal("t-car-page-vp__count", || 5usize);
        let drag_px = use_signal("t-car-page-vp__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-page-vp__vlen", || 440.0f32);
        let paused = use_signal("t-car-page-vp__paused", || false);
        viewport_len.set(440.0);
        snap_offset_to(id, 0.0);
        let mut ctx = CarouselCtx {
            id: id.into(),
            index,
            count,
            orientation: CarouselOrientation::Horizontal,
            loop_slides: false,
            seamless: false,
            snap: true,
            per_view: 2.2,
            autoplay_ms: 0,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Page,
            variant: CarouselVariant::Multi,
            end_align: CarouselEndAlign::Flush,
            page_snap: CarouselPageSnap::Viewport,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px,
            viewport_len,
            paused,
        };
        let vp = 440.0f32;
        scroll_next(&ctx);
        let after = offset_target(id).peek();
        assert!(
            (after + vp).abs() < 1.0,
            "viewport pageSnap must move one screen; got {after}"
        );
        auto_sync_offset_if_snapping(&ctx);
        scroll_next(&ctx);
        auto_sync_offset_if_snapping(&ctx);
        let end = max_scroll_offset(&ctx);
        assert!(
            (offset_target(id).peek() - end).abs() < 0.01,
            "the partial final page must remain flush after frame sync"
        );
        assert!(!can_scroll_next(&ctx));
        scroll_prev(&ctx);
        auto_sync_offset_if_snapping(&ctx);
        assert!((offset_target(id).peek() + vp).abs() < 0.01);
        assert!(can_scroll_next(&ctx));

        for (n, per_view, viewport, item_width, page_snap, stops) in [
            (
                5,
                2.2,
                440.0,
                0.0,
                CarouselPageSnap::Viewport,
                [0.0, -440.0, -560.0],
            ),
            // Both last pages round to slide index 3, but must remain distinct.
            (
                6,
                2.8,
                560.0,
                0.0,
                CarouselPageSnap::Viewport,
                [0.0, -560.0, -640.0],
            ),
            (
                4,
                3.8,
                760.0,
                0.0,
                CarouselPageSnap::Viewport,
                [0.0, -40.0, -40.0],
            ),
            (
                5,
                2.0,
                400.0,
                0.0,
                CarouselPageSnap::Viewport,
                [0.0, -400.0, -600.0],
            ),
            (
                5,
                2.2,
                440.0,
                0.0,
                CarouselPageSnap::Peek,
                [0.0, -400.0, -560.0],
            ),
            (
                5,
                2.2,
                450.0,
                200.0,
                CarouselPageSnap::Viewport,
                [0.0, -400.0, -550.0],
            ),
        ] {
            ctx.count.set(n);
            ctx.per_view = per_view;
            ctx.viewport_len.set(viewport);
            ctx.item_sizing = if item_width > 0.0 {
                CarouselItemSizing::Fixed
            } else {
                CarouselItemSizing::Fraction
            };
            ctx.item_width_px = item_width;
            ctx.page_snap = page_snap;
            ctx.index.set(0);
            snap_offset_to(id, 0.0);
            for expected in stops.into_iter().skip(1) {
                scroll_next(&ctx);
                for _ in 0..3 {
                    auto_sync_offset_if_snapping(&ctx);
                }
                assert!(
                    (offset_target(id).peek() - expected).abs() < 0.01,
                    "next: n={n}, per_view={per_view}, expected={expected}"
                );
            }
            assert!(!can_scroll_next(&ctx));
            scroll_next(&ctx);
            auto_sync_offset_if_snapping(&ctx);
            assert!((offset_target(id).peek() - stops[2]).abs() < 0.01);

            let previous = if stops[1] == stops[2] { 0.0 } else { stops[1] };
            scroll_prev(&ctx);
            auto_sync_offset_if_snapping(&ctx);
            assert!(
                (offset_target(id).peek() - previous).abs() < 0.01,
                "previous from partial final page must visit the preceding page"
            );

            let drag_from = |base_offset| CarouselDrag {
                id: id.into(),
                start_ptr: 0.0,
                base_offset,
                index: ctx.index,
                count: ctx.count,
                per_view: ctx.per_view,
                wraps: false,
                loop_clones: 0,
                end_align: ctx.end_align,
                variant: ctx.variant,
                item_sizing: ctx.item_sizing,
                item_width_px: ctx.item_width_px,
                scroll_by: ctx.scroll_by,
                page_snap: ctx.page_snap,
                snap: true,
                viewport_len: ctx.viewport_len,
                drag_px: ctx.drag_px,
                orientation: ctx.orientation,
                autoplay_ms: 0,
            };
            // Drag just past the midpoint of the short final segment.
            begin_carousel_drag(drag_from(previous));
            ctx.drag_px.set((stops[2] - previous) * 0.6);
            end_carousel_drag();
            auto_sync_offset_if_snapping(&ctx);
            assert!((offset_target(id).peek() - stops[2]).abs() < 0.01);
            assert!(!can_scroll_next(&ctx));

            begin_carousel_drag(drag_from(stops[2]));
            ctx.drag_px.set(1.0);
            end_carousel_drag();
            auto_sync_offset_if_snapping(&ctx);
            assert!(
                (offset_target(id).peek() - stops[2]).abs() < 0.01,
                "a tiny drag must not leave the final page"
            );

            apply_viewport_len(&ctx, viewport * 1.2);
            auto_sync_offset_if_snapping(&ctx);
            assert!(
                (offset_target(id).peek() - max_scroll_offset(&ctx)).abs() < 0.01,
                "the final page must remain flush after resize"
            );
            assert!(!can_scroll_next(&ctx));
        }
        // Shrinking a previously non-overflowing fixed-width list keeps the first page.
        ctx.count.set(2);
        ctx.viewport_len.set(450.0);
        ctx.index.set(0);
        snap_offset_to(id, 0.0);
        apply_viewport_len(&ctx, 300.0);
        auto_sync_offset_if_snapping(&ctx);
        assert!(offset_target(id).peek().abs() < 0.01);
        assert!(can_scroll_next(&ctx));
    });
}

#[test]
fn seamless_snap_drag_wraps_like_free_and_parks_on_grid() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-seamless-snap-wrap";
        let index = use_signal("t-car-seamless-snap-wrap__idx", || 2usize);
        let count = use_signal("t-car-seamless-snap-wrap__count", || 3usize);
        let drag_px = use_signal("t-car-seamless-snap-wrap__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-seamless-snap-wrap__vlen", || 300.0f32);
        let step = 300.0f32;
        let cc = 1usize;
        let hi = 2usize;
        let n = 3usize;
        // stop at last item
        let base = -((hi + cc) as f32) * step;
        snap_offset_to(id, base);
        let _ = take_instant_snap_to(id);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: base,
            index: index.clone(),
            count: count.clone(),
            per_view: 1.0,
            wraps: true,
            loop_clones: cc,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Single,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len: viewport_len.clone(),
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        // Dragging past the last item into the "next" direction should wrap back to the real band in ticks (not clone/blank).
        assert!(
            crate::utils::drag::routing::prepare_move(None, 200.0 - (step + 50.0), 0.0).dispatch()
        );
        let mid = offset_target(id).peek();
        let start = -(cc as f32) * step;
        let end = start - (n as f32) * step;
        assert!(
            mid <= start + 0.01 && mid > end,
            "snap+seamless drag must stay in real band; mid={mid} start={start} end={end}"
        );
        crate::utils::drag::routing::finish_all();
        assert!(!is_carousel_dragging_id(id));
        assert!(
            loop_anim(id).is_none(),
            "drag wrap should not leave a pending clone loop anim"
        );
        let parked = offset_target(id).peek();
        // Adsorb to grid point: within the true band -(i+cc)*step
        let grid = (0..=hi)
            .map(|i| -((i + cc) as f32) * step)
            .any(|g| (parked - g).abs() < 1.0);
        assert!(
            grid,
            "after release must park on slide grid; parked={parked}"
        );
    });
}

#[test]
fn seamless_snap_past_last_uses_clone_loop_not_long_scroll() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-seamless-snap-boundary";
        let index = use_signal("t-car-seamless-snap-boundary__idx", || 2usize);
        let count = use_signal("t-car-seamless-snap-boundary__count", || 3usize);
        let drag_px = use_signal("t-car-seamless-snap-boundary__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-seamless-snap-boundary__vlen", || 300.0f32);
        let step = 300.0f32;
        let cc = 1usize;
        let hi = 2usize;
        let n = 3usize;
        let last_off = -((hi + cc) as f32) * step; // -900
        let first_off = -(cc as f32) * step; // -300
        let clone_first = -((n + cc) as f32) * step; // -1200
        snap_offset_to(id, last_off);
        let _ = take_instant_snap_to(id);
        index.set(hi);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: last_off,
            index: index.clone(),
            count,
            per_view: 1.0,
            wraps: true,
            loop_clones: cc,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Single,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len,
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        // The tail item is dragged out more than halfway and enters the suffix cloning process (the full wrap has not yet been triggered).
        drag_px.set(-(step * 0.6));
        end_carousel_drag();
        assert_eq!(index.peek(), 0, "dots should already show first");
        let anim = loop_anim(id).expect("must use seamless LoopAnim, not long reverse tween");
        assert!(
            (anim.target_offset - clone_first).abs() < 1.0,
            "animate into first's clone; got {}",
            anim.target_offset
        );
        assert_eq!(anim.settle_index, 0);
        assert!((anim.settle_offset - first_off).abs() < 1.0);
        assert!(
            (offset_target(id).peek() - clone_first).abs() < 1.0,
            "target must be clone slot, not real first ({first_off}); got {}",
            offset_target(id).peek()
        );
        // After reaching the clone slot, settle → the real first item
        finish_loop_anim(id, index.clone());
        assert!((offset_target(id).peek() - first_off).abs() < 1.0);
        assert_eq!(index.peek(), 0);
    });
}

#[test]
fn snap_release_follows_visual_when_next_mostly_visible() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-snap-visual";
        let index = use_signal("t-car-snap-visual__idx", || 0usize);
        let count = use_signal("t-car-snap-visual__count", || 3usize);
        let drag_px = use_signal("t-car-snap-visual__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-snap-visual__vlen", || 300.0f32);
        let step = 300.0f32;
        let cc = 1usize;
        // The dragging is already halfway (simulating grabbing in the middle of the animation), and then the dragging is until the next item is almost full.
        let start_ideal = -(cc as f32) * step;
        let base = start_ideal - step * 0.55;
        let extra = -step * 0.40; // Total about 0.95 cells → nearest=1
        snap_offset_to(id, base);
        let _ = take_instant_snap_to(id);
        index.set(0);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 200.0,
            base_offset: base,
            index: index.clone(),
            count,
            per_view: 1.0,
            wraps: true,
            loop_clones: cc,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Single,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len,
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        // This time the gesture displacement is small (below the old threshold), but the vision is halfway through and close to the posterior term.
        drag_px.set(extra);
        assert!(extra.abs() > DRAG_THRESHOLD_PX || (base - start_ideal).abs() > step * 0.5);
        end_carousel_drag();
        assert_eq!(
            index.peek(),
            1,
            "must snap to mostly-visible next slide, not stick on start"
        );
        let want = -((1 + cc) as f32) * step;
        assert!(
            (offset_target(id).peek() - want).abs() < 1.0,
            "offset should park on slide 1; got {}",
            offset_target(id).peek()
        );
    });
}

#[test]
fn drag_clears_autoplay_deadline_until_release() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-ap-drag";
        let index = use_signal("t-car-ap-drag__idx", || 0usize);
        let count = use_signal("t-car-ap-drag__count", || 3usize);
        let drag_px = use_signal("t-car-ap-drag__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-ap-drag__vlen", || 300.0f32);
        let paused = use_signal("t-car-ap-drag__paused", || false);
        // The timer is about to expire before simulating drag.
        set_next_autoplay_at(id, Some(Instant::now() + Duration::from_millis(50)));
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 100.0,
            base_offset: 0.0,
            index,
            count,
            per_view: 1.0,
            wraps: true,
            loop_clones: 1,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Single,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len,
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 4000,
        });
        assert!(is_carousel_dragging_id(id), "drag session must be active");
        assert!(
            next_autoplay_at(id).is_none(),
            "drag must clear autoplay deadline"
        );
        drag_px.set(-10.0);
        end_carousel_drag();
        assert!(!is_carousel_dragging_id(id));
        let at = next_autoplay_at(id).expect("deadline restarted after release");
        let wait = at.saturating_duration_since(Instant::now());
        assert!(
            wait >= Duration::from_millis(3500),
            "must restart full interval, not fire stale timer; wait={wait:?}"
        );
        assert!(!paused.peek(), "drag must not leave hover-paused stuck");
    });
}

#[test]
fn first_viewport_measure_does_not_flag_layout_resize() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-vlen-first";
        let viewport_len = use_signal("t-car-vlen-first__vlen", || 0.0f32);
        let ctx = CarouselCtx {
            id: id.into(),
            index: use_signal("t-car-vlen-first__idx", || 0usize),
            count: use_signal("t-car-vlen-first__count", || 3usize),
            orientation: CarouselOrientation::Horizontal,
            loop_slides: true,
            seamless: false,
            snap: true,
            per_view: 1.0,
            autoplay_ms: 0,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Slide,
            variant: CarouselVariant::Single,
            end_align: CarouselEndAlign::Flush,
            page_snap: CarouselPageSnap::Peek,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px: use_signal("t-car-vlen-first__drag", || 0.0f32),
            viewport_len,
            paused: use_signal("t-car-vlen-first__paused", || false),
        };
        assert!(
            !apply_viewport_len(&ctx, 400.0),
            "first measure (0→width) is not a Taffy resize"
        );
        assert!((ctx.viewport_len.peek() - 400.0).abs() < 0.1);
        assert!(
            apply_viewport_len(&ctx, 520.0),
            "real resize still reports applied"
        );
    });
}

#[test]
fn carousel_sync_first_measure_is_not_layout_changed() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let mut root: Element = carousel_single("t-car-sync-vlen")
            .autoplay_ms(4000)
            .loop_slides(true)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 600.0, 400.0, [-1.0, -1.0]);
        let mut impact = LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut layout,
            &mut impact,
            600.0,
            400.0,
            [0.0, 0.0],
        );
        assert_ne!(
            impact,
            LayoutStatus::LayoutChanged,
            "first viewport measure must not FullClear the page (scrollbar flash)"
        );
        unregister_autoplay("t-car-sync-vlen");
    });
}

#[test]
fn autoplay_does_not_advance_offscreen_or_on_enter() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-ap-vis";
        let index = use_signal("t-car-ap-vis__idx", || 0usize);
        let count = use_signal("t-car-ap-vis__count", || 3usize);
        count.set(3);
        let ctx = CarouselCtx {
            id: id.into(),
            index: index.clone(),
            count,
            orientation: CarouselOrientation::Horizontal,
            loop_slides: true,
            seamless: false,
            snap: true,
            per_view: 1.0,
            autoplay_ms: 4000,
            controls: CarouselControls::Outside,
            scroll_by: CarouselScrollBy::Slide,
            variant: CarouselVariant::Single,
            end_align: CarouselEndAlign::Flush,
            page_snap: CarouselPageSnap::Peek,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            show_dots: false,
            stack_sides: 1,
            drag_px: use_signal("t-car-ap-vis__drag", || 0.0f32),
            viewport_len: use_signal("t-car-ap-vis__vlen", || 300.0f32),
            paused: use_signal("t-car-ap-vis__paused", || false),
        };
        register_autoplay(&ctx);

        begin_carousel_paint_frame();
        stamp_carousel_painted(id);
        tick_carousel_autoplay();
        assert_eq!(index.peek(), 0, "first on-screen tick restarts clock");
        assert!(carousel_next_wake_at().is_some());

        set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
        begin_carousel_paint_frame();
        tick_carousel_autoplay();
        assert_eq!(index.peek(), 0, "offscreen must not Advance");
        assert!(carousel_next_wake_at().is_none());

        set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
        begin_carousel_paint_frame();
        stamp_carousel_painted(id);
        tick_carousel_autoplay();
        assert_eq!(index.peek(), 0, "enter must not catch-up Advance");

        set_next_autoplay_at(id, Some(Instant::now() - Duration::from_millis(1)));
        begin_carousel_paint_frame();
        stamp_carousel_painted(id);
        tick_carousel_autoplay();
        assert_eq!(index.peek(), 1, "on-screen autoplay must still Advance");

        unregister_autoplay(id);
    });
}

#[test]
fn stack_coverflow_centers_active_and_offsets_sides() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let mut root: Element = carousel_stack("t-car-stack-geo")
            .controls(CarouselControls::Outside)
            .stack_sides(1)
            .loop_slides(true)
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .into();
        let mut layout = LayoutTree::new();
        for _ in 0..4 {
            let _ = layout.build_and_layout(&mut root, 700.0, 400.0, [-1.0, -1.0]);
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                700.0,
                400.0,
                [0.0, 0.0],
            );
        }
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let stage = content.layout_bounds;
        let stage_mid = stage[0] + stage[2] * 0.5;
        // The height is supported by the content (slide h=8rem), consistent with the Single viewport, and there should no longer be a fixed empty height.
        assert!(
            (stage[3] - 128.0).abs() < 24.0,
            "stack stage height should follow slide content (~8rem); got {}",
            stage[3]
        );
        let cards: Vec<_> = content
            .children
            .iter()
            .filter(|c| {
                c.attrs.get("data-carousel-stack-measure").is_none()
                    && c.style.position == taffy::style::Position::Absolute
            })
            .collect();
        assert_eq!(cards.len(), 5);

        let mut center = None;
        let mut left = None;
        let mut right = None;
        for card in cards {
            let w = card.layout_bounds[2];
            let render_x =
                card.layout_bounds[0] + card.style.translate[0] + w * card.style.translate_pct[0];
            let mid = render_x + w * 0.5;
            let shell_sc = card.style.scale[0];
            let child_sc = card
                .children
                .first()
                .map(|c| c.style.scale[0])
                .unwrap_or(1.0);
            assert!(
                card.style.shadow_color[3] < 0.01 && card.style.shadow_blur < 0.5,
                "transparent shell must not paint box-shadow (ghost frame)"
            );
            assert!(
                    (child_sc - 1.0).abs() < 0.01,
                    "content scale stays 1; shell carries coverflow scale via ancestor_scale / paint_scale"
                );
            // Card width should be a percentage of the viewport (same mode as Single basis%), not fixed px/rem.
            assert!(
                (w - stage[2] * 0.46).abs() < 8.0,
                "card width should be ~46% of stage; w={w} stage={}",
                stage[2]
            );
            if card.style.z_index >= 99_000 && card.style.opacity > 0.9 {
                center = Some((
                    mid,
                    w,
                    card.style.translate[0],
                    shell_sc,
                    card.style.opacity,
                ));
                assert!(
                    card.style.bg_color[3] < 0.05,
                    "center shell stays transparent; occlusion is page_stack_bands"
                );
            } else if card.style.opacity > 0.35 && card.style.translate[0] < -1.0 {
                left = Some((card.style.translate[0].abs(), shell_sc, card.style.opacity));
                assert!(
                    card.style.bg_color[3] < 0.05,
                    "side shell must stay transparent"
                );
            } else if card.style.opacity > 0.35 && card.style.translate[0] > 1.0 {
                right = Some((card.style.translate[0].abs(), shell_sc, card.style.opacity));
                assert!(
                    card.style.bg_color[3] < 0.05,
                    "side shell must stay transparent"
                );
            }
        }
        let (c_mid, c_w, c_tx, c_sc, c_op) = center.expect("active card");
        assert!(c_tx.abs() < 1.0, "active card must sit at tx=0, got {c_tx}");
        assert!(
            (c_mid - stage_mid).abs() < 4.0,
            "active card must be stage-centered; mid={c_mid} stage_mid={stage_mid}"
        );
        assert!(
            c_w < stage[2] * 0.55,
            "stack cards should be narrower than ~half stage; w={c_w} stage={}",
            stage[2]
        );
        assert!((c_sc - 1.0).abs() < 0.02 && (c_op - 1.0).abs() < 0.02);
        let (l_off, l_sc, l_op) = left.expect("left side card");
        let (r_off, r_sc, r_op) = right.expect("right side card");
        assert!(
                l_off >= c_w * 0.4 && r_off >= c_w * 0.4,
                "side offset must be ~half card width so sides peek clearly; left={l_off} right={r_off} card_w={c_w}"
            );
        assert!(
            l_sc < 0.9 && r_sc < 0.9 && l_op <= 0.73 && r_op <= 0.73,
            "sides must be scaled down and faded; L sc={l_sc} op={l_op}; R sc={r_sc} op={r_op}"
        );
    });
}

#[test]
fn stack_small_drag_keeps_previous_and_next_cards_visible() {
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerKind};
    use crate::core::reactivity::create_root;
    use crate::core::tree::{find_by_attr, sync_reactive_elements_subtree};

    for autoplay_ms in [0, 4000] {
        create_root(|| {
            let mut builder = carousel_stack(format!("stack-small-drag-{autoplay_ms}"))
                .autoplay_ms(autoplay_ms)
                .stack_sides(1)
                .loop_slides(true);
            for label in ["Stack A", "Stack B", "Stack C", "Stack D", "Stack E"] {
                builder = builder.slide(move || {
                    r#box()
                        .w("100%")
                        .h(128.0)
                        .child(Element::new().text(label))
                        .into()
                });
            }
            let mut root: Element = builder.into();
            let mut layout = LayoutTree::new();
            for _ in 0..4 {
                layout.build_and_layout(&mut root, 700.0, 400.0, [-1.0, -1.0]);
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut LayoutStatus::Clean,
                    700.0,
                    400.0,
                    [0.0, 0.0],
                );
            }
            let bounds = find_by_attr(&root, ATTR_SLOT, SLOT_CONTENT)
                .unwrap()
                .layout_bounds;
            let x = bounds[0] + bounds[2] * 0.5;
            let y = bounds[1] + bounds[3] * 0.5;
            let path = hit_path(&root, x, y);
            dispatch_pointer(
                &path,
                &mut PointerEvent::new(x, y, winit::event::MouseButton::Left, 0),
                PointerKind::Down,
            );
            let step = stack_step_px(bounds[2]);
            for fraction in [0.02, 0.15, 0.5, -0.02, -0.15, -0.5, 0.0] {
                tick_carousel_drag(x - fraction * step, y);
                sync_reactive_elements_subtree(
                    &mut root,
                    &mut layout,
                    &mut LayoutStatus::Clean,
                    700.0,
                    400.0,
                    [0.0, 0.0],
                );
                let content = find_by_attr(&root, ATTR_SLOT, SLOT_CONTENT).unwrap();
                let cards: Vec<_> = content
                    .children
                    .iter()
                    .filter(|c| c.attrs.get("data-carousel-stack-measure").is_none())
                    .collect();
                for idx in [1, 4] {
                    assert!(
                            cards[idx].style.opacity > 0.1,
                            "autoplay={autoplay_ms}, drag={fraction}: side card {idx} disappeared (opacity={})",
                            cards[idx].style.opacity,
                        );
                }
            }
            clear_carousel_drag_if_any();
        });
    }
}

#[test]
fn stack_step_keeps_scaled_side_cards_inside_narrow_viewport() {
    for w in [120.0_f32, 160.0, 200.0, 240.0, 400.0, 700.0] {
        let step = stack_step_px(w);
        let card_w = w * 0.46;
        let side_sc = 0.82;
        // The outer edge of the side card is relative to the left edge of the stage (centered at W/2 - step).
        let outer = w * 0.5 - step - card_w * side_sc * 0.5;
        assert!(
            outer >= -0.5,
            "narrow w={w}: step={step} must keep scaled side inside stage (outer={outer})"
        );
        // Regression: old clamp(min=56) would clip rounded corners when w≲234.
        if w < 230.0 {
            assert!(
                step < 56.0,
                "narrow w={w} must not force min step 56 (got {step})"
            );
        }
    }
    assert_eq!(stack_step_px(0.0), 0.0);
    assert_eq!(stack_step_px(4.0), 0.0);
}

#[test]
fn stack_drag_snap_advances_index_and_offset() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let id = "t-car-stack-drag";
        let index = use_signal("t-car-stack-drag__idx", || 0usize);
        let count = use_signal("t-car-stack-drag__count", || 5usize);
        let drag_px = use_signal("t-car-stack-drag__drag", || 0.0f32);
        let viewport_len = use_signal("t-car-stack-drag__vlen", || 700.0f32);
        let step = stack_step_px(700.0);
        begin_carousel_drag(CarouselDrag {
            id: id.into(),
            start_ptr: 0.0,
            base_offset: 0.0,
            index: index.clone(),
            count: count.clone(),
            per_view: 1.0,
            wraps: true,
            loop_clones: 0,
            end_align: CarouselEndAlign::Flush,
            variant: CarouselVariant::Stack,
            item_sizing: CarouselItemSizing::Fraction,
            item_width_px: 0.0,
            scroll_by: CarouselScrollBy::Slide,
            page_snap: CarouselPageSnap::Peek,
            snap: true,
            viewport_len: viewport_len.clone(),
            drag_px: drag_px.clone(),
            orientation: CarouselOrientation::Horizontal,
            autoplay_ms: 0,
        });
        // Drag half a frame → next picture.
        drag_px.set(-(step * 0.6));
        snap_offset_to(id, -(step * 0.6));
        end_carousel_drag();
        assert_eq!(
            index.peek(),
            1,
            "stack drag past half step should land on next"
        );
        assert!(
            (offset_target(id).peek() + step).abs() < 1.0,
            "stack ideal offset should be -index×step; got {}",
            offset_target(id).peek()
        );
    });
}

#[test]
fn snap_false_forced_true_when_autoplay() {
    use crate::core::reactivity::create_root;
    create_root(|| {
        let el: Element = carousel("t-car-snap-ap")
            .snap(false)
            .autoplay_ms(3000)
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        assert_eq!(el.attrs.get("data-snap").map(|s| s.as_str()), Some("1"));
    });
}

/// Return: When narrow screen + controls=outside, the first item of Single should be aligned with the full A, instead of exposing the B/C.
#[test]
fn narrow_outside_single_parks_on_first_slide_full_width() {
    use crate::core::reactivity::create_root;
    use crate::layout::dim_kind;
    use crate::layout::DimKind;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = format!("t-car-narrow-{}", N.fetch_add(1, Ordering::Relaxed));
    let id_for_spring = id.clone();
    create_root(move || {
        let carousel_el: Element = carousel(id)
            .loop_slides(true)
            .controls(CarouselControls::Outside)
            .variant(CarouselVariant::Single)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        let main: Element = r#box()
            .flex_grow(1.0)
            .min_w(0.0)
            .p("1rem")
            .child(carousel_el)
            .into();
        let side: Element = r#box().w(208.0).h("100%").into();
        let mut root: Element = hstack().w("100%").h("100%").children([side, main]).into();
        let mut layout = LayoutTree::new();
        for _ in 0..4 {
            let _ = layout.build_and_layout(&mut root, 500.0, 400.0, [-1.0, -1.0]);
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                500.0,
                400.0,
                [0.0, 0.0],
            );
        }

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let vp = content.layout_bounds[2];
        assert!(vp > 40.0, "viewport should be measurable, got {vp}");

        let track = content.children.first().expect("track");
        let offset = track.style.translate[0];
        let tgt = offset_target(&id_for_spring).peek();
        assert!(
                (tgt - offset).abs() < 1.0 || track.transition_snapshot.has_active_tweens(),
                "track translate should track offset target (or be mid CSS tween): translate={offset} target={tgt}"
            );

        let mut widths = Vec::new();
        for item in &track.children {
            if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                continue;
            }
            match dim_kind(item.style.size.width) {
                DimKind::Percent(p) => assert!(
                    (p - 1.0).abs() < 0.001,
                    "single slide basis must be 100%, got {p}"
                ),
                other => panic!("expected percent width, got {other:?}"),
            }
            widths.push(item.layout_bounds[2]);
            assert!(
                (item.layout_bounds[2] - vp).abs() < 3.0,
                "layout width must fill viewport ({vp}), got {}; widths={widths:?}",
                item.layout_bounds[2]
            );
        }
        assert!(
            widths.len() >= 3,
            "expected real+clone slides, got {}",
            widths.len()
        );
        assert!(
            (offset + vp).abs() < 3.0,
            "index0 must park at -viewport; offset={offset} vp={vp} widths={widths:?}"
        );
    });
}

/// User reproduction size 828×320: sidebar + max_w 36rem + outside, the first item must be full.
#[test]
fn size_828x320_outside_single_shows_full_first_slide() {
    use crate::core::reactivity::create_root;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = format!("t-car-828-{}", N.fetch_add(1, Ordering::Relaxed));
    let id_spring = id.clone();
    create_root(move || {
        let carousel_el: Element = carousel(id)
            .loop_slides(true)
            .controls(CarouselControls::Outside)
            .variant(CarouselVariant::Single)
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .slide(|| r#box().w("100%").h("8rem").into())
            .into();
        let wrap: Element = r#box().w("100%").max_w("36rem").child(carousel_el).into();
        let main: Element = r#box()
            .flex_grow(1.0)
            .min_w(0.0)
            .p("1.5rem")
            .child(wrap)
            .into();
        let side: Element = r#box().w(208.0).h("100%").into();
        let mut root: Element = hstack().w("100%").h("100%").children([side, main]).into();
        let mut layout = LayoutTree::new();
        for _ in 0..6 {
            let _ = layout.build_and_layout(&mut root, 828.0, 320.0, [-1.0, -1.0]);
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                828.0,
                320.0,
                [0.0, 0.0],
            );
        }

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let vp = content.layout_bounds[2];
        let track = content.children.first().expect("track");
        let offset = track.style.translate[0];
        let tgt = offset_target(&id_spring).peek();
        let mut widths = Vec::new();
        for item in &track.children {
            if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                continue;
            }
            widths.push(item.layout_bounds[2]);
        }
        eprintln!("828x320 diag: vp={vp} offset={offset} target={tgt} widths={widths:?}");
        assert!(vp > 100.0, "vp={vp}");
        assert!(
            widths.iter().all(|w| (*w - vp).abs() < 3.0),
            "percent-basis slides must lay out to viewport; vp={vp} widths={widths:?}"
        );
        assert!(
            (offset + vp).abs() < 3.0 && (tgt + vp).abs() < 3.0,
            "index0 must be -vp; offset={offset} target={tgt} vp={vp} widths={widths:?}"
        );
    });
}

/// Wide→Narrow: The remaining large slot width/old offset must not allow index0 to stop at B/C.
#[test]
fn resize_wide_to_narrow_realigns_single_loop_track() {
    use crate::core::reactivity::create_root;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = format!("t-car-resize-{}", N.fetch_add(1, Ordering::Relaxed));
    create_root(move || {
        let carousel_el: Element = carousel(id)
            .loop_slides(true)
            .controls(CarouselControls::Outside)
            .variant(CarouselVariant::Single)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        let mut root: Element = r#box().w("100%").child(carousel_el).into();
        let mut layout = LayoutTree::new();

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        fn settle(root: &mut Element, layout: &mut LayoutTree, w: f32, h: f32) {
            for _ in 0..4 {
                let _ = layout.build_and_layout(root, w, h, [-1.0, -1.0]);
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    root,
                    layout,
                    &mut impact,
                    w,
                    h,
                    [0.0, 0.0],
                );
            }
        }

        settle(&mut root, &mut layout, 900.0, 400.0);
        let wide_vp = find_slot(&root, SLOT_CONTENT).unwrap().layout_bounds[2];
        assert!(wide_vp > 400.0, "wide vp={wide_vp}");

        settle(&mut root, &mut layout, 420.0, 400.0);
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let vp = content.layout_bounds[2];
        let track = content.children.first().expect("track");
        let offset = track.style.translate[0];
        let mut widths = Vec::new();
        for item in &track.children {
            if item.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ITEM) {
                widths.push(item.layout_bounds[2]);
            }
        }
        assert!(
            widths.iter().all(|w| (*w - vp).abs() < 2.0),
            "after shrink, slides must match vp={vp}, widths={widths:?} (was wide_vp={wide_vp})"
        );
        assert!(
            (offset + vp).abs() < 3.0,
            "after shrink, index0 offset must be -vp; offset={offset} vp={vp} widths={widths:?}"
        );
    });
}

/// Regression: Clone track targets must not be set to a false ideal (≈0) of step≈1 when the viewport is not ready.
#[test]
fn looping_build_does_not_park_offset_at_fake_step_before_viewport() {
    use crate::core::reactivity::create_root;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = format!("t-car-pre-vp-{}", N.fetch_add(1, Ordering::Relaxed));
    let tgt = create_root(|| {
        let id2 = id.clone();
        let _el: Element = carousel(id)
            .loop_slides(true)
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .slide(|| r#box().w("100%").h("4rem").into())
            .into();
        offset_target(&id2).peek()
    })
    .1;
    assert!(
        tgt.abs() < 0.01,
        "pre-viewport offset target must stay at 0, got {tgt}"
    );
}
