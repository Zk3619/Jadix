# RUI 权威入口与统一计划（巩固期）

> **文档地位**：巩固期**唯一入口**与**唯一执行源**（勾选、排期、DoD、禁区摘要）。  
> **日期**：2026-08-12  
> **前提**：阶段一～十二 MVP 已落地；底座锁定（ADR-1～9）。本文**不**新开架构分叉。

读本文即可知道：现在做到哪、下一步做什么、争议找哪份规范。  
三份领域规范仍写「是什么」；三份旧计划降为**冻结展开**，不再各自勾选。

---

## 0. 为何改造

巩固期曾并列三套「权威 + 计划」：

| 域 | 规范（是什么） | 计划（做什么） |
|----|----------------|----------------|
| 工程化 | `ENGINEERING_GRADE_FRAMEWORK.md` | `ENGINEERING_IMPLEMENTATION_PLAN.md` |
| 组件 | `COMPONENT_LIBRARY.md` | `COMPONENT_LIBRARY_PLAN.md` |
| 响应式 | `REACTIVITY.md` | `REACTIVITY_PLAN.md` |

另有跨层契约 `element_identity.md`、分桶表 `component_inventory.md`。每份都自带「文档拓扑」和勾选表，结果是：

1. **六个入口**：README / PROJECT / 各文 §拓扑 互相指向，不知从哪读起。  
2. **三套 WP 编号**（`WP-*` / `C-WP-*` / `R-WP-*`）描述同一件事（焦点、a11y、Owner、golden、Gallery、SemVer），双清单互掐。  
3. **门禁分散**：工程说 W2.4 才算「可预测」；组件说未过 CV 不准 C1；响应式说硬化不挡发布——没有一张表说清**谁挡 Beta、谁可并行**。  
4. **身份债横切三域**（`id` / `focus_id` / `template_id` / 信号键），却拆在三份计划里分期，顺序靠「对齐」段落维持。

**改造原则**：规范可分域；**计划必须合一**。领域争议回规范，执行争议只改本文。

---

## 1. 文档治理（谁说了算）

### 1.1 层级

```text
AUTHORITY.md                         ★ 唯一入口 + 唯一勾选源（本文）
│
├─ 领域规范（只写「是什么 / 禁区」，不写排期）
│   ENGINEERING_GRADE_FRAMEWORK.md   工程门槛 · ADR · 锁定栈
│   COMPONENT_LIBRARY.md             组件产品面 · 双书写 · API 契约
│   REACTIVITY.md                    细粒度模型 · 三档更新 · 缺口事实
│   element_identity.md              身份四轴（跨组件 / 宏 / 运行时）
│
├─ 数据表（随代码改，不另开计划）
│   component_inventory.md           分桶 + id 策略 + planned
│
├─ 操作手册（验收步骤，不是第二套 WP）
│   desktop_smoke · a11y_smoke · ime · keyboard_nav
│   reactivity_and_hot_reload · headless · custom_components
│   color · font* · damage · layout_subtree · text_align · box_model
│   glyphon_audit · PUBLISH · size_and_memory · multi_window · tray
│
└─ 非执行源
    三份 *_PLAN.md                   冻结展开（历史 WP 细则；勾选作废）
    gap_analysis · STAGE7_ROADMAP · PROJECT · REFERENCE_COMPARISON
    archive/ · rui.md · bugs.md · site/README
```

### 1.2 争议路由

| 问题类型 | 权威 | 禁止 |
|----------|------|------|
| 要不要换 VDOM / 弃 wgpu / 废 Builder | 纲领 ADR + 本文 §3 | 在组件 PR 里静默改哲学 |
| 默认交付面 = shadcn？双书写？ | `COMPONENT_LIBRARY.md` | 另起纯无头产品包 |
| 这次 `set` 触发什么？Store/Resource 语义 | `REACTIVITY.md` | 以「追平 Leptos API」替代门槛 |
| `id` 是不是 focus / template / 信号键 | `element_identity.md` | 三轴混用；空串当身份 |
| 这周做什么、是否升 Beta | **本文** | 在三份旧计划里改勾选充进度 |
| 某组件 verified / broken | `component_inventory.md`（回写规范 §5.2.1） | 只改 Gallery 文案 |

### 1.3 旧计划处置

| 文件 | 即日起 | 后续（可选） |
|------|--------|----------------|
| `ENGINEERING_IMPLEMENTATION_PLAN.md` | 文首标明「展开附录；勾选以本文为准」 | 正文吸收完毕后迁 `archive/` |
| `COMPONENT_LIBRARY_PLAN.md` | 同上；**CV 门禁并入本文 Wave CV** | 同上 |
| `REACTIVITY_PLAN.md` | 同上；R-WP 映射为本文 WP 别名 | 同上 |

旧编号（`C-WP-*` / `R-WP-*`）仅作别名，见 §5.0。手册里已写的 `WP-1.1` 等**保持不变**。

---

## 2. 一句话结论

RUI 底座已验证：**Solid 式细粒度 + 保留 Element/taffy 树 + `winit/wgpu/taffy/glyphon` + shadcn 产品面 / `ui!`↔Builder 双轨**。  
巩固期不是再选哲学，而是把**原生可信、文本/像素、反应性卫生、组件信任度、发布契约**收成可承诺的 SemVer 面。

| 维度 | 现状（2026-08-12） | 工程级 |
|------|---------------------|--------|
| 代码 | `crates/` ≈ 5.3 万行；`components/` ≈ 45 模块 | 稳定面文档化；破坏性可追踪 |
| 原生 | Win 冒烟 #1–#9 已签；macOS/Linux 未签 | 三平台签署 |
| 文本/像素 | 测宽对齐 ☑；damage 策略 ☑；golden **仅 2** 场景 | ≥5 组件 golden |
| 反应性 | API 主路径齐；**WP-2.4 注册表→Owner ☑**；**WP-2.7 根裸 get 可观测 ☑**（2026-08-13） | 长会话无泄漏；更新路径可预测 |
| 组件 | Gallery 八分区 ☑；**WP-CV.0–.4 ☑ · CV 已退出（2026-08-13）**；golden → WP-2.6 | 分桶 + 测/冒烟；未过 CV 不做 C1 大重构 |
| 发布 | `PUBLISH.md` 齐；无 CHANGELOG；未 crates.io | `cargo add rui` + 文档站 |

---

## 3. 锁定决策与禁区（摘要）

展开见纲领 §2 / §8。此处只列**不可再分叉**项。

| ID | 决策 |
|----|------|
| ADR-1 | 细粒度 Signal，不采用 VDOM |
| ADR-2 | 保留 Element + taffy `NodeId` |
| ADR-3 | 图形主线 wgpu；Windows 默认优先 GL 以降内存 |
| ADR-4 | 布局固定 taffy；文本 measure 对齐 Pretext 思想 |
| ADR-5 | utility + 变体，单一 `rui_style_tokens` |
| ADR-6 | 组件 = shadcn 产品面 + headless 骨架 |
| ADR-7 | 热重载基于 `template_id`；`template_id` ≠ 焦点/a11y 主键 |
| ADR-8 | 工程级以原生+发布为准，不以 Solid API 百分百对齐为准 |
| ADR-9 | `ui!` 与 Builder **双书写面**一等公民，禁止只留其一 |

**巩固期明确不做**：换 VDOM / EUI DSL / 弃 wgpu；Solid DOM/SSR/hydration；移动/鸿蒙一等公民；完整 CSSOM；平行「纯 Radix 组件包」；未过 CV 对全量组件做行为大重构；无验收大重构；DevTools（Backlog）。

**三档更新**（开发者红线，展开 `REACTIVITY.md` §3）：

| 档 | API | 禁止 |
|----|-----|------|
| 属性 | `bind_*` | 根 `app_builder` 裸 `get()` 订整树 |
| 结构 | Show / For / `switch_reactive` / `reactive!` | 用结构岛表达纯文本闪烁 |
| 高频 chrome | `ReactiveSync` / peek | effect 风暴驱动 paint |

---

## 4. 三域边界（不互相吞并）

| 域 | 规范管 | 计划（本文）管 | 不管 |
|----|--------|----------------|------|
| **工程** | 六门槛、栈、发布叙事、质量门命令 | W1 冒烟、W2 文本/damage/golden、W3 外壳、W4 crates.io | 单个组件 variant 名 |
| **组件** | 分层、双书写、受控 API、清单、质量门模板 | CV 走查（仅 `components/` 控件）、行为下沉、Gallery 覆盖 | 重写 Signal 内核；Theme/utility SemVer |
| **响应式** | API 清单、缺口 G1–G9、与组件的受控边界 | Owner 注册表、双路径、Store/EB 契约 | 浮层 placement、chrome 色值 |
| **身份**（横切） | 四轴不变量 I-1… | 审计 + 空串收敛 + scoped API（并入 WP-2.4 / CV） | 当「第四套计划」 |

组件受控 Signal 的**语义**以反应性规范为准；**暴露面**（`*_signal` / 非受控键）以组件规范为准；**键的生命周期**以身份契约 + WP-2.4 为准。

---

## 5. 统一执行计划

### 5.0 编号规则

- **勾选只在本节。** 旧 `C-WP-*` / `R-WP-*` 是别名，便于对照冻结计划。  
- 已有手册引用的 `WP-1.1`…`WP-4.7` **不改号**。  
- 新增包接在所属 Wave 末尾（`WP-2.7`、`WP-CV.*`、`WP-3.8`…）。

```mermaid
flowchart TB
  D0[D0 文档收口]
  W1[W1 原生可信]
  CV[Wave CV 稳定面验证]
  W2[W2 文本像素与内核]
  W3[W3 产品面]
  W4[W4 发布]
  D0 --> W1
  W1 --> CV
  W1 --> W2
  CV --> W3c[W3 组件下沉 3.4/3.7/3.8]
  W2 --> W3
  W2 --> W4
  W3 --> W4
  CV --> W4
```

| 波次 | 主题 | 挡 Beta？ | 建议工期* |
|------|------|-----------|-----------|
| **D0** | 文档治理（本文） | 否（但挡「再开第七套权威」） | 本次 |
| **W1** | 原生：IME / 焦点 / a11y / 三平台冒烟 | Win 已退出；**三平台列挡 Beta** | 补签 1 周级 |
| **CV** | 稳定面走查 + 身份审计 + 最小测网 | **挡组件大重构**；不单独挡 0.1 Win-first | 1–2 周 |
| **W2** | 文本已签；Owner/Store；golden；**盒模型收口** | **WP-2.4 / 2.5 / 2.6 ☑**；WP-2.9 不挡 Beta | 2–3 周 |
| **W3** | Gallery / Chart / 选器 / 外壳；CV 后才下沉行为 | 分区已有；深化不全部挡 Beta | 2–3 周 |
| **W4** | CHANGELOG / crates.io / 文档站 / CI | **挡 Beta** | 1–2 周 |

\*单人全职量级。每波结束：代码合入 + 测试绿 + **只改本文勾选** + 必要时改对应规范事实表。

### 5.1 D0 — 文档收口

| ID | 任务 | 状态 |
|----|------|------|
| **D0.1** | 本文落地；根 README / `docs/` 入口改指本文 | ☑ 2026-08-12 |
| **D0.2** | 三份 `*_PLAN.md` 文首冻结声明；三规范「执行源」改指本文 | ☑ 2026-08-12 |
| **D0.3** | 手册与历史文（gap / STAGE7 / PROJECT / REFERENCE / identity）改链 | ☑ 2026-08-12 |
| **D0.4** | （可选）吸收旧计划正文后迁 `archive/` | ☐ |

### 5.2 Wave 1 — 原生可信

**目标**：桌面主路径像系统控件。细则仍见冻结总计划 §3 与各手册。

| ID | 主题 | 别名 | 状态 |
|----|------|------|------|
| **WP-1.1** | IME 光标区与滚动 | — | ☑ Win #3/#4（中文）；☐ 日/韩可选 |
| **WP-1.2** | 焦点环 / Tab / roving / Esc 顶层 | C-WP-1.3 | ☑ Win #5–#7；库测绿 |
| **WP-1.3** | AccessKit 主路径（桥 + 读屏） | C-WP-1.4 桥侧 | ☑ Win Narrator #9；组件身份仍 → CV / WP-2.4 |
| **WP-1.4** | 三平台真窗口冒烟 | — | ☑ Win #1–#9；☐ **macOS / Linux** |

**W1 开发机（Win）已退出**（2026-08-10）。升 Beta 前补齐 macOS/Linux 列。

### 5.3 Wave CV — 稳定面验证（原组件门禁）

**目标**：把「Gallery 能演示」变成「敢承诺」的最小**控件**集合。  
**CV 范围**：仅 `components/` 产品控件（Gallery 分区 + inventory §1.1–1.7）。**不含** Theme / utility（样式承诺面，见 inventory §1.8）、Form helpers（Context，已降 `experimental`）。  
**硬规则：未过 CV 不得开 WP-3.4 / 3.7 / 3.8 行为与 API 大重构。**  
broken 修不好 → 降出稳定面，禁止假装 `verified`。

| ID | 主题 | 别名 | 状态 |
|----|------|------|------|
| **WP-CV.0** | 分桶清单 + prelude 双书写 | C-WP-0.2 / 0.3 | ☑ inventory + `rui::prelude` |
| **WP-CV.1** | 稳定候选 Gallery / 手工走查 | C-WP-V.1 | ☑ 2026-08-12；§5.2.1 行为控件走查完成；Form 降出稳定面 |
| **WP-CV.2** | 最小自动化网（Button/Input/Dialog/Tabs/Checkbox\|Switch 布局或行为） | C-WP-V.2 | ☑ 2026-08-12；`crate_tests/components/cv_stable_layout.rs` 矩阵 8 测 |
| **WP-CV.3** | P0 修障或降级 | C-WP-V.3 | ☑ 2026-08-12；inventory 稳定面文档手工复核修错；`broken` = 无；无未分类 P0 |
| **WP-CV.4** | `id` 过度使用审计（分类表） | C-WP-V.5 | ☑ 2026-08-12；inventory §2.1；稳定面空串 I-1 已修（`resolve_instance_id`） |
| **WP-CV.5** | 核心 golden 骨架 | C-WP-V.4 | ⏭ **并入 WP-2.6**（2026-08-13；不单独阻塞 CV） |

**CV 已退出**（2026-08-13）。退出条件 = 下表四项 + CV.0–CV.4；**CV.5 不挡退出**，视觉 golden 交给 WP-2.6（挡 Beta / 「宣称视觉稳定」）。

**CV 退出（进入组件下沉前）**

- [x] §5.2.1 **行为控件**已标 `verified`（Gallery 走查 ☑ 2026-08-12）；Form → `experimental`（非控件）  
- [x] 行为控件无未分类 P0（CV.3 ☑ 2026-08-12；inventory §2 `broken` = 无）  
- [x] Button · Input · Dialog · Tabs · Checkbox|Switch 有 layout/行为测 **或** 冒烟勾选（CV.2 ☑ 2026-08-12）  
- [x] 行为控件 id 策略已按身份契约分类；空串不写入 `focus_id`/`template_id`（CV.4 ☑ 2026-08-12）  

分桶真源：[`component_inventory.md`](component_inventory.md) §2（控件）。  
**当前 `verified`（控件）**：Button、Label、Separator、Card、Input、Textarea、Checkbox、Switch、RadioGroup、Slider、Select、Combobox、Dialog、Toast、Tooltip、Tabs、ScrollArea、Scrollbar。  
**Form**：降出 §5.2.1（Context helpers → `experimental`）；加深见 WP-3.9。  
**CV.2 核心矩阵**（2026-08-12）：Button/Input/Dialog/Tabs/Checkbox/Switch — `cv_*` layout + Checkbox/Switch 行为测；见 `crate_tests/components/cv_stable_layout.rs`。  
**CV.3**（2026-08-12）：稳定面 inventory 文档手工验证修错后签署；无未分类 P0，不降级。  
**CV.4**（2026-08-12）：稳定面 id 分类表 → inventory §2.1；空串 I-1 修 → `utils/instance_id::resolve_instance_id`；API 去必填 / Gallery 减噪 → **WP-2.8 ☑**。  
**CV.5**：不单独开工；golden 五场景 → **WP-2.6**。  
**样式承诺面**（Theme / utility）：非控件、不进 CV；SemVer 与验收见 inventory §1.8、规范 §5.2.2、WP-2.9 / [`color.md`](color.md)。

### 5.4 Wave 2 — 文本、像素与内核硬化

**目标**：测宽≡绘制可测；damage 默认策略；Owner/ID 不泄漏；golden 可扩展。

| ID | 主题 | 别名 | 状态 |
|----|------|------|------|
| **WP-2.1** | 测宽与 glyphon 分行对齐 | — | ☑ |
| **WP-2.2** | 细粒度 damage 策略可测 | — | ☑（GPU PartialLoad = B8，不挡） |
| **WP-2.3** | 子树布局上溢升父 | — | ☑ |
| **WP-2.4** | Anchors/Bindings 挂 Owner + scoped ID | R-WP-1；消费方 C-WP-2.6 | ☑ 2026-08-13；`registry_owner` 测 + `scoped_template_id`/`scoped_focus_id`；Gallery「布局 → WP-2.4」 |
| **WP-2.5** | Store produce + Resource→EB 契约 | R-WP-4 测 | ☑ 2026-08-13；`produce` 切片测 + `throw_resource_error` e2e + `docs/CHANGELOG.md`；Gallery「布局 → WP-2.5」 |
| **WP-2.6** | 组件 golden 五场景 | C-WP-3.3 | ☑ 2026-08-13；Button/Input/Dialog/Tabs/ScrollArea；`golden_component_*` + `tests/golden/golden_*.rgba` |
| **WP-2.7** | 双路径 / 根裸 `get()` 可观测 | R-WP-2 | ☑ 2026-08-13；建议进 Beta；0.1 Win-first 可注明已知限制 |
| **WP-2.8** | 身份 API 收敛（展示型去必填；空串；试点 scoped） | C-WP-2.6 | ☑ 2026-08-13；前置 CV.4 ☑；框架能力来自 2.4 |
| **WP-2.9** | 类 CSS 盒模型完善（文档 / utility / 测宽叶纪律 / 命中测） | — | ☐ **不挡 Beta**；细则 [`box_model.md`](box_model.md) |

WP-2.4 任务要点（已签）：注册表随 Owner dispose（Anchors + Bindings + COMPONENT_*）；`scoped_template_id` / `scoped_focus_id`；长会话无膨胀测 `crate_tests/reactive/registry_owner.rs`；Gallery「布局 → WP-2.4」。文档：`reactivity_and_hot_reload.md` + `element_identity.md` §7。细则见冻结 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md) R-WP-1。

WP-2.5 任务要点（已签）：`Store::produce` ≡ `update` + 切片 `PartialEq` 契约测；Resource Err→EB 经**显式** `throw_resource_error`（巩固期不自动桥）；`docs/CHANGELOG.md` 实验面草案；Gallery「布局 → WP-2.5」。细则见 R-WP-4。

#### WP-2.4 验收（可判定）

| 层级 | 命令 / 操作 | 通过标准 | 失败标准 |
|------|-------------|----------|----------|
| **A 库测（权威，必须）** | `cargo test -p rui --lib registry_owner` | `2 passed`：`owner_dispose_clears_anchor_binding_and_component_props` + `registry_does_not_grow_across_repeated_owner_cycles` | 任一 `FAILED` |
| **A′ scoped id 测** | `cargo test -p rui --lib scoped_template_and_focus` | `1 passed`；两 root 下同名 local 生成不同字符串，且含 `::tpl:` / `::focus:` | 断言失败 |
| **B Gallery（建议）** | `cargo run -p gallery` → 侧栏「布局」→ 卡片「WP-2.4)」 | 见下「B 逐步」 | 卸载后 a/b/p **高于**基线；或 scoped 行缺少 `::tpl:`/`::focus:` |

**B 逐步（Gallery）**

1. 若结构岛已显示 → 点 **「卸载岛」**。  
2. 点 **「刷新当前」**，再点 **「记当前为卸载基线」**（仅卸载态有效；挂载态点了不会写入基线）。  
3. **「挂载岛」** → **「刷新当前」** → `当前 a` 应 **≥ 基线 a + 1**。  
4. **「卸载岛」** → **「刷新当前」** → `当前 a/b/p` 均 **≤** 对应基线（等于通过；任一更高 = 泄漏失败）。卡片状态行会显示「通过 / 失败」。  
5. 重复挂载/卸载至 **cycles ≥ 10**，最后停在卸载态并刷新 → 仍须 **≤ 基线**，且状态为「手工判定: 通过」。  
6. 挂载态下两行须含 `::tpl:gal_wp24_isle` 与 `::focus:gal_wp24_btn`（前缀为 owner path）。

**签署规则**：A（+A′）绿即可勾 WP-2.4；B 用于人工抽查与回归演示，不替代库测。

#### WP-2.5 验收（可判定）

| 层级 | 命令 / 操作 | 通过标准 | 失败标准 |
|------|-------------|----------|----------|
| **A produce（权威）** | `cargo test -p rui --lib produce_notifies` | `produce_notifies_select_only_when_slice_changes` passed | `FAILED` |
| **A′ Err→EB（权威）** | `cargo test -p rui --lib resource_err_to_error_boundary` | `resource_err_to_error_boundary_via_throw_resource_error` passed | `FAILED` |
| **A″ create_resource（权威）** | `cargo test -p rui --lib create_resource_err_sets` | `create_resource_err_sets_state_and_bridges_to_eb` passed | `FAILED` |
| **A‴ 实验面草稿** | 打开 `docs/CHANGELOG.md` | 含 Store / Resource / `throw_resource_error` 实验表 | 缺条目 |
| **B Gallery（建议）** | `cargo run -p gallery` →「布局」→「WP-2.5)」 | 见下「B 逐步」 | ticks 误增；或注入 Err 无 fallback |

**B 逐步（Gallery）**

1. **B1 produce**：记下 `name_ticks` / `age_ticks` →「age+1」仅 age +1 →「改名」仅 name +1 →「同值再 set」两 ticks 不变，且 note 为「未通知（符合契约）」。  
2. **B2 Err→EB**：框内初始 `children ok`；旁路行 `resource=` 同步。→「注入 Err」框内 `fallback: gallery boom` 且旁路为 `Err(...)` →「复位 Ready」回到 `children ok` / `Ready(1)`。

**签署规则**：A+A′+A″ 绿且 CHANGELOG 草案在即可勾 WP-2.5；B 不替代库测。巩固期 Err→EB **显式**桥，不承诺自动桥。

WP-2.6 任务要点（已签）：稳定面五场景像素 golden（**Button / Input / Dialog / Tabs / ScrollArea**）；离屏 harness（LayoutTree→`RuiPipeline`→readback，chrome-only）；`RUI_UPDATE_GOLDEN=1` 重录。VirtualList / Markdown **不**进本门禁（非 verified）。

#### WP-2.6 验收（可判定）

| 层级 | 命令 / 操作 | 通过标准 | 失败标准 |
|------|-------------|----------|----------|
| **A 组件 golden（权威）** | `RUI_FORCE_SOFTWARE=1 cargo test -p rui --lib golden_component -- --test-threads=1`（Win 加 `--features backend-dx12`） | 五测 passed：`button` / `input` / `dialog` / `tabs` / `scroll_area` | 任一 `FAILED` / mismatch |
| **A′ 既有 offscreen** | `cargo test -p rui --lib offscreen` | clear + stage8 等既有测绿 | `FAILED` |
| **A″ fixtures** | `crates/rui/tests/golden/golden_*.rgba` | ≥5 场景 fixture：`golden_button_default` 等 | 缺文件 |
| **重录** | `RUI_UPDATE_GOLDEN=1` + 同上 A 命令 | 写入/更新 `.rgba`；PR 勿混无关重录 | — |

**签署规则**：A+A′+A″ 绿即可勾 WP-2.6；名单以稳定面为准，禁止把 experimental 写回 Beta 门禁。

WP-2.7 任务要点（已签）：lifecycle `mark_next_effect_as_ui_root`；诊断钩子 `ui_root_effect_dep_count` / `root_bare_get_count`；debug 构建根订阅非空时 warn 一次；库测 `crate_tests/reactive/root_bare_get.rs`；Gallery「布局 → WP-2.7」。细则见冻结 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md) R-WP-2。

#### WP-2.7 验收（可判定）

| 层级 | 命令 / 操作 | 通过标准 | 失败标准 |
|------|-------------|----------|----------|
| **A 库测（权威）** | `cargo test -p rui --lib root_bare_get` | `3 passed`：裸 get 可观测 + bind + Show 根 dep=0 | 任一 `FAILED` |
| **A′ 诊断 API** | 文档 / rustdoc：`mark_next_effect_as_ui_root` · `ui_root_effect_dep_count` | 公开可调用 | 缺 API |
| **B Gallery（建议）** | `cargo run -p gallery` →「布局」→「WP-2.7)」 | 见下「B 逐步」 | 对照基线失败（root_deps 抬高） |

**B 逐步（Gallery）**

1. 「刷新根 dep」→「记为基线」（记下 `ui_root_effect_dep_count`；卫生 Gallery 多为 0）。  
2. **B1 bind**：点「文案 +1」数次 → 文案应变。  
3. **B2 Show**：点「卸载/挂载面板」切换结构岛。  
4. 「对照基线」→ `root_deps` **不得高于**基线（本卡操作不新增根订阅）。  

**签署规则**：A 绿即可勾 WP-2.7；B 用于人工抽查与回归演示，不替代库测。不消除双路径（bind vs 结构岛仍为设计）。

WP-2.8 任务要点（已签）：展示型 `navbar()` / `bar_chart()` / `line_chart()` / `progress()` 去必填 id；空串不写 `Some("")`（Progress/Toggle → `resolve_instance_id`）；Toggle 非受控 `use_scoped_signal` 试点；Gallery 减噪 +「布局 → WP-2.8」。细则见冻结 [`COMPONENT_LIBRARY_PLAN.md`](COMPONENT_LIBRARY_PLAN.md) C-WP-2.6。

#### WP-2.8 验收（可判定）

| 层级 | 命令 / 操作 | 通过标准 | 失败标准 |
|------|-------------|----------|----------|
| **A 展示型（权威）** | `cargo test -p rui --lib navbar_without_id`；`bar_chart_without_id` | 无 `template_id` / 非 `Some("")` | `FAILED` |
| **A′ Toggle 试点** | `cargo test -p rui --lib toggle_empty_id`；`toggle_uncontrolled_uses_scoped` | 空 id 非空 focus/template；跨 Owner 独立 | `FAILED` |
| **B Gallery（建议）** | `cargo run -p gallery` →「布局」→「WP-2.8)」 | Navbar/Chart/Progress/Toggle 无手写 `gal-*`；Toggle 可点 | 仍强制堆展示型 id |

**签署规则**：A+A′ 绿即可勾 WP-2.8；B 不替代库测。

#### WP-2.9 盒模型完善（展开）

**目标**：把「Taffy + BorderBox 默认 + 百分比 content-box」收成可承诺、可测、可写 utility 的契约；**不**追求完整 CSSOM。

| 子项 | 任务 | 映射 [`box_model.md`](box_model.md) | 状态 |
|------|------|--------------------------------------|------|
| **2.9.1** | 手册落地；纲领/文档站/README 互链；Gallery「布局」区链到契约摘要 | BM1 | ☑ 手册 2026-08-12；入口链 ☐ |
| **2.9.2** | Builder `.border_box()` + utility `box-border` / `box-content`（`rui_style_tokens` + 宏映射） | BM2 | ☐ |
| **2.9.3** | 稳定面「单节点文案 + padding」审计：该 ContentBox 的改 ContentBox 或拆叶；表进 inventory 或本手册附录 | BM3 | ☐ |
| **2.9.4** | 库测：outline 不扩大 hit；overflow+padding 裁剪边；（回归）percent_content_box 保持绿 | BM7 | ☐ |
| **2.9.5** | `em` 语义：文档标明「现相对根 16px」；可选试点「相对本节点 font-size」并加测 | BM4 | ☐ |
| **2.9.6** | （P2）分边 border API + 绘制，或写入 Backlog B15 并关闭本子项为「延后」 | BM6 | ☐ → 默认可降 B15 |
| **2.9.7** | （P2）视口单位：文档诚实写清「布局根 Percent」；真 `vw`/`vh` 另议 | BM5 | ☐ → 默认可降 B15 |

**退出**：2.9.1–2.9.4 ☑；2.9.5 至少文档诚实；2.9.6/7 完成或显式进 §7 Backlog。  
**禁区**：换 Taffy；outline/shadow 进布局；块级 margin collapse。

**W2 已签署**：2.1–2.8。（2.9 并行；2.9 不挡升 Beta。）

### 5.5 Wave 3 — 产品面与外壳

**目标**：Gallery 可作名片；关键键盘完整；外壳可演示。  
**3.4 / 3.7 / 3.8 仅 CV 通过后开工。**

| ID | 主题 | 别名 | 状态 |
|----|------|------|------|
| **WP-3.1** | Gallery 八分区 | C-WP-3.1 | ☑ 侧栏；「每稳定面一入口」随 CV 复核 |
| **WP-3.2** | Path → Chart 加深（含 Pie） | — | ☐ |
| **WP-3.3** | Date/Time/Color + Command 键盘 | C-WP-3.2 | ☐ |
| **WP-3.4** | 浮层单一路径（popup / Dialog 壳 / Esc） | C-WP-1.1；总计划 3.4 | ☐ **前置 CV ☑** |
| **WP-3.5** | Router + 次窗 Gallery 演示 | — | ☐ wired MVP 有；演示页无 |
| **WP-3.6** | 系统托盘原生接线（feature） | — | ☐ 挡发布则可标 stub |
| **WP-3.7** | Toggle/Checkbox/Switch 共享状态原语 | C-WP-1.2 | ☐ **前置 CV ☑** |
| **WP-3.8** | variant/size/token/受控 API 归一 + 双书写审计 | C-WP-2.1–2.5 | ☐ **前置 CV ☑**；仅 `verified` |
| **WP-3.9** | Form 校验路径 | C-WP-3.4 | ☐ |
| **WP-3.10** | VirtualMasonry | 总计划 3.7 | ☐ **P2**；依赖 2.1（已满足）但不挡 Beta |

### 5.6 Wave 4 — 发布与质量矩阵

| ID | 主题 | 别名 | 状态 |
|----|------|------|------|
| **WP-4.1** | SemVer 承诺面 + CHANGELOG | C-WP-4.1 | ☐ |
| **WP-4.2** | path 依赖 → crates.io 版本 | — | ☐ |
| **WP-4.3** | 首次 `cargo publish` | — | ☐ |
| **WP-4.4** | 文档站 Getting Started + 组件索引（双写法） | C-WP-4.2 | ☐ |
| **WP-4.5** | CI：golden≥5 + 既有门禁 | — | ☐ |
| **WP-4.6** | 体积/内存发布核对 | — | ☐ |
| **WP-4.7** | 对外叙事升 Beta | — | ☐ 依赖 §6 全勾 |

### 5.7 反应性加深（不挡 Win-first 0.1）

并入 W2 之后或与 W3 交错；勾选仍只在本表。

| ID | 主题 | 别名 | 挡 Beta？ | 状态 |
|----|------|------|-----------|------|
| （见 2.4 / 2.5 / 2.7） | 门禁三项 | R-WP-1 / 4 / 2 | 2.6 是；2.7 建议 | 2.4 ☑；2.5 ☑；**2.7 ☑** |
| **WP-R.5** | keyed For / Index | R-WP-5 | 否 | ☐ |
| **WP-R.6** | 热重载 × 反应槽不变量 | R-WP-6 | 否 | ☐ |
| **WP-R.7** | 宏降级漏绑 | R-WP-7 | 否 | ☐ |
| **WP-R.3** | Store 路径触发器（默认文档冻结 select+produce） | R-WP-3 | 否 · P2 | ☐ |
| **WP-R.8** | 信号图可观测（非 DevTools UI） | R-WP-8 | 否 · P2 | ☐ |

---

## 6. 升 Beta · Definition of Done

全部为真才可将对外叙事从 **Alpha / 工程级 MVP** 改为 **Beta / 工程可用**：

- [ ] `cargo publish` 三 crate 成功；干净环境最小窗口可跑（WP-4.3）  
- [ ] 文档站：Getting Started（含双写法）+ 更新契约 + 组件索引（WP-4.4）  
- [ ] 三平台真窗口冒烟 #1–#8（WP-1.4；**Win 已签**，macOS/Linux ☐）  
- [x] AccessKit 主路径可演示并有文档（WP-1.3）  
- [x] damage 策略可测（WP-2.2；GPU PartialLoad = B8）  
- [x] 测宽对齐测 + ≥5 组件 golden CI（WP-2.1 ☑，**WP-2.6 ☑ 2026-08-13**；WP-4.5 CI 矩阵仍可加深）  
- [x] WP-2.4：Owner 注册表无泄漏测（2026-08-13；`registry_owner`）  
- [ ] 开放 P0 = 0（冒烟列、crates、文档站、CHANGELOG）  
- [ ] README：SemVer 承诺面 / 实验面 + 状态 **Beta**（WP-4.1 / 4.7）  

组件库专项不另设第二套升 Beta 条件；稳定面表随 WP-4.1 冻结。未过 CV 的组件**不得**写入承诺面。

---

## 7. Backlog（Beta 后 / 明确延后）

| ID | 项 | 来源 |
|----|-----|------|
| B1 | DevTools（Signal 依赖图） | 工程 |
| B2 | `mergeProps` / `splitProps` / `children()` | 反应性 |
| B3 | backdrop-blur 离屏缓存 | 工程 |
| B4 | 移动 / 鸿蒙 `PlatformWindow` 实装 | 工程 |
| B5 | VirtualMasonry 产品化 | 工程 WP-3.10 / 组件 CB2 |
| B6 | 完整 3D/矩阵变换 | 工程 |
| B7 | 真窗口 CI | 工程 |
| B8 | GPU PartialLoad / retain 默认开 | 工程；见 `damage.md` |
| B9 | 完整 headless 公开模块 `rui::headless::*` | 组件 CB1 |
| B10 | 块级 markdown 编辑控件 | 组件 CB3 |
| B11 | 组件视觉回归（多 DPI / dark） | 组件 CB4 |
| B12 | 非默认 brand 主题包 | 组件 CB5 |
| B13 | 实验面批量升 verified | 组件 CB6；须走 CV 流程 |
| B14 | ShadCN planned P1（Badge/Avatar/Skeleton/…） | inventory §1.9；落地先 `experimental` |
| B15 | 盒模型加深：分边 border、真视口 `vw`/`vh`、局部 `em` | [`box_model.md`](box_model.md) BM4–BM6；WP-2.9.5–7 可降入 |

---

## 8. 执行节奏（建议）

| 周 | 焦点 |
|----|------|
| 0 | D0（本文）；并行起草 WP-4.1 CHANGELOG 骨架 |
| 1–2 | **CV**（走查 + 最小测网 + 身份审计）∥ 启动 WP-2.4；macOS/Linux 冒烟可并行 |
| 3–4 | WP-2.4 / 2.5 / 2.6 / 2.7 / **2.9**（盒模型）；CV 收尾 |
| 5–6 | CV 通过后：WP-3.4 / 3.7 / 3.8；同时 3.2 / 3.3 / 3.5 |
| 7 | WP-4.2–4.6；冻结承诺面 |
| 8 | publish + 升 Beta |

原则：**一 WP 一 PR**（或与别名旧号同 PR，描述只点本文 ID）。  
**未过 CV 不开 3.4/3.7/3.8。** 修不好的稳定候选**降级**，不阻塞 Beta。

测试基线：

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test -p rui --lib
cargo test -p rui --lib offscreen
cargo check -p gallery
```

---

## 9. 旧编号对照（只读）

| 旧 ID | 并入 |
|-------|------|
| C-WP-0.1～0.3 | D0 + WP-CV.0 |
| C-WP-V.1～V.5 | WP-CV.1～.5（V.5 = CV.4） |
| C-WP-1.1 | WP-3.4 |
| C-WP-1.2 | WP-3.7 |
| C-WP-1.3 | WP-1.2 |
| C-WP-1.4 | WP-1.3（桥）+ CV / 2.8（组件身份） |
| C-WP-2.1～2.5 | WP-3.8 |
| C-WP-2.6 | WP-2.8 |
| C-WP-3.1 | WP-3.1 |
| C-WP-3.2 | WP-3.3 |
| C-WP-3.3 / V.4 | WP-2.6 |
| C-WP-3.4 | WP-3.9 |
| C-WP-4.1 / 4.2 / 4.3 | WP-4.1 / 4.4 / 4.2–4.3 |
| R-WP-1 | WP-2.4 |
| R-WP-2 | WP-2.7 |
| R-WP-3 | WP-R.3 |
| R-WP-4 | WP-2.5 |
| R-WP-5～8 | WP-R.5～.8 |

---

## 10. 工作约定

1. **新工作包**必须映射到纲领六门槛之一或规范缺口表，否则进 §7 Backlog。  
2. **改勾选只改本文**；改「何为正确」改对应规范。  
3. 稳定面变更必须有测或 smoke 条目；新 props 默认双入口（标签 + Builder）。  
4. 新浮层不接受长期第二套 placement。  
5. 身份：新 API 默认不把用户 `id` 设为必填；空串不得写入 `focus_id`/`template_id`。  
6. 对照代码只读 `Reference/`，禁止无 ADR 把 Leptos/Floem/EUI 内核粘进 `crates/rui`。

---

*本文随 WP 关闭而改勾选。架构否决项变更须显式 ADR，不得默默回潮。*
