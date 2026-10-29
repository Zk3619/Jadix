use super::*;

#[test]
fn password_allows_ime_and_preserves_unicode() {
    assert!(InputType::Password.allows_ime());
    assert!(InputType::Text.allows_ime());
    assert_eq!(filter_password_insert("", "Ab1!你'好\n\u{7f}"), "Ab1!你'好");
    assert_eq!(
        filter_insert_for_type(InputType::Password, "", "pāss字"),
        "pāss字"
    );
}

#[test]
fn text_type_strips_escape_and_other_controls() {
    assert_eq!(
        filter_insert_for_type(InputType::Text, "", "a\u{1b}b\0c\u{7f}"),
        "abc"
    );
    assert_eq!(
        filter_insert_for_type(InputType::Search, "", "hi\u{1b}"),
        "hi"
    );
    assert_eq!(
        filter_insert_for_type(InputType::Text, "", "骄傲\u{1b}3"),
        "骄傲3"
    );
    assert_eq!(
        filter_insert_for_type(InputType::Text, "", "line\nkeep"),
        "line\nkeep"
    );
}

fn find_input_text_leaf(el: &Element) -> Option<&Element> {
    if el.text.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(find_input_text_leaf)
}

#[test]
fn input_placeholder_uses_ellipsis_overflow() {
    let el: Element = input("t-ph-ellipsis")
        .placeholder("Placeholder…")
        .truncate(true)
        .text_overflow("hidden")
        .into();
    let leaf = find_input_text_leaf(&el).expect("input text leaf");
    assert_eq!(
        leaf.text_style.overflow,
        crate::style::TextOverflow::Ellipsis,
        "empty field shows placeholder with trailing … when narrow"
    );
    assert!(!leaf.text_style.overflow.allows_caret_scroll());
}

#[test]
fn input_with_value_uses_hidden_overflow_for_caret_pan() {
    let sig = use_signal("__jadix_input_val_overflow", || "hello".to_string());
    let el: Element = input("t-val-hidden").value_signal(sig).into();
    let leaf = find_input_text_leaf(&el).expect("input text leaf");
    assert_eq!(leaf.text_style.overflow, crate::style::TextOverflow::Hidden);
    assert!(leaf.text_style.overflow.allows_caret_scroll());
}

#[test]
fn input_value_signal_seeds_store() {
    let sig = use_signal("__jadix_wp38_input_val", || "hello".to_string());
    let _el: Element = input("wp38-in").value_signal(sig).into();
    assert_eq!(
        use_signal(&input_value_signal_key("wp38-in"), String::new).peek(),
        "hello"
    );
    sig.set("world".into());
    let pulled = crate::utils::text_field::pull_controlled_into_store("wp38-in", "hello", false);
    assert_eq!(pulled.as_deref(), Some("world"));
}

#[test]
fn number_step_formats_and_commits() {
    let id = "num-step-fmt";
    use_signal(&input_value_signal_key(id), String::new).set("3".into());
    step_number_value(id, 1.0);
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        "4"
    );
    step_number_value(id, -2.0);
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        "2"
    );
    assert_eq!(format_number_step_result(1.5, 0.5), "1.5");
    assert_eq!(format_number_step_result(2.0, 1.0), "2");
}

#[test]
fn number_input_builds_inc_dec_triggers() {
    let id = "num-step-ui";
    use_signal(&input_value_signal_key(id), String::new).set("0".into());
    let el: Element = input(id)
        .input_type(InputType::Number)
        .number_steppers(true)
        .into();
    let inc_tid = input_number_inc_template_id(id);
    let dec_tid = input_number_dec_template_id(id);
    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }
    let inc = find_tid(&el, &inc_tid).expect("inc trigger");
    let dec = find_tid(&el, &dec_tid).expect("dec trigger");
    assert!(inc.on_mousedown.is_some());
    assert!(dec.on_mousedown.is_some());
    // Fire down handler once via the stored callback.
    let mut pev =
        crate::app::pointer_events::PointerEvent::new(0.0, 0.0, winit::event::MouseButton::Left, 1);
    pev.phase = crate::app::pointer_events::EventPhase::Target;
    (*inc.on_mousedown.as_ref().unwrap().borrow_mut())(&pev);
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        "1"
    );
    end_number_step_hold();
    (*dec.on_mousedown.as_ref().unwrap().borrow_mut())(&pev);
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        "0"
    );
    end_number_step_hold();
}

#[test]
fn number_steppers_opt_in_only() {
    let id = "num-step-opt";
    let el: Element = input(id).input_type(InputType::Number).into();
    fn find_tid(el: &Element, tid: &str) -> bool {
        if el.template_id.as_deref() == Some(tid) {
            return true;
        }
        el.children.iter().any(|c| find_tid(c, tid))
    }
    assert!(!find_tid(&el, &input_number_inc_template_id(id)));
    assert!(!find_tid(&el, &input_number_dec_template_id(id)));
}

#[test]
fn ring_width_and_color_are_authored_on_chrome() {
    let el: Element = input("ring-style")
        .ring_width(5.0)
        .ring_color("primary")
        .into();
    assert_eq!(el.style.focus_ring_width, Some(5.0));
    let want = crate::style::parse_color("primary");
    let got = el.style.focus_ring_color.expect("ring color");
    assert!((got[0] - want[0]).abs() < 0.001);
    assert!((got[1] - want[1]).abs() < 0.001);
    assert!((got[2] - want[2]).abs() < 0.001);
}

#[test]
fn ring_utility_class_is_authored_on_chrome() {
    let el: Element = input("ring-class").class("ring-2 ring-primary").into();
    assert_eq!(el.style.focus_ring_width, Some(2.0));
    let want = crate::style::parse_color("primary");
    let got = el.style.focus_ring_color.expect("ring color");
    assert!((got[0] - want[0]).abs() < 0.001);
}

#[test]
fn chrome_text_size_forwards_into_paint_sync() {
    let el: Element = input("sz-fwd").into();
    let el = el.text_size(20.0);
    let sync = el.input_paint_sync.as_ref().expect("paint sync");
    assert!(
        (sync.text_size_px - 20.0).abs() < 0.01,
        "Element::text_size must update inner glyphs, not only container text_style"
    );
    assert_eq!(el.text_style.size, Some(20.0));
}

#[test]
fn text_size_utility_class_forwards_into_paint_sync() {
    let el: Element = input("sz-class").class("text-lg").into();
    let sync = el.input_paint_sync.as_ref().expect("paint sync");
    assert!(
        (sync.text_size_px - 18.0).abs() < 0.01,
        "class text-lg (1.125rem) must land on input_paint_sync"
    );
}

#[test]
fn chrome_caret_color_forwards_into_paint_sync() {
    let el: Element = input("caret-fwd").into();
    let el = el.caret_color("#111111");
    let want = crate::core::tree::parse_text_color("#111111");
    let got = el
        .input_paint_sync
        .as_ref()
        .expect("paint sync")
        .caret_color
        .expect("caret");
    assert_eq!(got, want);
}
