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

#[test]
fn keyboard_page_uses_measured_rows_and_requests_reveal() {
    let (_owner, ()) = create_root(|| {
        let id = "si3_page_menu";
        let rest = format!("{id}__dd_scroll");
        activate_registry::reset(id);
        for enabled in [true, false, true, true, true, true] {
            activate_registry::push_enabled(id, std::rc::Rc::new(|| {}), enabled);
        }
        for (i, (top, height)) in [
            (0.0, 20.0),
            (20.0, 30.0),
            (50.0, 20.0),
            (120.0, 40.0),
            (160.0, 80.0),
            (240.0, 20.0),
        ]
        .into_iter()
        .enumerate()
        {
            activate_registry::update_bounds(id, i, [0.0, top, 120.0, height]);
        }
        let key = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(&rest).viewport_h;
        use_signal(&key, || 0.0_f32).set_silent(100.0);
        let active = use_signal(format!("{id}__dd_active").as_str(), || 0usize);
        assert!(navigate_menu(
            id,
            "dd_active",
            active,
            &Key::Named(NamedKey::PageDown)
        ));
        assert_eq!(
            active.peek(),
            3,
            "measured row geometry selects the closest enabled row"
        );
        assert!(crate::app::scroll_support::has_pending_scroll_item_align());
        crate::app::scroll_support::reset_scroll_item_into_view(&rest);
    });
}
