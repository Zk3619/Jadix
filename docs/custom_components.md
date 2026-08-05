# Creating Custom Components in RUI

RUI uses a layered architecture that makes it easy to build reusable, high-performance components using functional patterns and procedural macros.

## The `#[component]` Macro

The primary way to define a component is using the `#[component]` attribute macro. This macro simplifies the definition of props and the construction of the component's internal tree.

### Basic Syntax

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

`use_signal("name", ...)` 的字符串键在**同一组件多实例**时会碰撞。请使用稳定且唯一的键，例如 `format!("counter:{id}", id = props.instance_id)`，或与 `reactive!` 的显式 anchor id 同步命名。详见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)。

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
1.  It must use the `ui!` macro for its tree definition.
2.  Avoid complex side effects during the `build` phase (function body).
3.  State should strictly reside in `signals`.
