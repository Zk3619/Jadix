use jadix_core::virtualization::*;

#[test]
fn viewport_position_and_measurement_keep_the_reading_anchor_through_shrinkage() {
    let mut axis = VirtualAxis::new(0..100, 50.0).unwrap();
    assert_eq!(
        axis.scroll_position(105.0, 100.0).unwrap(),
        Some(ListScrollPosition {
            index: 2,
            offset: 5.0
        })
    );
    assert_eq!(
        axis.window(105.0, 100.0, 50.0, &[], 8)
            .unwrap()
            .visible
            .start,
        1
    );
    let offset = axis
        .measure_preserving_scroll(&0, 20.0, 105.0, 100.0, false)
        .unwrap();
    assert_eq!(offset, 75.0);
    assert_eq!(
        axis.scroll_position(offset, 100.0).unwrap(),
        Some(ListScrollPosition {
            index: 2,
            offset: 5.0
        })
    );
    let offset = axis
        .measure_preserving_scroll(&1, 80.0, offset, 100.0, false)
        .unwrap();
    assert_eq!(offset, 105.0);
    let end = axis
        .measure_preserving_scroll(&99, 10.0, axis.total() - 100.0, 100.0, true)
        .unwrap();
    assert_eq!(end, axis.total() - 100.0);
    let total = axis.total();
    assert_eq!(
        axis.measure_preserving_scroll(&0, 10.0, f64::NAN, 100.0, false),
        Err(VirtualError::InvalidExtent)
    );
    assert_eq!(axis.total(), total);
    assert_eq!(axis.scroll_position(0.0, 0.0).unwrap(), None);
    assert_eq!(
        VirtualAxis::new(Vec::<u32>::new(), 10.0)
            .unwrap()
            .scroll_position(0.0, 100.0)
            .unwrap(),
        None
    );
}

#[test]
fn shrinking_the_anchor_row_keeps_its_key_in_the_viewport() {
    for extent in [10.0, 0.125] {
        let mut axis = VirtualAxis::new(0..100, 50.0).unwrap();
        let offset = axis
            .measure_preserving_scroll(&2, extent, 145.0, 100.0, false)
            .unwrap();
        let anchor = axis.anchor(offset).unwrap().unwrap();
        assert_eq!(anchor.key, 2);
        assert!(anchor.within >= 0.0 && anchor.within < extent);
        assert_eq!(
            axis.scroll_position(offset, 100.0).unwrap().unwrap().index,
            2
        );
        assert_eq!(
            axis.window(offset, 100.0, 0.0, &[], 8)
                .unwrap()
                .visible
                .start,
            2
        );
    }
}

#[test]
fn hundred_thousand_rows_materialize_a_bounded_window_and_pin_an_editor() {
    let axis = VirtualAxis::new(0u64..100_000, 32.0).unwrap();
    let window = axis.window(1_600_000.0, 640.0, 64.0, &[7], 32).unwrap();
    assert_eq!(window.visible, 49_998..50_022);
    assert_eq!(window.mounted.len(), 25);
    assert_eq!(window.mounted[0], 7);
    assert_eq!(window.total, 3_200_000.0);
    assert_eq!(axis.key(window.visible.start), Some(&49_998));
    assert_eq!(
        axis.window(0.0, 0.0, 0.0, &[], 0).unwrap().mounted,
        Vec::<usize>::new()
    );
    assert_eq!(
        axis.window(0.0, 640.0, 0.0, &[], 19),
        Err(VirtualError::Capacity)
    );
}

#[test]
fn measured_heights_and_reordered_keys_preserve_a_scroll_anchor() {
    let mut axis = VirtualAxis::new(["a", "b", "c", "d"], 30.0).unwrap();
    let anchor = axis.anchor(65.0).unwrap().unwrap();
    assert_eq!(
        anchor,
        ScrollAnchor {
            key: "c",
            within: 5.0
        }
    );
    axis.measure(&"a", 80.0).unwrap();
    assert_eq!(axis.restore_anchor(&anchor, 30.0).unwrap(), 115.0);
    axis.replace(["b", "a", "c", "d"], 30.0).unwrap();
    assert_eq!(axis.extent(&"a"), Some(80.0));
    assert_eq!(axis.restore_anchor(&anchor, 30.0).unwrap(), 115.0);
    assert_eq!(axis.reveal(&"d", 0.0, 30.0).unwrap(), 140.0);
    assert_eq!(axis.window(110.0, 30.0, 0.0, &[], 1).unwrap().visible, 2..3);
    assert_eq!(axis.window(111.0, 30.0, 0.0, &[], 2).unwrap().visible, 2..4);
}

#[test]
fn focused_window_keeps_adjacent_rows_mounted_for_sequential_navigation() {
    let axis = VirtualAxis::new(["a", "b", "c", "d", "e"], 20.0).unwrap();
    let window = axis
        .window_with_focus(0.0, 20.0, 0.0, &[], Some(&"c"), 4)
        .unwrap();
    assert_eq!(window.visible, 0..1);
    assert_eq!(window.mounted, vec![0, 1, 2, 3]);

    let last = axis
        .window_with_focus(80.0, 20.0, 0.0, &[], Some(&"e"), 2)
        .unwrap();
    assert_eq!(last.visible, 4..5);
    assert_eq!(last.mounted, vec![3, 4]);
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], None, 1),
        axis.window(0.0, 20.0, 0.0, &[], 1)
    );
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], Some(&"missing"), 4),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], Some(&"c"), 3),
        Err(VirtualError::Capacity)
    );
}

#[test]
fn invalid_or_duplicate_updates_leave_the_index_unchanged() {
    let mut axis = VirtualAxis::new([1, 2, 3], 24.0).unwrap();
    assert_eq!(axis.replace([1, 1], 24.0), Err(VirtualError::DuplicateKey));
    assert_eq!(axis.measure(&2, f64::NAN), Err(VirtualError::InvalidExtent));
    assert_eq!(axis.measure(&9, 20.0), Err(VirtualError::UnknownKey));
    assert_eq!(axis.total(), 72.0);
    assert_eq!(axis.position(3), Some(72.0));
    assert!(axis.window(f64::INFINITY, 20.0, 0.0, &[], 4).is_err());
    assert!(axis.window(0.0, 20.0, 0.0, &[8], 4).is_err());
}

#[test]
fn two_dimensional_windows_bound_the_cross_product() {
    let grid = VirtualGrid {
        rows: VirtualAxis::new(0..100_000, 32.0).unwrap(),
        columns: VirtualAxis::new(0..10_000, 100.0).unwrap(),
    };
    let [columns, rows] = grid
        .window(
            [500_000.0, 1_600_000.0],
            [500.0, 320.0],
            [0.0; 2],
            &[],
            &[0],
            60,
        )
        .unwrap();
    assert_eq!(columns.visible, 5000..5005);
    assert_eq!(rows.visible, 50_000..50_010);
    assert_eq!(columns.mounted.len() * rows.mounted.len(), 60);
    assert!(grid
        .window([0.0; 2], [500.0, 320.0], [0.0; 2], &[], &[9999], 59)
        .is_err());
}

#[test]
fn prefix_index_matches_linear_layout_after_variable_measurements() {
    let mut axis = VirtualAxis::new(0..257, 12.0).unwrap();
    let heights: Vec<_> = (0..257)
        .map(|index| (index % 11 + 1) as f64 * 3.0)
        .collect();
    for (index, height) in heights.iter().copied().enumerate() {
        axis.measure(&index, height).unwrap();
    }
    let mut position = 0.0;
    for (index, height) in heights.iter().copied().enumerate() {
        assert_eq!(axis.position(index), Some(position));
        assert_eq!(
            axis.window(position, height, 0.0, &[], 1).unwrap().visible,
            index..index + 1
        );
        position += height;
    }
    assert_eq!(axis.total(), position);
    assert_eq!(axis.position(257), Some(position));
}

#[test]
fn revealing_a_partly_visible_oversized_row_preserves_the_current_window() {
    let axis = VirtualAxis::new(["before", "large", "after"], 600.0).unwrap();
    for offset in [250.0, 600.0, 850.0, 1100.0] {
        assert_eq!(axis.reveal(&"large", offset, 400.0).unwrap(), offset);
    }
    assert_eq!(axis.reveal(&"large", 0.0, 400.0).unwrap(), 600.0);
    assert_eq!(axis.reveal(&"large", 1300.0, 400.0).unwrap(), 600.0);
}

#[test]
fn append_matches_rebuild_across_prefix_boundaries_and_later_measurements() {
    let mut axis = VirtualAxis::new(0..0, 12.0).unwrap();
    let mut count = 0;
    for batch in [1, 2, 5, 8, 17, 32, 64, 128, 257] {
        let end = count + batch;
        assert_eq!(axis.append((count..end).map(|key| (key, 12.0))), Ok(batch));
        let measured = end / 2;
        axis.measure(&measured, (end + 1) as f64).unwrap();
        let rebuilt = axis.replacement(0..end, 12.0).unwrap();
        for index in 0..=end {
            assert_eq!(axis.position(index), rebuilt.position(index));
        }
        for offset in (0..axis.total() as usize).step_by(7) {
            assert_eq!(
                axis.window(offset as f64, 23.0, 0.0, &[], 8).unwrap(),
                rebuilt.window(offset as f64, 23.0, 0.0, &[], 8).unwrap()
            );
        }
        assert_eq!(axis.index(&(end - 1)), Some(end - 1));
        count = end;
    }
}

#[test]
fn append_rejects_entire_batch_and_preserves_existing_anchor() {
    let mut axis = VirtualAxis::new(0..3, 20.0).unwrap();
    let anchor = axis.anchor(25.0).unwrap().unwrap();
    for (batch, error) in [
        (vec![(3, 10.0), (1, 10.0)], VirtualError::DuplicateKey),
        (vec![(3, 10.0), (3, 10.0)], VirtualError::DuplicateKey),
        (vec![(3, 10.0), (4, f64::NAN)], VirtualError::InvalidExtent),
        (vec![(3, 10.0), (4, 0.0)], VirtualError::InvalidExtent),
        (
            vec![(3, f64::MAX), (4, f64::MAX)],
            VirtualError::InvalidExtent,
        ),
    ] {
        assert_eq!(axis.append(batch), Err(error));
        assert_eq!(axis.len(), 3);
        assert_eq!(axis.total(), 60.0);
        assert_eq!(axis.index(&3), None);
        assert_eq!(axis.position(3), Some(60.0));
    }
    assert_eq!(axis.append([]), Ok(0));
    assert_eq!(axis.append([(3, 45.0)]), Ok(1));
    assert_eq!(axis.restore_anchor(&anchor, 20.0).unwrap(), 25.0);
    assert_eq!(axis.reveal(&3, 0.0, 20.0).unwrap(), 60.0);
    axis.measure(&0, 35.0).unwrap();
    assert_eq!(axis.position(4), Some(120.0));
}

#[test]
fn append_capacity_error_does_not_publish_a_partial_batch() {
    let mut axis = VirtualAxis::new(0..999_999, 1.0).unwrap();
    assert_eq!(
        axis.append([(999_999, 1.0), (1_000_000, 1.0)]),
        Err(VirtualError::Capacity)
    );
    assert_eq!(axis.len(), 999_999);
    assert_eq!(axis.index(&999_999), None);
    assert_eq!(axis.append([(999_999, 1.0)]), Ok(1));
    assert_eq!(axis.append([]), Ok(0));
    assert_eq!(axis.position(1_000_000), Some(1_000_000.0));
}

#[test]
fn appended_small_rows_do_not_subtract_large_unrelated_prefixes() {
    let mut axis = VirtualAxis::new(0..5, 1.0).unwrap();
    axis.measure(&0, 1e16).unwrap();
    axis.append([(5, 3.0)]).unwrap();
    axis.measure(&0, 1.0).unwrap();
    // The first four rows have their own floating-point measurement rounding;
    // the new Fenwick range [4, 6) must still retain both small row extents.
    assert_eq!(axis.position(6).unwrap() - axis.position(4).unwrap(), 4.0);
}
