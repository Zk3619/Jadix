//! App-shell flex: fixed sidebar must not shrink when main content has large min-width.

use crate::components::scroll_area::scroll_area;
use crate::components::sidebar::sidebar;
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::vstack;

#[test]
fn sidebar_keeps_width_when_main_content_has_large_min_width() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let side: Element = sidebar("shell-side").w("13rem").into();
            let wide = vstack()
                .w("100%")
                .min_w(900.0)
                .child(Element::new().w(900.0).h(40.0).bg([1.0, 1.0, 1.0, 1.0]));
            let main = Element::from(
                scroll_area("shell-main")
                    .h("100%")
                    .min_w(0.0)
                    .flex_grow(1.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(wide),
            );
            let mut root = Element::new()
                .flex_row()
                .w("100%")
                .h("100%")
                .children([side, main]);

            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 720.0, 600.0, [-1.0, -1.0]);

            fn find_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
                if el.template_id.as_deref() == Some(id) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_id(c, id))
            }
            let side = find_id(&root, "shell-side").expect("sidebar");
            let side_w = side.layout_bounds[2];
            assert!(
                (side_w - 208.0).abs() < 1.0,
                "sidebar must stay 13rem (208px), got {side_w} — flex-shrink crushed the rail"
            );
        });
    });
}
