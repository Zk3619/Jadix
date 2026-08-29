# RUI 无头组件（Headless）契约

> **从属**：[COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md)（组件领域规范）。本文只展开三层边界与 Signal/a11y 细则；产品选型与清单以规范为准。实施见 [AUTHORITY.md](AUTHORITY.md) Wave CV / WP-3.*。  
> **日期**：2026-08-09；**2026-08-17** WP-3.7 `toggle_state`  

本文档说明 **primitives（布局/交互叶子）**、**tokens（设计令牌）**、**utils（行为工具）** 与 **components（样式层）** 的职责边界，以及如何与 `Signal`、`ui!`、`#[component]` 配合。

## 三层职责（路径以代码为准）

| 层级 | 路径 | 职责 | 禁止 |
|------|------|------|------|
| 布局 / 交互原语 | `crates/rui/src/primitives/` | `box`/`hstack`/`vstack`/`gstack`、`interactive`、`overlay`、`text`/`icon`、`path`、`Portal` | 不写配色、圆角、阴影等视觉常量 |
| 行为工具 | `crates/rui/src/utils/` | **`popup`**、**`presence`**、**`motion`**、**`keyed_overlay`**、**`layout_group`**、`roving`、`focus_trap`、`keyboard`、`text_field`、**`toggle_state`**、**`controlled`**（String/f32/open）、**`activate_registry`**、**`list_nav`**、**`pointer_drag`**、**`table_sort`** / **`tree_model`** / **`element_pack`** | 不绑定某一组件的外观变体 |
| 设计令牌 | `crates/rui/src/tokens/` + `style/theme` + `style/color_space` | 语义色、间距、圆角等可主题化数值 | 不包含业务逻辑 |
| 样式组件 | `crates/rui/src/components/` | 组合原语 + 令牌 + utils，形成 ShadCN 风格外观 | 不把复杂状态机永久只藏在样式层 |

**行为原语（已抽出）**：

| 目标原语 | 现状 | 计划 |
|----------|------|------|
| `toggle` 布尔状态 | [`utils/toggle_state`](../crates/rui/src/utils/toggle_state.rs) | **WP-3.7 ☑**；Toggle / Checkbox / Switch 共用受控 XOR Owner 作用域 |
| String / f32 / open | [`utils/controlled`](../crates/rui/src/utils/controlled.rs) | **WP-SI.2**；Tabs / Radio / Slider / Date·Color open+value |
| Activate 注册表 | [`utils/activate_registry`](../crates/rui/src/utils/activate_registry.rs) | **WP-SI.1**；菜单 / Command Enter 激活 |
| 打开列表键 | [`utils/list_nav`](../crates/rui/src/utils/list_nav.rs) | **WP-SI.4**；Combobox ↑↓/Home/End；Command 过滤框仅 ↑↓/Enter/Esc |
| Presence / ExitGate | [`utils/presence`](../crates/rui/src/utils/presence.rs) | **WP-PRIM.1**；退场播完再卸 |
| MotionStyle | [`utils/motion`](../crates/rui/src/utils/motion.rs) | **WP-PRIM.2**；只写目标、保 mid + transition；与 `@keyframes` 分工见 [`animation_and_motion.md`](animation_and_motion.md) |
| KeyedOverlay | [`utils/keyed_overlay`](../crates/rui/src/utils/keyed_overlay.rs) | **WP-PRIM.3**；portal + 同 key 调和（shadow/transform mid） |
| LayoutGroup / slot_list | [`utils/layout_group`](../crates/rui/src/utils/layout_group.rs) | **WP-PRIM.4**；rank → offset/scale 目标 |
| Toast 产品策略 | `components/toast/{timer,queue,swipe}` | 单组件拆分（**非**框架原语；已从 internal 折回） |
| Carousel 产品策略 | `components/carousel/{track,snap,loop_anim,autoplay}` | 同上 |

用户自定义组件（如 Alert）应只组合 `Box` / `Text` / `Interactive` 等，并通过 `#[component]` 暴露 props，与 [custom_components.md](custom_components.md) 一致。`#[component]` 默认实例 Owner（WP-3.14）；纯 Builder 复合根用 `component_root`。浮层子树用 `isolate_reads`。私有壳（`components/internal/text_edit`）不是公开无头 API；Dialog 模态壳在 `components/dialog.rs`。

## 受控与非受控

与常见 React 模式对齐：

- **非受控（Uncontrolled）**：内部使用 [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)（布尔控件经 [`resolve_bool_signal`](../crates/rui/src/utils/toggle_state.rs)），适用于独立控件。  
  `scope_id` **不要**默认等同于「用户必须手写的 HTML 式 id」——优先 Owner 作用域或框架派生键；仅多实例/跨树持久时要求稳定用户键。详见 [COMPONENT_LIBRARY.md §4.2](COMPONENT_LIBRARY.md)。
- **受控（Controlled）**：调用方持有 `Signal<T>`（或等价读写接口），组件只订阅/更新该信号，**不得**再为同一状态创建第二份 `use_signal`。

实现时需避免「双源真理」与「身份真理」：不要用同一个用户 `id` 字符串同时冒充 focus、template、测试选择器而不文档化派生规则。

## 与双书写面的关系（`ui!` ↔ Builder）

宏展开仍生成 `Element` 构建器链。内置标签（如 `Button`、`Switch`）对应 `components::*` 的 builder；**无标签的纯 Rust Builder / `vstack`·`interactive` 调用与标签等价**，可互嵌。

| 写法 | 示例 |
|------|------|
| 标签 | `ui! { <Button variant="outline">"OK"</Button> }` |
| Builder | `Element::from(button("ok").variant("outline").text("OK"))` |
| 互嵌 | `ui!` 子节点 `{ Element::from(button(…)) }`；或 `hstack().child(ui! { <Label>"x"</Label> })` |

权威契约见 [COMPONENT_LIBRARY.md §3](COMPONENT_LIBRARY.md)。

## 无障碍（AccessKit）

`crates/rui/src/a11y/bridge.rs`（[`rui::a11y`](../crates/rui/src/a11y/mod.rs)）提供 `tree_update_from_elements(root, focused_fid)`，从带 `focus_id` 的控件生成层级 `TreeUpdate`（窗口根 + 焦点部件）。**运行时**已在 `app::lifecycle` 中接入 `accesskit_winit::Adapter`（窗口首帧不可见创建适配器后再 `set_visible`），并于每帧 `render` 成功后 `update_if_active` 同步树；`TextInput` 角色映射依赖 `input:` 前缀的 `focus_id` 与输入值信号。

Tabs headless 条带的 `focus_id` 使用前缀 `tabstrip:`（常量 `components::tabs::TABSTRIP_FOCUS_PREFIX`），无障碍树中映射为 **TabList**。

细粒度响应式与热重载约定见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)；权威口径见 [REACTIVITY.md](REACTIVITY.md)。键盘 / `utils/focus_trap` 见 [keyboard_nav.md](keyboard_nav.md)。

## 无窗口布局测试

不启 `winit` 窗口即可跑布局：构造 [`Element`](../crates/rui/src/core/tree.rs) 树，调用 [`LayoutTree::build_and_layout`](../crates/rui/src/layout/layout_tree.rs)，再断言 `layout_bounds` 宽高 > 0。库测见 `crates/rui/src/crate_tests/layout/headless_smoke.rs`（`cargo test -p rui --lib headless_build_and_layout`）。
