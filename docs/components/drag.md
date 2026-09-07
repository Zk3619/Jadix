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
use rui::utils::drag::{drag_scope, draggable, droppable, drag_overlay, CollisionAlg};

drag_scope("kanban")
    .collision(CollisionAlg::MostIntersecting)
    .on_drag_end(|ev| { /* apply_reorder_vec / apply_transfer */ })
    .child(|| { /* Draggable + Droppable + DragOverlay */ })
```

```rust
ui! {
    <DragProvider id="kanban" collision="most-intersecting" onDragEnd={handler}>
        <Draggable id="card-1" item_type="card" list="todo" index={0}>
            { row() }
        </Draggable>
        <Droppable id="todo" list="todo" group="board" accept="card">
            { column() }
        </Droppable>
        <DragOverlay class="opacity-90 shadow-lg" />
    </DragProvider>
}
```

Tags: `DragProvider`, `Draggable`, `Droppable`, `DragOverlay`, `DragHandle`, `DragIgnore`, `SortableList`, `SortableItem`.

## 3. Session

1. `pointerdown` on a draggable (or `DragHandle`) → [`begin`](../../crates/rui/src/utils/drag/session.rs) (`Pending`).
2. Move past `threshold` **and** `delay_ms` → `Active`, `on_drag_start`, overlay opens.
3. Each move: axis / snap / restrict → overlay `pos` → collision (empty lists inflated by `empty_insert`) → `can_drop` veto → `on_drag_over` when the droppable changes. Siblings in the same list **shift**.
4. Pointer near a `scroll:` viewport edge → auto-scroll (also pumped while the pointer is held still).
5. `pointerup` → `on_drag_end`. **Esc** → [`cancel`](../../crates/rui/src/utils/drag/session.rs) (no commit). **↑↓←→** → [`nudge_insert`](../../crates/rui/src/utils/drag/session.rs).

Without `<DragOverlay>` the source item **translates** with the pointer. With an overlay the source is dimmed (`opacity: 0.4`) and the ghost follows as a Pinned portal whose `fixed` left/top is `overlay_pos` (item box, grab-offset). `<DragOverlay hide_source={true}>` / `.hide_source(true)` hides the source completely (the layout slot stays, only the ghost is visible). Default ghost **re-runs the active item's author body** (same text and chrome as the source). Styles on `<DragOverlay>` / `.style(...)` overlay that clone. A custom overlay **child** replaces the body entirely (Gallery「自定义子树」). The kit does **not** auto-move items: `on_drag_end` must park / `apply_reorder_vec` / `apply_transfer`. Follow-source clones pin `drag_extent` **only** when the author size is a percentage (`w=100%`), so it does not resolve against the portal CB; auto-sized clones and custom children keep author size. Grab / `overlay_pos` use pointer-space **layout slots** (scroll-adjusted, same space as the pointer) — not pre-scroll `layout_bounds`, otherwise a `ScrollArea` host pins the ghost below the fold. Collision and sortable insert-index **omit CSS `translate`**, so live sibling shift does not move the hit slots. The ghost is **viewport-pinned on purpose** (solid-dnd / dnd-kit): clipping it to the source parent would hide the overlay as soon as you drag into another container. Limit *motion* with `axis` / `restrict=viewport`, not overlay clip. `<DragHandle>` is **headless** — wrap any chrome (icon button, label, custom `Box`); `handle={true}` on the item starts the drag only from that child. Nested controls that must not start a drag: [`drag_ignore`](../../crates/rui/src/utils/drag/builder.rs). **Visual chrome** (fill, border, radius, `cursor`, `align-items` / `self`) lives on the caller — Gallery [`drag.rs`](../../examples/gallery/src/pages/drag.rs), not in `utils/drag`.

## 4. Collision and modifiers

| `collision=` | Algorithm |
|--------------|-----------|
| `most-intersecting` (default) | Jaccard overlap |
| `closest-center` | Distance of centers |
| `closest-corners` | Sum of corner distances |
| `pointer-within` | Smallest droppable containing the pointer |

Modifiers: `axis` (`xy` / `x` / `y`), `restrict` (`none` / `viewport`), `threshold`, `delay_ms`, `empty_insert`, `autoscroll`, `snap` / `can_drop` (Builder). `accept` matches `item_type` (comma-separated; Gallery「条件投放」is `容器1`←`组件2`, `容器2`←`组件1`); `group` must match when both are set.

## 5. Sortable helpers

- [`sortable_shift`](../../crates/rui/src/utils/drag/sortable.rs) — applied live to siblings while dragging.
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
| Live gap | CSS animation | sibling `translate` (FLIP duration = WP-DRAG.3) |
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
