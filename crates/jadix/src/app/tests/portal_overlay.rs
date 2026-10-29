use super::*;
use crate::core::reactivity::Signal;

fn clear() {
    with_stack_mut(|s| s.clear());
    set_page_scroll_locked(false);
    crate::utils::popup::arm_underlay_close();
}

fn depth() -> usize {
    with_stack_mut(|s| s.len())
}

#[test]
fn page_teardown_must_not_peek_disposed_overlay_open() {
    clear();
    let (owner, open) = crate::core::reactivity::create_root(|| {
        let open = Signal::new(true);
        assert!(portal_sync("sel", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        open
    });
    drop(owner);
    assert!(
        !open.is_alive(),
        "page island dispose must drop the overlay open signal"
    );
    // Wheel / pointer / next portal_sync prune the stack. Peek panics were
    // the gallery crash after opening a Select then switching pages.
    let _ = stacked_menu_or_modal_open();
    let _ = try_close_top();
    assert_eq!(depth(), 0, "disposed layers must prune off the stack");
    clear();
}

#[test]
fn inactive_modal_does_not_dismiss_live_exclusive_surfaces() {
    for kind in [
        OverlayKind::Menu,
        OverlayKind::Popover,
        OverlayKind::Tooltip,
    ] {
        clear();
        let open = Signal::new(true);
        let modal = Signal::new(false);
        assert!(portal_sync("live", open, kind, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sidebar", modal, OverlayKind::Modal, OverlayMode::Stacked).is_none());
        assert!(open.peek(), "inactive modal must not dismiss {kind:?}");
        assert_eq!(depth(), 1);
    }
    clear();
}

#[test]
fn closing_modal_dismisses_nested_exclusive_surface() {
    clear();
    let modal = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("dlg", modal, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 2);

    // Dialog close: portal_sync with open=false must clear nested Select too.
    modal.set(false);
    assert!(portal_sync("dlg", modal, OverlayKind::Modal, OverlayMode::Stacked).is_none());
    assert!(
        !menu.peek(),
        "nested menu open signal must be cleared with modal"
    );
    assert_eq!(depth(), 0);
    clear();
}

#[test]
fn top_stacked_panel_z_tracks_depth() {
    clear();
    assert_eq!(top_stacked_panel_z(), None);
    let a = Signal::new(true);
    assert!(portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert_eq!(top_stacked_panel_z(), Some(1001));
    let b = Signal::new(true);
    assert!(portal_sync("b", b, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert_eq!(top_stacked_panel_z(), Some(1011));
    assert!(try_close_top());
    assert_eq!(top_stacked_panel_z(), Some(1001));
}

/// WP-1.2 / smoke #7: Escape → [`try_close_top`] only closes the top layer and does not penetrate the lower layer.
#[test]
fn push_two_esc_closes_top_only() {
    clear();
    let a = Signal::new(true);
    let b = Signal::new(true);
    let za = portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).unwrap();
    let zb = portal_sync("b", b, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
    assert_eq!(za.underlay, 1000);
    assert_eq!(zb.underlay, 1010);
    assert_eq!(depth(), 2);

    assert!(try_close_top());
    assert!(!b.peek());
    assert!(a.peek());
    assert_eq!(depth(), 1);

    assert!(try_close_top());
    assert!(!a.peek());
    assert_eq!(depth(), 0);
    assert!(!try_close_top());
}

#[test]
fn three_layers_esc_closes_top_only_each_time() {
    clear();
    let a = Signal::new(true);
    let b = Signal::new(true);
    let c = Signal::new(true);
    assert!(portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("b", b, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("c", c, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 3);

    assert!(try_close_top());
    assert!(!c.peek());
    assert!(b.peek());
    assert!(a.peek());

    assert!(try_close_top());
    assert!(!b.peek());
    assert!(a.peek());

    assert!(try_close_top());
    assert!(!a.peek());
    assert_eq!(depth(), 0);
}

#[test]
fn pinned_does_not_enter_stack() {
    clear();
    let open = Signal::new(true);
    let z = portal_sync("pin", open, OverlayKind::Popover, OverlayMode::Pinned).unwrap();
    assert_eq!(z.underlay, 900);
    assert_eq!(depth(), 0);
    assert!(!try_close_top());
    assert!(open.peek());
}

#[test]
fn sync_false_pops_by_id() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    open.set(false);
    assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_none());
    assert_eq!(depth(), 0);
}

#[test]
fn same_id_does_not_duplicate() {
    clear();
    let open = Signal::new(true);
    let z1 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
    let z2 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
    assert_eq!(z1, z2);
    assert_eq!(depth(), 1);
}

#[test]
fn menu_blocks_page_wheel_outside_hoisted_surface() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    let page = [dummy_hit(None, None, false)];
    assert!(
        overlay_blocks_page_wheel(&page),
        "context menu must lock page wheel"
    );
    let on_menu = [dummy_hit(Some("panel:ctx"), Some("ctx"), true)];
    assert!(
        !overlay_blocks_page_wheel(&on_menu),
        "wheel over the menu panel must still reach overlay scroll"
    );
    open.set(false);
    assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_none());
    assert!(
        !overlay_blocks_page_wheel(&page),
        "page wheel resumes after menu closes"
    );
}

#[test]
fn modal_blocks_page_wheel_outside_hoisted_surface() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("cmd", open, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    let page = [dummy_hit(None, None, false)];
    assert!(
        overlay_blocks_page_wheel(&page),
        "command overlay must lock page wheel"
    );
    let on_underlay = [dummy_hit(Some("underlay:1000"), None, true)];
    assert!(
        !overlay_blocks_page_wheel(&on_underlay),
        "underlay is hoisted; wheel is consumed without page scroll"
    );
}

#[test]
fn popover_does_not_block_page_wheel() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    let page = [dummy_hit(None, None, false)];
    assert!(
        !overlay_blocks_page_wheel(&page),
        "popover/tooltip must not lock page scroll"
    );
}

#[test]
fn custom_set_page_scroll_locked_blocks_page_wheel() {
    clear();
    let page = [dummy_hit(None, None, false)];
    assert!(!overlay_blocks_page_wheel(&page));
    set_page_scroll_locked(true);
    assert!(
        overlay_blocks_page_wheel(&page),
        "set_page_scroll_locked(true) must freeze page wheel"
    );
    let on_overlay = [dummy_hit(Some("panel:x"), Some("x"), true)];
    assert!(
        !overlay_blocks_page_wheel(&on_overlay),
        "hoisted panel still receives wheel while the page is locked"
    );
    set_page_scroll_locked(false);
    assert!(!overlay_blocks_page_wheel(&page));
}

#[test]
fn lock_page_scroll_on_hover_toggles_global() {
    clear();
    let mut el = crate::core::tree::Element::new().lock_page_scroll_on_hover();
    let h = el.on_hover.take().expect("hover handler");
    (*h.borrow_mut())(true);
    assert!(page_scroll_locked());
    (*h.borrow_mut())(false);
    assert!(!page_scroll_locked());
}

#[test]
fn popover_can_opt_in_to_lock_page_scroll() {
    clear();
    let open = Signal::new(true);
    let mounted = Signal::new(true);
    assert!(portal_sync_presence_lock(
        "pop-lock",
        open,
        mounted,
        OverlayKind::Popover,
        OverlayMode::Stacked,
        true,
    )
    .is_some());
    let page = [dummy_hit(None, None, false)];
    assert!(
        overlay_blocks_page_wheel(&page),
        "explicit lock_page_scroll on a popover must lock the page"
    );
}

#[test]
fn menu_can_opt_out_of_lock_page_scroll() {
    clear();
    let open = Signal::new(true);
    let mounted = Signal::new(true);
    assert!(portal_sync_presence_lock(
        "menu-free",
        open,
        mounted,
        OverlayKind::Menu,
        OverlayMode::Stacked,
        false,
    )
    .is_some());
    let page = [dummy_hit(None, None, false)];
    assert!(
        !overlay_blocks_page_wheel(&page),
        "explicit lock_page_scroll=false must allow page scroll under a menu"
    );
}

#[test]
fn menu_default_freeze_is_page_not_all() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(active_scroll_freeze(), ScrollFreeze::Page);
    let on_menu = [dummy_hit(Some("panel:ctx"), Some("ctx"), true)];
    assert!(
        !overlay_blocks_wheel(&on_menu),
        "default page freeze must still allow overlay wheel"
    );
    let page = [dummy_hit(None, None, false)];
    assert!(overlay_blocks_wheel(&page));
    assert!(ScrollFreeze::Page.applies_to_rest(false));
    assert!(
        !ScrollFreeze::Page.applies_to_rest(true),
        "page freeze must not snap overlay scroll rests"
    );
}

#[test]
fn scroll_freeze_overlay_blocks_only_overlay_wheel() {
    clear();
    let open = Signal::new(true);
    let mounted = Signal::new(true);
    assert!(portal_sync_presence_lock(
        "ov-only",
        open,
        mounted,
        OverlayKind::Popover,
        OverlayMode::Stacked,
        ScrollFreeze::Overlay,
    )
    .is_some());
    let page = [dummy_hit(None, None, false)];
    let on_overlay = [dummy_hit(Some("panel:ov-only"), Some("ov-only"), true)];
    assert!(!overlay_blocks_wheel(&page));
    assert!(overlay_blocks_wheel(&on_overlay));
    assert!(!page_scroll_locked());
}

#[test]
fn scroll_freeze_all_blocks_page_and_overlay_wheel() {
    clear();
    let open = Signal::new(true);
    let mounted = Signal::new(true);
    assert!(portal_sync_presence_lock(
        "all",
        open,
        mounted,
        OverlayKind::Menu,
        OverlayMode::Stacked,
        ScrollFreeze::All,
    )
    .is_some());
    let page = [dummy_hit(None, None, false)];
    let on_overlay = [dummy_hit(Some("panel:all"), Some("all"), true)];
    assert!(overlay_blocks_wheel(&page));
    assert!(overlay_blocks_wheel(&on_overlay));
    assert!(ScrollFreeze::All.applies_to_rest(true));
    assert!(ScrollFreeze::All.applies_to_rest(false));
}

#[test]
fn parse_scroll_freeze_aliases() {
    assert_eq!(parse_scroll_freeze("page"), ScrollFreeze::Page);
    assert_eq!(parse_scroll_freeze("overlay"), ScrollFreeze::Overlay);
    assert_eq!(parse_scroll_freeze("all"), ScrollFreeze::All);
    assert_eq!(parse_scroll_freeze("none"), ScrollFreeze::None);
    assert_eq!(parse_scroll_freeze("true"), ScrollFreeze::Page);
    assert_eq!(parse_scroll_freeze("false"), ScrollFreeze::None);
    assert_eq!(
        ScrollFreeze::Page.combine(ScrollFreeze::Overlay),
        ScrollFreeze::All
    );
}

#[test]
fn set_scroll_freeze_all_blocks_overlay_wheel() {
    clear();
    let page = [dummy_hit(None, None, false)];
    let on_overlay = [dummy_hit(Some("panel:x"), Some("x"), true)];
    set_scroll_freeze(ScrollFreeze::All);
    assert!(overlay_blocks_wheel(&page));
    assert!(overlay_blocks_wheel(&on_overlay));
    set_scroll_freeze(ScrollFreeze::None);
    assert!(!overlay_blocks_wheel(&page));
    assert!(!overlay_blocks_wheel(&on_overlay));
}

#[test]
fn dismiss_viewport_change_closes_exclusive_keeps_modal() {
    clear();
    let dialog = Signal::new(true);
    let pop = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("dlg", dialog, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    // Resize / scrollbar must not wait for underlay arm (not an outside click).
    crate::utils::popup::disarm_underlay_close();
    assert!(dismiss_exclusive_on_viewport_change());
    assert!(dialog.peek(), "modal must survive viewport dismiss");
    assert!(!pop.peek(), "popover must close before layout follows");
    assert!(!menu.peek(), "menu must close before layout follows");
    assert_eq!(depth(), 1);
    clear();
}

#[test]
fn dismiss_viewport_change_noop_when_empty() {
    clear();
    assert!(!dismiss_exclusive_on_viewport_change());
}

#[test]
fn dismiss_outside_releases_borrow_before_set() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    crate::utils::popup::arm_underlay_close();
    // Must not panic with RefCell already borrowed (set runs after stack unlock).
    assert!(try_dismiss_outside_press());
    assert!(!open.peek());
    assert_eq!(depth(), 0);
    assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_none());
}

#[test]
fn dismiss_outside_respects_disarm() {
    clear();
    let open = Signal::new(true);
    assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    // portal_sync on new push disarms; dismiss must no-op until armed.
    assert!(!try_dismiss_outside_press());
    assert!(open.peek());
    crate::utils::popup::arm_underlay_close();
    assert!(try_dismiss_outside_press());
    assert!(!open.peek());
}

/// Presence keep-alive must stay stacked after outside-press so accordion-up
/// can play. Popping it made the next underlay click close the parent overlay
/// and the panel flashed then vanished.
#[test]
fn dismiss_outside_presence_keeps_layer_for_exit() {
    clear();
    let open = Signal::new(true);
    let mounted = Signal::new(true);
    assert!(portal_sync_presence(
        "sel",
        open,
        mounted,
        OverlayKind::Menu,
        OverlayMode::Stacked
    )
    .is_some());
    crate::utils::popup::arm_underlay_close();
    assert!(try_dismiss_outside_press());
    assert!(!open.peek());
    assert!(mounted.peek(), "keep-alive must still be mounted");
    assert_eq!(depth(), 1, "exiting menu must stay on the stack");
    assert!(
        portal_sync_presence(
            "sel",
            open,
            mounted,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_some(),
        "rebuild during exit must still receive a portal z"
    );
    assert!(
        try_dismiss_outside_press(),
        "second outside press must consume the exiting layer"
    );
    assert!(mounted.peek());
    assert_eq!(depth(), 1);
    clear();
}

#[test]
fn dismiss_outside_presence_does_not_close_modal_underneath() {
    clear();
    let dialog = Signal::new(true);
    let dialog_keep = Signal::new(true);
    let menu = Signal::new(true);
    let menu_keep = Signal::new(true);
    assert!(portal_sync_presence(
        "dlg",
        dialog,
        dialog_keep,
        OverlayKind::Modal,
        OverlayMode::Stacked
    )
    .is_some());
    assert!(portal_sync_presence(
        "sel",
        menu,
        menu_keep,
        OverlayKind::Menu,
        OverlayMode::Stacked
    )
    .is_some());
    crate::utils::popup::arm_underlay_close();
    assert!(try_dismiss_outside_press());
    assert!(!menu.peek());
    assert!(dialog.peek());
    assert_eq!(depth(), 2);
    assert!(
        try_dismiss_outside_press(),
        "mouse-up underlay must still be consumed by the exiting menu"
    );
    assert!(dialog.peek(), "modal must stay open during nested exit");
    assert!(menu_keep.peek());
    assert_eq!(depth(), 2);
    clear();
}

#[test]
fn peer_menu_replace_does_not_disarm() {
    clear();
    let a = Signal::new(true);
    let b = Signal::new(false);
    assert!(portal_sync("menu-a", a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    // Simulate post-click arm (opening gesture released).
    crate::utils::popup::arm_underlay_close();
    assert!(crate::utils::popup::underlay_close_is_armed());
    // Menubar hover/arrow switch: open peer while first is still open.
    b.set(true);
    assert!(portal_sync("menu-b", b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(
        crate::utils::popup::underlay_close_is_armed(),
        "replacing a peer Menu must keep underlay armed"
    );
    assert!(!a.peek(), "exclusive Menu should close the previous peer");
    assert!(try_dismiss_outside_press());
    assert!(!b.peek());
}

#[test]
fn nested_menu_sub_does_not_disarm() {
    clear();
    let parent = Signal::new(true);
    let sub = Signal::new(false);
    assert!(portal_sync("file", parent, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    crate::utils::popup::arm_underlay_close();
    sub.set(true);
    assert!(portal_sync("file__sub0", sub, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(
        crate::utils::popup::underlay_close_is_armed(),
        "opening nested __sub must keep underlay armed"
    );
    assert!(parent.peek(), "parent menu must stay open under nested sub");
}

/// Menubar namespaces menus as `{bar}__{menu}`; sub ids are `{bar}__{menu}__subN`.
/// Nested keep-parent must use the full parent overlay id, not a short suffix.
#[test]
fn nested_menubar_namespaced_sub_keeps_parent_open() {
    clear();
    let parent = Signal::new(true);
    let sub = Signal::new(false);
    let parent_id = "gal-menubar__file";
    let sub_id = format!("{parent_id}__sub0");
    assert!(portal_sync(parent_id, parent, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    crate::utils::popup::arm_underlay_close();
    sub.set(true);
    assert!(
        portal_sync(
            sub_id.as_str(),
            sub,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_some(),
        "namespaced sub must push onto the stack"
    );
    assert!(
        parent.peek(),
        "menubar parent Menu must stay open under namespaced __sub"
    );
    assert!(
        sub.peek(),
        "sub open signal must remain true after portal_sync"
    );
    assert_eq!(depth(), 2, "parent + nested sub must both remain stacked");
}

/// Regression: `arm` must run *after* the open `on_click` (which disarms). Arming first
/// then opening leaves the stack disarmed so the next outside press only blurs Combobox.
#[test]
fn arm_before_open_click_leaves_first_outside_dismiss_noop() {
    clear();
    let open = Signal::new(false);
    crate::utils::popup::arm_underlay_close();
    // Open click: disarm + push (portal_sync also disarms).
    crate::utils::popup::disarm_underlay_close();
    open.set(true);
    assert!(portal_sync("cb", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(
        !try_dismiss_outside_press(),
        "still disarmed after open — first outside dismiss must no-op"
    );
    assert!(open.peek());
    // Next release arms; second outside press closes.
    crate::utils::popup::arm_underlay_close();
    assert!(try_dismiss_outside_press());
    assert!(!open.peek());
}

#[test]
fn arm_after_open_click_allows_immediate_outside_dismiss() {
    clear();
    let open = Signal::new(false);
    crate::utils::popup::disarm_underlay_close();
    open.set(true);
    assert!(portal_sync("cb", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    crate::utils::popup::arm_underlay_close();
    assert!(try_dismiss_outside_press());
    assert!(!open.peek());
}

#[test]
fn hit_path_on_top_exclusive_ignores_other_hoisted_portals() {
    clear();
    let menu = Signal::new(true);
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(top_stacked_exclusive_id().as_deref(), Some("sel"));

    let on_dialog = [crate::app::pointer_events::HitPathEntry {
        hoisted_portal: true,
        portal_key: Some("panel:dlg".into()),
        ..Default::default()
    }];
    assert!(
        !hit_path_on_top_exclusive_surface(&on_dialog),
        "Dialog portal must not count as Select menu surface"
    );

    let on_menu = [crate::app::pointer_events::HitPathEntry {
        hoisted_portal: true,
        portal_key: Some("panel:sel".into()),
        floating_id: Some("sel".into()),
        ..Default::default()
    }];
    assert!(hit_path_on_top_exclusive_surface(&on_menu));
}

#[test]
fn opening_menu_closes_other_menu_and_popover() {
    clear();
    let menu_a = Signal::new(true);
    let pop = Signal::new(true);
    let menu_b = Signal::new(true);
    assert!(portal_sync("menu-a", menu_a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    assert!(!menu_a.peek(), "popover must dismiss prior menu");
    assert_eq!(depth(), 1);

    assert!(portal_sync("menu-b", menu_b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(
        pop.peek(),
        "Select/Menu must stack inside Popover (DatePicker caption)"
    );
    assert!(menu_b.peek());
    assert_eq!(depth(), 2);
}

#[test]
fn closing_popover_dismisses_nested_menu() {
    clear();
    let pop = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 2);
    pop.set(false);
    assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_none());
    assert!(!menu.peek(), "nested Select must close with DatePicker");
    assert_eq!(depth(), 0);
}

/// Exclusive keep-alive leaves the previous Popover on the stack so it can
/// play exit. Finishing that exit must not drain a live successor Popover
/// (gallery: first ColorPicker click while another overlay is still exiting).
#[test]
fn finishing_exclusive_exit_must_not_close_successor_popover() {
    clear();
    let leftover_open = Signal::new(false);
    let leftover_keep = Signal::new(true);
    assert!(portal_sync_presence(
        "old",
        leftover_open,
        leftover_keep,
        OverlayKind::Popover,
        OverlayMode::Stacked
    )
    .is_some());

    let fresh_open = Signal::new(true);
    let fresh_keep = Signal::new(true);
    assert!(portal_sync_presence(
        "new",
        fresh_open,
        fresh_keep,
        OverlayKind::Popover,
        OverlayMode::Stacked
    )
    .is_some());
    assert!(
        fresh_open.peek(),
        "opening a Popover must not clear its own open"
    );

    leftover_keep.set(false);
    assert!(portal_sync_presence(
        "old",
        leftover_open,
        leftover_keep,
        OverlayKind::Popover,
        OverlayMode::Stacked
    )
    .is_none());
    assert!(
        fresh_open.peek(),
        "finishing the previous Popover's exit must not close the live successor"
    );
    assert_eq!(depth(), 1);
    clear();
}

#[test]
fn finishing_menu_exit_keeps_peer_and_its_submenu_open() {
    clear();
    let file = Signal::new(true);
    let file_keep = Signal::new(true);
    let file_sub = Signal::new(true);
    let file_sub_keep = Signal::new(true);
    let edit = Signal::new(true);
    let edit_sub = Signal::new(true);
    for (id, open, mounted) in [
        ("bar__file", file, file_keep),
        ("bar__file__sub0", file_sub, file_sub_keep),
    ] {
        assert!(
            portal_sync_presence(id, open, mounted, OverlayKind::Menu, OverlayMode::Stacked)
                .is_some()
        );
    }
    assert!(portal_sync("bar__edit", edit, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(!file.peek());
    assert!(!file_sub.peek());
    assert!(portal_sync(
        "bar__edit__sub0",
        edit_sub,
        OverlayKind::Menu,
        OverlayMode::Stacked
    )
    .is_some());

    file_keep.set(false);
    assert!(portal_sync_presence(
        "bar__file",
        file,
        file_keep,
        OverlayKind::Menu,
        OverlayMode::Stacked
    )
    .is_none());
    assert!(edit.peek(), "finishing File must retain the peer Edit menu");
    assert!(edit_sub.peek(), "finishing File must retain Edit's submenu");
    assert_eq!(depth(), 2);

    // File's later submenu exit must also leave the successor intact.
    file_sub_keep.set(false);
    assert!(portal_sync_presence(
        "bar__file__sub0",
        file_sub,
        file_sub_keep,
        OverlayKind::Menu,
        OverlayMode::Stacked
    )
    .is_none());
    assert!(edit.peek());
    assert!(edit_sub.peek());
    edit.set(false);
    assert!(portal_sync("bar__edit", edit, OverlayKind::Menu, OverlayMode::Stacked).is_none());
    assert!(
        !edit_sub.peek(),
        "a real submenu must still close with its parent"
    );
    assert_eq!(depth(), 0);
    clear();
}

#[test]
fn finishing_exclusive_exit_still_closes_nested_menu() {
    clear();
    let pop_open = Signal::new(false);
    let pop_keep = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync_presence(
        "dp",
        pop_open,
        pop_keep,
        OverlayKind::Popover,
        OverlayMode::Stacked
    )
    .is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 2);

    pop_keep.set(false);
    assert!(portal_sync_presence(
        "dp",
        pop_open,
        pop_keep,
        OverlayKind::Popover,
        OverlayMode::Stacked
    )
    .is_none());
    assert!(
        !menu.peek(),
        "nested Select must still close with DatePicker"
    );
    assert_eq!(depth(), 0);
    clear();
}

fn dummy_hit(
    portal_key: Option<&str>,
    floating_id: Option<&str>,
    hoisted: bool,
) -> crate::app::pointer_events::HitPathEntry {
    crate::app::pointer_events::HitPathEntry {
        hoisted_portal: hoisted,
        portal_key: portal_key.map(str::to_string),
        floating_id: floating_id.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn nested_select_item_path_counts_as_inside_before_unmount() {
    clear();
    let pop = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("dp", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());

    let press_on_item = [dummy_hit(Some("panel:sel"), Some("sel"), true)];
    assert!(
        hit_path_on_top_exclusive_surface(&press_on_item),
        "release dismiss must use this pre-click path, not a re-hit after Select closes"
    );

    menu.set(false);
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_none());
    assert!(
        pop.peek(),
        "parent popover stays open when only the nested menu closes"
    );

    crate::utils::popup::arm_underlay_close();
    let miss_after_unmount = [dummy_hit(None, None, false)];
    assert!(
        !hit_path_on_top_exclusive_surface(&miss_after_unmount),
        "post-unmount re-hit would look outside — that path must not drive dismiss"
    );
    assert!(try_dismiss_outside_press());
    assert!(!pop.peek());
    clear();
}

#[test]
fn outside_path_is_not_on_exclusive_surface() {
    clear();
    let pop = Signal::new(true);
    assert!(portal_sync("dp", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
    let miss = [dummy_hit(None, None, false)];
    assert!(!hit_path_on_top_exclusive_surface(&miss));
    clear();
}

#[test]
fn sibling_menus_remain_exclusive() {
    clear();
    let a = Signal::new(true);
    let b = Signal::new(true);
    assert!(portal_sync("a", a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(portal_sync("b", b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(!a.peek());
    assert!(b.peek());
    assert_eq!(depth(), 1);
}

#[test]
fn menu_stacks_above_modal_without_closing_it() {
    clear();
    let modal = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("modal", modal, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("menu", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert!(modal.peek());
    assert!(menu.peek());
    assert_eq!(depth(), 2);
}

#[test]
fn overlay_stacks_are_isolated_by_window_scope() {
    use crate::app::window_scope::{enter, WindowScope};
    clear();
    let primary = Signal::new(true);
    assert!(portal_sync("p", primary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 1);

    let _g = enter(WindowScope::Secondary(9));
    clear();
    let secondary = Signal::new(true);
    assert!(portal_sync("s", secondary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    assert_eq!(depth(), 1);
    drop(_g);

    assert_eq!(depth(), 1);
    assert!(primary.peek());
    assert!(try_close_top());
    assert!(!primary.peek());
    assert_eq!(depth(), 0);
    assert!(secondary.peek());
    clear();
}

#[test]
fn dropping_secondary_overlay_stack_does_not_clear_primary() {
    use crate::app::window_scope::{enter, WindowScope};
    clear();
    let primary = Signal::new(true);
    assert!(portal_sync("p", primary, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    let _g = enter(WindowScope::Secondary(3));
    let secondary = Signal::new(true);
    assert!(portal_sync("s", secondary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    drop(_g);
    super::drop_overlay_stack(WindowScope::Secondary(3));
    assert_eq!(depth(), 1);
    assert!(primary.peek());
    clear();
}
