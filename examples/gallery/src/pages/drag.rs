//! WP-DRAG.1: solid-dnd / SortableJS / interact.js kit — RSX + Builder gallery.

use crate::imports::*;
use crate::shared::*;
use std::rc::Rc;

use rui::utils::drag::{
    apply_event, apply_reorder_vec, apply_transfer, begin_draggable, detect, drag_overlay,
    drag_scope, last_drag_extent, last_hit_rect, last_overlay_pos, last_pointer, run_scope_sensors,
    set_debug_drag_hits, sortable_item, sortable_list, use_drag, Axis, CollisionAlg, DragCtx,
    DragEvent, DragItemCtx, OverlayAlign, Restrict, RestrictAnchor, SnapGrid, SnapPointFn,
    SnapTargets,
};
use rui::utils::pointer_action::{
    begin_resize, last_gesture, set_gesture_opts, BoxRect, Edges, GestureOpts, GestureState,
    Invert, ResizeOpts,
};

/// `reactive_sync` runs in the frame prepass with no provider owner.
/// `use_drag()` / `use_context` walk `current_owner` and miss `DragCtx` —
/// capture the signals at column/zone build (same as droppable chrome).
fn zone_over_peek(ctx: Option<&DragCtx>, id: &str) -> bool {
    ctx.and_then(|c| c.active_droppable.peek()).as_deref() == Some(id)
}

fn zone_list_hover_peek(ctx: Option<&DragCtx>, list: &str) -> bool {
    ctx.and_then(|c| c.hover_list.peek()).as_deref() == Some(list)
}

/// Subscribe `active_droppable` (not `overlay_pos`); peek `hits()` for the tick cache.
fn hits_hud() -> Element {
    let ctx = use_drag();
    let over = ctx.as_ref().and_then(|c| c.active_droppable.get());
    let dragging = ctx
        .as_ref()
        .and_then(|c| c.active_draggable.get())
        .is_some();
    let hits = ctx.as_ref().map(|c| c.hits()).unwrap_or_default();
    let n_drag = hits.iter().filter(|h| h.is_draggable).count();
    let n_drop = hits.iter().filter(|h| h.is_droppable).count();
    let mode = if dragging {
        "拖中·layout walk"
    } else {
        "空闲·registered"
    };
    let text = format!(
        "use_drag().hits() · {mode} · drag={n_drag} drop={n_drop} over={}",
        over.as_deref().unwrap_or("—")
    );
    ui! {
        <Label text_size="0.75rem" color="muted-foreground">{ text }</Label>
    }
}

fn data_payload_ghost(item: &DragItemCtx) -> Element {
    let payload = item
        .data_as::<i32>()
        .map(|n| format!("data={n}"))
        .unwrap_or_else(|| "—".to_string());
    ui! {
        <HStack
            p="0.35rem 0.6rem"
            gap="0.375rem"
            items="center"
            rounded="9999px"
            bg="secondary"
        >
            <Label text_size="0.75rem" color="secondary-foreground">{ payload }</Label>
        </HStack>
    }
}

fn keyed_sortable_flags_chips(
    rows_id: &'static str,
    items: Signal<Vec<String>>,
    list: &'static str,
    item_type: &'static str,
    source_opacity: f32,
    middle_skip_shift: bool,
    middle_skip_follow: bool,
) -> Element {
    let items_snap = items.get();
    let rows = rui::for_keyed!(
        rows_id,
        items_snap.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items_snap.iter().position(|x| x == &name).unwrap_or(0);
            let id = format!("{list}:{name}");
            let skip_shift = middle_skip_shift && i == 1;
            let skip_follow = middle_skip_follow && i == 1;
            ui! {
                <SortableItem
                    id={id}
                    item_type={item_type}
                    accept={item_type}
                    list={list}
                    index={i}
                    source_opacity={source_opacity}
                    skip_shift={skip_shift}
                    skip_follow={skip_follow}
                >
                    { compact_chip(&name) }
                </SortableItem>
            }
        }
    );
    Element::from(vstack().w("100%").gap("0.375rem").children(rows))
}

fn keyed_pull_clone_chips(items: Signal<Vec<String>>) -> Element {
    let items_snap = items.get();
    let rows = rui::for_keyed!(
        "gal-drag-clone-rows",
        items_snap.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items_snap.iter().position(|x| x == &name).unwrap_or(0);
            let id = format!("clone:{name}");
            ui! {
                <SortableItem
                    id={id}
                    item_type="clone-tag"
                    accept="clone-tag"
                    list="clone-list"
                    index={i}
                    pull_clone={true}
                    source_opacity={0.35}
                >
                    { compact_chip(&name) }
                </SortableItem>
            }
        }
    );
    Element::from(vstack().w("100%").gap("0.375rem").children(rows))
}

fn rsx_sortable_flags_column(
    title: &str,
    rows_id: &'static str,
    list: &'static str,
    items: Signal<Vec<String>>,
    item_type: &'static str,
    source_opacity: f32,
    middle_skip_shift: bool,
    middle_skip_follow: bool,
) -> Element {
    let title = title.to_string();
    ui! {
        <VStack grow="1" min_w="9rem" gap="0.375rem">
            <Label text_size="0.75rem" font_weight="600">{ title }</Label>
            <SortableList id={list} accept={item_type}>
                { keyed_sortable_flags_chips(
                    rows_id,
                    items,
                    list,
                    item_type,
                    source_opacity,
                    middle_skip_shift,
                    middle_skip_follow,
                ) }
            </SortableList>
        </VStack>
    }
}

fn rsx_pull_clone_board(
    items: Signal<Vec<String>>,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    let n = items.get().len();
    let hint = format!("共 {n} 项 · `<SortableItem pull_clone={{true}}>` 松手 insert 副本");
    ui! {
        <DragProvider id="gal-drag-clone" collision="closest-center" onDragEnd={on_end}>
            <VStack gap="0.375rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground">{ hint }</Label>
                <SortableList id="clone-list" accept="clone-tag">
                    { keyed_pull_clone_chips(items) }
                </SortableList>
                <DragOverlay class="shadow-lg" />
            </VStack>
        </DragProvider>
    }
}

fn rsx_skip_flags_board(
    shift_normal: Signal<Vec<String>>,
    shift_pinned: Signal<Vec<String>>,
    shift_follow: Signal<Vec<String>>,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    ui! {
        <DragProvider id="gal-drag-skip-shift" collision="closest-center" onDragEnd={on_end}>
            <VStack gap="0.375rem" w="100%">
                <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                    { rsx_sortable_flags_column(
                        "默认 · 兄弟 FLIP",
                        "gal-drag-shift-normal-rows",
                        "shift-normal",
                        shift_normal,
                        "shift-demo",
                        0.35,
                        false,
                        false,
                    ) }
                    { rsx_sortable_flags_column(
                        "skip_shift · 中项",
                        "gal-drag-shift-pinned-rows",
                        "shift-pinned",
                        shift_pinned,
                        "shift-demo",
                        0.35,
                        true,
                        false,
                    ) }
                    { rsx_sortable_flags_column(
                        "skip_follow · 中项",
                        "gal-drag-shift-follow-rows",
                        "shift-follow",
                        shift_follow,
                        "shift-demo",
                        0.35,
                        false,
                        true,
                    ) }
                </HStack>
                <DragOverlay class="shadow-lg" />
            </VStack>
        </DragProvider>
    }
}

fn park_on_drop(parked: Signal<bool>, drop_id: &'static str) -> impl FnMut(&DragEvent) {
    move |ev: &DragEvent| {
        parked.set(ev.droppable_id.as_deref() == Some(drop_id));
    }
}

fn park_typed(
    alpha: Signal<bool>,
    beta: Signal<bool>,
    alpha_drop: &'static str,
    beta_drop: &'static str,
) -> impl FnMut(&DragEvent) {
    move |ev: &DragEvent| {
        let src = ev.draggable_id.as_str();
        let drop = ev.droppable_id.as_deref();
        if src == "alpha" || src == "alpha:c2" {
            alpha.set(drop == Some(alpha_drop));
        } else if src == "beta" || src == "beta:c1" {
            beta.set(drop == Some(beta_drop));
        }
    }
}

/// Empty slot so the source column does not collapse after the item parks in the droppable.
fn vacated_slot(hint: &str) -> Element {
    ui! {
        <Box
            min_h="2.5rem"
            min_w="7rem"
            p="0.5rem 0.65rem"
            rounded="0.5rem"
            bg="transparent"
            border_style="dashed"
            border_width="1px"
            border_color="border"
            items="center"
            justify="center"
        >
            <Label text_size="0.75rem" color="muted-foreground">{ hint.to_string() }</Label>
        </Box>
    }
}

fn drop_park_zone(
    drop_id: &'static str,
    parked: Signal<bool>,
    parked_body: impl FnOnce() -> Element,
) -> Element {
    let inside = parked.get();
    let zone = ui! {
        <Box
            min_h="4.5rem"
            min_w="8rem"
            grow="1"
            p="0.75rem"
            rounded="0.75rem"
            bg="muted"
            border_style="dashed"
            border_width="1px"
            border_color="border"
            items="center"
            justify="center"
        >
            { if inside {
                parked_body()
            } else {
                ui! {
                    <Label text_size="0.75rem" color="muted-foreground">"放到这里"</Label>
                }
            } }
        </Box>
    };
    let ctx = use_drag();
    zone.reactive_sync(move |el, _| {
        let over = zone_over_peek(ctx.as_ref(), drop_id);
        let bg = parse_color(if over { "accent" } else { "muted" });
        let border = parse_color(if over { "primary" } else { "border" });
        let mut changed = LayoutStatus::Clean;
        if el.style.bg_color != bg {
            el.style.bg_color = bg;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_color != border {
            el.style.border_color = border;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        changed
    })
}

/// solid-dnd `isActiveDroppable` analog: collision `hover_list`, not CSS `:hover`.
fn kanban_column(list: &'static str, title: &str, body: Element) -> Element {
    let title = title.to_string();
    let ctx = use_drag();
    let el = ui! {
        <VStack
            grow="1"
            min_h="8rem"
            p="0.5rem"
            gap="0.375rem"
            rounded="0.5rem"
            bg="muted"
            w="50%"
            border_style="solid"
            border_width="1px"
            border_color="border"
            overflow="visible"
        >
            <Label font_weight="600" text_size="0.75rem">{ title }</Label>
            { body }
        </VStack>
    };
    el.reactive_sync(move |el, _| {
        let over = zone_list_hover_peek(ctx.as_ref(), list);
        let bg = parse_color(if over { "accent" } else { "muted" });
        let border = parse_color(if over { "primary" } else { "border" });
        let mut changed = LayoutStatus::Clean;
        if el.style.bg_color != bg {
            el.style.bg_color = bg;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_color != border {
            el.style.border_color = border;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        changed
    })
}

fn card_chip(label: &str) -> Element {
    ui! {
        <HStack
            p="0.5rem 0.65rem"
            gap="0.5rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color="border"
            cursor="grab"
            bg="transparent"
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" color="foreground">{ label.to_string() }</Label>
        </HStack>
    }
}

/// Horizontal Sortable chips must not stretch `w=100%` (that would wrap into a column).
fn compact_chip(label: &str) -> Element {
    ui! {
        <HStack
            p="0.375rem 0.65rem"
            gap="0.375rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color="border"
            cursor="grab"
            bg="card"
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" color="foreground">{ label.to_string() }</Label>
        </HStack>
    }
}

fn swap_chip(label: &str, list: &'static str, index: usize, item_id: String) -> Element {
    let ctx = use_drag();
    compact_chip(label).reactive_sync(move |el, _| {
        let over = ctx.as_ref().is_some_and(|c| {
            c.active_draggable.peek().is_some()
                && c.hover_list.peek().as_deref() == Some(list)
                && c.hover_index.peek() == Some(index)
                && c.active_draggable.peek().as_deref() != Some(item_id.as_str())
        });
        let border = parse_color(if over { "primary" } else { "border" });
        let bg = parse_color(if over { "accent" } else { "card" });
        let mut changed = LayoutStatus::Clean;
        if el.style.bg_color != bg {
            el.style.bg_color = bg;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_color != border {
            el.style.border_color = border;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        changed
    })
}

fn multi_select_chip(label: &str, item_id: &str) -> Element {
    let ctx = use_drag();
    let selected = ctx
        .as_ref()
        .is_some_and(|c| c.selected.get().iter().any(|x| x == item_id));
    let bg = if selected { "accent" } else { "card" };
    let border = if selected { "primary" } else { "border" };
    ui! {
        <HStack
            p="0.375rem 0.65rem"
            gap="0.375rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color={border}
            cursor="grab"
            bg={bg}
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" color="foreground">{ label.to_string() }</Label>
        </HStack>
    }
}

fn apply_two_list_event(
    left: Signal<Vec<String>>,
    right: Signal<Vec<String>>,
    left_id: &str,
    ev: &DragEvent,
) {
    let from = ev.from_list.as_deref().unwrap_or("");
    let to = ev.to_list.as_deref().unwrap_or(from);
    if from == to {
        let mut v = if from == left_id {
            left.peek()
        } else {
            right.peek()
        };
        apply_event(&mut v, None, ev);
        if from == left_id {
            left.set(v);
        } else {
            right.set(v);
        }
        return;
    }
    let mut src = if from == left_id {
        left.peek()
    } else {
        right.peek()
    };
    let mut dst = if to == left_id {
        left.peek()
    } else {
        right.peek()
    };
    apply_event(&mut src, Some(&mut dst), ev);
    if from == left_id {
        left.set(src);
        right.set(dst);
    } else {
        right.set(src);
        left.set(dst);
    }
}

fn keyed_swap_chips(
    rows_id: &'static str,
    items: Signal<Vec<String>>,
    list: &'static str,
) -> Element {
    let items_snap = items.get();
    let rows = rui::for_keyed!(
        rows_id,
        items_snap.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items_snap.iter().position(|x| x == &name).unwrap_or(0);
            let id = format!("{list}:{name}");
            ui! {
                <SortableItem
                    id={id.clone()}
                    item_type="swap"
                    accept="swap"
                    group="swap-demo"
                    list={list}
                    index={i}
                    source_opacity={0.4}
                >
                    { swap_chip(&name, list, i, id.clone()) }
                </SortableItem>
            }
        }
    );
    Element::from(vstack().w("100%").gap("0.375rem").children(rows))
}

fn keyed_multi_chips(items: Signal<Vec<String>>) -> Element {
    let items_snap = items.get();
    let rows = rui::for_keyed!(
        "gal-drag-multi-rows",
        items_snap.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items_snap.iter().position(|x| x == &name).unwrap_or(0);
            let id = format!("multi:{name}");
            ui! {
                <SortableItem
                    id={id.clone()}
                    item_type="multi"
                    accept="multi"
                    list="multi-list"
                    index={i}
                    source_opacity={0.4}
                >
                    { multi_select_chip(&name, &id) }
                </SortableItem>
            }
        }
    );
    Element::from(vstack().w("100%").gap("0.375rem").children(rows))
}

fn rsx_swap_board(
    left: Signal<Vec<String>>,
    right: Signal<Vec<String>>,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    ui! {
        <DragProvider id="gal-drag-swap" collision="closest-center" sort_mode="swap" onDragEnd={on_end}>
            <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                <VStack gap="0.375rem" min_w="10rem">
                    <Label text_size="0.75rem" color="muted-foreground">"左 · 不让位，高亮对端"</Label>
                    <SortableList id="swap-left" group="swap-demo" accept="swap">
                        { keyed_swap_chips("gal-drag-swap-left-rows", left, "swap-left") }
                    </SortableList>
                </VStack>
                <VStack gap="0.375rem" min_w="10rem">
                    <Label text_size="0.75rem" color="muted-foreground">"右 · 跨列 mem::swap"</Label>
                    <SortableList id="swap-right" group="swap-demo" accept="swap">
                        { keyed_swap_chips("gal-drag-swap-right-rows", right, "swap-right") }
                    </SortableList>
                </VStack>
            </HStack>
            <DragOverlay class="shadow-lg" />
        </DragProvider>
    }
}

fn multi_selected_hud(n: usize) -> Element {
    let ctx = use_drag();
    let selected_n = ctx.as_ref().map(|c| c.selected.get().len()).unwrap_or(0);
    let hint = format!("共 {n} 项 · 已选 {selected_n} · 单击选中，拖已选项一次带走");
    ui! {
        <Label text_size="0.75rem" color="muted-foreground">{ hint }</Label>
    }
}

fn rsx_multi_board(
    items: Signal<Vec<String>>,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    let n = items.get().len();
    ui! {
        <DragProvider id="gal-drag-multi" collision="closest-center" multi_drag={true} onDragEnd={on_end}>
            <VStack gap="0.375rem" w="100%">
                { multi_selected_hud(n) }
                <SortableList id="multi-list" accept="multi">
                    { keyed_multi_chips(items) }
                </SortableList>
                <DragOverlay class="shadow-lg" />
            </VStack>
        </DragProvider>
    }
}

/// Overlay `child_for`: typed ghost smaller than the source (`align=center` pins the midpoint to the pointer).
fn typed_overlay_ghost(item: &DragItemCtx) -> Element {
    let (icon, caption) = match item.item_type.as_str() {
        "file" => ("file-text", "文件"),
        "folder" => ("folder", "文件夹"),
        _ => ("grip-vertical", item.id.as_str()),
    };
    ui! {
        <HStack
            p="0.35rem 0.6rem"
            gap="0.375rem"
            items="center"
            rounded="9999px"
            bg="primary"
        >
            <Icon name={icon} size="12px" color="primary-foreground" pointer_events="none" />
            <Label color="primary-foreground" text_size="0.75rem">{ caption.to_string() }</Label>
        </HStack>
    }
}

fn collision_hit_zone(drop_id: &'static str, title: &str, body: Element) -> Element {
    let title = title.to_string();
    let ctx = use_drag();
    let el = ui! {
        <VStack
            w="100%"
            h="100%"
            p="0.5rem"
            gap="0.375rem"
            rounded="0.75rem"
            bg="muted"
            border_style="dashed"
            border_width="1px"
            border_color="border"
        >
            <Label text_size="0.75rem" font_weight="600">{ title }</Label>
            { body }
        </VStack>
    };
    el.reactive_sync(move |el, _| {
        let over = zone_over_peek(ctx.as_ref(), drop_id);
        let bg = parse_color(if over { "accent" } else { "muted" });
        let border = parse_color(if over { "primary" } else { "border" });
        let mut changed = LayoutStatus::Clean;
        if el.style.bg_color != bg {
            el.style.bg_color = bg;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_color != border {
            el.style.border_color = border;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        changed
    })
}

/// Headless `<DragHandle>` wrapping author chrome (not a built-in grip widget).
fn handle_grip() -> Element {
    ui! {
        <DragHandle>
            <Box p="0.25rem" cursor="grab" items="center" justify="center" bg="transparent">
                <Icon name="grip-vertical" size="14px" color="muted-foreground" />
            </Box>
        </DragHandle>
    }
}

fn handle_row() -> Element {
    ui! {
        <HStack
            p="0.5rem 0.65rem"
            gap="0.5rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color="border"
            bg="transparent"
        >
            { handle_grip() }
            <Label text_size="0.875rem" grow="1" color="foreground">"拖左侧手柄；点文字不会启动"</Label>
        </HStack>
    }
}

fn ignore_row() -> Element {
    ui! {
        <HStack
            p="0.5rem 0.65rem"
            gap="0.5rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color="border"
            cursor="grab"
            bg="transparent"
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" grow="1" color="foreground">"拖卡片；点按钮不启动"</Label>
            <DragIgnore>
                <Button size="sm" variant="outline" pointer_events="auto">"忽略"</Button>
            </DragIgnore>
        </HStack>
    }
}

fn token_chip() -> Element {
    ui! {
        <Box p="0.5rem 0.75rem" rounded="0.5rem" bg="primary" cursor="grab">
            <Label color="primary-foreground" text_size="0.875rem">"Token · Builder"</Label>
        </Box>
    }
}

fn persist_free_pair(
    a: Signal<[f32; 2]>,
    b: Signal<[f32; 2]>,
    id_a: &'static str,
    id_b: &'static str,
) -> impl FnMut(&DragEvent) {
    move |ev: &DragEvent| {
        if ev.draggable_id == id_a {
            let p = a.peek();
            a.set([p[0] + ev.delta[0], p[1] + ev.delta[1]]);
        } else if ev.draggable_id == id_b {
            let p = b.peek();
            b.set([p[0] + ev.delta[0], p[1] + ev.delta[1]]);
        }
    }
}

fn persist_delta_pos(pos: Signal<[f32; 2]>) -> impl FnMut(&DragEvent) {
    move |ev: &DragEvent| {
        let p = pos.peek();
        pos.set([p[0] + ev.delta[0], p[1] + ev.delta[1]]);
    }
}

/// Paint-only background: keep the viewport-space lattice used by SnapGrid,
/// including after the page scrolls, without rebuilding the draggable children.
pub(crate) fn snap_grid_lines(grids: &[(SnapGrid, f32)], visible: Signal<bool>) -> Element {
    let mut layers = Vec::new();
    for &(grid, alpha) in grids {
        let origin = Rc::new(Cell::new([0.0, 0.0]));
        let painted_origin = origin.clone();
        let layer: Element = canvas()
            .w("100%")
            .h("100%")
            .paint(move |ctx, size| {
                ctx.clear("transparent");
                ctx.set_fill("foreground");
                for axis in 0..2 {
                    let step = [grid.x.abs(), grid.y.abs()][axis];
                    if !step.is_finite() || step < 1.0 {
                        continue;
                    }
                    let mut p = (grid.offset[axis] - painted_origin.get()[axis]).rem_euclid(step);
                    while p < [size.w, size.h][axis] {
                        if axis == 0 {
                            ctx.fill_rect(p, 0.0, 1.0, size.h);
                        } else {
                            ctx.fill_rect(0.0, p, size.w, 1.0);
                        }
                        p += step;
                    }
                }
                LayoutStatus::Clean
            })
            .into();
        let painted_color = Cell::new(parse_color("foreground"));
        layers.push(
            layer
                .absolute()
                .left(0.0)
                .top(0.0)
                .opacity(alpha)
                .pointer_events_none()
                .reactive_sync(move |el, _| {
                    let bounds = el.viewport_layout_bounds();
                    let next = [bounds[0], bounds[1]];
                    let color = parse_color("foreground");
                    let moved = origin.replace(next) != next;
                    let recolored = painted_color.replace(color) != color;
                    if !moved && !recolored {
                        return LayoutStatus::Clean;
                    }
                    if let Some(id) = el.get_attr(rui::primitives::canvas::ATTR_CANVAS_PAINT) {
                        rui::primitives::canvas::mark_canvas_dirty_hex(id);
                    }
                    LayoutStatus::StyleChanged
                }),
        );
    }
    Element::new()
        .absolute()
        .left(0.0)
        .top(0.0)
        .w("100%")
        .h("100%")
        .opacity(if visible.peek() { 1.0 } else { 0.0 })
        .pointer_events_none()
        .children(layers)
        .reactive_sync(move |el, _| {
            let opacity = if visible.peek() { 1.0 } else { 0.0 };
            if el.style.opacity == opacity {
                return LayoutStatus::Clean;
            }
            el.style.opacity = opacity;
            LayoutStatus::StyleChanged
        })
}

fn resize_handle(rect: Signal<[f32; 4]>, left: f32, top: f32, edges: Edges) -> Element {
    let cursor = edges.cursor().to_string();
    let chrome: Element = ui! {
        <Box
            absolute={true}
            left={left}
            top={top}
            w="12px"
            h="12px"
            rounded="2px"
            bg="primary"
            border_style="solid"
            border_width="1px"
            border_color="primary-foreground"
            cursor={cursor}
            z_index={2}
        />
    };
    interactive(chrome)
        .on_mousedown(move |e: &MouseEvent| {
            let r = rect.peek();
            begin_resize(
                BoxRect::from_xywh(r[0], r[1], r[2], r[3]),
                [e.x, e.y],
                ResizeOpts::default()
                    .edges(edges)
                    .invert(Invert::Reposition)
                    .min_size(64.0, 48.0)
                    .autoscroll(true),
                Some("gal_drag_resize_rect".into()),
            );
        })
        .into()
}

fn resize_board(rect: Signal<[f32; 4]>) -> Element {
    let r = rect.get();
    let [x, y, w, h] = r;
    let hx = 6.0;
    ui! {
        <ScrollArea id="gal-drag-resize-scroll" h="14rem" w="100%" overflow_y="auto">
            <Box relative={true} w="100%" min_h="28rem" rounded="0.75rem" bg="background" border_style="dashed" border_width="1px" border_color="border">
                <Box
                    absolute={true}
                    left={x}
                    top={y}
                    w={w}
                    h={h}
                    rounded="0.5rem"
                    bg="card"
                    border_style="solid"
                    border_width="2px"
                    border_color="primary"
                    items="center"
                    justify="center"
                >
                    <VStack items="center" gap="0.125rem" pointer_events="none">
                        <Label font_weight="700" text_size="0.875rem">"resize"</Label>
                        <Label color="muted-foreground" text_size="0.6875rem">
                            { format!("{:.0}×{:.0}", w, h) }
                        </Label>
                        <Label color="muted-foreground" text_size="0.625rem">"autoscroll · 拖到边缘滚"</Label>
                    </VStack>
                </Box>
                { resize_handle(rect, x - hx, y - hx, Edges::top_left()) }
                { resize_handle(rect, x + w * 0.5 - hx, y - hx, Edges::top()) }
                { resize_handle(rect, x + w - hx, y - hx, Edges::top_right()) }
                { resize_handle(rect, x - hx, y + h * 0.5 - hx, Edges::left()) }
                { resize_handle(rect, x + w - hx, y + h * 0.5 - hx, Edges::right()) }
                { resize_handle(rect, x - hx, y + h - hx, Edges::bottom_left()) }
                { resize_handle(rect, x + w * 0.5 - hx, y + h - hx, Edges::bottom()) }
                { resize_handle(rect, x + w - hx, y + h - hx, Edges::bottom_right()) }
            </Box>
        </ScrollArea>
    }
}

fn gesture_dot(glyph: &'static str, pos: Signal<[f32; 2]>) -> Element {
    let p = pos.get();
    let glyph = glyph.to_string();
    ui! {
        <Box
            absolute={true}
            left={p[0]}
            top={p[1]}
            w="2.25rem"
            h="2.25rem"
            rounded="9999px"
            bg="primary"
            items="center"
            justify="center"
            cursor="grab"
            z_index={2}
        >
            <Label color="primary-foreground" font_weight="700" pointer_events="none">{ glyph }</Label>
        </Box>
    }
}

/// Overlapping free-move tiles: light opaque fill + one huge unique glyph.
/// Overlap zone must show only the front letter; both letters = glyph bleed.
fn free_move_tile(
    glyph: &'static str,
    caption: &str,
    pos: Signal<[f32; 2]>,
    z: i32,
    tone: &'static str,
) -> Element {
    let p = pos.get();
    let glyph = glyph.to_string();
    let caption = caption.to_string();
    if tone == "accent" {
        ui! {
            <Box
                absolute={true}
                left={p[0]}
                top={p[1]}
                z_index={z}
                w="11rem"
                h="5.75rem"
                min_w="11rem"
                min_h="5.75rem"
                rounded="0.75rem"
                bg="card"
                border_style="solid"
                border_width="3px"
                border_color="destructive"
                cursor="grab"
                items="center"
                justify="center"
                overflow="hidden"
            >
                <VStack items="center" justify="center" gap="0.125rem" pointer_events="none">
                    <Label color="destructive" text_size="2.75rem" font_weight="800" leading="none">
                        { glyph }
                    </Label>
                    <Label color="muted-foreground" text_size="0.6875rem" font_weight="600">
                        { caption }
                    </Label>
                </VStack>
            </Box>
        }
    } else if tone == "secondary" {
        ui! {
            <Box
                absolute={true}
                left={p[0]}
                top={p[1]}
                z_index={z}
                w="11rem"
                h="5.75rem"
                min_w="11rem"
                min_h="5.75rem"
                rounded="0.75rem"
                bg="muted"
                border_style="solid"
                border_width="3px"
                border_color="secondary-foreground"
                cursor="grab"
                items="center"
                justify="center"
                overflow="hidden"
            >
                <VStack items="center" justify="center" gap="0.125rem" pointer_events="none">
                    <Label color="foreground" text_size="2.75rem" font_weight="800" leading="none">
                        { glyph }
                    </Label>
                    <Label color="muted-foreground" text_size="0.6875rem" font_weight="600">
                        { caption }
                    </Label>
                </VStack>
            </Box>
        }
    } else {
        ui! {
            <Box
                absolute={true}
                left={p[0]}
                top={p[1]}
                z_index={z}
                w="11rem"
                h="5.75rem"
                min_w="11rem"
                min_h="5.75rem"
                rounded="0.75rem"
                bg="card"
                border_style="solid"
                border_width="3px"
                border_color="primary"
                cursor="grab"
                items="center"
                justify="center"
                overflow="hidden"
            >
                <VStack items="center" justify="center" gap="0.125rem" pointer_events="none">
                    <Label color="primary" text_size="2.75rem" font_weight="800" leading="none">
                        { glyph }
                    </Label>
                    <Label color="muted-foreground" text_size="0.6875rem" font_weight="600">
                        { caption }
                    </Label>
                </VStack>
            </Box>
        }
    }
}

fn list_chip(label: &str) -> Element {
    ui! {
        <HStack
            w="100%"
            p="0.5rem 0.65rem"
            gap="0.5rem"
            items="center"
            rounded="0.5rem"
            border_style="solid"
            border_width="1px"
            border_color="border"
            cursor="grab"
            bg="card"
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" grow="1" color="foreground">{ label.to_string() }</Label>
        </HStack>
    }
}

/// Overlay rotate from ghost travel vs arena: NW −45°, NE 45°, SW −135°, SE 135°.
/// Center stays 0°. `origin` is the ghost top-left; `extent` is ghost size so the
/// clamped corners of `restrict` map onto the labeled angles (pointer-in-arena
/// never reaches 0/1 while the grab sits on the card).
/// Appearance only — CSS rotate on the overlay, not a kit modifier.
fn corner_card_angle(origin: [f32; 2], bounds: [f32; 4], extent: [f32; 2]) -> f32 {
    let travel_w = (bounds[2] - extent[0]).max(1.0);
    let travel_h = (bounds[3] - extent[1]).max(1.0);
    let u = ((origin[0] - bounds[0]) / travel_w).clamp(0.0, 1.0);
    let v = ((origin[1] - bounds[1]) / travel_h).clamp(0.0, 1.0);
    let top = -45.0 + 90.0 * u;
    let bot = -135.0 + 270.0 * u;
    top + (bot - top) * v
}

fn corner_overlay_angle(scope: &str, arena: &str) -> f32 {
    let Some(bounds) = last_hit_rect(scope, arena) else {
        return 0.0;
    };
    let origin = last_overlay_pos(scope).or_else(last_pointer);
    let Some(origin) = origin else {
        return 0.0;
    };
    let extent = last_drag_extent(scope).unwrap_or([0.0, 0.0]);
    corner_card_angle(origin, bounds, extent)
}

#[cfg(test)]
mod drop_position_tests {
    use super::*;
    use rui::core::reactivity::create_root;
    use rui::utils::drag::{begin, end, tick, BeginDrag};

    #[test]
    fn snap_drop_persists_restricted_position_without_resnapping() {
        let (_owner, ()) = create_root(|| {
            let pos = Signal::new([80.0, 40.0]);
            let _scope = drag_scope("snap-drop-regression")
                .threshold(0.0)
                .restrict(Restrict::Rect([0.0, 0.0, 200.0, 120.0]))
                .snap(SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]))
                .on_drag_end(persist_delta_pos(pos))
                .child(Element::new);
            begin(BeginDrag {
                scope_id: "snap-drop-regression".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(500.0, 500.0, &[], [800.0, 600.0]));
            assert!(end(&[]));
            assert_eq!(pos.peek(), [160.0, 96.0]);
        });
    }
}

#[cfg(test)]
mod corner_tilt_tests {
    use super::corner_card_angle;

    #[test]
    fn travel_corners_match_labels() {
        let bounds = [0.0, 0.0, 200.0, 120.0];
        let extent = [40.0, 24.0];
        let nw = corner_card_angle([0.0, 0.0], bounds, extent);
        let ne = corner_card_angle([160.0, 0.0], bounds, extent);
        let sw = corner_card_angle([0.0, 96.0], bounds, extent);
        let se = corner_card_angle([160.0, 96.0], bounds, extent);
        let mid = corner_card_angle([80.0, 48.0], bounds, extent);
        assert!((nw + 45.0).abs() < 0.01, "{nw}");
        assert!((ne - 45.0).abs() < 0.01, "{ne}");
        assert!((sw + 135.0).abs() < 0.01, "{sw}");
        assert!((se - 135.0).abs() < 0.01, "{se}");
        assert!(mid.abs() < 0.01, "{mid}");
    }
}

fn corner_hint(text: &str) -> Element {
    ui! {
        <Label text_size="0.6875rem" color="muted-foreground">{ text.to_string() }</Label>
    }
}

/// Keyed sortable rows. Called from a Droppable `Fn` so the Signal is re-read.
fn keyed_sortable_chips(
    rows_id: &'static str,
    items: Signal<Vec<String>>,
    item_type: &'static str,
    group: &'static str,
    list: &'static str,
    id_prefix: &'static str,
    source_opacity: f32,
) -> Element {
    let items = items.get();
    let rows = rui::for_keyed!(
        rows_id,
        items.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items.iter().position(|x| x == &name).unwrap_or(0);
            let id = if id_prefix.is_empty() {
                name.clone()
            } else {
                format!("{id_prefix}:{name}")
            };
            ui! {
                <SortableItem
                    id={id}
                    item_type={item_type}
                    accept={item_type}
                    group={group}
                    list={list}
                    index={i}
                    stack="auto"
                    source_opacity={source_opacity}
                >
                    { list_chip(&name) }
                </SortableItem>
            }
        }
    );
    Element::from(
        vstack()
            .w("100%")
            .min_h(0.0)
            .h("auto")
            .gap("0.375rem")
            .children(rows),
    )
}

fn keyed_h_chips(
    rows_id: &'static str,
    items: Signal<Vec<String>>,
    item_type: &'static str,
    list: &'static str,
    source_opacity: f32,
) -> Element {
    let items = items.get();
    let rows = rui::for_keyed!(
        rows_id,
        items.as_slice(),
        |s: &String| s.clone(),
        |label: &String| {
            let name = label.clone();
            let i = items.iter().position(|x| x == &name).unwrap_or(0);
            ui! {
                <SortableItem
                    id={name.clone()}
                    item_type={item_type}
                    accept={item_type}
                    list={list}
                    index={i}
                    stack="auto"
                    source_opacity={source_opacity}
                >
                    { compact_chip(&name) }
                </SortableItem>
            }
        }
    );
    Element::from(
        hstack()
            .w("100%")
            .min_h(0.0)
            .gap("0.375rem")
            .wrap("wrap")
            .children(rows),
    )
}

fn activation_column(
    title: &str,
    hint: &str,
    provider_id: &'static str,
    item_id: &'static str,
    activation: &'static str,
) -> Element {
    let title = title.to_string();
    let hint = hint.to_string();
    ui! {
        <VStack grow="1" min_w="14rem" gap="0.375rem">
            <Label text_size="0.75rem" font_weight="600">{ title }</Label>
            <Label text_size="0.75rem" color="muted-foreground">{ hint }</Label>
            <DragProvider
                id={provider_id}
                collision="pointer-within"
                delay_ms={400}
                threshold={40}
                activation={activation}
            >
                <Draggable id={item_id} item_type="chip" source_opacity={0}>
                    { card_chip("按住或拖开") }
                </Draggable>
                <DragOverlay class="shadow-lg" />
            </DragProvider>
        </VStack>
    }
}

fn reorder_same_list(items: Signal<Vec<String>>, list: &'static str) -> impl FnMut(&DragEvent) {
    move |ev: &DragEvent| {
        if ev.from_list.as_deref() != Some(list) {
            return;
        }
        let Some(from) = ev.from_index else {
            return;
        };
        let mut v = items.peek();
        let to = ev.to_index.unwrap_or(from);
        apply_reorder_vec(&mut v, from, to);
        items.set(v);
    }
}

/// Side-by-side Sortable column for the FLIP vs snap comparison.
fn sortable_anim_column(
    title: &str,
    provider_id: &'static str,
    list: &'static str,
    rows_id: &'static str,
    scroll_id: &'static str,
    items: Signal<Vec<String>>,
    animation: bool,
) -> Element {
    let title = title.to_string();
    let on_end = reorder_same_list(items, list);
    ui! {
        <VStack grow="1" min_w="0" gap="0.375rem">
            <Label text_size="0.75rem" font_weight="600">{ title }</Label>
            <DragProvider
                id={provider_id}
                collision="closest-center"
                axis="y"
                animation={animation}
                onDragEnd={on_end}
            >
                <ScrollArea id={scroll_id} h="11rem" w="100%" overflow_y="auto">
                    <SortableList id={list} accept="planet">
                        { keyed_sortable_chips(rows_id, items, "planet", "", list, "", 0.4) }
                    </SortableList>
                </ScrollArea>
                <DragOverlay class="shadow-lg" />
            </DragProvider>
        </VStack>
    }
}

fn apply_kanban(left: Signal<Vec<String>>, right: Signal<Vec<String>>, ev: &DragEvent) {
    let Some(from_list) = ev.from_list.as_deref() else {
        return;
    };
    let Some(from_index) = ev.from_index else {
        return;
    };
    let Some(to_list) = ev.to_list.as_deref() else {
        return;
    };
    if from_list == to_list {
        let mut v = if from_list == "todo" {
            left.peek()
        } else {
            right.peek()
        };
        let hover = ev.to_index.unwrap_or(from_index);
        apply_reorder_vec(&mut v, from_index, hover);
        if from_list == "todo" {
            left.set(v);
        } else {
            right.set(v);
        }
        return;
    }
    let mut src = if from_list == "todo" {
        left.peek()
    } else {
        right.peek()
    };
    let mut dst = if to_list == "todo" {
        left.peek()
    } else {
        right.peek()
    };
    let to_index = ev.to_index.unwrap_or(dst.len());
    apply_transfer(&mut src, &mut dst, from_index, to_index);
    if from_list == "todo" {
        left.set(src);
        right.set(dst);
    } else {
        right.set(src);
        left.set(dst);
    }
}

fn apply_nested_lists(
    folders: Signal<Vec<String>>,
    inbox: Signal<Vec<String>>,
    later: Signal<Vec<String>>,
    ev: &DragEvent,
) {
    let Some(from_list) = ev.from_list.as_deref() else {
        return;
    };
    let Some(to_list) = ev.to_list.as_deref() else {
        return;
    };
    if from_list == "tree-root" && to_list == "tree-root" {
        let Some(from_index) = ev.from_index else {
            return;
        };
        let mut v = folders.peek();
        apply_reorder_vec(&mut v, from_index, ev.to_index.unwrap_or(from_index));
        folders.set(v);
        return;
    }
    apply_kanban_named(inbox, later, "tree-inbox", "tree-later", ev);
}

fn apply_kanban_named(
    left: Signal<Vec<String>>,
    right: Signal<Vec<String>>,
    left_id: &str,
    right_id: &str,
    ev: &DragEvent,
) {
    let Some(from_list) = ev.from_list.as_deref() else {
        return;
    };
    let Some(from_index) = ev.from_index else {
        return;
    };
    let Some(to_list) = ev.to_list.as_deref() else {
        return;
    };
    if from_list != left_id && from_list != right_id {
        return;
    }
    if to_list != left_id && to_list != right_id {
        return;
    }
    if from_list == to_list {
        let mut v = if from_list == left_id {
            left.peek()
        } else {
            right.peek()
        };
        apply_reorder_vec(&mut v, from_index, ev.to_index.unwrap_or(from_index));
        if from_list == left_id {
            left.set(v);
        } else {
            right.set(v);
        }
        return;
    }
    let mut src = if from_list == left_id {
        left.peek()
    } else {
        right.peek()
    };
    let mut dst = if to_list == left_id {
        left.peek()
    } else {
        right.peek()
    };
    let to_index = ev.to_index.unwrap_or(dst.len());
    apply_transfer(&mut src, &mut dst, from_index, to_index);
    if from_list == left_id {
        left.set(src);
        right.set(dst);
    } else {
        right.set(src);
        left.set(dst);
    }
}

fn nest_list_for(folder: &str) -> &'static str {
    if folder == "Inbox" {
        "tree-inbox"
    } else {
        "tree-later"
    }
}

fn nest_rows_id(folder: &str) -> &'static str {
    if folder == "Inbox" {
        "gal-drag-nest-inbox-rows"
    } else {
        "gal-drag-nest-later-rows"
    }
}

fn nested_folder_card(title: &str, list: &'static str, body: Element) -> Element {
    let title = title.to_string();
    let ctx = use_drag();
    let header = ui! {
        <HStack gap="0.5rem" items="center" w="100%">
            { handle_grip() }
            <Label font_weight="600" text_size="0.75rem" grow="1">{ title }</Label>
        </HStack>
    };
    let el: Element = vstack()
        .w("100%")
        .min_h(0.0)
        .h("auto")
        .gap("0.375rem")
        .p("0.5rem")
        .rounded("0.5rem")
        .bg(parse_color("muted"))
        .border_style("solid")
        .border_width("1px")
        .border_color(parse_color("border"))
        .child(header)
        .child(body)
        .into();
    el.reactive_sync(move |el, _| {
        let over = zone_list_hover_peek(ctx.as_ref(), list);
        let bg = parse_color(if over { "accent" } else { "muted" });
        let border = parse_color(if over { "primary" } else { "border" });
        let mut changed = LayoutStatus::Clean;
        if el.style.bg_color != bg {
            el.style.bg_color = bg;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if el.style.border_color != border {
            el.style.border_color = border;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        changed
    })
}

fn nest_folder_tree(
    folders: Signal<Vec<String>>,
    inbox: Signal<Vec<String>>,
    later: Signal<Vec<String>>,
) -> Element {
    let items = folders.get();
    let rows = rui::for_keyed!(
        "gal-drag-nest-folders",
        items.as_slice(),
        |s: &String| s.clone(),
        |name: &String| {
            let name = name.clone();
            let i = items.iter().position(|x| x == &name).unwrap_or(0);
            let list = nest_list_for(&name);
            let rows_id = nest_rows_id(&name);
            let files = if name == "Inbox" { inbox } else { later };
            let title = name.clone();
            sortable_item(format!("folder:{name}"))
                .item_type("folder")
                .accept("folder")
                .list("tree-root")
                .index(i)
                .handle_only(true)
                .source_opacity(0.4)
                .child(move || {
                    // Do not `files.get()` here. This island's owner is the
                    // SortableItem; a re-run disposes nested `SortableList`
                    // reactive anchors (Solid child-owner). The inner list
                    // already tracks the vec via `keyed_sortable_chips`.
                    let inner = sortable_list(list)
                        .accept("file")
                        .group("nest")
                        .child(move || {
                            keyed_sortable_chips(rows_id, files, "file", "nest", list, list, 0.4)
                        });
                    nested_folder_card(&title, list, inner)
                })
        }
    );
    Element::from(
        vstack()
            .w("100%")
            .min_h(0.0)
            .h("auto")
            .gap("0.5rem")
            .children(rows),
    )
}

#[component]
pub(crate) fn DragShowcase() -> Element {
    let sort_items = use_signal("gal_drag_sort", || {
        [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    });
    let sort_hide = use_signal("gal_drag_sort_hide", || {
        [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    });
    let sort_flip = use_signal("gal_drag_sort_flip", || {
        ["Mercury", "Venus", "Earth", "Mars", "Jupiter"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let sort_snap = use_signal("gal_drag_sort_snap", || {
        ["Mercury", "Venus", "Earth", "Mars", "Jupiter"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let todo = use_signal("gal_drag_todo", || {
        ["Draft RFC", "Review PR"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let doing = use_signal("gal_drag_doing", || vec!["Write tests".to_string()]);
    let nest_folders = use_signal("gal_drag_nest_folders", || {
        vec!["Inbox".to_string(), "Later".to_string()]
    });
    let nest_inbox = use_signal("gal_drag_nest_inbox", || {
        vec!["Alpha".to_string(), "Beta".to_string()]
    });
    let nest_later = use_signal("gal_drag_nest_later", || vec!["Gamma".to_string()]);

    let inbox = use_signal("gal_drag_inbox", || false);
    let handle_inbox = use_signal("gal_drag_handle_inbox", || false);
    let custom_inbox = use_signal("gal_drag_custom_inbox", || false);
    let ghost_only = use_signal("gal_drag_ghost_only", || false);
    let alpha_park = use_signal("gal_drag_alpha_park", || false);
    let beta_park = use_signal("gal_drag_beta_park", || false);
    let arb_auto_1 = use_signal("gal_drag_arb_auto_1c", || [16.0_f32, 20.0]);
    let arb_auto_2 = use_signal("gal_drag_arb_auto_2c", || [88.0_f32, 52.0]);
    let arb_pin_1 = use_signal("gal_drag_arb_pin_1c", || [16.0_f32, 16.0]);
    let arb_pin_2 = use_signal("gal_drag_arb_pin_2c", || [88.0_f32, 48.0]);
    let arb_keep_1 = use_signal("gal_drag_arb_keep_1c", || [16.0_f32, 16.0]);
    let arb_keep_2 = use_signal("gal_drag_arb_keep_2c", || [88.0_f32, 48.0]);
    let debug_on = use_signal("gal_drag_debug", || false);
    let axis_col = use_signal("gal_drag_axis_col", || {
        ["Alpha", "Beta", "Gamma"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let axis_row = use_signal("gal_drag_axis_row", || {
        ["Mon", "Tue", "Wed", "Thu"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let snap_pos = use_signal("gal_drag_snap_pos", || [16.0_f32, 16.0]);
    let snap_multi_pos = use_signal("gal_drag_snap_multi_pos", || [16.0_f32, 16.0]);
    let snap_lines_visible = use_signal("gal_drag_snap_lines_visible", || false);
    let snap_fn_pos = use_signal("gal_drag_snap_fn_pos", || [16.0_f32, 40.0]);
    let dropzone_hud = use_signal("gal_drag_dropzone_hud", || {
        "overlap=center · 未进入".to_string()
    });
    let snap_end_pos = use_signal("gal_drag_snap_end_pos", || [16.0_f32, 16.0]);
    let start_lock_pos = use_signal("gal_drag_start_lock_pos", || [16.0_f32, 16.0]);
    let fling_pos = use_signal("gal_drag_fling_pos", || [16.0_f32, 16.0]);
    let resize_rect = use_signal("gal_drag_resize_rect", || [40.0_f32, 28.0, 160.0, 100.0]);
    let gesture_a = use_signal("gal_drag_gesture_a", || [48.0_f32, 72.0]);
    let gesture_b = use_signal("gal_drag_gesture_b", || [200.0_f32, 72.0]);
    set_gesture_opts(GestureOpts::default().autoscroll(true));
    let veto_open = use_signal("gal_drag_veto_open", || {
        vec!["Move me".to_string(), "Locked".to_string()]
    });
    let veto_empty = use_signal("gal_drag_veto_empty", Vec::<String>::new);
    let clone_tags = use_signal("gal_drag_clone", || {
        vec!["Rust".to_string(), "UI".to_string(), "Drag".to_string()]
    });
    let swap_left = use_signal("gal_drag_swap_left", || {
        vec![
            "Mercury".to_string(),
            "Venus".to_string(),
            "Earth".to_string(),
        ]
    });
    let swap_right = use_signal("gal_drag_swap_right", || {
        vec![
            "Mars".to_string(),
            "Jupiter".to_string(),
            "Saturn".to_string(),
        ]
    });
    let multi_tags = use_signal("gal_drag_multi", || {
        vec![
            "Alpha".to_string(),
            "Beta".to_string(),
            "Gamma".to_string(),
            "Delta".to_string(),
            "Epsilon".to_string(),
        ]
    });
    let shift_normal = use_signal("gal_drag_shift_normal", || {
        vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()]
    });
    let shift_pinned = use_signal("gal_drag_shift_pinned", || {
        vec!["Alpha".to_string(), "Pin".to_string(), "Gamma".to_string()]
    });
    let shift_follow = use_signal("gal_drag_shift_follow", || {
        vec![
            "Alpha".to_string(),
            "Follow".to_string(),
            "Gamma".to_string(),
        ]
    });
    let data_note = use_signal("gal_drag_data_note", || {
        "拖起看幽灵上的 payload".to_string()
    });
    let sensor_target = use_signal("gal_drag_sensor_target", || None::<String>);

    let clone_on_end = {
        let clone_tags = clone_tags.clone();
        move |ev: &DragEvent| {
            let mut v = clone_tags.peek();
            apply_event(&mut v, None, ev);
            clone_tags.set(v);
        }
    };
    let clone_board = rsx_pull_clone_board(clone_tags, clone_on_end);

    let swap_on_end = {
        let swap_left = swap_left.clone();
        let swap_right = swap_right.clone();
        move |ev: &DragEvent| {
            apply_two_list_event(swap_left, swap_right, "swap-left", ev);
        }
    };
    let swap_board = rsx_swap_board(swap_left, swap_right, swap_on_end);

    let multi_on_end = {
        let multi_tags = multi_tags.clone();
        move |ev: &DragEvent| {
            let mut v = multi_tags.peek();
            apply_event(&mut v, None, ev);
            multi_tags.set(v);
        }
    };
    let multi_board = rsx_multi_board(multi_tags, multi_on_end);

    let data_board = {
        let data_note = data_note.clone();
        Element::from(
            drag_scope("gal-drag-data")
                .on_drag_start({
                    let data_note = data_note.clone();
                    move |ev| {
                        if let Some(n) = ev
                            .data
                            .as_ref()
                            .and_then(|d| d.downcast_ref::<i32>().copied())
                        {
                            data_note.set(format!("drag_start · data={n}"));
                        }
                    }
                })
                .on_drag_end({
                    let data_note = data_note.clone();
                    move |ev| {
                        if let Some(n) = ev
                            .data
                            .as_ref()
                            .and_then(|d| d.downcast_ref::<i32>().copied())
                        {
                            data_note.set(format!("drag_end · data={n} · id={}", ev.draggable_id));
                        }
                    }
                })
                .child({
                    let data_note = data_note.clone();
                    move || {
                        let note = data_note.get();
                        ui! {
                            <VStack gap="0.375rem" w="100%">
                                <Label text_size="0.75rem" color="muted-foreground">{ note }</Label>
                                <HStack gap="0.5rem" w="100%" wrap="wrap">
                                    <Draggable
                                        id="data-a"
                                        item_type="payload"
                                        data={Rc::new(42i32)}
                                    >
                                        { compact_chip("payload 42 · RSX data=") }
                                    </Draggable>
                                    <Draggable
                                        id="data-b"
                                        item_type="payload"
                                        data={Rc::new(99i32)}
                                    >
                                        { compact_chip("payload 99") }
                                    </Draggable>
                                </HStack>
                                { Element::from(
                                    drag_overlay()
                                        .align(OverlayAlign::Center)
                                        .child_for(|item| data_payload_ghost(item)),
                                ) }
                            </VStack>
                        }
                    }
                }),
        )
    };

    let idle_hits_board = Element::from(
        drag_scope("gal-drag-idle-hits")
            .collision(CollisionAlg::PointerWithin)
            .child(|| {
                ui! {
                    <VStack gap="0.375rem" w="100%">
                        { hits_hud() }
                        <HStack gap="0.5rem" w="100%" wrap="wrap">
                            <Draggable id="idle-a" item_type="peek">
                                { compact_chip("idle draggable A") }
                            </Draggable>
                            <Draggable id="idle-b" item_type="peek">
                                { compact_chip("idle draggable B") }
                            </Draggable>
                            <Droppable id="idle-zone" accept="peek">
                                { collision_hit_zone(
                                    "idle-zone",
                                    "idle droppable",
                                    ui! {
                                        <Label text_size="0.75rem" color="muted-foreground">
                                            "未拖时 hits() 走 registered 表"
                                        </Label>
                                    },
                                ) }
                            </Droppable>
                        </HStack>
                    </VStack>
                }
            }),
    );

    let skip_on_end = move |ev: &DragEvent| match ev.from_list.as_deref() {
        Some("shift-normal") => {
            let Some(from) = ev.from_index else {
                return;
            };
            let mut v = shift_normal.peek();
            apply_reorder_vec(&mut v, from, ev.to_index.unwrap_or(from));
            shift_normal.set(v);
        }
        Some("shift-pinned") => {
            let Some(from) = ev.from_index else {
                return;
            };
            let mut v = shift_pinned.peek();
            apply_reorder_vec(&mut v, from, ev.to_index.unwrap_or(from));
            shift_pinned.set(v);
        }
        Some("shift-follow") => {
            let Some(from) = ev.from_index else {
                return;
            };
            let mut v = shift_follow.peek();
            apply_reorder_vec(&mut v, from, ev.to_index.unwrap_or(from));
            shift_follow.set(v);
        }
        _ => {}
    };
    let skip_shift_board =
        rsx_skip_flags_board(shift_normal, shift_pinned, shift_follow, skip_on_end);

    let sensor_board = {
        let sensor_target = sensor_target.clone();
        Element::from(
            drag_scope("gal-drag-sensor")
                .sensor("gallery-hotkey", {
                    let sensor_target = sensor_target.clone();
                    move |ctx| {
                        let Some(id) = sensor_target.peek() else {
                            return false;
                        };
                        sensor_target.set(None);
                        begin_draggable(&ctx.scope_id, &id, &ctx.begin)
                    }
                })
                .child({
                    let sensor_target = sensor_target.clone();
                    move || {
                        ui! {
                            <VStack gap="0.375rem" w="100%">
                                <Label text_size="0.75rem" color="muted-foreground">
                                    "`.sensor` + `run_scope_sensors`：按钮模拟键盘 activator，走与 pointerdown 相同的 `begin`。"
                                </Label>
                                <HStack gap="0.5rem" w="100%" items="center" wrap="wrap">
                                    <Draggable id="sensor-chip" item_type="sensor">
                                        { card_chip("可 pointer 或 sensor 启动") }
                                    </Draggable>
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        on_click={move || {
                                            sensor_target.set(Some("sensor-chip".into()));
                                            run_scope_sensors("gal-drag-sensor");
                                        }}
                                    >
                                        "sensor 启动"
                                    </Button>
                                </HStack>
                                { Element::from(drag_overlay()) }
                            </VStack>
                        }
                    }
                }),
        )
    };

    let builder_drop = {
        let parked = use_signal("gal_drag_parked", || false);
        Element::from(
            drag_scope("gal-drag-builder")
                .collision(CollisionAlg::PointerWithin)
                .axis(Axis::Xy)
                .restrict(Restrict::Viewport)
                .on_drag_end(park_on_drop(parked, "dock"))
                .child(move || {
                    ui! {
                        <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                            <Show when={!parked.get()}>
                                <Draggable id="token" item_type="token">
                                    { token_chip() }
                                </Draggable>
                            </Show>
                            <Show when={parked.get()}>
                                { vacated_slot("已放入右侧") }
                            </Show>
                            <Droppable id="dock" accept="token">
                                { drop_park_zone("dock", parked, || {
                                    ui! {
                                        <Draggable id="token:dock" item_type="token">
                                            { token_chip() }
                                        </Draggable>
                                    }
                                }) }
                            </Droppable>
                            <DragOverlay />
                        </HStack>
                    }
                }),
        )
    };

    let collision_prefer = Element::from(
        drag_scope("gal-drag-cfn")
            .collision(CollisionAlg::MostIntersecting)
            .collision_fn(|input| {
                if input.droppables.iter().any(|c| c.id == "hit-prefer") {
                    Some("hit-prefer".into())
                } else {
                    detect(
                        CollisionAlg::MostIntersecting,
                        input.pointer,
                        input.dragged,
                        input.droppables,
                        input.active_id,
                    )
                }
            })
            .child(|| {
                ui! {
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Draggable id="cfn-probe" item_type="probe">
                            { card_chip("探测块") }
                        </Draggable>
                        <Box relative={true} grow="1" min_w="18rem" min_h="8.5rem">
                            <Box absolute={true} left="0" top="0" w="72%" h="8rem">
                                <Droppable id="hit-outer" accept="probe">
                                    { collision_hit_zone(
                                        "hit-outer",
                                        "更大 overlap · outer",
                                        ui! {
                                            <Box h="4.5rem">
                                                <Label text_size="0.75rem" color="muted-foreground">
                                                    "MostIntersecting 会选这块"
                                                </Label>
                                            </Box>
                                        },
                                    ) }
                                </Droppable>
                            </Box>
                            <Box absolute={true} left="38%" top="1.5rem" w="58%" h="6rem">
                                <Droppable id="hit-prefer" accept="probe">
                                    { collision_hit_zone(
                                        "hit-prefer",
                                        "collision_fn · prefer",
                                        ui! {
                                            <Label text_size="0.75rem" color="muted-foreground">
                                                "重叠区函数强制点名；两槽是兄妹，不是嵌套 innermost。"
                                            </Label>
                                        },
                                    ) }
                                </Droppable>
                            </Box>
                        </Box>
                        <DragOverlay />
                    </HStack>
                }
            }),
    );

    let snap_grid = SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]);
    let snap_end_grid = SnapGrid::new(32.0, 32.0)
        .offset([16.0, 16.0])
        .range(18.0)
        .end_only(true);
    let snap_board = Element::from(
        drag_scope("gal-drag-snap")
            .axis(Axis::Xy)
            .restrict(Restrict::Parent)
            .snap(snap_grid)
            .on_drag_end(persist_delta_pos(snap_pos))
            .child(move || {
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="10rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        { snap_grid_lines(&[(snap_grid, 0.16)], snap_lines_visible) }
                        <Draggable id="snap-tile" stack="auto">
                            { free_move_tile("S", "snap 32px · restrict=parent", snap_pos, 0, "primary") }
                        </Draggable>
                    </Box>
                }
            }),
    );

    let snap_fine_grid = SnapGrid::new(8.0, 8.0);
    let snap_multi = SnapTargets::new([snap_fine_grid, snap_grid]).range(14.0);
    let snap_multi_board = Element::from(
        drag_scope("gal-drag-snap-multi")
            .axis(Axis::Xy)
            .restrict(Restrict::Parent)
            .snap_targets(snap_multi)
            .on_drag_end(persist_delta_pos(snap_multi_pos))
            .child(move || {
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="10rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        { snap_grid_lines(
                            &[(snap_fine_grid, 0.06), (snap_grid, 0.16)],
                            snap_lines_visible,
                        ) }
                        <Draggable id="snap-multi-tile" stack="auto">
                            { free_move_tile("M", "8px + 32px 双格 · 近者胜", snap_multi_pos, 0, "accent") }
                        </Draggable>
                    </Box>
                }
            }),
    );

    let snap_fn_line: SnapPointFn = Rc::new(|x, _y| Some([x, 72.0]));
    let snap_fn_board = ui! {
        <DragProvider
            id="gal-drag-snap-fn"
            axis="xy"
            restrict="parent"
            snap_fn={snap_fn_line}
            onDragEnd={persist_delta_pos(snap_fn_pos)}
        >
            <Box
                relative={true}
                w="100%"
                min_h="10rem"
                overflow="hidden"
                rounded="0.75rem"
                bg="background"
                border_style="dashed"
                border_width="1px"
                border_color="border"
            >
                <Box
                    absolute={true}
                    left="0"
                    top="72px"
                    w="100%"
                    h="1px"
                    bg="primary"
                    pointer_events="none"
                />
                <Draggable id="snap-fn-tile" stack="auto">
                    { free_move_tile("F", "snap_fn → y=72", snap_fn_pos, 0, "secondary") }
                </Draggable>
            </Box>
        </DragProvider>
    };

    let dropzone_board = ui! {
        <DragProvider id="gal-drag-dropzone">
            <HStack gap="0.75rem" w="100%" items="start">
                <Draggable id="dz-card" item_type="card" stack="auto">
                    { card_chip("拖进虚线框") }
                </Draggable>
                <Droppable
                    id="dz-target"
                    accept="card"
                    overlap="center"
                    onDragEnter={move |_| dropzone_hud.set("dragenter · 中心在槽内".into())}
                    onDragLeave={move |_| dropzone_hud.set("dragleave".into())}
                    onDrop={move |_| dropzone_hud.set("drop!".into())}
                >
                    <Box
                        min_h="8rem"
                        w="12rem"
                        rounded="0.75rem"
                        border_style="dashed"
                        border_width="2px"
                        border_color="border"
                        items="center"
                        justify="center"
                        p="0.75rem"
                    >
                        <Label text_size="0.75rem" color="muted-foreground">
                            { dropzone_hud.get() }
                        </Label>
                    </Box>
                </Droppable>
            </HStack>
        </DragProvider>
    };

    let snap_end_board = Element::from(
        drag_scope("gal-drag-snap-end")
            .axis(Axis::Xy)
            .restrict(Restrict::Parent)
            .restrict_anchor(RestrictAnchor::CENTER)
            .snap(snap_end_grid)
            .on_drag_end(persist_delta_pos(snap_end_pos))
            .child(move || {
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="10rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        <Draggable id="snap-end-tile" stack="auto">
                            { free_move_tile("E", "endOnly + range 18 · 中心钳位", snap_end_pos, 0, "secondary") }
                        </Draggable>
                    </Box>
                }
            }),
    );

    let start_lock_board = Element::from(
        drag_scope("gal-drag-start-lock")
            .axis(Axis::Start)
            .start_axis(Axis::Xy)
            .restrict(Restrict::Parent)
            .on_drag_end(persist_delta_pos(start_lock_pos))
            .child(move || {
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="10rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        <Draggable id="start-lock-tile" stack="auto">
                            { free_move_tile("L", "axis=start · 先滑再锁轴", start_lock_pos, 0, "accent") }
                        </Draggable>
                    </Box>
                }
            }),
    );

    let fling_board = Element::from(
        drag_scope("gal-drag-fling")
            .axis(Axis::Xy)
            .restrict(Restrict::Parent)
            .inertia(true)
            .on_drag_end(persist_delta_pos(fling_pos))
            .child(move || {
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="10rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        <Draggable id="fling-tile" stack="auto">
                            { free_move_tile("F", "甩出去 · inertia", fling_pos, 0, "primary") }
                        </Draggable>
                    </Box>
                }
            }),
    );

    let ga_now = gesture_a.get();
    let gb_now = gesture_b.get();
    let gesture = GestureState::from_pair([[48.0, 72.0], [200.0, 72.0]], [ga_now, gb_now]);
    let gesture_hud = format!(
        "scale={:.2}  angle={:.0}°  ds={:+.2}  da={:+.0}°",
        gesture.scale,
        gesture.angle.to_degrees(),
        gesture.ds,
        gesture.da.to_degrees()
    );
    let touch_g = last_gesture();
    let touch_hud = match touch_g {
        Some(g) => format!(
            "触屏 gesture  scale={:.2}  angle={:.0}°",
            g.scale,
            g.angle.to_degrees()
        ),
        None => "触屏双指会写 last_gesture()（桌面用两个圆点模拟）；set_gesture_opts(autoscroll) 开边缘滚".to_string(),
    };
    let preview_w = (72.0 * gesture.scale).clamp(24.0, 220.0);
    let preview_h = (48.0 * gesture.scale).clamp(16.0, 160.0);
    let preview_x = gesture.centroid[0] - preview_w * 0.5;
    let preview_y = gesture.centroid[1] - preview_h * 0.5;
    let gesture_board = Element::from(
        drag_scope("gal-drag-gesture")
            .restrict(Restrict::Parent)
            .on_drag_end(move |ev: &DragEvent| {
                let pos = if ev.draggable_id.contains("g-a") {
                    gesture_a
                } else {
                    gesture_b
                };
                let p = pos.peek();
                pos.set([p[0] + ev.delta[0], p[1] + ev.delta[1]]);
            })
            .child(move || {
                let preview = Element::from(ui! {
                    <Box
                        absolute={true}
                        left={preview_x}
                        top={preview_y}
                        w={preview_w}
                        h={preview_h}
                        rounded="0.5rem"
                        bg="accent"
                        pointer_events="none"
                    />
                })
                .opacity(0.35);
                ui! {
                    <Box
                        relative={true}
                        w="100%"
                        min_h="12rem"
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        { preview }
                        <Draggable id="g-a" stack="auto">
                            { gesture_dot("1", gesture_a) }
                        </Draggable>
                        <Draggable id="g-b" stack="auto">
                            { gesture_dot("2", gesture_b) }
                        </Draggable>
                    </Box>
                }
            }),
    );

    ui! {
        <MyCard title="Drag（utils::drag · WP-DRAG.1–2）">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utils::drag：solid-dnd 原语 + 超越 Sortable 的自动滚、实时让位、Esc 取消、方向键改插入点。替身默认跟随本体并钉在视口（才能拖出父级进另一个容器）；DragOverlay 子树可整段替换，或 Builder child_for 按 item_type 分支。松手后由 onDragEnd 把源项挂进 Droppable。不进 prelude。"
                </Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <Button
                        size="sm"
                        variant={if debug_on.get() { "default" } else { "outline" }}
                        on_click={move || {
                            let next = !debug_on.peek();
                            set_debug_drag_hits(next);
                            debug_on.set(next);
                        }}
                    >
                        { if debug_on.get() {
                            "关闭槽矩形 Debugger"
                        } else {
                            "显示槽矩形 Debugger"
                        } }
                    </Button>
                    <Label text_size="0.75rem" color="muted-foreground">
                        "进程级虚线槽：空闲蓝/灰，拖中源橙色、当前 droppable 品红加粗。对本页所有 DragProvider 生效。"
                    </Label>
                </HStack>

                <Label font_weight="600" text_size="0.875rem">"Droppable · RSX + 拖拽浮层"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "空 DragOverlay 是尚未半透明化的本体克隆；列表里的源项由套件降透明。松手落在右侧后卡片进入容器；拖到空白处放回。"
                </Label>
                <DragProvider
                    id="gal-drag-basic"
                    collision="pointer-within"
                    onDragEnd={park_on_drop(inbox, "inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!inbox.get()}>
                            <Draggable id="chip" item_type="chip">
                                { card_chip("可拖卡片") }
                            </Draggable>
                        </Show>
                        <Show when={inbox.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="inbox" accept="chip">
                            { drop_park_zone("inbox", inbox, || {
                                ui! {
                                    <Draggable id="chip:inbox" item_type="chip">
                                        { card_chip("可拖卡片") }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay />
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 四角旋转"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "容器内一张卡片。restrict 填容器 id（corners-arena）或 parent（无需给父级分配 id，钳作者容器并跳过 kit shell）。本例用 arena id，浮层钳在整块虚线框内而不是整个视口：左上 −45°、右上 45°、左下 −135°、右下 135°（中心 0°，按浮层在钳制行程内的位置插值）。缺省 restrict_overflow=visible 让旋转角透出虚线框；下例 hidden 则裁切。缺省/viewport 仍可拖出父级做跨容器投放。source_opacity 0，只看到旋转浮层；按下不改本体，一开始拖就出替身，松手卡片仍留在容器里。"
                </Label>
                <DragProvider id="gal-drag-corners" collision="pointer-within" threshold={0}>
                    <Droppable id="corners-arena">
                        <VStack
                            w="100%"
                            min_h="16rem"
                            p="0.5rem"
                            gap="0.25rem"
                            rounded="0.75rem"
                            bg="muted"
                            border_style="dashed"
                            border_width="1px"
                            border_color="border"
                            justify="between"
                        >
                            <HStack w="100%" justify="between">
                                { corner_hint("−45°") }
                                { corner_hint("45°") }
                            </HStack>
                            <Box w="100%" grow="1" items="center" justify="center">
                                <Draggable id="corner-card" item_type="chip" source_opacity={0} restrict="corners-arena">
                                    { card_chip("拖到四角") }
                                </Draggable>
                            </Box>
                            <HStack w="100%" justify="between">
                                { corner_hint("−135°") }
                                { corner_hint("135°") }
                            </HStack>
                        </VStack>
                    </Droppable>
                    <DragOverlay
                        class="shadow-lg"
                        source_opacity={0}
                        rotate={corner_overlay_angle("gal-drag-corners", "corners-arena")}
                    />
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 约束溢出隐藏"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "同样钳四角虚线框，但 restrict_overflow=hidden：旋转角裁在框内（透出是缺省 visible）。按下不改本体，一开始拖就出替身。"
                </Label>
                <DragProvider id="gal-drag-corners-clip" collision="pointer-within" threshold={0} restrict_overflow="hidden">
                    <Droppable id="corners-clip-arena">
                        <VStack
                            w="100%"
                            min_h="16rem"
                            p="0.5rem"
                            gap="0.25rem"
                            rounded="0.75rem"
                            bg="muted"
                            border_style="dashed"
                            border_width="1px"
                            border_color="border"
                            justify="between"
                        >
                            <HStack w="100%" justify="between">
                                { corner_hint("−45°") }
                                { corner_hint("45°") }
                            </HStack>
                            <Box w="100%" grow="1" items="center" justify="center">
                                <Draggable id="corner-clip-card" item_type="chip" source_opacity={0} restrict="corners-clip-arena">
                                    { card_chip("溢出隐藏") }
                                </Draggable>
                            </Box>
                            <HStack w="100%" justify="between">
                                { corner_hint("−135°") }
                                { corner_hint("135°") }
                            </HStack>
                        </VStack>
                    </Droppable>
                    <DragOverlay
                        class="shadow-lg"
                        source_opacity={0}
                        rotate={corner_overlay_angle("gal-drag-corners-clip", "corners-clip-arena")}
                    />
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 自定义子树"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "子节点整段替换克隆：按下不改本体；一开始拖就出「自定义替身」，源卡片 source_opacity 0。松手后源卡片进入容器。"
                </Label>
                <DragProvider
                    id="gal-drag-custom-ov"
                    collision="pointer-within"
                    threshold={0}
                    onDragEnd={park_on_drop(custom_inbox, "custom-inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!custom_inbox.get()}>
                            <Draggable id="custom-src" item_type="chip" source_opacity={0}>
                                { card_chip("源卡片") }
                            </Draggable>
                        </Show>
                        <Show when={custom_inbox.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="custom-inbox" accept="chip">
                            { drop_park_zone("custom-inbox", custom_inbox, || {
                                ui! {
                                    <Draggable id="custom-src:inbox" item_type="chip" source_opacity={0}>
                                        { card_chip("源卡片") }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay>
                        <HStack
                            p="0.5rem 0.75rem"
                            gap="0.5rem"
                            items="center"
                            rounded="0.5rem"
                            bg="primary"
                        >
                            <Icon name="grip-vertical" size="14px" color="primary-foreground" pointer_events="none" />
                            <Label color="primary-foreground" text_size="0.875rem">"自定义替身"</Label>
                        </HStack>
                    </DragOverlay>
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 无本体仅浮层"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "源项 source_opacity 0：按下不改本体；一开始拖源位留空，只看到跟随指针的浮层。松手进右侧；拖到空白处放回。"
                </Label>
                <DragProvider
                    id="gal-drag-hide-src"
                    threshold={0}
                    collision="pointer-within"
                    onDragEnd={park_on_drop(ghost_only, "ghost-inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!ghost_only.get()}>
                            <Draggable id="ghost-chip" item_type="chip" source_opacity={0}>
                                { card_chip("拖我 · 本体消失") }
                            </Draggable>
                        </Show>
                        <Show when={ghost_only.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="ghost-inbox" accept="chip">
                            { drop_park_zone("ghost-inbox", ghost_only, || {
                                ui! {
                                    <Draggable id="ghost-chip:inbox" item_type="chip" source_opacity={0}>
                                        { card_chip("拖我 · 本体消失") }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay />
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"Droppable · 条件 accept"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "容器1 只接受组件2，容器2 只接受组件1。不匹配的容器不会高亮，松手也不会停入。"
                </Label>
                <DragProvider
                    id="gal-drag-accept"
                    collision="pointer-within"
                    onDragEnd={park_typed(alpha_park, beta_park, "accept-c2", "accept-c1")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <VStack gap="0.375rem">
                            <Show when={!alpha_park.get()}>
                                <Draggable id="alpha" item_type="alpha" source_opacity={0}>
                                    { card_chip("组件1") }
                                </Draggable>
                            </Show>
                            <Show when={alpha_park.get()}>
                                { vacated_slot("已放入容器2") }
                            </Show>
                            <Show when={!beta_park.get()}>
                                <Draggable id="beta" item_type="beta" source_opacity={0}>
                                    { card_chip("组件2") }
                                </Draggable>
                            </Show>
                            <Show when={beta_park.get()}>
                                { vacated_slot("已放入容器1") }
                            </Show>
                        </VStack>
                        <VStack gap="0.25rem" grow="1">
                            <Label text_size="0.75rem" font_weight="600">"容器1 · accept=组件2"</Label>
                            <Droppable id="accept-c1" accept="beta">
                                { drop_park_zone("accept-c1", beta_park, || {
                                    ui! {
                                        <Draggable id="beta:c1" item_type="beta" source_opacity={0}>
                                            { card_chip("组件2") }
                                        </Draggable>
                                    }
                                }) }
                            </Droppable>
                        </VStack>
                        <VStack gap="0.25rem" grow="1">
                            <Label text_size="0.75rem" font_weight="600">"容器2 · accept=组件1"</Label>
                            <Droppable id="accept-c2" accept="alpha">
                                { drop_park_zone("accept-c2", alpha_park, || {
                                    ui! {
                                        <Draggable id="alpha:c2" item_type="alpha" source_opacity={0}>
                                            { card_chip("组件1") }
                                        </Draggable>
                                    }
                                }) }
                            </Droppable>
                        </VStack>
                    </HStack>
                    <DragOverlay />
                </DragProvider>

                <Label font_weight="600" text_size="0.875rem">"DragHandle · 自定义手柄"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "DragHandle 是无样式壳：里面可以放任意组件（这里是圆角 muted 握把）。handle={true} 时只有手柄能启动拖拽。"
                </Label>
                <DragProvider
                    id="gal-drag-handle"
                    collision="pointer-within"
                    onDragEnd={park_on_drop(handle_inbox, "handle-inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!handle_inbox.get()}>
                            <Draggable id="handled" item_type="chip" handle={true}>
                                { handle_row() }
                            </Draggable>
                        </Show>
                        <Show when={handle_inbox.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="handle-inbox" accept="chip">
                            { drop_park_zone("handle-inbox", handle_inbox, || {
                                ui! {
                                    <Draggable id="handled:inbox" item_type="chip" handle={true}>
                                        { handle_row() }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Droppable · Builder（restrict=viewport）"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "松手后 Token 进入容器，不是只改文案。拖到空白处放回。"
                </Label>
                { builder_drop }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Arbitrary drag move · 叠放层级"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "两块矩形故意叠住约一半。重叠区只应看到上层那一个大字母（蓝 A / 红 B）；若两个字母叠在一起，就是文字穿层。无 DragOverlay 时本体跟手，松手把 delta 写回 left/top。stack=auto 拖起置顶并保持；top / bottom 始终置顶或置底；keep 拖拽不改 z-index。"
                </Label>
                <Label text_size="0.75rem" color="muted-foreground">"stack=auto · 拖起置顶（初始同级，后拖的在上）"</Label>
                <DragProvider
                    id="gal-drag-arb-auto"
                    collision="pointer-within"
                    onDragEnd={persist_free_pair(arb_auto_1, arb_auto_2, "arb-auto-1", "arb-auto-2")}
                >
                    <Box
                        w="100%"
                        min_h="16rem"
                        relative={true}
                        overflow="hidden"
                        rounded="0.75rem"
                        bg="background"
                        border_style="dashed"
                        border_width="1px"
                        border_color="border"
                    >
                        <Draggable id="arb-auto-1" stack="auto">
                            { free_move_tile("A", "auto · 后拖置顶", arb_auto_1, 0, "primary") }
                        </Draggable>
                        <Draggable id="arb-auto-2" stack="auto">
                            { free_move_tile("B", "auto · 后拖置顶", arb_auto_2, 0, "accent") }
                        </Draggable>
                    </Box>
                </DragProvider>
                <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                    <VStack gap="0.375rem" grow="1" min_w="16rem">
                        <Label text_size="0.75rem" color="muted-foreground">
                            "stack=top / bottom · 始终置顶与置底（重叠区只见 T）"
                        </Label>
                        <DragProvider
                            id="gal-drag-arb-pin"
                            collision="pointer-within"
                            onDragEnd={persist_free_pair(arb_pin_1, arb_pin_2, "arb-pin-top", "arb-pin-bottom")}
                        >
                            <Box
                                w="100%"
                                min_h="14rem"
                                relative={true}
                                overflow="hidden"
                                rounded="0.75rem"
                                bg="background"
                                border_style="dashed"
                                border_width="1px"
                                border_color="border"
                            >
                                <Draggable id="arb-pin-top" stack="top">
                                    { free_move_tile("T", "始终置顶", arb_pin_1, 0, "primary") }
                                </Draggable>
                                <Draggable id="arb-pin-bottom" stack="bottom">
                                    { free_move_tile("B", "始终置底", arb_pin_2, 0, "secondary") }
                                </Draggable>
                            </Box>
                        </DragProvider>
                    </VStack>
                    <VStack gap="0.375rem" grow="1" min_w="16rem">
                        <Label text_size="0.75rem" color="muted-foreground">
                            "stack=keep · 拖拽不改层级（重叠区只见 2）"
                        </Label>
                        <DragProvider
                            id="gal-drag-arb-keep"
                            collision="pointer-within"
                            onDragEnd={persist_free_pair(arb_keep_1, arb_keep_2, "arb-keep-hi", "arb-keep-lo")}
                        >
                            <Box
                                w="100%"
                                min_h="14rem"
                                relative={true}
                                overflow="hidden"
                                rounded="0.75rem"
                                bg="background"
                                border_style="dashed"
                                border_width="1px"
                                border_color="border"
                            >
                                <Draggable id="arb-keep-hi" stack="keep">
                                    { free_move_tile("2", "keep · z=2", arb_keep_1, 2, "primary") }
                                </Draggable>
                                <Draggable id="arb-keep-lo" stack="keep">
                                    { free_move_tile("1", "keep · z=1", arb_keep_2, 1, "accent") }
                                </Draggable>
                            </Box>
                        </DragProvider>
                    </VStack>
                </HStack>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Sortable 列表 · source_opacity 0"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "同列重排 + 兄弟让位；靠近列表边缘会自动滚。scroll-lock-auto（默认）按 axis 推断；也可 scroll-lock-all / scroll-lock-x / scroll-lock-y / scroll-lock-none。Esc 取消，↑↓ 改插入点。若页面仍滚动，浮层钉在当前列。本体 source_opacity 0 时源槽留空。松手 apply_reorder_vec。"
                </Label>
                <DragProvider
                    id="gal-drag-sort-hide"
                    collision="closest-center"
                    axis="y"
                    scroll-lock-auto
                    onDragEnd={move |ev: &DragEvent| {
                        if ev.from_list.as_deref() != Some("planets-hide") {
                            return;
                        }
                        let Some(from) = ev.from_index else { return; };
                        let mut v = sort_hide.peek();
                        let to = ev.to_index.unwrap_or(from);
                        apply_reorder_vec(&mut v, from, to);
                        sort_hide.set(v);
                    }}
                >
                    <ScrollArea id="gal-drag-sort-hide-scroll" h="11rem" w="100%" overflow_y="auto">
                        <SortableList id="planets-hide" accept="planet">
                            { keyed_sortable_chips("gal-drag-sort-hide-rows", sort_hide, "planet", "", "planets-hide", "", 0.0) }
                        </SortableList>
                    </ScrollArea>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Sortable 列表 · 半透明本体"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "与上方同一套让位 / 插入索引；差别是 SortableItem source_opacity 0.4，空洞里留下半透明本体（0 时空洞是空的）。opacity 是组件默认透明度；拖起本体用 source_opacity。浮层半透明走 DragOverlay.style。滚动锁定默认 scroll-lock-auto。松手 apply_reorder_vec。"
                </Label>
                <DragProvider
                    id="gal-drag-sort"
                    collision="closest-center"
                    axis="y"
                    onDragEnd={move |ev: &DragEvent| {
                        if ev.from_list.as_deref() != Some("planets") {
                            return;
                        }
                        let Some(from) = ev.from_index else { return; };
                        let mut v = sort_items.peek();
                        let to = ev.to_index.unwrap_or(from);
                        apply_reorder_vec(&mut v, from, to);
                        sort_items.set(v);
                    }}
                >
                    <ScrollArea id="gal-drag-sort-scroll" h="11rem" w="100%" overflow_y="auto">
                        <SortableList id="planets" accept="planet">
                            { keyed_sortable_chips("gal-drag-sort-rows", sort_items, "planet", "", "planets", "", 0.4) }
                        </SortableList>
                    </ScrollArea>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Sortable · 让位平移动画"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "DragProvider animation 控制兄弟让位 / 半透本体就位的 translate FLIP。左侧默认开（150ms ease）；右侧关闭后即时跳位。Builder 为 .animation(false)。松手都 apply_reorder_vec。"
                </Label>
                <HStack gap="0.75rem" w="100%" items="start">
                    { sortable_anim_column(
                        "animation 开（默认）",
                        "gal-drag-sort-flip",
                        "planets-flip",
                        "gal-drag-sort-flip-rows",
                        "gal-drag-sort-flip-scroll",
                        sort_flip,
                        true,
                    ) }
                    { sortable_anim_column(
                        "animation 关",
                        "gal-drag-sort-snap",
                        "planets-snap",
                        "gal-drag-sort-snap-rows",
                        "gal-drag-sort-snap-scroll",
                        sort_snap,
                        false,
                    ) }
                </HStack>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"跨容器（Sortable group）"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "两列共享 group=board。同列半透本体留在原槽；跨列时本体平移到目标槽。容器高亮只打当前投放目标（hover_list / rui_drag_over），源列保持静息样式。颜色由调用方 reactive_sync 自定，不是 CSS :hover。松手 apply_transfer。"
                </Label>
                <DragProvider
                    id="gal-drag-board"
                    collision="most-intersecting"
                    onDragEnd={move |ev: &DragEvent| apply_kanban(todo, doing, ev)}
                >
                    <HStack gap="0.75rem" w="100%" items="stretch">
                        <Droppable id="todo" list="todo" group="board" accept="task">
                            { kanban_column(
                                "todo",
                                "Todo",
                                keyed_sortable_chips("gal-drag-todo-rows", todo, "task", "board", "todo", "todo", 0.4),
                            ) }
                        </Droppable>
                        <Droppable id="doing" list="doing" group="board" accept="task">
                            { kanban_column(
                                "doing",
                                "Doing",
                                keyed_sortable_chips("gal-drag-doing-rows", doing, "task", "board", "doing", "doing", 0.4),
                            ) }
                        </Droppable>
                    </HStack>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"嵌套列表"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "文件夹是外层 SortableItem（手柄拖动重排）；内部 SortableList 接文件。指针落在内层空槽时 hover 走内层，不会被外层文件夹行抢走。文件 group=nest 在文件夹之间 apply_transfer；文件夹 list=tree-root。"
                </Label>
                <DragProvider
                    id="gal-drag-nest"
                    collision="most-intersecting"
                    onDragEnd={move |ev: &DragEvent| {
                        apply_nested_lists(nest_folders, nest_inbox, nest_later, ev);
                    }}
                >
                    <SortableList id="tree-root" accept="folder">
                        { nest_folder_tree(nest_folders, nest_inbox, nest_later) }
                    </SortableList>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"DragOverlay · child_for + align=center"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "Builder child_for 按 item_type 画小幽灵（文件 / 文件夹），align=center 把幽灵中心钉在鼠标，不再贴抓点。RSX 函数 children 第一刀只走 Builder。"
                </Label>
                <DragProvider id="gal-drag-child-for" collision="pointer-within">
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Draggable id="cf-file" item_type="file" source_opacity={0}>
                            { card_chip("源 · 文件卡片") }
                        </Draggable>
                        <Draggable id="cf-folder" item_type="folder" source_opacity={0}>
                            { card_chip("源 · 文件夹卡片") }
                        </Draggable>
                    </HStack>
                    { Element::from(
                        drag_overlay()
                            .align(OverlayAlign::Center)
                            .source_opacity(0.0)
                            .child_for(|item| typed_overlay_ghost(item)),
                    ) }
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"SortableList.direction · 同 Provider 双轴"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "Provider axis=xy 不锁幽灵；左列 SortableList direction=y、右行 direction=x。未写 direction 的列表仍继承 Provider。两列不同 list / item_type，只在各自内部 apply_reorder_vec。"
                </Label>
                <DragProvider
                    id="gal-drag-axes"
                    collision="closest-center"
                    axis="xy"
                    onDragEnd={move |ev: &DragEvent| {
                        match ev.from_list.as_deref() {
                            Some("axis-col") => {
                                let Some(from) = ev.from_index else { return; };
                                let mut v = axis_col.peek();
                                apply_reorder_vec(&mut v, from, ev.to_index.unwrap_or(from));
                                axis_col.set(v);
                            }
                            Some("axis-row") => {
                                let Some(from) = ev.from_index else { return; };
                                let mut v = axis_row.peek();
                                apply_reorder_vec(&mut v, from, ev.to_index.unwrap_or(from));
                                axis_row.set(v);
                            }
                            _ => {}
                        }
                    }}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <VStack grow="1" min_w="12rem" gap="0.375rem">
                            <Label text_size="0.75rem" font_weight="600">"direction=y"</Label>
                            <SortableList id="axis-col" accept="col-tag" direction="y">
                                { keyed_sortable_chips(
                                    "gal-drag-axis-col-rows",
                                    axis_col,
                                    "col-tag",
                                    "",
                                    "axis-col",
                                    "",
                                    0.4,
                                ) }
                            </SortableList>
                        </VStack>
                        <VStack grow="1" min_w="16rem" gap="0.375rem">
                            <Label text_size="0.75rem" font_weight="600">"direction=x"</Label>
                            <SortableList id="axis-row" accept="row-tag" direction="x">
                                { keyed_h_chips(
                                    "gal-drag-axis-row-chips",
                                    axis_row,
                                    "row-tag",
                                    "axis-row",
                                    0.4,
                                ) }
                            </SortableList>
                        </VStack>
                    </HStack>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Activation · Or 对 And"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "两边 delay_ms=400、threshold=40。And（默认）必须同时满足距离和延迟；Or 是距离或延迟（solid-dnd 指针 sensor）。按住不动约 0.4s，只有右侧会抬起幽灵。"
                </Label>
                <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                    { activation_column(
                        "activation=and",
                        "要挪够 40px 并且等到 400ms",
                        "gal-drag-act-and",
                        "act-and",
                        "and",
                    ) }
                    { activation_column(
                        "activation=or",
                        "挪够 40px 或按住 400ms",
                        "gal-drag-act-or",
                        "act-or",
                        "or",
                    ) }
                </HStack>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"collision_fn · 优先点名"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "两块兄妹槽部分重叠（不是嵌套 innermost）。Builder collision_fn 只要候选里有 hit-prefer 就点名它；把探测块拖进重叠区时应高亮右侧小槽，枚举 MostIntersecting 会选左侧更大 overlap。"
                </Label>
                { collision_prefer }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"use_drag().hits · can_drop · empty_insert"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "HUD 订 active_droppable 再 peek hits()（不订 overlay_pos）。Locked 被 can_drop 否决，拖到空列不会停入。空列 empty_insert=48 放大命中盒。松手 apply_transfer。"
                </Label>
                <DragProvider
                    id="gal-drag-veto"
                    collision="most-intersecting"
                    empty_insert={48}
                    canDrop={move |ev: &DragEvent| !ev.draggable_id.contains("Locked")}
                    onDragEnd={move |ev: &DragEvent| {
                        apply_kanban_named(veto_open, veto_empty, "veto-open", "veto-empty", ev);
                    }}
                >
                    { hits_hud() }
                    <HStack gap="0.75rem" w="100%" items="stretch">
                        <Droppable id="veto-open" list="veto-open" group="veto" accept="veto">
                            { kanban_column(
                                "veto-open",
                                "可拖 · Locked 否决",
                                keyed_sortable_chips(
                                    "gal-drag-veto-open-rows",
                                    veto_open,
                                    "veto",
                                    "veto",
                                    "veto-open",
                                    "veto-open",
                                    0.4,
                                ),
                            ) }
                        </Droppable>
                        <Droppable id="veto-empty" list="veto-empty" group="veto" accept="veto">
                            { kanban_column(
                                "veto-empty",
                                "空列 · empty_insert=48",
                                keyed_sortable_chips(
                                    "gal-drag-veto-empty-rows",
                                    veto_empty,
                                    "veto",
                                    "veto",
                                    "veto-empty",
                                    "veto-empty",
                                    0.4,
                                ),
                            ) }
                        </Droppable>
                    </HStack>
                    <DragOverlay class="shadow-lg" />
                </DragProvider>

                <Separator />
                <HStack w="100%" items="center" justify="between" gap="0.75rem" wrap="wrap">
                    <Label font_weight="600" text_size="0.875rem">"SnapGrid · restrict=parent"</Label>
                    <Switch id="gal-drag-snap-lines" checked={snap_lines_visible}>
                        "网格线（两个区域）"
                    </Switch>
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    "Builder .snap(32px) 在 modifier 管道里取整幽灵原点；松手把 left/top 写回网格。restrict=parent 钳虚线框，无需给容器分配 id。无 Overlay：本体跟手。"
                </Label>
                { snap_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"SnapTargets · 双网格近者胜"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "interact snap.targets：8px 细格 + 32px 粗格（offset 16）；range=14 内选最近格点。"
                </Label>
                { snap_multi_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"snap_fn · 水平磁吸线"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "RSX `<DragProvider snap_fn={…}>` 与 Builder `.snap_fn` 同管道；把幽灵 Y 吸到 y=72。"
                </Label>
                { snap_fn_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"dropzone · overlap=center + 生命周期"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "RSX `<Droppable overlap=\"center\" onDragEnter={…} onDrop={…}>` 与 Builder `droppable().overlap(Center)` 双写。"
                </Label>
                { dropzone_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"SnapGrid endOnly + range · RestrictAnchor::Center"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "拖中自由走；靠近格子 18px 内松手才吸（interact endOnly + range）。restrict_anchor=center 只钳中点，角可以探出虚线框。"
                </Label>
                { snap_end_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"lockAxis: start"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "第一次超过阈值的位移决定锁 X 还是 Y（interact lockAxis:start），之后跟 live layout slot。竖滑改列、横滑排序不必预声明 axis。"
                </Label>
                { start_lock_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"resize · 边/角（非拖放）"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "interact resize：八个把手、invert=reposition、min_size、`.autoscroll(true)` 边缘滚（可选）。不进 Sortable tick。ShadCN 分栏仍用 ResizablePanelGroup。"
                </Label>
                { resize_board(resize_rect) }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"gesture · 双指（桌面两点）"</Label>
                <Label text_size="0.75rem" color="muted-foreground">{ gesture_hud }</Label>
                <Label text_size="0.75rem" color="muted-foreground">{ touch_hud }</Label>
                { gesture_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"inertia · 甩出去"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "自由拖 .inertia(true)：松手按速度指数衰减（与滚动 fling 同形）。Sortable 列表仍立即提交，不跟 FLIP 抢帧。"
                </Label>
                { fling_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · pull_clone · apply_event"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<SortableItem pull_clone={{true}}>` — 拖中不让位；松手 `apply_event` insert 副本。本段纯 RSX。"
                </Label>
                { clone_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§12 · Swap · SortMode"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<DragProvider sort_mode=\"swap\">` — 拖中不让位，高亮对端 `rui_drag_swap`；松手 `apply_event` 走 `Vec::swap` / 跨列 `mem::swap`。与插入 FLIP 互斥。"
                </Label>
                { swap_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§12 · MultiDrag · 多选拖"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<DragProvider multi_drag={{true}}>` — 单击（未过阈值）切换选中；拖已选项一次带走，Overlay 叠卡。v1 无 Shift 范围选。"
                </Label>
                { multi_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · data 袋 · DragItemCtx / DragEvent"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<Draggable data={Rc::new(T)}>` / `pull_clone` / `skip_shift` / `skip_follow` 已进 RSX；Overlay `child_for` 读 `item.data_as::<T>()`，`on_drag_*` 读 `ev.data`。`<DragProvider sensor={…} sensor_id=\"…\">` 与 `.sensor` / `add_sensor` 等价。"
                </Label>
                { data_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · 空闲 hits · registered 表"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "assemble 时 register_drag_slot_meta；未拖时 `use_drag().hits()` 走 registered_hits_in_scope，不必持有 app 根。HUD 标 idle vs 拖中。"
                </Label>
                { idle_hits_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · skip_shift · skip_follow"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<SortableItem skip_shift={{true}}>` / `skip_follow={{true}}>` — 中项 RSX 属性；三列对照默认 / skip_shift / skip_follow。"
                </Label>
                { skip_shift_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · sensor 槽"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<DragProvider sensor={activator} sensor_id=\"kbd\">` / Builder `.sensor` / 运行时 `add_sensor`；`run_scope_sensors` 或 activator 内 `begin_draggable` + `ctx.begin`。Gallery 按钮走 sensor 路径。"
                </Label>
                { sensor_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"DragIgnore · disabled"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "DragIgnore 拦住内部 pointerdown（Sortable filter）。disabled 项不能启动。右侧是对照可拖卡片。"
                </Label>
                <DragProvider id="gal-drag-ignore" collision="pointer-within">
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Draggable id="ignore-row" item_type="chip">
                            { ignore_row() }
                        </Draggable>
                        <Draggable id="disabled-chip" item_type="chip" disabled={true}>
                            { card_chip("disabled · 拖不动") }
                        </Draggable>
                        <Draggable id="plain-chip" item_type="chip">
                            { card_chip("对照可拖") }
                        </Draggable>
                    </HStack>
                    <DragOverlay />
                </DragProvider>
            </VStack>
        </MyCard>
    }
}
