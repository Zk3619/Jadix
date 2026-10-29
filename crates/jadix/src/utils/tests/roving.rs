use super::*;

#[test]
fn wraps_and_edges() {
    assert_eq!(roving_move(3, 0, -1), Some(2));
    assert_eq!(roving_move(3, 2, 1), Some(0));
    assert_eq!(roving_edge(5, false), Some(0));
    assert_eq!(roving_edge(5, true), Some(4));
    assert_eq!(roving_move(0, 0, 1), None);
    assert_eq!(roving_edge(0, true), None);
}

#[test]
fn multi_step_wrap() {
    assert_eq!(roving_move(4, 1, 2), Some(3));
    assert_eq!(roving_move(4, 1, -2), Some(3));
    assert_eq!(roving_move(1, 0, 1), Some(0));
    assert_eq!(roving_move(1, 0, -1), Some(0));
}

#[test]
fn clamps_stale_current() {
    assert_eq!(roving_move(3, 99, 1), Some(0));
    assert_eq!(roving_move(3, 99, -1), Some(1));
}

#[test]
fn menu_list_contract_home_end_arrows() {
    let n = 4;
    let mut i = 0;
    i = roving_move(n, i, 1).unwrap();
    assert_eq!(i, 1);
    i = roving_move(n, i, 1).unwrap();
    assert_eq!(i, 2);
    i = roving_edge(n, true).unwrap();
    assert_eq!(i, 3);
    i = roving_move(n, i, 1).unwrap();
    assert_eq!(i, 0);
    i = roving_edge(n, false).unwrap();
    assert_eq!(i, 0);
}

#[test]
fn tabs_strip_wraps_like_roving_move() {
    let n = 3;
    assert_eq!(roving_move(n, 2, 1), Some(0));
    assert_eq!(roving_move(n, 0, -1), Some(2));
}

#[test]
fn focus_id_only_does_not_set_focus_visible() {
    use_signal("__jadix_focus_visible", || false).set(false);
    focus_id_only("cal");
    assert_eq!(
        use_signal("__jadix_button_focus_id", String::new).peek(),
        "cal"
    );
    assert!(!use_signal("__jadix_focus_visible", || false).peek());
    focus_by_id("cal2");
    assert!(use_signal("__jadix_focus_visible", || false).peek());
}

#[test]
fn skipping_disabled() {
    let disabled = |i: usize| i == 1 || i == 2;
    assert_eq!(roving_move_skipping(4, 0, 1, disabled), Some(3));
    assert_eq!(roving_move_skipping(4, 3, -1, disabled), Some(0));
    assert_eq!(roving_move_skipping(3, 0, 1, |_| true), None);
}
