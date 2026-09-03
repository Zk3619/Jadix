//! IME preedit must not inflate textarea soft-wrap leaf height (vertical jitter on CJK compose).

use crate::components::input::{
    input_ime_anchor_signal_key, input_preedit_sel_signal_key, input_preedit_signal_key,
    input_value_signal_key,
};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use crate::utils::text_field::{input_text_template_id, text_edit_line_box_px};

fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(f) = find_tid(c, tid) {
            return Some(f);
        }
    }
    None
}

#[test]
fn textarea_ime_preedit_does_not_inflate_soft_wrap_leaf_height() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "ta-ime-height";
    let tid = input_text_template_id(id);
    let size = 14.0_f32;
    let lh = text_edit_line_box_px(size, None, None);
    let committed = "你好世界";

    use_signal("__rui_button_focus_id", String::new).set(format!("textarea:{id}"));
    use_signal(&input_value_signal_key(id), String::new).set(committed.into());

    let el: Element = textarea(id).rows(4).into();
    let mut tree = LayoutTree::new();
    let mut root = Element::new().w(72.0).h(200.0).child(el);
    let _ = tree.build_and_layout(&mut root, 72.0, 200.0, [-1.0, -1.0]);

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let _ = tree.build_and_layout_fast(&mut root, 72.0, 200.0, [-1.0, -1.0], false, false);
    let h_committed = find_tid(&root, &tid).expect("text leaf").layout_bounds[3];
    assert!(
        h_committed > lh * 1.2,
        "narrow CJK should soft-wrap to multiple lines; lh={lh} got {h_committed}"
    );

    let anchor = committed.len();
    use_signal(&input_ime_anchor_signal_key(id), || None::<usize>).set(Some(anchor));
    use_signal(&input_preedit_signal_key(id), String::new).set("nihaoshijie".into());
    use_signal(&input_preedit_sel_signal_key(id), || {
        None::<(usize, usize)>
    })
    .set(Some((0, "nihaoshijie".len())));

    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root.children[0], &mut tree, &mut impact);
    let _ = tree.build_and_layout_fast(&mut root, 72.0, 200.0, [-1.0, -1.0], false, false);
    let h_preedit = find_tid(&root, &tid).expect("text leaf after preedit").layout_bounds[3];

    assert!(
        (h_preedit - h_committed).abs() <= 0.5,
        "IME Latin preedit must not change soft-wrap leaf height \
         (committed={h_committed} preedit={h_preedit})"
    );
}
