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
#[path = "tests/tree.rs"]
mod tests;
