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
