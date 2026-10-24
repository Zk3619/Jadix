//! Logical viewport previews, independent of DevTools and available in release builds.
//!
//! ```ignore
//! scale_preview(390.0, 844.0, 0.5).child(page).into()
//! ui! { <ScalePreview viewport_width=390.0 viewport_height=844.0 scale=0.5>{page}</ScalePreview> }
//! ```

#![allow(non_snake_case)]

use crate::core::scaled_viewport::configure_viewport;
use crate::core::tree::Element;
use jadix_macros::component;
use std::sync::atomic::{AtomicUsize, Ordering};

static PREVIEW_SEQ: AtomicUsize = AtomicUsize::new(0);

fn positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

/// Fixed logical viewport with scaled layout footprint and local portals.
pub struct ScalePreviewBuilder {
    size: [f32; 2],
    scale: f32,
    children: Vec<Element>,
}

/// Preview a logical viewport at `scale` (1%-800%) without scaling surrounding UI.
/// Popovers, responsive classes and pointer hit testing use the logical viewport.
pub fn scale_preview(viewport_width: f32, viewport_height: f32, scale: f32) -> ScalePreviewBuilder {
    ScalePreviewBuilder {
        size: [
            positive(viewport_width, 390.0),
            positive(viewport_height, 844.0),
        ],
        scale: positive(scale, 1.0).clamp(0.01, 8.0),
        children: Vec::new(),
    }
}

impl ScalePreviewBuilder {
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }
}

impl From<ScalePreviewBuilder> for Element {
    fn from(builder: ScalePreviewBuilder) -> Self {
        let mount = format!(
            "__jadix_preview_{}_portals",
            PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let mut viewport = Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .children(builder.children);
        configure_viewport(&mut viewport, builder.size, builder.scale, &mount);
        let mut frame = Element::new()
            .relative()
            .w(builder.size[0] * builder.scale)
            .h(builder.size[1] * builder.scale)
            .flex_shrink(0.0)
            .child(viewport);
        frame.set_attr("data-slot", "scale-preview");
        frame
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ScalePreview(
    viewport_width: f32,
    viewport_height: f32,
    scale: Option<f32>,
    children: Option<Element>,
) -> Element {
    scale_preview(viewport_width, viewport_height, scale.unwrap_or(1.0))
        .children(children.map(crate::utils::take_packed).unwrap_or_default())
        .into()
}

#[cfg(test)]
mod tests {
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
                assert!(crate::app::tree_hit::find_click(
                    &root,
                    130.0 * scale,
                    80.0 * scale,
                    [0.0; 2]
                )
                .is_some());
                assert!(crate::app::tree_hit::find_click(
                    &root,
                    190.0 * scale,
                    80.0 * scale,
                    [0.0; 2]
                )
                .is_none());
            }
        });
    }
}
