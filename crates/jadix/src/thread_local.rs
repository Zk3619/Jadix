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
mod tests {
    use super::*;

    thread_local! {
        static FIRST: Cell<usize> = const { Cell::new(0) };
        static SECOND: RefCell<Vec<usize>> = RefCell::new(vec![FIRST.get()]);
        static PANICS_ONCE: usize = {
            assert!(FIRST.replace(1) != 0, "initialization fixture panic");
            42
        };
    }

    #[test]
    fn nested_initialization_and_access_release_registry_borrows() {
        FIRST.set(7);
        SECOND.with_borrow_mut(|values| {
            values.push(FIRST.get());
            FIRST.set(9);
        });
        SECOND.with_borrow(|values| assert_eq!(values, &[7, 7]));
        assert_eq!(FIRST.take(), 9);
    }

    #[test]
    fn values_are_isolated_and_dropped_per_thread() {
        static DROPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        struct Dropped;
        impl Drop for Dropped {
            fn drop(&mut self) {
                DROPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        thread_local! { static RESOURCE: Dropped = Dropped; }
        FIRST.set(17);
        std::thread::spawn(|| {
            assert_eq!(FIRST.get(), 0);
            FIRST.set(23);
            RESOURCE.with(|_| {});
        })
        .join()
        .unwrap();
        assert_eq!(FIRST.get(), 17);
        assert_eq!(DROPS.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_initializer_can_be_retried() {
        FIRST.set(0);
        assert!(std::panic::catch_unwind(|| PANICS_ONCE.with(|_| {})).is_err());
        assert_eq!(PANICS_ONCE.with(|value| *value), 42);
    }

    #[test]
    fn many_keys_share_one_registry() {
        let keys: Vec<_> = (0..512)
            .map(|_| Box::leak(Box::new(LocalKey::new(|| Cell::new(0)))))
            .collect();
        for (index, key) in keys.into_iter().enumerate() {
            key.set(index);
            assert_eq!(key.get(), index);
        }
    }
}
