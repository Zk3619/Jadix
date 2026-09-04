# RUI × GPUI 生态对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) 为准；底座禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](../ENGINEERING_GRADE_FRAMEWORK.md) §2–§3。  
> **对照源**：`Reference/gpui` · `Reference/gpui-component-main` · `Reference/gpui-vue-main` · `Reference/gpuix-main`；RUI 以 `crates/` 与工程文档为准。  
> **关联**：[REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md)（六参考 + 市场切割）· [FREYA_COMPARISON.md](FREYA_COMPARISON.md)（Freya 专章）· [component_inventory.md](../components/component_inventory.md)（RUI 分桶）  
> **日期**：2026-09-02（首版；基于 Reference 树快照与代码扫描）

---

## 0. 总判断

GPUI 生态与 RUI **不是同一条哲学路线**，但在 **「纯 Rust GPU 桌面 + 产品级组件密度」** 上构成最直接的外部标尺。

| 层级 | 代表 | 角色 |
|------|------|------|
| **渲染底座** | GPUI（Zed） | 原生窗口深度、Scene/Entity、AccessKit、IME、多窗——**原生可信度上限** |
| **Rust 组件库** | gpui-component | 60+ 控件、表格/编辑器/Dock/Markdown——**产品密度与商业验证标尺** |
| **前端绑定桥** | gpuix（React）· gpui-vue（Vue） | 保留树 + mutation 协议 + N-API/Wasm——**DX 与热重载叙事参照** |
| **本仓** | RUI | **纯 Rust · Solid 细粒度 · ShadCN 产品面**——不经过 JS 桥 |

一句话路线：

> **用 GPUI / gpui-component 的验收标准与密度清单卡自己；用 gpuix / gpui-vue 偷师「桥接层测试与热重载契约」；底座与反应性模型坚决不换。**

与 [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) §8 市场结论一致：RUI 争的是 **D 赛道（纯 Rust 自绘保留模式桌面）** 里 **Solid + ShadCN 味** 的空位，不是 React/Vue-on-GPUI 的替代品，也不是 Zed 内核的 fork。

---

## 1. 生态地图

```text
                    ┌─────────────────────────────────────────┐
                    │           应用开发者入口                  │
                    └─────────────────────────────────────────┘
           纯 Rust                    JS/TS 绑定（N-API / Wasm）
              │                                    │
    ┌─────────┴─────────┐              ┌───────────┴───────────┐
    │       RUI         │              │  gpuix (@gpuix/react)  │
    │  Signal + ui!      │              │  gpui-vue (Vue 3)      │
    └─────────┬─────────┘              └───────────┬───────────┘
              │                                    │
              │         ┌──────────────────────────┘
              │         │  RetainedTree + applyBatch()
              │         │  （mutation 协议，每帧 build_element）
              ▼         ▼
    ┌─────────────────────────────────────┐
    │      gpui-component（样式层）         │  ← Longbridge Pro 已产
    │      gpui-base（行为/无样式层）       │  ← 类 Base UI / Radix 行为
    └─────────────────┬───────────────────┘
                      ▼
    ┌─────────────────────────────────────┐
    │              GPUI（Zed）              │  ← Entity/View + Render
    │   gpui_platform / macos / windows …   │
    └─────────────────┬───────────────────┘
                      ▼
              Metal / DX11 / Vulkan / WebGPU
              Taffy · AccessKit · 平台文本
```

**shadcn 类比**（gpui-component 官方自述）：

| GPUI 生态 | Web 生态 |
|-----------|----------|
| GPUI | HTML + Tailwind |
| `gpui-base` | [Base UI](https://base-ui.com) |
| `gpui-component` | shadcn 样式组件层 |
| gpuix / gpui-vue | React / Vue 应用层 |
| **RUI** | **纯 Rust 一体化：Solid 反应性 + shadcn 面 + 自研 wgpu 栈** |

---

## 2. 定位与体量（快照 2026-09-02）

| 项目 | 语言 / 许可 | 成熟度 | 核心命题 | ~规模 | 相对 RUI |
|------|-------------|--------|----------|-------|----------|
| **RUI** | Rust · MIT | Alpha→Beta | Solid 细粒度 + winit/wgpu/taffy/glyphon + ShadCN | `crates/` ≈ **189k** `.rs` LOC；≈ **59** 组件模块 | 本仓主体 |
| **GPUI** | Rust · Apache-2.0 | pre-1.0 / **Zed 已产** | Entity/View + `Render`；GPU 原生窗口 | ≈ **116k** `.rs` | 原生深度上限；**无** Signal 图 |
| **gpui-component** | Rust · Apache-2.0 | **0.5.x** · crates.io · Longbridge Pro | `gpui-base` 行为 + `gpui-component` 样式；60+ 控件 | ≈ **275k** `.rs` | **密度标尺**；整库大于 GPUI 本体 |
| **gpui-vue** | Rust + TS · Apache-2.0 | **0.1.x** · 活跃 CI | Vue 3 custom renderer → N-API → RetainedTree | Rust ≈ **23k** + TS ≈ **8k** | 前端心智桥；**非** Rust 原生 |
| **gpuix** | Rust + TS · 见仓库 LICENSE | 预 1.0 · CLI + 发行示例 | React → mutation batch → GPUI | Rust ≈ **21k** + TS ≈ **15k** | 同上；生态更成熟（CLI、Web、发行包） |

> LOC 口径：`Get-ChildItem -Recurse -Filter *.rs` 行数合计；TS 仅计 `packages/`。不同快照日会有 ±5% 漂移。

---

## 3. 栈重叠速览

| 层 | RUI | GPUI | gpui-component | gpuix / gpui-vue |
|----|-----|------|----------------|------------------|
| **窗口** | winit | **平台原生**（`gpui_macos` / `gpui_windows` / `gpui_linux`）+ `gpui_platform` | 同 GPUI | 同 GPUI（经 N-API 线程模型） |
| **GPU** | wgpu（可选 dx12/vulkan feature） | Metal / DX11 / **wgpu**（`gpui_wgpu`） | 同 GPUI | 同 GPUI；gpuix 另有 **Wasm WebGPU** |
| **布局** | taffy Flex/Grid | **taffy** | 同 GPUI | 同 GPUI；样式经 `StyleDesc` 降级 |
| **文本** | glyphon / cosmic-text 定制 | 平台文本 + wgpu 路径 cosmic-text；自研 wrap | 富文本 / 编辑器 / LSP | 原生 `text` / `input` / `markdown` / `code` 元素 |
| **响应式** | **Signal / Owner** | Entity notify + `Render::render` 每帧重建 | GPUI 模型 + 组件内状态机 | **JS 框架 reconcile** → Rust RetainedTree |
| **组件面** | 自研 ShadCN 对齐 | 无（`div` 组合） | **60+** 生产级 | 宿主元素 + 少量 headless 复合件 |
| **a11y** | AccessKit MVP | AccessKit **深**（prepaint 全树） | 继承 GPUI + 语义件 | 依赖 GPUI；自动化测树 |
| **热重载** | `hot_reload_cli`（TCP JSON） | 无一等公民 | Story / examples | **bun --hot** 同窗 remount；gpuix Web Fast Refresh |

---

## 4. 相对成熟度（分析判断 1–5）

> 非 benchmark；供路线图优先级，不替代冒烟签署。

| 维度 | RUI | GPUI | gpui-component | gpuix | gpui-vue |
|------|-----|------|----------------|-------|----------|
| 原生桌面深度 | 3 | **5** | **5** | 4 | 4 |
| 细粒度响应式 | **5** | 2 | 2 | 1（React/Vue 组件级） | 1 |
| 布局引擎 | 4 | 4 | 4 | 4 | 4 |
| 文本 / 编辑器 | 3 | 4 | **5** | 4 | 4 |
| 组件密度 | 3 | 1 | **5** | 2 | 2 |
| 表格 / 虚拟化 | 3 | 2 | **5** | 3 | 3 |
| 测试 / 工具链 | 2 | 4 | 4 | 4 | 4 |
| 发布成熟度 | 2 | 3 | **4** | 3 | 2 |
| 纯 Rust DX | **5** | 4 | 4 | 1 | 1 |
| 前端栈 DX | — | — | — | **5** | **5** |
| 产品感外观 | 4 | 3 | **5** | 4 | 4 |

---

## 5. 各项目深读

### 5.1 GPUI（`Reference/gpui`）

**是什么**：Zed 拆出的 GPU UI 框架；Apache-2.0；[gpui.rs](https://gpui.rs) 文档站；crates.io `gpui` 0.2.x。

**crate 结构**（Reference 树顶层）：

| Crate | 职责 |
|-------|------|
| `gpui` | 核心：`Render`、`Context`、元素系统、Scene |
| `gpui_platform` | 跨平台应用入口、字体、窗口 |
| `gpui_macos` / `gpui_windows` / `gpui_linux` | 平台原生集成 |
| `gpui_wgpu` | wgpu 渲染路径 |
| `gpui_macros` | 派生宏 |
| `gpui_web` | 浏览器目标 |

**架构要点**：

- **模型**：`impl Render for MyView { fn render(&mut self, ...) -> impl IntoElement }`；状态多在 **Entity** 上，通过 `notify` 触发重绘，**非** Solid/Leptos 式依赖追踪。
- **帧循环**：每帧调用 `render` 构建元素树；GPUI 文档亦标为 immediate-mode 语义，靠 Entity 缓存与 Scene 优化摊销成本。
- **布局**：Taffy flexbox；样式 fluent API（`.flex()` `.gap_2()`）。
- **文本**：平台文本系统为主；Linux/wgpu 路径有 cosmic-text 集成；**整形与换行解耦**（`line_wrapper`）——见 [text_i18n_wrap_reference.md](../engine/text_i18n_wrap_reference.md) §2.3。
- **a11y / IME**：AccessKit 一等；`InputHandler` 全平台；Zed 级多窗、菜单、Actions/keymap。
- **组件库**：**无**；应用用 `div()` + 自建或依赖 gpui-component。

**优势**：原生可信度、已产验证、测试 harness、Scene 缓存、IME/多窗契约成熟。  
**劣势**：Entity 粒度粗；平台分叉重；默认无产品组件；与 Zed 仓库/发版节奏耦合。

---

### 5.2 gpui-component（`Reference/gpui-component-main`）

**是什么**：Longbridge 开源；从 **Longbridge Pro** 商业桌面提炼；crates.io **gpui-component 0.5.2**；README 宣称 **60+ 组件**、**120 FPS**、表格数十万行、编辑器 20 万行稳定。

**两层架构**（`docs/ARCHITECTURE.md`）：

```text
gpui-component（crates/ui）  →  完整样式、主题、图标、i18n
gpui-base（crates/base）     →  行为、状态、几何；无品牌视觉
GPUI
```

**设计原则**（官方）：

> Behavior belongs to the foundation. Presentation belongs to the application.

- **base 拥有**：焦点陷阱、弹出层碰撞、虚拟列表、日历网格、可调整面板、toast 栈、文本选区、键盘导航等**难交互**。
- **component 拥有**：颜色、圆角、密度、图标、变体、最终合成。
- 与 shadcn 对照：**base ≈ Radix/Base UI；component ≈ shadcn 样式层**。

**主要模块**（`crates/ui/src/lib.rs` 导出）：

| 类别 | 模块（节选） |
|------|----------------|
| 表单 | `button` `input` `checkbox` `radio` `switch` `slider` `select` `combobox` `color_picker` `form` |
| 浮层 | `dialog` `sheet` `popover` `hover_card` `tooltip` `menu` `command` |
| 数据 | `table` `virtual_list` `tree` `chart` `plot` `pagination` |
| 导航 | `tab` `sidebar` `breadcrumb` `stepper` `dock` |
| 文本/内容 | `text` `highlighter` `markdown`（经 text/attachment） `message` `message_scroller` |
| 布局 | `scroll` `resizable`（re-export base） `group_box` `collapsible` `accordion` |
| 壳 | `title_bar` `window_border` `status_bar` `native_menu` `notification` |
| 工具 | `theme` `inspector`（debug） `clipboard` `setting` |

**工程信号**：

- `rust-i18n` 多语言；`story` / `story-web` 演示；大量 `examples/`。
- 依赖 Zed 主线 `gpui` git + `gpui_platform` feature 全开。
- **体积**：组件库 LOC **大于 GPUI 本体**——产品密度来自多年商业打磨，非短期可抄。

**优势**：商业已产、组件最全、编辑器/表格/Dock 深度、base 分层可复用。  
**劣势**：绑定 GPUI/Zed 发版；无细粒度 Signal；C++/Slint 式「整视图 render」心智；Rust 纯应用若不要 JS 仍需学 GPUI Entity API。

---

### 5.3 gpuix（`Reference/gpuix-main`）

**是什么**：React 绑定 GPUI；作者 remorses；`@gpuix/react` + `@gpuix/cli`；**无 Electron、无 WebView**。

**架构**（README §Architecture）：

```text
React reconcile
  → applyBatch(JSON mutations)   // napi desktop / wasm-bindgen browser
  → RetainedTree（Rust）
  → GpuixView::render() → build_element() → GPUI 瞬时元素
  → Taffy layout → GPU
```

**关键设计**：

- **不对抗** GPUI 每帧重建：JS 侧保留树 + 最小 mutation 过 FFI；Rust 每帧从 RetainedTree **物化** GPUI 元素。
- **宿主元素**：`div` `text` `input` `markdown` `code` `virtual-list` `diff` `canvas` 等；样式含 `hover` 伪状态。
- **热重载**：`bun --hot` 同窗 remount React；Web 端 Fast Refresh + Wasm 单例不 reload。
- **工具链**：`bunx @gpuix/cli new`；发行 **chat** 示例二进制；Web 需 COOP/COEP。
- **GPUI 源**：`remorses/zed` fork（子模块 `zed/`），与上游 Zed 有漂移。

**优势**：React/TS 开发者零 Rust 即可出窗；mutation 协议清晰；Web+Desktop 双目标；示例与 CLI 成熟。  
**劣势**：**非细粒度**（React 组件级）；FFI + Node/Bun 运行时；`text` 须显式 `color`；浏览器事件回调尚不完整；依赖 fork 的 GPUI。

---

### 5.4 gpui-vue（`Reference/gpui-vue-main`）

**是什么**：Vue 3 **唯一**绑定；`gpui-vue-core`（Rust cdylib）+ `packages/gpui-vue`（renderer）。

**与 gpuix 同构差异**：

| 轴 | gpuix | gpui-vue |
|----|-------|----------|
| 前端框架 | React + `useState` | Vue 3 + `ref` / `v-model` |
| TS 规模 | ≈15k LOC | ≈8k LOC |
| 高级控件 | 文档偏宿主元素 | **headless shadcn 形**：Select/Combobox/Tooltip/Motion |
| 测试 | 示例 + 发行包 | **`mountGpui()`**、**`GpuiAutomation`**、Playwright 式 API |
| 多窗 | 单窗 `render()` 为主 | `createWindow()` 独立 Vue root |
| 音频/时间轴 | — | `useGpuiTimeline` `useGpuiAudioFrames` |
| GPUI 源 | remorses/zed | remorses/zed（同 rev 族） |

**NodeOps**：与 DOM reconciler 同构（`createElement` `insert` `patchProp` …）；commit 合并为单次 `applyBatch()`；样式 payload 去重与 sweep。

**优势**：Vue 生态；自动化测试 API 最完整；motion/composable 丰富。  
**劣势**：生态声量小于 gpuix；同样非 Rust 原生；版本 0.1.x。

---

### 5.5 RUI（本仓）

**是什么**：纯 Rust 保留模式 UI；Solid 式 `Signal`/`Owner`；ShadCN 产品面；`ui!` 与 Builder 双书写——见 [AUTHORITY.md](../AUTHORITY.md) §2。

**与 GPUI 生态的关键分野**：

| 轴 | RUI 选择 | GPUI 生态 |
|----|----------|-----------|
| 窗口 | **winit** 统一 | 平台原生 crate |
| GPU | **wgpu** 主线 | Metal/DX/wgpu 多路径 |
| 反应性 | **细粒度 Signal** | Entity notify / JS reconcile |
| 组件 | 自研 59 模块 | gpui-component 或 JS 宿主元素 |
| 语言边界 | **无 JS** | gpuix/gpui-vue 必需 TS 运行时 |
| 定位 | D 赛道 · Solid + ShadCN | D 赛道 · Zed 系 · 或 B 赛道前端迁移 |

**组件分桶**（详见 [component_inventory.md](../components/component_inventory.md)）：verified 约 20+；experimental 占多数；Chart/VirtualMasonry/Table 等已落地但信任度待 CV/golden。

---

## 6. 维度对照

### 6.1 响应式 / 树模型

| | RUI | GPUI | gpui-component | gpuix / gpui-vue |
|--|-----|------|----------------|------------------|
| **模型** | Signal + Owner | Entity + `Render` | 同 GPUI + 组件状态机 | JS VDOM → RetainedTree |
| **更新粒度** | `bind_*` / Keyed For | View 级 notify | 同左 | 框架 commit 级 |
| **每帧行为** | 脏子树/layout/paint | `render()` 重建元素 | 同左 | `build_element()` 物化 |
| **与 RUI** | — | 学 Scene/测试；**勿换 Entity** | 学交互/state 机；**勿换 render 主路径** | 学 mutation 测试；**禁搬 JS 桥** |

RUI 在「自绘 + 细粒度」轴上**相对 GPUI 全系最干净**；gpuix/gpui-vue 的 RetainedTree 是 **FFI 优化层**，不是 Signal 图。

### 6.2 窗口 · 渲染 · 布局 · 文本

| 层 | RUI 现状 | GPUI 生态强项 | RUI 差距 / 行动 |
|----|----------|---------------|-----------------|
| 窗口 | winit；次窗 MVP | GPUI 原生多窗、菜单、托盘 | 契约表扩写；托盘从 stub → 可演示 |
| 渲染 | wgpu SDF；FullClear 默认 | GPUI Scene；EUI 脏矩形（另参考） | PartialLoad backlog；Scene 批优化研究 |
| 布局 | taffy + 子树 dirty；**不 fork**（ADR-10 **已冻结**） | 同 taffy；component Dock/Tiles | VirtualMasonry ☑；上溢升父；栈预算见 [`stack_and_element_model.md`](../engine/stack_and_element_model.md) |
| 文本 | glyphon 深定制 | component **20 万行编辑器** + LSP | 块级 MD；wrap 解耦（gpui P3） |
| 表格 | Table experimental | **数十万行**虚拟表 | 列固定/调整/选区对标 component |

### 6.3 IME · a11y · 多窗

| 能力 | RUI | GPUI | gpui-component | gpuix / gpui-vue |
|------|-----|------|----------------|------------------|
| IME | Win 中文签 | 全平台 InputHandler | 富文本/编辑器级 | 原生 input textarea |
| AccessKit | MVP | 深 | 继承 + 语义件 | GPUI + 自动化树 |
| 焦点 | ring + roving | Actions/keymap | base focus_trap | 宿主 focus 事件 |
| 多窗 | 次窗 wired | 一等 | 多窗示例 | gpuix 单窗为主；vue `createWindow` |

### 6.4 组件 · 测试 · 发布

| 轴 | RUI | gpui-component | gpuix / gpui-vue |
|----|-----|----------------|------------------|
| 组件数 | ≈59 模块；verified 子集 | **60+** 生产级 | 宿主元素 + 少量复合 |
| 编辑器 | Input/Textarea | **Code editor + LSP** | `<code>` syntect |
| 图表 | `chart` grammar experimental | `chart` `plot` | canvas 命令 |
| 测试 | golden×7；crate_tests | story + CI | kittest 式 automation |
| 发布 | 未 crates.io | **crates.io 0.5.x** | npm `@gpuix/*` |

---

## 7. 组件覆盖对照（节选）

> RUI 列来自 [component_inventory.md](../components/component_inventory.md)；gpui-component 来自 `crates/ui` 导出。✓=成熟 / ~=有 experimental / -=弱或无 / plan=planned。

| 能力 | RUI | gpui-component |
|------|-----|----------------|
| Button / Input / Checkbox / Switch / Slider | ✓ verified | ✓ |
| Select / Combobox | ✓ verified | ✓ |
| Dialog / Sheet / Popover / Tooltip | ✓~ experimental 混 | ✓ |
| Tabs / Accordion / Collapsible | ✓ / experimental | ✓ |
| Table 大数据 | ~ experimental | **✓ 强项** |
| VirtualList / Masonry | ✓ experimental | ✓ VirtualList |
| Tree | ~ experimental | ✓ |
| Chart | ~ grammar | ✓ chart/plot |
| Code editor + LSP | - | **✓ 强项** |
| Markdown 块级 | plan / inline only | ✓ |
| Dock / Tiles 布局 | - | **✓ dock** |
| Date/Time/Color Picker | ~ experimental | ✓ |
| Command palette | ~ experimental | ✓ command |
| Rating / Tag / Badge | ~ / ✓ | ✓ |
| Native menu / Title bar | - / 部分 chrome | ✓ native_menu / title_bar |
| Motion / Spring | utility + CSS anim | base `motion` |
| Hot reload | TCP CLI | - / Story |
| React/Vue 绑定 | - | gpuix / gpui-vue |

---

## 8. 该学 / 不该搬

与 [gap_analysis.md](gap_analysis.md)、纲领 §3 一致。

| 参考 | 该学 | 不该搬 | 优先级 |
|------|------|--------|--------|
| **GPUI** | IME/光标区；多窗生命周期；Scene 缓存；AccessKit 深度；Actions 分层 | 弃 winit 换全原生分叉；Entity 取代 Signal | P0 原生 |
| **gpui-component** | **密度清单**：Table/Editor/Dock/MD；`gpui-base` 行为分层文档；Story 驱动；i18n | GPUI+component 整栈迁移；C++ 式整页 render 主路径 | P1 产品 |
| **gpuix** | mutation 批处理测试；`bun --hot` 同窗契约；CLI 上手路径；Wasm 单例热更策略 | React 主路径；Node 运行时依赖 | P1 对照 |
| **gpui-vue** | `mountGpui` / Automation API；headless 复合件形状；多窗 `createWindow` | Vue 绑定；N-API 架构 | P1 对照 |

### 决策卡片

| 类别 | 内容 |
|------|------|
| **坚持** | Solid Owner/Signal；winit + wgpu；taffy；glyphon；`ui!` + Builder；AccessKit |
| **优先偷师** | component 的 Table/Editor/Dock；GPUI IME/多窗；gpuix/vue 的无头测试形态；base 的「行为在下、样式在上」文档 |
| **明确避免** | 迁 GPUI 平台栈；引入 React/Vue 桥；用 `Render` trait 替换 Signal；为密度短期 fork gpui-component |
| **观察不跟** | remorses/zed fork 与上游合并节奏；gpui-component 对 Zed git 的硬依赖——仅在战略合作或栈统一时评估 |

---

## 9. 对 RUI 路线图的具体含义

嵌入 [AUTHORITY.md](../AUTHORITY.md) 波次，不新开架构编号。

| 波次 | 建议动作 | GPUI 生态对标 | 退出信号 |
|------|----------|---------------|----------|
| **W1 收口** | 三平台 smoke；IME 扩日韩 | GPUI InputHandler 契约 | 冒烟表可勾 |
| **W2 硬化** | golden≥5；无头 harness | gpui-vue Automation；GPUI test-support | 组件可信 |
| **W3 产品面** | Table 深化；块级 MD；Chart | component table/editor/markdown | Gallery 无 P0 洞 |
| **W4 发布** | crates.io；文档站 | gpui-component crates.io 节奏 | `cargo add rui` |

### 9.1 按项目的具体调整

**← gpui-component**

- 维护 **能力差距表**（§7），每季度对照 `crates/ui` 导出更新。
- Table：对标虚拟滚动、列固定、单元格选区——**实现仍走 RUI Signal + taffy**。
- 块级 Markdown / 代码高亮：优先读 component `text`/`highlighter` **交互契约**，不引入 GPUI 依赖。
- 文档：学习 `gpui-base` ARCHITECTURE.md 的「行为 vs 呈现」叙述，用于 [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) 复合件章节。

**← GPUI**

- [desktop_smoke.md](../ops/desktop_smoke.md) 扩成与 GPUI `InputHandler` 对齐的契约表。
- 文本：推进 [text_i18n_wrap_reference.md](../engine/text_i18n_wrap_reference.md) P3（整形/换行解耦）。

**← gpuix / gpui-vue**

- 无头测试：参考 `mountGpui` / tree snapshot / `findByTestId` 设计 RUI headless API（[headless.md](../components/headless.md)）。
- 热重载：RUI TCP 协议可对齐「同窗状态保留」语义，但**不**引入 JS 运行时。

---

## 10. 战略定位（与六参考文档合读）

| 问题 | 判断 |
|------|------|
| RUI 能否被 gpuix/gpui-vue **替代**？ | **不能互相替代**。前者要 React/Vue + Node；后者要纯 Rust + Solid。用户池重叠有限。 |
| RUI 能否跳过 GPUI 生态**独自**争 D 赛道？ | **可以且必须**。Iced/Xilem 也不依赖 GPUI；RUI 栈已锁定 winit/wgpu。 |
| gpui-component 是否是 RUI 的**直接竞品**？ | **是最近竞品**：同 ShadCN 叙事、同 GPU 桌面、组件更深。差异在 **Signal vs Render** 与 **栈独立 vs Zed 系**。 |
| 何时值得评估「迁 GPUI 底座」？ | 仅当 winit 路径在 IME/多窗/a11y 上 **持续无法签收** 且团队愿承担 Zed 耦合——当前纲领 **否**。 |

**KPI 改写**（延续 [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) §8.5）：

- 不追「GPUI 生态份额」，追 **「纯 Rust · 细粒度 · ShadCN」选型提及率**。
- 密度上追平 gpui-component 的 **表格 / 编辑器 / Dock** 可见能力，而非 60+ 数量本身。
- 测试上达到 gpui-vue 级 **自动化树 + 快照** 门禁。

---

## 11. 组件代码体量（为何 gpui-component 单文件更短）

gpui-component 的 `Collapsible` 合计 ~221 行，RUI 同功能 ~1108 行——**不是**同等实现下浪费 5 倍，而是 **motion 摊到 `gpui-base/motion/`（~1364 行共享）+ GPUI `Element` 抽象**，且 RUI 的 `accordion` / `collapsible` 存在 ~350 行重复 panel motion。

**完整根因、对照表、P0–P5 优化路线、未拆分单文件逐项深读**见专章 [COMPONENT_LAYERING.md](../components/COMPONENT_LAYERING.md)（§1.6 全景表 · §6 combobox/hover_card/color_picker/kbd/label 等）。

---

## 12. 交叉文档

| 文档 | 关系 |
|------|------|
| [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) | Slint/Xilem/EUI/Iced/egui 六参考 + 市场 |
| [FREYA_COMPARISON.md](FREYA_COMPARISON.md) | Freya 专章 |
| [AUTHORITY.md](../AUTHORITY.md) | 巩固期执行源 |
| [COMPONENT_LAYERING.md](../components/COMPONENT_LAYERING.md) | 组件体量 × gpui-component 分层；collapse 去重路线 |
| [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) | RUI 分层职责权威 |
| [component_inventory.md](../components/component_inventory.md) | RUI 组件分桶 |
| [text_i18n_wrap_reference.md](../engine/text_i18n_wrap_reference.md) | GPUI 文本换行对照 |
| [REACTIVITY.md](../REACTIVITY.md) | 细粒度机制；GPUI 不在反应性主标尺 |
| [headless.md](../components/headless.md) | 无头 harness 目标 |
| [desktop_smoke.md](../ops/desktop_smoke.md) | 原生冒烟 |

---

## 附录 A. Reference 路径索引

| 路径 | 说明 |
|------|------|
| `Reference/gpui/` | Zed GPUI 源码镜像 |
| `Reference/gpui-component-main/` | Longbridge gpui-component + gpui-base |
| `Reference/gpui-vue-main/` | Vue 3 绑定 monorepo |
| `Reference/gpuix-main/` | React 绑定 monorepo（含 `zed/` 子模块） |

## 附录 B. 快照命令（复现 LOC）

```powershell
$paths = @(
  "Reference/gpui",
  "Reference/gpui-component-main",
  "Reference/gpui-vue-main",
  "Reference/gpuix-main",
  "crates"
)
foreach ($p in $paths) {
  $loc = (Get-ChildItem $p -Recurse -Filter *.rs -File |
    Get-Content | Measure-Object -Line).Lines
  Write-Output "$p : $loc"
}
```
