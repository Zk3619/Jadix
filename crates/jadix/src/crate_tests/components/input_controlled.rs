//! N.7：受控 Input 编辑中外部同值 `set` 不丢 caret；合成中不拉 store。
//! Caret / 选区走 [`TextEditPaintSync`] peek，不建每帧 bind。

use crate::components::input::{
    focus_id_for_input, input, input_caret_signal_key, input_preedit_signal_key,
    input_selection_signal_key, input_value_signal_key,
};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::core::reactivity::{create_root, take_flushed_effects, use_signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::InputCaretDraw;
use crate::utils::text_field::{bind_controlled_value, pull_controlled_into_store};

fn find_input_caret(el: &Element) -> Option<&InputCaretDraw> {
    if let Some(c) = el.text_style.input_caret.as_ref() {
        return Some(c);
    }
    el.children.iter().find_map(find_input_caret)
}

#[test]
fn focused_same_value_external_set_keeps_caret_through_paint_sync() {
    let (_owner, ()) = create_root(|| {
        let id = "n7-same-val";
        let ext = use_signal("__jadix_n7_same_val", || "hello".to_string());
        bind_controlled_value(id, ext);
        use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
        use_signal(&input_value_signal_key(id), String::new).set("hello".into());
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(2));
        use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((1, 4)));

        let mut el: Element = input(id).value_signal(ext).into();
        let tid = el.template_id.clone();
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

        let before = find_input_caret(&el)
            .map(|c| (c.caret_byte_index, c.selection))
            .expect("caret after first sync");
        assert_eq!(before.0, 2);
        assert_eq!(before.1, Some((1, 4)));

        let _ = take_flushed_effects();
        ext.set("hello".into());
        assert!(
            take_flushed_effects().is_empty(),
            "same-value PartialEq set must not flush leaf binds"
        );
        assert!(
            pull_controlled_into_store(id, "hello", true).is_none(),
            "focused same-value set must not overwrite the store"
        );

        impact = LayoutStatus::Clean;
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
        let after = find_input_caret(&el)
            .map(|c| (c.caret_byte_index, c.selection))
            .expect("caret after same-value set");
        assert_eq!(after, before, "paint sync must not steal caret/selection");
        assert_eq!(
            el.template_id, tid,
            "same-value set must not rebuild the Input root"
        );
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "hello"
        );
    });
}

#[test]
fn composing_blocks_external_pull_before_paint() {
    let (_owner, ()) = create_root(|| {
        let id = "n7-ime";
        let ext = use_signal("__jadix_n7_ime", || "seed".to_string());
        bind_controlled_value(id, ext);
        use_signal(&input_preedit_signal_key(id), String::new).set("ni".into());
        ext.set("other".into());
        assert!(
            pull_controlled_into_store(id, "seed", false).is_none(),
            "uncommitted IME must not pull an external set into the store"
        );
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "seed"
        );
    });
}
