# PROJECT: Rust Native UI Stack (SolidJS + Tailwind + ShadCN 风格)

## 1. 项目愿景与目标 (Vision & Goals)

本项目旨在从底层构建一个 **纯 Rust 原生、跨平台、极高性能的桌面/移动端 UI 技术栈**。
我们不复用臃肿的高层 GUI 框架（如 Iced, Dioxus Desktop），而是直接基于 Rust 图形与窗口底层的原语（Primitives）搭积木，旨在实现对代码 100% 的掌控权。

在开发现代 Web 应用时，前沿开发者往往倾心于如下组合：
- **SolidJS** (极其优异的性能，基于 Signal 的细粒度响应式，抛弃 Virtual DOM)
- **UnoCSS** (比 Tailwind 更快的即时按需原子 CSS 引擎，支持 Attributify 属性模式、纯 CSS 图标集和极致灵活的预设)
- **ShadCN/UI** (极简、可定制、高颜值、自带无障碍的组件集)

**本项目核心目标就是将这套 Web 开发的“黄金组合”在纯 Rust Native 环境下一比一重现，并提供超越传统原生开发的顶级 DX (如 Dioxus 级别的高速热重载)。** 打造一个运行极速、体积小巧、内存开销极低，且具备像素级一致性（因为自绘）的强力界面引擎。

---

## 2. 技术选型与分层架构 (Architecture & Tech Stack)

为了确保既能灵活搭建所有效果（如底层重写 Shader 处理阴影圆角），又避开手写系统级代码（如窗口钩子、字体排版）的焦油坑，我们采取了“核心底层复用，中高层次完全自研”的战略。

| 层级 (Layer) | 核心技术 / Crate | 职责边界与作用 |
| :--- | :--- | :--- |
| **Window & OS** | `winit` + `accesskit_winit` | 跨平台窗口创建、系统剪贴板、输入法 (IME)、基础事件分发、系统无障碍 (A11y) 树映射。 |
| **Graphics & Text** | `wgpu` + `glyphon` (`cosmic-text`) | **绘制层**。使用 WebGPU 标准编写高性能、像素级完美的圆角与阴影 Shader；通过 `glyphon` 解决地狱级难度的字体 fallback 与 Shaping 问题。 |
| **Layout Engine** | `taffy` | **排版层**。Rust 生态中最快的 Flexbox/Grid 引擎用于计算每个 DOM 结点的尺寸与坐标，完全对标 Web 标准布局。 |
| **Reactivity (自研)** | `SignalGraph` (SolidJS 风格) | **响应式层**。彻底抛弃 VDOM，实现 `create_signal` 与 `create_effect`。数据变化时，直接触发特定 UI Node 属性更新及特定区域的屏幕重绘 (Dirty tracking)。 |
| **Styling (自研)** | 取鉴 UnoCSS API | **引擎构建与样式层**。不使用冗长的 Class 解析，借鉴 UnoCSS 原则：**属性化模式 (Attributify)** 注入样式，支持动态 Shortcuts 和纯图形渲染管线（如 `bg="blue-400"` 对应内置色）。整合 Iconify 单类图标解析技术直接用 Shader 画图。 |
| **Developer Tools** | Dioxus 风格热重载 | **开发体验层 (DX)**。开发阶段，通过内生 CLI 工具监控 UI 宏树改变，通过 WebSocket 或 IPC 实现在不重编二进制文件的前提下热加载 (Hot Reloading) 样式和结构。|
| **Components (自研)**| 仿 ShadCN/UI 组件库 | **组件层**。以无头组件 (Headless) 思想为核心，剥离交互状态与视图，对外提供一套现代高颜值的基础组件，内置支持 UnoCSS 驱动的图标库。 |

---

## 3. 核心机制设计

### 3.1 零 VDOM 的细粒度响应式 (SolidJS Style)
借鉴 Freya 最新重构思路及 SolidJS 的精髓，我们剥离了传统的 `diff` 过程。组件函数仅在挂载时运行一次，所有对状态的访问 `signal.get()` 都会自动将其闭包或相关的 UI 属性修改函数注册为该 Signal 的订阅者。
当 `signal.set()` 被调用时，仅仅执行与其绑定的闭包，精准更新对应的 `taffy` 布局节点或 `wgpu` 的渲染顶点数据，做到真正的 O(1) 更新代价。

### 3.2 像素级完美的“ShadCN”渲染管线
ShadCN 风格的界面依赖于完美的亚像素**圆角变形 (Border Radius)**、平滑的**多层软阴影 (Soft Box Shadows)** 以及**动态模糊 (Backdrop Filter)**。
传统的图形库在画带有边框的圆角矩形时容易出现毛边。我们将编写专属的纯数学 SDF (Signed Distance Field) WGSL Shader 用于 UI 矩形渲染，确保在任意显示器（无论是 1080p 还是 4k HiDPI）下都能获得极致锐利、无锯齿的分辨率无关级 UI 渲染。

### 3.3 样式系统与 UnoCSS 理念设计
抛弃复杂的 CSS 文件与 AST 解析，利用 Rust 强大的宏或链式调用结合 Attributify 思想，建立瞬时原子框架。为兼顾庞大生态，系统将在**底座上完全兼容 Tailwind 核心体系**：
- **主题映射 (Themes)**：原生支持并向下兼容 Tailwind 实用类（`bg-primary` 等），并接入底层调色盘库（Palette）做即时色彩解析。
- **高性能原生动效 (Animations)**：像 `animate-spin` 这样的类名，将不再依赖低效的 CSS 插值，而是直接映射给 Rust 原生弹簧（Spring）多线程动效引擎与 GPU 管线。
- **灵活的动态组件 (Plugins)**：借助 UnoCSS 原则，开发者可借由宏系统极速派生自定义复合样式（Shortcuts），提供比 Web Plugin 更稳固的底层能力拓展。

```rust
// 构想中结合 UnoCSS Attributify 思想的 UI 构建
rect!(
    // 灵活按需原子 API (类似于 UnoCSS shortcuts 或 attributify)
    bg: "primary", 
    text: "primary-foreground",
    p: "x-4 y-2",
    rounded: "md",
    shadow: "sm",
    hover_bg: "primary/90",
    
    // 子元素中运用 Pure CSS Icon 思路，映射到自定义图形着色器
    icon!(name: "i-lucide-mail"),
    "Send Email"
)
```

### 3.4 极致开发者体验 (DX) 和毫秒级热重载
借鉴 Dioxus 的 `dx serve` 热重载工作流，打造即时响应的开发体验：
- **Token 热修补 (Hot-patching)**：框架自带一个宏和 CLI 工具，当检测到源码中 UI 宏内定义（尺寸、颜色、文本等）改变时，CLI 会提取 AST 或 token stream，通过本地 Socket 打给正在运行的程序进行动态执行映射。
- **免编刷新**：对纯布局、样式甚至部分轻逻辑修改，直接实现 Subsecond 级别界面刷新，极大避免 Rust "编译慢" 的痛点。

---

## 4. 详尽阶段计划 (Phased Implementation Plan)

这个工程浩大犹如构建一个小型的浏览器内核，必须划分为严格的里程碑逐步推进。

### Stage 1: 基础设施与渲染管线破冰 (Week 1-3)
**目标：成功创建一个窗口，并利用 GPU 画出携带 ShadCN 风格圆角与阴影的矩形。**
*   **任务 1.1:** 搭建项目工作空间 (Workspace)，配置 `winit` 事件循环。实现基础窗口的开启与关闭。
*   **任务 1.2:** 初始化 `wgpu` 上下文 (Device, Queue, Swapchain, Surface)。
*   **任务 1.3:** 编写出第一个 `SDF Rect Shader` (WGSL)。接受 Uniform 参数：长宽、背景色、边框宽度与颜色、四角不同的半径。
*   **任务 1.4:** 实现简单的阴影 Shader 或多 Pass 模糊方法。验证基础的 HiDPI 缩放正确性。

### Stage 2: 布局引擎与文本集成 (Week 4-6)
**目标：能够用代码构建树状结构，文本与矩形能按 Flexbox 规则自动排列。**
*   **任务 2.1:** 设计基础的 Scene Graph (UI 树状数据结构: Node, Component, Element)。
*   **任务 2.2:** 集成 `taffy` 布局引擎。遍历 UI 树将样式参数(宽高, Padding, Flex) 喂给 Taffy 并取得绝对坐标。
*   **任务 2.3:** 引入 `glyphon` 和字体文件。处理文字渲染，将其作为纹理传递给 GPU 管线与 UI 矩形一起绘制。
*   **任务 2.4:** 建立粗颗粒度的渲染批处理 (Batching)，合并同类型图元的 Draw Call，提高渲染效率。

### Stage 3: 细粒度响应式引擎 (Week 7-9)
**目标：拥有像 SolidJS 一样的状态流转，点击按钮能局部重绘。**
*   **任务 3.1:** 实现 Reactive Core (`Signal`, `Effect`, `Memo`)，处理依赖收集与通知。
    - **现状（2026-08）：** 已落地 Owner / Scope 树、`on_cleanup`、`create_context` / `use_context`、惰性 memo + 深度调度、`Signal::with` / `update`、`use_scoped_signal`。
*   **任务 3.2:** 事件系统的完善（射线检测 Hit Testing）。将 `winit` 捕获的鼠标/键盘事件精准路由到排版计算好坐标的 Node 节点。
*   **任务 3.3:** 数据绑定机制。当 Signal 触发修改时排期 (Schedule) 受影响的 UI Node 参数。
*   **任务 3.4:** Dirty Tracking。计算出需要重绘的矩形区域（脏区），让 `wgpu` 执行 Scissor 测试，仅渲染变动部分以降低功耗。
    - **现状（2026-08）：** 响应式层已做子树 reconcile + 按需 `request_redraw`；GPU 侧仍为每帧全屏 `LoadOp::Clear`，**尚未**实现 scissor / swapchain damage。文档与实现以此为准，勿再宣称「已有脏区重绘」。

### Stage 4: 样式层 (UnoCSS 理念) 与动画引擎 (Week 10-12)
**目标：提供类 UnoCSS、灵活并具备 Attributify 体验的样式宏声明，以及平滑的动效支持。**
*   **任务 4.1:** 开发类 UnoCSS 语法的解析器(用于宏)。支持 Attributify 传参，将诸如 `bg-primary`, `p-4` 瞬间拆解映射至引擎底层参数。
*   **任务 4.2:** 弹簧动画 (Spring Animation) 引擎开发。拦截 Signal 数据变更，平滑地用时间插值过渡到新值，这对 ShadCN 质感至关重要。
*   **任务 4.3:** 建立图标引擎。借鉴 "Pure CSS Icons"，把流行图标集在编译时合并转化为紧凑的 GPU 画刷数据 (SVG to GPU Shader / Texture)。

### Stage 5: 热重载基建 (Hot Reload CLI) (Week 13-14)
**目标：复刻 Dioxus 那种 "dx serve" 级别的毫秒级样式开发刷新。**
*   **任务 5.1:** 编写外部 CLI 监视器 (Watcher)。监控 rs 源文件变化。
*   **任务 5.2:** 运行时桥接。在 Debug 模式内置 Socket 监听器接受 CLI 的修改指令。
*   **任务 5.3:** 宏的 AST 动态提取与替换算法，实现“免重新编译”注入，触发 Dirty Tacking 立刻完成屏幕重绘。

**当前进度（已启动，第一版链路已打通）**
- 已新增 `rui_hot_reload_cli`（Watcher），监听 `.rs` 变动并向本地 `127.0.0.1:39091` 发送 `reload` 指令。
- 已在 `rui` Debug 运行时内置 TCP 监听器，接收 `reload` 后触发 `app_builder` 重新构建 UI，并立刻 `request_redraw`。
- 下一步进入任务 5.3：在 `rui_macros` 增加 AST 差分与最小补丁协议，替换“整树重建”为局部热替换。

**当前进度（Stage 5.3 第二版协议）**
- CLI 已升级为发送结构化补丁消息 `template_patch`（JSON），包含变更文件路径与 `ui!` 模板片段摘要（hash/len）。
- Runtime 已支持解析 `reload` / `template_patch` 两类消息；收到补丁后先走“整树重建”兜底，并记录模板补丁信息。
- 下一阶段将把 `template_patch` 与宏 AST 差分结果对齐，落地“局部节点注入 + Dirty Tracking”。

**当前进度（Stage 5.3 第三版：模板级定向刷新）**
- `ui!` 宏已为生成树根节点注入稳定 `template_hash`。
- CLI 与 Macro 使用同一套模板规范化 + FNV-1a 哈希算法，确保热补丁 hash 可在运行时命中。
- Runtime 收到 `template_patch` 时，会先做 hash 命中检测；仅命中当前 UI 树模板时才触发重建，未命中则跳过刷新。

**当前进度（Stage 5.3 第四版：局部节点注入）**
- 修复模板匹配策略：由“内容 hash 身份”改为“模板 ID（文件路径 + 宏起始行号）”。
- `template_patch` 增加 `text_literals`，Runtime 对命中模板子树进行 DFS 文本节点局部注入。
- 事件循环补齐主动唤醒：收到热补丁后在 `about_to_wait` 阶段主动请求 `redraw`，避免窗口静止时补丁不生效。

### Stage 6: ShadCN 层与无头组件建设 (Week 15-18)
**目标：逐步用自建的引擎复刻 ShadCN 组件库，并达到产品级可用。**
*   **任务 6.1:** 头文件与基础元素封装 (`div`, `span`, `input`)。此时必须去深入适配 `winit` 的 IME (中文输入法) 支持。
*   **任务 6.2:** 基础组件开发：`Button`, `Input`, `Checkbox`, `RadioGroup`, `Slider` 等。
*   **任务 6.3:** 复杂覆盖层组件开发：利用引擎能力处理 Z-Index、焦点管理以及 Portal 技术，开发 `Select`, `Dialog`, `Popover`, `Toast`。

### Stage 7: 抛光与企业级特性 (Long-term)
**目标：开源并吸引社区参与，补充核心外围能力。**

**排期摘要：** 见 [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md)。**运行时**已集成 `accesskit_winit` + 每帧树更新；剩余工作见该文档（Headless CI、Router、文档站等）。

*   **任务 7.1:** 全面接入 `accesskit` 提供系统级无障碍支持（屏幕阅读器，自动化测试挂载点）。
*   **任务 6.2:** Headless 测试环境。支持不唤起窗口直接在内存绘制供集成测试比对视觉差异。
*   **任务 6.3:** Router 系统搭建（适配桌面端状态路由）。
*   **任务 6.4:** 撰写翔实的中英文文档站与完整的 Examples 系统。

---
*Generated based on pure Rust graphics primitive design references.*
