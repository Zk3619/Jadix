//! Retained cross-tree operations owned by a mounted node.

use super::{Context, Phase};
use crate::app::window_scope::{self, WindowScope};
use crate::core::reactivity::{current_owner, on_cleanup, with_owner, OwnerId};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::LayoutTree;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

type Callback = Rc<dyn Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus>;

pub(super) struct Operation {
    callback: RefCell<Option<Callback>>,
    cleanup: Option<Callback>,
    phase: Phase,
    owner: Option<OwnerId>,
    window: WindowScope,
}

#[derive(Clone)]
pub struct TreeSync(Rc<Operation>);

/// Attach retained reparenting or sibling presentation that requires the entire
/// window tree, including hoisted portals. Operations run after node callbacks
/// in this phase. They may move existing nodes, but must not remount the host.
/// A prepared operation expires when its node or Owner is removed.
pub fn attach_tree(
    element: &mut Element,
    phase: Phase,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
) {
    attach_tree_impl(element, phase, Rc::new(callback), None);
}

/// Cleanup runs once in the owning window's next matching paint phase after
/// removal or Owner disposal. It may restore retained paint, without Signals or
/// remounting. Closing the window releases the cleanup without executing it.
pub fn attach_tree_with_cleanup(
    element: &mut Element,
    phase: Phase,
    callback: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
    cleanup: impl Fn(&mut Element, &mut LayoutTree, Context) -> LayoutStatus + 'static,
) {
    attach_tree_impl(element, phase, Rc::new(callback), Some(Rc::new(cleanup)));
}

fn attach_tree_impl(
    element: &mut Element,
    phase: Phase,
    callback: Callback,
    cleanup: Option<Callback>,
) {
    let operation = Rc::new(Operation {
        callback: RefCell::new(Some(callback)),
        cleanup,
        phase,
        owner: current_owner(),
        window: window_scope::active(),
    });
    if operation.owner.is_some() {
        let weak = Rc::downgrade(&operation);
        on_cleanup(move || {
            if let Some(operation) = weak.upgrade() {
                let released = operation.callback.borrow_mut().take();
                drop(released);
            }
        });
    }
    element.tree_presentation_sync.push(TreeSync(operation));
    element.subtree_presentation_sync = true;
    element.collect_paint_hash = 0;
}

#[derive(Default)]
pub(crate) struct Retirements(Vec<(Weak<Operation>, Callback, Phase, WindowScope)>);

impl Retirements {
    fn track(&mut self, operation: &Rc<Operation>) {
        if let Some(cleanup) = &operation.cleanup {
            if operation.callback.borrow().is_none() {
                return;
            }
            let weak = Rc::downgrade(operation);
            if !self
                .0
                .iter()
                .any(|(current, _, _, _)| current.ptr_eq(&weak))
            {
                self.0
                    .push((weak, cleanup.clone(), operation.phase, operation.window));
            }
        }
    }

    pub(super) fn drain(&mut self, phase: Phase) -> Vec<(Callback, WindowScope)> {
        let mut ready = Vec::new();
        self.0.retain(|(operation, cleanup, target, window)| {
            if *target == phase
                && !operation
                    .upgrade()
                    .is_some_and(|operation| operation.callback.borrow().is_some())
            {
                ready.push((cleanup.clone(), *window));
                false
            } else {
                true
            }
        });
        ready
    }
}

pub(super) fn retire(
    root: &mut Element,
    layout: &mut LayoutTree,
    context: Context,
) -> LayoutStatus {
    let ready = layout.tree_retirements.drain(context.phase);
    let mut status = LayoutStatus::Clean;
    for (cleanup, window) in ready {
        let _window = window_scope::enter(window);
        status = status.merge(cleanup(root, layout, context));
    }
    status
}

pub(super) fn prepare(
    element: &Element,
    phase: Phase,
    pending: &mut Vec<Weak<Operation>>,
    layout: &mut LayoutTree,
) {
    for operation in &element.tree_presentation_sync {
        if operation.0.phase == phase {
            layout.tree_retirements.track(&operation.0);
        }
    }
    pending.extend(
        element
            .tree_presentation_sync
            .iter()
            .filter(|sync| sync.0.phase == phase)
            .map(|sync| Rc::downgrade(&sync.0)),
    );
}

pub(super) fn dispatch(
    pending: Vec<Weak<Operation>>,
    root: &mut Element,
    layout: &mut LayoutTree,
    context: Context,
) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    for operation in pending {
        let Some(operation) = operation.upgrade() else {
            continue;
        };
        let Some(callback) = operation.callback.borrow().clone() else {
            continue;
        };
        let _scope = window_scope::enter(operation.window);
        let mut run = || callback(root, layout, context);
        let changed = if let Some(owner) = operation.owner {
            with_owner(owner, run)
        } else {
            run()
        };
        status = status.merge(changed);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn reparenting_preserves_nodes_and_retires_later_removed_operations() {
        let called = Rc::new(Cell::new(false));
        let mut root = Element::new();
        let mut first = Element::new();
        attach_tree(&mut first, Phase::State, |root, _, _| {
            let retained = root.children.remove(1).children.remove(0);
            root.children[0].children.push(retained);
            LayoutStatus::StructureChanged
        });
        let mut second = Element::new();
        let later = called.clone();
        attach_tree(&mut second, Phase::State, move |_, _, _| {
            later.set(true);
            LayoutStatus::Clean
        });
        second.children.push(Element::new().text("retained"));
        root.children = vec![first, second];
        let status = super::super::sync_tree(
            &mut root,
            &mut LayoutTree::new(),
            Context::new(Phase::State, [800.0, 600.0]),
        );
        assert_eq!(status, LayoutStatus::StructureChanged);
        assert!(!called.get());
        assert_eq!(
            root.children[0].children[0].text.as_deref(),
            Some("retained")
        );
    }
    #[test]
    fn removed_and_disposed_tree_effects_restore_paint_once() {
        for dispose_owner in [false, true] {
            let calls = Rc::new(Cell::new(0));
            let cleaned = calls.clone();
            let (owner, node) = crate::core::reactivity::create_root(|| {
                let mut node = Element::new();
                attach_tree_with_cleanup(
                    &mut node,
                    Phase::BeforeLayout,
                    |root, _, _| {
                        root.style.opacity = 0.5;
                        LayoutStatus::StyleChanged
                    },
                    move |root, _, _| {
                        cleaned.set(cleaned.get() + 1);
                        root.style.opacity = 1.0;
                        LayoutStatus::StyleChanged
                    },
                );
                node
            });
            let mut root = Element::new().child(node);
            let mut layout = LayoutTree::new();
            let context = Context::new(Phase::BeforeLayout, [800.0, 600.0]);
            super::super::sync_tree(&mut root, &mut layout, context);
            assert_eq!(root.style.opacity, 0.5);
            if dispose_owner {
                owner.dispose();
            } else {
                root.children.clear();
            }
            super::super::sync_tree(&mut root, &mut layout, context);
            assert_eq!(root.style.opacity, 1.0);
            super::super::sync_tree(&mut root, &mut layout, context);
            assert_eq!(calls.get(), 1);
        }
    }
}
