//! Focused Input must not attach Latin U+2009 shape_suffix (baseline / font mixing).

use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

#[test]
fn focused_cjk_input_has_no_shape_suffix() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-cjk-suffix";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("你好".into());

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let text_el = el
        .children
        .first()
        .and_then(|wrap| wrap.children.first())
        .expect("input text child");
    assert_eq!(text_el.text.as_deref(), Some("你好"));
    assert!(
        text_el.text_shape_suffix.is_none(),
        "focused CJK must not use U+2009; got {:?}",
        text_el.text_shape_suffix
    );
}

#[test]
fn focused_ascii_input_has_no_shape_suffix() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-ascii-suffix";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let text_el = el
        .children
        .first()
        .and_then(|wrap| wrap.children.first())
        .expect("input text child");
    assert!(
        text_el.text_shape_suffix.is_none(),
        "focused ASCII also skips U+2009; Input text always Advanced (stable vs caret); got {:?}",
        text_el.text_shape_suffix
    );
}
