use crate::{HeadlessRoot, presentation::NodeContent, wire};
use serde_json::{Value, json};

impl HeadlessRoot {
    pub fn inspection(&self, include_text: bool) -> Value {
        let nodes: Vec<_> = self.nodes.iter().map(|(identifier, node)| {
            let mut descriptor = wire::descriptor(&node.descriptor);
            let password = matches!(node.descriptor.content, NodeContent::Input { password: true, .. });
            if !include_text || password {
                for key in ["text", "value", "placeholder", "source"] {
                    if descriptor.get(key).is_some() { descriptor[key] = Value::Null; }
                }
                if descriptor.get("options").is_some() { descriptor["options"] = Value::Null; }
            }
            json!({"id": identifier.token(), "parent": node.parent.map(|parent| parent.token()),
                "children": node.children.iter().map(|child| child.token()).collect::<Vec<_>>(),
                "edit_revision": node.edit_revision.to_string(), "descriptor": descriptor,
                "event_eligible": self.event_visible(*identifier), "focused": self.focus == Some(*identifier)})
        }).collect();
        let usage = self.resource_usage();
        let limits = self.resource_limits();
        json!({"protocol": 1, "root": self.id().token(), "revision": self.revision.to_string(),
            "disposed": self.lifetime.is_disposed(), "roots": self.roots.iter().map(|node| node.token()).collect::<Vec<_>>(), "nodes": nodes,
            "capabilities": {"node_kinds": self.capabilities.node_kinds.iter().map(|kind| format!("{kind:?}")).collect::<Vec<_>>(),
                "text_editing": self.capabilities.text_editing, "focus": self.capabilities.focus, "scrolling": self.capabilities.scrolling},
            "resources": {"nodes": usage.nodes, "retired_nodes": usage.retired_nodes, "retired_ranges": usage.retired_ranges,
                "text_bytes": usage.text_bytes, "image_bytes": usage.image_bytes, "image_pixels": usage.image_pixels},
            "limits": {"nodes": limits.nodes, "retired_nodes": limits.retired_nodes, "retired_ranges": limits.retired_ranges,
                "text_bytes": limits.text_bytes, "image_bytes": limits.image_bytes, "image_pixels": limits.image_pixels},
            "text_requested": include_text, "semantics_authority": "presentation metadata; platform accessibility is inspected separately"})
    }
}
