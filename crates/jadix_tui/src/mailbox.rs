//! Bounded FIFO delivery using the existing runtime-neutral EventSink contract.
use crate::{wake::Wake, WireEvent};
use jadix_tasks::{EventSendError, EventSink};
use std::{
    collections::VecDeque,
    io,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub enum HostEvent<T> {
    Input(WireEvent),
    Task(T),
}
struct State<T> {
    queue: VecDeque<T>,
    closed: bool,
}
struct Shared<T> {
    state: Mutex<State<T>>,
    capacity: usize,
    wake: Wake,
}
/// One application-owned receiver, many Send producers. Full returns the
/// original payload; ordered events are never silently coalesced or dropped.
/// Capacity bounds entries, not arbitrary payload bytes: callers also budget T.
pub struct Mailbox<T> {
    shared: Arc<Shared<T>>,
}
impl<T: Send + 'static> Mailbox<T> {
    pub fn new(capacity: NonZeroUsize) -> io::Result<Self> {
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    queue: VecDeque::new(),
                    closed: false,
                }),
                capacity: capacity.get(),
                wake: Wake::new()?,
            }),
        })
    }
    pub fn sink(&self) -> EventSink<T> {
        let shared = self.shared.clone();
        EventSink::new(move |event| {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.closed {
                return Err(EventSendError::Closed(event));
            }
            if state.queue.len() == shared.capacity {
                return Err(EventSendError::Full(event));
            }
            if state.queue.is_empty() && shared.wake.notify().is_err() {
                state.closed = true;
                return Err(EventSendError::Closed(event));
            }
            state.queue.push_back(event);
            Ok(())
        })
    }
    pub fn try_recv(&self) -> Option<T> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let event = state.queue.pop_front();
        if state.queue.is_empty() && !state.closed {
            self.shared.wake.reset();
        }
        event
    }
    pub fn is_closed(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closed
    }
    pub fn close(&self) {
        let events = {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            let _ = self.shared.wake.notify();
            std::mem::take(&mut state.queue)
        };
        // Drop user payloads outside the mutex (including reentrant destructors).
        drop(events);
    }
    pub(crate) fn wake(&self) -> &Wake {
        &self.shared.wake
    }
}
impl<T> Drop for Mailbox<T> {
    fn drop(&mut self) {
        let events = {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            let _ = self.shared.wake.notify();
            std::mem::take(&mut state.queue)
        };
        drop(events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn bounded_delivery_os_wake_rearm_and_close_return_payloads() {
        let mailbox = Mailbox::new(NonZeroUsize::new(2).unwrap()).unwrap();
        let sink = mailbox.sink();
        let producer = sink.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            producer.try_send(1).unwrap();
            producer.try_send(2).unwrap();
            assert_eq!(producer.try_send(3), Err(EventSendError::Full(3)));
        });
        assert!(mailbox.wake().wait(None, Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
        assert_eq!(mailbox.try_recv(), Some(1));
        assert!(mailbox.wake().wait(None, Duration::ZERO).unwrap());
        assert_eq!(mailbox.try_recv(), Some(2));
        assert!(!mailbox.wake().wait(None, Duration::ZERO).unwrap());
        sink.try_send(3).unwrap();
        assert!(mailbox.wake().wait(None, Duration::ZERO).unwrap());
        mailbox.close();
        assert_eq!(mailbox.try_recv(), None);
        assert_eq!(sink.try_send(4), Err(EventSendError::Closed(4)));
        drop(mailbox);
        assert_eq!(sink.try_send(5), Err(EventSendError::Closed(5)));
    }
    #[test]
    fn concurrent_producers_do_not_lose_wakes_or_ordered_payloads() {
        let mailbox = Mailbox::new(NonZeroUsize::new(16).unwrap()).unwrap();
        let threads: Vec<_> = (0..4)
            .map(|producer| {
                let sink = mailbox.sink();
                std::thread::spawn(move || {
                    for sequence in 0..100 {
                        loop {
                            match sink.try_send((producer, sequence)) {
                                Ok(()) => break,
                                Err(EventSendError::Full(_)) => std::thread::yield_now(),
                                _ => panic!("receiver closed early"),
                            }
                        }
                    }
                })
            })
            .collect();
        let mut next = [0; 4];
        for _ in 0..400 {
            assert!(mailbox.wake().wait(None, Duration::from_secs(2)).unwrap());
            let (producer, sequence) = mailbox.try_recv().unwrap();
            assert_eq!(sequence, next[producer]);
            next[producer] += 1;
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(next, [100; 4]);
    }
}
