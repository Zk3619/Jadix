//! Typed parent/child communication across independently mounted roots.
use super::WindowContent;
use crate::reactivity::{current_owner, on_cleanup};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::{Rc, Weak},
};

const MESSAGE_LIMIT: usize = 1024;
thread_local! {
    static NEXT_SESSION: Cell<u64> = const { Cell::new(1) };
    static SESSIONS: RefCell<BTreeMap<u64, Weak<dyn Any>>> = RefCell::new(BTreeMap::new());
}

struct Session<Args, Message, Result> {
    args: Args,
    parent_messages: RefCell<VecDeque<Message>>,
    child_messages: RefCell<VecDeque<Message>>,
    result: RefCell<Option<Result>>,
    attached: Cell<bool>,
    closed: Cell<bool>,
    parent_alive: Cell<bool>,
}

/// A session belongs to its creating reactive owner. A child factory receives
/// the opaque argument from `content` and attaches to the exact same Rust types.
/// Queued delivery lets each root consume messages outside the other root's
/// callback and avoids reentrant UI borrows.
pub struct WindowSession<Args: 'static, Message: 'static, Result: 'static> {
    id: u64,
    session: Rc<Session<Args, Message, Result>>,
}

pub struct WindowSessionChild<Args, Message, Result> {
    session: Rc<Session<Args, Message, Result>>,
}

impl<A: 'static, M: 'static, R: 'static> WindowSession<A, M, R> {
    pub fn new(args: A) -> Self {
        assert!(
            current_owner().is_some(),
            "WindowSession requires a reactive owner"
        );
        let id = NEXT_SESSION.with(|next| {
            let id = next.get();
            next.set(
                id.checked_add(1)
                    .expect("window session identity exhausted"),
            );
            id
        });
        let session = Rc::new(Session {
            args,
            parent_messages: RefCell::new(VecDeque::new()),
            child_messages: RefCell::new(VecDeque::new()),
            result: RefCell::new(None),
            attached: Cell::new(false),
            closed: Cell::new(false),
            parent_alive: Cell::new(true),
        });
        let erased: Rc<dyn Any> = session.clone();
        SESSIONS.with(|sessions| sessions.borrow_mut().insert(id, Rc::downgrade(&erased)));
        let weak = Rc::downgrade(&session);
        on_cleanup(move || {
            SESSIONS.with(|sessions| sessions.borrow_mut().remove(&id));
            if let Some(session) = weak.upgrade() {
                session.parent_alive.set(false);
                session.closed.set(true);
                session.parent_messages.borrow_mut().clear();
                session.child_messages.borrow_mut().clear();
                session.result.borrow_mut().take();
            }
        });
        Self { id, session }
    }

    pub fn content(&self, factory: impl Into<String>) -> WindowContent {
        WindowContent {
            factory: factory.into(),
            argument: Some(format!("jadix-session:{}", self.id)),
        }
    }

    pub fn attach(argument: &str) -> std::result::Result<WindowSessionChild<A, M, R>, String> {
        if current_owner().is_none() {
            return Err("window session attachment requires a reactive owner".into());
        }
        let id = argument
            .strip_prefix("jadix-session:")
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or("invalid window session argument")?;
        let erased = SESSIONS
            .with(|sessions| sessions.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or("window session expired")?;
        let session = erased
            .downcast::<Session<A, M, R>>()
            .map_err(|_| "window session type mismatch")?;
        if !session.parent_alive.get() || session.closed.get() {
            return Err("window session is closed".into());
        }
        if session.attached.replace(true) {
            return Err("window session already has a child".into());
        }
        let weak = Rc::downgrade(&session);
        on_cleanup(move || {
            if let Some(session) = weak.upgrade() {
                session.closed.set(true);
                session.child_messages.borrow_mut().clear();
            }
        });
        Ok(WindowSessionChild { session })
    }

    pub fn send(&self, message: M) -> std::result::Result<(), String> {
        self.session.enqueue(&self.session.child_messages, message)
    }
    pub fn try_recv(&self) -> Option<M> {
        self.session.parent_messages.borrow_mut().pop_front()
    }
    pub fn try_result(&self) -> Option<R> {
        self.session.result.borrow_mut().take()
    }
    pub fn is_closed(&self) -> bool {
        self.session.closed.get()
    }
}

impl<A, M, R> Session<A, M, R> {
    fn enqueue(&self, queue: &RefCell<VecDeque<M>>, message: M) -> std::result::Result<(), String> {
        if self.closed.get() || !self.parent_alive.get() {
            return Err("window session is closed".into());
        }
        let mut queue = queue.borrow_mut();
        if queue.len() >= MESSAGE_LIMIT {
            return Err("window session message limit exceeded".into());
        }
        queue.push_back(message);
        Ok(())
    }
}

impl<A, M, R> WindowSessionChild<A, M, R> {
    pub fn args(&self) -> &A {
        &self.session.args
    }
    pub fn send(&self, message: M) -> std::result::Result<(), String> {
        self.session.enqueue(&self.session.parent_messages, message)
    }
    pub fn try_recv(&self) -> Option<M> {
        self.session.child_messages.borrow_mut().pop_front()
    }
    pub fn finish(&self, result: R) -> std::result::Result<(), String> {
        if !self.session.parent_alive.get() || self.session.closed.replace(true) {
            return Err("window session is closed".into());
        }
        *self.session.result.borrow_mut() = Some(result);
        self.session.child_messages.borrow_mut().clear();
        Ok(())
    }
    pub fn is_closed(&self) -> bool {
        self.session.closed.get()
    }
}
