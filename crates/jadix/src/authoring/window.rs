//! Typed UI-thread storage partitioned by the active window and released at teardown.

use crate::app::window_scope::{self, WindowScope};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Captured window context for deferred product callbacks and cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scope(WindowScope);

impl Scope {
    pub fn capture() -> Self {
        Self(window_scope::active())
    }
    pub fn enter(self) -> impl Drop {
        window_scope::enter(self.0)
    }
}

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

type Values = HashMap<u64, Rc<dyn Any>>;
thread_local! {
    static VALUES: RefCell<HashMap<WindowScope, Values>> = RefCell::new(HashMap::new());
    static RETIRED: RefCell<HashSet<WindowScope>> = RefCell::new(HashSet::new());
}

/// A lazy typed slot. Each UI thread and window has its own value. No storage
/// borrow is held while the caller runs, so access to other slots and nested
/// access to interior-mutable values follow their ordinary Rust borrow rules.
pub struct Local<Value: 'static> {
    key: AtomicU64,
    initialize: fn() -> Value,
}

impl<Value: 'static> Local<Value> {
    pub const fn new(initialize: fn() -> Value) -> Self {
        Self {
            key: AtomicU64::new(0),
            initialize,
        }
    }

    pub fn with<Result>(&self, callback: impl FnOnce(&Value) -> Result) -> Result {
        let mut key = self.key.load(Ordering::Relaxed);
        if key == 0 {
            let allocated = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
            key = self
                .key
                .compare_exchange(0, allocated, Ordering::Relaxed, Ordering::Relaxed)
                .unwrap_or_else(|existing| existing);
            if key == 0 {
                key = allocated;
            }
        }
        let scope = window_scope::active();
        let existing = VALUES
            .try_with(|values| {
                values
                    .borrow()
                    .get(&scope)
                    .and_then(|values| values.get(&key))
                    .cloned()
            })
            .ok()
            .flatten();
        let value = if let Some(value) = existing {
            value
        } else {
            let value: Rc<dyn Any> = Rc::new((self.initialize)());
            if RETIRED
                .try_with(|retired| retired.borrow().contains(&scope))
                .unwrap_or(true)
            {
                value
            } else {
                VALUES
                    .try_with(|values| {
                        values
                            .borrow_mut()
                            .entry(scope)
                            .or_default()
                            .entry(key)
                            .or_insert_with(|| value.clone())
                            .clone()
                    })
                    .unwrap_or_else(|_| value.clone())
            }
        };
        callback(value.downcast_ref::<Value>().expect("typed window slot"))
    }
}

impl<Value: Copy + 'static> Local<Cell<Value>> {
    pub fn get(&self) -> Value {
        self.with(Cell::get)
    }
    pub fn set(&self, value: Value) {
        self.with(|slot| slot.set(value));
    }
    pub fn replace(&self, value: Value) -> Value {
        self.with(|slot| slot.replace(value))
    }
}

/// Define typed slots with the same callback shape as thread-local storage.
/// Window teardown removes values before releasing their captures. Late cleanup
/// in a retired window receives an ephemeral default and cannot resurrect it.
#[macro_export]
macro_rules! window_local {
    () => {};
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value:ty = const { $($initialize:tt)* }; $($rest:tt)*) => {
        $(#[$attribute])* $visibility static $name: $crate::authoring::window::Local<$value> =
            $crate::authoring::window::Local::new(|| { $($initialize)* });
        $crate::window_local! { $($rest)* }
    };
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value:ty = $initialize:expr; $($rest:tt)*) => {
        $(#[$attribute])* $visibility static $name: $crate::authoring::window::Local<$value> =
            $crate::authoring::window::Local::new(|| $initialize);
        $crate::window_local! { $($rest)* }
    };
}

pub use crate::window_local;

pub(crate) fn forget(scope: WindowScope) {
    RETIRED.with(|retired| retired.borrow_mut().insert(scope));
    let removed = VALUES.with(|values| values.borrow_mut().remove(&scope));
    let _scope = window_scope::enter(scope);
    drop(removed);
}

pub(crate) fn activate(scope: WindowScope) {
    RETIRED.with(|retired| retired.borrow_mut().remove(&scope));
}

#[cfg(test)]
#[path = "tests/window.rs"]
mod tests;
