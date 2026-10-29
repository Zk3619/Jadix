#[cfg(test)]
#[test]
fn scroll_repairs_merge_clustered_caption_slivers_and_keep_distant_edges() {
    let original = vec![
        [396.0, 2112.0, 3432.0, 36.0],
        [3810.0, 87.0, 15.0, 2025.0],
        [459.0, 84.0, 3306.0, 8.0],
        [454.0, 84.0, 5.0, 8.0],
        [454.0, 92.0, 143.0, 5.0],
        [600.0, 92.0, 120.0, 5.0],
        [723.0, 92.0, 150.0, 5.0],
        [876.0, 92.0, 92.0, 5.0],
        [970.0, 92.0, 113.0, 5.0],
    ];
    let mut repairs = original.clone();
    coalesce_scroll_repairs(&mut repairs);
    assert_eq!(
        repairs.len(),
        3,
        "one clustered header repair plus two edges"
    );
    for rect in original {
        let mut missing = vec![rect];
        for repair in &repairs {
            missing = missing
                .into_iter()
                .flat_map(|r| crate::app::damage::subtract_rect(r, *repair))
                .collect();
        }
        assert!(missing.is_empty(), "repair lost coverage: {rect:?}");
    }
    for (i, a) in repairs.iter().enumerate() {
        for b in &repairs[i + 1..] {
            assert!(intersect_rects(*a, *b).is_none(), "alpha replay overlaps");
        }
    }
    assert!(
        !repairs.iter().any(|r| r[0] <= 1920.0
            && r[0] + r[2] > 1920.0
            && r[1] <= 1080.0
            && r[1] + r[3] > 1080.0),
        "distant edges must keep copied middle pixels"
    );
}

#[cfg(test)]
#[test]
fn broad_scroll_repairs_keep_the_full_pass_cost_fallback() {
    let mut rects: Vec<_> = (0..12)
        .map(|i| [i as f32 * 32.0, 0.0, 24.0, 250.0])
        .collect();
    coalesce_scroll_repairs(&mut rects);
    assert!(scroll_repair_prefers_full(
        CullRects::from_rects(rects),
        3840,
        2160
    ));
}

#[cfg(test)]
#[test]
fn scroll_repair_cost_keeps_sparse_edges_but_bounds_fragmented_replays() {
    let edges = CullRects::from_rects([[0.0, 0.0, 3840.0, 24.0], [0.0, 2136.0, 3840.0, 24.0]]);
    assert!(!scroll_repair_prefers_full(edges, 3840, 2160));
    let fragmented = CullRects::from_rects((0..12).map(|i| [i as f32 * 32.0, 0.0, 8.0, 8.0]));
    assert!(scroll_repair_prefers_full(fragmented, 3840, 2160));
    let broad = Some(CullRects::one([0.0, 0.0, 3840.0, 1400.0]));
    assert!(scroll_repair_prefers_full(broad, 3840, 2160));
    assert!(scroll_repair_prefers_full(None, 3840, 2160));
}
