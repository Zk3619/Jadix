//! Scaled logical viewports: responsive sizing, transforms and local portal routing.
//!
//! This module contains framework mechanics, not a rendered component. For a
//! ready-made preview frame, enable `scale-preview` and use `components::ScalePreview`.

#[cfg(feature = "scale-preview")]
use crate::core::tree::Element;
#[cfg(feature = "scale-preview")]
use crate::layout::layout_tree::{
    ATTR_HOISTED_MOUNT, ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT, ATTR_PORTAL_SLOT,
};
#[cfg(feature = "scale-preview")]
use std::collections::HashMap;

#[cfg(feature = "scale-preview")]
pub(crate) const ATTR_SCOPE: &str = "data-jadix-preview-scope";
pub(crate) const ATTR_PORTALS: &str = "data-jadix-preview-portals";
#[cfg(feature = "scale-preview")]
const ATTR_SCOPED: &str = "data-jadix-preview-mount";

/// Configure an existing element as a scaled logical viewport with a local portal host.
///
/// `size` and `scale` must be finite and positive. `mount` must be unique in the
/// element tree. The caller owns positioning and the surrounding layout footprint.
#[cfg(feature = "scale-preview")]
pub fn configure_viewport(element: &mut Element, size: [f32; 2], scale: f32, mount: &str) {
    let frame = Element::new()
        .w(size[0])
        .h(size[1])
        .scale(scale)
        .transform_origin("left top")
        .overflow("hidden");
    element.style.size = frame.style.size;
    element.style.min_size = frame.style.size;
    element.style.max_size = frame.style.size;
    element.style.scale = frame.style.scale;
    element.style.transform_origin = frame.style.transform_origin.clone();
    element.style.overflow = frame.style.overflow;
    element.set_attr("data-jadix-viewport-width", size[0].to_string());
    element.set_attr(ATTR_SCOPE, mount);
    if !element
        .children
        .iter()
        .any(|child| child.template_id.as_deref() == Some(mount))
    {
        element.children.push(
            Element::new()
                .template_id(mount)
                .absolute()
                .inset(0.0)
                .w("100%")
                .h("100%")
                .z_index(1000)
                .pointer_events_box_none(),
        );
    }
    if let Some(host) = element
        .children
        .iter_mut()
        .find(|child| child.template_id.as_deref() == Some(mount))
    {
        host.set_attr(ATTR_PORTALS, "1");
    }
}

#[cfg(feature = "scale-preview")]
pub(crate) fn scope_portals(root: &mut Element) -> bool {
    fn collect(node: &Element, scope: Option<&str>, keys: &mut HashMap<String, Option<String>>) {
        let scope = node
            .attrs
            .get(ATTR_SCOPE)
            .cloned()
            .or_else(|| scope.map(str::to_string));
        if let Some(key) = node.attrs.get(ATTR_PORTAL_SLOT) {
            keys.insert(key.clone(), scope.clone());
        }
        let scope = if node.attrs.contains_key(ATTR_HOISTED_PORTAL) {
            node.attrs
                .get(ATTR_PORTAL_KEY)
                .and_then(|key| keys.get(key))
                .cloned()
                .unwrap_or(scope)
        } else {
            scope
        };
        for child in &node.children {
            collect(child, scope.as_deref(), keys);
        }
    }
    fn apply(
        node: &mut Element,
        scope: Option<&str>,
        keys: &HashMap<String, Option<String>>,
        changed: &mut bool,
    ) {
        let scope = node
            .attrs
            .get(ATTR_SCOPE)
            .cloned()
            .or_else(|| scope.map(str::to_string));
        let fresh = node
            .attrs
            .get("jadix_portal")
            .is_some_and(|value| value == "1");
        let hoisted = node.attrs.contains_key(ATTR_HOISTED_PORTAL);
        let scope = if hoisted {
            node.attrs
                .get(ATTR_PORTAL_KEY)
                .and_then(|key| keys.get(key))
                .cloned()
                .unwrap_or(scope)
        } else {
            scope
        };
        let attr = if fresh {
            ATTR_PORTAL_MOUNT
        } else {
            ATTR_HOISTED_MOUNT
        };
        let managed = node.attrs.contains_key(ATTR_SCOPED);
        let implicit = node.attrs.get(attr).is_none_or(String::is_empty);
        if (fresh || hoisted) && (implicit || managed) {
            if let Some(mount) = scope.as_deref() {
                if node.attrs.get(attr).map(String::as_str) != Some(mount) {
                    node.set_attr(attr, mount);
                    node.style.position_fixed = false;
                    *changed = true;
                }
                node.set_attr(ATTR_SCOPED, mount);
            } else if managed {
                node.attrs.remove(ATTR_SCOPED);
                node.attrs.remove(ATTR_PORTAL_MOUNT);
                node.attrs.remove(ATTR_HOISTED_MOUNT);
                node.style.position_fixed = hoisted;
                *changed = true;
            }
        }
        for child in &mut node.children {
            apply(child, scope.as_deref(), keys, changed);
        }
    }
    let mut keys = HashMap::new();
    collect(root, None, &mut keys);
    let mut changed = false;
    apply(root, None, &keys, &mut changed);
    changed
}
