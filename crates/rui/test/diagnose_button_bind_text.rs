//! Gallery pattern: `<Button>{ if sig.get() { … } else { … } }</Button>` via bind_text.

use crate::app::reactive::{bind_text, reset_registered_anchors};
use crate::components::button::{button, Button, ButtonProps, ButtonSize, ButtonVariant};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

#[test]
fn button_preserves_bind_text_caption_through_layout() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mark = use_signal("diag_btn_mark_invalid", || false);
            let caption = bind_text("diag:btn:mark-invalid", move || {
                if mark.get() {
                    "Clear invalid".to_string()
                } else {
                    "Mark invalid".to_string()
                }
            });
            let btn: Element = button("diag-mark-invalid")
                .variant("outline")
                .size("sm")
                .child(caption)
                .into();
            assert!(
                btn.children
                    .iter()
                    .any(|c| c.text.as_deref() == Some("Mark invalid")),
                "Button must keep bind_text caption; children={:?}",
                btn.children
                    .iter()
                    .map(|c| (c.template_id.clone(), c.text.clone()))
                    .collect::<Vec<_>>()
            );
            let mut root = Element::new().w(400.0).h(80.0).child(btn);
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
            assert!(
                result.text_nodes.iter().any(|n| n.text == "Mark invalid"),
                "layout must collect button caption; nodes={:?}",
                result
                    .text_nodes
                    .iter()
                    .map(|n| n.text.as_str())
                    .collect::<Vec<_>>()
            );
        });
    });
    reset_registered_anchors();
}

#[test]
fn button_component_empty_id_still_shows_bind_text() {
    reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mark = use_signal("diag_btn_comp_mark", || false);
            let caption = bind_text("diag:btn-comp:mark", move || {
                if mark.get() {
                    "Clear invalid".into()
                } else {
                    "Mark invalid".into()
                }
            });
            // Mimic `<Button variant outline size sm>` with default empty id.
            let el = Button(ButtonProps {
                id: String::new(),
                variant: Some(ButtonVariant::Outline),
                size: Some(ButtonSize::Sm),
                children: Some(caption),
                ..Default::default()
            });
            assert!(
                el.children
                    .iter()
                    .any(|c| c.text.as_deref() == Some("Mark invalid")),
                "Button component must surface bind_text; children={:?}",
                el.children
                    .iter()
                    .map(|c| (c.template_id.clone(), c.text.clone()))
                    .collect::<Vec<_>>()
            );
        });
    });
    reset_registered_anchors();
}
