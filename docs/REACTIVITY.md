# RUI Solid 细粒度响应式（权威规范）

> **文档地位**：细粒度响应式编程模型与实现口径的**领域规范**。衡量「组件挂载一次 + Signal 局部更新」的**机制覆盖度**与工程卫生，**不是** solid-main / Leptos / Floem 整仓或 API 表面对齐度。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md) 六条门槛与禁区（ADR-1）；**执行勾选**以 [`AUTHORITY.md`](AUTHORITY.md) **WP-2.4 / 2.5 / 2.7 / WP-R.*** 为准。冻结细则 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md)。  
> **事实来源**：`crates/rui/src/core/{reactivity,store,resource}.rs`、`crates/rui/src/app/{reactive,resource,error_boundary,state}.rs`、`rui_macros`；对照 [`Reference/SolidJS`](../Reference/SolidJS)、[`Reference/leptos-main`](../Reference/leptos-main)（`reactive_graph` / `reactive_stores` / `tachys`）、[`Reference/floem-main`](../Reference/floem-main)（`floem_reactive`）、[`Reference/dioxus`](../Reference/dioxus)（`dioxus-signals` + 组件 VNode）。  
> **日期**：2026-08-13（四框架机制对照 + 完整细粒度不变量；**战略：超过 Floem、逼近 Solid 机制**）  

交叉：[`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md)（开发者手册）· [`element_identity.md`](element_identity.md) · [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) · [`REFERENCE_COMPARISON.md`](REFERENCE_COMPARISON.md) · [`gap_analysis.md`](gap_analysis.md) §5（历史镜像）

---

## 0. 一句话结论

**细粒度机制目标：超过 Floem，逼近 SolidJS（权威 Web 细粒度标尺）。** ADR-1 底座不变；ADR-8 仍禁止用「Solid API 百分百 / DOM·SSR」替代工程发布门槛——逼近的是 **§5.7 + §5.9 机制不变量**，不是 JSX / `mergeProps` / DevTools。

| 问题 | 答案 |
|------|------|
| 要不要在工程化前「把 Solid 追到 Leptos API 面」？ | **否**。扩表面不解决 IME / a11y / 冒烟 / 发布（ADR-8）。 |
| 细粒度要追到哪？ | **超过 Floem 纯度**（列表项级 Owner 是决胜点）→ **逼近 Solid 机制**（For + Index + Store 路径 + effect 分相）。记分牌 §5.9。 |
| 「完整细粒度」指什么？ | run-once + 动态订阅 + Owner dispose + `bind_*` + 结构岛 + **项级 For** + **真 Index** + **字段级 Store** + **渲染绑 / 用户 effect 分相**。见 §5.7。 |
| 响应式还要不要做？ | **要，且升为主线加深**：G6（For/Index）、G4（Store 路径）、G10（effect 分相）、G8（宏漏绑）。卫生门禁 WP-2.4 / 2.5 / 2.7 已关。 |
| 对标谁？ | **标尺** → SolidJS；**原生近邻（要超过）** → Floem；**图协议可借鉴** → Leptos `reactive_graph`；**反例** → Dioxus。 |
| 不把谁当细粒度标杆？ | Dioxus 主路径（组件级 VNode diff）、Iced TEA、egui 即时模式、Slint 属性链、Xilem view-diff。 |

### 0.1 战略：超过 Floem ≠ 抄 Solid Web

```text
SolidJS（机制标尺）     ← 逼近：For mapArray · Index Accessor · Store 路径 · Computed/Render/User 分相
        ▲
RUI 目标带              ← 超过 Floem 纯度；桌面 retained + bind_* + taffy
        ▲
Floem（原生近邻）       ← 已齐：UpdaterEffect · Scope · view 一次构建；弱：列表偏 dyn diff、无 Solid Index/Store
```

| 宣称 | 前置（全部为真才许对外这么说） | 仍禁止说 |
|------|--------------------------------|----------|
| **细粒度纯度超过 Floem** | I-1…I-6 已齐 + **I-7（WP-R.5b）** 测绿：未变 key 不重跑项 `render` | 「全面超过 Floem 框架」；「已对齐 Solid」 |
| **逼近 SolidJS 细粒度** | 上表 + **I-11 真 Index** + **I-8 Store 路径** + **I-12 effect 分相** + G8 主路径无漏绑 | 「Solid 兼容」；SSR/hydration；DevTools 完备 |

Win-first 0.1 / 升 Beta **仍不**以本战略为硬门禁（原生+发布优先）；但反应性加深 **WP-R.5 → R.3 → R.9** 按此序排，不再把 I-7/I-8 当可无限延后的 P2。

---

## 1. 范围与禁区

### 1.1 在范围内

- Signal / Memo / Effect / Owner / Context / batch / untrack  
- 三档 UI 更新：`bind_*` · 结构岛 · `ReactiveSync`  
- Store（select + produce → **路径触发器**，逼近 Solid）/ Resource / Suspense / ErrorBoundary（桌面语义）  
- Show / For（项级 Owner）/ **Index（项 Accessor）** / Switch / Dynamic / Portal  
- Effect 分相：同步派生（Computed）/ 渲染绑（`bind_*`）/ 用户副作用  
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
| **SolidJS** | 编程心智 + 完整细粒度标尺 | 组件跑一次；动态订阅；`createRoot`/`onCleanup`；`<Show>`/`<For>`/`<Index>`；`mapArray` **项级 createRoot**；Store proxy 思想 | DOM 调度、SSR/hydration、`requestCallback`、浏览器 DevTools |
| **Leptos `reactive_graph`** | 最完善的可移植 Signal 图 | Owner 树；Clean/Check/Dirty（Reactively）；RenderEffect（同步渲染绑）vs Effect（async tick）；Arena/`Copy` 句柄；`reactive_stores` 字段触发器 | tachys HTML、SSR feature 分叉、wasm console、`server_fn` |
| **Floem `floem_reactive`** | 原生 GUI 最近邻 | `UpdaterEffect`（compute 订阅 / `on_change` 应用拆开）；Scope dispose↔动态子树；UI 线程 Unsync + 跨线程 waker；`dyn_stack`/`dyn_view` 纪律 | 整套 API 改名迁到 Floem；把 Resource 做成 Web 式 |
| **Dioxus `dioxus-signals`** | DX 对照 + **纯度反例** | RSX / 热重载产品化；Copy Signal；Memo 跳过组件重跑 | **细粒度渲染**（订阅单元仍是组件 `ReactiveContext` → 重跑 fn → VNode diff） |

本地路径：`Reference/SolidJS` · `Reference/leptos-main/reactive_graph` · `Reference/floem-main/reactive` · `Reference/dioxus/packages/signals`。

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

**公理（对齐 Solid + Floem；完整集见 §5.7）**

1. 组件函数 / `app_builder` **意图上挂载一次**；属性走 `bind_*`，结构走 Show/For/reactive。  
2. 根 `app_builder` 内裸 `signal.get()`（未走 bind/Show/For）会订**整树**根 effect——**卫生红线**（WP-2.7：`ui_root_effect_dep_count` 可测）。  
3. 动态子树必须能随 Owner dispose 清 effect 与注册表（Floem：child `Scope.dispose`）。  
4. 失效应区分 Layout ≠ Paint（Floem `request_*`；RUI `LayoutStatus` + damage）。  
5. 列表未变 key **不应**重跑项 `render`（Solid `mapArray`）；当前仍为整表 effect + keyed Element reconcile（I-7 / G6）。

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
| Effect | **done** | `create_effect`；重跑前 dispose 子 Owner | **未拆类型**：渲染绑走 `bind_*`（UpdaterEffect 形），用户副作用同调度；不强制拆 RenderEffect |
| Owner / Root / on_cleanup / on_mount | **done** | `create_root` · `OwnerHandle` | Floem 名 `Scope`；职责同构 |
| Context | **done** | `provide_context` / `use_context`；复合控件实例根用 `with_child_owner`（`#[component]` 不自动开 scope） | 对齐 Solid 组件执行域；省略则同父级多次 provide 会覆盖 |
| batch / untrack / isolate_reads | **done** | 同名 API | Floem 有 batch；Leptos 普通 Effect 靠 tick 合并 |
| Store select + produce | **done (MVP)** | `core/store.rs`；**无** proxy；`produce` 切片契约测 ☑ WP-2.5 | Leptos `reactive_stores`：改兄字段不重跑弟 getter（R-WP-3） |
| Resource + Suspense | **done (MVP)** | `app/resource.rs` | **勿**抄 SSR 水合 Resource |
| ErrorBoundary | **done (MVP)** | `app/error_boundary.rs`；Err→EB **显式** `throw_resource_error` ☑ | 自动桥 deferred（R-WP-4） |
| Show / For / Switch / Dynamic / Portal | **done / For partial** | `show_reactive` / `for_reactive`+`for_keyed`；**列表仍是整表 effect + Element reconcile**，非 Solid `mapArray` 项级 `createRoot` | Floem `dyn_stack` 同属「子树 diff」档；完整 FG → R-WP-5b |
| Index | **partial** | `for_each` = 下标 `template_id`，**不是** Solid `<Index>`（项为 Accessor、下标固定） | WP-R.5c（逼近 Solid） |
| bind_* | **done（亮点）** | 显式细粒度；`ui!` 自动降级 | 对标 Leptos 属性 RenderEffect / Floem UpdaterEffect |
| reactive! 结构岛 | **done** | `reactive_view` / anchor | 对标 Floem `dyn_*` |
| REGISTERED_* → Owner | **done** | Anchors + Bindings + COMPONENT_* 均 `on_cleanup`；`registry_owner` 测 | WP-2.4 ☑ |
| mergeProps / children() / DevTools | **missing（可空缺）** | — | 不阻塞工程级 |

### 4.1 Resource / ErrorBoundary 策略（WP-2.5）

| 场景 | 行为 |
|------|------|
| 首载 | `Loading { previous: None }`；`get()` 可标 Suspense pending |
| Refetch | generation 递增；过期结果丢弃；`previous` 可 stale-while-revalidate |
| Err | `ResourceState::Err`；**不**自动进 ErrorBoundary |
| Err→EB | 子树内显式 [`throw_resource_error`](../crates/rui/src/app/error_boundary.rs)（巩固期契约） |
| `produce` | ≡ `Store::update`；`store_select` 仅在切片 `PartialEq` 变化时通知 |

---

## 5. 四框架机制对照（Solid / Leptos / Floem / Dioxus × RUI）

> 对照源（只读）：`Reference/SolidJS/packages/solid/src/reactive/{signal,array}.ts` · `Reference/leptos-main/reactive_graph` · `Reference/floem-main/reactive` · `Reference/dioxus/packages/signals`。  
> **口径**：比的是**更新单元与图协议**，不是 crate 名或 JSX/RSX 糖。

### 5.1 定位

| | SolidJS | Leptos | Floem | Dioxus | RUI |
|--|---------|--------|-------|--------|-----|
| 主场 | Web DOM | Web / SSR | 原生 retained GUI | Web + 桌面壳（VDOM） | 原生 retained + ShadCN |
| 响应式核 | `solid-js` signal.ts | `reactive_graph`（可移植） | `floem_reactive`（灵感 leptos，非 fork） | `dioxus-signals` + `generational-box` | `core/reactivity`（自研，同哲学） |
| **UI 更新单元** | 叶子计算 / 属性 effect / 项级 root | RenderEffect → DOM 节点 | UpdaterEffect → `update_state` | **组件 fn 重跑 → VNode diff** | `bind_*` 就地改 Element；结构岛 reconcile |
| 细粒度纯度 | **标尺（最高）** | Rust 生态最高（Web） | 高（原生够用） | **低**（有 Signal，渲染仍粗） | 高（属性/结构岛）；**列表未到项级** |
| 调度 | microtask / `requestCallback`；user effect 晚于 render | Effect = async tick；RenderEffect = 同步首跑 | UI 线程 + `batch` pending；priority | 调度器标脏组件 | **同步** `flush_pending`（memo 深度 → effect 深度）+ 帧 `request_redraw` |

### 5.2 图协议

| 机制 | Solid | Leptos | Floem | Dioxus | RUI |
|------|-------|--------|-------|--------|-----|
| 源 / 派生 / 副作用 | Signal / Memo / Effect·Computed·RenderEffect | Signal / Memo / Effect·RenderEffect | Signal / Memo / Effect·UpdaterEffect | Signal / Memo；订阅者是 `ReactiveContext`（组件） | Signal / Memo / `create_effect` |
| 脏状态 | `STALE=1` / `PENDING=2` | **Clean / Check / Dirty**（Reactively：Check 时 pull 源 `update_if_necessary`） | 无三态；effect 入 pending 队列 | 组件 dirty 队列 | Memo 二元 `dirty`；effect 入 pending；**无 Check** |
| 菱形 / glitch | Updates 队列 + pure 先于 user | Check 递归确认后再跑 effect | batch 合并 | 组件重跑顺序 | `flush_pending`：**先按 depth 刷新 memo**（值 `PartialEq` 未变则不通知），再跑 effect。对 PartialEq memo **等价无 glitch**；不必为对齐而重写成 Check |
| 动态依赖 | 每轮清 `sources`/`observers` | `clear_sources` + Observer TLS | `observer_clean_up` | 组件重跑时重订阅读 | effect/memo 重跑清 `effect_deps` / `memo_deps` |
| Owner | `Owner` + `cleanNode` | `Owner` + arena | `Scope` + `Id::dispose` | `ScopeId` / `Owner` drop CopyValue | `OwnerHandle`；子 effect 重跑前 `dispose_owner_children` |
| Read/Write 拆分 | getter/setter 对 | `ReadSignal`/`WriteSignal` 可选 | `ReadSignal`/`WriteSignal` 一等 | `Readable`/`Writable` trait | 统一 `Signal`（可接受） |
| Trigger（无值通知） | — | `Trigger` / `ArcTrigger` | `Trigger` | — | **无**（可用 `Signal<()>`；不单开缺口） |
| 自定义 equals | `options.equals` | memo `(T, bool)` / `PartialEq` | `PartialEq` | Memo `PartialEq` | `PartialEq` only |

**RUI 图协议结论**：不迁 `reactive_graph`、不引入 Check 三态（巩固期禁换核）。现有 depth flush + PartialEq 已覆盖菱形；缺的不是图算法，而是**视图侧更新单元**（§5.4–§5.5）。

### 5.3 调度与线程

| | Solid | Leptos | Floem | Dioxus | RUI |
|--|-------|--------|-------|--------|-----|
| set 后何时跑 effect | 当前同步栈结束后的 Updates/Effects 队列 | 普通 Effect **下一 async tick**；RenderEffect **立即** | pending；出 `batch` 或立即 `run_pending_effects` | 标脏组件，下一调度 tick 重跑 | **立即** `flush_pending`（`batch` 内只入队） |
| 嵌套 create_effect | 进队列 | tick / 同步视类型 | 注册 + 立刻或 pending | hook 规则 | 若已在 flush 中 → `deferred_effects`（防 Win 栈溢出） |
| 跨线程 | 无（DOM 线程） | `Send` 图 + spawner | Unsync UI + sync waker | `SyncSignal` 可选 | Signal 非 Send；`run_on_ui_thread` / `spawn_blocking` |
| 与帧的关系 | rAF / 浏览器 | DOM commit | `request_style/layout/paint` 分层 | VirtualDom 工作循环 | `LayoutStatus` → taffy / damage / wgpu |

桌面正确做法是 **挂 UI 线程 + 帧**，不要抄 wasm microtask。RUI 同步 flush 对 `bind_*` 更贴近 Floem Updater / Leptos RenderEffect，这是优点。

### 5.4 视图绑定（细粒度纯度分界）

「有没有 Signal」**不够**。分界是：**一次 `set` 之后，谁被重跑、什么被 diff。**

```text
Solid / Leptos / Floem / RUI(bind_*)     Dioxus 主路径
─────────────────────────────            ─────────────────────────
组件/view 函数意图上跑一次                订阅该 Signal 的组件 fn 再跑一遍
叶子 text/attr 各挂自己的渲染 effect      产出新 VNode 树
只碰订阅了该 Signal 的叶子                runtime diff 新旧 VNode（即使 Memo 跳过了部分组件）
```

| 绑定形态 | Solid | Leptos | Floem | Dioxus | RUI |
|----------|-------|--------|-------|--------|-----|
| 文本 / 单属性 | JSX 文本与 attr 编译为 render effect | `view! { {value} }` → 节点上 RenderEffect | `Label::derived` / style updater（`UpdaterEffect`：compute 只订依赖，`on_change` 写视图） | `rsx! { "{signal}" }` → **该组件重跑** | **`bind_text` / `bind_prop` / `bind_*`**：`untrack` 初值 + `create_effect` 里 `try_update_element_by_template_id` |
| 结构条件 | `<Show>`：condition memo + 子 root | `<Show>` / tachys 动态视图 | `dyn_view` / 条件 `View` | `if` 在组件内 → VNode 有无 | `show_reactive` → `reactive_view`（独立 effect，不订根） |
| 高频 chrome | — | — | paint 请求分层 | — | `ReactiveSync` + `peek`（帧前写回） |
| 根误订整树 | 组件本就不重跑 | 少见（view 编译进叶子） | view 构建一次 | 父组件误读 Signal → 整段 rsx 重跑 | 根 `app_builder` 裸 `get()` → 根 effect（WP-2.7 可观测） |

**Dioxus 精确表述**（避免稻草人）：`dioxus-signals` 是 Copy + 局部订阅，Memo 可跳过**组件**重跑；子组件不读 Signal 则不订。这仍是 **「细粒度状态 + 粗粒度视图」**：更新单元是组件，不是属性节点。RUI **禁止**把「组件级 Signal 订阅」当成细粒度完成标准。

**RUI 绑定层结论**：`bind_*` 已对齐 Floem UpdaterEffect / Leptos 属性 RenderEffect，是桌面细粒度的正确主路径。不必再拆一个 `RenderEffect` 类型——分层用 API 角色表达即可（`bind_*` / `reactive_view` / 用户 `create_effect`）。

### 5.5 列表（当前最大机制缺口）

| | Solid `<For>` | Solid `<Index>` | Leptos keyed For | Floem `dyn_stack` | Dioxus | RUI 现状 |
|--|---------------|-----------------|------------------|-------------------|--------|----------|
| 列表变化时谁重跑 | **仅插入项**的 `mapFn`；移动项复用已有 mapped + disposer | 下标固定；**项值**是 Accessor，同下标更新不重建节点 | tachys keyed：按 key 复用视图块 | 子 View diff / 复用 | keyed 组件：仍可能重跑子组件 fn | **`for_reactive` 整表 `render_list()` 再跑一遍**，然后 `reconcile_keyed_children` |
| 项身份 | `mapArray`：每项 `createRoot(mapper)` + disposer；key = 项引用/值 | 下标；`indexArray` | key 函数 | View 身份 | `key` prop | `for_keyed` 盖 `{scope}:{key}` → `template_id` |
| 移动 / 删除 | 复用 mapped，dispose 删除项的 root | 不移动身份（下标即身份） | 复用块 | diff | VNode 移动 | `reconcile_keyed_children`：按 `template_id` HashMap 配对，命中则 `reconcile_elements`（复用 `layout_node` / `on_unmount`） |
| 与完整 FG 的距离 | **标尺** | 标尺（固定下标场景） | 高 | 中（子树 diff，非项级 signal） | 低 | **MVP**：Element 级复用有；**项级 Owner / 不重跑未变项 render 没有** |

RUI `for_each` ≠ Solid `<Index>`：只是用下标当 `template_id`，项仍是一次性 `&T` 快照，不是 `Accessor<T>`。

**看齐方向（R-WP-5，不换 VDOM）**：

1. **5a（硬化，先做）**：插入 / 删除 / 换位库测；文档三分 `for_reactive`+`for_keyed`（Keyed For 近似）/ `for_each`（下标键，非 Index）/ 真 Index（若做）。  
2. **5b（完整细粒度）**：列表 signal 只驱动 **insert / remove / reorder**；每 key 一个 child `Owner`（Solid `mapArray` 的 `createRoot`）；未动 key **不**重跑 `render` 闭包。思想可对 Floem `dyn_stack` 的 diff 纪律，实现自研，禁止抄 DOM。

### 5.6 Store

| | Solid `createStore` | Leptos `reactive_stores` | RUI `Store` |
|--|---------------------|--------------------------|-------------|
| 读路径 | Proxy：`store.user.name` 订该路径 | `#[derive(Store)]` 字段 getter，兄变不通知弟 | `store_select(store, \|s| s.name.clone())` → Memo |
| 写 | `setStore` / `produce` 路径更新 | 字段 `set` / 父通知子 | `produce` ≡ `update` 整根 Signal；select 切片 `PartialEq` 才通知下游 |
| 代价 | 路径触发，不扫兄弟 | 不 diff 兄弟字段 | **每次根 `produce` 都重跑所有 select 闭包**（未变切片不通知 effect，但仍付 compute） |

逼近 Solid：**WP-R.3 选 B**（路径触发器，兄变不重跑弟 compute）。巩固期仍允许 `store_select`+`produce` 作过渡；**不得**再把 B 当无限期 P2。实现自研（思想对齐 `reactive_stores` / Solid proxy），禁止抄 JS Proxy 进 Element。

### 5.7 完整细粒度 = 机制不变量（RUI 目标集）

不追求 Solid/Leptos **API 名**百分百。下列不变量同时成立，即 RUI 意义上的**完整细粒度**（**超过 Floem / 逼近 Solid** 的判定面）：

| # | 不变量 | 现状 | 映射 | 档 |
|---|--------|------|------|-----|
| I-1 | 组件 / `app_builder` **意图上挂载一次**；属性不靠根 `get()` | 卫生可测（WP-2.7）；宏漏绑仍在 | G8 / R-WP-7 | 超 Floem 卫生 |
| I-2 | `set` 只唤醒**当时仍订阅**该源的 memo/effect（动态剪边） | **done** | — | 已齐 |
| I-3 | 动态子树 dispose → effect + `REGISTERED_*` + 子 Owner 清零 | **done**（WP-2.4） | G1/G2 closed | 已齐 |
| I-4 | 菱形依赖无中间值泄漏（memo 先于 effect，值未变不传播） | **done**（depth flush + PartialEq） | — | 已齐 |
| I-5 | 纯文本/单属性走 `bind_*`，不重建子树；Layout ≠ Paint | **done**（亮点） | 三档 §3 | 已齐（≈ Floem Updater） |
| I-6 | 结构变化走 Show/For/`reactive!` 岛，不订根 effect | **done**（岛存在；误用可观测） | G3 closed | 已齐 |
| I-7 | **For**：未变 key 不重跑项 `render`，只 insert/remove/reorder 项 Owner | **未达**（整表 render + Element reconcile） | **G6 / WP-R.5b** | **超 Floem 决胜** |
| I-8 | Store：兄字段更新不强制重跑弟 select/compute | MVP select+produce | **G4 / WP-R.3 选 B** | **逼近 Solid** |
| I-9 | 热重载不破坏反应槽 `template_id` | 手册有；回归未齐 | G7 / R-WP-6 | DX |
| I-10 | 图可诊断（计数 / 泄漏），非完整 DevTools | 根 dep 计数有；全图 dump 无 | G9 / R-WP-8 P2 | 可后置 |
| I-11 | **Index**：下标固定，项为 Accessor；同下标改值不重建节点、只通知订了该项的叶子 | `for_each` ≠ Index | **G6 / WP-R.5c** | **逼近 Solid** |
| I-12 | **Effect 分相**：Memo/Computed 先 → `bind_*` 渲染绑 → 用户 `create_effect`（Solid：Computed / RenderEffect / Effect） | 同一 `flush_pending` 同步跑完 | **G10 / WP-R.9** | **逼近 Solid** |

**排序**：**WP-R.5b（I-7）→ 5c（I-11）→ R.3（I-8）→ R.9（I-12）→ R.7（G8）**。禁止用拆 `RenderEffect` 类型名或 JS Proxy 冒充完整度；禁止用 I-10 DevTools UI 冒充细粒度。

### 5.9 超过 Floem / 逼近 Solid 记分牌

> 只比**机制**。Floem 产品成熟度（inspector、动画、跨线程）不在此表。

| 机制 | Solid | Floem | RUI 现状 | 目标 |
|------|-------|-------|----------|------|
| View/组件 run-once | 是 | 是 | 意图 + isolate_reads 试点 | 主路径无漏绑（G8） |
| 叶子属性绑 | render effect | UpdaterEffect | `bind_*` | 保持；分相后仍先于用户 effect |
| Show / 结构岛 | `<Show>` memo + 子 root | `dyn_view` | `show_reactive` | 保持 |
| **Keyed For 项级 root** | `mapArray` + `createRoot` | `dyn_stack` **diff**（弱于 Solid） | 整表 render + reconcile | **I-7：达到 Solid，从而超过 Floem** |
| **Index 项 Accessor** | `indexArray` | 无对位 | `for_each` 下标键 | **I-11：达到 Solid** |
| Store 路径 | Proxy | DerivedRw 有限 | select+produce | **I-8：兄变不跑弟** |
| Effect 分相 | Computed / Render / User | Updater / Effect | 单队列同步 | **I-12：三相位，不必改 API 名** |
| Owner dispose | 是 | Scope | 是（WP-2.4） | 保持 |
| Check 三态图 | STALE/PENDING | 无 | depth flush | **不换核**（I-4 已够） |
| SSR / DevTools / mergeProps | 有 | 部分 / inspector | 无 | **不追** |

### 5.8 应学 / 切勿照抄

**应学（可移植）**

1. 图分层：源 / Memo / Effect；派生优先于 effect 写回。  
2. Owner（Scope）树 + cleanup ↔ 动态 UI 段。  
3. 动态依赖：重跑清订阅，死分支自动剪边。  
4. 渲染绑 vs 用户副作用：Leptos RenderEffect、Floem UpdaterEffect；RUI 用 `bind_*` + `ReactiveSync` + 用户 `create_effect` 表达，**不必改名拆类型**。  
5. 失效分层：paint ≠ layout ≠ style（Floem）；RUI `LayoutStatus` / damage。  
6. 列表项级 root：Solid `mapArray` 的 `createRoot` + disposer（R-WP-5b）。  
7. Store 路径：逼近 Solid 必做路径触发（WP-R.3B）；过渡期 `store_select` 可留。  
8. 组件 run-once：`#[component(reactive)]` + `isolate_reads` 扩面（R-WP-7）。

**切勿照抄**

1. Leptos SSR / hydrate / islands / `Resource` 编解码。  
2. `view!` HTML 与 DOM `prop:`。  
3. 为对齐把 RUI API 改名为 Floem / `leptos` / Solid 全家桶。  
4. 浏览器 microtask / `requestCallback`——调度挂 RUI 帧与 UI 线程桥。  
5. Dioxus「有 Signal = 细粒度渲染」；禁止把组件级 VNode diff 接回 ADR-1。  
6. 把 `core/reactivity` 换成依赖 `reactive_graph` / `floem_reactive`（对照可以，换核须独立 ADR）。  
7. 用 Check 三态重写内核来「证明」细粒度——I-4 已由 depth flush 覆盖。

---

## 6. 已知缺口（权威开放面）

> 执行勾选见 [`AUTHORITY.md`](AUTHORITY.md)。此处只列**事实**。冻结细则 [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md)。

| ID | 缺口 | 优先级 | 映射 |
|----|------|--------|------|
| G1 | `REGISTERED_*` / `COMPONENT_*` 随 Owner dispose | **closed（WP-2.4 ☑ 2026-08-13）** | `registry_owner` 测；Gallery 布局→WP-2.4 |
| G2 | scoped `template_id` / `focus_id`（owner path） | **closed（WP-2.4）** | `scoped_template_id` / `scoped_focus_id` |
| G3 | 双路径并存：bind 就地改 vs 根 rebuild 全树 diff | **closed（可观测，WP-2.7 ☑ 2026-08-13）** | `mark_next_effect_as_ui_root` + `ui_root_effect_dep_count`；双路径仍为设计 |
| G4 | Store 无路径触发器：根 `produce` 仍重跑所有 select 闭包 | **P1**（逼近 Solid） | WP-R.3 **选 B**；WP-2.5 ☑ produce 过渡契约 |
| G5 | Resource Err → ErrorBoundary 端到端契约 | **closed（WP-2.5 ☑ 2026-08-13）** | 显式 `throw_resource_error`；自动桥仍 deferred |
| G6 | 列表：整表 render + keyed Element reconcile；`for_each` ≠ Index | **P0 机制**（超 Floem + 逼近 Solid） | WP-R.5：**5a** 测；**5b** 项级 For Owner；**5c** 真 Index |
| G7 | 热重载 × 反应槽不变量文档与回归 | P1 DX | R-WP-6 |
| G8 | 宏自动降级漏绑面（Switch 糖、gallery 扩面） | **P1**（否则超 Floem 只写在纸上） | R-WP-7 |
| G9 | 信号图可观测 / 最小泄漏诊断（非完整 DevTools） | P2 | R-WP-8 |
| G10 | Effect 未分相：用户 `create_effect` 与 `bind_*` 同一同步 flush | **P1**（逼近 Solid） | WP-R.9：Computed → bind → 用户 effect |

**不做（巩固期）**：重写响应式内核（Check 三态 / 换 `reactive_graph`）；引入 VDOM；对齐 Leptos 0.8 / Solid **API 表面**；Web Suspense / SSR；DevTools UI；`mergeProps` / `children()`。

---

## 7. 与组件库的边界

| 层 | 响应式职责 | 组件职责 |
|----|------------|----------|
| 受控 / 非受控 | `Signal` 单源真理；禁止双 `use_signal` | Builder/RSX 暴露受控 props |
| 实例键 | `use_signal` / scoped 键规则 | [`element_identity.md`](element_identity.md) + inventory id 策略 |
| 开合 / roving | 可用 Signal 存状态 | 行为下沉 `utils/`（popup / focus_trap）；不把状态机只藏样式层 |
| 结构岛 | Dialog / Menu 内容用 `reactive!` / Show | 产品 API 与 variant；勿在稳定面依赖未稳定的 Store proxy |

组件规范见 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §4；实施见 [`AUTHORITY.md`](AUTHORITY.md) WP-CV.4 / WP-2.8（与 WP-2.4 对齐）。

---

## 8. 文档拓扑

```text
AUTHORITY.md                         ← ★执行源（WP-2.4 / 2.5 / 2.7 / WP-R.*）
ENGINEERING_GRADE_FRAMEWORK.md       ← 总纲领（ADR-1；§4.2 / §5 摘要）
REACTIVITY.md                        ← 本文件（§0.1 超过 Floem / 逼近 Solid；§5.7–§5.9）
REACTIVITY_PLAN.md                   ← 冻结展开（R-WP 验收矩阵）
  ├─ reactivity_and_hot_reload.md    ← 三档用法与热重载手册（从属）
  ├─ element_identity.md             ← 身份四轴（交叉权威）
  ├─ COMPONENT_LIBRARY.md §4         ← 受控 Signal / id 与组件边界
  └─ gap_analysis.md §5              ← 历史 Solid 表（镜像；非执行源）
```

争议：反应性口径以本文为准（超过 Floem / 逼近 Solid = §5.9 记分牌 + §5.7 不变量，不是 API 追平）；勾选以 [`AUTHORITY.md`](AUTHORITY.md) 为准；禁止静默改 ADR-1 或引入 VDOM / 组件级 diff。

---

*规范随 R-WP / WP-2.4·2.5·2.7 关闭与 §5.7 不变量修订；新增公开响应式 API 必须落入 §4 表并更新 SemVer 面。*
