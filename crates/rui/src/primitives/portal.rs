//! # Portal
//!
//! SolidJS-aligned portal: render children outside the source parent, either on the
//! **root overlay layer** (like `document.body`) or into a **named host** (`mount`).
//!
//! Layout each frame lifts `rui_portal=1` nodes via
//! [`crate::layout::layout_tree::hoist_portal_subtrees`]. After the move, a zero-size
//! `rui_portal_slot` marker stays in the source tree so the portal remains “wanted”
//! across frames (Solid keep-alive) — overlays must not be destroyed and recreated every
//! paint. Select / Dialog / Popover / Dropdown mark panels with
//! [`crate::utils::popup::portal_panel`]; this primitive is for app-authored overlays.
//!
//! ## Solid semantics (required)
//! - Default mount = root [`PORTAL_LAYER_ID`](crate::layout::layout_tree::PORTAL_LAYER_ID)
//!   (`document.body`): content is **outside cropping / scrolled ancestors**.
//! - Paint + hit-test clear inherited overflow clip and scroll accumulation on that layer.
//! - Body-mounted nodes are viewport-`fixed`; local `mount=` hosts keep host-relative absolute.
//! - Missing `mount` target falls back to the root layer (same as Solid falling back to body).
//!
//! ## 用法
//! ```ignore
//! // Default: root portal layer (Solid `<Portal>` → document.body)
//! <Portal id="overlay-root">
//!   <Show when={open.get()}>
//!     <Rect absolute=true inset="0" z_index="900">…</Rect>
//!   </Show>
//! </Portal>
//!
//! // Local: mount into a host by template_id (Solid `<Portal mount={el}>`)
//! <Rect id="card-host" relative=true w="100%" h="12rem">…</Rect>
//! <Portal id="card-overlay" mount="card-host">
//!   <Rect absolute=true inset="0" bg="black">…</Rect>
//! </Portal>
//! ```
//!
//! ## 注意
//! - Host lookup uses [`Element::template_id`]. Box/Rect/VStack `id` sets template_id.
//! - The Portal root uses [`pointer_events_box_none`](crate::core::tree::Element::pointer_events_box_none)
//!   so an empty wrapper does not steal scroll/clicks; interactive children stay hittable.
//! - Esc / z-index stacks: [`crate::app::portal_overlay`].
//! - Positioning is the child's job — same as Solid, where `<Portal>` only relocates.
//! - Dismissal for menus/popovers follows ShadCN/Radix “pointer outside” (stable portal key),
//!   not a one-frame flash.

#![allow(non_snake_case)]

use crate::core::tree::Element;
use crate::layout::layout_tree::{ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT};
use rui_macros::component;

/// Portal：标记 `rui_portal`，布局时提升到根叠层或 `mount` 宿主（对齐 Solid `<Portal>`）。
#[component(crate_path = "crate")]
pub fn Portal(id: String, mount: Option<String>, children: Option<Element>) -> Element {
    let mut inner = children.unwrap_or_else(Element::new);
    inner.template_id = Some(id.clone());
    inner.set_attr("rui_portal", "1");
    inner.set_attr(ATTR_PORTAL_KEY, id.as_str());
    if let Some(m) = mount.filter(|s| !s.trim().is_empty()) {
        inner.set_attr(ATTR_PORTAL_MOUNT, m.trim());
    }
    // Wrapper is a stacking shell (box-none); underlays / panels remain hittable as children.
    // Pre-hoist safety: absolute so it never flex-splits the source parent.
    inner
        .pointer_events_box_none()
        .absolute()
        .inset(0.0)
        .w("100%")
        .h("100%")
}
