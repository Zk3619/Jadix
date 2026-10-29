use super::*;
use crate::primitives::interactive::interactive;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn combined_hover_cursor_matches_cursor_walk_with_occlusion_and_transforms() {
    let mut below = Element::new()
        .cursor_kind(CursorKind::Pointer)
        .on_click(|| {});
    below.layout_bounds = [0.0, 0.0, 120.0, 100.0];
    let mut child = Element::new().on_click(|| {});
    child.layout_bounds = [25.0, 20.0, 55.0, 40.0];
    let mut above: Element = interactive(Element::new().cursor_kind(CursorKind::Text))
        .on_hover(|_| {})
        .into();
    above.layout_bounds = [20.0, 10.0, 90.0, 80.0];
    above.style.z_index = 5;
    above.style.rotate_deg = 23.0;
    above.style.scale = [1.2, 0.8];
    above.children.push(child);
    let mut root = Element::new().child(above).child(below);
    root.layout_bounds = [0.0, 0.0, 140.0, 120.0];
    for overflow in ["visible", "hidden"] {
        root = root.overflow(overflow);
        crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
        for scroll in [[0.0, 0.0], [11.0, 31.0]] {
            for y in (-20..160).step_by(7) {
                for x in (-20..160).step_by(7) {
                    let expected = pick_cursor(&root, x as f32, y as f32, scroll);
                    let actual = pick_hover_and_cursor(&root, x as f32, y as f32, scroll)
                        .into_option()
                        .and_then(|hit| hit.cursor);
                    assert_eq!(actual, expected, "{overflow} {scroll:?} at {x},{y}");
                }
            }
        }
    }
}

#[test]
fn cached_static_hit_bubbles_and_invalidated_handler_is_discovered() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    clear_hover_tracking();
    let entered = Rc::new(Cell::new(0));
    let entered_cb = entered.clone();
    let mut child = Element::new();
    child.layout_bounds = [20.0, 20.0, 30.0, 30.0];
    let mut root: Element = interactive(Element::new().cursor_kind(CursorKind::Pointer))
        .on_hover(move |on| {
            if on {
                entered_cb.set(entered_cb.get() + 1)
            }
        })
        .into();
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.children.push(child);
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.children[0].subtree_hover_cursor);
    apply_pointer_hover_reconcile(&root, 25.0, 25.0, true);
    assert_eq!(
        entered.get(),
        1,
        "static child must bubble to parent's handler"
    );
    root.children[0].cursor = Some(CursorKind::Text.into());
    invalidate_collect_paint_hash_path(&mut root, &[0]);
    let hit = pick_hover_and_cursor(&root, 25.0, 25.0, [0.0, 0.0])
        .into_option()
        .unwrap();
    assert_eq!(
        hit.cursor,
        Some(CursorKind::Text.into()),
        "new cursor must bypass invalid summary"
    );
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.children[0].subtree_hover_cursor);
    clear_hover_tracking();
}

#[test]
fn claiming_underlay_blocks_hover_on_content_below() {
    clear_hover_tracking();
    let below_hits = Rc::new(Cell::new(0u32));
    let below_hits_cb = below_hits.clone();
    let mut below: Element = interactive(Element::new().w(100.0).h(100.0))
        .on_hover(move |h| {
            if h {
                below_hits_cb.set(below_hits_cb.get() + 1);
            }
        })
        .into();
    below.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    // Full-screen underlay claims bounds via on_click but has no on_hover.
    let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
    underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    underlay.style.z_index = 1000;

    let mut root = Element::new().child(below).child(underlay);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let mut to_run = Vec::new();
    update_hovers(&root, 50.0, 50.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(
        below_hits.get(),
        0,
        "content under a claiming underlay must not receive hover"
    );
}

#[test]
fn claiming_underlay_blocks_scrollbar_focus_and_cursor() {
    use crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX;
    use crate::core::tree::CursorKind;

    let mut track = Element::new()
        .w(12.0)
        .h(100.0)
        .cursor_kind(CursorKind::Pointer);
    track.focus_id = Some(format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}gallery:y"));
    track.layout_bounds = [88.0, 0.0, 12.0, 100.0];

    // Underlay is an earlier sibling with higher z — DOM-order-only hit would miss it.
    let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
    underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    underlay.style.z_index = 1000;

    let mut root = Element::new().child(underlay).child(track);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    assert!(
        find_focus_id_at(&root, 94.0, 50.0, [0.0, 0.0]).is_none(),
        "scrollbar track under modal underlay must not win focus hit-test"
    );
    assert!(
        pick_cursor(&root, 94.0, 50.0, [0.0, 0.0]).is_none(),
        "scrollbar Pointer cursor must not leak through claiming underlay"
    );
}

#[test]
fn session_owned_cursor_locks_grabbing_over_hit_miss() {
    use crate::core::tree::CursorKind;
    let grab = Some(CursorKind::Grab.into());
    assert_eq!(
        session_owned_cursor(None, false, true).map(|c| c.kind),
        Some(CursorKind::Grabbing),
        "active drag must not fall back to Default when overlay pointer-events none misses"
    );
    assert_eq!(
        session_owned_cursor(grab.clone(), false, true).map(|c| c.kind),
        Some(CursorKind::Grabbing),
        "grabbing session must not flicker back to item grab"
    );
    assert!(
        session_owned_cursor(None, false, false).is_none(),
        "idle miss still means Default at apply_cursor"
    );
    assert_eq!(
        session_owned_cursor(grab, false, false).map(|c| c.kind),
        Some(CursorKind::Grab)
    );
    assert_eq!(
        session_owned_cursor(None, true, true).map(|c| c.kind),
        Some(CursorKind::NwseResize),
        "textarea resize wins over kit grabbing"
    );
}

#[test]
fn claiming_underlay_blocks_scroll_chain_underneath() {
    use crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX;

    let mut area = Element::new().w(100.0).h(100.0);
    area.focus_id = Some(format!("{SCROLL_AREA_FOCUS_PREFIX}gallery"));
    area.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let mut underlay = Element::new().w(100.0).h(100.0).on_click(|| {});
    underlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    underlay.style.z_index = 1000;

    let mut root = Element::new().child(underlay).child(area);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    let mut chain = Vec::new();
    let hit = crate::app::scroll_support::collect_scroll_chain_at(
        &root,
        50.0,
        50.0,
        [0.0, 0.0],
        &mut chain,
    );
    assert!(hit, "underlay must absorb the pointer");
    assert!(
        chain.is_empty(),
        "gallery scroll area under underlay must not enter hover/wheel chain"
    );
}

#[test]
fn claiming_overlay_blocks_input_text_hit_underneath() {
    use crate::layout::layout_tree::PORTAL_LAYER_ID;
    use crate::utils::text_field::TEXTAREA_FOCUS_PREFIX;

    // Page textarea under a color-picker-like portal panel (claims via on_click).
    let mut leaf = Element::new().w(200.0).h(80.0);
    leaf.layout_bounds = [40.0, 40.0, 200.0, 80.0];
    leaf.text = Some("hello".into());
    leaf.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}under-ta"));

    let mut field = Element::new()
        .w(200.0)
        .h(80.0)
        .on_click(|| {})
        .cursor_text();
    field.layout_bounds = [40.0, 40.0, 200.0, 80.0];
    field.focus_id = Some(format!("{TEXTAREA_FOCUS_PREFIX}under-ta"));
    field = field.child(leaf);

    let mut panel = Element::new().w(180.0).h(160.0).on_click(|| {});
    panel.layout_bounds = [60.0, 20.0, 180.0, 160.0];
    panel.style.z_index = 1001;
    panel.set_attr(crate::layout::layout_tree::ATTR_HOISTED_PORTAL, "1");

    let mut layer = Element::new()
        .w(400.0)
        .h(400.0)
        .pointer_events_box_none()
        .template_id(PORTAL_LAYER_ID)
        .child(panel);
    layer.layout_bounds = [0.0, 0.0, 400.0, 400.0];

    let mut root = Element::new().w(400.0).h(400.0).child(field).child(layer);
    root.layout_bounds = [0.0, 0.0, 400.0, 400.0];

    // Before the LayerHit fix, find_input_text_at walked past the empty panel
    // and focused the textarea under the SV / popover silhouette.
    assert!(
        find_input_text_at(&root, None, 120.0, 80.0, [0.0, 0.0]).is_none(),
        "textarea under a claiming floating panel must not win input hit-test"
    );
    assert_eq!(
        find_focus_id_at(&root, 120.0, 80.0, [0.0, 0.0]),
        None,
        "focus hit-test must also stop on the claiming panel"
    );
}

#[test]
fn button_hover_bubbles_from_child_label() {
    clear_hover_tracking();
    let hovered = Rc::new(Cell::new(false));
    let hovered_cb = hovered.clone();
    let mut label = Element::new().w(80.0).h(20.0);
    label.layout_bounds = [10.0, 10.0, 80.0, 20.0];

    let mut btn: Element = interactive(Element::new().w(100.0).h(40.0).child(label))
        .on_hover(move |h| hovered_cb.set(h))
        .into();
    btn.layout_bounds = [0.0, 0.0, 100.0, 40.0];

    let mut root = Element::new().child(btn);
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];

    let mut to_run = Vec::new();
    // Pointer over the inner label (not the button padding).
    update_hovers(&root, 40.0, 20.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run {
        (*cb.borrow_mut())(is_hover);
    }
    assert!(
        hovered.get(),
        "Button on_hover must fire when pointer is over a non-hover child"
    );
}

#[test]
fn rounded_interactive_hit_excludes_transparent_corners() {
    let mut button = Element::new().w(100.0).h(40.0).rounded(8.0);
    button.layout_bounds = [0.0, 0.0, 100.0, 40.0];
    assert!(
        element_self_pointer_hit(&button, 50.0, 20.0, [0.0, 0.0]),
        "the rounded control face must remain hittable"
    );
    assert!(
        !element_self_pointer_hit(&button, 1.0, 1.0, [0.0, 0.0]),
        "transparent rounded corners must not arm a square hover state"
    );
}

#[test]
fn update_hovers_is_diff_based_not_broadcast() {
    clear_hover_tracking();
    let a_hits = Rc::new(Cell::new(0u32));
    let b_hits = Rc::new(Cell::new(0u32));
    let a_hits_cb = a_hits.clone();
    let b_hits_cb = b_hits.clone();

    let mut a: Element = interactive(Element::new().w(40.0).h(40.0))
        .on_hover(move |h| {
            if h {
                a_hits_cb.set(a_hits_cb.get() + 1);
            }
        })
        .into();
    a.layout_bounds = [0.0, 0.0, 40.0, 40.0];

    let mut b: Element = interactive(Element::new().w(40.0).h(40.0))
        .on_hover(move |h| {
            if h {
                b_hits_cb.set(b_hits_cb.get() + 1);
            }
        })
        .into();
    b.layout_bounds = [50.0, 0.0, 40.0, 40.0];

    let mut root = Element::new().child(a).child(b);
    root.layout_bounds = [0.0, 0.0, 100.0, 40.0];

    let mut to_run = Vec::new();
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(a_hits.get(), 1);
    assert_eq!(b_hits.get(), 0);

    // Same target again: no callbacks (diff-based).
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    assert!(to_run.is_empty());
    assert_eq!(a_hits.get(), 1);

    // Move to B: leave A + enter B only.
    update_hovers(&root, 70.0, 20.0, [0.0, 0.0], &mut to_run);
    assert_eq!(to_run.len(), 2);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(a_hits.get(), 1);
    assert_eq!(b_hits.get(), 1);
}

#[test]
fn hover_region_keeps_child_hover_and_respects_occlusion_after_rebuild() {
    clear_hover_tracking();
    let events = Rc::new(RefCell::new(Vec::new()));
    let region_events = events.clone();
    let child_events = events.clone();
    let mut child = Element::new().focus_id("region-child").on_hover(move |h| {
        child_events.borrow_mut().push(("child", h));
    });
    child.layout_bounds = [20.0, 0.0, 20.0, 40.0];
    let mut region = Element::new()
        .template_id("hover-region")
        .child(child)
        .on_hover(move |h| {
            region_events.borrow_mut().push(("region", h));
        });
    region.set_attr(ATTR_HOVER_REGION, "1");
    region.layout_bounds = [0.0, 0.0, 80.0, 40.0];
    let mut root = Element::new().child(region);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    apply_pointer_hover_reconcile(&root, 30.0, 20.0, true);
    apply_pointer_hover_reconcile(&root, 10.0, 20.0, true);
    apply_pointer_hover_reconcile(&root, 30.0, 20.0, true);
    assert_eq!(
        *events.borrow(),
        [
            ("region", true),
            ("child", true),
            ("child", false),
            ("child", true)
        ]
    );

    // Replace the container handler while the child target stays the same.
    let rebuilt = events.clone();
    root.children[0].on_hover = Some(Rc::new(RefCell::new(move |h| {
        rebuilt.borrow_mut().push(("rebuilt", h));
    })));
    apply_pointer_hover_reconcile(&root, 30.0, 20.0, true);
    assert_eq!(
        events.borrow().len(),
        4,
        "stable region identity must not re-enter"
    );

    // An overlay without its own hover handler still blocks the region below.
    let mut overlay = Element::new().on_click(|| {});
    overlay.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    root.children.push(overlay);
    apply_pointer_hover_reconcile(&root, 30.0, 20.0, true);
    assert_eq!(
        &events.borrow()[4..],
        [("rebuilt", false), ("child", false)]
    );
    clear_hover_tracking();
}

#[test]
fn on_hover_origin_left_rotate_90_uses_painted_aabb() {
    clear_hover_tracking();
    let enters = Rc::new(Cell::new(0u32));
    let leaves = Rc::new(Cell::new(0u32));
    let enters_cb = enters.clone();
    let leaves_cb = leaves.clone();
    let mut tile: Element = interactive(Element::new().w(100.0).h(100.0))
        .on_hover(move |h| {
            if h {
                enters_cb.set(enters_cb.get() + 1);
            } else {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    tile.layout_bounds = [400.0, 200.0, 100.0, 100.0];
    tile.style.rotate_deg = 90.0;
    tile.style.transform_origin = crate::style::parse_transform_origin("left top");

    let mut to_run = Vec::new();
    update_hovers(&tile, 350.0, 250.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(
        enters.get(),
        1,
        "painted ink left of the layout box must enter on_hover"
    );
    assert_eq!(leaves.get(), 0);

    // Hover miss is two-frame (hysteresis). First miss holds; second leaves.
    update_hovers(&tile, 450.0, 250.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    update_hovers(&tile, 450.0, 250.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(
        leaves.get(),
        1,
        "old layout center must leave after origin-left 90°"
    );
}

#[test]
fn hover_identity_survives_handler_rc_rebuild() {
    clear_hover_tracking();
    let enters = Rc::new(Cell::new(0u32));
    let leaves = Rc::new(Cell::new(0u32));
    let enters_cb = enters.clone();
    let leaves_cb = leaves.clone();

    let mut btn: Element = interactive(Element::new().w(40.0).h(40.0))
        .focus_id("stable-btn")
        .on_hover(move |h| {
            if h {
                enters_cb.set(enters_cb.get() + 1);
            } else {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    btn.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    let mut root = Element::new().child(btn);
    root.layout_bounds = [0.0, 0.0, 40.0, 40.0];

    let mut to_run = Vec::new();
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(enters.get(), 1);
    assert_eq!(leaves.get(), 0);

    // Simulate reactive_view rebuild: new on_hover Rc, same focus_id.
    let enters_cb2 = enters.clone();
    let leaves_cb2 = leaves.clone();
    let mut btn2: Element = interactive(Element::new().w(40.0).h(40.0))
        .focus_id("stable-btn")
        .on_hover(move |h| {
            if h {
                enters_cb2.set(enters_cb2.get() + 1);
            } else {
                leaves_cb2.set(leaves_cb2.get() + 1);
            }
        })
        .into();
    btn2.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    root.children[0] = btn2;

    invalidate_hover_handlers();
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    // Soft-invalidate + same focus_id: re-assert enter only (no leave).
    assert_eq!(to_run.len(), 1);
    assert!(to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(leaves.get(), 0);
    assert_eq!(enters.get(), 2);

    // Simulate button chrome: `{focus_id}_hover` stays true while the pointer rests.
    crate::core::reactivity::use_signal("stable-btn_hover", || false).set_silent(true);

    // Quiet Rc swap without invalidate must not re-fire when the hover signal is still true.
    let enters_cb3 = enters.clone();
    let leaves_cb3 = leaves.clone();
    let mut btn3: Element = interactive(Element::new().w(40.0).h(40.0))
        .focus_id("stable-btn")
        .on_hover(move |h| {
            if h {
                enters_cb3.set(enters_cb3.get() + 1);
            } else {
                leaves_cb3.set(leaves_cb3.get() + 1);
            }
        })
        .into();
    btn3.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    root.children[0] = btn3;
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    assert!(to_run.is_empty());
    assert_eq!(enters.get(), 2);
    assert_eq!(leaves.get(), 0);

    // Rc swap + cleared hover signal: re-assert enter (post-jank heal).
    crate::core::reactivity::use_signal("stable-btn_hover", || false).set_silent(false);
    let enters_cb4 = enters.clone();
    let leaves_cb4 = leaves.clone();
    let mut btn4: Element = interactive(Element::new().w(40.0).h(40.0))
        .focus_id("stable-btn")
        .on_hover(move |h| {
            if h {
                enters_cb4.set(enters_cb4.get() + 1);
            } else {
                leaves_cb4.set(leaves_cb4.get() + 1);
            }
        })
        .into();
    btn4.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    root.children[0] = btn4;
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    assert_eq!(to_run.len(), 1);
    assert!(to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(enters.get(), 3);
    assert_eq!(leaves.get(), 0);
}

#[test]
fn hover_miss_uses_one_frame_hysteresis() {
    clear_hover_tracking();
    let leaves = Rc::new(Cell::new(0u32));
    let leaves_cb = leaves.clone();
    let mut btn: Element = interactive(Element::new().w(40.0).h(40.0))
        .focus_id("hyst-btn")
        .on_hover(move |h| {
            if !h {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    btn.layout_bounds = [0.0, 0.0, 40.0, 40.0];
    let mut root = Element::new().child(btn);
    root.layout_bounds = [0.0, 0.0, 40.0, 40.0];

    let mut to_run = Vec::new();
    update_hovers(&root, 20.0, 20.0, [0.0, 0.0], &mut to_run);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }

    // First miss: hold hover — but the deferred leave must pump its own frame.
    // `update_hovers` only runs from `CursorMoved` and the redraw pass, so for a pointer
    // that leaves the element and then stops, nothing else produces miss #2 and the leave
    // would be lost forever (Toast's hover stack then stays expanded with every
    // auto-dismiss timer frozen).
    let _ = crate::primitives::interactive::take_hover_paint_dirty();
    update_hovers(&root, 100.0, 100.0, [0.0, 0.0], &mut to_run);
    assert!(to_run.is_empty());
    assert_eq!(leaves.get(), 0);
    assert!(
        crate::primitives::interactive::take_hover_paint_dirty(),
        "first miss must request a frame so miss #2 can arrive under a still pointer"
    );

    // Second miss: leave.
    update_hovers(&root, 100.0, 100.0, [0.0, 0.0], &mut to_run);
    assert_eq!(to_run.len(), 1);
    assert!(!to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(leaves.get(), 1);
}

#[test]
fn scroll_viewport_visible_overflow_does_not_steal_sidebar_hover() {
    // Gallery shell: main ScrollArea is painted after the sidebar (later sibling = topmost).
    // If `scroll:` viewports with overflow cleared to Visible skip clip, a claiming content
    // tile whose abs box overlaps the sidebar coords absorbs the hit → sidebar leave/enter
    // thrash while the main pane scrolls under a still pointer.
    clear_hover_tracking();
    let enters = Rc::new(Cell::new(0u32));
    let leaves = Rc::new(Cell::new(0u32));
    let enters_cb = enters.clone();
    let leaves_cb = leaves.clone();

    let mut side: Element = interactive(Element::new().w(180.0).h(32.0))
        .focus_id("gal-nav-layout")
        .on_hover(move |h| {
            if h {
                enters_cb.set(enters_cb.get() + 1);
            } else {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    side.layout_bounds = [8.0, 100.0, 180.0, 32.0];
    // Claiming paint so Hit(None) would absorb if tested without clip.
    side.style.bg_color = [0.2, 0.2, 0.2, 1.0];

    let mut tile = Element::new();
    tile.layout_bounds = [8.0, 100.0, 64.0, 64.0];
    tile.style.bg_color = [1.0, 0.0, 0.0, 1.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:gal-layout".into());
    viewport.style.overflow.x = taffy::style::Overflow::Visible;
    viewport.style.overflow.y = taffy::style::Overflow::Visible;
    viewport.layout_bounds = [208.0, 0.0, 600.0, 800.0];
    viewport.scroll_offset = [0.0, 120.0];
    viewport = viewport.child(tile);

    let mut root = Element::new().child(side).child(viewport);
    root.layout_bounds = [0.0, 0.0, 808.0, 800.0];

    let mut to_run = Vec::new();
    update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
    assert_eq!(to_run.len(), 1, "sidebar button must win hover");
    assert!(to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(enters.get(), 1);
    assert_eq!(leaves.get(), 0);

    // Scroll offset changes every frame — hover identity must stay on the sidebar item.
    viewport_scroll_bump(&mut root, 180.0);
    update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
    assert!(
        to_run.is_empty(),
        "scroll must not leave/enter sidebar hover, got {} cbs",
        to_run.len()
    );
    assert_eq!(enters.get(), 1);
    assert_eq!(leaves.get(), 0);

    viewport_scroll_bump(&mut root, 240.0);
    update_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run);
    assert!(to_run.is_empty());
    assert_eq!(leaves.get(), 0);
}

#[test]
fn scroll_content_leave_fires_immediately_under_stationary_pointer() {
    clear_hover_tracking();
    let leaves = Rc::new(Cell::new(0u32));
    let leaves_cb = leaves.clone();

    let mut btn: Element = interactive(Element::new().w(80.0).h(32.0))
        .focus_id("scroll-btn")
        .on_hover(move |h| {
            if !h {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    btn.layout_bounds = [20.0, 100.0, 80.0, 32.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Visible;
    viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(btn);

    let mut root = Element::new().child(viewport);
    root.layout_bounds = [0.0, 0.0, 200.0, 200.0];

    let mut to_run = Vec::new();
    reconcile_pointer_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run, false);
    assert_eq!(to_run.len(), 1);
    assert!(to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }

    viewport_scroll_bump(&mut root, 80.0);
    reconcile_pointer_hovers(&root, 40.0, 110.0, [0.0, 0.0], &mut to_run, true);
    assert_eq!(
        to_run.len(),
        1,
        "scroll must leave immediately under still pointer"
    );
    assert!(!to_run[0].1);
    for (cb, is_hover) in to_run.drain(..) {
        (*cb.borrow_mut())(is_hover);
    }
    assert_eq!(leaves.get(), 1);
}

#[test]
fn apply_pointer_hover_reconcile_leaves_same_frame_as_scroll_miss() {
    clear_hover_tracking();
    let leaves = Rc::new(Cell::new(0u32));
    let leaves_cb = leaves.clone();

    let mut btn: Element = interactive(Element::new().w(80.0).h(32.0))
        .focus_id("unify-btn")
        .on_hover(move |h| {
            if !h {
                leaves_cb.set(leaves_cb.get() + 1);
            }
        })
        .into();
    btn.layout_bounds = [20.0, 100.0, 80.0, 32.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:unify".into());
    viewport.style.overflow.y = taffy::style::Overflow::Visible;
    viewport.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    viewport.scroll_offset = [0.0, 0.0];
    viewport = viewport.child(btn);

    let mut root = Element::new().child(viewport);
    root.layout_bounds = [0.0, 0.0, 200.0, 200.0];

    apply_pointer_hover_reconcile(&root, 40.0, 110.0, false);
    assert_eq!(leaves.get(), 0);

    viewport_scroll_bump(&mut root, 80.0);
    apply_pointer_hover_reconcile(&root, 40.0, 110.0, true);
    assert_eq!(
        leaves.get(),
        1,
        "on_hover leave must fire in the same call as the scroll miss (before CSS chrome)"
    );
    apply_pointer_hover_reconcile(&root, 40.0, 110.0, true);
    assert_eq!(leaves.get(), 1, "leave must be idempotent");
}

fn viewport_scroll_bump(root: &mut Element, sy: f32) {
    fn bump(el: &mut Element, sy: f32) -> bool {
        if el.focus_id.as_deref().is_some_and(|f| {
            f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
        }) {
            el.scroll_offset[1] = sy;
            if let Some(rest) = el.focus_id.as_deref().and_then(|f| {
                f.strip_prefix(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
            }) {
                crate::core::reactivity::use_signal(
                    &crate::core::scroll_viewport::scroll_offset_y_signal_key(rest),
                    || 0.0_f32,
                )
                .set_silent(sy);
            }
            return true;
        }
        for child in &mut el.children {
            if bump(child, sy) {
                return true;
            }
        }
        false
    }
    if !bump(root, sy) {
        // Legacy gallery shell: sidebar + main viewport siblings.
        if let Some(vp) = root.children.get_mut(1) {
            vp.scroll_offset[1] = sy;
        }
    }
}
