# Crate 清单

[English](CRATES.md) | [简体中文](CRATES.zh.md)

Jadix 在 `crates/` 下提供 **27 个库 crate**。下表说明各包职责及应用中的使用方式；示例、Gallery 应用和内置实现模块不计入这些库。

平台配置与打包见[构建指南](BUILD.zh.md)，能力选择见 [Feature 清单](FEATURES.md)。

## 应用产品库：4 个

| 包与版本 | 源码与包内说明 | 职责与使用边界 |
| --- | --- | --- |
| `jadix` · `0.1.0` | [crates/jadix](../crates/jadix/README.md) | 应用主门面，整合元素、响应式 UI、布局、文本、渲染、窗口，以及可选的文档、媒体、无障碍和开发能力。普通应用从这里接入；Builder 不要求过程宏，`macros` 启用 `ui!` 等声明式写法。图形后端须按目标平台选择。 |
| `jadix_components` · `0.1.0` | [crates/jadix_components](../crates/jadix_components/README.md) | 原生基础控件、表单、导航、浮层、数据展示及可选的文档/代码控件，基于 Jadix 并通过 feature 转发相应能力。默认 feature 为空，`macros` 可选。 |
| `jadix_lucide` · `0.1.0` | [crates/jadix_lucide](../crates/jadix_lucide/README.md) | Lucide 图标数据、UI 接入及按需启用的动画播放、路径变形。默认启用 `ui`、`paths`；动画、SVG 和路径动画分别通过 `animated`、`svg`、`animation-path` 选择。 |
| `jadix_dnd` · `0.1.0` | [crates/jadix_dnd](../crates/jadix_dnd/README.md) | 面向应用的高级拖放产品库，组织拖动源、接收目标及拖放交互。Builder 写法不依赖可选的 `macros`。 |

## 核心、文档与开发工具：9 个

| 包与版本 | 源码与包内说明 | 职责与使用边界 |
| --- | --- | --- |
| `jadix_core` · `0.1.0` | [crates/jadix_core](../crates/jadix_core/README.md) | 与窗口、渲染器无关的 Owner、Signal、Store 和细粒度响应式机制，可用于共享状态与非 UI 逻辑。 |
| `jadix_animation` · `0.1.0` | [crates/jadix_animation](../crates/jadix_animation/README.md) | 动画时钟、弹簧、惯性、标量播放器和时间线。默认启用 `animation-base`；扩展算法和路径操作按需启用。 |
| `jadix_layout` · `0.1.0` | [crates/jadix_layout](../crates/jadix_layout/README.md) | CSS 风格尺寸、逻辑单位和网格轨道算法，为布局提供可复用基础类型，与承担布局求解的 Taffy 分工。 |
| `jadix_style_tokens` · `0.1.0` | [crates/jadix_style_tokens](../crates/jadix_style_tokens/README.md) | 共享的 Tailwind/Uno 风格工具类 token 目录及变体解析，使运行时与宏使用一致的样式词汇。 |
| `jadix_theme` · `0.1.0` | [crates/jadix_theme](../crates/jadix_theme/README.md) | 主题调色板、设计 token、颜色解析和经校验的覆盖配置。默认 `theme-shadcn`；可选 Fluent、Material 3、Liquid Glass。 |
| `jadix_document` · `0.1.0` | [crates/jadix_document](../crates/jadix_document/README.md) | GUI 无关的文本快照、文档高度索引与文档操作，按 feature 提供 Markdown、编辑、数学/科学内容、差异和语法支持。负责文档语义，具体原生呈现和交互由框架、组件产品层承接。 |
| `jadix_lsp` · `0.1.0` | [crates/jadix_lsp](../crates/jadix_lsp/README.md) | GUI 无关的语言服务器传输与 UTF-16 文本编辑适配；应用仍需配置语言服务器，不随包附带服务器实现。 |
| `jadix_macros` · `0.1.0` | [crates/jadix_macros](../crates/jadix_macros/README.md) | `ui!`、`#[component]` 等编译期 UI 编写能力。通常通过产品的 `macros` feature 引入；纯 Builder 应用不必直接依赖。 |
| `jadix_hot_reload` · `0.1.1` | [crates/jadix_hot_reload](../crates/jadix_hot_reload/README.md) | 框架无关的热更新协议和接收端。`syntax` 可选；`cli` 包含 `jadix-hot-reload` 可执行工具及文件监视依赖。 |

## 渲染、适配与系统集成：9 个

| 包与版本 | 源码与包内说明 | 职责与使用边界 |
| --- | --- | --- |
| `jadix_renderer` · `0.1.0` | [crates/jadix_renderer](../crates/jadix_renderer/README.md) | 框架使用的 GPU 计算核、背景模糊管线、图形兼容支持与 emoji 分类，属于渲染支撑库，不是应用入口。 |
| `jadix_media` · `0.1.0` | [crates/jadix_media](../crates/jadix_media/README.md) | 后端无关的媒体播放契约，供 Jadix 与外部适配器接入；定义契约不代表已提供解码器或系统播放器。 |
| `jadix_tasks` · `0.1.0` | [crates/jadix_tasks](../crates/jadix_tasks/README.md) | 运行时无关的异步取消、执行和事件契约，不强制选择具体执行器。 |
| `jadix_net` · `0.1.0` | [crates/jadix_net](../crates/jadix_net/README.md) | 后端无关的网络契约及可选 `json` 支持；应用接入具体网络实现。 |
| `jadix_tokio` · `0.1.0` | [crates/jadix_tokio](../crates/jadix_tokio/README.md) | 为 `jadix_tasks` 提供有界 Tokio 执行适配，在需要时引入 Tokio，不让核心状态和任务契约强依赖它。 |
| `jadix_clipboard` · `0.1.1` | [crates/jadix_clipboard](../crates/jadix_clipboard/README.md) | 维护后的 clipboard-rs 分叉，支持文本、HTML、RTF、文件及可选图像，并对齐 Jadix 的平台依赖版本。保留上游许可声明，实际能力取决于操作系统。 |
| `jadix_window_materials` · `0.1.0` | [crates/jadix_window_materials](../crates/jadix_window_materials/README.md) | 跨平台窗口背景材质选项与调节参数，以及可选的 Windows 合成接入。原生效果取决于系统支持。 |
| `jadix_window_state` · `0.1.0` | [crates/jadix_window_state](../crates/jadix_window_state/README.md) | winit/Jadix 桌面窗口状态的 TOML 持久化，默认启用 `winit` 接入，`jadix` 接入可选。Android/iOS/OHOS 移动应用的依赖配置不应引入这项桌面依赖。 |
| `jadix_accesskit_ohos` · `0.1.0` | [crates/jadix_accesskit_ohos](../crates/jadix_accesskit_ohos/README.md) | 固定版本的 OpenHarmony AccessKit 适配回移，用于原生无障碍接入。默认 `api-15`，可选 `api-24`，需要匹配的 OHOS SDK 与宿主集成。 |

## 平台与文本维护分叉：5 个

| 包与版本 | 源码与包内说明 | 职责与维护边界 |
| --- | --- | --- |
| `jadix_winit` · `0.31.0-beta.3-jadix.1` | [crates/jadix_winit](../crates/jadix_winit/README.md) | 维护后的窗口与事件循环实现。核心类型、公共支撑与各系统后端均作为本包模块提供，通过 feature 和 target 配置选择。 |
| `jadix_wgpu` · `0.1.0` | [crates/jadix_wgpu](../crates/jadix_wgpu/README.md) | 包含 Jadix 后端与着色器修改的 GPU 实现，内部整合 core、HAL、types、bridge 和修改后的 Naga。Rust 库名为 `wgpu`，提供 `wgpu::hal`、`wgpu::wgc`、`wgpu::wgt`、`wgpu::naga` 入口。 |
| `jadix_cosmic_text` · `0.1.0` | [crates/jadix_cosmic_text](../crates/jadix_cosmic_text/README.md) | 多行文本整形与布局分叉，包含 Jadix 的对齐、段落方向相关修改，通常通过框架文本栈使用。 |
| `jadix_glyphon` · `0.1.0` | [crates/jadix_glyphon](../crates/jadix_glyphon/README.md) | GPU 文本渲染分叉，包含 CPU 图集镜像、脏区批量上传、字形驻留追踪及后端/着色器适配，与配套的 cosmic-text、wgpu 分叉协同。 |
| `jadix_taffy` · `0.1.0` | [crates/jadix_taffy](../crates/jadix_taffy/README.md) | Flexbox/Grid 布局引擎分叉，包含 Jadix 的布局失效与固有尺寸缓存修改，与 `jadix_layout` 的单位、网格基础算法职责不同。 |

窗口、GPU、文本与布局维护实现位于 `crates/`。`vendor/` 中已打补丁的 GPUI 实现仅用于对照基准。上游源码基线、许可和归属信息见 [vendor/RELEASE_FORKS.json](../vendor/RELEASE_FORKS.json)；未经修改的依赖沿用上游包身份。
