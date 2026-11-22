//! Character presentation and shared CPU layout of the core protocol.
//! No OS views, GPU or private application model. Geometry is in host units.
use crate::{ratatui, CellColors, TerminalEditor};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use jadix_core::presentation::{
    EventPayload, MeasureResult, NodeContent, NodeDescriptor, NodeId, PlatformEvent,
    PresentationNode, RootLifetime,
};
use jadix_layout::presentation::{LayoutStats, MeasureMode, PresentationLayout};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    io,
};
use unicode_segmentation::UnicodeSegmentation;
mod batch;
#[cfg(feature = "portable")]
mod checkpoint;
mod scrollbars;
mod search;
pub use search::{CellSearchMatch, CellSearchSummary};

struct Node {
    id: NodeId,
    parent: Option<usize>,
    descriptor: NodeDescriptor,
}
struct Field {
    editor: TerminalEditor,
    revision: u64,
    controlled: String,
    top_row: usize,
    preferred_column: Option<usize>,
    visual_caret: Option<crate::text_grid::Caret>,
}
// Stage only addressed fields before layout can accept a structural leaf batch.
struct StructuralLeaves<'a> {
    descriptors: BTreeMap<NodeId, &'a NodeDescriptor>,
    bytes: usize,
    fields: Vec<(NodeId, TerminalEditor, String, u64)>,
    retained: usize,
}
#[derive(Clone, Copy, Default)]
struct Geometry {
    frame: [i64; 4],
    clip: Rect,
    live: bool,
    scroll: [f64; 2],
    shift: [f64; 2],
}

fn cell_measure(
    cell: [f64; 2],
    request: jadix_layout::presentation::MeasureRequest,
    node: &NodeDescriptor,
) -> Result<MeasureResult, jadix_layout::presentation::LayoutError> {
    let label = label(&node.content);
    let value = match &node.content {
        NodeContent::Input {
            value, placeholder, ..
        }
        | NodeContent::Textarea {
            value, placeholder, ..
        } => {
            if value.is_empty() {
                placeholder
            } else {
                value
            }
        }
        _ => label.as_ref(),
    };
    let password = matches!(node.content, NodeContent::Input { password: true, .. });
    let width = if request.width_mode == MeasureMode::MinContent {
        1
    } else {
        (request.key.max_width / cell[0]).floor().max(1.0) as usize
    };
    let mut cols = 0;
    let mut rows = 1;
    grid(value, password, width, usize::MAX, |_, _, col, row, w| {
        cols = cols.max(col + w);
        rows = rows.max(row + 1);
    });
    if matches!(node.content, NodeContent::Text(_)) {
        if let Some(max) = node.style.max_lines.or_else(|| {
            (node.style.text_overflow != jadix_core::presentation::style::TextOverflow::Wrap)
                .then_some(1)
        }) {
            rows = rows.min(usize::from(max));
        }
    }
    Ok(MeasureResult {
        key: request.key,
        width: (cols.max(1) as f64 * cell[0]).min(request.key.max_width),
        height: (rows as f64 * cell[1]).min(request.key.max_height),
        baseline: None,
    })
}
/// One shared-protocol surface. Accepts explicit frames or shared CPU layout
/// through `present_auto`; it does not convert pixel UI into character art.
/// One cell is `cell_size` host units. Closed/zero-sized ancestors hide their
/// descendants from both input and paint, including nested modal scopes.
pub struct CellSurface {
    lifetime: RootLifetime,
    cell_size: [f64; 2],
    nodes: Vec<Node>,
    node_indices: HashMap<NodeId, usize>,
    candidate_bytes: usize,
    fields: BTreeMap<NodeId, Field>,
    // Accepted conservative document/history/clipboard sum; edits adjust only their owner.
    editor_bytes: usize,
    #[cfg(feature = "native-clipboard")]
    clipboard: Option<Box<dyn crate::TextClipboard>>,
    scroll: BTreeMap<NodeId, [f64; 2]>,
    geometry: Vec<Geometry>,
    revision: u64,
    focus: Option<NodeId>,
    modals: Vec<(NodeId, Option<NodeId>)>,
    capture: Option<NodeId>,
    area: Option<Rect>,
    active: bool,
    dirty: bool,
    colors: CellColors,
    default_colors: crate::DefaultColors,
    auto_layout: PresentationLayout,
    structural_layout: Option<jadix_layout::presentation::StructuralLayout>,
    paint_order: Vec<usize>,
    // Geometry-dependent paint/hit candidates. Full order remains for search.
    visible_paint_order: Vec<usize>,
    // Natural node order; rebuilt with accepted geometry/eligibility.
    tab_order: Vec<usize>,
    search: search::Search,
    event_revisions: BTreeMap<NodeId, u64>,
    // Emission makes exhaustion sticky until complete publication retires its owner.
    event_revision_exhausted: bool,
    navigation: BTreeSet<NodeId>,
    retired: jadix_core::presentation::retired::RetiredIds,
    automatic: bool,
    scroll_limits: Vec<[f64; 2]>,
    scrollbars: bool,
    scrollbar_capture: Option<scrollbars::Capture>,
}
impl CellSurface {
    pub const EDITOR_BYTES: usize = 32 * 1024 * 1024;
    /// Opt into a host text clipboard; None restores editor-local clipboard use.
    /// The provider owns any OS connection and may report read/write failures.
    #[cfg(feature = "native-clipboard")]
    pub fn set_clipboard(
        &mut self,
        clipboard: Option<Box<dyn crate::TextClipboard>>,
    ) -> io::Result<()> {
        if clipboard.is_some() {
            self.live()?;
        }
        self.clipboard = clipboard;
        Ok(())
    }
    pub fn new(lifetime: RootLifetime, cell_size: [f64; 2]) -> io::Result<Self> {
        if cell_size
            .iter()
            .any(|v| !v.is_finite() || *v < 0.01 || *v > 4096.0)
        {
            return Err(invalid("invalid cell size"));
        }
        Ok(Self {
            structural_layout: None,
            auto_layout: PresentationLayout::new(lifetime.clone()),
            lifetime,
            cell_size,
            nodes: Vec::new(),
            node_indices: HashMap::new(),
            candidate_bytes: 0,
            fields: BTreeMap::new(),
            editor_bytes: 0,
            #[cfg(feature = "native-clipboard")]
            clipboard: None,
            scroll: BTreeMap::new(),
            geometry: Vec::new(),
            revision: 0,
            focus: None,
            modals: Vec::new(),
            capture: None,
            area: None,
            active: true,
            dirty: true,
            colors: CellColors::default(),
            default_colors: crate::DefaultColors::default(),
            paint_order: Vec::new(),
            visible_paint_order: Vec::new(),
            tab_order: Vec::new(),
            search: search::Search::default(),
            event_revisions: BTreeMap::new(),
            event_revision_exhausted: false,
            navigation: BTreeSet::new(),
            retired: Default::default(),
            automatic: false,
            scroll_limits: Vec::new(),
            scrollbars: false,
            scrollbar_capture: None,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Changing output depth invalidates paint without replacing local state.
    pub fn set_colors(&mut self, colors: CellColors) {
        if self.colors != colors {
            self.colors = colors;
            self.dirty = true;
        }
    }
    /// Supply observed terminal defaults. Missing channels remain unknown; the
    /// historical black alpha-compositing fallback applies only to unknowns.
    pub fn set_default_colors(&mut self, colors: crate::DefaultColors) {
        if self.default_colors != colors {
            self.default_colors = colors;
            self.dirty = true;
        }
    }
    pub fn default_colors(&self) -> crate::DefaultColors {
        self.default_colors
    }
    pub fn colors(&self) -> CellColors {
        self.colors
    }
    /// Draw and hit-test scrollbars inside Scroll frames. Disabled by default;
    /// enabling overlays one edge cell without changing the layout contract.
    pub fn set_scrollbars(&mut self, enabled: bool) {
        if self.scrollbars != enabled {
            self.scrollbars = enabled;
            self.scrollbar_capture = None;
            self.dirty = true;
        }
    }
    pub fn cell_size(&self) -> [f64; 2] {
        self.cell_size
    }
    pub fn focused(&self) -> Option<NodeId> {
        self.focus
    }
    /// Set a visible, in-scope Scroll's absolute host-unit offset, clamped to
    /// its retained content bounds. Does not publish or lay out a new tree.
    pub fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> io::Result<bool> {
        self.live()?;
        let index = self
            .node_indices
            .get(&node)
            .copied()
            .ok_or_else(|| invalid("unknown scroll target"))?;
        if offset.iter().any(|v| !v.is_finite())
            || !matches!(self.nodes[index].descriptor.content, NodeContent::Scroll)
            || !self.geometry.get(index).is_some_and(|g| g.live)
            || !self.in_scope(index)
        {
            return Err(invalid("ineligible scroll target"));
        }
        let old = self.scroll.get(&node).copied().unwrap_or([0.; 2]);
        Ok(self
            .scroll_node(index, offset[0] - old[0], offset[1] - old[1])
            .is_some())
    }
    /// Inspect the last published tree, in preorder, without copying labels.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &NodeDescriptor)> {
        self.nodes.iter().map(|node| (node.id, &node.descriptor))
    }
    pub fn focus_node(&mut self, node: NodeId) -> io::Result<()> {
        self.live()?;
        let index = self
            .node_indices
            .get(&node)
            .copied()
            .ok_or_else(|| invalid("unknown focus target"))?;
        if !self.focusable(index) {
            return Err(invalid("ineligible focus target"));
        }
        if self.focus != Some(node) {
            self.capture = None;
            self.focus = Some(node);
            self.dirty = true;
        }
        self.reveal_focus();
        Ok(())
    }
    /// Bound nodes route plain navigation keys to shared roving-focus handlers.
    pub fn bind_navigation(&mut self, nodes: &[NodeId]) -> io::Result<()> {
        self.live()?;
        let ids: BTreeSet<_> = nodes.iter().copied().collect();
        if ids.len() != nodes.len() || ids.iter().any(|id| !self.nodes.iter().any(|n| n.id == *id))
        {
            return Err(invalid("invalid navigation targets"));
        }
        self.navigation = ids;
        Ok(())
    }
    pub fn needs_frame(&self) -> bool {
        self.dirty || (self.lifetime.is_disposed() && !self.nodes.is_empty())
    }
    pub fn field(&self, id: NodeId) -> Option<&TerminalEditor> {
        self.fields.get(&id).map(|field| &field.editor)
    }
    pub fn has_modal(&self) -> bool {
        !self.modals.is_empty()
    }
    pub fn editor_focused(&self) -> bool {
        self.focus.is_some_and(|id| self.fields.contains_key(&id))
    }
    pub fn input_active(&self) -> bool {
        self.active && !self.lifetime.is_disposed()
    }
    /// Atomically replace constraints for the current editors. External values
    /// and history survive; absent IDs reset to unlimited insertion.
    pub fn set_input_limits(&mut self, limits: &[(NodeId, usize)]) -> io::Result<()> {
        self.live()?;
        let mut seen = BTreeSet::new();
        if limits.iter().any(|(id, n)| {
            !self.fields.contains_key(id) || *n > TerminalEditor::MAX_BYTES || !seen.insert(*id)
        }) {
            return Err(invalid("invalid input limits"));
        }
        let limits: BTreeMap<_, _> = limits.iter().copied().collect();
        for (id, field) in &mut self.fields {
            let declared = self
                .node_indices
                .get(id)
                .and_then(|index| self.nodes[*index].descriptor.content.input_max_length());
            field
                .editor
                .set_max_length(limits.get(id).copied().or(declared))?;
        }
        Ok(())
    }
    /// Drop Taffy storage while retaining host measurements and active frames.
    /// The next `present_auto` rebuilds; this does not schedule a paint itself.
    pub fn release_layout_cache(&mut self) {
        self.auto_layout.release_layout_cache();
        if let Some(layout) = &mut self.structural_layout {
            layout.release_layout_cache();
        }
    }
    /// Layout the shared NodeStyle tree in host units, using terminal grapheme
    /// metrics. Native hosts use this same CPU engine with native measurements.
    /// `area` is in cells; frames are relative to its origin. No GPU/OS view is
    /// created. Candidate failure preserves active geometry and editor state.
    pub fn present_auto(
        &mut self,
        expected: u64,
        mut roots: Vec<PresentationNode>,
        area: Rect,
        environment_revision: u64,
    ) -> io::Result<LayoutStats> {
        self.live()?;
        if expected != self.revision {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.validate_candidate(&roots)?;
        let stats = self.layout_auto(&mut roots, area, environment_revision)?;
        self.present(expected, roots)?;
        self.automatic = true;
        if self.area != Some(area) {
            self.dirty = true;
        }
        self.layout(area);
        Ok(stats)
    }
    fn layout_auto(
        &mut self,
        roots: &mut [PresentationNode],
        area: Rect,
        environment_revision: u64,
    ) -> io::Result<LayoutStats> {
        let cell = self.cell_size;
        let stats = self
            .auto_layout
            .layout(
                roots,
                [
                    f64::from(area.width) * cell[0],
                    f64::from(area.height) * cell[1],
                ]
                .map(|v| v as f32),
                environment_revision,
                |request, node| cell_measure(cell, request, node),
            )
            .map_err(|error| io::Error::other(format!("cell layout: {error:?}")))?;
        Ok(stats)
    }
    /// A direct structural protocol path. Taffy sync processes changed records;
    /// warm leaf commits reuse topology and snapshots; frame collection still
    /// visits the full tree. Stable-frame text updates publish changed content.
    /// Snapshot and structural publishers cannot mix.
    pub fn commit_auto(
        &mut self,
        batch: &jadix_core::presentation::CommitBatch,
        area: Rect,
        environment: u64,
    ) -> io::Result<LayoutStats> {
        self.live()?;
        if batch.previous != self.revision
            || (self.structural_layout.is_none() && self.revision != 0)
        {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let leaf_updates = if self.automatic && self.area == Some(area) {
            self.structural_leaf_updates(batch)?
        } else {
            None
        };
        let mut layout = self.structural_layout.take().unwrap_or_else(|| {
            jadix_layout::presentation::StructuralLayout::new(self.lifetime.clone())
        });
        let cell = self.cell_size;
        let viewport = [
            f64::from(area.width) * cell[0],
            f64::from(area.height) * cell[1],
        ]
        .map(|v| v as f32);
        let result = if leaf_updates.is_some() {
            layout.commit(batch, viewport, environment, |request, node| {
                cell_measure(cell, request, node)
            })
        } else {
            layout.commit_checked(
                batch,
                viewport,
                environment,
                |request, node| cell_measure(cell, request, node),
                |roots| {
                    self.validate_candidate(roots)
                        .map_err(|_| jadix_layout::presentation::LayoutError::InvalidTree)
                },
            )
        };
        let stats = match result {
            Ok(stats) => stats,
            Err(error) => {
                self.structural_layout = Some(layout);
                return Err(io::Error::other(format!(
                    "structural cell layout: {error:?}"
                )));
            }
        };
        if let Some(StructuralLeaves {
            descriptors: updates,
            bytes,
            fields,
            retained,
        }) = leaf_updates
        {
            // Complete shared measurement coverage can prove every frame is
            // unchanged. Otherwise compare the complete computed geometry.
            let stable = if stats.geometry_reused {
                true
            } else {
                let mut pending: Vec<_> = layout.roots().iter().collect();
                let mut visited = 0;
                let mut stable = true;
                while let Some(node) = pending.pop() {
                    visited += 1;
                    if self.node_indices.get(&node.id).is_none_or(|&index| {
                        self.nodes[index].descriptor.frame != node.descriptor.frame
                    }) {
                        stable = false;
                        break;
                    }
                    pending.extend(node.children.iter());
                }
                stable && visited == self.nodes.len()
            };
            if stable {
                self.accept_structural_fields(fields, retained, true);
                for (id, descriptor) in updates {
                    self.search.touch(id);
                    self.nodes[self.node_indices[&id]].descriptor.content =
                        descriptor.content.clone();
                }
                self.candidate_bytes = bytes;
                self.revision = batch.revision;
                self.dirty = true;
                self.structural_layout = Some(layout);
                return Ok(stats);
            }
            // Topology, style, resources and editor metadata were admitted as
            // unchanged. Validate the complete frame set before writing it so
            // failure cannot expose a partially published geometry.
            let result = (|| {
                let mut pending: Vec<_> = layout.roots().iter().collect();
                let mut visited = 0;
                while let Some(node) = pending.pop() {
                    let frame = node.descriptor.frame;
                    if !frame.is_valid()
                        || [frame.x, frame.y, frame.width, frame.height]
                            .iter()
                            .any(|v| v.abs() > 1e9)
                    {
                        return Err(invalid("invalid cell geometry"));
                    }
                    if !self.node_indices.contains_key(&node.id) {
                        return Err(invalid("changed leaf topology"));
                    }
                    visited += 1;
                    pending.extend(node.children.iter());
                }
                if visited != self.nodes.len() {
                    return Err(invalid("incomplete leaf geometry"));
                }
                pending.clear();
                pending.extend(layout.roots().iter());
                while let Some(node) = pending.pop() {
                    let index = self.node_indices[&node.id];
                    self.nodes[index].descriptor.frame = node.descriptor.frame;
                    pending.extend(node.children.iter());
                }
                for (id, descriptor) in updates {
                    self.nodes[self.node_indices[&id]].descriptor.content =
                        descriptor.content.clone();
                }
                self.accept_structural_fields(fields, retained, false);
                self.candidate_bytes = bytes;
                self.revision = batch.revision;
                self.search.invalidate();
                self.update_scroll_limits();
                self.dirty = true;
                self.layout(area);
                self.clamp_scrolls();
                Ok(())
            })();
            self.structural_layout = Some(layout);
            if let Err(error) = result {
                self.lifetime.dispose();
                return Err(error);
            }
            return Ok(stats);
        }
        let result = self.present(batch.previous, layout.roots().to_vec());
        self.structural_layout = Some(layout);
        if let Err(error) = result {
            self.lifetime.dispose();
            return Err(error);
        }
        // Every accepted protocol batch advances the structural revision, even
        // if its final descriptors equal the previous publication.
        self.revision = batch.revision;
        self.automatic = true;
        self.layout(area);
        Ok(stats)
    }
    // Stable topology/style uses incremental budgets and editor staging. Reject
    // stale/over-budget editors before the shared layout accepts any revision.
    fn structural_leaf_updates<'a>(
        &self,
        batch: &'a jadix_core::presentation::CommitBatch,
    ) -> io::Result<Option<StructuralLeaves<'a>>> {
        let mut updates = BTreeMap::new();
        for operation in &batch.operations {
            let jadix_core::presentation::Operation::Update { node, descriptor } = operation else {
                return Ok(None);
            };
            let Some(&index) = self.node_indices.get(node) else {
                return Ok(None);
            };
            let old = &self.nodes[index].descriptor;
            let same_kind = match (&old.content, &descriptor.content) {
                (NodeContent::Text(_), NodeContent::Text(_)) => true,
                (
                    NodeContent::Button { enabled: a, .. },
                    NodeContent::Button { enabled: b, .. },
                ) => a == b,
                (
                    NodeContent::Input {
                        enabled: a,
                        password: p,
                        ..
                    },
                    NodeContent::Input {
                        enabled: b,
                        password: q,
                        ..
                    },
                ) => a == b && p == q,
                (
                    NodeContent::Textarea { enabled: a, .. },
                    NodeContent::Textarea { enabled: b, .. },
                ) => a == b,
                (
                    NodeContent::Checkbox { enabled: a, .. },
                    NodeContent::Checkbox { enabled: b, .. },
                )
                | (
                    NodeContent::Switch { enabled: a, .. },
                    NodeContent::Switch { enabled: b, .. },
                )
                | (
                    NodeContent::Select { enabled: a, .. },
                    NodeContent::Select { enabled: b, .. },
                ) => a == b,
                _ => false,
            };
            if !same_kind
                || old.style != descriptor.style
                || old.accessible_name != descriptor.accessible_name
                || old.accessible_role != descriptor.accessible_role
                || old.accessible_selected != descriptor.accessible_selected
                || old.accessible_expanded != descriptor.accessible_expanded
                || !descriptor.frame.is_valid()
                || [
                    descriptor.frame.x,
                    descriptor.frame.y,
                    descriptor.frame.width,
                    descriptor.frame.height,
                ]
                .iter()
                .any(|v| v.abs() > 1e9)
            {
                return Ok(None);
            }
            updates.insert(*node, descriptor);
        }
        let mut bytes = self.candidate_bytes;
        let mut retained = self.editor_bytes;
        let mut fields = Vec::new();
        for (id, descriptor) in &updates {
            if !descriptor.content.is_valid()
                || !inline_valid(descriptor)
                || content_revision(&descriptor.content) == Some(u64::MAX)
            {
                return Err(invalid("invalid structural leaf"));
            }
            bytes = bytes
                .checked_sub(descriptor_bytes(
                    &self.nodes[self.node_indices[id]].descriptor,
                ))
                .and_then(|bytes| bytes.checked_add(descriptor_bytes(descriptor)))
                .ok_or_else(|| invalid("cell text budget"))?;
            if let Some((value, revision)) = field_value(&descriptor.content) {
                let old = &self.fields[id];
                if revision < old.revision || (revision == old.revision && value != old.controlled)
                {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                if value.len() > TerminalEditor::MAX_BYTES {
                    return Err(invalid("cell editor budget"));
                }
                if revision > old.revision
                    || old.editor.max_length() != descriptor.content.input_max_length()
                {
                    let mut next = old.editor.clone();
                    if revision > old.revision {
                        next.apply_external(next.document().revision(), value)?;
                    }
                    next.set_max_length(descriptor.content.input_max_length())?;
                    retained = retained
                        .checked_sub(old.editor.retained_bytes())
                        .and_then(|n| n.checked_add(next.retained_bytes()))
                        .ok_or_else(|| invalid("cell aggregate editor budget"))?;
                    fields.push((*id, next, value.to_owned(), revision));
                }
            }
        }
        if bytes > 16 * 1024 * 1024 || retained > Self::EDITOR_BYTES {
            return Err(invalid("cell structural leaf budget"));
        }
        Ok(Some(StructuralLeaves {
            descriptors: updates,
            bytes,
            fields,
            retained,
        }))
    }
    fn accept_structural_fields(
        &mut self,
        fields: Vec<(NodeId, TerminalEditor, String, u64)>,
        retained: usize,
        stable: bool,
    ) {
        for (id, editor, controlled, revision) in fields {
            let old = self.fields.get_mut(&id).expect("accepted editor");
            if editor.document() != old.editor.document() {
                old.preferred_column = None;
                old.visual_caret = None;
            }
            old.editor = editor;
            old.controlled = controlled;
            old.revision = revision;
            if stable
                && self
                    .search
                    .active
                    .and_then(|at| self.search.matches.get(at))
                    .is_none_or(|found| found.node != id)
            {
                let index = self.node_indices[&id];
                reveal_caret(
                    old,
                    self.geometry[index].frame,
                    matches!(
                        self.nodes[index].descriptor.content,
                        NodeContent::Input { password: true, .. }
                    ),
                );
            }
        }
        self.editor_bytes = retained;
    }
    /// Full relayout after viewport/environment change, without a protocol commit.
    pub fn relayout_structural(&mut self, area: Rect, environment: u64) -> io::Result<LayoutStats> {
        self.live()?;
        let cell = self.cell_size;
        let layout = self
            .structural_layout
            .as_mut()
            .ok_or_else(|| invalid("no structural publisher"))?;
        let stats = layout
            .relayout(
                [
                    f32::from(area.width) * cell[0] as f32,
                    f32::from(area.height) * cell[1] as f32,
                ],
                environment,
                |request, node| cell_measure(cell, request, node),
            )
            .map_err(|e| io::Error::other(format!("structural relayout: {e:?}")))?;
        let mut pending: Vec<_> = layout.roots().iter().collect();
        while let Some(node) = pending.pop() {
            let index = self.node_indices[&node.id];
            self.nodes[index].descriptor.frame = node.descriptor.frame;
            pending.extend(node.children.iter());
        }
        self.update_scroll_limits();
        self.dirty = true;
        self.layout(area);
        self.clamp_scrolls();
        Ok(stats)
    }
    /// Last published host-unit geometry, useful for inspection and anchoring.
    pub fn frame(&self, id: NodeId) -> Option<jadix_core::presentation::Rect> {
        self.node_indices
            .get(&id)
            .map(|&index| self.nodes[index].descriptor.frame)
    }
    /// Validate the complete candidate before replacing live nodes. Stable IDs
    /// retain edit history/selection and scroll state. Same-version stale echoes
    /// cannot overwrite local editing; a higher controlled revision may replace it.
    pub fn present(&mut self, expected: u64, roots: Vec<PresentationNode>) -> io::Result<bool> {
        self.live()?;
        if self.structural_layout.is_some() {
            return Err(invalid(
                "snapshot publisher cannot replace structural publisher",
            ));
        }
        if expected != self.revision {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let next = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("cell revision exhausted"))?;
        self.validate_candidate(&roots)?;
        let mut nodes = Vec::new();
        let mut ids = BTreeSet::new();
        let mut editable = BTreeSet::new();
        let mut pending: Vec<_> = roots.into_iter().rev().map(|node| (node, None)).collect();
        while let Some((node, parent)) = pending.pop() {
            ids.insert(node.id);
            if field_value(&node.descriptor.content).is_some() {
                editable.insert(node.id);
            }
            let index = nodes.len();
            pending.extend(
                node.children
                    .into_iter()
                    .rev()
                    .map(|child| (child, Some(index))),
            );
            nodes.push(Node {
                id: node.id,
                parent,
                descriptor: node.descriptor,
            });
        }
        if self.nodes.len() == nodes.len()
            && self
                .nodes
                .iter()
                .zip(&nodes)
                .all(|(a, b)| a.id == b.id && a.parent == b.parent && a.descriptor == b.descriptor)
        {
            self.automatic = false;
            return Ok(false);
        }
        let mut visible = Vec::with_capacity(nodes.len());
        let has_overlays = nodes.iter().any(|node| {
            matches!(
                node.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            )
        });
        let ordered = if has_overlays {
            let metadata = nodes
                .iter()
                .map(|n| (n.id, (n.parent.map(|p| nodes[p].id), &n.descriptor.content)))
                .collect();
            let priority = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
            jadix_layout::presentation_overlay::overlay_order_with_priority(&metadata, &priority)
                .map_err(|_| invalid("invalid cell overlay dependencies"))?
        } else {
            Vec::new()
        };
        let mut layers = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let shown = node.parent.is_none_or(|parent| visible[parent]) && shown(&node.descriptor);
            visible.push(shown);
            let overlay = matches!(
                node.descriptor.content,
                NodeContent::Dialog { open: true, .. } | NodeContent::Popover { open: true, .. }
            );
            layers.push(if overlay {
                Some(node.id)
            } else {
                node.parent.and_then(|p| layers[p])
            });
        }
        let scopes: Vec<_> = ordered
            .into_iter()
            .filter(|id| {
                nodes
                    .iter()
                    .position(|n| n.id == *id)
                    .is_some_and(|i| visible[i])
            })
            .collect();
        if scopes.len() > 8 {
            return Err(invalid("cell modal budget"));
        }
        let mut paint_order: Vec<_> = (0..nodes.len()).filter(|i| layers[*i].is_none()).collect();
        for id in &scopes {
            paint_order.extend((0..nodes.len()).filter(|i| layers[*i] == Some(*id)));
        }
        let mut retained = 0usize;
        for node in &nodes {
            if let Some((value, revision)) = field_value(&node.descriptor.content) {
                let bytes = self.fields.get(&node.id).map_or(value.len(), |old| {
                    if revision > old.revision && value != old.editor.document().text() {
                        old.editor.retained_bytes()
                            - old.editor.document().len()
                            - old.editor.history_bytes()
                            + value.len()
                    } else {
                        old.editor.retained_bytes()
                    }
                });
                retained = retained.saturating_add(bytes);
                if retained > Self::EDITOR_BYTES {
                    return Err(invalid("cell aggregate editor budget"));
                }
            }
        }
        self.fields.retain(|id, _| editable.contains(id));
        self.scroll.retain(|id, _| ids.contains(id));
        self.event_revisions.retain(|id, _| ids.contains(id));
        self.event_revision_exhausted = self
            .event_revisions
            .values()
            .any(|revision| *revision == u64::MAX);
        self.navigation.retain(|id| ids.contains(id));
        for node in &nodes {
            if let Some((value, revision)) = field_value(&node.descriptor.content) {
                if let Some(old) = self.fields.get_mut(&node.id) {
                    if revision > old.revision {
                        if value != old.editor.document().text() {
                            old.preferred_column = None;
                            old.visual_caret = None;
                        }
                        old.editor
                            .apply_external(old.editor.document().revision(), value)?;
                        old.revision = revision;
                        old.controlled = value.into();
                    }
                } else {
                    self.fields.insert(
                        node.id,
                        Field {
                            editor: TerminalEditor::new(value)?,
                            revision,
                            controlled: value.into(),
                            top_row: 0,
                            preferred_column: None,
                            visual_caret: None,
                        },
                    );
                }
                self.fields
                    .get_mut(&node.id)
                    .unwrap()
                    .editor
                    .set_max_length(node.descriptor.content.input_max_length())?;
            }
        }
        self.editor_bytes = retained;
        let common = self
            .modals
            .iter()
            .map(|v| v.0)
            .zip(&scopes)
            .take_while(|(a, b)| a == *b)
            .count();
        while self.modals.len() > common {
            self.focus = self.modals.pop().unwrap().1;
        }
        for scope in scopes.into_iter().skip(common) {
            self.modals.push((scope, self.focus));
            self.focus = None;
        }
        if let Some(id) = self.capture {
            let old = self.nodes.iter().find(|n| n.id == id);
            let new = nodes.iter().find(|n| n.id == id);
            if old.zip(new).is_none_or(|(old, new)| {
                std::mem::discriminant(&old.descriptor.content)
                    != std::mem::discriminant(&new.descriptor.content)
            }) {
                self.capture = None;
            }
        }
        self.automatic = false;
        self.node_indices = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        self.candidate_bytes = nodes.iter().map(|n| descriptor_bytes(&n.descriptor)).sum();
        self.search.invalidate();
        self.nodes = nodes;
        self.update_scroll_limits();
        self.paint_order = paint_order;
        self.revision = next;
        self.dirty = true;
        if let Some(area) = self.area {
            self.layout(area);
            self.clamp_scrolls();
        }
        Ok(true)
    }
    // Geometry changes can shrink the retained range, even with stable IDs.
    fn clamp_scrolls(&mut self) {
        let mut changed = false;
        for (index, node) in self.nodes.iter().enumerate() {
            if !matches!(node.descriptor.content, NodeContent::Scroll) {
                continue;
            }
            if let Some(offset) = self.scroll.get_mut(&node.id) {
                let max = self.scroll_limits[index];
                let next = [offset[0].clamp(0., max[0]), offset[1].clamp(0., max[1])];
                changed |= next != *offset;
                *offset = next;
            }
        }
        if changed {
            self.dirty = true;
            if let Some(area) = self.area {
                self.layout(area);
            }
        }
    }
    fn validate_candidate(&self, roots: &[PresentationNode]) -> io::Result<()> {
        let mut ids = BTreeSet::new();
        let mut bytes = 0usize;
        let mut overlays = false;
        let mut pending: Vec<_> = roots.iter().rev().map(|node| (node, 0)).collect();
        while let Some((node, depth)) = pending.pop() {
            if ids.len() >= 65_536
                || depth > 128
                || node.id.root != self.lifetime.id()
                || !ids.insert(node.id)
                || self.retired.contains(&node.id)
                || !node.descriptor.frame.is_valid()
                || !node.descriptor.style.is_valid()
                || !inline_valid(&node.descriptor)
                || [
                    node.descriptor.frame.x,
                    node.descriptor.frame.y,
                    node.descriptor.frame.width,
                    node.descriptor.frame.height,
                ]
                .iter()
                .any(|v| v.abs() > 1e9)
                || (!node.children.is_empty() && !node.descriptor.content.accepts_children())
            {
                return Err(invalid("invalid cell tree"));
            }
            let content = &node.descriptor.content;
            if content_revision(content) == Some(u64::MAX) {
                return Err(invalid("cell event revision exhausted"));
            }
            overlays |= matches!(
                content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            );
            bytes = bytes
                .checked_add(
                    content_bytes(content)
                        + node.descriptor.style.font_family.len()
                        + node.descriptor.style.inline_styles.len()
                            * std::mem::size_of::<jadix_core::presentation::style::InlineStyle>()
                        + node
                            .descriptor
                            .accessible_name
                            .as_ref()
                            .map_or(0, |v| v.len()),
                )
                .ok_or_else(|| invalid("cell text budget"))?;
            if bytes > 16 * 1024 * 1024 {
                return Err(invalid("cell text budget"));
            }
            if !content.is_valid() {
                return Err(invalid("invalid cell content"));
            }
            if let Some((value, revision)) = field_value(content) {
                if value.len() > TerminalEditor::MAX_BYTES {
                    return Err(invalid("cell editor budget"));
                }
                if let Some(old) = self.fields.get(&node.id) {
                    if revision < old.revision
                        || (revision == old.revision && value != old.controlled)
                    {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                }
            }
            pending.extend(node.children.iter().rev().map(|child| (child, depth + 1)));
        }
        if !overlays {
            return Ok(());
        }
        let mut metadata = BTreeMap::new();
        let mut pending: Vec<_> = roots.iter().map(|node| (node, None)).collect();
        while let Some((node, parent)) = pending.pop() {
            metadata.insert(node.id, (parent, &node.descriptor.content));
            pending.extend(node.children.iter().map(|child| (child, Some(node.id))));
        }
        jadix_layout::presentation_overlay::overlay_order(&metadata)
            .map_err(|_| invalid("invalid cell overlay dependencies"))?;
        Ok(())
    }
    fn live(&self) -> io::Result<()> {
        if self.lifetime.is_disposed() {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
    fn in_scope(&self, index: usize) -> bool {
        let Some((scope, _)) = self.modals.last() else {
            return true;
        };
        let mut at = Some(index);
        while let Some(index) = at {
            if self.nodes[index].id == *scope {
                return true;
            }
            at = self.nodes[index].parent;
        }
        false
    }
    #[cfg(feature = "portable")]
    pub(super) fn event_target(&self, node: NodeId) -> bool {
        self.node_indices
            .get(&node)
            .copied()
            .is_some_and(|i| self.geometry.get(i).is_some_and(|g| g.live) && self.in_scope(i))
    }
    #[cfg(any(feature = "portable", feature = "shell"))]
    pub(super) fn cancel_capture(&mut self) {
        self.capture = None;
        self.scrollbar_capture = None;
    }
    #[cfg(feature = "portable")]
    pub(super) fn shortcut_rank(
        &self,
        target: NodeId,
        scope: Option<NodeId>,
        in_editors: bool,
    ) -> Option<usize> {
        if !self.active || self.lifetime.is_disposed() {
            return None;
        }
        let index = *self.node_indices.get(&target)?;
        if !self.focusable(index)
            || !matches!(
                self.nodes[index].descriptor.content,
                NodeContent::Button { enabled: true, .. }
            )
            || !in_editors && self.focus.is_some_and(|id| self.fields.contains_key(&id))
        {
            return None;
        }
        let Some(scope) = scope else {
            return Some(usize::MAX);
        };
        let scope = *self.node_indices.get(&scope)?;
        if !self.descendant(index, scope) {
            return None;
        }
        let mut at = self
            .focus
            .and_then(|id| self.node_indices.get(&id).copied());
        let mut rank = 0;
        while let Some(i) = at {
            if i == scope {
                return Some(rank);
            }
            rank += 1;
            at = self.nodes[i].parent;
        }
        None
    }
    fn focusable(&self, index: usize) -> bool {
        self.geometry.get(index).is_some_and(|g| g.live)
            && self.in_scope(index)
            && enabled(&self.nodes[index].descriptor.content)
    }
    fn tabbable(&self, index: usize) -> bool {
        #[cfg(test)]
        interaction_tests::TAB_QUERIES.with(|n| n.set(n.get() + 1));
        use jadix_core::presentation::AccessibleRole;
        self.focusable(index)
            && !matches!(
                self.nodes[index].descriptor.accessible_role,
                Some(AccessibleRole::RadioItem { tab_stop: false })
            )
            && !(self.nodes[index].descriptor.accessible_role == Some(AccessibleRole::TabItem)
                && self.nodes[index].descriptor.accessible_selected == Some(false))
    }
    fn layout(&mut self, area: Rect) {
        self.area = Some(area);
        let mut offsets = HashMap::new();
        self.geometry.resize(self.nodes.len(), Geometry::default());
        self.layout_geometry(area, &offsets, 0..self.nodes.len());
        if self.automatic
            && self.nodes.iter().any(|node| {
                matches!(
                    node.descriptor.content,
                    NodeContent::Popover { open: true, .. }
                )
            })
        {
            // At most eight overlays, in dependency order. Re-project descendants
            // after each moved portal so nested anchors see current coordinates.
            let overlays: Vec<_> = self
                .paint_order
                .iter()
                .copied()
                .filter(|i| {
                    matches!(
                        self.nodes[*i].descriptor.content,
                        NodeContent::Popover { open: true, .. }
                    )
                })
                .collect();
            for index in overlays {
                let node = &self.nodes[index];
                let NodeContent::Popover {
                    anchor,
                    placement,
                    gap,
                    ..
                } = node.descriptor.content
                else {
                    continue;
                };
                let Some(anchor) = self.node_indices.get(&anchor) else {
                    continue;
                };
                let g = self.geometry[*anchor].frame;
                let anchor = jadix_core::presentation::Rect {
                    x: (g[0] - i64::from(area.x)) as f64 * self.cell_size[0],
                    y: (g[1] - i64::from(area.y)) as f64 * self.cell_size[1],
                    width: (g[2] - g[0]) as f64 * self.cell_size[0],
                    height: (g[3] - g[1]) as f64 * self.cell_size[1],
                };
                let frame = node.descriptor.frame;
                let at = jadix_layout::presentation_overlay::position(
                    anchor,
                    [frame.width, frame.height],
                    [
                        f64::from(area.width) * self.cell_size[0],
                        f64::from(area.height) * self.cell_size[1],
                    ]
                    .map(|v| v as f32),
                    placement,
                    gap,
                );
                let shift = [at[0] - frame.x, at[1] - frame.y];
                if shift != self.geometry[index].shift {
                    offsets.insert(node.id, shift);
                    // Preorder descendants are contiguous. A moved portal cannot
                    // change preceding nodes or later siblings' geometry.
                    let end = self.nodes[index + 1..]
                        .iter()
                        .position(|node| node.parent.is_none_or(|parent| parent < index))
                        .map_or(self.nodes.len(), |relative| index + 1 + relative);
                    self.layout_geometry(area, &offsets, index..end);
                }
            }
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if let Some(field) = self.fields.get_mut(&node.id) {
                if self
                    .search
                    .active
                    .and_then(|at| self.search.matches.get(at))
                    .is_some_and(|found| found.node == node.id)
                {
                    continue;
                }
                reveal_caret(
                    field,
                    self.geometry[index].frame,
                    matches!(
                        node.descriptor.content,
                        NodeContent::Input { password: true, .. }
                    ),
                );
            }
        }
        self.tab_order = (0..self.nodes.len())
            .filter(|&index| self.tabbable(index))
            .collect();
        if self.focus.is_none_or(|id| {
            self.node_indices
                .get(&id)
                .is_none_or(|&index| !self.focusable(index))
        }) {
            self.focus = self.tab_order.first().map(|&index| self.nodes[index].id);
        }
        if self.capture.is_some_and(|id| {
            self.node_indices
                .get(&id)
                .is_none_or(|&index| !self.focusable(index))
        }) {
            self.capture = None;
        }
        self.validate_scrollbar_capture();
        self.visible_paint_order = self
            .paint_order
            .iter()
            .copied()
            .filter(|&index| {
                let node = &self.nodes[index];
                let geometry = self.geometry[index];
                matches!(
                    node.descriptor.content,
                    NodeContent::Dialog { open: true, .. }
                        | NodeContent::Popover { open: true, .. }
                ) || self.modals.last().is_some_and(|(id, _)| *id == node.id)
                    || (geometry.live
                        && !intersection(geometry.frame, geometry.clip)
                            .intersection(area)
                            .is_empty())
            })
            .collect();
    }
    fn layout_geometry(
        &mut self,
        area: Rect,
        offsets: &HashMap<NodeId, [f64; 2]>,
        range: std::ops::Range<usize>,
    ) {
        for index in range {
            let node = &self.nodes[index];
            #[cfg(test)]
            interaction_tests::GEOMETRY_VISITS.with(|n| n.set(n.get() + 1));
            let parent = node.parent.map(|index| self.geometry[index]);
            let portal = matches!(
                node.descriptor.content,
                NodeContent::Dialog { open: true, .. } | NodeContent::Popover { open: true, .. }
            );
            let inherited = if portal {
                [0.0; 2]
            } else {
                parent.map_or([0.0; 2], |p| p.scroll)
            };
            let shift = if portal {
                offsets.get(&node.id).copied().unwrap_or([0.; 2])
            } else {
                parent.map_or([0.; 2], |p| p.shift)
            };
            let mut frame = node.descriptor.frame;
            frame.x += shift[0];
            frame.y += shift[1];
            let x =
                i64::from(area.x) + ((frame.x - inherited[0]) / self.cell_size[0]).floor() as i64;
            let y =
                i64::from(area.y) + ((frame.y - inherited[1]) / self.cell_size[1]).floor() as i64;
            let right = i64::from(area.x)
                + ((frame.x + frame.width - inherited[0]) / self.cell_size[0]).ceil() as i64;
            let bottom = i64::from(area.y)
                + ((frame.y + frame.height - inherited[1]) / self.cell_size[1]).ceil() as i64;
            let live = parent.is_none_or(|p| p.live) && shown(&node.descriptor);
            let inherited_clip = if portal {
                area
            } else {
                parent.map_or(area, |p| p.clip)
            };
            let mut clip = inherited_clip;
            if matches!(
                node.descriptor.content,
                NodeContent::Scroll | NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            ) {
                clip = intersection([x, y, right, bottom], inherited_clip);
            }
            let own = self.scroll.get(&node.id).copied().unwrap_or([0.0; 2]);
            self.geometry[index] = Geometry {
                frame: [x, y, right, bottom],
                shift,
                clip,
                live,
                scroll: [inherited[0] + own[0], inherited[1] + own[1]],
            };
        }
    }
    fn hit(&self, x: u16, y: u16) -> Option<usize> {
        self.visible_paint_order
            .iter()
            .rev()
            .copied()
            .find(|index| {
                let g = self.geometry[*index];
                g.live
                    && self.in_scope(*index)
                    && g.clip.contains((x, y).into())
                    && i64::from(x) >= g.frame[0]
                    && i64::from(y) >= g.frame[1]
                    && i64::from(x) < g.frame[2]
                    && i64::from(y) < g.frame[3]
            })
    }
    fn reveal_focus(&mut self) {
        let Some(target) = self
            .focus
            .and_then(|id| self.node_indices.get(&id).copied())
        else {
            return;
        };
        let frame = self.geometry[target].frame;
        self.reveal_region(target, [0, 0, frame[2] - frame[0], frame[3] - frame[1]]);
    }
    fn reveal_region(&mut self, target: usize, region: [i64; 4]) {
        let mut at = self.nodes[target].parent;
        while let Some(index) = at {
            if matches!(self.nodes[index].descriptor.content, NodeContent::Scroll) {
                let frame = self.geometry[target].frame;
                let child = [
                    frame[0] + region[0],
                    frame[1] + region[1],
                    frame[0] + region[2],
                    frame[1] + region[3],
                ];
                let parent = self.geometry[index].frame;
                let delta = |lo: i64, hi: i64, p0: i64, p1: i64| {
                    jadix_core::presentation::input::focus_reveal_delta(
                        (lo - p0) as f64,
                        (hi - p0) as f64,
                        (p1 - p0) as f64,
                    )
                };
                self.scroll_node(
                    index,
                    delta(child[0], child[2], parent[0], parent[2]) * self.cell_size[0],
                    delta(child[1], child[3], parent[1], parent[3]) * self.cell_size[1],
                );
            }
            at = self.nodes[index].parent;
        }
    }
    fn scroll_node(&mut self, index: usize, dx: f64, dy: f64) -> Option<PlatformEvent> {
        let node = &self.nodes[index];
        let max = self.scroll_limits[index];
        let offset = self.scroll.entry(node.id).or_insert([0.0; 2]);
        let next = [
            (offset[0] + dx).clamp(0.0, max[0]),
            (offset[1] + dy).clamp(0.0, max[1]),
        ];
        if next == *offset {
            return None;
        }
        *offset = next;
        let id = node.id;
        self.dirty = true;
        if let Some(area) = self.area {
            self.layout(area);
        }
        Some(PlatformEvent {
            node: id,
            payload: EventPayload::Scroll {
                x: next[0],
                y: next[1],
            },
        })
    }
    pub fn handle(&mut self, event: Event) -> io::Result<Option<PlatformEvent>> {
        // Check exhaustion before editing, so failure cannot consume an edit.
        if self.event_revision_exhausted {
            return Err(invalid("cell event revision exhausted"));
        }
        let mut result = self.handle_event(event)?;
        if let Some(event) = &mut result {
            let revision = match &mut event.payload {
                EventPayload::Edit { revision, .. }
                | EventPayload::Toggle { revision, .. }
                | EventPayload::Select { revision, .. }
                | EventPayload::Dismiss { revision, .. } => Some(revision),
                _ => None,
            };
            if let Some(revision) = revision {
                let last = self.event_revisions.entry(event.node).or_default();
                *last = (*last)
                    .max(*revision)
                    .checked_add(1)
                    .ok_or_else(|| invalid("cell event revision exhausted"))?;
                *revision = *last;
                self.event_revision_exhausted |= *last == u64::MAX;
            }
        }
        Ok(result)
    }
    fn handle_event(&mut self, event: Event) -> io::Result<Option<PlatformEvent>> {
        self.live()?;
        if self.area.is_none() {
            return Err(invalid("render/layout cell surface before input"));
        }
        if matches!(event, Event::FocusLost | Event::FocusGained) {
            self.active = event == Event::FocusGained;
            self.capture = None;
            self.scrollbar_capture = None;
            self.dirty = true;
            return Ok(None);
        }
        if !self.active {
            return Ok(None);
        }
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                return Ok(None);
            }
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                if self.tab_order.is_empty() {
                    return Ok(None);
                }
                let backwards =
                    key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
                let count = self.tab_order.len();
                let at = self
                    .focus
                    .and_then(|id| self.node_indices.get(&id))
                    .and_then(|index| self.tab_order.binary_search(index).ok())
                    .unwrap_or(if backwards { 0 } else { count - 1 });
                let next = if backwards {
                    (at + count - 1) % count
                } else {
                    (at + 1) % count
                };
                self.focus = Some(self.nodes[self.tab_order[next]].id);
                self.capture = None;
                self.scrollbar_capture = None;
                self.dirty = true;
                self.reveal_focus();
                return Ok(self.focus.map(|node| PlatformEvent {
                    node,
                    payload: EventPayload::Focus,
                }));
            }
            if key.code == KeyCode::Esc {
                if let Some((id, _)) = self.modals.last() {
                    let index = self.node_indices.get(id).copied().unwrap();
                    return Ok(Some(PlatformEvent {
                        node: *id,
                        payload: EventPayload::Dismiss {
                            backdrop: false,
                            revision: model_revision(&self.nodes[index].descriptor.content),
                        },
                    }));
                }
            }
            if key.modifiers.is_empty()
                && matches!(
                    key.code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Home
                        | KeyCode::End
                )
            {
                if let Some(node) = self.focus.filter(|id| self.navigation.contains(id)) {
                    use jadix_core::presentation::NavigationKey;
                    let key = match key.code {
                        KeyCode::Up => NavigationKey::Up,
                        KeyCode::Down => NavigationKey::Down,
                        KeyCode::Left => NavigationKey::Left,
                        KeyCode::Right => NavigationKey::Right,
                        KeyCode::Home => NavigationKey::Home,
                        _ => NavigationKey::End,
                    };
                    return Ok(Some(PlatformEvent {
                        node,
                        payload: EventPayload::Navigate {
                            key,
                            shift: false,
                            control: false,
                        },
                    }));
                }
            }
            if !self.focus.is_some_and(|id| self.fields.contains_key(&id))
                && matches!(
                    key.code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                        | KeyCode::Home
                        | KeyCode::End
                )
            {
                let mut at = self
                    .focus
                    .and_then(|id| self.node_indices.get(&id).copied());
                let mut scroll = None;
                while let Some(index) = at {
                    if matches!(self.nodes[index].descriptor.content, NodeContent::Scroll) {
                        scroll = Some(index);
                        break;
                    }
                    at = self.nodes[index].parent;
                }
                let scroll = scroll.or_else(|| {
                    (0..self.nodes.len()).find(|index| {
                        self.geometry[*index].live
                            && self.in_scope(*index)
                            && matches!(self.nodes[*index].descriptor.content, NodeContent::Scroll)
                    })
                });
                if let Some(index) = scroll {
                    let height = self.nodes[index].descriptor.frame.height;
                    let (dx, dy) = match key.code {
                        KeyCode::Up => (0., -self.cell_size[1]),
                        KeyCode::Down => (0., self.cell_size[1]),
                        KeyCode::Left => (-self.cell_size[0], 0.),
                        KeyCode::Right => (self.cell_size[0], 0.),
                        KeyCode::PageUp => (0., -height),
                        KeyCode::PageDown => (0., height),
                        KeyCode::Home => (0., -1e18),
                        _ => (0., 1e18),
                    };
                    return Ok(self.scroll_node(index, dx, dy));
                }
            }
        }
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                if let Some((id, _)) = self.modals.last() {
                    let index = self.node_indices.get(id).copied().unwrap();
                    let g = self.geometry[index];
                    if !intersection(g.frame, g.clip).contains((mouse.column, mouse.row).into()) {
                        self.capture = None;
                        let revision = match self.nodes[index].descriptor.content {
                            NodeContent::Dialog {
                                close_on_overlay_click: true,
                                revision,
                                ..
                            }
                            | NodeContent::Popover {
                                close_on_outside_click: true,
                                revision,
                                ..
                            } => Some(revision),
                            _ => None,
                        };
                        return Ok(revision.map(|revision| PlatformEvent {
                            node: *id,
                            payload: EventPayload::Dismiss {
                                backdrop: true,
                                revision,
                            },
                        }));
                    }
                }
            }
            if let Some(result) = self.handle_scrollbar(mouse) {
                return Ok(result);
            }
            let hit = self.hit(mouse.column, mouse.row);
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            ) {
                let mut at = hit;
                while let Some(index) = at {
                    if matches!(self.nodes[index].descriptor.content, NodeContent::Scroll) {
                        let delta = match mouse.kind {
                            MouseEventKind::ScrollUp => (0.0, -3.0),
                            MouseEventKind::ScrollDown => (0.0, 3.0),
                            MouseEventKind::ScrollLeft => (-6.0, 0.0),
                            _ => (6.0, 0.0),
                        };
                        return Ok(self.scroll_node(
                            index,
                            delta.0 * self.cell_size[0],
                            delta.1 * self.cell_size[1],
                        ));
                    }
                    at = self.nodes[index].parent;
                }
            }
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                let mut at = hit;
                while let Some(index) = at {
                    if self.focusable(index) {
                        let id = self.nodes[index].id;
                        self.focus = Some(id);
                        self.capture = Some(id);
                        self.dirty = true;
                        if let Some(field) = self.fields.get_mut(&id) {
                            let g = self.geometry[index];
                            let password = matches!(
                                self.nodes[index].descriptor.content,
                                NodeContent::Input { password: true, .. }
                            );
                            let byte = hit_text(
                                field.editor.document().text(),
                                password,
                                field_frame(g.frame, field.top_row),
                                i64::from(mouse.column),
                                i64::from(mouse.row),
                            );
                            let anchor = if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                                field.editor.anchor()
                            } else {
                                byte
                            };
                            field.visual_caret = Some(
                                crate::text_grid::Layout::new(
                                    field.editor.document().text(),
                                    password,
                                    (g.frame[2] - g.frame[0]).max(1) as usize,
                                    usize::MAX,
                                )
                                .hit(
                                    (i64::from(mouse.column) - g.frame[0]).max(0) as usize,
                                    (i64::from(mouse.row) - g.frame[1]).max(0) as usize
                                        + field.top_row,
                                ),
                            );
                            field.editor.set_selection(anchor, byte)?;
                            field.preferred_column = None;
                        }
                        return Ok(Some(PlatformEvent {
                            node: id,
                            payload: EventPayload::Focus,
                        }));
                    }
                    at = self.nodes[index].parent;
                }
            }
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                let captured = self.capture.take();
                let mut at = hit;
                while let Some(index) = at {
                    if captured == Some(self.nodes[index].id) && self.focusable(index) {
                        return Ok(self.activate(index));
                    }
                    at = self.nodes[index].parent;
                }
            }
            if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
                if let Some(index) = self
                    .capture
                    .and_then(|id| self.node_indices.get(&id).copied())
                {
                    let id = self.nodes[index].id;
                    if let Some(field) = self.fields.get_mut(&id) {
                        let password = matches!(
                            self.nodes[index].descriptor.content,
                            NodeContent::Input { password: true, .. }
                        );
                        let byte = hit_text(
                            field.editor.document().text(),
                            password,
                            field_frame(self.geometry[index].frame, field.top_row),
                            i64::from(mouse.column),
                            i64::from(mouse.row),
                        );
                        let frame = self.geometry[index].frame;
                        field.visual_caret = Some(
                            crate::text_grid::Layout::new(
                                field.editor.document().text(),
                                password,
                                (frame[2] - frame[0]).max(1) as usize,
                                usize::MAX,
                            )
                            .hit(
                                (i64::from(mouse.column) - frame[0]).max(0) as usize,
                                (i64::from(mouse.row) - frame[1]).max(0) as usize + field.top_row,
                            ),
                        );
                        field.editor.set_selection(field.editor.anchor(), byte)?;
                        field.preferred_column = None;
                        self.dirty = true;
                        return Ok(Some(edit_event(id, field)));
                    }
                }
            }
        }
        let Some(index) = self
            .focus
            .and_then(|id| self.node_indices.get(&id).copied())
        else {
            return Ok(None);
        };
        let id = self.nodes[index].id;
        let retained = self.editor_bytes;
        if let Some(field) = self.fields.get_mut(&id) {
            if let Event::Key(key) = &event {
                let multiline = matches!(
                    self.nodes[index].descriptor.content,
                    NodeContent::Textarea { .. }
                );
                let navigation = matches!(
                    key.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
                ) || (multiline
                    && matches!(key.code, KeyCode::Up | KeyCode::Down));
                if navigation
                    && key.kind != KeyEventKind::Release
                    && !key.modifiers.intersects(
                        KeyModifiers::CONTROL
                            | KeyModifiers::ALT
                            | KeyModifiers::SUPER
                            | KeyModifiers::META,
                    )
                {
                    let width = (self.geometry[index].frame[2] - self.geometry[index].frame[0])
                        .max(1) as usize;
                    let password = matches!(
                        self.nodes[index].descriptor.content,
                        NodeContent::Input { password: true, .. }
                    );
                    let layout = crate::text_grid::Layout::new(
                        field.editor.document().text(),
                        password,
                        width,
                        usize::MAX,
                    );
                    let current = layout.caret(field.editor.caret(), field.visual_caret);
                    let next = match key.code {
                        KeyCode::Left | KeyCode::Right => {
                            field.preferred_column = None;
                            if !key.modifiers.contains(KeyModifiers::SHIFT)
                                && !field.editor.selection().is_empty()
                            {
                                let anchor = layout.caret(field.editor.anchor(), None);
                                if (anchor.row, anchor.col) < (current.row, current.col) {
                                    if key.code == KeyCode::Left {
                                        anchor
                                    } else {
                                        current
                                    }
                                } else if key.code == KeyCode::Right {
                                    anchor
                                } else {
                                    current
                                }
                            } else {
                                layout.step(current, key.code == KeyCode::Right)
                            }
                        }
                        KeyCode::Home | KeyCode::End => {
                            field.preferred_column = None;
                            let mut row = layout.stops.iter().filter(|s| s.row == current.row);
                            if key.code == KeyCode::Home {
                                row.next()
                            } else {
                                row.next_back()
                            }
                            .copied()
                            .unwrap_or(current)
                        }
                        _ => {
                            let preferred = *field.preferred_column.get_or_insert(current.col);
                            let row = if key.code == KeyCode::Up {
                                current.row.saturating_sub(1)
                            } else {
                                (current.row + 1).min(layout.end.1)
                            };
                            layout.hit(preferred, row)
                        }
                    };
                    let before = (
                        field.editor.anchor(),
                        field.editor.caret(),
                        field.visual_caret,
                    );
                    field.editor.set_selection(
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            field.editor.anchor()
                        } else {
                            next.byte
                        },
                        next.byte,
                    )?;
                    field.visual_caret = Some(next);
                    if before
                        == (
                            field.editor.anchor(),
                            field.editor.caret(),
                            field.visual_caret,
                        )
                    {
                        return Ok(None);
                    }
                    reveal_caret(field, self.geometry[index].frame, password);
                    self.dirty = true;
                    return Ok(Some(edit_event(id, field)));
                }
            }
            if matches!(
                self.nodes[index].descriptor.content,
                NodeContent::Input { password: true, .. }
            ) && matches!(&event,Event::Key(key) if key.modifiers.intersects(KeyModifiers::CONTROL|KeyModifiers::SUPER)
                    && matches!(key.code,KeyCode::Char('c'|'x')))
            {
                return Ok(None);
            }
            if matches!(&event,Event::Key(key) if key.code==KeyCode::Enter)
                && matches!(
                    self.nodes[index].descriptor.content,
                    NodeContent::Input { .. }
                )
            {
                return Ok(Some(PlatformEvent {
                    node: id,
                    payload: EventPayload::Activate,
                }));
            }
            #[cfg(feature = "native-clipboard")]
            let export = if self.clipboard.is_some()
                && matches!(crate::clipboard::shortcut(&event), Some('c' | 'x'))
            {
                let selected = &field.editor.document().text()[field.editor.selection()];
                crate::clipboard::check_transfer(selected)?;
                (!selected.is_empty()).then(|| selected.to_owned())
            } else {
                None
            };
            #[cfg(feature = "native-clipboard")]
            let host_paste = if crate::clipboard::shortcut(&event) == Some('v') {
                self.clipboard
                    .as_ref()
                    .map(|host| {
                        let text = host
                            .read_text()
                            .map_err(|e| io::Error::other(e.to_string()))?;
                        crate::clipboard::check_transfer(&text)?;
                        Ok::<_, io::Error>(Event::Paste(text))
                    })
                    .transpose()?
            } else {
                None
            };
            #[cfg(feature = "native-clipboard")]
            let event = host_paste.as_ref().unwrap_or(&event);
            #[cfg(not(feature = "native-clipboard"))]
            let event = &event;
            let normalized;
            let event = if matches!(
                self.nodes[index].descriptor.content,
                NodeContent::Input { .. }
            ) {
                if let Event::Paste(value) = event {
                    normalized =
                        Event::Paste(value.replace("\r\n", " ").replace(['\r', '\n'], " "));
                    &normalized
                } else {
                    event
                }
            } else {
                event
            };
            let mut candidate = field.editor.clone();
            let changed = candidate.handle(event)?;
            let next_bytes = retained - field.editor.retained_bytes() + candidate.retained_bytes();
            if next_bytes > Self::EDITOR_BYTES {
                return Err(invalid("cell aggregate editor budget"));
            }
            #[cfg(feature = "native-clipboard")]
            if let Some(text) = export {
                self.clipboard
                    .as_ref()
                    .unwrap()
                    .write_text(&text)
                    .map_err(|e| io::Error::other(e.to_string()))?;
            }
            field.visual_caret = None;
            field.preferred_column = None;
            if candidate.document() != field.editor.document() {
                self.search.touch(id);
            }
            field.editor = candidate;
            self.editor_bytes = next_bytes;
            if changed {
                reveal_caret(
                    field,
                    self.geometry[index].frame,
                    matches!(
                        self.nodes[index].descriptor.content,
                        NodeContent::Input { password: true, .. }
                    ),
                );
                self.dirty = true;
                return Ok(Some(edit_event(id, field)));
            }
        } else if matches!(event,Event::Key(key) if matches!(key.code,KeyCode::Enter|KeyCode::Char(' ')))
        {
            return Ok(self.activate(index));
        }
        Ok(None)
    }
    fn activate(&mut self, index: usize) -> Option<PlatformEvent> {
        let node = &self.nodes[index];
        let payload = match &node.descriptor.content {
            NodeContent::Button { enabled: true, .. } => EventPayload::Activate,
            NodeContent::Checkbox {
                checked,
                enabled: true,
                revision,
                ..
            }
            | NodeContent::Switch {
                checked,
                enabled: true,
                revision,
                ..
            } => EventPayload::Toggle {
                checked: !*checked,
                revision: *revision,
            },
            NodeContent::Select {
                options,
                value,
                enabled: true,
                revision,
                ..
            } => {
                let at = options.iter().position(|option| option.value == *value);
                let next = (1..=options.len())
                    .filter_map(|step| {
                        options.get(
                            (at.unwrap_or(options.len().saturating_sub(1)) + step)
                                % options.len().max(1),
                        )
                    })
                    .find(|option| option.enabled)?;
                EventPayload::Select {
                    value: next.value.clone(),
                    revision: *revision,
                }
            }
            _ => return None,
        };
        self.dirty = true;
        Some(PlatformEvent {
            node: node.id,
            payload,
        })
    }
}
impl Widget for &mut CellSurface {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if self.lifetime.is_disposed() {
            self.focus = None;
            self.nodes.clear();
            self.scroll_limits.clear();
            self.scrollbar_capture = None;
            self.node_indices.clear();
            self.candidate_bytes = 0;
            self.fields.clear();
            #[cfg(feature = "native-clipboard")]
            {
                self.clipboard = None;
            }
            self.editor_bytes = 0;
            self.scroll.clear();
            self.geometry.clear();
            self.modals.clear();
            self.paint_order.clear();
            self.visible_paint_order.clear();
            self.tab_order.clear();
            self.search = search::Search::default();
            self.event_revisions.clear();
            self.event_revision_exhausted = false;
            self.navigation.clear();
            self.auto_layout.invalidate_measurements();
            self.capture = None;
            self.dirty = false;
            return;
        }
        if self.area != Some(area) {
            self.layout(area);
        }
        self.refresh_search();
        let mut colors = HashMap::new();
        let default_background = self
            .default_colors
            .background
            .map(|c| Color::Rgb(c[0], c[1], c[2]));
        let default_foreground = self
            .default_colors
            .foreground
            .map(|c| Color::Rgb(c[0], c[1], c[2]));
        let mut resolve = |rgba: [u8; 4], background| {
            let background = if background == Color::Reset && rgba[3] > 0 {
                default_background.unwrap_or(background)
            } else {
                background
            };
            if self.colors == CellColors::TrueColor {
                return self.colors.resolve(rgba, background);
            }
            let key = (rgba, background);
            if let Some(color) = colors.get(&key) {
                return *color;
            }
            let color = self.colors.resolve(rgba, background);
            // Bound temporary memory even for hostile per-node palettes.
            if colors.len() < 256 {
                colors.insert(key, color);
            }
            color
        };
        let mut scrolls = Vec::new();
        for &index in &self.visible_paint_order {
            let node = &self.nodes[index];
            let portal = matches!(
                node.descriptor.content,
                NodeContent::Dialog { open: true, .. } | NodeContent::Popover { open: true, .. }
            );
            while scrolls
                .last()
                .is_some_and(|ancestor| portal || !self.descendant(index, *ancestor))
            {
                self.paint_scrollbars(scrolls.pop().unwrap(), buffer);
            }
            if self.modals.last().is_some_and(|(id, _)| *id == node.id) {
                for y in area.y.max(buffer.area.y)..area.bottom().min(buffer.area.bottom()) {
                    for x in area.x.max(buffer.area.x)..area.right().min(buffer.area.right()) {
                        let cell = &mut buffer[(x, y)];
                        let foreground = if cell.fg == Color::Reset {
                            default_foreground.unwrap_or(cell.fg)
                        } else {
                            cell.fg
                        };
                        cell.set_fg(resolve([0, 0, 0, 120], foreground));
                        cell.set_bg(resolve([0, 0, 0, 120], cell.bg));
                    }
                }
            }
            let geometry = self.geometry[index];
            if !geometry.live {
                continue;
            }
            let clip = intersection(geometry.frame, geometry.clip).intersection(buffer.area);
            if clip.is_empty() {
                continue;
            }
            if matches!(node.descriptor.content, NodeContent::Scroll) {
                scrolls.push(index);
            }
            let style = &node.descriptor.style;
            for y in clip.y..clip.bottom() {
                for x in clip.x..clip.right() {
                    let cell = &mut buffer[(x, y)];
                    if style.background[3] > 0 {
                        cell.set_symbol(" ");
                        cell.set_bg(resolve(style.background, cell.bg));
                    }
                }
            }
            let focused = self.focus == Some(node.id) && self.active;
            let field = self.fields.get(&node.id);
            let label = label(&node.descriptor.content);
            let mut value = field.map_or_else(
                || {
                    if label.is_empty() {
                        text(&node.descriptor.content)
                    } else {
                        label.as_ref()
                    }
                },
                |field| field.editor.document().text(),
            );
            let empty_field = field.is_some() && value.is_empty();
            if empty_field {
                value = match &node.descriptor.content {
                    NodeContent::Input { placeholder, .. }
                    | NodeContent::Textarea { placeholder, .. } => placeholder,
                    _ => value,
                };
            }
            let password = !empty_field
                && matches!(
                    node.descriptor.content,
                    NodeContent::Input { password: true, .. }
                );
            let selected = field.map(|field| field.editor.selection());
            let caret = field.filter(|_| focused).map(|field| field.editor.caret());
            let bidi_caret = !password && crate::text_grid::needs_bidi(value);
            let text_frame = field_frame(geometry.frame, field.map_or(0, |field| field.top_row));
            let end = paint_grid(
                value,
                password,
                (geometry.frame[2] - geometry.frame[0]).max(1) as usize,
                (i64::from(clip.bottom()) - text_frame[1]).max(0) as usize,
                if matches!(node.descriptor.content, NodeContent::Text(_)) {
                    style.text_overflow
                } else {
                    Default::default()
                },
                if matches!(node.descriptor.content, NodeContent::Text(_)) {
                    style.max_lines
                } else {
                    None
                },
                |byte, symbol, col, row, width| {
                    let x = text_frame[0] + col as i64;
                    let y = text_frame[1] + row as i64;
                    if x < i64::from(clip.x)
                        || x >= i64::from(clip.right())
                        || y < i64::from(clip.y)
                        || x + width as i64 > i64::from(clip.right())
                        || y >= i64::from(clip.bottom())
                    {
                        return;
                    }
                    let mut paint = Style::default()
                        .fg(resolve(style.foreground, buffer[(x as u16, y as u16)].bg));
                    if style.font_weight >= 600 {
                        paint = paint.add_modifier(Modifier::BOLD);
                    }
                    let span = style.inline_styles.partition_point(|s| s.range[1] <= byte);
                    if let Some(span) = style.inline_styles.get(span).filter(|s| s.range[0] <= byte)
                    {
                        if let Some(color) = span.foreground {
                            paint = paint.fg(resolve(color, buffer[(x as u16, y as u16)].bg));
                        }
                        if let Some(color) = span.background {
                            paint = paint.bg(resolve(color, buffer[(x as u16, y as u16)].bg));
                        }
                        for (value, flag) in [
                            (span.bold, Modifier::BOLD),
                            (span.italic, Modifier::ITALIC),
                            (span.underline, Modifier::UNDERLINED),
                            (span.strikethrough, Modifier::CROSSED_OUT),
                        ] {
                            if let Some(value) = value {
                                paint = if value {
                                    paint.add_modifier(flag)
                                } else {
                                    paint.remove_modifier(flag)
                                };
                            }
                        }
                    }
                    let (found, current) = self.search.highlights(node.id, byte);
                    if found {
                        paint = paint
                            .fg(resolve([0, 0, 0, 255], Color::Reset))
                            .bg(resolve([253, 224, 71, 255], Color::Reset));
                    }
                    if self.colors == CellColors::Monochrome && found {
                        paint = paint.add_modifier(Modifier::UNDERLINED);
                    }
                    if current {
                        if self.colors == CellColors::Monochrome {
                            paint = paint.add_modifier(Modifier::BOLD);
                        }
                        paint = paint
                            .fg(resolve([0, 0, 0, 255], Color::Reset))
                            .bg(resolve([251, 146, 60, 255], Color::Reset));
                    }
                    if focused && field.is_none() {
                        paint = paint.add_modifier(Modifier::REVERSED);
                    }
                    if selected.as_ref().is_some_and(|range| range.contains(&byte)) {
                        paint = if self.colors == CellColors::Monochrome {
                            paint.add_modifier(Modifier::REVERSED)
                        } else {
                            paint.bg(Color::Blue).fg(Color::White)
                        };
                    }
                    if !bidi_caret && caret == Some(byte) {
                        paint = paint.add_modifier(Modifier::REVERSED);
                    }
                    if width == 0 {
                        if !bidi_caret && caret == Some(byte) {
                            buffer[(x as u16, y as u16)].set_style(paint);
                        }
                    } else {
                        buffer.set_stringn(x as u16, y as u16, symbol, width, paint);
                    }
                },
            );
            if caret == Some(value.len()) || (bidi_caret && caret.is_some()) {
                let position = if bidi_caret {
                    let layout = crate::text_grid::Layout::new(
                        value,
                        false,
                        (geometry.frame[2] - geometry.frame[0]).max(1) as usize,
                        usize::MAX,
                    );
                    let c = layout.caret(caret.unwrap(), field.and_then(|f| f.visual_caret));
                    (c.col, c.row)
                } else {
                    end
                };
                let x = text_frame[0] + position.0 as i64;
                let y = text_frame[1] + position.1 as i64;
                if x >= i64::from(clip.x)
                    && y >= i64::from(clip.y)
                    && x < i64::from(clip.right())
                    && y < i64::from(clip.bottom())
                {
                    buffer[(x as u16, y as u16)]
                        .set_style(Style::default().add_modifier(Modifier::REVERSED));
                }
            }
        }
        for index in scrolls.into_iter().rev() {
            self.paint_scrollbars(index, buffer);
        }
        if self.colors == CellColors::Monochrome {
            // Selection, search and scrollbars also participate. Keep their
            // semantic emphasis when replacing color-only contrast.
            for y in area.y.max(buffer.area.y)..area.bottom().min(buffer.area.bottom()) {
                for x in area.x.max(buffer.area.x)..area.right().min(buffer.area.right()) {
                    let cell = &mut buffer[(x, y)];
                    cell.set_fg(Color::Reset).set_bg(Color::Reset);
                }
            }
        }
        self.dirty = false;
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn field_frame(mut frame: [i64; 4], top_row: usize) -> [i64; 4] {
    frame[1] -= top_row as i64;
    frame[3] -= top_row as i64;
    frame
}
fn reveal_caret(field: &mut Field, frame: [i64; 4], password: bool) {
    let text = field.editor.document().text();
    let layout = crate::text_grid::Layout::new(
        text,
        password,
        (frame[2] - frame[0]).max(1) as usize,
        usize::MAX,
    );
    let row = layout.caret(field.editor.caret(), field.visual_caret).row;
    let height = (frame[3] - frame[1]).max(1) as usize;
    if row < field.top_row {
        field.top_row = row;
    }
    if row >= field.top_row + height {
        field.top_row = row + 1 - height;
    }
}
fn edit_event(node: NodeId, field: &Field) -> PlatformEvent {
    let text = field.editor.document().text();
    let selection = field.editor.selection();
    PlatformEvent {
        node,
        payload: EventPayload::Edit {
            value: text.into(),
            revision: field.revision,
            selection_utf16: [
                text[..selection.start].encode_utf16().count(),
                text[..selection.end].encode_utf16().count(),
            ],
            composing_utf16: None,
        },
    }
}
fn shown(node: &NodeDescriptor) -> bool {
    node.frame.width > 0.0
        && node.frame.height > 0.0
        && !matches!(
            node.content,
            NodeContent::Dialog { open: false, .. } | NodeContent::Popover { open: false, .. }
        )
}
fn enabled(content: &NodeContent) -> bool {
    matches!(
        content,
        NodeContent::Button { enabled: true, .. }
            | NodeContent::Input { enabled: true, .. }
            | NodeContent::Textarea { enabled: true, .. }
            | NodeContent::Checkbox { enabled: true, .. }
            | NodeContent::Switch { enabled: true, .. }
            | NodeContent::Select { enabled: true, .. }
    )
}
fn text(content: &NodeContent) -> &str {
    match content {
        NodeContent::Text(value)
        | NodeContent::Input { value, .. }
        | NodeContent::Textarea { value, .. } => value,
        NodeContent::Button { label, .. }
        | NodeContent::Checkbox { label, .. }
        | NodeContent::Switch { label, .. } => label,
        NodeContent::Image { alt, .. } => alt,
        _ => "",
    }
}
fn content_bytes(content: &NodeContent) -> usize {
    match content {
        NodeContent::Select {
            options,
            value,
            placeholder,
            ..
        } => options.iter().fold(
            value.len().saturating_add(placeholder.len()),
            |total, option| {
                total
                    .saturating_add(option.value.len())
                    .saturating_add(option.label.len())
            },
        ),
        NodeContent::Image { source, alt } => source.len().saturating_add(alt.len()),
        NodeContent::Input {
            value, placeholder, ..
        }
        | NodeContent::Textarea {
            value, placeholder, ..
        } => value.len().saturating_add(placeholder.len()),
        _ => text(content).len(),
    }
}
fn descriptor_bytes(node: &NodeDescriptor) -> usize {
    content_bytes(&node.content)
        .saturating_add(node.style.font_family.len())
        .saturating_add(
            node.style.inline_styles.len()
                * std::mem::size_of::<jadix_core::presentation::style::InlineStyle>(),
        )
        .saturating_add(node.accessible_name.as_ref().map_or(0, |name| name.len()))
}
fn inline_valid(node: &NodeDescriptor) -> bool {
    node.style.inline_styles.is_empty()
        || matches!(&node.content,NodeContent::Text(text) if node.style.inline_valid(text))
}
fn field_value(content: &NodeContent) -> Option<(&str, u64)> {
    match content {
        NodeContent::Input {
            value, revision, ..
        }
        | NodeContent::Textarea {
            value, revision, ..
        } => Some((value, *revision)),
        _ => None,
    }
}
fn model_revision(content: &NodeContent) -> u64 {
    match content {
        NodeContent::Dialog { revision, .. } | NodeContent::Popover { revision, .. } => *revision,
        _ => 0,
    }
}
fn content_revision(content: &NodeContent) -> Option<u64> {
    match content {
        NodeContent::Input { revision, .. }
        | NodeContent::Textarea { revision, .. }
        | NodeContent::Checkbox { revision, .. }
        | NodeContent::Switch { revision, .. }
        | NodeContent::Select { revision, .. }
        | NodeContent::Dialog { revision, .. }
        | NodeContent::Popover { revision, .. } => Some(*revision),
        _ => None,
    }
}
fn intersection(frame: [i64; 4], clip: Rect) -> Rect {
    let x = frame[0].max(i64::from(clip.x)).min(i64::from(clip.right()));
    let y = frame[1]
        .max(i64::from(clip.y))
        .min(i64::from(clip.bottom()));
    let right = frame[2].min(i64::from(clip.right())).max(x);
    let bottom = frame[3].min(i64::from(clip.bottom())).max(y);
    Rect::new(x as u16, y as u16, (right - x) as u16, (bottom - y) as u16)
}
fn paint_grid(
    text: &str,
    password: bool,
    width: usize,
    rows: usize,
    overflow: jadix_core::presentation::style::TextOverflow,
    max_lines: Option<u16>,
    mut visit: impl FnMut(usize, &str, usize, usize, usize),
) -> (usize, usize) {
    use jadix_core::presentation::style::TextOverflow;
    if overflow == TextOverflow::Wrap && max_lines.is_none() {
        return grid(text, password, width, rows, visit);
    }
    let limit = usize::from(max_lines.unwrap_or(1)).min(rows);
    if limit == 0 {
        return (0, 0);
    }
    let layout = crate::text_grid::Layout::new(
        text,
        password,
        if overflow == TextOverflow::Clip {
            usize::MAX / 4
        } else {
            width
        },
        limit.saturating_add(1),
    );
    let hidden = layout
        .glyphs
        .iter()
        .any(|g| g.row >= limit || g.col.saturating_add(g.width) > width);
    let last = limit - 1;
    let rtl = layout
        .glyphs
        .iter()
        .find(|g| g.row == last)
        .is_some_and(|g| g.base_rtl);
    let ellipsis = hidden
        && matches!(
            overflow,
            TextOverflow::Ellipsis | TextOverflow::EllipsisStart | TextOverflow::EllipsisMiddle
        );
    for g in &layout.glyphs {
        if g.row >= limit {
            continue;
        }
        let end = if ellipsis && g.row == last && !rtl {
            width.saturating_sub(1)
        } else {
            width
        };
        if g.col + g.width <= end && !(ellipsis && g.row == last && rtl && g.col == 0) {
            visit(g.start, &g.symbol, g.col, g.row, g.width);
        }
    }
    if ellipsis {
        visit(
            usize::MAX,
            "…",
            if rtl { 0 } else { width.saturating_sub(1) },
            last,
            1,
        );
    }
    layout.end
}

// Render and hit testing use exactly the same grapheme widths and wrapping.
fn grid(
    text: &str,
    password: bool,
    width: usize,
    rows: usize,
    mut visit: impl FnMut(usize, &str, usize, usize, usize),
) -> (usize, usize) {
    if !password && crate::text_grid::needs_bidi(text) {
        let layout = crate::text_grid::Layout::new(text, false, width, rows);
        for glyph in &layout.glyphs {
            visit(
                glyph.start,
                &glyph.symbol,
                glyph.col,
                glyph.row,
                glyph.width,
            );
        }
        return layout.end;
    }
    let mut col = 0;
    let mut row = 0;
    for (byte, grapheme) in text.grapheme_indices(true) {
        if row >= rows {
            break;
        }
        if matches!(grapheme, "\n" | "\r\n") {
            visit(byte, "", col, row, 0);
            col = 0;
            row += 1;
            continue;
        }
        let safe = if password {
            "•"
        } else if grapheme == "\t" {
            "    "
        } else if grapheme.chars().any(char::is_control) {
            "�"
        } else {
            grapheme
        };
        let len = ratatui::text::Span::raw(safe).width();
        if len == 0 {
            continue;
        }
        if col + len > width {
            col = 0;
            row += 1;
        }
        if row >= rows {
            break;
        }
        visit(byte, safe, col, row, len);
        col += len;
    }
    if col >= width {
        col = 0;
        row += 1;
    }
    (col, row)
}
fn hit_text(text: &str, password: bool, frame: [i64; 4], x: i64, y: i64) -> usize {
    if !password && crate::text_grid::needs_bidi(text) {
        return crate::text_grid::Layout::new(
            text,
            false,
            (frame[2] - frame[0]).max(1) as usize,
            usize::MAX,
        )
        .hit(
            (x - frame[0]).max(0) as usize,
            (y - frame[1]).max(0) as usize,
        )
        .byte;
    }
    let mut result = text.len();
    let mut found = false;
    grid(
        text,
        password,
        (frame[2] - frame[0]).max(1) as usize,
        (y - frame[1] + 2).max(1) as usize,
        |byte, _, col, row, width| {
            if found {
                return;
            }
            let gy = frame[1] + row as i64;
            let gx = frame[0] + col as i64;
            if gy > y || (gy == y && (width == 0 || gx + width as i64 > x)) {
                result = if gy == y && width > 0 && (x - gx) * 2 + 1 >= width as i64 {
                    byte + text[byte..].graphemes(true).next().unwrap().len()
                } else {
                    byte
                };
                found = true;
            }
        },
    );
    result
}

fn label(content: &NodeContent) -> Cow<'_, str> {
    match content {
        NodeContent::Button { label, .. } => format!("[ {label} ]").into(),
        NodeContent::Checkbox { label, checked, .. } => {
            format!("[{}] {label}", if *checked { 'x' } else { ' ' }).into()
        }
        NodeContent::Switch { label, checked, .. } => {
            format!("{} {label}", if *checked { "ON" } else { "OFF" }).into()
        }
        NodeContent::Select {
            options,
            value,
            placeholder,
            ..
        } => format!(
            "{} ▾",
            options
                .iter()
                .find(|option| option.value == *value)
                .map_or(placeholder.as_str(), |option| option.label.as_ref())
        )
        .into(),
        NodeContent::Image { alt, .. } => format!("[image: {alt}]").into(),
        NodeContent::Text(value) => Cow::Borrowed(value),
        _ => Cow::Borrowed(""),
    }
}

#[cfg(test)]
mod input_cost_tests;

#[cfg(all(test, feature = "portable"))]
mod input_budget_tests;

#[cfg(test)]
mod interaction_tests;

#[cfg(test)]
mod search_delta_tests;

#[cfg(all(test, feature = "native-clipboard"))]
mod clipboard_tests;

#[cfg(test)]
mod geometry_publish_tests;

#[cfg(test)]
mod structural_editor_tests;
