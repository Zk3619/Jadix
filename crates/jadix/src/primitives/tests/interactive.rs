use super::*;
use crate::core::tree::{CursorKind, Element, LayoutStatus};
use crate::primitives::r#box::hstack;

#[test]
fn apply_disabled_stamps_attr_and_focus() {
    let mut el: Element = hstack().into();
    el.set_focusable(true);
    assert_eq!(apply_disabled(&mut el, true), LayoutStatus::StyleChanged);
    assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
    assert!(!el.focusable);
    assert_eq!(apply_disabled(&mut el, true), LayoutStatus::Clean);
    assert_eq!(apply_disabled(&mut el, false), LayoutStatus::StyleChanged);
    assert!(!el.attrs.contains_key("disabled"));
    assert!(
        !el.focusable,
        "generic nodes do not gain focus just because disabled is cleared"
    );
}

#[test]
fn apply_disabled_restores_focus_on_paint_sync_nodes() {
    let mut el: Element = hstack().into();
    el.disabled_sync = Some(|element, disabled| {
        let changed = element.attrs.contains_key("test_disabled") != disabled;
        if disabled {
            element.set_attr("test_disabled", "true");
        } else {
            element.attrs.remove("test_disabled");
        }
        changed
    });
    el.set_focusable(true);
    assert_eq!(apply_disabled(&mut el, true), LayoutStatus::StyleChanged);
    assert!(!el.focusable);
    assert!(el.attrs.contains_key("test_disabled"));
    assert_eq!(apply_disabled(&mut el, false), LayoutStatus::StyleChanged);
    assert!(el.focusable);
    assert!(!el.attrs.contains_key("test_disabled"));
}

#[test]
fn interactive_builder_disabled_sets_not_allowed_cursor() {
    let el: Element = interactive(hstack().into()).disabled(true).into();
    assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
    assert_eq!(el.cursor, Some(CursorKind::NotAllowed.into()));
}
