//! Regression: ScrollArea content must not flex-shrink into the viewport.
//!
//! Shrinking content to fit makes `content_h == viewport_h`, so overflow clip
//! appears when the window gets shorter but wheel/scrollbar never enable
//! (gallery root-scroll after viewport height reduce).

use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};

#[test]
fn short_viewport_enables_y_scroll_when_content_taller() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            // Tall column content (~480px) inside a 200px-tall ScrollArea.
            let content = Element::new()
                .flex_col()
                .w("100%")
                .gap(8.0)
                .children((0..12).map(|i| {
                    Element::new()
                        .w("100%")
                        .h(32.0)
                        .bg([0.9, 0.9, 0.9, 1.0])
                        .template_id(format!("row-{i}"))
                }));

            let mut root = Element::from(
                scroll_area("diag-shrink-scroll")
                    .w(300.0)
                    .h(200.0)
                    .overflow_y("auto")
                    .overflow_x("hidden")
                    .child(content),
            );

            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0, -1.0]);
            let (_id, bounds, max_scroll, content_size) = result
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == "diag-shrink-scroll")
                .expect("scroll metrics for diag-shrink-scroll");

            assert!(
                content_size[1] > bounds[3] + 1.0,
                "content must stay taller than viewport (got content_h={} vp_h={}); \
                 flex-shrink into the scrollport falsely disables scrolling",
                content_size[1],
                bounds[3]
            );
            assert!(
                max_scroll[1] > 1.0,
                "max_scroll_y must be > 0, got {}",
                max_scroll[1]
            );
        });
    });
}
