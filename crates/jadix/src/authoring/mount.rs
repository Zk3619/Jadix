use crate::core::tree::Element;

/// Render nested reactive views inline without taking over their live mount slots.
/// The previous mode is restored when the closure returns or unwinds.
pub fn with_inline_mount<Result>(body: impl FnOnce() -> Result) -> Result {
    let _guard = crate::app::reactive::InlineIslandGuard::enter();
    body()
}

pub fn is_reactive_placeholder(element: &Element) -> bool {
    element.template_hash.as_deref() == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH)
}

/// Replace a portal body while preserving its mount identity and paint synchronizer.
/// Call during the host's tree synchronization phase, not from a reactive callback.
pub fn replace_portal_content(host: &mut Element, replacement: Element) {
    use crate::layout::layout_tree::{
        ATTR_HOISTED_MOUNT, ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT,
    };
    let synchronizer = host.reactive_sync.take();
    let z_index = host.style.z_index;
    let attributes = [
        ATTR_PORTAL_KEY,
        "jadix_portal",
        ATTR_PORTAL_MOUNT,
        ATTR_HOISTED_PORTAL,
        ATTR_HOISTED_MOUNT,
    ]
    .map(|key| (key, host.attrs.get(key).cloned()));
    *host = replacement;
    host.reactive_sync = synchronizer;
    if z_index != 0 {
        host.style.z_index = z_index;
    }
    for (key, value) in attributes {
        if let Some(value) = value {
            host.set_attr(key, value);
        }
    }
}

/// Pointer-space collision bounds using the host's transform and scroll rules.
pub fn collision_bounds(element: &Element, scroll_offset: [f32; 2]) -> [f32; 4] {
    crate::app::tree_hit::collision_hit_box(element, scroll_offset)
}

pub fn child_scroll_offset(element: &Element, scroll_offset: [f32; 2]) -> [f32; 2] {
    crate::app::tree_hit::next_hit_scroll_acc(element, scroll_offset)
}

/// Invalidate the changed node and its ancestors in the retained collect cache.
pub fn invalidate_collect_paint(root: &mut Element, path: &[usize]) {
    crate::layout::collect_cache::invalidate_collect_paint_hash_path(root, path);
}

/// Effective scale from the last collect, including ancestor transforms.
pub fn painted_scale(element: &Element) -> [f32; 2] {
    crate::core::tree::paint_scale_with_ancestor(element, element.paint_ancestor_scale)
}

/// Damage the last collected visual bounds without exposing retained-paint fields.
pub fn invalidate_paint(element: &Element) {
    crate::app::damage::push_frame_paint_element(element, element.paint_scroll_acc);
}

/// Pin a followed overlay at its layout origin while collect owns the visual offset.
pub fn pin_overlay_origin(element: &mut Element, origin: [f32; 2]) {
    crate::utils::anchored_overlay::pin_overlay_follow_insets(element, origin);
}
