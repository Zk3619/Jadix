//! Node-owned presentation work executed in the host's existing paint phases.

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::LayoutTree;
use std::rc::Rc;
mod tree;
pub use tree::{attach_tree, attach_tree_with_cleanup, TreeSync};
pub(crate) use tree::Retirements;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintEdge {
    Bottom,
    Right,
}

/// An authored edge decoration that does not participate in layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePaint {
    pub edge: PaintEdge,
    pub width: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Hover,
    /// Retained sibling presentation after styles and before layout.
    BeforeLayout,
    State,
    /// Geometry producers run after layout and before paint collect.
    AfterLayout,
    /// Dependents observe all geometry producers from this frame.
    AfterLayoutDependents,
    /// Overflow rows reconcile again after fresh geometry is available.
    AfterGeometry,
}

#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub phase: Phase,
    pub viewport: [f32; 2],
    /// Horizontal constraint propagated through definite and clipped ancestors.
    pub width_budget: Option<f32>,
}

impl Context {
    pub const fn new(phase: Phase, viewport: [f32; 2]) -> Self {
        Self {
            phase,
            viewport,
            width_budget: None,
        }
    }
}

/// A node owns its callbacks; dropping the node releases their captured state.
#[derive(Clone)]
pub struct PresentationSync(
    Rc<dyn Fn(&mut Element, &mut LayoutTree, Context, bool) -> LayoutStatus>,
);

impl PresentationSync {
    pub fn run(
        &self,
        element: &mut Element,
        layout: &mut LayoutTree,
        context: Context,
    ) -> LayoutStatus {
        (self.0)(element, layout, context, true)
    }
}

/// Append a phase-specific callback without replacing previously attached work.
/// Callbacks may update their node's chrome, but must not remount the host tree.
pub fn attach(
    element: &mut Element,
    phase: Phase,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
) {
    attach_impl(element, phase, false, callback);
}

/// Append work for nodes visible in the viewport, using inherited scroll offsets.
/// An offscreen ancestor suppresses this work throughout its subtree.
pub fn attach_visible(
    element: &mut Element,
    phase: Phase,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
) {
    attach_impl(element, phase, true, callback);
}

fn attach_impl(
    element: &mut Element,
    phase: Phase,
    visible_only: bool,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
) {
    attach_observer(element, phase, move |element, layout, context, visible| {
        if !visible_only || visible {
            callback(element, layout, context)
        } else {
            LayoutStatus::Clean
        }
    });
}

pub(super) fn attach_observer(
    element: &mut Element,
    phase: Phase,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context, bool) -> LayoutStatus + 'static,
) {
    element.subtree_presentation_sync = true;
    element.collect_paint_hash = 0;
    let previous = element.presentation_sync.take();
    element.presentation_sync = Some(PresentationSync(Rc::new(
        move |element, layout, context, visible| {
            let status = previous.as_ref().map_or(LayoutStatus::Clean, |previous| {
                (previous.0)(element, layout, context, visible)
            });
            if context.phase == phase {
                status.merge(callback(element, layout, context, visible))
            } else {
                status
            }
        },
    )));
}

/// Whether a valid collect cache contains no live paint in this subtree.
pub fn is_cached_static(element: &Element) -> bool {
    element.collect_paint_hash != 0 && !element.collect_skip_live
}

/// Invalidate this node after attaching product-owned paint work.
pub fn invalidate_paint(element: &mut Element) { element.collect_paint_hash = 0; }

/// One traversal replaces product-specific whole-host paint scans.
pub fn sync_tree(root: &mut Element, layout: &mut LayoutTree, context: Context) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    let mut tree_work = Vec::new();
    let mut dirty_nodes = Vec::new();
    let budget = context
        .width_budget
        .or_else(|| (context.viewport[0] > 0.0).then_some(context.viewport[0]));
    let mut pending = vec![(root as &mut Element, [0.0; 2], true, budget)];
    while let Some((element, scroll, parent_visible, parent_budget)) = pending.pop() {
        if element.collect_paint_hash != 0 && !element.subtree_presentation_sync {
            continue;
        }
        tree::prepare(element, context.phase, &mut tree_work, layout);
        let visible = parent_visible
            && !crate::core::tree::element_is_offscreen(
                element,
                scroll,
                context.viewport[0],
                context.viewport[1],
            );
        let width_budget =
            crate::utils::overflow_fit::tighten_horizontal_constraint(element, parent_budget);
        if let Some(sync) = element.presentation_sync.clone() {
            let node_context = Context {
                width_budget,
                ..context
            };
            let changed = (sync.0)(element, layout, node_context, visible);
            if changed != LayoutStatus::Clean {
                if let Some(node) = element.layout_node {
                    dirty_nodes.push(node);
                }
            }
            status = status.merge(changed);
        }
        let child_scroll = super::mount::child_scroll_offset(element, scroll);
        pending.extend(
            element
                .children
                .iter_mut()
                .rev()
                .map(|child| (child, child_scroll, visible, width_budget)),
        );
    }
    status = status.merge(tree::retire(root, layout, context));
    status = status.merge(tree::dispatch(tree_work, root, layout, context));
    crate::layout::collect_cache::invalidate_collect_paint_hash_spines(root, &dirty_nodes);
    status
}

/// Node-owned paint work that observes the final computed styles of its children.
#[derive(Clone)]
pub struct AfterStyleSync(Rc<dyn Fn(&mut Element, &dyn Fn(&Element) -> bool) -> bool>);

impl AfterStyleSync {
    pub fn run(&self, element: &mut Element, hovered: &dyn Fn(&Element) -> bool) -> bool {
        (self.0)(element, hovered)
    }
}

/// Append paint-only work at the end of the existing style traversal for this node.
/// The hover query is valid for this call; callbacks must not replace or reorder children.
pub fn attach_after_style(
    element: &mut Element,
    callback: impl Fn(&mut Element, &dyn Fn(&Element) -> bool) -> bool + 'static,
) {
    element.collect_paint_hash = 0;
    let previous = element.after_style_sync.take();
    element.after_style_sync = Some(AfterStyleSync(Rc::new(move |element, hovered| {
        let changed = previous
            .as_ref()
            .is_some_and(|previous| previous.run(element, hovered));
        callback(element, hovered) || changed
    })));
}
