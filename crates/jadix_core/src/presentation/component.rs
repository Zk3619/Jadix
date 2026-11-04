use super::capabilities::NodeKind;
use crate::schema::Schema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSchema {
    pub name: String,
    pub kind: NodeKind,
    pub properties: Schema,
    pub events: BTreeMap<String, Schema>,
    pub children: bool,
}

impl ComponentSchema {
    pub fn is_valid(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= 128
            && self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && matches!(self.properties, Schema::Object { .. })
            && self.properties.is_valid()
            && self.events.len() <= 32
            && self
                .events
                .iter()
                .all(|(name, schema)| !name.is_empty() && name.len() <= 128 && schema.is_valid())
    }
}
