use authoring_provider::{ExplicitBase, ExplicitBaseProps, ExternalLeaf, ExternalLeafProps};
use ui_runtime::app::props::merge_props;
use ui_runtime::authoring::{author_class, data_slot};
use ui_runtime::core::tree::Element;

#[path = "board_drag.rs"]
mod copied_board_drag;

#[test]
fn framework_div_and_composition_do_not_require_product_components() {
    let mut node: Element = ui_runtime::prelude::div().opacity_group(true).into();
    assert_eq!(node.set_opacity_group(false), ui_runtime::core::tree::LayoutStatus::StyleChanged);
    let rsx = ui_runtime::ui! {
        <Div opacity_group={true} opacity={0.5}>
            <div w="20px" h="20px" bg="#ff0000" />
        </Div>
    };
    assert_eq!(rsx.get_attr("jadix_opacity_group").map(String::as_str), Some("1"));
    assert_eq!(rsx.style.opacity, 0.5);
    assert_eq!(rsx.children.len(), 1);
    assert_eq!(ui_runtime::app::app().opacity_groups(false).config().opacity_groups, Some(false));
}

#[test]
fn renamed_runtime_supports_cross_crate_generic_components_and_props() {
    let props = merge_props(
        ExternalLeafProps {
            value: Some(7usize),
            children: Some(Element::new().text("child")),
            base: Some(Box::new(|builder| builder.text_size(18.0))),
        },
        ExternalLeafProps {
            value: Some(9usize),
            base: Some(Box::new(|builder| builder.opacity(0.6))),
            ..Default::default()
        },
    );
    let element = ExternalLeaf(props);
    assert_eq!(element.text.as_deref(), Some("9"));
    assert_eq!(element.children[0].text.as_deref(), Some("child"));
    assert_eq!(element.text_style.size, Some(18.0));
    assert_eq!(element.style.opacity, 0.6);
}

#[test]
fn explicit_base_builder_composes_with_the_generated_base_callback() {
    let element = ExplicitBase(ExplicitBaseProps {
        value: Some("leaf".into()),
        base_builder: Some(Box::new(|builder| builder.opacity(0.3).text_size(17.0))),
        base: Some(Box::new(|builder| builder.opacity(0.8))),
        ..Default::default()
    });
    assert_eq!(element.text.as_deref(), Some("leaf"));
    assert_eq!(element.style.opacity, 0.8);
    assert_eq!(element.text_style.size, Some(17.0));
}

#[test]
fn exported_builder_macros_work_from_the_renamed_runtime() {
    let element = authoring_provider::surface()
        .content_class("p-3")
        .content_style(|builder| builder.text_size(22.0))
        .finish();
    assert_eq!(data_slot(&element), Some("content"));
    assert_eq!(author_class(&element), "p-3");
    assert_eq!(element.text_style.size, Some(22.0));
    let element: Element = authoring_provider::leaf()
        .class("rounded")
        .text("packed")
        .into();
    assert_eq!(author_class(&element), "rounded");
    assert_eq!(element.children[0].text.as_deref(), Some("packed"));
}

#[test]
fn external_product_registers_drag_handlers_without_host_product_paths() {
    use ui_runtime::utils::drag::routing;
    authoring_provider::start_external_drag();
    assert!(routing::blocks_content_pan());
    assert!(!routing::freezes_scroll());
    assert!(routing::prepare_move(None, 13.0, 21.0).dispatch());
    assert_eq!(
        authoring_provider::external_drag_position(),
        Some([13.0, 21.0])
    );
    routing::finish_all();
    assert_eq!(authoring_provider::external_drag_position(), None);
    assert!(!routing::blocks_content_pan());
}

#[test]
fn external_board_owns_advanced_drag_without_the_built_in_dnd_session() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_runtime::utils::drag::runtime::{self, MoveMode};
    let tree = Rc::new(RefCell::new(Element::new().text("board")));
    authoring_provider::board_drag::install(tree.clone());
    assert!(runtime::is_active());
    let update = runtime::prepare_move(
        Some(&tree.borrow()),
        [15.0, 30.0],
        [300.0, 200.0],
        MoveMode::Reconcile,
    );
    assert!(update.dispatch());
    assert_eq!(
        tree.borrow().text.as_deref(),
        Some("board moved to [15.0, 30.0]")
    );
    let release = runtime::prepare_end(&tree.borrow());
    assert!(release.dispatch());
    assert_eq!(tree.borrow().text.as_deref(), Some("board committed"));
    assert!(!runtime::is_active());
}

#[test]
fn the_same_board_source_can_be_embedded_directly_in_the_consumer() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_runtime::utils::drag::runtime::{self, DragKey, MoveMode};
    let tree = Rc::new(RefCell::new(Element::new().text("copied board")));
    copied_board_drag::install(tree.clone());
    let update = runtime::prepare_move(
        Some(&tree.borrow()),
        [9.0, 18.0],
        [300.0, 200.0],
        MoveMode::Follow,
    );
    assert!(update.dispatch());
    assert_eq!(
        tree.borrow().text.as_deref(),
        Some("copied board moved to [9.0, 18.0]")
    );
    assert!(runtime::key(DragKey::Cancel));
    assert!(!runtime::is_active());
}

#[test]
fn both_board_distribution_paths_defer_scroll_callbacks() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_runtime::utils::drag::runtime;
    for install in [
        authoring_provider::board_drag::install,
        copied_board_drag::install,
    ] {
        let tree = Rc::new(RefCell::new(Element::new().text("board")));
        install(tree.clone());
        let scroll = runtime::prepare_autoscroll(&tree.borrow(), [300.0, 200.0]);
        let inertia = runtime::prepare_inertia(&tree.borrow(), [300.0, 200.0]);
        assert_eq!(tree.borrow().text.as_deref(), Some("board"));
        assert!(scroll.dispatch());
        assert_eq!(tree.borrow().text.as_deref(), Some("board scrolled"));
        assert!(inertia.dispatch());
        assert_eq!(tree.borrow().text.as_deref(), Some("board coasted"));
        runtime::unregister();
    }
}

#[test]
fn renamed_runtime_ui_expands_primitives_components_and_product_sugar() {
    let (_owner, ()) = ui_runtime::core::reactivity::create_root(|| {
        let leaf = ui_runtime::ui! { <ExplicitBase value="leaf" /> };
        assert_eq!(leaf.text.as_deref(), Some("no base"));
        let primitive = ui_runtime::ui! { <Box w={64.0} h={32.0}>"box"</Box> };
        assert_eq!(primitive.children[0].text.as_deref(), Some("box"));
        let progress = authoring_provider::ui! {
            <Progress id="external-progress" value={0.4}><Box>"progress"</Box></Progress>
        };
        assert_eq!(progress.attrs.get("external-progress").map(String::as_str), Some("external-progress"));
        assert_eq!(progress.attrs.get("value").map(String::as_str), Some("0.4"));
    });
}

#[test]
fn renamed_runtime_for_implicit_item_uses_caller_scope() {
    ui_runtime::core::reactivity::create_root(|| {
        let tree = ui_runtime::ui! {
            <For each={1..=2} key={*item}>
                <Box>{format!("renamed {item}")}</Box>
            </For>
        };
        fn contains(node: &Element, text: &str) -> bool {
            node.text.as_deref() == Some(text)
                || node.children.iter().any(|child| contains(child, text))
        }
        assert!(contains(&tree, "renamed 2"));
    });
}

#[test]
fn framework_layers_are_available_without_product_components() {
    use ui_runtime::authoring::{SlotMap, SLOT_CONTENT, SLOT_OVERLAY, SLOT_SURFACE};
    use ui_runtime::core::tree::PointerEvents;
    let panel = SlotMap::default()
        .surface(|slot| slot.style(|base| base.opacity(0.4)))
        .layers(Some(Element::new()), Element::new().text("Content"),
            Some(Element::new().child(Element::new().on_click(|| {}))));
    assert_eq!(panel.children[0].get_attr("data-slot").map(String::as_str), Some(SLOT_SURFACE));
    assert_eq!(panel.children[1].get_attr("data-slot").map(String::as_str), Some(SLOT_CONTENT));
    assert_eq!(panel.children[2].get_attr("data-slot").map(String::as_str), Some(SLOT_OVERLAY));
    assert_eq!(panel.children[0].pointer_events_mode(), PointerEvents::None);
    assert_eq!(panel.children[2].pointer_events_mode(), PointerEvents::BoxNone);
    assert_eq!(panel.children[1].style.opacity, 1.0);
}
