use super::*;
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, ui_root_effect_dep_count, use_signal,
};
use crate::style::{TextAlign, TextAlignLast};
use std::cell::Cell;

#[test]
fn rsx_text_alignment_updates_leaves_and_disposes_bindings() {
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    runtime::install_test_reconcile_root(live.clone());
    let builds = Rc::new(Cell::new(0));
    let target = live.clone();
    let build_counter = builds.clone();
    let align = use_signal("text-align-leaf-regression", || "left");
    let last = use_signal("text-align-last-leaf-regression", || "center");
    align.set_silent("left");
    last.set_silent("center");
    let (owner, ()) = create_root(move || {
        mark_next_effect_as_ui_root();
        create_effect(move || {
            build_counter.set(build_counter.get() + 1);
            let mut leaf: Element = crate::core::reactivity::isolate_reads(|| {
                crate::ui! {
                    <Text text_align={align.get()} text_align_last={last.get()} class="hover:bg-red-500">
                        "retained label"
                    </Text>
                }
            });
            leaf.focus_id = Some("alignment-focus".into());
            *target.borrow_mut() =
                Element::new().children([leaf, Element::new().text("untouched")]);
        });
    });
    assert_eq!(builds.get(), 1);
    assert_eq!(ui_root_effect_dep_count(), 0);
    assert_eq!(registered_binding_count(), 2);
    let identity = live.borrow().children[0].template_id.clone();
    let sibling_identity = live.borrow().children[1].inner.as_ref() as *const _;
    for (alignment, ending, expected_align, expected_last) in [
        ("right", "left", TextAlign::Right, TextAlignLast::Left),
        (
            "center",
            "justify",
            TextAlign::Center,
            TextAlignLast::Justify,
        ),
        ("left", "right", TextAlign::Left, TextAlignLast::Right),
    ] {
        align.set(alignment);
        last.set(ending);
        let mut root = live.borrow_mut();
        let leaf = &mut root.children[0];
        crate::style::resolve_class_variants(leaf, &crate::style::VariantContext::default());
        assert_eq!(leaf.text_style.align, Some(expected_align));
        assert_eq!(leaf.text_style.align_last, Some(expected_last));
        assert_eq!(leaf.template_id, identity);
        assert_eq!(leaf.focus_id.as_deref(), Some("alignment-focus"));
        assert_eq!(
            root.children[1].inner.as_ref() as *const _,
            sibling_identity
        );
        assert_eq!(builds.get(), 1);
        assert_eq!(ui_root_effect_dep_count(), 0);
    }
    owner.dispose();
    assert_eq!(registered_binding_count(), 0);
    align.set("right");
    assert_eq!(
        live.borrow().children[0].text_style.align,
        Some(TextAlign::Left)
    );
    assert_eq!(builds.get(), 1);
    runtime::clear_test_reconcile_root();
    reset_registered_anchors();
    crate::core::reactivity::clear_ui_root_effect();
}
