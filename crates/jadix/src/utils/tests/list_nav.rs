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
