use super::*;
use crate::components::input::input;
use crate::core::reactivity::{create_effect, create_root};

fn tree_has_text(el: &Element, s: &str) -> bool {
    el.text.as_deref() == Some(s) || el.children.iter().any(|c| tree_has_text(c, s))
}

fn find_input_invalid(el: &Element) -> Option<bool> {
    if let Some(sync) = &el.input_paint_sync {
        return Some(sync.invalid);
    }
    el.children.iter().find_map(find_input_invalid)
}

#[test]
fn form_validate_sets_and_clears_errors() {
    let (_owner, _) = create_root(|| {
        let errors = form_errors_signal("f1");
        provide_context(form_context(), FormState { errors });
        assert!(form_validate(|| {}));
        assert!(!form_validate(|| set_field_error("email", "bad")));
        assert!(form_validate(|| {}));
    });
}

#[test]
fn form_field_error_text_matches_input_invalid() {
    let (_owner, _) = create_root(|| {
        form_provider("wp39-a", || {
            set_field_error("email", "必填");
            let field = form_field("email", "邮箱", Element::from(input("wp39-email")));
            assert!(
                tree_has_text(&field, "必填"),
                "error copy must render next to the field"
            );
            assert_eq!(
                find_input_invalid(&field),
                Some(true),
                "Input invalid chrome must follow the error map"
            );
            Element::new()
        });
        form_provider("wp39-b", || {
            let field = form_field("email", "邮箱", Element::from(input("wp39-email-ok")));
            assert!(
                !tree_has_text(&field, "必填"),
                "sibling form must not inherit the other map"
            );
            assert_eq!(find_input_invalid(&field), Some(false));
            Element::new()
        });
    });
}

#[test]
fn form_field_under_effect_does_not_overflow() {
    let (_owner, _) = create_root(|| {
        create_effect(|| {
            crate::core::reactivity::isolate_reads(|| {
                let _ = crate::core::reactivity::component_root(|| {
                    form_provider("wp39-fx", || {
                        form_field("email", "邮箱", Element::from(input("wp39-fx-email")))
                    })
                });
            });
        });
    });
}

#[test]
fn form_state_clear_field_drops_invalid() {
    let (_owner, _) = create_root(|| {
        form_provider("wp39-c", || {
            set_field_error("email", "bad");
            clear_field_error("email");
            let field = form_field("email", "邮箱", Element::from(input("wp39-email-clr")));
            assert!(!tree_has_text(&field, "bad"));
            assert_eq!(find_input_invalid(&field), Some(false));
            Element::new()
        });
    });
}
