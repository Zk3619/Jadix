# Multi-window（阶段六 / 十一）

## 现状（独立输入通道 + Gallery 验证）

- 主窗拥有完整 `AppState` / GPU 路径。
- 次窗经 [`open_secondary_window`](../crates/rui/src/app/multi_window.rs) → `RuiUserEvent::OpenWindow` 创建，登记在 `WindowRegistry`，各自持有独立 UI 树（`SecondaryWindowState`）与 **root `create_effect`**（可 `reactive_view` / `bind_text` / `#[component]`）。
- 设备 / queue 与主窗共享；次窗 resize / redraw 已接。
- **输入**：按 `WindowId` 分流。主窗走 `with_primary_input`；次窗走 [`secondary_input`](../crates/rui/src/app/secondary_input.rs)，只命中次窗树。
- **指针坐标**：`cursor_pos` 仍是进程级「最后一次物理移动」；主窗 redraw / `update_hovers` 仅在 `pointer_window == 主窗` 时使用该坐标，否则用窗外点清悬停。指针从主窗切到次窗时立即 `clear_primary_hovers`，避免次窗 Y 串到主窗 Command 高亮。主窗 redraw 在指针不在主窗时**不** `apply_cursor`，以免盖掉次窗 I-beam。
- **次窗 overlay 文本**：Dialog / Select 菜单等 portal 字形走独立 glyphon 带（与主窗同构的 band prepare/draw）。
- **遮罩**：Dialog 默认遮罩为黑 @ 0.5；次窗根为满窗容器，portal underlay 可铺满。
- **焦点 / Esc**：`__rui_*` focus / IME 与 [`portal_overlay`](../crates/rui/src/app/portal_overlay.rs) 栈按窗口 scope 隔离。
- **Owner 窗作用域**：次窗 `create_root` 会 `bind_current_owner_window_key_scope`；嵌套 effect 重跑时恢复 token。
- **Reactive 注册表**：`reactive_anchor` / `bind_*` 的进程级注册键按 window scope 加后缀，避免次窗与主窗同 id 时静默复用主窗 effect（表现为双窗同时响应 / 次窗点不动）。
- **窗作用域提交**：输入 / Focus / redraw 使用 `force_primary` / `force_secondary_id`（提交式），避免 `enter_*` guard 在 Drop 时把 `SIGNAL_KEY_SCOPE` 还原回次窗，导致主窗卡死或事件串窗。
- **无障碍 / IME**：次窗在 `set_visible` 前创建独立 AccessKit adapter；读屏 Action 按 `window_id` 打进该窗树。次窗 IME 事件在次窗 scope 组字，光标区绑到次窗 HWND（不再丢弃）。
- **协作**：主↔次正规通道是 [`shared_slot`](../crates/rui/src/app/multi_window.rs) + [`run_on_ui_thread`](../crates/rui/src/app/ui_bridge.rs)。
- Gallery：**导航**「Router + 次窗」与浮层页 **Secondary window** 打开同一验证树（Input / Select / Dialog / 共享计数）。

```rust,ignore
use rui::app::{open_secondary_window, run_on_ui_thread, shared_slot};

let count = shared_slot("my_bridge", || 0u32);
open_secondary_window("Inspector", move || {
    // 次窗树；id / Router / Navigator 不要与主窗撞名
    ui! { /* ... */ }
})?;

// 工作线程：
run_on_ui_thread(|| {
    let count = shared_slot("my_bridge", || 0u32);
    count.set(count.peek() + 1);
});
```

## 生命周期

1. **创建**：UI 线程 `open_secondary_window` → 事件循环 `create_window` → `insert_secondary_slot`。
2. **构建**：在次窗 `WindowScope` 下 `create_root` + `create_effect` 建独立 reactive tree。
3. **关闭**：用户关窗 → `drop_secondary`（丢掉 Owner / overlay 栈）+ `remove_secondary`；**不可**移除 primary。关**主窗**时 `shutdown_renderer` 会先隐藏并销毁全部次窗 HWND / 释放 Owner，再拆主窗 GPU，避免 Windows 上 `gallery.exe` 残留占锁。
4. **焦点协作**：次窗获焦时主窗走 `Focused(false)` 延迟释放 IME（TIP 候选）；主窗不会拆掉次窗已接管的 Imm 会话。次窗键 / IME 事件在次窗 scope 内处理，不改主窗 focus 信号。

## 与 Router / Navigator

```text
                    ┌──────────── 主窗 AppState ────────────┐
                    │  Router id = gal_shell_router         │
                    │  Navigator host = gal_nav_demo        │
                    │  overlay stack + focus keys（Primary） │
                    │  shared_slot gal_shell_bridge_count    │
                    └───────────────┬───────────────────────┘
                                    │ OpenWindow
                                    │ shared_slot / run_on_ui_thread
                                    ▼
                    ┌──────────── 次窗 Secondary ───────────┐
                    │  独立 Element 树 + surface + Owner     │
                    │  独立 pointer / keyboard 通道          │
                    │  overlay / focus keys（Secondary token）│
                    │  不共用 gal_shell_router / gal_nav_demo │
                    │  可读/写同一 shared_slot               │
                    └───────────────────────────────────────┘
```

- **Router**：命名路由 + back/forward。
- **Navigator**：转场栈（Host / Sheet）。
- 次窗通常跑独立 Router 或单页；不要与主窗共享 Navigator host id 或 Router id。

## 约定（主↔次 Signal）

| 做法 | 何时 |
|------|------|
| 同名 [`shared_slot`](../crates/rui/src/app/multi_window.rs) | 两窗都在 UI 线程读/写同一份状态 |
| 捕获 `Signal`（`Copy`）进次窗 builder | 打开时已在 UI 线程；builder 仍只在 UI 线程跑 |
| [`run_on_ui_thread`](../crates/rui/src/app/ui_bridge.rs) | 工作线程；闭包内再 `shared_slot` / `set`，不要把 `Signal` 标 `Send` |

**不要**把主窗控件 `id` / `focus_id` / overlay id 复用到次窗。
**不要**把任一窗的输入事件路由到另一窗的 Element 树。

## 后续（非阻塞）

- [x] 次窗 AccessKit adapter（读屏）
- [x] 次窗 IME 绑到次窗 HWND（组字 / 候选窗跟次窗 Input，不打进主窗）
- [x] 次窗独立 glyphon overlay 文本带（复杂 Dialog 与主窗同一套 hoist 已可用）
