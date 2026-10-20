use jadix::core::reactivity::use_signal;
use jadix::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use jadix::core::tree::{Element, LayoutStatus};
use jadix::layout::LayoutTree;
use jadix::style::sync_class_variants_subtree;

#[test]
fn newly_visible_scroll_content_matches_settled_variant_style() {
    use jadix::style::{apply_class_list, parse_color, tick_active_css_transitions_subtree};
    let scroll_id = "first-visible-variant";
    let mut tile = Element::new().w(120.0).h(40.0).flex_shrink(0.0);
    apply_class_list(&mut tile, "bg-red-500 dark:bg-blue-500");
    let mut root = Element::new()
        .flex_col()
        .w(120.0)
        .h(100.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(Element::new().w(120.0).h(800.0).flex_shrink(0.0))
        .child(tile)
        .child(Element::new().w(120.0).h(200.0).flex_shrink(0.0));
    root.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{scroll_id}"));
    let signal = use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32);
    signal.set_silent(0.0);
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 120.0, 100.0, [-1.0; 2]);
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0; 2],
        "",
        true,
        120.0,
        100.0,
        &mut tree,
        &mut impact,
    );
    tree.set_collect_skip_allowed(true);
    tree.build_and_layout_fast(&mut root, 120.0, 100.0, [-1.0; 2], true, false);

    root.scroll_offset[1] = 760.0;
    signal.set_silent(760.0);
    tick_active_css_transitions_subtree(
        &mut root,
        [-1.0; 2],
        "",
        true,
        120.0,
        100.0,
        [0.0; 2],
        &mut tree,
        &mut impact,
    );
    let first = tree.build_and_layout_fast(&mut root, 120.0, 100.0, [-1.0; 2], true, false);
    let first_bg = root.children[1].style.bg_color;
    sync_class_variants_subtree(
        &mut root,
        [-1.0; 2],
        "",
        true,
        120.0,
        100.0,
        &mut tree,
        &mut impact,
    );
    let settled = tree.build_and_layout_fast(&mut root, 120.0, 100.0, [-1.0; 2], false, false);
    let settled_bg = root.children[1].style.bg_color;
    assert_eq!(
        settled_bg,
        parse_color("blue-500"),
        "fixture must resolve the dark variant"
    );
    assert_eq!(
        first_bg, settled_bg,
        "first visible scroll frame must already have its settled style"
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&first.instances),
        bytemuck::cast_slice::<_, u8>(&settled.instances),
        "settling must not repaint newly revealed content with a different style"
    );
}
