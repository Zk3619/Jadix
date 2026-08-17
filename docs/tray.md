# System tray（WP-3.6）

RUI 提供 [`TrayIcon`](../crates/rui/src/app/tray.rs)。默认构建为 **stub**（`show` / `hide` 无原生副作用）；启用 feature 后接入 [`tray-icon`](https://crates.io/crates/tray-icon)（菜单走其 re-export 的 muda）。

```rust,ignore
use rui::app::{request_exit, tray_icon, TrayMenuItem};

let tray = tray_icon()
    .tooltip("My App")
    .menu_item(TrayMenuItem::new("ping", "Ping").on_click(|| { /* UI 线程 */ }))
    .menu_item(TrayMenuItem::new("quit", "Quit").on_click(|| request_exit()))
    .build();
tray.show();
// 句柄必须保持存活，否则 Drop 会移除原生图标。
```

## Feature

默认 **不** 拉取 `tray-icon`，以免增大默认体积：

```toml
rui = { version = "0.1", features = ["system-tray"] }
```

Gallery 演示：

```text
cargo run -p gallery --features system-tray
```

侧栏 **导航 → 系统托盘（外壳）** → Show tray。右键图标：Ping（toast）/ Quit（[`request_exit`](../crates/rui/src/app/ui_bridge.rs)）。

`TrayIcon::show` / `hide` 在 stub 下为空操作；`system_tray_native()` 可在运行时判断是否已接线。

可选 `.icon_rgba(rgba, width, height)`；未设置时原生路径使用内置 32×32 蓝点。

## 线程

原生菜单事件来自托盘线程。框架已把点击 hop 到 UI 线程再调 `on_click`，因此回调里可以直接 `Signal::set` / `toast_push` / `request_exit`。**不要**把 `Signal` 标 `Send` 或在回调里再开一套 Owner。

## 平台依赖

| 能力 | crate | 备注 |
|------|--------|------|
| 托盘图标 + 菜单 | `tray-icon` 0.21（`default-features = false`） | Windows / macOS 可直接用；Linux 仍需 gtk / appindicator 系统库 |

Linux 若开启本 feature，需安装发行版的 gtk3 / libayatana-appindicator（或 libappindicator）开发包。默认 CI / `cargo check -p gallery` **不**开此 feature。

## 退出

[`request_exit`](../crates/rui/src/app/ui_bridge.rs) 向事件循环投递 `Exit`，走与关闭主窗相同的 `shutdown_renderer` + `event_loop.exit()`。事件循环未启动时仅 warn。

关主窗或 Quit 时，`shutdown_renderer` 会先释放原生托盘，避免句柄活在 `thread_local` 里时于 TLS 析构期再碰回调表（`AccessError` → abort）。
