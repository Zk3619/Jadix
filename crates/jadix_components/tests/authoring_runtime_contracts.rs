use jadix_components::authoring::{mount, presentation};
use jadix_components::core::reactivity::create_root;
use jadix_components::core::tree::{Element, LayoutStatus};
use jadix_components::layout::LayoutTree;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn portal_replacement_retains_identity_z_and_synchronizer() {
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let mut host = Element::new().reactive_sync(move |_, _| {
        count.set(count.get() + 1);
        LayoutStatus::StyleChanged
    });
    host.style.z_index = 37;
    for key in [
        "jadix_portal_key",
        "jadix_portal",
        "jadix_portal_mount",
        "jadix_hoisted_portal",
        "jadix_portal_hoisted_mount",
    ] {
        host.set_attr(key, key);
    }
    host.set_attr("old-body", "removed");
    mount::replace_portal_content(&mut host, Element::new().text("replacement"));
    assert_eq!(host.style.z_index, 37);
    assert_eq!(host.text.as_deref(), Some("replacement"));
    assert!(!host.attrs.contains_key("old-body"));
    for key in [
        "jadix_portal_key",
        "jadix_portal",
        "jadix_portal_mount",
        "jadix_hoisted_portal",
        "jadix_portal_hoisted_mount",
    ] {
        assert_eq!(host.attrs.get(key).map(String::as_str), Some(key));
    }
    let sync = host.reactive_sync.clone().unwrap();
    assert_eq!(
        sync.run(&mut host, &mut LayoutTree::new()),
        LayoutStatus::StyleChanged
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn inline_mount_nesting_and_unwind_restore_reactive_placeholders() {
    let (_owner, ()) = create_root(|| {
        mount::with_inline_mount(|| {
            mount::with_inline_mount(|| {
                let child = jadix_components::app::reactive::reactive_view_sized(
                    "inline-nested",
                    20.0,
                    10.0,
                    || Element::new().text("live"),
                );
                assert!(!mount::is_reactive_placeholder(&child));
                assert_eq!(child.text.as_deref(), Some("live"));
            });
            let child = jadix_components::app::reactive::reactive_view_sized(
                "inline-outer",
                20.0,
                10.0,
                Element::new,
            );
            assert!(!mount::is_reactive_placeholder(&child));
        });
        let panic = std::panic::catch_unwind(|| mount::with_inline_mount(|| panic!("restore")));
        assert!(panic.is_err());
        let child = jadix_components::app::reactive::reactive_view_sized(
            "after-unwind",
            20.0,
            10.0,
            Element::new,
        );
        assert!(mount::is_reactive_placeholder(&child));
    });
}

#[test]
fn node_owned_presentation_composes_phases_and_releases_captures() {
    let captured = Rc::new(Cell::new(0));
    let mut node = Element::new();
    let hover_count = captured.clone();
    presentation::attach(
        &mut node,
        presentation::Phase::Hover,
        move |element, _, context| {
            assert_eq!(context.viewport, [400.0, 300.0]);
            hover_count.set(hover_count.get() + 1);
            element.text = Some("hover".into());
            LayoutStatus::StyleChanged
        },
    );
    presentation::attach(&mut node, presentation::Phase::State, |element, _, _| {
        element.set_attr("state", "checked");
        LayoutStatus::LayoutChanged
    });
    let mut layout = LayoutTree::new();
    let hover = presentation::Context {
        phase: presentation::Phase::Hover,
        viewport: [400.0, 300.0],
        width_budget: None,
    };
    assert_eq!(
        presentation::sync_tree(&mut node, &mut layout, hover),
        LayoutStatus::StyleChanged
    );
    assert_eq!(captured.get(), 1);
    assert!(!node.attrs.contains_key("state"));
    let state = presentation::Context {
        phase: presentation::Phase::State,
        ..hover
    };
    assert_eq!(
        presentation::sync_tree(&mut node, &mut layout, state),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(captured.get(), 1);
    drop(node);
    assert_eq!(Rc::strong_count(&captured), 1);
}

#[test]
fn product_checked_chrome_runs_through_node_owned_presentation() {
    let (_owner, ()) = create_root(|| {
        let checked =
            jadix_components::core::reactivity::use_signal("presentation-checked", || false);
        let mut root = Element::new()
            .child(
                jadix_components::components::switch::switch("presentation-switch")
                    .checked_signal(checked),
            )
            .child(
                jadix_components::components::checkbox::checkbox("presentation-checkbox")
                    .checked_signal(checked),
            );
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
        let before: Vec<_> = root
            .children
            .iter()
            .map(|child| child.style.bg_color)
            .collect();
        checked.set(true);
        let status = presentation::sync_tree(
            &mut root,
            &mut layout,
            presentation::Context {
                phase: presentation::Phase::State,
                viewport: [400.0, 300.0],
                width_budget: None,
            },
        );
        assert_ne!(status, LayoutStatus::Clean);
        let after: Vec<_> = root
            .children
            .iter()
            .map(|child| child.style.bg_color)
            .collect();
        assert_ne!(before, after);
    });
}

#[test]
fn after_style_callbacks_observe_children_and_compose_without_a_tree_scan() {
    let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut child = Element::new().w(40.0).h(30.0).on_hover(|_| {});
    let child_calls = calls.clone();
    presentation::attach_after_style(&mut child, move |element, hovered| {
        child_calls.borrow_mut().push("child");
        element.set_attr("child-style-ready", "true");
        let color = if hovered(element) {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0; 4]
        };
        let changed = element.style.bg_color != color;
        element.style.bg_color = color;
        changed
    });
    let mut root = Element::new().w(240.0).h(80.0).child(child);
    let first_calls = calls.clone();
    presentation::attach_after_style(&mut root, move |element, _| {
        assert_eq!(
            element.children[0]
                .attrs
                .get("child-style-ready")
                .map(String::as_str),
            Some("true")
        );
        first_calls.borrow_mut().push("parent-first");
        false
    });
    let second_calls = calls.clone();
    presentation::attach_after_style(&mut root, move |_, _| {
        second_calls.borrow_mut().push("parent-second");
        false
    });
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
    calls.borrow_mut().clear();
    let mut impact = LayoutStatus::Clean;
    jadix_components::style::sync_class_variants_subtree(
        &mut root,
        [5.0, 5.0],
        "",
        false,
        240.0,
        80.0,
        &mut layout,
        &mut impact,
    );
    assert_eq!(
        &*calls.borrow(),
        &["child", "parent-first", "parent-second"]
    );
    assert_eq!(root.children[0].style.bg_color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(impact, LayoutStatus::StyleChanged);
    drop(root);
    assert_eq!(Rc::strong_count(&calls), 1);
}

#[test]
fn visible_presentation_respects_scroll_and_keeps_unconditional_work() {
    let visits = Rc::new(Cell::new(0));
    let mut child = Element::new();
    child.layout_bounds = [0.0, 900.0, 50.0, 20.0];
    let visible_visits = visits.clone();
    presentation::attach_visible(&mut child, presentation::Phase::Hover, move |_, _, _| {
        visible_visits.set(visible_visits.get() + 1);
        LayoutStatus::Clean
    });
    presentation::attach(&mut child, presentation::Phase::Hover, |node, _, _| {
        node.set_attr("unconditional", "ran");
        LayoutStatus::Clean
    });
    let mut root = Element::new().child(child);
    root.style.overflow.y = taffy::style::Overflow::Scroll;
    root.layout_bounds = [0.0, 0.0, 200.0, 200.0];
    let mut layout = LayoutTree::new();
    let context = presentation::Context {
        phase: presentation::Phase::Hover,
        viewport: [200.0, 200.0],
        width_budget: None,
    };
    presentation::sync_tree(&mut root, &mut layout, context);
    assert_eq!(visits.get(), 0);
    assert_eq!(
        root.children[0]
            .attrs
            .get("unconditional")
            .map(String::as_str),
        Some("ran")
    );
    root.scroll_offset = [0.0, 850.0];
    presentation::sync_tree(&mut root, &mut layout, context);
    assert_eq!(visits.get(), 1);
    root.layout_bounds[1] = 900.0;
    presentation::sync_tree(&mut root, &mut layout, context);
    assert_eq!(
        visits.get(),
        1,
        "an offscreen parent suppresses visible-only work"
    );
}

#[test]
fn custom_focus_policy_is_explicit_and_independent_of_product_id() {
    use jadix_components::authoring::focus;
    let mut node = Element::new().focus_id("custom-control");
    assert!(!focus::blur_on_outside_press(&node, "custom-control"));
    node.set_attr(focus::ATTR_BLUR_ON_OUTSIDE_PRESS, "true");
    assert!(focus::blur_on_outside_press(&node, "custom-control"));
    assert!(!focus::blur_on_outside_press(&node, "missing"));
    assert!(!focus::blur_on_outside_press(
        &Element::new().focus_id("input-otp:legacy"),
        "input-otp:legacy"
    ));
    create_root(|| {
        let otp = Element::from(jadix_components::components::input_otp::input_otp(
            "custom-policy",
        ));
        assert!(focus::blur_on_outside_press(
            &otp,
            "input-otp:custom-policy"
        ));
    });
}

#[test]
fn text_command_handlers_belong_to_nodes_and_do_not_cross_trees() {
    use jadix_components::authoring::text;
    let state = Rc::new(Cell::new(0));
    let captured = state.clone();
    let first: text::KeyHandler = Rc::new(move |_, _| {
        captured.set(1);
        true
    });
    let second: text::KeyHandler = Rc::new(|_, _| false);
    let mut first_tree = Element::new().focus_id("textarea:editor");
    first_tree.text_edit_key_handler = Some(first.clone());
    let mut second_tree = Element::new().focus_id("textarea:editor");
    second_tree.text_edit_key_handler = Some(second.clone());
    assert!(Rc::ptr_eq(
        &text::key_handler(&first_tree, "textarea:editor").unwrap(),
        &first
    ));
    assert!(Rc::ptr_eq(
        &text::key_handler(&second_tree, "textarea:editor").unwrap(),
        &second
    ));
    assert!(text::key_handler(&first_tree, "textarea:other").is_none());
    drop(first_tree);
    drop(first);
    assert_eq!(Rc::strong_count(&state), 1);
    assert!(text::key_handler(&second_tree, "textarea:editor").is_some());
    create_root(|| {
        let editor = Element::from(
            jadix_components::components::text_editor::text_editor("node-command")
                .key_handler(|_, _| true),
        );
        assert!(text::key_handler(&editor, "textarea:node-command").is_some());
    });
}

#[test]
fn button_hover_and_focus_update_through_shared_presentation() {
    create_root(|| {
        let hovered = jadix_components::core::reactivity::use_signal("node-button_hover", || false);
        let focused =
            jadix_components::core::reactivity::use_signal("__jadix_button_focus_id", String::new);
        let focus_visible =
            jadix_components::core::reactivity::use_signal("__jadix_focus_visible", || false);
        let mut button = Element::from(
            jadix_components::components::button::button("node-button").text("hover"),
        );
        button.class_set = None;
        button.style.transition.clear();
        let mut root = Element::new().child(button);
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0; 2]);
        let before = root.children[0].style.bg_color;
        hovered.set_silent(true);
        focused.set_silent("node-button".into());
        focus_visible.set_silent(true);
        let changed = presentation::sync_tree(
            &mut root,
            &mut layout,
            presentation::Context {
                phase: presentation::Phase::Hover,
                viewport: [400.0, 300.0],
                width_budget: None,
            },
        );
        assert_ne!(changed, LayoutStatus::Clean);
        assert_ne!(root.children[0].style.bg_color, before);
        assert!(root.children[0].style.shadow_color[3] > 0.0);
        hovered.set_silent(false);
        focused.set_silent(String::new());
        focus_visible.set_silent(false);
        presentation::sync_tree(
            &mut root,
            &mut layout,
            presentation::Context {
                phase: presentation::Phase::Hover,
                viewport: [400.0, 300.0],
                width_budget: None,
            },
        );
        assert_eq!(root.children[0].style.bg_color, before);
    });
}

#[test]
fn width_budget_crosses_auto_wrappers_and_preserves_parent_constraints() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut leaf = Element::new().w("auto");
    leaf.layout_bounds = [0.0, 0.0, 300.0, 20.0];
    let budgets = seen.clone();
    presentation::attach(
        &mut leaf,
        presentation::Phase::AfterGeometry,
        move |_, _, context| {
            budgets.borrow_mut().push(context.width_budget);
            LayoutStatus::Clean
        },
    );
    let mut wrapper = Element::new().w("auto").child(leaf);
    wrapper.layout_bounds = [0.0, 0.0, 300.0, 40.0];
    let mut root = Element::new().w(72.0).child(wrapper);
    root.layout_bounds = [0.0, 0.0, 72.0, 120.0];
    let mut layout = LayoutTree::new();
    presentation::sync_tree(
        &mut root,
        &mut layout,
        presentation::Context::new(presentation::Phase::AfterGeometry, [400.0, 300.0]),
    );
    root.layout_bounds[2] = 44.0;
    presentation::sync_tree(
        &mut root,
        &mut layout,
        presentation::Context::new(presentation::Phase::AfterGeometry, [400.0, 300.0]),
    );
    assert_eq!(*seen.borrow(), [Some(72.0), Some(44.0)]);
}

#[test]
fn authored_opaque_fill_is_collected_and_invalidates_retained_paint() {
    let mut root = Element::new()
        .w(100.0)
        .h(50.0)
        .bg([1.0, 0.0, 0.0, 1.0])
        .opacity(0.5);
    root.set_attr("data-slot", "message-scroller-button");
    let mut layout = LayoutTree::new();
    layout.set_collect_skip_allowed(true);
    let ordinary = layout.build_and_layout(&mut root, 100.0, 50.0, [-1.0; 2]);
    assert!(ordinary
        .instances
        .iter()
        .any(|instance| instance.color == [1.0, 0.0, 0.0, 0.5]));
    root.attrs.remove("data-slot");
    root.paint_opaque_fill = true;
    let authored = layout.build_and_layout_fast(&mut root, 100.0, 50.0, [-1.0; 2], false, false);
    assert!(authored
        .instances
        .iter()
        .any(|instance| instance.color == [1.0, 0.0, 0.0, 1.0]));
    root.paint_opaque_fill = false;
    let restored = layout.build_and_layout_fast(&mut root, 100.0, 50.0, [-1.0; 2], false, false);
    assert!(restored
        .instances
        .iter()
        .any(|instance| instance.color == [1.0, 0.0, 0.0, 0.5]));
    root.paint_region_replay = true;
    assert_eq!(
        jadix_components::app::damage::paint_kind_for_element(&root),
        jadix_components::app::damage::PaintDirtyKind::Region
    );
}

#[test]
fn edge_paint_uses_authored_geometry_color_and_cached_changes() {
    use presentation::{EdgePaint, PaintEdge};
    let mut root = Element::new().w(100.0).h(50.0);
    root.paint_edge = Some(EdgePaint {
        edge: PaintEdge::Bottom,
        width: 3.0,
        color: [1.0, 0.0, 0.0, 1.0],
    });
    let mut layout = LayoutTree::new();
    layout.set_collect_skip_allowed(true);
    let bottom = layout.build_and_layout(&mut root, 100.0, 50.0, [-1.0; 2]);
    assert!(bottom
        .instances
        .iter()
        .any(|instance| instance.rect_pos == [0.0, 47.0]
            && instance.rect_size == [100.0, 3.0]
            && instance.color == [1.0, 0.0, 0.0, 1.0]));
    root.paint_edge = Some(EdgePaint {
        edge: PaintEdge::Right,
        width: 4.0,
        color: [0.0, 1.0, 0.0, 1.0],
    });
    let right = layout.build_and_layout_fast(&mut root, 100.0, 50.0, [-1.0; 2], false, false);
    assert!(right
        .instances
        .iter()
        .any(|instance| instance.rect_pos == [96.0, 0.0]
            && instance.rect_size == [4.0, 50.0]
            && instance.color == [0.0, 1.0, 0.0, 1.0]));
    assert_eq!(root.layout_bounds, [0.0, 0.0, 100.0, 50.0]);
    root.paint_edge = None;
    let cleared = layout.build_and_layout_fast(&mut root, 100.0, 50.0, [-1.0; 2], false, false);
    assert!(!cleared
        .instances
        .iter()
        .any(|instance| instance.color == [0.0, 1.0, 0.0, 1.0]));
}
