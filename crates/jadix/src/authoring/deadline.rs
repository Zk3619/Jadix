//! Node-owned deadline work, indexed weakly by the window during existing layout phases.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Instant;

use crate::app::window_scope::{self, WindowScope};
use crate::core::reactivity::{current_owner, on_cleanup, with_owner, OwnerId};
use crate::core::tree::{Element, LayoutStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Always,
    /// Pause outside the viewport or while the owning window cannot present.
    Visible,
}

#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub now: Instant,
    /// First poll after mounting or resuming visible work; discard overdue catch-up.
    pub resumed: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Poll {
    pub redraw: bool,
    pub next_wake: Option<Instant>,
}

type Callback = Rc<dyn Fn(Context) -> Poll>;

struct NodeDeadline {
    callback: RefCell<Option<Callback>>,
    owner: Option<OwnerId>,
    window: WindowScope,
    visibility: Visibility,
}

/// Poll mounted deadline work outside all host tree borrows, in its Owner and window scope.
/// Return no deadline when idle. Polling does not itself wake the loop or keep frames live.
/// Node removal, Owner disposal, and window destruction retire queued work and its captures.
pub fn attach(
    element: &mut Element,
    visibility: Visibility,
    callback: impl Fn(Context) -> Poll + 'static,
) {
    let task = Rc::new(NodeDeadline {
        callback: RefCell::new(Some(Rc::new(callback))),
        owner: current_owner(),
        window: window_scope::active(),
        visibility,
    });
    if task.owner.is_some() {
        let weak = Rc::downgrade(&task);
        on_cleanup(move || {
            if let Some(task) = weak.upgrade() {
                let callback = task.callback.borrow_mut().take();
                drop(callback);
            }
        });
    }
    super::presentation::attach_observer(
        element,
        super::presentation::Phase::AfterLayout,
        move |_, layout, _, visible| {
            layout.deadlines.track(&task, visible);
            LayoutStatus::Clean
        },
    );
}

struct Entry {
    task: Weak<NodeDeadline>,
    frame: u64,
    visible: bool,
    was_active: bool,
}

pub(crate) struct WindowDeadlines {
    frame: u64,
    entries: HashMap<usize, Entry>,
    lifetime: Rc<Cell<bool>>,
}

impl Default for WindowDeadlines {
    fn default() -> Self {
        Self {
            frame: 0,
            entries: HashMap::new(),
            lifetime: Rc::new(Cell::new(true)),
        }
    }
}

impl WindowDeadlines {
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn track(&mut self, task: &Rc<NodeDeadline>, visible: bool) {
        if task.window != window_scope::active() {
            return;
        }
        let entry = self
            .entries
            .entry(Rc::as_ptr(task) as usize)
            .or_insert_with(|| Entry {
                task: Rc::downgrade(task),
                frame: self.frame,
                visible,
                was_active: false,
            });
        entry.frame = self.frame;
        entry.visible = visible;
    }

    pub(crate) fn prepare(&mut self, can_present: bool) -> Vec<Pending> {
        let mut pending = Vec::new();
        self.entries.retain(|_, entry| {
            let Some(task) = entry.task.upgrade() else {
                return false;
            };
            if task.callback.borrow().is_none() {
                return false;
            }
            let active = entry.frame == self.frame
                && (task.visibility == Visibility::Always || (entry.visible && can_present));
            if active {
                pending.push(Pending {
                    task: entry.task.clone(),
                    window: Rc::downgrade(&self.lifetime),
                    resumed: !entry.was_active,
                });
            }
            entry.was_active = active;
            true
        });
        pending
    }
}

impl Drop for WindowDeadlines {
    fn drop(&mut self) {
        self.lifetime.set(false);
        let callbacks: Vec<_> = std::mem::take(&mut self.entries)
            .into_values()
            .filter_map(|entry| entry.task.upgrade())
            .filter_map(|task| task.callback.borrow_mut().take())
            .collect();
        drop(callbacks);
    }
}

pub(crate) struct Pending {
    task: Weak<NodeDeadline>,
    window: Weak<Cell<bool>>,
    resumed: bool,
}

impl Pending {
    pub(crate) fn dispatch(self, now: Instant) -> Poll {
        let Some(window) = self.window.upgrade().filter(|window| window.get()) else {
            return Poll::default();
        };
        let _window = window;
        let Some(task) = self.task.upgrade() else {
            return Poll::default();
        };
        let callback = task.callback.borrow().clone();
        let Some(callback) = callback else {
            return Poll::default();
        };
        let _scope = window_scope::enter(task.window);
        let run = || {
            callback(Context {
                now,
                resumed: self.resumed,
            })
        };
        task.owner.map_or_else(run, |owner| with_owner(owner, run))
    }
}

#[cfg(test)]
#[path = "tests/deadline.rs"]
mod tests;
