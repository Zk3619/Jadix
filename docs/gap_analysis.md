# 工程级差距分析（Gap Analysis）

> **历史审计底稿（非巩固期执行源）**。排期与勾选请以 [AUTHORITY.md](AUTHORITY.md) 为准；门槛与禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md)；**Solid 细粒度现状**见 [REACTIVITY.md](REACTIVITY.md)。  
> 审计对象：`crates/rui`、[`crates/rui_macros`](../crates/rui_macros/src/lib.rs)、`crates/rui_style_tokens`、`crates/rui_hot_reload_cli`、`docs/`  
> 对照基准（各司其职，见 §1）：[`Reference/SolidJS`](../Reference/SolidJS) 编程模型 · [`Reference/ShadCN-UI`](../Reference/ShadCN-UI) + [`Reference/TailwindCSS`](../Reference/TailwindCSS) 组件体验 · [`Reference/EUI-NEO-main`](../Reference/EUI-NEO-main) 产品完整度 · [`Reference/pretext-main`](../Reference/pretext-main) 文本算法  
> 主审计日期：2026-08-07；**勘误条**：2026-08-09（体量 / Solid 表 / 执行移交）；**2026-08-12**：§5 镜像移交 REACTIVITY.md；**2026-08-13**：公开自绘面移交 [`drawing.md`](drawing.md) / AUTHORITY WP-3.11–3.13；**2026-08-18**：§5 For/Index 行与 REACTIVITY §5.10 对齐

三个问题：**距离工程级 UI 框架还差什么**、**类 SolidJS 能力还缺什么**、**还需要哪些组件**。

**等级约定**

| 等级 | 含义 |
|------|------|
| P0 | 地基性缺陷，越晚改代价越大，或直接阻塞其他能力 |
| P1 | 影响可用性 / 完整度，但不阻塞架构演进 |
| P2 | 打磨项，可延后 |

**统计（阶段六后 · 历史口径）**：阶段清单 P0/P1/P2 项已关；开放面转为巩固期工程门槛（真窗冒烟、crates.io、文档站、组件 golden、稳定面 CV）。  
阶段一～六关闭摘要见下文 §7；阶段七～十二 MVP 见 [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md)。

> **勘误（2026-08-09）**  
> - 体量现约 **5.3 万行 / 182 `.rs`**（文内旧「3.3 万 / 121」为阶段六时点）。  
> - Solid 表：`ErrorBoundary`、`switch_reactive`、`on_mount` **已落地**（勿再读 §5 旧「无」）。  
> - 组件清单以 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §5 为准（约 45 模块），勿再依赖本节「26/3/21」旧计数。  
> - 巩固期开放 P0：冒烟签署、crates.io、文档站正文、CHANGELOG——见纲领 §4.4 / 总计划 §1。  
> - **公开 `<svg>` / `<canvas>` / 2D·3D** 不是历史 P0；W3 起见 [`drawing.md`](drawing.md)。emoji-svg 光栅 ≠ 公开 Svg。

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
| 布局局部更新 | ✅ **MVP** | 同宽文本 → `StyleChanged`；几何变更 → 子树 `mark_dirty` + 从根 `compute_layout`（脏缓存） |
| 属性更新 O(1) 直达 | ✅ MVP | `template_paths` 路径索引，查找 O(depth) |

**阶段五后**：文本测宽与跨帧 prepare 缓存已统一；计数器类 `bind_text` 在固有宽不变时不再触发全树布局。保留树 + Signal 路线继续加深，而不是另开架构分叉。

---

## 2. 总体判断

以 Alpha 期自绘框架衡量，完成度偏高：`winit + wgpu + taffy + glyphon` 全自绘栈、Solid 式 Owner / Signal / Memo / Context、按 `template_id` 的子树 reconcile，组件面已铺到表格 / 浮层 / 表单，热重载与体积 / 内存均有实测文档。骨架方向已经选对。（体量时点数见文首勘误。）

细粒度更新这一层做得比预期扎实。[`bind_text` / `bind_prop` / `bind_class` / `bind_style`](../crates/rui/src/app/reactive.rs) 这套显式绑定，加上子树 reconcile，方向完全正确。热重载链路也超出同规模项目的常见水平。

**阶段一（内核硬化）已落地**（见 [`core/reactivity.rs`](../crates/rui/src/core/reactivity.rs)）。  
**阶段二（样式与宏）已落地**：[`rui_style_tokens`](../crates/rui_style_tokens) 为宏与运行时共享目录；[`style/utility.rs`](../crates/rui/src/style/utility.rs) 支持变体与 `opacity` / `translate-*` / `truncate`。  
**阶段三（事件与渲染）已落地**：[`pointer_events`](../crates/rui/src/app/pointer_events.rs) 冒泡/捕获/capture；脏区 [`damage`](../crates/rui/src/app/damage.rs)+scissor；AccessKit 层级与 Action；静态文本选区；[`renderer/offscreen`](../crates/rui/src/renderer/offscreen.rs) golden 骨架。  
**阶段四（组件与生态）已落地 MVP**：[`sync_floating_placement`](../crates/rui/src/utils/popup.rs)；Tooltip / ContextMenu / Combobox；VirtualList / DataTable / TreeView；Form / Textarea；ToastHost / Resizable / FilePicker(`rfd`)。

接下来卡住「发布级」的**不是再换一条哲学路线，而是原生层与工程层的打磨**（§1 已说明为什么不换底座）。阶段七～十二 MVP 历史排期见 [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md)；**巩固期执行**见 [AUTHORITY.md](AUTHORITY.md)。

---

## 3. 五个结构性问题（阶段四后状态）

### 3.1 Owner 树 / 生命周期 — ✅ 已解决（阶段一）

[`create_root`](../crates/rui/src/core/reactivity.rs) / Owner 子树、`on_cleanup`、effect 重跑前 `dispose_owner_children`、owned `create_signal` 随 owner 回收。

**仍开放**：[`app/reactive.rs`](../crates/rui/src/app/reactive.rs) 的 `REGISTERED_ANCHORS` / `REGISTERED_BINDINGS` 仍是 UI 层手工表；可逐步改为挂在 owner 下。

### 3.2 全局字符串 ID — ✅ 部分解决（阶段一）

- [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)：键相对 owner path。
- [`use_signal`](../crates/rui/src/core/reactivity.rs)：**保留全局键**（Input 焦点、滚动等需跨 rebuild 存活）。

**仍开放**：`anchor_id` / `template_id` / `focus_id` 等 UI 层字符串仍扁平。跨层身份契约与空串/a11y 事故原型见 [`element_identity.md`](element_identity.md)。

### 3.3 样式交互态无声明式出口 — ✅ 已解决（阶段二）

变体引擎已落地：`hover:` / `focus:` / `focus-visible:` / `active:` / `disabled:` / `dark:` / `data-[key=value]:`。  
静态 `class=` 与动态 `bind_class` 走同一 [`apply_class_list`](../crates/rui/src/style/utility.rs)；每帧 chrome sync 调用 [`sync_class_variants_subtree`](../crates/rui/src/style/utility.rs)。

**仍开放**：无（断点 / `group-hover:` / `peer-*` / `transition-*` CSS 入口已 MVP）。

### 3.4 质量门禁 — ✅ 基础已落地（阶段一 + 三骨架）

- CI：`fmt --check` + build + test + `clippy --workspace -D warnings`
- MIT LICENSE + crate 元数据
- 离屏 wgpu clear/readback 烟雾测试（[`offscreen.rs`](../crates/rui/src/renderer/offscreen.rs)）

**仍开放**：完整场景 / 组件级 golden 对比可继续加深。热路径 unwrap 已 MVP 收敛。

### 3.5 文档承诺与实现不符 — ✅ 已纠偏（阶段一）；脏区实现 ✅ MVP（阶段三）

[`union_damage`](../crates/rui/src/app/state.rs) + `LoadOp::Load` + scissor；结构变更仍走全帧 Clear。细粒度按元素 damage 可继续打磨。

### 3.6 文本测量双真源 — ✅ MVP 已解决（阶段五）

[`text/measure`](../crates/rui/src/text/measure.rs)：`prepare` 分段 + glyphon/cosmic-text 测宽（跨帧缓存）+ `layout` 纯算术断行；[`layout_tree`](../crates/rui/src/layout/layout_tree.rs) 的 Taffy leaf measure 已改走 `measure_leaf`。字体不可用时回退启发式，避免测试/无字体环境 panic。

**仍开放**：逐行变宽绕排、块级 markdown（标题/列表/代码块）；glyphon 断行与 `layout_lines` 完全对齐仍可打磨。

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
| ~~P1~~ | 缺 Store（路径级订阅） | ✅ | [`Store::field`](../crates/rui/src/core/store.rs)（WP-R.3 / I-8）；`store_select`+`produce` 过渡可留 |
| ~~P1~~ | 绑定分发是 O(n) DFS，不是 O(1) 直达 | ✅ MVP | `template_paths: HashMap<template_id, Vec<usize>>`，查找 O(depth) |
| ~~P2~~ | 缺 `on()` / `createSelector` / `createDeferred` | ✅ MVP | [`on`](../crates/rui/src/core/reactivity.rs) / [`create_selector`](../crates/rui/src/core/reactivity.rs) / [`create_deferred`](../crates/rui/src/core/reactivity.rs)（单 flush 延迟，非 idle 超时） |

### 4.2 样式引擎

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有变体引擎 | ✅ | `rui_style_tokens` + `ClassSet` + 每帧 resolve |
| ~~P1~~ | 编译期与运行时两套 utility 表不一致 | ✅ | 共享 [`rui_style_tokens`](../crates/rui_style_tokens)；宏展开为 `with_class_list` |
| ~~P1~~ | 无 dark mode / 运行时换肤 | ✅ | `set_theme_mode` / `toggle_theme_mode` + dark tokens |
| ~~P1~~ | Tailwind 色板不完整 | ✅ | 常见 family × 50–950；`try_parse_color` 可区分未知 |
| ~~P2~~ | 缺断点、负值、任意值覆盖不全 | ✅ MVP | `sm:`–`2xl:` 断点 + `-m-*`/`-translate-*` + `group-hover:`/`peer-*`（含命名）；任意值间距已有 |
| ~~P2~~ | `transition-*` / `animate-*` 无类名入口 | ✅ MVP | `transition-opacity`/`transition-transform` → CSS `tick_element_transitions`；`animate-spin` 旋转 |

### 4.3 渲染与布局

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有脏区重绘（实现） | ✅ MVP | [`damage.rs`](../crates/rui/src/app/damage.rs) + scissor；细粒度 damage 仍可打磨 |
| ~~P1~~ | 缺 opacity / translate | ✅ MVP | `opacity-*` / `translate-x/y-*` + 布局收集乘 opacity |
| ~~P1~~ | 无局部布局：脏标记是全局单值 | ✅ MVP | `template_paths` 路径索引；`bind_text` 同宽 → `StyleChanged`；几何变更 → 子树 `mark_dirty`（见下行） |
| ~~P1~~ | 缺渐变 / 多层阴影 / rotate·scale | ✅ MVP | 两色渐变条带；`parse_box_shadows` 多层；`rotate_deg`/`scale` + WGSL 旋转。完整 3D/矩阵仍可加深 |
| ~~P1~~ | 缺 `position: fixed` 与 `aspect-ratio` | ✅ MVP | `position_fixed`（Absolute+忽略滚动）；`aspect_ratio`→taffy；utility `fixed`/`aspect-*` |
| ~~P2~~ | 单窗口 | ✅ MVP | [`multi_window`](../crates/rui/src/app/multi_window.rs) + [`secondary_window`](../crates/rui/src/app/secondary_window.rs)：`open_secondary_window(title, builder)`；次窗独立 layout/text/root；共享主窗 `device`/`queue` |
| ~~P2~~ | 无按子树的 taffy 局部重算 | ✅ MVP | `mark_dirty` + [`pick_subtree_compute_root`](../crates/rui/src/layout/layout_tree.rs) → `build_and_layout_fast_ex`；同父脏节点从该父 compute；否则 app root |

### 4.4 事件与文本

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 布局期文本测量与渲染期 shaping 是两套真源 | ✅ MVP | [`text/measure`](../crates/rui/src/text/measure.rs) + `layout_tree` 改走 `measure_leaf`；无字体时启发式回退 |
| ~~P1~~ | 无真实断行，行数靠比例估算 | ✅ MVP | `prepare` 分段（空白/CJK 断点）+ `layout` 贪心折行 |
| ~~P1~~ | 文本测量无跨帧缓存 | ✅ | `PREPARE_CACHE` 按 (text, size, weight, line_height) 长存 |
| ~~P1~~ | 无富文本 | ✅ MVP | `TextSpan` + chip/mention + [`parse_markdown_inline`](../crates/rui/src/text/markdown_inline.rs)（粗体/斜体/code/链接）；`Element::markdown_inline`；块级 md 仍可加深 |
| ~~P1~~ | 无 RTL / bidi、无 letter-spacing | ✅ MVP | `TextDirection` + LRM/RLM；`letter_spacing`/`tracking-*`（细空格近似） |
| ~~P0~~ | 无事件冒泡 / 捕获 / pointer capture | ✅ | [`pointer_events.rs`](../crates/rui/src/app/pointer_events.rs) |
| ~~P0~~ | 无右键、双击、系统文件拖放 | ✅ | [`window_events.rs`](../crates/rui/src/app/window_events.rs) + `on_file_drop` |
| ~~P1~~ | 静态文本不可选中 / 无 ellipsis | ✅ | Label 选区 + `truncate` / `TextOverflow::Ellipsis` |
| ~~P2~~ | 无触摸 / 手势，滚动无系统惯性 | ✅ MVP | Touch 主键 + 惯性；长按→右键路径；双指 pinch → `__rui_pinch_scale` |

> 阶段五已落地 Pretext 分层的布局侧：`prepare` / `layout`。P2 深化后 `TextLayoutInfo.prepared` 接入 `prepare_text` soft-wrap；主/次窗 shaping 与 measure 经 `install_shared_font_system` / `new_sharing_font_system` 共用同一 `FontSystem`。

### 4.5 无障碍

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | AccessKit 树扁平，且没有 Action | ✅ | 层级 `set_children` + Click/Focus Action |
| ~~P2~~ | 无 live region、无 name/description 映射 | ✅ MVP | `aria-label`/`aria-description`/`aria-live`（及 `accesskit_*`）→ label/description/`set_live` |

### 4.6 工程化

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 无 CI / LICENSE | ✅ | `.github/workflows/ci.yml` + MIT |
| ~~P0~~ | 无离屏 golden 骨架 | ✅ | [`renderer/offscreen.rs`](../crates/rui/src/renderer/offscreen.rs) clear/readback |
| ~~P1~~ | 完整场景 golden 未接入 CI | ✅ MVP | `assert_matches_golden` / `encode_rgba_golden`（裸 `RUIG` + RGBA8，无图像编解码依赖，`RUI_UPDATE_GOLDEN=1` 重录）；CI 跑 `cargo test -p rui --lib offscreen`；组件级场景 golden 可继续加深 |
| ~~P1~~ | 热路径大量 unwrap，release `panic=abort` | ✅ MVP | glyphon prepare/render 与 taffy.layout soft-fail+log；其余不变量仍可继续收敛 |
| ~~P1~~ | clippy `-D warnings` / fmt 门禁未收紧 | ✅ | `fmt --check` + workspace `-D warnings` |
| ~~P1~~ | 无跨平台冒烟 | ✅ MVP | CI 三平台 `RUI_FORCE_SOFTWARE=1` + 离屏 clear/readback；真窗口冒烟仍可本地补 |
| ~~P2~~ | 无 benchmark 基线、无结构化 tracing | ✅ MVP | feature `tracing` + layout/reconcile span；`cargo bench -p rui --bench layout_baseline` |

### 4.7 宏与 DX

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | `ui!` 报错 span 丢失 | ✅ | `err.to_compile_error()` |
| ~~P1~~ | 原语未知属性静默忽略 | ✅ | 宏展开 `#[deprecated]` 提示未知属性；告警暴露出的真实丢弃项（`absolute` / `relative` / `inset` / `top`·`right`·`bottom`·`left` / `z_index` / `text_color`）已在原语路径补齐 |
| ~~P1~~ | `.get()` 启发式误判 / 漏判 | ✅ | 收紧 receiver 链；零参 `.get()` + path/field |
| ~~P2~~ | 缺 Fragment / Spreading / ref / 动态标签 | ✅ MVP | `<Fragment>` / `..spread` / `ref={Signal}` / `<Dynamic component={…}>`；`IntoElementList` 扁平 fragment |
| ~~P2~~ | `#[component]` 不支持泛型，丢弃文档注释 | ✅ | 转发 generics + `///` / 参数 docs |

---

## 5. SolidJS 能力对齐

> **2026-08-09 勘误后口径**：编程模型主路径已齐；剩余多为 DX 糖与 DevTools。按 §1 收窄口径：衡量细粒度响应式覆盖度，非整仓 solid-main。DOM / SSR / hydration 不在范围内。  
> **2026-08-12**：下表为**历史镜像**；权威清单、四框架机制对照（Solid / Leptos / Floem / Dioxus）与完整细粒度不变量以 [REACTIVITY.md](REACTIVITY.md) §4–§6 / §5.7 为准；执行 [AUTHORITY.md](AUTHORITY.md)。  
> **2026-08-18**：For / Index 主路径已是 `for_keyed_reactive` / `index_reactive`（WP-R.5 ☑）。Store 路径触发器 `Store::field`（WP-R.3 ☑）。`store_select`+`produce` 仍为整根过渡。三框架源码对照见 [REACTIVITY.md](REACTIVITY.md) §5.10。机制深描见 [REACTIVITY.md](REACTIVITY.md) §0.2 / §5.10。

| Solid API | RUI 对应 | 状态 | 备注 |
|-----------|----------|------|------|
| `createSignal` | `Signal::new` / `create_signal` | 有 | 另有 `with` / `update` / `peek` / `try_set` |
| setter 函数式更新 | `Signal::update` | 有 | 阶段一 |
| `createEffect` | `create_effect` / `create_computed` | 有 | **WP-R.9 ☑** User / Computed 相；`bind_*` 为 Render 相 |
| `createMemo` | `create_memo` | 有 | 惰性 dirty + 深度 flush |
| `untrack` | `untrack` | 有 | |
| `batch` | `batch` | 有 | |
| `on(deps, fn)` | `on` | 有 | 阶段六；仅跟踪 deps，body 走 `untrack` |
| `createRoot` / Owner | `create_root` / `OwnerHandle` | 有 | 阶段一 |
| `onCleanup` | `on_cleanup` | 有 | 阶段一 |
| `onMount` | `on_mount` | 有 | 阶段九 MVP |
| `createContext` / `useContext` | `create_context` / `provide_context` / `use_context` | 有 | 阶段一 |
| `createStore` / `produce` | `create_store` / `Store::field` / `store_select` / produce | 有 | **WP-R.3 ☑** 路径触发；`store_select` 仍订根 |
| `reconcile` | `for_keyed` | 部分 | |
| `createResource` | [`create_resource`](../crates/rui/src/app/resource.rs) | 有 | 阶段六 |
| `Suspense` | [`suspense`](../crates/rui/src/app/resource.rs) | 有 | 阶段六 |
| `ErrorBoundary` | [`error_boundary`](../crates/rui/src/app/error_boundary.rs) | 有 | 阶段九 MVP；e2e 契约测见巩固期 WP-2.5 |
| `createSelector` | `create_selector` / `Selector::is` | 有 | 阶段六 |
| `createDeferred` / transition | `create_deferred` | 有 | 阶段六；单 flush 延迟，非 idle |
| `<Show>` | `show_reactive` / `show_scoped` | 有 | |
| `<For>` | `for_keyed_reactive`（主）/ `for_reactive`+`for_keyed`（整表降级） | 有 | **WP-R.5b ☑**：项级 Owner；无 key 的 `<For>` 仍整表。对照 Solid `mapArray`（REACTIVITY §5.5 / §5.10） |
| `<Index>` | `index_reactive`（主）/ `for_each`（下标键降级） | 有 | **WP-R.5c ☑**：项为 `Signal`；`for_each` ≠ Index |
| `<Switch>` / `<Match>` | `<Match key>` → `switch_reactive`；`<MatchArm>`/`<When>` → `match_when_reactive`（RUI `<Switch>` 是 toggle） | 有 | RSX 糖 ☑；toggle 名不占用 |
| `<Dynamic>` | `<Dynamic component={…}>` | 有 | 阶段六 |
| `<Portal>` | `Portal` | 有 | |
| `children()` helper | [`children`](../crates/rui/src/app/props.rs) / [`resolve_children`](../crates/rui/src/app/props.rs) | 有 | **B2 ☑**；fragment 展平；闭包版订 signal |
| `mergeProps` / `splitProps` | [`merge_props`](../crates/rui/src/app/props.rs) / `merge_props!` / `split_props!` | 有 | **B2 ☑**；后源 `Some` 覆盖；非 JS Proxy |
| 细粒度属性绑定 | `bind_*` | 有 | 亮点 |
| DevTools | Inspector MVP（`devtools` feature） | B1 P0–P2 ☑ 2026-08-19 |

---

## 6. 组件清单

历史计数「26 / 3 / 21」已过时。**现行清单与分桶**见 [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) §5 + [component_inventory.md](component_inventory.md)（约 43 产品模块；`verified` / `stable-candidate` / `experimental` / `broken`）。关键产品空洞均有 MVP；Router wired、托盘 stub。

**EUI 只作密度标尺**（不作架构 / API 参照，见 §1）。完整对照表在 inventory §3；此处仅保留仍落后的密度项：

| EUI 已有 | RUI 现状 | 密度结论 |
|----------|----------|----------|
| `piechart` | 无（仅 bar/line MVP） | 缺 |
| 块级 `markdown` | 仅 inline MVP | 缺 |
| `virtualmasonry` | **有（experimental，WP-3.10 ☑）** | 齐密度；升格见 AUTHORITY B5 |
| `segmented` | 无独立模块；用 ToggleGroup | 等价可接受 |

其余 EUI 控件头（button/input/tabs/dialog/toast/datepicker/…）RUI 已有对位；差距在**信任度（CV 分桶）**而非再扩表面积。

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
- [x] Store：[`create_store`](../crates/rui/src/core/store.rs) + `Store::field`（WP-R.3）+ `store_select`
- [x] 线程桥：[`run_on_ui_thread`](../crates/rui/src/app/ui_bridge.rs) / `spawn_blocking` + `RuiUserEvent::UiWake`
- [x] 跨平台离屏冒烟：CI `RUI_FORCE_SOFTWARE=1` + `offscreen` 测试
- [x] golden CI MVP：`assert_matches_golden` / encode-load roundtrip
- [x] 局部布局 MVP：`template_paths` 索引；`bind_text` 同宽 → `StyleChanged`
- [ ] 发布 crates.io + 文档站（仍延后）
- [x] 富文本 spans + RTL / letter-spacing MVP
- [x] 按子树 taffy `mark_dirty`（仍从根 `compute_layout`；脏缓存）
- [x] `prepare_text` 消费 `PreparedText` soft-wrap
- [x] measure↔renderer 共享 `FontSystem`（`install_shared_font_system`）

### 阶段六 · P2 打磨与深化 — ✅ 完成（2026-08-07）

> 原「打磨 / 深化 / 加深 / 加深++」已合并为本阶段，不再分 + 档。

- [x] `on` / `create_selector` / `create_deferred`
- [x] 断点 `sm:`–`2xl:` + 负间距 + `transition-*` / `animate-spin`；`group-hover:` / `peer-*`
- [x] AccessKit name/description/live region
- [x] `<Fragment>` / spreading / `ref` / `<Dynamic>`
- [x] 触摸主键 + 滚动惯性；长按 + pinch；`transition-*` CSS tween
- [x] `tracing` feature + `layout_baseline` bench
- [x] 多窗口：`OpenWindow` + [`open_secondary_window`](../crates/rui/src/app/multi_window.rs) + 次窗完整 UI 树
- [x] FontSystem 共享（measure + 主/次窗 shaping）：`install_shared_font_system` / `new_sharing_font_system`
- [x] `PreparedText` soft-wrap；chip/mention；`break_after`；mention 默认色底
- [x] 子树根 compute：`pick_subtree_compute_root` + `build_and_layout_fast_ex`
- [x] [`create_resource`](../crates/rui/src/app/resource.rs) / [`suspense`](../crates/rui/src/app/resource.rs)
- [x] markdown inline：[`parse_markdown_inline`](../crates/rui/src/text/markdown_inline.rs) + `Element::markdown_inline`

### 阶段七～十二 · 工程级发布路线 — ✅ MVP 已落地（见 [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md)）

> 单一清单，不分 +/++。细节与验收标准以 STAGE7_ROADMAP 为准。深化项可继续迭代。

- [x] **阶段七** · 原生输入与焦点：IME 光标区收窄、焦点环 token、roving/Combobox、AccessKit Role、冒烟表
- [x] **阶段八** · 文本与布局：块级 markdown、变宽绕排、damage helpers、scrollbar overlay、字体策略、CPU golden
- [x] **阶段九** · 响应式 DX：`ErrorBoundary`、`switch_reactive`、`on_mount`、Store `produce`、anchors 挂 Owner、headless 布局测
- [x] **阶段十** · 组件产品面：Date/Color 落地、Command/Menubar/Nav、Progress 系、路径+图表、Gallery（MVP）
- [x] **阶段十一** · 桌面外壳：Router、托盘 stub、多窗文档、剪贴板 MIME 助手（MVP）
- [x] **阶段十二** · 发布：`PUBLISH.md`、文档站 stub、CI lib smoke、bench/体积清单（MVP）

---

## 相关文档

- [AUTHORITY.md](AUTHORITY.md) — **巩固期唯一入口与执行源**
- [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) — 工程门槛、禁区、ADR
- [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) — 组件领域规范（shadcn 产品面 + headless 骨架）
- [REACTIVITY.md](REACTIVITY.md) — 细粒度响应式领域规范
- [PROJECT.md](PROJECT.md) — 愿景、分层架构与阶段计划
- [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md) — 阶段七～十二历史排期（执行已移交巩固期计划）
- [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md) — 三档更新与热重载模板契约
- [headless.md](headless.md) — 无头组件模式（从属组件规范）
- [desktop_smoke.md](desktop_smoke.md) — 真窗口冒烟清单
- [archive/README.md](archive/README.md) — 已冻结历史文档
