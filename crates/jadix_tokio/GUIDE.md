# jadix_tokio

`FutureSpawner` and `BlockingExecutor` for an application-owned Tokio runtime.
Use this crate directly, or enable `jadix/runtime-tokio` and import
`jadix::app::tasks::TokioExecutor` (also available in the prelude).

```rust
use jadix_tasks::FutureSpawner;
use jadix_tokio::TokioExecutor;

let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()?;
let executor = TokioExecutor::new(runtime.handle().clone(), 32)?;
let (send, receive) = tokio::sync::oneshot::channel();
executor.spawn(Box::pin(async move {
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let _ = send.send(42);
}))?;
assert_eq!(runtime.block_on(receive)?, 42);
# Ok::<(), Box<dyn std::error::Error>>(())
```

For GUI work, submit through `TaskScope::spawn(&executor, factory, on_ui)` or
`spawn_blocking_on(&executor, work, on_ui)`. `create_async_resource` accepts an
`Arc<dyn FutureSpawner>` holding the same adapter. Construct timer/I/O futures
inside the factory; apply Signal/Store updates only in `on_ui`.

Inside an entered Tokio runtime (including an async application entry point),
`TokioExecutor::try_current(32)?` attaches to the current handle. Outside that
context it returns `Rejected`. The returned executor can be passed to the UI
thread; entering a runtime is required only to construct it with this shortcut.

The constructor's capacity bounds all accepted asynchronous and blocking work,
including tasks queued in Tokio. Clones share that limit. Full/closed adapters
return `Rejected` immediately; they do not wait for a free slot. `close()`
permanently closes admission through every clone, without cancelling accepted
tasks or shutting down Tokio. TaskScope owns cancellation of its work.

Cancellation forwards to Tokio's abort handle. Started blocking tasks remain
cooperative and keep their capacity permit until they actually return. Their
work must check the TaskScope cancellation token at safe points.

This crate enables only Tokio `rt` and `sync`. Applications select `time`, `net`,
`rt-multi-thread`, TLS and transport dependencies as needed. An executor handle
does not keep a stopped runtime running. A current-thread runtime must be driven
on a dedicated background thread for GUI use; it only polls while `block_on` runs.
Alternatively use a multi-thread runtime alongside the GUI main thread. Never
block the UI to drive or shut down a runtime. Runtime destruction drops unfinished
futures; TaskScope reports them as `Abandoned` unless already cancelled/closed.
