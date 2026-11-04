use crate::ShellError;
use jadix_core::{
    presentation::RootLifetime,
    schema::{Schema, Value},
    Signal,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub session: String,
    pub sequence: String,
}

type EventCallback = Rc<RefCell<Box<dyn FnMut(Value)>>>;

enum Binding {
    State {
        signal: Signal<Value>,
        schema: Schema,
    },
    Event {
        callback: EventCallback,
        schema: Schema,
    },
}

struct Entry {
    lifetime: RootLifetime,
    binding: Binding,
    active: Rc<Cell<bool>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.active.set(false);
    }
}

pub struct EventDelivery {
    lifetime: RootLifetime,
    callback: EventCallback,
    arguments: Value,
    active: Rc<Cell<bool>>,
}

impl EventDelivery {
    pub fn dispatch(self) -> Result<(), ShellError> {
        if self.lifetime.is_disposed() || !self.active.get() {
            return Err(ShellError::StaleHandle);
        }
        let mut callback = self
            .callback
            .try_borrow_mut()
            .map_err(|_| ShellError::ReentrantCallback)?;
        callback(self.arguments);
        Ok(())
    }
}

pub(crate) struct HandleTable {
    session: String,
    next: u64,
    entries: BTreeMap<String, Entry>,
    limit: usize,
}

impl HandleTable {
    pub fn new(session: String, limit: usize) -> Self {
        Self {
            session,
            next: 0,
            entries: BTreeMap::new(),
            limit,
        }
    }

    fn insert(&mut self, lifetime: RootLifetime, binding: Binding) -> Result<Handle, ShellError> {
        self.entries
            .retain(|_, entry| !entry.lifetime.is_disposed());
        if self.entries.len() >= self.limit {
            return Err(ShellError::HandleLimit);
        }
        self.next = self.next.checked_add(1).ok_or(ShellError::HandleLimit)?;
        let sequence = self.next.to_string();
        self.entries.insert(
            sequence.clone(),
            Entry {
                lifetime,
                binding,
                active: Rc::new(Cell::new(true)),
            },
        );
        Ok(Handle {
            session: self.session.clone(),
            sequence,
        })
    }

    pub fn state(
        &mut self,
        lifetime: RootLifetime,
        signal: Signal<Value>,
        schema: Schema,
    ) -> Result<Handle, ShellError> {
        if !schema.is_valid()
            || signal
                .try_peek()
                .is_none_or(|value| !schema.accepts(&value))
        {
            return Err(ShellError::InvalidArguments);
        }
        self.insert(lifetime, Binding::State { signal, schema })
    }

    pub fn event(
        &mut self,
        lifetime: RootLifetime,
        schema: Schema,
        callback: impl FnMut(Value) + 'static,
    ) -> Result<Handle, ShellError> {
        if !schema.is_valid() {
            return Err(ShellError::InvalidArguments);
        }
        self.insert(
            lifetime,
            Binding::Event {
                callback: Rc::new(RefCell::new(Box::new(callback))),
                schema,
            },
        )
    }

    fn entry(&self, handle: &Handle) -> Result<&Entry, ShellError> {
        if handle.session != self.session {
            return Err(ShellError::StaleHandle);
        }
        self.entries
            .get(&handle.sequence)
            .filter(|entry| !entry.lifetime.is_disposed())
            .ok_or(ShellError::StaleHandle)
    }

    pub fn read(&self, handle: &Handle) -> Result<Value, ShellError> {
        let Binding::State { signal, .. } = &self.entry(handle)?.binding else {
            return Err(ShellError::InvalidHandleKind);
        };
        signal.try_peek().ok_or(ShellError::StaleHandle)
    }

    pub fn write(&self, handle: &Handle, value: Value) -> Result<(), ShellError> {
        let Binding::State { signal, schema } = &self.entry(handle)?.binding else {
            return Err(ShellError::InvalidHandleKind);
        };
        if !schema.accepts(&value) {
            return Err(ShellError::InvalidArguments);
        }
        if signal.try_set(value) {
            Ok(())
        } else {
            Err(ShellError::StaleHandle)
        }
    }

    pub fn delivery(&self, handle: &Handle, arguments: Value) -> Result<EventDelivery, ShellError> {
        let entry = self.entry(handle)?;
        let Binding::Event { callback, schema } = &entry.binding else {
            return Err(ShellError::InvalidHandleKind);
        };
        if !schema.accepts(&arguments) {
            return Err(ShellError::InvalidArguments);
        }
        Ok(EventDelivery {
            lifetime: entry.lifetime.clone(),
            callback: callback.clone(),
            arguments,
            active: entry.active.clone(),
        })
    }

    pub fn release(&mut self, handle: &Handle) -> Result<(), ShellError> {
        self.entry(handle)?;
        self.entries.remove(&handle.sequence);
        Ok(())
    }

    pub fn check_event(
        &self,
        handle: &Handle,
        root: crate::presentation::RootId,
        schema: &Schema,
    ) -> Result<(), ShellError> {
        let entry = self.entry(handle)?;
        let Binding::Event {
            schema: registered, ..
        } = &entry.binding
        else {
            return Err(ShellError::InvalidHandleKind);
        };
        if entry.lifetime.id() != root {
            return Err(ShellError::StaleHandle);
        }
        if registered != schema {
            return Err(ShellError::InvalidArguments);
        }
        Ok(())
    }

    pub fn prune(&mut self) {
        self.entries
            .retain(|_, entry| !entry.lifetime.is_disposed());
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
