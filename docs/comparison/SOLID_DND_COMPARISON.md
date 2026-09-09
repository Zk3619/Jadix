# RUI × solid-dnd 拖拽能力对照

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) **WP-DRAG.\*** 为准；产品手册见 [drag.md](../components/drag.md)；浮层几何见 [overlay_shells.md](../components/overlay_shells.md) §9。  
> **对照源**：`Reference/solid-dnd-main`（[@thisbeyond/solid-dnd](https://github.com/thisbeyond/solid-dnd) **0.7.5**）· RUI 以 `crates/rui/src/utils/drag` 为准。  
> **日期**：2026-09-08（§8 P0 落地 2026-09-09）。  
> **范围**：本专章只对照 **solid-dnd**。SortableJS / interact.js 是 RUI 套件的另外两条血统，只在「RUI 多出来的能力」里点到，不另开专章。

**明细入口**：有无不要只看 §0 结论——直接翻 **[§4 能力矩阵](#4-能力矩阵)**、**[§5 行为差异](#5-行为差异不是有无而是语义不同)**、**[§6 缺失](#6-相对-solid-dnd-的缺失)**、**[§7 值得参考](#7-值得参考)**、**[§8 建议优先级](#8-建议优先级不挡-beta)**。

---

## 0. 一句话结论

RUI 的 `utils/drag` **已经吃透 solid-dnd 的原语模型**（Provider / Draggable / Droppable / Overlay / 碰撞 + 平局 sticky / **不自动改 `Vec`**），并在其上叠了 SortableJS（自动滚、让位、group、handle）和 interact.js（轴锁、钳位、网格吸附）。相对 0.7.5 的 solid-dnd，**产品可用面更宽**；差距不在「会不会拖」，而在剩余 **插件缝**（自定义 sensor / `CollisionDetector` 函数 / Debugger / 项上 `data` 袋）。**位移修饰管道（RUI `DragModifier`）与嵌套列表已按 §8 P0 落地。**

| 轴 | solid-dnd 0.7.5 | RUI `utils/drag` |
|----|-----------------|------------------|
| **命题** | Solid 细粒度 + dnd-kit 精神的无头 toolkit | 同一套原语，落到自绘桌面 + RSX/Builder 双写 |
| **默认路径** | 指针 sensor + 三碰撞算法 + 可选 Overlay + 垂直 Sortable | 指针会话 + **四**碰撞算法 + Pinned Overlay + 可水平轴的 Sortable 让位 |
| **落点** | 调用方自己改数据 | 同样；另给 `apply_reorder_vec` / `apply_transfer` |
| **扩展哲学** | sensor / collision / transformer **都是插件点** | `DragModifier` **已是管道**（避开 CSS `transform` 同名）；collision / sensor 仍烘焙或仅枚举 |
| **成熟度** | 已发布 npm；无仓内测试树 | Experimental，不进 `prelude` / SemVer 0.1；库测覆盖 collision / session / overlay / RSX |

**不该做的事**：为对齐 solid-dnd 拆掉已落地的自动滚 / Esc / 让位；把 Overlay 改回「必须手写幽灵子树」；把 `CollisionAlg` 枚举删成只能自定义函数。缺的是 **插件缝**，不是换模型。

---

## 1. 定位与体量

| 项 | solid-dnd | RUI |
|----|-----------|-----|
| **一句话** | A lightweight drag and drop toolkit for Solid | Headless drag kit：solid-dnd 原语 + 超越 Sortable |
| **版本** | `@thisbeyond/solid-dnd` 0.7.5（peer `solid-js ^1.5`） | 工作区 `utils/drag`；WP-DRAG.1–2 ☑（2026-09-07） |
| **灵感** | [dnd-kit](https://dndkit.com)（README 自述） | solid-dnd + SortableJS + interact.js（`mod.rs` 模块头） |
| **源码规模** | `src/` 约 15 个文件；零运行时依赖（除 Solid） | `drag/{mod,builder,session,modifier,collision,geometry,overlay,sortable,autoscroll}.rs` + RSX 宏 |
| **文档** | README + CHANGELOG；无独立 docs 树 | [drag.md](../components/drag.md) + Gallery `DragShowcase` |
| **测试** | 参考树内 **无** `*.test.*` | collision / geometry / session / autoscroll / overlay / sortable / `tests/drag_rsx.rs` |
| **北极星** | 细粒度、零 re-render、CSS transform | **超越 SortableJS**（~40k★）；键盘一等；不进 prelude |

solid-dnd README「What's implemented」六条（`createDraggable` / `createDroppable` / `DragOverlay` / sensors / 三碰撞 / sortable + 多 Provider）在 RUI 都有对应物；RUI 额外宣称的自动滚、Esc、方向键、FLIP 时长 **不在** solid-dnd 的已实现清单里。

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
  CollisionDetector (fn)                 CollisionAlg (enum) + detect()
  Transformer 管道                       modifier.rs：`DragModifier` + `.modifier` / `add_modifier`
  DragDropDebugger                       （无）
```

### 2.1 会话

| 步骤 | solid-dnd | RUI |
|------|-----------|-----|
| 按下 | `createPointerSensor`：`button === 0`，绑 `pointermove/up` | `begin` → `DragPhase::Pending` |
| 激活 | **250 ms 或 10 px，先到者激活**（硬编码） | **距离 ≥ `threshold` 且 等待 ≥ `delay_ms`**（默认 4 px / 0 ms） |
| 位移 | `sensorMove` transformer（order 0）叠 `delta` | `tick`：`apply_modifiers`（axis → snap → pin → restrict → author）→ `overlay_pos` |
| 碰撞 | `detectCollisions()`：对 **transformed** overlay/draggable vs **未变换** droppable | 同思路：指针空间 **layout slot**（已扣 scroll / 不含 CSS translate） |
| 结束 | `dragEnd` → `onDragEnd(previous 快照)` | `end` → `on_drag_end`；**Esc → `cancel`（不提交）** |

两侧都 **不** 在 drop 时改调用方数组。Sortable 拖中只做视觉预览；持久化在 `onDragEnd`。

### 2.2 Overlay / 源项

| | solid-dnd `drag-overlay.tsx` | RUI `overlay.rs` |
|--|------------------------------|------------------|
| 挂载 | `Portal` → `document.body`；`position: fixed` | `portal_sync(Pinned)` + Unanchored `fixed` inset（避免 Point 菜单钳 Y） |
| 默认幽灵 | **必须**给 children；静态 JSX 或 `(activeDraggable) => JSX` | **空 `<DragOverlay />` 重跑源项 author body**（`clone_source_preview`） |
| 源项位移 | 有 overlay 时源 `transform` = noop（源留在原位） | 源不跟手；`source_opacity` 默认 0.4；0 则留空槽 |
| 尺寸 | overlay 实测后 **相对源框居中** | grab-offset；`w/h=100%` 的克隆钉 `drag_extent` |
| Esc 栈 | DOM overlay，与菜单栈无关 | Pinned，**不进** Esc overlay stack |

### 2.3 Sortable

| | solid-dnd | RUI |
|--|-----------|-----|
| 组合 | `createSortable` = draggable + droppable + `sortableOffset` transformer | `<SortableItem>` = `draggable.also_droppable(true)` |
| 顺序源 | `SortableProvider ids={...}`；拖中 `moveArrayItem` 改 **内部** `sortedIds` | 项上 `list` + `index`；`hover_index` 驱动让位 |
| 视觉 | 比较 initial vs sorted，把项 translate 到目标 slot 的 layout 差 | `sortable_shift`：兄弟 CSS `translate` FLIP（[`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150 ms） |
| 提交 | 松手 `sortedIds` 重置为 `props.ids`；用户改数据后再传入 | `apply_reorder_vec` / `apply_transfer` / `apply_event` |
| 方向 | README：**仅宣称垂直**；CHANGELOG 0.6.0 算法已支持水平（同尺寸才正常） | Provider `axis`；`SortableList.direction` **目前丢弃**（`let _ = self.direction`） |

---

## 3. 公开 API 映射

| solid-dnd | RUI | 备注 |
|-----------|-----|------|
| `DragDropProvider` | `drag_scope` / `<DragProvider>` | 两侧都隔离 context |
| `useDragDropContext` → `[Store, Actions]` | `use_drag` → `DragCtx` | RUI 更瘦：无 draggables/droppables 全表、无 modifier/sensor 动作 |
| `DragDropSensors` / `createPointerSensor` | 内建 `pointer_drag` | RUI **无**公开 sensor 插件 |
| `createDraggable(id, data?)` | `draggable(id)` + `item_type` / `group` / `list` / `index` | `data` 袋 vs 一等字段 |
| `createDroppable(id, data?)` | `droppable(id)` + `accept` / `group` / `list` | RUI 内建 type 过滤 |
| `createSortable` | `sortable_item` / `<SortableItem>` | |
| `SortableProvider` / `useSortableContext` | `sortable_list` / `<SortableList>` | 无独立 ids store |
| `DragOverlay` | `drag_overlay` / `<DragOverlay>` | 见 §2.2 |
| `closestCenter` / `closestCorners` / `mostIntersecting` | `CollisionAlg::{ClosestCenter, ClosestCorners, MostIntersecting}` | 默认都是 most-intersecting |
| （`layoutContainsPoint`，**未进 public index**） | `CollisionAlg::PointerWithin` | RUI 升成一等算法 |
| `layoutStyle` / `transformStyle` | 调用方 utility / `reactive_sync` | 无独立 style helper 导出 |
| `DragDropDebugger` | — | 缺失 |
| `Id = string \| number` | `String` | |
| `Transformer` | [`modifier.rs`](../../crates/rui/src/utils/drag/modifier.rs) `DragModifier` / `DragDelta` | 见 §6.1；避开 CSS `transform` |
| `dragActivators` 挂子节点 = handle | `<DragHandle>` + `handle_only` | RUI 另有 `drag_ignore` |
| `isActiveDraggable` / `isActiveDroppable` | `use_drag()` signals + `rui_drag_over` | 高亮仍归调用方 |

事件：`onDragStart` / `onDragMove` / `onDragOver` / `onDragEnd` 两侧都有。RUI 另有 `can_drop`（Sortable `onMove` 否决）。

---

## 4. 能力矩阵

图例：**有** = 一等 API；**超** = RUI 有而 solid-dnd 无或明显更深；**差** = 弱于 / 仅部分；**无** = 没有。

| 能力 | solid-dnd | RUI | 判定 |
|------|-----------|-----|------|
| Provider / 隔离多实例 | 有 | 有 | 对齐 |
| Draggable / Droppable | 有 | 有 | 对齐 |
| 不自动移动数据 | 有 | 有 | 对齐 |
| Overlay 幽灵 | 有（手写子树 / render-prop） | 有（**默认克隆** + 可整段替换） | **超**（DX）/ **差**（无 `(active) =>` 形参） |
| mostIntersecting / closestCenter / closestCorners | 有 + sticky 平局 | 有 + sticky 平局 | 对齐 |
| pointer-within / 点在区内 | 内部 `layoutContainsPoint`，未导出 | **一等** `PointerWithin` | **超** |
| 自定义 `CollisionDetector` 函数 | 有 | **无**（仅枚举） | **无** |
| Transformer / 可组合修饰 | 有 | 有（`DragModifier` builtins + author） | **对齐**（RUI 改名） |
| 自定义 Sensor | 有（`addSensor`） | 无 | **无** |
| Handle | `dragActivators` | `<DragHandle>` + `handle_only` | 对齐 |
| 忽略内部控件 | 无一等 | `drag_ignore` | **超** |
| disabled | 0.2.0 **已删** | `disabled` attr | **超** |
| type / accept / group | 靠 `data` + 自定义 detector | 一等 `item_type` / `accept` / `group` | **超** |
| 激活 delay / 距离 | 硬编码 250 ms **或** 10 px | 可配；**且**（默认 4 px / 0 ms） | **超**（可配）/ **差**（语义是 AND） |
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
| 嵌套列表 | 无一等 | innermost refine + Gallery ☑ P0 | **超**（产品浅，引擎已加深） |
| clone-pull | 无一等 | **WP-DRAG.3** ☐ | 双方浅 |
| Overlay 源自动隐藏 | 否（作者自管） | `source_opacity` | **超** |
| 任意移动 + 叠放 | 作者 `z-index` | `stack` auto/top/bottom/keep | **超** |
| 水平 Sortable | 文档滞后；同尺寸才稳 | Provider `axis`；list `direction` **未接线** | **差** |
| Debugger | `DragDropDebugger` | 无 | **无** |
| 项上自由 `data` | `Record<string, any>` | 无袋 | **无** |
| RSX 双写 | — | `<DragProvider>` 等 ADR-9 | **超** |
| 库测 | 无 | 有 | **超** |

Gallery 已演示：park 投放、自定义 Overlay 子树、四角旋转、accept 条件、Handle、restrict=viewport、stack 叠放、source_opacity 0/0.4、FLIP 开关、Kanban `group` + `apply_transfer`、ScrollArea 内自动滚。见 `examples/gallery/src/pages/drag.rs`。

---

## 5. 行为差异（不是有无，而是语义不同）

这些对齐错了会在迁移/对照 demo 时「看起来像 bug」。

### 5.1 激活门槛：OR vs AND

solid-dnd pointer sensor（`create-pointer-sensor.ts` 24–25、87–89）：

- `activationDelay = 250`
- `activationDistance = 10`
- **先到任一即 `onActivate`**

RUI `session.rs` `tick`（约 685–690 行）：

```text
if dist < threshold || waited_ms < delay_ms { return }  // 尚未激活
```

即必须 **同时** 满足距离与延迟。默认 `delay_ms = 0`，所以日常等于「过 4 px 就拖」；一旦把 delay 调成 Sortable 式 200 ms，**按住不动不会激活**（solid-dnd 会）。若要对齐 Sortable `delay` 的「长按即拖」，需要改成 OR，或单独提供 `delay_or_distance`。

### 5.2 Overlay 缺省幽灵

solid-dnd：不写 children 就没有可见幽灵；常见写法是 overlay 里再画一遍卡片，或 `(d) => <Card id={d.id} />`。  
RUI：空 Overlay **默认克隆本体**（未降透明），源岛用 `source_opacity`。这更贴「拖起一张真卡片」。自定义 `<DragOverlay>…</DragOverlay>` 是 **整段替换**，**收不到**当前 `active_draggable` 作为闭包参数——要按 id 分支得自己 `use_drag()`。

### 5.3 Overlay 几何

solid-dnd：松手测量 overlay 节点，若幽灵比源小/大，**把 overlay layout 向源框居中**。  
RUI：幽灵原点 = 源槽 + **抓取偏移**（指针在卡片内的位置）。自定义更小的替身不会自动居中到卡片中心，而是钉在抓点。Gallery「自定义替身」走这条。

### 5.4 碰撞输入

solid-dnd：draggable 用 **transformed** 布局；droppable 用 **未变换** layout（CHANGELOG / debugger 注释）。  
RUI：碰撞与插入索引用指针空间 layout slot（scroll 已计入，**不含** 让位 translate）。嵌套 `ScrollArea` 下轴锁项仍跟当前槽，而不是按下瞬间的 X（Gallery Saturn 偏移）。这是自绘/滚动世界比 DOM `getBoundingClientRect` 多付的代价，语义正确，但不能拿 DOM 库的「视觉矩形」逐像素对拍。

### 5.5 平局 sticky

两边都是：分数相等时保留 `activeDroppableId`（CHANGELOG 0.5.0：防排序后翻转）。RUI `keep_active_on_tie` 同构。

### 5.6 源项在 Overlay 打开时

solid-dnd：源 transform 清零 → 卡片留在列表原位，作者若要「挖洞」得自己 `opacity: 0`。  
RUI：默认半透明留在 **让位后的空洞**；跨 list 时半透明源可平移到目标空洞。这是 Sortable 体验，不是 solid-dnd 默认。

---

## 6. 相对 solid-dnd 的缺失

只列 **solid-dnd 有、RUI 没有或明显更弱** 的项。RUI 已有而对方没有的（自动滚、Esc、accept…）不算缺失。

### 6.1 Transformer 管道 — **☑ 2026-09-09**（RUI 名 `DragModifier`）

solid-dnd `Transformer { id, order, callback }`：按 item 注册，按 `order` reduce 到 `{x,y}`。RUI 现为 [`modifier.rs`](../../crates/rui/src/utils/drag/modifier.rs)：每 tick 组装 builtins（axis 10 / snap 20 / pin-locked-axis 25 / restrict 30）再叠 author `.modifier` / `add_modifier`。值类型是 [`DragDelta`](../../crates/rui/src/utils/drag/modifier.rs)（press-delta），**不是** CSS `transform`。`ghost_origin` 不再内联这四段。solid-dnd 另有 `skipTransform`（项上 CSS transform）；两边名字曾经叠在同一个词根上，RUI 故意拆开。

### 6.2 自定义 `CollisionDetector`

```4:8:Reference/solid-dnd-main/src/collision.ts
type CollisionDetector = (
  draggable: Draggable,
  droppables: Droppable[],
  context: { activeDroppableId: Id | null }
) => Droppable | null;
```

可在函数里读 `data`、做 type 过滤、嵌套时手写策略。RUI 只有 `CollisionAlg` 四枚举 + `can_drop` 事后否决。`can_drop` 能拒绝，**不能**在多个候选里换算法或按 payload 重排。

### 6.3 自定义 Sensor

`addSensor` + `activators`（目前只有 pointer）。键盘、触底长按、自定义手势都应走这条，而不是再开一套 session。RUI 指针路径焊在 `pointer_drag` / `window_events`。

### 6.4 `DragDropDebugger`

把每个 draggable/droppable 的 layout 画成 dashed overlay，拖中 `recomputeLayouts`。自绘引擎没有 DOM inspector 时，这种 **碰撞矩形可视化** 比再写一篇手册更能抓「scroll 空间 vs 布局空间」类 bug。

### 6.5 项上 `data` 袋

`createDraggable(id, { type: "card", column: "todo" })`。碰撞、overlay render-prop、`onDragEnd` 都读同一份。RUI 把 type/group/list/index 做成 attr，**没有**开放 `HashMap`/`Any`。复杂看板（优先级、禁用规则、来源列元数据）只能闭包捕获或外置 map。

### 6.6 Overlay render-prop

`children: JSX | ((activeDraggable) => JSX)`。多类型源共用一个 Overlay 时，solid-dnd 用函数子树分支。RUI 默认克隆已覆盖「长得像源」；**多形态幽灵**仍要 `use_drag().active_draggable` 自己分支，API 没把「当前源」喂给 Overlay 子树。

### 6.7 Overlay 尺寸居中

自定义幽灵与源不等大时，solid-dnd microtask 后按实测尺寸居中。RUI 钉 grab-offset。图标型幽灵会贴在抓点而不是卡片中心。

### 6.8 Context 的完整 store

`useDragDropContext()[0]` 暴露 `draggables` / `droppables` / `active.*` / overlay。RUI `DragCtx` 只有 id 与一组 Signal（active/from/hover/overlay_pos…），**没有**「列出本 scope 全部 droppable 矩形」的公共 API（内部 `collect_hits` 有，未稳定导出给作者做自定义 UI）。

### 6.9 `skipTransform`

solid-dnd directive 可 `skipTransform`，让作者自己写 **CSS** transform（sortable 内部就这么干）。RUI 跟手 / 让位 translate 由套件写死。这和 §6.1 的 `DragModifier`（改 press-delta）不是同一件事——solid-dnd 把两套都挂在 transform 词根上。

### 6.10 水平 Sortable 接线

`SortableListBuilder::direction` 存在但 `child` 里丢弃。水平重排目前只能靠 Provider `axis="x"` 碰运气，缺少 list 级方向与「异尺寸项」策略（solid-dnd 自己也承认异尺寸视觉差）。

### 6.11 其它小差

| 项 | 说明 |
|----|------|
| `Id` 为 number | RUI 仅 String |
| 移除不存在项时警告 | solid-dnd 0.7.1 改 warn；RUI panic/`Option` 路径不统一 |
| `recomputeLayouts` 公开 | 拖开始/结束强制刷新布局；RUI 靠每帧 hit 收集 |
| 仅主键点击 | solid-dnd 0.7.3 修过右键粘连；RUI 应核对 `dom_button` 是否同等 |

---

## 7. 值得参考

按「搬过来的杠杆」排序，不是按 solid-dnd 文件名。

### 7.1 高杠杆（建议进 WP-DRAG.3 或其后）

1. **Transformer 管道** — **☑ 2026-09-09**（RUI `DragModifier` / `DragDelta`）  
   `sensor delta → axis → snap → pin → restrict → author`。`.modifier` / `add_modifier`（Provider 重建后仍在）。不写 CSS `transform`。

2. **`CollisionDetector` 函数类型**  
   保留现有四枚举为内置实现，再允许 `drag_scope().collision_fn(|…|)`。`can_drop` 继续做否决；detector 做 **选择**。这是 solid-dnd README「Extendable」的核心句。

3. **Overlay 子树拿到 `active` id / `DragItemCtx`**  
   默认克隆保留；自定义 child 若是 `Fn(&DragItemCtx) -> Element`，Kanban / 多 type 幽灵不必每个 scope 手写 `use_drag`。对齐 `drag-overlay.tsx` 的函数 children。

4. **项 `data`（或 typed payload）**  
   不必上 `Any`。哪怕 `on_drag_end` 能带回 `item_type` 之外的一小袋字符串，条件投放就不用并行维护两套 id 表。

### 7.2 中杠杆（体验 / 排障）

5. **`DragDropDebugger` 的自绘版**  
   DevTools 里画 droppable/draggable slot 矩形 + active 加粗。专门打「幽灵钉在折页下 / 碰撞打到错列」这类 bug。

6. **激活策略可切换 OR / AND**  
   文档写清；提供 `activation="or"` 以对齐 solid-dnd / 长按拖。默认可保持现在的 AND + `delay_ms=0`。

7. **自定义 Overlay 居中策略**  
   `overlay_align="grab" | "center"`。center 对齐 solid-dnd microtask 居中。

8. **把 `collect_hits` / 当前候选做成只读 API**  
   作者才能写「自定义指示器 / 磁力线」而不复制 session。

### 7.3 低杠杆（可抄精神、不必 1:1）

9. **Sensor 插件** — 在键盘启动拖拽 / 触控长按真正要做之前，不必先做抽象。现在指针焊死是合理的。  
10. **`skipTransform`** — 自绘没有 DOM style 争夺，优先级低于管道。  
11. **`Id: string | number`** — 现有 String 足够。  
12. **SortableProvider `ids` store** — RUI 的 `index` attr + `for_keyed` 更贴细粒度列表；不必改成 ids 数组驱动，除非要做「拖中不改作者 Vec、只改内部 sortedIds」的纯预览模式（现在让位已是 paint-only）。

### 7.4 明确不必从 solid-dnd 搬回来的

- 去掉默认 Overlay 克隆（DX 更差）
- 删掉 auto-scroll / Esc / `accept` / `disabled`（这是 RUI 相对 Sortable 的超集）
- 用 DOM `getBoundingClientRect` 语义替换 layout slot（在 `ScrollArea` 里会退步）
- 把碰撞改回「只看 CSS overlap」（手册已否决）

---

## 8. 建议优先级（不挡 Beta）

对照审计，**不改** [AUTHORITY.md](../AUTHORITY.md) 勾选。现有 **WP-DRAG.3** 是「FLIP 时长 ☑ / **嵌套列表加深 ☑ P0** / clone-pull」。建议把「solid-dnd 扩展缝」视为 DRAG.3 的设计输入，而不是另起挡发布的 WP。

| 优先级 | 项 | 理由 | 与 WP-DRAG.3 |
|--------|----|------|----------------|
| **P0** | Transformer 管道（哪怕先内置 3 段、后允许注册） | 打开所有「随拖改位移」而不膨胀 session | **☑ 2026-09-09** `modifier.rs`（公开名 `DragModifier`，避开 CSS `transform`） |
| **P0** | 嵌套列表加深 | 已在 DRAG.3；solid-dnd 也只靠 nested droppable + `closestCorners` | **☑ 2026-09-09** innermost refine + Gallery；**09-09 晚** 同型 accept 默认 + Overlay 克隆不抢内层 island |
| **P1** | `CollisionDetector` 函数 + 保留枚举 | README 级扩展点；`can_drop` 不够选 | 新 |
| **P1** | clone-pull | DRAG.3；solid-dnd Overlay 本来就是「拉出的表示」 | 已列 |
| **P1** | Overlay `Fn(&DragItemCtx)` + `align=center` | 补齐对方 Overlay 的两处细节 | 新 |
| **P2** | 水平 `SortableList.direction` 真正接线 | API 已撒谎 | 小修复 |
| **P2** | 激活 OR 模式 | 长按拖 / 对照 demo | 新 |
| **P2** | Debugger 矩形 | 排障 | 新 |
| **P3** | `data` 袋 / 只读 hits API | 复杂看板 | 新 |
| **P3** | 键盘启动拖拽 / ARIA | 双方都空；真 a11y 对标 dnd-kit 而非 solid-dnd | 超出对照源 |
| **不做** | 多选拖、自定义 sensor 框架（直到有第二传感器） | 对方也没有多选；sensor 抽象过早 | — |

---

## 9. 文件地图（主张 → 源）

### solid-dnd（`Reference/solid-dnd-main`）

| 文件 | 职责 |
|------|------|
| `README.md` 84–100 | 已实现清单；不自动 move |
| `src/index.tsx` | 全部 public export |
| `src/drag-drop-context.tsx` 23–143、147+ | 类型、Provider、transformer、sensor、碰撞调度 |
| `src/create-pointer-sensor.ts` 24–25、42–105 | 250 ms / 10 px OR 激活 |
| `src/collision.ts` | 三算法 + sticky |
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
| `builder.rs` | `DragCtx`、builders、`clone_source_preview`、`drag_ignore`、`.modifier` |
| `session.rs` | Pending/Active、AND 激活、`cancel` / `nudge_insert`、`add_modifier` |
| `modifier.rs` | `DragModifier` / `DragDelta` / builtins / `apply_modifiers` |
| `collision.rs` | 四算法 + sticky + innermost nested refine |
| `overlay.rs` | Pinned ghost |
| `sortable.rs` | 让位 / transfer；**`direction` 未使用** |
| `autoscroll.rs` | 边缘滚 + `ScrollLock` |
| `geometry.rs` | `Axis` / `Restrict` / `SnapGrid` / `DragStack` / `contains_rect` |
| `crates/rui_macros/src/lib.rs` ~2092+ | `<DragProvider>` RSX |
| `examples/gallery/src/pages/drag.rs` | `DragShowcase` |
| [drag.md](../components/drag.md) | 产品手册 |

---

## 10. 总表（给排期用）

| | 数量感（定性） |
|--|----------------|
| **已对齐的 solid-dnd 原语** | Provider、拖/放、Overlay、三碰撞+sticky、Sortable 视觉预览、不改 Vec、Handle、多 Provider、四事件、Transformer 管道（RUI 名 `DragModifier`） |
| **RUI 超出 solid-dnd** | PointerWithin、accept/group、disabled、ignore、可配 delay/threshold、axis/restrict/snap、empty_insert、can_drop、自动滚、FLIP、Esc/方向键、默认克隆、source_opacity、stack、双写、测试、Gallery、嵌套 innermost |
| **相对缺失** | 自定义 detector、自定义 sensor、Debugger、data 袋、Overlay render-prop/居中、完整 store、skipTransform、list.direction 接线、clone-pull |
| **双方都没有、对标时应看 dnd-kit / Sortable 插件** | 键盘启动、多选、ARIA、clone 插件 |

一句话收束：**模型已经是 solid-dnd 的；P0 插件缝（`DragModifier` + 嵌套列表）已落地。产品能力已经超过 0.7.5。下一步是自定义碰撞函数 / Overlay render-prop，不是再堆一条 Gallery demo。**
