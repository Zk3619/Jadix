//! Frame render graph (WP-3.13 capability layer).
//!
//! Collects pass metadata for the current frame so 2D path, canvas, image, scene3d,
//! subtree capture, and effect shaders share one DAG. Execution still lives in
//! [`crate::app::state::AppState::render`]; this module is the IR + dump, not a
//! second renderer.

use std::fmt::Write as _;

/// Kind of GPU / CPU work a frame may emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassKind {
    Geometry2d,
    SubtreeCapture,
    Effect,
    Scene3d,
    Shader,
    Path2d,
    Composite,
    GlyphText,
    Present,
    Compute,
}

/// One node in the per-frame graph.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPassNode {
    pub kind: PassKind,
    pub label: String,
    /// Resource keys this pass reads (`jadix_path:…`, `jadix_canvas:…`, capture ids).
    pub reads: Vec<String>,
    /// Resource keys this pass writes.
    pub writes: Vec<String>,
}

impl RenderPassNode {
    pub fn new(kind: PassKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn read(mut self, key: impl Into<String>) -> Self {
        self.reads.push(key.into());
        self
    }

    pub fn write(mut self, key: impl Into<String>) -> Self {
        self.writes.push(key.into());
        self
    }
}

/// Per-frame pass list. Built during layout collect / render prep.
#[derive(Clone, Debug, Default)]
pub struct RenderGraph {
    pub nodes: Vec<RenderPassNode>,
}

impl RenderGraph {
    pub fn push(&mut self, node: RenderPassNode) {
        self.nodes.push(node);
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Human-readable DAG dump (`JADIX_RENDER_GRAPH=1`).
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "render-graph nodes={}", self.nodes.len());
        for (i, n) in self.nodes.iter().enumerate() {
            let _ = writeln!(
                out,
                "  [{i}] {:?} {} reads={:?} writes={:?}",
                n.kind, n.label, n.reads, n.writes
            );
        }
        out
    }

    /// Default skeleton used when a frame has no extra captures/effects.
    pub fn standard_composite(texture_keys: impl IntoIterator<Item = String>) -> Self {
        let mut g = Self::default();
        let keys: Vec<String> = texture_keys.into_iter().collect();
        if !keys.is_empty() {
            g.push(RenderPassNode {
                kind: PassKind::Geometry2d,
                label: "path-canvas-image".into(),
                reads: Vec::new(),
                writes: keys.clone(),
            });
        }
        g.push(
            RenderPassNode::new(PassKind::Composite, "paint-ops")
                .read("instances")
                .read("textured"),
        );
        g.push(RenderPassNode::new(PassKind::GlyphText, "glyphon"));
        g.push(RenderPassNode::new(PassKind::Present, "swapchain"));
        g
    }
}

thread_local! {
    static LAST_DUMP: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

pub fn store_last_dump(s: String) {
    LAST_DUMP.with(|c| *c.borrow_mut() = s);
}

pub fn take_last_dump() -> String {
    LAST_DUMP.with(|c| c.borrow().clone())
}

pub fn env_dump_enabled() -> bool {
    std::env::var("JADIX_RENDER_GRAPH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "tests/graph.rs"]
mod tests;
