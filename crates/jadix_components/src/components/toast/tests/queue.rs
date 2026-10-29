use super::*;

#[test]
fn toast_queue_budget_helpers() {
    assert!(!non_displace_budget_full(0));
    assert!(non_displace_budget_full(1));
    assert_eq!(max_displace_fades(1), 2);
    assert_eq!(max_displace_fades(3), 4);
    assert_eq!(TOAST_EXIT_GAP, Duration::from_millis(32));
}
