use jadix_core::virtualization::{ListScrollAlign, VirtualAxis};

#[test]
fn estimates_preserve_measurements_and_keyed_anchor_across_dataset_edits() {
    let mut axis = VirtualAxis::estimated(0..10_000, 44.0).unwrap();
    axis.set_gap(2.0).unwrap();
    let anchor = axis.anchor(5000.0 * 46.0 + 7.0).unwrap().unwrap();
    axis.measure(&0, 20.0).unwrap();
    axis.measure(&1, 100.0).unwrap();
    assert!(axis.estimate_from_measurements().unwrap());
    assert_eq!(axis.extent(&0), Some(20.0));
    assert_eq!(axis.extent(&5000), Some(60.0));
    let offset = axis.restore_anchor(&anchor, 500.0).unwrap();
    assert_eq!(axis.anchor(offset).unwrap().unwrap(), anchor);
    assert_eq!(axis.viewport_geometry(offset, 500.0).unwrap().visible.start, 5000);
    assert_eq!(axis.scroll_to_index(5000, 0.0, 500.0, ListScrollAlign::Start, 0.0).unwrap(), axis.position(5000).unwrap());
    axis.replace((0..10_000).rev().chain([10_000]), 44.0).unwrap();
    assert_eq!(axis.extent(&0), Some(20.0));
    assert_eq!(axis.extent(&10_000), Some(60.0));
    axis.measure(&10_000, 120.0).unwrap();
    axis.estimate_from_measurements().unwrap();
    assert_eq!(axis.extent(&20), Some(80.0));
    axis.splice(2..4, [(10_001, 200.0), (10_002, 40.0)]).unwrap();
    axis.append([(10_003, 80.0)]).unwrap();
    axis.estimate_from_measurements().unwrap();
    let sum: f64 = (0..axis.len()).map(|i| axis.extent(axis.key(i).unwrap()).unwrap()).sum();
    assert!((axis.total() - sum - (axis.len() - 1) as f64 * 2.0).abs() < 0.001);
    let mut prefix = 0.0;
    for i in 0..axis.len() {
        assert!((axis.position(i).unwrap() - prefix).abs() < 0.001);
        prefix += axis.extent(axis.key(i).unwrap()).unwrap() + 2.0;
    }
}

#[test]
fn equal_measurements_become_known_and_invalid_edits_are_atomic() {
    let mut axis = VirtualAxis::estimated(0..4, 44.0).unwrap();
    axis.measure(&0, 44.0).unwrap();
    axis.measure(&1, 88.0).unwrap();
    axis.estimate_from_measurements().unwrap();
    assert_eq!(axis.extent(&0), Some(44.0));
    assert_eq!(axis.extent(&3), Some(66.0));
    let total = axis.total();
    assert!(axis.measure(&2, f64::NAN).is_err());
    assert!(axis.replace([0, 0], 44.0).is_err());
    assert!(axis.splice(0..1, [(2, 10.0)]).is_err());
    assert_eq!(axis.total(), total);
    axis.append([(4, 22.0), (5, 110.0), (6, 66.0), (7, 66.0)]).unwrap();
    axis.estimate_from_measurements().unwrap();
    assert_eq!(axis.extent(&2), Some(66.0));
    for i in 0..axis.len() {
        let position = axis.position(i).unwrap();
        assert_eq!(axis.viewport_geometry(position, 1.0).unwrap().visible, i..i + 1);
    }
}
