# 工程级差距分析（Gap Analysis）

> 审计对象：`crates/rui`、[`crates/rui_macros`](../crates/rui_macros/src/lib.rs)、`crates/rui_style_tokens`、`crates/rui_hot_reload_cli`、`Docs/`
> 对照基准（各司其职，见 §1）：[`Reference/SolidJS`](../Reference/SolidJS) 编程模型 · [`Reference/ShadCN-UI`](../Reference/ShadCN-UI) + [`Reference/TailwindCSS`](../Reference/TailwindCSS) 组件体验 · [`Reference/EUI-NEO-main`](../Reference/EUI-NEO-main) 产品完整度 · [`Reference/pretext-main`](../Reference/pretext-main) 文本算法
> 日期：2026-08-06（P1 打磨 MVP 完成 · 阶段五接线复核后重补）

三个问题：**距离工程级 UI 框架还差什么**、**类 SolidJS 能力还缺什么**、**还需要哪些组件**。

**等级约定**

| 等级 | 含义 |
|------|------|
| P0 | 地基性缺陷，越晚改代价越大，或直接阻塞其他能力 |
| P1 | 影响可用性 / 完整度，但不阻塞架构演进 |
| P2 | 打磨项，可延后 |

**统计（P1 MVP 后）**：开放差距 9 条（P0 **0** / P1 **0** / P2 9）；SolidJS 能力已对齐 **14/28**（另有 3 项部分对齐）；组件 **已有 26 / 占位 3 / 缺失 21**。  
阶段一已关闭：Owner / `onCleanup` / Context / scoped ID / 惰性 memo / `with`+`update` / CI+LICENSE / 脏区文档纠偏。  
阶段二已关闭：utility 单一真源 / 未知类名告警 / 变体引擎 / Theme 切换+dark token / 完整色阶 / `ui!` span / `.get()` 收紧 / `#[component]` 泛型+文档。  
阶段三已关闭：事件冒泡/捕获/pointer capture / 右键·双击·文件拖放 / 脏区 scissor MVP / opacity+translate / AccessKit 层级+Action / 静态文本可选中+ellipsis / 离屏 golden 骨架。  
阶段四已关闭：统一 floating sync / Tooltip·ContextMenu·Combobox / VirtualList·DataTable·TreeView / Form·Textarea / Toast·Resizable·FilePicker(rfd)。**仍开放**：crates.io 发布与文档站。  
阶段五已关闭：[`text/measure`](../crates/rui/src/text/measure.rs) prepare/layout + 布局接入 / `create_store`+`store_select` / `run_on_ui_thread` 线程桥 / template_id 路径索引 + bind_text 同宽→StyleChanged / CI 离屏冒烟+golden。
P1 打磨已关闭：渐变条带 + 多层阴影 + rotate/scale / `fixed`+`aspect-ratio` / 富文本 spans / RTL+letter-spacing / 热路径 soft-fail / 宏未知属性 `#[deprecated]`。

> **接线复核（2026-08-06）**：一次批量脚本事故后的 `git checkout` 回滚，连带抹掉了阶段五中尚未提交的**接线**改动——`store` / `ui_bridge` 的模块声明与 `RuiUserEvent::UiWake`、`template_paths` 索引、golden 助手、CI 定义。文件正文幸存但未参与编译，等于「文档说有、构建里没有」。已全部重补并加测试兜底：`cargo test --workspace` 140 passed、`clippy --workspace --all-targets -D warnings` 干净。

---

## 1. 路线判断：三个参考基线各管一层

结论：**继续走 Solid 式细粒度响应式是对的，但「工业级」不等于「Solid 一比一复刻」。** EUI 与 Pretext 都值得参考，角色不同，谁都不替换当前底座。

| 层 | 选型 | 参考基线 | 该学什么 | 不该搬什么 |
|----|------|----------|----------|------------|
| 编程模型 | 自研 Solid 式 | [`SolidJS`](../Reference/SolidJS) | 组件跑一次、Signal 驱动局部更新；Owner / cleanup / Context；`Show` / `For` / memo 心智 | 浏览器 DOM、SSR / hydration、Web 专用并发调度 |
| 样式与组件体验 | utility + headless | [`ShadCN-UI`](../Reference/ShadCN-UI) / [`TailwindCSS`](../Reference/TailwindCSS) / [`UnoCSS`](../Reference/UnoCSS) | a11y 契约、无头交互拆分、变体命名 | Web 专有 CSS 特性 |
| 产品完整度与渲染纪律 | wgpu 主线 | [`EUI-NEO-main`](../Reference/EUI-NEO-main) | 组件密度标尺（其 `components/` 有 barchart / linechart / piechart / markdown / datepicker / timepicker / carousel / stepper / segmented / virtualmasonry 等 38+ 控件）、batch 与资源纪律、gallery 驱动开发、多后端抽象样本 | 命令式 / DSL 架构本身 |
| 盒模型布局 | `taffy` | — | — | 不动 |
| 文本排版 | `cosmic-text` / `glyphon` | [`pretext-main`](../Reference/pretext-main) | `prepare` 一次（分段 + 测量 + glue 缓存）/ `layout` 纯算术的分层；真实断行；逐行变宽（绕排）；`walkLineRanges` 式稳定行高测量；rich inline flow | 不当布局总方案，不原样移植 JS |

**为什么不换底座**

1. 自绘 UI 没有 DOM。VDOM（Dioxus / React 式）多一层 diff 成本；纯即时模式（egui）难撑复杂组件库与无障碍。Solid 式 retained + 细粒度更新最贴目标。
2. 换成 EUI 式命令式 / DSL，等于丢掉已投入的 Solid 内核与 `ui!` DX。
3. 已实测：要压到 EUI 的体积量级，基本要弃 `wgpu` / naga 并弱化文本栈，与「跨平台像素级自绘」冲突。

**推论**：工业级差距集中在 **原生层**（文本、IME、焦点环、AccessKit、多窗口、GPU batch、动效）与 **工程层**（Store / 线程桥、golden CI、跨平台冒烟、发布），Solid 本身解决不了这些，必须自己补。所以下一阶段不再开「是否放弃 Solid」的架构分叉，见 §7 阶段五。

### 1.1 执行模型：响应式保留模式（已是现状，非待选项）

「API 写起来像即时模式、底层执行是保留模式」——这条路线 RUI 已经在走，不是待决策项。核过代码后的现状：

| 主张 | 现状 | 证据 |
|------|------|------|
| 持久保留树，不每帧重建 | ✅ Element 树与 taffy 节点跨帧存活 | [`build_and_layout`](../crates/rui/src/layout/layout_tree.rs) 注释明确 “DO NOT clear taffy anymore; we rely on reconciliation and selective removal”；`reconcile_elements` 里 `new_el.layout_node = old_el.layout_node` 复用 taffy NodeId |
| 细粒度直达节点，不做全树 diff | ⚠️ **两条路并存** | 细粒度路径有 [`bind_text` / `bind_prop`](../crates/rui/src/app/reactive.rs) + `update_element_by_template_id`；根重建路径仍全树属性 diff |
| 区分 NeedsLayout 与 NeedsPaint 的脏标记 | ⚠️ **有，但是全局单值** | [`LayoutStatus`](../crates/rui/src/core/tree.rs) 四态分流；`bind_text` 同宽时已走 `StyleChanged` |
| 布局局部更新 | ⚠️ **MVP** | 同宽文本改动不再全量 taffy；几何变更仍从根 `compute_layout`。按子树 `mark_dirty` 仍开放（P2） |
| 属性更新 O(1) 直达 | ✅ MVP | `template_paths` 路径索引，查找 O(depth) |

**阶段五后**：文本测宽与跨帧 prepare 缓存已统一；计数器类 `bind_text` 在固有宽不变时不再触发全树布局。保留树 + Signal 路线继续加深，而不是另开架构分叉。

---

## 2. 总体判断

以 Alpha 期自绘框架衡量，完成度偏高：核心约 **3.3 万行** Rust（`crates/` 下 121 个 `.rs`）、`winit + wgpu + taffy + glyphon` 全自绘栈、Solid 式 Owner / Signal / Memo / Context、按 `template_id` 的子树 reconcile，组件面已铺到表格 / 浮层 / 表单，热重载与体积 / 内存均有实测文档。骨架方向已经选对。

细粒度更新这一层做得比预期扎实。[`bind_text` / `bind_prop` / `bind_class` / `bind_style`](../crates/rui/src/app/reactive.rs) 这套显式绑定，加上子树 reconcile，方向完全正确。热重载链路也超出同规模项目的常见水平。

**阶段一（内核硬化）已落地**（见 [`core/reactivity.rs`](../crates/rui/src/core/reactivity.rs)）。  
**阶段二（样式与宏）已落地**：[`rui_style_tokens`](../crates/rui_style_tokens) 为宏与运行时共享目录；[`style/utility.rs`](../crates/rui/src/style/utility.rs) 支持变体与 `opacity` / `translate-*` / `truncate`。  
**阶段三（事件与渲染）已落地**：[`pointer_events`](../crates/rui/src/app/pointer_events.rs) 冒泡/捕获/capture；脏区 [`damage`](../crates/rui/src/app/damage.rs)+scissor；AccessKit 层级与 Action；静态文本选区；[`renderer/offscreen`](../crates/rui/src/renderer/offscreen.rs) golden 骨架。  
**阶段四（组件与生态）已落地 MVP**：[`sync_floating_placement`](../crates/rui/src/utils/popup.rs)；Tooltip / ContextMenu / Combobox；VirtualList / DataTable / TreeView；Form / Textarea；ToastHost / Resizable / FilePicker(`rfd`)。

接下来卡住「发布级」的**不是再换一条哲学路线，而是原生层与工程层的打磨**（§1 已说明为什么不换底座）：

1. **文本深化**——`prepare_text` 直接消费 `PreparedText`；RTL / 富文本。
2. **按子树 taffy 局部重算**——阶段五已避免同宽文本触发全量；几何变更仍从根算。
3. **createResource / Suspense**——线程桥已有，可在其上长异步原语。
4. **crates.io + 文档站**——等 API 再稳一轮。

---

## 3. 五个结构性问题（阶段四后状态）

### 3.1 Owner 树 / 生命周期 — ✅ 已解决（阶段一）

[`create_root`](../crates/rui/src/core/reactivity.rs) / Owner 子树、`on_cleanup`、effect 重跑前 `dispose_owner_children`、owned `create_signal` 随 owner 回收。

**仍开放**：[`app/reactive.rs`](../crates/rui/src/app/reactive.rs) 的 `REGISTERED_ANCHORS` / `REGISTERED_BINDINGS` 仍是 UI 层手工表；可逐步改为挂在 owner 下。

### 3.2 全局字符串 ID — ✅ 部分解决（阶段一）

- [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)：键相对 owner path。
- [`use_signal`](../crates/rui/src/core/reactivity.rs)：**保留全局键**（Input 焦点、滚动等需跨 rebuild 存活）。

**仍开放**：`anchor_id` / `template_id` / `focus_id` 等 UI 层字符串仍扁平。

### 3.3 样式交互态无声明式出口 — ✅ 已解决（阶段二）

变体引擎已落地：`hover:` / `focus:` / `focus-visible:` / `active:` / `disabled:` / `dark:` / `data-[key=value]:`。  
静态 `class=` 与动态 `bind_class` 走同一 [`apply_class_list`](../crates/rui/src/style/utility.rs)；每帧 chrome sync 调用 [`sync_class_variants_subtree`](../crates/rui/src/style/utility.rs)。

**仍开放**：断点前缀（`sm:`）、`group-hover:` / `peer-*`、`transition-*` / `animate-*` 类名入口。

### 3.4 质量门禁 — ✅ 基础已落地（阶段一 + 三骨架）

- CI：`fmt --check` + build + test + `clippy --workspace -D warnings`
- MIT LICENSE + crate 元数据
- 离屏 wgpu clear/readback 烟雾测试（[`offscreen.rs`](../crates/rui/src/renderer/offscreen.rs)）

**仍开放**：完整场景 / 组件级 golden 对比可继续加深。热路径 unwrap 已 MVP 收敛。

### 3.5 文档承诺与实现不符 — ✅ 已纠偏（阶段一）；脏区实现 ✅ MVP（阶段三）

[`union_damage`](../crates/rui/src/app/state.rs) + `LoadOp::Load` + scissor；结构变更仍走全帧 Clear。细粒度按元素 damage 可继续打磨。

### 3.6 文本测量双真源 — ✅ MVP 已解决（阶段五）

[`text/measure`](../crates/rui/src/text/measure.rs)：`prepare` 分段 + glyphon/cosmic-text 测宽（跨帧缓存）+ `layout` 纯算术断行；[`layout_tree`](../crates/rui/src/layout/layout_tree.rs) 的 Taffy leaf measure 已改走 `measure_leaf`。字体不可用时回退启发式，避免测试/无字体环境 panic。

**仍开放**：渲染路径尚未直接消费同一份 `PreparedText` 句柄（测宽已同源，glyph 输出仍由 `prepare_text` 独立 shape）；rich-inline chip/mention 可加深。

---

## 4. 完整差距清单

### 4.1 响应式内核

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | Owner / 作用域树 | ✅ | `create_root` + effect 子 scope |
| ~~P0~~ | Signal 永不回收 | ✅ | owned signal 随 owner 回收 |
| ~~P0~~ | 缺 `onCleanup` | ✅ | `on_cleanup` |
| ~~P0~~ | 缺 Context / Provider | ✅ | `create_context` / `provide_context` / `use_context` |
| ~~P1~~ | memo 急切 + 无拓扑 | ✅ | 惰性 dirty + 深度序 flush |
| ~~P1~~ | 缺 `with` 借用读 | ✅ | `Signal::with` / `Memo::with` |
| ~~P1~~ | 缺原地更新 | ✅ | `Signal::update` |
| ~~P1~~ | scoped ID | ✅ | `use_scoped_signal` |
| ~~P1~~ | 单线程 runtime，无后台线程桥 | ✅ MVP | [`ui_bridge`](../crates/rui/src/app/ui_bridge.rs)：`run_on_ui_thread` / `spawn_blocking` + `RuiUserEvent::UiWake` |
| ~~P1~~ | 缺 Store（路径级订阅） | ✅ MVP | [`create_store`](../crates/rui/src/core/store.rs) + `store_select`（memo 路径）；完整 proxy/`produce` 仍可加深 |
| ~~P1~~ | 绑定分发是 O(n) DFS，不是 O(1) 直达 | ✅ MVP | `template_paths: HashMap<template_id, Vec<usize>>`，查找 O(depth) |
| P2 | 缺 `on()` / `createSelector` / `createDeferred` | 开放 | |

### 4.2 样式引擎

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有变体引擎 | ✅ | `rui_style_tokens` + `ClassSet` + 每帧 resolve |
| ~~P1~~ | 编译期与运行时两套 utility 表不一致 | ✅ | 共享 [`rui_style_tokens`](../crates/rui_style_tokens)；宏展开为 `with_class_list` |
| ~~P1~~ | 无 dark mode / 运行时换肤 | ✅ | `set_theme_mode` / `toggle_theme_mode` + dark tokens |
| ~~P1~~ | Tailwind 色板不完整 | ✅ | 常见 family × 50–950；`try_parse_color` 可区分未知 |
| P2 | 缺断点、负值、任意值覆盖不全 | 开放 | `sm:` 等仍未知；任意值 `[…]` 已支持间距 |
| P2 | `transition-*` / `animate-*` 无类名入口 | 开放 | 动画 API 在 [`animation.rs`](../crates/rui/src/style/animation.rs)，无 utility 入口 |

### 4.3 渲染与布局

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有脏区重绘（实现） | ✅ MVP | [`damage.rs`](../crates/rui/src/app/damage.rs) + scissor；细粒度 damage 仍可打磨 |
| ~~P1~~ | 缺 opacity / translate | ✅ MVP | `opacity-*` / `translate-x/y-*` + 布局收集乘 opacity |
| ~~P1~~ | 无局部布局：脏标记是全局单值 | ✅ MVP | `template_paths` 路径索引；`bind_text` 在固有宽不变时标 `StyleChanged` 跳过全量 taffy。**仍开放（P2）**：真正的按子树 `mark_dirty` + 局部 `compute_layout` |
| ~~P1~~ | 缺渐变 / 多层阴影 / rotate·scale | ✅ MVP | 两色渐变条带；`parse_box_shadows` 多层；`rotate_deg`/`scale` + WGSL 旋转。完整 3D/矩阵仍可加深 |
| ~~P1~~ | 缺 `position: fixed` 与 `aspect-ratio` | ✅ MVP | `position_fixed`（Absolute+忽略滚动）；`aspect_ratio`→taffy；utility `fixed`/`aspect-*` |
| P2 | 单窗口 | 开放 | |
| P2 | 无按子树的 taffy 局部重算 | 开放 | 阶段五 MVP 已避免「同宽文本改动」触发全量；几何变更仍从根 `compute_layout` |

### 4.4 事件与文本

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 布局期文本测量与渲染期 shaping 是两套真源 | ✅ MVP | [`text/measure`](../crates/rui/src/text/measure.rs) + `layout_tree` 改走 `measure_leaf`；无字体时启发式回退 |
| ~~P1~~ | 无真实断行，行数靠比例估算 | ✅ MVP | `prepare` 分段（空白/CJK 断点）+ `layout` 贪心折行 |
| ~~P1~~ | 文本测量无跨帧缓存 | ✅ | `PREPARE_CACHE` 按 (text, size, weight, line_height) 长存 |
| ~~P1~~ | 无富文本 | ✅ MVP | `TextSpan` + `set_rich_text`；chip/mention 式 inline flow 仍可加深 |
| ~~P1~~ | 无 RTL / bidi、无 letter-spacing | ✅ MVP | `TextDirection` + LRM/RLM；`letter_spacing`/`tracking-*`（细空格近似） |
| ~~P0~~ | 无事件冒泡 / 捕获 / pointer capture | ✅ | [`pointer_events.rs`](../crates/rui/src/app/pointer_events.rs) |
| ~~P0~~ | 无右键、双击、系统文件拖放 | ✅ | [`window_events.rs`](../crates/rui/src/app/window_events.rs) + `on_file_drop` |
| ~~P1~~ | 静态文本不可选中 / 无 ellipsis | ✅ | Label 选区 + `truncate` / `TextOverflow::Ellipsis` |
| P2 | 无触摸 / 手势，滚动无系统惯性 | 开放 | |

> 阶段五已落地 Pretext 分层的布局侧：`prepare` / `layout`。下一步是让 `prepare_text` 直接消费同一份 `PreparedText`，并补 RTL / 富文本。

### 4.5 无障碍

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | AccessKit 树扁平，且没有 Action | ✅ | 层级 `set_children` + Click/Focus Action |
| P2 | 无 live region、无 name/description 映射 | 开放 | |

### 4.6 工程化

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 无 CI / LICENSE | ✅ | `.github/workflows/ci.yml` + MIT |
| ~~P0~~ | 无离屏 golden 骨架 | ✅ | [`renderer/offscreen.rs`](../crates/rui/src/renderer/offscreen.rs) clear/readback |
| ~~P1~~ | 完整场景 golden 未接入 CI | ✅ MVP | `assert_matches_golden` / `encode_rgba_golden`（裸 `RUIG` + RGBA8，无图像编解码依赖，`RUI_UPDATE_GOLDEN=1` 重录）；CI 跑 `cargo test -p rui --lib offscreen`；组件级场景 golden 可继续加深 |
| ~~P1~~ | 热路径大量 unwrap，release `panic=abort` | ✅ MVP | glyphon prepare/render 与 taffy.layout soft-fail+log；其余不变量仍可继续收敛 |
| ~~P1~~ | clippy `-D warnings` / fmt 门禁未收紧 | ✅ | `fmt --check` + workspace `-D warnings` |
| ~~P1~~ | 无跨平台冒烟 | ✅ MVP | CI 三平台 `RUI_FORCE_SOFTWARE=1` + 离屏 clear/readback；真窗口冒烟仍可本地补 |
| P2 | 无 benchmark 基线、无结构化 tracing | 开放 | |

### 4.7 宏与 DX

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | `ui!` 报错 span 丢失 | ✅ | `err.to_compile_error()` |
| ~~P1~~ | 原语未知属性静默忽略 | ✅ | 宏展开 `#[deprecated]` 提示未知属性；告警暴露出的真实丢弃项（`absolute` / `relative` / `inset` / `top`·`right`·`bottom`·`left` / `z_index` / `text_color`）已在原语路径补齐 |
| ~~P1~~ | `.get()` 启发式误判 / 漏判 | ✅ | 收紧 receiver 链；零参 `.get()` + path/field |
| P2 | 缺 Fragment / keyof / ref / 动态标签 | 开放 | |
| ~~P2~~ | `#[component]` 不支持泛型，丢弃文档注释 | ✅ | 转发 generics + `///` / 参数 docs |

---

## 5. SolidJS 能力对齐

已对齐 **14** 项、部分对齐 **3** 项、完全缺失 **11** 项。

> 按 §1 的收窄口径读这张表：它衡量的是**细粒度响应式编程模型**的覆盖度，不是「与 solid-main 整仓对齐度」。表里剩下的缺口主要是 `createResource` / `Suspense` / `ErrorBoundary` / `createSelector`；`<Dynamic>`、`mergeProps` 一类是 DX 便利，可长期空着。DOM / SSR / hydration 相关的 Solid API 不在对齐范围内。

| Solid API | RUI 对应 | 状态 | 备注 |
|-----------|----------|------|------|
| `createSignal` | `Signal::new` / `create_signal` | 有 | 另有 `with` / `update` / `peek` |
| setter 函数式更新 | `Signal::update` | 有 | 阶段一 |
| `createEffect` | `create_effect` | 有 | 挂在 Owner 子树上 |
| `createMemo` | `create_memo` | 有 | 惰性 dirty + 深度 flush |
| `untrack` | `untrack` | 有 | |
| `batch` | `batch` | 有 | |
| `on(deps, fn)` | — | 无 | |
| `createRoot` / Owner | `create_root` / `OwnerHandle` | 有 | 阶段一 |
| `onCleanup` | `on_cleanup` | 有 | 阶段一 |
| `onMount` | — | 部分 | |
| `createContext` / `useContext` | `create_context` / `provide_context` / `use_context` | 有 | 阶段一 |
| `createStore` / `produce` | `create_store` / `store_select` | 有 | 阶段五 MVP；完整 proxy/`produce` 仍可加深 |
| `reconcile` | `for_keyed` | 部分 | |
| `createResource` | — | 无 | |
| `Suspense` | — | 无 | |
| `ErrorBoundary` | — | 无 | |
| `createSelector` | — | 无 | |
| `createDeferred` / transition | — | 无 | |
| `<Show>` | `show_reactive` / `show_scoped` | 有 | |
| `<For>` | `for_reactive` / `for_keyed` | 有 | |
| `<Index>` | `for_each` | 部分 | |
| `<Switch>` / `<Match>` | — | 无 | |
| `<Dynamic>` | — | 无 | |
| `<Portal>` | `Portal` | 有 | |
| `children()` helper | — | 无 | |
| `mergeProps` / `splitProps` | — | 无 | |
| 细粒度属性绑定 | `bind_*` | 有 | 亮点 |
| DevTools | — | 无 | |

---

## 6. 组件清单

阶段四后 **已有 26 / 占位 3 / 缺失 21**。关键 P0 产品空洞（定位引擎、Tooltip、ContextMenu、Combobox、VirtualList、DataTable、TreeView、Form、Textarea、Toast、Resizable、FilePicker）均有 MVP。仍缺：AlertDialog、Sheet、Menubar、Command 面板、Chart、桌面外壳（托盘/多窗口）等；DatePicker/ColorPicker/Router 仍为占位。

**以 EUI 作完整度标尺**（不作架构参照，见 §1）。逐项对照 [`Reference/EUI-NEO-main/components`](../Reference/EUI-NEO-main/components) 后，RUI 明确落后的产品面：

| EUI 已有 | RUI 现状 | 说明 |
|----------|----------|------|
| `barchart` / `linechart` / `piechart` | 缺失 | 需先补路径 / 多边形绘制能力 |
| `markdown`（内置 md4c） | 缺失 | 依赖富文本 inline flow（见 §4.4） |
| `datepicker` / `timepicker` | 占位壳 | `date_picker.rs` 仅 93 行 |
| `colorpicker` | 占位壳 | 48 行 |
| `carousel` / `card_slider` | 缺失 | |
| `stepper` / `segmented` / `progress` | 缺失 | |
| `virtualmasonry` | 缺失 | 依赖可信行高（见 §4.4） |
| `navbar` / `sidebar` | 缺失 | |

其中 `markdown` 与 `virtualmasonry` 都卡在同一个前提——文本管线，这进一步支持把文本排在阶段五第一位。

---

## 7. 建议推进顺序

### 阶段一 · 内核硬化（地基）— ✅ 完成（2026-08-05）

- [x] `Owner` / `Scope` 树 + `on_cleanup`，Signal 随 owner 回收
- [x] `create_context` / `provide_context` / `use_context`
- [x] `use_scoped_signal`（owner 相对路径）；`use_signal` 全局键保留 + 类型撞键 assert
- [x] memo 惰性求值 + 按深度分层调度（菱形无 glitch）
- [x] `Signal::with` / `update`
- [x] 三平台 CI（`fmt --check` + build + test + `clippy -D warnings`）
- [x] LICENSE（MIT）与 Cargo.toml 元数据
- [x] 修正 README / PROJECT.md「脏区重绘」表述
- [x] 清掉 workspace clippy / rustfmt 存量债

### 阶段二 · 样式与宏（表达力）— ✅ 完成（2026-08-05）

- [x] 合并编译期 / 运行时两张 utility 表为单一真源（`rui_style_tokens` + `with_class_list`）
- [x] 未知类名给编译期告警（`#[deprecated]` 提示）
- [x] 变体引擎：`hover:` / `focus:` / `disabled:` / `dark:` / `data-[state=*]:`
- [x] `Theme` 可切换 + dark token + 完整色阶
- [x] 修 `ui!` 错误 span（`to_compile_error`）
- [x] 收紧 `.get()` 启发式（receiver 链约束）
- [x] `#[component]` 转发泛型与文档注释

### 阶段三 · 事件与渲染（桌面完整度）— ✅ 完成（2026-08-05）

- [x] 事件冒泡 / 捕获 / pointer capture（[`pointer_events.rs`](../crates/rui/src/app/pointer_events.rs)）
- [x] 右键、双击、系统文件拖放
- [x] 脏区重绘（scissor + damage MVP）
- [x] 元素级 opacity 与 translate（`opacity-*` / `translate-x/y-*`）
- [x] AccessKit 语义层级与 Action
- [x] 静态文本可选中 + ellipsis（`truncate`）
- [x] 离屏 wgpu 像素 golden 测试骨架

### 阶段四 · 组件与生态（产品面）— ✅ MVP 完成（2026-08-05）

- [x] 统一浮层定位引擎（[`sync_floating_placement`](../crates/rui/src/utils/popup.rs)）
- [x] Tooltip / ContextMenu / Combobox
- [x] VirtualList → DataTable / TreeView
- [x] Form + 校验（Context）
- [x] Toast / Resizable / FilePicker / Textarea
- [ ] 发布 crates.io + 文档站

### 阶段五 · 文本与工程化（发布级）— ✅ MVP 完成（2026-08-06）

- [x] 文本管线：[`text/measure`](../crates/rui/src/text/measure.rs) `prepare`/`layout` + [`layout_tree`](../crates/rui/src/layout/layout_tree.rs) 接入
- [x] Store：[`create_store`](../crates/rui/src/core/store.rs) + `store_select`
- [x] 线程桥：[`run_on_ui_thread`](../crates/rui/src/app/ui_bridge.rs) / `spawn_blocking` + `RuiUserEvent::UiWake`
- [x] 跨平台离屏冒烟：CI `RUI_FORCE_SOFTWARE=1` + `offscreen` 测试
- [x] golden CI MVP：`assert_matches_golden` / encode-load roundtrip
- [x] 局部布局 MVP：`template_paths` 索引；`bind_text` 同宽 → `StyleChanged`
- [ ] 发布 crates.io + 文档站（仍延后）
- [x] 富文本 spans + RTL / letter-spacing MVP
- [ ] 按子树 taffy 局部重算 / `prepare_text` 直接消费 `PreparedText`（下一阶段打磨）

---

## 相关文档

- [PROJECT.md](PROJECT.md) — 愿景、分层架构与阶段计划
- [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md) — Stage 7+ 长期项排期
- [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md) — 三档更新与热重载模板契约
- [headless.md](headless.md) — 无头组件模式
