use super::*;
use crate::layout::collect_cache::{
    invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
};

fn viewport(id: &str, y: f32) -> Element {
    let mut el = Element::new();
    el.focus_id = Some(format!("scroll:{id}"));
    el.layout_bounds = [0.0, y, 200.0, 100.0];
    el.style.overflow.y = taffy::style::Overflow::Scroll;
    el
}

fn query(root: &Element, y: f32) -> (bool, Vec<String>, usize) {
    SCROLL_HIT_VISITS.with(|count| count.set(0));
    let mut out = Vec::new();
    let hit = collect_scroll_rest_chain_at(root, 50.0, y, [0.0; 2], &mut out);
    (hit, out, SCROLL_HIT_VISITS.with(|count| count.get()))
}

fn assert_matches_unstamped(root: &mut Element, y: f32) -> (Vec<String>, usize, usize) {
    crate::core::tree::walk_elements_mut(root, |el| el.collect_paint_hash = 0);
    let (raw_hit, raw, raw_visits) = query(root, y);
    stamp_subtree_paint_hashes(root, false);
    let (hit, actual, visits) = query(root, y);
    assert_eq!((hit, &actual), (raw_hit, &raw));
    (actual, visits, raw_visits)
}

#[test]
fn single_viewport_owner_avoids_list_descendants() {
    let mut root = viewport("outer", 0.0);
    for i in 0..200 {
        let mut row = Element::new();
        row.layout_bounds = [0.0, i as f32 * 20.0, 200.0, 20.0];
        root.children.push(row);
    }
    let (owners, visits, raw_visits) = assert_matches_unstamped(&mut root, 50.0);
    assert_eq!(owners, ["outer"]);
    assert_eq!(visits, 1);
    assert!(
        raw_visits > 100,
        "fixture must visit offscreen rows without the summary"
    );
    assert_matches_unstamped(&mut root, 150.0);
}

#[test]
fn nested_moving_viewports_and_occluders_keep_scroll_ownership() {
    let mut outer = viewport("outer", 0.0);
    stamp_subtree_paint_hashes(&mut outer, false);
    assert!(!outer
        .children
        .iter()
        .any(|child| child.collect_skip_has_scroll));
    outer.children.push(viewport("inner", 200.0));
    invalidate_collect_paint_hash_path(&mut outer, &[0]);
    assert_eq!(query(&outer, 50.0).1, ["outer"]);
    outer.scroll_offset[1] = 200.0;
    assert_eq!(query(&outer, 50.0).1, ["inner", "outer"]);
    stamp_subtree_paint_hashes(&mut outer, true);
    assert!(outer.children[0].collect_skip_has_scroll);
    assert_eq!(query(&outer, 50.0).1, ["inner", "outer"]);
    assert_matches_unstamped(&mut outer, 50.0);

    let mut cover = Element::new();
    cover.layout_bounds = [0.0, 0.0, 200.0, 100.0];
    let mut root = Element::new().children([outer, cover]);
    root.layout_bounds = [0.0, 0.0, 200.0, 100.0];
    assert!(assert_matches_unstamped(&mut root, 50.0).0.is_empty());
}
