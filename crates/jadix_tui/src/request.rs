//! Request revisions and cooperative cancellation, independent of an executor.
use jadix_tasks::{CancellationToken, EventSendError, EventSink};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

struct State {
    generation: u64,
    closed: bool,
    cancellation: CancellationToken,
}
/// One UI-thread-owned (!Send/!Sync) replaceable resource request. Drop closes
/// the slot and cancels its work; Send tickets can be given to any executor.
/// Use independent slots for independent resources; no global host is installed.
pub struct RequestSlot(Arc<Mutex<State>>, PhantomData<Rc<()>>);
#[derive(Clone)]
pub struct RequestTicket {
    state: Arc<Mutex<State>>,
    generation: u64,
    cancellation: CancellationToken,
}
pub struct TaskUpdate<T> {
    pub ticket: RequestTicket,
    pub value: T,
}
impl Default for RequestSlot {
    fn default() -> Self {
        Self(
            Arc::new(Mutex::new(State {
                generation: 0,
                closed: false,
                cancellation: CancellationToken::new(),
            })),
            PhantomData,
        )
    }
}
impl RequestSlot {
    pub fn accepts(&self, ticket: &RequestTicket) -> bool {
        Arc::ptr_eq(&self.0, &ticket.state) && ticket.is_current()
    }
    pub fn replace(&self) -> std::io::Result<RequestTicket> {
        let (previous, ticket) = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if s.closed {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            s.generation = s
                .generation
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("request generation exhausted"))?;
            let previous = std::mem::replace(&mut s.cancellation, CancellationToken::new());
            let ticket = RequestTicket {
                state: self.0.clone(),
                generation: s.generation,
                cancellation: s.cancellation.clone(),
            };
            (previous, ticket)
        };
        previous.cancel();
        Ok(ticket)
    }
    pub fn cancel(&self) {
        let token = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancellation
            .clone();
        token.cancel();
    }
    pub fn close(&self) {
        let token = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            s.closed = true;
            s.cancellation.clone()
        };
        token.cancel();
    }
}
impl Drop for RequestSlot {
    fn drop(&mut self) {
        self.close();
    }
}
impl RequestTicket {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    /// Check again when applying on the UI thread: a queued result can become
    /// obsolete after admission. Use `RequestSlot::accepts` to check destination
    /// identity too; generation numbers from different slots are not identities.
    pub fn is_current(&self) -> bool {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        !s.closed && s.generation == self.generation && !self.cancellation.is_cancelled()
    }
    pub fn publish<T>(
        &self,
        sink: &EventSink<TaskUpdate<T>>,
        value: T,
    ) -> Result<(), EventSendError<TaskUpdate<T>>> {
        let update = TaskUpdate {
            ticket: self.clone(),
            value,
        };
        if !self.is_current() {
            return Err(EventSendError::Closed(update));
        }
        sink.try_send(update)
    }
}
