use super::*;
use crate::primitives::vstack;
use crate::{
    app::{
        portal_overlay::{portal_sync, OverlayKind, OverlayMode},
        window_scope::{self, WindowScope},
    },
    core::reactivity::on_cleanup,
};
use std::{cell::RefCell, collections::HashMap};

struct MountedPresentation {
    controller: MediaPresentationController,
    open: Signal<bool>,
    min_height: Cell<Option<taffy::style::Dimension>>,
    previous_focus: RefCell<String>,
}

thread_local! {
    static PLAYERS: RefCell<HashMap<(WindowScope, String), Rc<MountedPresentation>>> = RefCell::new(HashMap::new());
}

pub(crate) fn register_presentation(id: &str, controller: MediaPresentationController) {
    let key = (window_scope::active(), id.to_string());
    let mounted = Rc::new(MountedPresentation {
        controller,
        open: Signal::new(false),
        min_height: Cell::new(None),
        previous_focus: RefCell::new(String::new()),
    });
    PLAYERS.with(|players| players.borrow_mut().insert(key.clone(), mounted.clone()));
    on_cleanup(move || {
        PLAYERS.with(|players| {
            let mut players = players.borrow_mut();
            if players
                .get(&key)
                .is_some_and(|current| Rc::ptr_eq(current, &mounted))
            {
                players.remove(&key);
            }
        });
        mounted.open.try_set(false);
        crate::utils::deactivate_focus_trap(&format!("media:{}", key.1));
    });
}

fn find_node<'a>(root: &'a mut Element, attr: &str, value: &str) -> Option<&'a mut Element> {
    if root.attrs.get(attr).map(String::as_str) == Some(value) {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_node(child, attr, value))
}

pub(crate) fn sync_media_presentation_subtree(root: &mut Element) -> bool {
    let scope = window_scope::active();
    if !PLAYERS.with(|players| players.borrow().keys().any(|(window, _)| *window == scope)) {
        return false;
    }
    let mut ids = Vec::new();
    crate::core::tree::walk_elements(root, |element| {
        if element.attrs.get("data-slot").map(String::as_str) == Some("media-player")
            && element.attrs.get("data-media-kind").map(String::as_str) == Some("video")
        {
            if let Some(id) = &element.template_id {
                ids.push(id.clone());
            }
        }
    });
    let mut changed = false;
    for id in ids {
        let Some(mounted) = PLAYERS.with(|players| {
            players
                .borrow()
                .get(&(window_scope::active(), id.clone()))
                .cloned()
        }) else {
            continue;
        };
        let panel_id = format!("{id}__presentation");
        let in_panel = find_node(root, "data-media-expanded", &id).is_some();
        if in_panel
            && mounted.open.try_peek() == Some(false)
            && mounted.controller.current().expanded()
        {
            mounted.controller.exit_expanded();
        }
        let expanded = mounted.controller.current().expanded();
        if mounted.open.try_peek() != Some(expanded) {
            mounted.open.try_set(expanded);
        }
        let z = portal_sync(
            &panel_id,
            mounted.open,
            OverlayKind::Modal,
            OverlayMode::Stacked,
        );
        let focus_owner = format!("media:{id}");
        if !expanded && in_panel {
            let panel = find_node(root, "data-media-expanded", &id).unwrap();
            let content = panel
                .children
                .iter_mut()
                .find(|child| child.attrs.contains_key("data-media-retained"))
                .unwrap();
            let children = std::mem::take(&mut content.children);
            let host = find_node(root, "data-media-presentation-host", &id).unwrap();
            host.children = children;
            if let Some(height) = mounted.min_height.take() {
                host.style.min_size.height = height;
            }
            crate::utils::sync_focus_trap(&focus_owner, false, "");
            let previous = mounted.previous_focus.borrow().clone();
            if !previous.is_empty() {
                crate::utils::roving::focus_id_only(previous);
            }
            changed = true;
        } else if expanded && !in_panel {
            let Some(host) = find_node(root, "data-media-presentation-host", &id) else {
                continue;
            };
            *mounted.previous_focus.borrow_mut() =
                crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).peek();
            mounted.min_height.set(Some(host.style.min_size.height));
            host.style.min_size.height = taffy::style::Dimension::length(host.layout_bounds[3]);
            let children = std::mem::take(&mut host.children);
            let mut content: Element = vstack()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .class("w-full min-w-0 min-h-0 gap-3")
                .children(children)
                .into();
            content.set_attr("data-media-retained", "true");
            let controller = mounted.controller.clone();
            let close: Element = button("")
                .variant("ghost")
                .size("sm")
                .child(
                    Element::new()
                        .text(crate::tr_default("media.exit_expanded", "Exit fullscreen")),
                )
                .on_click(move || controller.exit_expanded())
                .into();
            let mut panel: Element = vstack().class("absolute inset-0 w-full h-full min-w-0 min-h-0 gap-2 bg-card p-3 text-card-foreground overflow-hidden")
                .child(close.aria_label(crate::tr_default("media.exit_expanded", "Exit fullscreen")))
                .child(content).into();
            panel.template_id = Some(panel_id.clone());
            panel.set_attr("data-media-expanded", &id);
            panel.set_attr("jadix_portal", "1");
            panel.set_attr("jadix_portal_key", &panel_id);
            panel.set_attr("role", "dialog");
            panel.set_attr("aria-modal", "true");
            panel = panel.aria_label(crate::tr_default("media.video", "Video"));
            panel.style.z_index = z.map_or(1001, |z| z.panel);
            host.children.push(panel);
            changed = true;
        }
        if expanded {
            if let Some(panel) = find_node(root, "data-media-expanded", &id) {
                if let Some(z) = z {
                    panel.style.z_index = z.panel;
                }
                let mut focus_ids = Vec::new();
                crate::core::tree::walk_elements(panel, |element| {
                    if element.focusable {
                        if let Some(id) = &element.focus_id {
                            focus_ids.push(id.clone());
                        }
                    }
                });
                let ring = focus_ids.join(",");
                if panel.attrs.get("data-media-focus-ring") != Some(&ring) {
                    let initial = !panel.attrs.contains_key("data-media-focus-ring");
                    crate::utils::sync_focus_trap(&focus_owner, true, &ring);
                    panel.set_attr("data-media-focus-ring", ring);
                    if initial {
                        if let Some(first) = focus_ids.first() {
                            crate::utils::roving::focus_id_only(first.clone());
                        }
                    }
                }
            }
        }
    }
    changed
}
