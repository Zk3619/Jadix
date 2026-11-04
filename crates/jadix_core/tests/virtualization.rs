use jadix_core::virtualization::*;

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
