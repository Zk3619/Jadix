use crate::{handles::Handle, schema::Value};
use jadix_core::presentation::{component::ComponentSchema, style::NodeStyle, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    InvalidJson,
    Limit,
    Version,
    Component,
    Properties,
    Events,
    Children,
    Style,
}

#[derive(Default)]
pub struct ComponentRegistry {
    components: BTreeMap<String, ComponentSchema>,
}

impl ComponentRegistry {
    pub fn get(&self, name: &str) -> Option<&ComponentSchema> {
        self.components.get(name)
    }
    pub fn register(&mut self, schema: ComponentSchema) -> Result<(), WireError> {
        if !schema.is_valid()
            || self.components.contains_key(&schema.name)
            || self.components.len() >= 256
        {
            return Err(WireError::Component);
        }
        self.components.insert(schema.name.clone(), schema);
        Ok(())
    }

    pub fn validate(&self, root: &ElementSpec) -> Result<(), WireError> {
        let mut pending = vec![(root, 0)];
        let mut visited = 0;
        while let Some((node, depth)) = pending.pop() {
            visited += 1;
            if visited > 4096 || depth > 32 || pending.len() > 4096 {
                return Err(WireError::Limit);
            }
            let schema = self
                .components
                .get(&node.component)
                .ok_or(WireError::Component)?;
            if !schema.properties.accepts(&node.props) {
                return Err(WireError::Properties);
            }
            if !node.style.is_valid() {
                return Err(WireError::Style);
            }
            if node
                .events
                .keys()
                .any(|name| !schema.events.contains_key(name))
            {
                return Err(WireError::Events);
            }
            if !schema.children && !node.children.is_empty() {
                return Err(WireError::Children);
            }
            let mut keys = std::collections::BTreeSet::new();
            if node.key.as_ref().is_some_and(|key| key.is_empty() || key.len() > 256)
                || node.children.iter().filter_map(|child| child.key.as_ref()).any(|key| !keys.insert(key))
            {
                return Err(WireError::Properties);
            }
            pending.extend(node.children.iter().map(|child| (child, depth + 1)));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementSpec {
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub props: Value,
    #[serde(default)]
    pub style: NodeStyle,
    #[serde(default)]
    pub events: BTreeMap<String, Handle>,
    #[serde(default)]
    pub children: Vec<ElementSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Mount {
        root: String,
        tree: ElementSpec,
    },
    WriteState {
        handle: Handle,
        value: Value,
    },
    Event {
        handle: Handle,
        arguments: Value,
    },
    Call {
        request: String,
        method: String,
        arguments: Value,
    },
    Cancel {
        request: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub protocol: u32,
    pub session: String,
    pub command: Command,
}

impl Envelope {
    pub fn decode(bytes: &[u8], registry: &ComponentRegistry) -> Result<Self, WireError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Limit);
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| WireError::InvalidJson)?;
        if !value.is_valid() {
            return Err(WireError::Limit);
        }
        let envelope: Self = serde_json::from_slice(bytes).map_err(|_| WireError::InvalidJson)?;
        envelope.validate(registry)?;
        Ok(envelope)
    }

    pub fn encode(&self, registry: &ComponentRegistry) -> Result<Vec<u8>, WireError> {
        self.validate(registry)?;
        let bytes = serde_json::to_vec(self).map_err(|_| WireError::InvalidJson)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Limit);
        }
        Ok(bytes)
    }

    fn validate(&self, registry: &ComponentRegistry) -> Result<(), WireError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(WireError::Version);
        }
        if !token(&self.session) {
            return Err(WireError::Properties);
        }
        match &self.command {
            Command::Mount { root, tree } => {
                if !token(root) {
                    return Err(WireError::Properties);
                }
                registry.validate(tree)?;
                let mut pending = vec![tree];
                while let Some(node) = pending.pop() {
                    if node
                        .events
                        .values()
                        .any(|handle| handle.session != self.session || !token(&handle.sequence))
                    {
                        return Err(WireError::Events);
                    }
                    pending.extend(node.children.iter());
                }
                Ok(())
            }
            Command::WriteState { handle, value }
            | Command::Event {
                handle,
                arguments: value,
            } => {
                if handle.session != self.session || !token(&handle.sequence) || !value.is_valid() {
                    Err(WireError::Properties)
                } else {
                    Ok(())
                }
            }
            Command::Call {
                request,
                method,
                arguments,
            } => {
                if !token(request)
                    || method.is_empty()
                    || method.len() > 128
                    || !arguments.is_valid()
                {
                    Err(WireError::Properties)
                } else {
                    Ok(())
                }
            }
            Command::Cancel { request } => {
                if token(request) {
                    Ok(())
                } else {
                    Err(WireError::Properties)
                }
            }
        }
    }
}

fn token(value: &str) -> bool {
    value
        .parse::<u64>()
        .is_ok_and(|parsed| parsed > 0 && parsed.to_string() == value)
}
