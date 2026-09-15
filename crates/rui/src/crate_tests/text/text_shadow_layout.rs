//! Gallery-style `text_shadow` must reach `TextLayoutInfo` after layout collect.
//!
//! Catches regressions where RSX/`Label` base attrs or Bake routing drop shadows
//! before prepare (symptom: gallery「Text-shadow」shows plain text).

use crate::components::base::BaseComponentBuilder;
use crate::components::label::{label, Label, LabelProps};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn layout_shadow_nodes(mut root: Element) -> Vec<(String, Option<[u8; 4]>, [f32; 2], f32)> {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 200.0, [-1.0, -1.0]);
    result
        .text_nodes
        .into_iter()
        .map(|n| {
            (
                n.text,
                n.style.shadow_color,
                n.style.shadow_offset,
                n.style.shadow_blur,
            )
        })
        .collect()
}

#[test]
fn element_text_shadow_reaches_layout_text_nodes() {
    let root = Element::new().w(800.0).h(200.0).child(
        Element::new()
            .text("简写 dx/dy/blur")
            .text_size(17.6)
            .font_weight(800.0)
            .text_shadow("0.125rem 0.125rem 0.25rem black"),
    );
    let nodes = layout_shadow_nodes(root);
    let hit = nodes
        .iter()
        .find(|(t, _, _, _)| t.contains("简写"))
        .expect("text leaf present");
    assert!(
        hit.1.is_some(),
        "shadow_color must be set on TextLayoutInfo, got {:?}",
        hit
    );
    assert!(
        hit.2[0].abs() > 0.0 && hit.2[1].abs() > 0.0,
        "shadow offset expected, got {:?}",
        hit.2
    );
    assert!(hit.3 > 0.0, "shadow blur expected, got {}", hit.3);
}

#[test]
fn label_base_text_shadow_reaches_layout_text_nodes() {
    // Mirrors RSX `<Label text_shadow=...>` → LabelProps.base closure.
    let el = Label(LabelProps {
        id: String::new(),
        text: None,
        center: None,
        for_target: None,
        truncate: None,
        text_overflow: None,
        children: Some(Element::new().text("子属性覆盖")),
        base: Some(Box::new(|mut b: BaseComponentBuilder| {
            b = b.text_size("1.1rem");
            b = b.font_weight("800");
            b = b.text_color(crate::style::parse_text_color("foreground"));
            b = b.text_shadow("0.125rem 0.125rem 0.25rem black");
            b
        })),
    });
    let root = Element::new().w(800.0).h(200.0).child(el);
    let nodes = layout_shadow_nodes(root);
    let hit = nodes
        .iter()
        .find(|(t, _, _, _)| t.contains("子属性"))
        .expect("Label text leaf present");
    assert!(
        hit.1.is_some_and(|c| c[3] > 0),
        "Label base text_shadow must survive layout collect, got {:?}",
        hit
    );
    assert!(hit.3 > 0.0, "blur must be > 0, got {}", hit.3);
}

#[test]
fn label_builder_text_shadow_zero_blur_offset_reaches_nodes() {
    let el: Element = label("")
        .text("横向偏移")
        .size(17.6)
        .font_weight(800.0)
        .into();
    let el = el.text_shadow("0.25rem 0 0 black");
    let root = Element::new().w(800.0).h(200.0).child(el);
    let nodes = layout_shadow_nodes(root);
    let hit = nodes
        .iter()
        .find(|(t, _, _, _)| t.contains("横向"))
        .expect("text leaf");
    assert!(hit.1.is_some(), "color for blur=0 shadow: {:?}", hit);
    assert!(hit.2[0].abs() > 0.0, "dx for blur=0: {:?}", hit.2);
    assert_eq!(hit.3, 0.0, "blur should be 0, got {}", hit.3);
}
