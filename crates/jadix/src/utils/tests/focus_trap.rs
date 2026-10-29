use super::*;

#[test]
fn sync_activate_and_owner_scoped_deactivate() {
    sync_focus_trap("a", true, "x,y");
    assert_eq!(use_signal(FOCUS_TRAP_RING_KEY, String::new).peek(), "x,y");
    assert_eq!(use_signal(FOCUS_TRAP_OWNER_KEY, String::new).peek(), "a");
    assert_eq!(
        focus_trap_ring_ids(),
        vec!["x".to_string(), "y".to_string()]
    );

    // Other owner cannot clear.
    deactivate_focus_trap("b");
    assert_eq!(use_signal(FOCUS_TRAP_RING_KEY, String::new).peek(), "x,y");

    sync_focus_trap("a", false, "x,y");
    assert!(use_signal(FOCUS_TRAP_RING_KEY, String::new)
        .peek()
        .is_empty());
    assert!(use_signal(FOCUS_TRAP_OWNER_KEY, String::new)
        .peek()
        .is_empty());
}

#[test]
fn second_owner_takes_over() {
    activate_focus_trap("a", "one");
    activate_focus_trap("b", "two,three");
    assert_eq!(use_signal(FOCUS_TRAP_OWNER_KEY, String::new).peek(), "b");
    assert_eq!(
        focus_trap_ring_ids(),
        vec!["two".to_string(), "three".to_string()]
    );
    deactivate_focus_trap("a");
    assert_eq!(
        focus_trap_ring_ids(),
        vec!["two".to_string(), "three".to_string()]
    );
    deactivate_focus_trap("b");
    assert!(focus_trap_ring_ids().is_empty());
}
