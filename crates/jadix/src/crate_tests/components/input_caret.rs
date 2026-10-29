//! End-to-end: focused Input → sync → collect must emit input chrome instances
//! on the same InstanceInput path as borders (selection highlight + caret).

use crate::components::input::{
    focus_id_for_input, input, input_selection_signal_key, input_value_signal_key,
};
use crate::components::label::sync_label_selection_paint_subtree;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::style::{parse_text_color, InputCaretDraw};
use crate::text::install_shared_font_system;
use crate::utils::text_edit::sync_text_edit_paint_subtree;
use crate::utils::text_field::focus_id_for_text_edit;

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
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
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

fn find_caret_leaf(el: &Element) -> Option<&Element> {
    if el.text_style.input_caret.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(find_caret_leaf)
}

#[test]
fn ime_commit_with_unchanged_text_repairs_entire_preedit_run() {
    use crate::components::input::{
        input_caret_signal_key, input_ime_anchor_signal_key, input_preedit_sel_signal_key,
        input_preedit_signal_key,
    };
    for multiline in [false, true] {
        let id = if multiline {
            "ime-commit-ta"
        } else {
            "ime-commit-input"
        };
        use_signal("__jadix_button_focus_id", String::new)
            .set(focus_id_for_text_edit(id, multiline));
        let value = use_signal(&input_value_signal_key(id), String::new);
        value.set(String::new());
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(0));
        use_signal(&input_ime_anchor_signal_key(id), || None::<usize>).set(Some(0));
        let preedit = use_signal(&input_preedit_signal_key(id), String::new);
        let cursor = use_signal(&input_preedit_sel_signal_key(id), || None::<(usize, usize)>);
        preedit.set("Thus".into());
        cursor.set(Some((4, 4)));
        let field: Element = if multiline {
            textarea(id).into()
        } else {
            input(id).into()
        };
        let mut root = Element::new().w(400.0).h(160.0).child(field);
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 400.0, 160.0, [-1.0, -1.0]);
        sync_paint_like_app(&mut root, &mut tree, &mut LayoutStatus::Clean);
        let leaf = find_caret_leaf(&root).unwrap();
        assert_eq!(leaf.text.as_deref(), Some("Thus"));
        assert!(leaf
            .text_style
            .input_caret
            .as_ref()
            .unwrap()
            .preedit_underline
            .is_some());
        let key = crate::app::damage::paint_element_key(leaf);
        crate::app::damage::take_frame_paint_nodes();
        value.set("Thus".into());
        preedit.set(String::new());
        cursor.set(None);
        sync_paint_like_app(&mut root, &mut tree, &mut LayoutStatus::Clean);
        let leaf = find_caret_leaf(&root).unwrap();
        assert_eq!(leaf.text.as_deref(), Some("Thus"));
        assert!(leaf
            .text_style
            .input_caret
            .as_ref()
            .unwrap()
            .preedit_underline
            .is_none());
        assert!(
            crate::app::damage::take_frame_paint_nodes()
                .iter()
                .any(|dirty| dirty.key == key
                    && dirty.kind == crate::app::damage::PaintDirtyKind::Region),
            "committing identical display text must erase retained underline ink"
        );
    }
}

#[test]
fn ime_reversed_range_keeps_input_and_textarea_caret_after_inline_candidates() {
    use crate::components::input::{
        input_caret_signal_key, input_ime_anchor_signal_key, input_preedit_sel_signal_key,
        input_preedit_signal_key,
    };
    for multiline in [false, true] {
        let id = if multiline {
            "ime-range-ta"
        } else {
            "ime-range-input"
        };
        use_signal("__jadix_button_focus_id", String::new)
            .set(focus_id_for_text_edit(id, multiline));
        use_signal(&input_value_signal_key(id), String::new).set("前后".into());
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(3));
        use_signal(&input_ime_anchor_signal_key(id), || None::<usize>).set(Some(3));
        let preedit = use_signal(&input_preedit_signal_key(id), String::new);
        let cursor = use_signal(&input_preedit_sel_signal_key(id), || None::<(usize, usize)>);
        preedit.set("甲乙丙".into());
        cursor.set(Some((9, 3)));
        let mut el: Element = if multiline {
            textarea(id).into()
        } else {
            input(id).into()
        };
        let assert_caret = |el: &Element, at| {
            let caret = find_caret_leaf(el)
                .unwrap()
                .text_style
                .input_caret
                .as_ref()
                .unwrap();
            assert_eq!(caret.caret_byte_index, at);
        };
        assert_caret(&el, 12);
        let mut tree = LayoutTree::new();
        for (candidate, range, at) in [("甲丁丙戊己庚", (18, 6), 21), ("甲丁丙戊己庚", (3, 3), 6)]
        {
            preedit.set(candidate.into());
            cursor.set(Some(range));
            sync_paint_like_app(&mut el, &mut tree, &mut LayoutStatus::Clean);
            assert_caret(&el, at);
            assert_eq!(
                find_caret_leaf(&el).unwrap().text.as_deref(),
                Some(format!("前{candidate}后").as_str())
            );
        }
    }
}

#[test]
fn ime_hidden_caret_stays_hidden_while_inline_candidates_change() {
    use crate::components::input::{
        input_caret_signal_key, input_ime_anchor_signal_key, input_preedit_sel_signal_key,
        input_preedit_signal_key,
    };

    for multiline in [false, true] {
        let id = if multiline {
            "ime-inline-ta"
        } else {
            "ime-inline-input"
        };
        use_signal("__jadix_button_focus_id", String::new)
            .set(focus_id_for_text_edit(id, multiline));
        let value = use_signal(&input_value_signal_key(id), String::new);
        value.set("前后".into());
        let anchor = "前".len();
        use_signal(&input_caret_signal_key(id), || None::<usize>).set(Some(anchor));
        use_signal(&input_ime_anchor_signal_key(id), || None::<usize>).set(Some(anchor));
        let preedit = use_signal(&input_preedit_signal_key(id), String::new);
        let cursor = use_signal(&input_preedit_sel_signal_key(id), || None::<(usize, usize)>);
        preedit.set("你".into());
        cursor.set(None); // winit: IME requests a hidden cursor, not cursor-at-end.
        let mut el: Element = if multiline {
            textarea(id).into()
        } else {
            input(id).into()
        };
        let assert_hidden = |el: &Element| {
            let leaf = find_caret_leaf(el).expect("keep composition underline and geometry");
            let caret = leaf.text_style.input_caret.as_ref().unwrap();
            assert_eq!(
                caret.caret_color.unwrap()[3],
                0,
                "IME-hidden caret must not flash after a candidate"
            );
            assert!(caret.preedit_underline.is_some());
        };
        assert_hidden(&el); // Initial build must agree with subsequent paint sync.
        let mut tree = LayoutTree::new();
        for candidate in ["你好", "拟好", "你"] {
            preedit.set(candidate.into());
            let mut impact = LayoutStatus::Clean;
            sync_paint_like_app(&mut el, &mut tree, &mut impact);
            assert_hidden(&el);
            assert_eq!(
                find_caret_leaf(&el).unwrap().text.as_deref(),
                Some(format!("前{candidate}后").as_str())
            );
            assert_eq!(
                value.peek(),
                "前后",
                "candidate preview must not commit or overwrite text"
            );
        }
        cursor.set(Some(("你".len(), "你".len())));
        sync_paint_like_app(&mut el, &mut tree, &mut LayoutStatus::Clean);
        assert_ne!(
            find_caret_leaf(&el)
                .unwrap()
                .text_style
                .input_caret
                .as_ref()
                .unwrap()
                .caret_color
                .unwrap()[3],
            0
        );
        preedit.set(String::new());
        cursor.set(None);
        sync_paint_like_app(&mut el, &mut tree, &mut LayoutStatus::Clean);
        assert_ne!(
            find_caret_leaf(&el)
                .unwrap()
                .text_style
                .input_caret
                .as_ref()
                .unwrap()
                .caret_color
                .unwrap()[3],
            0,
            "clearing preedit restores the ordinary caret"
        );
    }
}

fn rgba8_to_rgb_f32(c: [u8; 4]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

fn rgb_close(got: [f32; 4], want: [f32; 3]) -> bool {
    (got[0] - want[0]).abs() < 0.02
        && (got[1] - want[1]).abs() < 0.02
        && (got[2] - want[2]).abs() < 0.02
}

fn assert_placeholder_caret_uses_foreground(mut el: Element) {
    let muted = parse_text_color("muted-foreground");
    let fg = parse_text_color("caret");
    assert_ne!(
        muted, fg,
        "theme must distinguish placeholder vs caret color"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);

    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    assert_eq!(
        leaf.text_style.color_resolved(),
        muted,
        "placeholder glyphs stay muted-foreground"
    );
    assert_eq!(
        leaf.text_style
            .input_caret
            .as_ref()
            .and_then(|c| c.caret_color),
        Some(fg),
        "I-beam must keep token `caret` while the node shows placeholder"
    );

    let mut root = Element::new().w(400.0).h(160.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 160.0, [-1.0, -1.0]);
    assert!(
        !result.input_chrome_instances.is_empty(),
        "focused empty field must emit a caret hairline"
    );
    let fg_rgb = rgba8_to_rgb_f32(fg);
    let muted_rgb = rgba8_to_rgb_f32(muted);
    for inst in &result.input_chrome_instances {
        assert!(
            rgb_close(inst.color, fg_rgb),
            "caret chrome must use foreground, got {:?}",
            inst.color
        );
        assert!(
            !rgb_close(inst.color, muted_rgb),
            "caret chrome must not use placeholder muted-foreground, got {:?}",
            inst.color
        );
    }
}

#[test]
fn focused_input_placeholder_caret_uses_foreground() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-placeholder";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let el: Element = input(id).placeholder("Type here").into();
    assert_placeholder_caret_uses_foreground(el);
}

#[test]
fn focused_textarea_placeholder_caret_uses_foreground() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-placeholder-ta";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_text_edit(id, true));
    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let el: Element = textarea(id).placeholder("Type here").into();
    assert_placeholder_caret_uses_foreground(el);
}

#[test]
fn focused_input_with_value_caret_still_uses_foreground() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-value-fg";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());

    let mut el: Element = input(id).placeholder("Type here").into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    let fg = parse_text_color("caret");
    assert_eq!(leaf.text_style.color_resolved(), fg);
    assert_eq!(
        leaf.text_style
            .input_caret
            .as_ref()
            .and_then(|c| c.caret_color),
        Some(fg)
    );
}

#[test]
fn input_caret_color_override_survives_placeholder() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-caret-color-override";
    let want = [16, 32, 48, 255];
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set(String::new());

    let mut el: Element = input(id).placeholder("Type here").caret_color(want).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    assert_eq!(
        leaf.text_style.color_resolved(),
        parse_text_color("muted-foreground")
    );
    assert_eq!(
        leaf.text_style
            .input_caret
            .as_ref()
            .and_then(|c| c.caret_color),
        Some(want)
    );
}

#[test]
fn input_selection_background_override_is_used_as_is() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-sel-color-override";
    let want = [10, 200, 80, 90];
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 5)));

    let mut el: Element = input(id).selection_background(want).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    let caret = leaf.text_style.input_caret.as_ref().expect("caret");
    assert_eq!(caret.selection_bg, want);
    assert_eq!(caret.selection, Some((0, 5)));
    assert_eq!(
        caret.selection_foreground,
        Some(InputCaretDraw::default_selection_foreground())
    );
}

#[test]
fn input_caret_and_selection_default_to_theme_tokens() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-sel-token-defaults";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 5)));

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    let caret = leaf.text_style.input_caret.as_ref().expect("caret");
    assert_eq!(caret.caret_color, Some(parse_text_color("caret")));
    assert_eq!(caret.selection_bg, parse_text_color("selection-background"));
    assert_eq!(
        caret.selection_foreground,
        Some(parse_text_color("selection-foreground"))
    );
    assert_eq!(
        caret.selection_recolor(),
        Some(((0, 5), parse_text_color("selection-foreground")))
    );
}

#[test]
fn input_selection_foreground_override_is_used_as_is() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-sel-fg-override";
    let want = [255, 255, 255, 255];
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 5)));

    let mut el: Element = input(id).selection_foreground(want).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    let caret = leaf.text_style.input_caret.as_ref().expect("caret");
    assert_eq!(caret.selection_foreground, Some(want));
    assert_eq!(caret.selection_recolor(), Some(((0, 5), want)));
}

#[test]
fn textarea_caret_and_selection_color_overrides() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-ta-color-override";
    let caret = [8, 8, 8, 255];
    let sel = [40, 80, 200, 70];
    let fg = [250, 250, 250, 255];
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_text_edit(id, true));
    use_signal(&input_value_signal_key(id), String::new).set("ab".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(Some((0, 2)));

    let mut el: Element = textarea(id)
        .caret_color(caret)
        .selection_background(sel)
        .selection_foreground(fg)
        .into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused textarea must install input_caret");
    let draw = leaf.text_style.input_caret.as_ref().expect("caret");
    assert_eq!(draw.caret_color, Some(caret));
    assert_eq!(draw.selection_bg, sel);
    assert_eq!(draw.selection_foreground, Some(fg));
}

#[test]
fn extra_carets_install_on_focused_input_paint() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-extra-carets";
    use_signal("__jadix_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal(&input_selection_signal_key(id), || None::<(usize, usize)>).set(None);
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(1));
    use_signal(
        &crate::components::input::input_extra_carets_signal_key(id),
        Vec::<(usize, usize)>::new,
    )
    .set(vec![(3usize, 3usize), (5, 5)]);

    let mut el: Element = input(id).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_paint_like_app(&mut el, &mut tree, &mut impact);
    let leaf = find_caret_leaf(&el).expect("focused field must install input_caret");
    let draw = leaf.text_style.input_caret.as_ref().expect("caret");
    assert_eq!(draw.caret_byte_index, 1);
    assert_eq!(draw.extra_carets, vec![3, 5]);
}
