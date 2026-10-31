# 桌面窗口、全屏与进程实例

## 全屏

Windows / Linux 默认使用 F11 切换当前窗口的无边框全屏；Windows 还支持 Alt+Enter。
macOS 使用 Control+Command+F，保留系统 F11 行为。Esc 只在全屏时退出全屏；如果当前窗口有可关闭的顶层弹层，先关闭弹层，再次 Esc 才退出全屏。
按住按键不会反复切换；附加修饰键不匹配上述快捷键。主窗、次窗均使用同一事件入口，只操作收到按键的窗口。原生 WebView 未截获的窗口键盘事件也走这个入口。

- 创建时：`.fullscreen(true)`。
- 当前窗口：`set_window_fullscreen(bool)` / `toggle_window_fullscreen()` / `window_is_fullscreen()`。
- 指定窗口：`WindowHandle::set_fullscreen` / `toggle_fullscreen` / `is_fullscreen`。
- 禁用默认快捷键：builder / `WindowOptions` 的 `.fullscreen_shortcuts(false)`，或 `<App fullscreen_shortcuts={false}>`；运行期用 `handle.set_fullscreen_shortcuts(false)`。

全屏不改变应用配置中的最大化、大小和位置；退出全屏的几何恢复交给 winit/窗口管理器。不会把原生全屏状态写回 RSX 请求值，因此普通响应式重建不会撤销用户的全屏切换。Web/移动端不安装桌面快捷键。

## 单例和多开

默认 `InstancePolicy::Multiple`，允许多进程启动。
需要单例时，在创建事件循环之前通过入口 builder 或 `run_with` 的 `WindowOptions` 设置稳定应用 ID：

```rust
use jadix::prelude::*;

fn main() {
    app()
        .title("Editor")
        .single_instance("org.example.editor:default-profile")
        .on_second_instance(|launch| {
            println!("args: {:?}, cwd: {:?}", launch.args, launch.working_directory);
        })
        .mount(|| Element::new().text("Editor"));
}
```

`InstanceLaunch.args` 包含可执行文件参数（argv[0]），保留 OS 字符串；工作目录独立传送，应用解释相对路径时应使用该目录。
第二次启动不创建窗口/GPU：已有进程接收参数，显示、取消最小化并请求聚焦主窗；主窗已关闭时选一个存活次窗，然后在该窗口作用域调用 UI 线程回调。
同一应用的不同配置可使用不同 ID 多开，也可显式选择 `.instance_policy(InstancePolicy::Multiple)`。不要使用会变化的标题或安装路径作为 ID。
RSX `<App>` 在窗口创建后构建，不能设置进程单例策略；纯 RSX 应用也应在 `main` 中先使用 builder 的 `run` 或 `mount` 设置该策略。

实现使用 OS 文件锁、仅监听 127.0.0.1 的动态 TCP 端口、系统随机认证令牌以及有界消息；不增加第三方依赖。
按用户隔离，元数据位于 Windows 的 `%LOCALAPPDATA%/jadix/instances` 或 Unix 的 `$HOME/.cache/jadix/instances`（目录 0700、文件 0600）。
锁文件保留但不包含进程存活标志；操作系统会在崩溃/强杀后释放锁，后续启动覆盖旧端点。
参数上限 64 KiB，等待实例就绪最多 3 秒；消息确认表示已入 UI 队列，不表示文档处理已经完成。
转交失败记录错误并停止本次启动，不会静默降级为多开。当前入口 API 返回 `()`，不提供启动失败退出码；需要此能力时应扩展有返回值的启动接口。
单例仅用于 Windows/macOS/Linux 桌面；不支持的平台显式报错。macOS Finder 的 open-document/reopen 事件不等于新进程启动，此处不替代原生文档事件适配。

## 次窗独立存活与优先级

```rust
use jadix::prelude::*;

fn open_tool() {
    open_secondary_window_with(
        WindowOptions::new()
            .title("工具窗口")
            .size(600.0, 400.0)
            .close_with_primary(false)
            .always_on_top(true),
        || Element::new().text("主窗关闭后依然可用"),
    ).unwrap();
}
```

- 默认 `close_with_primary(true)` 保留兼容行为：主窗关闭时连带关闭。
- `false`：主窗的原生窗口和 UI 树会被销毁，次窗保留自己的 UI、输入、交换链和状态；共享 GPU 上下文与字体系统保留到最后窗口退出，不保留隐藏主窗。
- 运行时 `handle.set_close_with_primary(bool)` 可切换依赖关系；`closes_with_primary()` 查询当前策略。主窗自己的此字段不参与关闭判断。
- 主窗关闭后可以继续创建次窗，不重新创建主窗。关掉最后一个窗口退出事件循环。
- `request_exit()` 是应用级 Quit：始终检查所有窗口，包括独立次窗。
- `WindowLifecycleHook::on_close_requested` 在任何目标拆除前检查全部待关闭窗口，任一否决则本次不关闭任何窗口；独立次窗不参与主窗关闭的否决，但参与 Quit。原生 Destroyed 无法否决，仍遵守依赖关系并在最后窗口销毁时退出。
- `always_on_top(true)` / `handle.set_always_on_top(true)` 使用原生全局置顶，使次窗优先于普通窗口；设为 false 恢复普通层级。这不是仅高于主窗的 owner/modal 关系，不锁定主窗输入，也不主动循环抢焦点。聚焦用 `handle.focus()`。焦点和置顶最终受桌面窗口管理器策略约束，Wayland 尤其如此。
- `WindowHandle` 是弱句柄，保存它不会使已关闭窗口存活；关闭后方法返回错误。所有 UI 树/Signal 操作仍应在 UI 线程进行。

独立次窗不依赖主窗的响应式根执行。捕获到次窗的业务状态必须拥有应用级生命周期；窗口局部组件 Signal 不应当被当作跨窗口业务存储。现有 `shared_slot` 或应用级状态适合跨窗协作。

## 类型化窗口会话

`WindowSession<Args, Message, Result>` 从 `jadix::app` 或 prelude 导入，只能在 UI 线程创建/使用，不需要序列化或额外依赖。

```rust
use jadix::prelude::*;

fn open_editor() -> Result<(), String> {
    let session = WindowSession::<String, String, String>::new("document.rs".into());
    session.on_child_message(|message| println!("child: {message}"));
    session.on_closed(|event| println!("result: {:?}", event.result));
    session.open(WindowOptions::new().title("Editor"), |context| {
        let text = use_signal("editor-text", || context.args().clone());
        context.on_parent_message(move |message| { text.try_set(message); });
        Element::new().text(context.args().clone()).on_click(move || {
            let _ = context.close_with_result(text.peek());
        })
    })
}
```

- `args()` 提供启动参数；`parent()`、`window()` 返回弱句柄，创建未完成时 `window()` 为 None。`on_opened` 在窗口就绪后通知父窗。
- 父窗安装 `on_child_message`，子窗安装 `on_parent_message`；用 `send_to_parent` / `send_to_child` 发送。消息 FIFO 入队，在后续事件循环中、接收方窗口作用域执行，避免输入处理重入。未就绪、接收器未安装或已关闭时返回错误；关闭后未投递消息取消。
- `close_with_result(value)` 请求关闭并保存结果；真正关闭后 `on_closed` 至多调用一次，包含父/子弱句柄及可选结果。原生关闭通常为 None；创建失败也通知，子窗句柄为 None。每个 session 只能 open 一次，保留 session 不会保活原生窗口。
- 父窗已退出时不要更新已销毁组件状态，可用 `Signal::try_set` 或应用级存储。回调避免强引用捕获同一 session，防止自行形成 Rc 引用环。仍遵守关闭否决和 `close_with_primary` 策略；不是跨进程 IPC。

## Gallery 交互展示

运行 `cargo run -p gallery`，打开侧栏「导航 / 窗口」，提供：

- **主次窗传参 / 双向通信 / 关闭回传**：读取启动参数；主窗发送消息更新次窗输入框；次窗回复主窗，或将输入框内容作为结果关闭；原生关闭显示“无返回值”。

- **桌面全屏**：切换、退出、读取原生状态，以及按窗口启用或禁用全屏快捷键；主窗和次窗都可操作。
- **单例与多开（进程）**：启动单例 A、使用不同应用 ID 的单例 B、或允许多开的新进程。重复点击同一单例按钮，观察原窗口激活、PID 保持不变、接收次数递增，以及参数与工作目录回传。演示进程使用独立入口，不写 Gallery 主窗的几何持久化文件。
- **多窗口与生命周期**：打开跟随次窗、独立次窗和独立置顶次窗；在次窗内可运行时切换跟随策略、取消或启用置顶、仅关闭本窗或退出整个进程。

也可直接启动精简演示窗口：

```powershell
cargo run -p gallery -- --gallery-instance-demo=single
cargo run -p gallery -- --gallery-instance-demo=profile
cargo run -p gallery -- --gallery-instance-demo=multiple
```

验证独立存活：在演示进程中打开一个跟随次窗和一个独立次窗，再点击「关闭主窗（验证独立存活）」。跟随次窗关闭，独立次窗保留；可以继续创建新窗口。若该进程为单例，再次启动相同单例会激活存活次窗，并在该窗口显示启动参数。关闭最后一个窗口结束进程。

