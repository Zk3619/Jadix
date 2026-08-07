# Multi-window（阶段六 / 十一）

## 现状（wired MVP）

- 主窗拥有完整 `AppState` / GPU 路径。
- 次窗经 [`open_secondary_window`](../crates/rui/src/app/multi_window.rs) → `RuiUserEvent::OpenWindow` 创建，登记在 `WindowRegistry`，各自持有独立 UI 树（`SecondaryWindowState`）。
- 设备 / queue 与主窗共享；输入路由仍以主窗为先（次窗 resize / redraw 已接）。

```rust,ignore
use rui::app::open_secondary_window;

open_secondary_window("Inspector", || {
    // 构建次窗根 Element
    Element::new().w("100%").h("100%")
})?;
```

## 生命周期

1. **创建**：UI 线程调用 `open_secondary_window` → 事件循环创建 `Window` → `insert_secondary_slot`。
2. **构建**：`SecondaryUiBuilder` 在次窗上下文执行，建立独立 reactive tree。
3. **关闭**：用户关窗 → `remove_secondary`；**不可**移除 primary。
4. **焦点协作（目标）**：次窗获焦时主窗浮层（portal Esc 栈）应保持独立；当前 Esc 栈仍为进程级 TLS，多窗深化时需改为 **per-window** overlay / focus 状态。

## 与 Router / Navigator

- **Router**：命名路由，适合主窗「页面」切换。
- **Navigator**：转场栈（Host / Sheet）。
- 次窗通常跑**独立** Router 或单页；不要与主窗共享同一个 Navigator host id。

## 后续

- [ ] 次窗完整 pointer / keyboard 路由
- [ ] per-window portal_overlay / focus
- [ ] 主↔次 Signal 协作约定（`run_on_ui_thread` + 显式桥）
