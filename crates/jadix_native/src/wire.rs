use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use unicode_segmentation::UnicodeSegmentation;

use crate::layout::{MeasureMode, MeasureRequest};
use crate::presentation::{
    AccessibleRole, CommitBatch, EventPayload, ImageLoadEvent, NodeContent, NodeDescriptor, NodeId,
    Operation, PlatformEvent, PopoverPosition, PresentationNode, RootId,
};
use crate::{BackendError, HeadlessRoot};

pub fn descriptor(descriptor: &NodeDescriptor) -> Value {
    let content = match &descriptor.content {
        NodeContent::Hosted { specification } => json!({"kind":"hosted", "specification":serde_json::from_str::<Value>(specification).expect("validated hosted content")}),
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
            max_length,
            value,
            placeholder,
            enabled,
            password,
            revision,
        } => {
            json!({"kind": "input", "text": value, "placeholder": placeholder, "enabled": enabled, "password": password, "maxLength": max_length, "editRevision": revision.to_string()})
        }
        NodeContent::Scroll => json!({"kind": "scroll"}),
        NodeContent::Image { source, alt } => {
            json!({"kind": "image", "source": source, "alt": alt})
        }
        NodeContent::Textarea {
            max_length,
            value,
            placeholder,
            enabled,
            revision,
        } => {
            json!({"kind": "textarea", "text": value, "placeholder": placeholder, "enabled": enabled, "maxLength": max_length, "editRevision": revision.to_string()})
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
    result["interaction"] = json!(descriptor.interaction);
    result["keyboard"] = json!(descriptor.keyboard);
    result["accessibility"] = json!(descriptor.accessibility);
    result["navigationKeys"] = json!(descriptor.navigation_keys);
    result["description"] = json!(descriptor.accessible_description);
    result["role"] = json!(descriptor.accessible_role.map(AccessibleRole::aria_role));
    if descriptor.accessible_role.is_none() { result["role"] = json!(descriptor.accessibility.role); }
    result["selected"] = json!(descriptor.accessible_selected);
    result["tabStop"] = match descriptor.accessible_role {
        Some(AccessibleRole::RadioItem { tab_stop } | AccessibleRole::ListBoxOption { tab_stop }) => json!(tab_stop),
        _ if descriptor.keyboard.focusable => json!(true),
        _ => serde_json::Value::Null,
    };
    result["grid"] = match descriptor.accessible_role {
        Some(AccessibleRole::Grid { rows, columns }) => json!({"rows": rows, "columns": columns}),
        _ => serde_json::Value::Null,
    };
    result["cell"] = match descriptor.accessible_role {
        Some(AccessibleRole::GridCell {
            row,
            column,
            row_span,
            column_span,
        }) => json!({"row": row, "column": column, "rowSpan": row_span, "columnSpan": column_span}),
        _ => serde_json::Value::Null,
    };
    result["columnHeader"] = match descriptor.accessible_role {
        Some(AccessibleRole::GridColumnHeader { column, column_span }) => {
            json!({"column": column, "columnSpan": column_span})
        }
        _ => serde_json::Value::Null,
    };
    result["expanded"] = json!(descriptor.accessible_expanded);
    result["accessibilityRelations"] = json!({
        "activeDescendant": descriptor.accessibility_relations.active_descendant
            .map(|target| json!({"root": target.root.token(), "id": target.token()})),
        "controls": descriptor.accessibility_relations.controls.iter()
            .map(|target| json!({"root": target.root.token(), "id": target.token()})).collect::<Vec<_>>(),
    });
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
    let mut result = json!({"id": request.key.node.token(), "node": descriptor(node), "maxWidth": request.key.max_width, "maxHeight": request.key.max_height,
        "widthMode": mode(request.width_mode), "heightMode": mode(request.height_mode)});
    let wrapping_text = match &node.content {
        NodeContent::Text(text)
            if node.style.text_overflow == crate::presentation::style::TextOverflow::Wrap =>
        {
            Some(text)
        }
        NodeContent::Textarea {
            value, placeholder, ..
        } => Some(if value.is_empty() { placeholder } else { value }),
        _ => None,
    };
    if let Some(text) = wrapping_text {
        let paragraphs: Vec<Vec<&str>> = text
            .split('\n')
            .map(|paragraph| paragraph.graphemes(true).collect())
            .collect();
        result["wrapParagraphs"] = json!(paragraphs);
    }
    result.to_string()
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
    Interaction { event: crate::presentation::interaction::InteractionEvent },
    Key { event: crate::presentation::keyboard::KeyEvent },
    Shortcut { id: String },
    AccessibilityAction { id: String, value: Option<f64> },
    Hosted { event: String, data: serde_json::Value },
    Navigate { key: crate::presentation::NavigationKey, shift: bool, control: bool },
    Activate,
    Hover { inside: bool },
    PointerMove { x: f64, y: f64 },
    ImageLoadStart,
    ImageProgress {
        loaded: u64,
        total: u64,
    },
    ImageLoad {
        width: u32,
        height: u32,
    },
    ImageError {
        message: String,
    },
    ImageLoadEnd,
    Submit {
        revision: String,
    },
    Expansion {
        expanded: bool,
    },
    Dismiss {
        backdrop: bool,
        revision: String,
    },
    PopoverCancel { backdrop: bool },
    PopoverLifecycle { open: bool },
    PopoverPosition { position: Option<PopoverPosition> },
    Toggle {
        checked: bool,
        revision: String,
    },
    Select {
        value: String,
        revision: String,
    },
    Focus,
    FocusVisible,
    Blur,
    Scroll {
        x: f64,
        y: f64,
        #[serde(default)]
        viewport: Option<[f64; 2]>,
    },
    Wheel {
        #[serde(rename = "deltaX")]
        delta_x: f64,
        #[serde(rename = "deltaY")]
        delta_y: f64,
    },
    #[serde(rename = "scrollActivity")]
    ScrollActivity { activity: crate::presentation::scroll::ScrollActivity },
    #[serde(rename = "scrollZoom")]
    ScrollZoom { scale: f64, x: f64, y: f64, width: f64, height: f64 },
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
        let node = NodeId::from_token(model.id(), &self.node)
            .filter(|node| model.node(*node).is_some())
            .ok_or(BackendError::InvalidEvent)?;
        let payload = match self.payload {
            WirePayload::Interaction { event } => EventPayload::Interaction(event),
            WirePayload::Key { event } => EventPayload::Key(event),
            WirePayload::Shortcut { id } => EventPayload::Shortcut { id },
            WirePayload::AccessibilityAction { id, value } => EventPayload::AccessibilityAction { id, value },
            WirePayload::Hover { inside } => EventPayload::Hover { inside },
            WirePayload::PointerMove { x, y } => EventPayload::PointerMove { x, y },
            WirePayload::Navigate { key, shift, control } => EventPayload::Navigate { key, shift, control },
            WirePayload::Hosted { event, data } => EventPayload::Hosted { event, data },
            WirePayload::ImageLoadStart => EventPayload::ImageLoad(ImageLoadEvent::Started),
            WirePayload::ImageProgress { loaded, total } => {
                EventPayload::ImageLoad(ImageLoadEvent::Progress { loaded, total })
            }
            WirePayload::ImageLoad { width, height } => {
                EventPayload::ImageLoad(ImageLoadEvent::Loaded { width, height })
            }
            WirePayload::ImageError { message } => {
                EventPayload::ImageLoad(ImageLoadEvent::Failed { message })
            }
            WirePayload::ImageLoadEnd => EventPayload::ImageLoad(ImageLoadEvent::Ended),
            WirePayload::Submit { revision } => {
                let parsed = revision
                    .parse::<u64>()
                    .map_err(|_| BackendError::InvalidEvent)?;
                if parsed.to_string() != revision {
                    return Err(BackendError::InvalidEvent);
                }
                EventPayload::Submit { revision: parsed }
            }
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
            WirePayload::PopoverCancel { backdrop } => EventPayload::PopoverCancel { backdrop },
            WirePayload::PopoverLifecycle { open } => EventPayload::PopoverLifecycle { open },
            WirePayload::PopoverPosition { position } => EventPayload::PopoverPosition { position },
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
            WirePayload::FocusVisible => EventPayload::FocusVisible,
            WirePayload::Blur => EventPayload::Blur,
            WirePayload::Scroll { x, y, viewport } => match viewport {
                Some([width, height]) => EventPayload::ScrollViewport { x, y, width, height },
                None => EventPayload::Scroll { x, y },
            },
            WirePayload::Wheel { delta_x, delta_y } => EventPayload::Wheel { delta_x, delta_y },
            WirePayload::ScrollActivity { activity } => EventPayload::ScrollActivity(activity),
            WirePayload::ScrollZoom { scale, x, y, width, height } => EventPayload::ScrollZoom { scale, x, y, width, height },
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
