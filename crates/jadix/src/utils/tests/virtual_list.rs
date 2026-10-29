use super::*;

#[test]
fn stale_scroll_after_document_shrink_cannot_exceed_item_count() {
    assert_eq!(visible_item_range(2_000_000.0, 220.0, 22.0, 3, 2), (3, 3));
    assert_eq!(visible_item_range(0.0, 220.0, 22.0, 3, 2), (0, 3));
}
