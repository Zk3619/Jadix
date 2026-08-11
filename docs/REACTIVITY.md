# RUI Solid 细粒度响应式（权威规范）

> **文档地位**：细粒度响应式编程模型与实现口径的**单一权威规范**。衡量「组件挂载一次 + Signal 局部更新」的覆盖度与工程卫生，**不是** solid-main / Leptos 整仓对齐度。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) 六条门槛与禁区（ADR-1）；巩固期勾选以 [`ENGINEERING_IMPLEMENTATION_PLAN.md`](ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.4 / WP-2.5** 为准；加深项执行源为 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md)（**R-WP-***，不另开架构分叉）。  
> **事实来源**：`crates/rui/src/core/{reactivity,store,resource}.rs`、`crates/rui/src/app/{reactive,resource,error_boundary}.rs`、`rui_macros`；对照 [`reference/leptos-main`](../reference/leptos-main)（`reactive_graph` / `tachys`）、[`reference/floem-main`](../reference/floem-main)（`floem_reactive`）、[`reference/SolidJS`](../reference/SolidJS)。  
> **日期**：2026-08-12  

交叉：[`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md)（开发者手册）· [`element_identity.md`](element_identity.md) · [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) · [`REFERENCE_COMPARISON.md`](REFERENCE_COMPARISON.md) · [`gap_analysis.md`](gap_analysis.md) §5（历史镜像）

---

## 0. 一句话结论

**Solid 式细粒度是已锁定的正确底座（ADR-1）；主路径 API 已齐，巩固期只做工程卫生与契约硬化，不追 Leptos Web 完备度，不迁 Floem API。**

| 问题 | 答案 |
|------|------|
| 要不要在工程化前「把 Solid 追到 Leptos」？ | **否**。扩 API 面不解决 IME / a11y / 冒烟 / 发布。 |
| 响应式还要不要做？ | **要**，但是 **硬化**：`REGISTERED_*`→Owner、双路径收敛、Store/列表契约——见计划 R-WP / WP-2.4·2.5。 |
| 对标谁？ | **哲学 / 图协议** → Leptos `reactive_graph`；**原生 retained 绑定** → Floem `UpdaterEffect` + `request_*`；**产品心智** → SolidJS。 |
| 不把谁当细粒度标杆？ | Dioxus（Signal + VDOM）、Iced TEA、egui 即时模式、Slint 属性链、Xilem view-diff。 |

---

## 1. 范围与禁区

### 1.1 在范围内

- Signal / Memo / Effect / Owner / Context / batch / untrack  
- 三档 UI 更新：`bind_*` · 结构岛 · `ReactiveSync`  
- Store（select + produce）/ Resource / Suspense / ErrorBoundary（桌面语义）  
- Show / For / Switch / Dynamic / Portal  
- 与 `template_id`、热重载、组件受控 Signal 的契约  

### 1.2 明确不在范围内（勿当缺口）

| 项 | 原因 |
|----|------|
| DOM / SSR / hydration / islands | Web 专属；RUI 无 DOM |
| `Resource` 编解码跨服务端水合 | `leptos_server` 专属 |
| `view!` HTML / `prop:` DOM 语义 | 浏览器模板 |
| `server_fn` / meta / router-as-web | 全栈 Web |
| 完整 Solid `mergeProps` / `children()` / DevTools | 可长期空缺（纲领 §4.2） |
| 换 VDOM 或弃 Owner/Signal | ADR-1 禁区 |

### 1.3 参考基线分工

| 参考 | 角色 | 学什么 | 不搬什么 |
|------|------|--------|----------|
| **SolidJS** | 编程心智 | 组件跑一次、细粒度订阅、Show/For | DOM 调度、浏览器专用 API |
| **Leptos `reactive_graph`** | 最完善的可移植 Signal 图 | Owner 树、Clean/Check/Dirty、RenderEffect vs Effect、Arena/`Copy` 句柄、Store 路径触发器思想 | tachys HTML、SSR feature 分叉、wasm console |
| **Floem `floem_reactive`** | 原生 GUI 最近邻 | `UpdaterEffect`（订阅与应用拆开）、`request_style/layout/paint` 分层、Scope dispose↔动态子树、UI 线程 Unsync + 跨线程 waker | 整套 API 改名迁到 Floem；把 Resource 做成 Web 式 |
| **Dioxus** | DX / 宏对照 | RSX 体验、热重载产品化 | **细粒度纯度**（仍以组件级 VDOM 为主） |

本地路径：`reference/leptos-main/reactive_graph` · `reference/floem-main/reactive` · `reference/SolidJS`。

---

## 2. RUI 架构图（实现 = 规范）

```text
  用户代码
  ui! / #[component] / Signal.get|set
            │
            ▼
  core/reactivity   ← Owner / Signal / Memo / Effect / Context / batch
  core/store        ← Store = 根 Signal + store_select；produce ≡ update
  core/resource     ← ResourceState + Suspense TLS
            │
            ▼
  app/reactive      ← REGISTERED_ANCHORS|BINDINGS · bind_* · show/for/switch · reactive_view
  app/resource      ← create_resource + suspense 壳
  app/error_boundary
            │
     ┌──────┼──────────────┐
     ▼      ▼              ▼
  bind 就地改节点   reconcile 子树    ReactiveSync 帧前 peek
  (template_id)    (结构岛)           (高频 chrome)
            │
            ▼
  LayoutChanged | StyleChanged → taffy / damage / wgpu
```

**公理（对齐 Solid + Floem）**

1. 组件函数 / `app_builder` **意图上挂载一次**；属性走 `bind_*`，结构走 Show/For/reactive。  
2. 根 `app_builder` 内裸 `signal.get()`（未走 bind/Show/For）会订**整树**根 effect——**卫生红线**。  
3. 动态子树必须能随 Owner dispose 清 effect 与注册表（Floem：child `Scope.dispose`）。  
4. 失效应区分 Layout ≠ Paint（Floem `request_*`；RUI `LayoutStatus` + damage）。

---

## 3. 三档更新契约

权威用法示例见 [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md)。纲领级摘要：

| 档位 | API | 允许 | 禁止 |
|------|-----|------|------|
| 属性 | `bind_text` / `bind_prop` / `bind_attr` / `bind_class` / `bind_style` | 就地改文本/样式/chrome | 用结构岛表达纯文本闪烁 |
| 结构 | `show_reactive` / `for_*` / `switch_reactive` / `reactive!` | 挂载/卸载子树 | 根上裸 `get()` 驱动整树 |
| 高频 chrome | `ReactiveSync` / peek 写回 | 每帧同步视觉 | effect 风暴驱动 paint |

热重载：`template_id` 是**槽位**身份（≠ focus / a11y 主键）；结构岛需显式稳定 id。详见 [`element_identity.md`](element_identity.md)。

---

## 4. API 清单（相对 Solid 编程模型）

| API 族 | 状态 | RUI 证据 | 相对 Leptos / Floem |
|--------|------|----------|---------------------|
| Signal / get·set·with·update / peek | **done** | `core/reactivity.rs` | 对齐；无 Read/Write 半边拆分亦可接受 |
| Memo（惰性 + depth flush） | **done** | `create_memo` | 对齐 Leptos Memo / Floem Memo |
| Effect | **done** | `create_effect`；重跑前 dispose 子 Owner | 未拆 RenderEffect / ImmediateEffect（见 §6） |
| Owner / Root / on_cleanup / on_mount | **done** | `create_root` · `OwnerHandle` | Floem 名 `Scope`；职责同构 |
| Context | **done** | `provide_context` / `use_context` | 对齐 |
| batch / untrack / isolate_reads | **done** | 同名 API | Floem 有 batch；Leptos 普通 Effect 靠 tick 合并 |
| Store select + produce | **partial** | `core/store.rs`；**无** proxy 路径触发器 | Leptos `reactive_stores` 更深 |
| Resource + Suspense | **done (MVP)** | `app/resource.rs` | **勿**抄 SSR 水合 Resource |
| ErrorBoundary | **done (MVP)** | `app/error_boundary.rs`；Err→EB 自动桥未齐 | WP-2.5 / R-WP-4 |
| Show / For / Switch / Dynamic / Portal | **done / For partial** | `app/reactive.rs`；keyed 深度弱于 Solid | Floem `dyn_stack` / `dyn_view` 可对照 |
| Index | **partial** | `for_each` ≠ Solid Index 语义 | R-WP-5 |
| bind_* | **done（亮点）** | 显式细粒度；`ui!` 自动降级 | 对标 Leptos 属性 RenderEffect / Floem UpdaterEffect |
| reactive! 结构岛 | **done** | `reactive_view` / anchor | 对标 Floem `dyn_*` |
| REGISTERED_* → Owner | **partial** | Anchor 有部分 `on_cleanup`；Binding 主要靠 unmount | **WP-2.4 / R-WP-1** |
| mergeProps / children() / DevTools | **missing（可空缺）** | — | 不阻塞工程级 |

---

## 5. Leptos × Floem × RUI 对照

### 5.1 定位

| | Leptos | Floem | RUI |
|--|--------|-------|-----|
| 主场 | Web / SSR | 原生 retained GUI | 原生 retained + ShadCN 组件 |
| 响应式 crate | `reactive_graph`（可移植） | `floem_reactive`（灵感自 leptos，非 fork） | `core/reactivity`（自研，同哲学） |
| 视图绑定 | `tachys` + RenderEffect → DOM | `UpdaterEffect` → `update_state` / `request_*` | `bind_*` / reconcile → Element + taffy |
| 细粒度纯度 | **最高（Rust 生态）** | 高（原生场景够用） | 高（模型 5/5）；工程卫生未收口 |

### 5.2 应学（可移植）

1. **图分层**：可写源 / Memo 派生 / Effect 副作用；派生优先于 effect 写回。  
2. **Owner（Scope）树 + cleanup**：动态 UI 段生命周期绑资源。  
3. **动态依赖**：重跑清订阅，分支自动剪边。  
4. **渲染绑定 vs 一般副作用**：Leptos `RenderEffect`；Floem `UpdaterEffect`；RUI 用 `bind_*` + `ReactiveSync` 表达同一分层。  
5. **失效分层**：paint ≠ layout ≠ style（Floem）；RUI 保持 `LayoutStatus` / damage。  
6. **Store 路径思想**：整 struct 过粗；字段级触发（实现可继续用 `store_select`，不必一上来 proxy）。  
7. **组件 run-once**：`#[component(reactive)]` + `isolate_reads` 已试点，需扩面防漏绑（R-WP-7）。

### 5.3 切勿照抄

1. Leptos SSR / hydrate / islands / `Resource` 编解码矩阵。  
2. `view!` HTML 与 DOM `prop:` 陷阱。  
3. 为对齐而把 RUI API 改名为 Floem/`leptos` 同名全家桶。  
4. 全局浏览器 microtask 假设——调度必须挂 RUI 帧 / UI 线程桥。  
5. Dioxus「有 Signal = 细粒度」话术——其主路径仍是组件级 diff。

---

## 6. 已知缺口（权威开放面）

> 执行勾选见 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md) 与总计划 WP-2.4 / 2.5。此处只列**事实**。

| ID | 缺口 | 优先级 | 映射 |
|----|------|--------|------|
| G1 | `REGISTERED_BINDINGS` / `COMPONENT_*` 未全面随 Owner dispose | P1 DX | WP-2.4 · R-WP-1 |
| G2 | scoped `template_id` / `focus_id`（owner path）未完成 | P1 | WP-2.4 · identity |
| G3 | 双路径并存：bind 就地改 vs 根 rebuild 全树 diff | P1 | R-WP-2 |
| G4 | Store 无 proxy 路径触发器；契约测 / SemVer 面未标清 | P1→P2 | WP-2.5 · R-WP-3 |
| G5 | Resource Err → ErrorBoundary 端到端契约 | P1 | WP-2.5 · R-WP-4 |
| G6 | keyed For / Index 语义深度不足 | P2 | R-WP-5 |
| G7 | 热重载 × 反应槽不变量文档与回归 | P1 DX | R-WP-6 |
| G8 | 宏自动降级漏绑面（Switch 糖、gallery 扩面） | P2 | R-WP-7 |
| G9 | 信号图可观测 / 最小泄漏诊断（非完整 DevTools） | P2 | R-WP-8 |

**不做（巩固期）**：重写响应式内核、引入 VDOM、对齐 Leptos 0.8 API 表面、Web Suspense 语义。

---

## 7. 与组件库的边界

| 层 | 响应式职责 | 组件职责 |
|----|------------|----------|
| 受控 / 非受控 | `Signal` 单源真理；禁止双 `use_signal` | Builder/RSX 暴露受控 props |
| 实例键 | `use_signal` / scoped 键规则 | [`element_identity.md`](element_identity.md) + inventory id 策略 |
| 开合 / roving | 可用 Signal 存状态 | 行为下沉 `utils/`（popup / focus_trap）；不把状态机只藏样式层 |
| 结构岛 | Dialog / Menu 内容用 `reactive!` / Show | 产品 API 与 variant；勿在稳定面依赖未稳定的 Store proxy |

组件规范见 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §4；实施见 C-WP-V.5 / 2.6（与 WP-2.4 对齐）。

---

## 8. 文档拓扑

```text
ENGINEERING_GRADE_FRAMEWORK.md       ← 总纲领（ADR-1；§4.2 / §5 摘要）
ENGINEERING_IMPLEMENTATION_PLAN.md   ← WP-2.4 / WP-2.5 勾选（巩固期门禁）
REACTIVITY.md                        ← 本文件（反应性权威现状与口径）
REACTIVITY_PLAN.md                   ← R-WP-* 执行源（加深 / 卫生，不重复 WP 正文）
  ├─ reactivity_and_hot_reload.md    ← 三档用法与热重载手册（从属）
  ├─ element_identity.md             ← 身份四轴（交叉权威）
  ├─ COMPONENT_LIBRARY.md §4         ← 受控 Signal / id 与组件边界
  └─ gap_analysis.md §5              ← 历史 Solid 表（镜像；非执行源）
```

争议：反应性口径以本文为准；勾选以总计划 WP-2.4/2.5 + [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md) 为准；禁止静默改 ADR-1 或引入 VDOM。

---

*规范随 R-WP / WP-2.4·2.5 关闭而修订；新增公开响应式 API 必须落入 §4 表并更新 SemVer 面。*
