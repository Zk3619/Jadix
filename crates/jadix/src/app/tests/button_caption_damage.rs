use super::AppState;
use crate::components::button::button;
use crate::core::tree::Element;

#[test]
fn rebuilt_hovered_button_keeps_its_used_chrome() {
    crate::core::reactivity::create_root(|| {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut tree = crate::layout::LayoutTree::new();
        let mut previous: Element = button("rebuild-hover")
            .variant("outline")
            .text("Toggle")
            .into();
        tree.build_and_layout(&mut previous, 240.0, 120.0, [-1.0; 2]);
        for time in [0.0, 200.0, 400.0] {
            crate::style::set_css_transition_timeline_ms(time);
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut previous,
                [10.0, 10.0],
                "",
                false,
                240.0,
                120.0,
                &mut tree,
                &mut impact,
            );
        }
        let hovered = previous.style.bg_color;
        let mut fresh: Element = button("rebuild-hover")
            .variant("outline")
            .text("Toggle")
            .into();
        AppState::reconcile_elements_for_test(&mut fresh, &mut previous);
        for time in [416.0, 432.0, 448.0] {
            crate::style::set_css_transition_timeline_ms(time);
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut fresh,
                [10.0, 10.0],
                "",
                false,
                240.0,
                120.0,
                &mut tree,
                &mut impact,
            );
            assert_eq!(
                fresh.style.bg_color, hovered,
                "rebuild must not restart hover chrome"
            );
        }
        crate::style::set_css_transition_timeline_ms(700.0);
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut fresh,
            [-1.0; 2],
            "",
            false,
            240.0,
            120.0,
            &mut tree,
            &mut impact,
        );
        crate::style::set_css_transition_timeline_ms(1000.0);
        crate::style::sync_class_variants_subtree(
            &mut fresh,
            [-1.0; 2],
            "",
            false,
            240.0,
            120.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            fresh.style.bg_color[3], 0.0,
            "preserving geometry must not pin hover"
        );
        crate::style::set_css_transition_virtual_timeline(false);
    });
}

#[test]
fn caption_layout_change_stays_local_to_button() {
    let root = Element::new().child(
        Element::from(button("trigger").text("Open"))
            .child(Element::new().template_id("trigger-caption")),
    );
    assert!(AppState::path_has_button_ancestor(&root, &[0, 0]));
    assert!(!AppState::path_has_button_ancestor(&root, &[]));
}

#[test]
fn conditional_button_caption_change_is_local_layout_only() {
    let old: Element = button("trigger").text("打开 RSX Trigger").into();
    let new: Element = button("trigger").text("关闭 RSX Trigger").into();
    assert!(AppState::button_caption_only_change(&new, &old));
    let old_page = Element::new().child(old);
    let new_page = Element::new().child(new);
    assert!(AppState::button_caption_only_change(&new_page, &old_page));

    let old_other = Element::new().child(Element::new().text("Open"));
    let new_other = Element::new().child(Element::new().text("Close"));
    assert!(!AppState::button_caption_only_change(
        &new_other, &old_other
    ));

    let old_structural: Element = button("trigger").text("Open").into();
    let mut new_structural: Element = button("trigger").text("Close").into();
    new_structural.children.push(Element::new());
    assert!(!AppState::button_caption_only_change(
        &new_structural,
        &old_structural
    ));
    assert!(AppState::has_button_caption_change(
        &new_structural,
        &old_structural
    ));
}

#[test]
fn portal_only_structure_change_does_not_schedule_window_clear() {
    let page_old: Element = crate::components::button::button("trigger")
        .text("Open")
        .into();
    let page_new: Element = crate::components::button::button("trigger")
        .text("Close")
        .into();
    let mut old_portal = Element::new().template_id("panel").text("old");
    old_portal.set_attr("jadix_portal", "1");
    let mut new_portal = Element::new().template_id("panel").text("new");
    new_portal.set_attr("jadix_portal", "1");
    let old = Element::new().child(page_old).child(old_portal);
    let new = Element::new().child(page_new).child(new_portal);
    assert!(AppState::portal_only_structure_change(&new, &old));

    let changed_page = Element::new()
        .child(Element::new().template_id("page").text("changed"))
        .child({
            let mut panel = Element::new().template_id("panel").text("new");
            panel.set_attr("jadix_portal", "1");
            panel
        });
    assert!(!AppState::portal_only_structure_change(&changed_page, &old));
}

#[test]
fn mixed_portal_caption_change_targets_trigger_only() {
    let mut old_button: Element = crate::components::button::button("trigger")
        .text("Open")
        .into();
    let mut new_button: Element = crate::components::button::button("trigger")
        .text("Close")
        .into();
    old_button.layout_bounds = [10.0, 20.0, 80.0, 24.0];
    new_button.layout_bounds = [10.0, 20.0, 92.0, 24.0];
    let mut old_panel = Element::new().template_id("panel").text("old");
    old_panel.set_attr("jadix_portal", "1");
    let mut new_panel = Element::new().template_id("panel").text("new");
    new_panel.set_attr("jadix_portal", "1");
    let old = Element::new().child(old_button).child(old_panel);
    let new = Element::new().child(new_button).child(new_panel);

    let damage = AppState::portal_button_caption_damage(&new, &old)
        .expect("caption change must produce trigger damage");
    assert_ne!(damage.0, 0);
    assert!(damage.1[2] > 0.0 && damage.1[3] > 0.0);
}
