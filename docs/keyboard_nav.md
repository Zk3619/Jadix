# 键盘导航契约（WP-1.2）

与 [desktop_smoke.md](desktop_smoke.md) #5–#7、[`FocusTrap`](../crates/rui/src/components/focus_trap.rs)、[`roving`](../crates/rui/src/utils/roving.rs)、[`focus_ring`](../crates/rui/src/style/focus_ring.rs) 对齐。

## Tab 序（应用级）

1. **显式 FocusTrap**：若 [`FOCUS_TRAP_RING_KEY`](../crates/rui/src/components/focus_trap.rs) 非空，仅在登记 id 环内循环。
2. **否则若有顶层 Stacked 浮层**（Dialog / Menu / Popover 等，`portal_overlay::top_stacked_panel_z`）：Tab **优先**在该层面板（portal 层内匹配 panel z）的可聚焦节点间循环；焦点在层外时下一次 Tab 落到层内第一项。
3. **否则**：全局 DFS 遍历带 `focusable` + `focus_id` 的节点（见 `input_controller` + `collect_focusable_ids`）。
4. 聚焦时写 `__rui_button_focus_id`；键盘移焦时写 `__rui_focus_visible = true`。
5. **指针按下**（主点击路径）将 `__rui_focus_visible = false`，因此非文本控件鼠标聚焦**不画**焦点环。

### 常见 `focus_id` 前缀

| 前缀 / 形态 | 控件 |
|-------------|------|
| `input:{id}` / `textarea:{id}` | Input / Textarea |
| 裸 `{id}` | Button、Checkbox、Switch、Toggle |
| `{id}:trigger` / `{id}:opt:{n}` | Select 触发器与选项 |
| `{id}__opt{n}` | Combobox 选项（触发器为 `input:{id}`） |
| `{menubar}:{section}:trigger` / `:item{n}` | Menubar |
| `{id}__ctx_trigger` / `{id}__item{n}` | ContextMenu |
| `tabstrip:{id}` | Tabs 无头条带 |
| `{group}:{value}` | RadioGroup / ToggleGroup / TabsList 芯片 |
| `scroll:{id}` | ScrollArea 视口（一般不进 Tab 环的视觉焦点环） |

Gallery / 应用应保证上述 id **页面内唯一**；Tab 顺序 = 树 DFS 遇到的可聚焦顺序。

## 焦点环

| 规则 | 说明 |
|------|------|
| 样式 | 与 Input 一致：主题 token `ring`（`ring-ring/50`）。默认 [`FocusRingStyle::Hard`](../crates/rui/src/style/theme.rs)（ShadCN v4：外侧 hard ring，spread `3`、blur `0`）；可 `set_focus_ring_style(Soft)` 切柔光。环画在控件盒外侧（与是否有边框无关，对齐 Tailwind `ring-*`）；有边框控件另同步 `border-ring`。无 ring-offset |
| 文本控件 | Input / Textarea：**任意聚焦**即画环（含鼠标） |
| 其它可受控控件 | Button、下拉触发器、菜单项、Checkbox / Switch / Toggle / Radio / TabsList 芯片、ContextMenu 触发器等：仅 **`focus-visible`（键盘 Tab）** 画环 |
| 实现 | 共享 [`focus_ring`](../crates/rui/src/style/focus_ring.rs)；Button snap 同步；其它 opt-in：`Element::focus_ring` + `sync_focus_ring_subtree` |

## Escape 关闭浮层

1. 顶层 portal / overlay 登记在 [`portal_overlay`](../crates/rui/src/app/portal_overlay.rs) Esc 栈。
2. Escape **只关闭最顶层**（`try_close_top`）；不穿透到下层对话框，除非顶层已关闭。
3. Combobox / Select / ContextMenu / Popover / Dialog / Menubar 菜单必须在打开时 `portal_sync` push、关闭时随 `open=false` pop。
4. 库测：`portal_overlay::push_two_esc_closes_top_only`、`three_layers_esc_closes_top_only_each_time`。

## 列表 / 菜单 Roving

| 控件 | 行为 |
|------|------|
| Combobox | 打开后 ↑↓ / Home / End（高亮项）；Enter 选中并关闭 |
| ContextMenu | 打开后 ↑↓ / Home / End；Enter/Space 激活高亮项 |
| Menubar | 关闭：←→ 移触发器焦点环；打开：↓ 进首项，↑↓ 移菜单项焦点环，←→ 换相邻菜单 |
| Tabs（无头 `tabstrip:`） | ←→ 环绕切换选中 index；Home / End |
| TabsList / 单选 ToggleGroup | ←→/↑↓ / Home / End **只移焦点环**；Enter / Space / 点击才改选中；TabsList `required` 禁止空选 |
| NavigationMenu | 顶栏芯片 ←→ 移焦点环；↓ 进入下拉首链；面板内 ↑↓；Esc 回触发器 |
| Sidebar | 槽位内可聚焦控件 ↑↓ / Home / End 移焦点环（`attach_focus_roving`） |

Helpers：[`roving_move`](../crates/rui/src/utils/roving.rs) / [`roving_edge`](../crates/rui/src/utils/roving.rs) / `roving_move_skipping`。

## 冒烟对照

| # | 步骤 | 期望 |
|---|------|------|
| 5 | Tab / Shift+Tab | 可受控控件出现焦点环；顺序合理 |
| 6 | Combobox：方向键 + Enter | 选项可选中并关闭 |
| 7 | Escape | 仅关闭最上层浮层 |

签署栏见 [desktop_smoke.md](desktop_smoke.md)。

- [x] 手工：Win 上 smoke **#5** 通过（2026-08-09，Kingdom / Win 10.0.26200）
- [x] 手工：Win 上 smoke **#6** 通过（Combobox 方向键 + Enter）
- [x] 手工：Win 上 smoke **#7** 通过（Escape 仅关顶层）
