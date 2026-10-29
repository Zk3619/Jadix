use super::*;
use crate::core::tree::Element;
use crate::layout::dimension::{dim_length_px, lp_length_px};

#[test]
fn cssom_text_alignment_roundtrip_and_invalid_values() {
    let mut element = Element::new();
    assert_eq!(
        apply_cssom_text(
            &mut element,
            "text-align: justify; text-align-last: center;"
        ),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(
        element.text_style.align,
        Some(crate::style::TextAlign::Justify)
    );
    assert_eq!(
        element.text_style.align_last,
        Some(crate::style::TextAlignLast::Center)
    );
    let css = crate::app::devtools::cssom_text_from_element(&element);
    assert!(css.contains("text-align: justify;"));
    assert!(css.contains("text-align-last: center;"));
    let mut restored = Element::new();
    apply_cssom_text(&mut restored, &css);
    assert_eq!(restored.text_style.align, element.text_style.align);
    assert_eq!(
        restored.text_style.align_last,
        element.text_style.align_last
    );
    for css in [
        "text-align: unknown;",
        "text-align-last: safe center;",
        "text-align-last: right trailing;",
    ] {
        assert!(cssom_alignment_error(css).is_some());
        let before = element.text_style.clone();
        assert_eq!(apply_cssom_text(&mut element, css), LayoutStatus::Clean);
        assert_eq!(element.text_style, before);
    }
    assert_eq!(cssom_alignment_error("align-items: FIRST\tBASELINE;"), None);
    apply_cssom_text(&mut element, "align-items: FIRST\tBASELINE;");
    assert_eq!(
        element.style.align_items,
        Some(taffy::style::AlignItems::BASELINE)
    );
}

#[test]
fn committed_override_follows_a_unique_authored_node_after_rebuild() {
    clear_committed_inspector_edits_for_test();
    let mut root = Element::new().child(
        Element::new()
            .hot_reload_node_type("Text")
            .template_id("source:12:4")
            .text("before"),
    );
    commit_inspector_cssom(&root, &[0], "padding: 23px;".into());
    let mut rebuilt = Element::new().children([
        Element::new().template_id("source:11:4"),
        Element::new()
            .hot_reload_node_type("Text")
            .template_id("source:12:4")
            .text("after"),
    ]);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    crate::app::test_support::reconcile_elements(&mut rebuilt, &mut root);
    reapply_inspector_overrides(&mut rebuilt, &mut tree);
    assert_ne!(
        lp_length_px(rebuilt.children[0].style.padding.top),
        Some(23.0)
    );
    assert_eq!(
        lp_length_px(rebuilt.children[1].style.padding.top),
        Some(23.0)
    );
    assert!(committed_inspector_cssom(&[1], rebuilt.children[1].inspector_instance_id).is_some());
    clear_committed_inspector_edits_for_test();
}

#[test]
fn overrides_belong_to_instances_across_window_roots_and_are_released() {
    clear_committed_inspector_edits_for_test();
    let mut first = Element::new().child(Element::new());
    let mut second = Element::new().child(Element::new());
    let first_id = first.children[0].inspector_instance_id;
    let second_id = second.children[0].inspector_instance_id;
    commit_inspector_cssom(&first, &[0], "padding: 11px;".into());
    commit_inspector_cssom(&second, &[0], "padding: 22px;".into());
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    reapply_inspector_overrides(&mut first, &mut tree);
    reapply_inspector_overrides(&mut second, &mut tree);
    assert_eq!(
        lp_length_px(first.children[0].style.padding.top),
        Some(11.0)
    );
    assert_eq!(
        lp_length_px(second.children[0].style.padding.top),
        Some(22.0)
    );
    drop(first);
    assert!(committed_inspector_cssom(&[0], first_id).is_none());
    assert!(committed_inspector_cssom(&[0], second_id).is_some());
    clear_committed_inspector_edits_for_test();
}

#[test]
fn committed_override_skips_ambiguous_or_anonymous_remounts() {
    for identified in [false, true] {
        clear_committed_inspector_edits_for_test();
        let make = || {
            if identified {
                Element::new().template_id("repeated")
            } else {
                Element::new()
            }
        };
        let root = Element::new().child(make());
        commit_inspector_cssom(&root, &[0], "padding: 23px;".into());
        let mut rebuilt = Element::new().children([make(), make()]);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        reapply_inspector_overrides(&mut rebuilt, &mut tree);
        assert!(rebuilt
            .children
            .iter()
            .all(|el| lp_length_px(el.style.padding.top) != Some(23.0)));
    }
    clear_committed_inspector_edits_for_test();
}

#[test]
fn committed_override_does_not_leak_to_a_replacement_at_the_same_path() {
    clear_committed_inspector_edits_for_test();
    let mut root = Element::new().child(Element::new().focus_id("original"));
    commit_inspector_cssom(&root, &[0], "padding: 23px;".into());
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    reapply_inspector_overrides(&mut root, &mut tree);
    root.children[0] = Element::new().focus_id("replacement");
    reapply_inspector_overrides(&mut root, &mut tree);
    assert_ne!(lp_length_px(root.children[0].style.padding.top), Some(23.0));
    clear_committed_inspector_edits_for_test();
}

#[test]
fn pending_inspector_edits_queue_and_take() {
    let _ = take_pending_inspector_edits(); // clear
    queue_inspector_cssom(vec![2], 7, "padding: 8px;".into());
    let edits = take_pending_inspector_edits();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].path, vec![2]);
    assert_eq!(edits[0].css, "padding: 8px;");
    assert!(take_pending_inspector_edits().is_empty());
}

#[test]
fn committed_overrides_reapply_after_style_wipe() {
    clear_committed_inspector_edits_for_test();
    let mut child = Element::new();
    let _ = crate::style::apply_class_list(&mut child, "p-2");
    let mut root = Element::new().child(child);
    commit_inspector_cssom(&root, &[0], "padding: 20px;".into());
    // Simulate chrome / variant restore wiping live style back to class base.
    if let Some(cs) = root.children[0].class_set.as_ref() {
        if let Some(base) = cs.base_style.as_ref() {
            root.children[0].style = Box::new(base.clone());
        }
    }
    let mut layout = crate::layout::layout_tree::LayoutTree::new();
    let _ = reapply_inspector_overrides(&mut root, &mut layout);
    assert_eq!(
        crate::layout::dimension::lp_length_px(root.children[0].style.padding.top),
        Some(20.0)
    );
    clear_committed_inspector_edits_for_test();
}

#[test]
fn padding_and_bg_patch() {
    let mut el = Element::new();
    apply_style_patch(&mut el, &StylePatch::PaddingUniform(8.0));
    assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
    apply_style_patch(&mut el, &StylePatch::BackgroundToken("#ff0000".into()));
    assert!((el.style.bg_color[0] - 1.0).abs() < 0.02);
    apply_style_patch(&mut el, &StylePatch::WidthPx(120.0));
    assert_eq!(dim_length_px(el.style.size.width), Some(120.0));
    apply_style_patch(&mut el, &StylePatch::Opacity(0.5));
    assert!((el.style.opacity - 0.5).abs() < 0.001);
}

#[test]
fn selected_element_override_does_not_mutate_siblings_or_stylesheets() {
    clear_committed_inspector_edits_for_test();
    let mut left = Element::new();
    let mut right = Element::new();
    crate::style::apply_class_list(&mut left, "p-2 bg-red-500");
    crate::style::apply_class_list(&mut right, "p-2 bg-red-500");
    let original = right.style.clone();
    let mut root = Element::new().children([left, right]);
    commit_inspector_cssom(&root, &[0], "padding: 23px; background: #00ff00;".into());
    let mut tree = crate::core::tree::LayoutTree::new();
    reapply_inspector_overrides(&mut root, &mut tree);
    assert_eq!(lp_length_px(root.children[0].style.padding.top), Some(23.0));
    assert_eq!(root.children[1].style, original);
    let mut newly_mounted = Element::new();
    crate::style::apply_class_list(&mut newly_mounted, "p-2 bg-red-500");
    assert_eq!(newly_mounted.style, original);
    clear_committed_inspector_edits_for_test();
}

#[test]
fn patch_at_path() {
    let mut root = Element::new().child(Element::new().focus_id("a"));
    assert!(patch_element_at_path(&mut root, &vec![0], StylePatch::FlexGrow(2.0)).is_some());
    assert!((root.children[0].style.flex_grow - 2.0).abs() < 0.001);
}

#[test]
fn cssom_updates_class_set_base_style() {
    let mut el = Element::new();
    let _ = crate::style::apply_class_list(&mut el, "p-2 bg-red-500");
    let before = el
        .class_set
        .as_ref()
        .and_then(|cs| cs.base_style.as_ref())
        .map(|s| s.bg_color)
        .expect("base_style");
    apply_cssom_text(&mut el, "background: #00ff00; padding: 20px;");
    let after = el
        .class_set
        .as_ref()
        .and_then(|cs| cs.base_style.as_ref())
        .expect("base_style after");
    assert!(
        (after.bg_color[1] - 1.0).abs() < 0.02,
        "base bg should be green"
    );
    assert_ne!(before, after.bg_color);
    assert_eq!(lp_length_px(after.padding.top), Some(20.0));
    // Authored dump must reflect the applied base (not a stale snapshot).
    let dump = crate::app::devtools::cssom_text_from_element(&el);
    assert!(dump.contains("background: #00ff00"), "dump={dump}");
}

#[test]
fn cssom_text_applies_whitelist_and_layout() {
    let mut el = Element::new();
    let status = apply_cssom_text(
        &mut el,
        "padding: 8px; width: 120px; opacity: 0.5; flex-grow: 2; overflow: hidden; z-index: 4;",
    );
    assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
    assert_eq!(dim_length_px(el.style.size.width), Some(120.0));
    assert!((el.style.opacity - 0.5).abs() < 0.001);
    assert!((el.style.flex_grow - 2.0).abs() < 0.001);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(el.style.z_index, 4);
    assert_ne!(status, crate::core::tree::LayoutStatus::Clean);
}

#[test]
fn cssom_unitless_is_px_not_rem() {
    let mut el = Element::new();
    apply_cssom_text(&mut el, "padding: 8; width: 40;");
    assert_eq!(lp_length_px(el.style.padding.top), Some(8.0));
    assert_eq!(dim_length_px(el.style.size.width), Some(40.0));
}

#[test]
fn cssom_alignment_dump_uses_editable_css_keywords() {
    let mut el = Element::new();
    el.style.justify_content = Some(taffy::style::JustifyContent::CENTER);
    el.style.align_items = Some(taffy::style::AlignItems::CENTER);
    el.style.align_self = Some(taffy::style::AlignSelf::CENTER);
    el.style.align_content = Some(taffy::style::AlignContent::CENTER);
    el.style.justify_self = Some(taffy::style::JustifySelf::CENTER);
    el.style.justify_items = Some(taffy::style::JustifyItems::CENTER);
    let dump = crate::app::devtools::cssom_text_from_element(&el);
    let declarations = parse_declarations(&dump);
    for property in [
        "justify-content",
        "align-items",
        "align-self",
        "align-content",
        "justify-self",
        "justify-items",
    ] {
        let value = declarations
            .iter()
            .find(|(name, _)| name == property)
            .unwrap();
        assert_eq!(
            value.1, "center",
            "{property} must be an editable CSS value"
        );
    }
}

#[test]
fn cssom_alignment_edits_change_layout_style() {
    let mut el = Element::new();
    el.style.justify_content = Some(taffy::style::JustifyContent::CENTER);
    el.style.align_items = Some(taffy::style::AlignItems::CENTER);
    let impact = apply_cssom_text(
        &mut el,
        "justify-content: flex-start; align-items: flex-start;",
    );
    assert_eq!(
        el.style.justify_content,
        Some(taffy::style::JustifyContent::FLEX_START)
    );
    assert_eq!(
        el.style.align_items,
        Some(taffy::style::AlignItems::FLEX_START)
    );
    assert_eq!(impact, LayoutStatus::LayoutChanged);
}

#[test]
fn cssom_alignment_roundtrips_keywords_and_safety() {
    for value in [
        "normal",
        "start",
        "end",
        "flex-start",
        "flex-end",
        "center",
        "stretch",
        "safe start",
        "safe end",
        "safe flex-start",
        "safe flex-end",
        "safe center",
        "unsafe start",
        "unsafe end",
        "unsafe flex-start",
        "unsafe flex-end",
        "unsafe center",
    ] {
        let mut el = Element::new();
        let css = [
            "justify-content",
            "align-items",
            "align-self",
            "align-content",
            "justify-self",
            "justify-items",
        ]
        .map(|property| format!("{property}: {value};"))
        .join("\n");
        assert!(cssom_alignment_error(&css).is_none(), "{css}");
        apply_cssom_text(&mut el, &css);
        let dump = crate::app::devtools::cssom_text_from_element(&el);
        let mut restored = Element::new();
        apply_cssom_text(&mut restored, &dump);
        assert_eq!(
            restored.style.justify_content, el.style.justify_content,
            "{value}"
        );
        assert_eq!(
            restored.style.align_content, el.style.align_content,
            "{value}"
        );
        assert_eq!(restored.style.align_items, el.style.align_items, "{value}");
        assert_eq!(restored.style.align_self, el.style.align_self, "{value}");
        assert_eq!(
            restored.style.justify_items, el.style.justify_items,
            "{value}"
        );
        assert_eq!(
            restored.style.justify_self, el.style.justify_self,
            "{value}"
        );
    }
    for value in ["space-between", "space-around", "space-evenly"] {
        let mut el = Element::new();
        apply_cssom_text(
            &mut el,
            &format!("justify-content: {value}; align-content: {value};"),
        );
        let dump = crate::app::devtools::cssom_text_from_element(&el);
        assert!(
            dump.contains(&format!("justify-content: {value};")),
            "{dump}"
        );
        assert!(dump.contains(&format!("align-content: {value};")), "{dump}");
    }
    for value in [
        "self-start",
        "self-end",
        "safe self-start",
        "unsafe self-end",
        "baseline",
    ] {
        let mut el = Element::new();
        apply_cssom_text(
            &mut el,
            &format!("align-items: {value}; justify-self: {value};"),
        );
        let dump = crate::app::devtools::cssom_text_from_element(&el);
        assert!(dump.contains(&format!("align-items: {value};")), "{dump}");
        assert!(dump.contains(&format!("justify-self: {value};")), "{dump}");
    }
}

#[test]
fn cssom_alignment_auto_restores_parent_alignment() {
    let mut el = Element::new();
    apply_cssom_text(&mut el, "align-self: center; justify-self: center;");
    assert!(el.style.align_self.is_some());
    assert!(el.style.justify_self.is_some());
    assert_eq!(
        apply_cssom_text(&mut el, "align-self: AUTO; justify-self: auto;"),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(el.style.align_self, None);
    assert_eq!(el.style.justify_self, None);
    let dump = crate::app::devtools::cssom_text_from_element(&el);
    assert!(dump.contains("align-self: auto;"));
    assert!(dump.contains("justify-self: auto;"));
}

#[test]
fn cssom_alignment_rejects_unsupported_values_without_mutation() {
    for css in [
        "justify-content: left;",
        "align-items: left;",
        "align-items: auto;",
        "justify-content: safe stretch;",
        "align-content: safe space-between;",
        "align-self: center trailing;",
        "justify-items: unknown;",
        "justify-self: alignitems { keyword: left, safety: default };",
    ] {
        let mut el = Element::new();
        let original = el.style.clone();
        assert!(cssom_alignment_error(css).is_some(), "{css}");
        assert_eq!(apply_cssom_text(&mut el, css), LayoutStatus::Clean, "{css}");
        assert_eq!(el.style, original, "{css}");
    }
}

#[test]
fn cssom_alignment_moves_children_in_real_layout() {
    use taffy::prelude::*;
    let mut parent = Element::new().w(200.0).h(100.0);
    parent.style.flex_direction = FlexDirection::Row;
    parent.style.justify_content = Some(JustifyContent::CENTER);
    parent.style.align_items = Some(AlignItems::CENTER);
    let child = Element::new().w(40.0).h(20.0);
    let mut layout: TaffyTree<()> = TaffyTree::new();
    let child_node = layout.new_leaf(child.style.to_taffy_style()).unwrap();
    let parent_node = layout
        .new_with_children(parent.style.to_taffy_style(), &[child_node])
        .unwrap();
    let available = Size {
        width: AvailableSpace::Definite(200.0),
        height: AvailableSpace::Definite(100.0),
    };
    layout.compute_layout(parent_node, available).unwrap();
    let centered = layout.layout(child_node).unwrap().location;
    assert_eq!((centered.x, centered.y), (80.0, 40.0));
    apply_cssom_text(
        &mut parent,
        "justify-content: flex-start; align-items: flex-start;",
    );
    layout
        .set_style(parent_node, parent.style.to_taffy_style())
        .unwrap();
    layout.compute_layout(parent_node, available).unwrap();
    let aligned = layout.layout(child_node).unwrap().location;
    assert_eq!((aligned.x, aligned.y), (0.0, 0.0));
}

#[test]
fn cssom_roundtrip_dump() {
    let mut el = Element::new();
    apply_cssom_text(&mut el, "padding: 8px; background: #ff0000; width: 100px;");
    let dump = crate::app::devtools::cssom_text_from_element(&el);
    let mut el2 = Element::new();
    apply_cssom_text(&mut el2, &dump);
    assert_eq!(lp_length_px(el2.style.padding.top), Some(8.0));
    assert_eq!(dim_length_px(el2.style.size.width), Some(100.0));
    assert!((el2.style.bg_color[0] - 1.0).abs() < 0.02);
}
