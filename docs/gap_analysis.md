# 工程级差距分析（Gap Analysis）

> 审计对象：`crates/rui`、[`crates/rui_macros`](../crates/rui_macros/src/lib.rs)、`crates/rui_style_tokens`、`crates/rui_hot_reload_cli`、`Docs/`
> 对照基准：[`Reference/SolidJS`](../Reference/SolidJS)、[`Reference/ShadCN-UI`](../Reference/ShadCN-UI)、[`Reference/TailwindCSS`](../Reference/TailwindCSS)
> 日期：2026-08-05（阶段四进度更新同日）

三个问题：**距离工程级 UI 框架还差什么**、**类 SolidJS 能力还缺什么**、**还需要哪些组件**。

**等级约定**

| 等级 | 含义 |
|------|------|
| P0 | 地基性缺陷，越晚改代价越大，或直接阻塞其他能力 |
| P1 | 影响可用性 / 完整度，但不阻塞架构演进 |
| P2 | 打磨项，可延后 |

**统计（阶段四完成后）**：开放差距约 18 条（P0 0 / P1 10 / P2 8）；SolidJS 能力已对齐 **13/28**（另有 3 项部分对齐）；组件约 **已有 26 / 占位 3 / 缺失 18**。  
阶段一已关闭：Owner / `onCleanup` / Context / scoped ID / 惰性 memo / `with`+`update` / CI+LICENSE / 脏区文档纠偏。  
阶段二已关闭：utility 单一真源 / 未知类名告警 / 变体引擎 / Theme 切换+dark token / 完整色阶 / `ui!` span / `.get()` 收紧 / `#[component]` 泛型+文档。  
阶段三已关闭：事件冒泡/捕获/pointer capture / 右键·双击·文件拖放 / 脏区 scissor MVP / opacity+translate / AccessKit 层级+Action / 静态文本可选中+ellipsis / 离屏 golden 骨架。  
阶段四已关闭：统一 floating sync / Tooltip·ContextMenu·Combobox / VirtualList·DataTable·TreeView / Form·Textarea / Toast·Resizable·FilePicker(rfd)。**仍开放**：crates.io 发布与文档站。

---

## 1. 总体判断

细粒度更新这一层做得比预期扎实。[`bind_text` / `bind_prop` / `bind_class` / `bind_style`](../crates/rui/src/app/reactive.rs) 这套显式绑定，加上按 `template_id` 的子树 reconcile，方向完全正确。热重载链路也超出同规模项目的常见水平。

**阶段一（内核硬化）已落地**（见 [`core/reactivity.rs`](../crates/rui/src/core/reactivity.rs)）。  
**阶段二（样式与宏）已落地**：[`rui_style_tokens`](../crates/rui_style_tokens) 为宏与运行时共享目录；[`style/utility.rs`](../crates/rui/src/style/utility.rs) 支持变体与 `opacity` / `translate-*` / `truncate`。  
**阶段三（事件与渲染）已落地**：[`pointer_events`](../crates/rui/src/app/pointer_events.rs) 冒泡/捕获/capture；脏区 [`damage`](../crates/rui/src/app/damage.rs)+scissor；AccessKit 层级与 Action；静态文本选区；[`renderer/offscreen`](../crates/rui/src/renderer/offscreen.rs) golden 骨架。  
**阶段四（组件与生态）已落地 MVP**：[`sync_floating_placement`](../crates/rui/src/utils/popup.rs)；Tooltip / ContextMenu / Combobox；VirtualList / DataTable / TreeView；Form / Textarea；ToastHost / Resizable / FilePicker(`rfd`)。

接下来卡住「发布级」的主要是：

1. **Store / 后台线程桥**——细粒度数据与异步回灌。
2. **完整 golden CI**——组件级像素对比与 bless。
3. **crates.io + 文档站**——以及浮层/表单/表格的产品打磨。

---

## 2. 五个结构性问题（阶段四后状态）

### 2.1 Owner 树 / 生命周期 — ✅ 已解决（阶段一）

[`create_root`](../crates/rui/src/core/reactivity.rs) / Owner 子树、`on_cleanup`、effect 重跑前 `dispose_owner_children`、owned `create_signal` 随 owner 回收。

**仍开放**：[`app/reactive.rs`](../crates/rui/src/app/reactive.rs) 的 `REGISTERED_ANCHORS` / `REGISTERED_BINDINGS` 仍是 UI 层手工表；可逐步改为挂在 owner 下。

### 2.2 全局字符串 ID — ✅ 部分解决（阶段一）

- [`use_scoped_signal`](../crates/rui/src/core/reactivity.rs)：键相对 owner path。
- [`use_signal`](../crates/rui/src/core/reactivity.rs)：**保留全局键**（Input 焦点、滚动等需跨 rebuild 存活）。

**仍开放**：`anchor_id` / `template_id` / `focus_id` 等 UI 层字符串仍扁平。

### 2.3 样式交互态无声明式出口 — ✅ 已解决（阶段二）

变体引擎已落地：`hover:` / `focus:` / `focus-visible:` / `active:` / `disabled:` / `dark:` / `data-[key=value]:`。  
静态 `class=` 与动态 `bind_class` 走同一 [`apply_class_list`](../crates/rui/src/style/utility.rs)；每帧 chrome sync 调用 [`sync_class_variants_subtree`](../crates/rui/src/style/utility.rs)。

**仍开放**：断点前缀（`sm:`）、`group-hover:` / `peer-*`、`transition-*` / `animate-*` 类名入口。

### 2.4 质量门禁 — ✅ 基础已落地（阶段一 + 三骨架）

- CI：`fmt --check` + build + test + `clippy --workspace -D warnings`
- MIT LICENSE + crate 元数据
- 离屏 wgpu clear/readback 烟雾测试（[`offscreen.rs`](../crates/rui/src/renderer/offscreen.rs)）

**仍开放**：完整场景 golden 对比接入 CI；热路径 unwrap 收敛。

### 2.5 文档承诺与实现不符 — ✅ 已纠偏（阶段一）；脏区实现 ✅ MVP（阶段三）

[`union_damage`](../crates/rui/src/app/state.rs) + `LoadOp::Load` + scissor；结构变更仍走全帧 Clear。细粒度按元素 damage 可继续打磨。

---

## 3. 完整差距清单

### 3.1 响应式内核

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
| P1 | 单线程 runtime，无后台线程桥 | 开放 | |
| P1 | 缺 Store（路径级订阅） | 开放 | |
| P2 | 缺 `on()` / `createSelector` / `createDeferred` | 开放 | |

### 3.2 样式引擎

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有变体引擎 | ✅ | `rui_style_tokens` + `ClassSet` + 每帧 resolve |
| ~~P1~~ | 编译期与运行时两套 utility 表不一致 | ✅ | 共享 [`rui_style_tokens`](../crates/rui_style_tokens)；宏展开为 `with_class_list` |
| ~~P1~~ | 无 dark mode / 运行时换肤 | ✅ | `set_theme_mode` / `toggle_theme_mode` + dark tokens |
| ~~P1~~ | Tailwind 色板不完整 | ✅ | 常见 family × 50–950；`try_parse_color` 可区分未知 |
| P2 | 缺断点、负值、任意值覆盖不全 | 开放 | `sm:` 等仍未知；任意值 `[…]` 已支持间距 |
| P2 | `transition-*` / `animate-*` 无类名入口 | 开放 | 动画 API 在 [`animation.rs`](../crates/rui/src/style/animation.rs)，无 utility 入口 |

### 3.3 渲染与布局

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 没有脏区重绘（实现） | ✅ MVP | [`damage.rs`](../crates/rui/src/app/damage.rs) + scissor；细粒度 damage 仍可打磨 |
| ~~P1~~ | 缺 opacity / translate | ✅ MVP | `opacity-*` / `translate-x/y-*` + 布局收集乘 opacity |
| P1 | 缺渐变 / 多层阴影 / rotate·scale | 开放 | translate 仅 MVP；完整 transform 矩阵未做 |
| P1 | 缺 `position: fixed` 与 `aspect-ratio` | 开放 | |
| P2 | 单窗口 | 开放 | |

### 3.4 事件与文本

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 无事件冒泡 / 捕获 / pointer capture | ✅ | [`pointer_events.rs`](../crates/rui/src/app/pointer_events.rs) |
| ~~P0~~ | 无右键、双击、系统文件拖放 | ✅ | [`window_events.rs`](../crates/rui/src/app/window_events.rs) + `on_file_drop` |
| ~~P1~~ | 静态文本不可选中 / 无 ellipsis | ✅ | Label 选区 + `truncate` / `TextOverflow::Ellipsis` |
| P1 | 无富文本 | 开放 | 仅 emoji 分段 |
| P1 | 无 RTL / bidi、无 letter-spacing | 开放 | |
| P2 | 无触摸 / 手势，滚动无系统惯性 | 开放 | |

### 3.5 无障碍

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | AccessKit 树扁平，且没有 Action | ✅ | 层级 `set_children` + Click/Focus Action |
| P2 | 无 live region、无 name/description 映射 | 开放 | |

### 3.6 工程化

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P0~~ | 无 CI / LICENSE | ✅ | `.github/workflows/ci.yml` + MIT |
| ~~P0~~ | 无离屏 golden 骨架 | ✅ | [`renderer/offscreen.rs`](../crates/rui/src/renderer/offscreen.rs) clear/readback |
| P1 | 完整场景 golden 未接入 CI | 开放 | 骨架可跑；缺组件级像素对比与 bless |
| P1 | 热路径大量 unwrap，release `panic=abort` | 开放 | |
| ~~P1~~ | clippy `-D warnings` / fmt 门禁未收紧 | ✅ | `fmt --check` + workspace `-D warnings` |
| P2 | 无 benchmark 基线、无结构化 tracing | 开放 | |

### 3.7 宏与 DX

| 等级 | 问题 | 状态 | 说明 |
|------|------|------|------|
| ~~P1~~ | `ui!` 报错 span 丢失 | ✅ | `err.to_compile_error()` |
| P1 | 原语未知属性静默忽略 | 开放 | `_ => {}` 兜底仍在 |
| ~~P1~~ | `.get()` 启发式误判 / 漏判 | ✅ | 收紧 receiver 链；零参 `.get()` + path/field |
| P2 | 缺 Fragment / keyof / ref / 动态标签 | 开放 | |
| ~~P2~~ | `#[component]` 不支持泛型，丢弃文档注释 | ✅ | 转发 generics + `///` / 参数 docs |

---

## 4. SolidJS 能力对齐

已对齐 **13** 项、部分对齐 **3** 项、完全缺失 **12** 项。（阶段三未改 Solid 表。）

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
| `createStore` / `produce` | — | 无 | |
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

## 5. 组件清单

阶段四后约 **已有 26 / 占位 3 / 缺失 18**。关键 P0 产品空洞（定位引擎、Tooltip、ContextMenu、Combobox、VirtualList、DataTable、TreeView、Form、Textarea、Toast、Resizable、FilePicker）均有 MVP。仍缺：AlertDialog、Sheet、Menubar、Command 面板、Chart、桌面外壳（托盘/多窗口）等；DatePicker/ColorPicker/Router 仍为占位。

---

## 6. 建议推进顺序

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

---

## 相关文档

- [PROJECT.md](PROJECT.md) — 愿景、分层架构与阶段计划
- [STAGE7_ROADMAP.md](STAGE7_ROADMAP.md) — Stage 7+ 长期项排期
- [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md) — 三档更新与热重载模板契约
- [headless.md](headless.md) — 无头组件模式
