use super::*;
use crate::core::reactivity::create_root;
use crate::primitives::r#box::vstack;

#[test]
fn focus_move_restores_scrolled_slot_ring_and_neighbor() {
    create_root(|| {
        use crate::app::damage::{take_frame_paint_nodes, PaintDirtyKind};
        let value = Signal::new("12".to_owned());
        let active = Signal::new(0usize);
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set("otp-ring-test".into());
        let mut slot = Element::new();
        slot.layout_bounds = [100.0, 700.0, 36.0, 36.0];
        slot.set_paint_scroll_offset([0.0, 600.0]);
        sync_otp_slot_paint(
            &mut slot,
            0,
            6,
            value,
            active,
            "otp-ring-test",
            false,
            false,
        );
        let _ = take_frame_paint_nodes();
        active.set(1);
        sync_otp_slot_paint(
            &mut slot,
            0,
            6,
            value,
            active,
            "otp-ring-test",
            false,
            false,
        );
        let dirty = take_frame_paint_nodes();
        assert!(
            dirty.iter().any(|d| d.kind == PaintDirtyKind::Node
                && d.bounds[0] <= 96.0
                && d.bounds[1] <= 96.0
                && d.bounds[0] + d.bounds[2] >= 140.0
                && d.bounds[1] + d.bounds[3] >= 140.0),
            "leaving ring must dest-reset the SDF outset, not a clear strip: {dirty:?}"
        );
        assert!(
            dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
            "focus-ring damage must not window-clear into a gray frame: {dirty:?}"
        );
    });
}

#[test]
fn grouped_focus_repaint_stays_inside_the_active_slot() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-group-hole").separator_at(3).into();
        let page = Element::new()
            .w(400.0)
            .h(200.0)
            .child(Element::new().w(400.0).h(48.0))
            .child(otp);
        let mut root = Element::new()
            .w(420.0)
            .h(80.0)
            .overflow_y("scroll")
            .child(page);
        root.focus_id = Some("scroll:otp-hole".into());
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 420.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        let _ = tree.build_and_layout_fast(&mut root, 420.0, 80.0, [-1.0; 2], true, true);
        let mut slots = Vec::new();
        fn walk(el: &Element, out: &mut Vec<(usize, [f32; 4])>) {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot") {
                let index = el
                    .attrs
                    .get("data-index")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                out.push((index, el.layout_bounds));
            }
            for c in &el.children {
                walk(c, out);
            }
        }
        walk(&root, &mut slots);
        slots.sort_by_key(|(i, _)| *i);
        assert!(slots.len() >= 6, "grouped otp slots: {slots:?}");
        let active = slots[0].1;
        clear_frame_paint_nodes();
        // Scroll changes after the last collect. The focus hole must stay the
        // slot, not the union of the stale document box and the live visual box.
        root.scroll_offset = [0.0, 40.0];
        use_signal(
            &crate::core::scroll_viewport::scroll_offset_y_signal_key("otp-hole"),
            || 0.0_f32,
        )
        .set(40.0);
        focus.set(input_otp_focus_id("otp-group-hole"));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            420.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let dirty = take_frame_paint_nodes();
        let holes: Vec<_> = dirty.iter().map(|d| d.bounds).collect();
        let vis = |b: [f32; 4]| [b[0], b[1] - 40.0, b[2], b[3]];
        let covers = |hole: [f32; 4], slot: [f32; 4]| {
            let cx = slot[0] + slot[2] * 0.5;
            let cy = slot[1] + slot[3] * 0.5;
            cx >= hole[0] && cy >= hole[1] && cx <= hole[0] + hole[2] && cy <= hole[1] + hole[3]
        };
        assert!(
            dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
            "focus must dest-reset the ring, not window-clear a region: {dirty:?}"
        );
        assert!(
            holes
                .iter()
                .all(|b| b[2] <= active[2] + 24.0 && b[3] <= active[3] + 24.0),
            "focus hole bigger than the active slot {active:?}: holes={holes:?} slots={slots:?}"
        );
        assert!(
            holes.iter().any(|hole| covers(*hole, vis(active))),
            "focus hole misses the visual active slot {:?}: holes={holes:?}",
            vis(active)
        );
        for (index, bounds) in slots.iter().skip(1) {
            assert!(
                holes.iter().all(|hole| !covers(*hole, vis(*bounds))),
                "focus hole covers slot {index} {bounds:?}: holes={holes:?}"
            );
        }
        let frame = tree.build_and_layout_fast(&mut root, 420.0, 80.0, [-1.0; 2], false, true);
        let mut after = Vec::new();
        walk(&root, &mut after);
        after.sort_by_key(|(i, _)| *i);
        let covers_slot = |inst: &crate::renderer::InstanceInput, slot: [f32; 4]| {
            let x = inst.rect_pos[0];
            let y = inst.rect_pos[1];
            let w = inst.rect_size[0];
            let h = inst.rect_size[1];
            (x - slot[0]).abs() <= 2.0
                && (y - slot[1]).abs() <= 2.0
                && (w - slot[2]).abs() <= 4.0
                && (h - slot[3]).abs() <= 4.0
                && inst.border_width > 0.5
        };
        for (index, bounds) in &after {
            let want = vis(*bounds);
            let on_screen = want[0] < 420.0
                && want[1] < 80.0
                && want[0] + want[2] > 0.0
                && want[1] + want[3] > 0.0;
            if !on_screen {
                continue;
            }
            assert!(
                frame.instances.iter().any(|inst| covers_slot(inst, want)),
                "slot {index} lost its border quad after focus: visual={want:?}"
            );
        }
        let fat: Vec<_> = frame
            .instances
            .iter()
            .filter(|inst| {
                let x = inst.rect_pos[0];
                let y = inst.rect_pos[1];
                let w = inst.rect_size[0];
                let h = inst.rect_size[1];
                let slot = vis(active);
                let overlaps = x < slot[0] + slot[2] + 8.0
                    && y < slot[1] + slot[3] + 8.0
                    && x + w > slot[0] - 8.0
                    && y + h > slot[1] - 8.0;
                overlaps && (w > active[2] + 16.0 || h > active[3] + 16.0)
            })
            .map(|inst| {
                (
                    inst.rect_pos,
                    inst.rect_size,
                    inst.color,
                    inst.shadow_blur,
                    inst.shadow_spread,
                )
            })
            .collect();
        assert!(
            fat.is_empty(),
            "focused slot painted a quad outside one cell: {fat:?} active={active:?}"
        );
        let slot_now = after[0].1;
        assert!(
            (slot_now[0] - active[0]).abs() < 1.0 && (slot_now[1] - active[1]).abs() < 1.0,
            "active slot moved when the caret was inserted: before={active:?} after={slot_now:?}"
        );
        let mut abs_boxes = Vec::new();
        fn walk_abs(el: &Element, in_slot: bool, out: &mut Vec<[f32; 4]>) {
            let in_slot =
                in_slot || el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot");
            if in_slot && el.style.position == taffy::style::Position::Absolute {
                out.push(el.layout_bounds);
            }
            for c in &el.children {
                walk_abs(c, in_slot, out);
            }
        }
        walk_abs(&root, false, &mut abs_boxes);
        for b in &abs_boxes {
            let inside = b[0] >= slot_now[0] - 1.0
                && b[1] >= slot_now[1] - 1.0
                && b[0] + b[2] <= slot_now[0] + slot_now[2] + 1.0
                && b[1] + b[3] <= slot_now[1] + slot_now[3] + 1.0;
            assert!(
                inside,
                "caret left the active slot: caret={b:?} slot={slot_now:?}"
            );
        }
        let scissor = holes.iter().copied().fold([0.0; 4], |acc, r| {
            if acc[2] <= 0.0 {
                r
            } else {
                crate::app::damage::union_rects(acc, r)
            }
        });
        let border = frame
            .instances
            .iter()
            .find(|inst| covers_slot(inst, vis(slot_now)))
            .expect("active border");
        assert!(
                crate::app::draw_cull::instance_hits_damage(border, scissor),
                "clear hole misses the active slot, so the slot stays erased\n scissor={scissor:?} slot={slot_now:?} inst=({:?}, {:?})",
                border.rect_pos,
                border.rect_size
            );
        let idx = frame
            .instances
            .iter()
            .position(|inst| covers_slot(inst, vis(slot_now)))
            .unwrap();
        let bands = frame.overlay_split.page_stack_band_ranges();
        let in_page_band = bands
            .iter()
            .any(|b| idx >= b.instance.0 && idx < b.instance.1)
            || (bands.is_empty() && idx < frame.overlay_split.instance_start);
        let in_paint_op = bands.iter().any(|b| {
            frame.paint_ops[b.paint_ops.0..b.paint_ops.1]
                .iter()
                .any(|op| match op {
                    crate::layout::layout_tree::PaintOp::Rects { start, count } => {
                        idx >= *start as usize && idx < (*start as usize + *count as usize)
                    }
                    crate::layout::layout_tree::PaintOp::Textured { .. } => false,
                })
        });
        assert!(
                in_page_band && (in_paint_op || bands.iter().all(|b| b.paint_ops.0 == b.paint_ops.1)),
                "active slot quad is outside page-band replay\n idx={idx} in_band={in_page_band} in_op={in_paint_op} overlay_start={} bands={:?}",
                frame.overlay_split.instance_start,
                bands
                    .iter()
                    .map(|b| (b.instance, b.paint_ops))
                    .collect::<Vec<_>>()
            );
        let owner = frame
            .node_paints
            .iter()
            .filter(|p| idx >= p.instance.0 && idx < p.instance.1)
            .min_by_key(|p| p.instance.1 - p.instance.0)
            .expect("border owner");
        let (chrome_lo, chrome_hi) =
            crate::app::draw_cull::slot_own_chrome_range(owner, &frame.instances, 1.0);
        assert!(
                idx >= chrome_lo && idx < chrome_hi,
                "focused slot border is outside its own-chrome replay\n idx={idx} chrome={chrome_lo}..{chrome_hi} owner_bounds={:?} owner_inst={:?} layered={} range={:?}",
                owner.bounds,
                owner.instance,
                owner.layered,
                frame.instances[owner.instance.0..owner.instance.1]
                    .iter()
                    .map(|i| (i.rect_pos, i.rect_size, i.border_width))
                    .collect::<Vec<_>>()
            );
        let scaled: Vec<crate::renderer::InstanceInput> = frame
            .instances
            .iter()
            .copied()
            .map(|mut inst| {
                inst.rect_pos[0] *= 1.5;
                inst.rect_pos[1] *= 1.5;
                inst.rect_size[0] *= 1.5;
                inst.rect_size[1] *= 1.5;
                inst
            })
            .collect();
        let (dpi_lo, dpi_hi) = crate::app::draw_cull::slot_own_chrome_range(owner, &scaled, 1.5);
        assert!(
                idx >= dpi_lo && idx < dpi_hi,
                "150% scale drops the focused slot border from own-chrome\n idx={idx} chrome={dpi_lo}..{dpi_hi} bounds={:?} inst=({:?}, {:?})",
                owner.bounds,
                border.rect_pos,
                border.rect_size
            );
        let restored = dirty
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::Node)
            .any(|d| {
                frame
                    .node_paints
                    .iter()
                    .filter(|p| p.key == d.key)
                    .any(|p| {
                        let (lo, hi) =
                            crate::app::draw_cull::slot_own_chrome_range(p, &frame.instances, 1.0);
                        (lo..hi).any(|i| covers_slot(&frame.instances[i], vis(slot_now)))
                    })
            });
        assert!(
            restored,
            "focus dest-reset does not redraw the focused slot\n dirty={dirty:?}"
        );
        let slot_vis = vis(slot_now);
        assert!(
            border.clip_size[0] < 0.0
                || (border.clip_pos[0] <= slot_vis[0] + 1.0
                    && border.clip_pos[1] <= slot_vis[1] + 1.0
                    && border.clip_pos[0] + border.clip_size[0] >= slot_vis[0] + slot_vis[2] - 1.0
                    && border.clip_pos[1] + border.clip_size[1] >= slot_vis[1] + slot_vis[3] - 1.0),
            "focused slot border is clipped off its own box\n clip=({:?}, {:?}) slot={slot_vis:?}",
            border.clip_pos,
            border.clip_size
        );
    });
}

/// Gallery RSX OTP: focus must dest-reset the active slot's visual box, not
/// the whole `input-otp` row. A row-wide clear looks like a gray focus frame
/// around every cell (截图「聚焦是整行」).
#[test]
fn rsx_focus_wipe_stays_on_the_active_slot_visual_not_the_row() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        // Gallery RSX shape: two groups of three + separator (builder path).
        let otp: Element = input_otp("gal-otp-rsx-focus")
            .max_length(6)
            .separator_at(3)
            .into();
        let mut root = Element::new().w(520.0).h(120.0).overflow_y("scroll").child(
            Element::new()
                .w("100%")
                .p(16.0)
                .bg([0.95, 0.95, 0.95, 1.0])
                .child(otp),
        );
        root.focus_id = Some("scroll:gal-otp-rsx-focus".into());
        root.scroll_offset = [0.0, 0.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 520.0, 120.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        let idle = tree.build_and_layout_fast(&mut root, 520.0, 120.0, [-1.0; 2], true, true);
        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &idle.node_paints,
            &idle.instances,
            &[],
            &idle.text_nodes,
            &idle.input_chrome_instances,
        );
        clear_frame_paint_nodes();
        focus.set(input_otp_focus_id("gal-otp-rsx-focus"));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            520.0,
            120.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut dirty = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 520.0, 120.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut dirty,
            &frame.node_paints,
            &last_present,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let slots = slot_boxes(&root);
        assert!(slots.len() >= 6, "rsx otp slots: {slots:?}");
        let active = slots[0].1;
        let row_w: f32 = {
            let left = slots
                .iter()
                .map(|(_, b)| b[0])
                .fold(f32::INFINITY, f32::min);
            let right = slots
                .iter()
                .map(|(_, b)| b[0] + b[2])
                .fold(f32::NEG_INFINITY, f32::max);
            right - left
        };
        assert!(
            row_w > active[2] * 2.5,
            "fixture row must be wider than one slot: row={row_w} slot={}",
            active[2]
        );
        assert!(
            dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
            "focus must not window-clear a Region around the row: {dirty:?}"
        );
        let rings: Vec<_> = frame
            .instances
            .iter()
            .filter(|i| i.shadow_spread >= 1.0 && i.shadow_color[3] > 0.2)
            .map(|i| [i.rect_pos[0], i.rect_pos[1], i.rect_size[0], i.rect_size[1]])
            .collect();
        assert_eq!(
            rings.len(),
            1,
            "focus must paint one Hard ring on the active slot, not the row: {rings:?}"
        );
        assert!(
            rings[0][2] <= active[2] + 4.0 && rings[0][3] <= active[3] + 4.0,
            "Hard ring quad is the whole row, not the active slot\n ring={:?} active={active:?}",
            rings[0]
        );
        for d in &dirty {
            if d.kind == PaintDirtyKind::Overlay {
                continue;
            }
            assert!(
                    d.bounds[2] <= active[2] + 24.0 && d.bounds[3] <= active[3] + 24.0,
                    "focus dirty spans the row, not the active visual slot\n dirty={d:?} active={active:?} row_w={row_w}"
                );
            let scissor = crate::app::draw_cull::node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                &last_present,
                &frame.node_paints,
                8.0,
            );
            if scissor[2] <= 0.5 || scissor[3] <= 0.5 {
                continue;
            }
            assert!(
                    scissor[2] <= active[2] + 32.0 && scissor[3] <= active[3] + 32.0,
                    "focus scissor is the whole row\n scissor={scissor:?} active={active:?} dirty={d:?}"
                );
            let prev =
                crate::app::draw_cull::last_present_prev_for_dirty(d.kind, d.key, &last_present);
            if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                d.key,
                d.bounds,
                &frame.node_paints,
                &frame.instances,
                &[],
                &[],
                1.0,
                prev,
            ) {
                let wipe = [reset.rect[0], reset.rect[1], reset.rect[2], reset.rect[3]];
                assert!(
                        wipe[2] <= active[2] + 24.0 && wipe[3] <= active[3] + 24.0,
                        "focus dest-reset wipe is the whole row\n wipe={wipe:?} active={active:?} dirty={d:?}"
                    );
                for (index, bounds) in slots.iter().skip(1) {
                    assert!(
                        !center_in(wipe, *bounds),
                        "focus wipe clears slot {index} (整行)\n wipe={wipe:?} slot={bounds:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn focus_inside_page_scroll_dirties_the_slot_not_the_viewport() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-page-hole").into();
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .overflow_y("hidden")
            .child(Element::new().w(240.0).h(32.0))
            .child(otp);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
        clear_frame_paint_nodes();
        focus.set(input_otp_focus_id("otp-page-hole"));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            800.0,
            600.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let dirty = take_frame_paint_nodes();
        assert!(
            !collect.page_layout_changed,
            "slot caret must not FullClear the page"
        );
        assert!(
            dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
            "focus inside a page scroll must not window-clear a region: {dirty:?}"
        );
        assert!(
            dirty
                .iter()
                .any(|d| d.kind == PaintDirtyKind::Node && d.bounds[2] > 0.0 && d.bounds[3] > 0.0),
            "active slot still needs a local dest-reset: {dirty:?}"
        );
        assert!(
            dirty
                .iter()
                .all(|d| d.bounds[2] <= 80.0 && d.bounds[3] <= 80.0),
            "OTP focus inside a page scroll recorded the ancestor clip, not the slot: {dirty:?}"
        );
    });
}

#[test]
fn typed_digit_is_collected_after_structure_sync() {
    create_root(|| {
        let value = use_signal("otp-collect-value", String::new);
        let otp: Element = input_otp("otp-collect").value_signal(value).into();
        let mut root = Element::new()
            .w(400.0)
            .h(100.0)
            .overflow_y("hidden")
            .child(otp);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 400.0, 100.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        tree.build_and_layout_fast(&mut root, 400.0, 100.0, [-1.0; 2], true, true);
        value.set("1".into());
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            400.0,
            100.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let dirty = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &dirty);
        let frame = tree.build_and_layout_fast(&mut root, 400.0, 100.0, [-1.0; 2], false, true);
        assert!(
            frame
                .text_nodes
                .iter()
                .any(|t| t.text == "1" && t.width > 0.0),
            "new OTP glyph must be laid out and collected on its input frame"
        );
    });
}

fn slot_boxes(root: &Element) -> Vec<(usize, [f32; 4])> {
    let mut slots = Vec::new();
    fn walk(el: &Element, out: &mut Vec<(usize, [f32; 4])>) {
        if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot") {
            let index = el
                .attrs
                .get("data-index")
                .and_then(|s| s.parse().ok())
                .unwrap_or(usize::MAX);
            let vis = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_offset());
            let b = if vis[2] > 0.5 && vis[3] > 0.5 {
                vis
            } else {
                el.layout_bounds
            };
            out.push((index, b));
        }
        for c in &el.children {
            walk(c, out);
        }
    }
    walk(root, &mut slots);
    slots.sort_by_key(|(i, _)| *i);
    slots
}

fn center_in(hole: [f32; 4], slot: [f32; 4]) -> bool {
    let cx = slot[0] + slot[2] * 0.5;
    let cy = slot[1] + slot[3] * 0.5;
    cx >= hole[0] && cy >= hole[1] && cx <= hole[0] + hole[2] && cy <= hole[1] + hole[3]
}

fn outset_box(b: [f32; 4], pad: f32) -> [f32; 4] {
    [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0]
}

/// Painted SDF extent of the focus ring currently on `el` (spread/blur/offset + border + vertex AA).
fn painted_chrome_outset(el: &Element) -> f32 {
    crate::authoring::widgets::shadows::sdf_outset_shadow_pad(
        el.style.shadow_blur,
        el.style.shadow_spread,
        el.style.shadow_offset,
        el.style.border_width,
    )
}

fn find_slot<'a>(root: &'a Element, index: usize) -> Option<&'a Element> {
    fn walk(el: &Element, index: usize) -> Option<&Element> {
        if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot")
            && el.attrs.get("data-index").and_then(|s| s.parse().ok()) == Some(index)
        {
            return Some(el);
        }
        for c in &el.children {
            if let Some(hit) = walk(c, index) {
                return Some(hit);
            }
        }
        None
    }
    walk(root, index)
}

#[test]
fn caret_blink_dirties_the_ibeam_not_the_slot() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
        OTP_CARET_FORCE.with(|c| c.set(Some(true)));
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(input_otp_focus_id("otp-blink"));
        let otp: Element = input_otp("otp-blink").into();
        // Gallery forms page: the OTP sits in a scroll/overflow clip. Blink must
        // not become that clip's island clear.
        let mut root = Element::new()
            .w(420.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 420.0, 80.0, [-1.0; 2]);
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            420.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let slots = slot_boxes(&root);
        let active = slots[0].1;
        clear_frame_paint_nodes();
        OTP_CARET_FORCE.with(|c| c.set(Some(false)));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            420.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        OTP_CARET_FORCE.with(|c| c.set(None));
        assert!(
            !collect.page_layout_changed,
            "caret blink must not FullClear the page"
        );
        let dirty = take_frame_paint_nodes();
        assert!(
            !dirty.is_empty(),
            "hiding the caret must dirty the i-beam so the stroke leaves the retain buffer"
        );
        for d in &dirty {
            assert!(
                    d.bounds[2] <= 12.0 && d.bounds[3] <= 28.0,
                    "caret blink cleared the slot (seams / focus ring), not the i-beam\n dirty={d:?} slot={active:?}"
                );
            assert!(
                    d.bounds[0] >= active[0] + 4.0
                        && d.bounds[0] + d.bounds[2] <= active[0] + active[2] - 4.0
                        && d.bounds[1] >= active[1] - 0.5
                        && d.bounds[1] + d.bounds[3] <= active[1] + active[3] + 0.5,
                    "caret blink hole is not inside the cell, so it clips the ring or the shared border\n dirty={d:?} slot={active:?}"
                );
        }
    });
}

#[test]
fn active_move_dirties_both_ring_halos_not_a_bystander_slot() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(input_otp_focus_id("otp-ring-move"));
        let value = use_signal("otp-ring-move__value", || "123".to_owned());
        let otp: Element = input_otp("otp-ring-move").value_signal(value).into();
        let mut root = Element::new().w(480.0).h(80.0).child(otp);
        let active = use_signal(&otp_active_key("otp-ring-move"), || 0usize);
        active.set(2);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let leaving = find_slot(&root, 2).expect("slot 2");
        let ring_pad = painted_chrome_outset(leaving);
        assert!(
            ring_pad > 4.0,
            "focused OTP slot must be wearing the outset ring, pad={ring_pad}"
        );
        let boxes = slot_boxes(&root);
        clear_frame_paint_nodes();
        active.set(3);
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let dirty = take_frame_paint_nodes();
        let holes: Vec<[f32; 4]> = dirty.iter().map(|d| d.bounds).collect();
        let old_ring = outset_box(boxes[2].1, ring_pad);
        let entered = find_slot(&root, 3).expect("slot 3");
        let new_pad = painted_chrome_outset(entered);
        let new_ring = outset_box(boxes[3].1, new_pad.max(ring_pad));
        let corners = |b: [f32; 4]| {
            [
                [b[0] + 0.5, b[1] + 0.5],
                [b[0] + b[2] - 0.5, b[1] + 0.5],
                [b[0] + 0.5, b[1] + b[3] - 0.5],
                [b[0] + b[2] - 0.5, b[1] + b[3] - 0.5],
            ]
        };
        let covers_pt = |h: [f32; 4], p: [f32; 2]| {
            p[0] >= h[0] && p[1] >= h[1] && p[0] <= h[0] + h[2] && p[1] <= h[1] + h[3]
        };
        assert!(
                corners(old_ring)
                    .iter()
                    .all(|p| holes.iter().any(|h| covers_pt(*h, *p))),
                "leaving slot's focus ring is outside the dirty hole (residue)\n ring={old_ring:?} holes={holes:?}"
            );
        assert!(
                corners(new_ring)
                    .iter()
                    .all(|p| holes.iter().any(|h| covers_pt(*h, *p))),
                "entering slot's focus ring is outside the dirty hole\n ring={new_ring:?} holes={holes:?}"
            );
        assert!(
            holes
                .iter()
                .all(|h| h[3] <= boxes[2].1[3] + ring_pad * 2.0 + 4.0),
            "active move cleared a column, not the ring\n holes={holes:?}"
        );
        for index in [0usize, 1, 4, 5] {
            assert!(
                holes.iter().all(|h| !center_in(*h, boxes[index].1)),
                "active move dirtied bystander slot {index} {:?}: holes={holes:?}",
                boxes[index].1
            );
        }
    });
}

#[test]
fn pattern_accepts_digits() {
    assert_eq!(InputOtpPattern::Digits.accept('5'), Some('5'));
    assert_eq!(InputOtpPattern::Digits.accept('a'), None);
    assert_eq!(InputOtpPattern::Alphanumeric.accept('a'), Some('A'));
}

#[test]
fn insert_chars_advances() {
    let (v, i) = insert_chars("", 6, 0, "123".chars(), InputOtpPattern::Digits);
    assert_eq!(v, "123");
    assert_eq!(i, 3);
}

#[test]
fn dual_surface_and_slots() {
    create_root(|| {
        let built: Element = input_otp("t-otp-b").max_length(4).separator_at(2).into();
        assert_eq!(
            built.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
        assert_eq!(built.focus_id.as_deref(), Some("input-otp:t-otp-b"));

        let rsx = InputOTP(InputOTPProps {
            id: "t-otp-t".into(),
            max_length: Some(6),
            ..Default::default()
        });
        assert_eq!(
            rsx.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
    });
}

#[test]
fn builder_slot_style() {
    create_root(|| {
        let el: Element = input_otp("t-otp-chrome")
            .max_length(1)
            .child(|| {
                input_otp_group()
                    .child(input_otp_slot().index(0).style(|b| b.text_size(20.0)))
                    .into()
            })
            .into();
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "otp slot .style(|b| b.text_size(20)) must land on the slot tree"
        );
    });
}

#[test]
fn slot_radii_use_css_tl_tr_br_bl_order() {
    // [TL, TR, BR, BL]
    assert_eq!(
        otp_slot_radii(EDGE_START),
        [SLOT_EDGE_RADIUS, 0.0, 0.0, SLOT_EDGE_RADIUS]
    );
    assert_eq!(
        otp_slot_radii(EDGE_END),
        [0.0, SLOT_EDGE_RADIUS, SLOT_EDGE_RADIUS, 0.0]
    );
    assert_eq!(otp_slot_radii(EDGE_MIDDLE), [0.0; 4]);
}

#[test]
fn group_patches_outer_radii_and_nudge() {
    create_root(|| {
        let el: Element = input_otp("t-otp-edge").max_length(3).into();
        // root → group → slots
        let group = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP))
            .expect("group");
        assert_eq!(group.children.len(), 3);
        let start = &group.children[0];
        let mid = &group.children[1];
        let end = &group.children[2];
        assert_eq!(
            start.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
            Some(EDGE_START)
        );
        assert_eq!(
            mid.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
            Some(EDGE_MIDDLE)
        );
        assert_eq!(
            end.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
            Some(EDGE_END)
        );
        assert_eq!(start.style.radii, otp_slot_radii(EDGE_START));
        assert_eq!(mid.style.radii, otp_slot_radii(EDGE_MIDDLE));
        assert_eq!(end.style.radii, otp_slot_radii(EDGE_END));
        assert!(start
            .attrs
            .get(crate::components::button_group::ATTR_PAINT_NUDGE)
            .is_none());
        assert_eq!(
            mid.attrs
                .get(crate::components::button_group::ATTR_PAINT_NUDGE)
                .map(String::as_str),
            Some("x")
        );
        assert_eq!(
            end.attrs
                .get(crate::components::button_group::ATTR_PAINT_NUDGE)
                .map(String::as_str),
            Some("x")
        );
    });
}

#[test]
fn focus_prefix() {
    assert_eq!(input_otp_focus_id("x"), "input-otp:x");
}

/// Backspace moves focus onto the previous slot and dirties the slot that
/// just blurred, which sits on the new ring's right. That leave-wipe must
/// not clear the focused ring's right outset.
#[test]
fn delete_must_not_clip_the_focused_slot_right_ring() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        OTP_CARET_FORCE.with(|c| c.set(Some(true)));
        let value = use_signal("otp-delete-ring-value", String::new);
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-delete-ring")
            .value_signal(value)
            .separator_at(3)
            .into();
        let mut root = Element::new()
            .w(480.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        root.style.bg_color = [1.0, 1.0, 1.0, 1.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        focus.set(input_otp_focus_id("otp-delete-ring"));
        value.set("12".into());
        use_signal(&otp_active_key("otp-delete-ring"), || 0usize).set(2usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let _ = take_frame_paint_nodes();
        let before = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &before.node_paints,
            &before.instances,
            &[],
            &before.text_nodes,
            &before.input_chrome_instances,
        );

        value.set("1".into());
        use_signal(&otp_active_key("otp-delete-ring"), || 0usize).set(1usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut prepared = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &frame.node_paints,
            &last_present,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        OTP_CARET_FORCE.with(|c| c.set(None));
        let slots = slot_boxes(&root);
        let slot = |index: usize| {
            slots
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, b)| *b)
                .unwrap_or_else(|| panic!("slot {index}: {slots:?}"))
        };
        let active = slot(1);
        let ring = frame
            .instances
            .iter()
            .find(|inst| inst.shadow_spread >= 2.0)
            .expect("ring");
        let paint = crate::app::draw_cull::instance_paint_bounds(ring);
        let active_right = active[0] + active[2];
        assert!(
                paint[0] + paint[2] >= active_right + 2.0,
                "ring quad has no right shadow past the focused slot\n paint={paint:?} active={active:?}"
            );
        let wipe_of = |slot: [f32; 4]| -> Option<[f32; 4]> {
            prepared.iter().find_map(|d| {
                if d.kind != PaintDirtyKind::Node || !center_in(d.bounds, slot) {
                    return None;
                }
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                )
                .map(|reset| reset.rect)
            })
        };
        let active_wipe = wipe_of(active).expect("focused slot dest-reset");
        assert!(
                active_wipe[0] + active_wipe[2] >= paint[0] + paint[2] - 0.5,
                "focused slot dest-reset clips its own right ring\n wipe={active_wipe:?} paint={paint:?}"
            );
        let hx = active_right + 1.0;
        let hy = active[1] + active[3] * 0.5;
        let foreign: Vec<_> = prepared
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::Node && !center_in(d.bounds, active))
            .filter_map(|d| {
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                let rect = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                )
                .map(|reset| reset.rect)?;
                let inside = hx >= rect[0]
                    && hy >= rect[1]
                    && hx < rect[0] + rect[2]
                    && hy < rect[1] + rect[3];
                inside.then_some((d.bounds, rect))
            })
            .collect();
        assert!(
                foreign.is_empty(),
                "a neighbor wipe clears the focused ring's right edge\n halo=({hx},{hy}) paint={paint:?} foreign={foreign:?}\n prepared={prepared:?}"
            );
    });
}

/// After "111" the caret sits on the first slot of the next group. That
/// ring's right edge overlaps the following cell, and each cell just left
/// still has its right-edge ring in the framebuffer. The dest-reset wipe
/// has to cover both fringes; clipping them off is the missing right
/// shadow and the gray line left on every typed cell.
#[test]
fn typed_group_keeps_the_ring_right_edge_and_clears_the_old_one() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        let value = use_signal("otp-right-edge-value", String::new);
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-right-edge")
            .value_signal(value)
            .separator_at(3)
            .into();
        let mut root = Element::new()
            .w(480.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        focus.set(input_otp_focus_id("otp-right-edge"));
        value.set("11".into());
        use_signal(&otp_active_key("otp-right-edge"), || 0usize).set(2usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let _ = take_frame_paint_nodes();
        let focused = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &focused.node_paints,
            &focused.instances,
            &[],
            &focused.text_nodes,
            &focused.input_chrome_instances,
        );

        value.set("111".into());
        use_signal(&otp_active_key("otp-right-edge"), || 0usize).set(3usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut prepared = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &frame.node_paints,
            &last_present,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let slots = slot_boxes(&root);
        let slot = |index: usize| {
            slots
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, b)| *b)
                .unwrap_or_else(|| panic!("slot {index}: {slots:?}"))
        };
        let left = slot(2);
        let active = slot(3);
        let ring = frame
            .instances
            .iter()
            .find(|inst| inst.shadow_spread >= 2.0)
            .expect("ring");
        let paint = crate::app::draw_cull::instance_paint_bounds(ring);
        assert!(
                paint[0] + paint[2] >= active[0] + active[2] + 2.0,
                "ring quad has no right shadow past the active slot\n paint={paint:?} active={active:?}"
            );
        let wipe_covering = |slot: [f32; 4]| -> Option<[f32; 4]> {
            prepared.iter().find_map(|d| {
                if d.kind != PaintDirtyKind::Node || !center_in(d.bounds, slot) {
                    return None;
                }
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                )
                .map(|reset| reset.rect)
            })
        };
        let active_wipe = wipe_covering(active).expect("active slot dest-reset");
        assert!(
            active_wipe[0] + active_wipe[2] >= paint[0] + paint[2] - 0.5,
            "dest-reset clips the ring's right shadow\n wipe={active_wipe:?} paint={paint:?}"
        );
        assert!(
            crate::app::draw_cull::node_present_restore_underlay(
                prepared
                    .iter()
                    .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                    .expect("active dirty")
                    .key,
                prepared
                    .iter()
                    .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                    .expect("active dirty")
                    .bounds,
                &frame.node_paints,
            ),
            "clearing the right shadow must redraw the next slot's border under the ring"
        );
        let leave_wipe = wipe_covering(left).expect("leaving slot dest-reset");
        assert!(
                leave_wipe[0] + leave_wipe[2] >= left[0] + left[2] + 2.0,
                "leaving slot does not erase the ring left on its right edge\n wipe={leave_wipe:?} slot={left:?}"
            );
    });
}

/// Empty first slot (no paint-nudge, no left neighbor) with the caret up.
/// Gallery pixels: halo ~190 (two 50% composites; one is ~215) and the
/// `bg-input/30` fill ~242 instead of ~248. A later caret blink must not
/// src-over that quad again.
#[test]
fn empty_first_slot_ring_is_one_composite_and_blink_does_not_restack_it() {
    create_root(|| {
        use crate::app::damage::{
            clear_frame_paint_nodes, paint_dirty_has_keyed_slot, paint_dirty_matches_present_slot,
            take_frame_paint_nodes,
        };
        use crate::layout::layout_tree::PaintOp;
        OTP_CARET_FORCE.with(|c| c.set(Some(true)));
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-empty-ring").separator_at(3).into();
        let mut root = Element::new()
            .w(480.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        root.style.bg_color = [1.0, 1.0, 1.0, 1.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        focus.set(input_otp_focus_id("otp-empty-ring"));
        use_signal(&otp_active_key("otp-empty-ring"), || 0usize).set(0usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut prepared = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &frame.node_paints,
            &[],
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let ring_idxs: Vec<usize> = frame
            .instances
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.shadow_spread >= 2.0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(ring_idxs.len(), 1, "empty slot collected {ring_idxs:?}");
        let ring_idx = ring_idxs[0];
        let draws = |dirty: &crate::app::damage::PaintDirty,
                     paints: &[crate::layout::layout_tree::NodePaint],
                     instances: &[crate::renderer::pipeline::InstanceInput],
                     ops: &[PaintOp],
                     prev: Option<[f32; 4]>,
                     idx: usize|
         -> (usize, String) {
            let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                paints,
                instances,
                &[],
                &[],
                1.0,
                prev,
            ) else {
                let has_keyed = paint_dirty_has_keyed_slot(
                    dirty.key,
                    dirty.bounds,
                    paints.iter().map(|p| (p.key, p.bounds)),
                );
                let mut n = 0usize;
                for p in paints {
                    if p.overlay
                        || !paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                    {
                        continue;
                    }
                    n += crate::app::draw_cull::slot_paint_ops(ops, p.instance, (0, 0))
                        .filter(|op| match op {
                            PaintOp::Rects { start, count } => {
                                let i = idx as u32;
                                i >= *start && i < *start + *count
                            }
                            PaintOp::Textured { .. } => false,
                        })
                        .count();
                }
                return (n, format!("no-reset ops={n}"));
            };
            let has_keyed = paint_dirty_has_keyed_slot(
                dirty.key,
                dirty.bounds,
                paints.iter().map(|p| (p.key, p.bounds)),
            );
            let hit = |range: (usize, usize)| {
                crate::app::draw_cull::slot_paint_ops(ops, range, (0, 0)).any(|op| match op {
                    PaintOp::Rects { start, count } => {
                        let i = idx as u32;
                        i >= start && i < start + count
                    }
                    PaintOp::Textured { .. } => false,
                })
            };
            let mut why = String::new();
            let mut n = usize::from(idx >= reset.replay.0 && idx < reset.replay.1);
            if n > 0 {
                why.push_str(&format!("replay={:?} ", reset.replay));
            }
            if crate::app::draw_cull::node_present_restore_underlay(dirty.key, dirty.bounds, paints)
            {
                for p in paints {
                    if p.overlay
                        || paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                        || !crate::app::draw_cull::node_present_underlay_paint(
                            dirty.key,
                            dirty.bounds,
                            p,
                            reset.rect,
                            paints,
                            instances,
                        )
                    {
                        continue;
                    }
                    if hit(p.instance) {
                        n += 1;
                        why.push_str(&format!("underlay key={} inst={:?} ", p.key, p.instance));
                    }
                }
            }
            for p in paints {
                if !p.overlay
                    && paint_dirty_matches_present_slot(
                        dirty.key,
                        dirty.bounds,
                        p.key,
                        p.bounds,
                        has_keyed,
                    )
                    && hit(p.instance)
                {
                    n += 1;
                    why.push_str(&format!("own key={} inst={:?} ", p.key, p.instance));
                }
            }
            (n, why)
        };
        let hits: Vec<_> = prepared
            .iter()
            .filter_map(|d| {
                let (n, why) = draws(
                    d,
                    &frame.node_paints,
                    &frame.instances,
                    &frame.paint_ops,
                    None,
                    ring_idx,
                );
                (n > 0).then_some((d.kind, d.bounds, n, why))
            })
            .collect();
        let total: usize = hits.iter().map(|(_, _, n, _)| n).sum();
        assert_eq!(
            total,
            1,
            "empty first slot src-overs the ring\n draws={total} hits={hits:?} caret_kids={}",
            find_slot(&root, 0).map(|s| s.children.len()).unwrap_or(0)
        );

        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &frame.node_paints,
            &frame.instances,
            &[],
            &frame.text_nodes,
            &frame.input_chrome_instances,
        );
        clear_frame_paint_nodes();
        OTP_CARET_FORCE.with(|c| c.set(Some(false)));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut blink_prepared = take_frame_paint_nodes();
        let blinked = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut blink_prepared,
            &blinked.node_paints,
            &last_present,
            &blinked.instances,
            &[],
            &[],
            1.0,
            None,
        );
        OTP_CARET_FORCE.with(|c| c.set(None));
        let slot_box = find_slot(&root, 0).map(|s| s.layout_bounds).expect("slot");
        assert!(
            !blink_prepared.is_empty(),
            "hiding the caret must dirty the stroke"
        );
        for d in &blink_prepared {
            assert!(
                d.bounds[0] >= slot_box[0] + 4.0
                    && d.bounds[0] + d.bounds[2] <= slot_box[0] + slot_box[2] - 4.0
                    && d.bounds[1] >= slot_box[1] - 0.5
                    && d.bounds[1] + d.bounds[3] <= slot_box[1] + slot_box[3] + 0.5,
                "caret blink hole reaches the focus ring\n dirty={d:?} slot={slot_box:?}"
            );
            if d.kind == crate::app::damage::PaintDirtyKind::Region {
                assert!(
                    !crate::app::draw_cull::region_replays_own_chrome_only(
                        d.key,
                        d.bounds,
                        &blinked.node_paints,
                    ),
                    "caret blink src-overs the slot fill\n dirty={d:?}"
                );
            } else {
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                let (n, why) = draws(
                    d,
                    &blinked.node_paints,
                    &blinked.instances,
                    &blinked.paint_ops,
                    prev,
                    blinked
                        .instances
                        .iter()
                        .position(|inst| inst.shadow_spread >= 2.0)
                        .expect("ring"),
                );
                assert_eq!(
                    n, 0,
                    "caret blink src-overs the focus ring onto retain\n dirty={d:?} {why}"
                );
            }
        }
    });
}

/// Slot fill is `bg-input/30`. Src-over of that quad twice darkens the cell
/// and the focus ring. Blur must clear the ring and paint the translucent
/// fill once, or the darkened cell stays as a gray patch.
#[test]
fn focus_ring_is_painted_once_and_blur_clears_it() {
    create_root(|| {
        use crate::app::damage::{
            clear_frame_paint_nodes, paint_dirty_has_keyed_slot, paint_dirty_matches_present_slot,
            take_frame_paint_nodes, PaintDirtyKind,
        };
        use crate::layout::layout_tree::PaintOp;
        let value = use_signal("otp-darken-value", String::new);
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-darken")
            .value_signal(value)
            .separator_at(3)
            .into();
        let mut root = Element::new()
            .w(480.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        focus.set(input_otp_focus_id("otp-darken"));
        value.set("111111".into());
        use_signal(&otp_active_key("otp-darken"), || 0usize).set(5usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut prepared = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &frame.node_paints,
            &[],
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let slots = slot_boxes(&root);
        let active = slots
            .iter()
            .find(|(i, _)| *i == 5)
            .map(|(_, b)| *b)
            .expect("slot 5");
        let ring_idxs: Vec<usize> = frame
            .instances
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.shadow_spread >= 2.0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            ring_idxs.len(),
            1,
            "focus collected {} ring quads at {:?}",
            ring_idxs.len(),
            ring_idxs
                .iter()
                .map(|i| frame.instances[*i].rect_pos)
                .collect::<Vec<_>>()
        );
        let ring_idx = ring_idxs[0];
        let ring_paint = crate::app::draw_cull::instance_paint_bounds(&frame.instances[ring_idx]);
        let draws_index = |dirty: &crate::app::damage::PaintDirty,
                           paints: &[crate::layout::layout_tree::NodePaint],
                           instances: &[crate::renderer::pipeline::InstanceInput],
                           ops: &[PaintOp],
                           idx: usize|
         -> usize {
            let prev = None;
            let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                paints,
                instances,
                &[],
                &[],
                1.0,
                prev,
            ) else {
                // No CLEAR. The slot quad is still submitted and src-overs
                // whatever the previous pass left, including its own ring.
                let has_keyed = paint_dirty_has_keyed_slot(
                    dirty.key,
                    dirty.bounds,
                    paints.iter().map(|p| (p.key, p.bounds)),
                );
                let mut n = 0;
                if dirty.kind == PaintDirtyKind::Region {
                    let b = crate::app::draw_cull::instance_paint_bounds(&instances[idx]);
                    if crate::app::damage::intersect_rects(dirty.bounds, b).is_some() {
                        n += 1;
                    }
                }
                for p in paints {
                    if p.overlay
                        || !paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                    {
                        continue;
                    }
                    n += crate::app::draw_cull::slot_paint_ops(ops, p.instance, (0, 0))
                        .filter(|op| match op {
                            PaintOp::Rects { start, count } => {
                                let i = idx as u32;
                                i >= *start && i < *start + *count
                            }
                            PaintOp::Textured { .. } => false,
                        })
                        .count();
                }
                return n;
            };
            let has_keyed = paint_dirty_has_keyed_slot(
                dirty.key,
                dirty.bounds,
                paints.iter().map(|p| (p.key, p.bounds)),
            );
            let op_count = |range: (usize, usize)| -> usize {
                crate::app::draw_cull::slot_paint_ops(ops, range, (0, 0))
                    .filter(|op| match op {
                        PaintOp::Rects { start, count } => {
                            let i = idx as u32;
                            i >= *start && i < *start + *count
                        }
                        PaintOp::Textured { .. } => false,
                    })
                    .count()
            };
            let mut n = 0;
            // `draw_sdf_range` submits replay indices directly, then the slot
            // paint op submits the same quad again.
            if idx >= reset.replay.0 && idx < reset.replay.1 {
                n += 1;
            }
            if crate::app::draw_cull::node_present_restore_underlay(dirty.key, dirty.bounds, paints)
            {
                for p in paints {
                    if p.overlay
                        || paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                        || !crate::app::draw_cull::node_present_underlay_paint(
                            dirty.key,
                            dirty.bounds,
                            p,
                            reset.rect,
                            paints,
                            instances,
                        )
                    {
                        continue;
                    }
                    if op_count(p.instance) > 0 {
                        n += op_count(p.instance);
                    }
                }
            }
            for p in paints {
                if p.overlay
                    || !paint_dirty_matches_present_slot(
                        dirty.key,
                        dirty.bounds,
                        p.key,
                        p.bounds,
                        has_keyed,
                    )
                {
                    continue;
                }
                n += op_count(p.instance);
            }
            n
        };
        let focus_hits: Vec<_> = prepared
            .iter()
            .filter_map(|d| {
                let n = draws_index(
                    d,
                    &frame.node_paints,
                    &frame.instances,
                    &frame.paint_ops,
                    ring_idx,
                );
                let reset = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    None,
                )
                .map(|r| (r.rect, r.replay));
                (n > 0).then_some((d.kind, d.bounds, n, reset))
            })
            .collect();
        let focus_draws: usize = focus_hits.iter().map(|(_, _, n, _)| *n).sum();
        assert_eq!(
                focus_draws, 1,
                "focus src-overs the translucent slot and the ring\n draws={focus_draws} hits={focus_hits:?} ring={ring_paint:?}"
            );
        let focus_dirty = prepared
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
            .expect("focused slot dirty");
        let focus_wipe = crate::app::draw_cull::node_present_dest_reset_full(
            focus_dirty.key,
            focus_dirty.bounds,
            &frame.node_paints,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        )
        .expect("focus dest-reset")
        .rect;
        let focus_covers = focus_wipe[0] <= ring_paint[0] + 0.5
            && focus_wipe[1] <= ring_paint[1] + 0.5
            && focus_wipe[0] + focus_wipe[2] >= ring_paint[0] + ring_paint[2] - 0.5
            && focus_wipe[1] + focus_wipe[3] >= ring_paint[1] + ring_paint[3] - 0.5;
        assert!(
            focus_covers,
            "focus clear misses the ring halo\n wipe={focus_wipe:?} ring={ring_paint:?}"
        );

        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &frame.node_paints,
            &frame.instances,
            &[],
            &frame.text_nodes,
            &frame.input_chrome_instances,
        );
        focus.set(String::new());
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let mut prepared = take_frame_paint_nodes();
        let blurred = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &blurred.node_paints,
            &last_present,
            &blurred.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let slots = slot_boxes(&root);
        let active = slots
            .iter()
            .find(|(i, _)| *i == 5)
            .map(|(_, b)| *b)
            .expect("slot 5 after blur");
        let blur_dirty = prepared
            .iter()
            .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
            .expect("blur dirty");
        let fill_idx = blurred
            .instances
            .iter()
            .position(|inst| {
                let b = [
                    inst.rect_pos[0],
                    inst.rect_pos[1],
                    inst.rect_size[0],
                    inst.rect_size[1],
                ];
                center_in(b, active) && inst.color[3] > 0.05 && inst.color[3] < 0.9
            })
            .expect("translucent slot fill");
        let blur_draws = draws_index(
            blur_dirty,
            &blurred.node_paints,
            &blurred.instances,
            &blurred.paint_ops,
            fill_idx,
        );
        assert_eq!(
            blur_draws, 1,
            "blur src-overs the translucent fill; the cell stays gray\n draws={blur_draws}"
        );
        let prev = crate::app::draw_cull::last_present_prev_for_dirty(
            blur_dirty.kind,
            blur_dirty.key,
            &last_present,
        );
        let wipe = crate::app::draw_cull::node_present_dest_reset_full(
            blur_dirty.key,
            blur_dirty.bounds,
            &blurred.node_paints,
            &blurred.instances,
            &[],
            &[],
            1.0,
            prev,
        )
        .expect("blur dest-reset")
        .rect;
        let covers = wipe[0] <= ring_paint[0] + 0.5
            && wipe[1] <= ring_paint[1] + 0.5
            && wipe[0] + wipe[2] >= ring_paint[0] + ring_paint[2] - 0.5
            && wipe[1] + wipe[3] >= ring_paint[1] + ring_paint[3] - 0.5;
        assert!(
            covers,
            "blur leaves the focus ring\n wipe={wipe:?} ring={ring_paint:?}"
        );
    });
}

/// Typing moves the ring. The redraw of that focus must dest-reset the
/// slot's own SDF. A layout-box Region window-clears and page-band-replays,
/// which paints the ring stroke off the active cell.
#[test]
fn typed_digit_focus_redraw_is_the_slot_ring_not_a_page_band_clear() {
    create_root(|| {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
        let value = use_signal("otp-type-ring-value", String::new);
        let focus = use_signal("__jadix_button_focus_id", String::new);
        focus.set(String::new());
        let otp: Element = input_otp("otp-type-ring")
            .value_signal(value)
            .separator_at(3)
            .into();
        let mut root = Element::new()
            .w(480.0)
            .h(80.0)
            .overflow_y("hidden")
            .child(otp);
        // Page fill so dest-reset has a backdrop. Without it the halo Node
        // is promoted to a window-clear Region.
        root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        clear_frame_paint_nodes();
        focus.set(input_otp_focus_id("otp-type-ring"));
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let _ = take_frame_paint_nodes();
        let focused = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        let last_present = crate::app::draw_cull::last_present_visual_slots(
            &focused.node_paints,
            &focused.instances,
            &[],
            &focused.text_nodes,
            &focused.input_chrome_instances,
        );

        value.set("4".into());
        use_signal(&otp_active_key("otp-type-ring"), || 0usize).set(1usize);
        clear_frame_paint_nodes();
        let mut collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut root,
            &mut tree,
            &mut LayoutStatus::Clean,
            480.0,
            80.0,
            [0.0; 2],
            false,
            &mut collect,
        );
        let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
        tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
        let dirty = take_frame_paint_nodes();
        let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
        let slots = slot_boxes(&root);
        let active = slots
            .iter()
            .find(|(i, _)| *i == 1)
            .map(|(_, b)| *b)
            .expect("active slot");
        let rings: Vec<_> = frame
            .instances
            .iter()
            .filter(|inst| inst.shadow_spread >= 2.0)
            .map(|inst| {
                (
                    inst.rect_pos,
                    inst.rect_size,
                    inst.shadow_spread,
                    inst.border_color,
                )
            })
            .collect();
        assert_eq!(
            rings.len(),
            1,
            "focus ring SDF must exist only on the active slot\n rings={rings:?}\n slots={slots:?}"
        );
        let (pos, size, _, _) = rings[0];
        assert!(
            (pos[0] - active[0]).abs() <= 2.0
                && (pos[1] - active[1]).abs() <= 2.0
                && (size[0] - active[2]).abs() <= 4.0
                && (size[1] - active[3]).abs() <= 4.0,
            "ring quad is not the active slot\n quad=({pos:?}, {size:?}) active={active:?}"
        );
        let mut prepared = dirty.clone();
        crate::app::draw_cull::prepare_node_present_dirties(
            &mut prepared,
            &frame.node_paints,
            &last_present,
            &frame.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let page_band: Vec<_> = prepared
            .iter()
            .filter(|d| {
                d.kind == PaintDirtyKind::Region
                    && !crate::app::draw_cull::region_replays_own_chrome_only(
                        d.key,
                        d.bounds,
                        &frame.node_paints,
                    )
            })
            .map(|d| d.bounds)
            .collect();
        assert!(
                page_band.is_empty(),
                "typing window-clears and page-band-replays the focus ring off the cell\n holes={page_band:?}\n slots={slots:?}\n dirty={dirty:?}"
            );
        let ring = frame
            .instances
            .iter()
            .find(|inst| inst.shadow_spread >= 2.0)
            .expect("ring");
        let paint = crate::app::draw_cull::instance_paint_bounds(ring);
        assert!(
                prepared.iter().any(|d| {
                    d.kind == PaintDirtyKind::Node
                        && d.bounds[0] <= paint[0] + 0.5
                        && d.bounds[1] <= paint[1] + 0.5
                        && d.bounds[0] + d.bounds[2] >= paint[0] + paint[2] - 0.5
                        && d.bounds[1] + d.bounds[3] >= paint[1] + paint[3] - 0.5
                }),
                "focus redraw does not dest-reset the ring SDF\n paint={paint:?}\n prepared={prepared:?}"
            );
        for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
            assert!(
                prepared.iter().all(|d| !center_in(d.bounds, *bounds)),
                "focus redraw covers slot {index} {bounds:?}: prepared={prepared:?}"
            );
        }
        // Screenshot after typing "1": the caret stays in slot 1, but a full
        // ring-colored outline is painted on slot 2 and later joined borders
        // are left blank. The ring quad (VS translation included) has to
        // contain the caret, and the active slot must not open a page-stack
        // band that also owns later groups — that composite is what leaves
        // the stroke off the cell.
        let caret = frame
            .instances
            .iter()
            .find(|inst| inst.rect_size[0] <= 2.0 && inst.rect_size[1] >= 8.0)
            .expect("caret line");
        let caret_c = [
            caret.rect_pos[0] + caret.rect_size[0] * 0.5 + caret.transform[4],
            caret.rect_pos[1] + caret.rect_size[1] * 0.5 + caret.transform[5],
        ];
        let ring_vis = [
            pos[0] + ring.transform[4],
            pos[1] + ring.transform[5],
            size[0],
            size[1],
        ];
        assert!(
                caret_c[0] >= ring_vis[0]
                    && caret_c[0] <= ring_vis[0] + ring_vis[2]
                    && caret_c[1] >= ring_vis[1]
                    && caret_c[1] <= ring_vis[1] + ring_vis[3],
                "focus ring is not on the caret's slot\n caret={caret_c:?} ring={ring_vis:?} xf={:?}\n slots={slots:?}",
                ring.transform
            );
        let ring_idx = frame
            .instances
            .iter()
            .position(|inst| inst.shadow_spread >= 2.0)
            .expect("ring index");
        let bands = frame.overlay_split.page_stack_band_ranges();
        for band in &bands {
            if ring_idx < band.instance.0 || ring_idx >= band.instance.1 {
                continue;
            }
            for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
                let swallowed = frame.instances[band.instance.0..band.instance.1]
                    .iter()
                    .any(|inst| {
                        center_in(
                            [
                                inst.rect_pos[0],
                                inst.rect_pos[1],
                                inst.rect_size[0],
                                inst.rect_size[1],
                            ],
                            *bounds,
                        )
                    });
                assert!(
                        !swallowed,
                        "active slot's page band also owns slot {index}; replay paints the ring off the cell\n band={:?} slots={slots:?}",
                        band.instance
                    );
            }
        }
        let ring_owner = frame
            .node_paints
            .iter()
            .filter(|p| ring_idx >= p.instance.0 && ring_idx < p.instance.1)
            .min_by_key(|p| p.instance.1 - p.instance.0)
            .expect("ring owner");
        assert!(
                !ring_owner.layered,
                "active otp slot is a page-stack layer, so dest-reset clears joined borders\n owner_bounds={:?} owner_inst={:?}",
                ring_owner.bounds,
                ring_owner.instance
            );
        for d in &prepared {
            if d.kind != PaintDirtyKind::Node {
                continue;
            }
            let prev =
                crate::app::draw_cull::last_present_prev_for_dirty(d.kind, d.key, &last_present);
            let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                d.key,
                d.bounds,
                &frame.node_paints,
                &frame.instances,
                &[],
                &[],
                1.0,
                prev,
            ) else {
                continue;
            };
            let wipe = [reset.rect[0], reset.rect[1], reset.rect[2], reset.rect[3]];
            assert!(
                    wipe[2] <= active[2] + 24.0 && wipe[3] <= active[3] + 24.0,
                    "focus dest-reset wipe is wider than the slot ring\n wipe={wipe:?} active={active:?} dirty={d:?}"
                );
            for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
                assert!(
                    !center_in(wipe, *bounds),
                    "focus dest-reset clears slot {index}\n wipe={wipe:?} slot={bounds:?}"
                );
            }
        }
    });
}

/// Gallery nests OTP in a column (`VStack` / `align-items: stretch`). With
/// `w("auto")` the focusable root fills the cross axis — a full-width AABB
/// while the slots only occupy the left. Anything that paints or dirties
/// that box (a11y focus rect, accidental root ring, row-wide wipe) looks
/// like「聚焦是整行」instead of the active slot's visual range.
#[test]
fn focusable_root_hugs_slot_row_not_parent_cross_axis() {
    create_root(|| {
        let otp: Element = input_otp("otp-hug-row")
            .max_length(6)
            .separator_at(3)
            .into();
        let mut root = vstack().w(480.0).p(16.0).child(otp).into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 480.0, 120.0, [-1.0; 2]);
        fn find_root(el: &Element) -> Option<&Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ROOT) {
                return Some(el);
            }
            el.children.iter().find_map(find_root)
        }
        let otp_root = find_root(&root).expect("otp root");
        let slots = slot_boxes(&root);
        assert!(slots.len() >= 6, "slots={slots:?}");
        let left = slots
            .iter()
            .map(|(_, b)| b[0])
            .fold(f32::INFINITY, f32::min);
        let right = slots
            .iter()
            .map(|(_, b)| b[0] + b[2])
            .fold(f32::NEG_INFINITY, f32::max);
        let top = slots
            .iter()
            .map(|(_, b)| b[1])
            .fold(f32::INFINITY, f32::min);
        let bottom = slots
            .iter()
            .map(|(_, b)| b[1] + b[3])
            .fold(f32::NEG_INFINITY, f32::max);
        let visual = [left, top, right - left, bottom - top];
        let shell = otp_root.layout_bounds;
        assert!(
                shell[2] <= visual[2] + 8.0,
                "focusable OTP root stretches to the column width (整行), not the slot row\n shell={shell:?} visual_row={visual:?} parent_w=480"
            );
        assert!(
                shell[3] <= visual[3] + 8.0,
                "focusable OTP root is taller than the slot row\n shell={shell:?} visual_row={visual:?}"
            );
        assert!(
                (shell[0] - visual[0]).abs() <= 2.0 && (shell[1] - visual[1]).abs() <= 2.0,
                "focusable OTP root origin drifts from the slot row\n shell={shell:?} visual_row={visual:?}"
            );
    });
}
