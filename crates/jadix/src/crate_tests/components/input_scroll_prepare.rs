//! Single-line Input caret-scroll must not `text::prepare` every chrome frame while
//! unfocused (gallery forms Input / Form rows). Multiline Textarea never hits this path.

use crate::components::input::input;
use crate::components::internal::text_edit::{
    sync_text_edit_paint_subtree, take_input_scroll_prepare_calls,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::vstack;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

#[test]
fn unfocused_single_line_inputs_skip_caret_scroll_prepare() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    use_signal("__jadix_button_focus_id", String::new).set(String::new());

    let mut root: Element = vstack()
        .w(400.0)
        .gap(8.0)
        .child(
            input("gal-like-a")
                .placeholder("很长的中文占位符用来模拟 gallery forms 单行输入")
                .clearable(true),
        )
        .child(
            input("gal-like-b")
                .placeholder("Placeholder…")
                .clearable(true),
        )
        .child(input("gal-like-c").placeholder("email").clearable(true))
        .child(
            input("gal-like-pw")
                .input_type("password")
                .password_toggle(true)
                .clearable(true),
        )
        .into();

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 600.0, [0.0, 0.0]);

    let mut impact = LayoutStatus::Clean;
    let _ = take_input_scroll_prepare_calls();
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    assert_eq!(
        take_input_scroll_prepare_calls(),
        0,
        "unfocused Inputs must not shape for caret scroll_x"
    );
    assert!(
        !matches!(
            impact,
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
        ),
        "steady unfocused sync should not force layout; got {impact:?}"
    );

    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    assert_eq!(
        take_input_scroll_prepare_calls(),
        0,
        "second frame must also skip prepare"
    );
}

#[test]
fn focused_nonempty_input_still_prepares_for_caret_scroll() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-like-focused";
    use_signal("__jadix_button_focus_id", String::new)
        .set(crate::components::input::focus_id_for_input(id));
    use_signal(
        &crate::components::input::input_value_signal_key(id),
        String::new,
    )
    .set("hello world that is long enough to pan".into());

    let mut el: Element = input(id).placeholder("…").into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 120.0, 40.0, [0.0, 0.0]);

    let mut impact = LayoutStatus::Clean;
    let _ = take_input_scroll_prepare_calls();
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    assert!(
        take_input_scroll_prepare_calls() >= 1,
        "focused nonempty Input still shapes for caret scroll"
    );
}
