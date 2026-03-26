# RUI - Rust Native UI Framework

> **GitHub Description (建议 / Suggested)**
>
> - 中文：基于 `winit + wgpu + taffy + glyphon` 的 Rust 原生 UI 框架，提供类 RSX 声明式 API、细粒度响应式与开发期热重载。
> - English: A Rust-native UI framework built on `winit + wgpu + taffy + glyphon`, featuring RSX-like declarative APIs, fine-grained reactivity, and dev-time hot reload.

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

- **声明式 UI 宏 | Declarative UI macro**  
  提供 `ui!` 与 `#[component]`，支持类似前端组件化的开发体验。

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
rust-native-ui/
├─ crates/
│  ├─ rui/                  # 核心运行时与组件系统 | Core runtime and components
│  ├─ rui_macros/           # #[component] / ui! 宏 | Proc-macro crate
│  └─ rui_hot_reload_cli/   # 热重载监听器 | Hot reload watcher
├─ examples/
│  └─ hello_world/          # 功能演示应用 | Showcase app
└─ Docs/                    # 设计与规划文档 | Design and planning notes
```

---

## 快速开始 | Quick Start

### 1) 环境要求 | Prerequisites

- Rust stable toolchain (`rustup`)
- 支持 `wgpu` 的图形驱动 / A GPU driver compatible with `wgpu`

### 2) 运行示例 | Run the demo

```bash
cargo run -p hello_world
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

---

## 架构目标 | Architecture Goals

- **中文**
  - 保留模式内部实现 + 声明式组件 API。
  - 信号驱动的局部更新与脏区重绘。
  - 面向现代 UI 的组件体系与样式表达。
  - 持续提升开发体验（热重载、可维护宏系统、示例完善）。

- **English**
  - Retained internals with a declarative component API.
  - Signal-driven partial updates and dirty-region redraw.
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

暂未声明 License。若你计划开源发布，建议尽快补充许可证（如 MIT / Apache-2.0）。  
No license has been declared yet. If you plan to publish this repository, consider adding one early (e.g., MIT / Apache-2.0).
