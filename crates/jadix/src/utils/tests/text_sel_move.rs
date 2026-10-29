use super::*;

#[test]
fn drop_inside_or_on_edge_is_forbidden() {
    assert!(!drop_allowed(2, 5, 2, 10));
    assert!(!drop_allowed(2, 5, 4, 10));
    assert!(!drop_allowed(2, 5, 5, 10));
    assert!(drop_allowed(2, 5, 1, 10));
    assert!(drop_allowed(2, 5, 6, 10));
    assert!(drop_allowed(2, 5, 10, 10));
}

#[test]
fn move_before_selection() {
    let (s, caret) = apply_move("abcdef", (2, 4), 0).unwrap();
    assert_eq!(s, "cdabef");
    assert_eq!(caret, 2);
}

#[test]
fn move_after_selection() {
    let (s, caret) = apply_move("abcdef", (1, 3), 6).unwrap();
    assert_eq!(s, "adefbc");
    assert_eq!(caret, 6);
}

#[test]
fn move_into_selection_is_none() {
    assert!(apply_move("abcdef", (1, 4), 2).is_none());
}

#[test]
fn caret_lands_at_end_of_moved_chunk() {
    // "hello world", move "world" in front of "hello "
    let (s, caret) = apply_move("hello world", (6, 11), 0).unwrap();
    assert_eq!(s, "worldhello ");
    assert_eq!(caret, "world".len());
}
