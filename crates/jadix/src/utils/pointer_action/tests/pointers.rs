use super::*;

#[test]
fn max_per_element_blocks_second_drag() {
    let mut set = PointerSet::new(Quota::default().max_per_element(1));
    set.upsert(1, [0.0, 0.0]);
    set.upsert(2, [10.0, 0.0]);
    assert!(set.try_begin(1, "card", ActionKind::Drag).is_ok());
    assert_eq!(
        set.try_begin(2, "card", ActionKind::Drag),
        Err(BeginError::MaxPerElement)
    );
    assert!(set.try_begin(2, "other", ActionKind::Drag).is_ok());
}

#[test]
fn gesture_replaces_same_element_drag() {
    let mut set = PointerSet::new(Quota::default().max_per_element(1));
    set.upsert(1, [0.0, 0.0]);
    set.upsert(2, [10.0, 0.0]);
    assert!(set.try_begin(1, "card", ActionKind::Drag).is_ok());
    assert!(set.try_begin(1, "card", ActionKind::Gesture).is_ok());
    assert_eq!(set.count_on("card"), 1);
    assert_eq!(set.action_count(), 1);
}

#[test]
fn global_max() {
    let mut set = PointerSet::new(Quota::default().max(1));
    set.upsert(1, [0.0, 0.0]);
    set.upsert(2, [1.0, 0.0]);
    assert!(set.try_begin(1, "a", ActionKind::Drag).is_ok());
    assert_eq!(
        set.try_begin(2, "b", ActionKind::Drag),
        Err(BeginError::MaxReached)
    );
}

#[test]
fn two_elements_concurrent_when_max_allows() {
    let mut set = PointerSet::new(Quota::default().max(8).max_per_element(1));
    set.upsert(1, [0.0, 0.0]);
    set.upsert(2, [40.0, 0.0]);
    assert!(set.try_begin(1, "a", ActionKind::Drag).is_ok());
    assert!(set.try_begin(2, "b", ActionKind::Drag).is_ok());
    assert_eq!(set.action_count(), 2);
}

#[test]
fn sync_touches_drives_gesture() {
    reset_global();
    sync_touches([(1, [0.0, 0.0]), (2, [10.0, 0.0])]);
    assert!(gesture_is_active());
    sync_touches([(1, [0.0, 0.0]), (2, [20.0, 0.0])]);
    let g = last_gesture().unwrap();
    assert!((g.scale - 2.0).abs() < 1e-3);
    sync_touches([(1, [0.0, 0.0])]);
    assert!(!gesture_is_active());
    reset_global();
}
