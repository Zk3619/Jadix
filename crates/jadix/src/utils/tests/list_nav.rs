use super::*;

#[test]
fn arrows_and_edges() {
    let down = Key::Named(NamedKey::ArrowDown);
    assert_eq!(list_nav_move(&down, 3, 0), Some(1));
    let home = Key::Named(NamedKey::Home);
    assert_eq!(list_nav_move(&home, 3, 2), Some(0));
    assert!(list_nav_move_arrows(&home, 3, 2).is_none());
}

#[test]
fn filter_key_set() {
    assert!(is_list_nav_key(&Key::Named(NamedKey::ArrowDown)));
    assert!(is_list_activate_key(&Key::Named(NamedKey::Enter)));
    assert!(!is_list_nav_key(&Key::Named(NamedKey::Home)));
}

#[test]
fn disabled_rows_are_skipped_and_all_disabled_has_no_target() {
    let down = Key::Named(NamedKey::ArrowDown);
    let up = Key::Named(NamedKey::ArrowUp);
    let home = Key::Named(NamedKey::Home);
    let end = Key::Named(NamedKey::End);
    let disabled = [false, true, true, false];
    assert_eq!(
        list_nav_move_skipping(&down, disabled.len(), 0, |i| disabled[i]),
        Some(3)
    );
    assert_eq!(
        list_nav_move_skipping(&up, disabled.len(), 3, |i| disabled[i]),
        Some(0)
    );
    assert_eq!(
        list_nav_move_skipping(&home, disabled.len(), 3, |i| disabled[i]),
        Some(0)
    );
    assert_eq!(
        list_nav_move_skipping(&end, disabled.len(), 0, |i| disabled[i]),
        Some(3)
    );
    assert_eq!(list_nav_move_skipping(&down, 2, 0, |_| true), None);
}
