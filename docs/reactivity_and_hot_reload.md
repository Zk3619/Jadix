# 细粒度响应式与热重载约定

本文档说明 `reactive!` / `Reactive` / `bind_*` / `<Show>` / `<For>` 与开发期热重载（`template_patch`）如何协同，避免补丁误伤子树 reconcile。

## 三档更新（对齐 SolidJS）

| 档位 | API | 用途 |
|------|-----|------|
| 文本 / 单属性 | [`bind_text`](../crates/rui/src/app/reactive.rs) / `bind_prop` / [`bind_attr`](../crates/rui/src/app/reactive.rs) / [`bind_style`](../crates/rui/src/app/reactive.rs) / `bind_class` / `bind_*!` | 就地改 `text` / style / class / Button·Input chrome / Icon name，**不**重建子树。 |
| 结构 | `reactive!` / `<Reactive>` / `reactive_view` / `reactive_component` / [`show_reactive`](../crates/rui/src/app/reactive.rs) / [`for_reactive`](../crates/rui/src/app/reactive.rs) | 挂载/卸载子树（菜单、Popover、条件块、列表、拖拽排序、VirtualList）。 |
| 高频 chrome | `ReactiveSync` / Button·Input paint sync | `peek` + 帧前写回，不订根 effect。 |

**根 effect 卫生：** 在根 `app_builder` 里裸写 `signal.get()`（且未走 `bind_*` / `<Show when={…get()}>` / `<For each={…get()}>` / `<Reactive>`）仍会订阅整棵 app。

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

`ui!` 自动降级（含 `.get()` 且不在 `<Reactive>` 内）：

- 文本 child → `bind_text`
- `class` / `bg` / `color` / `border_color` → bind
- `<Button variant|disabled={…get()}>` → `bind_attr` + `apply_bound_button_*`
- `<Input invalid|disabled={…get()}>` → `bind_attr` + `apply_bound_input_*`
- `<Icon name={…get()}>` → `bind_attr` + [`apply_bound_icon_name`](../crates/rui/src/utils/lucide.rs)

## `<Show>` / `<For>`

- `when` / `each` 含 `.get()` → `show_reactive` / `for_reactive`
- 否则 → `show_scoped` / 直接 `Vec<Element>`

## `#[component(reactive)]`（字段级 / getter-props）

- 快照 PartialEq **只含静态 props**；`Signal` / 闭包 / `Element` 排除。
- **必填 Signal props**：宏用 `expect`（不再 `unwrap_or_default`），适合 Solid 式把 Signal 当 getter 传入。
- 组件 effect 经 [`isolate_reads`](../crates/rui/src/core/reactivity.rs)：裸 `get()` 不订组件边界；字段更新靠 `ui!` 自动 bind / Show / For。
- 试点：`hello_world` 的 `ReactiveStatusBadge`。

## Owner / Context（阶段一）

| API | 用途 |
|-----|------|
| [`create_root`](../crates/rui/src/core/reactivity.rs) / `OwnerHandle` | 独立作用域；dispose 时级联清理 |
| [`on_cleanup`](../crates/rui/src/core/reactivity.rs) | owner 销毁时 LIFO 回调 |
| [`create_context`](../crates/rui/src/core/reactivity.rs) / `provide_context` / `use_context` | 沿 owner 父链查找 |
| [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs) | `{owner_path}::{local_id}`；全局持久状态仍用 `use_signal` |
| [`Signal::with`](../crates/rui/src/core/reactivity.rs) / `update` | 借用读 / 原地写 |
| [`create_memo`](../crates/rui/src/core/reactivity.rs) | 惰性 dirty；flush 按 depth，菱形无中间态 |

## 结构岛（故意保留 `reactive!`）

Grid drag / Navigator / VirtualList / Portal 等拓扑变化场景继续用结构边界，不要强行降级成 bind。

## `reactive!` 与稳定 `template_id`

需要热重载或长期稳定 reconcile 的区块写显式 `reactive!("my_feature:block", || ...)`。

## 相关源码

- [app/reactive.rs](../crates/rui/src/app/reactive.rs)
- [utils/lucide.rs](../crates/rui/src/utils/lucide.rs)
- [components/button.rs](../crates/rui/src/components/button.rs) / [input.rs](../crates/rui/src/components/input.rs)
- [rui_macros](../crates/rui_macros/src/lib.rs)

回归测试：`reactive_tests`、`bind_text_tests`、`core::reactivity::tests`。
