//! Build an [`accesskit::TreeUpdate`] snapshot from the RUI element tree for wiring to `accesskit_winit`.
use accesskit::{Action, ActionRequest, Node, NodeId, Rect, Role, Toggled, Tree, TreeUpdate};
use std::hash::{Hash, Hasher};

use crate::components::input::{input_value_signal_key, INPUT_FOCUS_PREFIX};
use crate::components::tabs::TABSTRIP_FOCUS_PREFIX;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;

const ROOT_ID: NodeId = NodeId(1);

pub(crate) fn node_id_for_key(key: &str) -> NodeId {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    NodeId(h.finish().max(2))
}

fn node_id_for_focus(fid: &str) -> NodeId {
    node_id_for_key(fid)
}

/// Stable key for nodes without `focus_id`: `template_id` or element path under the app root.
pub(crate) fn stable_node_key(el: &Element, path: &str) -> String {
    if let Some(fid) = el.focus_id.as_deref() {
        fid.to_string()
    } else if let Some(tid) = el.template_id.as_deref() {
        format!("rui:tpl:{tid}")
    } else {
        format!("rui:path:{path}")
    }
}

fn parse_accesskit_role(raw: &str) -> Option<Role> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "button" => Some(Role::Button),
        "checkbox" => Some(Role::CheckBox),
        "switch" => Some(Role::Switch),
        "textinput" | "text_input" | "textfield" => Some(Role::TextInput),
        "label" => Some(Role::Label),
        "group" => Some(Role::Group),
        "tablist" => Some(Role::TabList),
        "link" => Some(Role::Link),
        "list" => Some(Role::List),
        "listitem" | "list_item" => Some(Role::ListItem),
        "image" => Some(Role::Image),
        "paragraph" => Some(Role::Paragraph),
        _ => None,
    }
}

fn should_be_accesskit_node(el: &Element) -> bool {
    el.focus_id.is_some()
        || el.on_click.is_some()
        || el.text.is_some()
        || el.attrs.contains_key("accesskit_role")
        || el.switch_checked_sync.is_some()
        || el.checkbox_sync.is_some()
}

fn infer_role(el: &Element, fid: Option<&str>) -> Role {
    if let Some(raw) = el.attrs.get("accesskit_role") {
        if let Some(role) = parse_accesskit_role(raw) {
            return role;
        }
    }
    if el.switch_checked_sync.is_some() {
        Role::Switch
    } else if el.checkbox_sync.is_some() {
        Role::CheckBox
    } else if fid.is_some_and(|f| f.starts_with(INPUT_FOCUS_PREFIX)) {
        Role::TextInput
    } else if fid.is_some_and(|f| f.starts_with(TABSTRIP_FOCUS_PREFIX)) {
        Role::TabList
    } else if el.on_click.is_some() {
        Role::Button
    } else if el.text.is_some() {
        Role::Label
    } else {
        Role::GenericContainer
    }
}

fn bounds_from_layout(b: [f32; 4]) -> Option<Rect> {
    if b[2] > 0.0 && b[3] > 0.0 {
        Some(Rect {
            x0: f64::from(b[0]),
            y0: f64::from(b[1]),
            x1: f64::from(b[0] + b[2]),
            y1: f64::from(b[1] + b[3]),
        })
    } else {
        None
    }
}

fn build_node(el: &Element, key: &str) -> Node {
    let fid = el.focus_id.as_deref();
    let role = infer_role(el, fid);
    let mut node = Node::new(role);

    if let Some(bounds) = bounds_from_layout(el.layout_bounds) {
        node.set_bounds(bounds);
    }

    if let Some(text) = el.text.as_deref().filter(|t| !t.is_empty()) {
        node.set_label(text.to_string());
    }

    if el.on_click.is_some() {
        node.add_action(Action::Click);
    }
    if el.focusable {
        node.add_action(Action::Focus);
    }

    if let Some(s) = el.switch_checked_sync {
        node.set_toggled(if s.get() {
            Toggled::True
        } else {
            Toggled::False
        });
    } else if let Some(s) = el.checkbox_sync {
        node.set_toggled(if s.get() {
            Toggled::True
        } else {
            Toggled::False
        });
    } else if role == Role::TextInput {
        if let Some(rest) = fid.and_then(|f| f.strip_prefix(INPUT_FOCUS_PREFIX)) {
            let vk = input_value_signal_key(rest);
            let v = use_signal(&vk, String::new).get();
            node.set_value(v);
        }
    }

    let _ = key;
    node
}

fn walk_subtree(
    el: &Element,
    path: &str,
    nodes: &mut Vec<(NodeId, Node)>,
    parent_child_ids: &mut Vec<NodeId>,
) {
    let mut child_ids: Vec<NodeId> = Vec::new();
    for (i, child) in el.children.iter().enumerate() {
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}/{i}")
        };
        walk_subtree(child, &child_path, nodes, &mut child_ids);
    }

    if should_be_accesskit_node(el) {
        let key = stable_node_key(el, path);
        let id = node_id_for_key(&key);
        let mut node = build_node(el, &key);
        node.set_children(child_ids);
        nodes.push((id, node));
        parent_child_ids.push(id);
    } else {
        parent_child_ids.extend(child_ids);
    }
}

/// Hierarchical tree: window root with nested AccessKit nodes mirroring the element tree.
pub fn tree_update_from_elements(root: &Element, focused_fid: Option<&str>) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let mut root_children: Vec<NodeId> = Vec::new();

    for (i, child) in root.children.iter().enumerate() {
        walk_subtree(child, &i.to_string(), &mut nodes, &mut root_children);
    }

    // Root element itself may be an a11y node (unusual but supported).
    if should_be_accesskit_node(root) {
        let key = stable_node_key(root, "root");
        let id = node_id_for_key(&key);
        let mut node = build_node(root, &key);
        let mut merged = vec![id];
        merged.extend(root_children);
        node.set_children(merged);
        nodes.push((id, node));
        root_children = vec![id];
    }

    let mut root_node = Node::new(Role::Window);
    if let Some(bounds) = bounds_from_layout(root.layout_bounds) {
        root_node.set_bounds(bounds);
    }
    root_node.set_children(root_children);
    nodes.push((ROOT_ID, root_node));

    let focus = focused_fid.map(node_id_for_focus).unwrap_or(ROOT_ID);

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        focus,
    }
}

fn find_element_by_node_id<'a>(el: &'a Element, target: NodeId, path: &str) -> Option<&'a Element> {
    if should_be_accesskit_node(el) {
        let key = stable_node_key(el, path);
        if node_id_for_key(&key) == target {
            return Some(el);
        }
    }
    for (i, child) in el.children.iter().enumerate() {
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}/{i}")
        };
        if let Some(found) = find_element_by_node_id(child, target, &child_path) {
            return Some(found);
        }
    }
    None
}

/// Handle an AccessKit action request against the current element tree.
pub fn dispatch_accesskit_action(root: &Element, request: &ActionRequest) -> bool {
    let Some(el) = find_element_by_node_id(root, request.target, "root") else {
        return false;
    };
    match request.action {
        Action::Click => {
            if let Some(cb) = &el.on_click {
                cb.borrow_mut()();
                return true;
            }
        }
        Action::Focus if el.focusable => {
            if let Some(fid) = el.focus_id.as_deref() {
                use_signal("__rui_button_focus_id", String::new).set(fid.to_string());
                return true;
            }
        }
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    fn child_ids_of(nodes: &[(NodeId, Node)], id: NodeId) -> Vec<NodeId> {
        nodes
            .iter()
            .find(|(nid, _)| *nid == id)
            .map(|(_, n)| n.children().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn hierarchical_tree_nests_children() {
        let mut panel = Element::new().template_id("panel");
        panel
            .attrs
            .insert("accesskit_role".to_string(), "group".to_string());
        let btn = Element::new().focus_id("btn1").on_click(|| {});
        panel = panel.child(btn);
        let root = Element::new().child(panel);

        let update = tree_update_from_elements(&root, None);
        let panel_id = node_id_for_key("rui:tpl:panel");
        let btn_id = node_id_for_key("btn1");

        assert!(update.nodes.iter().any(|(id, _)| *id == panel_id));
        assert!(update.nodes.iter().any(|(id, _)| *id == btn_id));
        assert_eq!(child_ids_of(&update.nodes, ROOT_ID), vec![panel_id]);
        assert_eq!(child_ids_of(&update.nodes, panel_id), vec![btn_id]);
    }

    #[test]
    fn dispatch_click_invokes_on_click() {
        use std::cell::Cell;
        use std::rc::Rc;

        let fired = Rc::new(Cell::new(false));
        let fired2 = fired.clone();
        let btn = Element::new().focus_id("act-btn").on_click(move || {
            fired2.set(true);
        });
        let root = Element::new().child(btn);

        let target = node_id_for_key("act-btn");
        let handled = dispatch_accesskit_action(
            &root,
            &ActionRequest {
                action: Action::Click,
                target,
                data: None,
            },
        );
        assert!(handled);
        assert!(fired.get());
    }

    #[test]
    fn text_node_gets_label_role() {
        let label = Element::new().text("Hello");
        let root = Element::new().child(label);
        let update = tree_update_from_elements(&root, None);
        let label_id = node_id_for_key("rui:path:0");
        let node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == label_id)
            .map(|(_, n)| n);
        assert!(node.is_some());
        assert_eq!(node.unwrap().role(), Role::Label);
        assert_eq!(node.unwrap().label(), Some("Hello"));
    }
}
