use super::*;
use crate::renderer::InstanceInput;
use crate::style::TextStyle;

#[test]
fn dirty_slot_range_walk_matches_glyph_ownership_with_recycled_keys() {
    let text: Vec<_> = (0..80)
        .map(|i| sample_text("caption", i as f32 * 24.0))
        .collect();
    let mut paints: Vec<_> = (0..80)
        .map(|i| {
            slot(
                (i % 17) as u64,
                [i as f32 * 24.0, 0.0, 20.0, 18.0],
                (i, i + 1),
            )
        })
        .collect();
    // A broad ancestor with a recycled key must still require intersection.
    paints.push(slot(4, [0.0, 0.0, 1920.0, 100.0], (0, 80)));
    for key in 0..22 {
        for x in [0.0, 96.0, 504.0, 1800.0] {
            let dirty = PaintDirty {
                key,
                bounds: [x, 0.0, 38.0, 20.0],
                kind: PaintDirtyKind::Node,
            };
            let expected: BTreeSet<_> = text
                .iter()
                .enumerate()
                .filter(|(i, n)| glyph_belongs_to_dirty(*i, n, &dirty, &text, &paints))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                node_present_glyphs_for_dirty(&dirty, &text, &paints),
                expected
            );
        }
    }
}

fn blank_instance(x: f32, y: f32, w: f32, h: f32) -> InstanceInput {
    InstanceInput {
        rect_pos: [x, y],
        rect_size: [w, h],
        color: [1.0, 0.0, 0.0, 1.0],
        border_color: [0.0; 4],
        radii: [0.0; 4],
        border_width: 0.0,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0, 0.0],
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_inset: 0.0,
        backdrop_fade: 0.0,
        transform: MAT2D_IDENTITY,
        clip_pos: [0.0, 0.0],
        clip_size: InstanceInput::NO_CLIP_SIZE,
        border_extra: 0.0,
        backdrop_blur: 0.0,
        glass: [0.0; 4],
    }
}

#[test]
fn cull_instance_bounds_match_per_rect_checks() {
    let plain = blank_instance(10.0, 20.0, 40.0, 30.0);
    let mut transformed = blank_instance(90.0, 30.0, 36.0, 28.0);
    transformed.transform = [0.8, 0.6, -0.6, 0.8, 4.0, -3.0];
    transformed.shadow_blur = 8.0;
    transformed.shadow_offset = [2.0, 5.0];
    let mut clipped = blank_instance(170.0, 40.0, 48.0, 36.0);
    clipped.clip_pos = [180.0, 32.0];
    clipped.clip_size = [24.0, 42.0];
    let instances = [plain, transformed, clipped];
    let culls = [
        CullRects::empty(),
        CullRects::one([12.0, 22.0, 3.0, 4.0]),
        CullRects::from_rects([
            [0.0, 0.0, 5.0, 5.0],
            [120.0, 40.0, 8.0, 8.0],
            [196.0, 45.0, 4.0, 4.0],
        ])
        .unwrap(),
    ];
    for cull in culls {
        for instance in &instances {
            let reference = cull.strips().any(|r| instance_hits_damage(instance, r));
            assert_eq!(cull.hits_instance(instance), reference);
        }
    }
}

fn blank_textured(x: f32, y: f32, w: f32, h: f32, transform: [f32; 6]) -> ScaledTexturedDraw {
    ScaledTexturedDraw {
        rect: [x, y, w, h],
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_key: String::new(),
        opacity: 1.0,
        tint_rgb: [1.0, 1.0, 1.0],
        transform,
        clip_pos: [0.0, 0.0],
        clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
        rounded_clips: [[0.0; 4]; 4],
    }
}

#[test]
fn joined_border_fringe_keeps_local_repairs_but_transformed_ink_is_ordered() {
    let mut first = slot(1, [0.0, 22.0, 36.0, 36.0], (0, 1));
    first.rest = Some(first.bounds);
    first.instance = (0, 1);
    first.paint_ops = (0, 1);
    let mut second = slot(2, [35.0, 22.0, 37.0, 36.0], (1, 2));
    second.rest = Some([36.0, 22.0, 36.0, 36.0]);
    second.instance = (1, 2);
    second.paint_ops = (1, 2);
    assert!(!slot_requires_ordered_replay(
        1,
        first.bounds,
        &[first.clone(), second.clone()]
    ));
    second.bounds = [28.0, 22.0, 44.0, 36.0];
    assert!(slot_requires_ordered_replay(
        1,
        first.bounds,
        &[first, second]
    ));
}

#[test]
fn keyed_thumb_region_does_not_absorb_wide_neighbor() {
    let mut thumb = slot(7, [10.0, 10.0, 16.0, 16.0], (0, 0));
    thumb.instance = (2, 3);
    let mut wide = slot(9, [0.0, 8.0, 480.0, 28.0], (0, 0));
    wide.instance = (1, 2);
    let paints = [wide, thumb];
    let instances = [
        blank_instance(0.0, 0.0, 500.0, 50.0),
        blank_instance(0.0, 8.0, 480.0, 28.0),
        blank_instance(10.0, 10.0, 16.0, 16.0),
    ];
    let mut dirties = vec![
        PaintDirty {
            key: 7,
            bounds: [6.0, 6.0, 24.0, 24.0],
            kind: PaintDirtyKind::Region,
        },
        PaintDirty {
            key: 9,
            bounds: [0.0, 8.0, 480.0, 28.0],
            kind: PaintDirtyKind::Node,
        },
    ];
    prepare_node_present_dirties(&mut dirties, &paints, &[], &instances, &[], &[], 1.0, None);
    let thumb_dirty = dirties.iter().find(|d| d.key == 7).unwrap();
    assert!(
        region_replays_own_chrome_only(7, thumb_dirty.bounds, &paints),
        "thumb hole must stay a local chrome replay, bounds={:?}",
        thumb_dirty.bounds
    );
    assert_eq!(
        dirties.iter().find(|d| d.key == 9).unwrap().kind,
        PaintDirtyKind::Node,
        "wide track must not be promoted into a page Region"
    );
}

#[test]
fn trigger_in_popover_shadow_stays_node_not_region() {
    // Panel sits above the trigger with a 4px gap. Shadow blur ~10 overlaps
    // the button face. That intersection used to escalate every
    // transition-colors tick to Region (popover reshape + shadow src-over).
    let button = [100.0, 160.0, 160.0, 32.0];
    let mut page = slot(1, [0.0, 0.0, 400.0, 400.0], (0, 0));
    page.instance = (0, 1);
    let mut trigger = slot(4, button, (0, 0));
    trigger.instance = (1, 2);
    let instances = [
        blank_instance(0.0, 0.0, 400.0, 400.0),
        blank_instance(button[0], button[1], button[2], button[3]),
    ];
    let stamp = (7u64, [100.0, 100.0, 180.0, 56.0]);
    let occ = [88.0, 88.0, 204.0, 80.0];
    assert!(
        intersect_rects(button, occ).is_some(),
        "fixture must overlap the shadow"
    );
    assert!(
        intersect_rects(button, stamp.1).is_none(),
        "fixture must not cover the panel box"
    );
    let mut dirties = vec![PaintDirty {
        key: 4,
        bounds: button,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties_ex(
        &mut dirties,
        &[page, trigger],
        &[],
        &instances,
        &[],
        &[],
        1.0,
        Some(occ),
        &[stamp],
    );
    assert_eq!(
        dirties[0].kind,
        PaintDirtyKind::Node,
        "shadow fringe must stay Node so hover does not replay the popover\n {dirties:?}"
    );
}

#[test]
fn page_control_under_modal_scrim_still_region() {
    let button = [20.0, 200.0, 140.0, 32.0];
    let mut page = slot(1, [0.0, 0.0, 800.0, 600.0], (0, 0));
    page.instance = (0, 1);
    let mut trigger = slot(4, button, (0, 0));
    trigger.instance = (1, 2);
    let instances = [
        blank_instance(0.0, 0.0, 800.0, 600.0),
        blank_instance(button[0], button[1], button[2], button[3]),
    ];
    let stamp = (7u64, [400.0, 40.0, 120.0, 80.0]);
    let scrim = [0.0, 0.0, 800.0, 600.0];
    let mut dirties = vec![PaintDirty {
        key: 4,
        bounds: button,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties_ex(
        &mut dirties,
        &[page, trigger],
        &[],
        &instances,
        &[],
        &[],
        1.0,
        Some(scrim),
        &[stamp],
    );
    assert_eq!(
        dirties[0].kind,
        PaintDirtyKind::Region,
        "a control under the scrim, outside the panel shadow, must still rebuild\n {dirties:?}"
    );
}

#[test]
fn in_flow_layered_cell_does_not_window_clear() {
    let mut cell = slot(3, [48.0, 80.0, 36.0, 36.0], (0, 0));
    cell.layered = true;
    cell.instance = (0, 1);
    cell.rest = Some(cell.bounds);
    let mut page = slot(1, [0.0, 0.0, 320.0, 480.0], (0, 0));
    page.instance = (0, 1);
    cell.instance = (1, 2);
    let instances = [
        blank_instance(0.0, 0.0, 320.0, 480.0),
        blank_instance(48.0, 80.0, 36.0, 36.0),
    ];
    let mut dirties = vec![PaintDirty {
        key: 3,
        bounds: cell.bounds,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut dirties,
        &[page, cell],
        &[(3, [48.0, 80.0, 36.0, 36.0])],
        &instances,
        &[],
        &[],
        1.0,
        None,
    );
    assert_eq!(
        dirties[0].kind,
        PaintDirtyKind::Node,
        "in-flow cell must dest-reset its quad, not window-clear a rectangle\n {dirties:?}"
    );
    assert!(
        dirties[0].bounds[3] <= 48.0,
        "cell dirty grew into a column\n {:?}",
        dirties[0].bounds
    );
}

#[test]
fn separated_layered_pose_does_not_clear_the_gap() {
    let live = [48.0, 80.0, 36.0, 36.0];
    let stale = [48.0, 420.0, 36.0, 36.0];
    let mut cell = slot(3, live, (0, 0));
    cell.layered = true;
    cell.rest = Some(live);
    let mut dirties = vec![PaintDirty {
        key: 3,
        bounds: live,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut dirties,
        &[cell],
        &[(3, stale)],
        &[],
        &[],
        &[],
        1.0,
        None,
    );
    let gap_y = 250.0;
    assert!(
        dirties
            .iter()
            .all(|d| { gap_y < d.bounds[1] || gap_y > d.bounds[1] + d.bounds[3] }),
        "gap between the live cell and its last pose was cleared\n {dirties:?}"
    );
    assert!(
        dirties.iter().any(|d| aabb_covers(d.bounds, stale)),
        "the leftover pose was not erased\n {dirties:?}"
    );
}

#[test]
fn select_trigger_region_recaptures_child_caption() {
    // The trigger button owns the fill. SelectValue is a child slot, so
    // own-chrome replay covers the caption unless capture puts it back.
    let mut trigger = slot(1, [0.0, 0.0, 240.0, 32.0], (0, 0));
    trigger.instance = (0, 1);
    let caption = slot(2, [8.0, 8.0, 64.0, 16.0], (0, 1));
    let paints = [trigger, caption];
    let mut label = sample_text("Basic", 10.0);
    label.y = 8.0;
    label.width = 40.0;
    label.height = 16.0;
    label.paint_origin = [10.0, 8.0];
    let dirty = PaintDirty {
        key: 1,
        bounds: [0.0, 0.0, 240.0, 32.0],
        kind: PaintDirtyKind::Region,
    };
    assert!(region_replays_own_chrome_only(1, dirty.bounds, &paints));
    assert!(region_needs_glyph_capture(
        &dirty,
        &paints,
        &[label.clone()]
    ));
    // Collect records the caption inside the trigger's own text range, so
    // own-chrome is false. The Region clear still has to recapture it.
    let mut owned = slot(3, [0.0, 0.0, 240.0, 32.0], (0, 1));
    owned.instance = (0, 1);
    let owned_dirty = PaintDirty {
        key: 3,
        bounds: [0.0, 0.0, 240.0, 32.0],
        kind: PaintDirtyKind::Region,
    };
    assert!(!region_replays_own_chrome_only(
        3,
        owned_dirty.bounds,
        &[owned]
    ));
    assert!(region_needs_glyph_capture(&owned_dirty, &[owned], &[label]));
    let mut thumb = slot(7, [10.0, 40.0, 16.0, 16.0], (0, 0));
    thumb.instance = (0, 1);
    let thumb_dirty = PaintDirty {
        key: 7,
        bounds: [8.0, 38.0, 20.0, 20.0],
        kind: PaintDirtyKind::Region,
    };
    assert!(!region_needs_glyph_capture(
        &thumb_dirty,
        &[thumb],
        &[sample_text("Basic", 10.0)],
    ));
}

#[test]
fn parent_chrome_prefix_stops_before_descendant_quads() {
    let mut parent = slot(1, [0.0, 0.0, 400.0, 300.0], (0, 0));
    parent.instance = (0, 4);
    let instances = [
        blank_instance(0.0, 0.0, 400.0, 300.0),
        blank_instance(10.0, 10.0, 16.0, 16.0),
        blank_instance(40.0, 10.0, 16.0, 16.0),
        blank_instance(70.0, 10.0, 16.0, 16.0),
    ];
    let (lo, hi) = slot_own_chrome_range(&parent, &instances, 1.0);
    assert_eq!((lo, hi), (0, 1));
}

#[test]
fn parent_texture_run_skips_descendant_range() {
    let mut parent = slot(1, [0.0, 0.0, 400.0, 300.0], (0, 0));
    parent.textured = (0, 5);
    let mut child = slot(2, [10.0, 10.0, 20.0, 8.0], (0, 0));
    child.textured = (1, 4);
    let paints = [parent, child];
    let mut runs = Vec::new();
    for_each_own_textured_run(&paints[0], &paints, 5, |a, b| runs.push((a, b)));
    assert_eq!(runs, vec![(0, 1), (4, 5)]);
}

#[test]
fn absolute_track_without_text_stays_local_chrome() {
    let mut track = slot(3, [0.0, 4.0, 280.0, 8.0], (0, 0));
    track.layered = true;
    track.textured = (0, 1);
    assert!(region_replays_own_chrome_only(3, track.bounds, &[track]));
}

#[test]
fn solid_chart_indicator_over_raster_requires_ordered_replay() {
    let mut line = slot(3, [100.0, 20.0, 1.0, 180.0], (0, 0));
    line.layered = true;
    line.instance = (1, 2);
    let mut chart = slot(2, [20.0, 20.0, 280.0, 180.0], (0, 0));
    chart.textured = (0, 1);
    assert!(!region_replays_own_chrome_only(
        3,
        line.bounds,
        &[chart, line]
    ));
    chart.overflow_clip = Some([20.0, 20.0, 60.0, 180.0]);
    assert!(region_replays_own_chrome_only(
        3,
        line.bounds,
        &[chart, line]
    ));
}

#[test]
fn far_sibling_instance_is_culled() {
    let a = blank_instance(0.0, 0.0, 20.0, 20.0);
    let b = blank_instance(400.0, 0.0, 20.0, 20.0);
    let damage = [0.0, 0.0, 40.0, 40.0];
    assert!(instance_hits_damage(&a, damage));
    assert!(!instance_hits_damage(&b, damage));
    let runs = visible_instance_runs(&[a, b], 0, 2, CullRects::one(damage));
    assert_eq!(runs, vec![(0, 1)]);
}

#[test]
fn outset_shadow_keeps_instance_when_box_is_outside() {
    let mut i = blank_instance(50.0, 0.0, 10.0, 10.0);
    i.shadow_offset = [-40.0, 0.0];
    i.shadow_blur = 8.0;
    i.shadow_color = [0.0, 0.0, 0.0, 1.0];
    let damage = [0.0, 0.0, 20.0, 20.0];
    assert!(
        instance_hits_damage(&i, damage),
        "drop-shadow must inflate cull bounds"
    );
}

#[test]
fn instance_paint_bounds_covers_sdf_blur_twice() {
    // Select menu Elevation::Lg blur=12. Shader ext is blur*2+2; 1× blur
    // occupancy left the halo on the calendar after dest-reset of the fill.
    let mut i = blank_instance(100.0, 100.0, 40.0, 160.0);
    i.shadow_blur = 12.0;
    i.shadow_color = [0.0, 0.0, 0.0, 0.14];
    let b = instance_paint_bounds(&i);
    let pad = sdf_outset_shadow_pad(12.0, 0.0, [0.0, 0.0], 0.0);
    assert!(pad >= 26.0, "SDF ext is blur*2+2 = 26, got {pad}");
    assert!(
        b[0] <= 100.0 - 24.0 && b[1] <= 100.0 - 24.0,
        "occupancy must cover the SDF halo, got {b:?}"
    );
    assert!(b[2] >= 40.0 + 48.0 && b[3] >= 160.0 + 48.0);
}

#[test]
fn instance_paint_bounds_rotated_keeps_diamond_outside_layout_clip() {
    let mut i = blank_instance(0.0, 0.0, 80.0, 80.0);
    i.transform = crate::style::mat2d_world_center_pivot(
        &MAT2D_IDENTITY,
        &crate::style::mat2d_rotate(45.0_f32.to_radians()),
        [40.0, 40.0],
        [80.0, 80.0],
    );
    i.clip_pos = [0.0, 0.0];
    i.clip_size = [80.0, 80.0];
    let b = instance_paint_bounds(&i);
    assert!(
        b[1] < -8.0 && b[0] < -8.0 && b[2] > 90.0 && b[3] > 90.0,
        "world AABB of the 45° quad must keep diamond tips, not layout clip [0,0,80,80], got {b:?}"
    );
    let mut identity = blank_instance(0.0, 0.0, 80.0, 80.0);
    identity.clip_pos = [8.0, 8.0];
    identity.clip_size = [40.0, 40.0];
    let clipped = instance_paint_bounds(&identity);
    assert!(
        (clipped[0] - 8.0).abs() < 0.01 && (clipped[2] - 40.0).abs() < 0.01,
        "identity dest-reset still intersects layout clip, got {clipped:?}"
    );
}

#[test]
fn middle_gap_splits_runs() {
    let inst = [
        blank_instance(0.0, 0.0, 10.0, 10.0),
        blank_instance(200.0, 0.0, 10.0, 10.0),
        blank_instance(8.0, 0.0, 10.0, 10.0),
    ];
    let damage = [0.0, 0.0, 30.0, 30.0];
    assert_eq!(
        visible_instance_runs(&inst, 0, 3, CullRects::one(damage)),
        vec![(0, 1), (2, 1)]
    );
}

#[test]
fn complete_region_repair_preserves_unclipped_glyph_overhang() {
    let text = sample_text("secondary", 20.0);
    assert!(text.clip_bounds.is_none());
    let runs = clip_text_nodes_to_cull(&[text], CullRects::one([-10.0, -10.0, 300.0, 200.0]));
    assert_eq!(runs.len(), 1);
    assert!(runs[0].clip_bounds.is_none());
}

#[test]
fn fractional_repair_text_covers_every_cleared_pixel() {
    let physical = [415.0, 467.0, 450.0, 493.0];
    for scale in [1.25, 1.5, 1.75, 2.0] {
        let cull = physical_repair_text_cull(CullRects::one(physical), scale);
        let mut text = sample_text("repair edge", 0.0);
        text.width = 2000.0;
        text.height = 2000.0;
        let runs = clip_text_nodes_to_cull(&[text], cull);
        let clip = runs[0].clip_bounds.unwrap().map(|value| value * scale);
        let snapped = crate::renderer::prepare_text::integerize_clip_rect(clip).unwrap();
        assert!(
            crate::app::damage::rect_contains_rect(snapped, physical),
            "scale={scale} clip={snapped:?}"
        );
    }
}

fn sample_text(text: &str, x: f32) -> TextLayoutInfo {
    TextLayoutInfo {
        text: text.into(),
        shape_suffix: None,
        spans: None,
        template_id: None,
        x,
        y: 0.0,
        width: 20.0,
        height: 16.0,
        clip_bounds: None,
        style: TextStyle::default(),
        prepared: None,
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: MAT2D_IDENTITY,
        paint_origin: [x, 0.0],
        glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
    }
}

#[test]
fn panned_input_tail_stays_in_selection_repaint() {
    let mut text = sample_text("long input tail", -300.0);
    text.width = 120.0;
    text.style.overflow = crate::style::TextOverflow::editable_single_line();
    text.style.scroll_x = 400.0;
    text.clip_bounds = Some([100.0, 0.0, 120.0, 16.0]);
    assert_eq!(text_paint_bounds(&text), [100.0, 0.0, 120.0, 16.0]);
    assert!(text_hits_damage(&text, [100.0, 0.0, 120.0, 16.0]));
}

#[test]
fn selection_capture_kernel_stays_inside_text_scrollport() {
    let viewport = [12.0, 0.0, 176.0, 16.0];
    let mut text = sample_text("long overflow", -300.0);
    text.width = 600.0;
    text.clip_bounds = Some(viewport);
    let mut paint = slot(71, [-300.0, 0.0, 600.0, 16.0], (0, 1));
    paint.overflow_clip = Some(viewport);
    let dirty = PaintDirty {
        key: 71,
        bounds: viewport,
        kind: PaintDirtyKind::Region,
    };
    let captured = node_present_clipped_capture_text_wipes(&[dirty], &[text], &[paint], &[]);
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].clip_bounds, Some(viewport));
}

#[test]
fn opaque_fill_occlusion_respects_world_clip_at_ui_scale() {
    for scale in [1.0, 1.5, 2.0] {
        let mut fill = blank_instance(0.0, 0.0, 200.0 * scale, 20.0 * scale);
        fill.color = [0.0, 0.0, 0.0, 1.0];
        fill.transform[4] = 100.0 * scale;
        fill.clip_pos = [120.0 * scale, 0.0];
        fill.clip_size = [40.0 * scale, 20.0 * scale];
        assert_eq!(
            logical_opaque_fill_aabb(&fill, scale),
            Some([120.0, 0.0, 40.0, 20.0])
        );
        fill.clip_pos[0] = 320.0 * scale;
        assert_eq!(logical_opaque_fill_aabb(&fill, scale), None);
    }
}

#[test]
fn lower_layer_fill_does_not_occlude_higher_glyph_capture() {
    let lower_bounds = [140.0, 0.0, 60.0, 20.0];
    let upper_bounds = [110.0, 0.0, 60.0, 20.0];
    let mut lower = slot(1, lower_bounds, (0, 1));
    lower.layered = true;
    lower.instance = (0, 1);
    let mut upper = slot(2, upper_bounds, (1, 2));
    upper.layered = true;
    upper.instance = (1, 2);
    let mut ancestor = slot(3, [0.0, 0.0, 240.0, 40.0], (0, 2));
    ancestor.instance = (0, 2);
    let mut fills = [
        blank_instance(140.0, 0.0, 60.0, 20.0),
        blank_instance(110.0, 0.0, 60.0, 20.0),
    ];
    for fill in &mut fills {
        fill.color = [1.0; 4];
    }
    let text = [sample_text("B", 140.0), sample_text("A", 110.0)];
    let paints = [lower, upper, ancestor];
    assert_eq!(
        clip_minus_foreign_opaque_plates(1, upper_bounds, &text, &paints, &fills, 1.0),
        vec![upper_bounds],
        "lower B must not cut the higher A, including through ancestor ranges"
    );
    assert_eq!(
        clip_minus_foreign_opaque_plates(0, lower_bounds, &text, &paints, &fills, 1.0),
        vec![[170.0, 0.0, 30.0, 20.0]],
        "higher A must still cover the lower B"
    );
}

#[test]
fn unrelated_portal_does_not_preserve_moving_page_slot() {
    let bounds = [100.0, 0.0, 176.0, 80.0];
    let dirty = PaintDirty {
        key: 1,
        bounds,
        kind: PaintDirtyKind::Region,
    };
    for portal in [[86.0, 422.0, 264.0, 168.0], [100.0, 40.0, 100.0, 100.0]] {
        let mut paint = slot(1, [200.0, 0.0, 76.0, 80.0], (0, 1));
        // The distant portal must also be harmless without a translation.
        paint.rest = (portal[1] < 100.0).then_some(bounds);
        let stamps = [(2, portal)];
        assert_eq!(
            region_scissors_keeping_stable_page_captions(
                vec![bounds],
                &dirty,
                &[dirty],
                &[paint],
                &stamps,
                Some(portal),
            ),
            vec![bounds],
            "drag must clear the previous pose even while another portal stays mounted"
        );
    }
}

#[test]
fn clipped_neighbor_selection_fill_does_not_occlude_retained_glyphs() {
    let mut neighbor = sample_text("retained selection first line", 20.0);
    neighbor.width = 200.0;
    neighbor.style.input_caret = Some(crate::style::InputCaretDraw {
        selection: Some((0, 20)),
        ..Default::default()
    });
    let mut selected = sample_text("long nowrap selected line", -300.0);
    selected.width = 1000.0;
    selected.style.input_caret = neighbor.style.input_caret.clone();
    selected.clip_bounds = Some([300.0, 0.0, 200.0, 60.0]);
    let a = slot(11, [20.0, 0.0, 200.0, 60.0], (0, 1));
    let mut b = slot(12, [-300.0, 0.0, 1000.0, 60.0], (1, 2));
    b.instance = (0, 1);
    b.layered = true;
    b.overflow_clip = selected.clip_bounds;
    let mut fill = blank_instance(-300.0, 0.0, 1000.0, 20.0);
    fill.color = [0.0, 0.0, 0.0, 1.0];
    fill.clip_pos = [300.0, 0.0];
    fill.clip_size = [200.0, 60.0];
    let clips = clip_minus_foreign_opaque_plates(
        0,
        [20.0, 0.0, 200.0, 20.0],
        &[neighbor, selected],
        &[a, b],
        &[fill],
        1.0,
    );
    assert_eq!(
        clips,
        vec![[20.0, 0.0, 200.0, 20.0]],
        "offscreen selection fill must not erase neighbor glyphs"
    );
}

#[test]
fn measured_single_line_text_must_not_overlap_later_layered_rows() {
    let page_text: Vec<_> = ["Thinking...", "Reading 4 files", "Generating response…"]
        .into_iter()
        .enumerate()
        .map(|(index, caption)| {
            let mut text = sample_text(caption, 40.0);
            text.y = 40.0 + index as f32 * 32.0;
            text.height = 20.0;
            let prepared = crate::text::prepare(caption, 14.0, 400, 20.0);
            text.width = crate::text::intrinsic_width(&prepared).ceil();
            text.prepared = Some(prepared);
            text
        })
        .collect();
    let paints: Vec<_> = page_text
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let mut paint = slot(
                index as u64 + 1,
                [text.x, text.y, text.width, text.height],
                (index, index + 1),
            );
            paint.layered = true;
            paint
        })
        .collect();
    let dirties: Vec<_> = paints
        .iter()
        .map(|paint| PaintDirty {
            key: paint.key,
            bounds: paint.bounds,
            kind: PaintDirtyKind::Node,
        })
        .collect();
    assert!(!glyph_hits_layered_hole(&page_text[0], paints[2].bounds));
    let captured = node_present_clipped_capture_text_wipes_excluding_overlay(
        &dirties,
        &page_text,
        &paints,
        &[],
        &[],
        &[],
        1.0,
    );
    assert_eq!(
        captured
            .iter()
            .map(|text| text.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Thinking...", "Reading 4 files", "Generating response…"],
    );
}

#[test]
fn layered_underlay_must_recapture_wrap_lines_below_first_strut() {
    // Gallery `<Label>` under Latest: collect height is the first-line
    // strut; later wrap lines live in clip_bounds. Hide-settle dest-reset
    // punches "removed from the tab order…" and first-line hit-tests miss.
    let mut wrap = sample_text(
            "The scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.",
            16.0,
        );
    wrap.y = 200.0;
    wrap.width = 280.0;
    wrap.height = 18.0;
    wrap.clip_bounds = Some([16.0, 200.0, 280.0, 72.0]);
    wrap.paint_origin = [16.0, 200.0];
    let hole = [80.0, 230.0, 100.0, 36.0];
    assert!(
        !text_hits_damage(&wrap, hole),
        "precondition: first-line strut must miss the hide hole"
    );
    let mut latest = slot(9, [110.0, 228.0, 80.0, 32.0], (0, 0));
    latest.layered = true;
    let dirty = crate::app::damage::PaintDirty {
        key: 9,
        bounds: hole,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let underlay = node_present_layered_underlay_capture_text_wipes(
        &[dirty],
        &[wrap.clone()],
        &[latest],
        &[(9, hole)],
        &[],
        1.0,
    );
    assert!(
        underlay
            .iter()
            .any(|t| t.text.contains("removed from the tab order")),
        "hide settle must recapture wrap lines below the first-line strut, got {}",
        underlay.len()
    );
    let clip = underlay[0].clip_bounds.expect("wrap recapture clip");
    assert!(
            (underlay[0].x - wrap.x).abs() < 0.5 && (underlay[0].width - wrap.width).abs() < 0.5,
            "recapture must keep wrap layout origin/width so wrap lines still shape\n run=({},{},{},{}) wrap=({},{},{},{})",
            underlay[0].x,
            underlay[0].y,
            underlay[0].width,
            underlay[0].height,
            wrap.x,
            wrap.y,
            wrap.width,
            wrap.height
        );
    assert!(
            clip[2] < wrap.width - 8.0 && clip[3] < 80.0,
            "glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} hole={hole:?} wrap_w={}",
            wrap.width
        );
}

#[test]
fn ancestor_text_range_still_cuts_jump_fill_out_of_wrap_clip() {
    // MessageScroller is a stacking context, so its text range includes the
    // wrapping paragraph and its instance range includes the button fill.
    // Skipping that paint because the glyph index falls inside it left the
    // viewport clip intact, and the hole flush drew "es is the" on Latest.
    let mut wrap = sample_text(
            "MessageScrollerContent sets role=log and aria-relevant=additions by default, so screen readers announce new messages as they stream in.",
            16.0,
        );
    wrap.y = 200.0;
    wrap.width = 280.0;
    wrap.height = 18.0;
    let plate = [120.0, 210.0, 80.0, 32.0];
    let mut fill = blank_instance(plate[0], plate[1], plate[2], plate[3]);
    fill.color = [1.0, 1.0, 1.0, 1.0];
    let mut scroller = slot(1, [0.0, 0.0, 320.0, 384.0], (0, 1));
    scroller.layered = true;
    scroller.instance = (0, 1);
    let viewport = [0.0, 0.0, 320.0, 384.0];
    let parts = clip_minus_foreign_opaque_plates(
        0,
        viewport,
        &[wrap.clone()],
        &[scroller.clone()],
        &[fill],
        1.0,
    );
    for c in &parts {
        assert!(
            crate::app::damage::intersect_rects(*c, plate).is_none(),
            "wrap clip must not cover the chip, got {c:?}"
        );
    }
    let mut latest = sample_text("Latest", 148.0);
    latest.y = 218.0;
    latest.width = 40.0;
    latest.height = 16.0;
    let label = clip_minus_foreign_opaque_plates(0, viewport, &[latest], &[scroller], &[fill], 1.0);
    assert!(
        label
            .iter()
            .any(|c| crate::app::damage::intersect_rects(*c, plate).is_some()),
        "Latest starts on the chip; punching that fill would drop the label"
    );
}

#[test]
fn panned_edit_glyphs_are_not_occluded_by_their_ancestor_background() {
    let clip = [300.0, 20.0, 200.0, 24.0];
    let mut text = sample_text("long selected nowrap text", -600.0);
    text.y = 20.0;
    text.width = 1100.0;
    text.clip_bounds = Some(clip);
    text.style.input_caret = Some(crate::style::InputCaretDraw {
        selection: Some((0, text.text.len())),
        ..Default::default()
    });
    let mut background = blank_instance(0.0, 0.0, 600.0, 100.0);
    background.color = [1.0; 4];
    let mut selection = blank_instance(-600.0, 20.0, 1100.0, 24.0);
    selection.color = [0.0, 0.0, 0.0, 1.0];
    selection.clip_pos = [clip[0], clip[1]];
    selection.clip_size = [clip[2], clip[3]];
    let mut ancestor = slot(1, [0.0, 0.0, 600.0, 100.0], (0, 2));
    ancestor.instance = (0, 2);
    let mut leaf = slot(2, [-600.0, 20.0, 1100.0, 24.0], (0, 1));
    leaf.instance = (1, 2);
    let mut root = slot(3, [0.0, 0.0, 600.0, 400.0], (0, 2));
    root.instance = (0, 2);
    assert_eq!(
        clip_minus_foreign_opaque_plates(
            0,
            clip,
            &[text, sample_text("neighbor", 20.0)],
            &[root, ancestor, leaf],
            &[background, selection],
            1.0,
        ),
        vec![clip],
        "ancestor background must stay behind panned selected glyphs"
    );
}

#[test]
fn nowrap_selection_capture_survives_scrollbar_enter() {
    // Replay the Gallery failure: a horizontally panned, selected run and
    // an unrelated scrollbar fade dirty arrive in the same node present.
    let mut text = sample_text("wrap=off scrolls horizontally (no clip).", 362.55566);
    text.y = 309.0;
    text.width = 730.44434;
    text.height = 25.0;
    text.clip_bounds = Some([863.0, 301.0, 230.0, 119.0]);
    text.style.white_space = crate::style::WhiteSpace::Nowrap;
    text.style.input_caret = Some(crate::style::InputCaretDraw {
        selection: Some((0, text.text.len())),
        ..Default::default()
    });
    assert!(glyph_hits_layered_hole(&text, [863.0, 309.0, 230.0, 25.0]));
    let mut leaf = slot(1, [text.x, text.y, text.width, text.height], (0, 1));
    leaf.layered = true;
    leaf.overflow_clip = text.clip_bounds;
    let mut bar = slot(2, [855.0, 410.0, 248.0, 8.0], (1, 1));
    bar.layered = true;
    let dirties = [
        PaintDirty {
            key: 2,
            bounds: bar.bounds,
            kind: PaintDirtyKind::Node,
        },
        PaintDirty {
            key: 1,
            bounds: [1091.0, 308.5, 2.0, 22.0],
            kind: PaintDirtyKind::Node,
        },
    ];
    let captured = node_present_clipped_capture_text_wipes_excluding_overlay(
        &dirties,
        &[text],
        &[leaf, bar],
        &[
            (1, [863.0, 307.0, 230.0, 28.0]),
            (2, [855.0, 410.0, 248.0, 8.0]),
        ],
        &[],
        &[],
        1.0,
    );
    assert_eq!(
        captured.len(),
        1,
        "scrollbar below the single line must not remove its selection recolor capture"
    );
    assert_eq!(captured[0].clip_bounds, Some([863.0, 307.0, 230.0, 28.0]));
}

#[test]
fn selection_fill_does_not_occlude_its_own_edit_glyphs() {
    let clip = [16.0, 0.0, 200.0, 72.0];
    let mut text = sample_text("some selected text", 16.0);
    text.style.input_caret = Some(crate::style::InputCaretDraw {
        selection: Some((5, 13)),
        ..Default::default()
    });
    let mut fill = blank_instance(80.0, 0.0, 100.0, 24.0);
    fill.color = [0.0, 0.0, 0.0, 1.0];
    let mut leaf = slot(2, clip, (0, 1));
    leaf.text = (0, 1);
    let mut ancestor = slot(1, [0.0, 0.0, 240.0, 100.0], (0, 1));
    ancestor.text = (0, 2);
    let parts = clip_minus_foreign_opaque_plates(
        0,
        clip,
        &[text, sample_text("neighbor", 220.0)],
        &[ancestor, leaf],
        &[fill],
        1.0,
    );
    assert_eq!(parts, vec![clip], "selection must preserve selected glyphs");
}

#[test]
fn layered_text_wrapper_resets_glyphs_owned_by_its_leaf() {
    let bounds = [40.0, 48.0, 100.0, 20.0];
    let mut text = sample_text("Thinking...", bounds[0]);
    text.y = bounds[1];
    text.width = bounds[2];
    text.height = bounds[3];
    text.glyph_fx = Some("shimmer".into());
    let mut leaf = slot(1, bounds, (0, 1));
    leaf.instance = (1, 1);
    leaf.layered = true;
    leaf.rest = Some(bounds);
    let mut wrapper = leaf.clone();
    wrapper.key = 2;
    let paints = [leaf, wrapper];
    let mut card = blank_instance(0.0, 0.0, 320.0, 240.0);
    card.color = [0.09, 0.09, 0.09, 1.0];
    let instances = [card];
    let previous = last_present_visual_slots(&paints, &instances, &[], &[text], &[]);
    assert!(previous.iter().any(|(key, _)| *key == 1));
    let wrapper_previous = previous
        .iter()
        .find(|(key, _)| *key == 2)
        .map(|(_, bounds)| *bounds);
    assert!(wrapper_previous.is_none());
    let reset = node_present_dest_reset_full(
        2,
        bounds,
        &paints,
        &instances,
        &[],
        &[],
        1.0,
        wrapper_previous,
    )
    .expect("a text wrapper without its own last-present ink must still clear retained glyphs");
    assert_eq!(reset.rect, bounds);
    assert_eq!(reset.clear, [0.09, 0.09, 0.09, 1.0]);
    let mut dirties = vec![PaintDirty {
        key: 2,
        bounds,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut dirties,
        &paints,
        &previous,
        &instances,
        &[],
        &[],
        1.0,
        None,
    );
    assert_eq!(dirties[0].kind, PaintDirtyKind::Node);
}

#[test]
fn first_hover_backdrop_replays_retained_caption_in_scene_order() {
    let bounds = [8.0, 161.0, 240.0, 32.0];
    let mut fill = blank_instance(bounds[0], bounds[1], bounds[2], bounds[3]);
    fill.color = [0.35, 0.35, 0.39, 0.25];
    fill.backdrop_blur = 20.0;
    fill.radii = [4.0; 4];
    let mut hover = slot(2, bounds, (0, 1));
    hover.instance = (0, 1);
    hover.layered = true;
    hover.overflow_clip = Some([0.0, 49.0, 256.0, 602.0]);
    let mut dirties = vec![PaintDirty {
        key: 2,
        bounds,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(&mut dirties, &[hover], &[], &[fill], &[], &[], 1.0, None);
    assert_eq!(dirties[0].kind, PaintDirtyKind::Region);
}

#[test]
fn latest_enter_stays_on_the_ink_wipe() {
    // The pill owns glyphs. It used to become Region and window-clear the
    // expanded AABB (the white rectangle). Enter stays Node: no leftover
    // src-overs retain; a later frame dest-resets only the ink, and
    // wrapping in that wipe is the underlay drawn before the pill.
    let btn = [110.0, 228.0, 88.0, 32.0];
    let slid = [110.0, 236.0, 84.0, 30.0];
    let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let mut fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    fill.color = [1.0, 1.0, 1.0, 0.45];
    fill.radii = [16.0; 4];
    fill.border_width = 1.0;
    fill.border_color = [0.1, 0.1, 0.1, 0.4];
    let mut wrap = sample_text("messages as they stream", 16.0);
    wrap.y = 232.0;
    wrap.width = 280.0;
    wrap.height = 18.0;
    wrap.paint_origin = [16.0, 232.0];
    let mut latest = sample_text("Latest", 128.0);
    latest.y = 236.0;
    latest.width = 40.0;
    latest.height = 16.0;
    latest.paint_origin = [128.0, 236.0];
    let mut thread = slot(1, [16.0, 200.0, 280.0, 80.0], (0, 1));
    thread.instance = (0, 1);
    let mut pill = slot(2, btn, (1, 2));
    pill.layered = true;
    pill.instance = (1, 2);
    let paints = [thread, pill];
    let instances = [card, fill];
    let text = [wrap, latest];

    let mut first = vec![PaintDirty {
        key: 2,
        bounds: btn,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(&mut first, &paints, &[], &instances, &[], &[], 1.0, None);
    assert_eq!(
        first[0].kind,
        PaintDirtyKind::Node,
        "first visible Latest src-overs retain; a Region clear is the white rectangle"
    );
    assert!(
        (first[0].bounds[2] - btn[2]).abs() < 0.5 && (first[0].bounds[3] - btn[3]).abs() < 0.5,
        "first paint must not grow the dirty into a window hole, got {:?}",
        first[0].bounds
    );

    let mut mid = vec![PaintDirty {
        key: 2,
        bounds: btn,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut mid,
        &paints,
        &[(2, slid)],
        &instances,
        &[],
        &[],
        1.0,
        None,
    );
    assert_eq!(
        mid[0].kind,
        PaintDirtyKind::Node,
        "fade with leftover ink dest-resets the pill, not a Region window clear"
    );
    let reset =
        node_present_dest_reset_full(2, btn, &paints, &instances, &[], &[], 1.0, Some(slid))
            .expect("ink wipe");
    assert!(
        reset.rect[2] < btn[2] + 48.0 && reset.rect[3] < btn[3] + 48.0,
        "wipe stays on the pill+slide, not an expanded window rect, got {:?}",
        reset.rect
    );
    let scale = 1.0_f32;
    let wipe_logical = [
        reset.rect[0] / scale,
        reset.rect[1] / scale,
        reset.rect[2] / scale,
        reset.rect[3] / scale,
    ];
    let underlay = node_present_layered_underlay_capture_text_wipes(
        &mid,
        &text,
        &paints,
        &[(2, wipe_logical)],
        &[],
        1.0,
    );
    assert!(
        underlay.iter().any(|t| t.text.contains("stream")),
        "wrapping inside the ink wipe is that hole's underlay"
    );
    assert!(
        underlay.iter().all(|t| t.text != "Latest"),
        "the label is the overlay, drawn after the pill"
    );
    let clip = underlay[0].clip_bounds.expect("wipe clip");
    assert!(
        clip[2] < 160.0 && clip[3] < 80.0,
        "underlay TextBounds is the wipe, clip={clip:?}"
    );
}

#[test]
fn text_free_region_does_not_steal_page_glyphs() {
    let mut wrap = sample_text("messages as they stream", 290.0);
    wrap.y = 40.0;
    wrap.width = 40.0;
    wrap.height = 18.0;
    wrap.paint_origin = [290.0, 40.0];
    let mut bar = slot(3, [300.0, 8.0, 8.0, 120.0], (0, 0));
    bar.layered = true;
    bar.instance = (0, 1);
    let dirty = PaintDirty {
        key: 3,
        bounds: [296.0, 8.0, 16.0, 120.0],
        kind: PaintDirtyKind::Region,
    };
    let underlay = node_present_layered_underlay_capture_text(&[dirty], &[wrap.clone()], &[bar]);
    assert!(
        underlay.is_empty(),
        "scrollbar-style Region must not take the Latest underlay path"
    );
    let page = node_present_clipped_capture_text_wipes(&[dirty], &[wrap], &[bar], &[]);
    assert!(
        page.iter().any(|t| t.text.contains("stream")),
        "text-free Region must keep neighboring glyphs on the page flush"
    );
}

#[test]
fn caret_dirty_must_not_suppress_textarea_repaint() {
    let mut field = slot(1, [306.0, 1042.0, 2204.0, 100.0], (0, 1));
    field.instance = (0, 1);
    let mut leaf = slot(2, [319.0, 1051.0, 2178.0, 25.0], (0, 1));
    leaf.layered = true;
    leaf.instance = (1, 1);
    let field_dirty = PaintDirty {
        key: 1,
        bounds: field.bounds,
        kind: PaintDirtyKind::Node,
    };
    let caret_dirty = PaintDirty {
        key: 2,
        bounds: [342.0, 1051.0, 2.0, 22.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(!skip_in_flow_under_layered_overlay(
        &field_dirty,
        &[field_dirty, caret_dirty],
        &[field, leaf],
        &[blank_instance(306.0, 1042.0, 2204.0, 100.0)],
    ));
}

#[test]
fn entering_scrollbar_must_not_suppress_textarea_focus_repaint() {
    let bounds = [306.0, 350.0, 256.0, 120.0];
    let rail = [310.0, 459.0, 248.0, 8.0];
    let mut field = slot(1, bounds, (0, 0));
    field.instance = (0, 2);
    let mut track = slot(2, rail, (0, 0));
    track.instance = (1, 2);
    track.layered = true;
    let mut focus = blank_instance(bounds[0], bounds[1], bounds[2], bounds[3]);
    focus.border_width = 1.0;
    focus.border_color = [0.5, 0.5, 0.5, 1.0];
    focus.shadow_spread = 3.0;
    focus.shadow_color = [0.5, 0.5, 0.5, 0.5];
    let mut fade = blank_instance(rail[0], rail[1], rail[2], rail[3]);
    fade.color = [0.9, 0.9, 0.9, 0.4];
    let field_dirty = PaintDirty {
        key: 1,
        bounds,
        kind: PaintDirtyKind::Node,
    };
    let rail_dirty = PaintDirty {
        key: 2,
        bounds: rail,
        kind: PaintDirtyKind::Node,
    };
    for spread in [0.0, 3.0] {
        focus.shadow_spread = spread;
        for alpha in [0.0, 0.4, 1.0] {
            fade.color[3] = alpha;
            track.instance = if alpha == 0.0 { (1, 1) } else { (1, 2) };
            assert!(
                !skip_in_flow_under_layered_overlay(
                    &field_dirty,
                    &[field_dirty, rail_dirty],
                    &[field.clone(), track.clone()],
                    &[focus, fade],
                ),
                "a fading rail cannot own the field repaint: spread={spread} alpha={alpha}"
            );
        }
    }
}

#[test]
fn latest_fade_and_wrap_dirty_must_not_stamp_thread_on_pill() {
    // Click-to-bottom / enter fade: wrapping dest-reset + hole_text flush
    // after Latest stamped "gh" / "as they stream" onto the translucent pill.
    let btn = [110.0, 228.0, 80.0, 32.0];
    let wrap_box = [16.0, 200.0, 280.0, 72.0];
    let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 0.4];
    btn_fill.radii = [16.0; 4];
    let mut wrap = sample_text("bottom — no ghost focus stops.", 16.0);
    wrap.y = 220.0;
    wrap.width = 280.0;
    wrap.height = 18.0;
    wrap.clip_bounds = Some(wrap_box);
    wrap.paint_origin = [16.0, 220.0];
    let mut latest_label = sample_text("Latest", btn[0] + 20.0);
    latest_label.y = btn[1] + 8.0;
    latest_label.width = 40.0;
    latest_label.paint_origin = [btn[0] + 20.0, btn[1] + 8.0];
    let wrap_p = slot(1, wrap_box, (0, 1));
    let mut btn_p = slot(2, btn, (1, 2));
    btn_p.layered = true;
    btn_p.instance = (1, 2);
    let instances = [card, btn_fill];
    let paints = [wrap_p, btn_p];
    let wrap_dirty = crate::app::damage::PaintDirty {
        key: 1,
        bounds: wrap_box,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let btn_dirty = crate::app::damage::PaintDirty {
        key: 2,
        bounds: btn,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let dirties = [wrap_dirty, btn_dirty];
    assert!(
        skip_in_flow_under_layered_overlay(&wrap_dirty, &dirties, &paints, &instances),
        "wrapping dest-reset under Latest must yield the overlap to the overlay slot"
    );
    assert!(
        !skip_in_flow_under_layered_overlay(&btn_dirty, &dirties, &paints, &instances),
        "Latest itself is the overlay slot"
    );
    assert!(
        node_present_dest_reset_full(2, btn, &paints, &instances, &[], &[], 1.0, None).is_none(),
        "first visible Latest src-overs wrapping still in retain"
    );
    let fade = node_present_dest_reset_full(2, btn, &paints, &instances, &[], &[], 1.0, Some(btn))
        .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
        fade.rect[2] <= btn[2] + 40.0,
        "fade dest-reset must stay on the pill, wipe={:?}",
        fade.rect
    );
    assert!(
        node_present_flush_layered_underlay_glyphs(2, btn, &paints),
        "fade dest-reset must recapture wrapping in the wipe before Latest paints"
    );
    let underlay = node_present_layered_underlay_capture_text_wipes(
        &[btn_dirty],
        &[wrap.clone(), latest_label.clone()],
        &paints,
        &[(2, btn)],
        &[],
        1.0,
    );
    assert!(
        underlay.iter().any(|t| t.text == wrap.text),
        "wrapping in the dest-reset hole must restore under the pill, got {:?}",
        underlay.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
    );
    assert!(
        underlay.iter().all(|t| t.text != "Latest"),
        "underlay must not steal Latest's label"
    );
    let hole_text = node_present_clipped_capture_text_wipes(
        &dirties,
        &[wrap.clone(), latest_label.clone()],
        &paints,
        &[(2, btn)],
    );
    assert!(
        hole_text.iter().all(|t| t.text != wrap.text),
        "page capture must not flush wrapping after the pill, got {:?}",
        hole_text
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        hole_text.iter().any(|t| t.text == "Latest"),
        "Latest label stays in the overlay slot capture"
    );

    let mut btn_hidden = btn_p;
    btn_hidden.instance = (2, 2);
    btn_hidden.text = (1, 1);
    assert!(
        node_present_flush_layered_underlay_glyphs(2, btn, &[wrap_p, btn_hidden]),
        "hide settle must recapture wrapping in the dest-reset hole"
    );
    let opaque = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    let mut opaque_fill = opaque;
    opaque_fill.color = [1.0, 1.0, 1.0, 1.0];
    opaque_fill.radii = [16.0; 4];
    let opaque_reset = node_present_dest_reset_full(
        2,
        btn,
        &paints,
        &[card, opaque_fill],
        &[],
        &[],
        1.0,
        Some(btn),
    )
    .expect("settled Latest with last-present dest-resets leftover (enter ghost)");
    assert!(
        opaque_reset.rect[2] <= btn[2] + 40.0,
        "settled dest-reset must stay on the pill, wipe={:?}",
        opaque_reset.rect
    );
}

#[test]
fn layered_underlay_recapture_must_keep_mid_line_wrap_layout_clip() {
    // Gallery follow-up: "Got it. Stay at the live edge and I'll keep following."
    // Hide dest-reset CLEARs the pill in the middle of the line. Wrap-box
    // TextBounds stamped wrapping onto Latest; hole TextBounds must keep
    // wrap layout origin so mid-line glyphs still shape.
    let mut wrap = sample_text(
        "Got it. Stay at the live edge and I'll keep following.",
        16.0,
    );
    wrap.y = 320.0;
    wrap.width = 280.0;
    wrap.height = 18.0;
    wrap.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
    wrap.paint_origin = [16.0, 320.0];
    let hole = [80.0, 318.0, 100.0, 36.0];
    assert!(
        !aabbs_disjoint([wrap.x, wrap.y, wrap.width, wrap.height], hole),
        "precondition: first-line strut must overlap the hide hole"
    );
    let mut latest = slot(9, [110.0, 328.0, 80.0, 32.0], (0, 0));
    latest.layered = true;
    let dirty = crate::app::damage::PaintDirty {
        key: 9,
        bounds: hole,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let underlay = node_present_layered_underlay_capture_text_wipes(
        &[dirty],
        &[wrap.clone()],
        &[latest],
        &[(9, hole)],
        &[],
        1.0,
    );
    let captured = underlay
        .iter()
        .find(|t| t.text.contains("keep following"))
        .expect("hide settle must recapture the punched wrap line");
    assert!(
        (captured.x - wrap.x).abs() < 0.5 && (captured.width - wrap.width).abs() < 0.5,
        "recapture must keep wrap layout origin/width\n x={} w={} wrap_x={} wrap_w={}",
        captured.x,
        captured.width,
        wrap.x,
        wrap.width
    );
    let clip = captured.clip_bounds.expect("wrap recapture clip");
    assert!(
            clip[2] < wrap.width - 8.0 && clip[3] < 80.0,
            "glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} hole={hole:?} wrap_w={}",
            wrap.width
        );
}

#[test]
fn viewport_clip_must_not_recapture_foreign_caption_or_latest_as_underlay() {
    // Ancestor overflow used to inflate wrap extent to the 24rem viewport.
    // Hide-settle then stacked composer + "Latest" in the pill hole.
    let mut caption = sample_text("Thanks — that matches Slack / iMessage.", 16.0);
    caption.y = 24.0;
    caption.width = 240.0;
    caption.height = 18.0;
    caption.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
    caption.paint_origin = [16.0, 24.0];
    let mut latest = sample_text("Latest", 120.0);
    latest.y = 340.0;
    latest.width = 64.0;
    latest.height = 16.0;
    latest.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
    latest.paint_origin = [120.0, 340.0];
    let mut composer = sample_text("No messages queued. Reset the conversation.", 16.0);
    composer.y = 400.0;
    composer.width = 288.0;
    composer.height = 18.0;
    composer.clip_bounds = Some([0.0, 0.0, 320.0, 480.0]);
    composer.paint_origin = [16.0, 400.0];
    let hole = [80.0, 330.0, 100.0, 36.0];
    let mut pill = slot(9, [110.0, 328.0, 80.0, 32.0], (1, 2));
    pill.layered = true;
    let dirty = crate::app::damage::PaintDirty {
        key: 9,
        bounds: hole,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let underlay = node_present_layered_underlay_capture_text_wipes(
        &[dirty],
        &[caption, latest, composer],
        &[pill],
        &[(9, hole)],
        &[],
        1.0,
    );
    assert!(
        underlay
            .iter()
            .all(|t| t.text != "Latest" && !t.text.contains("Slack") && !t.text.contains("queued")),
        "viewport clip must not pull foreign/Latest/composer into the pill hole, got {:?}",
        underlay.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
    );
}

fn slot(key: u64, bounds: [f32; 4], text: (usize, usize)) -> NodePaint {
    NodePaint {
        key,
        bounds,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text,
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    }
}

/// Click toggles the caption via `bind_text`, which also dirties the button.
/// Reset restores the page; the matching node draw restores the rounded
/// button exactly once before its caption. A caption-only dirty instead
/// needs the button as an underlay replay.
#[test]
fn click_rounded_button_dest_reset_keeps_caption_on_button_fill() {
    let page = [0.0, 0.0, 800.0, 600.0];
    let button = [10.0, 40.0, 220.0, 32.0];
    let mut page_fill = blank_instance(page[0], page[1], page[2], page[3]);
    page_fill.color = [0.98, 0.98, 0.97, 1.0];
    let mut button_fill = blank_instance(button[0], button[1], button[2], button[3]);
    button_fill.color = [0.12, 0.18, 0.55, 1.0];
    button_fill.radii = [6.0; 4];
    let mut page_p = slot(1, page, (0, 0));
    page_p.instance = (0, 1);
    let mut button_p = slot(2, button, (0, 1));
    button_p.instance = (1, 2);
    let instances = [page_fill, button_fill];
    let paints = [page_p, button_p];
    let reset =
        node_present_dest_reset_full(2, button, &paints, &instances, &[], &[], 1.0, Some(button))
            .expect("caption click dest-resets the button");
    let cx = button[0] + button[2] * 0.5;
    let cy = button[1] + button[3] * 0.5;
    let lum = |c: [f32; 4]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    assert_eq!(
        reset.replay,
        (0, 0),
        "own chrome is drawn by the dirty slot"
    );
    assert_eq!(reset.clear, page_fill.color);
    let own = slot_paint_ops(
        &[crate::layout::layout_tree::PaintOp::Rects { start: 0, count: 2 }],
        button_p.instance,
        button_p.textured,
    )
    .collect::<Vec<_>>();
    assert!(matches!(
        own.as_slice(),
        [crate::layout::layout_tree::PaintOp::Rects { start: 1, count: 1 }]
    ));
    let visible = if button_p.instance.1 > button_p.instance.0 {
        (button_p.instance.0..button_p.instance.1)
            .rev()
            .find_map(|i| {
                let inst = &instances[i];
                let [x, y] = inst.rect_pos;
                let [w, h] = inst.rect_size;
                (inst.color[3] > 0.95 && cx >= x && cy >= y && cx < x + w && cy < y + h)
                    .then_some(inst.color)
            })
            .unwrap_or(reset.clear)
    } else {
        reset.clear
    };
    let page_lum = lum(page_fill.color);
    let button_lum = lum(button_fill.color);
    assert!(
        lum(visible) < page_lum - 0.25 && (lum(visible) - button_lum).abs() < 0.05,
        "click must keep the caption on the button fill, not the lighter page; \
             visible={visible:?} replay={:?} clear={:?}",
        reset.replay,
        reset.clear
    );

    let caption = [18.0, 46.0, 140.0, 20.0];
    let mut text_p = slot(4, caption, (0, 1));
    text_p.instance = (2, 2);
    let paints = [page_p, button_p, text_p];
    let text_reset = node_present_dest_reset_full(
        4,
        caption,
        &paints,
        &instances,
        &[],
        &[],
        1.0,
        Some(caption),
    )
    .expect("text leaf still dest-resets when the button is not dirty");
    assert!(
        text_reset.replay.0 <= 1 && text_reset.replay.1 > 1,
        "text-only wipe must still replay the rounded button, got {:?}",
        text_reset.replay
    );
}

#[test]
fn layered_clip_must_not_cut_overlay_rail_against_full_width_jump_dock() {
    // Latest dock is a full-width empty bar. Occupancy clip against it used
    // to chop the fading overlay rail, leaving a groove+thumb remnant.
    let mut rail = slot(1, [310.0, 2.0, 10.0, 380.0], (0, 0));
    rail.overlay = true;
    rail.layered = true;
    rail.rest = Some(rail.bounds);
    let mut dock = slot(2, [0.0, 336.0, 320.0, 32.0], (0, 0));
    dock.layered = true;
    dock.rest = Some(dock.bounds);
    let wipe =
        clip_rect_against_layered_siblings(rail.bounds, rail.key, rail.bounds, &[rail, dock], 1.0);
    let remnant = [310.0, 336.0, 10.0, 32.0];
    assert!(
        intersect_rects(wipe, remnant).is_some(),
        "rail wipe must still overlap Latest's Y, wipe={wipe:?}"
    );
    assert!(
        wipe[1] + wipe[3] + 1.0 >= remnant[1] + remnant[3],
        "rail wipe must keep the bottom under Latest, wipe={wipe:?} remnant={remnant:?}"
    );
}

#[test]
fn layered_track_hide_must_not_carve_out_its_thumb() {
    // Settled auto-hide emits no thumb quads. Range nesting is empty, so the
    // thumb used to be a foreign layered peer and `shrink_aabb_to_exclude`
    // dropped the track span under it.
    let track = [2.0, 84.0, 216.0, 10.0];
    let thumb = [2.0, 85.0, 73.56, 8.0];
    let mut track_p = slot(1, track, (0, 0));
    track_p.layered = true;
    track_p.rest = Some(track);
    let mut thumb_p = slot(2, thumb, (0, 0));
    thumb_p.layered = true;
    thumb_p.rest = Some(thumb);
    let wipe =
        clip_rect_against_layered_siblings(track, track_p.key, track, &[track_p, thumb_p], 1.0);
    assert!(
            wipe[0] <= track[0] + 0.5
                && wipe[1] <= track[1] + 0.5
                && wipe[0] + wipe[2] + 0.5 >= track[0] + track[2]
                && wipe[1] + wipe[3] + 0.5 >= track[1] + track[3],
            "track hide wipe must cover the painted track, not the sliver beside the thumb\n wipe={wipe:?} track={track:?} thumb={thumb:?}"
        );
}

#[test]
fn horizontal_track_reset_keeps_span_over_earlier_textarea_viewport() {
    // Gallery's absolute text viewport precedes the overlay rail. Its
    // layout box overlaps all but the last 10px of the rail, even though
    // its text is painted above the rail's Y. It is an underlay, not a
    // foreground peer to carve out of the track's repaint scissor.
    let field = [306.0, 350.0, 256.0, 121.0];
    let track = [311.0, 460.0, 248.0, 8.0];
    let thumb = [311.0, 461.0, 77.46, 6.0];
    let mut viewport = slot(1, [319.0, 351.0, 230.0, 119.0], (0, 1));
    viewport.instance = (1, 1);
    viewport.layered = true;
    viewport.rest = Some(viewport.bounds);
    let mut thumb_p = slot(2, thumb, (0, 0));
    thumb_p.instance = (2, 3);
    thumb_p.layered = true;
    thumb_p.rest = Some(thumb);
    let mut track_p = slot(3, track, (0, 0));
    track_p.instance = (1, 3);
    track_p.layered = true;
    track_p.rest = Some(track);
    let mut field_p = slot(4, field, (0, 1));
    field_p.instance = (0, 3);
    field_p.rest = Some(field);
    let paints = [viewport, thumb_p, track_p, field_p];
    for alpha in [0.0, 0.3, 1.0] {
        let mut fill = blank_instance(field[0], field[1], field[2], field[3]);
        fill.color = [1.0; 4];
        let mut rail = blank_instance(track[0], track[1], track[2], track[3]);
        rail.color = [0.96, 0.96, 0.96, alpha];
        let rail_ink = instance_paint_bounds(&rail);
        let mut pill = blank_instance(thumb[0], thumb[1], thumb[2], thumb[3]);
        pill.color = [0.0, 0.0, 0.0, alpha];
        let reset = node_present_dest_reset_full(
            3,
            track,
            &paints,
            &[fill, rail, pill],
            &[],
            &[],
            1.0,
            Some(track),
        )
        .expect("track needs a reset through hide and re-entry");
        assert_eq!(
            reset.rect, rail_ink,
            "alpha={alpha}: viewport carved the rail down to its right end"
        );
    }
    let foreground = clip_rect_against_layered_siblings(
        track,
        3,
        track,
        &[paints[2].clone(), paints[0].clone()],
        1.0,
    );
    assert_eq!(
        foreground,
        [549.0, 460.0, 10.0, 8.0],
        "later foreground must still clip"
    );

    let mut title = paints[0].clone();
    title.bounds = [319.0, 440.0, 230.0, 20.0];
    title.rest = Some(title.bounds);
    let caption_clip = clip_rect_against_layered_siblings(
        [311.0, 440.0, 248.0, 28.0],
        3,
        track,
        &[title, paints[2].clone()],
        1.0,
    );
    assert_eq!(
        caption_clip, track,
        "earlier rest-disjoint title must still clip the halo"
    );
}

#[test]
fn text_outside_damage_filtered() {
    let kept = filter_text_nodes(
        &[sample_text("in", 0.0), sample_text("out", 400.0)],
        [0.0, 0.0, 40.0, 40.0],
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text, "in");
}

#[test]
fn scroll_layer_text_clip_drops_rail_captions_in_the_same_y_band() {
    // Gallery MessageScroller: inset column starts ~216px, 16px instance pad
    // grazes a full-width sidebar label. Glyphs must follow the unpadded viewport.
    let mut rail = sample_text("布局", 8.0);
    rail.y = 200.0;
    rail.width = 200.0;
    rail.height = 20.0;
    let mut msg = sample_text("hello", 260.0);
    msg.y = 200.0;
    msg.width = 80.0;
    msg.height = 20.0;
    let inset = [216.0, 8.0, 1056.0, 784.0];
    let padded = padded_xywh(inset, DAMAGE_CULL_PAD_LOGICAL);
    assert!(
        text_hits_damage(&rail, padded),
        "16px instance pad is what used to steal rail captions"
    );
    let tight = padded_xywh(inset, SCROLL_LAYER_TEXT_PAD_LOGICAL);
    let kept = clip_text_nodes_to_rect(&[rail.clone(), msg.clone()], tight);
    assert!(
        kept.iter().all(|n| n.text != "布局"),
        "scroll-layer glyphon must not prepare 布局, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
    assert!(
        kept.iter().any(|n| n.text == "hello"),
        "in-scroller glyphs must stay, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );

    let ms = [240.0, 128.0, 400.0, 384.0];
    let mut y_band = sample_text("Gallery", 8.0);
    y_band.y = 180.0;
    y_band.width = 48.0;
    y_band.height = 20.0;
    let kept_ms =
        clip_text_nodes_to_rect(&[y_band], padded_xywh(ms, SCROLL_LAYER_TEXT_PAD_LOGICAL));
    assert!(
        kept_ms.is_empty(),
        "Y-overlapping rail caption must not follow the inner MessageScroller viewport"
    );
}

#[test]
fn scroll_strip_keeps_full_glyph_line_vertical_coverage() {
    let mut message = sample_text("message", 240.0);
    message.y = 100.0;
    message.width = 120.0;
    message.height = 20.0;

    let clipped = clip_text_nodes_to_cull(&[message], CullRects::one([220.0, 110.0, 80.0, 10.0]));
    let bounds = clipped
        .first()
        .and_then(|node| node.clip_bounds)
        .expect("the exposed strip must prepare the intersecting message");

    assert_eq!(bounds[0], 240.0);
    assert_eq!(bounds[2], 60.0);
    assert_eq!(bounds[1], 100.0);
    assert_eq!(bounds[3], 20.0);
}

#[test]
fn disjoint_scroll_repairs_do_not_capture_glyph_pixels_twice() {
    let mut message = sample_text("message", 240.0);
    message.y = 100.0;
    message.width = 120.0;
    message.height = 20.0;
    message.clip_bounds = Some([220.0, 80.0, 180.0, 100.0]);
    let clipped = clip_text_nodes_to_cull(
        &[message],
        CullRects::from_rects([[220.0, 100.0, 100.0, 8.0], [240.0, 112.0, 120.0, 8.0]]).unwrap(),
    );
    for (i, a) in clipped.iter().enumerate() {
        for b in &clipped[i + 1..] {
            assert!(intersect_rects(a.clip_bounds.unwrap(), b.clip_bounds.unwrap()).is_none());
        }
    }
}

/// Nested Select accordion occupancy is the stamp. Overlay glyphon ignores the
/// wgpu scissor used for chrome, so labels below the clip survive as leftover ink.
#[test]
fn overlay_stamp_clips_select_labels_below_accordion() {
    let mut hex = sample_text("HEX", 8.0);
    hex.y = 8.0;
    hex.width = 64.0;
    hex.height = 32.0;
    let mut rgb = sample_text("RGB", 8.0);
    rgb.y = 80.0;
    rgb.width = 64.0;
    rgb.height = 32.0;
    let stamp = [0.0, 0.0, 80.0, 40.0];
    let kept = clip_text_nodes_to_rect(&[hex, rgb], stamp);
    assert!(
        kept.iter().any(|n| n.text == "HEX"),
        "visible first row must stay, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
    assert!(
        kept.iter().all(|n| n.text != "RGB"),
        "labels below accordion must not enter overlay glyphon, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
    for n in &kept {
        let pb = text_paint_bounds(n);
        assert!(
            pb[1] + pb[3] <= stamp[1] + stamp[3] + 1.0,
            "prepared overlay text must clip to the stamp, paint={pb:?} stamp={stamp:?}"
        );
    }
}

#[test]
fn dual_axis_strips_must_not_prepare_overlap_only_glyphs() {
    // Viewport 200×100, delta [12, 8]: bottom 8px band + right 12×92 band.
    let exp_h = [0.0, 92.0, 200.0, 8.0];
    let exp_v = [188.0, 0.0, 12.0, 92.0];
    let mut overlap = sample_text("kept-by-copy", 40.0);
    overlap.y = 20.0;
    overlap.width = 80.0;
    overlap.height = 20.0;
    let mut bottom = sample_text("new-bottom", 10.0);
    bottom.y = 94.0;
    bottom.width = 40.0;
    bottom.height = 12.0;
    let mut right = sample_text("new-right", 190.0);
    right.y = 10.0;
    right.width = 16.0;
    right.height = 20.0;
    let mut spanning = sample_text("span", 0.0);
    spanning.y = 0.0;
    spanning.width = 200.0;
    spanning.height = 100.0;
    let kept = clip_text_nodes_to_strips(&[overlap, bottom, right, spanning], &[exp_h, exp_v]);
    assert!(
        kept.iter().all(|n| n.text != "kept-by-copy"),
        "overlap-only glyphs must stay in the copied retain, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
    assert!(kept.iter().any(|n| n.text == "new-bottom"));
    assert!(kept.iter().any(|n| n.text == "new-right"));
    let span_clips: Vec<_> = kept
        .iter()
        .filter(|n| n.text == "span")
        .filter_map(|n| n.clip_bounds)
        .collect();
    assert_eq!(
        span_clips.len(),
        2,
        "spanning glyph must split into two strip clips"
    );
    assert!(span_clips
        .iter()
        .any(|c| (c[1] - 92.0).abs() < 0.1 && (c[3] - 8.0).abs() < 0.1));
    assert!(span_clips
        .iter()
        .any(|c| (c[0] - 188.0).abs() < 0.1 && (c[2] - 12.0).abs() < 0.1));
}

#[test]
fn cull_rects_from_rects_bounds_repair_count() {
    let too_many: Vec<_> = (0..CULL_RECTS_CAP + 1)
        .map(|i| [i as f32, 0.0, 1.0, 1.0])
        .collect();
    assert!(CullRects::from_rects(too_many).is_none());
    let ok: Vec<_> = (0..CULL_RECTS_CAP)
        .map(|i| [i as f32, 0.0, 1.0, 1.0])
        .collect();
    let cull = CullRects::from_rects(ok).expect("cap");
    assert_eq!(cull.count(), CULL_RECTS_CAP);
    assert!(CullRects::from_rects(std::iter::empty()).is_none());
}

#[test]
fn pin_hole_must_prepare_overlap_glyph_that_strips_drop() {
    let exp_h = [0.0, 92.0, 200.0, 8.0];
    let exp_v = [188.0, 0.0, 12.0, 92.0];
    let pin = [8.0, 8.0, 40.0, 24.0];
    let mut overlap = sample_text("sticky-header", 10.0);
    overlap.y = 10.0;
    overlap.width = 36.0;
    overlap.height = 20.0;
    let strips_only = clip_text_nodes_to_strips(&[overlap.clone()], &[exp_h, exp_v]);
    assert!(
        strips_only.iter().all(|n| n.text != "sticky-header"),
        "overlap-only sticky must not be in L strips"
    );
    let with_pin = clip_text_nodes_to_strips(&[overlap], &[exp_h, exp_v, pin]);
    assert!(
        with_pin.iter().any(|n| n.text == "sticky-header"),
        "sticky pin hole must re-prepare the header after copy"
    );
}

#[test]
fn send_append_must_not_rebuild_sidebar_glyphon_band() {
    // Gallery Send: new bubble is later in collect order. Band 0 is the rail.
    let mut layout = sample_text("布局", 8.0);
    layout.y = 200.0;
    let mut gallery = sample_text("Gallery", 8.0);
    gallery.y = 236.0;
    let mut hello = sample_text("hello", 260.0);
    hello.y = 200.0;
    let mut next = sample_text("next", 260.0);
    next.y = 240.0;
    let prev = vec![layout.clone(), gallery.clone(), hello.clone()];
    let curr = vec![layout, gallery, hello, next];
    assert!(
        !page_stack_band_needs_glyphon(&prev, &curr, 0, 2, None, true),
        "sidebar band must reuse after Send"
    );
    assert!(
        page_stack_band_needs_glyphon(&prev, &curr, 2, 4, None, true),
        "message band must rebuild after Send"
    );
    assert!(
        page_stack_band_needs_glyphon(&prev, &curr, 0, 2, None, false),
        "FullClear still rebuilds the rail band"
    );
    let ms = [240.0, 128.0, 400.0, 384.0];
    assert!(
        !page_stack_band_needs_glyphon(&prev, &curr, 0, 2, Some(CullRects::one(ms)), true),
        "inner follow Clear must not submit the rail band"
    );
    assert!(
        page_stack_band_needs_glyphon(&prev, &curr, 2, 4, Some(CullRects::one(ms)), true),
        "inner follow Clear must submit the message band"
    );
}

#[test]
fn retained_shimmer_text_only_replays_inside_cleared_damage() {
    let mut animated = sample_text("Thinking...", 260.0);
    animated.glyph_fx = Some("shimmer".into());
    animated.shimmer = Some(crate::renderer::ShimmerParams::default());
    let caption = sample_text("Gallery", 8.0);
    let previous = vec![caption.clone(), animated.clone()];
    let current = vec![caption, animated];
    for _ in 0..4 {
        assert!(page_stack_band_submit_text(&previous, &current, 0, 2, None, true).is_none());
    }
    let damage = CullRects::one([250.0, 0.0, 200.0, 200.0]);
    let submitted = page_stack_band_submit_text(&previous, &current, 0, 2, Some(damage), true)
        .expect("a cleared shimmer hole must replay the current animated ink");
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].text, "Thinking...");
}

#[test]
fn send_append_mixed_band_must_not_submit_sidebar_runs() {
    // Production often keeps 布局 and the new bubble in one stacking band.
    // PartialLoad must submit only the new run — not re-prepare the rail.
    let mut layout = sample_text("布局", 8.0);
    layout.y = 200.0;
    let mut gallery = sample_text("Gallery", 8.0);
    gallery.y = 236.0;
    let mut hello = sample_text("hello", 260.0);
    hello.y = 200.0;
    let mut next = sample_text("next", 260.0);
    next.y = 240.0;
    let prev = vec![layout.clone(), gallery.clone(), hello.clone()];
    let curr = vec![layout, gallery, hello, next];
    let submitted = page_stack_band_submit_text(&prev, &curr, 0, 4, None, true)
        .expect("Send must submit the new bubble");
    assert!(
        submitted.iter().any(|n| n.text == "next"),
        "new bubble must be submitted, got {:?}",
        submitted
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        submitted
            .iter()
            .all(|n| n.text != "布局" && n.text != "Gallery" && n.text != "hello"),
        "unchanged rail/bubble runs must stay in retain, got {:?}",
        submitted
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    let full = page_stack_band_submit_text(&prev, &curr, 0, 4, None, false)
        .expect("FullClear still owns the whole band");
    assert_eq!(full.len(), 4);
}

#[test]
fn toggle_caption_is_not_submitted_twice_with_its_stable_suffix() {
    // Rapid click: "打开 RSX Trigger" → "关闭 RSX Trigger". The Latin suffix
    // does not change. The button is a live Node dirty (capture draws the
    // whole line) and the popover Restore hole also contains that line.
    // Page glyphon must not submit it again.
    let mut caption = sample_text("关闭 RSX Trigger", 18.0);
    caption.y = 46.0;
    caption.width = 140.0;
    caption.height = 20.0;
    let mut heading = sample_text("Trigger · Builder", 10.0);
    heading.y = 120.0;
    heading.width = 120.0;
    heading.height = 16.0;
    let trigger = PaintDirty {
        key: 2,
        bounds: [10.0, 40.0, 160.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let panel = [10.0, 76.0, 220.0, 96.0];
    let restore = PaintDirty {
        key: 9,
        bounds: [0.0, 30.0, 250.0, 160.0],
        kind: PaintDirtyKind::OverlayRestore,
    };
    let page = vec![caption.clone(), heading.clone()];
    let paints = vec![slot(2, trigger.bounds, (0, 1))];
    let kept = page_submit_excluding_live_node_capture(
        vec![caption.clone(), heading.clone()],
        &page,
        &[restore, trigger],
        &paints,
        &[(9, panel)],
        None,
        true,
    );
    assert!(
        kept.iter().all(|n| n.text != "关闭 RSX Trigger"),
        "capture owns the toggled caption; a second page draw flickers the suffix, got {:?}",
        kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
    assert!(
        kept.iter().any(|n| n.text == "Trigger · Builder"),
        "unchanged heading outside the button must stay in the page mesh"
    );
    let full = page_submit_excluding_live_node_capture(
        vec![caption.clone(), heading],
        &page,
        &[restore, trigger],
        &paints,
        &[(9, panel)],
        None,
        false,
    );
    assert!(
        full.iter().any(|n| n.text == "关闭 RSX Trigger"),
        "FullClear has no capture flush; dropping the caption blanks it"
    );
    let mut sibling = sample_text("icon", 20.0);
    sibling.y = 48.0;
    sibling.width = 100.0;
    sibling.height = 20.0;
    let with_sibling = vec![caption, sibling.clone()];
    let sibling_kept = page_submit_excluding_live_node_capture(
        with_sibling.clone(),
        &with_sibling,
        &[restore, trigger],
        &paints,
        &[(9, panel)],
        None,
        true,
    );
    assert!(
        sibling_kept.iter().all(|n| n.text != "icon"),
        "restore cleared the icon; the page mesh would draw it under the shadow"
    );
    assert!(
        overlay_restore_fringe_text(&with_sibling, &[restore, trigger], &[(9, panel)], None)
            .iter()
            .any(|n| n.text == "icon"),
        "the icon sits outside the panel stamp, so page glyphon must not redraw it"
    );
    let chip_text = {
        let mut t = sample_text("chip", 24.0);
        t.y = 96.0;
        t.width = 32.0;
        t.height = 16.0;
        t
    };
    let chip = PaintDirty {
        key: 3,
        bounds: [20.0, 90.0, 40.0, 28.0],
        kind: PaintDirtyKind::Node,
    };
    let chip_page = vec![chip_text.clone()];
    let swallowed = page_submit_excluding_live_node_capture(
        vec![chip_text],
        &chip_page,
        &[restore, chip],
        &[slot(3, chip.bounds, (0, 1))],
        &[(9, panel)],
        None,
        true,
    );
    assert!(
        swallowed.iter().any(|n| n.text == "chip"),
        "a Node dirty inside the panel stamp is not captured; Restore must still draw it"
    );
}

#[test]
fn overlay_restore_without_node_capture_keeps_neighbor_text() {
    // Closing the reaction popover can be OverlayRestore-only. The restore
    // hole intersects the next bubble's line, but no capture renderer owns
    // that page text. It must remain in the page glyphon submission.
    let mut ghost = sample_text(
        "Ghost bubbles work for assistant text without a frame.",
        0.0,
    );
    ghost.y = 76.0;
    ghost.width = 334.0;
    ghost.height = 20.0;
    let restore = PaintDirty {
        key: 9,
        bounds: [139.0, 68.0, 48.0, 28.0],
        kind: PaintDirtyKind::OverlayRestore,
    };
    let panel = [(9u64, [139.0, 68.0, 48.0, 28.0])];
    let ancestor = slot(1, [0.0, 0.0, 400.0, 120.0], (0, 1));
    let kept = page_submit_excluding_live_node_capture(
        vec![ghost.clone()],
        &[ghost],
        &[restore],
        &[ancestor],
        &panel,
        None,
        true,
    );
    assert_eq!(
        kept.len(),
        1,
        "OverlayRestore without capture must not drop the neighboring line"
    );
    assert_eq!(
        kept[0].text,
        "Ghost bubbles work for assistant text without a frame."
    );
}

#[test]
fn toggle_caption_fade_restore_leaves_suffix_in_retain() {
    // Fade tick: the button is not a Node dirty. OverlayRestore's padded
    // halo still contains the trigger. Clearing that line and flushing it
    // after the shadow blends the Latin coverage against the animating
    // shadow, so "RSX Trigger" flickers even though its box did not move.
    // Page glyphon must not redraw it either (that draw is under the shadow).
    let mut caption = sample_text("关闭 RSX Trigger", 18.0);
    caption.y = 46.0;
    caption.width = 140.0;
    caption.height = 20.0;
    let mut chip = sample_text("chip", 24.0);
    chip.y = 96.0;
    chip.width = 32.0;
    chip.height = 16.0;
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let restore_bounds = padded_xywh(halo, 16.0);
    let restore = PaintDirty {
        key: 9,
        bounds: restore_bounds,
        kind: PaintDirtyKind::OverlayRestore,
    };
    let page = vec![caption.clone(), chip.clone()];
    let stamps = [(9u64, panel)];
    let mut button_paint = slot(2, [10.0, 40.0, 220.0, 32.0], (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(
        4,
        [caption.x, caption.y, caption.width, caption.height],
        (1, 2),
    );
    caption_paint.text = (1, 2);
    let cleared = scissors_keeping_stable_page_captions(
        vec![restore_bounds],
        &[button_paint, caption_paint],
        &stamps,
        Some(halo),
    );
    let latin_x = caption.x + caption.width - 8.0;
    let latin_y = caption.y + caption.height * 0.5;
    let cjk_x = caption.x + 4.0;
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared
            .iter()
            .all(|r| !hits(r, latin_x, latin_y) && !hits(r, cjk_x, latin_y)),
        "fade clear/shadow must leave both scripts in retain, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(r, 100.0, 73.0)),
        "the gap between trigger and panel must still be rebuilt"
    );
    let page_draw = page_submit_excluding_live_node_capture(
        page.clone(),
        &page,
        &[restore],
        &[button_paint, caption_paint],
        &stamps,
        Some(halo),
        true,
    );
    assert!(
        page_draw.iter().all(|n| n.text != caption.text),
        "page glyphon runs before the shadow and would flicker the suffix, got {:?}",
        page_draw
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        page_draw.iter().any(|n| n.text == "chip"),
        "Restore still has to draw the panel chip"
    );
    let mut under = sample_text("under", 300.0);
    under.y = 80.0;
    under.width = 40.0;
    under.height = 16.0;
    let wide_shadow = [280.0, 60.0, 80.0, 50.0];
    let wide_restore = PaintDirty {
        key: 9,
        bounds: [0.0, 0.0, 400.0, 200.0],
        kind: PaintDirtyKind::OverlayRestore,
    };
    let under_page = vec![under.clone()];
    let under_draw = page_submit_excluding_live_node_capture(
        under_page.clone(),
        &under_page,
        &[wide_restore],
        &[],
        &stamps,
        Some(wide_shadow),
        true,
    );
    assert!(
        under_draw.iter().any(|n| n.text == "under"),
        "text fully inside the shadow is still cleared, so the page mesh must redraw it"
    );
}

#[test]
fn hover_during_overlay_fade_keeps_unchanged_suffix() {
    // Pointer stays on the trigger. Hover/focus transition dirties the
    // button, not the caption leaf. OverlayRestore is the open/close fade.
    let trigger = [10.0, 40.0, 220.0, 32.0];
    let caption = [18.0, 46.0, 140.0, 20.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let stamps = [(9u64, panel)];
    let coverage = crate::app::damage::overlay_replay_coverage(&stamps, Some(halo));
    let wipe = [10.0, 40.0, 220.0, 48.0];
    let pieces = crate::app::damage::page_scissors_excluding_portal_boxes(wipe, &stamps);
    let face = crate::app::damage::page_node_present_scissors_preserving_portal_halo(
        pieces, trigger, coverage, &stamps,
    );
    let button = PaintDirty {
        key: 2,
        bounds: trigger,
        kind: PaintDirtyKind::Node,
    };
    let restore = PaintDirty {
        key: 9,
        bounds: padded_xywh(halo, 16.0),
        kind: PaintDirtyKind::OverlayRestore,
    };
    let mut button_paint = slot(2, trigger, (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(4, caption, (0, 1));
    caption_paint.text = (0, 1);
    let paints = [button_paint, caption_paint];
    let hover_dirties = [restore, button];
    // Encode clears this face. Carving the caption out freezes the fill
    // under every glyph while hover/active/focus keeps easing, then the
    // click paints the page color into that hole — the whole line flashes.
    let cleared = node_scissors_keeping_unchanged_captions(face, &paints, &hover_dirties, &stamps);
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    let latin_x = caption[0] + caption[2] - 8.0;
    let latin_y = caption[1] + caption[3] * 0.5;
    let cjk_x = caption[0] + 4.0;
    assert!(
        cleared.iter().any(|r| hits(r, latin_x, latin_y))
            && cleared.iter().any(|r| hits(r, cjk_x, latin_y)),
        "hover fill must repaint under both scripts, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(r, 14.0, 42.0)),
        "button fill around the caption must still transition, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().all(|r| !hits(r, 100.0, 73.0)),
        "gap halo pixel must stay uncleared"
    );
    let mut line = sample_text("关闭 Builder Trigger", caption[0]);
    line.y = caption[1];
    line.width = caption[2];
    line.height = caption[3];
    let hover_capture = node_present_clipped_capture_text_wipes_excluding_overlay(
        &hover_dirties,
        &[line.clone()],
        &paints,
        &[(button.key, trigger)],
        &stamps,
        &[],
        1.0,
    );
    assert_eq!(
        hover_capture.len(),
        1,
        "the wiped caption must come back as one run, got {:?}",
        hover_capture
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    let clip = hover_capture[0].clip_bounds.expect("caption clip");
    assert!(
        hits(&clip, latin_x, latin_y) && hits(&clip, cjk_x, latin_y),
        "one clip must cover both scripts, got {clip:?}"
    );
    let ring = [4.0_f32, 34.0, 232.0, 44.0];
    let ring_capture = node_present_clipped_capture_text_wipes_excluding_overlay(
        &hover_dirties,
        &[line.clone()],
        &paints,
        &[(button.key, ring)],
        &stamps,
        &[],
        1.0,
    );
    assert_eq!(
        ring_capture.first().and_then(|n| n.clip_bounds),
        Some(clip),
        "a growing focus ring must not move the caption clip"
    );
    let text_dirty = PaintDirty {
        key: 4,
        bounds: caption,
        kind: PaintDirtyKind::Node,
    };
    let click_dirties = [restore, button, text_dirty];
    let click_clear = node_scissors_keeping_unchanged_captions(
        crate::app::damage::page_node_present_scissors_preserving_portal_halo(
            crate::app::damage::page_scissors_excluding_portal_boxes(wipe, &stamps),
            trigger,
            coverage,
            &stamps,
        ),
        &paints,
        &click_dirties,
        &stamps,
    );
    assert!(
        click_clear.iter().any(|r| hits(r, latin_x, latin_y))
            && click_clear.iter().any(|r| hits(r, cjk_x, latin_y)),
        "the click must repaint the button under both scripts, scissors={click_clear:?}"
    );
    let click_capture = node_present_clipped_capture_text_wipes_excluding_overlay(
        &click_dirties,
        &[line.clone()],
        &paints,
        &[(button.key, trigger), (text_dirty.key, caption)],
        &stamps,
        &[],
        1.0,
    );
    assert_eq!(
        click_capture.len(),
        1,
        "the changed caption is one run, got {:?}",
        click_capture
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        click_capture[0].clip_bounds,
        Some(clip),
        "click and hover must clip the line to the same box"
    );
}

#[test]
fn keyboard_toggle_flush_stays_inside_the_caption() {
    // Space/Enter toggles the label and the popover. The button is not
    // hovered, so the only Node hole is the caption. The flush scissor
    // must be that hole: the portal halo covers both scripts and the gap,
    // and the coverage kernel then src-overs the whole line every tick.
    let caption = [18.0, 46.0, 140.0, 20.0];
    let button = [10.0, 40.0, 220.0, 32.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let restore = PaintDirty {
        key: 9,
        bounds: padded_xywh(halo, 16.0),
        kind: PaintDirtyKind::OverlayRestore,
    };
    let mut button_paint = slot(2, button, (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(4, caption, (1, 2));
    caption_paint.text = (1, 2);
    let pieces = glyph_flush_logical_pieces(
        &[caption],
        &[restore],
        &[button_paint, caption_paint],
        &[(9u64, panel)],
        Some(halo),
    );
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    let latin_x = caption[0] + caption[2] - 8.0;
    let mid_y = caption[1] + caption[3] * 0.5;
    let cjk_x = caption[0] + 4.0;
    assert!(
        pieces
            .iter()
            .any(|r| hits(r, latin_x, mid_y) && hits(r, cjk_x, mid_y)),
        "one flush piece must cover both scripts, pieces={pieces:?}"
    );
    assert!(
        pieces
            .iter()
            .all(|r| !(hits(r, latin_x, mid_y) && hits(r, 100.0, 73.0))),
        "the caption scissor must not be the portal halo, pieces={pieces:?}"
    );
    assert!(
        pieces
            .iter()
            .all(|r| !(hits(r, latin_x, mid_y) && hits(r, 14.0, 42.0))),
        "button chrome outside the caption stays in retain, pieces={pieces:?}"
    );
    let region = PaintDirty {
        key: 3,
        bounds: [20.0, 90.0, 40.0, 28.0],
        kind: PaintDirtyKind::Region,
    };
    let with_region = glyph_flush_logical_pieces(&[], &[region], &[], &[], None);
    assert!(
        with_region.iter().any(|r| hits(r, 30.0, 100.0)),
        "a Region clear still has to flush the glyphs it captured"
    );
}

#[test]
fn keyboard_toggle_flush_never_blends_a_pixel_twice() {
    // Space/Enter dirties the caption and the fading popover. Separate
    // repair rectangles must stay disjoint after physical rounding so
    // the same antialiased edge is never blended twice.
    let caption = [18.0, 46.0, 140.0, 20.0];
    let button = [10.0, 40.0, 220.0, 32.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let restore = PaintDirty {
        key: 9,
        bounds: padded_xywh(halo, 16.0),
        kind: PaintDirtyKind::OverlayRestore,
    };
    let mut button_paint = slot(2, button, (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(4, caption, (1, 2));
    caption_paint.text = (1, 2);
    let pieces = glyph_flush_logical_pieces(
        &[caption],
        &[restore],
        &[button_paint, caption_paint],
        &[(9u64, panel)],
        Some(halo),
    );
    assert!(
            pieces.len() > 1,
            "precondition: the caption hole and the portal fringe are separate scissors, pieces={pieces:?}"
        );
    let physical: Vec<_> = pieces
        .iter()
        .filter_map(|r| {
            crate::app::damage::damage_to_scissor_physical(*r, 0.0, 1.25, 400, 250)
                .map(|(x, y, w, h)| [x as f32, y as f32, w as f32, h as f32])
        })
        .collect();
    let scissors = capture_glyph_scissors_physical(&physical, 400, 250);
    for (i, a) in scissors.iter().enumerate() {
        for b in &scissors[i + 1..] {
            assert!(
                intersect_rects(*a, *b).is_none(),
                "shared glyph pixels: {a:?} {b:?}"
            );
        }
    }
}

#[test]
fn menu_side_shadow_is_not_carved_by_neighboring_calendar_cells() {
    let shadow = [78.0, 58.0, 184.0, 224.0];
    for height in [180.0, 36.0] {
        let stamp = [100.0, 80.0, 140.0, height];
        let region = crate::app::damage::portal_composite_damage(
            &[
                PaintDirty {
                    key: 9,
                    bounds: stamp,
                    kind: PaintDirtyKind::Overlay,
                },
                PaintDirty {
                    key: 9,
                    bounds: shadow,
                    kind: PaintDirtyKind::OverlayRestore,
                },
                PaintDirty {
                    key: 4,
                    bounds: [240.0, 194.0, 32.0, 32.0],
                    kind: PaintDirtyKind::Node,
                },
            ],
            Some(shadow),
            [0.0, 0.0, 1000.0, 640.0],
        )
        .expect("portal changes require a composite region");
        assert!(
            crate::app::damage::rect_contains_rect(region, shadow),
            "lower-z calendar cells must not carve the portal composite: {region:?}"
        );
    }
    let day = PaintDirty {
        key: 4,
        bounds: [240.0, 194.0, 32.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    assert!(crate::app::damage::portal_composite_damage(
        &[day],
        Some(shadow),
        [0.0, 0.0, 1000.0, 640.0]
    )
    .is_some());
    assert!(
        crate::app::damage::portal_composite_damage(&[day], None, [0.0, 0.0, 1000.0, 640.0])
            .is_none()
    );
    assert!(crate::app::damage::portal_composite_damage(
        &[day],
        Some([400.0, 58.0, 100.0, 224.0]),
        [0.0, 0.0, 1000.0, 640.0],
    )
    .is_none());
}

#[test]
fn shrinking_menu_restore_clears_captions_in_the_vacated_strip() {
    // Select expand-up: the restore hole is the previous menu, the live
    // occupancy is the shorter panel. A calendar/page caption in the strip
    // the menu just left must be cleared. Passing only the live box carves
    // that caption out, and the last close frame never revisits it.
    let previous = [100.0, 80.0, 140.0, 180.0];
    let live = [100.0, 80.0, 140.0, 36.0];
    let day = [112.0, 200.0, 32.0, 32.0];
    let mut day_paint = slot(4, day, (0, 1));
    day_paint.text = (0, 1);
    let shadow = overlay_restore_caption_shadow(previous, Some(live));
    let cleared = scissors_keeping_stable_page_captions(
        vec![previous],
        &[day_paint],
        &[(9u64, live)],
        Some(shadow),
    );
    let cx = day[0] + day[2] * 0.5;
    let cy = day[1] + day[3] * 0.5;
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared.iter().any(|r| hits(r, cx, cy)),
        "vacated caption must be cleared with the old menu, scissors={cleared:?}"
    );
    assert!(
        crate::app::damage::rect_contains_rect(shadow, previous),
        "shadow must cover the previous menu, not only the live panel"
    );
}

#[test]
fn closing_menu_does_not_carve_calendar_rows_out_of_the_hole() {
    // A week row contains the day and misses the shrunk stamp, so it looks
    // like the stable trigger. Carving it leaves the old menu in bands.
    let previous = [100.0, 80.0, 140.0, 180.0];
    let live = [100.0, 80.0, 140.0, 36.0];
    let week = [40.0, 190.0, 320.0, 40.0];
    let day = [112.0, 194.0, 32.0, 32.0];
    let mut week_paint = slot(3, week, (0, 0));
    week_paint.text = (0, 0);
    let mut day_paint = slot(4, day, (0, 1));
    day_paint.text = (0, 1);
    let shadow = overlay_restore_caption_shadow(previous, Some(live));
    let cleared = scissors_keeping_stable_page_captions(
        vec![previous],
        &[week_paint, day_paint],
        &[(9u64, live)],
        Some(shadow),
    );
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared.iter().any(|r| hits(r, 170.0, 210.0)),
        "calendar row inside the old menu must be cleared, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(r, 170.0, 100.0)),
        "the live menu body must stay one piece, scissors={cleared:?}"
    );
}

#[test]
fn vacated_shadow_fringe_stays_in_the_restore_scissor() {
    // Day cells carve the old menu. The leftover under them is a <8px
    // shadow cap. It is inside the hole and outside the live panel, so
    // the overlay will not repaint it.
    let hole = [100.0, 80.0, 140.0, 200.0];
    let live = [100.0, 80.0, 140.0, 36.0];
    let day = [100.0, 200.0, 140.0, 74.0];
    let mut day_paint = slot(4, day, (0, 1));
    day_paint.text = (0, 1);
    let cleared = scissors_keeping_stable_page_captions(
        vec![hole],
        &[day_paint],
        &[(9u64, live)],
        Some(hole),
    );
    let x = hole[0] + hole[2] * 0.5;
    let y = hole[1] + hole[3] - 3.0;
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared.iter().any(|r| hits(r, x, y)),
        "vacated shadow cap must be cleared, scissors={cleared:?}"
    );
}

#[test]
fn fade_shadow_over_caption_must_not_clear_the_line() {
    // Popup Md halo (blur 10, outset ~23px) plus the enter slide covers the
    // trigger caption. Leaving that face in the restore clear wipes both
    // scripts on every opacity tick.
    let caption = [18.0, 46.0, 140.0, 20.0];
    let button = [10.0, 40.0, 220.0, 32.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [0.0, 40.0, 240.0, 160.0];
    assert!(
        crate::app::damage::rect_contains_rect(halo, caption),
        "precondition: the animated halo contains the caption"
    );
    let mut button_paint = slot(2, button, (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(4, caption, (0, 1));
    caption_paint.text = (0, 1);
    let cleared = scissors_keeping_stable_page_captions(
        vec![padded_xywh(halo, 16.0)],
        &[button_paint, caption_paint],
        &[(9u64, panel)],
        Some(halo),
    );
    let latin_x = caption[0] + caption[2] - 8.0;
    let latin_y = caption[1] + caption[3] * 0.5;
    let cjk_x = caption[0] + 4.0;
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared
            .iter()
            .all(|r| !hits(r, latin_x, latin_y) && !hits(r, cjk_x, latin_y)),
        "restore/shadow must not clear a trigger caption, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(r, 100.0, 90.0)),
        "panel pixels still have to be rebuilt"
    );
    let button_cx = button[0] + button[2] * 0.5;
    let button_cy = button[1] + button[3] * 0.5;
    assert!(
        cleared.iter().all(|r| !hits(r, button_cx, button_cy)),
        "restore must not punch the trigger face after its own dest-reset, scissors={cleared:?}"
    );
}

#[test]
fn region_repair_beside_open_portal_keeps_stable_trigger_out_of_clear() {
    // Opening ColorPicker can classify the trigger row as a Region dirty.
    // Rebuilding that unchanged row from the capture mesh makes the trigger
    // flicker and exposes a one-pixel caption seam beside the entering panel.
    let trigger = [606.0, 0.0, 114.0, 38.0];
    let panel = [512.0, 42.0, 280.0, 360.0];
    let region = [614.0, 0.0, 106.0, 38.0];
    let mut trigger_paint = slot(2, trigger, (0, 1));
    trigger_paint.text = (0, 1);
    let cleared = scissors_keeping_stable_page_captions(
        vec![region],
        &[trigger_paint],
        &[(9u64, panel)],
        None,
    );
    let center = [trigger[0] + trigger[2] * 0.5, trigger[1] + trigger[3] * 0.5];
    let hits = |r: &[f32; 4]| {
        center[0] >= r[0] && center[0] < r[0] + r[2] && center[1] >= r[1] && center[1] < r[1] + r[3]
    };
    assert!(
        cleared.iter().all(|r| !hits(r)),
        "stable trigger must not be cleared by the portal-open Region repair: {cleared:?}"
    );
}

#[test]
fn region_beside_popover_replays_the_grazed_line_prefix() {
    // The page region beside an open ColorPicker clears the card title.
    // Replaying only the halo fringe kept "orPic" and left the prefix blank.
    let line_box = [40.0, 200.0, 420.0, 22.0];
    let stamp = [360.0, 120.0, 240.0, 320.0];
    let region = [24.0, 100.0, 560.0, 360.0];
    let mut title = slot(4, line_box, (0, 1));
    title.text = (0, 1);
    let mut line = sample_text("2c) Inline Calendar · ColorPicker", line_box[0]);
    line.y = line_box[1];
    line.width = line_box[2];
    line.height = line_box[3];
    let overlay = PaintDirty {
        key: 9,
        bounds: stamp,
        kind: PaintDirtyKind::Overlay,
    };
    let region_dirty = PaintDirty {
        key: 3,
        bounds: region,
        kind: PaintDirtyKind::Region,
    };
    let (replay, flush) = overlay_grazing_caption_replay(
        &[overlay, region_dirty],
        &[title],
        &[(9u64, stamp)],
        Some(stamp),
        &[line],
        1.0,
    );
    let hits =
        |c: [f32; 4], x: f32, y: f32| x >= c[0] && x < c[0] + c[2] && y >= c[1] && y < c[1] + c[3];
    let y = line_box[1] + 8.0;
    assert!(
        replay
            .iter()
            .any(|n| n.clip_bounds.is_some_and(|c| hits(c, line_box[0] + 4.0, y))),
        "prefix cleared beside the popover must stay in TextBounds, got {:?}",
        replay.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
    assert!(
        replay
            .iter()
            .all(|n| n.clip_bounds.is_none_or(|c| !hits(c, 900.0, y))),
        "replay must not cover pixels outside the cleared region"
    );
    assert!(
        replay.iter().all(|n| {
            n.clip_bounds.is_none_or(|clip| {
                flush.iter().any(|f| {
                    f[0] <= clip[0]
                        && f[1] <= clip[1]
                        && f[0] + f[2] >= clip[0] + clip[2]
                        && f[1] + f[3] >= clip[1] + clip[3]
                })
            })
        }),
        "caption flush must cover the padded replay clip: {flush:?}"
    );
}

#[test]
fn region_repair_reopens_changed_trigger_but_keeps_idle_caption_out() {
    let idle = [18.0, 46.0, 96.0, 20.0];
    let trigger = [132.0, 46.0, 92.0, 30.0];
    let region = [8.0, 30.0, 280.0, 100.0];
    let paints = [slot(1, idle, (0, 1)), slot(2, trigger, (1, 2))];
    let dirties = [
        PaintDirty {
            key: 9,
            bounds: region,
            kind: PaintDirtyKind::Region,
        },
        PaintDirty {
            key: 2,
            bounds: trigger,
            kind: PaintDirtyKind::Node,
        },
    ];
    let kept = region_scissors_keeping_stable_page_captions(
        vec![region],
        &dirties[0],
        &dirties,
        &paints,
        &[(7, [8.0, 90.0, 80.0, 20.0])],
        None,
    );
    let hits = |rect: [f32; 4], point: [f32; 2]| {
        point[0] >= rect[0]
            && point[0] < rect[0] + rect[2]
            && point[1] >= rect[1]
            && point[1] < rect[1] + rect[3]
    };
    assert!(
        kept.iter().any(|r| hits(*r, [150.0, 55.0])),
        "changed trigger must remain in the region clear: {kept:?}"
    );
    assert!(
        kept.iter().all(|r| !hits(*r, [30.0, 55.0])),
        "idle caption must stay out of the clear: {kept:?}"
    );
}

#[test]
fn page_mesh_keeps_title_prefix_outside_the_halo_replay() {
    // Opening the picker removes the grazing line from the page mesh and
    // replays only the shadow fringe. A full present then shows "orPic".
    // The letters left of that fringe stay on the page run.
    let mut line = sample_text("2c) Inline Calendar · ColorPicker", 0.0);
    line.y = 46.0;
    line.width = 720.0;
    line.height = 24.0;
    let mut replay = line.clone();
    replay.clip_bounds = Some([475.0, 46.0, 48.0, 24.0]);
    let page = page_runs_excluding_caption_replay(vec![line], &[replay]);
    let hits =
        |c: [f32; 4], x: f32| x >= c[0] && x < c[0] + c[2] && 50.0 >= c[1] && 50.0 < c[1] + c[3];
    assert!(
        page.iter()
            .any(|n| n.clip_bounds.is_some_and(|c| hits(c, 40.0))),
        "prefix stays in the page mesh, got {:?}",
        page.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
    assert!(
        page.iter()
            .all(|n| n.clip_bounds.is_none_or(|c| !hits(c, 490.0))),
        "fringe stays with the halo replay, got {:?}",
        page.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
    assert!(
        page.iter()
            .any(|n| n.clip_bounds.is_some_and(|c| hits(c, 700.0))),
        "tail past the popover stays in the page mesh, got {:?}",
        page.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
}

#[test]
fn avatar_expand_halo_keeps_neighbor_caption() {
    // The +N pill grazes the label above it. The top shadow lives in that
    // line, so the clear must keep those pixels. Glyphs are replayed after
    // the overlay; the tail outside the halo stays in retain.
    let caption = [16.0, 40.0, 420.0, 18.0];
    let stamp = [16.0, 54.0, 220.0, 40.0];
    let halo = [-4.0, 34.0, 268.0, 80.0];
    let encode = padded_xywh(halo, 16.0);
    let mut caption_paint = slot(4, caption, (0, 1));
    caption_paint.text = (0, 1);
    let cleared = scissors_keeping_stable_page_captions(
        vec![encode],
        &[caption_paint],
        &[(9u64, stamp)],
        Some(halo),
    );
    let hits =
        |r: [f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        encode[0] <= 80.0
            && 80.0 < encode[0] + encode[2]
            && encode[1] <= 46.0
            && 46.0 < encode[1] + encode[3],
        "precondition: encode pad covers the caption"
    );
    assert!(
        cleared.iter().any(|r| hits(*r, 80.0, 46.0)),
        "top shadow sits in the caption line and must stay in the clear, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(*r, 80.0, 72.0)),
        "pill pixels still clear, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().all(|r| !hits(*r, 400.0, 46.0)),
        "caption tail outside the halo stays in retain, scissors={cleared:?}"
    );
    let mut line = sample_text("悬停 / 点击 +N", caption[0]);
    line.y = caption[1];
    line.width = caption[2];
    line.height = caption[3];
    let replay = grazing_caption_replay_runs(&[caption_paint], &[(9u64, stamp)], &[line], &cleared);
    assert!(
        replay.iter().any(|n| {
            n.clip_bounds
                .is_some_and(|c| hits(c, 80.0, 46.0) && c[3] >= caption[3] - 1.0)
        }),
        "replay keeps the full line so the glyph is not dropped, got {:?}",
        replay.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
    assert!(
        replay
            .iter()
            .all(|n| n.clip_bounds.is_none_or(|c| !hits(c, 400.0, 46.0))),
        "replay must not src-over the uncleared tail, got {:?}",
        replay.iter().map(|n| n.clip_bounds).collect::<Vec<_>>()
    );
}

#[test]
fn grazing_caption_replay_keeps_disjoint_restore_and_live_halo_pieces() {
    let mut line = sample_text("expand_placement / expand_class", 16.25);
    line.y = 30.25;
    line.width = 280.0;
    line.height = 32.0;
    let paint = slot(4, [16.25, 30.25, 280.0, 32.0], (0, 1));
    let cleared = [[8.25, 39.25, 170.0, 24.0], [248.25, 39.25, 18.0, 24.0]];
    let replay = grazing_caption_replay_runs(
        &[paint],
        &[(9, [7.25, 55.25, 140.0, 40.0])],
        &[line],
        &cleared,
    );
    assert_eq!(
        replay.len(),
        1,
        "one source must repair all cleared pieces without repeated blending"
    );
    let clip = replay[0].clip_bounds.unwrap();
    assert!(
        clip[0] <= 248.25 && clip[0] + clip[2] >= 266.25,
        "the narrow previous-footprint restore must retain glyphs too: {clip:?}"
    );
}

#[test]
fn grazing_caption_replay_does_not_inherit_full_viewport_clip_height() {
    // A collected text node commonly carries the page/scrollport clip.
    // Replay must keep the complete glyph line horizontally, while its
    // vertical extent stays bounded by that line. Inheriting a 600px
    // viewport clip makes ColorPicker title repairs touch unrelated rows.
    let mut line = sample_text("Calendar · ColorPicker", 0.0);
    line.y = 46.0;
    line.width = 720.0;
    line.height = 24.0;
    line.clip_bounds = Some([0.0, 0.0, 800.0, 600.0]);
    let paint = slot(4, [0.0, 46.0, 720.0, 24.0], (0, 1));
    let replay = grazing_caption_replay_runs(
        &[paint],
        &[(9u64, [512.0, 42.0, 280.0, 360.0])],
        &[line],
        &[[476.0, 39.0, 352.0, 399.0]],
    );
    let clip = replay
        .first()
        .and_then(|n| n.clip_bounds)
        .expect("title must be replayed");
    assert!(clip[3] <= 26.0, "replay escaped the text line: {clip:?}");
    assert!(
        clip[1] >= 45.0,
        "replay started above the text line: {clip:?}"
    );
}

#[test]
fn grazing_caption_replay_deduplicates_shared_parent_and_leaf_text() {
    let line = sample_text("原地向下盖住展开", 8.0);
    let mut parent = slot(1, [0.0, 0.0, 120.0, 20.0], (0, 1));
    let mut leaf = slot(2, [0.0, 0.0, 120.0, 20.0], (0, 1));
    parent.text = (0, 1);
    leaf.text = (0, 1);
    let cleared = [[0.0, 0.0, 120.0, 20.0]];
    let replay = grazing_caption_replay_runs(
        &[parent, leaf],
        &[(9u64, [50.0, 10.0, 80.0, 40.0])],
        &[line],
        &cleared,
    );
    assert_eq!(
        replay.len(),
        1,
        "the same TextLayoutInfo must have one late replay source, got {:?}",
        replay.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn capture_glyph_scissors_exclude_current_portal_pixels() {
    let pieces = capture_glyph_scissors_excluding_portals_physical(
        &[[0.0, 0.0, 100.0, 30.0]],
        &[[40.0, 0.0, 20.0, 30.0]],
        100,
        30,
    );
    assert_eq!(
        pieces,
        vec![[0.0, 0.0, 40.0, 30.0], [60.0, 0.0, 40.0, 30.0]]
    );
}

#[test]
fn capture_glyph_scissors_keep_fractional_portal_edge_pixels() {
    let pieces = capture_glyph_scissors_excluding_portals_physical(
        &[[0.0, 0.0, 100.0, 100.0]],
        &[[16.25, 51.35, 220.0, 20.2]],
        100,
        100,
    );
    assert!(
        pieces.iter().any(|r| r[1] == 0.0 && r[3] == 52.0),
        "the partially covered top edge must remain available for underlay replay: {pieces:?}"
    );
    assert!(
        pieces.iter().any(|r| r[1] == 71.0),
        "the partially covered bottom edge must remain available for underlay replay: {pieces:?}"
    );
}

#[test]
fn fade_overlay_keeps_stable_suffix_out_of_the_clear() {
    // 打开 and 关闭 share this suffix box. The restore pad contains it, so
    // the old clear wiped "RSX Trigger" on every opacity tick.
    let caption = [18.0, 46.0, 140.0, 20.0];
    let button = [10.0, 40.0, 220.0, 32.0];
    let panel = [10.0, 76.0, 220.0, 96.0];
    let halo = [4.0, 54.0, 232.0, 120.0];
    let restore = padded_xywh(halo, 16.0);
    let latin_x = caption[0] + caption[2] - 8.0;
    let latin_y = caption[1] + caption[3] * 0.5;
    let mut button_paint = slot(2, button, (0, 1));
    button_paint.text = (0, 1);
    let mut caption_paint = slot(4, caption, (1, 2));
    caption_paint.text = (1, 2);
    let stamps = [(9u64, panel)];
    let cleared = scissors_keeping_stable_page_captions(
        vec![restore],
        &[button_paint, caption_paint],
        &stamps,
        Some(halo),
    );
    let hits =
        |r: &[f32; 4], x: f32, y: f32| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    assert!(
        cleared.iter().all(|r| !hits(r, latin_x, latin_y)),
        "fade clear/shadow must leave the stable suffix, scissors={cleared:?}"
    );
    assert!(
        cleared.iter().any(|r| hits(r, 100.0, 73.0)),
        "the gap between trigger and panel must still be rebuilt, scissors={cleared:?}"
    );
    let chip = [24.0, 96.0, 32.0, 16.0];
    let mut chip_paint = slot(3, chip, (2, 3));
    chip_paint.text = (2, 3);
    let with_chip = scissors_keeping_stable_page_captions(
        vec![restore],
        &[button_paint, caption_paint, chip_paint],
        &stamps,
        Some(halo),
    );
    // A label that sits entirely in the shadow, and is not the trigger
    // caption, must still be cleared. Punching it would leave a bright hole.
    let beside = [300.0, 80.0, 40.0, 16.0];
    let wide_shadow = [280.0, 60.0, 80.0, 50.0];
    let mut beside_paint = slot(8, beside, (3, 4));
    beside_paint.text = (3, 4);
    let with_beside = scissors_keeping_stable_page_captions(
        vec![[0.0, 0.0, 400.0, 200.0]],
        &[beside_paint],
        &stamps,
        Some(wide_shadow),
    );
    assert!(
        with_beside.iter().any(|r| hits(r, 310.0, 88.0)),
        "text fully inside the shadow stays in the scissor, scissors={with_beside:?}"
    );
    assert!(
        with_chip.iter().any(|r| hits(r, 30.0, 100.0)),
        "a chip inside the panel stamp stays in the restore scissor"
    );
}

#[test]
fn send_insert_before_rail_must_not_submit_unchanged_captions() {
    let mut layout = sample_text("布局", 8.0);
    layout.y = 200.0;
    let mut gallery = sample_text("Gallery", 8.0);
    gallery.y = 236.0;
    let mut next = sample_text("next", 260.0);
    next.y = 200.0;
    let prev = vec![layout.clone(), gallery.clone()];
    let curr = vec![next.clone(), layout, gallery];
    let submitted = page_stack_band_submit_text(&prev, &curr, 0, 3, None, true)
        .expect("inserted bubble must submit");
    assert!(submitted.iter().any(|n| n.text == "next"));
    assert!(
        submitted
            .iter()
            .all(|n| n.text != "布局" && n.text != "Gallery"),
        "index shift must not re-submit rail ink, got {:?}",
        submitted
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn clip_bounds_only_change_must_not_resubmit_rail() {
    let mut prev = sample_text("布局", 8.0);
    prev.y = 200.0;
    let mut curr = prev.clone();
    curr.clip_bounds = Some([0.0, 0.0, 208.0, 800.0]);
    assert!(
        page_stack_band_submit_text(&[prev], &[curr], 0, 1, None, true).is_none(),
        "layout-rebuilt clip is not new ink"
    );
}

#[test]
fn node_dirty_owns_centered_label_not_sibling() {
    let dirty = [8.0, 8.0, 88.0, 36.0];
    assert!(text_owned_by_node_dirty(
        &sample_text("Default", 24.0),
        dirty
    ));
    assert!(!text_owned_by_node_dirty(
        &sample_text("Outline", 104.0),
        dirty
    ));
}

#[test]
fn chrome_filter_drops_far_rects() {
    let kept = filter_instances(
        &[
            blank_instance(0.0, 0.0, 10.0, 10.0),
            blank_instance(400.0, 0.0, 10.0, 10.0),
        ],
        CullRects::one([0.0, 0.0, 40.0, 40.0]),
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].rect_pos, [0.0, 0.0]);
}

#[test]
fn physical_overlay_instance_converts_to_logical_occupancy() {
    let mut i = blank_instance(800.0, 400.0, 240.0, 176.0);
    i.shadow_blur = 16.0;
    i.shadow_color = [0.0, 0.0, 0.0, 0.4];
    let logical = scale_xywh(instance_paint_bounds(&i), 0.5);
    assert!(logical[2] > 120.0);
    assert!(logical[3] > 88.0);
    assert!(logical[0] < 400.0);
    assert!(logical[1] < 200.0);
}

#[test]
fn opaque_fill_skips_transparent_and_picks_topmost() {
    let mut card = blank_instance(0.0, 0.0, 100.0, 100.0);
    card.color = [0.9, 0.9, 0.9, 1.0];
    let mut link = blank_instance(10.0, 10.0, 40.0, 20.0);
    link.color = [0.0, 0.0, 0.0, 0.0];
    assert_eq!(
        opaque_fill_color_at(&[card, link], 20.0, 15.0),
        Some([0.9, 0.9, 0.9, 1.0])
    );
}

#[test]
fn node_dirty_has_opaque_fill_false_for_transparent_slot() {
    let mut inst = blank_instance(0.0, 0.0, 40.0, 20.0);
    inst.color = [0.0; 4];
    let p = NodePaint {
        key: 1,
        bounds: [0.0, 0.0, 40.0, 20.0],
        overlay: false,
        instance: (0, 1),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    assert!(!node_dirty_has_opaque_fill(
        1,
        [0.0, 0.0, 40.0, 20.0],
        &[p],
        &[inst]
    ));
    inst.color = [0.0, 0.0, 0.0, 1.0];
    assert!(node_dirty_has_opaque_fill(
        1,
        [0.0, 0.0, 40.0, 20.0],
        &[p],
        &[inst]
    ));
}

#[test]
fn node_reset_ignores_foreground_chrome_when_resolving_underlay() {
    let bounds = [20.0, 20.0, 100.0, 32.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 100.0);
    page.color = [0.95, 0.95, 0.95, 1.0];
    let mut button = blank_instance(20.0, 20.0, 100.0, 32.0);
    button.color = [1.0, 1.0, 1.0, 0.08];
    button.radii = [4.0; 4];
    let mut foreground = blank_instance(110.0, 10.0, 20.0, 60.0);
    foreground.color = [0.0, 0.4, 0.8, 1.0];
    foreground.radii = [8.0; 4];
    let paint = paint_slot(7, bounds, Some(bounds), (1, 2));
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let mut instances = [page, button, foreground];
        for instance in &mut instances {
            instance.rect_pos = instance.rect_pos.map(|value| value * scale);
            instance.rect_size = instance.rect_size.map(|value| value * scale);
            instance.radii = instance.radii.map(|value| value * scale);
        }
        let reset = node_present_dest_reset_full(
            7,
            bounds,
            &[paint.clone()],
            &instances,
            &[],
            &[],
            scale,
            None,
        )
        .expect("page underlay");
        assert_eq!(reset.clear, page.color, "scale={scale}");
        assert_eq!(reset.replay, (0, 0), "scale={scale}");
    }
}

#[test]
fn patterned_underlay_requires_region_replay_for_hover_reset() {
    let mut page = blank_instance(0.0, 0.0, 200.0, 100.0);
    page.color = [0.05, 0.05, 0.05, 1.0];
    let mut button = blank_instance(20.0, 20.0, 100.0, 32.0);
    button.color = [1.0, 1.0, 1.0, 0.08];
    button.radii = [4.0; 4];
    let paint = NodePaint {
        key: 7,
        bounds: [20.0, 20.0, 100.0, 32.0],
        instance: (2, 3),
        overlay: false,
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    for stripe_x in [30.0, 65.0] {
        let mut stripe = blank_instance(stripe_x, 0.0, 12.0, 100.0);
        stripe.color = [0.0, 0.4, 0.8, 1.0];
        let reset = node_present_dest_reset_full(
            7,
            paint.bounds,
            &[paint.clone()],
            &[page, stripe, button],
            &[],
            &[],
            1.0,
            None,
        )
        .unwrap();
        assert_eq!(
            reset.clear[3], 0.0,
            "patterned underlay needs ordered region replay"
        );
    }
    let mut clipped = page;
    clipped.color = [0.0, 0.4, 0.8, 1.0];
    clipped.clip_pos = [0.0, 0.0];
    clipped.clip_size = [80.0, 100.0];
    let reset = node_present_dest_reset_full(
        7,
        paint.bounds,
        &[paint],
        &[page, clipped, button],
        &[],
        &[],
        1.0,
        None,
    )
    .unwrap();
    assert_eq!(
        reset.clear[3], 0.0,
        "clipped fill must cover the complete reset"
    );
}

#[test]
fn translucent_underlay_must_not_become_an_opaque_hover_reset() {
    let mut page = blank_instance(0.0, 0.0, 200.0, 100.0);
    page.color = [0.05, 0.05, 0.05, 1.0];
    let mut material = page;
    material.color = [0.8, 0.8, 0.8, 0.18];
    let mut button = blank_instance(20.0, 20.0, 100.0, 32.0);
    button.color = [1.0, 1.0, 1.0, 0.08];
    button.radii = [4.0; 4];
    let paint = NodePaint {
        key: 7,
        bounds: [20.0, 20.0, 100.0, 32.0],
        instance: (2, 3),
        overlay: false,
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let reset = node_present_dest_reset_full(
        7,
        paint.bounds,
        &[paint],
        &[page, material, button],
        &[],
        &[],
        1.0,
        None,
    )
    .unwrap();
    assert_eq!(reset.clear, page.color);
    assert_eq!(reset.replay, (1, 2));
}

#[test]
fn dest_reset_skips_self_and_uses_card_behind() {
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let mut button = blank_instance(10.0, 10.0, 40.0, 20.0);
    button.color = [0.1, 0.1, 0.1, 1.0];
    let p = NodePaint {
        key: 7,
        bounds: [10.0, 10.0, 40.0, 20.0],
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let instances = [card, button];
    assert_eq!(
        node_present_behind_color(7, p.bounds, &[p], &instances, 30.0, 20.0),
        Some([0.96, 0.96, 0.96, 1.0])
    );
    let (rect, color) =
        node_present_dest_reset(7, p.bounds, &[p], &instances).expect("card behind default");
    assert_eq!(color, [0.96, 0.96, 0.96, 1.0]);
    assert!(rect[0] <= 10.0 - 2.0);
    assert!(rect[1] <= 10.0 - 2.0);
    assert!(rect[0] + rect[2] >= 50.0 + 2.0);
    assert!(rect[1] + rect[3] >= 30.0 + 2.0);
    assert!(rect[2] < 80.0, "must not expand to the whole card");
    assert!(node_present_needs_glyphs(7, p.bounds, &[p], &instances));
}

#[test]
fn dest_reset_covers_rotated_textured_spin_not_just_unrotated_fill() {
    // Icon+spin is a lucide texture + transform. A same-slot unrotated fill
    // used to win dest-reset; the previous 45° corners stayed in retain.
    let mut card = blank_instance(0.0, 0.0, 80.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(32.0, 32.0, 16.0, 16.0);
    fill.color = [0.0, 0.0, 0.0, 0.0];
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let spin = blank_textured(32.0, 32.0, 16.0, 16.0, [c, s, -s, c, 0.0, 0.0]);
    let p = NodePaint {
        key: 11,
        bounds: [32.0, 32.0, 16.0, 16.0],
        overlay: false,
        instance: (1, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let instances = [card, fill];
    let (without_tex, _) =
        node_present_dest_reset(11, p.bounds, &[p], &instances).expect("fill fallback");
    let (with_tex, _) =
        node_present_dest_reset_scaled(11, p.bounds, &[p], &instances, &[spin], 1.0, None)
            .expect("spin behind card");
    let spun = crate::renderer::aabb_of_centered_transform(
        [32.0, 32.0, 16.0, 16.0],
        &[c, s, -s, c, 0.0, 0.0],
    );
    assert!(
        with_tex[0] <= spun[0] + 0.5,
        "wipe must cover the rotated quad, not only the 16×16 fill\n\
             fill_wipe={without_tex:?} spin_wipe={with_tex:?} spun={spun:?}"
    );
    assert!(with_tex[1] <= spun[1] + 0.5);
    assert!(with_tex[0] + with_tex[2] >= spun[0] + spun[2] - 0.5);
    assert!(with_tex[1] + with_tex[3] >= spun[1] + spun[3] - 0.5);
    assert!(
        with_tex[2] > without_tex[2] + 1.0 || with_tex[3] > without_tex[3] + 1.0,
        "rotated ink must expand the dest-reset beyond the unrotated fill"
    );
}

#[test]
fn dest_reset_spin_must_wipe_circumcircle_at_identity_angle() {
    // 0° ink is the unrotated box. Leftover 45° coverage is last frame's
    // transformed AABB (`prev_bounds`), not this frame's scissor-padded dirty.
    let mut card = blank_instance(0.0, 0.0, 80.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(32.0, 32.0, 16.0, 16.0);
    fill.color = [0.0, 0.0, 0.0, 0.0];
    let box16 = [32.0, 32.0, 16.0, 16.0];
    let p = NodePaint {
        key: 12,
        bounds: box16,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let spun = crate::renderer::aabb_of_centered_transform(box16, &[c, s, -s, c, 0.0, 0.0]);
    let (wipe, _) =
        node_present_dest_reset_scaled(12, box16, &[p], &[card, fill], &[], 1.0, Some(spun))
            .expect("card behind spin");
    assert!(
        wipe[0] <= spun[0] + 0.5
            && wipe[1] <= spun[1] + 0.5
            && wipe[0] + wipe[2] >= spun[0] + spun[2] - 0.5
            && wipe[1] + wipe[3] >= spun[1] + spun[3] - 0.5,
        "0° dest-reset must still cover the 45° circumcircle\n wipe={wipe:?} spun={spun:?}"
    );
    assert!(
        wipe[2] < 80.0 && wipe[3] < 80.0,
        "must not expand to the whole card, got {wipe:?}"
    );
}

#[test]
fn dest_reset_unions_previous_visual_slot() {
    let mut card = blank_instance(0.0, 0.0, 200.0, 200.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let mut icon = blank_instance(32.0, 32.0, 16.0, 16.0);
    icon.color = [0.0, 0.0, 0.0, 0.0];
    let p = NodePaint {
        key: 4,
        bounds: [32.0, 32.0, 16.0, 16.0],
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let prev = [32.0, 112.0, 16.0, 16.0];
    let (rect, _) =
        node_present_dest_reset_scaled(4, p.bounds, &[p], &[card, icon], &[], 1.0, Some(prev))
            .expect("card behind");
    assert!(
        rect[1] <= 32.0 && rect[1] + rect[3] >= 112.0 + 16.0,
        "wipe must cover previous visual Y and now, got {rect:?}"
    );
}

#[test]
fn sidebar_hover_scissor_ignores_forms_viewport_to_the_right() {
    let bounds = [8.0, 125.0, 239.0, 32.0];
    let sidebar = NodePaint {
        key: 1,
        bounds,
        overflow_clip: Some([0.0, 49.0, 255.0, 898.0]),
        instance: (0, 1),
        overlay: false,
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 1),
        layered: false,
        rest: Some(bounds),
    };
    // On a wide window this textarea clip exceeds the sidebar's area.
    let forms = NodePaint {
        key: 2,
        bounds: [312.0, 660.0, 1344.0, 200.0],
        overflow_clip: Some([312.0, 660.0, 1344.0, 200.0]),
        ..sidebar
    };
    let paints = [sidebar, forms];
    let scissor = node_present_scissor_logical(PaintDirtyKind::Node, 1, bounds, &[], &paints, 8.0);
    assert!(crate::app::damage::rect_contains_rect(scissor, bounds));
    assert!(
        scrolled_out_top_into_title([320.0, 600.0, 80.0, 32.0], None, &paints,),
        "a genuinely culled Forms node must still be rejected"
    );
}

#[test]
fn dest_reset_scrolled_out_unmatched_must_not_punch_above_overflow() {
    let mut page = blank_instance(0.0, 0.0, 400.0, 212.0);
    page.color = [0.94, 0.94, 0.95, 1.0];
    let mut title = blank_instance(0.0, 0.0, 400.0, 52.0);
    title.color = [1.0, 1.0, 1.0, 1.0];
    let mut body = blank_instance(0.0, 52.0, 400.0, 160.0);
    body.color = [0.96, 0.96, 0.96, 1.0];
    let scroller = NodePaint {
        key: 10,
        bounds: [0.0, 52.0, 400.0, 160.0],
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 52.0, 400.0, 160.0]),
        layered: false,
        rest: None,
    };
    // Culled spin: prev∪now strip through the title, key not in node_paints.
    let dirty = [-8.0, -156.0, 32.0, 228.0];
    let (wipe, _) = node_present_dest_reset_scaled(
        99,
        dirty,
        &[scroller],
        &[page, title, body],
        &[],
        1.0,
        Some([0.0, 54.0, 16.0, 16.0]),
    )
    .expect("clipped leftover");
    assert!(
        wipe[1] + 0.5 >= 52.0,
        "scrolled-out unmatched dest-reset must stay in the ScrollArea\n wipe={wipe:?}"
    );
    assert!(
        wipe[1] > 24.0 || wipe[1] + wipe[3] <= 24.0,
        "must not punch the title bar\n wipe={wipe:?}"
    );
    let scissor = node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        99,
        dirty,
        &[(99, [0.0, 54.0, 16.0, 16.0])],
        &[scroller],
        8.0,
    );
    assert!(
        scissor[3] <= 0.5 || scissor[1] + 0.5 >= 52.0,
        "scrolled-out unmatched scissor must stay in the ScrollArea\n scissor={scissor:?}"
    );
    let region = node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Region,
        99,
        dirty,
        &[(99, [0.0, 54.0, 16.0, 16.0])],
        &[scroller],
        0.0,
    );
    assert!(
        region[3] <= 0.5 || region[1] + 0.5 >= 52.0,
        "culled-shimmer Region scissor must stay in the ScrollArea\n scissor={region:?}"
    );
    assert!(
        !rect_contains_point(region, 8.0, 12.0),
        "culled-shimmer Region scissor punches 基础\n scissor={region:?}"
    );
    // Next shimmer tick: last-present already dropped the culled key, dirty
    // sits in the title bar. Must not CLEAR 「基础」 just because prev is gone.
    let title_phantom = [0.0, 12.0, 400.0, 32.0];
    assert!(
        node_present_dest_reset_scaled(
            99,
            title_phantom,
            &[scroller],
            &[page, title, body],
            &[],
            1.0,
            None,
        )
        .is_none(),
        "culled shimmer dest-reset with no last-present must not punch 基础"
    );
    let phantom_scissor = node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        99,
        title_phantom,
        &[],
        &[scroller],
        8.0,
    );
    assert!(
            phantom_scissor[2] <= 0.5 || phantom_scissor[3] <= 0.5,
            "culled shimmer scissor with no last-present must not punch 基础\n scissor={phantom_scissor:?}"
        );
    assert!(
        !rect_contains_point(phantom_scissor, 200.0, 12.0),
        "culled shimmer scissor with no last-present punches 基础\n scissor={phantom_scissor:?}"
    );
}

#[test]
fn dest_reset_works_for_transparent_ghost_on_card() {
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let mut ghost = blank_instance(60.0, 10.0, 40.0, 20.0);
    ghost.color = [0.2, 0.2, 0.2, 0.0];
    let p = NodePaint {
        key: 8,
        bounds: [60.0, 10.0, 40.0, 20.0],
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let instances = [card, ghost];
    assert!(!node_dirty_has_opaque_fill(8, p.bounds, &[p], &instances));
    let (_, color) =
        node_present_dest_reset(8, p.bounds, &[p], &instances).expect("card behind ghost");
    assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
    assert!(node_present_needs_glyphs(8, p.bounds, &[p], &instances));
}

#[test]
fn dest_reset_and_glyphs_for_text_only_bind_label() {
    // Gallery MouseEvent log: bind_text Label has glyphs, no SDF fill.
    // Node-present used to skip capture (needs_glyphs false) so mousemove
    // froze the retain buffer until a FullClear (click / focus).
    let mut card = blank_instance(0.0, 0.0, 400.0, 160.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let bounds = [12.0, 80.0, 376.0, 56.0];
    let label = slot(9, bounds, (0, 1));
    let instances = [card];
    assert!(
        node_dirty_has_text(9, bounds, &[label]),
        "text-only slot must be recognized as having glyphs"
    );
    assert!(
        node_present_needs_glyphs(9, bounds, &[label], &instances),
        "bind_text label with no fill must still capture glyphs"
    );
    let (rect, color) = node_present_dest_reset(9, bounds, &[label], &instances)
        .expect("must wipe the text box with the card behind it");
    assert_eq!(color, [0.94, 0.94, 0.95, 1.0]);
    assert!(rect[0] <= 12.0);
    assert!(rect[1] <= 80.0);
    assert!(rect[0] + rect[2] >= 388.0);
    assert!(rect[1] + rect[3] >= 136.0);
    assert!(rect[2] < 400.0, "must not expand to the whole card");

    // Gallery bind_text used to key dirty by bounds hash, not Taffy id.
    let bounds_keyed = PaintDirty {
        key: 0xDEAD_BEEF,
        bounds,
        kind: PaintDirtyKind::Node,
    };
    let page = [sample_text("mousemove", 20.0)];
    let owned = node_present_glyphs_for_dirty(&bounds_keyed, &page, &[label]);
    assert!(
        owned.contains(&0),
        "same-box dirty must still own the label glyphs without a Taffy key match"
    );
    let (rect2, _) =
        node_present_dest_reset(bounds_keyed.key, bounds_keyed.bounds, &[label], &instances)
            .expect("same-box text-only dirty must dest-reset");
    assert!((rect2[2] - rect[2]).abs() < 1.0);
}

#[test]
fn caret_blink_text_leaf_must_not_wipe_flex_content_box() {
    // Blink dirties the I-beam. That dirty is a swapchain overlay — dest-reset
    // never sees it. Last-present of the leaf is the glyph run (typing wipe),
    // not chrome and not the flex-grown content box.
    let leaf = [12.0, 20.0, 300.0, 24.0];
    let ibeam = [40.0, 22.0, 2.0, 18.0];
    let mut ph = sample_text("Search", leaf[0]);
    ph.y = leaf[1];
    ph.width = 80.0;
    ph.height = leaf[3];
    ph.style.input_caret = Some(crate::style::InputCaretDraw::default());
    let p = slot(5, leaf, (0, 1));
    let dirty = PaintDirty {
        key: 5,
        bounds: ibeam,
        kind: PaintDirtyKind::Node,
    };
    assert!(
        paint_dirty_is_caret_overlay(&dirty, &[p], &[]),
        "I-beam dirty on a text leaf is caret overlay"
    );
    let prev = last_present_visual_slots(&[p], &[], &[], &[ph], &[])
        .into_iter()
        .find(|(k, _)| *k == 5)
        .map(|(_, b)| b)
        .expect("caret leaf last-present is the glyph run");
    assert!(
        prev[2] > ibeam[2] * 2.0,
        "last-present must follow glyphs, not the I-beam\n prev={prev:?} ibeam={ibeam:?}"
    );
    assert!(
        prev[2] < leaf[2],
        "last-present must not be the flex-grown leaf\n prev={prev:?} leaf={leaf:?}"
    );
}

#[test]
fn caret_blink_must_not_recapture_placeholder_glyphs() {
    // I-beam is a swapchain overlay after retain blit. Blink must not dest-reset
    // or recapture Search (竖条 / 叠影). Last-present of the leaf is the run.
    let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
    card.color = [0.96, 0.96, 0.97, 1.0];
    let leaf = [12.0, 20.0, 300.0, 24.0];
    let ibeam = [40.0, 22.0, 2.0, 18.0];
    let mut caret = blank_instance(ibeam[0], ibeam[1], ibeam[2], ibeam[3]);
    caret.color = [0.1, 0.1, 0.1, 1.0];
    let mut ph = sample_text("Search documentation", leaf[0]);
    ph.y = leaf[1];
    ph.width = 180.0;
    ph.height = leaf[3];
    ph.style.input_caret = Some(crate::style::InputCaretDraw::default());
    let p_on = NodePaint {
        key: 5,
        bounds: leaf,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 1),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let p_off = NodePaint {
        chrome: (0, 0),
        ..p_on
    };
    let texts = [ph.clone()];
    let dirty = PaintDirty {
        key: 5,
        bounds: ibeam,
        kind: PaintDirtyKind::Node,
    };
    assert!(
        paint_dirty_is_caret_overlay(&dirty, &[p_on], &[card]),
        "blink I-beam dirty must be caret overlay (encode skips dest-reset + recapture)"
    );
    let prev_on = last_present_visual_slots(&[p_on], &[card], &[], &texts, &[caret.clone()])
        .into_iter()
        .find(|(k, _)| *k == 5)
        .map(|(_, b)| b)
        .expect("caret leaf last-present");
    assert!(
            !aabb_covers(prev_on, ibeam) || prev_on[2] > ibeam[2] * 4.0,
            "last-present must be the placeholder run, not the I-beam overlay\n prev={prev_on:?} ibeam={ibeam:?}"
        );
    assert!(
        prev_on[2] < leaf[2],
        "last-present must not be the flex leaf\n prev={prev_on:?} leaf={leaf:?}"
    );
    let prev_off = last_present_visual_slots(&[p_off], &[card], &[], &texts, &[])
        .into_iter()
        .find(|(k, _)| *k == 5)
        .map(|(_, b)| b);
    if let (Some(a), Some(b)) = (Some(prev_on), prev_off) {
        assert!(
            (a[2] - b[2]).abs() < 1.0 && (a[0] - b[0]).abs() < 1.0,
            "ON/OFF last-present of the leaf must match (chrome is overlay)\n on={a:?} off={b:?}"
        );
    }
    let hole = node_present_clipped_capture_text_wipes(&[dirty], &texts, &[p_on], &[]);
    assert!(
        hole.is_empty() || paint_dirty_is_caret_overlay(&dirty, &[p_on], &[card]),
        "overlay blink must not recapture placeholder into retain"
    );
    let _ = caret;
}

#[test]
fn dest_reset_glyph_clip_covers_scissor_ceil_pixel_through_s() {
    // I-beam at a fractional origin: CLEAR floor/ceils to 3 device px,
    // glyphon floor/floors the float wipe to 2. The extra pixel through S
    // stayed white. Capture clip must be that scissor, still hairline.
    let wipe = [40.3, 22.0, 2.0, 18.0];
    let dirty = PaintDirty {
        key: 5,
        bounds: wipe,
        kind: PaintDirtyKind::Node,
    };
    let clip = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.0);
    let scissor = dest_reset_wipe_scissor_logical(wipe, 1.0);
    assert_eq!(scissor, [40.0, 22.0, 3.0, 18.0]);
    assert!(
        clip[0] <= scissor[0]
            && clip[1] <= scissor[1]
            && clip[0] + clip[2] >= scissor[0] + scissor[2]
            && clip[1] + clip[3] >= scissor[1] + scissor[3],
        "glyphon clip must cover the CLEAR scissor\n clip={clip:?} scissor={scissor:?}"
    );
    assert!(
        (clip[2] - scissor[2]).abs() <= NODE_PRESENT_GLYPH_CLIP_PAD * 2.0 + 0.01,
        "coverage kernel is 1px, not vertex-AA through S\n clip={clip:?} scissor={scissor:?}"
    );
    assert!(
        clip[2] < 8.0,
        "ceil pixel is not an AA pad through the rest of S\n clip={clip:?}"
    );

    let clip_hi = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.5);
    let scissor_hi = dest_reset_wipe_scissor_logical(wipe, 1.5);
    assert!(
        clip_hi[0] <= scissor_hi[0] && clip_hi[0] + clip_hi[2] >= scissor_hi[0] + scissor_hi[2],
        "1.5x glyphon clip must cover the CLEAR scissor\n clip={clip_hi:?} scissor={scissor_hi:?}"
    );
    assert!(
        scissor_hi[2] > wipe[2],
        "1.5x must include the ceil device pixel\n scissor={scissor_hi:?} wipe={wipe:?}"
    );
    assert!(
        scissor_hi[2] < wipe[2] + 2.0,
        "must not grow by vertex-AA pad\n scissor={scissor_hi:?}"
    );

    let on_pixel = [40.0, 22.0, 2.0, 18.0];
    assert_eq!(
        dest_reset_wipe_scissor_logical(on_pixel, 1.0),
        on_pixel,
        "integer wipe must stay identity"
    );
}

#[test]
fn dest_reset_placeholder_must_not_src_over_left_of_s() {
    // Focused empty Search uses text-overflow:ellipsis, so clip_bounds is the
    // leaf. I-beam at byte 0 sits on that left edge. Recapture TextBounds must
    // keep a 1px kernel so glyphon's hard clip is not on S's stem (白线), and
    // the painted region (TextBounds ∩ CLEAR scissor) must stay the hole
    // (叠粗).
    let wipe = [40.3, 22.0, 2.0, 18.0];
    let dirty = PaintDirty {
        key: 5,
        bounds: wipe,
        kind: PaintDirtyKind::Node,
    };
    let mut search = sample_text("Search…", 40.3);
    search.y = 22.0;
    search.width = 80.0;
    search.height = 18.0;
    search.clip_bounds = Some([40.3, 22.0, 80.0, 18.0]);
    search.paint_origin = [40.3, 22.0];
    let p = slot(5, [40.3, 22.0, 80.0, 18.0], (0, 1));
    let owner = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.0);
    let hole =
        node_present_clipped_capture_text_wipes(&[dirty], &[search.clone()], &[p], &[(5, owner)]);
    let cap = hole
        .iter()
        .find(|n| n.text.contains("Search"))
        .expect("must recapture Search")
        .clip_bounds
        .expect("wipe clip");
    let scissor = dest_reset_wipe_scissor_logical(wipe, 1.0);
    let int_clip =
        crate::renderer::prepare_text::integerize_clip_rect(cap).expect("positive capture clip");
    assert!(
            int_clip[0] <= scissor[0] - 0.99,
            "ellipsis leaf clip must not eat the coverage kernel left of Search「S」\n cap={cap:?} int={int_clip:?} scissor={scissor:?} owner={owner:?}"
        );
    assert!(
        cap[2] < 8.0,
        "clip must stay the I-beam hole + kernel, not a pad through S\n cap={cap:?}"
    );
    assert!(
            int_clip[0] + int_clip[2] >= scissor[0] + scissor[2] - 0.01,
            "integerized clip must cover the ceil pixel through S\n int={int_clip:?} scissor={scissor:?}"
        );
    let paint = crate::app::damage::intersect_rects(int_clip, scissor).expect("paint");
    assert!(
            paint[0] >= scissor[0] - 0.01 && paint[0] + paint[2] <= scissor[0] + scissor[2] + 0.01,
            "GPU scissor must keep the kernel from painting left of CLEAR (叠粗)\n paint={paint:?} scissor={scissor:?}"
        );
    for scale in [1.25_f32, 1.5, 1.75] {
        let scissor_hi = dest_reset_wipe_scissor_logical(wipe, scale);
        let owner_hi = node_present_glyph_clip_logical(&dirty, Some(wipe), scale);
        let hole_hi = node_present_clipped_capture_text_wipes(
            &[dirty],
            &[search.clone()],
            &[p],
            &[(5, owner_hi)],
        );
        let cap_hi = hole_hi
            .iter()
            .find(|n| n.text.contains("Search"))
            .expect("must recapture Search at hi-dpi")
            .clip_bounds
            .expect("wipe clip");
        let scaled = [
            cap_hi[0] * scale,
            cap_hi[1] * scale,
            cap_hi[2] * scale,
            cap_hi[3] * scale,
        ];
        let int_hi = crate::renderer::prepare_text::integerize_clip_rect(scaled)
            .expect("positive hi-dpi clip");
        let scissor_px = [
            scissor_hi[0] * scale,
            scissor_hi[1] * scale,
            scissor_hi[2] * scale,
            scissor_hi[3] * scale,
        ];
        assert!(
                int_hi[0] + int_hi[2] >= scissor_px[0] + scissor_px[2] - 0.01
                    && int_hi[0] <= scissor_px[0] + 0.01,
                "{scale}x integerize must cover CLEAR through S\n int={int_hi:?} scissor={scissor_px:?}"
            );
    }
}

#[test]
fn transparent_item_must_not_skip_title_dest_reset() {
    // Gallery Item outline→default: both the row and the bind_text title are
    // dirty. Geometry nesting used to skip the title wipe; the Item has no
    // fill, so "outline" glyphs stayed in retain under "default".
    let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
    card.color = [0.96, 0.96, 0.97, 1.0];
    card.radii = [8.0; 4];
    let item_box = [8.0, 8.0, 384.0, 56.0];
    let title_box = [48.0, 16.0, 200.0, 20.0];
    let item_p = slot(2, item_box, (0, 0));
    let title_p = slot(3, title_box, (0, 1));
    let item_dirty = PaintDirty {
        key: 2,
        bounds: item_box,
        kind: PaintDirtyKind::Node,
    };
    let title_dirty = PaintDirty {
        key: 3,
        bounds: title_box,
        kind: PaintDirtyKind::Node,
    };
    let dirties = [item_dirty, title_dirty];
    let paints = [item_p, title_p];
    let instances = [card];
    assert!(
        crate::app::damage::paint_dirty_nested_in_larger_node(&title_dirty, &dirties),
        "precondition: title center sits in the Item dirty"
    );
    assert!(
        crate::app::damage::paint_dirty_skips_node_dest_reset(&title_dirty, &dirties, false),
        "precondition: geometry-only skip would fire"
    );
    assert!(
        !paint_dirty_skips_node_dest_reset_covered(
            &title_dirty,
            &dirties,
            false,
            &paints,
            &instances,
            1.0,
        ),
        "transparent Item must not skip the title dest-reset"
    );
    assert!(
        node_present_dest_reset_full(
            3,
            title_box,
            &paints,
            &instances,
            &[],
            &[],
            1.0,
            Some(title_box),
        )
        .is_some(),
        "title dest-reset must wipe leftover outline glyphs using the card behind"
    );
}

#[test]
fn transparent_item_with_icon_must_not_skip_title_dest_reset() {
    // Gallery Item has ItemMedia (gear). Collect range includes that opaque
    // descendant; skip must look at Item's own chrome only.
    let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
    card.color = [0.96, 0.96, 0.97, 1.0];
    let mut item_fill = blank_instance(8.0, 8.0, 384.0, 56.0);
    item_fill.color = [0.0, 0.0, 0.0, 0.0];
    let mut icon = blank_instance(16.0, 20.0, 24.0, 24.0);
    icon.color = [0.4, 0.4, 0.45, 1.0];
    let item_box = [8.0, 8.0, 384.0, 56.0];
    let title_box = [48.0, 16.0, 200.0, 20.0];
    let mut item_p = slot(2, item_box, (0, 0));
    item_p.instance = (1, 3);
    let title_p = slot(3, title_box, (0, 1));
    let item_dirty = PaintDirty {
        key: 2,
        bounds: item_box,
        kind: PaintDirtyKind::Node,
    };
    let title_dirty = PaintDirty {
        key: 3,
        bounds: title_box,
        kind: PaintDirtyKind::Node,
    };
    let dirties = [item_dirty, title_dirty];
    assert!(
        node_dirty_has_opaque_fill(2, item_box, &[item_p, title_p], &[card, item_fill, icon]),
        "precondition: descendant icon would look like Item fill"
    );
    assert!(
        !paint_dirty_skips_node_dest_reset_covered(
            &title_dirty,
            &dirties,
            false,
            &[item_p, title_p],
            &[card, item_fill, icon],
            1.0,
        ),
        "opaque ItemMedia must not make default Item skip the title dest-reset"
    );
}

#[test]
fn opaque_shell_still_skips_nested_input_dest_reset() {
    // Kbd Input inside InputAdornment: the shell redraws opaque fill, so the
    // nested field still skips its own wipe (avoids punching gap-2 neighbors).
    let mut fill = blank_instance(10.0, 10.0, 320.0, 40.0);
    fill.color = [1.0, 1.0, 1.0, 1.0];
    let mut shell_p = slot(1, [10.0, 10.0, 320.0, 40.0], (0, 0));
    shell_p.instance = (0, 1);
    let input_p = slot(2, [12.0, 12.0, 260.0, 36.0], (0, 0));
    let shell = PaintDirty {
        key: 1,
        bounds: [10.0, 10.0, 320.0, 40.0],
        kind: PaintDirtyKind::Node,
    };
    let input = PaintDirty {
        key: 2,
        bounds: [12.0, 12.0, 260.0, 36.0],
        kind: PaintDirtyKind::Node,
    };
    let dirties = [shell, input];
    assert!(
        paint_dirty_skips_node_dest_reset_covered(
            &input,
            &dirties,
            false,
            &[shell_p, input_p],
            &[fill],
            1.0,
        ),
        "opaque InputAdornment must still nested-skip the field dest-reset"
    );
}

#[test]
fn clipped_capture_keeps_neighbor_caption_out_of_input_dirty() {
    let mut nav = sample_text("NavLayout", 170.0);
    nav.width = 40.0;
    let mut typed = sample_text("asdasd", 240.0);
    typed.width = 80.0;
    let page = [nav.clone(), typed.clone()];
    let dirty_nav = PaintDirty {
        key: 1,
        bounds: [8.0, 0.0, 200.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let dirty_input = PaintDirty {
        key: 2,
        bounds: [208.0, 0.0, 400.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let paints = [
        slot(1, dirty_nav.bounds, (0, 1)),
        slot(2, dirty_input.bounds, (1, 2)),
    ];
    assert!(text_owned_by_node_dirty(&nav, dirty_nav.bounds));
    assert!(!text_owned_by_node_dirty(&nav, dirty_input.bounds));

    let input_owned = node_present_glyphs_for_dirty(&dirty_input, &page, &paints);
    assert!(input_owned.contains(&1));
    assert!(
        !input_owned.contains(&0),
        "Input's own glyph set must not include NavLayout"
    );

    let clipped = node_present_clipped_capture_text(&[dirty_nav, dirty_input], &page, &paints);
    let nav_clip = clipped
        .iter()
        .find(|n| n.text == "NavLayout")
        .and_then(|n| n.clip_bounds)
        .expect("NavLayout stays in the capture with an owner clip");
    assert!(
        nav_clip[0] + nav_clip[2] <= dirty_input.bounds[0] + NODE_PRESENT_GLYPH_CLIP_PAD + 1e-3,
        "NavLayout clip {nav_clip:?} must not extend into Input dirty {:?}",
        dirty_input.bounds
    );
    let typed_clip = clipped
        .iter()
        .find(|n| n.text == "asdasd")
        .and_then(|n| n.clip_bounds)
        .expect("typed text stays in the capture with an owner clip");
    assert!(
        typed_clip[0] + 1e-3 >= dirty_input.bounds[0] - NODE_PRESENT_GLYPH_CLIP_PAD,
        "typed clip {typed_clip:?} must stay in the Input dirty {:?}",
        dirty_input.bounds
    );
}

#[test]
fn modal_scrim_occludes_clicked_trigger_glyph_capture() {
    // AlertDialog: dest-reset skip still left node-present glyphon to flush
    // "one" onto the retained scrim. Occupancy must drop that dirty first.
    let trigger = [24.0, 80.0, 96.0, 32.0];
    let scrim = [0.0, 0.0, 1280.0, 800.0];
    let dirty = PaintDirty {
        key: 7,
        bounds: trigger,
        kind: PaintDirtyKind::Node,
    };
    let mut caption = sample_text("one", 28.0);
    caption.y = 86.0;
    caption.paint_origin = [28.0, 86.0];
    let paints = [slot(7, trigger, (0, 1))];
    let page = [caption];
    let captured = node_present_clipped_capture_text(&[dirty], &page, &paints);
    assert!(
        captured.iter().any(|n| n.text == "one"),
        "without occupancy the clicked caption is in the capture buffer"
    );
    let dirties: Vec<PaintDirty> = [dirty]
        .into_iter()
        .filter(|d| {
            !crate::app::damage::page_node_present_occluded_by_overlay(d.bounds, &[(1, scrim)])
        })
        .collect();
    let hidden = node_present_clipped_capture_text(&dirties, &page, &paints);
    assert!(
        hidden.iter().all(|n| n.text != "one"),
        "fullscreen scrim occupancy must drop the trigger from glyphon capture"
    );
}

#[test]
fn node_dirty_owns_only_its_collect_slot_not_center_in_box() {
    // Gallery: padded Input dirty can cover a sidebar caption's center
    // (「布局」 sharing a horizontal band with Kbd Search) without that
    // caption belonging to the Input collect slot.
    let mut caption = sample_text("布局", 190.0);
    caption.width = 40.0; // center x=210, inside Input dirty x=208
    let mut typed = sample_text("asdasd", 240.0);
    typed.width = 80.0;
    let page = [caption, typed];
    let dirty_input = PaintDirty {
        key: 2,
        bounds: [208.0, 0.0, 400.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let paints = [
        slot(1, [8.0, 0.0, 200.0, 32.0], (0, 1)),
        slot(2, [208.0, 0.0, 400.0, 32.0], (1, 2)),
    ];
    assert!(
        text_owned_by_node_dirty(&page[0], dirty_input.bounds),
        "precondition: caption center sits inside the padded Input dirty"
    );
    let owned = node_present_glyphs_for_dirty(&dirty_input, &page, &paints);
    assert!(owned.contains(&1), "Input text must stay owned");
    assert!(
        !owned.contains(&0),
        "sidebar caption must not ride along because its center sits in the Input pad"
    );
    let capture = node_present_capture_text_indices(&[dirty_input], &page, &paints);
    assert!(
        !capture.contains(&0),
        "capture must not include 布局 when only Input is dirty"
    );
}

fn aabb_covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] + 0.5
        && outer[1] <= inner[1] + 0.5
        && outer[0] + outer[2] >= inner[0] + inner[2] - 0.5
        && outer[1] + outer[3] >= inner[1] + inner[3] - 0.5
}

fn input_like(x: f32, y: f32, w: f32, h: f32) -> InstanceInput {
    let mut i = blank_instance(x, y, w, h);
    i.color = [0.0, 0.0, 0.0, 0.0];
    i.border_width = 1.0;
    i.border_color = [0.72, 0.72, 0.75, 1.0];
    i.radii = [6.0; 4];
    i
}

#[test]
fn dest_reset_hard_ring_spread_is_dest_ink() {
    // ShadCN Hard ring: blur 0, spread 3, ~50% ring color. Transparent fill.
    let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let box_ = [8.0, 8.0, 220.0, 36.0];
    let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
    focused.border_color = [0.45, 0.35, 0.95, 1.0];
    focused.shadow_blur = 0.0;
    focused.shadow_spread = 3.0;
    focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let p = NodePaint {
        key: 5,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let ring = instance_paint_bounds(&focused);
    assert!(
        ring[0] <= box_[0] - 2.5 && ring[0] + ring[2] >= box_[0] + box_[2] + 2.5,
        "focused paint bounds must include the 3px outset, got {ring:?}"
    );
    let (wipe, _) =
        node_present_dest_reset(5, box_, &[p], &[card, focused]).expect("card behind input");
    assert!(
        aabb_covers(wipe, ring),
        "focused dest-reset must erase the Hard ring\n wipe={wipe:?} ring={ring:?}"
    );
}

#[test]
fn dest_reset_spread_only_ring_without_border_is_still_ink() {
    // Ghost / plain chrome: no fill, no border, Hard ring is the only outset ink.
    let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let box_ = [8.0, 8.0, 220.0, 36.0];
    let mut ring_only = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    ring_only.color = [0.0, 0.0, 0.0, 0.0];
    ring_only.shadow_blur = 0.0;
    ring_only.shadow_spread = 3.0;
    ring_only.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let p = NodePaint {
        key: 6,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let ring = instance_paint_bounds(&ring_only);
    let (wipe, _) =
        node_present_dest_reset(6, box_, &[p], &[card, ring_only]).expect("card behind ring");
    assert!(
        aabb_covers(wipe, ring),
        "spread-only shadow must be dest-ink so unfocus can erase it\n wipe={wipe:?} ring={ring:?}"
    );
}

#[test]
fn dest_reset_unfocus_prev_must_cover_hard_ring() {
    let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let box_ = [8.0, 8.0, 220.0, 36.0];
    let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
    focused.border_color = [0.45, 0.35, 0.95, 1.0];
    focused.shadow_blur = 0.0;
    focused.shadow_spread = 3.0;
    focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let mut unfocused = input_like(box_[0], box_[1], box_[2], box_[3]);
    unfocused.shadow_blur = 1.0;
    unfocused.shadow_spread = 0.0;
    unfocused.shadow_color = [0.0, 0.0, 0.0, 0.08];
    let p = NodePaint {
        key: 7,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let ring = instance_paint_bounds(&focused);
    let prev = node_paint_visual_bounds(&p, &[card, focused], &[], &[], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(7, box_, &[p], &[card, unfocused], &[], 1.0, Some(prev))
            .expect("card behind unfocused input");
    assert!(
            aabb_covers(wipe, ring),
            "unfocus dest-reset must union last-present ring AABB\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
}

#[test]
fn dest_reset_with_last_present_must_not_pad_beyond_real_ink() {
    // Caret blink dest-resets a focused Input with last-present. RESET_PAD on
    // top of ring ink (spread 3 + vertex 2) punched `form_field` gap(4) labels.
    let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let box_ = [8.0, 8.0, 220.0, 36.0];
    let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
    focused.border_color = [0.45, 0.35, 0.95, 1.0];
    focused.shadow_blur = 0.0;
    focused.shadow_spread = 3.0;
    focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let p = NodePaint {
        key: 9,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let ring = instance_paint_bounds(&focused);
    let prev = node_paint_visual_bounds(&p, &[card, focused], &[], &[], &[]);
    let expected = super::union_xywh(ring, prev);
    let (wipe, _) =
        node_present_dest_reset_scaled(9, box_, &[p], &[card, focused], &[], 1.0, Some(prev))
            .expect("card behind focused input");
    assert!(
        aabb_covers(wipe, ring),
        "wipe must still cover real ring ink\n wipe={wipe:?} ring={ring:?}"
    );
    assert!(
            (wipe[0] - expected[0]).abs() <= 0.5
                && (wipe[1] - expected[1]).abs() <= 0.5
                && (wipe[2] - expected[2]).abs() <= 0.5
                && (wipe[3] - expected[3]).abs() <= 0.5,
            "steady-state dest-reset must be the ink ∪ last-present box, not +RESET_PAD\n wipe={wipe:?} expected={expected:?}"
        );
}

#[test]
fn dest_reset_focus_ring_must_not_wipe_sibling_label() {
    // WP-3.9: Hard ring spread 3 sits in form_field gap(4). Vertex AA / dest-reset
    // pad must not punch the label slot above the Input.
    let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let label = [8.0, 0.0, 40.0, 12.0];
    let box_ = [8.0, 16.0, 220.0, 36.0];
    let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
    focused.border_color = [0.45, 0.35, 0.95, 1.0];
    focused.shadow_blur = 0.0;
    focused.shadow_spread = 3.0;
    focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let label_p = NodePaint {
        key: 1,
        bounds: label,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let input_p = NodePaint {
        key: 5,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let paints = [label_p, input_p];
    let visible_ring = super::padded_xywh(box_, crate::style::FOCUS_RING_HARD_SPREAD);
    let prev = node_paint_visual_bounds(&input_p, &[card, focused], &[], &[], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(5, box_, &paints, &[card, focused], &[], 1.0, Some(prev))
            .expect("card behind focused input");
    assert!(
        aabb_covers(wipe, visible_ring),
        "wipe must still cover the 3px Hard ring in the gap\n wipe={wipe:?} ring={visible_ring:?}"
    );
    assert!(
        !super::point_in_rect(wipe, [label[0] + 8.0, label[1] + label[3] - 0.5]),
        "focus dest-reset must not punch the label slot\n wipe={wipe:?} label={label:?}"
    );
    let scissor = super::node_present_scissor_logical(
        PaintDirtyKind::Node,
        5,
        [box_[0] - 3.0, box_[1] - 3.0, box_[2] + 6.0, box_[3] + 6.0],
        &[(5, prev)],
        &paints,
        8.0,
    );
    assert!(
            !super::point_in_rect(scissor, [label[0] + 8.0, label[1] + label[3] - 0.5]),
            "node scissor must not cover the label after sibling clip\n scissor={scissor:?} label={label:?}"
        );
}

#[test]
fn dest_reset_unfocus_high_dpi_prev_must_cover_scaled_hard_ring() {
    // RESET_PAD is 3 physical px. Hard ring is 3 logical px = 6 physical at 200%.
    // Unfocused current ink (blur 1 logical) + pad cannot cover it; prev must.
    let s = 2.0_f32;
    let mut card = blank_instance(0.0, 0.0, 240.0 * s, 80.0 * s);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let box_ = [8.0, 8.0, 220.0, 36.0];
    let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
    focused.border_color = [0.45, 0.35, 0.95, 1.0];
    focused.shadow_blur = 0.0;
    focused.shadow_spread = 3.0;
    focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
    let mut unfocused = input_like(box_[0] * s, box_[1] * s, box_[2] * s, box_[3] * s);
    unfocused.shadow_blur = 1.0 * s;
    unfocused.shadow_spread = 0.0;
    unfocused.shadow_color = [0.0, 0.0, 0.0, 0.08];
    unfocused.border_width = 1.0 * s;
    let p = NodePaint {
        key: 8,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let ring_logical = instance_paint_bounds(&focused);
    let ring = scale_xywh(ring_logical, s);
    let prev = node_paint_visual_bounds(
        &p,
        &[blank_instance(0.0, 0.0, 240.0, 80.0), focused],
        &[],
        &[],
        &[],
    );
    let (wipe, _) =
        node_present_dest_reset_scaled(8, box_, &[p], &[card, unfocused], &[], s, Some(prev))
            .expect("card behind 2x unfocused input");
    assert!(
            aabb_covers(wipe, ring),
            "2x unfocus dest-reset must still erase the 6px physical ring\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
    let (wipe_no_prev, _) =
        node_present_dest_reset_scaled(8, box_, &[p], &[card, unfocused], &[], s, None)
            .expect("card behind 2x unfocused input without prev");
    assert!(
            aabb_covers(wipe_no_prev, ring),
            "2x unfocus without last-present must still cover the Hard ring\n wipe={wipe_no_prev:?} ring={ring:?}"
        );
    assert!(
        node_present_needs_glyphs_scaled(
            8,
            box_,
            &[p],
            &[card, unfocused],
            &[],
            &[],
            s,
            Some(prev),
        ),
        "wipe of the field must recapture glyphs (placeholder / value)"
    );
}

#[test]
fn dest_reset_input_parent_slot_must_capture_child_text() {
    // Input container dirty; glyphs live on the text child inside the parent range.
    let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
    card.color = [0.94, 0.94, 0.95, 1.0];
    let input_box = [8.0, 8.0, 380.0, 36.0];
    let mut field = input_like(input_box[0], input_box[1], input_box[2], input_box[3]);
    field.color = [1.0, 1.0, 1.0, 1.0];
    let parent = NodePaint {
        key: 10,
        bounds: input_box,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let child = NodePaint {
        key: 11,
        bounds: [16.0, 14.0, 80.0, 20.0],
        overlay: false,
        instance: (2, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let mut typed = sample_text("asdasd", 16.0);
    typed.y = 14.0;
    typed.width = 80.0;
    typed.height = 20.0;
    let dirty = PaintDirty {
        key: 10,
        bounds: [
            input_box[0] - 8.0,
            input_box[1] - 8.0,
            input_box[2] + 16.0,
            input_box[3] + 16.0,
        ],
        kind: PaintDirtyKind::Node,
    };
    let paints = [parent, child];
    let owned = node_present_glyphs_for_dirty(&dirty, &[typed.clone()], &paints);
    assert!(
        owned.contains(&0),
        "Input container dirty must own the child text run"
    );
    assert!(
        node_present_needs_glyphs(10, dirty.bounds, &paints, &[card, field]),
        "dest-reset of the field must recapture its glyphs"
    );
    let (wipe, color) =
        node_present_dest_reset(10, dirty.bounds, &paints, &[card, field]).expect("wipe");
    assert_eq!(color, [0.94, 0.94, 0.95, 1.0]);
    assert!(
        aabb_covers(wipe, [16.0, 14.0, 80.0, 20.0]),
        "wipe must cover the text box so dest-reset cannot leave a gray glyph hole\n wipe={wipe:?}"
    );
}

#[test]
fn dest_reset_outline_remove_must_union_prev() {
    // CSS outline paints outside the layout box. Collect skip now busts on
    // outline color/width (Wave 12); dest-reset of *now* only leaves the ring.
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let outline_w = 8.0_f32;
    let mut outline = blank_instance(
        box_[0] - outline_w,
        box_[1] - outline_w,
        box_[2] + outline_w * 2.0,
        box_[3] + outline_w * 2.0,
    );
    outline.color = [0.0, 0.0, 0.0, 0.0];
    outline.border_width = outline_w;
    outline.border_color = [0.0, 0.0, 1.0, 1.0];
    let p_on = NodePaint {
        key: 21,
        bounds: box_,
        overlay: false,
        instance: (1, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let p_off = NodePaint {
        instance: (1, 2),
        ..p_on
    };
    let ring = instance_paint_bounds(&outline);
    let prev = node_paint_visual_bounds(&p_on, &[card, fill, outline], &[], &[], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, Some(prev))
            .expect("card behind after outline off");
    assert!(
            aabb_covers(wipe, ring),
            "removing outline must dest-reset the previous ring\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
    let (wipe_no_prev, _) =
        node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, None)
            .expect("wipe without last-present");
    assert!(
            !aabb_covers(wipe_no_prev, ring),
            "3px reset pad on the fill cannot cover an 8px outline; prev visual is required\n wipe={wipe_no_prev:?} ring={ring:?}"
        );
}

#[test]
fn dest_reset_extra_shadow_remove_must_union_prev() {
    // Extra-shadow is its own instance (Wave 11). Dropping it must union last-present.
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let mut shadow = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    shadow.color = [0.0, 0.0, 0.0, 0.0];
    shadow.shadow_color = [0.0, 0.0, 0.0, 0.8];
    shadow.shadow_offset = [12.0, 12.0];
    shadow.shadow_blur = 4.0;
    let p_on = NodePaint {
        key: 22,
        bounds: box_,
        overlay: false,
        instance: (1, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let p_off = NodePaint {
        instance: (1, 2),
        ..p_on
    };
    let blob = instance_paint_bounds(&shadow);
    let prev = node_paint_visual_bounds(&p_on, &[card, fill, shadow], &[], &[], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(22, box_, &[p_off], &[card, fill], &[], 1.0, Some(prev))
            .expect("card behind after extra-shadow off");
    assert!(
            aabb_covers(wipe, blob),
            "removing extra-shadow must dest-reset the previous outset\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
    let (wipe_no_prev, _) =
        node_present_dest_reset_scaled(22, box_, &[p_off], &[card, fill], &[], 1.0, None)
            .expect("wipe without last-present");
    assert!(
            !aabb_covers(wipe_no_prev, blob),
            "fill + 3px pad cannot cover offset 12 / blur 4 extra-shadow; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
}

#[test]
fn ancestor_clip_caps_shadow_wipe_and_last_present() {
    let mut fill = blank_instance(120.0, 102.0, 80.0, 30.0);
    fill.color = [1.0; 4];
    fill.shadow_color = [0.0, 0.0, 0.0, 0.25];
    fill.shadow_blur = 8.0;
    let mut p = slot(7, [120.0, 102.0, 80.0, 30.0], (0, 0));
    p.instance = (0, 1);
    p.overflow_clip = Some([0.0, 100.0, 400.0, 300.0]);
    let prev = node_paint_visual_bounds(&p, &[fill], &[], &[], &[]);
    assert!(
        prev[1] >= 100.0,
        "shadow must not invent pixels above viewport"
    );
    let wipe = node_present_reset_rect(
        7,
        p.bounds,
        &[p],
        &[fill],
        &[],
        &[],
        NODE_PRESENT_RESET_PAD_PHYSICAL,
        1.0,
        Some(prev),
    )
    .unwrap();
    assert!(wipe[1] >= 100.0, "shadow expansion must not erase header");
    for clip in [[0.0; 4], [0.0, 250.0, 400.0, 100.0]] {
        p.overflow_clip = Some(clip);
        let hidden = node_paint_visual_bounds(&p, &[fill], &[], &[], &[]);
        assert_eq!(
            hidden, [0.0; 4],
            "culled ink must not become last-present damage"
        );
    }
}

#[test]
fn dest_reset_overflow_hidden_must_cover_own_outset_shadow() {
    // Gallery SidebarInset: overflow:hidden + Elevation::Md (blur 8, offset y 2).
    // Own chrome paints the halo outside overflow (Dialog/Drawer rule). Dest-reset
    // / last-present must still wipe that halo or PartialLoad src-overs it darker
    // every CSS tick.
    let box_ = [40.0, 40.0, 200.0, 120.0];
    let mut page = blank_instance(0.0, 0.0, 400.0, 300.0);
    page.color = [0.12, 0.12, 0.14, 1.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [0.98, 0.98, 0.99, 1.0];
    fill.radii = [12.0; 4];
    fill.shadow_color = [0.0, 0.0, 0.0, 0.25];
    fill.shadow_offset = [0.0, 2.0];
    fill.shadow_blur = 8.0;
    let p = NodePaint {
        key: 7,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        // collect records the parent clip, not this node's own overflow.
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let blob = instance_paint_bounds(&fill);
    assert!(
        blob[0] < box_[0] - 8.0 && blob[1] < box_[1] - 8.0,
        "precondition: outset halo extends past the overflow box, got blob={blob:?} box={box_:?}"
    );
    let prev = node_paint_visual_bounds(&p, &[page, fill], &[], &[], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(7, box_, &[p], &[page, fill], &[], 1.0, Some(prev))
            .expect("page fill behind inset");
    assert!(
            aabb_covers(wipe, blob),
            "overflow:hidden dest-reset must wipe own outset shadow\n wipe={wipe:?} blob={blob:?} prev={prev:?} overflow={:?}",
            p.overflow_clip
        );
    let scissor = node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        7,
        box_,
        &[(7, prev)],
        &[p],
        8.0,
    );
    let painted = intersect_rects(blob, scissor).expect("halo sits in the node scissor");
    assert!(
            aabb_covers(wipe, painted),
            "GPU scissor paints halo that dest-reset missed — PartialLoad stacks it\n wipe={wipe:?} painted={painted:?} scissor={scissor:?} blob={blob:?}"
        );
}

#[test]
fn dest_reset_filter_wrap_shrink_must_union_prev() {
    // CSS filter capture is a textured wrap larger than the layout box.
    // Shrinking blur must dest-reset the previous outset (Wave 11 collect + Wave 13 present).
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let big = blank_textured(8.0, 8.0, 64.0, 44.0, MAT2D_IDENTITY);
    let small = blank_textured(16.0, 16.0, 48.0, 28.0, MAT2D_IDENTITY);
    let p_big = NodePaint {
        key: 23,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let p_small = NodePaint {
        textured: (0, 1),
        ..p_big
    };
    let prev = node_paint_visual_bounds(&p_big, &[card, fill], &[big.clone()], &[], &[]);
    let blob = textured_paint_bounds(&big);
    let (wipe, _) = node_present_dest_reset_scaled(
        23,
        box_,
        &[p_small],
        &[card, fill],
        &[small.clone()],
        1.0,
        Some(prev),
    )
    .expect("card behind after filter shrink");
    assert!(
            aabb_covers(wipe, blob),
            "shrinking filter wrap must dest-reset the previous capture AABB\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
    let (wipe_no_prev, _) =
        node_present_dest_reset_scaled(23, box_, &[p_small], &[card, fill], &[small], 1.0, None)
            .expect("wipe without last-present");
    assert!(
            !aabb_covers(wipe_no_prev, blob),
            "current small wrap + 3px pad cannot cover the previous filter outset; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
}

#[test]
fn last_present_must_not_union_foreign_descendant_scale() {
    // Reaction bar collect range includes the chip Bake. Hover:scale grows that
    // quad; last-present of the pill must stay on own chrome or expand_paint
    // dest-resets the capsule as a sharp rect over outline / sibling chips.
    let bar = [10.0, 10.0, 48.0, 28.0];
    let chip = [32.0, 16.0, 16.0, 16.0];
    let leaf = [32.0, 18.0, 16.0, 12.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 80.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    fill.color = [0.82, 0.82, 0.84, 1.0];
    fill.radii = [14.0; 4];
    let rest_bake = blank_textured(30.0, 14.0, 20.0, 20.0, MAT2D_IDENTITY);
    let hover_bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
    let bar_p = NodePaint {
        key: 1,
        bounds: bar,
        overlay: false,
        instance: (1, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let chip_p = NodePaint {
        key: 2,
        bounds: chip,
        overlay: false,
        instance: (2, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let leaf_p = NodePaint {
        key: 3,
        bounds: leaf,
        overlay: false,
        instance: (2, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let paints = [bar_p, chip_p, leaf_p];
    let instances = [page, fill];
    let (rest_slots, raster_owners) =
        last_present_visual_slots_with_raster_owners(&paints, &instances, &[rest_bake], &[], &[]);
    assert_eq!(raster_owners, HashSet::from([3]));
    let hover_slots = last_present_visual_slots(&paints, &instances, &[hover_bake], &[], &[]);
    let rest_bar = rest_slots
        .iter()
        .find(|(k, _)| *k == 1)
        .map(|(_, b)| *b)
        .expect("bar rest");
    let hover_bar = hover_slots
        .iter()
        .find(|(k, _)| *k == 1)
        .map(|(_, b)| *b)
        .expect("bar hover");
    assert!(
            !crate::app::damage::node_present_slot_moved(rest_bar, hover_bar),
            "parent last-present must not grow with child hover:scale Bake\n rest={rest_bar:?} hover={hover_bar:?}"
        );
    // Equal-span ink belongs to the leaf. Recording it on the wrapping
    // chip as well would dirty two slots for one transformed bake.
    let rest_chip = rest_slots
        .iter()
        .find(|(k, _)| *k == 3)
        .map(|(_, b)| *b)
        .expect("chip rest");
    let hover_chip = hover_slots
        .iter()
        .find(|(k, _)| *k == 3)
        .map(|(_, b)| *b)
        .expect("chip hover");
    assert!(
            crate::app::damage::node_present_slot_moved(rest_chip, hover_chip),
            "chip last-present must keep the leaf Bake so shrink can dest-reset it\n rest={rest_chip:?} hover={hover_chip:?}"
        );
    let mut dirties = vec![crate::app::damage::PaintDirty {
        key: 2,
        bounds: hover_chip,
        kind: crate::app::damage::PaintDirtyKind::Node,
    }];
    crate::app::damage::expand_paint_dirties_for_translated_slots(
        &mut dirties,
        &hover_slots,
        &rest_slots,
    );
    assert!(
        dirties
            .iter()
            .all(|d| d.kind != crate::app::damage::PaintDirtyKind::Node || d.key != 1),
        "hover:scale must not dest-reset the parent pill\n dirties={dirties:?}"
    );
    assert!(
        !crate::app::damage::unnamed_translated_page_slots(&hover_slots, &rest_slots, &dirties,),
        "parent visual growth used to FullClear the page as an unnamed translate"
    );
}

#[test]
fn last_present_must_not_union_overflow_visible_spin_sticking_out() {
    // Gallery Marker row (icon + "Thinking...") is taller-parent by width, not
    // equal-span. 45° spin `NodePaint.bounds` is the circumcircle and sticks
    // out of the row's 16px box — `aabb_contains(row, icon)` is false.
    let row = [0.0, 8.0, 104.0, 16.0];
    let icon = [-3.3, 4.7, 22.6, 22.6];
    let content = [24.0, 8.0, 80.0, 16.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 48.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut row_fill = blank_instance(row[0], row[1], row[2], row[3]);
    row_fill.color = [0.0, 0.0, 0.0, 0.0];
    let mut content_fill = blank_instance(content[0], content[1], content[2], content[3]);
    content_fill.color = [0.85, 0.85, 0.85, 1.0];
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let spin = blank_textured(0.0, 8.0, 16.0, 16.0, [c, s, -s, c, 0.0, 0.0]);
    let rest_spin = blank_textured(0.0, 8.0, 16.0, 16.0, MAT2D_IDENTITY);
    let row_p = NodePaint {
        key: 1,
        bounds: row,
        overlay: false,
        instance: (1, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let icon_p = NodePaint {
        key: 2,
        bounds: icon,
        overlay: false,
        instance: (3, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let content_p = NodePaint {
        key: 3,
        bounds: content,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let paints = [row_p, icon_p, content_p];
    let instances = [page, row_fill, content_fill];
    let rest_slots = last_present_visual_slots(&paints, &instances, &[rest_spin], &[], &[]);
    let spin_slots = last_present_visual_slots(&paints, &instances, &[spin], &[], &[]);
    let rest_row = rest_slots
        .iter()
        .find(|(k, _)| *k == 1)
        .map(|(_, b)| *b)
        .expect("row rest");
    let spin_row = spin_slots
        .iter()
        .find(|(k, _)| *k == 1)
        .map(|(_, b)| *b)
        .expect("row spin");
    assert!(
            !crate::app::damage::node_present_slot_moved(rest_row, spin_row),
            "Marker row last-present must not grow with child spin circumcircle\n rest={rest_row:?} spin={spin_row:?}"
        );
    let mut dirties = vec![crate::app::damage::PaintDirty {
        key: 2,
        bounds: icon,
        kind: crate::app::damage::PaintDirtyKind::Node,
    }];
    crate::app::damage::expand_paint_dirties_for_translated_slots(
        &mut dirties,
        &spin_slots,
        &rest_slots,
    );
    assert!(
        dirties
            .iter()
            .all(|d| d.kind != crate::app::damage::PaintDirtyKind::Node || d.key != 1),
        "idle spin must not dirty the Marker row"
    );
    assert!(
        !crate::app::damage::paint_dirty_nested_in_larger_node(&dirties[0], &dirties),
        "icon dest-reset must run; nested-skip under the row leaves 45° corners in retain"
    );
}

#[test]
fn dest_reset_spin_must_not_be_cut_by_overflow_visible_marker_row() {
    // Gallery idle loader-circle: dest-reset of the icon is the circumcircle.
    // `clip_rect_against_foreign_siblings` treated the Marker row (wider, same
    // 16px height) as a foreign sibling because `aabb_contains(row, circumcircle)`
    // is false. Shrink then kept only the leftover strips *outside* the row —
    // the spinner body src-overs forever and the gap seals.
    let row = [0.0, 8.0, 104.0, 16.0];
    let icon = [-3.3, 4.7, 22.6, 22.6];
    let content = [24.0, 8.0, 80.0, 16.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 48.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut row_fill = blank_instance(row[0], row[1], row[2], row[3]);
    row_fill.color = [0.0, 0.0, 0.0, 0.0];
    let mut content_fill = blank_instance(content[0], content[1], content[2], content[3]);
    content_fill.color = [0.85, 0.85, 0.85, 1.0];
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let xf = [c, s, -s, c, 0.0, 0.0];
    let spin = blank_textured(0.0, 8.0, 16.0, 16.0, xf);
    let row_p = NodePaint {
        key: 1,
        bounds: row,
        overlay: false,
        instance: (1, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let icon_p = NodePaint {
        key: 2,
        bounds: icon,
        overlay: false,
        instance: (3, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let content_p = NodePaint {
        key: 3,
        bounds: content,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let paints = [row_p, icon_p, content_p];
    let instances = [page, row_fill, content_fill];
    let prev = [0.0, 8.0, 16.0, 16.0];
    let (wipe, _) =
        node_present_dest_reset_scaled(2, icon, &paints, &instances, &[spin], 1.0, Some(prev))
            .expect("card behind spinner");
    let spun = crate::renderer::aabb_of_centered_transform([0.0, 8.0, 16.0, 16.0], &xf);
    assert!(
            wipe[0] <= spun[0] + 0.5
                && wipe[1] <= spun[1] + 0.5
                && wipe[0] + wipe[2] >= spun[0] + spun[2] - 0.5
                && wipe[1] + wipe[3] >= spun[1] + spun[3] - 0.5,
            "idle dest-reset must wipe the loader-circle body, not leftover strips outside the Marker row\n wipe={wipe:?} spun={spun:?} row={row:?}"
        );
    assert!(
        wipe[2] > 18.0 && wipe[3] > 18.0,
        "wipe must stay the circumcircle, not a 3px leftover band\n wipe={wipe:?}"
    );
}

#[test]
fn node_present_scissor_must_not_crop_overlay_reaction_hover_scale_top() {
    // Overlay-layout pill nips bubble content. Content is an earlier collect
    // slot; shrinking occupancy against it set scissor.top = content.bottom
    // and AABB-cropped the 😮 top during hover:scale (intact after settle).
    let content = [0.0, 0.0, 166.0, 40.0];
    let chip = [135.66, 37.66, 16.68, 16.68];
    let dest_ink = [137.58, 38.93, 13.18, 13.10];
    let dirty = [130.30, 33.76, 27.40, 24.58];
    let bar_clip = [110.0, 32.0, 48.0, 28.0];
    let content_p = NodePaint {
        key: 1,
        bounds: content,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let chip_p = NodePaint {
        key: 2,
        bounds: chip,
        overlay: false,
        instance: (0, 0),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(bar_clip),
        layered: false,
        rest: None,
    };
    let paints = [content_p, chip_p];
    let sc = node_present_scissor_logical(
        crate::app::damage::PaintDirtyKind::Node,
        2,
        dirty,
        &[],
        &paints,
        node_present_scissor_pad(crate::app::damage::PaintDirtyKind::Node),
    );
    assert!(
        sc[1] <= dest_ink[1] + 0.5,
        "occupancy must not crop overlay hover:scale dest at content.bottom; \
             scissor={sc:?} dest_ink={dest_ink:?} content={content:?}"
    );
}

#[test]
fn dest_reset_overlay_reaction_leave_must_wipe_hover_scale_top() {
    // Occupancy draws the enlarged dest over content. Leave dest-reset still
    // sibling-clipped to content.bottom, so the overlap strip stays
    // (live: 收起残影 at the top of 😮).
    let content = [0.0, 0.0, 166.0, 40.0];
    let chip = [135.66, 37.66, 16.68, 16.68];
    let prev = [130.30, 33.76, 27.40, 24.58];
    let dirty = [132.0, 36.0, 20.0, 20.0];
    let bar_clip = [110.0, 32.0, 48.0, 28.0];
    let content_p = NodePaint {
        key: 1,
        bounds: content,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let chip_p = NodePaint {
        key: 2,
        bounds: chip,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(bar_clip),
        layered: false,
        rest: None,
    };
    let paints = [content_p, chip_p];
    let wipe =
        super::node_present_reset_rect(2, dirty, &paints, &[], &[], &[], 0.0, 1.0, Some(prev))
            .expect("leave dest-reset");
    assert!(
        wipe[1] <= prev[1] + 0.5,
        "leave dest-reset must wipe the enlarged top over content (ghost); \
             wipe={wipe:?} prev={prev:?} content={content:?}"
    );
}

fn paint_slot(
    key: u64,
    bounds: [f32; 4],
    rest: Option<[f32; 4]>,
    instance: (usize, usize),
) -> NodePaint {
    NodePaint {
        key,
        bounds,
        overlay: false,
        instance,
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest,
    }
}

#[test]
fn moving_card_joins_overlapping_icon_region_repair() {
    let current = [373.0, 172.0, 94.0, 16.0];
    let previous = [357.0, 172.0, 94.0, 16.0];
    let icon = [374.0, 173.0, 14.0, 14.0];
    let mut card = paint_slot(1, current, Some(current), (1, 2));
    card.layered = true;
    card.text = (0, 2);
    let mut grip = paint_slot(2, icon, Some(icon), (2, 2));
    grip.layered = true;
    grip.text = (0, 1);
    let paints = [card, grip];
    let mut page = blank_instance(0.0, 0.0, 800.0, 400.0);
    page.color = [1.0; 4];
    let mut outline = blank_instance(current[0], current[1], current[2], current[3]);
    outline.border_width = 1.0;
    outline.border_color = [0.5, 0.5, 0.5, 1.0];
    outline.radii = [8.0; 4];
    let instances = [page, outline];
    for (region, previous) in [
        ([372.0, 171.0, 18.0, 18.0], previous),
        ([600.0, 300.0, 18.0, 18.0], current),
    ] {
        let mut dirties = vec![
            PaintDirty {
                key: 1,
                bounds: union_xywh(current, previous),
                kind: PaintDirtyKind::Node,
            },
            PaintDirty {
                key: 2,
                bounds: region,
                kind: PaintDirtyKind::Region,
            },
        ];
        prepare_node_present_dirties(
            &mut dirties,
            &paints,
            &[(1, previous)],
            &instances,
            &[],
            &[],
            1.0,
            None,
        );
        let repair = dirties.iter().find(|d| d.key == 1).unwrap();
        if intersect_rects(current, region).is_some() {
            assert_eq!(
                repair.kind,
                PaintDirtyKind::Region,
                "card must not clear the glyph Region that was just repaired"
            );
            assert!(aabb_covers(repair.bounds, padded_xywh(current, 2.0)));
            assert!(aabb_covers(repair.bounds, padded_xywh(previous, 2.0)));
        } else {
            assert_eq!(
                repair.kind,
                PaintDirtyKind::Node,
                "a distant Region must preserve the compact card's Node path"
            );
        }
    }
}

#[test]
fn translated_card_texture_region_replays_parent_chrome_and_background() {
    let card_bounds = [312.0, 140.0, 94.0, 16.0];
    let mut card = paint_slot(1, card_bounds, Some([306.0, 140.0, 94.0, 16.0]), (0, 1));
    card.layered = true;
    card.textured = (0, 1);
    card.text = (0, 1);
    card.paint_ops = (0, 2);
    let icon_bounds = [313.0, 141.0, 14.0, 14.0];
    let mut grip = paint_slot(2, icon_bounds, Some(icon_bounds), (1, 1));
    grip.layered = true;
    grip.textured = (0, 1);
    grip.paint_ops = (1, 2);
    let repair = [305.0, 139.0, 24.0, 18.0];
    assert!(
        !region_replays_own_chrome_only(2, repair, &[card, grip]),
        "texture repair must restore the translated card outline and old background"
    );
    card.rest = Some(card_bounds);
    assert!(
        !region_replays_own_chrome_only(2, repair, &[card, grip]),
        "returning to rest still repairs the old texture position"
    );
    assert!(
        !region_replays_own_chrome_only(2, icon_bounds, &[card, grip]),
        "stationary follow-up repairs must preserve the parent chrome too"
    );
    assert!(
        region_replays_own_chrome_only(2, icon_bounds, &[grip]),
        "an isolated texture retains its local repair path"
    );
}

#[test]
fn slowly_translated_composite_repairs_old_and_new_raster_fringe() {
    for scale in [1.0, 1.5, 2.0] {
        for step in [0.0, 0.25, 0.5, 1.0, 4.0] {
            let previous = [320.0, 144.0, 94.0, 16.0];
            let current = [previous[0] + step, previous[1] + step * 0.5, 94.0, 16.0];
            let icon = [current[0] + 1.0, current[1] + 1.0, 14.0, 14.0];
            let old_icon = [previous[0] + 1.0, previous[1] + 1.0, 14.0, 14.0];
            let mut card = paint_slot(1, current, Some(previous), (1, 2));
            card.layered = true;
            card.text = (0, 2);
            let mut grip = paint_slot(2, icon, Some(old_icon), (2, 2));
            grip.layered = true;
            grip.text = (0, 1);
            let paints = [card, grip];
            let mut page = blank_instance(0.0, 0.0, 800.0 * scale, 400.0 * scale);
            page.color = [1.0; 4];
            let physical = scale_xywh(current, scale);
            let mut outline = blank_instance(physical[0], physical[1], physical[2], physical[3]);
            outline.color = [0.0; 4];
            outline.border_width = scale;
            outline.border_color = [0.5, 0.5, 0.5, 1.0];
            outline.radii = [8.0 * scale; 4];
            let mut dirties = vec![
                PaintDirty {
                    key: 1,
                    bounds: union_xywh(previous, current),
                    kind: PaintDirtyKind::Node,
                },
                PaintDirty {
                    key: 2,
                    bounds: union_xywh(old_icon, icon),
                    kind: PaintDirtyKind::Node,
                },
            ];
            prepare_node_present_dirties(
                &mut dirties,
                &paints,
                &[(1, previous), (2, old_icon)],
                &[page, outline],
                &[],
                &[],
                scale,
                None,
            );
            let repair = dirties.iter().find(|d| d.key == 1).unwrap();
            if step == 0.0 {
                assert_eq!(repair.kind, PaintDirtyKind::Node);
            } else {
                assert!(dirties.iter().all(|d| d.kind == PaintDirtyKind::Region));
                assert!(aabb_covers(
                    repair.bounds,
                    padded_xywh(previous, 2.0 / scale)
                ));
                assert!(aabb_covers(
                    repair.bounds,
                    padded_xywh(current, 2.0 / scale)
                ));
                // A second paint without another pointer move must keep
                // the translated composite ordered, including fractional AA.
                let mut follow_up = vec![
                    PaintDirty {
                        key: 1,
                        bounds: current,
                        kind: PaintDirtyKind::Node,
                    },
                    PaintDirty {
                        key: 2,
                        bounds: icon,
                        kind: PaintDirtyKind::Node,
                    },
                ];
                prepare_node_present_dirties(
                    &mut follow_up,
                    &paints,
                    &[(1, current), (2, icon)],
                    &[page, outline],
                    &[],
                    &[],
                    scale,
                    None,
                );
                assert!(follow_up.iter().all(|d| d.kind == PaintDirtyKind::Region));
            }
        }
    }
}

#[test]
fn culled_paint_owner_restores_previous_ink_as_local_region() {
    let previous = [22.5, 174.5, 24.0, 35.5];
    let current = [24.5, 221.3, 20.0, 11.2];
    let viewport = [0.0, 0.0, 320.0, 210.0];
    let mut owner = slot(7, current, (0, 0));
    owner.overflow_clip = Some(viewport);
    let paints = [owner];
    let background = [blank_instance(0.0, 0.0, 480.0, 315.0)];
    let mut dirties = vec![PaintDirty {
        key: 7,
        bounds: current,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(
        &mut dirties,
        &paints,
        &[(7, previous)],
        &background,
        &[],
        &[],
        1.5,
        None,
    );
    assert_eq!(dirties.len(), 1);
    assert_eq!(dirties[0].kind, PaintDirtyKind::Region);
    assert_eq!(dirties[0].bounds, previous);
    assert_eq!(
        node_present_scissor_logical(
            dirties[0].kind,
            7,
            dirties[0].bounds,
            &[(7, previous)],
            &paints,
            8.0,
        ),
        previous,
    );
}

#[test]
fn layered_checkbox_icon_is_not_underlay_for_following_switch() {
    let checkbox = [313.0, 139.66669, 16.0, 16.0];
    let switch = [305.0, 159.66669, 60.0, 40.0];
    let mut icon = sample_text("\u{e06c}", 315.0);
    icon.y = 141.66669;
    icon.width = 12.0;
    icon.height = 12.0;
    icon.style.size = Some(12.0);
    icon.style.font_family = Some("lucide".into());
    icon.clip_bounds = Some([264.0, 60.0, 728.0, 572.0]);
    assert!(!glyph_hits_layered_hole(&icon, switch));
    let mut check_paint = slot(7, checkbox, (0, 1));
    check_paint.layered = true;
    check_paint.instance = (0, 1);
    let mut switch_paint = slot(8, switch, (1, 1));
    switch_paint.layered = true;
    switch_paint.instance = (1, 2);
    let paints = [check_paint, switch_paint];
    let dirties = [
        PaintDirty {
            key: 7,
            bounds: checkbox,
            kind: PaintDirtyKind::Node,
        },
        PaintDirty {
            key: 8,
            bounds: switch,
            kind: PaintDirtyKind::Node,
        },
    ];
    let captured = node_present_clipped_capture_text_wipes_excluding_overlay(
        &dirties,
        &[icon],
        &paints,
        &[(7, checkbox), (8, switch)],
        &[],
        &[],
        1.5,
    );
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].text, "\u{e06c}");
}

#[test]
fn canvas_background_replays_overlapping_foreground_in_scene_order() {
    let board = [20.0, 20.0, 600.0, 160.0];
    let card = [36.0, 36.0, 176.0, 92.0];
    let mut grid = paint_slot(1, board, Some(board), (2, 2));
    grid.textured = (0, 1);
    grid.paint_ops = (2, 3);
    grid.layered = true;
    let mut foreground = paint_slot(2, card, Some(card), (2, 3));
    foreground.text = (0, 1);
    foreground.paint_ops = (3, 4);
    foreground.layered = true;
    let paints = [grid, foreground];
    let mut dirties = vec![PaintDirty {
        key: 1,
        bounds: board,
        kind: PaintDirtyKind::Node,
    }];
    prepare_node_present_dirties(&mut dirties, &paints, &[], &[], &[], &[], 1.0, None);
    assert_eq!(
        dirties[0].kind,
        PaintDirtyKind::Region,
        "grid repair must restore the card in the same frame"
    );
    assert!(!region_replays_own_chrome_only(1, board, &paints));

    // A clipped or separate foreground does not change the isolated canvas path.
    foreground.overflow_clip = Some([700.0, 20.0, 20.0, 20.0]);
    assert!(!slot_requires_ordered_replay(1, board, &[grid, foreground]));
    foreground.overflow_clip = None;
    foreground.bounds = [20.0, 200.0, 176.0, 92.0];
    assert!(!slot_requires_ordered_replay(1, board, &[grid, foreground]));
    // Ancestor ranges include the canvas and are not later foreground draws.
    foreground.bounds = board;
    foreground.paint_ops = (1, 4);
    assert!(!slot_requires_ordered_replay(1, board, &[grid, foreground]));
}

#[test]
fn dest_reset_later_avatar_must_not_punch_earlier_disk_crescent() {
    // 32px circles, -8px overlap. Earlier-sibling occupancy used to skip the
    // clip (hover:scale leave). AABB dest-reset of B then CLEARed A's crescent.
    let a = [0.0, 0.0, 32.0, 32.0];
    let b = [24.0, 0.0, 32.0, 32.0];
    let c = [48.0, 0.0, 32.0, 32.0];
    let mut page = blank_instance(0.0, 0.0, 320.0, 80.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fa = blank_instance(a[0], a[1], a[2], a[3]);
    fa.color = [0.9, 0.2, 0.2, 1.0];
    fa.radii = [16.0; 4];
    let mut fb = blank_instance(b[0], b[1], b[2], b[3]);
    fb.color = [0.2, 0.7, 0.3, 1.0];
    fb.radii = [16.0; 4];
    let mut fc = blank_instance(c[0], c[1], c[2], c[3]);
    fc.color = [0.95, 0.95, 0.95, 1.0];
    fc.radii = [16.0; 4];
    let paints = [
        paint_slot(1, a, Some(a), (1, 2)),
        paint_slot(2, b, Some(b), (2, 3)),
        paint_slot(3, c, Some(c), (3, 4)),
        paint_slot(
            0,
            [0.0, 0.0, 320.0, 80.0],
            Some([0.0, 0.0, 320.0, 80.0]),
            (0, 1),
        ),
    ];
    let instances = [page, fa, fb, fc];
    let (wipe_b, _) = node_present_dest_reset_scaled(2, b, &paints, &instances, &[], 1.0, None)
        .expect("card behind B");
    let crescent = [22.0, 16.0];
    let in_a_disk = {
        let dx = crescent[0] - 16.0;
        let dy = crescent[1] - 16.0;
        dx * dx + dy * dy <= 16.0 * 16.0
    };
    let in_b_disk = {
        let dx = crescent[0] - 40.0;
        let dy = crescent[1] - 16.0;
        dx * dx + dy * dy <= 16.0 * 16.0
    };
    assert!(
        in_a_disk && !in_b_disk,
        "precondition: (22,16) is A's crescent vs B"
    );
    assert!(
        crescent[0] < wipe_b[0]
            || crescent[0] >= wipe_b[0] + wipe_b[2]
            || crescent[1] < wipe_b[1]
            || crescent[1] >= wipe_b[1] + wipe_b[3],
        "B dest-reset must not AABB-clear A's crescent, wipe={wipe_b:?}"
    );
    let (wipe_c, _) = node_present_dest_reset_scaled(3, c, &paints, &instances, &[], 1.0, None)
        .expect("card behind C");
    let crescent_bc = [46.0, 16.0];
    assert!(
        crescent_bc[0] < wipe_c[0]
            || crescent_bc[0] >= wipe_c[0] + wipe_c[2]
            || crescent_bc[1] < wipe_c[1]
            || crescent_bc[1] >= wipe_c[1] + wipe_c[3],
        "C dest-reset must not AABB-clear B's crescent, wipe={wipe_c:?}"
    );
}

#[test]
fn dest_reset_layered_ping_must_not_paint_or_wipe_caption() {
    // Gallery ping is `position:absolute` (layered). Scale-2 visual overlaps
    // the caption; dest-reset and scissor must clip that rest-disjoint sibling
    // so the ring cannot src-over the label.
    let label = [8.0, 0.0, 40.0, 14.0];
    let rest = [16.0, 22.0, 32.0, 32.0];
    let visual = [0.0, 6.0, 64.0, 64.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(visual[0], visual[1], visual[2], visual[3]);
    fill.color = [0.2, 0.2, 0.85, 1.0];
    fill.radii = [32.0; 4];
    let mut ping_slot = paint_slot(2, visual, Some(rest), (1, 2));
    ping_slot.layered = true;
    let mut caption = paint_slot(1, label, Some(label), (0, 0));
    caption.text = (0, 1);
    let paints = [caption, ping_slot];
    let (wipe, _) =
        node_present_dest_reset_scaled(2, visual, &paints, &[page, fill], &[], 1.0, Some(visual))
            .expect("card behind ping");
    assert!(
        super::intersect_rects(wipe, label).is_none(),
        "layered ping dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
    );
    assert!(
        wipe[2] > 20.0 && wipe[3] > 20.0,
        "wipe must still cover the ping body, not collapse\n wipe={wipe:?}"
    );
    let sc = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        visual,
        &[(2, visual)],
        &paints,
        node_present_scissor_pad(PaintDirtyKind::Node),
    );
    assert!(
        super::intersect_rects(sc, label).is_none(),
        "layered ping scissor must not paint the caption\n scissor={sc:?} label={label:?}"
    );
}

#[test]
fn dest_reset_play_state_spin_must_cover_circumcircle_against_caption_and_button() {
    // Gallery play-state stages the 2.5rem spinner in 5rem so the 45°
    // circumcircle (+ 8px scissor pad) stays off the caption/button.
    let label = [8.0, 0.0, 72.0, 14.0];
    let rest = [20.0, 40.0, 40.0, 40.0];
    let button = [16.0, 106.0, 48.0, 28.0];
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let xf = [c, s, -s, c, 0.0, 0.0];
    let visual = crate::renderer::aabb_of_centered_transform(rest, &xf);
    let mut page = blank_instance(0.0, 0.0, 200.0, 150.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(rest[0], rest[1], rest[2], rest[3]);
    fill.color = [0.1, 0.1, 0.12, 1.0];
    fill.radii = [6.0; 4];
    fill.transform = xf;
    let mut btn_fill = blank_instance(button[0], button[1], button[2], button[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 1.0];
    btn_fill.radii = [6.0; 4];
    let spin_slot = paint_slot(2, visual, Some(rest), (1, 2));
    let mut caption = paint_slot(1, label, Some(label), (0, 0));
    caption.text = (0, 1);
    let paints = [
        caption,
        spin_slot,
        paint_slot(3, button, Some(button), (2, 3)),
    ];
    let instances = [page, fill, btn_fill];
    let (wipe, _) =
        node_present_dest_reset_scaled(2, visual, &paints, &instances, &[], 1.0, Some(visual))
            .expect("card behind play-state spin");
    assert!(
            aabb_covers(wipe, visual),
            "pause dest-reset must wipe the 45° circumcircle, not leftover frames\n wipe={wipe:?} visual={visual:?} rest={rest:?} label={label:?} button={button:?}"
        );
    assert!(
        super::intersect_rects(wipe, label).is_none(),
        "spin dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
    );
    let sc = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        visual,
        &[(2, visual)],
        &paints,
        node_present_scissor_pad(PaintDirtyKind::Node),
    );
    assert!(
        aabb_covers(sc, visual),
        "pause scissor must cover the 45° circumcircle\n scissor={sc:?} visual={visual:?}"
    );
    assert!(
        super::intersect_rects(sc, label).is_none(),
        "spin scissor must not dest-reset the caption\n scissor={sc:?} label={label:?}"
    );
    let encode = super::union_xywh(sc, wipe);
    assert!(
        super::intersect_rects(encode, label).is_none(),
        "GPU scissor ∪ wipe must not dest-reset the caption\n encode={encode:?} label={label:?}"
    );
}

#[test]
fn dest_reset_play_state_spin_must_not_punch_caption_when_circumcircle_nips_it() {
    // Tight column (no stage): 45° tips nip the title. Occupancy clip must
    // still keep dest-reset/scissor off the caption (ghost 「play-state」).
    let label = [8.0, 0.0, 72.0, 14.0];
    let rest = [16.0, 20.0, 40.0, 40.0];
    let button = [12.0, 66.0, 48.0, 28.0];
    let deg45 = std::f32::consts::FRAC_PI_4;
    let (s, c) = deg45.sin_cos();
    let xf = [c, s, -s, c, 0.0, 0.0];
    let visual = crate::renderer::aabb_of_centered_transform(rest, &xf);
    let mut page = blank_instance(0.0, 0.0, 200.0, 120.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(rest[0], rest[1], rest[2], rest[3]);
    fill.color = [0.1, 0.1, 0.12, 1.0];
    fill.radii = [6.0; 4];
    fill.transform = xf;
    let mut btn_fill = blank_instance(button[0], button[1], button[2], button[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 1.0];
    btn_fill.radii = [6.0; 4];
    let spin_slot = paint_slot(2, visual, Some(rest), (1, 2));
    let mut caption = paint_slot(1, label, Some(label), (0, 0));
    caption.text = (0, 1);
    let paints = [
        caption,
        spin_slot,
        paint_slot(3, button, Some(button), (2, 3)),
    ];
    let (wipe, _) = node_present_dest_reset_scaled(
        2,
        visual,
        &paints,
        &[page, fill, btn_fill],
        &[],
        1.0,
        Some(visual),
    )
    .expect("card behind play-state spin");
    assert!(
            super::intersect_rects(wipe, label).is_none(),
            "tight-column spin dest-reset must not punch the caption\n wipe={wipe:?} label={label:?} visual={visual:?}"
        );
    assert!(
        aabb_covers(wipe, rest),
        "wipe must still cover the unrotated box\n wipe={wipe:?} rest={rest:?}"
    );
    let sc = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        visual,
        &[(2, visual)],
        &paints,
        node_present_scissor_pad(PaintDirtyKind::Node),
    );
    assert!(
            super::intersect_rects(sc, label).is_none(),
            "tight-column spin scissor must not dest-reset the caption\n scissor={sc:?} label={label:?}"
        );
}

#[test]
fn dest_reset_bounce_translate_must_not_punch_caption_above() {
    let label = [8.0, 0.0, 48.0, 14.0];
    let rest = [16.0, 22.0, 32.0, 32.0];
    let visual = [16.0, -3.0, 32.0, 32.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(visual[0], visual[1], visual[2], visual[3]);
    fill.color = [0.92, 0.92, 0.94, 1.0];
    fill.radii = [6.0; 4];
    let mut caption = paint_slot(1, label, Some(label), (0, 0));
    caption.text = (0, 1);
    let paints = [caption, paint_slot(2, visual, Some(rest), (1, 2))];
    let (wipe, _) =
        node_present_dest_reset_scaled(2, visual, &paints, &[page, fill], &[], 1.0, Some(visual))
            .expect("card behind bounce");
    let lx = label[0] + label[2] * 0.5;
    let ly = label[1] + label[3] * 0.5;
    assert!(
        !super::point_in_rect(wipe, [lx, ly]),
        "bounce dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
    );
}

#[test]
fn dest_reset_pulse_must_not_crop_to_grown_ping_neighbor() {
    // HStack gap 20px: ping scale-2 visual nips the padded pulse scissor but
    // rest boxes stay disjoint. Clipping occupancy against the grown visual
    // left nested dest-reset frames on alternate-pulse.
    let ping_rest = [0.0, 20.0, 32.0, 32.0];
    let ping_visual = [-16.0, 4.0, 64.0, 64.0];
    let pulse = [52.0, 20.0, 48.0, 32.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut ping_fill = blank_instance(
        ping_visual[0],
        ping_visual[1],
        ping_visual[2],
        ping_visual[3],
    );
    ping_fill.color = [0.2, 0.2, 0.85, 0.4];
    ping_fill.radii = [32.0; 4];
    let mut pulse_fill = blank_instance(pulse[0], pulse[1], pulse[2], pulse[3]);
    pulse_fill.color = [0.55, 0.35, 0.85, 0.5];
    pulse_fill.radii = [6.0; 4];
    let paints = [
        paint_slot(1, ping_visual, Some(ping_rest), (1, 2)),
        paint_slot(2, pulse, Some(pulse), (2, 3)),
    ];
    let instances = [page, ping_fill, pulse_fill];
    let (wipe, _) =
        node_present_dest_reset_scaled(2, pulse, &paints, &instances, &[], 1.0, Some(pulse))
            .expect("card behind pulse");
    assert!(
            aabb_covers(wipe, pulse),
            "pulse dest-reset must wipe the full rounded box, not a leftover inner frame\n wipe={wipe:?} pulse={pulse:?}"
        );
    let sc = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        pulse,
        &[(2, pulse)],
        &paints,
        node_present_scissor_pad(PaintDirtyKind::Node),
    );
    assert!(
            aabb_covers(sc, pulse),
            "pulse scissor must not crop to ping's grown visual\n scissor={sc:?} pulse={pulse:?} ping_visual={ping_visual:?}"
        );
}

#[test]
fn dest_reset_pulse_last_present_must_cover_vertex_aa() {
    // `animation: … alternate pulse` only changes opacity. Last-present dest-reset
    // used the layout box (no +2 vertex AA), so rounded-md corners left a frame.
    let pulse = [52.0, 20.0, 48.0, 32.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut pulse_fill = blank_instance(pulse[0], pulse[1], pulse[2], pulse[3]);
    pulse_fill.color = [0.55, 0.35, 0.85, 0.5];
    pulse_fill.radii = [6.0; 4];
    let paints = [paint_slot(2, pulse, Some(pulse), (1, 2))];
    let ink = instance_paint_bounds(&pulse_fill);
    let (wipe, _) = node_present_dest_reset_scaled(
        2,
        pulse,
        &paints,
        &[page, pulse_fill],
        &[],
        1.0,
        Some(pulse),
    )
    .expect("card behind pulse");
    assert!(
            aabb_covers(wipe, ink),
            "pulse dest-reset must cover SDF vertex AA, not just the layout box\n wipe={wipe:?} ink={ink:?} pulse={pulse:?}"
        );
    assert!(
        wipe[0] <= pulse[0] - 1.5 && wipe[1] <= pulse[1] - 1.5,
        "wipe must outset the rounded box by vertex AA\n wipe={wipe:?} pulse={pulse:?}"
    );
}

#[test]
fn instance_paint_bounds_includes_vertex_aa_for_simple_fill() {
    let i = blank_instance(10.0, 10.0, 40.0, 20.0);
    let b = instance_paint_bounds(&i);
    assert!(
        b[0] <= 8.0 && b[1] <= 8.0 && b[0] + b[2] >= 52.0 && b[1] + b[3] >= 32.0,
        "simple fill occupancy must include shader vertex AA (+2), got {b:?}"
    );
}

#[test]
fn layered_scale_reset_and_last_present_cover_sdf_antialiasing() {
    for scale in [0.2, 0.34, 0.75, 1.0] {
        let mut logical = blank_instance(40.0, 40.0, 20.0, 56.0);
        logical.radii = [6.0; 4];
        logical.transform = [1.0, 0.0, 0.0, scale, 0.0, 28.0 * (1.0 - scale)];
        let bounds = instance_fill_bounds(&logical);
        let ink = instance_paint_bounds(&logical);
        let mut owner = paint_slot(7, bounds, Some([40.0, 40.0, 20.0, 56.0]), (1, 2));
        owner.layered = true;
        let paints = [owner];
        let visual = node_paint_visual_bounds_in(
            &paints[0],
            &[blank_instance(0.0, 0.0, 200.0, 120.0), logical],
            &[],
            &[],
            &[],
            &paints,
        );
        assert!(
            aabb_covers(visual, ink),
            "scale={scale} visual={visual:?} ink={ink:?}"
        );
        let mut physical = blank_instance(60.0, 60.0, 30.0, 84.0);
        physical.radii = [9.0; 4];
        physical.transform = [1.0, 0.0, 0.0, scale, 0.0, 42.0 * (1.0 - scale)];
        let physical_ink = instance_paint_bounds(&physical);
        let wipe = node_present_reset_rect(
            7,
            bounds,
            &paints,
            &[blank_instance(0.0, 0.0, 300.0, 180.0), physical],
            &[],
            &[],
            NODE_PRESENT_RESET_PAD_PHYSICAL,
            1.5,
            Some(bounds),
        )
        .unwrap();
        assert!(
            aabb_covers(wipe, physical_ink),
            "scale={scale} wipe={wipe:?} ink={physical_ink:?}"
        );
    }
}

#[test]
fn dest_reset_scale_chip_must_not_aabb_fill_capsule() {
    // Reaction chip hover:scale dest-resets a Bake AABB that sits on a capsule
    // with outline. Filling that AABB with the pill's muted color squares the
    // end and erases the 3px card ring. Clear must be the card; replay is the
    // pill's own SDF chrome (fill + outline), not the parent's children.
    let bar = [10.0, 10.0, 48.0, 28.0];
    let chip = [32.0, 16.0, 16.0, 16.0];
    let mut page = blank_instance(0.0, 0.0, 200.0, 80.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    fill.color = [0.82, 0.82, 0.84, 1.0];
    fill.radii = [14.0; 4];
    let mut outline = blank_instance(7.0, 7.0, 54.0, 34.0);
    outline.color = [0.0, 0.0, 0.0, 0.0];
    outline.border_color = [0.96, 0.96, 0.96, 1.0];
    outline.border_width = 3.0;
    outline.radii = [17.0; 4];
    let bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
    let bar_p = NodePaint {
        key: 1,
        bounds: bar,
        overlay: false,
        instance: (1, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let chip_p = NodePaint {
        key: 2,
        bounds: chip,
        overlay: false,
        instance: (3, 3),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let card_color = page.color;
    let muted = fill.color;
    let paints = [bar_p, chip_p];
    let instances = [page, fill, outline];
    let bake_bounds = textured_paint_bounds(&bake);
    let reset = node_present_dest_reset_full(2, chip, &paints, &instances, &[bake], &[], 1.0, None)
        .expect("card under capsule");
    assert_eq!(
            reset.clear, card_color,
            "AABB clear of the pill fill would square the capsule; must use the card\n clear={:?} muted={:?}",
            reset.clear, muted
        );
    assert_eq!(
        reset.replay,
        (1, 3),
        "replay must be the pill fill+outline, not descendant Bake\n replay={:?}",
        reset.replay
    );
    assert!(
        aabb_covers(reset.rect, bake_bounds),
        "wipe must still cover the Bake quad so shrink can erase it\n wipe={:?} bake={:?}",
        reset.rect,
        bake_bounds
    );
}

#[test]
fn dest_reset_replay_must_not_dump_descendant_instances() {
    // Gallery hitch: hover:scale dest-reset used to replay the owner's full
    // collect range (every descendant quad, cull:None) when overflow chips
    // sit outside the pill AABB so nested-foreign detection misses them.
    let bar = [10.0, 10.0, 48.0, 28.0];
    let chip = [32.0, 16.0, 16.0, 16.0];
    let mut page = blank_instance(0.0, 0.0, 800.0, 2400.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    fill.color = [0.82, 0.82, 0.84, 1.0];
    fill.radii = [14.0; 4];
    let mut outline = blank_instance(7.0, 7.0, 54.0, 34.0);
    outline.color = [0.0, 0.0, 0.0, 0.0];
    outline.border_color = [0.96, 0.96, 0.96, 1.0];
    outline.border_width = 3.0;
    outline.radii = [17.0; 4];
    let extra = 64usize;
    let mut instances = vec![page, fill, outline];
    let mut paints = Vec::new();
    let bar_end = 3 + extra;
    paints.push(NodePaint {
        key: 1,
        bounds: bar,
        overlay: false,
        instance: (1, bar_end),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    });
    for i in 0..extra {
        let x = 80.0 + i as f32 * 12.0;
        let mut child = blank_instance(x, 48.0, 10.0, 10.0);
        child.color = [1.0, 0.0, 0.0, 1.0];
        instances.push(child);
        let idx = 3 + i;
        paints.push(NodePaint {
            key: 10 + i as u64,
            bounds: [x, 48.0, 10.0, 10.0],
            overlay: false,
            instance: (idx, idx + 1),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        });
    }
    paints.push(NodePaint {
        key: 2,
        bounds: chip,
        overlay: false,
        instance: (bar_end, bar_end),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    });
    let bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
    let reset = node_present_dest_reset_full(2, chip, &paints, &instances, &[bake], &[], 1.0, None)
        .expect("card under capsule");
    assert_eq!(
            reset.replay,
            (1, 3),
            "replay must stop at pill fill+outline, not dump {extra} overflow descendants\n replay={:?}",
            reset.replay
        );
}

#[test]
fn dest_reset_text_shadow_shrink_must_union_prev() {
    // Glyph ink is not dest-ink (3px pad would eat a 1px parent border).
    // Last-present visual must still cover text-shadow outset.
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let mut shadowed = sample_text("hi", box_[0]);
    shadowed.y = box_[1];
    shadowed.width = box_[2];
    shadowed.height = box_[3];
    shadowed.style.shadow_offset = [12.0, 12.0];
    shadowed.style.shadow_blur = 4.0;
    let p = NodePaint {
        key: 24,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let blob = text_paint_bounds(&shadowed);
    let prev = node_paint_visual_bounds(&p, &[card, fill], &[], &[shadowed], &[]);
    let (wipe, _) =
        node_present_dest_reset_scaled(24, box_, &[p], &[card, fill], &[], 1.0, Some(prev))
            .expect("card behind after text-shadow shrink");
    assert!(
            aabb_covers(wipe, blob),
            "shrinking text-shadow must dest-reset the previous glyph outset\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
    let (wipe_no_prev, _) =
        node_present_dest_reset_scaled(24, box_, &[p], &[card, fill], &[], 1.0, None)
            .expect("wipe without last-present");
    assert!(
            !aabb_covers(wipe_no_prev, blob),
            "fill + 3px pad cannot cover offset 12 / blur 4 text-shadow; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
}

fn rect_contains_point(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

#[test]
fn dest_reset_overflowing_glyphs_must_not_wipe_header_or_sibling() {
    // Gallery Marker: unclipped "Generating response…" / shimmer-box last-present
    // grew to the glyph AABB and dest-reset punched the sticky title and the
    // next column. This node only recaptures its own slot.
    let mut page = blank_instance(0.0, 0.0, 400.0, 200.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let header = [0.0, 0.0, 400.0, 32.0];
    let mut header_fill = blank_instance(header[0], header[1], header[2], header[3]);
    header_fill.color = [1.0, 1.0, 1.0, 1.0];
    let sibling = [220.0, 40.0, 160.0, 24.0];
    let mut sibling_fill = blank_instance(sibling[0], sibling[1], sibling[2], sibling[3]);
    sibling_fill.color = [0.2, 0.4, 0.8, 1.0];
    let label_box = [8.0, 40.0, 80.0, 20.0];
    let mut overflow = sample_text("Generating response…", 8.0);
    overflow.y = 16.0;
    overflow.width = 360.0;
    overflow.height = 48.0;
    overflow.clip_bounds = None;
    let p = NodePaint {
        key: 31,
        bounds: label_box,
        overlay: false,
        instance: (3, 3),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let instances = [page, header_fill, sibling_fill];
    let prev = node_paint_visual_bounds(&p, &instances, &[], &[overflow], &[]);
    assert!(
            !rect_contains_point(prev, 200.0, 16.0),
            "last-present must not follow unclipped overflow into the header\n prev={prev:?} slot={label_box:?}"
        );
    assert!(
            !rect_contains_point(prev, sibling[0] + 8.0, sibling[1] + 8.0),
            "last-present must not follow unclipped overflow into a sibling column\n prev={prev:?} sibling={sibling:?}"
        );
    let (wipe, _) =
        node_present_dest_reset_scaled(31, label_box, &[p], &instances, &[], 1.0, Some(prev))
            .expect("page behind overflowing label");
    assert!(
            !rect_contains_point(wipe, 200.0, 16.0),
            "overflow dest-reset must not punch the page header\n wipe={wipe:?} header={header:?} prev={prev:?}"
        );
    assert!(
            !rect_contains_point(wipe, sibling[0] + 8.0, sibling[1] + 8.0),
            "overflow dest-reset must not punch a sibling column\n wipe={wipe:?} sibling={sibling:?} prev={prev:?}"
        );
    assert!(
        wipe[2] < 160.0 && wipe[3] < 80.0,
        "wipe must stay near the label slot, not a page band\n wipe={wipe:?}"
    );
}

#[test]
fn dest_reset_recycled_prev_must_not_punch_title_or_sibling() {
    // Gallery: NodeId recycle / last-present of a parent slot made prev the
    // whole inset. Dest-reset unioned that AABB and emptied the title bar
    // and sibling Marker columns while this leaf's glyphs redrew.
    let mut page = blank_instance(0.0, 0.0, 400.0, 208.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let title = [0.0, 0.0, 400.0, 48.0];
    let body_clip = [0.0, 48.0, 400.0, 160.0];
    let sibling = [220.0, 56.0, 160.0, 80.0];
    let label_box = [8.0, 56.0, 80.0, 20.0];
    let mut fill = blank_instance(label_box[0], label_box[1], label_box[2], label_box[3]);
    fill.color = [0.9, 0.9, 0.92, 1.0];
    let p = NodePaint {
        key: 41,
        bounds: label_box,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(body_clip),
        layered: false,
        rest: None,
    };
    let huge_prev = [0.0, 0.0, 400.0, 208.0];
    let (wipe, _) = node_present_dest_reset_scaled(
        41,
        label_box,
        &[p],
        &[page, fill],
        &[],
        1.0,
        Some(huge_prev),
    )
    .expect("page behind recycled prev");
    assert!(
        !rect_contains_point(wipe, title[0] + 40.0, title[1] + 24.0),
        "recycled last-present must not dest-reset the title\n wipe={wipe:?} title={title:?}"
    );
    assert!(
            !rect_contains_point(wipe, sibling[0] + 8.0, sibling[1] + 8.0),
            "recycled last-present must not dest-reset a sibling column\n wipe={wipe:?} sibling={sibling:?}"
        );
    assert!(
        wipe[2] < 200.0 && wipe[3] < 120.0,
        "wipe must stay near the leaf, not the inset\n wipe={wipe:?}"
    );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        41,
        label_box,
        &[(41, huge_prev)],
        &[p],
        8.0,
    );
    assert!(
        !rect_contains_point(scissor, title[0] + 40.0, title[1] + 24.0),
        "node-present scissor must not cover the title\n scissor={scissor:?} title={title:?}"
    );
    // Title bottom sits 4px above the ScrollArea. Pad-after-overflow without
    // re-intersect leaks 8px into that strip; the helper must not.
    assert!(
            !rect_contains_point(scissor, title[0] + 40.0, title[1] + title[3] - 4.0),
            "scissor pad must not leak past overflow clip into the title bottom\n scissor={scissor:?} title={title:?}"
        );
}

#[test]
fn dest_reset_unmatched_island_key_must_wipe_old_title_box() {
    // Gallery events → 基础: Match stamps a new Taffy id on the title island.
    // Dest-reset used to return None when the dirty key is gone from this
    // frame's node_paints, so PartialLoad kept 「指针事件」 under 「基础」.
    let old_title = [80.0, 8.0, 96.0, 24.0];
    let new_title = [80.0, 8.0, 36.0, 24.0];
    let mut page = blank_instance(0.0, 0.0, 400.0, 200.0);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut now_fill = blank_instance(new_title[0], new_title[1], new_title[2], new_title[3]);
    now_fill.color = [0.96, 0.96, 0.96, 1.0];
    let now = NodePaint {
        key: 99,
        bounds: new_title,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let (wipe, _) =
        node_present_dest_reset_scaled(41, old_title, &[now], &[page, now_fill], &[], 1.0, None)
            .expect("unmatched island dirty must still dest-reset the snapshotted box");
    assert!(
            aabb_covers(wipe, old_title),
            "must wipe leftover 「指针事件」 glyphs, not only the new 「基础」 box\n wipe={wipe:?} old={old_title:?} now={new_title:?}"
        );
    assert!(
        !aabb_covers(wipe, [0.0, 0.0, 400.0, 200.0]),
        "must not fall back to a window wipe\n wipe={wipe:?}"
    );
}

#[test]
fn dest_reset_reused_page_island_must_wipe_previous_page_band() {
    // Gallery 浮层 → 数据: Match reuses the ScrollArea Taffy id. Occupancy
    // used to shrink that dest-reset against a tall foreign slot whose
    // center sits above the island, leaving ViewportModal in retain.
    let island = [200.0, 80.0, 800.0, 600.0];
    let leftover = [220.0, 100.0, 400.0, 180.0];
    let table = [220.0, 300.0, 760.0, 140.0];
    let tall_foreign = [200.0, -1000.0, 800.0, 1200.0];
    let mut page = blank_instance(island[0], island[1], island[2], island[3]);
    page.color = [0.96, 0.96, 0.96, 1.0];
    let mut table_fill = blank_instance(table[0], table[1], table[2], table[3]);
    table_fill.color = [1.0, 1.0, 1.0, 1.0];
    let mut foreign_fill = blank_instance(
        tall_foreign[0],
        tall_foreign[1],
        tall_foreign[2],
        tall_foreign[3],
    );
    foreign_fill.color = [0.9, 0.9, 0.9, 1.0];
    let host = NodePaint {
        key: 1,
        bounds: island,
        overlay: false,
        instance: (0, 1),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(island),
        layered: false,
        rest: Some(island),
    };
    let table_slot = NodePaint {
        key: 2,
        bounds: table,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(island),
        layered: false,
        rest: Some(table),
    };
    let foreign = NodePaint {
        key: 3,
        bounds: tall_foreign,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: Some(tall_foreign),
    };
    let (wipe, _) = node_present_dest_reset_scaled(
        1,
        island,
        &[host, table_slot, foreign],
        &[page, table_fill, foreign_fill],
        &[],
        1.0,
        Some(island),
    )
    .expect("reused page island must dest-reset");
    let lx = leftover[0] + leftover[2] * 0.5;
    let ly = leftover[1] + leftover[3] * 0.5;
    assert!(
            rect_contains_point(wipe, lx, ly),
            "must wipe leftover ViewportModal band, not occupancy-clip it away\n wipe={wipe:?} leftover={leftover:?}"
        );
    assert!(
            aabb_covers(wipe, table) || rect_contains_point(wipe, table[0] + 8.0, table[1] + 8.0),
            "island dest-reset should still cover the new table neighborhood\n wipe={wipe:?} table={table:?}"
        );
}

#[test]
fn overlay_restore_scissor_is_visual_occupancy() {
    assert_eq!(
            node_present_scissor_pad(PaintDirtyKind::OverlayRestore),
            0.0,
            "Restore dest-reset is already visual occupancy; extra pad leaves a white frame the overlay fill cannot cover"
        );
}

#[test]
fn slot_paint_ops_keep_track_below_thumb_and_exclude_neighbors() {
    use crate::layout::layout_tree::PaintOp;
    let ops = [
        PaintOp::Rects { start: 0, count: 4 },
        PaintOp::Textured { start: 0, count: 2 },
        PaintOp::Rects { start: 4, count: 3 },
    ];
    let clipped: Vec<_> = slot_paint_ops(&ops, (2, 5), (1, 2)).collect();
    assert!(matches!(
        clipped.as_slice(),
        [
            PaintOp::Rects { start: 2, count: 2 },
            PaintOp::Textured { start: 1, count: 1 },
            PaintOp::Rects { start: 4, count: 1 },
        ]
    ));
}

#[test]
fn region_own_paint_includes_antialias_from_adjacent_slots() {
    use crate::layout::layout_tree::PaintOp;
    let mut adjacent = slot(1, [0.0, 11.0, 20.0, 10.0], (0, 0));
    adjacent.instance = (0, 1);
    let ops = region_own_paint_ops(
        &[PaintOp::Rects { start: 0, count: 1 }],
        &[adjacent],
        &[blank_instance(0.0, 11.0, 20.0, 10.0)],
        0,
        [0.0, 9.0, 20.0, 1.0],
        1.0,
    );
    assert!(matches!(
        ops.as_slice(),
        [PaintOp::Rects { start: 0, count: 1 }]
    ));
}

#[test]
fn region_own_paint_replays_parent_track_before_child_thumb() {
    use crate::layout::layout_tree::PaintOp;
    let mut track = slot(1, [10.0, 10.0, 12.0, 200.0], (0, 0));
    track.instance = (0, 2);
    let mut thumb = slot(2, [10.0, 10.0, 12.0, 40.0], (0, 0));
    thumb.instance = (1, 2);
    let instances = [
        blank_instance(10.0, 10.0, 12.0, 200.0),
        blank_instance(10.0, 10.0, 12.0, 40.0),
    ];
    let ops = region_own_paint_ops(
        &[PaintOp::Rects { start: 0, count: 2 }],
        &[thumb, track],
        &instances,
        0,
        thumb.bounds,
        1.0,
    );
    assert!(matches!(
        ops.as_slice(),
        [
            PaintOp::Rects { start: 0, count: 1 },
            PaintOp::Rects { start: 1, count: 1 },
        ]
    ));
}

#[test]
fn overlay_restore_must_cover_intersecting_page_caption() {
    let hole = [303.0, 990.0, 176.0, 232.0];
    let caption = slot(8, [450.0, 1010.0, 300.0, 22.0], (0, 1));
    let mut menu = slot(7, [319.0, 1006.0, 144.0, 200.0], (1, 2));
    menu.overlay = true;
    assert_eq!(
        node_present_scissor_logical(
            PaintDirtyKind::OverlayRestore,
            7,
            hole,
            &[],
            &[menu, caption],
            0.0,
        ),
        hole,
        "overlay restore reconstructs the caption and must retain the whole shadow hole"
    );
}

#[test]
fn node_present_scissor_pad_must_not_leak_past_overflow_clip() {
    let title = [0.0, 0.0, 400.0, 48.0];
    let body_clip = [0.0, 48.0, 400.0, 160.0];
    let label_box = [8.0, 56.0, 80.0, 20.0];
    let p = NodePaint {
        key: 41,
        bounds: label_box,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some(body_clip),
        layered: false,
        rest: None,
    };
    let huge_prev = [0.0, 0.0, 400.0, 208.0];
    let clipped = intersect_rects(
        union_xywh(
            label_box,
            clamp_last_present_prev(huge_prev, label_box).unwrap_or(label_box),
        ),
        body_clip,
    )
    .expect("prev ∩ body");
    let leaked = padded_xywh(clipped, 8.0);
    assert!(
            rect_contains_point(leaked, title[0] + 40.0, title[1] + title[3] - 4.0),
            "precondition: pad after overflow without re-intersect leaks into the title\n leaked={leaked:?}"
        );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        41,
        label_box,
        &[(41, huge_prev)],
        &[p],
        8.0,
    );
    assert!(
            !rect_contains_point(scissor, title[0] + 40.0, title[1] + title[3] - 4.0),
            "product scissor must re-intersect overflow after pad\n scissor={scissor:?} leaked={leaked:?}"
        );
    assert!(
        rect_contains_point(scissor, label_box[0] + 4.0, label_box[1] + 4.0),
        "scissor must still cover the dirty leaf\n scissor={scissor:?} slot={label_box:?}"
    );
}

fn outline_remove_fixture() -> (
    InstanceInput,
    InstanceInput,
    InstanceInput,
    NodePaint,
    NodePaint,
    [f32; 4],
    [f32; 4],
) {
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let outline_w = 8.0_f32;
    let mut outline = blank_instance(
        box_[0] - outline_w,
        box_[1] - outline_w,
        box_[2] + outline_w * 2.0,
        box_[3] + outline_w * 2.0,
    );
    outline.color = [0.0, 0.0, 0.0, 0.0];
    outline.border_width = outline_w;
    outline.border_color = [0.0, 0.0, 1.0, 1.0];
    let p_on = NodePaint {
        key: 21,
        bounds: box_,
        overlay: false,
        instance: (1, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let p_off = NodePaint {
        instance: (1, 2),
        ..p_on
    };
    let ring = instance_paint_bounds(&outline);
    (card, fill, outline, p_on, p_off, box_, ring)
}

#[test]
fn last_present_after_instance_take_drops_outline_outset() {
    // Product path used to record last-present *after* take: instances empty,
    // visual collapsed to the layout box, dest-reset could not cover the ring.
    let (card, fill, outline, p_on, p_off, box_, ring) = outline_remove_fixture();
    let mut instances = vec![card, fill, outline];
    let before = last_present_visual_slots(&[p_on], &instances, &[], &[], &[]);
    let _taken = std::mem::take(&mut instances);
    let after = last_present_visual_slots(&[p_on], &instances, &[], &[], &[]);
    let prev_before = before.iter().find(|(k, _)| *k == 21).map(|(_, b)| *b);
    let prev_after = after.iter().find(|(k, _)| *k == 21).map(|(_, b)| *b);
    assert!(
        aabb_covers(prev_before.expect("slot before take"), ring),
        "logical last-present must cover outline outset before take"
    );
    assert!(
            !prev_after.is_some_and(|b| aabb_covers(b, ring)),
            "recording after take drops instance outset (empty visual is omitted)\n after={prev_after:?} ring={ring:?}"
        );
    let (wipe_before, _) =
        node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, prev_before)
            .expect("card behind");
    assert!(
        aabb_covers(wipe_before, ring),
        "prev recorded before take must dest-reset the ring"
    );
    let (wipe_after, _) =
        node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, prev_after)
            .expect("card behind after-take prev");
    assert!(
        !aabb_covers(wipe_after, ring),
        "prev recorded after take cannot dest-reset the ring\n wipe={wipe_after:?} ring={ring:?}"
    );
}

#[test]
fn last_present_prev_stays_logical_at_2x() {
    let (card, fill, outline, p_on, p_off, box_, ring) = outline_remove_fixture();
    let instances = [card, fill, outline];
    let prev = last_present_visual_slots(&[p_on], &instances, &[], &[], &[])
        .into_iter()
        .find(|(k, _)| *k == 21)
        .map(|(_, b)| b)
        .expect("slot");
    let s = 2.0_f32;
    let ring_phys = scale_xywh(ring, s);
    let (wipe, _) =
        node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], s, Some(prev))
            .expect("2x dest-reset");
    assert!(
            aabb_covers(wipe, ring_phys),
            "logical prev × ui_scale must cover the physical ring\n wipe={wipe:?} ring={ring_phys:?} prev={prev:?}"
        );
    let (wipe_double, _) = node_present_dest_reset_scaled(
        21,
        box_,
        &[p_off],
        &[card, fill],
        &[],
        s,
        Some(scale_xywh(prev, s)),
    )
    .expect("double-scaled prev");
    assert_ne!(
            wipe_double, wipe,
            "recording after scale_instances_in_place would mis-scale prev and over-wipe\n logical_wipe={wipe:?} doubled={wipe_double:?}"
        );
}

#[test]
#[cfg(feature = "product-tests")]
fn scaled_preview_hover_capture_preserves_caption_clip() {
    crate::core::reactivity::create_root(|| {
        let button: crate::core::tree::Element = crate::components::button("preview-hover")
            .text("Destructive")
            .into();
        let preview = crate::core::tree::Element::new()
            .w(390.0)
            .h(844.0)
            .absolute()
            .left(40.0)
            .top(40.0)
            .scale(0.65)
            .transform_origin("left top")
            .overflow("hidden")
            .child(button.w(120.0).h(32.0));
        let frame = crate::core::tree::Element::new()
            .absolute()
            .left(40.0)
            .top(40.0)
            .w(267.0)
            .h(562.0)
            .z_index(1)
            .child(
                crate::core::tree::Element::new()
                    .absolute()
                    .left(253.0)
                    .top(0.0)
                    .w(14.0)
                    .h(548.0)
                    .bg([0.8, 0.8, 0.8, 1.0]),
            )
            .child(
                crate::core::tree::Element::new()
                    .absolute()
                    .left(0.0)
                    .top(548.0)
                    .w(267.0)
                    .h(14.0)
                    .bg([0.8, 0.8, 0.8, 1.0]),
            );
        let mut root = crate::core::tree::Element::new()
            .w(680.0)
            .h(720.0)
            .children([preview, frame]);
        let mut tree = crate::core::tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 680.0, 720.0, [-1.0; 2]);
        let button = &root.children[0].children[0];
        let dirty = PaintDirty {
            key: crate::app::damage::paint_element_key(button),
            bounds: crate::core::tree::visual_paint_aabb(button, [0.0; 2]),
            kind: PaintDirtyKind::Node,
        };
        let caption = result
            .text_nodes
            .iter()
            .find(|node| node.text == "Destructive")
            .unwrap();
        let capture = node_present_clipped_capture_text_wipes_excluding_overlay(
            &[dirty],
            &result.text_nodes,
            &result.node_paints,
            &[],
            &[],
            &result.instances,
            1.0,
        );
        let captured = capture
            .iter()
            .find(|node| node.text == "Destructive")
            .expect("hover must recapture caption");
        let ink = text_paint_bounds(caption);
        let clip = captured.clip_bounds.unwrap();
        assert!(
            clip[1] <= ink[1] && clip[1] + clip[3] >= ink[1] + ink[3],
            "caption ink={ink:?} clip={clip:?} dirty={dirty:?} paints={:?}",
            result.node_paints
        );
    });
}

#[test]
fn scaled_text_damage_and_present_bounds_cover_the_painted_run() {
    let mut text = sample_text("scaled", 565.5);
    text.y = 819.0;
    text.width = 43.0;
    text.height = 12.0;
    text.paint_scale = 3.0;
    let bounds = [565.5, 819.0, 129.0, 36.0];
    let paint = NodePaint {
        key: 1,
        bounds,
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    assert_eq!(text_paint_bounds(&text), bounds);
    assert_eq!(
        last_present_visual_slots(&[paint], &[], &[], &[text], &[]),
        vec![(1, bounds)],
        "scroll repairs must include the bottom of enlarged text",
    );
}

#[test]
fn last_present_before_take_covers_filter_wrap_and_text_shadow() {
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let big = blank_textured(8.0, 8.0, 64.0, 44.0, MAT2D_IDENTITY);
    let p_tex = NodePaint {
        key: 23,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 1),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let mut tex = vec![big.clone()];
    let before = last_present_visual_slots(&[p_tex], &[card, fill], &tex, &[], &[]);
    let _ = std::mem::take(&mut tex);
    let after = last_present_visual_slots(&[p_tex], &[card, fill], &tex, &[], &[]);
    let blob = textured_paint_bounds(&big);
    let prev_before = before.iter().find(|(k, _)| *k == 23).map(|(_, b)| *b);
    let prev_after = after.iter().find(|(k, _)| *k == 23).map(|(_, b)| *b);
    assert!(aabb_covers(prev_before.expect("tex before"), blob));
    assert!(
        !aabb_covers(prev_after.expect("tex after take"), blob),
        "taking textured draws must drop filter-wrap outset from last-present"
    );

    let mut shadowed = sample_text("hi", box_[0]);
    shadowed.y = box_[1];
    shadowed.width = box_[2];
    shadowed.height = box_[3];
    shadowed.style.shadow_offset = [12.0, 12.0];
    shadowed.style.shadow_blur = 4.0;
    let p_text = NodePaint {
        key: 24,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let glyph = text_paint_bounds(&shadowed);
    let prev_text = last_present_visual_slots(&[p_text], &[card, fill], &[], &[shadowed], &[])
        .into_iter()
        .find(|(k, _)| *k == 24)
        .map(|(_, b)| b)
        .expect("text slot");
    assert!(
            aabb_covers(prev_text, glyph),
            "last-present recorded with texts must cover text-shadow outset\n prev={prev_text:?} glyph={glyph:?}"
        );
    let (wipe, _) = node_present_dest_reset_scaled(
        24,
        box_,
        &[p_text],
        &[card, fill],
        &[],
        1.0,
        Some(prev_text),
    )
    .expect("card behind text-shadow");
    assert!(aabb_covers(wipe, glyph));
}

#[test]
fn last_present_omits_input_caret_overlay() {
    // I-beam is a swapchain overlay after retain blit. Last-present / dest-reset
    // of chrome punched Search into a hairline slice (竖条).
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let mut caret = blank_instance(box_[0] + box_[2] + 8.0, box_[1], 2.0, 16.0);
    caret.color = [0.1, 0.1, 0.1, 1.0];
    let p_on = NodePaint {
        key: 41,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 1),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let caret_box = [
        caret.rect_pos[0],
        caret.rect_pos[1],
        caret.rect_size[0],
        caret.rect_size[1],
    ];
    let chrome = vec![caret];
    let prev = last_present_visual_slots(&[p_on], &[card, fill], &[], &[], &chrome)
        .into_iter()
        .find(|(k, _)| *k == 41)
        .map(|(_, b)| b);
    if let Some(prev) = prev {
        assert!(
            !aabb_covers(prev, caret_box),
            "last-present must omit I-beam overlay\n prev={prev:?} caret={caret_box:?}"
        );
    }
    let (wipe_now, _) =
        node_present_dest_reset_full(41, box_, &[p_on], &[card, fill], &[], &chrome, 1.0, None)
            .map(|d| (d.rect, d.clear))
            .expect("now without prev");
    assert!(
            !aabb_covers(wipe_now, caret_box),
            "dest-reset of *now* must not treat chrome as dest-ink\n wipe={wipe_now:?} caret={caret_box:?}"
        );
    let ibeam_dirty = crate::app::damage::PaintDirty {
        key: 5,
        bounds: [40.0, 22.0, 2.0, 18.0],
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let leaf = slot(5, [12.0, 20.0, 300.0, 24.0], (0, 1));
    assert!(
        paint_dirty_is_caret_overlay(&ibeam_dirty, &[leaf], &[]),
        "I-beam dirty on a text leaf with no SDF is caret overlay"
    );
    assert!(!paint_dirty_is_caret_overlay(
        &ibeam_dirty,
        &[p_on],
        &[card, fill]
    ));
}

#[test]
fn retained_caret_off_is_not_overlay_and_wipes_the_stroke() {
    // OTP caret is a retained 1px box, not swapchain chrome. Blink OFF
    // (alpha 0) must dest-reset that stroke. Treating it as caret overlay
    // skips the wipe, so the I-beam stays solid.
    let mut cell = blank_instance(10.0, 10.0, 36.0, 36.0);
    cell.color = [0.95, 0.95, 0.95, 0.3];
    cell.border_width = 1.0;
    cell.border_color = [0.2, 0.2, 0.2, 1.0];
    let caret_box = [27.0, 18.0, 1.0, 16.0];
    let mut stroke = blank_instance(caret_box[0], caret_box[1], caret_box[2], caret_box[3]);
    stroke.color = [0.1, 0.1, 0.1, 0.0];
    let line = NodePaint {
        key: 9,
        bounds: caret_box,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let dirty = PaintDirty {
        key: 9,
        bounds: caret_box,
        kind: PaintDirtyKind::Node,
    };
    let instances = [cell, stroke];
    assert!(
        !paint_dirty_is_caret_overlay(&dirty, &[line], &instances),
        "opacity-0 retained caret must dest-reset, not skip as overlay chrome"
    );
    let reset = node_present_dest_reset_full(
        9,
        caret_box,
        &[line],
        &instances,
        &[],
        &[],
        1.0,
        Some(caret_box),
    )
    .expect("blink OFF must wipe the previous stroke");
    assert!(
        reset.rect[0] <= caret_box[0] + 0.5
            && reset.rect[1] <= caret_box[1] + 0.5
            && reset.rect[0] + reset.rect[2] >= caret_box[0] + caret_box[2] - 0.5
            && reset.rect[1] + reset.rect[3] >= caret_box[1] + caret_box[3] - 0.5,
        "wipe missed the stroke\n rect={:?} stroke={caret_box:?}",
        reset.rect
    );
    assert!(
        reset.rect[2] < 20.0 && reset.rect[3] < 28.0,
        "caret wipe grew into the slot\n rect={:?}",
        reset.rect
    );
}

#[test]
fn last_present_prev_for_dirty_skips_overlay_kinds() {
    // Overlay dest-reset scissors its hole; unioning a page last-present AABB
    // would punch the page behind the overlay.
    let page = [10.0, 20.0, 30.0, 40.0];
    let slots = [(50u64, page)];
    assert_eq!(
        last_present_prev_for_dirty(PaintDirtyKind::Node, 50, &slots),
        Some(page)
    );
    assert_eq!(
        last_present_prev_for_dirty(PaintDirtyKind::Node, 99, &slots),
        None
    );
    assert_eq!(
        last_present_prev_for_dirty(PaintDirtyKind::Overlay, 50, &slots),
        None
    );
    assert_eq!(
        last_present_prev_for_dirty(PaintDirtyKind::OverlayRestore, 50, &slots),
        None
    );
}

#[test]
fn named_scroll_blit_pins_union_clamped_prev() {
    let dirty = PaintDirty {
        key: 7,
        bounds: [100.0, 10.0, 40.0, 20.0],
        kind: PaintDirtyKind::Node,
    };
    let prev = [(7u64, [10.0, 10.0, 40.0, 20.0])];
    let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
    assert_eq!(pins.len(), 1);
    let p = pins[0];
    assert!(
        p[0] <= 2.0 && p[0] + p[2] >= 148.0,
        "scroll blit pin must dest-reset old hover ∪ new, got {p:?}"
    );
    let now_only = named_scroll_blit_pins(&[dirty], &[], &[]);
    assert!(
        now_only[0][2] < p[2] - 40.0,
        "without prev the pin must be the now box, got now={:?} with_prev={p:?}",
        now_only[0]
    );
}

#[test]
fn named_scroll_blit_pins_keep_hover_scale_leftover_past_caption() {
    // `hover:scale` last-present hangs into the caption below. Occupancy clip
    // (node-present) cuts that leftover so dest-reset cannot punch wrapping
    // text. Scroll blit copies those pixels in the overlap — the pin must
    // still dest-reset the scaled hole or hover sticks until blit stops.
    let rest = [20.0, 40.0, 80.0, 32.0];
    let scaled = [20.0, 32.0, 80.0, 48.0];
    let caption = [20.0, 72.0, 80.0, 24.0];
    let dirty = PaintDirty {
        key: 2,
        bounds: rest,
        kind: PaintDirtyKind::Node,
    };
    let chip_p = NodePaint {
        key: 2,
        bounds: rest,
        overlay: false,
        instance: (0, 1),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 200.0, 200.0]),
        layered: false,
        rest: Some(rest),
    };
    let cap_p = NodePaint {
        key: 3,
        bounds: caption,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 200.0, 200.0]),
        layered: false,
        rest: Some(caption),
    };
    let occupancy = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        rest,
        &[(2, scaled)],
        &[chip_p.clone(), cap_p.clone()],
        8.0,
    );
    assert!(
        occupancy[1] + occupancy[3] <= caption[1] + 1.0,
        "node-present occupancy must still spare the caption, got {occupancy:?}"
    );
    let pins = named_scroll_blit_pins(&[dirty], &[(2, scaled)], &[chip_p, cap_p]);
    assert_eq!(pins.len(), 1);
    let p = pins[0];
    assert!(
        p[1] <= scaled[1] + 1.0 && p[1] + p[3] >= scaled[1] + scaled[3] - 1.0,
        "scroll blit pin must dest-reset scaled leftover, got {p:?} scaled={scaled:?}"
    );
}

#[test]
fn named_scroll_blit_pins_overlay_skips_page_prev() {
    let dirty = PaintDirty {
        key: 7,
        bounds: [200.0, 200.0, 80.0, 40.0],
        kind: PaintDirtyKind::Overlay,
    };
    let prev = [(7u64, [0.0, 0.0, 800.0, 600.0])];
    let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
    assert_eq!(pins.len(), 1);
    let p = pins[0];
    assert!(
        p[2] < 200.0 && p[3] < 120.0,
        "overlay pin must not inherit a page last-present AABB, got {p:?}"
    );
}

#[test]
fn named_scroll_blit_pins_clamps_recycled_prev() {
    let dirty = PaintDirty {
        key: 1,
        bounds: [8.0, 80.0, 32.0, 16.0],
        kind: PaintDirtyKind::Node,
    };
    let prev = [(1u64, [0.0, 0.0, 800.0, 600.0])];
    let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
    assert_eq!(pins.len(), 1);
    let p = pins[0];
    assert!(
        p[2] * p[3] < 200.0 * 200.0,
        "recycled inset prev must clamp to the dirty neighborhood, got {p:?}"
    );
}

#[test]
fn last_present_visual_slots_omit_overlay_nodes() {
    let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
    card.color = [0.96, 0.96, 0.96, 1.0];
    let box_ = [20.0, 20.0, 40.0, 20.0];
    let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
    fill.color = [1.0, 0.0, 0.0, 1.0];
    let page = NodePaint {
        key: 60,
        bounds: box_,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let overlay = NodePaint {
        key: 61,
        overlay: true,
        instance: (0, 0),
        ..page
    };
    let slots = last_present_visual_slots(&[page, overlay], &[card, fill], &[], &[], &[]);
    assert!(
        slots.iter().any(|(k, _)| *k == 60),
        "page nodes stay in last-present"
    );
    assert!(
        slots.iter().all(|(k, _)| *k != 61),
        "overlay nodes must not enter last-present (dest-reset does not use overlay prev)"
    );
}

#[test]
fn layered_overlay_dest_reset_covers_overlapping_underlay() {
    // Jump pill hangs off the last bubble. In-flow dest-reset would shrink
    // the wipe away from that sibling; a stacking overlay must still erase
    // its own ink on the bubble.
    let mut card = blank_instance(0.0, 0.0, 400.0, 300.0);
    card.color = [0.96, 0.96, 0.97, 1.0];
    let bubble = [100.0, 200.0, 200.0, 60.0];
    let mut bubble_fill = blank_instance(bubble[0], bubble[1], bubble[2], bubble[3]);
    bubble_fill.color = [0.15, 0.15, 0.18, 1.0];
    bubble_fill.radii = [16.0; 4];
    let btn = [160.0, 240.0, 80.0, 36.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [0.1, 0.1, 0.12, 1.0];
    btn_fill.radii = [18.0; 4];
    btn_fill.shadow_color = [0.0, 0.0, 0.0, 0.2];
    btn_fill.shadow_blur = 8.0;
    btn_fill.shadow_offset = [0.0, 4.0];
    let bubble_p = NodePaint {
        key: 1,
        bounds: bubble,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let btn_p = NodePaint {
        key: 2,
        bounds: btn,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (1, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: true,
        rest: None,
    };
    let instances = [card, bubble_fill, btn_fill];
    let blob = instance_paint_bounds(&instances[2]);
    let (wipe, _) = node_present_dest_reset_scaled(
        2,
        btn,
        &[bubble_p, btn_p],
        &instances,
        &[],
        1.0,
        Some(blob),
    )
    .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
        aabb_covers(wipe, btn),
        "layered dest-reset must erase leftover overlay on the bubble\n wipe={wipe:?} btn={btn:?}"
    );
    assert!(
            aabb_covers(wipe, blob) || wipe[1] + wipe[3] >= btn[1] + btn[3] + 4.0,
            "layered dest-reset must wipe the outset halo or PartialLoad stacks it\n wipe={wipe:?} btn={btn:?} blob={blob:?}"
        );
    assert!(
        node_present_flush_layered_underlay_glyphs(2, btn, &[bubble_p, btn_p]),
        "dest-reset of Latest must recapture wrapping in the wipe"
    );
    let mut btn_hidden = btn_p;
    btn_hidden.instance = (3, 3);
    let (wipe, _) = node_present_dest_reset_scaled(
        2,
        btn,
        &[bubble_p, btn_hidden],
        &instances,
        &[],
        1.0,
        Some(blob),
    )
    .expect("hide dest-reset of jump");
    assert!(
        aabb_covers(wipe, btn),
        "layered dest-reset must erase the opaque pill on the bubble\n wipe={wipe:?} btn={btn:?}"
    );
    assert!(
            aabb_covers(wipe, blob),
            "layered dest-reset must wipe the outset halo or PartialLoad stacks it\n wipe={wipe:?} btn={btn:?} blob={blob:?}"
        );
    assert!(
        node_present_restore_underlay(2, btn, &[bubble_p, btn_p]),
        "layered slot must restore underlay in the dest-reset hole"
    );
    let mut hello = sample_text("hello", 170.0);
    hello.y = 245.0;
    hello.width = 48.0;
    hello.height = 16.0;
    let dirty = PaintDirty {
        key: 2,
        bounds: padded_xywh(btn, 8.0),
        kind: PaintDirtyKind::Node,
    };
    let page_text = [hello];
    let captured = node_present_glyphs_for_dirty(&dirty, &page_text, &[bubble_p, btn_p]);
    assert!(
            captured.is_empty(),
            "post-overlay capture is Latest only; underlay wrapping must not stamp the thread, got {captured:?}"
        );
    let underlay =
        node_present_layered_underlay_capture_text(&[dirty], &page_text, &[bubble_p, btn_p]);
    assert_eq!(
        underlay.len(),
        1,
        "layered dest-reset must restore underlay glyphs under the overlay slot, got {} runs",
        underlay.len()
    );
}

#[test]
fn layered_jump_transformed_shadow_must_not_expand_dest_reset() {
    // Gallery Latest: `0 4px 14px` + hide scale 0.95 / ancestor card matrix.
    // `instance_paint_bounds` of that halo is ~144×96. Dest-reset of it
    // punched wrapping thread and the overlay rail.
    let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let wrap = [16.0, 300.0, 280.0, 80.0];
    let mut wrap_fill = blank_instance(wrap[0], wrap[1], wrap[2], wrap[3]);
    wrap_fill.color = [0.96, 0.96, 0.97, 1.0];
    let btn = [120.0, 336.0, 80.0, 32.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 1.0];
    btn_fill.radii = [16.0; 4];
    btn_fill.shadow_color = [0.06, 0.09, 0.16, 0.18];
    btn_fill.shadow_blur = 14.0;
    btn_fill.shadow_offset = [0.0, 4.0];
    // Subpixel ancestor translate — not identity, used to take the shadow path.
    btn_fill.transform = [1.0, 0.0, 0.0, 1.0, 0.25, 0.0];
    let bar = [310.0, 0.0, 10.0, 120.0];
    let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    bar_fill.color = [0.1, 0.1, 0.1, 0.4];
    let wrap_p = NodePaint {
        key: 1,
        bounds: wrap,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: false,
        rest: Some(wrap),
    };
    let btn_p = NodePaint {
        key: 2,
        bounds: btn,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (1, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(btn),
    };
    let bar_p = NodePaint {
        key: 3,
        bounds: bar,
        overlay: false,
        instance: (3, 4),
        textured: (0, 0),
        text: (1, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(bar),
    };
    let instances = [card, wrap_fill, btn_fill, bar_fill];
    let halo = instance_paint_bounds(&instances[2]);
    let dirty = padded_xywh(btn, 32.0);
    let paints = [wrap_p, btn_p, bar_p];
    let reset =
        node_present_dest_reset_full(2, dirty, &paints, &instances, &[], &[], 1.0, Some(halo))
            .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
        aabb_covers(reset.rect, btn) || intersect_rects(reset.rect, btn).is_some(),
        "wipe must still cover the leftover pill\n wipe={:?} btn={btn:?}",
        reset.rect
    );
    assert!(
        intersect_rects(reset.rect, bar).is_none(),
        "dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
        reset.rect
    );
    assert!(
        node_present_flush_layered_underlay_glyphs(2, dirty, &paints),
        "dest-reset of Latest must recapture wrapping in the wipe"
    );
    let mut btn_hidden = btn_p;
    btn_hidden.instance = (3, 3);
    let hide_paints = [wrap_p, btn_hidden, bar_p];
    let reset = node_present_dest_reset_full(
        2,
        dirty,
        &hide_paints,
        &[card, wrap_fill, bar_fill],
        &[],
        &[],
        1.0,
        Some(halo),
    )
    .expect("hide dest-reset of Latest");
    let wrap_line = [16.0, 368.0, 280.0, 16.0];
    assert!(
        aabb_covers(reset.rect, btn) || intersect_rects(reset.rect, btn).is_some(),
        "wipe must still cover the opaque pill\n wipe={:?} btn={btn:?}",
        reset.rect
    );
    assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 8.0,
            "transformed shadow halo must dest-reset or PartialLoad stacks the fade\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
    assert!(
        intersect_rects(reset.rect, bar).is_none(),
        "dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
        reset.rect
    );
    let mut wrap_run = sample_text("wrapping", wrap_line[0] + 8.0);
    wrap_run.y = wrap_line[1];
    wrap_run.width = 80.0;
    wrap_run.height = wrap_line[3];
    let halo_dirty = PaintDirty {
        key: 2,
        bounds: dirty,
        kind: PaintDirtyKind::Node,
    };
    let underlay =
        node_present_layered_underlay_capture_text(&[halo_dirty], &[wrap_run], &hide_paints);
    assert!(
        !underlay.is_empty(),
        "halo dest-reset must restore wrapping glyphs under the overlay, got {} runs",
        underlay.len()
    );
    let last = last_present_visual_slots(&paints, &instances, &[], &[], &[]);
    let btn_last = last
        .iter()
        .find(|(k, _)| *k == 2)
        .map(|(_, b)| *b)
        .expect("button last-present");
    assert!(
        aabb_covers(btn_last, btn) || intersect_rects(btn_last, btn).is_some(),
        "layered last-present must keep the pill\n last={btn_last:?} btn={btn:?}"
    );
    assert!(
            aabb_covers(btn_last, halo) || btn_last[1] + btn_last[3] >= btn[1] + btn[3] + 8.0,
            "layered last-present must cover the SDF halo so hide-settle can erase it\n last={btn_last:?} halo={halo:?} btn={btn:?}"
        );
    let scissor =
        node_present_scissor_logical(PaintDirtyKind::Node, 2, dirty, &[(2, halo)], &paints, 8.0);
    assert!(
            scissor[2] < 200.0,
            "scissor must stay on the pill+halo, not the 100% dock\n scissor={scissor:?} dirty={dirty:?}"
        );
    assert!(
        intersect_rects(scissor, bar).is_none(),
        "scissor must not include the overlay scrollbar\n scissor={scissor:?} bar={bar:?}"
    );
}

#[test]
fn layered_stacking_context_wrapping_must_not_clip_jump_halo() {
    // MessageScroller `data-stacking-context` marks wrapping bubbles layered.
    // Occupancy shrink against those siblings left the SDF halo in retain
    // (stacked enter ring / hide ghost) and punched a pill-sized hole.
    let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let wrap = [16.0, 300.0, 280.0, 80.0];
    let mut wrap_fill = blank_instance(wrap[0], wrap[1], wrap[2], wrap[3]);
    wrap_fill.color = [0.96, 0.96, 0.97, 1.0];
    let btn = [120.0, 336.0, 80.0, 32.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 1.0];
    btn_fill.radii = [16.0; 4];
    btn_fill.shadow_color = [0.0, 0.0, 0.0, 0.05];
    btn_fill.shadow_blur = 2.0;
    btn_fill.shadow_offset = [0.0, 1.0];
    let bar = [308.0, 8.0, 8.0, 368.0];
    let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    bar_fill.color = [0.1, 0.1, 0.1, 0.4];
    let wrap_p = NodePaint {
        key: 1,
        bounds: wrap,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(wrap),
    };
    let btn_p = NodePaint {
        key: 2,
        bounds: btn,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (1, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(btn),
    };
    let bar_p = NodePaint {
        key: 3,
        bounds: bar,
        overlay: false,
        instance: (3, 4),
        textured: (0, 0),
        text: (1, 1),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(bar),
    };
    let instances = [card, wrap_fill, btn_fill, bar_fill];
    let halo = instance_paint_bounds(&instances[2]);
    let dirty = padded_xywh(btn, 16.0);
    let paints = [wrap_p, btn_p, bar_p];
    let reset =
        node_present_dest_reset_full(2, dirty, &paints, &instances, &[], &[], 1.0, Some(halo))
            .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 4.0,
            "stacking-context wrapping must not occupancy-clip the halo\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
    assert!(
        intersect_rects(reset.rect, bar).is_none(),
        "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
        reset.rect
    );
    let mut btn_hidden = btn_p;
    btn_hidden.instance = (3, 3);
    let hide_paints = [wrap_p, btn_hidden, bar_p];
    let reset = node_present_dest_reset_full(
        2,
        dirty,
        &hide_paints,
        &[card, wrap_fill, bar_fill],
        &[],
        &[],
        1.0,
        Some(halo),
    )
    .expect("hide dest-reset of Latest");
    assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 4.0,
            "stacking-context wrapping must not occupancy-clip the halo\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
    assert!(
        intersect_rects(reset.rect, bar).is_none(),
        "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
        reset.rect
    );
    let mut wrap_run = sample_text("wrapping", wrap[0] + 8.0);
    wrap_run.y = btn[1] + 8.0;
    wrap_run.width = 80.0;
    wrap_run.height = 16.0;
    let dirty_node = PaintDirty {
        key: 2,
        bounds: dirty,
        kind: PaintDirtyKind::Node,
    };
    let underlay =
        node_present_layered_underlay_capture_text(&[dirty_node], &[wrap_run], &hide_paints);
    assert!(
        !underlay.is_empty(),
        "halo dest-reset must recapture wrapping in the stacking-context band"
    );

    let bar_reset =
        node_present_dest_reset_full(3, bar, &paints, &instances, &[], &[], 1.0, Some(bar))
            .expect("overlay rail dest-reset");
    assert!(
        intersect_rects(bar_reset.rect, btn).is_none(),
        "scrollbar fade dest-reset must not punch Latest's pill\n wipe={:?} btn={btn:?}",
        bar_reset.rect
    );
}

#[test]
fn layered_dest_reset_must_not_punch_overlay_scrollbar() {
    // Full-width dock last-present / scissor used to dest-reset across the
    // overlay auto rail. Layered clip must keep the wipe on the pill.
    let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
    card.color = [1.0, 1.0, 1.0, 1.0];
    let btn = [120.0, 340.0, 80.0, 32.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [0.1, 0.1, 0.12, 1.0];
    btn_fill.radii = [16.0; 4];
    let bar = [308.0, 8.0, 8.0, 368.0];
    let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
    bar_fill.color = [0.7, 0.7, 0.72, 0.4];
    let dock = [0.0, 336.0, 320.0, 40.0];
    let btn_p = NodePaint {
        key: 2,
        bounds: btn,
        overlay: false,
        instance: (1, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(btn),
    };
    let bar_p = NodePaint {
        key: 3,
        bounds: bar,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
        layered: true,
        rest: Some(bar),
    };
    let instances = [card, btn_fill, bar_fill];
    let visible = node_present_dest_reset_full(
        2,
        dock,
        &[btn_p, bar_p],
        &instances,
        &[],
        &[],
        1.0,
        Some(dock),
    )
    .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
        aabb_covers(visible.rect, btn) || intersect_rects(visible.rect, btn).is_some(),
        "wipe must still cover the leftover pill\n wipe={:?} btn={btn:?}",
        visible.rect
    );
    assert!(
            visible.rect[2] <= btn[2] + 40.0,
            "full-width dock leftover must not expand Latest wipe into wrapping\n wipe={:?} btn={btn:?} dock={dock:?}",
            visible.rect
        );
    assert!(
        intersect_rects(visible.rect, bar).is_none(),
        "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
        visible.rect
    );
    let mut btn_hidden = btn_p;
    btn_hidden.instance = (3, 3);
    let wipe = node_present_dest_reset_full(
        2,
        dock,
        &[btn_hidden, bar_p],
        &[card, bar_fill],
        &[],
        &[],
        1.0,
        Some(dock),
    )
    .expect("hide dest-reset of Latest")
    .rect;
    assert!(
        aabb_covers(wipe, btn) || intersect_rects(wipe, btn).is_some(),
        "wipe must still cover the pill\n wipe={wipe:?} btn={btn:?}"
    );
    assert!(
            wipe[2] <= btn[2] + 40.0,
            "full-width dock dirty must not expand Latest wipe into wrapping\n wipe={wipe:?} btn={btn:?} dock={dock:?}"
        );
    assert!(
        intersect_rects(wipe, bar).is_none(),
        "Latest dest-reset must not punch the overlay scrollbar\n wipe={wipe:?} bar={bar:?}"
    );
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        2,
        dock,
        &[(2, dock)],
        &[btn_p, bar_p],
        8.0,
    );
    assert!(
            intersect_rects(scissor, bar).is_none(),
            "Latest node-present scissor must not include the overlay scrollbar\n scissor={scissor:?} bar={bar:?}"
        );
    let pins = named_scroll_blit_pins(
        &[PaintDirty {
            key: 2,
            bounds: dock,
            kind: PaintDirtyKind::Node,
        }],
        &[(2, dock)],
        &[btn_p, bar_p],
    );
    assert!(
        pins.iter().all(|p| intersect_rects(*p, bar).is_none()),
        "scroll-blit pin of Latest must stay off the overlay scrollbar, got {pins:?}"
    );
}

#[test]
fn layered_dest_reset_replays_rounded_underlay_from_skip_blob() {
    // Viewport fingerprint-replay records one blob covering card+bubble.
    // Dest-reset of the jump pill must still replay the bubble SDF, not
    // fill the AABB with the card behind that blob.
    let mut card = blank_instance(0.0, 0.0, 400.0, 300.0);
    card.color = [0.96, 0.96, 0.97, 1.0];
    let bubble = [100.0, 200.0, 200.0, 60.0];
    let mut bubble_fill = blank_instance(bubble[0], bubble[1], bubble[2], bubble[3]);
    bubble_fill.color = [0.15, 0.15, 0.18, 1.0];
    bubble_fill.radii = [16.0; 4];
    let btn = [160.0, 240.0, 80.0, 36.0];
    let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
    btn_fill.color = [1.0, 1.0, 1.0, 1.0];
    btn_fill.radii = [18.0; 4];
    let viewport = NodePaint {
        key: 10,
        bounds: [0.0, 0.0, 400.0, 300.0],
        overlay: false,
        instance: (0, 2),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let btn_p = NodePaint {
        key: 2,
        bounds: btn,
        overlay: false,
        instance: (2, 3),
        textured: (0, 0),
        text: (0, 0),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: true,
        rest: None,
    };
    let instances = [card, bubble_fill];
    let visible = node_present_dest_reset_full(
        2,
        btn,
        &[viewport, btn_p],
        &[card, bubble_fill, btn_fill],
        &[],
        &[],
        1.0,
        Some(btn),
    )
    .expect("leftover Latest dest-resets so the fade cannot ghost");
    assert!(
        visible.replay.0 <= 1 && visible.replay.1 > 1,
        "must replay the rounded bubble under leftover dest-reset, got {:?}",
        visible.replay
    );
    let mut btn_hidden = btn_p;
    btn_hidden.instance = (2, 2);
    let reset = node_present_dest_reset_full(
        2,
        btn,
        &[viewport, btn_hidden],
        &instances,
        &[],
        &[],
        1.0,
        Some(btn),
    )
    .expect("hide dest-reset of layered pill");
    assert!(
        reset.replay.0 <= 1 && reset.replay.1 > 1,
        "must replay the rounded bubble, not empty owner-chrome, got {:?}",
        reset.replay
    );
    assert!(
        reset.clear[0] > 0.9 && reset.clear[1] > 0.9,
        "wipe behind the bubble is the card, got {:?}",
        reset.clear
    );
}

#[test]
fn layered_region_restores_peer_glyphs_after_drag_moves_away() {
    let peer = sample_text("A", 64.0);
    let dragged = sample_text("B", 364.0);
    let peer_p = slot(1, [0.0, 0.0, 176.0, 92.0], (0, 1));
    let mut dragged_p = slot(2, [300.0, 0.0, 176.0, 92.0], (1, 2));
    dragged_p.layered = true;
    let paints = [peer_p, dragged_p];
    let text = [peer, dragged];
    for bounds in [[0.0, 0.0, 176.0, 92.0], [0.0, 0.0, 400.0, 92.0]] {
        let dirty = PaintDirty {
            key: 2,
            bounds,
            kind: PaintDirtyKind::Region,
        };
        let captured = node_present_glyphs_for_dirty(&dirty, &text, &paints);
        assert!(captured.contains(&0), "the vacated pose must restore A");
        assert_eq!(captured.contains(&1), bounds[2] > 300.0);
    }
}

#[test]
fn translated_layer_occupancy_follows_the_visible_plate() {
    let bounds = [100.0, 100.0, 176.0, 92.0];
    let mut host = slot(1, bounds, (0, 1));
    host.layered = true;
    host.rest = Some(bounds);
    let mut peer = slot(2, [300.0, 250.0, 176.0, 92.0], (1, 2));
    peer.layered = true;
    peer.rest = Some([180.0, 140.0, 176.0, 92.0]);
    peer.instance = (1, 2);
    assert_eq!(
        clip_rect_against_layered_siblings(bounds, 1, bounds, &[host, peer], 1.0),
        bounds,
        "a moved plate must not block repaint at its old layout position"
    );
    let lower = [320.0, 270.0, 176.0, 92.0];
    host.bounds = lower;
    host.rest = Some(lower);
    assert_ne!(
        clip_rect_against_layered_siblings(lower, 1, lower, &[host, peer], 1.0),
        lower,
        "the moved plate must occupy its current position"
    );
}

#[test]
fn separated_layered_region_clears_old_pose_raster_fringe() {
    let old = [20.0, 100.0, 176.0, 92.0];
    let now = [300.0, 100.0, 176.0, 92.0];
    let mut paint = slot(2, now, (0, 0));
    paint.layered = true;
    paint.instance = (0, 1);
    let mut fill = blank_instance(now[0], now[1], now[2], now[3]);
    fill.color = [1.0; 4];
    fill.radii = [12.0; 4];
    for scale in [1.0, 2.0] {
        let mut dirties = vec![PaintDirty {
            key: 2,
            bounds: now,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirties,
            &[paint],
            &[(2, old)],
            &[fill],
            &[],
            &[],
            scale,
            None,
        );
        let fringe = padded_xywh(old, 2.0 / scale);
        assert!(
            dirties.iter().any(|dirty| {
                dirty.kind == PaintDirtyKind::Region && aabb_covers(dirty.bounds, fringe)
            }),
            "the separated old outline must be fully cleared: {dirties:?}"
        );
    }
}

#[test]
fn in_flow_dest_reset_still_skips_neighbor_captions() {
    let mut nav = sample_text("NavLayout", 8.0);
    nav.width = 40.0;
    nav.height = 16.0;
    let mut typed = sample_text("asdasd", 80.0);
    typed.width = 40.0;
    typed.height = 16.0;
    let input_p = NodePaint {
        key: 2,
        bounds: [72.0, 0.0, 120.0, 32.0],
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (1, 2),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let dirty = PaintDirty {
        key: 2,
        bounds: [72.0, 0.0, 120.0, 32.0],
        kind: PaintDirtyKind::Node,
    };
    let captured = node_present_glyphs_for_dirty(&dirty, &[nav, typed], &[input_p]);
    assert!(
        captured.contains(&1) && !captured.contains(&0),
        "in-flow Node capture stays membership-only, got {captured:?}"
    );
    assert!(!node_present_restore_underlay(2, dirty.bounds, &[input_p]));
}

#[test]
fn recycled_key_must_not_capture_neighbor_text_from_broad_parent_slot() {
    // A tracked rebuild can reuse the parent's NodeId for a smaller trigger.
    // The parent NodePaint range still contains both captions; membership
    // alone would recapture the unchanged sibling and src-over it in retain.
    let trigger = sample_text("打开 Trigger", 8.0);
    let sibling = sample_text("静态说明", 160.0);
    let parent = NodePaint {
        key: 42,
        bounds: [0.0, 0.0, 240.0, 32.0],
        overlay: false,
        instance: (0, 0),
        textured: (0, 0),
        text: (0, 2),
        chrome: (0, 0),
        paint_ops: (0, 0),
        overflow_clip: None,
        layered: false,
        rest: None,
    };
    let dirty = PaintDirty {
        key: 42,
        bounds: [0.0, 0.0, 80.0, 32.0],
        kind: PaintDirtyKind::Node,
    };

    let captured = node_present_glyphs_for_dirty(&dirty, &[trigger, sibling], &[parent]);

    assert_eq!(captured, BTreeSet::from([0]));
}
