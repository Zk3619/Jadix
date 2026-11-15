use super::*;

#[test]
fn stale_scroll_after_document_shrink_cannot_exceed_item_count() {
    assert_eq!(visible_item_range(2_000_000.0, 220.0, 22.0, 3, 2), (3, 3));
    assert_eq!(visible_item_range(0.0, 220.0, 22.0, 3, 2), (0, 3));
}

#[test]
fn fractional_scroll_includes_both_partially_visible_edges() {
    assert_eq!(visible_item_range(5.0, 20.0, 20.0, 100, 0), (0, 2));
    assert_eq!(visible_item_range(20.0, 20.0, 20.0, 100, 0), (1, 2));
    assert_eq!(visible_item_range(25.0, 20.0, 20.0, 100, 1), (0, 4));
    assert_eq!(visible_item_range(-5.0, 20.0, 20.0, 100, 0), (0, 1));
    assert_eq!(visible_item_range(5.0, 0.0, 20.0, 100, 2), (0, 0));
    assert_eq!(visible_item_range(f32::NAN, 20.0, 20.0, 100, 0), (0, 0));
}
