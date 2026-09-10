# RUI × interact.js 修饰子集对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) **WP-DRAG.\*** 为准；产品手册见 [drag.md](../components/drag.md) §4；solid-dnd 原语见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md)。  
> **对照源**：`Reference/interact.js-main`（[interact.js](https://interactjs.io) **1.10.28** 语义）· RUI 以 `crates/rui/src/utils/drag/{geometry,modifier,session}.rs` 为准。  
> **日期**：2026-09-10 初稿并冻结修饰主路径 · **同日** 推进超越：`lockAxis: start` / `startAxis` / snap `range`+`endOnly` / restrict `endOnly`+`elementRect` · **同日** 指针动作：resize / gesture / inertia / 多指并发落在 [`pointer_action`](../../crates/rui/src/utils/pointer_action/mod.rs)（**不是** `utils/drag` tick）。  
> **范围**：拖拽位移修饰（`utils/drag`）+ interact **dropzone** 语义（映射到 `Droppable` + [`dropzone.rs`](../../crates/rui/src/utils/drag/dropzone.rs)）+ 拖拽动作（`utils/pointer_action`）。

**明细入口**：**[§0.1 对齐判定](#01-对齐判定何时可以宣称修饰子集对齐)**、**[§4 能力矩阵](#4-能力矩阵)**、**[§5 行为差异](#5-行为差异)**、**[§7 指针动作](#7-指针动作resize--gesture--inertia--多指)**、**[§7.6 autoScroll 三方对照](#76-边缘自动滚interact-插件-vs-sortable-autoscroll-vs-rui)**、**[§9 全库 parity](#9-全库-parity-的意义与必要性)**。

---

## 0. 一句话结论

RUI `utils/drag` 把 interact.js 的 **Modifier 管道精神**（按 order reduce 位移）和三条常用内置（`lockAxis` / `restrict` / `snap`）落进 [`DragModifier`](../../crates/rui/src/utils/drag/modifier.rs) + [`geometry.rs`](../../crates/rui/src/utils/drag/geometry.rs)，并与 solid-dnd 会话、Sortable 让位、Pinned Overlay **共用一个 tick**。interact **dropzone** 不另起插件：[`DropOverlap`](../../crates/rui/src/utils/drag/dropzone.rs) + per-zone 生命周期钩子挂在 [`DroppableBuilder`](../../crates/rui/src/utils/drag/builder.rs) 上，与 solid-dnd 碰撞面共用 `session::tick`。resize / 双指 gesture / inertia / 多指并发 **属于拖拽、不属于拖放**，落在兄弟模块 [`utils/pointer_action`](../../crates/rui/src/utils/pointer_action/mod.rs)，不焊进 Sortable/Overlay tick。

在修饰子集内，RUI **已经超过** interact 1.10 的 drag modifiers：`lockAxis: 'start'` 叠 live layout slot；`startAxis` 不匹配时 **留在 Pending**（对方取消 prepared）；`SnapGrid.range` + `end_only` 一个结构体；`RestrictAnchor`（`elementRect`）叠每 tick 的 parent/element 解析。

| 轴 | interact.js 1.10.x | RUI `utils/drag` |
|----|-------------------|------------------|
| **命题** | 通用 pointer 交互（drag / resize / gesture / drop） | Headless **列表/看板拖放**；interact 只贡献位移修饰 |
| **位移管道** | `modifiers` 数组 reduce `{x,y}` | `apply_modifiers`：`axis`→`snap`→`pin`→`restrict`→ author |
| **轴锁** | `lockAxis: 'x'|'y'|'start'` | `Axis::{X,Y,Xy,Start}` + `pin-locked-axis`（跟 live slot） |
| **钳位** | `restrict` / `restriction: 'parent'` / `elementRect` / `endOnly` | `Restrict` + `RestrictAnchor` + `.restrict_end_only` |
| **网格** | `snap` `{targets, range, endOnly}` | `SnapGrid { x, y, offset, range, end_only }` |
| **门槛** | `hold` / `distance` | `delay_ms` + `threshold` + `Activation::{And,Or}`（solid-dnd 血统，见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §5.1） |
| **没吃** | interact **autoScroll 插件形态**（`@interactjs/auto-scroll`、per-action `container`/`margin`/`speed`） | 边缘滚走 **Sortable `plugins/AutoScroll` 血统** + RUI `scroll:` 视口（§7.6）；**不**移植 interact 插件包 |

**不该做的事**：把 resize/gesture **焊进** `utils/drag` 的 Sortable tick；把 `DragModifier` 改回 CSS `transform`；拆掉 Sortable 自动滚或 solid-dnd 碰撞面。

### 0.1 对齐判定（何时可以宣称「修饰子集对齐」）

本专章的「对齐」指：**作者从 interact.js drag + modifiers 迁到 RUI 时，轴锁 / 钳位 / 网格 / 门槛四条主路径有对应 API，语义可预期。** 不宣称全库 parity。

| 判定项 | interact.js | RUI 现在 | 对齐后 |
|--------|-------------|----------|--------|
| 轴锁 x/y | `lockAxis` | `axis` / `Axis` + pin modifier | ☑ |
| `lockAxis: 'start'` | 有 | **`Axis::Start`**（激活后锁；长按无位移先保持 Xy） | **☑ 超**（+ live slot pin） |
| `startAxis` | 不匹配则取消 prepared | `.start_axis` / RSX `start_axis`；不匹配 **留 Pending** | **☑ 超** |
| 视口钳位 | `restrict: 'parent'|'self'|element|rect` | `Restrict::Viewport` / `Parent` / `Element` / `Rect` | ☑ |
| `elementRect` | 有 | `RestrictAnchor::{FULL,CENTER}` / RSX `restrict_anchor` | ☑ |
| restrict `endOnly` | 有 | `.restrict_end_only(true)` | ☑ |
| 网格吸附 | `snap.grid` | `SnapGrid` + `.snap()` | ☑ |
| snap `range` / `endOnly` | 有（targets 数组） | `SnapGrid.range` / `end_only` + `SnapTargets` | **☑ 超** |
| snap `targets[]` | 多格近者胜 | `.snap_targets(SnapTargets::new([…]))` | **☑ 超** |
| snap 自定义函数 | `targets` 里的 function | `.snap_fn(f)` / `SnapTarget::Fn` | **☑ 超** |
| dropzone `overlap` | `pointer` / `center` / ratio | Builder + RSX `overlap=` | **☑** |
| dropzone 生命周期 | activate / enter / leave / drop | Builder + RSX `onDragEnter` / `onDrop` / … | **☑** |
| 距离门槛 | `distance` / hold | `threshold` + `delay_ms` | ☑（And 默认；Or 见 solid-dnd 专章） |
| Modifier 管道 | `modifiers[]` | `DragModifier` + order | ☑ |
| resize | 一等 | [`pointer_action::resize`](../../crates/rui/src/utils/pointer_action/resize.rs) | **☑ 超**（min/max、from_center） |
| gesture / inertia | 一等 | [`gesture`](../../crates/rui/src/utils/pointer_action/gesture.rs) / [`inertia`](../../crates/rui/src/utils/pointer_action/inertia.rs) | **☑ 超**（centroid+translation；smoothEnd；与滚动同形衰减） |

---

## 1. 定位与体量

| 项 | interact.js | RUI |
|----|-------------|-----|
| **一句话** | Drag & drop, resizing and multi-touch gestures | 拖放套件里的 **press-delta 修饰** 子集 |
| **版本** | 1.10.28（npm） | 工作区 `utils/drag`；与 WP-DRAG.1–2 + §7 solid-dnd 同期 |
| **源码规模** | 整库 drag+resize+gesture+actions | `geometry.rs` + `modifier.rs` 内置四段 + author hook |
| **文档** | interactjs.io docs | [drag.md](../components/drag.md) §4 + Gallery `DragShowcase` |
| **测试** | 上游单测 | `modifier` / `geometry` / session tick 单测 + Gallery |

---

## 2. API 映射（drag modifiers only）

| interact.js | RUI | 备注 |
|-------------|-----|------|
| `draggable({ lockAxis: 'x' })` | `.axis(Axis::X)` / RSX `axis="x"` | 自由轴仍跟指针；锁轴跟 **live layout slot**（嵌套 scroll） |
| `lockAxis: 'start'` | `.axis(Axis::Start)` / `axis="start"` | 第一次超阈值位移锁 X 或 Y；**不**改 Sortable `sort_axis`（仍为 Y） |
| `startAxis: 'x'\|'y'` | `.start_axis(Axis::X)` / `start_axis="x"` | 不匹配时 RUI 留 Pending，interact 取消 prepared |
| `draggable({ modifiers: [restrictRect(...)] })` | `.restrict(Restrict::…)` | Provider 或 item；item 覆盖 Provider |
| `restriction: 'parent'` | `Restrict::Parent` | 跳过 kit item shell，对齐 interact parent 语义 |
| `elementRect: {0.5,0.5,0.5,0.5}` | `.restrict_anchor(RestrictAnchor::CENTER)` | 只钳中点；角可探出框 |
| `restrict endOnly` | `.restrict_end_only(true)` | 拖中自由，松手 clamp |
| `snap: { targets: [grid(...)], range, endOnly }` | `.snap(SnapGrid::…)` 或 `.snap_targets(SnapTargets::new([…]).range(r))` | 作用在 ghost **origin**；多 target 选最近 in-range |
| `snap.targets` function | `.snap_fn(f)` / RSX `snap_fn={…}` | `SnapPointFn`：`fn(x,y) -> Option<[f32;2]>`；可与 grid 混排 |
| `snap.targets[]` | `.snap_targets(…)` / RSX `snap_targets={…}` | 多格近者胜；可与 `snap_fn` 混排 |
| `dropzone({ overlap })` | `.overlap(…)` / RSX `overlap="center"\|"pointer"\|"0.25"` | 在 `can_drop` 之后过滤候选；ratio 用 ghost∩zone 面积比 |
| `dropzone` checker | `.drop_checker(f)` / RSX `drop_checker={…}` | 自定义是否接受（在 `accept` / `can_drop` 之后） |
| `dropzone` 事件 | Builder + RSX `onDragEnter` / `onDragLeave` / `onDrop` / `onDropMove` / `onDropActivate` / `onDropDeactivate` | per-zone 回调；全局仍走 `on_drag_over` / `on_drag_end` |
| `inertia: true` | `.inertia(true)` / RSX `inertia`（自由拖；Sortable 不滑行） | [`pointer_action::Inertia`](../../crates/rui/src/utils/pointer_action/inertia.rs) + `DragPhase::Coasting` |
| `autoScroll: true` | `.autoscroll(true)` / RSX `autoscroll` + `scroll_sensitivity` / `scroll_speed` | [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs)；见 **§7.6** |
| resize `autoScroll` | `ResizeOpts::autoscroll(true)` / `.autoscroll_opts` | `pointer_action::pump_edge_scroll` |
| gesture `autoScroll` | `set_gesture_opts(GestureOpts::…autoscroll(true))` | 双指 centroid 作指针 |
| `modifiers` reduce | [`apply_modifiers`](../../crates/rui/src/utils/drag/modifier.rs) | builtins 10→20→25→30；author 默认 40 |
| `distance` / hold | `threshold` / `delay_ms` | 与 solid-dnd 共用 `Activation` |

内置顺序（固定，不必作者拼）：

```text
MODIFIER_ORDER_AXIS        = 10   // lockAxis
MODIFIER_ORDER_SNAP        = 20   // snap grid
MODIFIER_ORDER_PIN_AXIS    = 25   // locked axis ← live slot (ScrollArea)
MODIFIER_ORDER_RESTRICT    = 30   // restrict box
MODIFIER_ORDER_AUTHOR      = 40   // .modifier / add_modifier
```

---

## 3. 架构对照

```text
interact.js draggable                RUI session::tick
  pointer delta                        pointer delta (press space)
  modifiers[] reduce                   apply_modifiers(builtins + author)
  → set translate on target            → overlay_pos inset (Pinned ghost)
  (DOM style)                          sortable_shift (sibling CSS translate, 不进管道)
```

- **值类型**：interact 与 dnd-kit 改 `{x,y}`；RUI 用 [`DragDelta`](../../crates/rui/src/utils/drag/modifier.rs)（press-delta），**故意**不叫 `Transform`（与 CSS / FLIP 分开）。
- **跟手**：interact 写 DOM `transform`；RUI 写 viewport `fixed` inset（[overlay_shells.md](../components/overlay_shells.md) §9）。
- **让位**：interact 无 Sortable 兄弟预览；RUI FLIP 走 `sortable_shift`，不进 `DragModifier`。

---

## 4. 能力矩阵

图例同 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §4。

| 能力 | interact.js | RUI | 判定 |
|------|-------------|-----|------|
| 轴锁 X/Y | 有 | 有 | 对齐 |
| `lockAxis: start` | 有 | **有** + live slot pin + 无位移时不瞎锁成 X | **超** |
| `startAxis` | 取消 prepared | 留 Pending，可改方向后再激活 | **超** |
| 视口钳位 | 有 | `Restrict::Viewport` | 对齐 |
| 父级钳位（无 id） | `parent` | `Restrict::Parent` | 对齐 |
| 元素 id 钳位 | 有 | `Restrict::Element` | **超**（每 tick live box） |
| `elementRect` | 有 | `RestrictAnchor` | 对齐 |
| restrict `endOnly` | 有 | `.restrict_end_only` | 对齐 |
| 显式矩形钳位 | 有 | `Restrict::Rect` | 对齐 |
| 网格吸附 | 有 | `SnapGrid` | 对齐 |
| snap `range` / `endOnly` | targets 数组 | `SnapGrid` 一等字段 + [`SnapTargets`](../../crates/rui/src/utils/drag/geometry.rs) | **超** |
| Modifier 管道 + order | 有 | `DragModifier` | 对齐（改名避 CSS） |
| 距离 / hold 门槛 | 有 | `threshold` + `delay_ms` | 对齐（组合语义见 solid-dnd 专章） |
| 边缘自动滚 | `@interactjs/auto-scroll` 插件 | Sortable `plugins/AutoScroll` → RUI `autoscroll.rs` | **对齐 Sortable**；**超** interact 列表场景（按住仍泵、`scroll_lock`） |
| resize | 一等 | `pointer_action` 边/角 + invert/aspect/min-max | **超** |
| gesture（pinch/rotate） | 一等 | 双指 + centroid/translation | **超** |
| inertia | 有 | 自由拖 `.inertia`；Sortable 不滑行 | **超**（列表不跟 FLIP 抢） |
| dropzone `overlap` | `pointer` / `center` / ratio | [`DropOverlap`](../../crates/rui/src/utils/drag/dropzone.rs) + RSX `overlap=` | **对齐** |
| dropzone 生命周期 | activate / enter / leave / drop | [`dropzone.rs`](../../crates/rui/src/utils/drag/dropzone.rs) + RSX `onDragEnter` / `onDrop` / … | **对齐** |
| dropzone checker | 有 | `.drop_checker` / RSX `drop_checker` | **对齐** |
| multi-touch | 有 | `PointerSet` + `max`/`maxPerElement`；触屏 `sync_touches` | **超**（配额 + 同元素升 gesture） |
| 库测 + Gallery | 示例站 | drag + pointer_action 单测 + Gallery | **超** |

Gallery 已演示：`axis`、Saturn `restrict=parent`、四角 `restrict` 元素 id、`SnapGrid` / **`SnapTargets` 双格近者胜** / **RSX `snap_fn` 水平磁吸线**、`endOnly`+`range`+中心钳位、`axis=start`、Provider/item 覆盖 restrict、modifier 管道 author 段、**RSX `<Droppable overlap="center" onDragEnter onDrop>`**、**边角 resize / 双指 gesture / inertia 甩砖**。见 `examples/gallery/src/pages/drag.rs`。

---

## 5. 行为差异

### 5.1 坐标空间

interact.js 用 DOM `getBoundingClientRect` + CSS transform。RUI 用 **pointer 空间 layout slot**（scroll 已计入，不含让位 translate）。嵌套 `ScrollArea` 下锁轴项跟 **当前槽** 而不是按下瞬间坐标——自绘世界的必要代价，见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §5.4。

### 5.2 `restrict: 'parent'`

interact 绑 DOM offsetParent。RUI `Restrict::Parent` 绑 **作者容器**（跳过 kit 的 hug-content draggable shell），Gallery「四角旋转」若误用 `parent` 会钳在内层居中 Box——文档与 demo 用命名 Droppable id（`corners-arena`）。

### 5.3 snap 作用点

interact snap 可挂多个 target；RUI 单格用 `SnapGrid`，多格用 [`SnapTargets`](../../crates/rui/src/utils/drag/geometry.rs)（近者胜），自定义点用 `.snap_fn` / `SnapTarget::Fn`（与 grid 可混在同一 `SnapTargets`）。`range > 0` 时只在距格点不超过 range 时吸附（磁铁）；`range = 0` 总是 round（interact 默认 Infinity）。`end_only` 拖中跳过 builtin，[`end`](../../crates/rui/src/utils/drag/session.rs) 再跑一遍并把 `DragEvent.delta` 写成吸附后的幽灵位移。

### 5.4 `lockAxis: start` 与 Sortable

interact 的 start 锁同时改指针坐标。RUI 把 **运动锁**（`motion_axis`）和 **列表方向**（`axis.sort_axis()`，`Start` 仍当 Y）拆开：竖列里横滑锁轴不会把插入算法改成水平。锁上之后仍走 `pin-locked-axis`（嵌套 `ScrollArea`）。`startAxis` 不匹配时 interact 取消本次 prepared；RUI **留在 Pending**，用户改方向后仍能激活。

### 5.5 与 solid-dnd Transformer 同名

interact / dnd-kit 叫 `Modifier`；solid-dnd 叫 `Transformer`；RUI 统一 **`DragModifier`** + **`DragDelta`**，避免与 CSS `transform` / FLIP `translate` 三词叠在一起。对照见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) §6.1。

---

## 6. 已吃进的路径（相对 interact drag modifiers）

| 项 | 落地 | 源文件 |
|----|------|--------|
| 轴锁 + live slot pin | ☑ WP-DRAG.1 | `geometry::Axis`, `modifier` pin @ 25 |
| `Axis::Start` / `start_axis` | ☑ 2026-09-10 | `geometry.rs`, `session::tick` |
| restrict 五态 | ☑ WP-DRAG.1 | `geometry::Restrict` |
| `RestrictAnchor` / `restrict_end_only` | ☑ 2026-09-10 | `geometry.rs`, `modifier` restrict @ 30 |
| SnapGrid + range / end_only | ☑ 2026-09-10 | `geometry::SnapGrid` |
| `SnapTargets` 多格近者胜 | ☑ 2026-09-10 | `geometry::SnapTargets` + `.snap_targets` |
| `snap_fn` / `SnapTarget::Fn` | ☑ 2026-09-10 | `geometry::SnapPointFn` + `.snap_fn` / RSX `snap_fn` |
| dropzone overlap + 生命周期 | ☑ 2026-09-10 | `dropzone.rs` + Builder / RSX 双写 |
| RSX 双写（snap / dropzone） | ☑ 2026-09-10 | `rui_macros` + `droppable_from_rsx` / `<DragProvider snap_*>` |
| resize / gesture / inertia / 多指 | ☑ 2026-09-10 | `pointer_action/` + Gallery |
| Author modifier 槽 | ☑ P0 2026-09-09 | `modifier.rs`, `session::add_modifier` |
| Gallery 四角 / parent / snap / start / endOnly | ☑ | `examples/gallery/src/pages/drag.rs` |

---

## 7. 指针动作（resize / gesture / inertia / 多指）

这些是 **拖拽**，不是 Sortable/Overlay **拖放**。实现在 [`utils/pointer_action`](../../crates/rui/src/utils/pointer_action/mod.rs)，**不**进 `utils/drag` 的碰撞 / FLIP tick。

### 7.1 resize

interact `edges` / `invert: none|negate|reposition` / `square` / `preserveAspectRatio` / `margin` 命中。RUI：[`ResizeSession`](../../crates/rui/src/utils/pointer_action/resize.rs) + [`hit_edges`](../../crates/rui/src/utils/pointer_action/resize.rs)。**超出**：`min_size` / `max_size`、`from_center`。ShadCN 一维分栏仍是 [`resizable`](../../crates/rui/src/components/resizable.rs)，不要拆掉。

### 7.2 gesture

interact 两指 `distance` / `angle` / `scale` / `ds` / `da` / `box`。RUI 另给 **centroid** 与 **translation**。触屏：`lifecycle` 把全部 touch 喂给 [`sync_touches`](../../crates/rui/src/utils/pointer_action/pointers.rs)（逻辑像素），并保留 `__rui_pinch_scale`。桌面 Gallery 用两个可拖圆点驱动同一套纯函数。

### 7.3 inertia

interact `resistance=10` / `minSpeed=100` / `endSpeed=10` / `smoothEndDuration`。RUI 指数衰减与滚动 fling 同形。自由拖 `.inertia(true)` / RSX `inertia` 松手进入 [`DragPhase::Coasting`](../../crates/rui/src/utils/drag/session.rs)；**Sortable（`from_list`）立即提交**，不跟 FLIP 抢帧。速度不够但有 `endOnly` 修饰时走 smoothEnd。Esc 取消滑行。再按同一项且 `allow_resume` 则抓住继续拖。

### 7.4 多指并发

interact `max` / `maxPerElement`。RUI [`PointerSet`](../../crates/rui/src/utils/pointer_action/pointers.rs) + [`pointer_drag::begin_pointer`](../../crates/rui/src/utils/pointer_drag.rs)。同元素第二指 **升为 Gesture**（替换该元素上的 Drag）；不同元素在 `max` 内可各拖一条。主触点仍走 mouse 路径（不拆 Sortable）。

### 7.5 dropzone（映射到 `Droppable`）

interact 的 dropzone 是独立 action；RUI **不**复制插件注册表，把语义挂在 solid-dnd 原语上：

| interact | RUI |
|----------|-----|
| `overlap: 'pointer'`（默认） | `DropOverlap::Pointer` — 指针在 zone 内 |
| `overlap: 'center'` | `DropOverlap::Center` — 幽灵中心在 zone 内 |
| `overlap: 0.25` | `DropOverlap::Ratio(0.25)` — `ghost ∩ zone / ghost` ≥ 阈值 |
| `checker` | `.drop_checker(f)` — `can_drop` 之后 |
| `ondropactivate` / `ondropdeactivate` | `.on_drop_activate` / `.on_drop_deactivate` |
| `ondragenter` / `ondragleave` | `.on_drag_enter` / `.on_drag_leave` |
| `ondropmove` | `.on_drop_move` |
| `ondrop` | `.on_drop` |

实现：[`dropzone.rs`](../../crates/rui/src/utils/drag/dropzone.rs) 过滤候选 + 在 [`session.rs`](../../crates/rui/src/utils/drag/session.rs) 的 `tick` / `commit_end` / `cancel` 点火。全局 `on_drag_over` / `on_drag_end` 仍保留；per-zone 钩子用于高亮 / HUD，不必订阅全局 `active_droppable`。

自定义 snap 函数：`.snap_fn(|x,y| Some([x, line_y]))` 进入 builtin snap @ 20，与 `SnapGrid` / `SnapTargets` 同一管道（不必再写 author `.modifier`）。RSX：`<DragProvider snap_fn={…}>` / `<Droppable overlap="center" onDragEnter={…}>`。

### 7.6 边缘自动滚（interact 插件 vs Sortable `AutoScroll` vs RUI）

三家解决的是同一用户问题：**拖/缩到滚动容器边缘时，容器应继续滚，以便把项放进视口外区域。** 实现路径不同；RUI **刻意**跟 Sortable 列表血统，不移植 interact 的插件包。

| 维度 | interact.js `@interactjs/auto-scroll` | Sortable `plugins/AutoScroll` | RUI [`autoscroll.rs`](../../crates/rui/src/utils/drag/autoscroll.rs) |
|------|--------------------------------------|------------------------------|---------------------------------------------------------------------|
| **挂载** | 独立插件；`interact(element).draggable({ autoScroll: true \| {…} })` 也可挂在 resize/gesture | 内置插件 `pluginName: 'scroll'`，`Sortable.mount(AutoScroll)`；默认 `scroll: true` | `DragProvider` / `drag_scope` 会话内建；`.autoscroll(false)` / RSX `autoscroll={false}` 关闭 |
| **触发** | drag/resize/gesture **move** 时检查指针 vs 容器 margin | `dragover` / `pointermove` / `mousemove` / `touchmove`（fallback 路径） | `session::tick` 指针移动 + [`pump_autoscroll`](../../crates/rui/src/utils/drag/session.rs)（**按住不动仍泵**） |
| **滚谁** | `container`：`window` 或指定 `HTMLElement` | `bubbleScroll`：从 `elementFromPoint` 向上走 **DOM overflow** 祖先链 | layout walk 收集带 `scroll:` 的 **RUI 视口**（`ScrollHit`），非 DOM `overflow` 嗅探 |
| **边缘带** | `margin`（默认 60px） | `scrollSensitivity`（默认 30px） | `sensitivity`（默认 40px） |
| **速度** | `speed` **px/s** + rAF 积分；另有 `interval` / `distance` | `scrollSpeed` **px / 24ms interval**（`setInterval`） | `speed` **px / 显示帧**（按 `dt` 缩放，默认约 14px@60Hz） |
| **轴向** | 指针在 margin 内则 ±1 方向滚 X/Y | 同上（`vx`/`vy` 层叠多层 scroll parent） | `edge_delta` 同上；另加 [`ScrollLock`](../../crates/rui/src/utils/drag/autoscroll.rs)（`scroll-lock-all` / `x` / `y` / `auto`）冻结泵轴 |
| **与幽灵** | 改 DOM scroll；跟手靠 CSS `transform` | fallback 时 scroll 间隔内 **重发** `_onTouchMove` 更新 ghost | 滚 `scroll_offset_*` signal（`set_silent` 防重挂载）；overlay 跟 `overlay_pos` + layout slot |
| **resize/gesture** | autoScroll 同样服务 resize/gesture action | 仅 Sortable 列表拖 | **可选**：`ResizeOpts::autoscroll` / `set_gesture_opts(GestureOpts::autoscroll)` + `pump_edge_scroll`（复用 `edge_scroll.rs`） |
| **可配置项** | per-action `margin` / `speed` / `container` | `scrollSensitivity` / `scrollSpeed` | 拖放：`.scroll_sensitivity` / `.scroll_speed` / `.autoscroll_opts` + RSX 双写；默认 `sensitivity=40` `speed=14` |

**结论（写进「没吃」行的含义）**：

- **没吃的是 interact 插件形态**（独立包、per-action `container`/`margin`/`speed` 字典、与 resize/gesture 共用的那一套注册表）——不是「没有边缘自动滚」。
- **列表/看板作者体验**应对标 **Sortable `AutoScroll`**（bubble 祖先、边缘带、/list 拖），RUI 在此基础上加了 **静止泵** 与 **`scroll_lock`**，并绑定自绘 `ScrollArea` 信号而非 DOM overflow。
- **不必**移植 `@interactjs/auto-scroll` 插件包：拖放 + resize/gesture 均复用 [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs)（`ScrollHit` / `edge_delta` / `AutoScroll`），按会话可选开启。

---

## 8. 文件地图

| RUI | 职责 |
|-----|------|
| [`geometry.rs`](../../crates/rui/src/utils/drag/geometry.rs) | `Axis`（含 `Start`）、`Restrict`、`RestrictAnchor`、`SnapGrid` / `SnapTargets` / `SnapTarget::Fn`、`Activation` |
| [`dropzone.rs`](../../crates/rui/src/utils/drag/dropzone.rs) | `DropOverlap`、`DropzoneHooks`、overlap 过滤与生命周期 |
| [`modifier.rs`](../../crates/rui/src/utils/drag/modifier.rs) | `DragModifier`, `DragDelta`, builtins, `apply_modifiers` |
| [`session.rs`](../../crates/rui/src/utils/drag/session.rs) | `tick` 组装 ctx 并写 `overlay_pos`；`.inertia` → `Coasting` |
| [`pointer_action/`](../../crates/rui/src/utils/pointer_action/mod.rs) | resize / gesture / inertia / PointerSet |
| [`pointer_drag.rs`](../../crates/rui/src/utils/pointer_drag.rs) | 按 pointer id 的会话表 |
| [`edge_scroll.rs`](../../crates/rui/src/utils/edge_scroll.rs) | 边缘滚（拖放 + resize/gesture 共用；`scroll:` 视口 + `ScrollLock` + 可配置 sensitivity/speed） |
| [`drag.md`](../components/drag.md) | 产品手册 |
| [`SOLID_DND_COMPARISON.md`](SOLID_DND_COMPARISON.md) | 原语 / Overlay / Sortable（本专章不重复） |

---

## 9. 全库 parity 的意义与必要性

**interact.js 全库**指：统一 `interact(target)` 注册表 + draggable / dropzone / resizable / gesturable actions + modifiers 管道 + **插件生态**（autoScroll、dev-tool、reflow…）+ 以 **DOM `transform` / `getBoundingClientRect`** 为中心的跟手与命中。

**本专章「对齐」**只覆盖迁到 RUI 时有价值的子集（修饰、dropzone、指针动作、§7.6 列表边缘滚）。**全库 parity** 则还要复刻统一 action 注册、全部官方插件、CSS transform 写回，以及与 Sortable 重叠的列表模型——相当于在 RUI 里再实现一个 interact.js。

**是否有必要：否（不建议作为 WP）**：（1）命题不同 — RUI 是 solid-dnd + Sortable + Overlay 拖放套件，全库 parity 与 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md) 重复建设；（2）坐标不可兼容 — interact 跟 DOM + `transform`，RUI 跟 layout slot + Pinned overlay（§5.1）；（3）高价值能力已拆进 `utils/drag` / `pointer_action` / `autoscroll`；（4）全库 parity 会施压把 resize/gesture 焊回 interact 式 mega-session，破坏 §7 边界。

**值得单独开项（非全库）**：interact 片段迁移 → 本专章 + [drag.md](../components/drag.md) §4；resize 时要边缘滚 → 复用 `autoscroll.rs` 接 `ResizeSession`。

---

## 10. 总表（给排期用）

| | |
|--|--|
| **已对齐的 interact drag 修饰** | axis、restrict（viewport/parent/element/rect）、snap、threshold/delay、modifier 管道 |
| **RUI 超出 interact 修饰子集** | `Axis::Start`+live slot、`start_axis` 留 Pending、`SnapGrid.range`/`end_only`、`RestrictAnchor`、live element 钳位、Sortable 自动滚、solid-dnd 碰撞 + Overlay、双写 RSX、库测 |
| **已对齐的 interact 拖拽动作** | resize、gesture、inertia、multi-touch（`pointer_action`，不进拖放 tick） |
| **相对缺失（挡「修饰子集对齐」）** | 无 |
| **dropzone + snap 函数 + RSX 双写** | ☑ §7.5 |
| **autoScroll** | ☑ §7.6（Sortable 血统；**不**移植 interact 插件包） |
| **interact 全库 parity** | **不建议**；见 **§9** |
| **solid-dnd 血统** | 会话 / Overlay / Sortable 见 [SOLID_DND_COMPARISON.md](SOLID_DND_COMPARISON.md)（**已冻结**） |

一句话收束：**interact.js 对本项目有价值的部分（修饰、dropzone、snap 函数、指针动作、列表边缘滚）已对齐或超越；边缘滚跟 Sortable 而非 interact 插件包；全库 parity 既不必要也与 Sortable/solid-dnd 架构冲突。** 不要把 resize/gesture 焊回 Sortable tick。
