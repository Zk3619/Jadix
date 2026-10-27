# Jadix 后台执行契约

仅依赖 Rust 标准库的公共接口，被 GUI 与网络适配器共享；不安装线程池、executor、reactor、定时器或全局运行时。设计与完整职责划分见 [异步架构](../../docs/async-architecture.md)。

## 公共 API

| API | 约定 |
| --- | --- |
| `CancellationToken` | clone 共享取消状态；`checkpoint()` 支持合作式取消 |
| `CancellationToken::on_cancel` | 对接后端原生 abort；取消前后注册都执行一次，回调不持锁 |
| `CancellationToken::link_handle` | 将外部任务句柄绑定到逻辑取消，避免同一 token 自捕获 |
| `TaskHandle` | clone 共享取消控制，不是 join handle；drop 某个 clone 不取消 |
| `BlockingExecutor` | 非阻塞接纳 `BlockingTask`，由应用线程池实际执行 |
| `FutureSpawner` | 非阻塞接纳 `BoxTask`，由兼容的外部 runtime poll |
| `EventSink<E>` | 类型安全的非阻塞入口；`Full(E)` / `Closed(E)` 交回未接纳的数据 |

执行器也可直接使用满足对应签名的 `Send + Sync` 闭包。两类 trait 均可作为 trait object 使用。`Err(TaskError)` 表示没有接纳任务；执行器不得一边返回拒绝、一边运行任务。接纳后必须执行或释放任务，不能永久遗忘。

## 取消与错误

取消首先是逻辑状态，不保证杀死 OS 线程、终止数据库事务或回滚外部副作用。阻塞任务在安全检查点退出；具备原生 abort 的后端用 `TaskHandle::with_abort` 绑定原生句柄。abort 与取消回调必须非阻塞、不可 panic、可从任意取消线程调用。

```rust
use jadix_tasks::{CancellationToken, TaskHandle};

let token = CancellationToken::new();
let handle = TaskHandle::cooperative(token.clone());
assert!(token.checkpoint().is_ok());
assert!(handle.cancel());
assert!(!handle.cancel());
assert!(token.checkpoint().is_err());
```

`TaskErrorKind` 区分 `Rejected`、`Failed`、`Cancelled`、`Panicked`、`Abandoned`。领域错误无需都转成字符串：任务可以返回 `Ok(Result<T, DomainError>)`，由 UI 单独处理执行错误与业务错误。不要把 URL 凭据、请求正文、SQL 参数或 panic payload 原文塞进通用诊断。

## GUI 接入

`jadix::app::TaskScope` 绑定现有 UI owner，负责回投、关闭入口、取消、窗口/owner 恢复及重绘；它本身不可跨线程移动。

```rust,ignore
use jadix::prelude::*;

let scope = TaskScope::new();
let text = Signal::new(String::new());
let task = scope.spawn_blocking(
    move |cancel| {
        cancel.checkpoint()?;
        std::fs::read_to_string("config.toml")
            .map_err(|error| TaskError::new(TaskErrorKind::Failed, error.to_string()))
    },
    move |result| {
        if let Ok(value) = result { text.set(value); }
    },
)?;
```

该便利方法每次创建一个线程，仅适合偶发工作；大量任务用 `spawn_blocking_on(&pool, work, on_ui)`。Future 用 `scope.spawn(&spawner, factory, on_ui)`；factory 在运行时内调用，不能将需要运行时上下文的初始化提前放在 UI 线程执行。

```rust,ignore
let executor = jadix_tokio::TokioExecutor::new(runtime.handle().clone(), 64)?;
scope.spawn(&executor, factory, on_ui)?;
```

官方 [`jadix_tokio`](../jadix_tokio/README.md) 同时实现 FutureSpawner 与 BlockingExecutor，
异步/阻塞任务共享容量上限，满载立即拒绝。GUI 可选 `runtime-tokio` feature，
从 `jadix::app::tasks` 或 prelude 导入 TokioExecutor。无需自行重复编写适配器。

应用必须维持 runtime 的寿命、IO/timer driver 和匹配后端的运行环境。句柄不拥有 runtime；runtime 停止后接纳的 Future 可能被直接 drop，GUI 会把未完成任务标记为 `Abandoned`，不会伪称成功。这里不提供隐藏的 `block_on`。

## 验证

```text
cargo test -p jadix_tasks
cargo tree -p jadix_tasks -e normal
cargo test -p jadix --features net-json --lib app::tasks::tests
```
