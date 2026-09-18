//! Shared types for the morphicons core (DOM-free).

use std::collections::HashMap;

/// Lucide-style icon data: a `[tag, attrs]` list.
pub type IconNode = (String, HashMap<String, String>);

/// Input the pipeline can lower: a raw `d` or a node list.
#[derive(Clone, Debug)]
pub enum IconInput {
    D(String),
    Nodes(Vec<IconNode>),
}

impl From<&str> for IconInput {
    fn from(s: &str) -> Self {
        IconInput::D(s.to_string())
    }
}

impl From<String> for IconInput {
    fn from(s: String) -> Self {
        IconInput::D(s)
    }
}

impl From<Vec<IconNode>> for IconInput {
    fn from(nodes: Vec<IconNode>) -> Self {
        IconInput::Nodes(nodes)
    }
}

/// Cubic subpath: packed `[p0, c1, c2, p1, c1', c2', p2, …]` as xy pairs.
#[derive(Clone, Debug)]
pub struct CubicPath {
    pub pts: Vec<f64>,
    pub closed: bool,
}

impl CubicPath {
    pub fn seg_count(&self) -> usize {
        if self.pts.len() < 8 {
            0
        } else {
            (self.pts.len() / 2 - 1) / 3
        }
    }
}

/// Subpath sampled at N arc-length points.
#[derive(Clone, Debug)]
pub struct Sampled {
    pub pts: Vec<f64>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphError(pub String);

impl std::fmt::Display for MorphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "morphicons: {}", self.0)
    }
}

impl std::error::Error for MorphError {}

pub(crate) fn morph_err(msg: impl Into<String>) -> MorphError {
    MorphError(msg.into())
}
