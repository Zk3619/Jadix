# RUI × solid-dnd 拖拽能力对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) **WP-DRAG.\*** 为准；产品手册见 [drag.md](../components/drag.md)；浮层几何见 [overlay_shells.md](../components/overlay_shells.md) §9。  
> **对照源**：`Reference/solid-dnd-main`（[@thisbeyond/solid-dnd](https://github.com/thisbeyond/solid-dnd) **0.7.5**）· RUI 以 `crates/rui/src/utils/drag` 为准。  
> **日期**：2026-09-08 初稿 · **2026-09-09** §8 P0 · **2026-09-10** §7 + **§11 Extendable** + WP-DRAG.3 落地 · **同日** interact.js 专章冻结 → [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)。  
> **冻结**：**2026-09-10** — §7 + §11 + WP-DRAG.3 已全部 ☑；库测 `cargo test -p rui --lib utils::drag` 162 passed。本专章 **不再扩 scope**。  
> **范围**：本专章只对照 **solid-dnd**。SortableJS 列表血统见 [SORTABLE_JS_COMPARISON.md](SORTABLE_JS_COMPARISON.md)；**interact.js** 见 [INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)（**已冻结**）。

**明细入口**：**[§0.1](#01-对齐判定何时可以宣称对齐-075)** · **[§4](#4-能力矩阵)** · **[§5](#5-行为差异不是有无而是语义不同)** · **[§6](#6-相对-solid-dnd-已补的缺口)** · **[§7](#7-已落地契约对齐-extendable)** · **[§8](#8-建议优先级)** · **[§11](#11-extendable-2026-09-10-已落地)**。

---

## 0. 一句话结论

RUI 的 `utils/drag` **已经吃透 solid-dnd 的原语模型**（Provider / Draggable / Droppable / Overlay / 碰撞 + 平局 sticky / **不自动改 `Vec`**），并在其上叠了 SortableJS（自动滚、让位、group、handle）和 interact.js（轴锁、钳位、网格吸附）。相对 0.7.5 的 solid-dnd，**产品可用面更宽，可以宣称超越**（自动滚、Esc、方向键、默认克隆幽灵、`accept`/`disabled`/`stack`、嵌套 innermost、库测、Gallery）。

差距不在「会不会拖」。README **Extendable** 五项 + Sortable **clone-pull** 已落地（2026-09-10，见 [§11](#11-extendable-2026-09-10-已落地)）。`collision_fn`、Overlay `child_for` / `align`、激活 Or、`SortableList.direction`、槽矩形 Debugger、拖中/空闲 `use_drag().hits()` 已落地（2026-09-10）。

| 轴 | solid-dnd 0.7.5 | RUI `utils/drag` |
|----|-----------------|------------------|
| **命题** | Solid 细粒度 + dnd-kit 精神的无头 toolkit | 同一套原语，落到自绘桌面 + RSX/Builder 双写 |
| **默认路径** | 指针 sensor + 三碰撞算法 + 可选 Overlay + 垂直 Sortable | 指针会话 + **四**碰撞算法 + Pinned Overlay + 可水平轴的 Sortable 让位 |
| **落点** | 调用方自己改数据 | 同样；另给 `apply_reorder_vec` / `apply_transfer` |
| **扩展哲学** | sensor / collision / transformer **都是插件点** | `DragModifier` **已是管道**；`collision_fn` + 枚举；**sensor 槽 ☑**（[`sensor.rs`](../../crates/rui/src/utils/drag/sensor.rs)） |
| **成熟度** | 已发布 npm；无仓内测试树 | Experimental，不进 `prelude` / SemVer 0.1；库测覆盖 collision / session / overlay / RSX |

**不该做的事**：为对齐 solid-dnd 拆掉已落地的自动滚 / Esc / 让位；把 Overlay 改回「必须手写幽灵子树」；把 `CollisionAlg` 枚举删成只能自定义函数。Extendable 与 clone-pull 契约见 [§11](#11-extendable-2026-09-10-已落地) 与 [drag.md](../components/drag.md) §8。

### 0.1 对齐判定（何时可以宣称「对齐 0.7.5」）

solid-dnd README「What's implemented」六条，RUI **已经全部有对应物**。README **Extendable** 三条里，Transformer 已对齐（RUI 名 `DragModifier`）；自定义 Sensor 槽 **☑**（见 [§11.1](#111-自定义-sensor-槽--)）。

§7.1–7.5 与拖中/空闲 `use_drag().hits()` 已落地。Extendable 见 **[§11](#11-extendable-2026-09-10-已落地)**。

| 判定项 | 0.7.5 | RUI 现在 | 对齐后 |
|--------|-------|----------|--------|
| 原语六条 | 有 | **有** | — |
| Transformer 管道 | 有 | **有**（`DragModifier`） | — |
| `CollisionDetector` 函数 | 有 | **有**（`collision_fn` + 四枚举回退） | ☑ §7.1 |
| Overlay `(active) =>` children | 有 | **有**（`.child_for(|item| …)`） | ☑ §7.2 |
| Overlay 不等大居中 | 有 | **有**（`OverlayAlign::Center`） | ☑ §7.2 |
| 激活 delay **或** distance | OR | `Activation::Or`（默认仍 And） | ☑ §7.3 |
| `SortableList.direction` | 水平算法有、文档滞后 | **已接线**（未调用则继承 Provider `axis`） | ☑ §7.4 |
| Debugger | 有 | **有**（`set_debug_drag_hits` / `.debug_hits(true)`） | ☑ §7.5 |
| 只读 hit 表（拖中） | context store | **有**（`use_drag().hits()`） | ☑ §7.7 |

---

## 1. 定位与体量

| 项 | solid-dnd | RUI |
|----|-----------|-----|
| **一句话** | A lightweight drag and drop toolkit for Solid | Headless drag kit：solid-dnd 原语 + 超越 Sortable |
| **版本** | `@thisbeyond/solid-dnd` 0.7.5（peer `solid-js ^1.5`） | 工作区 `utils/drag`；WP-DRAG.1–2 ☑（2026-09-07）；P0 modifier / nested ☑（2026-09-09） |
| **灵感** | [dnd-kit](https://dndkit.com)（README 自述） | solid-dnd + SortableJS + interact.js（`mod.rs` 模块头） |
| **源码规模** | `src/` 约 15 个文件；零运行时依赖（除 Solid） | `drag/{mod,builder,session,modifier,collision,geometry,overlay,sortable,autoscroll,debug}.rs` + RSX 宏 |
| **文档** | README + CHANGELOG；无独立 docs 树 | [drag.md](../components/drag.md) + Gallery `DragShowcase` |
| **测试** | 参考树内 **无** `*.test.*` | collision / geometry / session / autoscroll / overlay / sortable / `tests/drag_rsx.rs` |
| **北极星** | 细粒度、零 re-render、CSS transform | **超越 SortableJS**（~40k★）；键盘一等；不进 prelude |

solid-dnd README「What's implemented」六条（`createDraggable` / `createDroppable` / `DragOverlay` / sensors / 三碰撞 / sortable + 多 Provider）在 RUI 都有对应物；RUI 额外宣称的自动滚、Esc、方向键、FLIP 时长、`stack`、默认克隆 **不在** solid-dnd 的已实现清单里。

---

## 2. 架构对照

```text
solid-dnd                              RUI
DragDropProvider                       drag_scope / <DragProvider>
  DragDropSensors → PointerSensor        pointer_drag · DragKind::Scope
    createDraggable                        draggable / <Draggable>
    createDroppable                        droppable / <Droppable>
    createSortable + SortableProvider      sortable_item + sortable_list
    DragOverlay → Portal(body)             drag_overlay → Pinned portal
    CollisionDetector (fn)                 CollisionFn + CollisionAlg + detect_with()
    Transformer 管道                       modifier.rs：`DragModifier` + `.modifier` / `add_modifier`
    DragDropDebugger                       set_debug_drag_hits / `.debug_hits`
```

### 2.1 会话

| 步骤 | solid-dnd | RUI |
|------|-----------|-----|
| 按下 | `createPointerSensor`：`button === 0`，绑 `pointermove/up` | `begin` → `DragPhase::Pending` |
| 激活 | **250 ms 或 10 px，先到者激活**（硬编码） | 默认 **距离且延迟**（`Activation::And`，4 px / 0 ms）；`.activation(Or)` 对齐对方 |
| 位移 | `sensorMove` transformer（order 0）叠 `delta` | `tick`：`apply_modifiers`（axis → snap → pin → restrict → author）→ `overlay_pos` |
| 碰撞 | `detectCollisions()`：对 **transformed** overlay/draggable vs **未变换** droppable | 同思路：指针空间 **layout slot**（已扣 scroll / 不含 CSS translate） |
| 结束 | `dragEnd` → `onDragEnd(previous 快照)` | `end` → `on_drag_end`；**Esc → `cancel`（不提交）** |

两侧都 **不** 在 drop 时改调用方数组。Sortable 拖中只做视觉预览；持久化在 `onDragEnd`。

### 2.2 Overlay / 源项

| | solid-dnd `drag-overlay.tsx` | RUI `overlay.rs` |
|--|------------------------------|------------------|
| 挂载 | `Portal` → `document.body`；`position: fixed` | `portal_sync(Pinned)` + Unanchored `fixed` inset（避免 Point 菜单钳 Y） |
| 默认幽灵 | **必须**给 children；静态 JSX 或 `(activeDraggable) => JSX` | **空 `<DragOverlay />` 重跑源项 author body**（`clone_source_preview`） |
| 跟手 | CSS transform 改 overlay | `overlay_pos` **peek + `reactive_sync` 写 inset**（订阅 pos 会每帧重建幽灵） |
| 源项位移 | 有 overlay 时源 `transform` = noop（源留在原位） | 源不跟手；`source_opacity` 默认 0.4；0 则留空槽 |
| 尺寸 | overlay 实测后 **相对源框居中** | 默认 grab-offset；`.align(Center)` 源框中心 − 幽灵半宽高；`w/h=100%` 的克隆钉 `drag_extent` |
| Esc 栈 | DOM overlay，与菜单栈无关 | Pinned，**不进** Esc overlay stack |

### 2.3 Sortable

| | solid-dnd | RUI |
|--|-----------|-----|
| 组合 | `createSortable` = draggable + droppable + `sortableOffset` transformer | `<SortableItem>` = `draggable.also_droppable(true)` |
| 顺序源 | `SortableProvider ids={...}`；拖中 `moveArrayItem` 改 **内部** `sortedIds` | 项上 `list` + `index`；`hover_index` 驱动让位 |
| 视觉 | 比较 initial vs sorted，把项 translate 到目标 slot 的 layout 差 | `sortable_shift`：兄弟 CSS `translate` FLIP（[`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150 ms） |
| 提交 | 松手 `sortedIds` 重置为 `props.ids`；用户改数据后再传入 | `apply_reorder_vec` / `apply_transfer` / `apply_event` |
| 方向 | README：**仅宣称垂直**；CHANGELOG 0.6.0 算法已支持水平（同尺寸才正常） | Provider `axis` 驱动 `insert_index_along_axis`；`SortableList.direction` **覆盖**（未调用则继承 Provider） |

---

## 3. 公开 API 映射

| solid-dnd | RUI | 备注 |
|-----------|-----|------|
| `DragDropProvider` | `drag_scope` / `<DragProvider>` | 两侧都隔离 context |
| `useDragDropContext` → `[Store, Actions]` | `use_drag` → `DragCtx` | RUI 更瘦：无全表 store；拖中 `hits()` 是 tick 缓存 |
| `DragDropSensors` / `createPointerSensor` | 内建 `pointer_drag` | RUI **无**公开 sensor 插件 |
| `createDraggable(id, data?)` | `draggable(id)` + `item_type` / `group` / `list` / `index` | `data` 袋 vs 一等字段 |
| `createDroppable(id, data?)` | `droppable(id)` + `accept` / `group` / `list` | RUI 内建 type 过滤 |
| `createSortable` | `sortable_item` / `<SortableItem>` | |
| `SortableProvider` / `useSortableContext` | `sortable_list` / `<SortableList>` | 无独立 ids store |
| `DragOverlay` | `drag_overlay` / `<DragOverlay>` | 见 §2.2 |
| `closestCenter` / `closestCorners` / `mostIntersecting` | `CollisionAlg::{ClosestCenter, ClosestCorners, MostIntersecting}` | 默认都是 most-intersecting |
| （`layoutContainsPoint`，**未进 public index**） | `CollisionAlg::PointerWithin` | RUI 升成一等算法 |
| `layoutStyle` / `transformStyle` | 调用方 utility / `reactive_sync` | 无独立 style helper 导出 |
| `DragDropDebugger` | [`debug.rs`](../../crates/rui/src/utils/drag/debug.rs) 槽矩形 | ☑ 2026-09-10 |
| `Id = string \| number` | `String` | |
| `Transformer` | [`modifier.rs`](../../crates/rui/src/utils/drag/modifier.rs) `DragModifier` / `DragDelta` | 见 §6.1；避开 CSS `transform` |
| `dragActivators` 挂子节点 = handle | `<DragHandle>` + `handle_only` | RUI 另有 `drag_ignore` |
| `isActiveDraggable` / `isActiveDroppable` | `use_drag()` signals + `rui_drag_over` | 高亮仍归调用方 |
| （内部 `recomputeLayouts`） | `collect_hits` / `last_hits` / `use_drag().hits()` | 拖中 tick 缓存；空闲全表仍须持有树根 |

事件：`onDragStart` / `onDragMove` / `onDragOver` / `onDragEnd` 两侧都有。RUI 另有 `can_drop`（Sortable `onMove` 否决）。

---

## 4. 能力矩阵

图例：**有** = 一等 API；**超** = RUI 有而 solid-dnd 无或明显更深；**差** = 弱于 / 仅部分；**无** = 没有。

| 能力 | solid-dnd | RUI | 判定 |
|------|-----------|-----|------|
| Provider / 隔离多实例 | 有 | 有 | 对齐 |
| Draggable / Droppable | 有 | 有 | 对齐 |
| 不自动移动数据 | 有 | 有 | 对齐 |
| Overlay 幽灵 | 有（手写子树 / render-prop） | 有（**默认克隆** + `.child()` + `.child_for`） | **超**（DX） |
| Overlay 跟手不重建子树 | CSS transform | `peek` inset；岛不订阅 `overlay_pos` | **对齐**（路径不同） |
| mostIntersecting / closestCenter / closestCorners | 有 + sticky 平局 | 有 + sticky 平局 | 对齐 |
| pointer-within / 点在区内 | 内部 `layoutContainsPoint`，未导出 | **一等** `PointerWithin` | **超** |
| 自定义 `CollisionDetector` 函数 | 有 | 有（`collision_fn` + 枚举回退） | **对齐** |
| Transformer / 可组合修饰 | 有 | 有（`DragModifier` builtins + author） | **对齐**（RUI 改名） |
| 自定义 Sensor | 有（`addSensor`） | **有**（`.sensor` / `add_sensor`） | **对齐** |
| Handle | `dragActivators` | `<DragHandle>` + `handle_only` | 对齐 |
| 忽略内部控件 | 无一等 | `drag_ignore` | **超** |
| disabled | 0.2.0 **已删** | `disabled` attr | **超** |
| type / accept / group | 靠 `data` + 自定义 detector | 一等 `item_type` / `accept` / `group` | **超** |
| 激活 delay / 距离 | 硬编码 250 ms **或** 10 px | 可配；默认 And，`.activation(Or)` 对齐 | **超**（可配） |
| 轴锁 | 示例：transformer `{ x: 0 }` | `axis` / `Axis` | **超**（一等） |
| 视口 / 矩形 / **parent** / **元素 id** 钳位 | 无内置 | `restrict` Viewport / Rect / Parent（无需父级 id）/ Element(id)，项可覆盖 Provider | **超** |
| 网格吸附 | 无内置 | `SnapGrid` | **超** |
| 空容器插入阈值 | 无 | `empty_insert` | **超** |
| `can_drop` 否决 | 无（自定义 detector） | 有 | **超** |
| 边缘自动滚 | **无** | 有；按住仍泵；`scroll_lock` | **超** |
| 实时兄弟让位 | Sortable transform 预览 | 让位 + FLIP 150 ms / 可关 | **超** |
| 跨容器 | 多 droppable + 用户改树 | `group` + `apply_transfer` | 对齐并更方便 |
| Esc 取消 | 无 | 有（`input_controller`） | **超** |
| 方向键改插入点 | 无 | `nudge_insert` | **超** |
| 键盘 **启动** 拖拽 | 无 | 无 | 双方无（dnd-kit KeyboardSensor 才有） |
| a11y / ARIA live | 无 | 无（仅 Esc/方向键） | 双方弱 |
| 多选拖 | 无 | 无 | 双方无 |
| 嵌套列表 | 无一等 | innermost refine + Gallery ☑ P0 | **超** |
| clone-pull | 无一等 | **`.pull_clone` + `apply_clone_event`** | **超**（Sortable 血统） |
| Overlay 源自动隐藏 | 否（作者自管） | `source_opacity` | **超** |
| 任意移动 + 叠放 | 作者 `z-index` | `stack` auto/top/bottom/keep | **超** |
| 水平 Sortable | 文档滞后；同尺寸才稳 | Provider `axis`；list `.direction()` 覆盖 | 对齐 |
| 只读 hit 表 | context store | 拖中 `use_drag().hits()`；空闲 `registered_hits_in_scope` | **对齐** |
| Debugger | `DragDropDebugger` | `debug_drag_hits` 槽矩形 | **对齐** |
| 项上自由 `data` | `Record<string, any>` | **`.data(Rc<dyn Any>)`** | **对齐** |
| RSX 双写 | — | `<DragProvider>` 等 ADR-9 | **超** |
| 库测 | 无 | 有 | **超** |

Gallery 已演示：park 投放、自定义 Overlay 子树、`child_for` + `align=center`、四角旋转、accept 条件、Handle、restrict=viewport/parent/元素 id、stack 叠放、source_opacity 0/0.4、FLIP 开关、Kanban `group` + `apply_transfer`、ScrollArea 内自动滚、嵌套列表、`debug_hits` 开关、`SortableList.direction` 双轴、`activation=or`、`collision_fn` 优先点名、`use_drag().hits` HUD、`can_drop` + `empty_insert`、`SnapGrid`、`DragIgnore` / `disabled`。见 `examples/gallery/src/pages/drag.rs`。

---

## 5. 行为差异（不是有无，而是语义不同）

这些对齐错了会在迁移/对照 demo 时「看起来像 bug」。

### 5.1 激活门槛：OR vs AND

solid-dnd pointer sensor（`create-pointer-sensor.ts` 24–25、87–89）：

- `activationDelay = 250`
- `activationDistance = 10`
- **先到任一即 `onActivate`**

RUI 默认 `Activation::And`（须同时满足距离与延迟）。`Activation::Or` 对齐 solid-dnd：先到 delay 或 distance 即激活。默认 `delay_ms = 0` 时 And 日常仍等于「过 4 px 就拖」。

### 5.2 Overlay 缺省幽灵

solid-dnd：不写 children 就没有可见幽灵；常见写法是 overlay 里再画一遍卡片，或 `(d) => <Card id={d.id} />`。  
RUI：空 Overlay **默认克隆本体**（未降透明），源岛用 `source_opacity`。这更贴「拖起一张真卡片」。自定义 `<DragOverlay>…</DragOverlay>` 是 **整段替换**。多形态幽灵走 `.child_for(|item: &DragItemCtx| …)`（只订阅 `open` / `active` / `extent`，不订 `overlay_pos`）。

### 5.3 Overlay 几何

solid-dnd：松手测量 overlay 节点，若幽灵比源小/大，**把 overlay layout 向源框居中**。  
RUI 默认 `OverlayAlign::Grab`（源槽 + 抓取偏移）。`.align(OverlayAlign::Center)` 把幽灵中心对齐源框中心再叠加指针位移（不等大替身不再钉抓点）。

### 5.4 碰撞输入

solid-dnd：draggable 用 **transformed** 布局；droppable 用 **未变换** layout（CHANGELOG / debugger 注释）。  
RUI：碰撞与插入索引用指针空间 layout slot（scroll 已计入，**不含** 让位 translate）。嵌套 `ScrollArea` 下轴锁项仍跟当前槽，而不是按下瞬间的 X（Gallery Saturn 偏移）。这是自绘/滚动世界比 DOM `getBoundingClientRect` 多付的代价，语义正确，但不能拿 DOM 库的「视觉矩形」逐像素对拍。

### 5.5 平局 sticky

两边都是：分数相等时保留 `activeDroppableId`（CHANGELOG 0.5.0：防排序后翻转）。RUI `keep_active_on_tie` 同构。

### 5.6 源项在 Overlay 打开时

solid-dnd：源 transform 清零 → 卡片留在列表原位，作者若要「挖洞」得自己 `opacity: 0`。  
RUI：默认半透明留在 **让位后的空洞**；跨 list 时半透明源可平移到目标空洞。这是 Sortable 体验，不是 solid-dnd 默认。

---

## 6. 相对 solid-dnd 已补的缺口

只列 **曾经缺、现已落地** 的项。RUI 已有而对方没有的（自动滚、Esc、accept…）不算缺口。Extendable §11 见 **[§11](#11-extendable-2026-09-10-已落地)**。

### 6.1 Transformer 管道 — **☑ 2026-09-09**（RUI 名 `DragModifier`）

solid-dnd `Transformer { id, order, callback }`：按 item 注册，按 `order` reduce 到 `{x,y}`。RUI 现为 [`modifier.rs`](../../crates/rui/src/utils/drag/modifier.rs)：每 tick 组装 builtins（axis 10 / snap 20 / pin-locked-axis 25 / restrict 30）再叠 author `.modifier` / `add_modifier`。值类型是 [`DragDelta`](../../crates/rui/src/utils/drag/modifier.rs)（press-delta），**不是** CSS `transform`。`ghost_origin` 不再内联这四段。solid-dnd 另有 `skipTransform`（项上 CSS transform）；两边名字曾经叠在同一个词根上，RUI 故意拆开。

### 6.2 自定义 `CollisionDetector` — **☑ 2026-09-10**

```4:8:Reference/solid-dnd-main/src/collision.ts
type CollisionDetector = (
  draggable: Draggable,
  droppables: Droppable[],
  context: { activeDroppableId: Id | null }
) => Droppable | null;
```

RUI：[`CollisionFn`](../../crates/rui/src/utils/drag/collision.rs) + [`detect_with`](../../crates/rui/src/utils/drag/collision.rs)。`.collision_fn` 优先于四枚举；返回后仍跑 innermost refine（除非 `skip_innermost`）。`can_drop` 继续事后否决。

### 6.3 自定义 Sensor — **☑ 2026-09-10**，见 [§11.1](#111-自定义-sensor-槽--)

对方 `addSensor` + `activators`。RUI：`.sensor` / RSX `sensor` + `sensor_id` / `add_sensor` / [`run_scope_sensors`](../../crates/rui/src/utils/drag/sensor.rs) / [`begin_draggable`](../../crates/rui/src/utils/drag/sensor.rs)。指针仍是默认路径。

### 6.4 `DragDropDebugger` — **☑ 2026-09-10**

[`debug.rs`](../../crates/rui/src/utils/drag/debug.rs)：`set_debug_drag_hits(true)` 或 Provider `.debug_hits(true)` / RSX `debug_hits={true}`。布局 collect 末尾按 `collect_hits`（拖中用 `tick` 缓存）画 dashed 槽；active source / droppable 加粗实线。不订 `overlay_pos`、不重建岛。Gallery 页顶按钮调 `set_debug_drag_hits`（进程级；`.debug_hits(true)` 只开不关）。

### 6.5 项上 `data` 袋 — **☑ 2026-09-10**，见 [§11.2](#112-项上-data-袋)

`.data(Rc<dyn Any>)`；`DragItemCtx::data_as` / `DragEvent.data`；RSX `data={…}`。TLS registry，不进 Sync `Context`。

### 6.6 Overlay render-prop — **☑ 2026-09-10**

`children: JSX | ((activeDraggable) => JSX)`。RUI：`.child_for(|item: &DragItemCtx| …)` 喂源项 id / type / list / index；默认克隆与 `.child()` 仍在。RSX 函数 children 第一刀只走 Builder。

### 6.7 Overlay 尺寸居中 — **☑ 2026-09-10**

`.align(OverlayAlign::Center)`：源框中心 − 幽灵半宽高，再叠加指针位移。默认仍是 `Grab`。

### 6.8 Context 的完整 store — **☑ 2026-09-10**，见 [§11.3](#113-空闲无树根的全表-hits)

`useDragDropContext()[0]` 暴露 store。RUI `DragCtx::hits()`：拖中 layout walk；空闲 [`registered_hits_in_scope`](../../crates/rui/src/utils/drag/registry.rs)。Gallery HUD 订 `active_droppable` 再 peek。

### 6.9 `skipTransform` — **☑ 2026-09-10**，见 [§11.4](#114-skiptransform--skip_shift--skip_follow)

`.skip_shift` / `.skip_follow` + RSX；`apply_live_drag_chrome` / `pack_item_translate` 尊重项级 flag。

### 6.10 水平 Sortable 接线 — **☑ 2026-09-10**

`SortableListBuilder::direction` 写入 list 壳 `rui_drag_axis`。未调用则继承 Provider `axis`（避免破坏现有 `axis=X` 水平列表）。同一 Provider 内可一列 Y、一行 X。

### 6.11 其它小差

| 项 | 说明 |
|----|------|
| `Id` 为 number | RUI 仅 String；不必改 |
| 移除不存在项时警告 | solid-dnd 0.7.1 改 warn；RUI panic/`Option` 路径不统一 |
| 仅主键点击 | solid-dnd 0.7.3 修过右键粘连；RUI `dom_button != 0` 已拒非主键 |

---

## 7. 已落地契约（对齐 Extendable）

按「搬过来就能宣称对齐 / 堵迁移坑」排序。§7.1–7.5 与 §7.7 **已全部落地（2026-09-10）**；§11 Extendable 与 WP-DRAG.3 clone-pull 见 **[§11](#11-extendable-2026-09-10-已落地)**。

### 7.1 `collision_fn`（P1 · **☑ 2026-09-10**）

保留四枚举为内置。`can_drop` 继续做否决；detector 做 **选择**。

```text
pub type CollisionFn = Rc<dyn for<'a> Fn(&CollisionInput<'a>) -> Option<String>>;

pub struct CollisionInput<'a> {
    pub pointer: [f32; 2],
    pub dragged: Rect,           // 幽灵 / 源 layout slot
    pub droppables: &'a [Candidate], // 已 type/group 过滤
    pub active_id: Option<&'a str>,
    pub draggable_id: &'a str,
}

drag_scope("board")
    .collision(CollisionAlg::MostIntersecting) // fn 为 None 时用
    .collision_fn(|input| {
        // 可先调 detect(alg, …) 再改写；或完全手写
        detect(CollisionAlg::PointerWithin, input.pointer, input.dragged, input.droppables, input.active_id)
    })
```

- `collision_fn` 优先于枚举；`None` 走现有 `detect`。
- 函数返回后 **仍跑** `refine_innermost_nested`（除非 input 里带 `skip_innermost: true`，默认不跳）。嵌套列表 P0 不能被自定义 detector 悄悄关掉。
- 库测：`collision_fn` 选出非枚举赢家；与 `can_drop` 叠加时先选后否决。
- **Done**：Builder + `ScopeRuntime` 接线；`session` tick 走 fn；`utils::drag` 单测绿。Gallery「collision_fn · 优先点名」。

### 7.2 Overlay `Fn(&DragItemCtx)` + `align`（P1 · **☑ 2026-09-10**）

默认克隆保留。自定义 child 增加一条不破坏现有 `Fn() -> Element` 的路径：

```text
drag_overlay()
    .align(OverlayAlign::Grab)    // 默认；现行为
    .align(OverlayAlign::Center)  // 源框中心 − 幽灵半宽高
    .child_for(|item: &DragItemCtx| { /* 按 item.item_type / id 分支 */ })
```

- `child_for` 每帧不要重跑：只在 `active_draggable` / `open` / `extent` 变时重建（与现岛订阅契约相同；**禁止**订阅 `overlay_pos`）。
- `Center`：用 `drag_extent`（或克隆后 layout）相对源 `start_bounds` 居中，再叠加指针位移；`Grab` 仍是 `origin = slot + grab_offset`。
- RSX：`align="grab" | "center"`；函数 children 只走 Builder（RSX 闭包喂 `DragItemCtx` 不必第一刀）。
- **Done**：Kanban / 多 type 幽灵不必每个 scope 手写 `use_drag` + HashMap；图标替身 `align=center` 不贴抓点。测：`child_for` 看到的 id 等于 `active_draggable`；center 时幽灵中心对齐源中心（允许 0.5px）。Gallery「DragOverlay · child_for」。

### 7.3 激活 `or` / `and`（P2 · **☑ 2026-09-10**）

```text
pub enum Activation { And, Or }  // 默认 And（现行为）

drag_scope("x").delay_ms(250.0).threshold(10.0).activation(Activation::Or)
```

- `And`：现 `dist < threshold || waited < delay` 未激活。
- `Or`：`dist >= threshold || waited >= delay` 即激活（solid-dnd / 长按拖）。
- 默认保持 `And` + `delay_ms=0`。Gallery「Activation · Or 对 And」用同一 `delay_ms` / `threshold` 对照。
- **Done**：`delay_ms=200, threshold=999, Or` 按住不动会激活；`And` 不会。单测即可。

自定义 Sensor 见 **[§11.1](#111-自定义-sensor-槽--)**（`.sensor` / `run_scope_sensors` / RSX `sensor`）。

### 7.4 `SortableList.direction` 接线（P2 · **☑ 2026-09-10**）

`child` 不再丢弃 `direction`。写入 list 壳 attr（例如 `rui_drag_axis=x|y`），`insert_index_along_axis` / `sortable_shift` 对该 list **覆盖** Provider `axis`（`Xy` 当 `Y`）。

- 同一 Provider 内允许一列 `direction=Y`、一行 `direction=X`。
- 异尺寸项不承诺像素级对齐 solid-dnd（对方自己也不稳）。
- **Done**：水平 list 的 `hover_index` 随指针 X 变；垂直兄弟不受影响。测：两 list 不同 direction。Gallery「SortableList.direction」。

### 7.5 Debugger 矩形（P2 · **☑ 2026-09-10**）

DevTools 或 `debug_hits`：按 `collect_hits` 画 slot 矩形，active droppable 加粗。专门打「幽灵钉在折页下 / 碰撞打到错列」。Gallery 页顶「槽矩形 Debugger」开关。

### 7.6 明确不必搬

- 去掉默认 Overlay 克隆
- 删掉 auto-scroll / Esc / `accept` / `disabled` / `stack`
- 用 DOM `getBoundingClientRect` 语义替换 layout slot
- 把碰撞改回「只看 CSS overlap」
- `Id: string | number`
- SortableProvider `ids` store（`index` + `for_keyed` 更贴细粒度；让位已是 paint-only）
- 多选拖、键盘启动拖拽、ARIA live（对标 dnd-kit / Sortable 插件，不是 0.7.5 核心）

已落地、不必再当作缺口：**§11**（sensor / `data` / 空闲 `hits` / `skip_*`）与 **WP-DRAG.3 clone-pull** — 见 **[§11](#11-extendable-2026-09-10-已落地)**。

### 7.7 `use_drag().hits()`（P3 · **☑ 2026-09-10**）

solid-dnd context store 随时能读 `droppables`。RUI 拖中走 tick 缓存：

```text
let ctx = use_drag()?;
let slots = ctx.hits(); // 本 scope；peek，不订 overlay_pos
```

- 岛要重跑时订 `active_droppable` / `overlay_open`，再 `peek` `hits()`。
- 空闲：`registered_hits_in_scope`（assemble + 末次 layout 矩形）；作者仍可用 `collect_hits(&root)` 做全树 fixture。
- **Done**：`last_hits` / `last_hits_in_scope` + `DragCtx::hits` 双分支；Gallery HUD；单测。

---

## 8. 建议优先级

| 优先级 | 项 | 状态 |
|--------|----|------|
| **P0** | Transformer 管道 | **☑ 2026-09-09** |
| **P0** | 嵌套列表加深 | **☑ 2026-09-09** |
| **P1** | §7.1 `collision_fn` | **☑ 2026-09-10** |
| **P1** | §7.2 Overlay `child_for` + `align` | **☑ 2026-09-10** |
| **P2** | §7.4 `SortableList.direction` | **☑ 2026-09-10** |
| **P2** | §7.3 激活 OR | **☑ 2026-09-10** |
| **P2** | §7.5 Debugger 矩形 | **☑ 2026-09-10** |
| **P3** | §7.7 `use_drag().hits()` | **☑ 2026-09-10** |
| **§11** | sensor / `data` / 空闲 `hits` / `skip_*` | **☑ 2026-09-10** |
| **WP-DRAG.3** | clone-pull | **☑ 2026-09-10** |
| **超出对照源** | 键盘启动 / ARIA / 多选 | —（对标 dnd-kit，非 0.7.5 核心） |

---

## 9. 文件地图（主张 → 源）

### solid-dnd（`Reference/solid-dnd-main`）

| 文件 | 职责 |
|------|------|
| `README.md` 84–100 | 已实现清单；不自动 move；Extendable 三句 |
| `src/index.tsx` | 全部 public export |
| `src/drag-drop-context.tsx` 23–143、147+ | 类型、Provider、transformer、sensor、碰撞调度 |
| `src/create-pointer-sensor.ts` 24–25、42–105 | 250 ms / 10 px OR 激活 |
| `src/collision.ts` | 三算法 + sticky + `CollisionDetector` |
| `src/layout.ts` 167–174 | `layoutContainsPoint`（未进 index） |
| `src/drag-overlay.tsx` | Portal、函数 children、居中 |
| `src/create-sortable.ts` 38–68 | `sortableOffset` |
| `src/sortable-context.tsx` 38–58 | `sortedIds` 视觉重排 |
| `src/drag-drop-debugger.tsx` | layout 高亮 |
| `CHANGELOG.md` | 水平 sort、disabled 删除、平局、右键 |

### RUI

| 文件 | 职责 |
|------|------|
| `crates/rui/src/utils/drag/mod.rs` | 模块头：三血统 + 北极星 |
| `builder.rs` | `DragCtx` / `DragItemCtx`、builders、`clone_source_preview`、`drag_ignore`、`.modifier` |
| `session.rs` | Pending/Active、`Activation` And/Or、`cancel` / `nudge_insert`、`add_modifier`、`collect_hits` / `last_hits` |
| `modifier.rs` | `DragModifier` / `DragDelta` / builtins / `apply_modifiers` |
| `collision.rs` | 四算法 + sticky + innermost + `CollisionFn` / `detect_with` |
| `overlay.rs` | Pinned ghost；pos peek 跟手；`child_for` / `align` |
| `sortable.rs` | 让位 / transfer；`direction` 写 `rui_drag_axis` |
| `autoscroll.rs` | 边缘滚 + `ScrollLock` |
| `geometry.rs` | `Axis` / `Restrict` / `SnapGrid` / `DragStack` / `contains_rect` |
| `debug.rs` | `DragDropDebugger`：槽矩形 overlay |
| `crates/rui_macros/src/lib.rs` ~2092+ | `<DragProvider>` RSX |
| `examples/gallery/src/pages/drag.rs` | `DragShowcase` |
| [drag.md](../components/drag.md) | 产品手册 |

---

## 10. 总表（给排期用）

| | 数量感（定性） |
|--|----------------|
| **已对齐的 solid-dnd 原语** | Provider、拖/放、Overlay、`child_for` / `align`、碰撞+sticky、自定义 detector、Sortable 预览、不改 Vec、Handle、四事件、`DragModifier`、激活 Or、list `direction`、Debugger、`hits()`、**§11 Extendable**、clone-pull |
| **RUI 超出 solid-dnd** | PointerWithin、accept/group、disabled、ignore、axis/restrict/snap、empty_insert、can_drop、自动滚、FLIP、Esc/方向键、默认克隆、source_opacity、stack、双写、测试、Gallery、嵌套 innermost、`run_scope_sensors` |
| **相对缺失（挡「Extendable 对齐」）** | 无 |
| **双方都没有、对标 dnd-kit / Sortable 插件** | 多选、ARIA live |
| **interact.js 血统** | 专章 **[INTERACT_JS_COMPARISON.md](INTERACT_JS_COMPARISON.md)**（**已冻结**） |
| **冻结** | §7 + §11 ☑ · `utils::drag` 162 passed · **2026-09-10** |

一句话收束：**模型已经是 solid-dnd 的；产品能力已经超过 0.7.5。§7 Extendable + §11 + WP-DRAG.3 已全部落地并验证。本专章 2026-09-10 冻结。**

---

## 11. Extendable（**☑ 2026-09-10 已落地**）

API 见 [`drag.md`](../components/drag.md) §8 与 `utils/drag` 导出。

| 项 | API | 源文件 |
|----|-----|--------|
| **11.1 自定义 Sensor** | `.sensor` / RSX `sensor` + `sensor_id` / `add_sensor` / [`run_scope_sensors`](../../crates/rui/src/utils/drag/sensor.rs) / [`begin_draggable`](../../crates/rui/src/utils/drag/sensor.rs) | [`sensor.rs`](../../crates/rui/src/utils/drag/sensor.rs) |
| **11.2 `data` 袋** | `DraggableBuilder::data` → `DragItemCtx::data()` / `DragEvent.data` | [`registry.rs`](../../crates/rui/src/utils/drag/registry.rs) |
| **11.3 空闲全表 `hits`** | `DragCtx::hits()` 空闲走 `registered_hits_in_scope`；`collect_hits` 同步 registry | [`registry.rs`](../../crates/rui/src/utils/drag/registry.rs) |
| **11.4 skipTransform** | `.skip_shift` / `.skip_follow`（**不叫** `skipTransform`） | [`builder.rs`](../../crates/rui/src/utils/drag/builder.rs) |
| **WP-DRAG.3 clone-pull** | `.pull_clone(true)` + `apply_clone_event` / `DragEvent.pull_clone` | [`sortable.rs`](../../crates/rui/src/utils/drag/sortable.rs) |

### 11.1 自定义 Sensor 槽 — **☑**

| | |
|--|--|
| **对方** | `addSensor` + `activators`。仓内只有 `createPointerSensor`。 |
| **RUI 现在** | `DragScopeBuilder::sensor(id, activator)` + 运行时 `add_sensor` / `remove_sensor`。指针仍是默认路径。 |
| **顶什么** | 键盘启动、触底长按、自定义手势。 |
| **Done** | 槽 + TLS 合并；[`run_scope_sensors`](../../crates/rui/src/utils/drag/sensor.rs) / [`begin_draggable`](../../crates/rui/src/utils/drag/sensor.rs)；Gallery sensor 按钮 |

### 11.2 项上 `data` 袋

| | |
|--|--|
| **对方** | `createDraggable(id, { type, column, … })`，碰撞 / Overlay / `onDragEnd` 读同一份 `Record<string, any>`。 |
| **RUI 现在** | `.data(Rc<dyn Any>)`；读 `DragItemCtx::data()` / `data_as::<T>()` / `DragEvent.data`。 |
| **顶什么** | 共享非 type/list/index 载荷。 |
| **Done** | TLS registry；不进 Context（避免 Sync 约束）。 |

### 11.3 空闲、无树根的全表 `hits`

| | |
|--|--|
| **对方** | context store 随时能读全部 `droppables`（空闲也在）。 |
| **RUI 现在** | 拖中：`use_drag().hits()` tick 缓存。空闲：`registered_hits_in_scope`（mount + 末次 `collect_hits` 矩形）。 |
| **顶什么** | 未按下遍历 droppable / HUD / fixture。 |
| **Done** | `register_slot_meta` + `sync_registered_from_collect`；`DragCtx::hits()` 自动分支。 |

### 11.4 `skipTransform` → `skip_shift` / `skip_follow`

| | |
|--|--|
| **对方** | directive `skipTransform`：作者自己写 **CSS** `transform`（sortable 内部用这条避开套件位移）。 |
| **RUI 现在** | `.skip_shift(true)` / `.skip_follow(true)` + attrs `rui_drag_skip_*`。 |
| **顶什么** | 作者自管 translate 与套件让位/跟手打架。 |
| **Done** | `apply_live_drag_chrome` + `pack_item_translate` 尊重项级 flag。 |
