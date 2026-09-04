# RUI - Rust Native UI Framework

> **GitHub Description (建议 / Suggested)**
>
> - 中文：基于 `winit + wgpu + taffy + glyphon` 的 Rust 原生 UI 框架，提供类 RSX 与纯 Rust Builder 双书写面、细粒度响应式与开发期热重载。
> - English: A Rust-native UI framework built on `winit + wgpu + taffy + glyphon`, featuring dual authoring (RSX-like `ui!` and pure-Rust builders), fine-grained reactivity, and dev-time hot reload.

---

## 项目简介 | Overview

**中文**  
RUI 是一个纯 Rust 原生 UI 工作区，目标是在不依赖高层 GUI 框架的前提下，构建现代化、可组合、可扩展的跨平台 UI 引擎。项目参考了 Docs 中的核心设计思路（分层架构、信号驱动更新、ShadCN 风格组件体验、开发期热重载），并已在实际代码中落地了可运行的核心链路。

**English**  
RUI is a pure Rust-native UI workspace aimed at building a modern, composable, and extensible cross-platform UI engine without relying on heavyweight high-level GUI frameworks. It follows the core design ideas documented in `Docs` (layered architecture, signal-driven updates, ShadCN-like component experience, and dev-time hot reload), with working implementations already present in code.

---

## 当前状态 | Current Status

- **中文**：当前阶段为 **Alpha / 实验性**，核心渲染、布局、文本、组件与宏链路可运行，API 仍在快速迭代。  
- **English**: The project is currently **Alpha / experimental**. Core rendering, layout, text, components, and macro pipelines are functional, while APIs are still evolving quickly.

---

## 核心特性 | Key Features

- **分层架构 | Layered architecture**  
  基于 `winit`（窗口/事件）、`wgpu`（GPU 渲染）、`taffy`（Flex/Grid 布局）、`glyphon`（文本）构建。

- **声明式 UI · 双书写面 | Dual authoring**  
  - **中文**：`ui!` 类 RSX 标签与 `button()` / `*Builder` 纯 Rust 链式 API **同等一等公民**，可互嵌；不强制类 HTML 标签。详见 [docs/COMPONENT_LIBRARY.md](docs/COMPONENT_LIBRARY.md) §3。  
  - **English**: RSX-like `ui!` tags and pure-Rust `*Builder` APIs are **both first-class** and nestable; HTML-like tags are optional. See [docs/COMPONENT_LIBRARY.md](docs/COMPONENT_LIBRARY.md) §3.

- **组件宏 | Component macro**  
  提供 `#[component]`，支持类似前端组件化的复用；函数体可用标签或 Builder 构建子树。

- **细粒度响应式 | Fine-grained reactivity**  
  通过 `use_signal` 驱动状态更新，减少不必要重绘。

- **组件与样式能力 | Components and styling**  
  内置 `Button`、`Input`、`Label`、`ScrollArea`、`Scrollbar` 等组件，支持类 utility 的样式属性写法。

- **开发期热重载 | Dev-time hot reload**  
  `rui_hot_reload_cli` 监听 `crates/` 与 `examples/` 变更，通过本地 TCP 通知运行时刷新模板与资源。

---

## 技术栈 | Tech Stack

- `winit` - windowing and input
- `wgpu` - GPU rendering backend
- `taffy` - layout engine (Flexbox/Grid)
- `glyphon` / `cosmic-text` - text shaping and rendering
- `accesskit` / `accesskit_winit` - accessibility bridge
- `syn` / `quote` / `proc-macro2` - macro infrastructure

---

## 工作区结构 | Workspace Structure

```text
rui/
├─ crates/
│  ├─ rui/                  # 核心运行时与组件系统 | Core runtime and components
│  ├─ rui_macros/           # #[component] / ui! 宏 | Proc-macro crate
│  ├─ rui_style_tokens/     # utility 真源 | Shared style tokens
│  └─ rui_hot_reload_cli/   # 热重载监听器 | Hot reload watcher
├─ examples/
│  └─ gallery/              # 组件与能力展示 | Component gallery
└─ docs/                    # 权威规范 + changelog；专题见子目录（AUTHORITY.md）
```

**文档入口 | Docs entry**：[AUTHORITY.md](docs/AUTHORITY.md)（唯一执行源）· [文档地图](docs/README.md) · [COMPONENT_LIBRARY.md](docs/COMPONENT_LIBRARY.md) · [REACTIVITY.md](docs/REACTIVITY.md) · [CHANGELOG.md](docs/CHANGELOG.md)

---

## 快速开始 | Quick Start

### 1) 环境要求 | Prerequisites

- Rust stable toolchain (`rustup`)
- 支持 `wgpu` 的图形驱动 / A GPU driver compatible with `wgpu`

### 2) 运行示例 | Run the demo

```bash
cargo run -p gallery
```

### 3) 启用开发期热重载（可选） | Enable dev hot reload (optional)

在一个终端运行示例，在另一个终端运行监听器：  
Run the demo in one terminal, then run watcher in another:

```bash
cargo run -p rui_hot_reload_cli -- .
```

默认会连接到 `127.0.0.1:39091`。  
By default, it connects to `127.0.0.1:39091`.

---

## 最小示例 | Minimal Example

```rust
#![allow(non_snake_case)]

use rui::component;
use rui::core::reactivity::use_signal;
use rui::ui;

#[component]
fn Counter() -> rui::core::tree::Element {
    let count = use_signal("count", || 0);

    ui! {
        <VStack gap="0.5rem" p="1rem">
            <Label>{ format!("Count: {}", count.get()) }</Label>
            <Button on_click={move || count.set(count.get() + 1)}>
                "Increment"
            </Button>
        </VStack>
    }
}

fn main() {
    rui::app::run(|| {
        ui! { <Counter /> }
    });
}
```

同一棵树也可用 **Builder（无类 HTML 标签）** 拼装，并与 `ui!` 互嵌，例如：

```rust
use rui::components::button;
use rui::core::tree::Element;
use rui::primitives::r#box::vstack;
use rui::ui;

fn toolbar() -> Element {
    Element::from(
        vstack().gap("0.5rem").child(
            // Builder 子树嵌进布局；亦可反过来在 ui! 里写 { Element::from(button(…)) }
            Element::from(button("save").variant("outline")),
        ),
    )
}

// 标签树中嵌入 Builder 产物：
// ui! { <VStack>{ toolbar() }</VStack> }
```

---

## 细粒度原语（阶段 3/4）| Fine-grained Primitives (Stage 3/4)

设计与热重载约定见 **[docs/ops/reactivity_and_hot_reload.md](docs/ops/reactivity_and_hot_reload.md)**。

- **`reactive!`**：为一段子树创建独立 effect，并通过稳定 anchor 做子树级 reconcile。  
  Use `reactive!(id, || ui! { ... })` to localize updates to a subtree.
- **`create_memo`**：派生只读值，依赖变化时重算；读者通过 `.get()` 订阅 memo 输出（见 `crates/rui/src/core/reactivity.rs`）。
- **`show`**：条件渲染辅助（`when == false` 时返回空节点）。  
  Conditional rendering helper that returns an empty element when false.
- **`<For>` / `for_keyed!` / `for_keyed`**：列表渲染；推荐标签式 `<For>`，需要显式 key 时传 `key={...}`。  
  List rendering with optional explicit keys (`key={...}`) for stable reconciliation.

```rust
use rui::core::reactivity::use_signal;
use rui::ui;

fn FineGrainedDemo() -> rui::core::tree::Element {
    let selected = use_signal("selected", || "default".to_string());
    let items = vec!["A", "B", "C"];

    ui! {
        <VStack gap="0.5rem">
            <Reactive id="demo:panel">
                <VStack gap="0.5rem">
                    <Show when={selected.get() != "default"}>
                        <Label>"Changed"</Label>
                    </Show>
                    <For each={items.clone()} key={*item}>
                        <Label>{ *item }</Label>
                    </For>
                </VStack>
            </Reactive>
        </VStack>
    }
}
```

---

## 架构目标 | Architecture Goals

- **中文**
  - 保留模式内部实现 + 声明式组件 API。
  - 信号驱动的局部更新（子树 reconcile + 按需 `request_redraw`）；整帧 GPU 绘制（脏区 / scissor 局部重绘尚未落地）。
  - 面向现代 UI 的组件体系与样式表达。
  - 持续提升开发体验（热重载、可维护宏系统、示例完善）。

- **English**
  - Retained internals with a declarative component API.
  - Signal-driven partial updates (subtree reconcile + on-demand `request_redraw`); full-frame GPU present (dirty-region / scissor redraw is not implemented yet).
  - Modern component system and style expression.
  - Better developer experience (hot reload, maintainable macros, richer examples).

---

## 路线图（简版） | Roadmap (Short)

- 完善组件分层与复用（primitives/components patterns）
- 加强宏能力与错误提示质量
- 优化布局与渲染热路径性能
- 扩展示例与文档覆盖面
- 持续推进无障碍与跨平台细节

---

## 贡献与反馈 | Contributing & Feedback

- **中文**：欢迎提交 Issue / PR 讨论 API、性能与组件设计。当前版本迭代较快，建议先在 Issue 中对齐方向。  
- **English**: Issues and PRs are welcome for API, performance, and component design discussions. Since the project is evolving quickly, opening an issue first is recommended.

---

## License

MIT — see [LICENSE](LICENSE).
