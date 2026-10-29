use super::*;
use crate::components::input::{input_caret_signal_key, input_value_signal_key};
use crate::core::reactivity::use_signal;
use crate::utils::text_field::{textarea_resize_focus_id, TEXTAREA_RESIZE_FOCUS_PREFIX};

fn find_focus(el: &Element, want: &str) -> bool {
    if el.focus_id.as_deref() == Some(want) {
        return true;
    }
    el.children.iter().any(|c| find_focus(c, want))
}

#[test]
fn textarea_focus_prefix() {
    assert_eq!(textarea_focus_id("bio"), "textarea:bio");
    let el: Element = textarea("bio").rows(3).placeholder("Say hi").into();
    assert_eq!(el.focus_id.as_deref(), Some("textarea:bio"));
}

#[test]
fn textarea_default_resize_grip() {
    let el: Element = textarea("bio").rows(3).into();
    let grip = textarea_resize_focus_id("bio");
    assert!(grip.starts_with(TEXTAREA_RESIZE_FOCUS_PREFIX));
    assert!(find_focus(&el, &grip), "expected resize grip focus_id");
}

#[test]
fn textarea_resizable_false_omits_grip() {
    let el: Element = textarea("bio").resizable(false).into();
    assert!(!find_focus(&el, &textarea_resize_focus_id("bio")));
}

#[test]
fn textarea_default_leaf_soft_wraps() {
    use crate::style::WhiteSpace;
    use crate::utils::text_field::input_text_template_id;

    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }

    let id = "ta-soft";
    let el: Element = textarea(id).rows(3).into();
    let tid = input_text_template_id(id);
    let leaf = find_tid(&el, &tid).expect("text leaf");
    assert_eq!(leaf.text_style.white_space, WhiteSpace::Normal);
}

#[test]
fn textarea_wrap_off_is_nowrap() {
    use crate::style::WhiteSpace;
    use crate::utils::text_field::input_text_template_id;

    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }

    let id = "ta-nowrap";
    let el: Element = textarea(id).rows(3).wrap("off").into();
    let tid = input_text_template_id(id);
    let leaf = find_tid(&el, &tid).expect("text leaf");
    assert_eq!(leaf.text_style.white_space, WhiteSpace::Nowrap);
}

#[test]
fn textarea_value_signal_keys() {
    let _ = use_signal(&input_value_signal_key("t1"), String::new);
    let _ = use_signal(&input_caret_signal_key("t1"), || None::<usize>);
    let el: Element = textarea("t1").into();
    assert_eq!(el.focus_id.as_deref(), Some("textarea:t1"));
}

#[test]
fn builder_style_reaches_textarea() {
    let el: Element = textarea("ta-style").style(|b| b.text_size(20.0)).into();
    assert!(
        el.text_style.size.is_some_and(|s| (s - 20.0).abs() < 0.01)
            || el
                .children
                .iter()
                .any(|c| c.text_style.size.is_some_and(|s| (s - 20.0).abs() < 0.01)),
        "textarea .style(|b| b.text_size(20)) must land on the field"
    );
}
