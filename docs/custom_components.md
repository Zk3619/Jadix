# Creating Custom Components in RUI

> 分层与选型见权威规范 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md)（含 **§2.3 作者面**、**§3 双书写面**）；无头细则见 [headless.md](headless.md)。  
> **W3 实证 / WP-3.14**：复合控件难写，主因曾是框架默认值。现 `#[component]` 默认实例 Owner；Builder 根与跟踪纪律见下文配方。诊断：[COMPONENT_LIBRARY.md §2.3](COMPONENT_LIBRARY.md)。

RUI 用分层 + `#[component]` 组合原语。`#[component]` **每实例自动** [`component_root`](../crates/rui/src/core/reactivity.rs)（WP-3.14）。纯 Builder 复合根（自己 `provide_context` / 浮层）仍须显式调用同一 helper。浮层子树用 `isolate_reads`，不要用 `untrack`。

## Two equivalent ways to write UI

Both are first-class and must stay interchangeable ([COMPONENT_LIBRARY.md §3](COMPONENT_LIBRARY.md)):

| Style | Example |
|-------|---------|
| **A · `ui!` tags** | `ui! { <Button variant="outline">"OK"</Button> }` |
| **B · Builder (no HTML-like tags)** | `Element::from(button("ok").variant("outline"))` |

You can nest either form inside the other (`ui!` children may be `{ Element::from(button(…)) }`; builders may `.child(ui! { … })`). Prefer whichever fits the call site; do not treat tags as the only “modern” API.

稳定面 `variant` / `size` / 受控 `*_signal` 命名见 [component_variants.md](component_variants.md)（WP-3.8）。

## The `#[component]` Macro

The primary way to define a **reusable** component is the `#[component]` attribute macro. The function body may return a tree built with **`ui!` and/or Builders**—both are valid.

### Basic Syntax (`ui!`)

```rust
use rui::prelude::*;

#[component]
pub fn MyButton(label: String, primary: bool) -> Element {
    let bg_color = if primary { "primary" } else { "secondary" };
    
    ui! {
        <Button 
            bg={bg_color}
            px="1rem"
            py="0.5rem"
            rounded="0.375rem"
        >
            {label}
        </Button>
    }
}
```

### Same idea with Builders only

```rust
use rui::prelude::*;

// Pure Rust: no class-HTML tags required（与 ui! 同等一等公民）。
fn counter_row(count_label: Element, on_inc: impl Fn() + 'static) -> Element {
    vstack()
        .gap("0.5rem")
        .child(count_label)
        .child(Element::from(
            button("inc").variant("default").on_click(move |_| on_inc()),
        ))
        .into()
}
```

(Exact Builder method names follow each component’s `//!` docs; the point is **parity of capability**, not a second component system.)

`ui!` accepts ShadCN-style camelCase aliases that the macro rewrites to snake_case props: `onClick`, `defaultValue`, `captionLayout`, `valueSignal`, `showAlpha`. Builders always use snake_case (`caption_layout`, `value_signal`, …).

### How it Works

1.  **Props Generation**: The macro automatically generates a `MyButtonProps` struct containing the arguments of your function.
2.  **Instance Owner**: The body runs inside [`component_root`](../crates/rui/src/core/reactivity.rs) (WP-3.14). Sibling instances do not share `provide_context`.
3.  **Element Implementation**: It implements `From<MyButtonProps> for Element`, allowing you to use `<MyButton />` inside other `ui!` macros.
4.  **Destructuring**: Inside the function body, the props are automatically destructured for easy access.

## Advanced Component Patterns

### Optional Props with `Option<T>`

If you define a prop as `children: Option<Element>`, the `ui!` macro will automatically fill it with any nested tags.

```rust
#[component]
pub fn Card(title: String, children: Option<Element>) -> Element {
    ui! {
        <VStack bg="card" p="1rem" rounded="0.5rem" shadow="sm">
            <Text size="1.25rem" weight="600">{title}</Text>
            <Box h="1px" bg="border" my="0.5rem" />
            {children}
        </VStack>
    }
}

// Usage:
ui! {
    <Card title="Hello World">
        <Text>"This is inside the card!"</Text>
    </Card>
}
```

### Reactivity with Signals

`use_signal("key", …)` **不是**按组件实例自动隔离的：字符串键落在运行时全局表（与 Owner 正交）。多实例优先受控 Signal、`use_scoped_signal`、或带实例前缀的键。`#[component]` 只保证 **Context / 子 effect** 挂在该实例 Owner 上。

```rust
#[component]
pub fn Counter() -> Element {
    // 单实例演示可以；第二份 <Counter/> 会撞 "counter_state"。
    let count = use_signal("counter_state", || 0);
    
    ui! {
        <HStack gap="1rem" items="center">
            <Button on_click={move || count.set(count.get() - 1)}>"-"</Button>
            <Text>{count.get().to_string()}</Text>
            <Button on_click={move || count.set(count.get() + 1)}>+</Button>
        </HStack>
    }
}
```

#### Instance keys（多实例组件）

`use_signal("name", ...)` 的字符串键在**同一组件多实例**时会碰撞。优先：

1. **受控**：状态放在调用方 `Signal`，组件可不依赖全局 `id`。  
2. **Owner 作用域**：[`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)；布尔 on/off 用 [`resolve_bool_signal`](../crates/rui/src/utils/toggle_state.rs)（WP-3.7），不要再手写 `{id}__checkbox`。  
3. 否则使用稳定唯一键，例如 `format!("counter:{id}", id = props.instance_id)`，或与 `reactive!` 的显式 anchor id 同步。

**不要**给每个子控件都发明一个 `id`「以防万一」。身份轴拆分见 [COMPONENT_LIBRARY.md §4.2](COMPONENT_LIBRARY.md)。热重载约定见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)。

### 复合控件配方（WP-3.14）

#### `#[component]`（默认实例 Owner）

```rust
use rui::prelude::*;
use rui::core::reactivity::{create_context, provide_context, use_context, Context};
use std::sync::OnceLock;

fn menu_ctx() -> Context<String> {
    static CTX: OnceLock<Context<String>> = OnceLock::new();
    *CTX.get_or_init(create_context)
}

#[component]
pub fn Slot(label: String) -> Element {
    // 无需 with_child_owner：宏已包 component_root。
    provide_context(menu_ctx(), label.clone());
    ui! { <Text>{label}</Text> }
}
```

两个 `<Slot/>` 兄弟互不覆盖 Context。库测：`component_macro_sibling_context_isolated_without_manual_owner`。

#### Builder 根（必须显式）

```rust
use rui::prelude::*;

fn my_combobox(id: &str, body: impl FnOnce() -> Element) -> Element {
    component_root(|| {
        provide_context(menu_ctx(), id.to_string());
        body()
    })
}
```

`with_child_owner` 是 `component_root` 的别名。库测：`component_root_sibling_context_isolated`。

#### 四条红线

| 要做 | 不要做 | 失败外观 |
|------|--------|----------|
| `#[component]` 即可；Builder 根包 [`component_root`](../crates/rui/src/core/reactivity.rs) | Builder 上同父级连续 `provide_context` 且不包根 | 三个 Combobox 共用最后一个菜单 |
| 浮层 / `reactive_anchor` 子树用 [`isolate_reads`](../crates/rui/src/core/reactivity.rs) | 用 [`untrack`](../crates/rui/src/core/reactivity.rs) 包整棵面板 | 锚点不注册，浮层空壳（只剩分割线） |
| 浮层走 `mount_anchored_overlay` / `mount_viewport_portal_stage` + portal Esc 栈 | 再抄一套 0×0 portal / 滚动锚 / 嵌套 `rui_portal` | 滚动错位、叠层 Esc 穿透；**WP-3.4 ☑** 后视为回归 |
| 每实例自己的选中/开合 Signal（或调用方受控 Signal） | `use_signal("chart_pin")` 这类全局键 | 多图/多选器串状态 |

`untrack`：**关掉** `current_effect`，`reactive_anchor` 直接 return。  
`isolate_reads`：**保持** tracking，只丢弃这次收集的 deps。契约测：`untrack_skips_anchor_but_isolate_reads_registers`。

完整诊断表：[COMPONENT_LIBRARY.md §2.3](COMPONENT_LIBRARY.md)。

#### Derived values: `create_memo`

对依赖多个 `Signal` 的昂贵派生逻辑，使用 `rui::core::reactivity::create_memo`（返回 `rui::core::reactivity::Memo`）缓存结果；订阅方只依赖 memo 的输出信号，避免在根 `create_effect` 上拉入过多依赖。

```rust
use rui::core::reactivity::{create_memo, use_signal};

let a = use_signal("a", || 1_i32);
let doubled = create_memo(move || a.get() * 2);
let _ = doubled.get();
```

## Styling & Design Tokens

RUI components should use design tokens provided by the `DesignToken` system. This ensures consistency across your application and supports theme switching.

-   **Colors**: Use `parse_color("primary")`, `parse_color("background")`, etc.
-   **Dimensions**: Use `IntoDimension` which supports `rem`, `px`, and percentage strings.
-   **Typography**: Use `text_size`, `font_weight`, and `line_height` attributes.

## Performance Optimization

-   **Avoid clones**: Use signals for data that changes frequently.
-   **Static Trees**: The `ui!` macro generates specialized builder calls. Avoid complex logic inside the `ui!` block; perform calculations in the function body instead.
-   **Layout**: Use `HStack` and `VStack` for standard flexbox-like layouts. For custom positioning, use `Box` with absolute positioning or specific margins.

## Drawing primitives（W3）

Custom charts: prefer [`define_chart`] + built-in marks / [`custom_mark`] (same path kernel as Svg). Icons / freeform paint go through public **Svg / Canvas** leaves (not a second renderer). Spec: [`drawing.md`](drawing.md). `#[component]` may wrap `svg()` / `canvas().paint(…)`; keep paint callbacks on `peek` so they do not subscribe the component root.

## Hot Reload Compatibility

For a component to support hot reload:
1.  Prefer the `ui!` macro for the hot-patched template region (or a `ui!` subtree with a stable `template_id`).
2.  Pure Builder trees remain fully supported at runtime; they simply may not receive template hot patches.
3.  Avoid complex side effects during the `build` phase (function body).
4.  State should strictly reside in `signals`.
