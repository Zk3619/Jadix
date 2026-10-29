# Jadix 异步架构与网络契约

基础接口、owner 桥接、可选 Tokio 适配及远程编码字节缓存可用；完整资源 registry、调度策略及协议适配器的范围见实现矩阵。



## 1. 分层与依赖

```text
UI 主线程：事件处理 / 响应式状态 / 布局 / 渲染
    ↑ 有界订阅邮箱 / 一次性完成回投 / EventLoopProxy 唤醒
jadix::app::TaskScope                 jadix::net::HttpController [net 可选]
    ↓                                   ↓
jadix_tasks ←──────────────────────── jadix_net
    ↑                                   ↑
应用线程池 / FutureSpawner          应用 HttpStarter / HttpBackend
    ↓                                   ↓
文件 / 数据库 / CPU 工作           第三方 HTTP / TLS / runtime
```

- `jadix_tasks` 默认仅依赖标准库，统一取消、任务句柄、执行器和事件入口。GUI 和独立网络适配器共享它，而不是相互依赖。
- `jadix_net` 默认只有这一项本地依赖，不依赖 GUI、Reqwest、Tokio 或 TLS。JSON 仍单独可选。
- `jadix/net` 与 `net-json` 默认关闭；启用网络不安装全局客户端、图片 fetcher 或运行时。
- 不把布局、绘制、组件构建、Signal 更新改成 async；UI 回调只做短时状态应用。
- 应用决定执行器配置与寿命。框架不尝试用一个通用 poll 循环替代第三方 IO/timer driver。

## 2. 公共任务契约

| 对象 | 实现与约束 |
| --- | --- |
| `CancellationToken` | 共享逻辑取消；检查点与原生 abort hook；取消回调在锁外执行 |
| `TaskHandle` | clone 共享取消；drop 不隐式取消；不代表 OS 任务已经终止 |
| `BlockingExecutor` | 接纳闭包到外部池；满载/关闭立即拒绝，不等待队列空位 |
| `FutureSpawner` | 接纳 Send Future 到外部 runtime；由该 runtime 负责唤醒与 IO |
| `EventSink<E>` | 类型化事件入口，不使用 `Any`；拒绝时归还 payload |
| `TaskScope` | owner 级 UI 桥接；关闭后禁止新任务和迟到回投 |

`spawn_blocking_on` 与 `spawn` 的返回值只表示执行器接纳；`on_ui(Result<T, TaskError>)` 表示执行结果。拒绝只通过返回值报告，不再调用 `on_ui`。任务开始前检查取消；取消后不调用结果回调，界面需要即时取消状态时应在自己的取消处理器中更新。

`spawn` 接收 `FnOnce(CancellationToken) -> Future` 工厂，而非强迫 UI 提前构造依赖运行时的 Future。任务/结果/回调要求 `Send + 'static`；UI 只能在回投回调里访问 Signal。回调捕获 Signal ID 不代表可以在 worker 上读取 Signal：状态存储仍属于 UI 线程。

偶发阻塞工作可用 `TaskScope::spawn_blocking`，每次一个命名原生线程；它不是有界线程池。需要并发上限、优先级、CPU/IO 隔离时注入外部 `BlockingExecutor`。已有自由函数 `app::spawn_blocking` / `run_on_ui_thread` 保持兼容，是低层、不绑定 owner 的接口；新业务优先使用 `TaskScope`。全局 UI 回投队列仍不是有界消息系统：高并发一次性任务须在外部执行器限制接纳量，高频事件用有界订阅，不能把“单订阅有界”误解成全应用内存自动有界。

## 3. 生命周期、并发与失败

1. 在页面/窗口稳定 owner 下创建 TaskScope，而不是在每次 render 中创建。
2. 提交工作时登记域内 token，完成/取消后移除，不积累一次性任务历史。
3. owner 销毁或 `scope.close()`：先永久关入口，再在锁外取消未完成任务。
4. 完成事件在入 UI 队列前、UI 执行前两次检查域与取消状态；已排队不代表一定更新。
5. UI 回调恢复提交域的 window scope 和 owner，因此新建 Signal/任务归属正确，重绘指向原窗口。不会借用“此刻恰好激活”的另一个窗口。
6. 已结束任务排队的回调仍可被该任务 handle 显式取消；owner 关闭也可屏蔽它。
7. 取消回调、后端 start、原生 abort 不得阻塞、panic 或 join 后台线程。应用不得在 UI 线程 drop 一个会等待 worker 退出的 runtime。

阻塞任务在 unwind 模式下的 panic 映射为 `Panicked`，不转发可能敏感的 panic payload；进程使用 panic=abort 时无法恢复。外部执行器释放尚未完成的闭包/Future 时映射为 `Abandoned`。Future panic 通常由外部 runtime 捕获并释放，GUI 此时只能观测 Abandoned，不能凭空区分 panic 与 runtime 停止。若执行器永久遗忘任务，框架无法自行检测或回收它。

取消是合作式/尽力而为，不承诺强杀线程、即时打断同步 read、撤销已写文件或回滚已提交事务。关闭 UI 和释放执行资源是不同的保证：UI 可以立即停止接收，底层阻塞调用可能要等超时或检查点。

## 4. 一次性任务

适用：文件读写、查询、解码、字体解析、大文件索引、复杂计算、压缩、导出。

```rust,ignore
let scope = TaskScope::new();
let task = scope.spawn_blocking_on(pool.as_ref(), move |cancel| {
    cancel.checkpoint()?;
    let result = parse_in_chunks(path, &cancel)?;
    Ok(result)
}, move |result| {
    output.set(result);
})?;
```

- 文件与解析按块检查取消；返回摘要/分页数据，避免把整个大文件搬进 UI。
- 写文件的原子替换、事务和失败回滚是领域后端职责，不由任务域假装保证。
- 非 Send 数据库连接在专用 worker 内创建和拥有，只跨线程传命令和普通数据。
- 图片/字体的读取、CPU 解码与解析在后台；GPU 上传、字体注册和必要的重新布局留在 UI/渲染线程。
- CPU 高并发和长期阻塞 IO 不共占同一个小型池；具体线程数、调度、优先级由执行器配置，不固化到 UI API。
- 任务进度使用类型化 EventSink/订阅，不再为每个业务引入一套 progress dispatcher；百分比未知时保留已完成计数和未知总量，不伪造数值。

## 5. 持续事件源与背压

`TaskScope::subscribe(capacity, start, on_ui)` 实现统一入口。start 接收 `EventSink<E>`、CancellationToken，立即返回 TaskHandle；可以启动线程、外部 runtime 或仅注册系统回调。start 本身不能执行监听循环。

```rust,ignore
let subscription = scope.subscribe(
    128,
    move |sink, cancellation| device_backend.start(sink, cancellation),
    move |event: DeviceEvent| update_device_ui(event),
)?;
```

订阅邮箱的具体保证：

- FIFO、容量大于零、非阻塞发送；满载返回 `Full(event)`，关闭返回 `Closed(event)`。
- 同一订阅最多挂一个 UI drain；每轮最多处理 64 个，剩余数据续投下一轮，避免持续生产者吞掉输入/绘制机会。
- 调用用户回调时不持队列锁或 callback 锁，允许回调取消自身；关闭会释放未交付数据。
- 容量以事件数计，不是字节数；事件/帧大小和 producer 侧缓冲也必须受限。一个巨大的事件仍可占用大量内存。
- 可靠命令、协议帧、终态不得静默丢弃。producer 在自己的执行环境暂停/重试，或定义领域级断开策略；UI 线程禁止阻塞等待容量。
- 传感器最新值/进度可由适配器显式合并；日志可采用领域明确的丢旧策略。基础邮箱不偷偷决定丢什么，也没有自动 async ready 通知。
- 自然结束、失败、重连状态都由领域 enum 显式表达；所有 producer sink 与待消费批次释放后自动移除域内登记。需要提前终止或后端继续保留 sink 时显式取消句柄。没有内置 WebSocket/SSE/串口状态机。

对于 WebSocket、串口、USB、IPC：输入事件与输出命令分离；后端拥有 socket/设备句柄，UI 只持非阻塞命令入口。帧顺序、心跳、重连、设备权限和流量控制属于后端。不要用有限 HTTP 的 Completed(body) 模拟无限流。

## 6. 资源值与资源管线

沿用现有 `Resource<T>`、`ResourceState<T>`、Suspense、ErrorBoundary，不创建平行的 AssetState 供现有 UI 混用。

| API | 用途 |
| --- | --- |
| `create_resource(source, fetcher)` | 保持现有签名，偶发同步加载；现在通过 TaskScope 管生命周期 |
| `create_resource_on(source, executor, fetcher)` | 外部池 + 可取消同步加载 |
| `create_async_resource(source, spawner, fetcher)` | 外部兼容 runtime + 可取消 Future 加载 |

source 改变：推进单调 generation → 取消旧任务 → Loading 并保留旧 Ready 值 → 提交新工作 → 仅最新 generation 可完成。generation 耗尽失败，不回绕。执行器拒绝或未完成任务被释放进入 Err，不永久悬在 Loading。后台任务的取消用于 refetch/owner 关闭；Resource 公共状态保持原有 Loading/Ready/Err，不为了已销毁 UI 添加无消费者的 Cancelled 状态。

兼容 `create_resource` 的 fetcher 签名不接收 token：已经开始的阻塞函数不能由框架强行打断，只会屏蔽结果。需要分块检查取消时使用 `create_resource_on`；需要原生 abort 时由外部执行器句柄提供。

Resource 目前不是通用资产缓存。后续需要跨控件共享时，在资源层增加独立的键化 registry，而非改变 HTTP 后端或 renderer：

```text
request(key) -> 命中缓存：共享 Arc 值
             -> 命中 in-flight：新增订阅者
             -> 新任务：读取/下载 -> CPU 转换 -> UI/GPU 安装 -> 通知
release(subscription) -> 最后订阅者离开才取消共享加载
```

键必须包含内容版本/指纹、目标尺寸、解码选项及租户/权限等实际影响内容的维度；不能把带敏感参数的 URL 当可公开日志 key。失败不永久负缓存；重试由显式策略触发。缓存需容量/字节预算、淘汰、版本失效；GPU 对象按设备与窗口需要区分，不能凭一个路径跨设备复用。优先级与离屏预取由可见性/资源层提出，交给可支持的执行器处理。

完整 registry、最后订阅者取消、解码/GPU 总预算与优先级仍是设计边界，尚未实现。
可选 `remote-asset-cache` 已提供 RemoteAssetFetcher 编码字节缓存、同 URL 下载合并、TTL 及 payload 字节权重，
Image/Avatar 通过共享字节入口使用它；见[共享远程资源缓存](remote-assets.md)。它不会自动安装，也不替代现有 CPU/GPU 缓存。

## 7. 网络契约优化

保留媒体风格 `HttpBackend::dispatch/poll_event`，不强迫现有适配器重写。新增 push seam：

```rust
use jadix_net::{CancellationToken, HttpError, HttpEventSink, HttpRequest, HttpStarter, TaskHandle};

fn start(
    backend: &dyn HttpStarter,
    request_id: u64,
    request: HttpRequest,
    events: HttpEventSink,
    cancellation: CancellationToken,
) -> Result<TaskHandle, HttpError> {
    backend.start(request_id, request, events, cancellation)
}
```

`HttpDispatcher` 将它接回 Send / Cancel / Shutdown 单槽命令协议；`HttpController::with_backend(snapshot, Arc<dyn HttpStarter>)` 提供直接 GUI 接入。无需框架轮询线程，不需要网络 crate 了解窗口。

核心语义：

- start 非阻塞；Err 未接纳、不发事件；Ok 之后运行失败通过 Failed 事件回传。同步/线程池/异步 runtime 后端统一此语义。
- 接纳新任务后才取消旧任务；拒绝保留旧任务。token 严格递增，取消后也不复用；多并发请求用多个槽，或在应用管理多个 dispatcher。
- dispatcher 为每个工作创建独立 token，绑定返回的原生 TaskHandle；旧请求取消不影响新请求。owner 销毁永久 Shutdown；重入 shutdown 会在正在进行的派发返回时补发，不遗留新接纳工作。
- sink 保留原始 request_id；取消后和错误 id 的事件被拒绝；GUI snapshot 再做 generation/终态校验。每个已接纳且未取消请求最终只发一个 Completed/Failed 终态，backend 自行保证。
- HTTP 完整响应仍受 request.timeout 和 max_response_bytes 约束。后端从接纳时起用单调时钟覆盖排队、连接、重定向和收完整 body 的总期限，不逐阶段重置；读取/解压期间限制交付的 body 字节数，进度与 total 使用一致的字节口径。snapshot 仅做最后校验，取消 hook 不能替代总超时。
- `HttpController::backend_event_sink` 可向后端返回 Closed；兼容 `event_sink()` 仍是回调形式。两者不提供 body-stream 背压；进度必须在后端合并，单个完整响应有大小上限。
- HTTP 4xx/5xx 仍是 Completed，不与传输失败混淆；JSON、text 解码、error_for_status 保持显式。
- TLS、证书、代理、cookie、redirect、鉴权、连接池、自动重试仍由外部库/应用决定；不得默认重试有副作用的请求。

第一阶段只承诺有限 buffered HTTP + 上传/下载进度。文件落盘下载需要后端受控路径与流量/磁盘策略；不得为了“统一 API”先把大文件完整载入内存。SSE/WebSocket 属于持续源，不在当前 HttpResponse 契约中。

## 8. 定时器、平台与关停

- 定时任务可由外部 runtime 产生 Source 事件；动画续帧继续使用现有帧调度/失效系统，不能用网络计时器驱动布局。
- 不提供隐藏 `block_on`，不在渲染/事件回调里 poll 任意 Future，不创建隐式 Tokio runtime。
- 目前通用 Future seam 要求 Send；未实现 !Send `spawn_local`、UI executor 或浏览器本地任务桥接。不能用 `unsafe impl Send` 掩盖不兼容。
- std-only 的任务契约本身不使用线程；`TaskScope::spawn_blocking` 的原生线程便利路径不承诺在浏览器可用。WASM 适配需显式设计本地 spawner、浏览器限制与取消，不因有 trait 就宣布全平台支持。
- 退出顺序：关闭各 owner scope/网络槽 → 停止新提交 → 请求取消/原生 abort → 清理适配器 → 在非 UI 阻塞路径回收外部 worker/runtime。运行时最终由应用关闭。

## 9. 实现矩阵与验证

| 范围 | 状态 |
| --- | --- |
| std-only 公共取消、TaskHandle、执行器、EventSink | 已实现 |
| owner/window 恢复、一次性任务、可取消 Future 工厂 | 已实现 |
| 有界 FIFO 订阅、Full/Closed、UI 分批消费 | 已实现 |
| Resource 取消旧加载、外部池、Future 加载 | 已实现 |
| HTTP push seam、原生 abort 绑定、命令适配 | 已实现 |
| 可选 Tokio Future/Blocking 适配与共享接纳上限 | 已实现：`jadix_tokio` / `runtime-tokio`，应用持有 runtime |
| 可选远程编码字节缓存与同 URL 下载合并 | 已实现：`remote-asset-cache`，应用显式安装 |
| 外部线程池、第三方 runtime / HTTP / TLS | 由应用或独立适配器提供 |
| 完整共享资产 registry、解码/GPU 总预算、优先级 | 已设计边界，未实现 |
| WS/SSE/串口/USB/IPC 协议、重连与心跳 | 已设计接入，未实现后端 |
| !Send / spawn_local、浏览器运行桥接 | 未实现，不承诺 |

测试分层与命令见[测试规范](testing.md)；本页定义消费者的公共契约与可用性边界。
