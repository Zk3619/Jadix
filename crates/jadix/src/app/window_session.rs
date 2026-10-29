//! Typed UI-thread sessions for secondary-window arguments, messages and return values.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use super::multi_window::{current_window, open_secondary_window_with, WindowHandle};
use super::window_chrome::WindowOptions;
use super::window_scope::{self, WindowScope};
use crate::core::tree::Element;

thread_local! {
    static TASKS: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
}

fn post(task: impl FnOnce() + 'static) {
    TASKS.with(|tasks| tasks.borrow_mut().push_back(Box::new(task)));
    super::ui_bridge::wake();
}

pub(crate) fn drain_tasks() -> usize {
    let tasks: Vec<_> = TASKS.with(|tasks| {
        let mut tasks = tasks.borrow_mut();
        let count = tasks.len().min(super::ui_bridge::UI_TASK_BATCH);
        tasks.drain(..count).collect()
    });
    let count = tasks.len();
    for task in tasks {
        crate::core::reactivity::with_ui_build_stack(task);
    }
    if TASKS.with(|tasks| !tasks.borrow().is_empty()) {
        super::ui_bridge::wake();
    }
    count
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    New,
    Pending,
    Open,
    Closed,
}

/// Delivered once after actual teardown, including native Close, parent shutdown,
/// and creation failure. A native close has no result unless one was supplied.
pub struct WindowClosed<Result> {
    pub parent: Option<WindowHandle>,
    pub window: Option<WindowHandle>,
    pub result: Option<Result>,
}

type MessageHandler<Message> = Rc<RefCell<Box<dyn FnMut(Message)>>>;
type ClosedHandler<Result> = Box<dyn FnOnce(WindowClosed<Result>)>;

struct SessionState<Args, Message, Result> {
    args: Args,
    parent: Option<WindowHandle>,
    parent_scope: WindowScope,
    window: RefCell<Option<WindowHandle>>,
    status: Cell<Status>,
    to_parent: RefCell<Option<MessageHandler<Message>>>,
    to_child: RefCell<Option<MessageHandler<Message>>>,
    opened: RefCell<Option<Box<dyn FnOnce(WindowHandle)>>>,
    closed: RefCell<Option<ClosedHandler<Result>>>,
    result: RefCell<Option<Result>>,
}

/// A cloneable, UI-thread-only link between a parent and one secondary window.
/// Messages are FIFO and delivered on a later event-loop turn in the receiver's
/// window scope, never while an input handler is borrowing the sender's tree.
pub struct WindowSession<Args: 'static, Message: 'static = (), Result: 'static = ()> {
    inner: Rc<SessionState<Args, Message, Result>>,
}

impl<Args, Message, Result> Clone for WindowSession<Args, Message, Result> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Args, Message, Result> WindowSession<Args, Message, Result> {
    pub fn new(args: Args) -> Self {
        Self {
            inner: Rc::new(SessionState {
                args,
                parent: current_window(),
                parent_scope: window_scope::active(),
                window: RefCell::new(None),
                status: Cell::new(Status::New),
                to_parent: RefCell::new(None),
                to_child: RefCell::new(None),
                opened: RefCell::new(None),
                closed: RefCell::new(None),
                result: RefCell::new(None),
            }),
        }
    }

    pub fn args(&self) -> &Args {
        &self.inner.args
    }

    pub fn parent(&self) -> Option<WindowHandle> {
        self.inner.parent.clone()
    }

    pub fn window(&self) -> Option<WindowHandle> {
        self.inner
            .window
            .borrow()
            .clone()
            .filter(WindowHandle::is_alive)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.status.get() == Status::Closed
    }

    pub fn on_opened(&self, handler: impl FnOnce(WindowHandle) + 'static) {
        *self.inner.opened.borrow_mut() = Some(Box::new(handler));
    }

    pub fn on_closed(&self, handler: impl FnOnce(WindowClosed<Result>) + 'static) {
        *self.inner.closed.borrow_mut() = Some(Box::new(handler));
    }

    pub fn on_parent_message(&self, handler: impl FnMut(Message) + 'static) {
        *self.inner.to_child.borrow_mut() = Some(Rc::new(RefCell::new(Box::new(handler))));
    }

    pub fn on_child_message(&self, handler: impl FnMut(Message) + 'static) {
        *self.inner.to_parent.borrow_mut() = Some(Rc::new(RefCell::new(Box::new(handler))));
    }

    pub fn send_to_parent(&self, message: Message) -> std::result::Result<(), String> {
        self.send(message, false)
    }

    pub fn send_to_child(&self, message: Message) -> std::result::Result<(), String> {
        self.send(message, true)
    }

    fn send(&self, message: Message, to_child: bool) -> std::result::Result<(), String> {
        if self.inner.status.get() != Status::Open {
            return Err("window session is not open".into());
        }
        let handler = if to_child {
            &self.inner.to_child
        } else {
            &self.inner.to_parent
        }
        .borrow()
        .clone()
        .ok_or("window message handler is not installed")?;
        let session = self.clone();
        post(move || {
            if session.is_closed() {
                return;
            }
            let _scope = if to_child {
                let Some(window) = session.window() else {
                    return;
                };
                window_scope::enter_secondary_id(window.id())
            } else {
                if session
                    .inner
                    .parent
                    .as_ref()
                    .is_some_and(|parent| !parent.is_alive())
                {
                    return;
                }
                window_scope::enter(session.inner.parent_scope)
            };
            (handler.borrow_mut())(message);
        });
        Ok(())
    }

    pub fn open(
        &self,
        options: WindowOptions,
        mut builder: impl FnMut(Self) -> Element + 'static,
    ) -> std::result::Result<(), String> {
        if self.inner.status.get() != Status::New {
            return Err("a window session can only be opened once".into());
        }
        self.inner.status.set(Status::Pending);
        let lifetime = SessionLifetime(self.clone());
        open_secondary_window_with(options, move || {
            let session = lifetime.session();
            if session.inner.status.get() == Status::Pending {
                if let Some(window) = current_window() {
                    *session.inner.window.borrow_mut() = Some(window.clone());
                    session.inner.status.set(Status::Open);
                    let callback = session.inner.opened.borrow_mut().take();
                    let scope = session.inner.parent_scope;
                    if let Some(callback) = callback {
                        post(move || {
                            let _scope = window_scope::enter(scope);
                            callback(window);
                        });
                    }
                }
            }
            builder(session.clone())
        })
    }

    pub fn close_with_result(&self, result: Result) -> std::result::Result<(), String> {
        let window = self.window().ok_or("window session is not open")?;
        window.close()?;
        *self.inner.result.borrow_mut() = Some(result);
        Ok(())
    }

    fn finish(&self) {
        if self.inner.status.replace(Status::Closed) == Status::Closed {
            return;
        }
        let callback = self.inner.closed.borrow_mut().take();
        let event = WindowClosed {
            parent: self.parent(),
            window: self.inner.window.borrow_mut().take(),
            result: self.inner.result.borrow_mut().take(),
        };
        if let Some(callback) = callback {
            let scope = self.inner.parent_scope;
            post(move || {
                let _scope = window_scope::enter(scope);
                callback(event);
            });
        }
    }
}

struct SessionLifetime<Args: 'static, Message: 'static, Result: 'static>(
    WindowSession<Args, Message, Result>,
);

impl<Args, Message, Result> SessionLifetime<Args, Message, Result> {
    fn session(&self) -> &WindowSession<Args, Message, Result> {
        &self.0
    }
}

impl<Args, Message, Result> Drop for SessionLifetime<Args, Message, Result> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

#[cfg(test)]
#[path = "tests/window_session.rs"]
mod tests;
