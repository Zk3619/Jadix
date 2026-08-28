//! Minimal `Future::poll` loop for wgpu bootstrap / offscreen setup (replaces `pollster`).

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake};

struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(self: Arc<Self>) {}
    fn wake_by_ref(self: &Arc<Self>) {}
}

/// Block the current thread until `future` completes.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Arc::new(NoopWaker).into();
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
