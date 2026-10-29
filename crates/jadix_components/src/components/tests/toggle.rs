use super::*;
use crate::core::reactivity::{create_root, use_scoped_signal, use_signal};
use crate::utils::toggle_state::bool_slot_key;

#[test]
fn toggle_empty_id_never_writes_empty_focus_or_template() {
    let el: Element = toggle("").text("B").into();
    assert_ne!(el.focus_id.as_deref(), Some(""));
    assert_ne!(el.template_id.as_deref(), Some(""));
    assert!(el.focus_id.as_ref().is_some_and(|s| !s.is_empty()));
    assert!(el.template_id.as_ref().is_some_and(|s| !s.is_empty()));
}

#[test]
fn toggle_uncontrolled_uses_scoped_signal_per_owner() {
    let key = bool_slot_key("same_local", SLOT_TOGGLE);
    let (h1, a) = create_root(|| {
        let _el: Element = toggle("same_local").default_pressed(false).into();
        let sig = use_scoped_signal(&key, || false);
        sig.set(true);
        sig.peek()
    });
    assert!(a);

    let (h2, b) = create_root(|| {
        let _el: Element = toggle("same_local").default_pressed(false).into();
        let sig = use_scoped_signal(&key, || false);
        // Fresh owner → independent slot (still default false).
        sig.peek()
    });
    assert!(!b);

    h1.dispose();
    h2.dispose();
}

#[test]
fn toggle_controlled_keeps_explicit_id() {
    let pressed = use_signal("__jadix_test_toggle_ctrl", || true);
    let el: Element = toggle("t-ctrl").pressed_signal(pressed).into();
    assert_eq!(el.template_id.as_deref(), Some("t-ctrl"));
    assert_eq!(el.focus_id.as_deref(), Some("t-ctrl"));
    assert!(pressed.peek());
}

#[test]
fn builder_style() {
    let (_owner, el): (_, Element) = create_root(|| {
        toggle("t-chrome")
            .style(|b| b.text_size(20.0))
            .text("B")
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "toggle .style(|b| b.text_size(20)) must land on the toggle"
    );
}

#[test]
fn toggle_hover_keeps_foreground() {
    let mut el: Element = toggle("t-hover").text("B").into();
    assert!(el
        .class_set
        .as_ref()
        .is_some_and(crate::style::ClassSet::has_hover_color));
    let rest_bg = el.style.bg_color;
    let rest_fg = parse_text_color("foreground");
    let attrs = el.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        &mut el,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, parse_color("muted"));
    assert_ne!(el.style.bg_color, rest_bg);
    let mut caption_fg = None;
    fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
        if el.text.is_some() {
            *acc = el.text_style.color;
        }
        for c in &el.children {
            child_fg(c, acc);
        }
    }
    child_fg(&el, &mut caption_fg);
    assert_eq!(
        caption_fg,
        Some(rest_fg),
        "hover matches observed ShadCN ghost Button: caption stays foreground"
    );
    assert_ne!(
        caption_fg,
        Some(parse_text_color("muted-foreground")),
        "hover must not dim the caption to muted-foreground"
    );
    assert_ne!(
        caption_fg,
        Some(parse_text_color("accent-foreground")),
        "hover must not shift caption to accent-foreground"
    );
}

#[test]
fn toggle_pressed_hover_keeps_accent() {
    let mut el: Element = toggle("t-on").default_pressed(true).text("B").into();
    assert_eq!(el.attrs.get("data-state").map(String::as_str), Some("on"));
    let attrs = el.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        &mut el,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, parse_color("accent"));
    let mut caption_fg = None;
    fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
        if el.text.is_some() {
            *acc = el.text_style.color;
        }
        for c in &el.children {
            child_fg(c, acc);
        }
    }
    child_fg(&el, &mut caption_fg);
    assert_eq!(
        caption_fg,
        Some(parse_text_color("accent-foreground")),
        "pressed hover must keep data-[state=on] accent-foreground"
    );
}

#[test]
fn toggle_outline_hover_uses_accent() {
    let mut el: Element = toggle("t-out").variant("outline").text("B").into();
    let attrs = el.attrs.clone();
    let _ = crate::style::resolve_class_variants(
        &mut el,
        &crate::style::VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..Default::default()
        },
    );
    assert_eq!(el.style.bg_color, parse_color("accent"));
    assert_eq!(
        el.text_style.color,
        Some(parse_text_color("foreground")),
        "outline hover keeps foreground, matching ghost Button"
    );
}
