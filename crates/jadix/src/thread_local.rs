use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

enum Entry {
    Initializing,
    Ready(Rc<dyn Any>),
}

std::thread_local! {
    static VALUES: RefCell<HashMap<usize, Entry>> = RefCell::new(HashMap::new());
}

pub(crate) struct LocalKey<Value: 'static> {
    initialize: fn() -> Value,
    marker: PhantomData<fn() -> Value>,
}

struct Initialization(usize);

impl Drop for Initialization {
    fn drop(&mut self) {
        let _ = VALUES.try_with(|values| {
            let mut values = values.borrow_mut();
            if matches!(values.get(&self.0), Some(Entry::Initializing)) {
                values.remove(&self.0);
            }
        });
    }
}

impl<Value: 'static> LocalKey<Value> {
    pub(crate) const fn new(initialize: fn() -> Value) -> Self {
        Self {
            initialize,
            marker: PhantomData,
        }
    }

    pub(crate) fn try_with<ResultValue>(
        &'static self,
        callback: impl FnOnce(&Value) -> ResultValue,
    ) -> Result<ResultValue, std::thread::AccessError> {
        let key = self as *const Self as usize;
        let existing = VALUES.try_with(|values| {
            let mut values = values.borrow_mut();
            match values.get(&key) {
                Some(Entry::Ready(value)) => Some(value.clone()),
                Some(Entry::Initializing) => panic!("recursive thread-local initialization"),
                None => {
                    values.insert(key, Entry::Initializing);
                    None
                }
            }
        })?;
        let value = if let Some(value) = existing {
            value
        } else {
            let guard = Initialization(key);
            let value: Rc<dyn Any> = Rc::new((self.initialize)());
            VALUES.try_with(|values| {
                values.borrow_mut().insert(key, Entry::Ready(value.clone()));
            })?;
            drop(guard);
            value
        };
        Ok(callback(
            value.downcast_ref().expect("thread-local type mismatch"),
        ))
    }

    pub(crate) fn with<ResultValue>(
        &'static self,
        callback: impl FnOnce(&Value) -> ResultValue,
    ) -> ResultValue {
        self.try_with(callback)
            .expect("thread-local value accessed during destruction")
    }
}

impl<Value> LocalKey<Cell<Value>> {
    pub(crate) fn get(&'static self) -> Value
    where
        Value: Copy,
    {
        self.with(Cell::get)
    }
    pub(crate) fn set(&'static self, value: Value) {
        self.with(|cell| cell.set(value));
    }
    pub(crate) fn replace(&'static self, value: Value) -> Value {
        self.with(|cell| cell.replace(value))
    }
    pub(crate) fn take(&'static self) -> Value
    where
        Value: Default,
    {
        self.with(Cell::take)
    }
}

impl<Value> LocalKey<RefCell<Value>> {
    pub(crate) fn with_borrow<ResultValue>(
        &'static self,
        callback: impl FnOnce(&Value) -> ResultValue,
    ) -> ResultValue {
        self.with(|cell| callback(&cell.borrow()))
    }
    pub(crate) fn with_borrow_mut<ResultValue>(
        &'static self,
        callback: impl FnOnce(&mut Value) -> ResultValue,
    ) -> ResultValue {
        self.with(|cell| callback(&mut cell.borrow_mut()))
    }
}

macro_rules! thread_local {
    () => {};
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value_type:ty = const { $($initialize:tt)* }; $($rest:tt)*) => {
        $(#[$attribute])*
        $visibility static $name: $crate::thread_local::LocalKey<$value_type> =
            $crate::thread_local::LocalKey::new(|| { $($initialize)* });
        thread_local! { $($rest)* }
    };
    ($(#[$attribute:meta])* $visibility:vis static $name:ident: $value_type:ty = $initialize:expr; $($rest:tt)*) => {
        $(#[$attribute])*
        $visibility static $name: $crate::thread_local::LocalKey<$value_type> =
            $crate::thread_local::LocalKey::new(|| $initialize);
        thread_local! { $($rest)* }
    };
}

#[cfg(test)]
#[path = "tests/thread_local.rs"]
mod tests;
