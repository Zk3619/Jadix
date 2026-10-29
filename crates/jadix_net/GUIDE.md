# Jadix 网络适配契约

沿用 `jadix_media` 的分层：**框架统一请求/响应、命令/事件及 UI 状态，应用选用第三方网络库并实现适配器**。这里没有 HTTP 客户端实现，也没有 Reqwest、TLS、Tokio 或全局网络服务。默认仅依赖本地 std-only `jadix_tasks`，共享取消与任务句柄。

| 媒体边界 | 网络边界 |
| --- | --- |
| `MediaBackend` | `HttpBackend` |
| `MediaCommand` / `MediaEvent` | `HttpCommand` / `HttpEvent` |
| `MediaSnapshot` | `HttpSnapshot` |
| `MediaController` | 可选 `jadix::net::HttpController` |
| 外部解码、设备与时钟 | 外部 HTTP、TLS、运行时与实际 I/O |

## 启用方式

```toml
[dependencies]
jadix = { version = "0.1", features = ["net"] }
# 需要 JSON 编解码时，改用 features = ["net-json"]。
```

独立适配器可以只依赖 `jadix_net = "0.1"`，不链接 GUI。`jadix_net/json` 只引入可选的 Serde 编解码；既不联网，也不引入异步运行时。未启用 `jadix/net` 时，GUI 核心完全不依赖该 crate。

## 请求与 UI 状态

以下代码在应用的 page/window owner 下运行，命令接收端交给应用自己的适配器：

```rust,ignore
use jadix::prelude::*;
use std::{sync::mpsc, time::Duration};

let (commands, receiver) = mpsc::channel::<HttpCommand>();
let controller = HttpController::new(
    Signal::new(HttpSnapshot::default()),
    move |command| commands.send(command).map_err(|_| {
        HttpError::new(HttpErrorKind::Disconnected, "adapter disconnected")
    }),
);

let request_id = controller.send(
    HttpRequest::get("https://api.example.com/users")
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(10))
        .max_response_bytes(1024 * 1024),
)?;

let events = controller.event_sink();
// 应用将 receiver 和 events 交给自己的第三方库适配器；框架不会消费 receiver。
// 后端任务完成时调用 events(HttpEvent::new(request_id, kind))。
// events 可跨线程克隆，内部只投递事件，不在工作线程访问 Signal。
```

`send` **不阻塞，也不是 async 网络调用**：返回的 `u64` 是已接受请求的 token，不是响应。一个 controller 对应一个可替换的请求槽及独立的命令/事件通道；多项并发请求使用多个 controller。token 只在所属通道内有意义，不得将多个 controller 的事件混入同一未分流通道。

`HttpRequest::new(HttpMethod::Put, url)` 等支持 GET / POST / PUT / PATCH / DELETE / HEAD / OPTIONS。`header` 追加重复项；值为原始字节，响应可保留重复 `Set-Cookie` 和非 UTF-8 头值。`body` 接受共享的 `Arc<[u8]>` 或 `Vec<u8>`。请求只做基本 URL/头值准入校验，完整 URL 解析和协议行为仍属于第三方后端。

```rust,ignore
let snapshot = controller.snapshot();
let state = snapshot.get(); // 在 UI 响应式上下文读取
match state.state {
    HttpState::Pending => { /* loading + state.upload / state.download */ }
    HttpState::Completed => {
        let response = state.response.unwrap().error_for_status()?;
        let text = response.text()?;
    }
    HttpState::Failed => { /* state.error */ }
    HttpState::Cancelled => { /* 已取消 */ }
    HttpState::Idle => {}
}

controller.cancel()?;
```

`Completed` 表示收到合法且未超限的完整响应，HTTP 404/500 不是传输失败；用 `is_success` / `error_for_status` 显式决定业务行为。`text` 严格按 UTF-8 解码。启用 JSON 后可用 `HttpRequest::post(url).json(&payload)?` 和 `response.json::<User>()?`；JSON 解码本身不检查 HTTP 状态。

## 适配器责任

除既有 command/poll 外，可使用事件驱动的 `HttpStarter::start(request_id, request, sink, cancellation) -> Result<TaskHandle, HttpError>`。`HttpDispatcher` 负责命令转换、接纳新任务后取消旧任务、永久 Shutdown；`HttpController::with_backend(snapshot, Arc<dyn HttpStarter>)` 自动连接 typed sink 与 owner 生命周期。

start 只接纳工作，实际执行由后端决定。返回的 TaskHandle 用于绑定原生 abort；也可以共享 CancellationToken 做合作式取消。逻辑取消不代表同步 read 已立即停止。`HttpEventSink::try_send` 返回 Full/Closed 时，后端必须处理，不能把可靠终态静默丢弃。详细异步方案见[完整架构](ASYNC_ARCHITECTURE.md)。

`HttpBackend` 仅提供两个非阻塞方法：

```rust
use jadix_net::{HttpBackend, HttpCommand, HttpError, HttpEvent};

fn pump_on_ui(backend: &mut dyn HttpBackend, mut publish: impl FnMut(HttpEvent)) {
    while let Some(event) = backend.poll_event() {
        publish(event);
    }
}
```

应用也可以像媒体库一样直接连接命令闭包与事件 callback，不必套一层 trait 对象。`examples/queue_adapter.rs` 展示无网络依赖的通道适配结构；真正的第三方库集成留在应用或独立适配器 crate。

适配器必须满足以下约定：

- `dispatch` 只校验/入队，不在 UI 线程连接、等待 Future 或执行阻塞 HTTP。返回 `Err` 表示未接受工作；运行中的失败通过 `HttpEventKind::Failed` 回传。
- 接受新的 `Send` 后取消该通道旧任务；若新请求被拒绝，旧任务保持不变。为每个任务捕获其原始 `request_id`，不得在任务完成时读取最新 token 重新贴标。
- `Cancel` 幂等，过期 token 不影响新任务；确认停止后可发 `Cancelled`。`Shutdown` 幂等且永久关闭该通道，取消所有任务并释放资源；收到后拒绝后续 `Send`。资源回收不能阻塞 UI。
- 由第三方适配器从接纳请求时起使用单调时钟执行总超时，包含后端排队、连接、重定向及接收响应体，不在每个阶段重新计时；无法支持的策略必须以 `Unsupported` 拒绝。仅过滤旧事件不等于取消了网络任务。
- 在读取和解压过程中限制响应体，不能等分配完整大响应后才检查。默认 30 秒、8 MiB；设为 0 字节的上限表示只接受空响应体。snapshot 的完成事件校验只是最后一道防线。
- 以实际传输的 body 字节报告上传/下载进度，不伪造已完成上传。transferred 与 total 必须使用同一字节口径，不把压缩 Content-Length 与解压后计数混用。未知总量使用 `None`，已知总量不得小于已完成量；每个方向的进度不可回退。建议在适配器内合并为 100 ms 一次，完成/错误立即发出，避免灌满 UI 队列。
- HTTP/TLS、证书、代理、cookies、redirect、缓存、鉴权及重试策略由应用和第三方库负责；不要默认重试非幂等请求。不要在错误消息中泄露 URL 凭据、query、Authorization 或 body。这里的 Debug 隐去 URL、头值和正文。

## 生命周期和会话隔离

在 owner 下创建并持有 controller，不要在每次渲染中重新创建。`event_sink()` 复用既有 `run_on_ui_thread` 队列，controller 本身不可跨线程移动。已在 UI 线程的事件可直接 `publish`。

新请求在派发前清除旧响应、错误和进度并建立新 token 边界。clone 共享递增计数器；被拒绝的请求仍消耗 token，`u64` 耗尽返回错误而不回绕。派发拒绝且状态尚未变化时恢复旧 snapshot；同步确认或更新请求不会被旧失败回滚。

取消在派发前先阻止本次迟到响应；取消命令被拒绝且状态未被更新时恢复原状态。终态不再接受事件；旧 token、未来 token 及重复进度不会触发通知或重绘。`request_error()` 存放命令拒绝错误，`snapshot.error` 存放执行错误。

owner 销毁时关闭事件入口并尽力派发 `Shutdown`，即使仍持有 controller 的克隆或 queued callback 也不能复活 UI。适配器仍由应用持有、驱动和回收；`Shutdown` 入队失败会记录日志，不能声称框架已经终止底层网络。只丢弃某个 controller 克隆不关闭通道。

`remote-asset` / `RemoteAssetFetcher` 保持原有注入方式，不会因启用 `net` 自动联网或安装全局 fetcher。浏览器适配器自行处理 CORS、受限 header 和浏览器取消机制；这里不承诺已提供 WASM transport。WebSocket、SSE、无限流、大文件落盘、请求缓存和声明式 query 不在本次 HTTP 有限响应契约中。

## 验证

```text
cargo test -p jadix_net --no-default-features
cargo test -p jadix_net --all-features
cargo test -p jadix --features net-json --test net
cargo test -p jadix --features net-json --lib net::tests
cargo tree -p jadix_net --no-default-features -e normal
```

所有测试均使用本地命令/事件 fixture，不访问外网；最后一条默认仅输出 `jadix_net` 与本地 std-only `jadix_tasks`，不包含第三方网络或运行时依赖。
