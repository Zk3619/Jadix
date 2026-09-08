# Drag (`utils/drag`)

> **Status**: **WP-DRAG.1 ☑** + **WP-DRAG.2 ☑** headless kit + RSX + Gallery. Experimental — not in `prelude`, not SemVer 0.1.  
> **Parent**: [COMPONENT_LIBRARY.md](../COMPONENT_LIBRARY.md) §2.1, [overlay_shells.md](overlay_shells.md) §9, [headless.md](headless.md).  
> **References**: `Reference/solid-dnd-main`, `Reference/Sortable-master`, `Reference/interact.js-main`.  
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
        ├── utils/pointer_drag     session kind `DragKind::Scope`
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
| **interact.js** | Axis lock, restrict, snap, distance threshold | Combined with overlay + sortable in one session |

Not a ShadCN product component. Ghost uses `portal_sync(Pinned)` + author `fixed` inset from `overlay_pos` + `mount_anchored_overlay` (Unanchored geometry — Point placement would clamp Y like a cursor menu; [overlay_shells.md](overlay_shells.md) §9.3).

## 2. Dual-write (ADR-9)

Builder is the source of truth. RSX expands to the same functions. **Not in prelude.**

```rust
use rui::utils::drag::{drag_scope, draggable, droppable, drag_overlay, CollisionAlg, ScrollLock};

drag_scope("kanban")
    .collision(CollisionAlg::MostIntersecting)
    .scroll_lock(ScrollLock::Auto) // default; or "all" / "x" / "y" / "none"
    .animation(true) // default; false snaps sibling FLIP
    .on_drag_end(|ev| { /* apply_reorder_vec / apply_transfer */ })
    .child(|| { /* Draggable + Droppable + DragOverlay */ })
```

```rust
ui! {
    <DragProvider id="kanban" collision="most-intersecting" scroll-lock-auto animation={true} onDragEnd={handler}>
        <Draggable id="card-1" item_type="card" list="todo" index={0} stack="auto">
            { row() }
        </Draggable>
        <Droppable id="todo" list="todo" group="board" accept="card">
            { column() }
        </Droppable>
        <DragOverlay class="shadow-lg" />
    </DragProvider>
}
```

Tags: `DragProvider`, `Draggable`, `Droppable`, `DragOverlay`, `DragHandle`, `DragIgnore`, `SortableList`, `SortableItem`.

## 3. Session

1. `pointerdown` on a draggable (or `DragHandle`) → [`begin`](../../crates/rui/src/utils/drag/session.rs) (`Pending`).
2. Move past `threshold` **and** `delay_ms` → `Active`, `on_drag_start`, overlay opens.
3. Each move: axis / snap / restrict → overlay `pos` → collision (empty lists inflated by `empty_insert`) → `can_drop` veto → `on_drag_over` when the droppable changes. Siblings in the same list **shift** (CSS `transition: transform` [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) = 150ms ease when `animation` is on, the default; `animation={false}` / `.animation(false)` snaps; pointer-follow and drop/cancel **snap** so commit layout is not inverted).
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

With an overlay, siblings FLIP a hole at the insert index. `opacity` is the **root** style on every element (`0`–`1` or `"40%"` / `40`). Drag-time 本体 paint is **`source_opacity`** (`<SortableItem source_opacity={0}>` / `<Draggable source_opacity={0}>` / `<DragOverlay source_opacity={0}>` / `.source_opacity(0)`): `0` leaves the hole empty; default overlay is [`OVERLAY_SOURCE_OPACITY`](../../crates/rui/src/utils/drag/session.rs) (`0.4`) **in the hole** (not under the siblings that closed the origin). Item `source_opacity` overrides the overlay session value. Cross-list hover (`source_opacity` > 0) translates the dimmed source onto the **target** hole using hit-slot origins, and raises the origin container `z-index` so it paints above the target column. The ghost follows as a Pinned portal whose `fixed` left/top is `overlay_pos` (item box, grab-offset). Default ghost **re-runs the active item's author body before chrome** — an undimmed clone, not the live dimmed island. Styles on `<DragOverlay>` / `.style(...)` decorate that clone (Gallery Sortable uses `class="shadow-lg"` only). RSX `opacity` on the overlay is the ghost's resting transparency; a custom overlay **child** replaces the body entirely (Gallery「自定义子树」). Droppable **park** is still a tree move (`Show` + a **different** draggable id inside the zone) — `source_opacity` cannot substitute for unmounting the origin and mounting the parked child. The kit does **not** auto-move items: `on_drag_end` must park / `apply_reorder_vec` / `apply_transfer`. Live sibling shift is **paint-only** (`reactive_sync` + CSS `translate` FLIP); the item island does not remount on `hover_index` / pointer move, or `h=auto` placeholders collapse and the list height jitters. Shift distance is the **slot pitch** (item box + author gap), not the dragged box alone. Follow-source clones pin `drag_extent` **only** when the author size is a percentage (`w=100%`), so it does not resolve against the portal CB; auto-sized clones and custom children keep author size. Grab / `overlay_pos` use pointer-space **layout slots** (scroll-adjusted, same space as the pointer) — not pre-scroll `layout_bounds`, otherwise a `ScrollArea` host pins the ghost below the fold. Collision and sortable insert-index use pointer-space **layout slots** (scroll-adjusted, no CSS `translate`). Droppable identity is `(scope_id, id)`: Gallery's two planet lists reuse names, so a bare id lookup would bind hover to the source_opacity-0 column. The ghost is **viewport-pinned on purpose** (solid-dnd / dnd-kit): clipping it to the source parent would hide the overlay as soon as you drag into another container. Limit *motion* with `axis` / `restrict=viewport`, not overlay clip. `axis` locks that axis to the **live** pointer-space layout slot (scroll-adjusted): a parent `ScrollArea` pan must not leave an `axis="y"` ghost stuck at press-time X (Gallery Saturn offset). The free axis still follows the pointer. `<DragHandle>` is **headless** — wrap any chrome (icon button, label, custom `Box`); `handle={true}` on the item starts the drag only from that child. Nested controls that must not start a drag: [`drag_ignore`](../../crates/rui/src/utils/drag/builder.rs). **Visual chrome** (fill, border, radius, `cursor`, `align-items` / `self`) lives on the caller — Gallery [`drag.rs`](../../examples/gallery/src/pages/drag.rs), not in `utils/drag`.

## 4. Collision and modifiers

| `collision=` | Algorithm |
|--------------|-----------|
| `most-intersecting` (default) | Jaccard overlap |
| `closest-center` | Distance of centers |
| `closest-corners` | Sum of corner distances |
| `pointer-within` | Smallest droppable containing the pointer |

Modifiers: `axis` (`xy` / `x` / `y` — locked motion axis tracks the current layout slot so nested scroll does not desync the ghost). **`scroll_lock`** freezes **edge auto-scroll** for **every** drag in that provider (not only axis-locked ones), independently of `axis`. `autoscroll={false}` turns the whole pump off; `scroll-lock-all` still lets `pump_autoscroll` re-tick the overlay after a parent `scroll_behavior=smooth` chase. `restrict` (`none` / `viewport`), `threshold`, `delay_ms`, `empty_insert`, `autoscroll`, `animation` (`true` default / `false` snaps sibling FLIP), `snap` / `can_drop` (Builder). Item `stack` (`auto` / `top` / `bottom` / `keep`). `accept` matches `item_type` (comma-separated; Gallery「条件投放」is `容器1`←`组件2`, `容器2`←`组件1`); `group` must match when both are set. Overlay `rotate` / `.style(|el| el.rotate(…))` is author chrome (Gallery「四角旋转」peeks pointer vs the arena).

| RSX | Builder | Auto-scroll |
|-----|---------|-------------|
| `scroll-lock-all` | `.scroll_lock("all")` | freeze X and Y |
| `scroll-lock-x` | `.scroll_lock("x")` | freeze X only |
| `scroll-lock-y` | `.scroll_lock("y")` | freeze Y only |
| `scroll-lock-none` | `.scroll_lock("none")` | both axes (even if `axis="y"`) |
| `scroll-lock-auto` (default) | `.scroll_lock("auto")` | follow motion `axis` (`y` skips X, `x` skips Y, `xy` both) |

Value form is the same: `scroll_lock="all"` / `scroll-lock="x"` / `scrollLock="none"`. Flags may also be `scroll-lock-x={true}`.

Container highlight is **not** CSS `:hover`, and the kit does **not** paint colors. [solid-dnd](https://github.com/thisbeyond/solid-dnd) exposes `createDroppable().isActiveDroppable` from `state.active.droppableId` (collision result) and leaves chrome to the author. This kit mirrors that: paint-only `rui_drag_over` on the **current drop target** (`hover_list` / `active_droppable` — not the origin `from_list`). Same-list reorder still lights the list because it *is* the target. Authors restyle in `reactive_sync` from `use_drag()` (`active_droppable` / `hover_list` / `from_list` if origin chrome is wanted). Gallery park zones and Kanban columns only restyle the hover target.

## 5. Sortable helpers

- [`sortable_shift`](../../crates/rui/src/utils/drag/sortable.rs) — applied live to **siblings** (close origin, open insert hole). `source_opacity` 0 leaves that hole empty. Dimmed overlay translates the body onto the hole ([`sortable_source_to_hole`](../../crates/rui/src/utils/drag/sortable.rs)). Distance changes ease over [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) unless the provider sets `animation={false}`; `end` / `cancel` snap so leftover translate cannot invert the rebuilt layout.
- [`apply_reorder_vec`](../../crates/rui/src/utils/drag_reorder.rs) — same list, on drop.
- [`apply_transfer`](../../crates/rui/src/utils/drag/sortable.rs) — cross list.
- `<SortableItem>` = draggable + droppable (`also_droppable`).
- `<SortableList>` / `<Droppable list=…>` = container (no index **appends**).

## 6. Roadmap vs SortableJS

| | SortableJS | `utils/drag` |
|--|------------|----------------|
| List reorder / group | yes | yes |
| Handle / delay / filter | yes | handle + `delay_ms` + `drag_ignore` |
| Auto-scroll | rAF | mousemove **and** wait-loop pump |
| Ghost | CSS class | Pinned `DragOverlay` (viewport inset, not cursor-menu clamp) |
| Live gap | CSS animation | sibling `translate` + FLIP [`SORTABLE_FLIP_MS`](../../crates/rui/src/utils/drag/sortable.rs) (150ms ease; `animation={false}` snaps; drop always snaps) |
| Keyboard | weak | Esc cancel + arrow insert |
| Collision | geometry guess | four named algorithms |
| Clone / nested / multi-drag | plugins | **WP-DRAG.3** |

## 7. Checklist

- [x] Coordinates live in Signals (`overlay_pos`); island writes `fixed` left/top from that signal
- [x] Ghost = Pinned portal + Unanchored viewport inset (not Esc-stacked, not cursor-menu clamp)
- [x] Collision: four algorithms + hysteresis on ties
- [x] Cross-container via `group` / `apply_transfer`
- [x] Handle-only start; grab/collision use the **item** box; default overlay clones the source (custom child keeps its own size)
- [x] Live sibling shift + edge auto-scroll + Esc / arrows
- [x] RSX + Builder + Gallery (`DragShowcase` in 浮层 / 反馈)
- [x] Library tests: geometry / collision / session / auto-scroll / RSX dual-write
