use jadix::portable::{
    authoring::native::{ElementProjection, ProjectionError},
    create_root, create_signal,
    presentation::{AccessibleRole, EventPayload, NodeContent, PlatformEvent, PresentationNode, RootId},
};
use jadix_components::portable::emoji_picker;

fn nodes(root: &PresentationNode) -> Vec<&PresentationNode> {
    let mut pending = vec![root];
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        result.push(node);
        pending.extend(&node.children);
    }
    result
}

#[test]
fn shared_picker_preserves_search_identity_and_releases_replaced_pages() {
    let (owner, (mut element, selected)) = create_root(|| {
        let selected = create_signal(String::new());
        let element = emoji_picker("picker").value_signal(selected).build();
        (element, selected)
    });
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let input = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    assert!(nodes(&initial).len() < 200);
    assert!(nodes(&initial).iter().all(|node| !matches!(node.descriptor.content, NodeContent::Hosted { .. })));
    let list = nodes(&initial).into_iter().find(|node| node.descriptor.accessible_role == Some(AccessibleRole::ListBox)).unwrap();
    assert_eq!(list.descriptor.accessible_name.as_deref(), Some("Emoji"));
    let search = nodes(&initial).into_iter().find(|node| node.id == input).unwrap();
    assert_eq!(search.descriptor.accessibility_relations.controls, vec![list.id]);
    assert_eq!(search.descriptor.accessibility_relations.active_descendant, None);
    assert!(nodes(list).iter().any(|node| node.descriptor.accessible_role == Some(AccessibleRole::Heading)));
    let options: Vec<_> = nodes(list).into_iter().filter(|node| matches!(node.descriptor.content, NodeContent::Button { .. })).collect();
    assert!(!options.is_empty());
    assert!(options.iter().all(|node| node.descriptor.accessible_role == Some(AccessibleRole::ListBoxOption { tab_stop: false }) && node.descriptor.accessible_name.is_some()));
    let mut page_owners = [None, None];
    for revision in 1..=12 {
        projection.dispatch(&mut element, &PlatformEvent { node: input, payload: EventPayload::Edit {
            value: if revision % 2 == 0 { "thumbs up" } else { "cat" }.into(), revision,
            selection_utf16: [0, 0], composing_utf16: None,
        }}).unwrap();
        let page = projection.snapshot(&mut element).unwrap();
        assert!(nodes(&page).iter().any(|node| node.id == input));
        let owners = jadix::portable::live_owner_count();
        let budget = &mut page_owners[revision as usize % 2];
        if let Some(previous) = budget { assert_eq!(owners, *previous, "replacing a query page must release its scopes"); }
        else { *budget = Some(owners); }
    }
    let page = projection.snapshot(&mut element).unwrap();
    let thumb = nodes(&page).into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == "👍")).unwrap().id;
    projection.dispatch(&mut element, &PlatformEvent { node: thumb, payload: EventPayload::Activate }).unwrap();
    assert_eq!(selected.get(), "👍");
    owner.dispose();
}

#[test]
fn disabled_picker_rejects_activation() {
    let (_owner, mut element) = create_root(|| emoji_picker("disabled").disabled(true).build());
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    for node in nodes(&page) {
        if matches!(node.descriptor.content, NodeContent::Button { .. }) {
            for payload in [EventPayload::Activate, EventPayload::Hover { inside: true }, EventPayload::PointerMove { x: 1., y: 1. }] {
                assert_eq!(projection.dispatch(&mut element, &PlatformEvent { node: node.id, payload }), Err(ProjectionError::InvalidEvent));
            }
        }
    }
}

#[test]
fn controller_navigation_focus_exact_lookup_and_disposal_share_the_portable_tree() {
    use jadix::portable::presentation::NavigationKey;
    use jadix_components::emoji_picker::{EmojiAnnotation, EmojiPickerController, EmojiQuery};
    let controller = EmojiPickerController::default();
    let (owner, (mut element, selected, cancelled)) = create_root(|| {
        let selected = create_signal(String::new());
        let cancelled = create_signal(0usize);
        let element = emoji_picker("navigation").controller(controller.clone()).auto_focus(true)
            .query(EmojiQuery { annotations: vec![EmojiAnnotation { emoji: "👍".into(), label: "赞".into(), keywords: vec!["支持".into()] }], ..Default::default() })
            .value_signal(selected).on_cancel(move || cancelled.set(cancelled.get() + 1)).build();
        (element, selected, cancelled)
    });
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    let input = nodes(&page).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    assert_eq!(projection.take_focus_request(&element), Some(input));
    assert_eq!(projection.take_focus_request(&element), None);
    assert!(controller.matched_count() > 1800);
    assert_eq!(controller.entry("👍🏿").unwrap().name, "赞");
    assert!(controller.highlight("👍🏿"));
    controller.navigate(NavigationKey::Enter);
    assert_eq!(selected.get(), "👍");
    controller.focus(); assert_eq!(projection.take_focus_request(&element), Some(input));
    projection.dispatch(&mut element, &PlatformEvent { node: input, payload: EventPayload::Edit {
        value: "支持".into(), revision: 1, selection_utf16: [2, 2], composing_utf16: None,
    }}).unwrap();
    let filtered = projection.snapshot(&mut element).unwrap();
    assert!(controller.matched_count() < 10);
    assert!(nodes(&filtered).iter().any(|node| node.descriptor.accessible_selected == Some(true)));
    assert_eq!(nodes(&filtered).into_iter().find(|node| node.id == input).unwrap().descriptor.accessible_description.as_deref(), Some("赞"));
    let selected_option = nodes(&filtered).into_iter().find(|node| node.descriptor.accessible_selected == Some(true)).unwrap();
    assert_eq!(nodes(&filtered).into_iter().find(|node| node.id == input).unwrap().descriptor.accessibility_relations.active_descendant, Some(selected_option.id));
    selected.set(String::new());
    controller.navigate(NavigationKey::Enter);
    assert_eq!(selected.get(), "👍");
    // Exact lookup is independent of the active filter.
    assert!(controller.entry("😀").is_some());
    controller.navigate(NavigationKey::Escape);
    projection.snapshot(&mut element).unwrap();
    assert_eq!(cancelled.get(), 0); assert!(controller.matched_count() > 1800);
    controller.navigate(NavigationKey::Escape); assert_eq!(cancelled.get(), 1);
    owner.dispose();
    assert_eq!(controller.matched_count(), 0);
    assert!(!controller.highlight("👍"));
    assert!(controller.entry("👍").is_none());
}

#[test]
fn hover_highlights_without_scrolling_and_part_hooks_preserve_controls() {
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiPickerParts, EmojiQuery};
    use std::{cell::RefCell, rc::Rc};
    let controller = EmojiPickerController::default();
    let customized = Rc::new(RefCell::new(Vec::new()));
    let hook = |name| {
        let customized = customized.clone();
        Some(Rc::new(move |element: jadix::portable::Element| {
            customized.borrow_mut().push(name);
            element
        }) as Rc<dyn Fn(jadix::portable::Element) -> jadix::portable::Element>)
    };
    let (_owner, (mut element, highlighted, selected)) = create_root(|| {
        let highlighted = create_signal(String::new());
        let selected = create_signal(String::new());
        let element = emoji_picker("hover").controller(controller.clone())
            .query(EmojiQuery { query: "thumbs up".into(), ..Default::default() })
            .parts(EmojiPickerParts { search_bar: hook("search_bar"), search: hook("search"), list: hook("list"),
                row: hook("row"), item: hook("item"), group: hook("group") })
            .on_highlight_change(move |entry| highlighted.set(entry.map_or(String::new(), |entry| entry.text)))
            .value_signal(selected).build();
        (element, highlighted, selected)
    });
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    let target = nodes(&page).into_iter().find(|node|
        matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == "👍")
    ).unwrap().id;
    projection.dispatch(&mut element, &PlatformEvent { node: target, payload: EventPayload::PointerMove { x: 10., y: 10. } }).unwrap();
    assert_eq!(highlighted.get(), "👍");
    let page = projection.snapshot(&mut element).unwrap();
    assert_eq!(nodes(&page).into_iter().find(|node| node.id == target).unwrap().descriptor.accessible_selected, Some(true));
    assert!(nodes(&page).iter().all(|node| node.descriptor.style.scroll_to.is_none()));
    projection.dispatch(&mut element, &PlatformEvent { node: target, payload: EventPayload::Hover { inside: false } }).unwrap();
    controller.navigate(jadix::portable::presentation::NavigationKey::Enter);
    assert_eq!(selected.get(), "👍");
    for part in ["search_bar", "search", "list", "row", "item"] { assert!(customized.borrow().contains(&part)); }
    assert!(!customized.borrow().contains(&"group")); // Query results have no section headings.
}

#[test]
fn group_callback_tracks_actual_viewport_and_repeated_controller_requests() {
    use jadix_components::emoji_picker::EmojiPickerController;
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, group)) = create_root(|| {
        let group = create_signal(None);
        let element = emoji_picker("groups").controller(controller.clone()).on_group_change(move |value| group.set(value)).build();
        (element, group)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    let scroll = nodes(&first).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Scroll { .. })).unwrap().id;
    assert_eq!(group.get(), Some(0));
    assert!(controller.scroll_to_group(1));
    let page = projection.snapshot(&mut element).unwrap();
    let position = nodes(&page).into_iter().find(|node| node.id == scroll).unwrap().descriptor.style.scroll_to.unwrap()[1];
    assert_eq!(group.get(), Some(0)); // Smooth navigation retains the actual viewport until the host scrolls.
    assert_eq!(nodes(&page).into_iter().find(|node| node.id == scroll).unwrap().descriptor.style.scroll_behavior, Some(jadix::portable::presentation::style::ScrollBehavior::Smooth));
    projection.dispatch(&mut element, &PlatformEvent { node: scroll, payload: EventPayload::ScrollViewport {
        x: 0., y: position, width: 400., height: 260.,
    }}).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert_eq!(group.get(), Some(1));
    projection.dispatch(&mut element, &PlatformEvent { node: scroll, payload: EventPayload::ScrollViewport {
        x: 0., y: 0., width: 400., height: 260.,
    }}).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert_eq!(group.get(), Some(0));
    assert!(controller.scroll_to_group(1));
    let page = projection.snapshot(&mut element).unwrap();
    assert_eq!(nodes(&page).into_iter().find(|node| node.id == scroll).unwrap().descriptor.style.scroll_to.unwrap()[1], position);
}

#[test]
fn hidden_search_exposes_active_option_and_clears_unmounted_relations() {
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, query)) = create_root(|| {
        let query = create_signal("thumbs up".to_owned());
        (emoji_picker("hidden__picker").controller(controller.clone()).searchable(false)
            .query(EmojiQuery::default()).query_signal(query).build(), query)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    let focus = nodes(&first).into_iter().find(|node| !node.descriptor.navigation_keys.is_empty()).unwrap();
    let active = nodes(&first).into_iter().find(|node| node.descriptor.accessible_selected == Some(true)).unwrap();
    let list = nodes(&first).into_iter().find(|node| node.descriptor.accessible_role == Some(AccessibleRole::ListBox)).unwrap();
    assert_eq!(focus.descriptor.accessibility_relations.active_descendant, Some(active.id));
    assert_eq!(focus.descriptor.accessibility_relations.controls, vec![list.id]);
    assert_eq!(focus.descriptor.accessible_description, active.descriptor.accessible_name);
    assert!(nodes(&first).iter().all(|node| !matches!(node.descriptor.content, NodeContent::Input { .. })));
    assert!(focus.descriptor.navigation_keys.contains(&jadix::portable::presentation::NavigationKey::Home));
    assert!(nodes(&first).iter().all(|node| !matches!(node.descriptor.content, NodeContent::Select { .. })));
    let focus_id = focus.id;
    query.set("no-emoji-matches-this-query".into());
    let empty = projection.snapshot(&mut element).unwrap();
    let focus = nodes(&empty).into_iter().find(|node| node.id == focus_id).unwrap();
    assert_eq!(focus.descriptor.accessibility_relations.active_descendant, None);
    assert!(focus.descriptor.accessibility_relations.controls.is_empty());
    assert_eq!(focus.descriptor.accessible_description, None);
    assert_eq!(controller.matched_count(), 0);
}

#[test]
fn optional_headings_and_measured_viewport_use_shared_geometry() {
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    use jadix::portable::presentation::NavigationKey;
    let controller = EmojiPickerController::default();
    let (_owner, mut element) = create_root(|| emoji_picker("plain_groups").controller(controller.clone())
        .group_labels(vec![]).query(EmojiQuery { groups: Some(vec![0, 1]), ..Default::default() }).build());
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&first).iter().all(|node| node.descriptor.accessible_role != Some(AccessibleRole::Heading)));
    let input = nodes(&first).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap();
    assert!(!input.descriptor.navigation_keys.contains(&NavigationKey::Home));
    assert!(!input.descriptor.navigation_keys.contains(&NavigationKey::End));
    let scroll = nodes(&first).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Scroll { .. })).unwrap().id;
    projection.dispatch(&mut element, &PlatformEvent { node: scroll, payload: EventPayload::ScrollViewport {
        x: 0., y: 0., width: 400., height: 80.,
    }}).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert!(controller.highlight("😀"));
    controller.navigate(NavigationKey::PageDown);
    let page = projection.snapshot(&mut element).unwrap();
    let selected = nodes(&page).into_iter().find(|node| node.descriptor.accessible_selected == Some(true)).unwrap();
    let mut search = jadix_components::emoji_picker::EmojiSearch::default();
    search.search(&EmojiQuery { groups: Some(vec![0, 1]), ..Default::default() }).unwrap();
    assert_eq!(selected.descriptor.accessible_name.as_deref(), Some(search.matches().nth(16).unwrap().name.as_str()));
    assert!(controller.scroll_to_group(1));
}

#[test]
fn custom_item_content_keeps_common_button_semantics_and_search_identity() {
    use jadix::portable::Element;
    use jadix_components::emoji_picker::EmojiQuery;
    let (_owner, (mut element, selected)) = create_root(|| {
        let selected = create_signal(String::new());
        let element = emoji_picker("custom_content")
            .query(EmojiQuery { query: "thumbs up".into(), ..Default::default() })
            .render_item(|entry| {
                let mut image = Element::new().w(24.).h(24.);
                image.descriptor.content = NodeContent::Image {
                    source: format!("asset:/emoji/{}.png", entry.base), alt: entry.name.clone(),
                };
                Element::new().flex_row().gap(4.).child(image)
                    .child(Element::new().text(entry.name.clone()))
            }).value_signal(selected).build();
        (element, selected)
    });
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let input = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    let button = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Button { .. })).unwrap();
    assert!(matches!(&button.descriptor.content, NodeContent::Button { label, enabled: true } if label.is_empty()));
    assert_eq!(button.descriptor.accessible_role, Some(AccessibleRole::ListBoxOption { tab_stop: false }));
    assert_eq!(button.descriptor.accessible_name.as_deref(), Some("thumbs up"));
    let image = nodes(button).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Image { .. })).unwrap();
    assert!(matches!(&image.descriptor.content, NodeContent::Image { source, .. } if source == "asset:/emoji/👍.png"));
    let (button_id, image_id) = (button.id, image.id);
    projection.dispatch(&mut element, &PlatformEvent { node: button_id, payload: EventPayload::PointerMove { x: 10., y: 10. } }).unwrap();
    let highlighted = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&highlighted).iter().any(|node| node.id == input));
    assert!(nodes(&highlighted).iter().any(|node| node.id == image_id));
    assert_eq!(nodes(&highlighted).into_iter().find(|node| node.id == button_id).unwrap().descriptor.accessible_selected, Some(true));
    projection.dispatch(&mut element, &PlatformEvent { node: button_id, payload: EventPayload::Activate }).unwrap();
    assert_eq!(selected.get(), "👍");

    let mut caption: Element = jadix_components::portable::button("caption").text("Save").into();
    let caption = projection.snapshot(&mut caption).unwrap();
    assert!(matches!(&caption.descriptor.content, NodeContent::Button { label, .. } if label == "Save"));
    assert!(caption.children.is_empty());
}

#[test]
fn configured_geometry_drives_virtualization_reveal_and_page_navigation() {
    use jadix::portable::presentation::NavigationKey;
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    let controller = EmojiPickerController::default();
    let (_owner, mut element) = create_root(|| emoji_picker("large_options").controller(controller.clone())
        .columns(4).row_height(64.).item_size(56.).height(128.).group_labels(vec![])
        .query(EmojiQuery { groups: Some(vec![0]), ..Default::default() }).build());
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let scroll = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Scroll { .. })).unwrap().id;
    let option = nodes(&initial).into_iter().find(|node| node.descriptor.accessible_role == Some(AccessibleRole::ListBoxOption { tab_stop: false })).unwrap();
    assert_eq!(option.descriptor.style.width, Some(0.));
    assert_eq!(option.descriptor.style.grow, 1.);
    assert_eq!(option.descriptor.style.height, Some(56.));
    assert!(controller.highlight("😀"));
    controller.navigate(NavigationKey::PageDown);
    let pending = projection.snapshot(&mut element).unwrap();
    let scroll_node = nodes(&pending).into_iter().find(|node| node.id == scroll).unwrap();
    assert_eq!(scroll_node.descriptor.style.height, Some(128.));
    assert_eq!(scroll_node.descriptor.style.scroll_to.unwrap()[1], 64.);
    let selected = nodes(&pending).into_iter().find(|node| node.descriptor.accessible_selected == Some(true)).unwrap();
    let mut search = jadix_components::emoji_picker::EmojiSearch::default();
    search.search(&EmojiQuery { groups: Some(vec![0]), ..Default::default() }).unwrap();
    assert_eq!(selected.descriptor.accessible_name.as_deref(), Some(search.matches().nth(8).unwrap().name.as_str()));
    projection.dispatch(&mut element, &PlatformEvent { node: scroll, payload: EventPayload::ScrollViewport {
        x: 0., y: 64., width: 240., height: 192.,
    }}).unwrap();
    projection.snapshot(&mut element).unwrap();
    controller.navigate(NavigationKey::PageDown);
    let resized = projection.snapshot(&mut element).unwrap();
    let selected = nodes(&resized).into_iter().find(|node| node.descriptor.accessible_selected == Some(true)).unwrap();
    assert_eq!(selected.descriptor.accessible_name.as_deref(), Some(search.matches().nth(20).unwrap().name.as_str()));
}

#[test]
fn section_reveal_can_jump_then_animate_without_replacing_search() {
    use jadix::portable::presentation::style::ScrollBehavior;
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    let controller = EmojiPickerController::default();
    let (_owner, mut element) = create_root(|| emoji_picker("section_jump").controller(controller.clone())
        .query(EmojiQuery { groups: Some(vec![0, 9]), ..Default::default() }).build());
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let input_id = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    assert!(controller.scroll_to_group_with_animation(9, false));
    let jumped = projection.snapshot(&mut element).unwrap();
    let scroll = nodes(&jumped).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Scroll { .. })).unwrap();
    assert_eq!(scroll.descriptor.style.scroll_behavior, Some(ScrollBehavior::Instant));
    assert!(scroll.descriptor.style.scroll_to.unwrap()[1] > 260.);
    assert!(nodes(&jumped).iter().any(|node| node.descriptor.accessible_role == Some(AccessibleRole::Heading) && node.descriptor.accessible_name.as_deref() == Some("Flags")));
    assert!(nodes(&jumped).iter().any(|node| node.id == input_id));
    assert!(controller.scroll_to_group(0));
    let pending = projection.snapshot(&mut element).unwrap();
    let scroll = nodes(&pending).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Scroll { .. })).unwrap();
    assert_eq!(scroll.descriptor.style.scroll_behavior, Some(ScrollBehavior::Smooth));
    assert_eq!(scroll.descriptor.style.scroll_to, Some([0., 0.]));
    assert!(!controller.scroll_to_group_with_animation(11, false));
}

#[test]
fn reactive_catalog_options_relocalize_and_filter_without_replacing_search() {
    use jadix_components::emoji_picker::{EmojiAnnotation, EmojiPickerController};
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, recent, groups, annotations, version, query)) = create_root(|| {
        let recent = create_signal(vec!["😀".to_owned()]);
        let groups = create_signal(Some(vec![10]));
        let annotations = create_signal(Vec::<EmojiAnnotation>::new());
        let version = create_signal(None::<String>);
        let query = create_signal(String::new());
        let element = emoji_picker("dynamic_catalog").controller(controller.clone()).query_signal(query)
            .recent_fn(move || recent.get()).groups_fn(move || groups.get())
            .annotations_fn(move || annotations.get()).emoji_version_fn(move || version.get())
            .show_filters(true).build();
        (element, recent, groups, annotations, version, query)
    });
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let input_id = nodes(&initial).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    assert_eq!(controller.matched_count(), 1);
    assert!(controller.highlight("😀"));
    annotations.set(vec![EmojiAnnotation { emoji: "😀".into(), label: "Localized smile".into(), keywords: vec!["customkeyword".into()] }]);
    let localized = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&localized).iter().any(|node| node.id == input_id));
    assert!(nodes(&localized).iter().any(|node| node.descriptor.accessible_name.as_deref() == Some("Localized smile") && node.descriptor.accessible_selected == Some(true)));
    query.set("customkeyword".into());
    projection.snapshot(&mut element).unwrap();
    assert_eq!(controller.matched_count(), 1);
    query.set(String::new());
    recent.set(vec!["👍".into(), "😀".into()]);
    projection.snapshot(&mut element).unwrap();
    assert_eq!(controller.matched_count(), 2);
    groups.set(Some(vec![9]));
    let flags = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&flags).iter().any(|node| node.id == input_id));
    assert!(controller.matched_count() > 2);
    let category = nodes(&flags).into_iter().find(|node| node.descriptor.accessible_name.as_deref() == Some("Emoji category")).unwrap();
    assert!(matches!(&category.descriptor.content, NodeContent::Select { options, .. } if options.len() == 2 && options[1].value == "9"));
    version.set(Some("0.0".into()));
    let empty = projection.snapshot(&mut element).unwrap();
    assert_eq!(controller.matched_count(), 0);
    assert!(nodes(&empty).iter().any(|node| node.id == input_id));
    assert_eq!(controller.entry("😀"), None);
}

#[test]
fn skin_tone_signal_updates_catalog_and_accepts_filter_edits() {
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, tone)) = create_root(|| {
        let tone = create_signal(Some(5u8));
        (emoji_picker("reactive_tone").controller(controller.clone()).skin_tone_signal(tone)
            .query(EmojiQuery { query: "thumbs up".into(), ..Default::default() }).show_filters(true).build(), tone)
    });
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let filter_id = nodes(&initial).into_iter().find(|node| node.descriptor.accessible_name.as_deref() == Some("Skin tone")).unwrap().id;
    assert_eq!(controller.entry("👍").unwrap().text, "👍🏿");
    tone.set(Some(1));
    let updated = projection.snapshot(&mut element).unwrap();
    assert_eq!(controller.entry("👍").unwrap().text, "👍🏻");
    assert!(nodes(&updated).iter().any(|node| node.id == filter_id));
    let revision = match &nodes(&updated).into_iter().find(|node| node.id == filter_id).unwrap().descriptor.content {
        NodeContent::Select { revision, .. } => revision + 1, _ => unreachable!(),
    };
    projection.dispatch(&mut element, &PlatformEvent { node: filter_id, payload: EventPayload::Select { value: "3".into(), revision } }).unwrap();
    projection.snapshot(&mut element).unwrap();
    assert_eq!(tone.get(), Some(3));
    assert_eq!(controller.entry("👍").unwrap().text, "👍🏽");
}

#[test]
fn controlled_query_proposes_edits_and_waits_for_parent_acceptance() {
    use jadix_components::emoji_picker::EmojiPickerController;
    use jadix::portable::presentation::NavigationKey;
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, query, proposals)) = create_root(|| {
        let query = create_signal("cat".to_owned());
        let proposals = create_signal(Vec::<String>::new());
        let element = emoji_picker("controlled_query").controller(controller.clone())
            .query_fn(move || query.get())
            .on_query_change(move |value| proposals.update(|items| items.push(value))).build();
        (element, query, proposals)
    });
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    let input = nodes(&page).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    let revision = nodes(&page).into_iter().find_map(|node| match &node.descriptor.content { NodeContent::Input { revision, .. } if node.id == input => Some(*revision + 1), _ => None }).unwrap();
    let count = controller.matched_count();
    assert!(count > 0);
    projection.dispatch(&mut element, &PlatformEvent { node: input, payload: EventPayload::Edit {
        value: "thumbs up".into(), revision, selection_utf16: [9, 9], composing_utf16: None,
    }}).unwrap();
    let page = projection.snapshot(&mut element).unwrap();
    assert_eq!(query.get(), "cat");
    assert_eq!(controller.matched_count(), count);
    assert_eq!(proposals.get(), vec!["thumbs up"]);
    assert!(nodes(&page).iter().any(|node| node.id == input && matches!(&node.descriptor.content, NodeContent::Input { value, .. } if value == "cat")));
    query.set("thumbs up".into());
    projection.snapshot(&mut element).unwrap();
    assert!(controller.matched_count() < count);
    assert_eq!(proposals.get(), vec!["thumbs up"]);
    controller.navigate(NavigationKey::Escape);
    projection.snapshot(&mut element).unwrap();
    assert_eq!(query.get(), "thumbs up");
    assert_eq!(proposals.get(), vec!["thumbs up", ""]);
}

#[test]
fn controlled_query_can_accept_a_transformed_proposal_synchronously() {
    let (_owner, (mut element, query)) = create_root(|| {
        let query = create_signal(String::new());
        let element = emoji_picker("accepted_query").query_fn(move || query.get())
            .on_query_change(move |value| query.set(value.to_lowercase())).build();
        (element, query)
    });
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    let input = nodes(&page).into_iter().find(|node| matches!(node.descriptor.content, NodeContent::Input { .. })).unwrap().id;
    projection.dispatch(&mut element, &PlatformEvent { node: input, payload: EventPayload::Edit {
        value: "CAT".into(), revision: 1, selection_utf16: [3, 3], composing_utf16: None,
    }}).unwrap();
    let page = projection.snapshot(&mut element).unwrap();
    assert_eq!(query.get(), "cat");
    assert!(nodes(&page).iter().any(|node| node.id == input && matches!(&node.descriptor.content, NodeContent::Input { value, .. } if value == "cat")));
}

#[test]
fn stationary_pointer_does_not_override_keyboard_or_rebuilt_results() {
    use jadix_components::emoji_picker::{EmojiPickerController, EmojiQuery};
    use jadix::portable::presentation::NavigationKey;
    let controller = EmojiPickerController::default();
    let (_owner, (mut element, highlighted, query)) = create_root(|| {
        let highlighted = create_signal(String::new());
        let query = create_signal("face".to_string());
        let element = emoji_picker("motion").controller(controller.clone())
            .query(EmojiQuery::default()).query_signal(query)
            .on_highlight_change(move |entry| highlighted.set(entry.map_or(String::new(), |entry| entry.text)))
            .build();
        (element, highlighted, query)
    });
    let projection = ElementProjection::new(RootId::new());
    let page = projection.snapshot(&mut element).unwrap();
    let options: Vec<_> = nodes(&page).into_iter().filter(|node| matches!(node.descriptor.accessible_role, Some(AccessibleRole::ListBoxOption { .. }))).map(|node| node.id).collect();
    assert!(options.len() > 1);
    let motion = |node, x| PlatformEvent { node, payload: EventPayload::PointerMove { x, y: 10. } };
    projection.dispatch(&mut element, &motion(options[0], 10.)).unwrap();
    let first = highlighted.get();
    projection.dispatch(&mut element, &motion(options[1], 10.)).unwrap();
    assert_eq!(highlighted.get(), first);
    controller.navigate(NavigationKey::Right);
    let keyboard = highlighted.get();
    assert_ne!(keyboard, first);
    projection.dispatch(&mut element, &motion(options[0], 10.)).unwrap();
    assert_eq!(highlighted.get(), keyboard);
    assert_eq!(projection.dispatch(&mut element, &motion(options[0], f64::NAN)), Err(ProjectionError::InvalidEvent));
    query.set("cat".into());
    let page = projection.snapshot(&mut element).unwrap();
    let next = nodes(&page).into_iter().find(|node| matches!(node.descriptor.accessible_role, Some(AccessibleRole::ListBoxOption { .. }))).unwrap().id;
    let rebuilt = highlighted.get();
    projection.dispatch(&mut element, &motion(next, 10.)).unwrap();
    assert_eq!(highlighted.get(), rebuilt);
    projection.dispatch(&mut element, &motion(next, 11.)).unwrap();
    let page = projection.snapshot(&mut element).unwrap();
    assert_eq!(nodes(&page).into_iter().find(|node| node.id == next).unwrap().descriptor.accessible_selected, Some(true));
}
