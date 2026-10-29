use super::*;
use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerKind};
use crate::app::test_support::sync_reactive_elements_subtree;
use crate::core::tree::LayoutTree;

fn assert_thumb_halo_not_cut(vertical: bool, vw: f32, vh: f32) {
    let mut builder = slider("shadow-clip-probe").track_texture("ramp");
    if vertical {
        builder = builder.orientation("vertical");
    }
    let mut root: Element = builder.into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, vw, vh, [0.0, 0.0]);
    let kinds: Vec<&str> = result
        .paint_ops
        .iter()
        .map(|op| match op {
            crate::layout::layout_tree::PaintOp::Rects { .. } => "r",
            crate::layout::layout_tree::PaintOp::Textured { .. } => "t",
        })
        .collect();
    let tex_at = kinds.iter().position(|k| *k == "t");
    let rect_after = kinds.iter().rposition(|k| *k == "r");
    assert!(
        tex_at.is_some() && rect_after.is_some() && tex_at.unwrap() < rect_after.unwrap(),
        "ramp must paint before the thumb, ops={kinds:?}"
    );
    let thumb = result
        .node_paints
        .iter()
        .find(|p| {
            p.bounds[2] > 16.0 && p.bounds[2] < 30.0 && p.bounds[3] > 16.0 && p.bounds[3] < 30.0
        })
        .expect("thumb slot");
    let pad = THUMB_SHADOW_PAD;
    let b = thumb.bounds;
    let dirty = [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0];
    let scissor = crate::app::draw_cull::node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        thumb.key,
        dirty,
        &[],
        &result.node_paints,
        8.0,
    );
    let wipe = crate::app::draw_cull::node_present_reset_rect(
        thumb.key,
        dirty,
        &result.node_paints,
        &result.instances,
        &[],
        &[],
        0.0,
        1.0,
        None,
    )
    .expect("thumb dest-reset");
    // Horizontal spacers sit on X; vertical spacers sit on Y. Both are inkless.
    let (origin, span, extent, label) = if vertical {
        (1, 3, b[1] + b[3], "vertical")
    } else {
        (0, 2, b[0] + b[2], "horizontal")
    };
    assert!(
        scissor[origin] <= b[origin] - pad + 1.0,
        "{label} leading halo clipped: thumb={b:?} scissor={scissor:?}"
    );
    assert!(
        scissor[origin] + scissor[span] >= extent + pad - 1.0,
        "{label} trailing halo clipped: thumb={b:?} scissor={scissor:?}"
    );
    assert!(
        wipe[origin] <= b[origin] - pad + 1.0 && wipe[origin] + wipe[span] >= extent + pad - 1.0,
        "{label} dest-reset still cut: thumb={b:?} wipe={wipe:?}"
    );
    assert!(
        result
            .clip_polys
            .get(thumb.instance.0)
            .is_none_or(|p| !p.is_active()),
        "thumb shadow must not inherit a clip poly"
    );
}

#[test]
fn horizontal_thumb_shadow_scissor_keeps_lateral_halo() {
    assert_thumb_halo_not_cut(false, 240.0, 40.0);
}

#[test]
fn vertical_thumb_shadow_scissor_keeps_axial_halo() {
    assert_thumb_halo_not_cut(true, 40.0, 240.0);
}

#[test]
fn owned_slider_hover_snaps_shadow_instantly() {
    let (_owner, mut root) =
        crate::core::reactivity::create_root(|| Element::from(slider("owned-hover-snap")));
    let mut layout = LayoutTree::new();
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    assert_eq!(
        find_slot_deep_mut(&mut root, SLOT_THUMB)
            .unwrap()
            .style
            .shadow_blur,
        THUMB_SHADOW_REST.1
    );
    root.on_hover.as_ref().unwrap().borrow_mut()(true);
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    assert_eq!(
        find_slot_deep_mut(&mut root, SLOT_THUMB)
            .unwrap()
            .style
            .shadow_blur,
        THUMB_SHADOW_HOVER.1,
        "hover must snap to elevated shadow (no multi-frame cubic)"
    );
    // Further ticks must stay Clean — continuous StyleChanged was the inline
    // ColorPicker dual-slider hitch.
    for _ in 0..4 {
        crate::style::tick_animations(0.04);
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
        assert_eq!(
            impact,
            LayoutStatus::Clean,
            "settled hover must not keep StyleChanged hot"
        );
    }
    root.on_hover.as_ref().unwrap().borrow_mut()(false);
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    assert_eq!(
        find_slot_deep_mut(&mut root, SLOT_THUMB)
            .unwrap()
            .style
            .shadow_blur,
        THUMB_SHADOW_REST.1
    );
}

#[test]
fn inline_dual_slider_hover_switch_settles_without_sustained_style_changed() {
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        // Inline ColorPicker: two page (non-overlay) full-width tracks.
        Element::new()
            .child(Element::from(
                slider("inline-hue").track_texture("jadix_cp_hue_ramp"),
            ))
            .child(Element::from(
                slider("inline-alpha").track_texture("jadix_cp_alpha:inline"),
            ))
    });
    for (i, child) in root.children.iter_mut().enumerate() {
        child.layout_bounds = [40.0, 100.0 + i as f32 * 28.0, 220.0, 20.0];
        if let Some(thumb) = find_slot_deep_mut(child, SLOT_THUMB) {
            thumb.layout_bounds = [140.0, 100.0 + i as f32 * 28.0, THUMB_PX, THUMB_PX];
        }
    }
    let mut layout = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);

    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(true);
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    assert_eq!(impact, LayoutStatus::StyleChanged);

    // Switch hover hue → alpha (leave + enter).
    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(false);
    root.children[1].on_hover.as_ref().unwrap().borrow_mut()(true);
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    assert_eq!(
        impact,
        LayoutStatus::StyleChanged,
        "switch frame may paint both thumbs once"
    );

    let mut hot = 0u32;
    for _ in 0..8 {
        crate::style::tick_animations(0.016);
        impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
        if impact != LayoutStatus::Clean {
            hot += 1;
        }
    }
    assert_eq!(
        hot, 0,
        "after snap, hover switch must not sustain StyleChanged frames (got {hot})"
    );
    assert!(
        !crate::style::tick_animations(0.016).any,
        "slider hover must not leave cubic tweens requesting redraw"
    );
}

#[test]
fn vertical_shadow_damage_is_scroll_adjusted_and_keyed() {
    let mut root: Element = slider("vertical-damage").orientation("vertical").into();
    root.layout_bounds = [80.0, 500.0, 20.0, 128.0];
    root.set_paint_scroll_offset([0.0, 400.0]);
    // No thumb layout yet → damage falls back to the slider root AABB.
    crate::app::damage::clear_frame_paint_nodes();
    push_slider_thumb_paint_damage(&root);
    let damage = crate::app::damage::take_frame_paint_nodes();
    assert_eq!(damage.len(), 1);
    assert_eq!(damage[0].key, crate::app::damage::paint_element_key(&root));
    assert_eq!(damage[0].bounds, [70.0, 90.0, 40.0, 148.0]);
    root.paint_overlay_stamp = Some(71);
    push_slider_thumb_paint_damage(&root);
    let damage = crate::app::damage::take_frame_paint_nodes();
    assert_eq!(damage[0].key, 71);
    assert_eq!(damage[0].kind, crate::app::damage::PaintDirtyKind::Overlay);
}

/// Inline ColorPicker: page Node dest-reset keys by NodePaint slot. Root-keyed
/// damage expands to the full `w:100%` track (encode 90–180ms). Thumb key keeps
/// the wipe local — the floating path never hits this because it is Overlay.
#[test]
fn page_dual_slider_hover_damage_keys_thumb_not_full_track() {
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        Element::new()
            .child(Element::from(
                slider("page-hue").track_texture("jadix_cp_hue_ramp"),
            ))
            .child(Element::from(
                slider("page-alpha").track_texture("jadix_cp_alpha:page"),
            ))
    });
    for (i, child) in root.children.iter_mut().enumerate() {
        child.layout_bounds = [40.0, 100.0 + i as f32 * 28.0, 220.0, 20.0];
        child.set_paint_scroll_offset([0.0, 0.0]);
        if let Some(thumb) = find_slot_deep_mut(child, SLOT_THUMB) {
            thumb.layout_bounds = [140.0, 100.0 + i as f32 * 28.0, THUMB_PX, THUMB_PX];
        }
    }
    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(true);
    let mut layout = LayoutTree::new();
    crate::app::damage::clear_frame_paint_nodes();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(false);
    root.children[1].on_hover.as_ref().unwrap().borrow_mut()(true);
    crate::app::damage::clear_frame_paint_nodes();
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    let damage = crate::app::damage::take_frame_paint_nodes();
    let page: Vec<_> = damage
        .into_iter()
        .filter(|d| d.kind == crate::app::damage::PaintDirtyKind::Node)
        .collect();
    assert!(
        !page.is_empty(),
        "page hover switch must emit Node thumb damage"
    );
    for (i, child) in root.children.iter().enumerate() {
        let root_key = crate::app::damage::paint_element_key(child);
        let thumb_key = find_slot_deep(child, SLOT_THUMB)
            .map(crate::app::damage::paint_element_key)
            .expect("thumb");
        assert_ne!(
            root_key, thumb_key,
            "slider[{i}] root and thumb must key differently (bounds-key)"
        );
        for d in &page {
            assert_ne!(
                d.key, root_key,
                "page damage must not key the full-width slider root (dest-reset expands to track)"
            );
        }
        if let Some(d) = page.iter().find(|d| d.key == thumb_key) {
            assert!(
                d.bounds[2] < 80.0 && d.bounds[3] < 80.0,
                "thumb-keyed damage must stay local, got {:?}",
                d.bounds
            );
        }
    }
    assert!(
        page.iter().any(|d| {
            find_slot_deep(&root.children[1], SLOT_THUMB).map(crate::app::damage::paint_element_key)
                == Some(d.key)
        }),
        "enter-alpha frame must dirty the alpha thumb key"
    );
}

/// ColorPicker hue↔alpha: two full-width sliders under one Overlay stamp.
/// Paint-only hover must dirty the thumb halo, not union full-width strips into
/// the whole popover.
#[test]
fn adjacent_overlay_slider_hover_damage_stays_thumb_local() {
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        Element::new()
            .child(Element::from(slider("cp-hue")))
            .child(Element::from(slider("cp-alpha")))
    });
    const STAMP: u64 = 4242;
    for (i, child) in root.children.iter_mut().enumerate() {
        child.paint_overlay_stamp = Some(STAMP);
        child.layout_bounds = [40.0, 100.0 + i as f32 * 28.0, 220.0, 20.0];
        child.set_paint_scroll_offset([0.0, 0.0]);
        if let Some(thumb) = find_slot_deep_mut(child, SLOT_THUMB) {
            thumb.layout_bounds = [40.0 + 100.0, 100.0 + i as f32 * 28.0, THUMB_PX, THUMB_PX];
        }
    }
    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(true);

    let mut layout = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    root.children[0].on_hover.as_ref().unwrap().borrow_mut()(false);
    root.children[1].on_hover.as_ref().unwrap().borrow_mut()(true);
    crate::app::damage::clear_frame_paint_nodes();
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
    let damage = crate::app::damage::take_frame_paint_nodes();
    let overlay: Vec<_> = damage
        .into_iter()
        .filter(|d| d.kind == crate::app::damage::PaintDirtyKind::Overlay)
        .collect();
    assert!(
        !overlay.is_empty(),
        "hover switch must emit Overlay thumb damage"
    );
    assert!(overlay.iter().all(|d| d.key == STAMP));
    let x0 = overlay.iter().map(|d| d.bounds[0]).fold(f32::MAX, f32::min);
    let y0 = overlay.iter().map(|d| d.bounds[1]).fold(f32::MAX, f32::min);
    let x1 = overlay
        .iter()
        .map(|d| d.bounds[0] + d.bounds[2])
        .fold(f32::MIN, f32::max);
    let y1 = overlay
        .iter()
        .map(|d| d.bounds[1] + d.bounds[3])
        .fold(f32::MIN, f32::max);
    let union_w = x1 - x0;
    let union_h = y1 - y0;
    assert!(
        union_w < 80.0,
        "overlay hover damage width must stay thumb-local, got {union_w} (full track is 220)"
    );
    assert!(
        union_h < 80.0,
        "overlay hover damage height must stay thumb-local, got {union_h}"
    );
    // Snap: subsequent frames stay Clean (no sustained cubic).
    for _ in 0..4 {
        impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
        assert_eq!(impact, LayoutStatus::Clean);
    }
}

#[test]
fn dragging_slider_does_not_elevate_other_thumbs() {
    clear_slider_drag_if_any();
    let (_owner, mut root) = crate::core::reactivity::create_root(|| {
        Element::new()
            .child(Element::from(slider("drag-source")))
            .child(Element::from(slider("drag-disabled").disabled(true)))
            .child(Element::from(slider("drag-other")))
    });
    let mut layout = LayoutTree::new();
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    let mut event = PointerEvent::new(60.0, 10.0, winit::event::MouseButton::Left, 1);
    event.phase = EventPhase::Target;
    event.current_bounds = [0.0, 0.0, 120.0, 20.0];
    root.children[0].on_mousedown.as_ref().unwrap().borrow_mut()(&event);
    sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    for _ in 0..3 {
        crate::style::tick_animations(0.075);
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut LayoutStatus::Clean);
    }
    clear_slider_drag_if_any();
    for sibling in &mut root.children[1..] {
        let thumb = find_slot_deep_mut(sibling, SLOT_THUMB).unwrap();
        assert_eq!(
            thumb.style.shadow_blur, THUMB_SHADOW_REST.1,
            "unrelated and disabled thumbs must stay at rest"
        );
    }
}

fn has_slot(el: &Element, slot: &str) -> bool {
    crate::core::tree::any_in_tree(el, |n| {
        n.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot)
    })
}

#[test]
fn track_texture_omits_primary_fill() {
    let el: Element = slider("tex").track_texture("jadix_cp_hue_ramp").into();
    assert!(!has_slot(&el, SLOT_FILL));
    assert!(has_slot(&el, SLOT_THUMB));
    let track = &el.children[0];
    assert_eq!(track.image_texture.as_deref(), Some("jadix_cp_hue_ramp"));
    assert!(
        track.style.bg_color[3] > 0.5,
        "textured track must keep an opaque bed, a={}",
        track.style.bg_color[3]
    );
}

#[test]
fn shadow_pad_is_omnidirectional_and_insets_track() {
    use crate::layout::{lp_length_px, lpa_length_px};

    let horizontal: Element = slider("shadow-pad-h").into();
    assert_eq!(
        lp_length_px(horizontal.style.padding.left),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(horizontal.style.padding.right),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(horizontal.style.padding.top),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(horizontal.style.padding.bottom),
        Some(THUMB_SHADOW_PAD)
    );
    // Cross-axis must grow: otherwise T/B halo is clipped flush to the thumb.
    assert_eq!(
        crate::layout::dim_length_px(horizontal.style.size.height),
        Some(ROOT_CROSS)
    );
    let h_track = &horizontal.children[0];
    assert_eq!(
        lpa_length_px(h_track.style.inset.left),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lpa_length_px(h_track.style.inset.right),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lpa_length_px(h_track.style.inset.top),
        Some(THUMB_SHADOW_PAD + TRACK_INSET)
    );
    assert_eq!(h_track.style.z_index, 0);
    assert_eq!(horizontal.children[1].style.z_index, 0);

    let vertical: Element = slider("shadow-pad-v").orientation("vertical").into();
    assert_eq!(
        lp_length_px(vertical.style.padding.top),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(vertical.style.padding.bottom),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(vertical.style.padding.left),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lp_length_px(vertical.style.padding.right),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        crate::layout::dim_length_px(vertical.style.size.width),
        Some(ROOT_CROSS)
    );
    let v_track = &vertical.children[0];
    assert_eq!(
        lpa_length_px(v_track.style.inset.top),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lpa_length_px(v_track.style.inset.bottom),
        Some(THUMB_SHADOW_PAD)
    );
    assert_eq!(
        lpa_length_px(v_track.style.inset.left),
        Some(THUMB_SHADOW_PAD + TRACK_INSET)
    );
    assert_eq!(v_track.style.z_index, 0);
    assert_eq!(vertical.children[1].style.z_index, 0);
}

#[test]
fn pointer_x_maps_to_value_ends() {
    let travel = 100.0;
    let b = [10.0, 0.0, travel + THUMB_PX + 2.0 * THUMB_SHADOW_PAD, 20.0];
    let lo = value_from_pointer_x(10.0 + THUMB_SHADOW_PAD + THUMB_PX * 0.5, b, 0.0, 1.0);
    let hi = value_from_pointer_x(
        10.0 + THUMB_SHADOW_PAD + THUMB_PX * 0.5 + travel,
        b,
        0.0,
        1.0,
    );
    assert!(lo <= 0.02, "lo={lo}");
    assert!(hi >= 0.98, "hi={hi}");
}

#[test]
fn pointer_drag_is_continuous_not_stepped() {
    let travel = 100.0;
    let b = [0.0, 0.0, travel + THUMB_PX + 2.0 * THUMB_SHADOW_PAD, 20.0];
    // 中点附近：若按 0.25 步进会落到 0.25/0.5；连续应接近真实比例。
    let mid = value_from_pointer_x(THUMB_SHADOW_PAD + THUMB_PX * 0.5 + 37.0, b, 0.0, 1.0);
    assert!((mid - 0.37).abs() < 0.02, "mid={mid}");
    assert!(
        (mid - 0.25).abs() > 0.05 && (mid - 0.5).abs() > 0.05,
        "must not snap to 0.25 step grid, got {mid}"
    );
}

#[test]
fn pointer_down_on_thumb_row_sets_value_via_bubble() {
    clear_slider_drag_if_any();
    let value = use_signal("__jadix_test_slider_ptr", || 0.2_f32);
    value.set_silent(0.2);
    let mut root: Element = slider("test-slider-ptr").value_signal(value).into();
    // 模拟已布局：根宽 120（含拇指），高 20
    root.layout_bounds = [0.0, 0.0, 120.0, 20.0];
    let bounds = root.layout_bounds;
    for c in &mut root.children {
        c.layout_bounds = bounds;
        for g in &mut c.children {
            g.layout_bounds = [0.0, 0.0, 40.0, 20.0];
        }
    }
    let mut layout_tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

    // 点在右半：应抬高 value（叶子命中后 bubble 到根）
    let x = 100.0;
    let y = 10.0;
    let path = hit_path(&root, x, y);
    assert!(!path.is_empty(), "expected hit path");
    assert!(
        path.iter().any(|e| e.on_mousedown.is_some()),
        "root handler must be on path"
    );
    let mut pev = PointerEvent::new(x, y, winit::event::MouseButton::Left, 1);
    dispatch_pointer(&path, &mut pev, PointerKind::Down);
    assert!(
        value.peek() > 0.5,
        "click should raise value, got {}",
        value.peek()
    );
    assert!(crate::utils::drag::routing::freezes_scroll());
    assert!(
        crate::utils::drag::routing::prepare_move(Some(&root), 110.0, 10.0).dispatch(),
        "drag should be active"
    );
    crate::utils::drag::routing::finish_all();
    assert!(!is_slider_dragging());
}

#[test]
fn pointer_y_maps_to_value_ends_vertical() {
    let travel = 100.0;
    let b = [0.0, 10.0, 20.0, travel + THUMB_PX + 2.0 * THUMB_SHADOW_PAD];
    let lo = value_from_pointer_y(
        10.0 + THUMB_SHADOW_PAD + THUMB_PX * 0.5 + travel,
        b,
        0.0,
        1.0,
    );
    let hi = value_from_pointer_y(10.0 + THUMB_SHADOW_PAD + THUMB_PX * 0.5, b, 0.0, 1.0);
    assert!(lo <= 0.02, "lo={lo}");
    assert!(hi >= 0.98, "hi={hi}");
}

#[test]
fn vertical_pointer_uses_visual_bounds_not_layout_bounds() {
    // layout y=200, scroll 180 → visual y=20; pointer at thumb (~40% from bottom).
    let h = 128.0;
    let layout_b = [0.0, 200.0, 20.0, h];
    let visual_b = [0.0, 20.0, 20.0, h];
    let inner = (h - 2.0 * THUMB_SHADOW_PAD - THUMB_PX).max(1.0);
    let y_thumb = visual_b[1] + THUMB_SHADOW_PAD + inner * 0.6 + THUMB_PX * 0.5;
    let with_layout = value_from_pointer_y(y_thumb, layout_b, 0.0, 1.0);
    let with_visual = value_from_pointer_y(y_thumb, visual_b, 0.0, 1.0);
    assert!(
        with_layout > 0.95,
        "layout_bounds under scroll skews to max, got {with_layout}"
    );
    assert!(
        (with_visual - 0.4).abs() < 0.05,
        "visual bounds should map thumb center, got {with_visual}"
    );
}

#[test]
fn vertical_drag_tick_uses_captured_visual_bounds() {
    clear_slider_drag_if_any();
    let value = use_signal("__jadix_test_slider_vert_drag", || 0.2_f32);
    value.set_silent(0.2);
    let visual_b = [40.0, 60.0, 20.0, 128.0];
    begin_slider_drag(
        "vert-drag",
        SliderDrag {
            value,
            min: 0.0,
            max: 1.0,
            bounds: Rc::new(Cell::new(visual_b)),
            vertical: true,
        },
    );
    let mid_y = visual_b[1] + (visual_b[3] - THUMB_PX) * 0.5 + THUMB_PX * 0.5;
    assert!(tick_slider_drag(0.0, mid_y));
    assert!(
        (value.peek() - 0.5).abs() < 0.08,
        "mid drag y should land near 50%, got {}",
        value.peek()
    );
    clear_slider_drag_if_any();
}

#[test]
fn builder_thumb_style_reaches_thumb_slot() {
    let el: Element = slider("thumb-chrome")
        .slot(SLOT_THUMB, |c| c.style(|b| b.text_size(20.0)))
        .into();
    fn find_thumb_size(el: &Element, want: f32) -> bool {
        let is_thumb = el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_THUMB);
        (is_thumb && el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01))
            || el.children.iter().any(|c| find_thumb_size(c, want))
    }
    assert!(
        find_thumb_size(&el, 20.0),
        "slider .slot(slider-thumb) style must land on data-slot=slider-thumb"
    );
}
