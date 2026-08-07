# 键盘导航契约（阶段七）

与 [desktop_smoke.md](desktop_smoke.md)、[`FocusTrap`](../crates/rui/src/components/focus_trap.rs)、[`roving`](../crates/rui/src/utils/roving.rs) 对齐。

## Tab 序

1. 无 FocusTrap：全局 Tab 遍历带 `focusable` + `focus_id` 的节点（DFS，见 `input_controller`）。
2. 有 FocusTrap：仅在 trap 登记的 id 环内循环。
3. 聚焦时写 `__rui_button_focus_id` 与 `__rui_focus_visible`（焦点环）。

## Escape 关闭浮层

1. 顶层 portal / overlay 登记在 [`portal_overlay`](../crates/rui/src/app/portal_overlay.rs) Esc 栈。
2. Escape **只关闭最顶层**；不穿透到下层对话框，除非顶层已关闭。
3. Combobox / Select / ContextMenu / Popover / Dialog 必须在打开时 push、关闭时 pop。

## 列表 / 菜单 Roving

- 方向键：[`roving_move`](../crates/rui/src/utils/roving.rs)
- Home / End：[`roving_edge`](../crates/rui/src/utils/roving.rs)
- Enter / Space：激活当前项并关闭（若为菜单）

## 焦点环

- 主题 token：`ring`（[`DesignTokens.ring`](../crates/rui/src/style/theme.rs)）
- 共享 helper：[`apply_focus_ring_shadow`](../crates/rui/src/style/focus_ring.rs)
