//! Build an [`accesskit::TreeUpdate`] snapshot from the RUI element tree for wiring to `accesskit_winit`.
use accesskit::{Node, NodeId, Role, Toggled, Tree, TreeUpdate};
use std::hash::{Hash, Hasher};

use crate::components::input::{input_value_signal_key, INPUT_FOCUS_PREFIX};
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::components::tabs::TABSTRIP_FOCUS_PREFIX;

const ROOT_ID: NodeId = NodeId(1);

fn node_id_for_focus(fid: &str) -> NodeId {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    fid.hash(&mut h);
    let v = h.finish();
    NodeId(v.max(2))
}

fn push_widget(el: &Element, nodes: &mut Vec<(NodeId, Node)>, root_children: &mut Vec<NodeId>) {
    if let Some(fid) = el.focus_id.as_deref() {
        let id = node_id_for_focus(fid);
        let role = if el.switch_checked_sync.is_some() {
            Role::Switch
        } else if el.checkbox_sync.is_some() {
            Role::CheckBox
        } else if fid.starts_with(INPUT_FOCUS_PREFIX) {
            Role::TextInput
        } else if fid.starts_with(TABSTRIP_FOCUS_PREFIX) {
            Role::TabList
        } else if el.on_click.is_some() {
            Role::Button
        } else {
            Role::GenericContainer
        };
        let mut node = Node::new(role);
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
            if let Some(rest) = fid.strip_prefix(INPUT_FOCUS_PREFIX) {
                let vk = input_value_signal_key(rest);
                let v = use_signal(&vk, String::new).get();
                node.set_value(v);
            }
        }
        nodes.push((id, node));
        root_children.push(id);
    }
    for c in &el.children {
        push_widget(c, nodes, root_children);
    }
}

/// Flat tree: one window root and all focusable widgets as direct children (MVP). Pass current keyboard focus id if any.
pub fn tree_update_from_elements(root: &Element, focused_fid: Option<&str>) -> TreeUpdate {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let mut root_children: Vec<NodeId> = Vec::new();
    push_widget(root, &mut nodes, &mut root_children);

    let mut root_node = Node::new(Role::Window);
    root_node.set_children(root_children);
    nodes.push((ROOT_ID, root_node));

    let focus = focused_fid
        .map(node_id_for_focus)
        .unwrap_or(ROOT_ID);

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        focus,
    }
}
