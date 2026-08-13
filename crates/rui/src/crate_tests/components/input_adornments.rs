//! Input clearable + password_toggle adornments.

use crate::components::input::{
    focus_id_for_input, input, input_password_reveal_signal_key, input_value_signal_key, InputType,
};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn find_text<'a>(el: &'a Element, tid: &str) -> Option<&'a str> {
    if el.template_id.as_deref() == Some(tid) {
        return el.text.as_deref();
    }
    for c in &el.children {
        if let Some(t) = find_text(c, tid) {
            return Some(t);
        }
    }
    None
}

#[test]
fn clearable_shows_clear_when_nonempty_and_clears_value() {
    let id = "diag-input-clear";
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut el: Element = input(id).clearable(true).into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let clear = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .expect("clear adornment present");
    assert!(
        (clear.style.opacity - 1.0).abs() < 0.001,
        "unfocused nonempty must show clear"
    );
    clear.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        ""
    );
    assert_eq!(
        use_signal("__rui_button_focus_id", String::new).peek(),
        focus_id_for_input(id),
        "clear click must focus the field"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
}

#[test]
fn password_toggle_reveals_plaintext() {
    let id = "diag-input-pw-toggle";
    use_signal(&input_value_signal_key(id), String::new).set("secret".into());
    use_signal("__rui_button_focus_id", String::new)
        .set(crate::components::input::focus_id_for_input(id));

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .into();

    let text_tid = format!("__rui_input:{id}:text");
    let display = find_text(&el, &text_tid).unwrap().to_string();
    assert_eq!(display.chars().count(), 6);
    assert!(display.chars().all(|c| c == '•'), "masked: {display}");

    let reveal_tid = format!("__rui_input:{id}:reveal");
    let reveal = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(reveal_tid.as_str()))
        .expect("reveal btn");
    reveal.on_click.as_ref().unwrap().borrow_mut()();
    assert!(use_signal(&input_password_reveal_signal_key(id), || false).peek());

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let display = find_text(&el, &text_tid).unwrap().to_string();
    assert_eq!(display, "secret");
}

#[test]
fn adornment_slots_keep_stable_width_when_hidden() {
    let id = "diag-input-slot-stable";
    use_signal(&input_value_signal_key(id), String::new).set("ab".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new()); // unfocused

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .clearable(true)
        .into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    let clear_w = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    let eye_w = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(reveal_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    assert_eq!(clear_w, eye_w);

    use_signal("__rui_button_focus_id", String::new)
        .set(crate::components::input::focus_id_for_input(id));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    let clear_w2 = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    assert_eq!(
        clear_w, clear_w2,
        "focus must not change adornment slot width"
    );
}

#[test]
fn masked_hit_index_maps_to_real_caret() {
    use crate::components::input::map_masked_byte_index_to_real;
    let real = "secret";
    // 3rd bullet starts at byte 6 in "••••••" (each • is 3 bytes)
    assert_eq!(map_masked_byte_index_to_real(real, 6), 2);
    assert_eq!(map_masked_byte_index_to_real(real, 18), 6);
}

fn adorn_opacity(el: &Element, tid: &str) -> Option<f32> {
    el.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(tid))
        .map(|c| c.style.opacity)
}

#[test]
fn nonempty_makes_clear_visible_even_when_unfocused() {
    let id = "diag-adorn-opacity";
    use_signal(&input_value_signal_key(id), String::new).set("secret".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .clearable(true)
        .into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    assert_eq!(
        adorn_opacity(&el, &clear_tid),
        Some(1.0),
        "nonempty clear must be clickable while unfocused"
    );
    assert_eq!(
        adorn_opacity(&el, &reveal_tid),
        Some(0.0),
        "eye stays focused-only"
    );

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    assert_eq!(
        adorn_opacity(&el, &clear_tid),
        Some(1.0),
        "focused+nonempty clear must stay opaque"
    );
    assert_eq!(
        adorn_opacity(&el, &reveal_tid),
        Some(1.0),
        "focused eye must be opaque"
    );

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut root = Element::new().w(400.0).h(80.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let lucide: Vec<_> = result
        .text_nodes
        .iter()
        .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
        .collect();
    assert!(
        lucide.len() >= 2,
        "expected clear+eye lucide glyphs, got {}",
        lucide.len()
    );
    for n in &lucide {
        assert!(
            n.style.color[3] > 0,
            "lucide alpha 0: {:?} color={:?}",
            n.text,
            n.style.color
        );
        assert!(
            n.width > 0.5 && n.height > 0.5,
            "lucide zero size w={} h={}",
            n.width,
            n.height
        );
    }
}

#[test]
fn string_bool_props_mount_adornments_like_rsx() {
    use crate::components::{Input, InputProps};
    use crate::core::tree::parse_bool_like;

    let id = "diag-str-bool-adorn";
    let el = Input(InputProps {
        id: id.into(),
        placeholder: None,
        disabled: None,
        invalid: None,
        clearable: Some(parse_bool_like("true")),
        password_toggle: Some(parse_bool_like("true")),
        input_type: Some("password".into()),
        text_size: None,
        on_click: None,
        on_key_down: None,
        children: None,
        base: None,
    });
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    assert!(
        el.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(clear_tid.as_str())),
        "clearable=\"true\" path must mount clear; tids={:?}",
        el.children
            .iter()
            .map(|c| c.template_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(el
        .children
        .iter()
        .any(|c| c.template_id.as_deref() == Some(reveal_tid.as_str())));
}
