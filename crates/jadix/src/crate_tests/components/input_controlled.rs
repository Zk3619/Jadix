//! N.7：受控 Input 编辑中外部同值 `set` 不丢 caret；合成中不拉 store。
//! Caret / 选区走 [`TextEditPaintSync`] peek，不建每帧 bind。

use crate::components::input::{
    focus_id_for_input, input, input_caret_signal_key, input_preedit_signal_key,
    input_selection_signal_key, input_value_signal_key,
};
use crate::core::reactivity::{create_root, take_flushed_effects, use_signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::InputCaretDraw;
use crate::utils::text_edit::sync_text_edit_paint_subtree;
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

#[test]
fn editor_foreground_survives_paint_edit_placeholder_and_disabled_states() {
    fn glyph<'a>(el: &'a Element, id: &str) -> &'a Element {
        fn find<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|child| find(child, tid))
        }
        find(el, &crate::utils::text_field::input_text_template_id(id)).unwrap()
    }
    let (_owner, ()) = create_root(|| {
        for multiline in [false, true] {
            for disabled in [false, true] {
                let id = format!("foreground-{multiline}-{disabled}");
                use_signal(&input_value_signal_key(&id), String::new).set("draft".into());
                let el: Element = if multiline {
                    crate::components::textarea::textarea(&id)
                        .placeholder("hint")
                        .disabled(disabled)
                        .into()
                } else {
                    input(&id).placeholder("hint").disabled(disabled).into()
                };
                let mut el = el.text_color([21, 73, 169, 200]);
                let mut tree = LayoutTree::new();
                let mut impact = LayoutStatus::Clean;
                let alpha = if disabled { 100 } else { 200 };
                sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
                assert_eq!(glyph(&el, &id).text_style.color, Some([21, 73, 169, alpha]));

                // A live class changes the root color through the same path as
                // reactive style bindings; subsequent edits must preserve it.
                el = el.class("text-red-500");
                let color = el.text_style.color.unwrap();
                use_signal(&input_value_signal_key(&id), String::new).set("updated".into());
                sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
                let mut expected = color;
                if disabled {
                    expected[3] = (expected[3] as f32 * 0.5) as u8;
                }
                assert_eq!(glyph(&el, &id).text_style.color, Some(expected));

                use_signal(&input_value_signal_key(&id), String::new).set(String::new());
                sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
                let mut muted = crate::style::parse_text_color("muted-foreground");
                if disabled {
                    muted[3] = (muted[3] as f32 * 0.5) as u8;
                }
                assert_eq!(glyph(&el, &id).text_style.color, Some(muted));
                assert_eq!(glyph(&el, &id).text.as_deref(), Some("hint"));
                use_signal(&input_value_signal_key(&id), String::new).set("restored".into());
                sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
                assert_eq!(glyph(&el, &id).text_style.color, Some(expected));
            }
        }
    });
}
