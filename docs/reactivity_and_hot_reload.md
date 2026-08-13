# 细粒度响应式与热重载约定

> **文档地位**：开发者**用法手册**（三档更新、宏降级、热重载槽）。  
> **权威口径**：编程模型、四框架机制对照、完整细粒度不变量 → [`REACTIVITY.md`](REACTIVITY.md) §5；执行包 → [`AUTHORITY.md`](AUTHORITY.md) WP-2.4 / 2.5 / 2.7 / WP-R.5。  
> **身份轴**：[`element_identity.md`](element_identity.md)。  

本文档说明 `reactive!` / `Reactive` / `bind_*` / `<Show>` / `<For>` 与开发期热重载（`template_patch`）如何协同，避免补丁误伤子树 reconcile。

## 三档更新（对齐 SolidJS）

| 档位 | API | 用途 |
|------|-----|------|
| 文本 / 单属性 | [`bind_text`](../crates/rui/src/app/reactive.rs) / `bind_prop` / [`bind_attr`](../crates/rui/src/app/reactive.rs) / [`bind_style`](../crates/rui/src/app/reactive.rs) / `bind_class` / `bind_*!` | 就地改 `text` / style / class / Button·Input chrome / Icon name，**不**重建子树。 |
| 结构 | `reactive!` / `<Reactive>` / `reactive_view` / `reactive_component` / [`show_reactive`](../crates/rui/src/app/reactive.rs) / [`for_reactive`](../crates/rui/src/app/reactive.rs) / [`switch_reactive`](../crates/rui/src/app/reactive.rs) | 挂载/卸载子树（菜单、Popover、条件块、列表、多分支）。 |
| 高频 chrome | `ReactiveSync` / Button·Input paint sync | `peek` + 帧前写回，不订根 effect。 |

**根 effect 卫生：** 在根 `app_builder` 里裸写 `signal.get()`（且未走 `bind_*` / `<Show when={…get()}>` / `<For each={…get()}>` / `<Reactive>`）仍会订阅整棵 app。

**诊断（WP-2.7 ☑）：** lifecycle 对根 effect 调用 [`mark_next_effect_as_ui_root`](../crates/rui/src/core/reactivity.rs)；完成后用 [`ui_root_effect_dep_count`](../crates/rui/src/core/reactivity.rs) / `root_bare_get_count`（应为 `0`）。debug 构建下若根订阅非空，首次 warn 一次。库测：`cargo test -p rui --lib root_bare_get`。

### 仍须整树 / 结构路径的场景（不是 bug）

双路径是**设计**：属性走 bind 就地改；下列情况仍走结构岛或根 `rebuild_ui`，不要用「消灭 rebuild」当目标。

| 场景 | 正确做法 | 勿做 |
|------|----------|------|
| 纯文本 / 单属性闪烁 | `bind_text` / `bind_*` / `ui!` 自动降级 | 根上裸 `get()` 拼字符串驱动整树 |
| 条件挂载 / 列表 | `show_reactive` / `for_*` / `reactive!` | `if sig.get() { … } else { … }` 写在根 builder |
| 拓扑变化（Navigator / Portal / VirtualList） | 结构岛 + 稳定 `template_id` | 强行降成 bind |
| 热重载空 templates / `StructureChanged` | 框架全树 reconcile（见 `state::rebuild_ui`） | 手写第二套 diff |
| 高频 chrome（hover / caret） | `ReactiveSync` + `peek` | effect 风暴订根 |

## 首帧与订阅

根 `app_builder` 在 `lifecycle::resumed` 中会先**无 effect 上下文**执行一次以建树；随后注册的根 `create_effect` 再次执行时，`signal.get()` 才会挂到根 effect 上。

## `bind_*` API

| API | 用途 |
|-----|------|
| `bind_text` / `bind_text!` | 就地改文本 |
| `bind_prop` | 通用 `compute → apply` |
| `bind_attr` / `bind_attr!` | `bind_prop` 别名（variant / invalid / icon name…） |
| `bind_class` | utility class 字符串 |
| `bind_style` / `bind_style!` + [`StylePatch`](../crates/rui/src/app/reactive.rs) | 批量 bg / text_color / border_color / class |

**`bind_text` 与布局（对齐 DOM / Solid）：** 固有宽变大、或超过已挂载 `layout_bounds` 宽度时返回 `LayoutChanged`，父级（如 ShadCN 式 `inline-flex` + `whitespace-nowrap` 的 Button）拉宽；同宽或变窄可走 `StyleChanged` 仅重绘。勿用过宽的「同宽」阈值跳过布局，否则文案会在旧宽里 soft-wrap 而按钮尺寸不变。

`ui!` 自动降级（含 `.get()` 且不在 `<Reactive>` 内）：

- 文本 child → `bind_text`
- `class` / `bg` / `color` / `border_color` → bind
- `<Button variant|disabled={…get()}>` → `bind_attr` + `apply_bound_button_*`
- `<Input invalid|disabled={…get()}>` → `bind_attr` + `apply_bound_input_*`
- `<Icon name={…get()}>` → `bind_attr` + [`apply_bound_icon_name`](../crates/rui/src/utils/lucide.rs)

## `<Show>` / `<For>` / `<Switch>` / ErrorBoundary

- `when` / `each` 含 `.get()` → `show_reactive` / `for_reactive`
- 否则 → `show_scoped` / 直接 `Vec<Element>`
- 多分支：[`switch_reactive`](../crates/rui/src/app/reactive.rs)（`key` + `render(&key)`）
- 子树错误：[`error_boundary`](../crates/rui/src/app/error_boundary.rs) + [`throw_error`](../crates/rui/src/app/error_boundary.rs)；Resource Err 须显式 [`throw_resource_error`](../crates/rui/src/app/error_boundary.rs)（WP-2.5）

**列表粒度（勿当成已对齐 Solid `<For>`）**

| API | 实际机制 | 不是 |
|-----|----------|------|
| `for_reactive` + `for_keyed` | 列表 signal 变 → **整表** `render_list()` 再跑 → `reconcile_keyed_children` 按 `template_id` 复用 Element/`layout_node` | Solid `mapArray`：未变 key **不**重跑项 `render`、每项独立 `createRoot` |
| `for_each` | 下标当 `{scope}:{idx}` | Solid `<Index>`（项为 Accessor、下标固定） |

战略：[`REACTIVITY.md`](REACTIVITY.md) §0.1 / §5.9 —— **超过 Floem 须 WP-R.5b**；**逼近 Solid 还须 5c Index + Store 路径 + effect 分相**。执行 WP-R.5（5a → 5b → 5c）。

`reactive_anchor` / `bind_prop` / 首次 `reactive_component` 注册时均挂 [`on_cleanup`](../crates/rui/src/core/reactivity.rs)：owner dispose 时从 `REGISTERED_ANCHORS` / `REGISTERED_BINDINGS` / `COMPONENT_PROPS`（及未 take 的 `COMPONENT_CHILDREN`）移除并 dispose effect（WP-2.4 ☑）。诊断计数：`registered_anchor_count` / `registered_binding_count` / `component_props_count`。

## `#[component(reactive)]`（字段级 / getter-props）

- 快照 PartialEq **只含静态 props**；`Signal` / 闭包 / `Element` 排除。
- **必填 Signal props**：宏用 `expect`（不再 `unwrap_or_default`），适合 Solid 式把 Signal 当 getter 传入。
- 组件 effect 经 [`isolate_reads`](../crates/rui/src/core/reactivity.rs)：裸 `get()` 不订组件边界；字段更新靠 `ui!` 自动 bind / Show / For。
- 试点：`gallery` 的 `ReactiveStatusBadge`。

## Owner / Context（阶段一 + 九）

| API | 用途 |
|-----|------|
| [`create_root`](../crates/rui/src/core/reactivity.rs) / `OwnerHandle` | 独立作用域；dispose 时级联清理 |
| [`on_cleanup`](../crates/rui/src/core/reactivity.rs) | owner 销毁时 LIFO 回调 |
| [`on_mount`](../crates/rui/src/core/reactivity.rs) | owner 首次 tick 跑一次（不随无关 signal 重跑） |
| [`create_context`](../crates/rui/src/core/reactivity.rs) / `provide_context` / `use_context` | 沿 owner 父链查找 |
| [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs) | owner 本地槽；全局持久状态仍用 `use_signal` |
| [`scoped_template_id`](../crates/rui/src/core/reactivity.rs) / [`scoped_focus_id`](../crates/rui/src/core/reactivity.rs) | Owner path 前缀的 UI 身份键（WP-2.4） |
| [`Signal::with`](../crates/rui/src/core/reactivity.rs) / `update` | 借用读 / 原地写 |
| [`create_memo`](../crates/rui/src/core/reactivity.rs) | 惰性 dirty；flush 按 depth，菱形无中间态 |
| [`Store::produce`](../crates/rui/src/core/store.rs) | Solid 式原地更新（`update` 别名） |

## 结构岛（故意保留 `reactive!`）

Grid drag / Navigator / VirtualList / Portal 等拓扑变化场景继续用结构边界，不要强行降级成 bind。

## `reactive!` 与稳定 `template_id`

需要热重载或长期稳定 reconcile 的区块写显式 `reactive!("my_feature:block", || ...)`。

`template_id` 是**槽位**身份，不是 AccessKit / 焦点全局主键；空串与四轴拆分见 [`element_identity.md`](element_identity.md)。

## 相关源码

- [app/reactive.rs](../crates/rui/src/app/reactive.rs)
- [app/error_boundary.rs](../crates/rui/src/app/error_boundary.rs)
- [utils/lucide.rs](../crates/rui/src/utils/lucide.rs)
- [components/button.rs](../crates/rui/src/components/button.rs) / [input.rs](../crates/rui/src/components/input.rs)
- [rui_macros](../crates/rui_macros/src/lib.rs)

回归测试：`crate_tests::reactive`、`core::reactivity::tests`、`crate_tests::layout::headless_smoke`。
