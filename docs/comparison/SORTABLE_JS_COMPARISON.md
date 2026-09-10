# RUI × SortableJS 列表拖放对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) **WP-DRAG.\*** 为准；产品手册见 [drag.md](../components/drag.md)；solid-dnd 原语见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md)；interact 修饰见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)。  
> **对照源**：`reference/Sortable-master`（[SortableJS](https://github.com/SortableJS/Sortable) **1.15.7** 语义；`package.json`）· RUI 以 `crates/rui/src/utils/drag/{sortable,session}.rs` + [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) 及 Builder/RSX 为准。  
> **日期**：2026-09-10 初稿 · **同日** 对 `src/Sortable.js` defaults / `_prepareGroup` / AutoScroll / 四插件钉死并冻结。  
> **冻结**：**2026-09-10** — §0.1 列表血统判定全部 ☑；WP-DRAG.1–3 ☑。列表核心不再扩 scope。**同日** Swap / MultiDrag 按产品加深落地（WP-DRAG.4）；OnSpill 仍未做。`forceFallback` / `store` 是架构永不。  
> **范围**：列表重排 / 跨容器投放 / 边缘滚 / 让位预览 — SortableJS 血统能力；**不**重复 solid-dnd 原语专章与 interact 修饰专章。

**明细入口**：**[§0.1 对齐判定](#01-对齐判定何时可以宣称列表血统对齐)**、**[§3.1 默认值](#31-默认值源码钉死)**、**[§4 能力矩阵](#4-能力矩阵)**、**[§5 行为差异](#5-行为差异)**、**[§7 官方插件](#7-官方插件对照)**、**[§9 分类](#9-未做项的真实原因三项产品--两项架构)**、**[§10 文件地图](#10-文件地图)**。

---

## 0. 一句话结论

RUI `utils/drag` **不是** SortableJS 的 DOM 移植，而是 **headless 列表/看板拖放套件**：吸收 Sortable 的 **列表语义**（group、handle、delay、empty-insert、`onMove` 否决、兄弟让位、边缘滚、clone-pull），落在自绘 layout slot + Pinned Overlay + solid-dnd 碰撞面上。相对 Sortable 1.15.7，**列表作者体验已对齐或超越**。五项常被误写成「全都不移植」：真正与架构冲突的只有 **原生 HTML5 DnD / `forceFallback`** 和 **`store` 持久化**；Swap / MultiDrag 已按产品落地；OnSpill 见 [§9](#9-未做项的真实原因三项产品--两项架构)。

| 轴 | SortableJS 1.15.7 | RUI `utils/drag` |
|----|-------------------|------------------|
| **命题** | 浏览器 DOM 列表重排插件 | Headless 拖放 + Sortable **血统**列表语义 |
| **数据** | 拖完 **改 DOM**（`insertBefore`） | **不改 `Vec`**；`on_drag_end` + `apply_reorder_vec` / `apply_transfer` |
| **幽灵** | `ghostClass` + 可选 `forceFallback` 克隆节点 | Pinned `DragOverlay`（viewport inset，见 [overlay_shells.md](../components/overlay_shells.md) §9） |
| **插入索引** | `swapThreshold` / `invertSwap` + 中点启发 | `insert_index_along_axis`（中点 + 变高行 pitch） |
| **跨列表** | `group: { name, pull, put }` | `group` 字符串 + `accept` / `item_type` + `apply_transfer` |
| **边缘滚** | `plugins/AutoScroll`（`bubbleScroll`） | [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) + **按住仍泵** + `scroll_lock` + 可配 sensitivity/speed |
| **动画** | `animation` ms（**源码默认 0**；README 示例写 150） | 兄弟 FLIP [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms（可 `.animation(false)` 关） |
| **没吃（分类）** | OnSpill / `store` / `forceFallback` | Swap / MultiDrag ☑；其余见 §9 |

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
| Swap 模式 | Swap 插件 | `SortMode::Swap` + `apply_swap_event` | ☑ |
| MultiDrag | MultiDrag 插件 | `multi_drag` + 选择集 + Overlay 叠卡 | ☑（v1：单击选，无 Shift） |
| spill 回滚/删除 | OnSpill 插件 | 近邻吸附 + 始终 `on_drag_end`；Esc ≠ spill | ✗ 产品 §9.2 |

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
| `touchStartThreshold` | `threshold`（默认 4px） | Sortable **源码**默认 `devicePixelRatio`（原生 DnD 时强制 1）；README 写 0。RUI 4px 更接近 solid-dnd 距离门槛，无 `delayOnTouchOnly` 分叉 |
| `animation` | `.animation(true/false)` / RSX `animation` | Sortable **源码默认 0**（`Sortable.js` 381）；README 示例 150。RUI 默认 **开** 150ms，对齐 README 示例而非源码 0 |
| `easing` | FLIP `ease` | 时长固定 [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs)；无 `cubic-bezier` 旋钮 |
| `direction` | Provider `axis` + `SortableList.direction` | list 级覆盖 provider；**无** CSS flex/grid 自动探测（§5.9） |
| `emptyInsertThreshold` | `.empty_insert(px)` / RSX `empty_insert` | 默认 8 vs Sortable 5（`Sortable.js` 399） |
| `onMove` | `.can_drop(f)` / RSX `canDrop` | 碰撞 pick **之后**；`false` 否决。Sortable 还可返回 `-1`/`1` 强制插前/插后（§5.8） |
| `onChoose` / `onUnchoose` | `begin` → Pending；无独立 unchoose | 未达门槛则会话丢弃；无 `chosenClass` |
| `onEnd` | `.on_drag_end` / `onDragEnd` | RUI 不自动改数组 |
| `onAdd` / `onRemove` / `onUpdate` / `onSort` | 合并在 `on_drag_end` + `DragEvent` 字段 | 无独立事件名（§5.7） |
| `onChange` | `hover_index` / `hover_list` signal | 让位预览每 tick；不改 Vec |
| `onClone` | `pull_clone` + Overlay 克隆 | 无 DOM clone 节点；提交走 `apply_clone_event` |
| `toArray` / `sort` / `save` | 作者 `Vec` | 无 `store` / `dataIdAttr` |
| `disabled` | `.disabled(true)` / RSX `disabled` | |
| `filter` | `drag_ignore` / `<DragIgnore>` | 子控件不启动拖 |
| `scroll` + AutoScroll | `.autoscroll` + `.scroll_sensitivity` / `.scroll_speed` | 见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) §7.6 |
| `swapThreshold` | （无同名选项） | 固定中点启发 + 测试覆盖的 slot 几何 |
| `invertSwap` | — | 无「反转插入侧」开关 |
| `sort: false` | — | 无「只接收不重排源列表」一等选项 |
| `store` | — | 无内置 localStorage 持久化 |
| `ghostClass` / `chosenClass` | `source_opacity` + Overlay 样式 | 样式在 author / `DragOverlay` |
| `forceFallback` | — | 始终 Overlay/跟手，非 DOM 克隆 fallback |

### 3.1 默认值（源码钉死）

README Options 块是**示例**，与 `src/Sortable.js` 构造器 `defaults`（361–400）不完全一致。对照以源码为准。

| 选项 | Sortable 源码 | README 示例 | RUI |
|------|---------------|-------------|-----|
| `animation` | **0** | 150 | **开** 150ms（`SORTABLE_FLIP_MS`）；`.animation(false)` = 源码 0 |
| `delay` | 0 | 0 | `delay_ms` 0 |
| `emptyInsertThreshold` | **5** | 5 | `empty_insert` **8** |
| `swapThreshold` | 1（整行可换） | 1 | 无旋钮；行 slot 中点 |
| `invertSwap` | false | false | 无 |
| `scroll` / `bubbleScroll` | true / true（AutoScroll 插件） | — | `autoscroll` 默认 true |
| `scrollSensitivity` | **30** | — | **40** |
| `scrollSpeed` | **10** px / **24ms** interval | — | **14** px / 显示帧（~16.7ms；按 `dt` 积分） |
| `touchStartThreshold` | `devicePixelRatio`（原生 DnD → 1） | 0 | `threshold` **4** |
| `group` | `null` | `"name"` | 空字符串 = 不约束组 |
| `sort` | true | true | 无同名；列表默认可重排 |
| `direction` | **函数** `_detectDirection`（flex/grid/float） | `'horizontal'` | 显式 `axis` / `direction`；省略则垂直插入（`Axis::sort_axis`） |

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
| Swap 插件 | 交换两节点 | `SortMode::Swap` live + `apply_swap_event` | ☑ |
| MultiDrag | 多选拖 | `multi_drag` + `DragEvent.items` + Overlay 叠卡 | ☑ |
| revert/remove OnSpill | spill 插件 | 近邻列表吸附；Esc ≠ spill | ✗ 产品（§9.2） |
| `store` 持久化 | 内置 | 作者持有 `Vec` | ✗ 架构（§9.1） |
| `invertSwap` 调参 | 有 | — | ✗ |
| 原生 HTML5 DnD | 可选主路径 | 自绘 pointer 会话 | **不同命题** |

Gallery 已演示：Kanban `group` + `apply_transfer`、长列表 `ScrollArea` 边缘滚、混高文件夹、`SortableList.direction` 横竖并存、`pull_clone`、`animation` 开关、嵌套列表、空列 `empty_insert`、**Swap / MultiDrag（§12）**。见 `examples/gallery/src/pages/drag.rs`。

---

## 5. 行为差异

### 5.1 坐标与幽灵

Sortable 跟 **DOM 盒 + CSS `transform`**（或 `forceFallback` 时把克隆挂到 `body`）。RUI 跟 **pointer 空间 layout slot**（scroll 已计入，不含兄弟让位 `translate`），幽灵为 **viewport Pinned** inset（[`overlay_pos`](../../crates/rui/src/utils/drag/session.rs)）。因此 RUI 可拖出父级进另一列而不被 overflow 裁切；限制应用 `restrict` / `axis`，而非 `overflow: hidden`。

### 5.2 插入索引 vs `swapThreshold`

Sortable `_getSwapDirection`（`Sortable.js` 1832–1901）用目标行矩形上的指针位置与 `swapThreshold`（默认 **1** = 整行都是交换区）及 `invertSwap` 决定插在行前/行后。RUI [`insert_index_along_axis`](../../crates/rui/src/utils/drag/session.rs) 用 **行 slot 原点链 + 尾行 pitch** 的中点规则，并处理 **不等高行**（`slot_extent_along_axis`）。无作者-facing `swapThreshold` 旋钮；行为由 layout 实测 slot 决定，库测覆盖 append / 聚簇 slot / 拉伸列等。

空列表：Sortable `_detectNearestEmptySortable`（230–245）用 `emptyInsertThreshold` **向外扩** 空根的 `getRect`。RUI `empty_insert` 膨胀空容器 hit box（默认 8 vs 5）。

### 5.3 `group` / `pull` / `put`

Sortable `_prepareGroup`（247–287）把 `group` 收成对象：`name`、`checkPull` / `checkPut` 可为 `true` / `false` / `'clone'` / 函数 / 组名数组；`revertClone` 把克隆动画回原位。RUI：

- `group`：**字符串**，双方非空且不等则拒绝；
- `accept`：**item_type** 过滤（逗号分隔；空 accept + 双方有 type 时默认同 type）；
- `pull_clone`：对应 `pull: 'clone'`，无 `revertClone`（幽灵是 Overlay 克隆，源项不卸 DOM）；
- 无 `put: false` 的「只出不进」细粒度；可用 `can_drop` 模拟。

看板作者：源列/目标列设同一 `group`；类型用 `item_type` + `accept` 表达 Sortable 里靠不同组名实现的约束。

### 5.4 数据提交

Sortable **drop 时改 DOM**，`onEnd` 里可读 `oldIndex` / `newIndex`。RUI **从不**自动 `Vec::remove`/`insert`；`DragEvent` 带 `from_list` / `to_list` / `from_index` / `to_index`，作者在 `on_drag_end` 调 [`apply_reorder_vec`](../../crates/rui/src/utils/drag_reorder.rs) / [`apply_transfer`](../../crates/rui/src/utils/drag/sortable.rs) / [`apply_event`](../../crates/rui/src/utils/drag/sortable.rs)。与 solid-dnd 契约一致（[SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §2.2）。

### 5.5 边缘自动滚

详见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) **§7.6** 三方表。Sortable `plugins/AutoScroll`（`pluginName: 'scroll'`，`initializeByDefault: true`）：`scrollSensitivity` 30、`scrollSpeed` 10、`bubbleScroll` 从 `elementFromPoint` 向上走 DOM overflow；`setInterval` **24ms** 泵，fallback 间隔内重发 `_onTouchMove` 更新 ghost。

相对它，RUI：默认 **更宽** sensitivity（40 vs 30）、按 **显示帧 dt** 积分（非固定 24ms）、**按住不动仍泵**（`pump_autoscroll`）、[`ScrollLock`](../../crates/rui/src/utils/edge_scroll.rs) 可冻结轴；滚动目标是 RUI `scroll:` 视口 signal，非 DOM `overflow` 嗅探。实现在 [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs)（`utils/drag/autoscroll.rs` 只是 re-export）。resize/gesture 可选边缘滚见 `pointer_action`。

### 5.6 动画默认值

Sortable **源码** `animation: 0`（即时）；README Options 示例写 150。`Animation.js` 在 `options.animation` 为假时直接 `callback()` 不补间。RUI Provider `animation` 默认 **true**（[`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms ease）——对齐 README 示例，**不是**源码默认。`animation={false}` / `.animation(false)` 与 Sortable `animation: 0` 同类；`end` / `cancel` 仍 **snap** 掉残留 translate，避免提交布局反转。无 `easing` 旋钮。

### 5.7 事件面

Sortable `EventDispatcher` 按 `on` + 首字母大写派发：`onChoose` / `onUnchoose` / `onStart` / `onEnd` / `onAdd` / `onRemove` / `onUpdate` / `onSort` / `onFilter` / `onClone` / `onChange`；`onMove` 走独立 `CustomEvent('move')`。RUI 四钩子：`on_drag_start` / `on_drag_move` / `on_drag_over` / `on_drag_end`。跨列 add/remove/update 由作者读 `DragEvent.from_list` ≠ `to_list` 自行分支。Esc `cancel` **不**触发 `on_drag_end`（Sortable drop 总会 `_onDrop`）。

### 5.8 `onMove` 返回值

Sortable `onMove`（README + `Sortable.js` 1769–1798）：`false` 取消；`-1` 插到 target 前；`1` 插到 target 后；`true` / `void` 保持默认。RUI `can_drop` **只**布尔否决（再加 per-zone `drop_checker`）。插入侧由 layout 中点决定，不暴露「强制插前/插后」返回码——避免与 `swapThreshold`/`invertSwap` 双轨。若作者要钉某一侧，在 `on_drag_end` 改 `to_index`。

### 5.9 `direction` 自动探测

Sortable 默认 `direction` 是函数：`_detectDirection`（163–207）看 flex / grid / float / 子宽。RUI **不**嗅探 CSS：Provider `axis` + `SortableList.direction` 显式覆盖；省略时 `Axis::sort_axis` 把 `Xy`/`Start` 当垂直列表。同一 Provider 可混 `direction=y` 列与 `direction=x` 行（Gallery「SortableList.direction」）。

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
| **AutoScroll** | `scroll`/`bubbleScroll`/`scrollSensitivity=30`/`scrollSpeed=10`；pointermove + **24ms** `setInterval`；fallback 重发 `_onTouchMove` | `edge_scroll` + `autoscroll` 配置 + 静止泵 | **对齐 + 超**（§5.5） |
| **Swap** | `dragOverValid` 高亮 `swapClass`；drop 时 `swapNodes` 交换两节点 DOM | `sort_mode="swap"`：停 FLIP、`rui_drag_swap` 对端、`apply_swap_event` | **☑** |
| **MultiDrag** | `multiDragKey` + Shift 多选；`selectedClass`；一次拖多项折叠 | 单击选中 + 拖已选项冻结集；Overlay 叠卡；`apply_multi_event`。v1 无 Shift / `multiDragKey` | **☑** |
| **OnSpill** | `elementFromPoint` 不在列表内 → `revertOnSpill` 插回 `startIndex` 或 `removeOnSpill` 删节点 | `cancel()` 放弃提交；无自动回滚/删除 | **部分** |

**结论**：列表拖放主路径只需 **AutoScroll 血统**（已吃）。Swap / MultiDrag 已按 headless 产品落地（WP-DRAG.4）；OnSpill 仍用 Droppable，不移植 `elementFromPoint`。见 §9。

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

## 9. 未做项的真实原因（三项产品 · 两项架构）

上一轮把 Swap / MultiDrag / OnSpill / `store` / `forceFallback` 捆成一句「不移植」。那是错的：**只有后两项与 RUI 命题冲突**；前三项是 Sortable 插件形态的产品。Swap / MultiDrag 已按 headless 落地（WP-DRAG.4）；OnSpill 仍未做。

### 9.1 架构永不（进套件会拆命题）

| Sortable | 对方在干什么 | 为什么进 `utils/drag` 会拆命题 |
|---------|--------------|--------------------------------|
| **`forceFallback` / `fallbackOnBody` / 原生 HTML5 DnD** | 默认走浏览器 `draggable` + `dragover`；`forceFallback: true` 才克隆节点挂 `body`（为旧浏览器 / 触屏手感一致） | RUI **没有** HTML5 DnD 主路径。Pinned Overlay **就是**唯一跟手，相当于 Sortable 的 fallback 已是默认。再加一条 native DnD，等于第二套坐标、第二套幽灵。 |
| **`store` + `toArray` / `sort` / `save`** | 插件改完 DOM 后把 `data-id` 数组写入 localStorage | 套件 **从不**持有 `Vec`。持久化是应用层（signal / 文件 / 自己的 store）。给 kit 加 `store` 会逼它改数据，与 solid-dnd 契约相反。 |

这两条继续标 ✗。不是「以后再做」，是做了会变成另一个库。

### 9.2 产品未做（可以做，不是 DOM 禁区）

| Sortable 插件 | 对方 UX | RUI 现在实际有什么 | 为何没进 WP-DRAG | 若要做该长什么样 |
|---------------|---------|---------------------|------------------|-----------------|
| **Swap** | `swap: true`：拖过时 **不让位**，只高亮对端 `swapClass`；松手 `swapNodes` 交换两项 | **`SortMode::Swap`**：hover 钉对端 index、关掉 shift、`rui_drag_swap`、`apply_swap_event`（同列 `Vec::swap` / 跨列 `mem::swap`） | 插入覆盖 Kanban 主路径；Swap 是另一种预览 | 已落地（Gallery §12）。**不是** DOM `swapNodes` 插件移植。 |
| **OnSpill** | 松手时 `elementFromPoint` 不在任何 Sortable 内 → 插回原位或从 DOM 删除 | **Esc `cancel` 不是 spill**（有意放弃，不触发 `on_drag_end`）。松手始终 `commit_end` + `on_drag_end`。`hover_list` 在列表外会 **回退到最近槽 / `from_list`**（solid-dnd `closestCenter`），所以「丢到空白」常常仍像同列重排 | 近邻吸附是有意的（避免幽灵洞）。垃圾桶 / 撤销 miss 用 **Droppable**（`accept` + 作者在 `on_drop` 删）比「丢到虚空」更可发现 | 若要 miss=撤销：在 `on_drag_end` 看 `to_list`/`droppable_id` 为 `None` 则不 `apply_*`。不要移植 `elementFromPoint` spill。 |
| **MultiDrag** | 单击/修饰键多选 + `selectedClass`；一次拖一组，放下按原相对序展开 | **`multi_drag`**：`DragCtx.selected` 单击 toggle；拖已选项冻结 `DragEvent.items`；Overlay 叠卡；`apply_multi_event` | 选择集在 kit 内（named signal），提交仍 headless | 已落地（Gallery §12）。v1 无 Shift 范围选 / `multiDragKey`。`PointerSet` 仍是 resize/双指，**不是**多选拖。 |

旧文档把 MultiDrag 和 `PointerSet` 写在一起，容易读成「多指所以不能多选」。**两件事**：多指是 interact 动作；多选拖是列表产品。

### 9.3 其余「未暴露」——仍不是欠账

| Sortable | 原因 |
|----------|------|
| **`invertSwap` / `swapThreshold` 旋钮** | 用 layout slot 中点 + 测试锁语义；避免双轨调参 |
| **`sort: false` 一等** | 「只 droppable 不 draggable」或 `can_drop` |
| **`group.pull`/`put` 函数** | `group` + `accept` + `can_drop` 覆盖看板 |
| **`delayOnTouchOnly`** | `delay_ms` 对所有指针生效 |
| **`direction` 函数自动探测** | 显式 `axis` / `SortableList.direction` |
| **`onMove` 返回 `-1`/`1`** | 插入侧由 slot 中点决定 |

**值得单独开项（非 Sortable parity）**：Kanban「只进不出」→ 已有 `can_drop`；OnSpill miss 仍用 Droppable，不移植 `elementFromPoint`。

---

## 10. 文件地图（主张 → 源）

### SortableJS（`reference/Sortable-master`，1.15.7）

| 文件 | 职责 |
|------|------|
| `package.json` | 版本 **1.15.7** |
| `README.md` Options | 示例块（`animation: 150` 等）；**以源码 defaults 为准**（§3.1） |
| `src/Sortable.js` 361–400 | 构造器 `defaults`：`animation: 0`、`emptyInsertThreshold: 5`、`swapThreshold: 1`、`delay: 0` |
| `src/Sortable.js` 163–207 | `_detectDirection`（flex / grid / float） |
| `src/Sortable.js` 230–245 | `_detectNearestEmptySortable` |
| `src/Sortable.js` 247–287 | `_prepareGroup` → `checkPull` / `checkPut` / `revertClone` |
| `src/Sortable.js` 1769–1798 | `onMove` 事件 + 返回值 |
| `src/Sortable.js` 1832–1914 | `_getSwapDirection` / `_getInsertDirection` |
| `src/Animation.js` | `captureAnimationState` / `animateAll`；`animation` 为假则跳过 |
| `src/EventDispatcher.js` | `onStart` / `onEnd` / `onAdd`…；`pullMode` |
| `plugins/AutoScroll/AutoScroll.js` | `pluginName: 'scroll'`；sensitivity 30 / speed 10 / 24ms interval / `bubbleScroll` |
| `plugins/Swap/Swap.js` | `swapClass` + `swapNodes` |
| `plugins/MultiDrag/MultiDrag.js` | `multiDragKey` / `selectedClass` |
| `plugins/OnSpill/OnSpill.js` | `revertOnSpill` / `removeOnSpill` |

### RUI

| 文件 | 职责 |
|------|------|
| [`sortable.rs`](../../crates/rui/src/utils/drag/sortable.rs) | `sortable_shift` / `sortable_preview` / `apply_*` / `SortableList` / `SortableItem` / `SORTABLE_FLIP_MS` |
| [`session.rs`](../../crates/rui/src/utils/drag/session.rs) | `tick` / `insert_index_along_axis` / `hover_list` / `cancel` / `nudge_insert` / `pump_autoscroll` |
| [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) | 边缘滚（拖放 + 可选 resize/gesture）；`drag/autoscroll.rs` 为 re-export |
| [`drag_reorder.rs`](../../crates/rui/src/utils/drag_reorder.rs) | `apply_reorder_vec` |
| [`builder.rs`](../../crates/rui/src/utils/drag/builder.rs) | `drag_scope` / `sortable_*` / `empty_insert` / `animation` / `autoscroll` / `can_drop` / `pull_clone` |
| [`geometry.rs`](../../crates/rui/src/utils/drag/geometry.rs) | `Axis::sort_axis`（`Xy`/`Start` → 垂直插入） |
| `examples/gallery/src/pages/drag.rs` | Kanban / 长列表 / 横竖 `direction` / `pull_clone` / 嵌套 / 空列 |
| [`drag.md`](../components/drag.md) | 产品手册 §5 / §6 |
| [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) | 原语 / Overlay（本专章不重复） |
| [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md) | autoScroll 三方表 §7.6 |

---

## 11. 总表（给排期用）

| | |
|--|--|
| **已对齐的 Sortable 列表核心** | 重排、跨列、group、handle、delay、empty-insert、onMove→`can_drop`、direction、边缘滚、让位动画、disabled、filter→ignore、clone-pull |
| **RUI 超出 Sortable** | 碰撞四算法、Overlay、Esc/方向键、嵌套 innermost、变高 pitch、静止泵边缘滚、scroll 可配、interact 修饰、RSX 双写、库测 |
| **相对缺失（挡「列表血统对齐」）** | 无 |
| **架构永不** | `forceFallback` / 原生 HTML5 DnD、`store`（§9.1） |
| **产品未做（非禁区）** | OnSpill miss（§9.2） |
| **产品已做（非列表核心冻结）** | Swap live（`SortMode::Swap`）、MultiDrag 选择集（§9.2 / WP-DRAG.4） |
| **全库 parity** | **不建议** |
| **solid-dnd / interact 血统** | [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md)（**已冻结**）· [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)（**已冻结**） |
| **冻结** | §0.1 ☑ · WP-DRAG.1–3 ☑ · **2026-09-10** 列表核心；Swap / MultiDrag **同日产品加深**（不重开列表核心 scope） |

一句话收束：**SortableJS 对 RUI 有价值的部分（列表语义、边缘滚、让位、group/clone）已对齐或超越。`forceFallback` 与 `store` 进套件会拆命题；Swap / MultiDrag 已按 headless 产品落地；OnSpill 仍用 Droppable，不是 DOM 禁区。** 数据提交保持 headless。列表核心 2026-09-10 冻结。
