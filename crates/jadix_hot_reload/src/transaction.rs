use crate::TemplateFragment;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeChange {
    pub instance: u64,
    pub before: Option<Vec<usize>>,
    pub after: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub transaction_ack: u32,
    pub static_tree: u32,
    pub declaration_reset: u32,
    pub max_frame_bytes: usize,
    pub queue_capacity: usize,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            transaction_ack: 1,
            static_tree: 1,
            declaration_reset: 1,
            max_frame_bytes: 4 * 1024 * 1024,
            queue_capacity: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateDescription {
    pub schema: u32,
    pub signature: String,
    pub declarations_signature: String,
    #[serde(default)]
    pub tree: Option<crate::TemplateNode>,
}

impl TemplateDescription {
    pub fn parse_tree(source: Option<&str>) -> Option<crate::TemplateNode> {
        source.and_then(|source| serde_json::from_str(source).ok())
    }

    pub fn new(signature: impl Into<String>, declarations_signature: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            signature: signature.into(),
            declarations_signature: declarations_signature.into(),
            tree: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub protocol: u32,
    pub schema: u32,
    pub build: String,
    pub revision: u64,
    #[serde(default)]
    pub capabilities: Capabilities,
}

impl SessionInfo {
    pub fn new(build: String) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            schema: SCHEMA_VERSION,
            build,
            revision: 0,
            capabilities: Capabilities::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub protocol: u32,
    pub schema: u32,
    pub build: String,
    pub base_revision: u64,
    pub revision: u64,
    #[serde(default)]
    pub templates: Vec<TemplateFragment>,
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(default)]
    pub stylesheets: Vec<String>,
}

impl Transaction {
    pub fn new(session: &SessionInfo) -> Option<Self> {
        Some(Self {
            protocol: session.protocol,
            schema: session.schema,
            build: session.build.clone(),
            base_revision: session.revision,
            revision: session.revision.checked_add(1)?,
            templates: Vec::new(),
            assets: Vec::new(),
            stylesheets: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitStatus {
    Applied,
    Deferred,
    Unchanged,
    NeedsRebuild,
    OutOfSync,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitAck {
    pub build: String,
    pub revision: u64,
    pub status: CommitStatus,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<NodeChange>,
}

impl CommitAck {
    pub fn committed(&self) -> bool {
        matches!(
            self.status,
            CommitStatus::Applied | CommitStatus::Deferred | CommitStatus::Unchanged
        )
    }
}

pub struct SessionGate {
    session: SessionInfo,
    last: Option<(Transaction, CommitAck)>,
}

impl SessionGate {
    pub fn new(session: SessionInfo) -> Self {
        Self {
            session,
            last: None,
        }
    }
    pub fn info(&self) -> &SessionInfo {
        &self.session
    }

    pub fn apply(
        &mut self,
        transaction: &Transaction,
        commit: impl FnOnce(&Transaction) -> Result<CommitStatus, String>,
    ) -> CommitAck {
        self.apply_with_changes(transaction, |transaction| {
            commit(transaction).map(|status| (status, Vec::new()))
        })
    }

    pub fn apply_with_changes(
        &mut self,
        transaction: &Transaction,
        commit: impl FnOnce(&Transaction) -> Result<(CommitStatus, Vec<NodeChange>), String>,
    ) -> CommitAck {
        let rejected = |status, reason: &str| CommitAck {
            build: self.session.build.clone(),
            revision: self.session.revision,
            status,
            reason: reason.into(),
            changes: Vec::new(),
        };
        if transaction.protocol != PROTOCOL_VERSION || transaction.schema != SCHEMA_VERSION {
            return rejected(CommitStatus::Rejected, "protocol or schema mismatch");
        }
        if transaction.build != self.session.build {
            return rejected(CommitStatus::NeedsRebuild, "different build session");
        }
        if let Some((previous, acknowledgement)) = &self.last {
            if previous == transaction {
                return acknowledgement.clone();
            }
        }
        if transaction.base_revision != self.session.revision
            || self.session.revision.checked_add(1) != Some(transaction.revision)
        {
            return rejected(
                CommitStatus::OutOfSync,
                "request a current session snapshot",
            );
        }
        let mut ids = std::collections::HashSet::new();
        if transaction
            .templates
            .iter()
            .any(|template| template.id.is_empty() || !ids.insert(&template.id))
        {
            return rejected(
                CommitStatus::Rejected,
                "empty or duplicate template identity",
            );
        }
        if transaction
            .templates
            .iter()
            .any(|template| template.signature.is_empty())
        {
            return rejected(
                CommitStatus::Rejected,
                "template requires a compiled capability signature",
            );
        }
        let (status, changes) = match commit(transaction) {
            Ok((
                status @ (CommitStatus::Applied | CommitStatus::Deferred | CommitStatus::Unchanged),
                changes,
            )) => (status, changes),
            Ok(_) => return rejected(CommitStatus::Rejected, "invalid commit status"),
            Err(reason) => {
                return CommitAck {
                    reason,
                    ..rejected(CommitStatus::NeedsRebuild, "")
                }
            }
        };
        self.session.revision = transaction.revision;
        let acknowledgement = CommitAck {
            build: self.session.build.clone(),
            revision: self.session.revision,
            status,
            reason: String::new(),
            changes,
        };
        self.last = Some((transaction.clone(), acknowledgement.clone()));
        acknowledgement
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeImpact {
    Paint,
    Layout,
}

pub fn attribute_impact(name: &str) -> Option<AttributeImpact> {
    use AttributeImpact::*;
    match name {
        "bg"
        | "color"
        | "rounded"
        | "radius"
        | "text_decoration"
        | "text_decoration_line"
        | "text_decoration_color"
        | "text_decoration_style" => Some(Paint),
        "class"
        | "text_size"
        | "font_size"
        | "font_weight"
        | "text_weight"
        | "p"
        | "padding"
        | "m"
        | "margin"
        | "w"
        | "width"
        | "h"
        | "height"
        | "gap"
        | "animation"
        | "animation_name"
        | "animation_duration"
        | "animation_delay"
        | "animation_timing_function"
        | "animation_iteration_count"
        | "animation_direction"
        | "animation_fill_mode"
        | "animation_play_state"
        | "transition"
        | "transition_property"
        | "transition_duration"
        | "transition_delay"
        | "transition_timing_function" => Some(Layout),
        _ => None,
    }
}

pub fn is_live_attribute(name: &str) -> bool {
    attribute_impact(name).is_some()
}

#[cfg(test)]
#[path = "tests/transaction.rs"]
mod tests;
