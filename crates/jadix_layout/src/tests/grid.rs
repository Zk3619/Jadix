use super::*;

#[test]
fn first_track_fr_roundtrips_flex_tracks() {
    let mut rows = vec![track_fr(0.0)];
    assert_eq!(first_track_fr(&rows), Some(0.0));
    set_first_track_fr(&mut rows, 1.0);
    assert!((first_track_fr(&rows).unwrap() - 1.0).abs() < 1e-6);
    set_first_track_fr(&mut rows, 0.5);
    assert!((first_track_fr(&rows).unwrap() - 0.5).abs() < 1e-6);
}

#[test]
fn tracks_fr_list_rejects_mixed_and_lerps_equal_length() {
    let two = vec![track_fr(0.0), track_fr(1.0)];
    assert_eq!(tracks_fr_list(&two), Some(vec![0.0, 1.0]));
    let mut rows = two.clone();
    set_tracks_fr_list(&mut rows, &[0.25, 0.75]);
    assert_eq!(tracks_fr_list(&rows), Some(vec![0.25, 0.75]));
    let mixed = vec![track_fr(1.0), GridTemplateComponent::Single(length(40.0))];
    assert!(tracks_fr_list(&mixed).is_none());
}
