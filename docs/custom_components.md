# Creating Custom Components in RUI

> 分层与选型见权威规范 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md)（含 **§3 双书写面**）；无头细则见 [headless.md](headless.md)。

RUI uses a layered architecture that makes it easy to build reusable, high-performance components using functional patterns and procedural macros.

## Two equivalent ways to write UI

Both are first-class and must stay interchangeable ([COMPONENT_LIBRARY.md §3](COMPONENT_LIBRARY.md)):

| Style | Example |
|-------|---------|
| **A · `ui!` tags** | `ui! { <Button variant="outline">"OK"</Button> }` |
| **B · Builder (no HTML-like tags)** | `Element::from(button("ok").variant("outline"))` |

You can nest either form inside the other (`ui!` children may be `{ Element::from(button(…)) }`; builders may `.child(ui! { … })`). Prefer whichever fits the call site; do not treat tags as the only “modern” API.

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

### How it Works

1.  **Props Generation**: The macro automatically generates a `MyButtonProps` struct containing the arguments of your function.
2.  **Element Implementation**: It implements `From<MyButtonProps> for Element`, allowing you to use `<MyButton />` inside other `ui!` macros.
3.  **Destructuring**: Inside the function body, the props are automatically destructured for easy access.

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

Use `use_signal` to manage internal component state. Signals are scoped to the component instance and persist across re-renders.

```rust
#[component]
pub fn Counter() -> Element {
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
2. **Owner 作用域**：[`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)，减少手写全局 id。  
3. 否则使用稳定唯一键，例如 `format!("counter:{id}", id = props.instance_id)`，或与 `reactive!` 的显式 anchor id 同步。

**不要**给每个子控件都发明一个 `id`「以防万一」。身份轴拆分见 [COMPONENT_LIBRARY.md §4.2](COMPONENT_LIBRARY.md)。热重载约定见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)。

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

## Hot Reload Compatibility

For a component to support hot reload:
1.  Prefer the `ui!` macro for the hot-patched template region (or a `ui!` subtree with a stable `template_id`).
2.  Pure Builder trees remain fully supported at runtime; they simply may not receive template hot patches.
3.  Avoid complex side effects during the `build` phase (function body).
4.  State should strictly reside in `signals`.
