# RUI × 六参考项目对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](AUTHORITY.md) 为准；底座禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) §2–§3；**细粒度反应性**见 [REACTIVITY.md](REACTIVITY.md)（Solid / Leptos / Floem / Dioxus **机制**对照 + §5.7 完整 FG 不变量）。  
> **对照源**：`Reference/gpui` · `Reference/slint-master` · `Reference/xilem-main` · `Reference/EUI-NEO-main` · `Reference/iced-master` · `Reference/egui-main`；反应性另见 `Reference/leptos-main` · `Reference/floem-main` · `Reference/SolidJS` · `Reference/dioxus`；RUI 以 `crates/` 与工程文档为准。  
> **日期**：2026-08-10（由对照 Canvas 落盘；同日补 iced / egui）；**2026-08-12** 链入 REACTIVITY*；**2026-08-13** 反应性四框架机制移交 REACTIVITY §5；公开自绘面见 [`drawing.md`](drawing.md)（对照 [canvas-ui](https://github.com/DavidHDev/canvas-ui)「包住子树」· [react-bits](https://github.com/DavidHDev/react-bits) 动效目录；不搬 HTML-in-canvas / CSS 运行时）

---

## 0. 总判断

RUI 的架构赌注（**Solid 细粒度 + winit/wgpu/taffy/glyphon**）相对六者仍是正确定位。差距在**原生可信度、文本/golden 质量门、组件深度与发布工程**——不是换哲学。细粒度工程卫生（`REGISTERED_*`→Owner、双路径）见 [REACTIVITY.md](REACTIVITY.md) / [AUTHORITY.md](AUTHORITY.md) WP-2.4，不拉低「模型轴」评分。

六参考各管一层：

| 参考 | 角色 |
|------|------|
| **GPUI** | 原生桌面深度上限 |
| **Slint** | 产品化工具链与发布契约 |
| **Xilem / Masonry** | 测试 / pass / a11y 纪律 |
| **EUI-NEO** | 组件密度与脏绘纪律 |
| **Iced** | 同栈近邻（winit/wgpu/cosmic-text）与 Elm 产品面 / DevTools |
| **egui** | 即时模式采用上限；集成 / kittest 对照；**禁搬架构** |

一句话路线：**用 GPUI/Slint 的验收标准卡自己，用 Xilem 的测试纪律硬化内核，用 EUI 的密度填洞，用 Iced 校准同栈产品与调试体验，用 egui 卡住「不做什么」——底座不动。**

市场结论见 §9：**完善后可在「纯 Rust 保留模式 + 产品感桌面」赛道争头部席位，但无法独占半壁江山**——Rust UI 是多赛道割据，不是单赢家市场。

---

## 1. 定位与体量

| 项目 | 语言 / 许可 | 成熟度 | 核心命题 | ~规模（快照） | 相对 RUI |
|------|-------------|--------|----------|---------------|----------|
| **RUI** | Rust · 工作区自有 | Alpha→Beta | Solid 式细粒度 + 自绘桌面 + ShadCN 组件面 | crates ≈ **57k** LOC | 本仓主体 |
| **GPUI** | Rust · Apache-2.0 | pre-1.0 / 已产（Zed） | Entity/View + 原生窗口；GPU UI | ≈ **116k** `.rs` | 原生深度上限；无 Solid 信号图 |
| **Slint** | Rust 核 · 三许可 | **1.18** 稳定 | `.slint` DSL + AOT；多语言/嵌入式/桌面 | Rust ≈ **298k** + `.slint` 大量 | 产品化标杆；非纯 Rust UI |
| **Xilem** | Rust · Apache-2.0 | **0.4** alpha | View-diff → Masonry；Vello/Parley | 树 ≈ **80k** LOC | 测试/pass/a11y 近邻 |
| **EUI-NEO** | C++17 · Apache-2.0 | **0.5.5** | 命令式 compose DSL；脏矩形 + 组件密度 | 自有 C++ ≈ **54k** | 产品密度标尺；禁搬架构 |
| **Iced** | Rust · MIT | **0.14** 发布 / 树内 **0.15.0-dev** | Elm TEA（State/Message/Update/View）；wgpu + tiny-skia | ≈ **84k** `.rs` | **栈最近邻**；粗粒度重建 vs Signal |
| **egui** | Rust · MIT/Apache-2.0 | **0.36.x**；接口仍流动 | 即时模式；任意三角形后端可嵌 | ≈ **107k** `.rs` | 工具/游戏 UI 采用王；纲领已否决架构 |

---

## 2. 栈重叠速览

| 层 | RUI | GPUI | Slint | Xilem | EUI | Iced | egui |
|----|-----|------|-------|-------|-----|------|------|
| 窗口 | winit | 原生 AppKit / Win32 / Wayland·X11 | winit / Qt / KMS | winit | GLFW / SDL2 | winit | winit（eframe）/ 引擎自嵌 |
| GPU | wgpu | Metal / DX11 / wgpu | FemtoVG / Skia / software | Vello→wgpu | OpenGL / Vulkan | wgpu + tiny-skia 回退 | glow / wgpu / 自定义 mesh |
| 布局 | taffy | taffy | 自研 + taffy flex | 自研 measure/layout | 自研 flex-ish | 自研 row/column/grid | 即时 horizontal/vertical/columns |
| 文本 | glyphon / cosmic-text | 平台文本系统 | parley 等 | Parley | FreeType | cosmic-text（wgpu 侧曾复用 glyphon Pipeline） | epaint 自研 shaping |
| 响应式 | Signal / Owner | Entity notify | Property 脏链 | View rebuild/diff | 整页 recompose | TEA：每消息 update + view 重建 | 每帧立即模式调用 |
| a11y | AccessKit MVP | AccessKit 深 | AccessKit 成熟 | AccessKit 一等 | 未作一等公民 | **未见 AccessKit 主线** | AccessKit + `egui_kittest` |

---

## 3. 相对成熟度（分析判断 1–5）

> 非运行时 benchmark；来源：reference 树 + RUI 工程文档快照 · 2026-08。

| 维度 | RUI | GPUI | Slint | Xilem | EUI | Iced | egui |
|------|-----|------|-------|-------|-----|------|------|
| 原生桌面深度 | 3 | **5** | **5** | 4 | 4 | 4 | 3 |
| 细粒度响应式 | **5** | 2 | 3 | 2 | 1 | 2 | 1 |
| 布局引擎 | 4 | 4 | 4 | 3 | 2 | 3 | 2 |
| 文本可信度 | 3 | 4 | 4 | 4 | 3 | 4 | 3 |
| 组件密度 | 3 | 2 | **5** | 3 | **5** | 4 | 3 |
| 测试 / 工具链 | 2 | 4 | **5** | 4 | 3 | **5** | 4 |
| 发布成熟度 | 2 | 3 | **5** | 2 | 3 | 4 | **5** |
| Rust 原生 DX | **5** | 4 | 2 | 4 | 1 | 4 | **5**（极简上手） |
| 产品感外观 | 4 | 3 | **5** | 3 | 4 | 3 | 2 |

---

## 4. 优劣势一句话

| 项目 | 优势 | 劣势 |
|------|------|------|
| **RUI** | Solid 细粒度最贴自绘；双书写面；热重载；taffy + utility 组件方向清晰 | 三平台冒烟未齐；golden 仅 2；crates.io/文档站未发；GPU PartialLoad 仍 backlog；组件信任度低于密度 |
| **GPUI** | IME/菜单/多窗/原生 GPU 已产；测试 harness 强；Scene 缓存成熟 | Entity 粒度非信号；平台分叉重；组件库靠拼 `div`；Zed 耦合感强 |
| **Slint** | 稳定 API、主题控件、LSP/预览、多语言多目标、行为测试海量 | DSL 税；许可复杂；默认非 wgpu-first；与「UI as Rust」目标相悖 |
| **Xilem** | Pass 系统清晰；截图测试文化；AccessKit/IME 一等；Masonry 控件基线 | rebuild/diff 非细粒度；无 taffy；alpha；eager view 需 memoize |
| **EUI-NEO** | 图表/Markdown/Picker/VirtualMasonry 密度；脏矩形与静态层缓存文档极深；体积路径友好 | C++；整页 recompose；布局弱于 taffy；a11y/发布叙事弱于 RUI 目标栈 |
| **Iced** | 与 RUI **同族栈**；TEA 心智清晰；0.14 起 IME/无头测/time-travel/devtools/热重载；widget 面宽（table/grid/markdown/pane_grid） | 非细粒度；无 AccessKit；布局非 taffy Flex/Grid；外观偏「框架默认」非 ShadCN 产品感；长期 experimental 叙事 |
| **egui** | 上手最快；crates.io 依赖面最大；引擎可嵌；AccessKit + kittest；Rerun 等已产 | 即时模式难撑无头/Portal/复杂 a11y 产品面；外观非原生；接口持续 breaking；纲领明确否决 |

---

## 5. 维度对照

### 5.1 响应式 / 树模型

| | RUI | GPUI | Slint | Xilem | EUI | Iced | egui |
|--|-----|------|-------|-------|-----|------|------|
| **模型** | Signal + Owner；挂载一次 | Entity notify + Render | Property 懒求值脏链 | View build/rebuild/diff | compose 重建草图 | TEA：Message → update → view | 每帧 `ui.button(...)` |
| **更新粒度** | `bind_*` / Show·For | View 缓存 + Scene replay | 属性依赖列表 | 类型化 diff + memoize | 常整页；交互可跳过 compose | 消息级整树 view（+ lazy/reactive 优化） | 全 UI 每帧重述 |
| **与 RUI 关系** | — | 可学缓存/测试，勿换 Entity | 可学 AOT/局部脏，勿换 DSL | 可学 pass/测试，勿换 reconcile | **禁搬** | 可学同栈工程/DevTools/测试，**勿换 TEA** | **禁搬架构**；可学集成与 kittest |

在「自绘 + 细粒度」轴上，RUI/Solid 路线相对六者最干净；Iced/Xilem/GPUI 偏粗粒度失效；egui/EUI 偏整帧/整页重建。

**反应性四框架**（不在上表六参考内；权威在 [REACTIVITY.md](REACTIVITY.md) §5）：Solid = 完整 FG 标尺（含 `mapArray` 项级 root）；Leptos = 可移植图协议 + RenderEffect；Floem = 原生 UpdaterEffect / `dyn_stack`；Dioxus = Copy Signal + **组件级 VNode diff**（有 Signal ≠ 细粒度渲染）。RUI 属性 `bind_*` 已对齐 Floem/Leptos 叶子绑；**列表仍是整表 effect + Element reconcile**（G6 / WP-R.5），勿在本文六参考表上把「细粒度响应式」打成满分后误以为 For 已追平 Solid。

### 5.2 窗口 · 渲染 · 布局 · 文本

| 层 | RUI 现状 | 对标强项 | RUI 差距 |
|----|----------|----------|----------|
| 窗口 | winit；次窗 MVP | GPUI 原生多窗/菜单；Slint muda/tray；Iced 多窗 + window | 托盘 stub；次窗焦点/Portal TLS |
| 渲染 | wgpu SDF + FullClear 默认 | EUI 脏矩形/层缓存；GPUI Scene；Iced layer merge / reactive rendering | GPU PartialLoad = B8；缺场景批优化 |
| 布局 | taffy Flex/Grid + 子树 dirty MVP | GPUI/Slint 同族 taffy；Iced row/column/grid；Xilem 自研 | 上溢升父规则未签；VirtualMasonry 缺 |
| 文本 | glyphon；WP-2.1 Win 签 | Xilem Parley↔a11y；GPUI 平台文本；Iced cosmic-text + shaping 策略 | paint 再 shape；golden 矩阵未齐 |

### 5.3 IME · a11y · 多窗

| 能力 | RUI | GPUI | Slint | Xilem | EUI | Iced | egui |
|------|-----|------|-------|-------|-----|------|------|
| IME | Win 中文签；日韩未测 | 全平台 InputHandler | 后端统一 InputMethod* | StartIme + TextArea | IME rect + composition | 0.14+ Input method / preedit | eframe web 移动端 IME 近年加强；桌面依集成 |
| AccessKit | 主路径 Win Narrator | prepaint 全树深 | 成熟 + 文本 a11y | Widget 一等 pass | 非一等公民 | **主线缺口**（相对 RUI/egui/Xilem） | 默认（eframe）+ 无障碍指南 |
| 焦点 / 键位 | ring + roving + Esc | Actions/keymap JSON | 编译期 focus pass | focus/disabled passes | 焦点 + 顶层 hit-test | widget operation / selector | `Sense` + Id 记忆 |
| 多窗 / 壳 | 次窗 wired；Router MVP | 一等多窗 + 通知 | 菜单/tray/Android | 多窗 + xilem_web | 模态多窗 + 托盘 | 一等多窗 + daemon | 逻辑 Window；壳靠 eframe/引擎 |

升 Beta 的硬缺口仍是 **macOS/Linux 冒烟与读屏复现**——向 GPUI/Slint 学契约清单，向 egui/Xilem 学 AccessKit 测试形态；Iced 证明同栈也能把 IME/无头测做深，但 **a11y 不是它的强项**。细则见 [desktop_smoke.md](desktop_smoke.md)、[ime.md](ime.md)、[a11y_smoke.md](a11y_smoke.md)。

### 5.4 组件 · 测试 · 发布

| 轴 | RUI | 强对标 | 行动含义 |
|----|-----|--------|----------|
| 组件 | ~44 模块；信任＜密度 | EUI 图表/MD/Picker；Slint 主题包；Iced table/grid/markdown/pane_grid | CV 门禁后再加深；Pie/VirtualMasonry 跟 EUI；表格/编辑器跟 Iced 能力表 |
| 测试 | 三平台 CI；golden×2 | Xilem 截图；Slint cases；Iced headless + e2e；egui_kittest | 组件 golden≥5；Harness 风格冒烟 |
| 工具链 | `hot_reload_cli` | Slint LSP；Iced comet/devtools/time-travel；egui inspection/MCP | 不造 DSL；Gallery + 可选 inspector（B1）；热重载对齐 Iced `hot` 叙事即可 |
| 发布 | 未 crates.io；无 CHANGELOG | Slint 1.x；egui 高频发版；Iced 0.14 契约 | W4：SemVer 面 + 文档站 + [PUBLISH.md](PUBLISH.md) |

---

## 6. 该学 / 不该搬

与 [gap_analysis.md](gap_analysis.md) §1、纲领 §3 一致：六参考各管一层；底座禁区不变。

| 参考 | 该学 | 不该搬 | 优先级 |
|------|------|--------|--------|
| **GPUI** | IME/光标区契约；多窗生命周期；Scene/view 缓存思路；`#[test]` harness；Actions 键位分层 | 弃 winit 换全原生平台分叉；Entity 取代 Signal；Metal/DX 双栈 | P0 原生 |
| **Slint** | 行为用例矩阵；主题/控件验收表；partial dirty 策略文档化；发布 feature 门 | `.slint` DSL / AOT 主路径；换 FemtoVG/Skia 默认；三许可模型 | P1 工程 |
| **Xilem** | Pass 语义（event→layout→paint→a11y）；TestHarness + 截图；WidgetMut 式元数据一致；IME signal 边界 | View-diff 取代 Signal；弃 taffy/glyphon 换 Parley/Vello 全家桶 | P0/P1 |
| **EUI-NEO** | Gallery 驱动；脏矩形合并策略；静态层缓存；图表/MD/Picker 密度清单 | 整页 compose DSL；GLFW/GL 换栈；FreeType 独占文本 | P1 产品 |
| **Iced** | 同栈工程成熟度：IME、无头/e2e、devtools、reactive rendering、layer 合并；widget 能力清单（table/editor/markdown）；feature 拆分与 CHANGELOG 节奏 | Elm TEA 取代 Signal；弃 taffy；为对齐 Iced 砍 AccessKit；整树 view 重建主路径 | P0/P1 同栈 |
| **egui** | `egui_kittest` + AccessKit 验收形态；极简集成边界；「工具 UI」与「产品 UI」市场切割话术 | 即时模式主路径；为体积/上手度放弃保留树与无头组件；epaint 换 glyphon | P1 对照 / **架构禁区** |

### 决策卡片

| 类别 | 内容 |
|------|------|
| **坚持** | Solid Owner/Signal；winit + wgpu；taffy；glyphon；`ui!` + Builder 双面；AccessKit |
| **优先偷师** | Xilem 截图门禁；GPUI IME/多窗契约；EUI 密度与脏绘文档；Slint 用例矩阵；**Iced 同栈测试/DevTools/IME**；egui kittest 形态 |
| **明确避免** | VDOM；EUI 命令式主模型；**egui 即时模式**；弃 wgpu 追体积；Slint DSL 主路径；GPUI 全平台自写窗口；**Iced TEA 换核** |
| **观察不跟** | Parley/Vello 生态（Xilem）；FemtoVG-wgpu（Slint）；GPUI Linux wgpu 文本栈；Iced tiny-skia 默认双渲染器——仅在 glyphon/体积瓶颈时再评估 |

---

## 7. RUI 优化与调整建议

下列建议**嵌进** [AUTHORITY.md](AUTHORITY.md) 波次，**不新开架构编号**；DevTools / GPU PartialLoad 仍属 Backlog。

### 7.1 按波次

| 波次 | 建议动作 | 主要对标 | 退出信号 |
|------|----------|----------|----------|
| **W1 收口** | 补签 macOS/Linux smoke #1–#9；JP/KR IME 可选；Narrator 外再签一平台读屏 | GPUI / Slint / Iced IME | 三列冒烟可勾 |
| **W2 硬化** | 上溢升父验收；`REGISTERED_*`→Owner；组件 golden≥5；测宽≡绘制进 CI；无头 harness 对齐 Iced/Xilem/egui_kittest 之一 | Xilem / Iced / egui | WP-2.3–2.6 + golden 门 |
| **W3 产品面** | Pie/Chart path；VirtualMasonry；Picker 键位/a11y；次窗 Portal/焦点隔离；托盘从 stub 到可演示 | EUI 密度；GPUI 多窗；Iced table/markdown | Gallery 八区无 P0 洞 |
| **W4 发布** | CHANGELOG + SemVer 稳/实验表；crates.io；文档站 Getting Started；feature 门对照 Slint/Iced 精简默认 | Slint / Iced / egui 发版节奏 | `cargo add rui` 可用 |

### 7.2 按参考项目的具体调整

**← GPUI**

- 把 [desktop_smoke.md](desktop_smoke.md) 扩成「契约表」：IME 光标区、候选窗、次窗焦点、菜单/快捷键分层。
- 中期：View/Scene 式 paint op 缓存研究（**不改** Signal）；短期勿分叉 Metal/DX。

**← Slint**

- 引进「用例文件」思维：每组件 1 页行为断言（焦点/禁用/键盘），不必上 DSL。
- 发布面：默认 feature 空、opt-in 重依赖——已与 [PUBLISH.md](PUBLISH.md) 一致，保持。

**← Xilem / Masonry**

- 抽取 pass 清单文档化（pointer→focus→layout→paint→a11y），对齐 masonry 概念层。
- TestHarness：headless 用户模拟 + `assert_render_snapshot`，替代仅靠人工 Gallery。

**← EUI-NEO**

- 组件优先级按 EUI gallery：Pie、VirtualMasonry、MD 块级、Picker 键盘；脏区文档继续写清默认 FullClear（见 [damage.md](damage.md)）。
- 体积：接受 wgpu 税；用 feature 拆分追「可裁剪」，不追 OpenGL 量级（见 [size_and_memory.md](size_and_memory.md)）。

**← Iced**

- 把「同栈近邻」用满：IME preedit、无头/e2e、devtools 指标、reactive/dirty 渲染叙事——对照其 CHANGELOG 能力表做 **差距勾选**，不对照其 TEA API。
- Widget 能力表：`text_editor` / `table` / `pane_grid` / incremental markdown 作为 W3+ 产品面参考，实现仍走 RUI 组件 + Signal。
- 发布：学习其 feature 粒度（wgpu / tiny-skia / markdown / highlighter 分门）与 SemVer 节奏；**不要**为对齐而引入第二套软件渲染器作为默认。

**← egui**

- 只偷 **测试与市场切割**：kittest/AccessKit 验收；对外明确 RUI ≠ 工具即时 UI。
- 纲领已否决即时模式（见 [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) §2.1）——任何「先 egui 再迁 RUI」原型路径不得污染主 crate API。

### 7.3 优先级排序

| 序 | 项 | 为何现在做 | 不做的代价 |
|----|-----|------------|------------|
| 1 | 三平台 smoke + a11y 复现 | Beta DoD 硬门槛；GPUI/Slint/egui 已证明可签 | 只能自称 Win Alpha+ |
| 2 | 组件 golden 矩阵 + 无头 harness | Xilem/Iced/egui 证明自动化门比文档有效 | 组件「有」但不「信」 |
| 3 | Owner/ID 与上溢升父 | 细粒度优势若泄漏全局表会空转 | 大型 App 难维护 |
| 4 | EUI/Iced 密度缺口（Chart/Masonry/Picker/Table） | 产品叙事对标需要可见差距关闭 | Gallery 看起来「半成品」 |
| 5 | crates.io + 文档站 + CHANGELOG | Slint/egui/Iced 证明契约 = 采用前提 | 无人 `cargo add` |
| 6 | GPU PartialLoad / DevTools | Iced comet 证明有价值但非 Beta 阻塞 | 可进 Backlog |

---

## 8. 若 RUI 完善后，能否占据 Rust UI「半壁江山」？

### 8.1 先定义市场

「Rust UI 库」不是单一品类，至少四条并行赛道：

| 赛道 | 代表 | 用户要什么 |
|------|------|------------|
| **A. 工具 / 游戏内嵌 / 原型** | **egui**（绝对优势） | 15 行出窗、可嵌引擎、不在乎原生观感 |
| **B. WebView / 前端心智桌面** | Dioxus、Tauri 系 | React/HTML 迁移、a11y「免费」、快速出货 |
| **C. DSL / 多语言 / 嵌入式产品** | **Slint** | 稳定契约、设计师工具链、非纯 Rust |
| **D. 纯 Rust 自绘保留模式桌面** | Iced、Xilem、GPUI、**RUI** | 无 WebView、类型安全、可做复杂应用壳 |

RUI 的设计目标落在 **D**，并额外押注 **Solid 细粒度 + ShadCN 产品感**——这是 D 赛道里差异化最强的组合，但 **不会自动吃掉 A/B/C**。

### 8.2 直答

| 问题 | 判断 |
|------|------|
| 完善后能否「半壁江山」（≈全 Rust UI 份额一半）？ | **不能，也不该以此为 KPI。** 赛道分裂是结构事实：egui 守住工具链、Slint 守住 DSL/商业、WebView 系守住前端迁移。 |
| 完善后能否在 **D 赛道** 进头部（与 Iced 并列或局部领先）？ | **可以。** 条件是 W1–W4 退出信号全部兑现，且组件信任度追上「有模块」的叙事。 |
| 完善后能否成为「产品感纯 Rust 桌面」的默认心智？ | **有机会成为候选默认之一**，前提是 crates.io + 文档站 + 三平台冒烟 + AccessKit 可读屏——这些是 Iced/egui 已付过的门票。 |

### 8.3 为何「半壁」不现实

1. **egui 的护城河是品类，不是功能表。** 它公开声明不做原生外观；工具作者不会为了 ShadCN 迁走。RUI 完善 ≠ egui 萎缩。
2. **WebView 赛道满足的是另一批人。** Narrator/IME「开箱」与前端招聘池，不是自绘栈短期能整体替代的。
3. **Slint 已占「稳定产品 + 多语言」心智。** RUI 坚持纯 Rust / 非 DSL，等于主动让出 C 赛道。
4. **D 赛道内部也会长期多强并存。** Iced（TEA + 同栈）、Xilem（Masonry 传承）、GPUI（Zed 已产）不会因 RUI 变好而消失；最多是份额重切。

### 8.4 完善后的现实天花板（乐观但可辩护）

若 RUI 达到「工程级」六条（纲领 §1）且持续发版：

| 结果 | 概率感（定性） | 含义 |
|------|----------------|------|
| D 赛道 **Top-2 心智**（与 Iced 二分「纯 Rust 桌面应用」讨论） | 高（若执行到位） | 博客/选型文里稳定出现；Gallery 可对标 Iced tour |
| 「要 Solid/ShadCN 味的 Rust 桌面」**默认选项** | 中高 | 这是 RUI 独有空位；Iced/egui/Xilem 都不占这个坑 |
| 全 Rust UI **半壁江山** | 极低 | 需 egui/WebView/Slint 同时让位——与各自激励不符 |
| 短期（&lt;2 年）crates.io 依赖数超 egui | 极低 | egui 是多年网络效应；先追「可发现 + 可信任」 |

### 8.5 战略含义（写进路线，不写进幻想）

- **KPI 改写**：不追「Rust UI 占比」，追「D 赛道选型提及率」与「`cargo add rui` 后 30 分钟跑通 Gallery 级 Demo」。
- **话术切割**：对标时说清——不是 egui 替代品，不是 Slint 替代品，是 **纯 Rust · 细粒度 · 产品组件面** 的桌面框架。
- **竞争焦点**：同赛道真正对手是 **Iced（同栈、更成熟工程）** 与未来 **Xilem（测试/a11y 纪律）**；egui 是边界对照，不是抢用户主战场。
- **执行仍回 §7**：半壁江山是叙事幻觉；**三平台冒烟、golden、crates.io** 才是进入 D 赛道头部的硬门票。

---

## 9. 交叉文档

| 文档 | 关系 |
|------|------|
| [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) | 权威纲领与禁区（含否决 egui 即时模式） |
| [AUTHORITY.md](AUTHORITY.md) | 巩固期唯一执行源 |
| [gap_analysis.md](gap_analysis.md) | 历史审计；§1 参考分层与本文一致（本文扩展 iced/egui） |
| [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) | 组件领域规范；CV 见 AUTHORITY Wave CV |
| [desktop_smoke.md](desktop_smoke.md) · [ime.md](ime.md) · [damage.md](damage.md) · [PUBLISH.md](PUBLISH.md) | 原生 / 脏区 / 发布细则 |
| [size_and_memory.md](size_and_memory.md) | 体积叙事：不追 egui/EUI 量级而弃 wgpu |
