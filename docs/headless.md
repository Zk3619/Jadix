# RUI 无头组件（Headless）契约

> **从属**：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md)（组件库权威规范）。本文只展开三层边界与 Signal/a11y 细则；产品选型（默认 shadcn 样式面）与清单以规范为准。实施见 [COMPONENT_LIBRARY_PLAN.md](COMPONENT_LIBRARY_PLAN.md)。

本文档说明 **primitives（无头层）**、**tokens（设计令牌）** 与 **components（样式层）** 的职责边界，以及如何与 `Signal`、`ui!`、`#[component]` 配合。与 [rui.md](rui.md) 中的分层模型一致。

## 三层职责

| 层级 | 路径 | 职责 | 禁止 |
|------|------|------|------|
| 无头原语 | `crates/rui/src/primitives/` | 交互状态、键盘/指针路由、弹层开关逻辑等 | 不写配色、圆角、阴影等视觉常量 |
| 设计令牌 | `crates/rui/src/tokens/` | 语义色、间距、圆角等可主题化数值 | 不包含业务逻辑 |
| 样式组件 | `crates/rui/src/components/` | 组合原语 + 令牌，形成 ShadCN 风格外观 | 不把复杂状态机藏在仅样式层（应下沉到原语） |

用户自定义组件（如 Alert）应只组合 `Box` / `Text` / `Interactive` / `toggle` 等，并通过 `#[component]` 暴露 props，与 [custom_components.md](custom_components.md) 一致。

## 受控与非受控

与常见 React 模式对齐：

- **非受控（Uncontrolled）**：原语内部使用 `use_signal(scope_id, init)` 持有状态，适用于独立控件。  
  `scope_id` **不要**默认等同于「用户必须手写的 HTML 式 id」——优先 Owner 作用域（`use_scoped_signal`）或框架派生键；仅多实例/跨树持久时要求稳定用户键。详见 [COMPONENT_LIBRARY.md §4.2](COMPONENT_LIBRARY.md)。
- **受控（Controlled）**：调用方持有 `Signal<T>`（或等价读写接口），原语只订阅/更新该信号，**不得**再为同一状态创建第二份 `use_signal`。

实现时需避免「双源真理」：同一布尔值不能既写内部 signal 又写外部传入而不做单一写入点。亦需避免「身份真理」：不要用同一个用户 `id` 字符串同时冒充 focus、template、测试选择器而不文档化派生规则。

## 原语清单（演进中）

| 原语 | 职责 |
|------|------|
| `interactive` | hover、click、焦点 id、可选 `on_key_down` / 激活键 |
| `toggle` | 布尔切换、disabled、受控/非受控 |
| `popup` | 打开状态、锚点、点击外部关闭、Escape（依赖键盘路由） |

复合组件（Dialog、Dropdown、Tabs）在无头逻辑稳定后，基于 `popup` / `toggle` 等与样式层组合实现。

## 与双书写面的关系（`ui!` ↔ Builder）

宏展开仍生成 `Element` 构建器链。内置标签（如 `Button`、`Switch`）对应 `components::*` 的 builder；**无标签的纯 Rust Builder / `vstack`·`interactive` 调用与标签等价**，可互嵌。

| 写法 | 示例 |
|------|------|
| 标签 | `ui! { <Button variant="outline">"OK"</Button> }` |
| Builder | `Element::from(button("ok").variant("outline").text("OK"))` |
| 互嵌 | `ui!` 子节点 `{ Element::from(button(…)) }`；或 `hstack().child(ui! { <Label>"x"</Label> })` |

无头原语亦可在 `#[component]`、`ui!` 或纯 Builder 树中组装。权威契约见 [COMPONENT_LIBRARY.md §3](COMPONENT_LIBRARY.md)。

## 无障碍（AccessKit）

`crates/rui/src/accesskit_bridge.rs` 提供 `tree_update_from_elements(root, focused_fid)`，从带 `focus_id` 的控件生成扁平 `TreeUpdate`（窗口根 + 焦点部件）。**运行时**已在 `app::lifecycle` 中接入 `accesskit_winit::Adapter`（窗口首帧不可见创建适配器后再 `set_visible`），并于每帧 `render` 成功后 `update_if_active` 同步树；`TextInput` 角色映射依赖 `input:` 前缀的 `focus_id` 与输入值信号。

`tabs_headless` 的 `focus_id` 使用前缀 `tabstrip:`（常量 `rui::primitives::TABSTRIP_FOCUS_PREFIX`），无障碍树中映射为 **TabList**。

细粒度响应式与热重载约定见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)。

## 无窗口布局测试

不启 `winit` 窗口即可跑布局：构造 [`Element`](../crates/rui/src/core/tree.rs) 树，调用 [`LayoutTree::build_and_layout`](../crates/rui/src/layout/layout_tree.rs)，再断言 `layout_bounds` 宽高 > 0。库测见 `crates/rui/test/headless_layout.rs`（`cargo test -p rui --lib headless_build_and_layout`）。
