use super::*;

#[test]
fn material_effects_and_overlay_hit_policy_stay_separate_from_content() {
    let slots = SlotMap::default()
        .surface(|slot| slot.style(|base| base.bg("red")))
        .content(|slot| slot.class("p-2"))
        .overlay(|slot| slot.class("rounded-sm"));
    let mut root = slots.layers(
        Some(Element::new().opacity(0.4)),
        Element::new().w(160.0).h(80.0).text("Content"),
        Some(Element::new().child(Element::new().w(12.0).h(12.0).on_click(|| {}))),
    );
    assert_eq!(root.children.len(), 3);
    let mut layout = crate::layout::LayoutTree::new();
    layout.build_and_layout(&mut root, 320.0, 200.0, [-1.0; 2]);
    let layer = |name| {
        crate::core::tree::find_in_tree(&root, |node| {
            node.get_attr("data-slot").map(String::as_str) == Some(name)
        })
        .unwrap()
    };
    let surface = layer(SLOT_SURFACE);
    let content = layer(SLOT_CONTENT);
    let overlay = layer(SLOT_OVERLAY);
    assert_eq!(
        surface.get_attr("data-slot").map(String::as_str),
        Some(SLOT_SURFACE)
    );
    assert_eq!(
        content.get_attr("data-slot").map(String::as_str),
        Some(SLOT_CONTENT)
    );
    assert_eq!(
        overlay.get_attr("data-slot").map(String::as_str),
        Some(SLOT_OVERLAY)
    );
    assert_eq!(
        surface.pointer_events_mode(),
        crate::core::tree::PointerEvents::None
    );
    assert_eq!(
        overlay.pointer_events_mode(),
        crate::core::tree::PointerEvents::BoxNone
    );
    assert_eq!(content.style.opacity, 1.0);
    assert!(content.layout_bounds[2] >= 160.0);
    assert!(overlay.children[0].on_click.is_some());
    let plain = SlotMap::default().layers(None, Element::new().text("Only content"), None);
    assert_eq!(plain.children.len(), 1);
}
