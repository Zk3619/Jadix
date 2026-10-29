use super::*;

#[test]
fn scroll_replay_keeps_inherited_masks_and_moves_local_masks() {
    let mut inherited = GpuClipPoly::from_axis_rect([0.0, 0.0, 300.0, 288.0]);
    inherited.round_radii = [12.0; 4];
    inherited.count = 3;
    inherited.pts[..3].copy_from_slice(&[[0.0, 0.0], [300.0, 0.0], [150.0, 288.0]]);
    let mut poly = inherited;
    poly.round_rect2 = [10.0, 100.0, 40.0, 40.0];
    poly.round_radii2 = [4.0; 4];
    poly.count2 = 3;
    poly.pts2[..3].copy_from_slice(&[[10.0, 100.0], [50.0, 100.0], [30.0, 140.0]]);
    nudge_clip_poly(
        &mut poly,
        [3.0, -8.0],
        &GeomKey::default(),
        &GeomKey::default(),
        &inherited,
    );
    assert_eq!(poly.round_rect, inherited.round_rect);
    assert_eq!(poly.round_radii, inherited.round_radii);
    assert_eq!(poly.pts, inherited.pts);
    assert_eq!(poly.round_rect2, [13.0, 92.0, 40.0, 40.0]);
    assert_eq!(poly.round_radii2, [4.0; 4]);
    assert_eq!(
        &poly.pts2[..3],
        &[[13.0, 92.0], [53.0, 92.0], [33.0, 132.0]]
    );
}

#[test]
fn inherited_mask_change_invalidates_replay_with_same_clip_aabb() {
    let mut clip = GpuClipPoly::from_axis_rect([0.0, 0.0, 300.0, 288.0]);
    let original = GeomKey::default().with_inherited_clip(&clip);
    clip.round_radii = [12.0; 4];
    assert!(!geom_eq(
        &original,
        &GeomKey::default().with_inherited_clip(&clip)
    ));
    clip.round_radii = [0.0; 4];
    clip.round_rect[1] = 8.0;
    assert!(!geom_eq(
        &original,
        &GeomKey::default().with_inherited_clip(&clip)
    ));
}

#[test]
fn stamped_text_edit_discovery_preserves_focus_and_carets() {
    let mut root = Element::new().child(Element::new().focus_id("input:stamped"));
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!stamped_subtree_holds_live_text_edit(&root, ""));
    assert!(!stamped_subtree_holds_live_text_edit(&root, "button:other"));
    assert!(stamped_subtree_holds_live_text_edit(&root, "input:stamped"));
    assert!(!stamped_subtree_holds_live_text_edit(&root, "input:other"));

    root.children[0].text_style.input_caret = Some(Default::default());
    root.collect_paint_hash = 0;
    assert!(stamped_subtree_holds_live_text_edit(&root, ""));
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(stamped_subtree_holds_live_text_edit(&root, ""));
    root.children[0].text_style.input_caret = None;
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!stamped_subtree_holds_live_text_edit(&root, ""));
}

#[test]
fn paint_hash_uses_transform_inputs_not_previous_layout_bounds() {
    let mut el = Element::new()
        .w(80.0)
        .h(40.0)
        .rotate(30.0)
        .css_transform("translateX(50%)")
        .transform_origin("left top");
    el.layout_bounds = [0.0, 0.0, 80.0, 40.0];
    stamp_subtree_paint_hashes(&mut el, false);
    let specified = el.collect_paint_hash;
    el.layout_bounds = [0.0, 0.0, 120.0, 60.0];
    stamp_subtree_paint_hashes(&mut el, false);
    assert_eq!(
        el.collect_paint_hash, specified,
        "geometry is validated separately by GeomKey"
    );
    el = el.css_transform("translateX(25%)");
    stamp_subtree_paint_hashes(&mut el, false);
    assert_ne!(
        el.collect_paint_hash, specified,
        "transform values must invalidate paint"
    );
}

#[test]
fn dirty_subtree_reuses_snapshot_storage() {
    let mut root = Element::new()
        .w(80.0)
        .h(40.0)
        .child(Element::new().w(80.0).h(40.0).bg([1.0, 0.0, 0.0, 1.0]));
    let result = crate::layout::layout_tree::LayoutTree::new()
        .build_and_layout(&mut root, 80.0, 40.0, [-1.0; 2]);
    let el = &mut root.children[0];
    let node = el.layout_node.unwrap();
    // Supply a valid stamp: hash zero intentionally bypasses cache storage.
    el.collect_paint_hash = 1;
    assert!(subtree_is_skippable(el));
    let mut cache = CollectSkipState {
        allowed: true,
        ..Default::default()
    };
    let store = |cache: &mut CollectSkipState, el: &Element| {
        cache.store(
            node,
            el,
            GeomKey::default(),
            DlCursor::default(),
            &result.instances,
            &result.textured_draws,
            &result.text_nodes,
            &result.input_chrome_instances,
            &result.clip_polys,
            &result.paint_ops,
            [80.0, 40.0],
        );
        cache.flush_snapshots(
            &result.instances,
            &result.textured_draws,
            &result.text_nodes,
            &result.input_chrome_instances,
            &result.clip_polys,
        );
    };
    cache.begin_frame();
    store(&mut cache, el);
    let cache_capacity = cache.prev.capacity();
    cache.end_frame();
    let first = &cache.prev[&node];
    let storage = (
        first.payload.as_ref().unwrap().instances.as_ptr(),
        first.payload.as_ref().unwrap().clip_polys.as_ptr(),
        first.paint_ops.as_ptr(),
    );
    assert!(!first.payload.as_ref().unwrap().instances.is_empty());
    cache.begin_frame();
    el.collect_paint_hash = el.collect_paint_hash.wrapping_add(1);
    store(&mut cache, el);
    cache.end_frame();
    let second = &cache.prev[&node];
    assert_eq!(
        storage,
        (
            second.payload.as_ref().unwrap().instances.as_ptr(),
            second.payload.as_ref().unwrap().clip_polys.as_ptr(),
            second.paint_ops.as_ptr()
        ),
        "same-size dirty frames must reuse cached allocations"
    );
    assert_eq!(
        cache.prev.capacity(),
        cache_capacity,
        "keep cache map allocation"
    );
}

#[test]
fn nested_snapshots_share_payload_and_bound_history() {
    let mut root = Element::new().w(80.0).h(40.0).bg([1.0, 0.0, 0.0, 1.0]);
    for _ in 0..32 {
        root = Element::new().w(80.0).h(40.0).child(root);
    }
    root = Element::new().w(80.0).h(40.0).child(root);
    let result = crate::layout::layout_tree::LayoutTree::new()
        .build_and_layout(&mut root, 80.0, 40.0, [-1.0; 2]);
    let root = &mut root.children[0];
    let mut cache = CollectSkipState {
        allowed: true,
        ..Default::default()
    };
    for frame in 0..8 {
        cache.begin_frame();
        let mut element = &mut *root;
        loop {
            element.collect_paint_hash = frame + 1;
            cache.store(
                element.layout_node.unwrap(),
                element,
                GeomKey::default(),
                DlCursor::default(),
                &result.instances,
                &result.textured_draws,
                &result.text_nodes,
                &result.input_chrome_instances,
                &result.clip_polys,
                &result.paint_ops,
                [80.0, 40.0],
            );
            if element.children.is_empty() || frame > 0 {
                break;
            }
            element = &mut element.children[0];
        }
        cache.flush_snapshots(
            &result.instances,
            &result.textured_draws,
            &result.text_nodes,
            &result.input_chrome_instances,
            &result.clip_polys,
        );
        cache.end_frame();
        let payload = cache.prev[&root.layout_node.unwrap()]
            .payload
            .as_ref()
            .unwrap();
        if frame == 0 {
            assert_eq!(cache.prev.len(), 33);
            assert!(cache
                .prev
                .values()
                .all(|cached| Rc::ptr_eq(cached.payload.as_ref().unwrap(), payload)));
        }
        assert_eq!(
            cache.snapshot_items,
            result.instances.len()
                + result.textured_draws.len()
                + result.text_nodes.len()
                + result.input_chrome_instances.len()
                + result.clip_polys.len()
                + payload.node_paints.len()
        );
        let unique: HashMap<_, _> = cache
            .prev
            .values()
            .map(|cached| {
                let payload = cached.payload.as_ref().unwrap();
                (Rc::as_ptr(payload) as usize, payload.inline_bytes())
            })
            .collect();
        assert!(unique.values().sum::<usize>() <= cache.frame_bytes * 3);
    }
    assert_eq!(
        cache.prev.len(),
        33,
        "cold siblings remain hot while within budget"
    );
    cache.frame_bytes = 0;
    cache.end_frame();
    assert!(
        cache.prev.is_empty(),
        "over-budget arenas are evicted together"
    );
}
#[test]
fn cached_texture_scroll_moves_rounded_clip_with_image() {
    let mut draw = TexturedDraw::clipped([10.0, 20.0, 40.0, 40.0], None, "photo", 1.0).unwrap();
    draw.rounded_clips = [[10.0, 20.0, 40.0, 40.0], [20.0; 4], [0.0; 4], [0.0; 4]];
    nudge_textured(
        &mut draw,
        [3.0, -8.0],
        &GeomKey::default(),
        &GeomKey::default(),
        &GpuClipPoly::NONE,
    );
    assert_eq!(draw.rect, [13.0, 12.0, 40.0, 40.0]);
    assert_eq!(draw.rounded_clips[0], draw.rect);
    assert_eq!(draw.rounded_clips[1], [20.0; 4]);
    assert_eq!(draw.rounded_clips[2], [0.0; 4]);
}

fn el_with_child() -> Element {
    let mut el = Element::new();
    el.children.push(Element::new().text("x"));
    el
}

#[test]
fn portal_layer_is_not_skippable() {
    let mut el = el_with_child();
    el.template_id = Some(PORTAL_LAYER_ID.to_string());
    assert!(!subtree_is_skippable(&el));
}

#[test]
fn hoisted_portal_is_not_skippable() {
    let mut el = el_with_child();
    el.set_attr(ATTR_HOISTED_PORTAL, "1");
    assert!(!subtree_is_skippable(&el));
}

#[test]
fn jadix_portal_is_not_skippable() {
    let mut el = el_with_child();
    el.set_attr("jadix_portal", "1");
    assert!(!subtree_is_skippable(&el));
}

#[test]
#[cfg(feature = "product-tests")]
fn drag_overlay_is_not_skippable() {
    let _product = crate::utils::drag::drag_overlay();
    let mut el = el_with_child();
    el.set_attr(crate::utils::drag::ATTR_DRAG_OVERLAY, "1");
    assert!(crate::utils::drag::runtime::keeps_collect_live(&el));
    assert!(!subtree_is_skippable(&el));
}

#[test]
fn ordinary_subtree_with_children_is_skippable() {
    let mut el = el_with_child();
    // Wave 52: hash 0 means surgically invalidated / never stamped — not skippable.
    el.collect_paint_hash = 1;
    for c in &mut el.children {
        c.collect_paint_hash = 1;
    }
    assert!(subtree_is_skippable(&el));
}

#[test]
fn scroll_band_cull_keeps_a_live_paint_subtree() {
    let mut shader = Element::new();
    shader.set_attr(crate::primitives::shader::ATTR_SHADER, "1");
    let mut root = Element::new().child(shader);

    stamp_subtree_paint_hashes(&mut root, false);

    assert!(root.children[0].collect_skip_live);
    assert!(stamped_subtree_holds_live_paint(&root.children[0]));
}

#[test]
fn stacking_band_node_is_not_skippable() {
    let mut abs = el_with_child();
    abs.style.position = taffy::style::Position::Absolute;
    assert!(
        !subtree_is_skippable(&abs),
        "absolute dock skip would drop descendant dest-reset slots"
    );
    let mut zed = el_with_child();
    zed.style.z_index = 20;
    assert!(!subtree_is_skippable(&zed));
}

#[test]
fn empty_leaf_is_skippable() {
    let mut el = Element::new();
    el.collect_paint_hash = 1;
    assert!(
        subtree_is_skippable(&el),
        "stamped leaf replay avoids collect_paint_node on every glyph/rect"
    );
    assert!(
        !subtree_is_skippable(&Element::new()),
        "hash 0 (invalidated / never stamped) must recollect"
    );
}

#[test]
fn overlay_scrollbar_thumb_is_not_skippable() {
    let mut thumb = Element::new().w(8.0).h(24.0);
    thumb.focus_id = Some(format!(
        "{}sa:y",
        crate::core::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX
    ));
    assert!(
        !subtree_is_skippable(&thumb),
        "relative overlay thumb skip replays Taffy leftover and freezes the rail"
    );
    let mut track = Element::new().w(8.0).h(120.0);
    track.focus_id = Some(format!(
        "{}sa:y",
        crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX
    ));
    assert!(!subtree_is_skippable(&track));
}

#[test]
fn overflow_hidden_is_skippable_for_exact_replay() {
    let mut el = el_with_child();
    el.style.overflow.x = taffy::style::Overflow::Hidden;
    assert!(
        !subtree_is_skippable(&el),
        "a fresh node has no valid cached display list"
    );
    stamp_subtree_paint_hashes(&mut el, false);
    assert!(
        subtree_is_skippable(&el),
        "overflow-hidden may exact-replay; translate is refused separately"
    );
    el.collect_paint_hash = 0;
    assert!(
        !subtree_is_skippable(&el),
        "an invalidated overflow node must recollect"
    );
}

#[test]
fn z_index_child_does_not_force_stamp_of_idle_siblings() {
    let mut viewport = Element::new();
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    viewport.focus_id = Some(format!(
        "{}stamp-z",
        crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX
    ));
    let mut zed = Element::new().text("overlay");
    zed.style.z_index = 10;
    let mut idle = Element::new();
    for i in 0..40 {
        idle.children.push(Element::new().text(format!("row{i}")));
    }
    viewport.children.push(zed);
    viewport.children.push(idle);

    STAMP_HASHED.with(|c| c.set(0));
    stamp_subtree_paint_hashes(&mut viewport, false);
    let full = STAMP_HASHED.with(|c| c.get());
    assert!(full >= 40, "cold stamp must hash idle rows, hashed={full}");

    STAMP_HASHED.with(|c| c.set(0));
    stamp_subtree_paint_hashes(&mut viewport, true);
    let reuse = STAMP_HASHED.with(|c| c.get());
    assert!(
        reuse <= 4,
        "z-index sibling must not poison idle row stamp (reuse hashed={reuse}, cold={full})"
    );
    assert!(
        !viewport.children[1].collect_skip_live,
        "idle column must not inherit live from a stacking sibling"
    );
}

/// Inline ColorPicker sits beside Calendar / hosts a format Select portal under the
/// same page parent. Slider StyleChanged zeros that parent hash; portal side-channel
/// must not poison the neighbor into a full restamp (the hue↔alpha hover hitch).
#[test]
fn portal_sibling_style_changed_must_not_restamp_idle_neighbor() {
    let mut portal = Element::new().text("select-menu");
    portal
        .attrs
        .insert(ATTR_HOISTED_PORTAL.to_string(), "1".into());
    let mut portal_host = Element::new().child(portal);
    for i in 0..20 {
        portal_host
            .children
            .push(Element::new().text(format!("menu-row{i}")));
    }
    let mut idle = Element::new();
    for i in 0..80 {
        idle.children
            .push(Element::new().text(format!("sv-cell{i}")));
    }
    let slider = Element::new().text("hue-thumb");
    let mut row = Element::new().child(idle).child(slider).child(portal_host);
    stamp_subtree_paint_hashes(&mut row, false);
    assert!(
        !row.children[0].collect_skip_live,
        "idle SV neighbor must not inherit collect_skip_live from a portal sibling"
    );
    assert!(
        !row.children[0].collect_skip_side_channel,
        "idle SV neighbor must not inherit portal side-channel"
    );
    assert!(
        row.children[2].collect_skip_side_channel,
        "portal host must track side-channel for collect (overlay cursors)"
    );
    assert!(
        !row.collect_skip_live,
        "row must not be anim-live merely because a child hosts a portal"
    );
    assert!(
        row.collect_skip_side_channel,
        "row tracks portal side-channel without poisoning anim-live"
    );
    let idle_hash = row.children[0].collect_paint_hash;
    let portal_host_hash = row.children[2].collect_paint_hash;

    // Paint-only StyleChanged on the slider spine (same as reactive_sync path bust).
    invalidate_collect_paint_hash_path(&mut row, &[1]);
    STAMP_HASHED.with(|c| c.set(0));
    stamp_subtree_paint_hashes(&mut row, true);
    let hashed = STAMP_HASHED.with(|c| c.get());
    assert!(
            hashed <= 6,
            "reuse_idle after sibling StyleChanged must not rehash idle SV / portal host body (hashed={hashed})"
        );
    assert_eq!(
        row.children[0].collect_paint_hash, idle_hash,
        "idle neighbor hash must be reused"
    );
    assert_eq!(
        row.children[2].collect_paint_hash, portal_host_hash,
        "portal host with stable paint must reuse its stamp"
    );
    assert!(
        subtree_is_skippable(&row.children[0]),
        "idle neighbor must stay collect-skippable after parent rehash"
    );
    assert!(
        !subtree_is_skippable(&row.children[2]),
        "portal host must remain unskippable for collect (side-channel cursors)"
    );
}

#[test]
fn hover_spine_invalidation_does_not_restamp_idle_sibling() {
    use crate::layout::layout_tree::LayoutTree;
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .w(240.0)
        .h(40.0)
        .child(Element::new().w(80.0).h(32.0).text("idle"))
        .child(Element::new().w(80.0).h(32.0).text("hover"));
    let _ = tree.build_and_layout(&mut root, 240.0, 40.0, [-1.0, -1.0]);
    stamp_subtree_paint_hashes(&mut root, false);
    let idle_hash = root.children[0].collect_paint_hash;
    assert_ne!(idle_hash, 0);
    let hover_id = root.children[1].layout_node.expect("hover leaf laid out");
    assert!(invalidate_collect_paint_hash_spines(&mut root, &[hover_id]));
    assert_eq!(
        root.children[0].collect_paint_hash, idle_hash,
        "invalidating the hover spine must not zero the idle sibling"
    );
    assert_eq!(root.collect_paint_hash, 0);
    assert_eq!(root.children[1].collect_paint_hash, 0);
    STAMP_HASHED.with(|c| c.set(0));
    stamp_subtree_paint_hashes(&mut root, true);
    let hashed = STAMP_HASHED.with(|c| c.get());
    assert!(
        hashed < 12,
        "reuse_idle after a hover spine bust must not rehash the idle sibling (hashed={hashed})"
    );
    assert_eq!(root.children[0].collect_paint_hash, idle_hash);
}

#[test]
fn animation_recollect_rehashes_shell_not_idle_page() {
    let mut page = Element::new();
    for i in 0..500 {
        page.children.push(Element::new().text(format!("row {i}")));
    }
    let shell = Element::new()
        .template_id("dialog__modal_zoom")
        .child(Element::new().text("Animated caption"));
    let mut root = Element::new().child(page).child(shell);
    stamp_subtree_paint_hashes(&mut root, false);
    let page_hash = root.children[0].collect_paint_hash;
    let old = root.children[1].collect_paint_hash;
    root.children[1].style.opacity = 0.0;
    invalidate_animation_shell_hashes(&mut root);
    STAMP_HASHED.with(|c| c.set(0));
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(
        STAMP_HASHED.with(|c| c.get()) <= 4,
        "second collect must not rehash 500 idle rows"
    );
    assert_eq!(root.children[0].collect_paint_hash, page_hash);
    assert_ne!(
        root.children[1].collect_paint_hash, old,
        "final transparent pose must invalidate"
    );
}

#[test]
fn geom_key_includes_opacity() {
    let a = geom_key(
        [0.0, 0.0],
        [10.0, 10.0],
        [0.0, 0.0],
        None,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    let b = geom_key(
        [0.0, 0.0],
        [10.0, 10.0],
        [0.0, 0.0],
        None,
        0.4,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    let c = geom_key(
        [0.0, 0.0],
        [10.0, 10.0],
        [0.0, 0.0],
        None,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    assert!(
        !geom_eq(&a, &b),
        "opacity change must bust skip geom, not replay the previous fade"
    );
    assert!(geom_eq(&a, &c));
}

#[test]
fn scroll_translate_delta_is_old_minus_new() {
    let clip = Some([0.0, 0.0, 80.0, 120.0]);
    let rest = geom_key(
        [0.0, 80.0],
        [80.0, 40.0],
        [0.0, 0.0],
        clip,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    let scrolled = geom_key(
        [0.0, 80.0],
        [80.0, 40.0],
        [0.0, 48.0],
        clip,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    assert_eq!(scroll_translate_delta(&rest, &scrolled), Some([0.0, -48.0]));
    let el = el_with_child();
    assert!(subtree_fully_inside_clip(&rest, &el));
    assert!(subtree_fully_inside_clip(&scrolled, &el));
    let entering = geom_key(
        [0.0, 120.0],
        [80.0, 40.0],
        [0.0, 0.0],
        clip,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    assert!(
        !subtree_fully_inside_clip(&entering, &el),
        "clip-culled rows must not translate-replay an empty DL"
    );
}

#[test]
fn geom_key_includes_ancestor_linear() {
    let id = geom_key(
        [0.0, 0.0],
        [10.0, 10.0],
        [0.0, 0.0],
        None,
        1.0,
        [1.0, 1.0],
        crate::style::MAT2D_IDENTITY,
    );
    let rot = geom_key(
        [0.0, 0.0],
        [10.0, 10.0],
        [0.0, 0.0],
        None,
        1.0,
        [1.0, 1.0],
        crate::style::mat2d_rotate(90.0f32.to_radians()),
    );
    assert!(
        !geom_eq(&id, &rot),
        "ancestor rotate must bust skip geom even when Taffy origin is unchanged"
    );
}

fn self_paint_hash(el: &Element) -> u64 {
    let mut h = DefaultHasher::new();
    hash_paint_self(el, &mut h);
    h.finish()
}

#[test]
fn paint_hash_includes_extra_shadow_values() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.extra_shadows.push(BoxShadow {
        color: [1.0, 0.0, 0.0, 0.8],
        offset: [2.0, 2.0],
        blur: 4.0,
        spread: 0.0,
        inset: false,
    });
    let before = self_paint_hash(&el);
    el.style.extra_shadows[0].color = [0.0, 1.0, 0.0, 0.8];
    assert_ne!(
        before,
        self_paint_hash(&el),
        "same-length extra_shadows color change must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_gradient_stop_values() {
    use crate::style::LinearGradient;
    let mut el = Element::new();
    el.style.bg_gradient = Some(Gradient::Linear(LinearGradient::two_stop(
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, 1.0],
        90.0,
    )));
    let before = self_paint_hash(&el);
    if let Some(Gradient::Linear(g)) = el.style.bg_gradient.as_mut() {
        g.stops[0].color = [0.0, 0.0, 1.0, 1.0];
    }
    assert_ne!(
        before,
        self_paint_hash(&el),
        "same-shape gradient stop color must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_filter_blur_amount() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.filter = crate::style::parse_filter("blur(4px)");
    let before = self_paint_hash(&el);
    el.style.filter = crate::style::parse_filter("blur(12px)");
    assert_ne!(
        before,
        self_paint_hash(&el),
        "filter blur amount (not just discriminant) must bust paint-hash skip"
    );
}

#[test]
fn filter_capture_keeps_ancestors_live_for_collect_side_effects() {
    let mut filtered = Element::new();
    filtered.style.filter = crate::style::parse_filter("blur(4px)");
    let mut root = Element::new().child(filtered).child(Element::new());
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(root.collect_skip_side_channel);
    assert!(root.children[0].collect_skip_side_channel);
    assert!(!subtree_is_skippable(&root));
    assert!(!subtree_is_skippable(&root.children[0]));
    assert!(subtree_is_skippable(&root.children[1]));
}

#[test]
fn paint_hash_includes_backdrop_blur() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.backdrop_blur = 0.0;
    let before = self_paint_hash(&el);
    el.style.backdrop_blur = 8.0;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "backdrop_blur must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_glass_optical_parameters() {
    let mut element = Element::new().bg("card");
    let plain = self_paint_hash(&element);
    element.style.glass = Some(crate::style::GlassMaterial::default());
    let glass = self_paint_hash(&element);
    assert_ne!(plain, glass);
    for field in 0..4 {
        let mut material = crate::style::GlassMaterial::default();
        match field {
            0 => material.refraction_index = 1.8,
            1 => material.thickness = 24.0,
            2 => material.dispersion = 0.1,
            _ => material.highlight = 0.9,
        }
        element.style.glass = Some(material);
        assert_ne!(glass, self_paint_hash(&element));
    }
}

#[test]
fn paint_hash_includes_clip_path_values() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.clip_path = crate::style::parse_clip_path("circle(40% at 50% 50%)").unwrap();
    let before = self_paint_hash(&el);
    el.style.clip_path = crate::style::parse_clip_path("circle(20% at 50% 50%)").unwrap();
    assert_ne!(
        before,
        self_paint_hash(&el),
        "clip-path circle radius must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_outline_values() {
    use crate::style::LineStyle;
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.outline_style = LineStyle::Solid;
    el.style.outline_width = 2.0;
    el.style.outline_color = [0.0, 0.0, 1.0, 1.0];
    let before = self_paint_hash(&el);
    el.style.outline_color = [0.0, 1.0, 0.0, 1.0];
    assert_ne!(
        before,
        self_paint_hash(&el),
        "outline color must bust paint-hash skip"
    );
    let after_color = self_paint_hash(&el);
    el.style.outline_width = 6.0;
    assert_ne!(
        after_color,
        self_paint_hash(&el),
        "outline width must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_per_side_border_color() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.set_uniform_border_width(2.0);
    el.style.set_border_side_color(0, [0.0, 0.0, 1.0, 1.0]);
    let before = self_paint_hash(&el);
    el.style.set_border_side_color(0, [0.0, 1.0, 0.0, 1.0]);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "per-side border color must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_border_image_and_text_chrome() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]).text("hi");
    el.style.border_image_source = Some("tex:a".into());
    el.text_style.letter_spacing = 0.0;
    el.text_style.underline = Some(false);
    let before = self_paint_hash(&el);
    el.style.border_image_source = Some("tex:b".into());
    assert_ne!(
        before,
        self_paint_hash(&el),
        "border-image source must bust paint-hash skip"
    );
    el.style.border_image_source = Some("tex:a".into());
    assert_eq!(before, self_paint_hash(&el));
    el.text_style.letter_spacing = 2.0;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "letter-spacing must bust paint-hash skip"
    );
    el.text_style.letter_spacing = 0.0;
    el.text_style.underline = Some(true);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "underline must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_corner_shape_values() {
    use crate::style::CornerShape;
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.corner_shape = CornerShape::Round;
    let before = self_paint_hash(&el);
    el.style.corner_shape = CornerShape::Notch;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "corner-shape keyword must bust paint-hash skip"
    );
    el.style.corner_shape = CornerShape::Superellipse(2.0);
    let se = self_paint_hash(&el);
    el.style.corner_shape = CornerShape::Superellipse(8.0);
    assert_ne!(
        se,
        self_paint_hash(&el),
        "superellipse exponent (not just discriminant) must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_translate_pct() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.translate_pct = [0.0, 0.0];
    let before = self_paint_hash(&el);
    el.style.translate_pct = [0.5, 0.0];
    assert_ne!(
        before,
        self_paint_hash(&el),
        "translate_pct must bust paint-hash skip even when layout_bounds are still 0"
    );
}

#[test]
fn paint_hash_includes_transform_origin() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.rotate_deg = 45.0;
    assert!(
        el.layout_bounds[2] == 0.0 && el.layout_bounds[3] == 0.0,
        "fingerprint-time box must still be 0×0"
    );
    el.style.transform_origin = crate::style::parse_transform_origin("center center");
    let center = self_paint_hash(&el);
    el.style.transform_origin = crate::style::parse_transform_origin("left top");
    assert_ne!(
            center,
            self_paint_hash(&el),
            "transform-origin keywords must bust paint-hash skip when the combined matrix sees a 0×0 box"
        );
    el.style.transform_origin = crate::style::parse_transform_origin("10px 10px");
    let px = self_paint_hash(&el);
    el.style.transform_origin = crate::style::parse_transform_origin("20px 10px");
    assert_ne!(
        px,
        self_paint_hash(&el),
        "transform-origin length px must bust paint-hash skip (origin_frac collapses px to 0.5)"
    );
}

#[test]
fn paint_hash_includes_transform_list_percent_translate() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    assert!(
        el.layout_bounds[2] == 0.0 && el.layout_bounds[3] == 0.0,
        "fingerprint-time box must still be 0×0"
    );
    el.style.transform = crate::style::parse_transform("translate(50%)");
    let a = self_paint_hash(&el);
    el.style.transform = crate::style::parse_transform("translate(10%)");
    assert_ne!(
        a,
        self_paint_hash(&el),
        "transform list % translate must bust skip when the combined matrix sees a 0×0 box"
    );
}

#[test]
fn paint_hash_includes_text_layout_fields() {
    use crate::style::{TextAlign, TextDirection, TextOverflow, WhiteSpace, WritingMode};
    let mut el = Element::new().text("hi");
    let before = self_paint_hash(&el);
    el.text_style.font_family = Some("Inter".into());
    assert_ne!(
        before,
        self_paint_hash(&el),
        "font_family must bust paint-hash skip"
    );
    el.text_style.font_family = None;
    assert_eq!(before, self_paint_hash(&el));
    el.text_style.align = Some(TextAlign::Center);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "align must bust paint-hash skip"
    );
    el.text_style.align = None;
    el.text_style.align_last = Some(crate::style::TextAlignLast::Center);
    assert_ne!(before, self_paint_hash(&el));
    el.text_style.align_last = None;
    el.text_style.white_space = WhiteSpace::Nowrap;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "white_space must bust paint-hash skip"
    );
    el.text_style.white_space = WhiteSpace::Normal;
    el.text_style.wrap_mode = crate::text::WrapMode::Char;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "wrap_mode must bust paint-hash skip"
    );
    el.text_style.wrap_mode = crate::text::WrapMode::Word;
    el.text_style.overflow = TextOverflow::Ellipsis;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "text overflow must bust paint-hash skip"
    );
    el.text_style.overflow = TextOverflow::Clip;
    el.text_style.direction = TextDirection::Rtl;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "direction must bust paint-hash skip"
    );
    el.text_style.direction = TextDirection::Auto;
    el.text_style.line_height_px = Some(20.0);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "line_height_px must bust paint-hash skip"
    );
    el.text_style.line_height_px = None;
    el.text_style.line_height_scale = Some(1.5);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "line_height_scale must bust paint-hash skip"
    );
    el.text_style.line_height_scale = None;
    el.style.writing_mode = WritingMode::VerticalRl;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "writing_mode must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_offset_distance() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    el.style.offset_distance = 0.0;
    let before = self_paint_hash(&el);
    el.style.offset_distance = 1.0;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "offset_distance must bust paint-hash skip (paint-only motion path)"
    );
    el.style.offset_distance = 0.0;
    el.style.offset_rotate_deg = Some(15.0);
    assert_ne!(
        before,
        self_paint_hash(&el),
        "offset_rotate_deg must bust paint-hash skip"
    );
    el.style.offset_rotate_deg = None;
    el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 80 0")"#).unwrap();
    assert_ne!(
        before,
        self_paint_hash(&el),
        "offset_path d-string must bust paint-hash skip"
    );
}

#[test]
fn paint_hash_includes_overflow_clip_mode() {
    let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
    el.style.overflow.y = taffy::style::Overflow::Visible;
    let before = self_paint_hash(&el);
    el.style.overflow.y = taffy::style::Overflow::Hidden;
    assert_ne!(
        before,
        self_paint_hash(&el),
        "overflow:hidden must bust paint-hash skip (instance clip is a collect side effect)"
    );
    el.style.overflow.y = taffy::style::Overflow::Visible;
    el.set_attr(crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    assert_ne!(
        before,
        self_paint_hash(&el),
        "jadix_escape_overflow_clip must bust paint-hash skip"
    );
}
