//! Stylesheet intermediate representation (utility distributor — not a CSSOM).

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::style::ThemeOverrides;

/// Loaded stylesheet: optional theme patch + utility rules.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stylesheet {
    pub theme: Option<ThemeOverrides>,
    pub rules: Vec<StyleRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    /// Space-separated utilities, handed to [`crate::style::apply_class_list`].
    pub classes: String,
    /// Higher wins later in the merge; default 0. Later registration overrides earlier
    /// at the same priority.
    #[serde(default)]
    pub priority: u16,
}

/// Supported selectors. Combinators (` `, `>`, `+`, `~`), `:nth-child`, `:not()`,
/// attribute wildcards, `@media`, and `!important` are rejected at parse time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Selector {
    DataSlot(String),
    DataEq { key: String, value: String },
    Class(String),
    Id(String),
    And(Vec<Selector>),
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataSlot(s) => write!(f, "[data-slot={s}]"),
            Self::DataEq { key, value } => write!(f, "[{key}={value}]"),
            Self::Class(c) => write!(f, ".{c}"),
            Self::Id(id) => write!(f, "#{id}"),
            Self::And(parts) => {
                for p in parts {
                    write!(f, "{p}")?;
                }
                Ok(())
            }
        }
    }
}

/// A rule that matched an element (DevTools / diagnostics).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchedStyleRule {
    pub selector: String,
    pub classes: String,
    pub priority: u16,
    pub source: Option<PathBuf>,
}
