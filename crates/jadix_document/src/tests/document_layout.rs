use super::*;

#[test]
fn measured_heights_preserve_anchor() {
    let mut index = DocumentHeightIndex::new([20.0, 80.0, 30.0]);
    let anchor = index.anchor(35.0);
    assert_eq!(anchor.block, 1);
    assert!(index.measure(0, 50.0));
    assert_eq!(index.restore(anchor), 65.0);
    assert_eq!(index.total(), 160.0);
    assert_eq!(index.visible(55.0, 10.0, 0), 1..2);
}

#[test]
fn stale_offsets_and_invalid_heights_are_bounded() {
    let empty = DocumentHeightIndex::default();
    assert_eq!(empty.visible(9e20, 500.0, 4), 0..0);
    let index = DocumentHeightIndex::new([f64::NAN, -3.0, 15.0]);
    assert_eq!(index.total(), 17.0);
    assert_eq!(index.visible(9e20, 500.0, 4), 0..3);
    assert_eq!(index.block_at(2.0), 2);
}

#[test]
fn large_index_matches_prefix_sums() {
    let mut index = DocumentHeightIndex::new((0..100_000).map(|block| (block % 5 + 1) as f64));
    assert_eq!(index.total(), 300_000.0);
    assert_eq!(index.top(50_000), 150_000.0);
    index.measure(2, 103.0);
    assert_eq!(index.top(50_000), 150_100.0);
    assert_eq!(index.block_at(150_100.0), 50_000);
}
