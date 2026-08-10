# RUI × 四参考项目对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) / [COMPONENT_LIBRARY_PLAN.md](COMPONENT_LIBRARY_PLAN.md) 为准；底座禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) §2–§3。  
> **对照源**：`reference/gpui` · `reference/slint-master` · `reference/xilem-main` · `reference/EUI-NEO-main`；RUI 以 `crates/` 与工程文档为准。  
> **日期**：2026-08-10（由对照 Canvas 落盘）

---

## 0. 总判断

RUI 的架构赌注（**Solid 细粒度 + winit/wgpu/taffy/glyphon**）相对四者仍是正确定位。差距在**原生可信度、文本/golden 质量门、组件深度与发布工程**——不是换哲学。

四参考各管一层：

| 参考 | 角色 |
|------|------|
| **GPUI** | 原生桌面深度上限 |
| **Slint** | 产品化工具链与发布契约 |
| **Xilem / Masonry** | 测试 / pass / a11y 纪律 |
| **EUI-NEO** | 组件密度与脏绘纪律 |

一句话路线：**用 GPUI/Slint 的验收标准卡自己，用 Xilem 的测试纪律硬化内核，用 EUI 的密度填洞——底座不动。**

---

## 1. 定位与体量

| 项目 | 语言 / 许可 | 成熟度 | 核心命题 | ~规模（快照） | 相对 RUI |
|------|-------------|--------|----------|---------------|----------|
| **RUI** | Rust · 工作区自有 | Alpha→Beta | Solid 式细粒度 + 自绘桌面 + ShadCN 组件面 | crates ≈ **57k** LOC | 本仓主体 |
| **GPUI** | Rust · Apache-2.0 | pre-1.0 / 已产（Zed） | Entity/View + 原生窗口；GPU UI | ≈ **116k** `.rs` | 原生深度上限；无 Solid 信号图 |
| **Slint** | Rust 核 · 三许可 | **1.18** 稳定 | `.slint` DSL + AOT；多语言/嵌入式/桌面 | Rust ≈ **298k** + `.slint` 大量 | 产品化标杆；非纯 Rust UI |
| **Xilem** | Rust · Apache-2.0 | **0.4** alpha | View-diff → Masonry；Vello/Parley | 树 ≈ **80k** LOC | 测试/pass/a11y 近邻 |
| **EUI-NEO** | C++17 · Apache-2.0 | **0.5.5** | 命令式 compose DSL；脏矩形 + 组件密度 | 自有 C++ ≈ **54k** | 产品密度标尺；禁搬架构 |

---

## 2. 栈重叠速览

| 层 | RUI | GPUI | Slint | Xilem | EUI |
|----|-----|------|-------|-------|-----|
| 窗口 | winit | 原生 AppKit / Win32 / Wayland·X11 | winit / Qt / KMS | winit | GLFW / SDL2 |
| GPU | wgpu | Metal / DX11 / wgpu | FemtoVG / Skia / software | Vello→wgpu | OpenGL / Vulkan |
| 布局 | taffy | taffy | 自研 + taffy flex | 自研 measure/layout | 自研 flex-ish |
| 文本 | glyphon / cosmic-text | 平台文本系统 | parley 等 | Parley | FreeType |
| 响应式 | Signal / Owner | Entity notify | Property 脏链 | View rebuild/diff | 整页 recompose |
| a11y | AccessKit MVP | AccessKit 深 | AccessKit 成熟 | AccessKit 一等 | 未作一等公民 |

---

## 3. 相对成熟度（分析判断 1–5）

> 非运行时 benchmark；来源：reference 树 + RUI 工程文档快照 · 2026-08。

| 维度 | RUI | GPUI | Slint | Xilem | EUI |
|------|-----|------|-------|-------|-----|
| 原生桌面深度 | 3 | **5** | **5** | 4 | 4 |
| 细粒度响应式 | **5** | 2 | 3 | 2 | 1 |
| 布局引擎 | 4 | 4 | 4 | 3 | 2 |
| 文本可信度 | 3 | 4 | 4 | 4 | 3 |
| 组件密度 | 3 | 2 | **5** | 3 | **5** |
| 测试 / 工具链 | 2 | 4 | **5** | 4 | 3 |
| 发布成熟度 | 2 | 3 | **5** | 2 | 3 |
| Rust 原生 DX | **5** | 4 | 2 | 4 | 1 |

---

## 4. 优劣势一句话

| 项目 | 优势 | 劣势 |
|------|------|------|
| **RUI** | Solid 细粒度最贴自绘；双书写面；热重载；taffy + utility 组件方向清晰 | 三平台冒烟未齐；golden 仅 2；crates.io/文档站未发；GPU PartialLoad 仍 backlog；组件信任度低于密度 |
| **GPUI** | IME/菜单/多窗/原生 GPU 已产；测试 harness 强；Scene 缓存成熟 | Entity 粒度非信号；平台分叉重；组件库靠拼 `div`；Zed 耦合感强 |
| **Slint** | 稳定 API、主题控件、LSP/预览、多语言多目标、行为测试海量 | DSL 税；许可复杂；默认非 wgpu-first；与「UI as Rust」目标相悖 |
| **Xilem** | Pass 系统清晰；截图测试文化；AccessKit/IME 一等；Masonry 控件基线 | rebuild/diff 非细粒度；无 taffy；alpha；eager view 需 memoize |
| **EUI-NEO** | 图表/Markdown/Picker/VirtualMasonry 密度；脏矩形与静态层缓存文档极深；体积路径友好 | C++；整页 recompose；布局弱于 taffy；a11y/发布叙事弱于 RUI 目标栈 |

---

## 5. 维度对照

### 5.1 响应式 / 树模型

| | RUI | GPUI | Slint | Xilem | EUI |
|--|-----|------|-------|-------|-----|
| **模型** | Signal + Owner；挂载一次 | Entity notify + Render | Property 懒求值脏链 | View build/rebuild/diff | compose 重建草图 |
| **更新粒度** | `bind_*` / Show·For | View 缓存 + Scene replay | 属性依赖列表 | 类型化 diff + memoize | 常整页；交互可跳过 compose |
| **与 RUI 关系** | — | 可学缓存/测试，勿换 Entity | 可学 AOT/局部脏，勿换 DSL | 可学 pass/测试，勿换 reconcile | **禁搬**；纲领已否决 |

在「自绘 + 细粒度」轴上，RUI/Solid 路线相对四者最干净；其余更偏粗粒度失效或整页重建。

### 5.2 窗口 · 渲染 · 布局 · 文本

| 层 | RUI 现状 | 对标强项 | RUI 差距 |
|----|----------|----------|----------|
| 窗口 | winit；次窗 MVP | GPUI 原生多窗/菜单；Slint muda/tray | 托盘 stub；次窗焦点/Portal TLS |
| 渲染 | wgpu SDF + FullClear 默认 | EUI 脏矩形/层缓存；GPUI Scene | GPU PartialLoad = B8；缺场景批优化 |
| 布局 | taffy Flex/Grid + 子树 dirty MVP | GPUI/Slint 同族 taffy；Xilem 自研 | 上溢升父规则未签；VirtualMasonry 缺 |
| 文本 | glyphon；WP-2.1 Win 签 | Xilem Parley↔a11y；GPUI 平台文本 | paint 再 shape；golden 矩阵未齐 |

### 5.3 IME · a11y · 多窗

| 能力 | RUI | GPUI | Slint | Xilem | EUI |
|------|-----|------|-------|-------|-----|
| IME | Win 中文签；日韩未测 | 全平台 InputHandler | 后端统一 InputMethod* | StartIme + TextArea | IME rect + composition |
| AccessKit | 主路径 Win Narrator | prepaint 全树深 | 成熟 + 文本 a11y | Widget 一等 pass | 非一等公民 |
| 焦点 / 键位 | ring + roving + Esc | Actions/keymap JSON | 编译期 focus pass | focus/disabled passes | 焦点 + 顶层 hit-test |
| 多窗 / 壳 | 次窗 wired；Router MVP | 一等多窗 + 通知 | 菜单/tray/Android | 多窗 + xilem_web | 模态多窗 + 托盘 |

升 Beta 的硬缺口仍是 **macOS/Linux 冒烟与读屏复现**——向 GPUI/Slint 学契约清单，不必学它们的平台分叉实现。细则见 [desktop_smoke.md](desktop_smoke.md)、[ime.md](ime.md)、[a11y_smoke.md](a11y_smoke.md)。

### 5.4 组件 · 测试 · 发布

| 轴 | RUI | 强对标 | 行动含义 |
|----|-----|--------|----------|
| 组件 | ~44 模块；信任＜密度 | EUI 图表/MD/Picker；Slint 主题包 | CV 门禁后再加深；Pie/VirtualMasonry 跟 EUI 密度 |
| 测试 | 三平台 CI；golden×2 | Xilem 截图；Slint cases；GPUI TestApp | 组件 golden≥5；TestHarness 风格冒烟 |
| 工具链 | `hot_reload_cli` | Slint LSP/预览/Figma | 不造 DSL；Gallery 驱动 + 可选 inspector（B1） |
| 发布 | 未 crates.io；无 CHANGELOG | Slint 1.x 契约 | W4：SemVer 面 + 文档站 + [PUBLISH.md](PUBLISH.md) |

---

## 6. 该学 / 不该搬

与 [gap_analysis.md](gap_analysis.md) §1、纲领 §3 一致：四参考各管一层；底座禁区不变。

| 参考 | 该学 | 不该搬 | 优先级 |
|------|------|--------|--------|
| **GPUI** | IME/光标区契约；多窗生命周期；Scene/view 缓存思路；`#[test]` harness；Actions 键位分层 | 弃 winit 换全原生平台分叉；Entity 取代 Signal；Metal/DX 双栈 | P0 原生 |
| **Slint** | 行为用例矩阵；主题/控件验收表；partial dirty 策略文档化；发布 feature 门 | `.slint` DSL / AOT 主路径；换 FemtoVG/Skia 默认；三许可模型 | P1 工程 |
| **Xilem** | Pass 语义（event→layout→paint→a11y）；TestHarness + 截图；WidgetMut 式元数据一致；IME signal 边界 | View-diff 取代 Signal；弃 taffy/glyphon 换 Parley/Vello 全家桶 | P0/P1 |
| **EUI-NEO** | Gallery 驱动；脏矩形合并策略；静态层缓存；图表/MD/Picker 密度清单 | 整页 compose DSL；GLFW/GL 换栈；FreeType 独占文本 | P1 产品 |

### 决策卡片

| 类别 | 内容 |
|------|------|
| **坚持** | Solid Owner/Signal；winit + wgpu；taffy；glyphon；`ui!` + Builder 双面；AccessKit |
| **优先偷师** | Xilem 截图门禁；GPUI IME/多窗契约；EUI 密度与脏绘文档；Slint 用例矩阵形态 |
| **明确避免** | VDOM；EUI 命令式主模型；弃 wgpu 追体积；Slint DSL 主路径；GPUI 全平台自写窗口 |
| **观察不跟** | Parley/Vello 生态（Xilem）；FemtoVG-wgpu（Slint）；GPUI Linux wgpu 文本栈——仅在 glyphon 瓶颈时再评估 |

---

## 7. RUI 优化与调整建议

下列建议**嵌进** [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) 波次，**不新开架构编号**；DevTools / GPU PartialLoad 仍属 Backlog。

### 7.1 按波次

| 波次 | 建议动作 | 主要对标 | 退出信号 |
|------|----------|----------|----------|
| **W1 收口** | 补签 macOS/Linux smoke #1–#9；JP/KR IME 可选；Narrator 外再签一平台读屏 | GPUI / Slint 冒烟表形态 | 三列冒烟可勾 |
| **W2 硬化** | 上溢升父验收；`REGISTERED_*`→Owner；组件 golden≥5（Button/Input/Dialog/VirtualList/Markdown）；测宽≡绘制进 CI 矩阵 | Xilem 截图；EUI dirty 纪律 | WP-2.3–2.6 + golden 门 |
| **W3 产品面** | Pie/Chart path；VirtualMasonry；Picker 键位/a11y；次窗 Portal/焦点隔离；托盘从 stub 到可演示 | EUI 密度；GPUI 多窗 | Gallery 八区无 P0 洞 |
| **W4 发布** | CHANGELOG + SemVer 稳/实验表；crates.io；文档站 Getting Started；feature 门对照 Slint 精简默认 | Slint 1.x 契约 | `cargo add rui` 可用 |

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

### 7.3 优先级排序

| 序 | 项 | 为何现在做 | 不做的代价 |
|----|-----|------------|------------|
| 1 | 三平台 smoke + a11y 复现 | Beta DoD 硬门槛；GPUI/Slint 已证明可签 | 只能自称 Win Alpha+ |
| 2 | 组件 golden 矩阵 | Xilem 证明截图门比文档有效 | 组件「有」但不「信」 |
| 3 | Owner/ID 与上溢升父 | 细粒度优势若泄漏全局表会空转 | 大型 App 难维护 |
| 4 | EUI 密度缺口（Chart/Masonry/Picker） | 产品叙事对标需要可见差距关闭 | Gallery 看起来「半成品」 |
| 5 | crates.io + 文档站 + CHANGELOG | Slint 证明契约 = 采用前提 | 无人 `cargo add` |
| 6 | GPU PartialLoad / DevTools | 有价值但非 Beta 阻塞 | 可进 Backlog |

---

## 8. 交叉文档

| 文档 | 关系 |
|------|------|
| [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) | 权威纲领与禁区 |
| [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) | 巩固期执行源（W1–W4） |
| [gap_analysis.md](gap_analysis.md) | 历史审计；§1 参考分层与本文一致 |
| [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) / [COMPONENT_LIBRARY_PLAN.md](COMPONENT_LIBRARY_PLAN.md) | 组件面与 CV 门禁 |
| [desktop_smoke.md](desktop_smoke.md) · [ime.md](ime.md) · [damage.md](damage.md) · [PUBLISH.md](PUBLISH.md) | 原生 / 脏区 / 发布细则 |
