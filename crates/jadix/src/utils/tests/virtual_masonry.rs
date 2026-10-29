use super::*;

#[test]
fn pack_shortest_column_first() {
    let layout = pack_masonry(&[80.0, 40.0, 40.0], 2, 200.0, 10.0);
    assert!((layout.column_width - 95.0).abs() < 0.01);
    assert_eq!(layout.items[0].column, 0);
    assert_eq!(layout.items[1].column, 1);
    // col0=80, col1=40 → 第三项进更短的 col1
    assert_eq!(layout.items[2].column, 1);
    assert!((layout.items[2].y - 50.0).abs() < 0.01);
    assert!((layout.total_height - 90.0).abs() < 0.01);
}

#[test]
fn height_cache_overrides_estimate() {
    let mut cache = HashMap::new();
    cache.insert(0, 120.0);
    assert!((height_from_cache(&cache, 0, 90.0, |_, _| 80.0) - 120.0).abs() < 0.01);
    assert!((height_from_cache(&cache, 1, 90.0, |_, _| 80.0) - 80.0).abs() < 0.01);
}

#[test]
fn cache_reflow_shifts_later_item_in_same_column() {
    let est = pack_masonry(&[80.0, 80.0, 80.0], 1, 100.0, 8.0);
    let measured = pack_masonry(&[120.0, 80.0, 80.0], 1, 100.0, 8.0);
    assert!(measured.items[1].y > est.items[1].y);
    assert!((measured.items[1].y - 128.0).abs() < 0.01);
}

#[test]
fn visible_window_skips_offscreen() {
    let layout = pack_masonry(&[50.0; 20], 1, 100.0, 0.0);
    let vis = visible_masonry_indices(&layout.items, 200.0, 100.0, 50.0);
    assert!(vis.first().copied().unwrap() >= 2);
    assert!(*vis.last().unwrap() <= 8);
    assert!(vis.len() < 20);
}
