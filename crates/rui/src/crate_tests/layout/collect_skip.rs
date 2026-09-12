//! Paint-only collect can replay a fingerprint display list for clean sibling subtrees
//! (GPUI `Entity::cached` analog — skip collect, not omit pixels).

use crate::app::frame_stats::{last_skipped_siblings, reset_collect_skip_counters};
use crate::core::tree::{Element, LayoutTree};

fn two_column_page() -> Element {
    let col = |bg: [f32; 4], label: &str| {
        Element::new()
            .flex_col()
            .flex_shrink(0.0)
            .w(80.0)
            .h(80.0)
            .child(
                Element::new()
                    .w(80.0)
                    .h(40.0)
                    .flex_shrink(0.0)
                    .bg(bg),
            )
            .child(
                Element::new()
                    .w(80.0)
                    .h(40.0)
                    .flex_shrink(0.0)
                    .text(label),
            )
    };
    Element::new()
        .flex_row()
        .w(160.0)
        .h(80.0)
        .child(col([1.0, 0.0, 0.0, 1.0], "left"))
        .child(col([0.0, 0.0, 1.0, 1.0], "right"))
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
fn scroll_only_does_not_use_fingerprint_skip() {
    reset_collect_skip_counters();
    let mut root = two_column_page();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 160.0, 80.0, [-1.0, -1.0]);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut root, 160.0, 80.0, [-1.0, -1.0], true, false);
    assert_eq!(
        tree.last_collect_skip_counts().0,
        0,
        "scroll_only keeps band cull; fingerprint skip stays off"
    );
}
