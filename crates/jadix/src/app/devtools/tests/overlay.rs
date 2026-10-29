use super::*;
use crate::app::devtools::snapshot::{find_element_by_path_mut, find_element_by_template};
use crate::app::devtools::state::DevToolsState;

fn test_tree() -> Element {
    let mut target_a = Element::new().template_id("node-a");
    target_a.layout_bounds = [10.0, 20.0, 100.0, 30.0];
    let mut target_b = Element::new().template_id("node-b");
    target_b.layout_bounds = [10.0, 80.0, 100.0, 30.0];
    let mut app = Element::new().children(vec![target_a, target_b]);
    app.template_id = Some("app".into());
    let mut host = Element::new().children(vec![app, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    Element::new().child(host)
}

#[test]
fn retained_highlight_stamps_include_inherited_preview_scale() {
    let mut target = Element::new();
    target.paint_ancestor_scale = [0.25, 0.25];
    begin_highlight_collect();
    note_element_paint_visual_from_abs(&mut target, [100.0, 200.0], [100.0, 40.0], [0.0; 2]);
    assert_eq!(target.paint_visual_bounds, [137.5, 215.0, 25.0, 10.0]);
    assert_eq!(target.layout_bounds, [100.0, 200.0, 100.0, 40.0]);
}

#[test]
fn scaled_highlight_survives_retained_layout_and_inspector_window_collect() {
    crate::core::reactivity::create_root(|| {
        let _scope = crate::app::window_scope::enter_primary();
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        let target = Element::new()
            .template_id("scaled-retained-target")
            .w(100.0)
            .h(40.0);
        let mut root = Element::new().w(800.0).h(600.0).child(
            Element::new()
                .absolute()
                .left(100.0)
                .top(80.0)
                .w(400.0)
                .h(400.0)
                .scale(0.25)
                .transform_origin("left top")
                .child(target),
        );
        DevToolsState::set_selected_path(vec![0, 0]);
        DevToolsState::set_hover_highlight_path(vec![0, 0]);
        let mut layout = crate::core::tree::LayoutTree::new();
        for _ in 0..3 {
            layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
            let model = highlight_box_model(&root, &[0, 0]).unwrap();
            assert!((model.width - 25.0).abs() < 0.1, "scaled model: {model:?}");
            {
                let _secondary = crate::app::window_scope::enter(
                    crate::app::window_scope::WindowScope::Secondary(412),
                );
                begin_highlight_collect();
            }
            let model = highlight_box_model(&root, &[0, 0]).unwrap();
            assert!(
                (model.width - 25.0).abs() < 0.1,
                "secondary overwrote page stamps: {model:?}"
            );
        }
        DevToolsState::set_open(false);
    });
}

#[test]
fn highlight_uses_scaled_ancestor_paint_geometry_and_scales_box_edges() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    let mut target = Element::new().template_id("preview-highlight-test").p(8.0);
    target.layout_bounds = [100.0, 200.0, 200.0, 100.0];
    let mut root = Element::new().scale(0.25).child(target);
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];
    DevToolsState::set_selected_path(vec![0]);
    DevToolsState::clear_hover_highlight();
    DevToolsState::set_hover_highlight_path(vec![0]);
    begin_highlight_collect();
    note_element_paint_visual(&mut root.children[0], [25.0, 50.0, 50.0, 25.0]);
    let model = highlight_box_model(&root, &[0]).unwrap();
    assert_eq!(
        [model.x, model.y, model.width, model.height],
        [25.0, 50.0, 50.0, 25.0]
    );
    assert_eq!(model.padding_top, 2.0);
    assert_eq!(model.padding_left, 2.0);
    DevToolsState::set_open(false);
}

#[test]
fn scaled_highlight_fallback_keeps_collected_geometry_after_tree_rebuild() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        let mut root = Element::new().w(680.0).h(720.0).child(
            Element::new()
                .absolute()
                .left(100.0)
                .top(80.0)
                .w(390.0)
                .h(844.0)
                .scale(0.5)
                .transform_origin("left top")
                .child(
                    Element::new()
                        .focus_id("scaled-perf-button")
                        .w(140.0)
                        .h(28.0)
                        .p(8.0),
                ),
        );
        DevToolsState::set_hover_highlight_path(vec![0, 0]);
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 680.0, 720.0, [-1.0; 2]);
        let expected = root.children[0].children[0].paint_visual_bounds;
        begin_highlight_collect();
        let model = highlight_box_model(&root, &[0, 0]).unwrap();
        assert_eq!([model.x, model.y, model.width, model.height], expected);
        assert_eq!(model.padding_left, 4.0);
        DevToolsState::set_open(false);
    });
}

#[test]
fn highlight_damage_replays_content_beneath_its_own_layer() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new().w(200.0).h(100.0).child(highlight_slot());
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
        crate::app::damage::take_frame_paint_nodes();
        let content = &mut root.children[0].children[4];
        stamp_rect(content, 20.0, 20.0, 100.0, 28.0, [0.0; 2], [0.2; 4]);
        let damage = crate::app::damage::take_frame_paint_nodes();
        assert!(!damage.is_empty());
        assert!(damage
            .iter()
            .all(|dirty| dirty.key != crate::app::damage::paint_element_key(content)));
    });
}

#[test]
fn highlight_slot_keeps_ancestors_live_for_hover_only_collects() {
    let mut root = test_tree();
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(!crate::layout::collect_cache::subtree_is_skippable(&root));
    assert!(!crate::layout::collect_cache::subtree_is_skippable(
        &root.children[0]
    ));
    assert!(crate::layout::collect_cache::subtree_is_skippable(
        &root.children[0].children[0],
    ));
}

#[test]
fn highlight_changes_restore_the_composite_under_old_and_new_rects() {
    crate::core::reactivity::create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};

        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_highlight_hover_layer(None);
        DevToolsState::set_highlight_layer(HighlightLayer::All);
        let mut root = test_tree();
        DevToolsState::set_selected_path(vec![0, 0, 0]);
        DevToolsState::set_hover_highlight_path(vec![0, 0, 0]);
        refresh_highlight(&mut root);
        clear_frame_paint_nodes();
        DevToolsState::set_selected_path(vec![0, 0, 1]);
        DevToolsState::set_hover_highlight_path(vec![0, 0, 1]);
        assert!(refresh_highlight(&mut root));
        let dirties = take_frame_paint_nodes();
        for point in [[50.0, 30.0], [50.0, 90.0]] {
            assert!(
                dirties.iter().any(|dirty| {
                    dirty.kind == PaintDirtyKind::Region
                        && point[0] >= dirty.bounds[0]
                        && point[1] >= dirty.bounds[1]
                        && point[0] < dirty.bounds[0] + dirty.bounds[2]
                        && point[1] < dirty.bounds[1] + dirty.bounds[3]
                }),
                "highlight left unreplayed pixels at {point:?}: {dirties:?}"
            );
        }
        DevToolsState::set_highlight_layer(HighlightLayer::Content);
        assert!(
            refresh_highlight(&mut root),
            "fill-only change must invalidate paint"
        );
        assert!(take_frame_paint_nodes()
            .iter()
            .all(|dirty| dirty.kind == PaintDirtyKind::Region));
        assert!(!refresh_highlight(&mut root));
        assert!(take_frame_paint_nodes().is_empty());
        DevToolsState::set_open(false);
        assert!(refresh_highlight(&mut root));
        assert!(
            !take_frame_paint_nodes().is_empty(),
            "hiding must restore old pixels"
        );
        let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).unwrap();
        for layer in &slot.children {
            assert_eq!(
                layer.style.border_width, 0.0,
                "hidden highlight must not paint a stale Taffy stroke"
            );
            assert_eq!(layer.style.border_color[3], 0.0);
            assert_eq!(layer.style.bg_color[3], 0.0);
        }
    });
}

#[test]
fn box_model_fill_hover_replays_dimension_text_as_a_composite() {
    crate::core::reactivity::create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};

        DevToolsState::enable_for_window();
        DevToolsState::set_highlight_hover_layer(None);
        DevToolsState::set_highlight_layer(HighlightLayer::All);
        let bm = crate::app::devtools::snapshot::box_model_snapshot(
            find_element_by_path(&test_tree(), &[0, 0, 0]).unwrap(),
        );
        let mut diagram = box_model_diagram(&bm, HighlightLayer::All);
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut diagram, 320.0, 200.0, [-1.0; 2]);
        clear_frame_paint_nodes();
        DevToolsState::set_highlight_hover_layer(Some(HighlightLayer::Padding));
        let mut impact = LayoutStatus::Clean;
        crate::app::test_support::sync_reactive_elements_subtree(
            &mut diagram,
            &mut layout,
            &mut impact,
        );
        let dirties = take_frame_paint_nodes();
        assert!(!dirties.is_empty());
        assert!(
            dirties
                .iter()
                .all(|dirty| dirty.kind == PaintDirtyKind::Region),
            "ring fill must replay intersecting labels, not only its plate: {dirties:?}"
        );
    });
}

#[test]
fn box_model_ring_hover_targets_the_selected_element() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_selected_path(vec![0, 0, 0]);
        let diagram = box_model_diagram(
            &crate::app::devtools::snapshot::box_model_snapshot(
                find_element_by_path(&test_tree(), &[0, 0, 0]).unwrap(),
            ),
            HighlightLayer::All,
        );
        let ring = find_element_by_template(&diagram, BM_RING_CONTENT).unwrap();
        ring.on_hover.as_ref().unwrap().borrow_mut()(true);
        assert_eq!(
            DevToolsState::hover_path_key(),
            DevToolsState::selected_path_key()
        );
        assert_eq!(DevToolsState::highlight_layer(), HighlightLayer::Content);
        ring.on_hover.as_ref().unwrap().borrow_mut()(false);
        assert!(!DevToolsState::has_hover_highlight());
        assert_eq!(DevToolsState::highlight_layer(), HighlightLayer::All);
    });
}

#[test]
fn highlight_collect_cache_tracks_selection_change() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut root = test_tree();
    // Make sure that if `highlight_path` is empty we *don't* accidentally highlight root.
    root.layout_bounds = [0.0, 0.0, 800.0, 600.0];
    begin_highlight_collect();
    set_highlight_collect_root(&root);
    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 800.0, 600.0];

    DevToolsState::set_hover_highlight_path(vec![0, 0, 0]);
    assert!(refresh_highlight_slot(slot));
    let a = stamped_highlight_abs_rect(&slot.children[4]).expect("content layer");
    assert!((a[1] - 20.0).abs() < 0.5, "first hover y={}", a[1]);

    DevToolsState::set_hover_highlight_path(vec![0, 0, 1]);
    assert!(refresh_highlight_slot(slot));
    let b = stamped_highlight_abs_rect(&slot.children[4]).expect("content layer");
    assert!((b[1] - 80.0).abs() < 0.5, "hover highlight y={}", b[1]);
    assert!((a[1] - b[1]).abs() > 10.0);

    DevToolsState::clear_hover_highlight_if_path(&vec![0, 0, 1]);
    assert!(!DevToolsState::has_hover_highlight());
    assert!(DevToolsState::highlight_path().is_empty());
    assert!(refresh_highlight_slot(slot));
    assert!(stamped_highlight_abs_rect(&slot.children[4]).is_none());

    clear_highlight_collect_root();
}

#[test]
fn highlight_slot_prefers_noted_paint_visual_over_abs_scroll() {
    use taffy::style::Overflow;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut target = Element::new().template_id("btn");
    target.layout_bounds = [10.0, 250.0, 80.0, 40.0];
    // Paint uses abs − scroll + translate; reconstruction historically omitted translate.
    target.style.translate = [0.0, 12.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 100.0]);
    viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    viewport = viewport.child(target);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    let mut root = Element::new().child(host);
    root.layout_bounds = [0.0, 0.0, 400.0, 200.0];

    let path = vec![0, 0, 0];
    DevToolsState::set_hover_highlight_path(path.clone());

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    // Same space as collect `render_rect` (250 − 100 + 12).
    {
        let target_ref = find_element_by_path_mut(&mut root, &path).expect("target");
        note_element_paint_visual(target_ref, [10.0, 162.0, 80.0, 40.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border layer");
    assert!(
        (border[1] - 162.0).abs() < 0.5,
        "noted paint visual must win over abs−scroll, got y={}",
        border[1]
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn stamped_highlight_prefers_element_paint_visual_bounds() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut target = Element::new().template_id("card");
    target.layout_bounds = [20.0, 400.0, 128.0, 302.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 180.0]);
    viewport = viewport.child(target);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.style.overflow.y = taffy::style::Overflow::Hidden;
    host.scroll_offset = [0.0, 80.0];
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];

    let mut root = Element::new().child(host);
    let path = vec![0, 0, 0];
    DevToolsState::set_hover_highlight_path(path.clone());

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let t = find_element_by_path_mut(&mut root, &path).unwrap();
        // Paint space (already scrolled); must win over abs−(viewport+host) reconstruction.
        note_element_paint_visual(t, [20.0, 220.0, 128.0, 302.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[1] - 220.0).abs() < 0.5,
        "must keep paint visual y=220, got {}",
        border[1]
    );
    // Reconstruction that wrongly includes host overflow:hidden offset would land at 140.
    assert!(
        (border[1] - 140.0).abs() > 10.0,
        "must not double-subtract host scroll"
    );
    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_resolves_by_template_id_when_child_index_path_is_stale() {
    // Gallery repro: Elements row still shows card caption/tid, but live child indices
    // shifted so the snapshotted path now points at a button (Outline-sized).
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut card = Element::new()
        .template_id("examples/gallery/src/main.rs:191:5")
        .p(16.0);
    card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

    let mut outline = Element::new()
        .template_id("outline-btn")
        .focus_id("btn-outline");
    outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

    // Live tree: viewport[outline, card] — path [0,0,0] is Outline.
    let mut viewport = Element::new().children(vec![outline, card]);
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 40.0]);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut root = Element::new().child(host);

    // Stale path → Outline; identity → 1b card (what the tree row still labels).
    let stale_path = vec![0, 0, 0];
    DevToolsState::set_hover_highlight(
        stale_path,
        "examples/gallery/src/main.rs:191:5".to_string(),
        Vec::new(),
        (164.0, 334.0),
    );

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
        let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
        note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[1] - 320.0).abs() < 0.5 && (border[3] - 334.0).abs() < 0.5,
        "identity must win over stale path (Outline); got {:?}",
        border
    );
    assert!(
        (border[3] - 36.0).abs() > 10.0,
        "must not stamp the Outline-sized rect"
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_resolves_anonymous_child_via_identity_relative_path() {
    // Screenshot repro: hover the description Label under 1b card (no tid). Absolute
    // path drifted to Outline; anchor tid + relative [1,0] must still hit the label.
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut title = Element::new().text("1b) Hover");
    title.layout_bounds = [28.0, 370.0, 120.0, 24.0];
    let mut desc = Element::new().text("默认 hover_easing");
    desc.layout_bounds = [28.0, 400.0, 120.0, 36.0];
    let inner = Element::new().children(vec![desc]);
    let mut card = Element::new()
        .template_id("examples/gallery/src/main.rs:191:5")
        .children(vec![title, inner]);
    card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

    let mut outline = Element::new().template_id("outline-btn");
    outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

    let mut viewport = Element::new().children(vec![outline, card]);
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 40.0]);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut root = Element::new().child(host);

    // Absolute path points at Outline; relative [1,0] under card is the description.
    DevToolsState::set_hover_highlight(
        vec![0, 0, 0],
        "examples/gallery/src/main.rs:191:5".to_string(),
        vec![1, 0],
        (120.0, 36.0),
    );

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
        let desc = find_element_by_path_mut(&mut root, &[0, 0, 1, 1, 0]).unwrap();
        note_element_paint_visual(desc, [28.0, 360.0, 120.0, 36.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[1] - 360.0).abs() < 0.5 && (border[3] - 36.0).abs() < 0.5,
        "must highlight description label via relative path, got {:?}",
        border
    );
    assert!(
        (border[0] - 40.0).abs() > 1.0 || (border[2] - 72.0).abs() > 1.0,
        "must not stamp Outline geometry"
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn selecting_node_clears_stale_pick_hover_overlay() {
    // Repro: Styles shows the clicked 1b card (128×298) while the page overlay still
    // stamps a leftover Ghost/Outline hover from pick / a previous tree row.
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut card = Element::new()
        .template_id(r"examples\gallery\src\main.rs:191:5")
        .p(16.0);
    card.layout_bounds = [20.0, 360.0, 164.0, 334.0];

    let mut ghost = Element::new()
        .template_id("ghost-btn")
        .focus_id("btn-ghost");
    ghost.layout_bounds = [40.0, 120.0, 72.0, 36.0];

    let mut viewport = Element::new().children(vec![ghost, card]);
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 40.0]);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut root = Element::new().child(host);

    // Leftover pick hover on Ghost (absolute path, no useful identity).
    DevToolsState::set_hover_highlight_path(vec![0, 0, 0]);
    // Click the 1b card in the Elements tree (backslash tid as on Windows `file!()`).
    DevToolsState::set_selected(
        vec![0, 0, 1],
        r"examples\gallery\src\main.rs:191:5".to_string(),
        Vec::new(),
        (164.0, 334.0),
    );

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let ghost = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        note_element_paint_visual(ghost, [40.0, 80.0, 72.0, 36.0]);
        let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
        note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(!DevToolsState::has_hover_highlight());
    let _ = refresh_highlight_slot(slot);
    assert!(stamped_highlight_abs_rect(&slot.children[2]).is_none());

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn identity_match_normalizes_path_separators() {
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut card = Element::new()
        .template_id("examples/gallery/src/main.rs:191:5")
        .p(16.0);
    card.layout_bounds = [20.0, 360.0, 164.0, 334.0];
    let mut outline = Element::new().template_id("outline-btn");
    outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];
    let viewport = Element::new().children(vec![outline, card]);
    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut root = Element::new().child(host);

    // Snapshot / UI shows Windows separators; live tree keeps forward slashes.
    DevToolsState::set_hover_highlight(
        vec![0, 0, 0],
        r"examples\gallery\src\main.rs:191:5".to_string(),
        Vec::new(),
        (164.0, 334.0),
    );

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        note_element_paint_visual(outline, [40.0, 80.0, 72.0, 36.0]);
        let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
        note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
    }
    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[3] - 334.0).abs() < 0.5,
        "backslash identity must match slash tid; got {:?}",
        border
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_size_hint_rejects_stale_path_without_identity() {
    // Screenshot: Styles shows 1b card 164×334 while absolute path drifted to Outline.
    // No usable identity — size hint must still find the card.
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut card = Element::new().p(16.0);
    card.layout_bounds = [20.0, 360.0, 164.0, 334.0];
    let mut outline = Element::new().template_id("outline-btn");
    outline.layout_bounds = [40.0, 80.0, 72.0, 36.0];

    let mut viewport = Element::new().children(vec![outline, card]);
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 40.0]);

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut root = Element::new().child(host);

    DevToolsState::set_hover_highlight(vec![0, 0, 0], String::new(), Vec::new(), (164.0, 334.0));

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let outline = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        note_element_paint_visual(outline, [40.0, 40.0, 72.0, 36.0]);
        let card = find_element_by_path_mut(&mut root, &[0, 0, 1]).unwrap();
        note_element_paint_visual(card, [20.0, 320.0, 164.0, 334.0]);
    }

    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[1] - 320.0).abs() < 0.5 && (border[3] - 334.0).abs() < 0.5,
        "size hint must reject Outline path; got {:?}",
        border
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_rejects_unscrolled_paint_visual_when_ancestors_scrolled() {
    // Narrow+scroll repro: paint_visual_gen is current but bounds equal raw abs
    // (scroll_acc was 0 when noted). Stamp must use abs−scroll, not the stale paint.
    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    let mut target = Element::new()
        .focus_id("btn-default")
        .template_id("btn-default");
    target.layout_bounds = [20.0, 280.0, 140.0, 36.0];

    let mut viewport = Element::new().child(target);
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = taffy::style::Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 120.0]);
    viewport.layout_bounds = [0.0, 0.0, 400.0, 500.0];

    let mut host = Element::new().children(vec![viewport, highlight_slot()]);
    host.template_id = Some("__jadix_app_host".into());
    host.layout_bounds = [0.0, 0.0, 400.0, 500.0];
    let mut root = Element::new().child(host);

    DevToolsState::set_hover_highlight(
        vec![0, 0, 0],
        "btn-default".to_string(),
        Vec::new(),
        (140.0, 36.0),
    );

    begin_highlight_collect();
    set_highlight_collect_root(&root);
    {
        let t = find_element_by_path_mut(&mut root, &[0, 0, 0]).unwrap();
        // Bug shape: gen current, but Y is raw abs (280) instead of visual (160).
        note_element_paint_visual(t, [20.0, 280.0, 140.0, 36.0]);
    }
    let slot = find_element_by_template_mut(&mut root, HIGHLIGHT_SLOT_ID).unwrap();
    slot.layout_bounds = [0.0, 0.0, 400.0, 500.0];
    assert!(refresh_highlight_slot(slot));
    let border = stamped_highlight_abs_rect(&slot.children[2]).expect("border");
    assert!(
        (border[1] - 160.0).abs() < 0.5,
        "must prefer abs−scroll (160) over unscrolled paint (280); got {:?}",
        border
    );

    clear_highlight_collect_root();
    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_matches_paint_under_narrow_wrap_and_scroll() {
    // Gallery repro (user-confirmed): docked Inspector squeezes host → HStack wrap
    // stacks buttons; ScrollArea offset > 0. Highlight of the first button must track
    // its painted instance — not drift toward lower siblings (Disabled / "Selected variant").
    use crate::app::devtools::panel::wrap_app;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_panel_width(360.0);

    create_root(|| {
        let mk_btn = |id: &str, label: &str, color: [f32; 4]| {
            Element::new()
                .focus_id(id)
                .template_id(id)
                .w(140.0)
                .h(36.0)
                .flex_shrink(0.0)
                .bg(color)
                .text(label)
        };
        let row = Element::new()
            .w("100%")
            .min_w(0.0)
            .flex_row()
            .flex_wrap_kw("wrap")
            .gap(8.0)
            .children(vec![
                mk_btn("btn-default", "Default", [0.1, 0.1, 0.1, 1.0]),
                mk_btn("btn-secondary", "Secondary", [0.3, 0.3, 0.3, 1.0]),
                mk_btn("btn-outline", "Outline", [0.5, 0.5, 0.5, 1.0]),
                mk_btn("btn-ghost", "Ghost", [0.6, 0.6, 0.6, 1.0]),
                mk_btn("btn-destructive", "Destructive", [0.8, 0.2, 0.2, 1.0]),
                mk_btn("btn-link", "Link", [0.2, 0.4, 0.8, 1.0]),
                mk_btn("btn-disabled", "Disabled", [0.7, 0.7, 0.7, 1.0]),
            ]);
        let label = Element::new()
            .w("100%")
            .h(20.0)
            .flex_shrink(0.0)
            .text("Selected variant: default");
        let spacer = Element::new().w("100%").h(400.0).flex_shrink(0.0);
        let col = Element::new()
            .flex_col()
            .w("100%")
            .flex_shrink(0.0)
            .gap(8.0)
            .p(16.0)
            .children(vec![row, label, spacer]);

        // Real ScrollArea (signal-backed offset) — gallery uses this, not a bare overflow node.
        let scroll: Element = crate::app::devtools::controls::scroll_area("gallery-narrow")
            .w("100%")
            .h("100%")
            .overflow_x("auto")
            .overflow_y("auto")
            .child(col)
            .into();

        let app = Element::new().w("100%").h("100%").min_w(0.0).child(scroll);
        let mut root = wrap_app(app);
        let _ = crate::app::devtools::panel::sync_overlay(&mut root);

        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);
        let _ = layout.build_and_layout(&mut root, 520.0, 420.0, [-1.0, -1.0]);

        // Drive scroll like the runtime prepass (signal → element).
        let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key("gallery-narrow");
        crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).set(120.0);
        fn apply_sy(el: &mut Element) {
            if el.focus_id.as_deref() == Some("scroll:gallery-narrow") {
                el.scroll_offset[1] = 120.0;
            }
            for c in &mut el.children {
                apply_sy(c);
            }
        }
        apply_sy(&mut root);

        // Stale snapshot identity (remounted auto btn-id) + size of a button.
        DevToolsState::set_hover_highlight(
            vec![0, 0, 0, 0, 0, 0, 0],
            "btn-default".to_string(),
            Vec::new(),
            (140.0, 36.0),
        );

        let result =
            layout.build_and_layout_fast(&mut root, 520.0, 420.0, [-1.0, -1.0], true, false);

        let btn = crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
            .expect("btn-default");
        let btn_path = path_of_element(&root, btn).expect("path of btn-default");
        let paint = result.instances.iter().find(|i| {
            (i.rect_size[0] - btn.layout_bounds[2]).abs() < 2.0
                && (i.rect_size[1] - btn.layout_bounds[3]).abs() < 2.0
                && (i.color[0] - 0.1).abs() < 0.05
        });
        let paint_y = paint.map(|i| i.rect_pos[1]);
        let paint_x = paint.map(|i| i.rect_pos[0]);

        let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
        let border = &slot.children[2];
        let got_y = border.layout_bounds[1];
        let got_x = border.layout_bounds[0];

        let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &btn_path);
        let expect_y = btn.layout_bounds[1] - acc[1];
        let expect_x = btn.layout_bounds[0] - acc[0];

        assert!(
                (got_y - expect_y).abs() < 2.0,
                "narrow+wrap+scroll: highlight y={got_y} expect abs-scroll={expect_y} abs={} acc={:?} paint_y={paint_y:?} path={btn_path:?}",
                btn.layout_bounds[1],
                acc,
            );
        if let Some(py) = paint_y {
            assert!(
                    (got_y - py).abs() < 2.0,
                    "highlight y={got_y} must match first-button paint y={py} (got_x={got_x}, paint_x={paint_x:?})"
                );
        }
        assert!(
            (got_x - expect_x).abs() < 2.0,
            "highlight x={got_x} expect {expect_x}"
        );
    });

    DevToolsState::clear_hover_highlight();
}

#[test]
fn clip_cull_after_narrow_wrap_refreshes_descendant_bounds_for_highlight() {
    // User report: after content squeeze, highlight mixed new wrap layout with old abs.
    // Clip-cull used to leave nested buttons at pre-wrap layout_bounds; a later
    // scroll_only band-skip then stamped paint_visual = old_abs − new_scroll.
    use crate::app::devtools::panel::wrap_app;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);
    DevToolsState::set_panel_width(360.0);

    create_root(|| {
        let mk_btn = |id: &str, color: [f32; 4]| {
            Element::new()
                .focus_id(id)
                .template_id(id)
                .w(140.0)
                .h(36.0)
                .flex_shrink(0.0)
                .bg(color)
        };
        let row = Element::new()
            .w("100%")
            .min_w(0.0)
            .flex_row()
            .flex_wrap_kw("wrap")
            .gap(8.0)
            .children(vec![
                mk_btn("btn-default", [0.1, 0.1, 0.1, 1.0]),
                mk_btn("btn-second", [0.3, 0.3, 0.3, 1.0]),
            ]);
        // Push the wrap row far below the short viewport so full collect clip-culls it.
        let spacer = Element::new().w("100%").h(500.0).flex_shrink(0.0);
        let col = Element::new()
            .flex_col()
            .w("100%")
            .flex_shrink(0.0)
            .gap(8.0)
            .p(8.0)
            .children(vec![spacer, row]);

        let scroll: Element = crate::app::devtools::controls::scroll_area("gallery-cull")
            .w("100%")
            .h("100%")
            .overflow_x("auto")
            .overflow_y("auto")
            .child(col)
            .into();

        let app = Element::new().w("100%").h("100%").min_w(0.0).child(scroll);
        let mut root = wrap_app(app);
        let _ = crate::app::devtools::panel::sync_overlay(&mut root);

        let mut layout = LayoutTree::new();
        let _ = layout.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);

        let wide_second =
            crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-second")
                .expect("btn-second wide");
        let wide_x = wide_second.layout_bounds[0];
        assert!(
            wide_x > 100.0,
            "precondition: wide layout places second button to the right (x={wide_x})"
        );

        // Squeeze + short host: row wraps AND sits below the clip → clip-cull path.
        let _ = layout.build_and_layout(&mut root, 520.0, 200.0, [-1.0, -1.0]);

        let second = crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-second")
            .expect("btn-second narrow");
        let default =
            crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
                .expect("btn-default narrow");
        assert!(
            (second.layout_bounds[0] - default.layout_bounds[0]).abs() < 4.0,
            "clip-cull after wrap must refresh descendant abs from Taffy (stacked), \
                 not keep wide-row x (default={:?} second={:?} wide_x={wide_x})",
            default.layout_bounds,
            second.layout_bounds,
        );
        assert!(
            second.layout_bounds[1] > default.layout_bounds[1] + 20.0,
            "wrapped second button must sit below default after cull sync"
        );

        // scroll_only band-skip while deep scrolled: highlight must track abs−scroll.
        let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key("gallery-cull");
        crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).set(480.0);
        fn apply_sy(el: &mut Element) {
            if el.focus_id.as_deref() == Some("scroll:gallery-cull") {
                el.scroll_offset[1] = 480.0;
            }
            for c in &mut el.children {
                apply_sy(c);
            }
        }
        apply_sy(&mut root);

        DevToolsState::set_hover_highlight(
            vec![0, 0, 0, 0, 0, 0, 0],
            "btn-default".to_string(),
            Vec::new(),
            (140.0, 36.0),
        );

        let _ = layout.build_and_layout_fast(&mut root, 520.0, 200.0, [-1.0, -1.0], true, false);

        let btn = crate::app::devtools::snapshot::find_element_by_identity(&root, "btn-default")
            .expect("btn-default");
        let btn_path = path_of_element(&root, btn).expect("path");
        let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &btn_path);
        let expect_y = btn.layout_bounds[1] - acc[1];
        let expect_x = btn.layout_bounds[0] - acc[0];

        let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
        let border = &slot.children[2];
        assert!(
            (border.layout_bounds[1] - expect_y).abs() < 2.0,
            "highlight y={} expect abs-scroll {} (abs={} acc={:?})",
            border.layout_bounds[1],
            expect_y,
            btn.layout_bounds[1],
            acc,
        );
        assert!(
            (border.layout_bounds[0] - expect_x).abs() < 2.0,
            "highlight x={} expect {}",
            border.layout_bounds[0],
            expect_x,
        );
    });

    DevToolsState::clear_hover_highlight();
}

#[test]
fn highlight_collect_matches_target_instance_under_docked_scroll() {
    use crate::app::devtools::panel::wrap_app;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    DevToolsState::enable_for_window();
    DevToolsState::set_open(true);

    create_root(|| {
        // wrap_app: host[app, highlight] + panel. App is a tall scrolled column.
        let target = Element::new()
            .w(120.0)
            .h(40.0)
            .bg([0.2, 0.4, 0.8, 1.0])
            .template_id("target-btn")
            .focus_id("target-btn");
        // Leave room above so scrolling moves the button.
        let spacer = Element::new().w(120.0).h(300.0).flex_shrink(0.0);
        let col = Element::new()
            .flex_col()
            .w(200.0)
            .flex_shrink(0.0)
            .children(vec![spacer, target]);

        let mut viewport = Element::new()
            .w("100%")
            .h("100%")
            .overflow_y("scroll")
            .focus_id("scroll:gallery-main")
            .child(col);
        crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 180.0]);

        let app = Element::new().w("100%").h("100%").child(viewport);
        let mut root = wrap_app(app);

        // Path: host(0) → app(0) → viewport(0) → col(0) → target(1)
        let path = vec![0, 0, 0, 0, 1];
        DevToolsState::set_hover_highlight_path(path.clone());

        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 1000.0, 600.0, [-1.0, -1.0]);

        let target_el = find_element_by_path(&root, &path).expect("target");
        let target_bounds = target_el.layout_bounds;
        let acc = crate::app::devtools::snapshot::scroll_acc_along_path(&root, &path);
        let expect_y = target_bounds[1] - acc[1];

        // Highlight content layer should have been stamped during collect.
        let slot = find_element_by_template(&root, HIGHLIGHT_SLOT_ID).expect("slot");
        let content = &slot.children[4];
        let stamped = stamped_highlight_abs_rect(content);
        // Cache is cleared after collect — read layout_bounds stamped onto the layer instead.
        let got_y = content.layout_bounds[1];
        let got_h = content.layout_bounds[3];

        // Also find a painted instance near the target color / size.
        let target_inst = result.instances.iter().find(|i| {
            (i.rect_size[0] - 120.0).abs() < 1.0
                && (i.rect_size[1] - 40.0).abs() < 1.0
                && i.color[2] > 0.5
        });
        let paint_y = target_inst.map(|i| i.rect_pos[1]);

        assert!(
                (got_y - expect_y).abs() < 1.5,
                "highlight y={got_y} expect visual y={expect_y} (abs={}, acc={:?}, stamped={:?}, paint_y={paint_y:?})",
                target_bounds[1],
                acc,
                stamped,
            );
        if let Some(py) = paint_y {
            assert!(
                (got_y - py).abs() < 1.5,
                "highlight y={got_y} must match target paint y={py} (h={got_h})"
            );
        }
    });

    DevToolsState::clear_hover_highlight();
}

#[test]
fn scroll_offset_centers_row_and_clamps() {
    // Viewport 100, content 400, row at 200..220 → center at 210 → scroll 160.
    assert!((scroll_offset_to_center_rect(200.0, 20.0, 100.0, 400.0) - 160.0).abs() < 0.01);
    // Near top clamps to 0.
    assert_eq!(scroll_offset_to_center_rect(0.0, 20.0, 100.0, 400.0), 0.0);
    // Near bottom clamps to max.
    let max = 300.0;
    assert!((scroll_offset_to_center_rect(380.0, 20.0, 100.0, 400.0) - max).abs() < 0.01);
}
