# System tray（阶段十一）

RUI 提供 [`TrayIcon`](../crates/rui/src/app/tray.rs) **stub API**：

```rust,ignore
use rui::app::{tray_icon, TrayMenuItem};

let tray = tray_icon()
    .tooltip("My App")
    .menu_item(TrayMenuItem::new("quit", "Quit").on_click(|| { /* … */ }))
    .build();
tray.show();
```

## 平台依赖说明

真实托盘需要额外 crate（建议后续接入）：

| 能力 | 候选 crate | 备注 |
|------|------------|------|
| 托盘图标 | `tray-icon` | 跨 Windows / macOS / Linux |
| 原生菜单 | `muda` | 与 tray-icon 常用组合 |

默认 **不** 拉取这些依赖，以免增大默认体积。可选 feature：

```toml
rui = { version = "0.1", features = ["system-tray"] }
```

当前 `system-tray` 仅作编译期门闸；原生注册尚未接线。`TrayIcon::show` / `hide` 在 stub 下为空操作。

## Esc / 生命周期

托盘菜单点击回调应通过 `run_on_ui_thread` 回到 UI 线程再改 Signal，避免跨线程碰 Owner。
