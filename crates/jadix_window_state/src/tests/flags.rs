use super::*;

#[test]
fn all_contains_each_flag() {
    let all = StateFlags::all();
    assert!(all.contains(StateFlags::SIZE));
    assert!(all.contains(StateFlags::POSITION));
    assert!(all.contains(StateFlags::MAXIMIZED));
    assert!(all.contains(StateFlags::VISIBLE));
    assert!(all.contains(StateFlags::DECORATIONS));
    assert!(all.contains(StateFlags::FULLSCREEN));
    assert!(!StateFlags::SIZE.contains(StateFlags::POSITION));
}

#[test]
fn default_is_all() {
    assert_eq!(StateFlags::default(), StateFlags::all());
}
