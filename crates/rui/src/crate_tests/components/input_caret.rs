//! End-to-end: focused Input → sync → collect must emit input chrome instances
//! on the same InstanceInput path as borders (selection highlight + caret).

use crate::components::input::{
    focus_id_for_input, input, input_selection_signal_key, input_value_signal_key,
};
use crate::components::internal::text_edit::sync_text_edit_paint_subtree;
use crate::components::label::sync_label_selection_paint_subtree;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::style::{parse_text_color, InputCaretDraw};
use crate::text::install_shared_font_system;
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

fn find_caret_leaf(el: &Element) -> Option<&Element> {
    if el.text_style.input_caret.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(find_caret_leaf)
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_text_edit(id, true));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_text_edit(id, true));
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
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
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
