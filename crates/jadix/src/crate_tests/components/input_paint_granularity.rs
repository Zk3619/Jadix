//! Hold-key typing/delete must stay on the paint-only Input path and must not
//! reshape the live string twice per chrome frame (prefix == full text at end caret).

use crate::components::input::{
    focus_id_for_input, input, input_caret_signal_key, input_value_signal_key,
};
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;
use crate::utils::text_edit::{sync_text_edit_paint_subtree, take_input_scroll_prepare_calls};
use crate::utils::text_field::input_text_template_id;

fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, tid))
}

#[test]
fn text_fields_clear_selection_on_blur_before_refocus() {
    use crate::utils::text_field::{focus_id_for_text_edit, input_selection_signal_key};
    let focus = use_signal("__jadix_button_focus_id", String::new);
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        for multiline in [false, true] {
            let id = if multiline { "blur-ta" } else { "blur-input" };
            focus.set(focus_id_for_text_edit(id, multiline));
            let selection = use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>);
            let _el: Element = if multiline {
                textarea(id).into()
            } else {
                input(id).into()
            };
            selection.set(Some((0, 4)));
            focus.set("textarea:other".into());
            assert_eq!(selection.peek(), None, "blur must clear {id} immediately");
            focus.set(focus_id_for_text_edit(id, multiline));
            assert_eq!(
                selection.peek(),
                None,
                "refocus must not resurrect {id}'s selection"
            );
        }
    });
}

#[test]
fn changing_blur_policy_replaces_the_focus_subscription() {
    let id = "blur-policy-rebuild";
    let focus = use_signal("__jadix_button_focus_id", String::new);
    focus.set(focus_id_for_input(id));
    let _owner = crate::core::reactivity::create_root(|| {
        let _clear: Element = input(id).into();
        let _retain: Element = input(id).selection_on_blur("retain").into();
        let selection = use_signal(
            &crate::utils::text_field::input_selection_signal_key(id),
            || None::<(usize, usize)>,
        );
        selection.set(Some((0, 4)));
        focus.set("input:policy-other".into());
        assert_eq!(
            selection.peek(),
            Some((0, 4)),
            "old clear subscription must be released"
        );
    });
}

#[test]
fn text_fields_retain_muted_selection_and_scroll_on_blur() {
    use crate::utils::text_field::{focus_id_for_text_edit, input_selection_signal_key};
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let focus = use_signal("__jadix_button_focus_id", String::new);
    for multiline in [false, true] {
        let id = if multiline {
            "retain-ta"
        } else {
            "retain-input"
        };
        use_signal(&input_value_signal_key(id), String::new).set("long selected text ".repeat(20));
        focus.set(focus_id_for_text_edit(id, multiline));
        let selection = use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>);
        selection.set(Some((0, 18)));
        let mut el: Element = if multiline {
            textarea(id).wrap("off").selection_on_blur("retain").into()
        } else {
            input(id).selection_on_blur("retain").into()
        };
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut el, 240.0, 120.0, [-1.0, -1.0]);
        let mut impact = LayoutStatus::Clean;
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
        let leaf = find_tid(&el, &input_text_template_id(id)).unwrap();
        let active = leaf.text_style.input_caret.clone().unwrap();
        let scroll_x = leaf.text_style.scroll_x;
        focus.set("textarea:other-retain".into());
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
        assert_eq!(selection.peek(), Some((0, 18)));
        let leaf = find_tid(&el, &input_text_template_id(id)).unwrap();
        let inactive = leaf.text_style.input_caret.as_ref().unwrap();
        assert_eq!(inactive.selection, active.selection);
        assert!(inactive.selection_bg[3] < active.selection_bg[3]);
        assert_eq!(
            inactive.selection_foreground,
            Some(leaf.text_style.color_resolved())
        );
        assert_eq!(inactive.caret_color, Some([0; 4]));
        assert_eq!(leaf.text_style.scroll_x, scroll_x);
        let rebuilt: Element = if multiline {
            textarea(id).wrap("off").selection_on_blur("retain").into()
        } else {
            input(id).selection_on_blur("retain").into()
        };
        assert_eq!(
            find_tid(&rebuilt, &input_text_template_id(id))
                .unwrap()
                .text_style
                .input_caret
                .as_ref()
                .unwrap()
                .selection_bg,
            inactive.selection_bg
        );
        focus.set(focus_id_for_text_edit(id, multiline));
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
        assert_eq!(
            find_tid(&el, &input_text_template_id(id))
                .unwrap()
                .text_style
                .input_caret
                .as_ref()
                .unwrap()
                .selection_bg,
            active.selection_bg
        );
    }
}

#[test]
fn focused_end_caret_input_prepares_once_per_paint() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "hold-key-end-caret";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    // Caret at end (hold-repeat insert / backspace-from-end).
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(5));

    let mut el: Element = input(id).placeholder("…").into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 240.0, 40.0, [0.0, 0.0]);

    let mut impact = LayoutStatus::Clean;
    let _ = take_input_scroll_prepare_calls();
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    assert_eq!(
        take_input_scroll_prepare_calls(),
        1,
        "end-caret Input must shape the live string once, not full+prefix"
    );
}

#[test]
fn hold_repeat_insert_at_end_does_not_double_prepare() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "hold-key-burst";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    let val = use_signal(&input_value_signal_key(id), String::new);
    val.set(String::new());
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(0));

    let mut el: Element = input(id).placeholder("…").into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 240.0, 40.0, [0.0, 0.0]);

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    let _ = take_input_scroll_prepare_calls();

    let n = 40_usize;
    for i in 1..=n {
        let s = "a".repeat(i);
        val.set(s.clone());
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(s.len()));
        impact = LayoutStatus::Clean;
        sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
        assert!(
            !matches!(
                impact,
                LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
            ),
            "fixed-size Input typing must stay StyleChanged/Clean; got {impact:?} at len={i}"
        );
        let leaf = find_tid(&el, &input_text_template_id(id)).expect("text leaf");
        assert_eq!(leaf.text.as_deref(), Some(s.as_str()));
    }
    assert_eq!(
        take_input_scroll_prepare_calls(),
        n,
        "each hold-repeat insert at end caret must prepare once"
    );
}

#[test]
fn textarea_same_line_insert_does_not_layout_on_every_key() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "hold-key-ta";
    use_signal("__jadix_button_focus_id", String::new)
        .set(crate::utils::text_field::textarea_focus_id(id));
    let val = use_signal(&input_value_signal_key(id), String::new);
    val.set("hello".into());
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(5));

    let mut root: Element = textarea(id).rows(4).resizable(false).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 360.0, 200.0, [0.0, 0.0]);

    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    // Settle leaf height / line-box pin from the first sync, then type.
    let _ = tree.build_and_layout(&mut root, 360.0, 200.0, [0.0, 0.0]);
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let settled = impact;

    val.set("helloa".into());
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(6));
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    // Adding one ASCII char on an existing visual line must not dirty Taffy
    // (that FullClears the gallery on hold-repeat). Height is unchanged.
    assert!(
        !matches!(
            impact,
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
        ),
        "same-line textarea insert must stay paint-only; got {impact:?} (settled {settled:?})"
    );
}

#[test]
fn drag_select_does_not_jump_scroll_to_line_end() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "in-drag-no-jump";
    let long = "abcdefghijklmnopqrstuvwxyz0123456789".repeat(8);
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set(long.clone());
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(long.len()));
    crate::utils::text_field::set_text_drag_select_active(true);

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 80.0, 40.0, [0.0, 0.0]);
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let sx = use_signal(
        &crate::utils::text_field::input_scroll_x_signal_key(id),
        || 0.0_f32,
    )
    .peek();
    crate::utils::text_field::set_text_drag_select_active(false);
    assert!(
        sx.abs() < 1.0,
        "drag-select must not ensure-caret-jump to the line end; sx={sx}"
    );
}

#[test]
fn panned_input_caret_damage_stays_inside_its_paint_clip() {
    use crate::app::damage::take_frame_paint_nodes;
    use crate::style::{InputCaretDraw, TextOverflow};
    use crate::utils::text_edit::push_focused_text_edit_caret_damage;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let text = "abcdefghijklmnopqrstuvwxyz";
    let mut leaf = Element::new().text(text).text_size(14.0);
    leaf.layout_bounds = [10.0, 20.0, 40.0, 20.0];
    leaf.text_style.overflow = TextOverflow::editable_single_line();
    leaf.text_style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        ..Default::default()
    });
    let _ = take_frame_paint_nodes();
    push_focused_text_edit_caret_damage(&leaf);
    assert!(
        take_frame_paint_nodes().is_empty(),
        "fully clipped caret must not dirty retained text"
    );

    let prefix = crate::text::measure_prefix_advance(text, 14.0, 400, 20.0);
    leaf.text_style.scroll_x = prefix - 41.0;
    push_focused_text_edit_caret_damage(&leaf);
    let right = take_frame_paint_nodes();
    assert_eq!(right.len(), 1);
    assert!((right[0].bounds[0] - 51.0).abs() < 0.05);
    assert!((right[0].bounds[2] - 1.0).abs() < 0.05);

    leaf.text_style
        .input_caret
        .as_mut()
        .unwrap()
        .caret_byte_index = 0;
    leaf.text_style.scroll_x = 3.0;
    push_focused_text_edit_caret_damage(&leaf);
    let left = take_frame_paint_nodes();
    assert_eq!(left.len(), 1);
    assert!((left[0].bounds[0] - 8.0).abs() < 0.05);
    assert!((left[0].bounds[2] - 1.0).abs() < 0.05);

    leaf.text_style.scroll_x = 20.0;
    push_focused_text_edit_caret_damage(&leaf);
    assert!(take_frame_paint_nodes().is_empty());
}

#[test]
fn widening_viewport_clamps_overflow_scroll_same_collect() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "in-widen-clamp";
    let long = "abcdefghijklmnopqrstuvwxyz0123456789".repeat(8);
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set(long.clone());
    use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(long.len()));

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 80.0, 40.0, [0.0, 0.0]);
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut el, 80.0, 40.0, [0.0, 0.0]);

    let sx_narrow = use_signal(
        &crate::utils::text_field::input_scroll_x_signal_key(id),
        || 0.0_f32,
    )
    .peek();
    assert!(
        sx_narrow > 40.0,
        "narrow overflow must pan the end caret, sx={sx_narrow}"
    );

    // Same-frame resize: layout+collect without another paint (paint still has stale bounds).
    let result = tree.build_and_layout(&mut el, 400.0, 40.0, [0.0, 0.0]);
    let leaf = find_tid(&el, &input_text_template_id(id)).expect("text leaf");
    let sx_wide = leaf.text_style.scroll_x;
    assert!(
        sx_wide < sx_narrow - 20.0,
        "collect must drop leftover pan when the clip grows; narrow={sx_narrow} wide={sx_wide}"
    );
    let node = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(input_text_template_id(id).as_str()))
        .expect("input text node");
    let expected_x = leaf.layout_bounds[0] - sx_wide;
    assert!(
        (node.x - expected_x).abs() < 1.5,
        "glyphs must pan with the clamped offset; node.x={} expected={expected_x} sx={sx_wide}",
        node.x
    );
}
