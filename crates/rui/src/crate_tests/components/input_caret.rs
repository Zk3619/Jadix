//! End-to-end: focused Input → sync → collect must emit input chrome instances
//! on the same InstanceInput path as borders (selection highlight + caret).

use crate::components::input::{
    focus_id_for_input, input, input_selection_signal_key, input_value_signal_key,
};
use crate::components::label::sync_label_selection_paint_subtree;
use crate::components::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::style::InputCaretDraw;
use crate::text::install_shared_font_system;

/// Live frame order: class variants → text_edit paint → `sync_label_selection`.
/// Label sync must not wipe Input-owned `input_caret`.
fn sync_paint_like_app(el: &mut Element, tree: &mut LayoutTree, impact: &mut LayoutStatus) {
    sync_text_edit_paint_subtree(el, tree, impact);
    sync_label_selection_paint_subtree(el, impact);
}

#[test]
fn focused_input_emits_selection_chrome_in_collect() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-chrome";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 5)));

    let mut el: Element = input(id).placeholder("…").into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let caret = el
        .children
        .first()
        .and_then(|wrap| wrap.children.first())
        .and_then(|c| c.text_style.input_caret.as_ref());
    assert!(
        caret.is_some(),
        "sync must install input_caret (and label sync must not wipe it)"
    );

    let mut root = Element::new().w(400.0).h(80.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let caret_nodes: Vec<&InputCaretDraw> = result
        .text_nodes
        .iter()
        .filter_map(|n| n.style.input_caret.as_ref())
        .collect();
    assert!(!caret_nodes.is_empty(), "collect must preserve input_caret");
    assert_eq!(caret_nodes[0].selection, Some((0, 5)));
    let sel_bg = caret_nodes[0].selection_bg;
    let want = [
        sel_bg[0] as f32 / 255.0,
        sel_bg[1] as f32 / 255.0,
        sel_bg[2] as f32 / 255.0,
    ];
    let has_sel = result.instances.iter().any(|i| {
        (i.color[0] - want[0]).abs() < 0.02
            && (i.color[1] - want[1]).abs() < 0.02
            && (i.color[2] - want[2]).abs() < 0.02
            && i.color[3] > 0.05
            && i.rect_size[0] > 1.0
    });
    assert!(
        has_sel,
        "selection must land in main instances (under glyphs); none matched selection_bg"
    );
}

#[test]
fn focused_input_emits_caret_chrome_when_no_selection() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-only";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hi".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(None);

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);

    let mut root = Element::new().w(400.0).h(80.0).child(el);
    // Retry across blink phases (~520ms).
    let mut saw = false;
    for _ in 0..6 {
        let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
        if !result.input_chrome_instances.is_empty() {
            saw = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(280));
        if let Some(child) = root.children.first_mut() {
            sync_paint_like_app(child, &mut tree, &mut impact);
        }
    }
    assert!(
        saw,
        "caret hairline must appear in input_chrome_instances during blink ON"
    );
}
