# RUI Docs Site（stub）

Markdown-only 静态大纲；后续可换成 mdBook / VitePress（巩固期 WP-4.4）。

> **权威入口**：[AUTHORITY.md](../AUTHORITY.md)。领域规范与手册见下。历史选型文在 [`../archive/`](../archive/README.md)。

## Getting Started · 快速开始

| EN | 中文 | Link |
|----|------|------|
| Authority + unified plan | 唯一入口与执行计划 | [AUTHORITY.md](../AUTHORITY.md) |
| Engineering-grade charter | 工程级门槛与 ADR | [ENGINEERING_GRADE_FRAMEWORK.md](../ENGINEERING_GRADE_FRAMEWORK.md) |
| Component library | 组件库领域规范 | [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) |
| Variant / size / controlled API | 稳定面词汇与双书写审计 | [component_variants.md](../components/component_variants.md) |
| Reactivity | 细粒度响应式规范 | [REACTIVITY.md](../REACTIVITY.md) |
| Vision | 愿景与分层初衷 | [PROJECT.md](../archive/PROJECT.md) |
| Custom components | 自定义组件 | [custom_components.md](../components/custom_components.md) |
| Reactivity & hot reload | 响应式与热重载 | [reactivity_and_hot_reload.md](../ops/reactivity_and_hot_reload.md) |
| Headless patterns | Headless 模式 | [headless.md](../components/headless.md) |
| Color tokens | 颜色与混色 | [color.md](../engine/color.md) |
| Box model | 类 CSS 盒模型 | [box_model.md](../engine/box_model.md) |
| Drawing (Svg/Canvas/3D) | 公开自绘面 | [drawing.md](../drawing.md) |

### Dual writing · 双书写（Button / Input）

```rust
use rui::prelude::*;

ui! { <Button variant="outline">"保存"</Button> }
let _: Element = button("save").variant("outline").text("保存").into();

let name = use_signal("name", || String::new());
ui! { <Input id="name" value={name} /> }
let _: Element = input("name").value_signal(name).into();
```

完整词汇与审计表：[component_variants.md](../components/component_variants.md)。

## Desktop shell · 桌面外壳

| EN | 中文 | Link |
|----|------|------|
| Multi-window | 多窗口 / 无边框 / 透明模糊 | [multi_window.md](../ops/multi_window.md) |
| System tray | 系统托盘 | [tray.md](../ops/tray.md) |
| Desktop smoke | 真窗口冒烟 | [desktop_smoke.md](../ops/desktop_smoke.md) |
| IME | 输入法 | [ime.md](../ops/ime.md) |
| Publish to crates.io | 发布清单 | [PUBLISH.md](../ops/PUBLISH.md) |
| Size & memory | 体积与内存 | [size_and_memory.md](../ops/size_and_memory.md) |

## Historical · 历史（非执行源）

| EN | 中文 | Link |
|----|------|------|
| Gap analysis | 差距审计底稿 | [gap_analysis.md](../comparison/gap_analysis.md) |
| Stage 7–12 roadmap | 阶段七～十二历史排期 | [STAGE7_ROADMAP.md](../archive/STAGE7_ROADMAP.md) |
| Archived notes | 选型与早期随笔 | [archive/README.md](../archive/README.md) |

## Examples · 示例

```text
cargo run -p gallery
cargo run -p calculator
```

## Bilingual note

本目录为双语索引 stub：正文以 [`../README.md`](../README.md) 地图为准（根目录权威规范；专题在 `components/` / `engine/` / `ops/` / `comparison/` / `archive/`）。
