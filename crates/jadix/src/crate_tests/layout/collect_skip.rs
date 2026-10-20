//! Paint-only collect can replay a fingerprint display list for clean sibling subtrees
//! (GPUI `Entity::cached` analog — skip collect, not omit pixels).

use crate::app::frame_stats::{last_skipped_siblings, reset_collect_skip_counters};
use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::input_adornment::input_adornment;
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::core::tree::{sync_reactive_elements_subtree, Element, LayoutStatus, LayoutTree};
use crate::layout::collect_cache::collect_skip_allowed_for_frame;
use crate::layout::layout_tree::{ATTR_PORTAL_KEY, PORTAL_LAYER_ID};
use crate::primitives::r#box::{hstack, vstack};
use crate::renderer::font_loader;
use crate::style::sync_class_variants_subtree;
use crate::text::install_shared_font_system;

#[test]
fn transparent_canvas_defers_paint_until_visible() {
    use std::cell::Cell;
    use std::rc::Rc;
    let paints = Rc::new(Cell::new(0));
    let count = paints.clone();
    let canvas: Element = crate::primitives::canvas::canvas()
        .w(64.0)
        .h(32.0)
        .paint(move |ctx, _| {
            count.set(count.get() + 1);
            ctx.clear("red");
            LayoutStatus::Clean
        })
        .into();
    let mut root = Element::new().w(64.0).h(32.0).opacity(0.0).child(canvas);
    let mut tree = LayoutTree::new();
    let hidden = tree.build_and_layout(&mut root, 64.0, 32.0, [-1.0; 2]);
    assert_eq!(paints.get(), 0, "invisible GPU work must not be queued");
    assert!(hidden.textured_draws.is_empty());
    root.style.opacity = 1.0;
    let visible = tree.build_and_layout_fast(&mut root, 64.0, 32.0, [-1.0; 2], false, false);
    assert_eq!(paints.get(), 1, "deferred paint must run on reveal");
    assert!(!visible.textured_draws.is_empty());
}

fn two_column_page() -> Element {
    let col = |bg: [f32; 4], label: &str| {
        Element::new()
            .flex_col()
            .flex_shrink(0.0)
            .w(80.0)
            .h(80.0)
            .child(Element::new().w(80.0).h(40.0).flex_shrink(0.0).bg(bg))
            .child(Element::new().w(80.0).h(40.0).flex_shrink(0.0).text(label))
    };
    Element::new()
        .flex_row()
        .w(160.0)
        .h(80.0)
        .child(col([1.0, 0.0, 0.0, 1.0], "left"))
        .child(col([0.0, 0.0, 1.0, 1.0], "right"))
}

#[test]
#[ignore = "CPU retained collect benchmark; run explicitly with --ignored --nocapture"]
fn retained_collect_frame_budget() {
    use std::time::Instant;
    for workload in ["idle", "one-leaf", "all"] {
        let mut root = Element::new().w(800.0).h(800.0).flex_col();
        for _ in 0..256 {
            let mut row = Element::new().w(800.0).h(2.0).bg([0.2, 0.4, 0.6, 1.0]);
            for _ in 0..4 {
                row = Element::new().w(800.0).h(2.0).child(row);
            }
            root.children.push(row);
        }
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 800.0, 800.0, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        let mut times = Vec::new();
        for frame in 0..220 {
            if workload != "idle" {
                let count = if workload == "all" { 256 } else { 1 };
                for row in root.children.iter_mut().take(count) {
                    let mut leaf = row;
                    for _ in 0..4 {
                        leaf = &mut leaf.children[0];
                    }
                    leaf.style.bg_color = [0.2, (frame % 2) as f32, 0.6, 1.0];
                }
            }
            let begin = Instant::now();
            let result =
                tree.build_and_layout_fast(&mut root, 800.0, 800.0, [-1.0; 2], false, false);
            std::hint::black_box(&result);
            if frame >= 20 {
                times.push(begin.elapsed().as_secs_f64() * 1000.0);
            }
            assert_eq!(result.instances.len(), 256);
            tree.recycle_collect_buffers(
                result.instances,
                result.textured_draws,
                result.input_chrome_instances,
            );
        }
        times.sort_by(f64::total_cmp);
        eprintln!("retained-collect workload={workload} rows=256 depth=5 samples=200 p50_ms={:.4} p95_ms={:.4}", times[100], times[190]);
    }
}

#[test]
fn retained_collect_history_matches_uncached_output() {
    fn assert_output(
        a: &crate::layout::layout_tree::RenderResult,
        b: &crate::layout::layout_tree::RenderResult,
        frame: usize,
    ) {
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.instances),
            bytemuck::cast_slice::<_, u8>(&b.instances),
            "instances frame {frame}"
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.clip_polys),
            bytemuck::cast_slice::<_, u8>(&b.clip_polys),
            "clips frame {frame}"
        );
        assert_eq!(
            format!("{:?}", a.paint_ops),
            format!("{:?}", b.paint_ops),
            "paint order frame {frame}"
        );
        let texts = |r: &crate::layout::layout_tree::RenderResult| {
            r.text_nodes
                .iter()
                .map(|n| {
                    (
                        n.text.clone(),
                        [n.x, n.y, n.width, n.height],
                        n.clip_bounds,
                        format!("{:?}", n.style),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(a), texts(b), "text frame {frame}");
        let slots = |r: &crate::layout::layout_tree::RenderResult| {
            r.node_paints
                .iter()
                .map(|n| {
                    (
                        n.bounds,
                        n.instance,
                        n.textured,
                        n.text,
                        n.chrome,
                        n.paint_ops,
                        n.overflow_clip,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(slots(a), slots(b), "node ranges frame {frame}");
    }
    let mut cached_root = two_column_page();
    let mut full_root = two_column_page();
    let mut cached = LayoutTree::new();
    let mut full = LayoutTree::new();
    cached.build_and_layout(&mut cached_root, 160.0, 80.0, [-1.0; 2]);
    full.build_and_layout(&mut full_root, 160.0, 80.0, [-1.0; 2]);
    for frame in 0..80 {
        for root in [&mut cached_root, &mut full_root] {
            match frame % 10 {
                2 => root.children[0].children[1].text = Some(format!("frame {frame}")),
                3 => root.children[1].children[0].style.bg_color = [0.1, 0.8, 0.3, 0.5],
                4 => root.children[0].style.opacity = 0.0,
                5 => root.children[0].style.opacity = 1.0,
                6 => root.children[1].children[0].style.bg_color = [0.0, 0.0, 1.0, 1.0],
                _ => {}
            }
        }
        let width = if frame % 10 == 7 { 240.0 } else { 160.0 };
        cached.set_collect_skip_allowed(frame % 10 != 8);
        full.set_collect_skip_allowed(false);
        let a =
            cached.build_and_layout_fast(&mut cached_root, width, 80.0, [-1.0; 2], false, false);
        let b = full.build_and_layout_fast(&mut full_root, width, 80.0, [-1.0; 2], false, false);
        assert_output(&a, &b, frame);
        if frame % 10 == 1 {
            assert!(
                cached.last_collect_skip_counts().0 > 0,
                "exercise actual replay"
            );
        }
    }
}

#[test]
fn retained_collect_structural_history_preserves_hits_and_reclaims_nodes() {
    fn fixture() -> Element {
        Element::new()
            .w(320.0)
            .h(240.0)
            .flex_row()
            .children((0..4).map(|column| {
                Element::new()
                    .w(80.0)
                    .h(240.0)
                    .flex_col()
                    .children((0..3).map(|row| {
                        Element::new()
                            .w(80.0)
                            .h(40.0)
                            .flex_shrink(0.0)
                            .template_id(format!("c{column}-r{row}"))
                            .focus_id(format!("c{column}-r{row}"))
                            .bg([column as f32 * 0.2, row as f32 * 0.3, 0.5, 1.0])
                            .text(format!("{column}:{row}"))
                    }))
            }))
    }
    let mut roots = [fixture(), fixture()];
    let mut trees = [LayoutTree::new(), LayoutTree::new()];
    for (root, tree) in roots.iter_mut().zip(&mut trees) {
        tree.build_and_layout(root, 320.0, 240.0, [-1.0; 2]);
    }
    for frame in 0..96 {
        for root in &mut roots {
            match frame % 8 {
                1 => root.children.swap(0, 1),
                2 => root.children[0].children.swap(0, 1),
                3 => {
                    let moved = root.children[0].children.remove(0);
                    root.children[1].children.push(moved);
                }
                4 => {
                    root.children[1].children.pop();
                }
                5 => root.children[0].children.push(
                    Element::new()
                        .w(80.0)
                        .h(40.0)
                        .flex_shrink(0.0)
                        .focus_id(format!("insert-{frame}"))
                        .bg([0.2, 0.8, 0.4, 1.0])
                        .text(format!("new {frame}")),
                ),
                6 => root.children[2].style.opacity = 0.4,
                7 => root.children[2].style.opacity = 1.0,
                _ => {}
            }
        }
        trees[0].set_collect_skip_allowed(frame % 16 != 15);
        trees[1].set_collect_skip_allowed(false);
        let a =
            trees[0].build_and_layout_fast(&mut roots[0], 320.0, 240.0, [-1.0; 2], false, false);
        let b =
            trees[1].build_and_layout_fast(&mut roots[1], 320.0, 240.0, [-1.0; 2], false, false);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.instances),
            bytemuck::cast_slice::<_, u8>(&b.instances),
            "rects frame {frame}"
        );
        assert_eq!(
            format!("{:?}", a.paint_ops),
            format!("{:?}", b.paint_ops),
            "order frame {frame}"
        );
        let texts = |result: &crate::layout::layout_tree::RenderResult| {
            result
                .text_nodes
                .iter()
                .map(|n| {
                    (
                        &n.text,
                        n.x.to_bits(),
                        n.y.to_bits(),
                        n.width.to_bits(),
                        n.height.to_bits(),
                    )
                })
                .map(|(text, x, y, w, h)| (text.clone(), x, y, w, h))
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&a), texts(&b), "text frame {frame}");
        for x in [20.0, 100.0, 180.0, 260.0] {
            for y in [20.0, 60.0, 100.0, 140.0, 180.0] {
                assert_eq!(
                    crate::app::tree_hit::find_focus_id_at(&roots[0], x, y, [0.0; 2]),
                    crate::app::tree_hit::find_focus_id_at(&roots[1], x, y, [0.0; 2]),
                    "hit frame {frame}"
                );
            }
        }
        for (tree, root) in trees.iter().zip(&roots) {
            let mut count = 0;
            crate::core::tree::walk_elements(root, |_| count += 1);
            assert_eq!(
                tree.stats().taffy_nodes,
                count,
                "orphan retention frame {frame}"
            );
        }
    }
}

#[test]
fn paint_only_skips_clean_sibling_subtree() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    assert!(
        first.instances.len() >= 2,
        "both columns must paint on the first collect"
    );
    assert_eq!(tree.last_collect_skip_counts(), (0, 0));

    // Full rebuild does not clone the DL. One allowed collect warms `prev`.
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(
        warmed.instances.len(),
        first.instances.len(),
        "warm collect must still emit both columns"
    );

    root.children[0].children[1].text = Some("LEFT".into());
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let (skipped, _patched) = tree.last_collect_skip_counts();
    assert!(
        skipped >= 1,
        "clean right column must replay last-frame ops (skipped={skipped})"
    );
    assert_eq!(
        second.instances.len(),
        first.instances.len(),
        "skip replays ops; FullClear must not drop sibling pixels"
    );
    assert!(last_skipped_siblings() >= 1);

    let third = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "replayed subtrees must stay in the fingerprint map"
    );
    assert_eq!(third.instances.len(), first.instances.len());
}

#[test]
fn paint_only_skips_clean_leaf_rect() {
    reset_collect_skip_counters();
    let mut root = Element::new().w(80.0).h(40.0).child(
        Element::new()
            .w(80.0)
            .h(40.0)
            .flex_shrink(0.0)
            .bg([1.0, 0.0, 0.0, 1.0]),
    );
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 80.0, 40.0, [-1.0, -1.0]);
    assert!(!first.instances.is_empty());
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    let second = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "leaf rect must fingerprint-replay instead of collect_paint_node"
    );
    assert_eq!(second.instances.len(), first.instances.len());
}

#[test]
fn paint_only_text_change_must_emit_new_glyphs_not_replay_old() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    root.children[0].children[1].text = Some("LEFT".into());
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        second.text_nodes.iter().any(|t| t.text.contains("LEFT")),
        "StyleChanged bind_text must recollect new glyphs, not replay the previous string; got {:?}",
        second
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !second.text_nodes.iter().any(|t| t.text == "left"),
        "stale caption must not remain in the display list after a paint-only text patch"
    );
}

/// Sidebar Header / flex-1 Content / Footer: maximize grows the column but the footer
/// box size is unchanged. Skip geom used to key only parent origin + size, so the
/// footer display list stayed at the pre-maximize Y (Gallery Send after maximize).
#[test]
fn flex_column_resize_must_not_replay_footer_at_old_y() {
    reset_collect_skip_counters();
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("HeaderChrome")),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("FooterChrome")),
        );

    let mut tree = LayoutTree::new();
    let short = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);

    let footer_short = short
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at 200h");
    assert!(
        footer_short.y + footer_short.height > 150.0,
        "footer must sit near the bottom at 200h, got y={}",
        footer_short.y
    );

    let tall = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], false, false);
    let footer_tall = tall
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at 400h");
    assert!(
        footer_tall.y > footer_short.y + 80.0,
        "maximize must recollect Footer at the new bottom, not replay the 200h Y\n\
         short_y={} tall_y={}",
        footer_short.y,
        footer_tall.y
    );
    assert!(
        footer_tall.y + footer_tall.height > 350.0,
        "footer must sit near the 400h bottom, got y={}",
        footer_tall.y
    );

    let restored = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let footer_restored = restored
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer text at restored 200h");
    assert!(
        (footer_restored.y - footer_short.y).abs() < 2.0,
        "restore/minimize must recollect Footer at the short bottom, not keep the 400h Y\n\
         short_y={} restored_y={}",
        footer_short.y,
        footer_restored.y
    );
}

/// AppState live resize is `Clean` + `scroll_only`. Viewport change must still
/// recompute flex locations (maximize flake / minimize-stuck).
#[test]
fn scroll_only_resize_must_reflow_flex_footer() {
    reset_collect_skip_counters();
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("HeaderChrome")),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
        .child(
            Element::new()
                .w("100%")
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("FooterChrome")),
        );

    let mut tree = LayoutTree::new();
    let short = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let footer_short = short
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at 200h");

    let tall = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], true, false);
    let footer_tall = tall
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at 400h scroll_only");
    assert!(
        footer_tall.y > footer_short.y + 80.0,
        "scroll_only maximize must still compute Footer Y\nshort_y={} tall_y={}",
        footer_short.y,
        footer_tall.y
    );

    let restored = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], true, false);
    let footer_restored = restored
        .text_nodes
        .iter()
        .find(|n| n.text == "FooterChrome")
        .expect("footer at restored scroll_only");
    assert!(
        (footer_restored.y - footer_short.y).abs() < 2.0,
        "scroll_only restore must compute Footer back to the short bottom\n\
         short_y={} restored_y={}",
        footer_short.y,
        footer_restored.y
    );
}

#[test]
fn collect_skip_is_paint_only() {
    use LayoutStatus::*;
    assert!(collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        false,
        false
    ));
    assert!(collect_skip_allowed_for_frame(
        Clean, false, false, false, false
    ));
    assert!(
        collect_skip_allowed_for_frame(LayoutChanged, false, false, false, false),
        "LayoutChanged after maximize/fit must still skip idle siblings; GeomKey busts moved boxes"
    );
    assert!(!collect_skip_allowed_for_frame(
        StructureChanged,
        false,
        false,
        false,
        false
    ));
    assert!(
        collect_skip_allowed_for_frame(StyleChanged, true, false, false, false),
        "smooth scroll must skip clean siblings; GeomKey.scroll busts the moving viewport"
    );
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        true,
        false,
        false
    ));
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        true,
        false
    ));
    assert!(!collect_skip_allowed_for_frame(
        StyleChanged,
        false,
        false,
        false,
        true
    ));
}

#[test]
fn paint_only_scroll_only_replays_clean_siblings() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    let second = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "idle ViewportsOnly collect is paint-only; fingerprint skip must replay clean siblings"
    );
    assert_eq!(warmed.instances.len(), first.instances.len());
    assert_eq!(second.instances.len(), first.instances.len());
}

#[test]
fn skip_disallowed_frame_drops_stale_absolute_dls() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(tree.last_collect_skip_counts().0 >= 1);

    tree.set_collect_skip_allowed(false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(tree.last_collect_skip_counts().0, 0);

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "skip-off must drop prev so the next paint-only frame cannot replay pre-layout DLs"
    );
}

fn sidebar_and_scroll_page() -> Element {
    let sidebar = Element::new()
        .flex_col()
        .w(80.0)
        .h(120.0)
        .flex_shrink(0.0)
        .bg([0.15, 0.15, 0.18, 1.0])
        .child(Element::new().text("SidebarChrome"));
    let rows = Element::new().flex_col().w(80.0).children((0..8).map(|i| {
        Element::new()
            .w(80.0)
            .h(40.0)
            .flex_shrink(0.0)
            .child(Element::new().text(format!("Row{i}")))
    }));
    let mut viewport = Element::new()
        .w(80.0)
        .h(120.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(rows);
    viewport.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}skip-scroll"));
    Element::new()
        .flex_row()
        .w(160.0)
        .h(120.0)
        .child(sidebar)
        .child(viewport)
}

#[test]
fn smooth_scroll_must_skip_clean_sidebar_and_keep_fingerprint() {
    reset_collect_skip_counters();
    let mut root = sidebar_and_scroll_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 120.0, [-1.0, -1.0]);
    let row2 = first
        .text_nodes
        .iter()
        .find(|n| n.text == "Row2")
        .expect("row2 at rest");
    let y2 = row2.y;

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 120.0, [-1.0, -1.0], true, false);

    root.children[1].scroll_offset[1] = 48.0;
    use_signal(&scroll_offset_y_signal_key("skip-scroll"), || 0.0_f32).set(48.0);
    let scrolled = tree.build_and_layout_fast(&mut root, 160.0, 120.0, [-1.0, -1.0], true, false);
    let (skipped, _patched) = tree.last_collect_skip_counts();
    assert!(
        skipped >= 1,
        "smooth-scroll collect must replay the idle sidebar, not recollect the whole page (skipped={skipped})"
    );
    let row2_scrolled = scrolled
        .text_nodes
        .iter()
        .find(|n| n.text == "Row2")
        .expect("row2 after scroll");
    assert!(
        row2_scrolled.y < y2 - 20.0,
        "viewport children must recollect at the new scroll_acc, rest_y={y2} scrolled_y={}",
        row2_scrolled.y
    );
    assert!(
        (row2_scrolled.y - (y2 - 48.0)).abs() < 1.0,
        "fully-visible row must translate by -scroll_acc, rest_y={y2} scrolled_y={}",
        row2_scrolled.y
    );
    assert!(
        scrolled.text_nodes.iter().any(|n| n.text == "Row3"),
        "row that entered the clip must recollect, not replay an empty culled DL"
    );
    assert!(
        skipped >= 2,
        "smooth-scroll must translate fully-visible rows in addition to the idle sidebar (skipped={skipped})"
    );
    assert!(
        scrolled
            .text_nodes
            .iter()
            .any(|n| n.text == "SidebarChrome"),
        "skip replays sidebar glyphs; FullClear must still receive them"
    );

    // Second collect (bubble +N placement / portal follow) must not wipe skip.
    let again = tree.build_and_layout_fast(&mut root, 160.0, 120.0, [-1.0, -1.0], true, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "a follow-up collect while skip stays allowed must still replay the sidebar"
    );
    assert!(again.text_nodes.iter().any(|n| n.text == "SidebarChrome"));
}

const SPIN_BG: [f32; 4] = [0.91, 0.12, 0.88, 1.0];

fn header_spinner_page() -> Element {
    Element::new()
        .flex_col()
        .w(200.0)
        .h("100%")
        .child(
            Element::new().w("100%").h(40.0).flex_shrink(0.0).child(
                Element::new()
                    .w(16.0)
                    .h(16.0)
                    .flex_shrink(0.0)
                    .bg(SPIN_BG)
                    .child(Element::new().w(1.0).h(1.0)),
            ),
        )
        .child(
            Element::new()
                .w("100%")
                .flex_grow(1.0)
                .min_h(0.0)
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(Element::new().text("NavItem")),
        )
}

fn spinner_instance_transform(result: &crate::layout::layout_tree::RenderResult) -> [f32; 6] {
    result
        .instances
        .iter()
        .find(|i| {
            (i.color[0] - SPIN_BG[0]).abs() < 0.02
                && (i.color[1] - SPIN_BG[1]).abs() < 0.02
                && (i.color[2] - SPIN_BG[2]).abs() < 0.02
                && i.rect_size[0] >= 8.0
                && i.rect_size[0] <= 24.0
        })
        .map(|i| i.transform)
        .expect("spinner instance")
}

fn spinner_fill(
    result: &crate::layout::layout_tree::RenderResult,
) -> &crate::renderer::InstanceInput {
    result
        .instances
        .iter()
        .find(|i| {
            (i.color[0] - SPIN_BG[0]).abs() < 0.02
                && (i.color[1] - SPIN_BG[1]).abs() < 0.02
                && (i.color[2] - SPIN_BG[2]).abs() < 0.02
                && i.rect_size[0] >= 8.0
        })
        .expect("spinner fill")
}

fn transform_is_identity(m: &[f32; 6]) -> bool {
    let id = crate::style::MAT2D_IDENTITY;
    m.iter()
        .zip(id.iter())
        .all(|(a, b)| (*a - *b).abs() <= 1e-4)
}

/// LayoutTree-level: even if skip is wrongly left on across a grow, viewport
/// invalidate plus geom origin must not replay the rest-pose spinner.
/// keeps the same box; hashing only origin/size replayed the rest-pose DL and froze
/// `animate-spin` after the window grew.
#[test]
fn collect_skip_must_not_freeze_rotate_after_maximize() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let rest = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    assert!(
        transform_is_identity(&spinner_instance_transform(&rest)),
        "warm collect is rest pose"
    );

    root.children[0].children[0].style.rotate_deg = 45.0;
    let maxed = tree.build_and_layout_fast(&mut root, 200.0, 400.0, [-1.0, -1.0], false, false);
    let xf = spinner_instance_transform(&maxed);
    assert!(
        !transform_is_identity(&xf),
        "maximize LayoutChanged must recollect the 45° spin, not replay the rest-pose DL\n\
         transform={xf:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_rotated_sibling() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);

    root.children[0].children[0].style.rotate_deg = 90.0;
    let spun = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let xf = spinner_instance_transform(&spun);
    assert!(
        !transform_is_identity(&xf),
        "paint-hash skip must bust when rotate_deg changes, got {xf:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_transform_origin() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    root.children[0].children[0].style.rotate_deg = 90.0;
    root.children[0].children[0].style.transform_origin =
        crate::style::parse_transform_origin("center");
    let center = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let center_xf = spinner_instance_transform(&center);
    assert!(
        center_xf[4].abs() < 0.05 && center_xf[5].abs() < 0.05,
        "center origin 90° spinner must keep shader tx/ty at 0, got {center_xf:?}"
    );

    root.children[0].children[0].style.transform_origin =
        crate::style::parse_transform_origin("left top");
    let left_top = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let lt_xf = spinner_instance_transform(&left_top);
    assert!(
        (lt_xf[4] - center_xf[4]).abs() > 1.0 || (lt_xf[5] - center_xf[5]).abs() > 1.0,
        "paint-only transform-origin must move the shader pivot, not replay center-pivot DL\n\
         center={center_xf:?} left-top={lt_xf:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_scale_transform_origin() {
    reset_collect_skip_counters();
    let mut root = header_spinner_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    root.children[0].children[0].style.scale = [2.0, 1.0];
    root.children[0].children[0].style.transform_origin =
        crate::style::parse_transform_origin("center");
    let center = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let center_x = spinner_fill(&center).rect_pos[0];

    root.children[0].children[0].style.transform_origin =
        crate::style::parse_transform_origin("left top");
    let left_top = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let lt_x = spinner_fill(&left_top).rect_pos[0];
    assert!(
        (lt_x - center_x).abs() > 1.0,
        "paint-only origin must move scale bake vis_x, not replay center-inset DL\n\
         center_x={center_x} left_top_x={lt_x}"
    );
}

#[test]
fn collect_skip_must_not_replay_faded_sibling() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    assert!(
        first
            .instances
            .iter()
            .any(|i| (i.color[0] - 1.0).abs() < 0.02 && i.color[3] > 0.9),
        "warm collect paints an opaque left-column fill"
    );

    root.children[0].style.opacity = 0.4;
    let faded = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        faded
            .instances
            .iter()
            .any(|i| (i.color[0] - 1.0).abs() < 0.02 && (i.color[3] - 0.4).abs() < 0.05),
        "paint-hash / geom skip must bust when opacity changes; got {:?}",
        faded.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
    assert!(
        !faded
            .instances
            .iter()
            .any(|i| (i.color[0] - 1.0).abs() < 0.02 && i.color[3] > 0.9),
        "must not replay the opaque left-column fill after a paint-only fade"
    );
}

fn extra_shadow(color: [f32; 4]) -> crate::style::BoxShadow {
    crate::style::BoxShadow {
        color,
        offset: [4.0, 4.0],
        blur: 2.0,
        spread: 0.0,
        inset: false,
    }
}

fn approx_shadow(c: [f32; 4], want: [f32; 4]) -> bool {
    (0..4).all(|i| (c[i] - want[i]).abs() < 0.05)
}

#[test]
fn collect_skip_must_not_replay_extra_shadow_color() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let red = [1.0, 0.0, 0.0, 0.8];
    let green = [0.0, 1.0, 0.0, 0.8];
    root.children[0].children[0]
        .style
        .extra_shadows
        .push(extra_shadow(red));

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        warmed
            .instances
            .iter()
            .any(|i| approx_shadow(i.shadow_color, red)),
        "warm collect must emit the red extra shadow"
    );

    root.children[0].children[0].style.extra_shadows[0].color = green;
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        next.instances
            .iter()
            .any(|i| approx_shadow(i.shadow_color, green)),
        "paint-hash skip must bust when extra_shadows color changes; got {:?}",
        next.instances
            .iter()
            .map(|i| i.shadow_color)
            .collect::<Vec<_>>()
    );
    assert!(
        !next
            .instances
            .iter()
            .any(|i| approx_shadow(i.shadow_color, red)),
        "must not replay the previous extra-shadow color"
    );
}

#[test]
fn collect_skip_must_not_replay_gradient_stops() {
    use crate::style::{Gradient, LinearGradient};
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].children[0].style.bg_gradient = Some(Gradient::Linear(
        LinearGradient::two_stop([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 90.0),
    ));

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let old_keys: Vec<String> = warmed
        .textured_draws
        .iter()
        .map(|d| d.texture_key.clone())
        .collect();
    assert!(
        !old_keys.is_empty(),
        "warm collect must emit a gradient texture draw"
    );

    if let Some(Gradient::Linear(g)) = root.children[0].children[0].style.bg_gradient.as_mut() {
        g.stops[0].color = [0.0, 0.0, 1.0, 1.0];
    }
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let new_keys: Vec<String> = next
        .textured_draws
        .iter()
        .map(|d| d.texture_key.clone())
        .collect();
    assert!(
        new_keys.iter().any(|k| !old_keys.contains(k)),
        "paint-hash skip must bust when gradient stops change; old={old_keys:?} new={new_keys:?}"
    );
    for k in &old_keys {
        assert!(
            !new_keys.contains(k),
            "must not replay the previous gradient texture key {k}"
        );
    }
}

#[test]
fn collect_skip_must_not_replay_filter_blur() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].style.filter = crate::style::parse_filter("blur(4px)");

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let old_keys: Vec<String> = warmed
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("jadix_filter:"))
        .map(|d| d.texture_key.clone())
        .collect();
    assert!(
        !old_keys.is_empty(),
        "warm collect must emit a CSS filter wrap; textured={:?}",
        warmed
            .textured_draws
            .iter()
            .map(|d| d.texture_key.as_str())
            .collect::<Vec<_>>()
    );

    root.children[0].style.filter = crate::style::parse_filter("blur(12px)");
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let new_keys: Vec<String> = next
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("jadix_filter:"))
        .map(|d| d.texture_key.clone())
        .collect();
    assert!(
        new_keys.iter().any(|k| !old_keys.contains(k)),
        "paint-hash skip must bust when filter blur amount changes; old={old_keys:?} new={new_keys:?}"
    );
    for k in &old_keys {
        assert!(
            !new_keys.contains(k),
            "must not replay the previous filter wrap key {k}"
        );
    }
}

fn first_active_clip(polys: &[crate::style::GpuClipPoly]) -> Option<(u32, [f32; 2])> {
    polys
        .iter()
        .find(|p| p.is_active())
        .map(|p| (p.count, p.pts[0]))
}

#[test]
fn collect_skip_must_not_replay_clip_path() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].style.clip_path =
        crate::style::parse_clip_path("circle(40% at 50% 50%)").unwrap();

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let old = first_active_clip(&warmed.clip_polys).expect("warm collect must emit clip-path poly");

    root.children[0].style.clip_path =
        crate::style::parse_clip_path("circle(20% at 50% 50%)").unwrap();
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let new = first_active_clip(&next.clip_polys).expect("recollect must emit clip-path poly");
    assert_ne!(
        old, new,
        "paint-hash skip must bust when clip-path radius changes; old={old:?} new={new:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_outline_color() {
    use crate::style::LineStyle;
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let fill = &mut root.children[0].children[0];
    fill.style.outline_style = LineStyle::Solid;
    fill.style.outline_width = 2.0;
    fill.style.outline_color = [0.0, 0.0, 1.0, 1.0];

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        warmed
            .instances
            .iter()
            .any(|i| i.border_width > 1.0 && (i.border_color[2] - 1.0).abs() < 0.05),
        "warm collect must emit the blue outline"
    );

    root.children[0].children[0].style.outline_color = [0.0, 1.0, 0.0, 1.0];
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        next.instances
            .iter()
            .any(|i| i.border_width > 1.0 && (i.border_color[1] - 1.0).abs() < 0.05),
        "paint-hash skip must bust when outline color changes; got {:?}",
        next.instances
            .iter()
            .map(|i| (i.border_width, i.border_color))
            .collect::<Vec<_>>()
    );
    assert!(
        !next.instances.iter().any(|i| i.border_width > 1.0
            && (i.border_color[2] - 1.0).abs() < 0.05
            && i.border_color[1] < 0.2),
        "must not replay the previous outline color"
    );
}

#[test]
fn collect_skip_must_not_replay_per_side_border_color() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let fill = &mut root.children[0].children[0];
    fill.style.set_uniform_border_width(4.0);
    fill.style.border_style = crate::style::LineStyle::Solid;
    fill.style.set_border_side_color(0, [0.0, 0.0, 1.0, 1.0]);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    // Per-side borders collect as fill strips (`instance.color`), not SDF `border_color`.
    assert!(
        warmed
            .instances
            .iter()
            .any(|i| (i.color[2] - 1.0).abs() < 0.05 && i.color[0] < 0.2 && i.rect_size[1] <= 5.0),
        "warm collect must emit the blue top-border strip"
    );

    root.children[0].children[0]
        .style
        .set_border_side_color(0, [0.0, 1.0, 0.0, 1.0]);
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        next.instances
            .iter()
            .any(|i| (i.color[1] - 1.0).abs() < 0.05 && i.color[0] < 0.2 && i.rect_size[1] <= 5.0),
        "paint-hash skip must bust when a side border color changes; got {:?}",
        next.instances
            .iter()
            .map(|i| (i.rect_size, i.color))
            .collect::<Vec<_>>()
    );
    assert!(
        !next.instances.iter().any(|i| i.rect_size[1] <= 5.0
            && (i.color[2] - 1.0).abs() < 0.05
            && i.color[1] < 0.2
            && i.color[0] < 0.2),
        "must not replay the previous side-border strip color"
    );
}

#[test]
fn collect_skip_must_not_replay_letter_spacing() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].children[1].text_style.letter_spacing = 0.0;
    root.children[0].children[1]
        .text_style
        .letter_spacing_specified = true;

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    root.children[0].children[1].text_style.letter_spacing = 4.0;
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let left = next
        .text_nodes
        .iter()
        .find(|n| n.text == "left" || n.text == "LEFT")
        .expect("left caption");
    assert!(
        (left.style.letter_spacing - 4.0).abs() < 0.01,
        "paint-hash skip must bust when letter-spacing changes; got {}",
        left.style.letter_spacing
    );
}

#[test]
fn collect_skip_must_not_replay_text_align() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].children[1].text_style.align = Some(crate::style::TextAlign::Start);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    root.children[0].children[1].text_style.align = Some(crate::style::TextAlign::Center);
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let left = next
        .text_nodes
        .iter()
        .find(|n| n.text == "left" || n.text == "LEFT")
        .expect("left caption");
    assert_eq!(
        left.style.align,
        Some(crate::style::TextAlign::Center),
        "paint-hash skip must bust when text-align changes"
    );
}

#[test]
fn collect_skip_must_not_replay_font_family() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    root.children[0].children[1].text_style.font_family = Some("sans-serif".into());

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);

    root.children[0].children[1].text_style.font_family = Some("monospace".into());
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let left = next
        .text_nodes
        .iter()
        .find(|n| n.text == "left" || n.text == "LEFT")
        .expect("left caption");
    assert_eq!(
        left.style.font_family.as_deref(),
        Some("monospace"),
        "paint-hash skip must bust when font-family changes"
    );
}

#[test]
fn collect_skip_must_not_replay_overflow_hidden() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut spill = Element::new()
        .w(80.0)
        .h(200.0)
        .absolute()
        .left(0.0)
        .top(0.0)
        .bg([0.0, 1.0, 0.0, 1.0]);
    spill = spill.child(Element::new().w(1.0).h(1.0));
    root.children[0].style.overflow.y = taffy::style::Overflow::Visible;
    root.children[0].children.push(spill);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let spill_clip = |result: &crate::layout::layout_tree::RenderResult| {
        result
            .instances
            .iter()
            .find(|i| {
                (i.color[1] - 1.0).abs() < 0.05 && i.color[0] < 0.1 && i.rect_size[1] >= 150.0
            })
            .map(|i| i.clip_size)
    };
    let warm_clip = spill_clip(&warmed).expect("warm collect must emit the overflowing fill");
    assert!(
        warm_clip[0] < 0.0,
        "visible overflow must not clip the spill fill; clip={warm_clip:?}"
    );

    root.children[0].style.overflow.y = taffy::style::Overflow::Hidden;
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let hidden_clip = spill_clip(&next).expect("recollect must still emit the spill fill");
    assert!(
        hidden_clip[0] >= 0.0 && hidden_clip[1] > 0.0 && hidden_clip[1] <= 80.5,
        "paint-hash skip must bust when overflow becomes hidden (layout clip, not rotated vis); \
         clip={hidden_clip:?}"
    );
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "right column must still fingerprint-replay"
    );
}

#[test]
fn collect_skip_must_not_replay_corner_shape() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let fill = &mut root.children[0].children[0];
    fill.style.radii = [8.0, 8.0, 8.0, 8.0];
    fill.style.corner_shape = crate::style::CornerShape::Round;

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let round_extra = warmed
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.radii[0] > 1.0)
        .map(|i| i.border_extra)
        .expect("warm collect must emit the rounded left fill");

    root.children[0].children[0].style.corner_shape = crate::style::CornerShape::Notch;
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let notch_extra = next
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.radii[0] > 1.0)
        .map(|i| i.border_extra)
        .expect("left fill after corner-shape change");
    assert_ne!(
        round_extra, notch_extra,
        "paint-hash skip must bust when corner-shape changes; still extra={round_extra}"
    );
}

#[test]
fn collect_skip_must_not_replay_offset_distance() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let fill = &mut root.children[0].children[0];
    fill.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    fill.style.offset_distance = 0.0;

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let rest_x = warmed
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.rect_size[0] > 40.0)
        .map(|i| i.rect_pos[0])
        .expect("warm collect must emit the left fill");

    root.children[0].children[0].style.offset_distance = 1.0;
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let moved_x = next
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.rect_size[0] > 40.0)
        .map(|i| i.rect_pos[0])
        .expect("left fill after offset_distance change");
    assert!(
        (moved_x - rest_x - 40.0).abs() < 0.5,
        "paint-hash skip must bust when offset_distance moves the fill; rest={rest_x} moved={moved_x}"
    );
}

#[test]
fn collect_skip_must_not_replay_offset_rotate() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let fill = &mut root.children[0].children[0];
    fill.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
    fill.style.offset_distance = 0.0;
    fill.style.offset_rotate_deg = Some(0.0);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let rest = warmed
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.rect_size[0] > 40.0)
        .map(|i| i.transform)
        .expect("warm collect must emit the left fill");

    root.children[0].children[0].style.offset_rotate_deg = Some(90.0);
    let next = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let spun = next
        .instances
        .iter()
        .find(|i| i.color[0] > 0.9 && i.rect_size[0] > 40.0)
        .map(|i| i.transform)
        .expect("left fill after offset_rotate change");
    assert!(
        (spun[1] - rest[1]).abs() > 0.5 || (spun[2] - rest[2]).abs() > 0.5,
        "paint-hash skip must bust when offset_rotate_deg changes; rest={rest:?} spun={spun:?}"
    );
}

#[test]
fn collect_skip_must_not_replay_parent_rotate_on_centered_child() {
    reset_collect_skip_counters();
    let child: Element = crate::primitives::r#box::r#box()
        .w(80.0)
        .h(20.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .absolute(true)
        .left(60.0)
        .top(90.0)
        .into();
    let parent = Element::new().w(200.0).h(200.0).child(child);
    let mut root = Element::new().w(200.0).h(200.0).child(parent);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);

    root.children[0].style.rotate_deg = 90.0;
    let spun = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], false, false);
    let child_xf = spun
        .instances
        .iter()
        .find(|i| i.color[0] > 0.5 && (i.rect_size[0] - 80.0).abs() < 1.0)
        .map(|i| i.transform)
        .expect("centered child fill");
    assert!(
        child_xf[1].abs() > 0.5 || child_xf[2].abs() > 0.5,
        "parent rotate must recollect a centered child (origin unchanged); got {child_xf:?}"
    );
}

#[test]
fn viewport_resize_drops_skip_cache() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "warm skip cache must replay a clean sibling"
    );

    let _ = tree.build_and_layout_fast(&mut root, 160.0, 240.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "maximize must drop the fingerprint DL; replaying absolute glyphs stacks the old Y"
    );
}

#[test]
fn collect_skip_must_not_replay_shimmer_box() {
    reset_collect_skip_counters();
    let mut box_el = Element::new()
        .w(200.0)
        .h(40.0)
        .bg([0.9, 0.9, 0.9, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    crate::style::apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    let mut root = Element::new().w(200.0).h(80.0).child(box_el).child(
        Element::new()
            .w(200.0)
            .h(40.0)
            .child(Element::new().text("next-section")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    let second = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        root.children[0].collect_skip_live,
        "shimmer-box subtree must be marked live so skip cannot freeze the sheen"
    );
    assert_eq!(
        second
            .text_nodes
            .iter()
            .filter(|n| n.text.contains("shimmer-box overlay"))
            .count(),
        1,
        "label must paint once, not a skip-ghost plus a live collect"
    );
}

#[test]
fn maximize_must_not_ghost_shimmer_label() {
    reset_collect_skip_counters();
    let mut box_el = Element::new()
        .w(200.0)
        .h(40.0)
        .bg([0.9, 0.9, 0.9, 1.0])
        .child(Element::new().text("shimmer-box overlay on container"));
    crate::style::apply_class_list(&mut box_el, "shimmer-box shimmer-duration-1000");
    let mut root = Element::new().flex_col().w(200.0).child(box_el).child(
        Element::new()
            .w(200.0)
            .h(40.0)
            .child(Element::new().text("1c-msg) Message")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], false, false);
    let grown = tree.build_and_layout_fast(&mut root, 200.0, 240.0, [-1.0, -1.0], false, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "viewport grow must drop skip cache"
    );
    let labels: Vec<_> = grown
        .text_nodes
        .iter()
        .filter(|n| n.text.contains("shimmer-box overlay"))
        .collect();
    assert_eq!(
        labels.len(),
        1,
        "maximize must not leave a skip-ghost copy of the shimmer label"
    );
    let msg_y = grown
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-msg"))
        .map(|n| n.y)
        .expect("message heading");
    assert!(
        labels[0].y + 8.0 < msg_y,
        "shimmer label y={} must stay above the next section y={msg_y}, not overlay it",
        labels[0].y
    );
}

#[test]
fn collect_skip_must_not_replay_animate_spin() {
    reset_collect_skip_counters();
    let mut spin = Element::new()
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .bg(SPIN_BG)
        .child(Element::new().w(1.0).h(1.0));
    crate::style::apply_class_list(&mut spin, "animate-spin");
    let mut root = Element::new().w(80.0).h(40.0).child(
        Element::new()
            .w(80.0)
            .h(40.0)
            .child(spin)
            .child(Element::new().text("idle")),
    );
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 40.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 40.0, [-1.0, -1.0], false, false);
    assert!(
        root.children[0].children[0].collect_skip_live,
        "animate-spin must mark the spinner live"
    );
    assert!(
        root.children[0].children[0].children[0].collect_skip_live,
        "spin dummy child must recollect with the spinner, not fingerprint-replay"
    );
    let skipped = tree.last_collect_skip_counts().0;
    assert!(
        skipped <= 1,
        "only the idle sibling may skip; spinner subtree must recollect (skipped={skipped})"
    );
}

#[test]
fn css_spin_must_keep_rotating_after_maximize_skip() {
    reset_collect_skip_counters();
    crate::style::utility::set_frame_dt(1.0 / 30.0);
    let mut spin = Element::new()
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .bg(SPIN_BG)
        .child(Element::new().w(1.0).h(1.0));
    crate::style::apply_class_list(&mut spin, "animate-spin");
    let mut root = Element::new().w(80.0).h("100%").child(spin);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 80.0, [-1.0, -1.0], false, false);

    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        80.0,
        240.0,
        &mut tree,
        &mut impact,
    );
    let a = tree.build_and_layout_fast(&mut root, 80.0, 240.0, [-1.0, -1.0], false, false);
    let xf_a = spinner_instance_transform(&a);

    crate::style::sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        80.0,
        240.0,
        &mut tree,
        &mut impact,
    );
    let b = tree.build_and_layout_fast(&mut root, 80.0, 240.0, [-1.0, -1.0], false, false);
    let xf_b = spinner_instance_transform(&b);
    assert!(
        xf_a.iter()
            .zip(xf_b.iter())
            .any(|(x, y)| (*x - *y).abs() > 1e-4),
        "CSS spin must advance on post-maximize skip frames, got {xf_a:?} then {xf_b:?}"
    );
}

/// Gallery Kbd Search: focus is paint-only. If the Input subtree's fingerprint
/// ignores `input_caret`, collect-skip replays the unfocused DL (no hairline)
/// while dest-reset wipes the box — caret gone, inner fill looks nested.
#[test]
fn focused_kbd_input_must_not_replay_unfocused_chrome() {
    reset_collect_skip_counters();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "skip-kbd-search";
    use_signal(&input_value_signal_key(id), String::new).set(String::new());
    use_signal("__jadix_button_focus_id", String::new).set(String::new());

    let mut root = Element::new()
        .w(400.0)
        .h(80.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(
            hstack().w("100%").h(36.0).px(12.0).items("center").child(
                input_adornment()
                    .child(input(id).placeholder("Search…").plain_chrome(true))
                    .addon("inline-end", Element::new().text("⌘K")),
            ),
        );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let unfocused = tree.build_and_layout_fast(&mut root, 400.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        unfocused.input_chrome_instances.is_empty(),
        "unfocused field must not emit a caret hairline"
    );

    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 80.0, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        400.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    fn tree_has_input_caret(el: &Element) -> bool {
        el.text_style.input_caret.is_some() || el.children.iter().any(tree_has_input_caret)
    }
    fn tree_has_paint_sync(el: &Element) -> bool {
        el.input_paint_sync.is_some() || el.children.iter().any(tree_has_paint_sync)
    }
    assert!(
        tree_has_paint_sync(&root),
        "shell must contain the Search Input paint-sync node"
    );
    assert!(
        tree_has_input_caret(&root),
        "sync must install input_caret on the focused field before collect"
    );
    let focused = tree.build_and_layout_fast(&mut root, 400.0, 80.0, [-1.0, -1.0], false, false);

    let has_caret_draw = focused
        .text_nodes
        .iter()
        .any(|n| n.style.input_caret.is_some());
    assert!(
        has_caret_draw,
        "focus collect must keep input_caret on text nodes (skip must not replay the unfocused leaf)"
    );
    assert!(
        !focused.input_chrome_instances.is_empty(),
        "focused Kbd Input must collect caret chrome on a skip-allowed StyleChanged frame; \
         fingerprint skip replayed the unfocused DL (no hairline)"
    );
    assert!(
        focused.node_paints.iter().any(|p| p.chrome.0 < p.chrome.1),
        "some collect slot must own the caret chrome range so node-present can redraw it"
    );
    // 图一: click focus must paint Hard ring on the adornment shell (not just
    // border-color + caret — that looks like 图四 default).
    let shell_ring = focused.instances.iter().any(|i| {
        i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05 && i.shadow_blur.abs() < 0.5
    });
    assert!(
        shell_ring,
        "图一: focused InputAdornment shell must emit Hard-ring (spread>1, blur≈0); \
         got only caret / border without ring chrome"
    );

    // Overflow:hidden + rounded must not attach a clip-poly / clip_size that
    // scissors the shell's own Hard ring (placeholder flicker with no ring).
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let shell_el = find_slot(&root, "input-adornment").expect("shell");
    let shell_key = crate::app::damage::paint_element_key(shell_el);
    if let Some(p) = focused.node_paints.iter().find(|p| p.key == shell_key) {
        let lo = p.instance.0.min(focused.instances.len());
        let hi = p.instance.1.min(focused.instances.len());
        for (i, inst) in focused.instances[lo..hi].iter().enumerate() {
            if inst.shadow_spread.abs() > 1.0 && inst.shadow_color[3] > 0.05 {
                assert!(
                    inst.clip_size == crate::renderer::InstanceInput::NO_CLIP_SIZE
                        || inst.clip_size[0] < 0.0,
                    "shell Hard-ring instance must not be overflow-clipped; clip={:?} idx={}",
                    inst.clip_size,
                    lo + i
                );
                if let Some(poly) = focused.clip_polys.get(lo + i) {
                    assert!(
                        !poly.is_active(),
                        "shell Hard-ring must not sit under an active overflow clip-poly"
                    );
                }
            }
        }
    }
}

/// Gallery: scroll settles under ViewportsOnly `reuse_idle`, then one click on
/// Search. `StyleChanged` Hard ring must bust the idle ancestor spine — otherwise
/// skip-replays the unfocused DL (placeholder flicker only; no ring/caret).
#[test]
fn scroll_settle_click_search_must_recollect_hard_ring_under_reuse_idle() {
    reset_collect_skip_counters();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "skip-kbd-settle-click";
    use_signal(&input_value_signal_key(id), String::new).set(String::new());
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
    use_signal(&scroll_offset_y_signal_key("kbd-settle-main"), || 0.0_f32).set(0.0);

    let mut root = Element::new()
        .w(400.0)
        .h(320.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(
            scroll_area("kbd-settle-main").w("100%").h("100%").child(
                vstack()
                    .w("100%")
                    .gap(12.0)
                    .child(
                        input_adornment()
                            .child(input(id).placeholder("Search…").plain_chrome(true))
                            .addon("inline-end", Element::new().text("⌘K")),
                    )
                    .child(Element::new().w("100%").h(480.0).bg([0.9, 0.9, 0.91, 1.0])),
            ),
        );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 400.0, 320.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);

    // Scroll + settle while unfocused: stamps idle hashes on wrappers (same as
    // Gallery page ScrollArea after the wheel stops).
    use_signal(&scroll_offset_y_signal_key("kbd-settle-main"), || 0.0_f32).set(12.0);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 320.0, [-1.0, -1.0], true, false);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 320.0, [-1.0, -1.0], true, false);

    // Click Search: focus + adornment StyleChanged under still-ViewportsOnly.
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 320.0, [0.0, 0.0]);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    crate::renderer::restart_caret_blink();
    assert!(
        matches!(impact, LayoutStatus::StyleChanged),
        "focus must be paint-only StyleChanged (got {:?})",
        impact
    );

    // Frame path keeps `scroll_only` when ViewportsOnly + StyleChanged.
    let focused = tree.build_and_layout_fast(&mut root, 400.0, 320.0, [-1.0, -1.0], true, false);
    assert!(
        focused.instances.iter().any(|i| {
            i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05 && i.shadow_blur.abs() < 0.5
        }),
        "settle-click Search must recollect Hard ring under reuse_idle; \
         idle ancestors skip-replayed the unfocused DL (placeholder-only flicker)"
    );
    assert!(
        !focused.input_chrome_instances.is_empty(),
        "settle-click Search must collect caret chrome under reuse_idle"
    );
    // Placeholder stays visible while empty+focused (native <input>). Caret must
    // sit at the inline-start of that run — mid-word means stacked glyphs or a
    // scroll_x/ellipsis mismatch (Gallery: caret overlaid on "Search…").
    let ph = focused
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Search"))
        .expect("placeholder text node");
    let caret = focused
        .input_chrome_instances
        .iter()
        .min_by(|a, b| {
            a.rect_pos[0]
                .partial_cmp(&b.rect_pos[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("caret hairline");
    assert!(
        (caret.rect_pos[0] - ph.x).abs() < 4.0,
        "focused empty Search caret must start at placeholder origin; caret_x={} ph_x={} text={:?}",
        caret.rect_pos[0],
        ph.x,
        ph.text
    );
}

/// Gallery 图二: focused Kbd Search stays focused while the page ScrollArea
/// moves. Fingerprint skip on the scroll frame must not replay the unfocused
/// DL (caret disappears until the next blink ON — 图三).
#[test]
fn focused_kbd_input_must_keep_caret_chrome_across_scroll() {
    reset_collect_skip_counters();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "skip-kbd-scroll";
    use_signal(&input_value_signal_key(id), String::new).set(String::new());
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
    use_signal(&scroll_offset_y_signal_key("kbd-scroll-main"), || 0.0_f32).set(0.0);

    // Gallery Kbd Search lives *inside* the page ScrollArea. Scroll a few px so the
    // field stays fully visible — clip-cull must not be the failure mode.
    let mut root = Element::new()
        .w(400.0)
        .h(320.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .child(
            scroll_area("kbd-scroll-main").w("100%").h("100%").child(
                vstack()
                    .w("100%")
                    .gap(12.0)
                    .child(
                        input_adornment()
                            .child(input(id).placeholder("Search…").plain_chrome(true))
                            .addon("inline-end", Element::new().text("⌘K")),
                    )
                    .child(Element::new().w("100%").h(480.0).bg([0.9, 0.9, 0.91, 1.0])),
            ),
        );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 400.0, 320.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);

    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 320.0, [0.0, 0.0]);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    crate::renderer::restart_caret_blink();
    let focused = tree.build_and_layout_fast(&mut root, 400.0, 320.0, [-1.0, -1.0], false, false);
    assert!(
        !focused.input_chrome_instances.is_empty(),
        "precondition: focused Search must collect caret chrome (blink ON)"
    );

    // Page scroll while the field keeps focus (wheel / trackpad). 12px keeps the
    // ~36px Search fully on-screen so empty chrome is skip/fingerprint, not cull.
    use_signal(&scroll_offset_y_signal_key("kbd-scroll-main"), || 0.0_f32).set(12.0);
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 320.0, [0.0, 0.0]);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    crate::renderer::restart_caret_blink();
    let after_scroll =
        tree.build_and_layout_fast(&mut root, 400.0, 320.0, [-1.0, -1.0], false, false);
    assert!(
        after_scroll
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "scroll must keep input_caret on the focused leaf"
    );
    assert!(
        !after_scroll.input_chrome_instances.is_empty(),
        "图二: focused Search must keep caret chrome across scroll; \
         fingerprint skip replayed an unfocused DL (no hairline)"
    );
    assert!(
        after_scroll
            .node_paints
            .iter()
            .any(|p| p.chrome.0 < p.chrome.1),
        "some collect slot must still own the caret chrome range after scroll"
    );
    assert!(
        after_scroll.instances.iter().any(|i| {
            i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05 && i.shadow_blur.abs() < 0.5
        }),
        "图二: focused adornment Hard ring must survive page scroll (not fall back to 图四 default)"
    );
}

/// Portal overlay_split is a walk-time cursor, not part of the cached DL.
/// Skipping an ancestor of `__jadix_portal_layer` splices rects/glyphs but leaves
/// `overlay_split` unmarked → finish_collect parks the split at `len` (empty
/// overlay suffix). Present then dest-resets the dialog hole with no overlay
/// redraw: first frame looks right, later frames lose scrim + panel fill.
fn page_under_portal_overlay() -> Element {
    let page = Element::new()
        .w(400.0)
        .h(300.0)
        .bg([0.96, 0.96, 0.96, 1.0])
        .child(Element::new().text("SheetSlide · Builder"));
    let mut overlay = Element::new()
        .w(400.0)
        .h(300.0)
        .bg([0.0, 0.0, 0.0, 0.28])
        .backdrop_blur(2.0)
        .child(
            Element::new()
                .w(200.0)
                .h(80.0)
                .bg([1.0, 1.0, 1.0, 1.0])
                .rounded(12.0)
                .child(Element::new().text("ViewportModal · RSX")),
        );
    overlay.set_attr("jadix_portal", "1");
    overlay.set_attr(ATTR_PORTAL_KEY, "vm-scrim");
    Element::new().w(400.0).h(300.0).child(page).child(overlay)
}

fn assert_portal_overlay_split_keeps_chrome(result: &crate::layout::layout_tree::RenderResult) {
    assert!(
        result.overlay_split.instance_start > 0
            && result.overlay_split.instance_start < result.instances.len(),
        "portal layer must split after page instances (not 0 = all overlay, not len = empty overlay); \
         start={} n={}",
        result.overlay_split.instance_start,
        result.instances.len()
    );
    let split_t = result.overlay_split.text_start.min(result.text_nodes.len());
    assert!(
        result.text_nodes[..split_t]
            .iter()
            .any(|t| t.text.contains("SheetSlide")),
        "page label must collect before the overlay split; texts={:?}",
        result
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    let overlay_inst = &result.instances[result.overlay_split.instance_start..];
    assert!(
        overlay_inst
            .iter()
            .any(|i| i.rect_size[0] > 300.0 && i.color[3] > 0.05),
        "scrim missing in overlay suffix; overlay={:?}",
        overlay_inst
            .iter()
            .map(|i| (i.rect_size, i.color[3]))
            .collect::<Vec<_>>()
    );
    assert!(
        overlay_inst
            .iter()
            .any(|i| i.rect_size[0] > 100.0 && i.rect_size[0] < 280.0 && i.color[3] > 0.5),
        "opaque panel fill missing in overlay suffix; overlay={:?}",
        overlay_inst
            .iter()
            .map(|i| (i.rect_size, i.color[3]))
            .collect::<Vec<_>>()
    );
}

#[test]
fn paint_only_skip_must_not_drop_portal_overlay_split() {
    reset_collect_skip_counters();
    let mut root = page_under_portal_overlay();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
    assert_portal_overlay_split_keeps_chrome(&first);

    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 400.0, 300.0, [-1.0, -1.0], false, false);
    assert_portal_overlay_split_keeps_chrome(&warmed);

    let skipped = tree.build_and_layout_fast(&mut root, 400.0, 300.0, [-1.0, -1.0], false, false);
    assert_portal_overlay_split_keeps_chrome(&skipped);
    assert!(
        skipped.overlay_split.is_marked(),
        "production hoist parks the split on `__jadix_portal_layer`; skip must not leave it unmarked"
    );
    assert_eq!(
        root.children.last().and_then(|c| c.template_id.as_deref()),
        Some(PORTAL_LAYER_ID),
        "build_and_layout must hoist the overlay onto the body portal layer"
    );
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "page sibling must still fingerprint-replay; only portal-containing ancestors stay live"
    );
    let split_t = skipped
        .overlay_split
        .text_start
        .min(skipped.text_nodes.len());
    assert!(
        skipped.text_nodes[split_t..]
            .iter()
            .any(|t| t.text.contains("ViewportModal")),
        "dialog title must stay in the overlay text suffix after skip replay"
    );
}

/// Portal-layer child bands (`overlay_split.bands`) are walk-time, like
/// `overlay_split` itself. Skipping the layer would splice both panels' DLs as one
/// blob and present would paint all overlay rects before all overlay glyphs.
fn page_with_stacked_portals() -> Element {
    let page = Element::new()
        .w(200.0)
        .h(80.0)
        .bg([0.95, 0.95, 0.95, 1.0])
        .child(Element::new().text("page"));
    let mut lower = Element::new()
        .bg([1.0, 1.0, 1.0, 1.0])
        .w(160.0)
        .h(80.0)
        .absolute()
        .left(10.0)
        .top(20.0)
        .z_index(1000)
        .child(Element::new().text("lower-glyph"));
    lower.set_attr("jadix_portal", "1");
    lower.set_attr(ATTR_PORTAL_KEY, "panel:lower");
    let mut upper = Element::new()
        .bg([0.9, 0.9, 1.0, 1.0])
        .w(160.0)
        .h(80.0)
        .absolute()
        .left(40.0)
        .top(40.0)
        .z_index(1010)
        .child(Element::new().text("upper-glyph"));
    upper.set_attr("jadix_portal", "1");
    upper.set_attr(ATTR_PORTAL_KEY, "panel:upper");
    Element::new()
        .w(400.0)
        .h(200.0)
        .child(page)
        .child(lower)
        .child(upper)
}

fn assert_stacked_portal_bands(result: &crate::layout::layout_tree::RenderResult) {
    assert!(
        result.overlay_split.is_marked(),
        "stacked portals must mark the overlay split"
    );
    assert!(
        result.overlay_split.bands.len() >= 2,
        "expected ≥2 portal bands after skip, got {}",
        result.overlay_split.bands.len()
    );
    let ranges = result.overlay_split.band_ranges(
        result.instances.len(),
        result.textured_draws.len(),
        result.text_nodes.len(),
        result.input_chrome_instances.len(),
        result.paint_ops.len(),
    );
    assert!(ranges.len() >= 2);
    let a = ranges[ranges.len() - 2];
    let b = ranges[ranges.len() - 1];
    assert!(
        result.text_nodes[a.text.0..a.text.1]
            .iter()
            .any(|t| t.text == "lower-glyph"),
        "lower portal glyph must stay in its band; texts={:?}",
        result
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        result.text_nodes[b.text.0..b.text.1]
            .iter()
            .any(|t| t.text == "upper-glyph"),
        "upper portal glyph must stay in the last band"
    );
}

#[test]
fn paint_only_skip_must_not_drop_stacked_portal_bands() {
    reset_collect_skip_counters();
    let mut root = page_with_stacked_portals();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert_stacked_portal_bands(&first);

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    let skipped = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    assert_stacked_portal_bands(&skipped);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "page sibling must still fingerprint-replay; portal layer stays live"
    );
}

const DRAG_CHIP: [f32; 4] = [1.0, 0.45, 0.0, 1.0];

fn page_with_drag_overlay() -> Element {
    let page = Element::new()
        .w(160.0)
        .h(80.0)
        .bg([0.2, 0.2, 0.8, 1.0])
        .child(Element::new().text("page"));
    let mut ghost = Element::new()
        .w(40.0)
        .h(24.0)
        .bg(DRAG_CHIP)
        .child(Element::new().w(1.0).h(1.0));
    ghost.set_attr(crate::utils::drag::ATTR_DRAG_OVERLAY, "1");
    Element::new().w(160.0).h(80.0).child(page).child(ghost)
}

fn drag_chip_instance(
    result: &crate::layout::layout_tree::RenderResult,
) -> &crate::renderer::InstanceInput {
    result
        .instances
        .iter()
        .find(|i| {
            (i.color[0] - DRAG_CHIP[0]).abs() < 0.02
                && (i.color[1] - DRAG_CHIP[1]).abs() < 0.05
                && i.rect_size[0] >= 20.0
        })
        .expect("drag overlay chip")
}

/// Drag overlay is a collect side channel (not stored in the fingerprint DL).
/// Skipping an ancestor freezes the ghost at last-frame `overlay_pos` / translate.
#[test]
fn paint_only_skip_must_not_freeze_drag_overlay() {
    reset_collect_skip_counters();
    let mut root = page_with_drag_overlay();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    let _ = drag_chip_instance(&first);

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    let skipped = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "page sibling must still fingerprint-replay; only the drag overlay stays live"
    );
    assert!(
        root.children[1].collect_skip_side_channel && !root.children[1].collect_skip_live,
        "drag overlay must recollect its side channel without making idle descendants live"
    );
    let _ = drag_chip_instance(&skipped);

    root.children[1].style.bg_color = [0.1, 0.8, 0.2, 1.0];
    let moved = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], false, false);
    assert!(
        moved
            .instances
            .iter()
            .any(|i| (i.color[1] - 0.8).abs() < 0.08 && i.rect_size[0] >= 20.0),
        "paint-only skip must recollect the drag ghost after a paint change"
    );
    assert!(
        !moved.instances.iter().any(|i| {
            (i.color[0] - DRAG_CHIP[0]).abs() < 0.02
                && (i.color[1] - DRAG_CHIP[1]).abs() < 0.05
                && i.rect_size[0] >= 20.0
        }),
        "must not replay the rest-pose drag chip color"
    );
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "updating the ghost must not drop skip of the clean page sibling"
    );
}

fn assert_page_stack_keeps_high_glyph(
    result: &crate::layout::layout_tree::RenderResult,
    high: &str,
) {
    assert!(
        result.overlay_split.page_stack_bands.len() >= 2,
        "page_stack_bands are a walk-time cursor; skip of the stacking parent parks them empty \
         (all rects then all text → glyph bleed). got {}",
        result.overlay_split.page_stack_bands.len()
    );
    let ranges = result.overlay_split.page_stack_band_ranges();
    let last = ranges.last().expect("band ranges");
    assert!(
        result.text_nodes[last.text.0..last.text.1]
            .iter()
            .any(|t| t.text == high),
        "higher z text must stay in the last page-stack band after skip; texts={:?}",
        result
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
}

fn page_beside_stacking_pile(attr: bool) -> Element {
    let sibling = Element::new()
        .w(80.0)
        .h(80.0)
        .bg([0.2, 0.8, 0.2, 1.0])
        .child(Element::new().text("left"));
    let lower = Element::new()
        .bg([1.0, 0.0, 0.0, 1.0])
        .w(120.0)
        .h(80.0)
        .absolute()
        .left(10.0)
        .top(10.0)
        .z_index(1)
        .child(Element::new().text("stack-low"));
    let upper = Element::new()
        .bg([0.0, 0.0, 1.0, 1.0])
        .w(120.0)
        .h(80.0)
        .absolute()
        .left(40.0)
        .top(30.0)
        .z_index(2)
        .child(Element::new().text("stack-high"));
    let mut pile = Element::new()
        .w(200.0)
        .h(120.0)
        .relative()
        .child(lower)
        .child(upper);
    if attr {
        pile.attrs
            .insert("data-stacking-context".to_string(), "1".to_string());
    }
    Element::new()
        .flex_row()
        .w(400.0)
        .h(200.0)
        .child(sibling)
        .child(pile)
}

/// `page_stack_bands` are not in the fingerprint DL. Skipping a stacking-context
/// parent splices rects/glyphs but leaves bands empty — present then paints every
/// page glyph after every card fill (CarouselStack bleed).
#[test]
fn paint_only_skip_must_not_drop_page_stack_bands() {
    reset_collect_skip_counters();
    let mut root = page_beside_stacking_pile(true);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert_page_stack_keeps_high_glyph(&first, "stack-high");

    tree.set_collect_skip_allowed(true);
    let warmed = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    assert_page_stack_keeps_high_glyph(&warmed, "stack-high");

    let skipped = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    assert_page_stack_keeps_high_glyph(&skipped, "stack-high");
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "left sibling must still fingerprint-replay; only the stacking parent stays live"
    );
    assert!(
        root.children[1].collect_skip_side_channel && !root.children[1].collect_skip_live,
        "stacking bands must recollect without making idle descendants live"
    );
}

/// Toast-style pile: z-ordered cards, no `data-stacking-context`. Collect still
/// auto-opens page_stack_bands; skip must not freeze that cursor.
#[test]
fn paint_only_skip_must_not_drop_z_index_page_stack_bands() {
    reset_collect_skip_counters();
    let mut root = page_beside_stacking_pile(false);
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 400.0, 200.0, [0.0, 0.0]);
    assert_page_stack_keeps_high_glyph(&first, "stack-high");

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    let skipped = tree.build_and_layout_fast(&mut root, 400.0, 200.0, [0.0, 0.0], false, false);
    assert_page_stack_keeps_high_glyph(&skipped, "stack-high");
    assert!(
        tree.last_collect_skip_counts().0 >= 1,
        "left sibling must still fingerprint-replay; only the z-index parent stays live"
    );
    assert!(
        root.children[1].collect_skip_side_channel && !root.children[1].collect_skip_live,
        "z-index stacking bands must recollect without making idle descendants live"
    );
}

/// Breadcrumb / Link `hover:underline` + caption fg are paint-only (`StyleChanged`).
/// ViewportsOnly `reuse_idle` must not skip-replay the rest glyphs.
#[test]
fn scroll_only_must_recollect_link_hover_underline_and_fg() {
    use crate::components::button::{button, sync_button_hover_paint_subtree};
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutStatus;
    use crate::style::parse_text_color;

    reset_collect_skip_counters();
    let muted = parse_text_color("muted-foreground");
    let fg = parse_text_color("foreground");
    let (_h, link) = create_root(|| -> Element {
        button("skip-link-ul")
            .variant("link")
            .size("inline")
            .text_color(muted)
            .class("underline-offset-[2px]")
            .text("Home")
            .into()
    });
    let idle = Element::new()
        .w(80.0)
        .h(40.0)
        .flex_shrink(0.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .child(Element::new().text("idle"));
    let mut root = Element::new()
        .flex_row()
        .w(240.0)
        .h(40.0)
        .child(idle)
        .child(link);

    let mut tree = LayoutTree::new();
    let rest = tree.build_and_layout(&mut root, 240.0, 40.0, [-1.0, -1.0]);
    let rest_home = rest
        .text_nodes
        .iter()
        .find(|t| t.text.contains("Home"))
        .expect("Home at rest");
    assert_eq!(rest_home.style.color_resolved(), muted);
    assert!(
        !rest_home.style.underline_resolved(),
        "rest Link must not paint underline"
    );

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 240.0, 40.0, [-1.0, -1.0], true, false);

    let (mouse, on_hover) = {
        let link = &root.children[1];
        let b = link.layout_bounds;
        (
            [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5],
            link.on_hover.clone(),
        )
    };
    if let Some(hover) = on_hover {
        hover.borrow_mut()(true);
    }
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(
        &mut root.children[1],
        &mut tree,
        &mut impact,
        240.0,
        40.0,
        [0.0, 0.0],
    );
    sync_class_variants_subtree(
        &mut root,
        mouse,
        "",
        false,
        240.0,
        40.0,
        &mut tree,
        &mut impact,
    );

    let hovered = tree.build_and_layout_fast(&mut root, 240.0, 40.0, mouse, true, false);
    let home = hovered
        .text_nodes
        .iter()
        .find(|t| t.text.contains("Home"))
        .expect("Home after hover");
    assert_eq!(
        home.style.color_resolved(),
        fg,
        "scroll_only collect skip must not keep rest muted fg; got {:?}",
        home.style.color
    );
    assert!(
        home.style.underline_resolved(),
        "scroll_only collect skip must not drop hover:underline"
    );
}

/// Hover node-present replaces the scissor with the border-box dest-reset.
/// Breadcrumb `underline-offset` paints below that box, so the line is clipped
/// until a scroll FullClear. The decoration hole must contain it, and leave
/// must still cover the previous quad.
#[test]
fn breadcrumb_hover_underline_escapes_border_box_until_decoration_hole() {
    use crate::app::draw_cull::{expand_scissor_for_text_decoration, node_present_reset_rect};
    use crate::app::tree_hit::clear_hover_tracking;
    use crate::components::breadcrumb::breadcrumb;
    use crate::components::button::sync_button_hover_paint_subtree;
    use crate::core::reactivity::create_root;
    use crate::renderer::prepare_text::text_decoration_ink_logical;
    use crate::renderer::InstanceInput;
    use bytemuck::Zeroable;

    clear_hover_tracking();
    let (_h, mut root) = create_root(|| {
        Element::new().w(320.0).h(80.0).child(
            breadcrumb()
                .link("Home", || {})
                .link("Docs", || {})
                .page("Current"),
        )
    });
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 320.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 320.0, 80.0, [-1.0, -1.0], false, false);

    fn find_link<'a>(el: &'a Element, label: &str) -> Option<&'a Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some("breadcrumb-link")
            && el.children.iter().any(|c| c.text.as_deref() == Some(label))
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_link(c, label))
    }
    let mouse = {
        let link = find_link(&root, "Home").expect("Home link");
        let b = link.layout_bounds;
        [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
    };
    if let Some(hover) = find_link(&root, "Home").and_then(|l| l.on_hover.clone()) {
        hover.borrow_mut()(true);
    }
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, 320.0, 80.0, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut root,
        mouse,
        "",
        false,
        320.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    let frame = tree.build_and_layout_fast(&mut root, 320.0, 80.0, mouse, false, false);
    let idx = frame
        .text_nodes
        .iter()
        .position(|t| t.text == "Home")
        .expect("Home text");
    let home = &frame.text_nodes[idx];
    assert!(
        home.style.underline_resolved(),
        "hover collect must carry underline"
    );
    let ink = text_decoration_ink_logical(home, 1.0).expect("decoration ink");
    let link_bounds = find_link(&root, "Home").expect("link").layout_bounds;
    let slot = frame
        .node_paints
        .iter()
        .find(|p| {
            (p.bounds[0] - link_bounds[0]).abs() < 2.0
                && (p.bounds[1] - link_bounds[1]).abs() < 2.0
                && (p.bounds[2] - link_bounds[2]).abs() < 2.0
                && (p.bounds[3] - link_bounds[3]).abs() < 2.0
        })
        .expect("button slot");
    let textured: &[crate::renderer::ScaledTexturedDraw] = &[];
    let hole = node_present_reset_rect(
        slot.key,
        slot.bounds,
        &frame.node_paints,
        &frame.instances,
        textured,
        &frame.input_chrome_instances,
        3.0,
        1.0,
        None,
    )
    .unwrap_or(slot.bounds);
    let ink_bottom = ink[1] + ink[3];
    let hole_bottom = hole[1] + hole[3];
    assert!(
        ink_bottom > hole_bottom + 0.5,
        "underline must stick out of the border-box wipe ink={ink:?} hole={hole:?} text_box=[{}, {}, {}, {}] offset={}",
        home.x,
        home.y,
        home.width,
        home.height,
        home.style.underline_offset_resolved()
    );
    let expanded = expand_scissor_for_text_decoration(
        hole,
        slot.key,
        slot.bounds,
        &frame.node_paints,
        &frame.text_nodes,
        &[],
        1.0,
    );
    assert!(
        ink_bottom <= expanded[1] + expanded[3] + 0.5,
        "decoration hole must cover the underline ink={ink:?} expanded={expanded:?}"
    );

    let mut left = frame.text_nodes.clone();
    left[idx].style.underline = Some(false);
    let mut prev = InstanceInput::zeroed();
    prev.rect_pos = [ink[0], ink[1]];
    prev.rect_size = [ink[2], ink[3]];
    let leave = expand_scissor_for_text_decoration(
        hole,
        slot.key,
        slot.bounds,
        &frame.node_paints,
        &left,
        &[prev],
        1.0,
    );
    assert!(
        ink_bottom <= leave[1] + leave[3] + 0.5,
        "leave must still cover the previous underline leave={leave:?}"
    );

    let mut far = prev;
    far.rect_pos[1] = slot.bounds[1] + slot.bounds[3] + 40.0;
    let isolated = expand_scissor_for_text_decoration(
        hole,
        slot.key,
        slot.bounds,
        &frame.node_paints,
        &left,
        &[far],
        1.0,
    );
    assert!(
        (isolated[1] + isolated[3] - hole_bottom).abs() < 0.5,
        "a line 40px below belongs to the next row, not this slot isolated={isolated:?} hole={hole:?}"
    );
    clear_hover_tracking();
}

fn instance_has_chip_color(
    result: &crate::layout::layout_tree::RenderResult,
    color: [f32; 4],
) -> bool {
    result.instances.iter().any(|i| {
        i.rect_size[0] > 20.0
            && i.rect_size[1] > 20.0
            && (0..4).all(|c| (i.color[c] - color[c]).abs() < 0.06)
    })
}

/// Chrome hover runs before collect. ViewportsOnly `reuse_idle` must not
/// skip-replay `hover:bg` after freeze leave (same order as AppState chrome).
#[test]
fn scroll_only_skip_replay_must_not_keep_hover_bg_after_pointer_miss() {
    use crate::core::tree::LayoutStatus;
    use crate::style::{apply_class_list, parse_color, tick_active_css_transitions_subtree};

    reset_collect_skip_counters();
    let hover_bg = parse_color("muted");
    let mut chip = Element::new().w(80.0).h(40.0).flex_shrink(0.0);
    apply_class_list(&mut chip, "bg-transparent hover:bg-muted");
    let rest_bg = chip.style.bg_color;
    let filler = Element::new()
        .w(80.0)
        .h(200.0)
        .flex_shrink(0.0)
        .bg([0.2, 0.2, 0.22, 1.0]);
    let mut root = Element::new()
        .flex_col()
        .w(80.0)
        .h(80.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(chip)
        .child(filler);
    root.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}hover-leave-scroll"));

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 80.0, [-1.0, -1.0], true, false);

    let mouse = {
        let b = root.children[0].layout_bounds;
        [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
    };
    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        mouse,
        "",
        false,
        80.0,
        80.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "stationary pointer over the chip must enter hover"
    );
    let hovered = tree.build_and_layout_fast(&mut root, 80.0, 80.0, mouse, true, false);
    assert!(
        instance_has_chip_color(&hovered, hover_bg),
        "hovered chip must collect hover:bg; instances={:?}",
        hovered
            .instances
            .iter()
            .map(|i| i.color)
            .collect::<Vec<_>>()
    );

    // Same order as render chrome: live offset, then freeze leave, then collect.
    root.scroll_offset[1] = 30.0;
    use_signal(&scroll_offset_y_signal_key("hover-leave-scroll"), || {
        0.0_f32
    })
    .set_silent(30.0);
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        mouse,
        "",
        false,
        80.0,
        80.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "content sliding out from under a still pointer must leave hover in chrome"
    );
    assert_eq!(root.children[0].style.bg_color, rest_bg);
    let left = tree.build_and_layout_fast(&mut root, 80.0, 80.0, mouse, true, false);
    assert!(
        !instance_has_chip_color(&left, hover_bg),
        "scroll_only skip-replay must not keep hover:bg after pointer miss; instances={:?}",
        left.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
}

/// Gallery Buttons use CVA `hover:bg-*`. Smooth-scroll chrome is freeze-path
/// `tick_active` then collect — `on_hover` / `{id}_hover` lag one frame behind.
/// Leave must restore rest fill on that same collect, not wait for settle.
#[test]
fn scroll_only_button_hover_enter_then_leave_must_restore_bg() {
    use crate::components::button::{
        button, button_chrome_for_variant, sync_button_hover_paint_subtree,
    };
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutStatus;
    use crate::style::tick_active_css_transitions_subtree;

    fn colors_near(a: [f32; 4], b: [f32; 4]) -> bool {
        (0..4).all(|i| (a[i] - b[i]).abs() < 0.02)
    }

    reset_collect_skip_counters();
    crate::style::set_theme_mode(crate::style::ThemeMode::Light);
    let (rest_bg, _, _) = button_chrome_for_variant("default", 0.0);
    let (hover_bg, _, _) = button_chrome_for_variant("default", 1.0);
    let (_h, btn) = create_root(|| -> Element {
        button("skip-btn-hover-leave")
            .variant("default")
            .text("Go")
            .into()
    });
    let filler = Element::new()
        .w(80.0)
        .h(200.0)
        .flex_shrink(0.0)
        .bg([1.0, 0.0, 0.0, 1.0]);
    let mut root = Element::new()
        .flex_col()
        .w(80.0)
        .h(80.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(btn)
        .child(filler);
    root.focus_id = Some(format!(
        "{SCROLL_VIEWPORT_FOCUS_PREFIX}btn-hover-leave-scroll"
    ));

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 80.0, [-1.0, -1.0], true, false);

    let mouse = {
        let b = root.children[0].layout_bounds;
        [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5]
    };
    crate::app::tree_hit::clear_hover_tracking();
    crate::app::tree_hit::apply_pointer_hover_reconcile(&root, mouse[0], mouse[1], false);
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, 80.0, 80.0, [0.0, 0.0]);
    tick_active_css_transitions_subtree(
        &mut root,
        mouse,
        "",
        false,
        80.0,
        80.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(true),
        "stationary pointer over the button must enter hover"
    );
    assert!(
        colors_near(root.children[0].style.bg_color, hover_bg),
        "freeze hover enter must overlay button hover:bg, got {:?}",
        root.children[0].style.bg_color
    );
    let hovered = tree.build_and_layout_fast(&mut root, 80.0, 80.0, mouse, true, false);
    assert!(
        instance_has_chip_color(&hovered, hover_bg),
        "hovered button must collect hover:bg; instances={:?}",
        hovered
            .instances
            .iter()
            .map(|i| i.color)
            .collect::<Vec<_>>()
    );

    // Production leave frame: `on_hover` (`{id}_hover`) and CSS `:hover` leave
    // together before chrome — same mouse, immediate miss under still pointer.
    root.scroll_offset[1] = 30.0;
    use_signal(
        &scroll_offset_y_signal_key("btn-hover-leave-scroll"),
        || 0.0_f32,
    )
    .set_silent(30.0);
    crate::app::tree_hit::apply_pointer_hover_reconcile(&root, mouse[0], mouse[1], true);
    impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut root, &mut tree, &mut impact, 80.0, 80.0, [0.0, 0.0]);
    tick_active_css_transitions_subtree(
        &mut root,
        mouse,
        "",
        false,
        80.0,
        80.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    assert_eq!(
        root.children[0]
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_pointer_hover),
        Some(false),
        "content sliding out from under a still pointer must leave button hover in chrome"
    );
    assert!(
        colors_near(root.children[0].style.bg_color, rest_bg),
        "freeze hover leave must restore button rest bg, got {:?}",
        root.children[0].style.bg_color
    );
    let left = tree.build_and_layout_fast(&mut root, 80.0, 80.0, mouse, true, false);
    assert!(
        !instance_has_chip_color(&left, hover_bg),
        "scroll_only skip-replay must not keep button hover:bg after pointer miss; instances={:?}",
        left.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
}

/// Gallery Item `variant={sig.get()}` is paint-only. Clicking a still-hovered
/// button keeps ViewportsOnly `reuse_idle`; idle ancestors must not skip-replay
/// the previous variant until pointer-leave hash-busts the whole tree.
#[test]
fn scroll_only_must_recollect_item_variant_bind_without_pointer_leave() {
    use crate::components::{apply_bound_item_variant, apply_item_paint, ItemSize, ItemVariant};
    use crate::layout::collect_cache::invalidate_collect_paint_hash_path;

    reset_collect_skip_counters();
    let mut item = Element::new().w(200.0).h(40.0).flex_shrink(0.0);
    assert_eq!(
        apply_item_paint(&mut item, ItemVariant::Outline, ItemSize::Default),
        LayoutStatus::StyleChanged
    );
    let outline_bg = item.style.bg_color;
    let idle = Element::new()
        .w(200.0)
        .h(40.0)
        .flex_shrink(0.0)
        .bg([1.0, 0.0, 0.0, 1.0]);
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .h(80.0)
        .child(item)
        .child(idle);

    let mut tree = LayoutTree::new();
    let rest = tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
    assert!(
        instance_has_chip_color(&rest, outline_bg),
        "outline Item must paint its background; instances={:?}",
        rest.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], true, false);

    assert_eq!(
        apply_bound_item_variant(&mut root.children[0], "muted"),
        LayoutStatus::StyleChanged
    );
    let muted_bg = root.children[0].style.bg_color;
    assert!(
        (0..4).any(|c| (muted_bg[c] - outline_bg[c]).abs() > 0.05),
        "muted chrome must differ from outline"
    );

    let stale = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        instance_has_chip_color(&stale, outline_bg),
        "reuse_idle without a hash-path bust skip-replays outline; instances={:?}",
        stale.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
    assert!(
        !instance_has_chip_color(&stale, muted_bg),
        "muted must not appear until the bind hash path is invalidated"
    );

    invalidate_collect_paint_hash_path(&mut root, &[0]);
    let fresh = tree.build_and_layout_fast(&mut root, 200.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        instance_has_chip_color(&fresh, muted_bg),
        "bind StyleChanged must recollect Item variant without pointer leave; instances={:?}",
        fresh.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
    assert!(
        !instance_has_chip_color(&fresh, outline_bg),
        "outline fill must not remain after variant bind; instances={:?}",
        fresh.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
}

/// Gallery Item outline→default: `border-transparent` must not emit a 1px SDF
/// stroke, title stays `foreground`, and dest-reset must restore the rounded
/// parent card (otherwise PartialLoad keeps the outline ring and darkens glyphs).
#[test]
fn item_outline_to_default_must_drop_border_and_keep_title_foreground() {
    use crate::components::{apply_bound_item_variant, apply_item_paint, ItemSize, ItemVariant};
    use crate::layout::collect_cache::invalidate_collect_paint_hash_path;
    use crate::style::parse_text_color;

    reset_collect_skip_counters();
    let fg = parse_text_color("foreground");
    let mut item = Element::new().w(200.0).h(48.0).flex_shrink(0.0).child(
        Element::new()
            .text("Reactive variant: outline")
            .text_color(fg),
    );
    assert_eq!(
        apply_item_paint(&mut item, ItemVariant::Outline, ItemSize::Default),
        LayoutStatus::StyleChanged
    );
    let outline_bg = item.style.bg_color;
    let outline_border = item.style.border_color;
    assert!(outline_border[3] > 0.5, "outline border must be opaque");
    // Distinct from Item outline `background` so leftover fill is not confused
    // with the parent card.
    let card_bg = [1.0, 0.0, 0.0, 1.0];
    let mut root = Element::new()
        .flex_col()
        .w(220.0)
        .h(80.0)
        .bg(card_bg)
        .rounded(8.0)
        .child(item);

    let mut tree = LayoutTree::new();
    let rest = tree.build_and_layout(&mut root, 220.0, 80.0, [-1.0, -1.0]);
    assert!(
        instance_has_chip_color(&rest, outline_bg),
        "outline Item must paint its background"
    );
    assert!(
        rest.instances.iter().any(|i| {
            i.rect_size[0] > 100.0
                && i.rect_size[1] > 20.0
                && i.border_width > 0.5
                && i.border_color[3] > 0.5
        }),
        "outline Item must emit a visible 1px border; instances={:?}",
        rest.instances
            .iter()
            .map(|i| (i.color, i.border_color, i.border_width))
            .collect::<Vec<_>>()
    );

    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 220.0, 80.0, [-1.0, -1.0], true, false);

    assert_eq!(
        apply_bound_item_variant(&mut root.children[0], "default"),
        LayoutStatus::StyleChanged
    );
    root.children[0].children[0].text = Some("Reactive variant: default".into());
    // Production variant bind only names the Item path; descendants (title) must
    // still rehash or skip-replay keeps "outline" glyphs stacked on the new frame.
    invalidate_collect_paint_hash_path(&mut root, &[0]);
    let fresh = tree.build_and_layout_fast(&mut root, 220.0, 80.0, [-1.0, -1.0], true, false);

    assert!(
        !instance_has_chip_color(&fresh, outline_bg),
        "outline fill must not remain after default bind; instances={:?}",
        fresh.instances.iter().map(|i| i.color).collect::<Vec<_>>()
    );
    let item_strokes: Vec<_> = fresh
        .instances
        .iter()
        .filter(|i| i.rect_size[0] > 100.0 && i.rect_size[1] > 20.0 && i.border_width > 0.5)
        .map(|i| (i.color, i.border_color, i.border_width))
        .collect();
    assert!(
        item_strokes.is_empty(),
        "default Item (border-transparent) must not emit a 1px SDF stroke; got {item_strokes:?}"
    );
    let title = fresh
        .text_nodes
        .iter()
        .find(|t| t.text.contains("default"))
        .expect("recollected title");
    assert!(
        !fresh.text_nodes.iter().any(|t| t.text.contains("outline")),
        "stale 'outline' glyphs must not remain in the display list; got {:?}",
        fresh
            .text_nodes
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        title.style.color_resolved(),
        fg,
        "title must stay foreground after outline→default, got {:?}",
        title.style.color_resolved()
    );

    let item_key = u64::from(root.children[0].layout_node.expect("item taffy"));
    let item_bounds = root.children[0].layout_bounds;
    let reset = crate::app::draw_cull::node_present_dest_reset_full(
        item_key,
        item_bounds,
        &fresh.node_paints,
        &fresh.instances,
        &[],
        &[],
        1.0,
        Some(item_bounds),
    )
    .expect(
        "outline→default dest-reset must wipe the old ring and restore the parent card; \
         None leaves outline pixels in PartialLoad retain",
    );
    assert!(
        (reset.clear[0] - card_bg[0]).abs() < 0.06 && reset.clear[3] > 0.5
            || reset.replay.1 > reset.replay.0,
        "underlay restore must use the parent card; clear={:?} replay={:?}",
        reset.clear,
        reset.replay
    );

    let title_el = &root.children[0].children[0];
    let title_key = u64::from(title_el.layout_node.expect("title taffy"));
    let title_bounds = title_el.layout_bounds;
    let item_dirty = crate::app::damage::PaintDirty {
        key: item_key,
        bounds: item_bounds,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let title_dirty = crate::app::damage::PaintDirty {
        key: title_key,
        bounds: title_bounds,
        kind: crate::app::damage::PaintDirtyKind::Node,
    };
    let dirties = [item_dirty, title_dirty];
    assert!(
        !crate::app::draw_cull::paint_dirty_skips_node_dest_reset_covered(
            &title_dirty,
            &dirties,
            false,
            &fresh.node_paints,
            &fresh.instances,
            1.0,
        ),
        "transparent default Item must not nested-skip the title dest-reset"
    );
    assert!(
        crate::app::draw_cull::node_present_dest_reset_full(
            title_key,
            title_bounds,
            &fresh.node_paints,
            &fresh.instances,
            &[],
            &[],
            1.0,
            Some(title_bounds),
        )
        .is_some(),
        "title dest-reset must wipe leftover outline glyphs on the parent card"
    );
}

/// C5: absolute later-sibling opacity must recollect under `reuse_idle`
/// without a product `data-slot`.
#[test]
fn out_of_flow_page_chrome_opacity_must_recollect_without_product_slot() {
    reset_collect_skip_counters();
    let mut pill = Element::new().w(80.0).h(32.0).bg([1.0, 0.2, 0.2, 1.0]);
    pill.style.opacity = 0.0;
    let mut dock = Element::new().w(160.0).h(32.0);
    dock.style.position = taffy::style::Position::Absolute;
    dock = dock.child(pill);
    let mut root = Element::new()
        .w(160.0)
        .h(80.0)
        .flex_col()
        .child(Element::new().w(160.0).h(80.0).bg([0.1, 0.1, 0.2, 1.0]))
        .child(dock);

    let mut tree = LayoutTree::new();
    let _first = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let hidden = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        hidden
            .instances
            .iter()
            .all(|i| i.color[0] < 0.9 || i.color[3] < 0.05),
        "opacity 0 pill must not emit an opaque red fill"
    );

    root.children[1].children[0].style.opacity = 1.0;
    crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &[1, 0]);
    let shown = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        shown
            .instances
            .iter()
            .any(|i| i.color[0] > 0.9 && i.color[3] > 0.9),
        "out-of-flow opacity 0→1 must recollect without a product slot"
    );
}

/// Gallery 基础: Marker/Message card is taller than the page clip. After scroll the
/// card still straddles while `1c-ms)` sits at the live edge — at the same visual Y
/// Thinking occupied at rest. Skip-replay at rest Y (or an incomplete parent DL)
/// stamps those internals onto the next card until the whole region leaves the clip.
#[test]
fn straddling_card_internals_must_not_paint_at_rest_y() {
    reset_collect_skip_counters();
    let scroll_id = "straddle-msg-card";
    let mut thinking = Element::new()
        .w(200.0)
        .h(40.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Thinking..."));
    thinking.set_attr("data-slot", "thinking");
    thinking.set_attr(crate::renderer::ATTR_GLYPH_FX, "shimmer");
    let mut border = Element::new()
        .w(200.0)
        .h(40.0)
        .flex_shrink(0.0)
        .child(Element::new().text("Border"));
    border.set_attr("data-slot", "border");
    let card = Element::new()
        .flex_col()
        .w(200.0)
        .flex_shrink(0.0)
        .child(
            Element::new()
                .w(200.0)
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("Row0")),
        )
        .child(
            Element::new()
                .w(200.0)
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("Row1")),
        )
        .child(thinking)
        .child(
            Element::new()
                .w(200.0)
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("Row3")),
        )
        .child(border)
        .child(
            Element::new()
                .w(200.0)
                .h(40.0)
                .flex_shrink(0.0)
                .child(Element::new().text("Reactive")),
        );
    let mut ms = Element::new()
        .w(200.0)
        .h(28.0)
        .flex_shrink(0.0)
        .child(Element::new().text("1c-ms) MessageScroller"));
    ms.set_attr("data-slot", "ms-title");
    let column = Element::new()
        .flex_col()
        .w(200.0)
        .child(card)
        .child(ms)
        .child(Element::new().w(200.0).h(400.0).flex_shrink(0.0));
    let mut viewport = Element::new()
        .w(200.0)
        .h(200.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(column);
    viewport.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{scroll_id}"));
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    let mut root = Element::new().w(200.0).h(200.0).child(viewport);

    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
    let think_rest = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Thinking"))
        .expect("Thinking at rest");
    let think_rest_y = think_rest.y;
    let ms_rest = first
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .map(|n| n.y);
    tree.set_collect_skip_allowed(true);
    let _warm = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], true, false);

    // 1c-ms rest Y − Thinking rest Y ≈ card-below-thinking + title. Scroll by that
    // delta so 1c-ms lands on Thinking's rest Y while the card still straddles.
    let scroll_y = (ms_rest.unwrap_or(think_rest_y + 160.0) - think_rest_y).max(80.0);
    root.children[0].scroll_offset[1] = scroll_y;
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set(scroll_y);
    let scrolled = tree.build_and_layout_fast(&mut root, 200.0, 200.0, [-1.0, -1.0], true, false);

    let texts: Vec<_> = scrolled
        .text_nodes
        .iter()
        .map(|n| format!("{:?} y={:.1}", n.text, n.y))
        .collect();
    let dump = format!(
        "scroll={scroll_y:.1} think_rest={think_rest_y:.1} skipped={:?}\n texts={texts:?}",
        tree.last_collect_skip_counts()
    );

    let ms = scrolled
        .text_nodes
        .iter()
        .find(|n| n.text.contains("1c-ms)"))
        .unwrap_or_else(|| panic!("1c-ms in view after straddle scroll\n{dump}"));
    let ms_cy = ms.y + ms.height * 0.5;
    assert!(
        (ms_cy - think_rest_y).abs() < 20.0,
        "fixture: 1c-ms must sit at Thinking's rest Y after scroll\n ms_cy={ms_cy:.1}\n{dump}"
    );

    let think_hits: Vec<_> = scrolled
        .text_nodes
        .iter()
        .filter(|n| n.text.contains("Thinking"))
        .collect();
    assert!(
        think_hits.len() <= 1,
        "skip-ghost + live double-paint of Thinking\n{dump}"
    );
    for n in &think_hits {
        assert!(
            (n.y - ms.y).abs() >= 14.0,
            "straddling Thinking painted at rest Y onto 1c-ms) (错位)\n y={:.1} ms_y={:.1}\n{dump}",
            n.y,
            ms.y
        );
        assert!(
            n.y + n.height <= 1.0,
            "Thinking must leave the clip, not stay at rest Y\n y={:.1}\n{dump}",
            n.y
        );
    }

    let border = scrolled
        .text_nodes
        .iter()
        .find(|n| n.text == "Border")
        .unwrap_or_else(|| panic!("Border still in the straddling card\n{dump}"));
    let border_expected = think_rest_y + 80.0 - scroll_y;
    assert!(
        (border.y - border_expected).abs() < 2.0,
        "idle Border inside the straddling card must track page scroll, not rest Y\n y={:.1} expected={border_expected:.1}\n{dump}",
        border.y
    );
}

#[test]
fn scroll_replay_rotated_text_matches_fresh_collect() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    fn scene() -> Element {
        let mut rotated = Element::new()
            .w(96.0)
            .h(56.0)
            .flex_shrink(0.0)
            .flex_col()
            .items_center()
            .justify_center()
            .bg("#dddddd")
            .child(
                Element::new()
                    .w(72.0)
                    .h(20.0)
                    .text("Bake Ag")
                    .text_size(16.0),
            );
        rotated.style.rotate_deg = 12.0;
        let viewport = Element::new()
            .w(300.0)
            .h(240.0)
            .flex_shrink(0.0)
            .overflow_y("scroll")
            .child(
                Element::new()
                    .w(300.0)
                    .flex_col()
                    .child(Element::new().w(300.0).h(100.0).flex_shrink(0.0))
                    .child(rotated)
                    .child(Element::new().w(300.0).h(1200.0).flex_shrink(0.0)),
            );
        Element::new()
            .w(300.0)
            .h(288.0)
            .flex_col()
            .rounded(12.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .child(Element::new().w(300.0).h(48.0).flex_shrink(0.0))
            .child(viewport)
    }
    let mut root = scene();
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 288.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    tree.build_and_layout_fast(&mut root, 300.0, 288.0, [-1.0; 2], true, false);
    for offset in [1.0, 8.0, 16.0, 45.0, 60.0, 100.0, 130.0, 100.0, 16.0, 0.0] {
        root.children[1].scroll_offset[1] = offset;
        let retained = tree.build_and_layout_fast(&mut root, 300.0, 288.0, [-1.0; 2], true, false);
        let mut fresh_root = scene();
        fresh_root.children[1].scroll_offset[1] = offset;
        let fresh = LayoutTree::new().build_and_layout(&mut fresh_root, 300.0, 288.0, [-1.0; 2]);
        if offset <= 60.0 {
            assert!(
                !fresh.textured_draws.is_empty(),
                "fixture must exercise Bake"
            );
        }
        assert_eq!(
            retained.textured_draws.len(),
            fresh.textured_draws.len(),
            "bake count at {offset}"
        );
        for (a, b) in retained.textured_draws.iter().zip(&fresh.textured_draws) {
            assert_eq!(a.texture_key, b.texture_key, "bake key at {offset}");
            for (field, a, b) in [
                ("rect", a.rect.as_slice(), b.rect.as_slice()),
                (
                    "rounded_clips",
                    a.rounded_clips.as_flattened(),
                    b.rounded_clips.as_flattened(),
                ),
                ("transform", a.transform.as_slice(), b.transform.as_slice()),
                ("clip_pos", a.clip_pos.as_slice(), b.clip_pos.as_slice()),
                ("clip_size", a.clip_size.as_slice(), b.clip_size.as_slice()),
            ] {
                assert!(
                    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.001),
                    "{field} at {offset}: retained={a:?}, fresh={b:?}"
                );
            }
        }
    }
}

