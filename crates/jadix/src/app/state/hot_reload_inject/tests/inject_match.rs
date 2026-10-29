use super::*;
use crate::core::tree::Element;

fn tpl(id: &str, hash: &str, literals: &[&str]) -> crate::app::hot_reload::TemplateFragment {
    crate::app::hot_reload::TemplateFragment {
        id: id.to_string(),
        hash: hash.to_string(),
        len: 1,
        signature: String::new(),
        declarations_signature: String::new(),
        tree: None,
        index: 0,
        text_literals: literals.iter().map(|s| (*s).to_string()).collect(),
        text_replacements: vec![],
        style_attrs: vec![],
    }
}

#[test]
fn hot_reload_overlapping_templates_fail_before_any_tree_mutation() {
    let mut root = Element::new()
        .template_id("outer")
        .child(Element::new().template_id("inner").text("old"));
    let updates = [
        tpl("outer", "next-outer", &["outer"]),
        tpl("inner", "next-inner", &["inner"]),
    ];
    assert!(prepare_templates(&root, &updates).is_err());
    assert!(apply_templates_to_tree(&mut root, &updates).is_err());
    assert_eq!(root.children[0].text.as_deref(), Some("old"));
}

#[cfg(feature = "macros")]
#[test]
fn static_ui_structure_reuses_the_original_compiled_slot() {
    let mut root: Element =
        crate::ui! { <Box><Text>"before"</Text>{Element::new().text("capture")}</Box> };
    assert!(root.attrs.contains_key("jadix_hr_tree"));
    let description: jadix_hot_reload::TemplateDescription =
        serde_json::from_str(root.attrs.get("jadix_ui_description").unwrap()).unwrap();
    let jadix_hot_reload::TemplateNode::Element { children, .. } =
        description.tree.clone().unwrap()
    else {
        unreachable!()
    };
    let slot_id = root.children[1].inspector_instance_id;
    let mut update = tpl(root.template_id.as_ref().unwrap(), "structural-update", &[]);
    update.signature = description.signature;
    update.tree = Some(jadix_hot_reload::TemplateNode::Element {
        name: "Box".into(),
        key: None,
        attributes: vec![],
        children: vec![jadix_hot_reload::TemplateNode::Element {
            name: "HStack".into(),
            key: Some("wrapper".into()),
            attributes: vec![("p".into(), "8px".into())],
            children: vec![children[1].clone()],
        }],
    });
    preflight(&root, &update).unwrap();
    apply_templates_to_tree(&mut root, &[update]).unwrap();
    assert_eq!(root.children[0].children[0].inspector_instance_id, slot_id);
    assert_eq!(
        root.children[0].children[0].text.as_deref(),
        Some("capture")
    );
    assert_eq!(
        crate::layout::dimension::lp_length_px(root.children[0].style.padding.top),
        Some(8.0)
    );
}

#[test]
fn all_instances_validate_before_any_patch_mutates_and_keep_window_identity() {
    let make = |signature: &str| {
        Element::new()
            .text("old")
            .template_id("same-source")
            .hot_reload_description(jadix_hot_reload::TemplateDescription::new(
                signature,
                "declarations",
            ))
    };
    let mut primary = Element::new().children(vec![make("compiled"), make("compiled")]);
    let mut secondary = Element::new().child(make("compiled"));
    let primary_ids: Vec<_> = primary
        .children
        .iter()
        .map(|element| element.inspector_instance_id)
        .collect();
    let secondary_id = secondary.children[0].inspector_instance_id;
    let mut update = tpl("same-source", "new-hash", &["new"]);
    update.signature = "compiled".into();
    let primary_plan = prepare_templates(&primary, &[update.clone()]).unwrap();
    let secondary_plan = prepare_templates(&secondary, &[update.clone()]).unwrap();
    apply_prepared_templates(&mut primary, primary_plan).unwrap();
    apply_prepared_templates(&mut secondary, secondary_plan).unwrap();
    assert!(primary
        .children
        .iter()
        .all(|element| element.text.as_deref() == Some("new")));
    assert_eq!(
        primary_ids,
        primary
            .children
            .iter()
            .map(|element| element.inspector_instance_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(secondary.children[0].inspector_instance_id, secondary_id);
    let mut invalid = Element::new().children(vec![make("compiled"), make("different capture")]);
    assert!(apply_templates_to_tree(&mut invalid, &[update]).is_err());
    assert!(invalid
        .children
        .iter()
        .all(|element| element.text.as_deref() == Some("old")));
}

#[test]
fn declaration_removal_restores_constructor_values_and_clears_class_animation() {
    let mut root = Element::new()
        .hot_reload_declaration_base(&["p", "class", "animation"], false)
        .template_id("fixture")
        .template_hash("compiled");
    let original_style = root.style.clone();
    let original_text_style = root.text_style.clone();
    crate::style::apply_class_list(&mut root, "p-8 bg-red-500 hover:bg-blue-500");
    root = root.animation("spin 1s linear infinite");
    let mut update = tpl("fixture", "patched", &[]);
    update.style_attrs.push(crate::app::hot_reload::StyleAttr {
        element_index: 0,
        name: "p".into(),
        value: "12px".into(),
    });
    assert!(preflight(&root, &update).is_ok());
    apply_templates_to_tree(&mut root, &[update]).unwrap();
    assert_eq!(
        crate::layout::dimension::lp_length_px(root.style.padding.top),
        Some(12.0)
    );
    assert_eq!(root.style.bg_color, original_style.bg_color);
    assert!(root.style.animations.is_empty());
    assert!(root.class_set.is_none());
    apply_templates_to_tree(&mut root, &[tpl("fixture", "removed", &[])]).unwrap();
    assert_eq!(root.style.padding, original_style.padding);
    assert_eq!(root.text_style, original_text_style);
}

#[test]
fn explicit_builder_description_controls_commit_without_source_or_macros() {
    let description =
        crate::core::tree::TemplateDescription::new("plain-template", "plain-declarations");
    let mut root = Element::new()
        .text("old")
        .template_id("plain")
        .hot_reload_description(description.clone());
    let mut update = tpl("plain", "updated", &["new"]);
    update.signature = description.signature;
    update.declarations_signature = description.declarations_signature;
    assert!(preflight(&root, &update).is_ok());
    apply_templates_to_tree(&mut root, &[update.clone()]).unwrap();
    assert_eq!(root.text.as_deref(), Some("new"));
    update.signature = "new Rust slot".into();
    assert!(preflight(&root, &update).is_err());
    assert!(apply_templates_to_tree(&mut root, &[update]).is_err());
    assert_eq!(root.text.as_deref(), Some("new"));
}

#[cfg(feature = "macros")]
#[test]
fn ui_and_explicit_builder_share_description_and_rejection_rules() {
    let mut compiled: Element = crate::ui! { <Box p="8">"old"</Box> };
    let description: crate::core::tree::TemplateDescription =
        serde_json::from_str(compiled.get_attr("jadix_ui_description").unwrap()).unwrap();
    let mut plain = Element::new()
        .hot_reload_declaration_base(&["p"], false)
        .text("old")
        .template_id("plain")
        .hot_reload_description(description.clone());
    let mut update = tpl("plain", "next", &["new"]);
    update.signature = description.signature.clone();
    update.declarations_signature = description.declarations_signature.clone();
    update.style_attrs = vec![crate::app::hot_reload::StyleAttr {
        element_index: 0,
        name: "p".into(),
        value: "12px".into(),
    }];
    assert!(preflight(&compiled, &update).is_ok());
    assert!(preflight(&plain, &update).is_ok());
    inject_style_attrs(&mut compiled, &update.style_attrs);
    inject_style_attrs(&mut plain, &update.style_attrs);
    assert_eq!(compiled.style.padding, plain.style.padding);
    update.signature.push_str("changed slot");
    assert!(preflight(&compiled, &update).is_err());
    assert!(preflight(&plain, &update).is_err());
}

#[test]
fn invalid_numbers_and_opaque_class_are_rejected_before_mutation() {
    let root = Element::new()
        .hot_reload_declaration_base(&[], true)
        .template_id("fixture");
    let mut update = tpl("fixture", "changed", &[]);
    for (name, value) in [
        ("p", "NaN"),
        ("w", "inf%"),
        ("class", "p-4"),
        ("bg", "not-a-color"),
        ("animation_duration", "-1s"),
        ("animation_play_state", "maybe"),
    ] {
        update.style_attrs = vec![crate::app::hot_reload::StyleAttr {
            element_index: 0,
            name: name.into(),
            value: value.into(),
        }];
        assert!(preflight(&root, &update).is_err(), "{name}={value}");
    }
    assert_eq!(root.style.padding, crate::style::Style::default().padding);
}

#[test]
fn apply_hits_matching_line_not_first_same_column() {
    let mut root = Element::new().children(vec![
        Element::new()
            .template_id("examples/gallery/src/main.rs:188:9")
            .template_hash("old-a")
            .text("first"),
        Element::new()
            .template_id("examples/gallery/src/main.rs:400:9")
            .template_hash("old-b")
            .text("second"),
    ]);
    apply_templates_to_tree(
        &mut root,
        &[tpl(
            "d:/projects/jadix/examples/gallery/src/main.rs:400:9",
            "new-b",
            &["second-new"],
        )],
    )
    .unwrap();
    assert_eq!(root.children[0].text.as_deref(), Some("first"));
    assert_eq!(root.children[1].text.as_deref(), Some("second-new"));
}

#[test]
fn text_replacement_crosses_component_identity_but_not_nested_ui() {
    let component = Element::new()
        .template_id("examples/gallery/src/main.rs:188:Button#1")
        .children(vec![Element::new().text("Secondary")]);
    let nested = Element::new()
        .template_id("examples/gallery/src/shared.rs:275:5")
        .template_hash("nested")
        .text("Secondary");
    let mut root = Element::new()
        .template_id("examples/gallery/src/main.rs:188:9")
        .children(vec![component, nested]);
    let mut patch = tpl("examples/gallery/src/main.rs:188:9", "changed", &[]);
    patch.text_replacements = vec![crate::app::hot_reload::TextReplacement {
        index: 0,
        from: "Secondary".into(),
        to: "Updated".into(),
    }];
    apply_templates_to_tree(&mut root, &[patch]).unwrap();
    assert_eq!(
        root.children[0].children[0].text.as_deref(),
        Some("Updated")
    );
    assert_eq!(root.children[1].text.as_deref(), Some("Secondary"));
}

#[test]
fn apply_rejects_line_drift_instead_of_selecting_the_nearest_block() {
    let mut root = Element::new().children(vec![
        Element::new()
            .template_id("examples/gallery/src/main.rs:188:9")
            .template_hash("old-a")
            .text("first"),
        Element::new()
            .template_id("examples/gallery/src/main.rs:400:9")
            .template_hash("old-b")
            .text("second"),
    ]);
    apply_templates_to_tree(
        &mut root,
        &[tpl(
            "d:/projects/jadix/examples/gallery/src/main.rs:402:9",
            "new-b",
            &["second-new"],
        )],
    )
    .unwrap();
    assert_eq!(root.children[0].text.as_deref(), Some("first"));
    assert_eq!(root.children[1].text.as_deref(), Some("second"));
}

#[test]
fn apply_updates_label_inside_switch_reactive_island() {
    // Gallery overwrites the page `ui!` template_id with `App:section_body`.
    // Hot reload must still find the island via the preserved source id.
    let mut island = Element::new()
        .template_id("examples/gallery/App:section_body")
        .template_hash("old-nav")
        .children(vec![
            Element::new().children(vec![Element::new().text("导航")])
        ]);
    island.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    let mut root = Element::new().children(vec![
        Element::new()
            .template_id("examples/gallery/src/main.rs:100:9")
            .text("chrome"),
        island,
    ]);
    apply_templates_to_tree(
        &mut root,
        &[tpl(
            "d:/projects/jadix/examples/gallery/src/main.rs:4334:21",
            "new-nav",
            &["导航改"],
        )],
    )
    .unwrap();
    assert_eq!(root.children[0].text.as_deref(), Some("chrome"));
    assert_eq!(
        root.children[1].children[0].children[0].text.as_deref(),
        Some("导航改")
    );
    assert_eq!(
        root.children[1].template_id.as_deref(),
        Some("examples/gallery/App:section_body"),
        "island identity must stay stable after inject"
    );
}

#[test]
fn apply_twice_keeps_island_children_and_updates_text() {
    let mut island = Element::new()
        .template_id("examples/gallery/App:section_body")
        .template_hash("old-nav")
        .children(vec![
            Element::new().children(vec![Element::new().text("导航")])
        ]);
    island.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    let nested = Element::new()
        .template_id("examples/gallery/src/main.rs:4179:5")
        .template_hash("nested-showcase")
        .text("navbar");
    island.children[0].children.push(nested);
    let mut root = Element::new().children(vec![island]);

    apply_templates_to_tree(
        &mut root,
        &[tpl(
            "examples/gallery/src/main.rs:4334:21",
            "nav-1",
            &["导航改"],
        )],
    )
    .unwrap();
    apply_templates_to_tree(
        &mut root,
        &[tpl(
            "examples/gallery/src/main.rs:4334:21",
            "nav-2",
            &["导航再改"],
        )],
    )
    .unwrap();
    assert_eq!(
        root.children[0].children[0].children[0].text.as_deref(),
        Some("导航再改")
    );
    assert_eq!(
        root.children[0].children[0].children.len(),
        2,
        "second inject must not drop nested showcase children"
    );
    assert_eq!(
        root.children[0].children[0].children[1].text.as_deref(),
        Some("navbar")
    );
}

#[test]
fn apply_style_skips_scroll_viewport_and_scrollbar_chrome() {
    // CLI RSX DFS: 0 ScrollArea, 1 VStack, 2 Label (text_size).
    // Runtime `From<ScrollAreaBuilder>` wraps the body in a `scroll:` viewport
    // and appends scrollbar siblings — those must not consume element_index.
    let label = Element::new()
        .text("导航")
        .text_size(1.25 * crate::core::tree::CSS_ROOT_FONT_SIZE_PX);
    let vstack = Element::new().children(vec![label]);
    let viewport = Element::new()
        .focus_id("scroll:scroll-navigation")
        .children(vec![vstack]);
    let vbar = Element::new()
        .template_id("__jadix_v_scrollbar_scroll-navigation")
        .focus_id("scrollbar-track:scroll-navigation:y");
    let hbar = Element::new()
        .template_id("__jadix_h_scrollbar_scroll-navigation")
        .focus_id("scrollbar-track:scroll-navigation:x");
    let mut island = Element::new()
        .template_id("examples/gallery/App:section_body")
        .template_hash("old-nav")
        .children(vec![viewport, vbar, hbar]);
    island.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    let mut root = Element::new().children(vec![island]);

    let mut patch = tpl(
        "examples/gallery/src/main.rs:4334:21",
        "new-nav-size",
        &["导航"],
    );
    patch.style_attrs = vec![crate::app::hot_reload::StyleAttr {
        element_index: 2,
        name: "text_size".into(),
        value: "2rem".into(),
    }];
    apply_templates_to_tree(&mut root, &[patch]).unwrap();

    let vstack = &root.children[0].children[0].children[0];
    let label = &vstack.children[0];
    let expected = 2.0 * crate::core::tree::CSS_ROOT_FONT_SIZE_PX;
    assert_eq!(
        label.text_style.size_resolved(),
        expected,
        "text_size must land on Label (CLI index 2), not the viewport/VStack wrapper"
    );
    assert_eq!(
        vstack.text_style.size, None,
        "VStack must not receive the Label style patch"
    );
    assert_eq!(
        label.text_style.size,
        Some(expected),
        "explicit size must survive cascade and not be overwritten"
    );
}

#[test]
fn live_class_and_animation_updates_use_framework_parsers() {
    let mut root = Element::new().template_id("live.rs:1:1").text("content");
    let attrs = vec![
        crate::app::hot_reload::StyleAttr {
            element_index: 0,
            name: "class".into(),
            value: "p-4 bg-red-500".into(),
        },
        crate::app::hot_reload::StyleAttr {
            element_index: 0,
            name: "animation".into(),
            value: "spin 2s linear infinite".into(),
        },
        crate::app::hot_reload::StyleAttr {
            element_index: 0,
            name: "transition".into(),
            value: "opacity 300ms ease-in".into(),
        },
    ];
    assert!(inject_style_attrs_by_id(&mut root, "live.rs:1:1", &attrs));
    assert_eq!(root.style.animations.len(), 1);
    assert_eq!(root.style.animations[0].name, "spin");
    assert!(!root.style.transition.is_empty());
    assert_eq!(root.text.as_deref(), Some("content"));
    assert!(root.class_set.is_some());
}

#[test]
fn deferred_override_applies_when_island_later_mounts() {
    // CLI only sends the changed `ui!`. If that page is not mounted, the
    // live tree has no match — the patch must still apply when the
    // `switch_reactive` island is created later from compile-time source.
    let patch = tpl(
        "d:/projects/jadix/examples/gallery/src/main.rs:4334:21",
        "hmr-nav-new",
        &["导航改"],
    );

    let mut live = Element::new().children(vec![Element::new()
        .template_id("examples/gallery/src/main.rs:100:9")
        .template_hash("chrome")
        .text("chrome")]);
    apply_templates_to_tree(&mut live, std::slice::from_ref(&patch)).unwrap();
    assert_eq!(
        live.children[0].text.as_deref(),
        Some("chrome"),
        "unrelated mounted template must stay untouched"
    );

    let mut island = Element::new()
        .template_id("examples/gallery/App:section_body")
        .template_hash("compile-time-old")
        .children(vec![Element::new()
            .focus_id("scroll:scroll-navigation")
            .children(vec![
                Element::new().children(vec![Element::new().text("导航")])
            ])]);
    island.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    apply_templates_to_tree(&mut island, std::slice::from_ref(&patch)).unwrap();
    assert_eq!(
        island.children[0].children[0].children[0].text.as_deref(),
        Some("导航改"),
        "entering the page must replay the stored hot-reload literals"
    );
}
