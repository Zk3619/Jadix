# RUI × SortableJS 列表拖放对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) **WP-DRAG.\*** 为准；产品手册见 [drag.md](../components/drag.md)；solid-dnd 原语见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md)；interact 修饰见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)。  
> **对照源**：`Reference/Sortable-master`（[SortableJS](https://github.com/SortableJS/Sortable) **1.15.7** 语义）· RUI 以 `crates/rui/src/utils/drag/{sortable,session,edge_scroll}.rs` 及 Builder/RSX 为准。  
> **日期**：2026-09-10 初稿。  
> **范围**：列表重排 / 跨容器投放 / 边缘滚 / 让位预览 — SortableJS 血统能力；**不**重复 solid-dnd 原语专章与 interact 修饰专章。

**明细入口**：**[§0.1 对齐判定](#01-对齐判定何时可以宣称列表血统对齐)**、**[§4 能力矩阵](#4-能力矩阵)**、**[§5 行为差异](#5-行为差异)**、**[§7 官方插件](#7-官方插件对照)**、**[§9 刻意未做](#9-刻意未做与不建议做的项)**。

---

## 0. 一句话结论

RUI `utils/drag` **不是** SortableJS 的 DOM 移植，而是 **headless 列表/看板拖放套件**：吸收 Sortable 的 **列表语义**（group、handle、delay、empty-insert、`onMove` 否决、兄弟让位、边缘滚、clone-pull），落在自绘 layout slot + Pinned Overlay + solid-dnd 碰撞面上。相对 Sortable 1.15.7，**列表作者体验已对齐或超越**；**全库 parity**（Swap / MultiDrag / revertOnSpill / `forceFallback` / `store` / 原生 HTML5 DnD）**既不必要也与架构冲突**。

| 轴 | SortableJS 1.15.7 | RUI `utils/drag` |
|----|-------------------|------------------|
| **命题** | 浏览器 DOM 列表重排插件 | Headless 拖放 + Sortable **血统**列表语义 |
| **数据** | 拖完 **改 DOM**（`insertBefore`） | **不改 `Vec`**；`on_drag_end` + `apply_reorder_vec` / `apply_transfer` |
| **幽灵** | `ghostClass` + 可选 `forceFallback` 克隆节点 | Pinned `DragOverlay`（viewport inset，见 [overlay_shells.md](../components/overlay_shells.md) §9） |
| **插入索引** | `swapThreshold` / `invertSwap` + 中点启发 | `insert_index_along_axis`（中点 + 变高行 pitch） |
| **跨列表** | `group: { name, pull, put }` | `group` 字符串 + `accept` / `item_type` + `apply_transfer` |
| **边缘滚** | `plugins/AutoScroll`（`bubbleScroll`） | [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) + **按住仍泵** + `scroll_lock` + 可配 sensitivity/speed |
| **动画** | `animation` ms（默认 0） | 兄弟 FLIP [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms（可 `.animation(false)` 关） |
| **没吃** | Swap / MultiDrag / OnSpill / `store` / `forceFallback` | 见 §9；高价值列表路径已在 §4 ☑ |

**不该做的事**：为 parity 把 RUI 改回 DOM `transform` 跟手；拖放 tick 里焊 resize/gesture；拆掉 solid-dnd 碰撞面或 Overlay 换 Sortable 式 `ghostClass`。

### 0.1 对齐判定（何时可以宣称「列表血统对齐」）

「对齐」指：从 SortableJS **列表/Kanban** 场景迁到 RUI 时，重排、跨列、把手、延迟、空列表插入、移动否决、边缘滚、clone 拖出有对应 API，行为可预期。**不**宣称插件生态 parity。

| 判定项 | SortableJS | RUI 现在 | 对齐后 |
|--------|------------|----------|--------|
| 同列表重排 | `sort: true` + DOM 插入 | `apply_reorder_vec` + `sortable_shift` 预览 | ☑ |
| 跨列表投放 | `group` + `pull`/`put` | `group` + `apply_transfer` | ☑（语义简化，见 §5.3） |
| `pull: 'clone'` | `group.pull: 'clone'` | `.pull_clone` + `apply_clone_event` | ☑ |
| Handle | `handle: '.my-handle'` | `<DragHandle>` / `handle={true}` | ☑ |
| `delay` | `delay` ms | `delay_ms` + `threshold` + `Activation` | ☑ |
| `emptyInsertThreshold` | 默认 5px | `empty_insert`（默认 8） | ☑ |
| `onMove` 否决 | `onMove` 返回 `false` | `can_drop` / per-zone `drop_checker` | **☑ 超**（+ 四碰撞算法） |
| `direction` | `direction` 函数 / 轴 | Provider `axis` + `SortableList.direction` | ☑ |
| 边缘自动滚 | AutoScroll 插件 | `autoscroll` + `scroll_sensitivity` / `scroll_speed` | **☑ 超**（静止泵、`scroll_lock`） |
| 兄弟让位动画 | `animation` ms | FLIP 150ms / 可关 | ☑ |
| `disabled` | `disabled: true` | `disabled` attr / Builder | ☑ |
| `filter` | `filter` + `preventOnFilter` | `drag_ignore` / `DragIgnore` | ☑（不同 API） |
| Swap 模式 | Swap 插件 | — | ✗ §9 |
| MultiDrag | MultiDrag 插件 | — | ✗ §9 |
| spill 回滚/删除 | OnSpill 插件 | Esc `cancel`（不提交） | **部分**（无 DOM revert） |

---

## 1. 定位与体量

| 项 | SortableJS | RUI |
|----|------------|-----|
| **一句话** | Modern browsers drag-and-drop lists | Headless 拖放套件里的 **Sortable 血统**子集 |
| **版本** | 1.15.7（`package.json`） | 工作区 `utils/drag`；与 WP-DRAG.1–3 同期 |
| **源码规模** | `src/Sortable.js` ~2k 行 + 4 官方插件 | `sortable.rs` + `session` 插入/让位 + `edge_scroll.rs` |
| **依赖** | 零运行时依赖 | solid-dnd 会话 + interact 修饰（另专章） |
| **测试** | TestCafe 浏览器矩阵 | `sortable` / `session` / `edge_scroll` 单测 + Gallery `DragShowcase` |
| **北极星** | ~40k★ DOM 列表插件 | **超越 Sortable**（碰撞、Overlay、键盘、双写）；见 [drag.md](../components/drag.md) |

---

## 2. 架构对照

```text
SortableJS                           RUI utils/drag
  pointerdown on item                  pointerdown → begin (Pending)
  delay / touchStartThreshold          delay_ms + threshold + Activation
  drag start → ghost / fallback        Active → DragOverlay (Pinned portal)
  dragover → swapThreshold 插入         tick → collision → insert_index_along_axis
  sibling translate (animation ms)       sortable_shift + CSS translate FLIP
  AutoScroll plugin (DOM overflow)     edge_scroll on scroll: viewport signals
  drop → DOM insertBefore              end → on_drag_end (author mutates Vec)
```

| 层 | SortableJS | RUI |
|----|------------|-----|
| **命中** | 目标元素 `getBoundingClientRect` + `elementFromPoint` | Layout **slot** walk（`collect_hits`）；不含 CSS `translate` |
| **插入** | 指针在目标行上的位置 vs `swapThreshold` | 行 slot 中点 + 变高 `slot_extent`（§5.2） |
| **让位** | 改兄弟 `transform`；`animation` 控制时长 | `reactive_sync` + `translate`；[`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) |
| **跨列** | `putSortable` + `group.checkPut` | `hover_list` + `group`/`accept` 过滤 + `apply_transfer` |
| **滚** | 祖先链 `overflow` + `setInterval` 24ms | `ScrollHit` layout walk + `pump_autoscroll` / 静止泵 |

---

## 3. API 映射

| SortableJS | RUI | 备注 |
|------------|-----|------|
| `new Sortable(el, opts)` | `drag_scope(...).child(|| sortable_list / sortable_item)` | 无单元素构造器；Provider 作用域 |
| `group: 'shared'` | `.group("shared")` / RSX `group=` | RUI 为 **字符串相等**；无 `pull`/`put` 函数对象 |
| `group: { pull: 'clone' }` | `.pull_clone(true)` / RSX `pull_clone` | 仅 clone；无 `revertClone` |
| `handle: '.handle'` | `<DragHandle>` 包裹 / `handle={true}` | 项级把手 |
| `delay` | `delay_ms` | 与 `threshold` 组合见 `Activation` |
| `touchStartThreshold` | `threshold`（默认 4px） | 无 `delayOnTouchOnly` 分叉 |
| `animation` | `.animation(true/false)` / RSX `animation` | RUI 默认 **开** FLIP；Sortable 默认 **0** |
| `direction` | Provider `axis` + `SortableList.direction` | list 级覆盖 provider |
| `emptyInsertThreshold` | `.empty_insert(px)` / RSX `empty_insert` | 默认 8 vs Sortable 5 |
| `onMove` | `.can_drop(f)` / RSX `canDrop` | 在碰撞 pick **之后** |
| `onEnd` | `.on_drag_end` / `onDragEnd` | RUI 不自动改数组 |
| `onAdd` / `onRemove` / `onUpdate` | 合并在 `on_drag_end` + `DragEvent` 字段 | 无独立事件名 |
| `disabled` | `.disabled(true)` / RSX `disabled` | |
| `filter` | `drag_ignore` / `<DragIgnore>` | 子控件不启动拖 |
| `scroll` + AutoScroll | `.autoscroll` + `.scroll_sensitivity` / `.scroll_speed` | 见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) §7.6 |
| `swapThreshold` | （无同名选项） | 固定中点启发 + 测试覆盖的 slot 几何 |
| `invertSwap` | — | 无「反转插入侧」开关 |
| `sort: false` | — | 无「只接收不重排源列表」一等选项 |
| `store` | — | 无内置 localStorage 持久化 |
| `ghostClass` / `chosenClass` | `source_opacity` + Overlay 样式 | 样式在 author / `DragOverlay` |
| `forceFallback` | — | 始终 Overlay/跟手，非 DOM 克隆 fallback |

---

## 4. 能力矩阵

图例：**对齐** = Sortable 列表场景可预期替代；**超** = RUI 有意加强；**✗** = 未做且不建议做全库 parity。

| 能力 | SortableJS | RUI | 判定 |
|------|------------|-----|------|
| 同列表重排 | DOM 插入 | `apply_reorder_vec` + 预览让位 | 对齐 |
| 跨列表 | `group` + pull/put | `group` + `apply_transfer` | 对齐（API 简化） |
| clone-pull | `pull: 'clone'` | `pull_clone` + `apply_clone_event` | 对齐 |
| Handle | CSS 选择器 | `DragHandle` 组件 | 对齐 |
| 延迟启动 | `delay` | `delay_ms` + `Activation::{And,Or}` | **超**（Or 见 solid-dnd 专章） |
| 空列表插入 | `emptyInsertThreshold` | `empty_insert` | 对齐 |
| 移动否决 | `onMove` | `can_drop` + `drop_checker` | **超** |
| 水平列表 | `direction` | `axis` + `SortableList.direction` | 对齐 |
| 边缘自动滚 | AutoScroll 插件 | `edge_scroll` + 可配 + 静止泵 | **超** |
| 兄弟动画 | `animation` ms | FLIP 150ms / 可关 | 对齐 |
| 变高行距 | 同行高较稳 | `slot_extent` 按行 layout pitch | **超** |
| 嵌套列表 | 易误触父行 | innermost 碰撞 + `hover_list` 规则 | **超** |
| Esc 取消 | 无一等 | `cancel()` 不触发 `on_drag_end` | **超** |
| 方向键改插入点 | 无 | `nudge_insert` | **超** |
| 碰撞算法 | 几何 + `swapThreshold` | 四算法 + `collision_fn` + sticky | **超**（solid-dnd 血统） |
| Overlay 幽灵 | ghost / fallback 节点 | Pinned `DragOverlay` + `child_for` | **超** |
| interact 修饰 | 无 | axis/restrict/snap（另专章） | **超** |
| RSX 双写 | 各框架绑定 | Builder + RSX 同源 | **超** |
| Swap 插件 | 交换两节点 | — | ✗ |
| MultiDrag | 多选拖 | — | ✗ |
| revert/remove OnSpill | spill 插件 | — | ✗ |
| `store` 持久化 | 内置 | — | ✗ |
| `invertSwap` 调参 | 有 | — | ✗ |
| 原生 HTML5 DnD | 可选主路径 | 自绘 pointer 会话 | **不同命题** |

Gallery 已演示：Kanban `group` + `apply_transfer`、长列表 `ScrollArea` 边缘滚、混高文件夹、`SortableList.direction` 横竖并存、`pull_clone`、`animation` 开关、嵌套列表、空列 `empty_insert`。见 `examples/gallery/src/pages/drag.rs`。

---

## 5. 行为差异

### 5.1 坐标与幽灵

Sortable 跟 **DOM 盒 + CSS `transform`**（或 `forceFallback` 时把克隆挂到 `body`）。RUI 跟 **pointer 空间 layout slot**（scroll 已计入，不含兄弟让位 `translate`），幽灵为 **viewport Pinned** inset（[`overlay_pos`](../../crates/rui/src/utils/drag/session.rs)）。因此 RUI 可拖出父级进另一列而不被 overflow 裁切；限制应用 `restrict` / `axis`，而非 `overflow: hidden`。

### 5.2 插入索引 vs `swapThreshold`

Sortable 用目标行矩形上的指针位置与 `swapThreshold`（0–1）及 `invertSwap` 决定插在行前/行后。RUI [`insert_index_along_axis`](../../crates/rui/src/utils/drag/session.rs) 用 **行 slot 原点链 + 尾行 pitch** 的中点规则，并处理 **不等高行**（`slot_extent_along_axis`）。无作者-facing `swapThreshold` 旋钮；行为由 layout 实测 slot 决定，库测覆盖 append / 聚簇 slot / 拉伸列等。

### 5.3 `group` / `pull` / `put`

Sortable `group` 是对象：`name`、`pull`、`put` 可为 `true` / `false` / `'clone'` / 函数 / 组名数组。RUI：

- `group`：**字符串**，双方非空且不等则拒绝；
- `accept`：**item_type** 过滤（逗号分隔；空 accept + 双方有 type 时默认同 type）；
- `pull_clone`：对应 `pull: 'clone'`，无 `revertClone`；
- 无 `put: false` 的「只出不进」细粒度；可用 `can_drop` 模拟。

看板作者：源列/目标列设同一 `group`；类型用 `item_type` + `accept` 表达 Sortable 里靠不同组名实现的约束。

### 5.4 数据提交

Sortable **drop 时改 DOM**，`onEnd` 里可读 `oldIndex` / `newIndex`。RUI **从不**自动 `Vec::remove`/`insert`；`DragEvent` 带 `from_list` / `to_list` / `from_index` / `to_index`，作者在 `on_drag_end` 调 [`apply_reorder_vec`](../../crates/rui/src/utils/drag_reorder.rs) / [`apply_transfer`](../../crates/rui/src/utils/drag/sortable.rs) / [`apply_event`](../../crates/rui/src/utils/drag/sortable.rs)。与 solid-dnd 契约一致（[SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §2.2）。

### 5.5 边缘自动滚

详见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) **§7.6** 三方表。相对 Sortable `AutoScroll`：RUI 默认 **更宽** sensitivity（40 vs 30）、按 **显示帧 dt** 积分（非固定 24ms interval）、**按住不动仍泵**、[`ScrollLock`](../../crates/rui/src/utils/edge_scroll.rs) 可冻结轴；滚动目标是 RUI `scroll:` 视口 signal，非 DOM `overflow` 嗅探。resize/gesture 可选边缘滚见 `pointer_action`（同 `edge_scroll`）。

### 5.6 动画默认值

Sortable `animation` 默认 **0**（即时）。RUI Provider `animation` 默认 **true**（[`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms ease）。`animation={false}` / `.animation(false)` 与 Sortable `animation: 0` 同类；`end` / `cancel` 仍 **snap** 掉残留 translate，避免提交布局反转。

---

## 6. 已吃进的路径（相对 Sortable 列表核心）

| 项 | 落地 | 源文件 |
|----|------|--------|
| 兄弟让位 `sortable_shift` | ☑ | [`sortable.rs`](../../crates/rui/src/utils/drag/sortable.rs) |
| 跨列表 `sortable_preview` / `apply_transfer` | ☑ | `sortable.rs` |
| clone-pull | ☑ | `apply_clone_event` + `pull_clone` |
| `SortableList` / `SortableItem` | ☑ | `sortable.rs` + Builder |
| `insert_index_along_axis` | ☑ | [`session.rs`](../../crates/rui/src/utils/drag/session.rs) |
| `empty_insert` | ☑ | `DragScopeBuilder` |
| `can_drop` ← `onMove` | ☑ | `session::tick` |
| `delay_ms` + handle | ☑ | `session` + `DragHandle` |
| `SortableList.direction` | ☑ 2026-09-10 | `sortable_list` + `rui_drag_axis` |
| 边缘滚（Sortable 血统） | ☑ | [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) |
| FLIP `animation` 开关 | ☑ | Provider `.animation` |
| Gallery 列表/Kanban 段 | ☑ | `examples/gallery/src/pages/drag.rs` |

---

## 7. 官方插件对照

Sortable 1.15.7 内置插件目录：`plugins/AutoScroll`、`Swap`、`MultiDrag`、`OnSpill`（`revertOnSpill` / `removeOnSpill`）。

| 插件 | Sortable 行为 | RUI | 判定 |
|------|---------------|-----|------|
| **AutoScroll** | `scroll`/`bubbleScroll`/`scrollSensitivity`/`scrollSpeed`；pointermove + 24ms fallback | `edge_scroll` + `autoscroll` 配置 + 静止泵 | **对齐 + 超**（§5.5） |
| **Swap** | 拖过有效目标时 **交换**两节点 DOM，非插入 | 仅插入语义 | ✗ 未做 |
| **MultiDrag** | Shift/修饰键多选；一次拖多项 | 单指单拖；`PointerSet` 配额给 resize/gesture | ✗ 未做 |
| **OnSpill** | 投到列表外 → revert 或 remove DOM | `cancel()` 放弃提交；无自动 DOM 回滚 | **部分** |

**结论**：列表拖放只需 **AutoScroll 血统**（已吃）。Swap / MultiDrag / OnSpill 是 Sortable **插件生态**，不是 WP-DRAG 核心；若未来要做「交换两项」用 `on_drag_end` + 作者交换 `Vec` 即可，不必复刻 Swap 插件 DOM 交换。

---

## 8. RUI 超出 Sortable 的列表相关能力

这些来自 solid-dnd / interact / 自绘栈，Sortable 本体没有：

| 能力 | 说明 |
|------|------|
| 四碰撞算法 + `collision_fn` | `MostIntersecting` / `ClosestCenter` / `ClosestCorners` / `PointerWithin` |
| Pinned Overlay + `child_for` / `align` | 视口钉住幽灵；按 `item_type` 分支 |
| `source_opacity` / 跨列洞 translate | 让位空洞 + 半透明源跟目标洞 |
| Esc `cancel` | 不触发 `on_drag_end` |
| `nudge_insert` | 键盘改插入索引 |
| 嵌套列表 innermost | 文件夹/文件列不误 hover |
| `debug_hits` | 槽矩形调试 |
| `restrict` / `snap` / `axis` | interact 修饰（[INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)） |
| 变高行 pitch / 跨列 folder 伸缩 | layout 级 FLIP，非统一 `offsetHeight` |

---

## 9. 刻意未做与不建议做的项

| Sortable 能力 | 原因 |
|---------------|------|
| **全库 parity** | 命题是 headless 套件 + 自绘坐标，不是 DOM 插件 |
| **Swap 插件** | 插入模型已覆盖 95% 列表 UX；交换可由作者在 `on_drag_end` 实现 |
| **MultiDrag** | 无 DOM 多选节点；需产品级多选 + 会话模型，非 Sortable 单插件移植 |
| **revertOnSpill / removeOnSpill** | 无 DOM  spill；用 `cancel` + 作者不提交代替 |
| **`store`** | 持久化属于应用层，非拖放内核 |
| **`forceFallback` / `fallbackOnBody`** | 无 HTML5 DnD fallback 路径 |
| **`invertSwap` / `swapThreshold` 暴露** | 用 layout slot 中点 + 测试锁语义；避免双轨调参 |
| **`sort: false` 一等** | 可用「只 droppable 不 draggable」或 `can_drop` 组合；未单独命名 |
| **`group.pull`/`put` 函数** | `group` + `accept` + `can_drop` 覆盖看板；避免 HRTB 式 group 回调 |

**值得单独开项（非 parity）**：Kanban 列「只进不出」→ `can_drop`；交换两卡 → `on_drag_end` 交换索引；长列表性能 → virtual list + drag 已有 slot 几何。

---

## 10. 文件地图

| RUI | 职责 |
|-----|------|
| [`sortable.rs`](../../crates/rui/src/utils/drag/sortable.rs) | `sortable_shift` / `sortable_preview` / `apply_*` / `SortableList` / `SortableItem` |
| [`session.rs`](../../crates/rui/src/utils/drag/session.rs) | `tick` / `insert_index_along_axis` / `hover_list` / `cancel` / `nudge_insert` |
| [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) | 边缘滚（拖放 + 可选 resize/gesture） |
| [`drag_reorder.rs`](../../crates/rui/src/utils/drag_reorder.rs) | `apply_reorder_vec` |
| [`builder.rs`](../../crates/rui/src/utils/drag/builder.rs) | `drag_scope` / `sortable_*` / `empty_insert` / `animation` / `autoscroll` |
| [`drag.md`](../components/drag.md) | 产品手册 |
| [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) | 原语 / Overlay（本专章不重复） |
| [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) | autoScroll 三方表 §7.6 |

---

## 11. 总表（给排期用）

| | |
|--|--|
| **已对齐的 Sortable 列表核心** | 重排、跨列、group、handle、delay、empty-insert、onMove→`can_drop`、direction、边缘滚、让位动画、disabled、filter→ignore、clone-pull |
| **RUI 超出 Sortable** | 碰撞四算法、Overlay、Esc/方向键、嵌套 innermost、变高 pitch、静止泵边缘滚、scroll 可配、interact 修饰、RSX 双写、库测 |
| **相对缺失（挡「列表血统对齐」）** | 无 |
| **Sortable 插件未做** | Swap、MultiDrag、OnSpill、store（**不建议** WP） |
| **全库 parity** | **不建议**；见 §9 |

一句话收束：**SortableJS 对 RUI 有价值的部分（列表语义、边缘滚、让位、group/clone）已对齐或超越；DOM 插件生态与 swap/multi/spill 不必移植；数据提交保持 headless，与 solid-dnd 一致。**
