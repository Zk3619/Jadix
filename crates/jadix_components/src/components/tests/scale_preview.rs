use super::*;

#[test]
fn scaled_preview_clicks_and_responsive_classes_follow_logical_viewport() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        for scale in [0.35, 0.57, 1.0, 1.5] {
            let button = Element::new()
                .absolute()
                .left(80.0)
                .top(60.0)
                .w(100.0)
                .h(40.0)
                .on_click(|| {});
            let mut root = Element::new().children([
                scale_preview(390.0, 300.0, scale)
                    .children([button, Element::new().class("w-10 md:w-32")])
                    .into(),
                Element::new().class("w-10 md:w-32"),
            ]);
            let mut layout = crate::layout::layout_tree::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut root,
                [-1.0; 2],
                "",
                false,
                1200.0,
                800.0,
                &mut layout,
                &mut impact,
            );
            assert_eq!(
                root.children[0].children[0].children[1].style.size.width,
                taffy::style::Dimension::length(40.0)
            );
            assert_eq!(
                root.children[1].style.size.width,
                taffy::style::Dimension::length(128.0)
            );
            layout.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
            assert!(
                crate::app::tree_hit::find_click(&root, 130.0 * scale, 80.0 * scale, [0.0; 2])
                    .is_some()
            );
            assert!(
                crate::app::tree_hit::find_click(&root, 190.0 * scale, 80.0 * scale, [0.0; 2])
                    .is_none()
            );
        }
    });
}
