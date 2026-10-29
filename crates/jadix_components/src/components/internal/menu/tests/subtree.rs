use super::*;
use crate::core::reactivity::create_root;

#[test]
fn close_open_subs_clears_open_flags() {
    let (_o, ()) = create_root(|| {
        let parent = "si3_parent";
        let sid = format!("{parent}__sub0");
        let open = use_signal(format!("{sid}__open").as_str(), || false);
        open.set(true);
        assert!(open.peek());
        close_open_subs(parent);
        assert!(!open.peek());
    });
}

#[test]
fn open_or_remount_keyboard_path_stays_open() {
    let (_o, ()) = create_root(|| {
        let open = use_signal("si3_sub__open", || false);
        open_or_remount_sub_ex("si3_sub", open, false);
        assert!(open.peek());
    });
}
