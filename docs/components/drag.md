# Drag (`utils/drag`)

> **Status**: **WP-DRAG.1 ☑** + **WP-DRAG.2 ☑** + **WP-DRAG.3 ☑** + **WP-DRAG.4 ☑**（Swap live / MultiDrag）+ P0 modifier / nested lists + **§7.1–7.5 / §7.7** + **§11 Extendable**（sensor / `data` / 空闲 `hits` / `skip_*` / clone-pull）+ **interact 修饰超越** + **指针动作**（resize / gesture / inertia / 多指 → [`pointer_action`](../../crates/rui/src/utils/pointer_action/mod.rs)）。Experimental — not in `prelude`, not SemVer 0.1. 相对 solid-dnd 0.7.5 **产品面已超**；Extendable 契约见 [SOLID_DND_COMPARISON.md](../comparison/SOLID_DND_COMPARISON.md) **§11** 与下文 **§8**。interact.js：[INTERACT_JS_COMPARISON.md](../comparison/INTERACT_JS_COMPARISON.md)。SortableJS 列表血统：[SORTABLE_JS_COMPARISON.md](../comparison/SORTABLE_JS_COMPARISON.md)（**2026-09-10 冻结**列表核心；Swap / MultiDrag 同日落地）。  
> **Parent**: [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2.1, [overlay_shells.md](overlay_shells.md) §9, [headless.md](headless.md). 分层 / 可选 crate：[pointer_and_drag_architecture.md](../engine/pointer_and_drag_architecture.md)。  
> **References**: `reference/solid-dnd-main`, `reference/Sortable-master`, `reference/interact.js-main`。对照专章（均 **2026-09-10 冻结**）：[SOLID_DND_COMPARISON.md](../comparison/SOLID_DND_COMPARISON.md)、[SORTABLE_JS_COMPARISON.md](../comparison/SORTABLE_JS_COMPARISON.md)、[INTERACT_JS_COMPARISON.md](../comparison/INTERACT_JS_COMPARISON.md)。  
> **North star**: **surpass SortableJS** (~40k★) — not clone it. Sortable is a DOM list plugin; this kit is a deep module: collision, overlay, auto-scroll, keyboard, and dual-write sit behind a small surface. Product chrome stays in the caller.

```
Author / Gallery
        │
        ▼
┌─────────────────────────────────────────┐
│  utils/drag  (this feature)             │
│  DragProvider · Draggable · Droppable   │
│  DragOverlay · DragHandle · Sortable*   │
└─────────────────────────────────────────┘
        │
        ├── utils/pointer_drag     session kind `DragKind::Scope`（按 pointer id 可并发）
        ├── utils/pointer_action   resize / 双指 gesture / inertia / 多指配额
        ├── utils/anchored_overlay Point ghost + `OverlayMode::Pinned`
        ├── utils/drag_reorder     `apply_reorder_vec`
        └── app/window_events      tick / end / auto-scroll pump
```

`utils/drag_reorder` remains the **legacy** Grid helper. New UIs should use this module.

## 1. What it is (vs SortableJS)

| Source | Adopted | Beyond Sortable |
|--------|---------|-----------------|
| **solid-dnd** | Provider, draggable / droppable, `DragOverlay`, four collision algorithms + hysteresis, **do not auto-move the `Vec`** | Collision is first-class, not CSS-overlap luck |
| **SortableJS** | Lists, `group`, handle, delay, empty-insert, `onMove` veto (`can_drop`), auto-scroll, ghost | Live sibling shift; auto-scroll **while held still**; Esc cancel; arrow-key insert nudge |
| **interact.js** | Axis lock (`start` too), restrict + anchor, snap range/endOnly/`snap_fn`, dropzone overlap + hooks, distance threshold | Combined with overlay + sortable + live-slot pin in one session. [INTERACT_JS_COMPARISON.md](../comparison/INTERACT_JS_COMPARISON.md) |

Not a ShadCN product component. Ghost uses `portal_sync(Pinned)` + author `fixed` inset from `overlay_pos` + `mount_anchored_overlay` (Unanchored geometry — Point placement would clamp Y like a cursor menu; [overlay_shells.md](overlay_shells.md) §9.3).

## 2. Dual-write (ADR-9)

Builder is the source of truth. RSX expands to the same functions. **Not in prelude.**

```rust
use rui::utils::drag::{
    drag_scope, draggable, droppable, drag_overlay, detect, CollisionAlg, OverlayAlign,
    Activation, Axis, ScrollLock, MODIFIER_ORDER_AUTHOR,
};

drag_scope("kanban")
    .collision(CollisionAlg::MostIntersecting)
    .collision_fn(|input| {
        detect(
            CollisionAlg::PointerWithin,
            input.pointer,
            input.dragged,
            input.droppables,
            input.active_id,
        )
    })
    .activation(Activation::And) // default; Or = delay or distance
    .scroll_lock(ScrollLock::Auto) // default; or "all" / "x" / "y" / "none"
    .scroll_sensitivity(40.0) // Sortable scrollSensitivity (edge band px)
    .scroll_speed(14.0) // px per display frame (~14 @ 60Hz)
    // .autoscroll_opts(AutoScroll::default().speed(10.0)) // full struct
    .animation(true) // default; false snaps sibling FLIP
    .modifier("nudge", MODIFIER_ORDER_AUTHOR, |t, _| t)
    .on_drag_end(|ev| { /* apply_reorder_vec / apply_transfer */ })
    .child(|| {
        /* Draggable + Droppable + */
        drag_overlay()
            .align(OverlayAlign::Center)
            .child_for(|item| { /* branch on item.item_type / id */ })
    })
```

```rust
ui! {
    <DragProvider id="kanban" collision="most-intersecting" activation="and" scroll-lock-auto animation={true} onDragEnd={handler}>
        <Draggable id="card-1" item_type="card" list="todo" index={0} stack="auto">
            { row() }
        </Draggable>
        <SortableItem
            id="tpl"
            item_type="tag"
            list="palette"
            index={0}
            pull_clone={true}
            skip_shift={false}
            skip_follow={false}
            data={Rc::new(42i32)}
            source_opacity={0.35}
        >
            { chip() }
        </SortableItem>
        <Droppable id="todo" list="todo" group="board" accept="card">
            { column() }
        </Droppable>
        <SortableList id="tags" accept="tag" direction="x">{ /* chips */ }</SortableList>
        <DragOverlay class="shadow-lg" align="center" />
    </DragProvider>
}
```

项级 RSX（`<Draggable>` / `<SortableItem>` 均支持）：`pull_clone` / `pullClone`、`skip_shift` / `skipShift`、`skip_follow` / `skipFollow`、`data={Rc::new(T)}`。Provider 级：`<DragProvider sensor={activator} sensor_id="kbd">`（见 §8）。

`collision_fn` / Overlay `child_for` 仍走 Builder（闭包类型）。槽矩形 Debugger：`set_debug_drag_hits(true)` 或 `.debug_hits(true)` / RSX `debug_hits={true}`（`true` 只开不关；Gallery 用按钮调 `set_debug_drag_hits`）。

Tags: `DragProvider`, `Draggable`, `Droppable`, `DragOverlay`, `DragHandle`, `DragIgnore`, `SortableList`, `SortableItem`.

## 3. Session

1. `pointerdown` on a draggable (or `DragHandle`) → [`begin`](../../crates/rui/src/utils/drag/session.rs) (`Pending`).
2. Move past `threshold` **and** `delay_ms` (default `Activation::And`) → `Active`, `on_drag_start`, overlay opens. `.activation(Activation::Or)` activates on delay **or** distance (solid-dnd pointer sensor).
3. Each move: modifier pipeline (axis / snap / pin-locked-axis / restrict / author `.modifier`) → overlay `pos` → collision (empty lists inflated by `empty_insert`) → `can_drop` veto → `on_drag_over` when the droppable changes. Siblings in the same list **shift** (CSS `transition: transform` [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms ease when `animation` is on, the default; `animation={false}` / `.animation(false)` snaps; pointer-follow and drop/cancel **snap** so commit layout is not inverted).
4. Pointer near a `scroll:` viewport edge → auto-scroll (also pumped while the pointer is held still).
5. `pointerup` → `on_drag_end`. **Esc** → [`cancel`](../../crates/rui/src/utils/drag/session.rs) (no commit). **↑↓←→** → [`nudge_insert`](../../crates/rui/src/utils/drag/session.rs).

Without `<DragOverlay>` the source item **translates** with the pointer (solid-dnd arbitrary move). Persist the delta in `on_drag_end` onto `left` / `top` if the item should stay where it was dropped. Overlapping items in one container use **`stack`** on `<Draggable>` / `.stack(...)`:

| `stack=` | Rest | While dragging |
|----------|------|----------------|
| `auto` (default) | author `z-index`, then last-drag rank | raise above Auto / Keep siblings; keep the new order after drop |
| `top` | always above Auto / Keep / Bottom | unchanged |
| `bottom` | always below Auto / Keep / Top | unchanged (will not come to front) |
| `keep` | author `z-index` | unchanged. Same or different author `z-index` both work |

`<DragProvider>` marks its root `data-stacking-context` and lifts item `z-index` onto the Draggable **shell** (also a stacking-context). Overlapping shells then paint as `rects → text` per item, so a front pill’s fill occludes the back pill’s glyphs. Provider-only stacking-context is not enough when items sit in a wrapping arena inside an already-open page paint stack (Gallery `ScrollArea`). Nested `z-index` on the inner chip alone does not split bands.

With an overlay, siblings FLIP a hole at the insert index. `opacity` is the **root** style on every element (`0`–`1` or `"40%"` / `40`). Drag-time 本体 paint is **`source_opacity`** (`<SortableItem source_opacity={0}>` / `<Draggable source_opacity={0}>` / `<DragOverlay source_opacity={0}>` / `.source_opacity(0)`): `0` leaves the hole empty; default overlay is [`OVERLAY_SOURCE_OPACITY`](../../crates/rui/src/utils/drag/session.rs) (`0.4`) **in the hole** (not under the siblings that closed the origin). Item `source_opacity` overrides the overlay session value. Cross-list hover (`source_opacity` > 0) translates the dimmed source onto the **target** hole using hit-slot origins, and raises the origin container `z-index` so it paints above the target column. The ghost follows as a Pinned portal whose `fixed` left/top is `overlay_pos` (item box, grab-offset; `.align(OverlayAlign::Center)` centers a smaller ghost on the source box). Default ghost **re-runs the active item's author body before chrome** — an undimmed clone, not the live dimmed island. `.child_for(|item| …)` builds a typed ghost from `DragItemCtx` (id / type / list / index) without subscribing `overlay_pos`. Styles on `<DragOverlay>` / `.style(...)` decorate that clone (Gallery Sortable uses `class="shadow-lg"` only). RSX `opacity` on the overlay is the ghost's resting transparency; a custom overlay **child** replaces the body entirely (Gallery「自定义子树」). Droppable **park** is still a tree move (`Show` + a **different** draggable id inside the zone) — `source_opacity` cannot substitute for unmounting the origin and mounting the parked child. The kit does **not** auto-move items: `on_drag_end` must park / `apply_reorder_vec` / `apply_transfer`. Live sibling shift is **paint-only** (`reactive_sync` + CSS `translate` FLIP); the item island does not remount on `hover_index` / pointer move, or `h=auto` placeholders collapse and the list height jitters. Shift distance is the **slot pitch** (item box + author gap), not the dragged box alone. Follow-source clones pin `drag_extent` **only** when the author size is a percentage (`w=100%`), so it does not resolve against the portal CB; auto-sized clones and custom children keep author size. Grab / `overlay_pos` use pointer-space **layout slots** (scroll-adjusted, same space as the pointer) — not pre-scroll `layout_bounds`, otherwise a `ScrollArea` host pins the ghost below the fold. Collision and sortable insert-index use pointer-space **layout slots** (scroll-adjusted, no CSS `translate`). Droppable identity is `(scope_id, id)`: Gallery's two planet lists reuse names, so a bare id lookup would bind hover to the source_opacity-0 column. The ghost is **viewport-pinned on purpose** (solid-dnd / dnd-kit): clipping it to the source parent would hide the overlay as soon as you drag into another container. Limit *motion* with `axis` / `restrict=viewport`, not overlay clip. `axis` locks that axis to the **live** pointer-space layout slot (scroll-adjusted): a parent `ScrollArea` pan must not leave an `axis="y"` ghost stuck at press-time X (Gallery Saturn offset). The free axis still follows the pointer. `<DragHandle>` is **headless** — wrap any chrome (icon button, label, custom `Box`); `handle={true}` on the item starts the drag only from that child. Nested controls that must not start a drag: [`drag_ignore`](../../crates/rui/src/utils/drag/builder.rs). **Visual chrome** (fill, border, radius, `cursor`, `align-items` / `self`) lives on the caller — Gallery [`drag.rs`](../../examples/gallery/src/pages/drag.rs), not in `utils/drag`.

## 4. Collision and modifiers

| `collision=` | Algorithm |
|--------------|-----------|
| `most-intersecting` (default) | Jaccard overlap |
| `closest-center` | Distance of centers |
| `closest-corners` | Sum of corner distances |
| `pointer-within` | Smallest droppable containing the pointer |

Builder `.collision_fn(|input| …)` wins over the enum; innermost nested refine still runs unless `input.skip_innermost`. `can_drop` vetoes after the pick. Gallery「collision_fn · 优先点名」在更大 overlap 的兄妹槽上仍点亮 `hit-prefer`。

Collision-slot overlay: [`set_debug_drag_hits(true)`](../../crates/rui/src/utils/drag/debug.rs) / `.debug_hits(true)` / RSX `debug_hits={true}`（虚线空闲槽，加粗 active droppable / 橙色 active source）。进程级 flag：`.debug_hits(true)` 只开不关；关掉走 `set_debug_drag_hits(false)`（Gallery 页顶按钮）。`use_drag().hits()`：拖中读 layout walk（[`last_hits_in_scope`](../../crates/rui/src/utils/drag/session.rs)）；**空闲**读 [`registered_hits_in_scope`](../../crates/rui/src/utils/drag/registry.rs)（订 `active_droppable` 再 peek；不订 `overlay_pos`）。`can_drop` / `empty_insert` / `SnapGrid` / `<DragIgnore>` / `disabled` 见 Gallery 末段；Extendable §11 见 **§8**。

Modifiers: `axis` (`xy` / `x` / `y` / **`start`** — `start` is interact `lockAxis: 'start'`: the first move past the threshold locks X or Y; Sortable insert still uses vertical `sort_axis`. Locked motion tracks the current layout slot so nested scroll does not desync the ghost). **`start_axis`** (`xy` default / `x` / `y`) is interact `startAxis`: the dominant first move must match or the session stays **Pending** (interact cancels the prepared action). **Edge auto-scroll** (Sortable `scroll` / interact `autoScroll`): `autoscroll={false}` turns the pump off; `scroll_sensitivity` / `scroll_speed` (Builder `.scroll_sensitivity` / `.scroll_speed` or `.autoscroll_opts(AutoScroll)`) tune the edge band and step; **`scroll_lock`** freezes scroll axes for **every** drag in that provider, independently of `axis`. `scroll-lock-all` still lets `pump_autoscroll` re-tick the overlay after a parent `scroll_behavior=smooth` chase. `restrict` (`none` default / `viewport` / `parent` / **element id**) on the provider **or** the item (item wins): omitted inherits; `viewport` clamps to the window (cross-parent drops still work); `parent` clamps to the author container with no id required (skips the kit hug-content item shell, like interact.js `restriction: 'parent'`); any other token is a live pointer-space box (Droppable id, `template_id`, or `id`) resolved each tick. Gallery「四角旋转」uses `restrict="corners-arena"` (the dashed Droppable); `restrict="parent"` would bind the inner centering `Box` instead. `.restrict_end_only(true)` / RSX `restrict_end_only` clamps only on drop. `.restrict_anchor(RestrictAnchor::CENTER)` / `restrict_anchor="center"` only pins the midpoint (interact `elementRect`). `threshold`, `delay_ms`, `empty_insert`, `autoscroll`, `animation` (`true` default / `false` snaps sibling FLIP), `snap` / `can_drop` (Builder). `SnapGrid::new(x,y).range(r).end_only(true)`: `range=0` always snaps; `end_only` snaps in `end` (delta on `on_drag_end` is the snapped travel). **Snap**：`.snap_targets(SnapTargets::new([…]))` 多格近者胜；`.snap_fn(f)` 自定义吸附点（interact `targets` function）。**Dropzone**（interact dropzone → `droppable`）：`.overlap(DropOverlap::Pointer|Center|Ratio(f))`、`.drop_checker`、`.on_drop_activate` / `.on_drop_deactivate` / `.on_drag_enter` / `.on_drag_leave` / `.on_drop_move` / `.on_drop`（per-zone；全局仍 `on_drag_over` / `on_drag_end`）。Item `stack` (`auto` / `top` / `bottom` / `keep`). `accept` matches `item_type` (comma-separated; Gallery「条件投放」is `容器1`←`组件2`, `容器2`←`组件1`); `group` must match when both are set. Overlay `rotate` / `.style(|el| el.rotate(…))` is author chrome (Gallery「四角旋转」peeks pointer vs the arena).

Ghost origin is a **modifier pipeline** (dnd-kit / interact.js `Modifier`; solid-dnd called this `Transformer` — RUI avoids that name so it does not collide with CSS `transform`). Built-ins `axis` (10) → `snap` (20) → `pin-locked-axis` (25) → `restrict` (30). Author stages: `.modifier("id", order, |t, ctx| …)` / [`add_modifier`](../../crates/rui/src/utils/drag/session.rs) (live; survives provider rebuild). Default author order is 40 ([`MODIFIER_ORDER_AUTHOR`](../../crates/rui/src/utils/drag/modifier.rs)). `t` is a [`DragDelta`](../../crates/rui/src/utils/drag/modifier.rs) (press-delta, not a CSS matrix); `ctx.origin_of(t)` is the ghost top-left. Overlay `rotate` / FLIP `translate` stay CSS `transform` and do not go through this pipeline. `restrict` Element ids are resolved to a layout `Rect` before the builtin clamp so nested `ScrollArea` stays live.

Nested lists: collision refines to the **innermost** droppable that contains the pointer when the winner wraps a smaller candidate. Sortable `hover_list` uses the same innermost hit (row or list shell), so empty nested padding is not stolen by the wrapping folder row. While the pointer is still in the lifted source island, hover stays on `from_list` (do not drop a folder into its own child list). Empty `accept` with **both** sides setting `item_type` only matches the same type — a file row will not accept a folder drag (and a folder row will not accept a file). Overlay follow-source clones of a nested `SortableItem` use distinct inner island ids so the ghost does not steal the live nested list. Sortable slots use each row's **layout size** (plus author gap), not a uniform first-pair stride: reordering mixed-height folders does not grow the list; only insert/remove does. Cross-list hover adds the dragged extent as **target** list padding so the destination folder can grow around the hole, and a matching negative end-margin on the **source** list so the origin folder shrinks (sibling FLIP still closes the visual gap; the margin pulls that vacated band out of the parent card).

| RSX | Builder | 说明 |
|-----|---------|------|
| `pull_clone={true}` | `.pull_clone(true)` | Sortable clone-pull |
| `skip_shift={true}` | `.skip_shift(true)` | 不参与兄弟 FLIP |
| `skip_follow={true}` | `.skip_follow(true)` | 跳过 kit 跟手 / 洞 translate |
| `data={Rc::new(T)}` | `.data(Rc::new(T))` | 项上 payload 袋 |
| `scroll-lock-all` | `.scroll_lock("all")` | 冻结 X/Y 自动滚 |
| `scroll-lock-x` | `.scroll_lock("x")` | freeze X only |
| `scroll-lock-y` | `.scroll_lock("y")` | freeze Y only |
| `scroll-lock-none` | `.scroll_lock("none")` | both axes (even if `axis="y"`) |
| `scroll-lock-auto` (default) | `.scroll_lock("auto")` | follow motion `axis` (`y` skips X, `x` skips Y, `xy` both) |
| `scroll_sensitivity={40}` | `.scroll_sensitivity(40)` | edge band px (default 40) |
| `scroll_speed={14}` | `.scroll_speed(14)` | px per display frame |
| `snap={grid}` / `snap_fn={f}` / `snap_targets={t}` | `.snap` / `.snap_fn` / `.snap_targets` | interact snap 管道 @ 20 |
| `overlap="center"` / `drop_checker={f}` | `.overlap` / `.drop_checker` | interact dropzone 重叠语义 |
| `sort_mode="swap"` | `.sort_mode(SortMode::Swap)` | 停兄弟 FLIP；高亮对端；`apply_swap_event` |
| `multi_drag={true}` | `.multi_drag(true)` | 单击选中；拖已选项一次带走；Overlay 叠卡 |

Value form is the same: `scroll_lock="all"` / `scroll-lock="x"` / `scrollLock="none"`. Flags may also be `scroll-lock-x={true}`.

Container highlight is **not** CSS `:hover`, and the kit does **not** paint colors. [solid-dnd](https://github.com/thisbeyond/solid-dnd) exposes `createDroppable().isActiveDroppable` from `state.active.droppableId` (collision result) and leaves chrome to the author. This kit mirrors that: paint-only `rui_drag_over` on the **current drop target** (`hover_list` / `active_droppable` — not the origin `from_list`). Same-list reorder still lights the list because it *is* the target. Authors restyle in `reactive_sync` from `use_drag()` (`active_droppable` / `hover_list` / `from_list` if origin chrome is wanted). Gallery park zones and Kanban columns only restyle the hover target.

## 5. Sortable helpers

- [`sortable_shift`](../../crates/rui/src/utils/drag/sortable.rs) — applied live to **siblings** (close origin, open insert hole). `source_opacity` 0 leaves that hole empty. Dimmed overlay translates the body onto the hole ([`sortable_source_to_hole`](../../crates/rui/src/utils/drag/sortable.rs)). Distance changes ease over [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) unless the provider sets `animation={false}`; `end` / `cancel` snap so leftover translate cannot invert the rebuilt layout.
- [`apply_reorder_vec`](../../crates/rui/src/utils/drag_reorder.rs) — same list, on drop.
- [`apply_transfer`](../../crates/rui/src/utils/drag/sortable.rs) — cross list.
- [`apply_event`](../../crates/rui/src/utils/drag/sortable.rs) — routes clone / **swap** / **multi** / reorder.
- [`apply_swap_event`](../../crates/rui/src/utils/drag/sortable.rs) — `SortMode::Swap`：同列 `Vec::swap`，跨列 `mem::swap`。拖中不让位；对端 `rui_drag_swap`。
- [`apply_multi_event`](../../crates/rui/src/utils/drag/sortable.rs) — 抽出选中项（相对序）整块插入 `to_index`。Provider `multi_drag`：单击（未过阈值）toggle `DragCtx.selected`；拖已选项冻结 `DragEvent.items`。v1 无 Shift 范围选。
- `<SortableItem>` = draggable + droppable (`also_droppable`).
- `<SortableList>` / `<Droppable list=…>` = container (no index **appends**). `.direction(Axis::X|Y)` / RSX `direction="x"|"y"` writes list-level insert/FLIP axis; omitting it inherits the provider `axis`. Same provider may host one `direction=y` column and one `direction=x` row (Gallery「SortableList.direction」).

## 6. Alignment vs SortableJS

列表血统 **已对齐/超**（[SORTABLE_JS_COMPARISON.md](../comparison/SORTABLE_JS_COMPARISON.md) **2026-09-10 冻结**列表核心）。`forceFallback` / `store` **不进套件**（架构）；OnSpill 仍是产品未做。**Swap / MultiDrag 已落地**（WP-DRAG.4）。源码默认 `animation: 0`；本套件默认开 150ms FLIP（对齐 Sortable README 示例）。

| | SortableJS 1.15.7 | `utils/drag` |
|--|-------------------|----------------|
| List reorder / group | DOM `insertBefore` + `group.pull`/`put` | `apply_reorder_vec` / `apply_transfer`；`group` 字符串 + `accept` |
| Handle / delay / filter | `handle` / `delay` / `filter` | `<DragHandle>` + `delay_ms` + `drag_ignore` |
| Empty insert | `emptyInsertThreshold` 5px | `empty_insert` 8px |
| `onMove` | `false` 否决；`-1`/`1` 强制插侧 | `can_drop` 布尔否决；插侧 = slot 中点 |
| Auto-scroll | AutoScroll 30px / 10px/24ms | `edge_scroll` 40px / 14px/帧 + **静止泵** + `scroll_lock` |
| Ghost | `ghostClass` / `forceFallback` 克隆 | Pinned `DragOverlay`（viewport inset） |
| Live gap | `animation` ms（源码默认 0） | sibling `translate` + FLIP [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs)（默认开；`animation={false}` snaps；drop 必 snap） |
| Keyboard | 无一等 | Esc `cancel` + `nudge_insert` |
| Collision | `swapThreshold` 几何 | 四算法 + `collision_fn` + sticky |
| Clone / nested / multi-drag | clone 在 `group.pull`；嵌套易误触；MultiDrag 插件 | nested innermost ☑ / `pull_clone` ☑ / `multi_drag` ☑ |
| Swap 模式 | Swap 插件 `swapClass` + `swapNodes` | `sort_mode="swap"` + `apply_swap_event` ☑ |

## 7. Checklist

- [x] Coordinates live in Signals (`overlay_pos`); island writes `fixed` left/top from that signal
- [x] Ghost = Pinned portal + Unanchored viewport inset (not Esc-stacked, not cursor-menu clamp)
- [x] Collision: four algorithms + hysteresis on ties
- [x] Cross-container via `group` / `apply_transfer`
- [x] Handle-only start; grab/collision use the **item** box; default overlay clones the source (custom child keeps its own size)
- [x] Live sibling shift + edge auto-scroll + Esc / arrows
- [x] RSX + Builder + Gallery (`DragShowcase` in 浮层 / 反馈)
- [x] Nested lists: innermost collision + hover_list (folder wrap); Gallery 嵌套列表
- [x] Modifier pipeline (axis/snap/pin/restrict + author `.modifier`; not CSS `transform`)
- [x] Extendable §7.1–7.5 / §7.7：`collision_fn`、Overlay `child_for` / `align`、`Activation::Or`、`SortableList.direction`、`debug_hits`、`use_drag().hits()`；Gallery `DragShowcase` 末段
- [x] Extendable §11 / WP-DRAG.3：`.sensor` / `add_sensor`、`.data` / `DragEvent.data`、`registered_hits_in_scope`（空闲 `hits()`）、`.skip_shift` / `.skip_follow`、`.pull_clone` + `apply_clone_event` / `apply_event`；Gallery `DragShowcase` §11 段
- [x] WP-DRAG.4：`SortMode::Swap` + `multi_drag` 选择集 / Overlay 叠卡 / `apply_swap_event` / `apply_multi_event`；Gallery §12
- [x] 指针动作（非拖放）：[`pointer_action`](../../crates/rui/src/utils/pointer_action/mod.rs) resize / gesture / inertia / 多指；自由拖 `.inertia(true)`；Gallery 边角缩放、双指圆点、甩砖

## 8. Extendable（§11 / WP-DRAG.3）

solid-dnd README **Extendable** 五项 + Sortable **clone-pull** 均已落地（2026-09-10）。RSX 与 Builder 双写：`pull_clone` / `skip_*` / `data` on `<Draggable>` / `<SortableItem>`；`<DragProvider sensor={…} sensor_id="…">` 与 `.sensor` / [`add_sensor`](../../crates/rui/src/utils/drag/sensor.rs) 等价。

| 能力 | Builder / RSX | 读回 | 提交 / 运行 |
|------|-----------------|------|-------------|
| **clone-pull** | `.pull_clone(true)` / `pull_clone={true}` | `DragEvent.pull_clone` | [`apply_clone_event`](../../crates/rui/src/utils/drag/sortable.rs) or [`apply_event`](../../crates/rui/src/utils/drag/sortable.rs) |
| **Swap** | `.sort_mode(SortMode::Swap)` / `sort_mode="swap"` | `DragEvent.sort_mode` / `rui_drag_swap` | [`apply_swap_event`](../../crates/rui/src/utils/drag/sortable.rs)（`apply_event` 路由） |
| **MultiDrag** | `.multi_drag(true)` / `multi_drag={true}` | `DragCtx.selected` / `DragEvent.items` / `rui_drag_selected` | [`apply_multi_event`](../../crates/rui/src/utils/drag/sortable.rs)；单击 Pending 结束 toggle，不触发 `on_drag_end` |
| **`data` 袋** | `.data(Rc::new(T))` / `data={Rc::new(T)}` | [`DragItemCtx::data_as`](../../crates/rui/src/utils/drag/builder.rs) / `DragEvent.data` | TLS registry（不进 Sync `Context`） |
| **空闲 `hits`** | assemble → [`register_drag_slot_meta`](../../crates/rui/src/utils/drag/builder.rs) | [`use_drag().hits()`](../../crates/rui/src/utils/drag/builder.rs) — 拖中 layout / 空闲 [`registered_hits_in_scope`](../../crates/rui/src/utils/drag/registry.rs) | — |
| **skipTransform** | `.skip_shift` / `.skip_follow` + RSX 同名 | attrs `rui_drag_skip_*` | 作者自管 motion 时用 `skip_follow` |
| **sensor 槽** | `.sensor(id, \|ctx\| …)` / RSX `sensor` + `sensor_id` / [`add_sensor`](../../crates/rui/src/utils/drag/sensor.rs) | `DragSensorCtx { scope_id, begin }` | [`run_scope_sensors`](../../crates/rui/src/utils/drag/sensor.rs) 或 activator 内 [`begin_draggable`](../../crates/rui/src/utils/drag/sensor.rs) / `ctx.begin(BeginDrag { … })` |

```rust
use std::rc::Rc;
use rui::utils::drag::{
    apply_event, begin_draggable, drag_scope, run_scope_sensors, sortable_item, DragItemCtx,
};

drag_scope("palette")
    .sensor("keyboard", |ctx| {
        if keyboard_armed() {
            return begin_draggable(&ctx.scope_id, "tpl", &ctx.begin);
        }
        false
    })
    .on_drag_end(|ev| { apply_event(&mut items, None, ev); })
    .child(|| {
        sortable_item("tpl")
            .pull_clone(true)
            .data(Rc::new(42i32))
            .child(|| { /* chip */ });
    });

// keydown / 按钮：run_scope_sensors("palette");
```

```rust
ui! {
    <DragProvider id="palette" sensor_id="keyboard" sensor={kbd_activator} onDragEnd={on_end}>
        <SortableItem id="tpl" pull_clone={true} data={Rc::new(42i32)}>{ chip() }</SortableItem>
        <DragOverlay />
    </DragProvider>
}
```

Gallery [`drag.rs`](../../examples/gallery/src/pages/drag.rs) **§11** 段：`pull_clone`、`data` HUD、空闲 `hits`、`skip_shift` 对照、**sensor 按钮 + `run_scope_sensors`**。

## 9. Pointer actions (`utils/pointer_action`)

interact.js 的 resize / gesture / inertia / multi-touch **属于拖拽**，但不是列表投放。不要把它们塞进 `utils/drag` 的碰撞 / FLIP tick。

| 动作 | 入口 | 超出 interact |
|------|------|----------------|
| 边/角缩放 | `begin_resize` / `ResizeSession` | `min_size` / `max_size` / `from_center`；**`.autoscroll(true)`** / `.autoscroll_opts(AutoScroll)` 可选边缘滚 |
| 双指 | `GestureState::from_pair` / `sync_touches` | centroid + translation；任意点 `box_of`；**`set_gesture_opts(GestureOpts::…autoscroll(true))`** |
| 边缘滚配置 | [`AutoScroll`](../../crates/rui/src/utils/edge_scroll.rs)（`utils/edge_scroll`） | 拖放：`scroll_sensitivity` / `scroll_speed` / `scroll_lock`；resize/gesture 共用同一 struct |
| 惯性 | `.inertia(true)` / `InertiaOpts` | 与滚动 fling 同形；Sortable 不滑行；`endOnly` 可 smoothEnd |
| 多指 | `PointerSet` + `begin_pointer` | 同元素第二指升 Gesture；`max` / `maxPerElement` |

分栏仍用 [`ResizablePanelGroup`](../../crates/rui/src/components/resizable.rs)。自由砖松手甩：`<DragProvider inertia>`（无 `list`/`index`）。对照 [INTERACT_JS_COMPARISON.md](../comparison/INTERACT_JS_COMPARISON.md) §7。
