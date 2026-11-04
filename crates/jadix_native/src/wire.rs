use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::layout::{MeasureMode, MeasureRequest};
use crate::presentation::{
    AccessibleRole, CommitBatch, EventPayload, NodeContent, NodeDescriptor, NodeId, Operation,
    PlatformEvent, PresentationNode, RootId,
};
use crate::{BackendError, HeadlessRoot};

pub fn descriptor(descriptor: &NodeDescriptor) -> Value {
    let content = match &descriptor.content {
        NodeContent::Select {
            options,
            value,
            placeholder,
            enabled,
            revision,
        } => {
            let options: Vec<_> = options.iter().map(|option| json!({"value": option.value, "label": option.label, "enabled": option.enabled})).collect();
            json!({"kind": "select", "options": options, "value": value, "placeholder": placeholder, "enabled": enabled, "editRevision": revision.to_string()})
        }
        NodeContent::Container => json!({"kind": "box"}),
        NodeContent::Text(text) => json!({"kind": "text", "text": text}),
        NodeContent::Button { label, enabled } => {
            json!({"kind": "button", "text": label, "enabled": enabled})
        }
        NodeContent::Input {
            value,
            placeholder,
            enabled,
            password,
            revision,
        } => {
            json!({"kind": "input", "text": value, "placeholder": placeholder, "enabled": enabled, "password": password, "editRevision": revision.to_string()})
        }
        NodeContent::Scroll => json!({"kind": "scroll"}),
        NodeContent::Image { source, alt } => {
            json!({"kind": "image", "source": source, "alt": alt})
        }
        NodeContent::Textarea {
            value,
            placeholder,
            enabled,
            revision,
        } => {
            json!({"kind": "textarea", "text": value, "placeholder": placeholder, "enabled": enabled, "editRevision": revision.to_string()})
        }
        NodeContent::Switch {
            label,
            checked,
            enabled,
            revision,
        } => {
            json!({"kind": "switch", "text": label, "checked": checked, "enabled": enabled, "editRevision": revision.to_string()})
        }
        NodeContent::Dialog {
            open,
            close_on_overlay_click,
            revision,
        } => {
            json!({"kind": "dialog", "open": open, "closeOnOverlayClick": close_on_overlay_click, "editRevision": revision.to_string()})
        }
        NodeContent::Popover {
            anchor,
            placement,
            gap,
            open,
            close_on_outside_click,
            revision,
        } => {
            json!({"kind": "popover", "anchor": anchor.token(), "placement": placement, "gap": gap, "open": open, "closeOnOutsideClick": close_on_outside_click, "editRevision": revision.to_string()})
        }
        NodeContent::Checkbox {
            label,
            checked,
            enabled,
            revision,
        } => {
            json!({"kind": "checkbox", "text": label, "checked": checked, "enabled": enabled, "editRevision": revision.to_string()})
        }
    };
    let mut result = content;
    let frame = descriptor.frame;
    result["frame"] = json!([frame.x, frame.y, frame.width, frame.height]);
    result["style"] = serde_json::to_value(&descriptor.style).expect("validated node style");
    result["name"] = json!(descriptor.accessible_name);
    result["role"] = json!(match descriptor.accessible_role {
        Some(AccessibleRole::RadioGroup) => Some("radiogroup"),
        Some(AccessibleRole::RadioItem { .. }) => Some("radio"),
        Some(AccessibleRole::TabList) => Some("tablist"),
        Some(AccessibleRole::TabItem) => Some("tab"),
        Some(AccessibleRole::TabPanel) => Some("tabpanel"),
        Some(AccessibleRole::Tree) => Some("tree"),
        Some(AccessibleRole::TreeItem) => Some("treeitem"),
        Some(AccessibleRole::Grid { .. }) => Some("grid"),
        Some(AccessibleRole::GridCell { .. }) => Some("gridcell"),
        Some(AccessibleRole::GridColumnHeader { .. }) => Some("columnheader"),
        None => None,
    });
    result["selected"] = json!(descriptor.accessible_selected);
    result["tabStop"] = match descriptor.accessible_role {
        Some(AccessibleRole::RadioItem { tab_stop }) => json!(tab_stop),
        _ => serde_json::Value::Null,
    };
    result["grid"] = match descriptor.accessible_role {
        Some(AccessibleRole::Grid { rows, columns }) => json!({"rows": rows, "columns": columns}),
        _ => serde_json::Value::Null,
    };
    result["cell"] = match descriptor.accessible_role {
        Some(AccessibleRole::GridCell { row, column }) => json!({"row": row, "column": column}),
        _ => serde_json::Value::Null,
    };
    result["columnHeader"] = match descriptor.accessible_role {
        Some(AccessibleRole::GridColumnHeader { column }) => json!({"column": column}),
        _ => serde_json::Value::Null,
    };
    result["expanded"] = json!(descriptor.accessible_expanded);
    result
}

pub fn measurement(request: MeasureRequest, node: &NodeDescriptor) -> String {
    fn mode(mode: MeasureMode) -> &'static str {
        match mode {
            MeasureMode::Definite => "exact",
            MeasureMode::MinContent => "min",
            MeasureMode::MaxContent => "max",
        }
    }
    json!({"node": descriptor(node), "maxWidth": request.key.max_width, "maxHeight": request.key.max_height,
        "widthMode": mode(request.width_mode), "heightMode": mode(request.height_mode)}).to_string()
}

pub fn batch(batch: &CommitBatch) -> String {
    let operations: Vec<_> = batch.operations.iter().map(|operation| match operation {
        Operation::Create { node, parent, index, descriptor: content } => json!({"op": "create", "id": node.token(), "parent": parent.map(NodeId::token), "index": index, "descriptor": descriptor(content)}),
        Operation::Update { node, descriptor: content } => json!({"op": "update", "id": node.token(), "descriptor": descriptor(content)}),
        Operation::Move { node, parent, index } => json!({"op": "move", "id": node.token(), "parent": parent.map(NodeId::token), "index": index}),
        Operation::Remove { node } => json!({"op": "remove", "id": node.token()}),
        Operation::Focus { node } => json!({"op": "focus", "id": node.map(NodeId::token)}),
    }).collect();
    json!({"protocol": 2, "root": batch.root.token(), "previous": batch.previous.to_string(), "revision": batch.revision.to_string(), "operations": operations}).to_string()
}

pub fn snapshot(root: RootId, revision: u64, roots: &[PresentationNode]) -> String {
    fn append(output: &mut Vec<Value>, nodes: &[PresentationNode], parent: Option<NodeId>) {
        for node in nodes {
            let mut value = descriptor(&node.descriptor);
            value["id"] = json!(node.id.token());
            value["parent"] = json!(parent.map(NodeId::token));
            output.push(value);
            append(output, &node.children, Some(node.id));
        }
    }
    let mut nodes = Vec::new();
    append(&mut nodes, roots, None);
    json!({"protocol": 1, "root": root.token(), "revision": revision.to_string(), "nodes": nodes})
        .to_string()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireEvent {
    pub root: String,
    pub node: String,
    pub payload: WirePayload,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WirePayload {
    Activate,
    Expansion {
        expanded: bool,
    },
    Dismiss {
        backdrop: bool,
        revision: String,
    },
    Toggle {
        checked: bool,
        revision: String,
    },
    Select {
        value: String,
        revision: String,
    },
    Focus,
    Scroll {
        x: f64,
        y: f64,
    },
    Edit {
        value: String,
        revision: String,
        selection: [usize; 2],
        composing: Option<[usize; 2]>,
    },
}

impl WireEvent {
    pub fn resolve(self, model: &HeadlessRoot) -> Result<PlatformEvent, BackendError> {
        if model.lifetime().is_disposed() {
            return Err(BackendError::Disposed);
        }
        if self.root != model.id().token() {
            return Err(BackendError::WrongRoot);
        }
        let mut pending = model.roots().to_vec();
        let mut found = None;
        while let Some(id) = pending.pop() {
            if id.token() == self.node {
                found = Some(id);
                break;
            }
            if let Some(node) = model.node(id) {
                pending.extend(&node.children);
            }
        }
        let node = found.ok_or(BackendError::InvalidEvent)?;
        let payload = match self.payload {
            WirePayload::Select { value, revision } => {
                let parsed = revision
                    .parse::<u64>()
                    .map_err(|_| BackendError::InvalidEvent)?;
                if parsed.to_string() != revision {
                    return Err(BackendError::InvalidEvent);
                }
                EventPayload::Select {
                    value,
                    revision: parsed,
                }
            }
            WirePayload::Activate => EventPayload::Activate,
            WirePayload::Expansion { expanded } => EventPayload::Expansion { expanded },
            WirePayload::Dismiss { backdrop, revision } => {
                let parsed = revision
                    .parse::<u64>()
                    .map_err(|_| BackendError::InvalidEvent)?;
                if parsed.to_string() != revision {
                    return Err(BackendError::InvalidEvent);
                }
                EventPayload::Dismiss {
                    backdrop,
                    revision: parsed,
                }
            }
            WirePayload::Toggle { checked, revision } => {
                let parsed = revision
                    .parse::<u64>()
                    .map_err(|_| BackendError::InvalidEvent)?;
                if parsed.to_string() != revision {
                    return Err(BackendError::InvalidEvent);
                }
                EventPayload::Toggle {
                    checked,
                    revision: parsed,
                }
            }
            WirePayload::Focus => EventPayload::Focus,
            WirePayload::Scroll { x, y } => EventPayload::Scroll { x, y },
            WirePayload::Edit {
                value,
                revision,
                selection,
                composing,
            } => {
                let revision_number = revision
                    .parse::<u64>()
                    .map_err(|_| BackendError::InvalidEvent)?;
                if revision_number.to_string() != revision {
                    return Err(BackendError::InvalidEvent);
                }
                EventPayload::Edit {
                    value,
                    revision: revision_number,
                    selection_utf16: selection,
                    composing_utf16: composing,
                }
            }
        };
        Ok(PlatformEvent { node, payload })
    }
}

pub const MAX_EVENT_BYTES: usize = 1024 * 1024;

pub fn decode_event(json: &str, model: &HeadlessRoot) -> Result<PlatformEvent, BackendError> {
    if json.len() > MAX_EVENT_BYTES {
        return Err(BackendError::InvalidEvent);
    }
    let event: WireEvent = serde_json::from_str(json).map_err(|_| BackendError::InvalidEvent)?;
    event.resolve(model)
}
