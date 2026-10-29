use super::*;
use crate::core::reactivity::{create_root, Signal};
use crate::layout::layout_tree::ATTR_PORTAL_KEY;

#[test]
fn overlay_prefers_nested_menu_dismiss_before_closing_modal() {
    use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};
    use crate::utils::popup::arm_underlay_close;

    clear_overlay_stack();
    let dialog = Signal::new(true);
    let menu = Signal::new(true);
    assert!(portal_sync("dlg", dialog, OverlayKind::Modal, OverlayMode::Stacked).is_some());
    assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
    arm_underlay_close();

    let underlay: Element = overlay(1000).close_on_click(dialog).into();
    if let Some(cb) = &underlay.on_click {
        cb.borrow_mut()();
    }
    assert!(
        !menu.peek(),
        "underlay click must dismiss nested menu first"
    );
    assert!(dialog.peek(), "dialog must stay open while menu was on top");
    clear_overlay_stack();
}

/// Pointer-down already started the nested menu's exit (`open=false`, keep-alive).
/// Underlay `on_click` on mouse-up must not pop that layer or close the dialog —
/// that skipped accordion-up and flashed the panel.
#[test]
fn overlay_click_must_not_kill_nested_menu_exit() {
    use crate::app::portal_overlay::{
        clear_overlay_stack, overlay_stack_depth_for_test, portal_sync_presence, OverlayKind,
        OverlayMode,
    };
    use crate::utils::popup::arm_underlay_close;

    clear_overlay_stack();
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
    arm_underlay_close();

    assert!(crate::app::portal_overlay::try_dismiss_outside_press());
    assert!(!menu.peek(), "pointer-down dismisses the nested menu");
    assert!(menu_keep.peek(), "menu must stay mounted for exit");
    assert!(dialog.peek());
    assert_eq!(overlay_stack_depth_for_test(), 2);

    let underlay: Element = overlay(1000).close_on_click(dialog).into();
    if let Some(cb) = &underlay.on_click {
        cb.borrow_mut()();
    }
    assert!(
        dialog.peek(),
        "underlay mouse-up must not close the dialog after nested dismiss"
    );
    assert!(
        menu_keep.peek(),
        "exiting menu keep-alive must survive the underlay click"
    );
    assert_eq!(
        overlay_stack_depth_for_test(),
        2,
        "exiting menu must stay on the stack for accordion-up"
    );
    clear_overlay_stack();
}

#[test]
fn overlay_sets_underlay_portal_key_and_blur() {
    let (_owner, el) = create_root(|| {
        let open = Signal::new(true);
        Element::from(
            overlay(1200)
                .color(crate::tokens::overlay_scrim(0.5))
                .backdrop_blur(12.0)
                .close_on_click(open),
        )
    });
    assert_eq!(
        el.attrs.get(ATTR_PORTAL_KEY).map(String::as_str),
        Some("underlay:1200")
    );
    assert!((el.style.backdrop_blur - 12.0).abs() < 0.01);
    assert!((el.style.bg_color[3] - 0.5).abs() < 0.01);
    assert!(el.on_click.is_some());
}
