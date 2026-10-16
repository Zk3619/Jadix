//! WP-DRAG.1: solid-dnd / SortableJS / interact.js kit — RSX + Builder gallery.

use crate::imports::*;
use crate::shared::*;
use std::rc::Rc;

use jadix::utils::drag::{
    apply_event, apply_reorder_vec, apply_transfer, begin_draggable, detect, drag_overlay,
    drag_scope, last_drag_extent, last_hit_rect, last_overlay_pos, last_pointer, run_scope_sensors,
    set_debug_drag_hits, sortable_item, sortable_list, use_drag, Axis, CollisionAlg, DragCtx,
    DragEvent, DragItemCtx, OverlayAlign, Restrict, RestrictAnchor, SnapGrid, SnapPointFn,
    SnapTargets,
};
use jadix::utils::pointer_action::{
    begin_resize, bind_resize_rect, last_gesture, resize_on_edges, set_gesture_opts, BoxRect,
    Edges, GestureOpts, GestureState, Invert, ResizeOpts,
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

#[derive(Clone, Debug, PartialEq)]
struct SortableFlagItem {
    key: String,
    label: String,
}

fn sortable_flag_items(list: &str, labels: &[&str]) -> Vec<SortableFlagItem> {
    labels
        .iter()
        .map(|label| SortableFlagItem {
            key: format!("{list}:{label}"),
            label: (*label).into(),
        })
        .collect()
}

fn keyed_sortable_flags_chips(
    rows_id: &'static str,
    items: Signal<Vec<SortableFlagItem>>,
    list: &'static str,
    item_type: &'static str,
    source_opacity: f32,
) -> Element {
    let items_snap = items.get();
    let rows_data: Vec<_> = items_snap.iter().cloned().enumerate().collect();
    let rows = jadix::for_keyed!(
        rows_id,
        rows_data.as_slice(),
        |(_, item): &(usize, SortableFlagItem)| item.key.clone(),
        |(index, item): &(usize, SortableFlagItem)| {
            let name = item.label.clone();
            let i = *index;
            let id = item.key.clone();
            let skip_shift = id == "shift-pinned:Pin";
            let skip_follow = id == "shift-follow:Follow";
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

#[derive(Clone, Debug, PartialEq)]
struct PullCloneItem {
    key: u64,
    label: String,
}

fn apply_pull_clone_event(items: &mut Vec<PullCloneItem>, ev: &DragEvent) {
    let len = items.len();
    let key = items
        .iter()
        .map(|item| item.key)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    apply_event(items, None, ev);
    if items.len() > len {
        let index = ev.to_index.unwrap_or(len).min(len);
        items[index].key = key;
    }
}

fn keyed_pull_clone_chips(
    items: Signal<Vec<PullCloneItem>>,
    rows_id: &'static str,
    list: &'static str,
) -> Element {
    let items_snap = items.get();
    let rows = jadix::for_keyed!(
        rows_id,
        items_snap.as_slice(),
        |item: &PullCloneItem| item.key,
        |item: &PullCloneItem| {
            let name = item.label.clone();
            let i = items_snap
                .iter()
                .position(|x| x.key == item.key)
                .unwrap_or(0);
            let id = format!("{list}:{}", item.key);
            ui! {
                <SortableItem
                    id={id}
                    item_type="clone-tag"
                    accept="clone-tag"
                    list={list}
                    index={i}
                    pull_clone={true}
                    source_opacity={0.35}
                >
                    { full_width_chip(&name) }
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
    items: Signal<Vec<SortableFlagItem>>,
    item_type: &'static str,
    source_opacity: f32,
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
                ) }
            </SortableList>
        </VStack>
    }
}

fn rsx_pull_clone_board(
    items: Signal<Vec<PullCloneItem>>,
    preview: bool,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    let (scope, list, rows, count) = if preview {
        (
            "gal-drag-clone-preview",
            "clone-preview-list",
            "gal-drag-clone-preview-rows",
            "gal-drag-clone-preview-count",
        )
    } else {
        (
            "gal-drag-clone",
            "clone-list",
            "gal-drag-clone-rows",
            "gal-drag-clone-count",
        )
    };
    let hint = jadix::bind_text!(count, move || {
        format!(
            "共 {} 项 · `<SortableItem pull_clone={{true}}>` 松手 insert 副本",
            items.get().len()
        )
    });
    ui! {
        <DragProvider id={scope} collision="closest-center" reorder_preview={preview} onDragEnd={on_end}>
            <VStack gap="0.375rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground">{ hint }</Label>
                <SortableList id={list} accept="clone-tag">
                    { keyed_pull_clone_chips(items, rows, list) }
                </SortableList>
                <DragOverlay class="shadow-lg" />
            </VStack>
        </DragProvider>
    }
}

fn rsx_skip_flags_board(
    shift_normal: Signal<Vec<SortableFlagItem>>,
    shift_pinned: Signal<Vec<SortableFlagItem>>,
    shift_follow: Signal<Vec<SortableFlagItem>>,
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
                    ) }
                    { rsx_sortable_flags_column(
                        "skip_shift · Pin",
                        "gal-drag-shift-pinned-rows",
                        "shift-pinned",
                        shift_pinned,
                        "shift-demo",
                        0.35,
                    ) }
                    { rsx_sortable_flags_column(
                        "skip_follow · Follow",
                        "gal-drag-shift-follow-rows",
                        "shift-follow",
                        shift_follow,
                        "shift-demo",
                        0.35,
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

/// The pull-clone example is a vertical list, so its source and cloned rows
/// should retain the list width when the body is rendered in the overlay.
fn full_width_chip(label: &str) -> Element {
    compact_chip(label).w("100%")
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
    let item_id = item_id.to_string();
    let selected = ctx
        .as_ref()
        .is_some_and(|c| c.selected.get().contains(&item_id));
    let mut el = ui! {
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
    };
    el.style.bg_color = parse_color(if selected { "accent" } else { "card" });
    el.style.border_color = parse_color(if selected { "primary" } else { "border" });
    el
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
    batch(|| {
        if from == left_id {
            left.set(src);
            right.set(dst);
        } else {
            right.set(src);
            left.set(dst);
        }
    });
}

fn keyed_swap_chips(
    rows_id: &'static str,
    items: Signal<Vec<String>>,
    list: &'static str,
) -> Element {
    let items_snap = items.get();
    let rows = jadix::for_keyed!(
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
    let rows = jadix::for_keyed!(
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
    preview: bool,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    let (scope, left_list, right_list, left_rows, right_rows) = if preview {
        (
            "gal-drag-swap-preview",
            "swap-preview-left",
            "swap-preview-right",
            "gal-drag-swap-preview-left-rows",
            "gal-drag-swap-preview-right-rows",
        )
    } else {
        (
            "gal-drag-swap",
            "swap-left",
            "swap-right",
            "gal-drag-swap-left-rows",
            "gal-drag-swap-right-rows",
        )
    };
    let left_title = if preview {
        "左 · 半透明交换预览"
    } else {
        "左 · 不让位，高亮对端"
    };
    ui! {
        <DragProvider id={scope} collision="closest-center" sort_mode="swap" reorder_preview={preview} onDragEnd={on_end}>
            <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                <VStack gap="0.375rem" min_w="10rem">
                    <Label text_size="0.75rem" color="muted-foreground">{ left_title }</Label>
                    <SortableList id={left_list} group="swap-demo" accept="swap">
                        { keyed_swap_chips(left_rows, left, left_list) }
                    </SortableList>
                </VStack>
                <VStack gap="0.375rem" min_w="10rem">
                    <Label text_size="0.75rem" color="muted-foreground">"右 · 跨列 mem::swap"</Label>
                    <SortableList id={right_list} group="swap-demo" accept="swap">
                        { keyed_swap_chips(right_rows, right, right_list) }
                    </SortableList>
                </VStack>
            </HStack>
            <DragOverlay class="shadow-lg" />
        </DragProvider>
    }
}

fn multi_selected_hud(items: Signal<Vec<String>>) -> Element {
    let selected = use_drag().expect("MultiDrag provider").selected;
    let hint = jadix::bind_text!("gal-drag-multi-count", move || {
        format!(
            "共 {} 项 · 已选 {} · 单击选中，Shift 范围选中",
            items.get().len(),
            selected.get().len()
        )
    });
    ui! {
        <Label text_size="0.75rem" color="muted-foreground">{ hint }</Label>
    }
}

fn rsx_multi_board(
    items: Signal<Vec<String>>,
    on_end: impl FnMut(&DragEvent) + 'static,
) -> Element {
    ui! {
        <DragProvider id="gal-drag-multi" collision="closest-center" multi_drag={true} onDragEnd={on_end}>
            <VStack gap="0.375rem" w="100%">
                { multi_selected_hud(items) }
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
                    if let Some(id) = el.get_attr(jadix::primitives::canvas::ATTR_CANVAS_PAINT) {
                        jadix::primitives::canvas::mark_canvas_dirty_hex(id);
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

const MAGNETIC_LINE_TOP: f32 = 72.0;
const MAGNETIC_LINE_RANGE: f32 = 14.0;

/// Track the painted guide itself: snap callbacks receive viewport-space origins,
/// whereas the guide's authored top is relative to the arena's padding box.
fn magnetic_snap_line() -> (Element, SnapPointFn) {
    let viewport_y = Rc::new(Cell::new(None::<f32>));
    let measured_y = viewport_y.clone();
    let line = Element::new()
        .absolute()
        .left(0.0)
        .top(MAGNETIC_LINE_TOP)
        .w("100%")
        .h(1.0)
        .bg("primary")
        .pointer_events_none()
        .reactive_sync(move |el, _| {
            let bounds = el.viewport_layout_bounds();
            measured_y.set((bounds[2] > 0.0 && bounds[3] > 0.0).then_some(bounds[1]));
            LayoutStatus::Clean
        });
    let snap: SnapPointFn = Rc::new(move |x, y| {
        let line_y = viewport_y.get()?;
        ((y - line_y).abs() <= MAGNETIC_LINE_RANGE).then_some([x, line_y])
    });
    (line, snap)
}

fn resize_handle(
    rect: Signal<[f32; 4]>,
    bounds: Rc<Cell<BoxRect>>,
    left: &str,
    top: &str,
    edges: Edges,
) -> Element {
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
            if e.phase == jadix::EventPhase::Capture
                || jadix::app::pointer_events::dom_button(e.button) != 0
            {
                return;
            }
            let r = rect.peek();
            begin_resize(
                BoxRect::from_xywh(r[0], r[1], r[2], r[3]),
                [e.x, e.y],
                ResizeOpts::default()
                    .edges(edges)
                    .invert(Invert::Reposition)
                    .min_size(64.0, 48.0)
                    .restrict_rect(bounds.get())
                    .autoscroll(true),
                Some("gal_drag_resize_rect".into()),
            );
            e.prevent_default();
            e.stop_propagation();
        })
        .into()
}

fn resize_board(rect: Signal<[f32; 4]>) -> Element {
    let bounds = Rc::new(Cell::new(BoxRect::default()));
    let chrome = ui! {
        <Box absolute={true} rounded="0.5rem" bg="card" border_style="solid" border_width="2px" border_color="primary" items="center" justify="center">
            <VStack items="center" gap="0.125rem" pointer_events="none">
                <Label font_weight="700" text_size="0.875rem">"resize"</Label>
                <Label color="muted-foreground" text_size="0.6875rem">{ format!("{:.0}×{:.0}", rect.get()[2], rect.get()[3]) }</Label>
                <Label color="muted-foreground" text_size="0.625rem">"autoscroll · 拖到边缘滚"</Label>
            </VStack>
            { resize_handle(rect, bounds.clone(), "-6px", "-6px", Edges::top_left()) }
            { resize_handle(rect, bounds.clone(), "calc(50% - 6px)", "-6px", Edges::top()) }
            { resize_handle(rect, bounds.clone(), "calc(100% - 6px)", "-6px", Edges::top_right()) }
            { resize_handle(rect, bounds.clone(), "-6px", "calc(50% - 6px)", Edges::left()) }
            { resize_handle(rect, bounds.clone(), "calc(100% - 6px)", "calc(50% - 6px)", Edges::right()) }
            { resize_handle(rect, bounds.clone(), "-6px", "calc(100% - 6px)", Edges::bottom_left()) }
            { resize_handle(rect, bounds.clone(), "calc(50% - 6px)", "calc(100% - 6px)", Edges::bottom()) }
            { resize_handle(rect, bounds.clone(), "calc(100% - 6px)", "calc(100% - 6px)", Edges::bottom_right()) }
        </Box>
    };
    let chrome = bind_resize_rect(chrome, rect);
    let arena: Element = ui! {
        <Box relative={true} w="100%" h="28rem" min_h="28rem" rounded="0.75rem" bg="background" border_style="dashed" border_width="1px" border_color="border">
            { chrome }
        </Box>
    };
    let arena = arena.on_mousedown(move |event| {
        if event.phase == jadix::EventPhase::Capture {
            let [_, _, w, h] = event.current_bounds;
            // Keep the 12px handles inside the fixed content area as well.
            bounds.set(BoxRect::from_xywh(
                6.0,
                6.0,
                (w - 12.0).max(0.0),
                (h - 12.0).max(0.0),
            ));
        }
    });
    ui! {
        <ScrollArea id="gal-drag-resize-scroll" h="14rem" w="100%" overflow_x="hidden" overflow_y="auto" scrollbar_visibility="always">
            { arena }
        </ScrollArea>
    }
}

fn resize_edges_board(rect: Signal<[f32; 4]>) -> Element {
    let chrome = ui! {
        <Box absolute={true} rounded="0.5rem" bg="card" border_style="solid" border_width="2px" border_color="accent" items="center" justify="center">
            <VStack items="center" gap="0.25rem" pointer_events="none">
                <Label font_weight="700" text_size="0.875rem">"无把手 resize"</Label>
                <Label color="muted-foreground" text_size="0.6875rem">{ format!("{:.0}×{:.0}", rect.get()[2], rect.get()[3]) }</Label>
                <Label color="muted-foreground" text_size="0.6875rem">"靠近四边或四角拖动"</Label>
            </VStack>
        </Box>
    };
    let chrome = resize_on_edges(
        chrome,
        rect,
        ResizeOpts::default().min_size(120.0, 80.0),
        8.0,
    );
    ui! {
        <ScrollArea id="gal-drag-resize-edges-scroll" h="14rem" w="100%" overflow_y="auto">
            <Box relative={true} w="100%" min_h="28rem" rounded="0.75rem" bg="background" border_style="dashed" border_width="1px" border_color="border">
                { chrome }
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
    use jadix::core::reactivity::create_root;
    use jadix::utils::drag::{begin, end, tick, BeginDrag};

    #[test]
    fn pull_clone_keeps_distinct_draggable_rows_after_drop() {
        let (_owner, ()) = create_root(|| {
            let items = Signal::new(vec![
                PullCloneItem {
                    key: 1,
                    label: "Rust".into(),
                },
                PullCloneItem {
                    key: 2,
                    label: "UI".into(),
                },
                PullCloneItem {
                    key: 3,
                    label: "Drag".into(),
                },
            ]);
            let event = DragEvent {
                scope_id: "gal-drag-clone".into(),
                draggable_id: "clone:1".into(),
                droppable_id: Some("clone:3".into()),
                from_list: Some("clone-list".into()),
                from_index: Some(0),
                to_list: Some("clone-list".into()),
                to_index: Some(3),
                pointer: [0.0, 0.0],
                delta: [0.0, 0.0],
                phase: jadix::utils::drag::DragPhase::Active,
                pull_clone: true,
                data: None,
                sort_mode: jadix::utils::drag::SortMode::Insert,
                items: Vec::new(),
            };
            let mut values = items.peek();
            apply_pull_clone_event(&mut values, &event);
            assert_eq!(values.len(), 4);
            assert_eq!(values[3].label, "Rust");
            assert_ne!(values[0].key, values[3].key);
            let copied_key = values[3].key;
            let event = DragEvent {
                from_index: Some(3),
                to_index: Some(1),
                ..event
            };
            apply_pull_clone_event(&mut values, &event);
            assert_eq!(
                values
                    .iter()
                    .map(|item| item.label.as_str())
                    .collect::<Vec<_>>(),
                ["Rust", "Rust", "UI", "Drag", "Rust"]
            );
            assert_eq!(
                values[4].key, copied_key,
                "the source copy keeps its identity"
            );
            assert_ne!(
                values[1].key, copied_key,
                "a copy of a copy gets a fresh key"
            );
            items.set(values);
            let board = drag_scope("gal-drag-clone")
                .child(move || keyed_pull_clone_chips(items, "gal-drag-clone-rows", "clone-list"));
            let mut ids = Vec::new();
            fn collect(el: &Element, ids: &mut Vec<String>) {
                if let Some(id) = &el.template_id {
                    if id.starts_with("gal-drag-clone-rows:") {
                        ids.push(id.clone());
                    }
                }
                for child in &el.children {
                    collect(child, ids);
                }
            }
            collect(&board, &mut ids);
            assert_eq!(ids.len(), 5, "each drop adds one visible row");
            ids.sort();
            ids.dedup();
            assert_eq!(
                ids.len(),
                5,
                "same-label copies need distinct drag identities"
            );
            fn collect_draggable_ids(el: &Element, ids: &mut Vec<String>) {
                if let Some(id) = &el.template_id {
                    if id.starts_with("gal-drag-clone__draggable:") {
                        ids.push(id.clone());
                    }
                }
                for child in &el.children {
                    collect_draggable_ids(child, ids);
                }
            }
            let mut draggable_ids = Vec::new();
            collect_draggable_ids(&board, &mut draggable_ids);
            assert_eq!(draggable_ids.len(), 5);
            draggable_ids.sort();
            draggable_ids.dedup();
            assert_eq!(draggable_ids.len(), 5, "drag identities are unique too");
        });
    }

    #[test]
    fn skip_flags_board_commits_cross_list_move_to_both_columns() {
        let (_owner, ()) = create_root(|| {
            let normal = Signal::new(sortable_flag_items(
                "shift-normal",
                &["Alpha", "Beta", "Gamma"],
            ));
            let pinned = Signal::new(sortable_flag_items(
                "shift-pinned",
                &["Alpha", "Pin", "Gamma"],
            ));
            let follow = Signal::new(sortable_flag_items(
                "shift-follow",
                &["Alpha", "Follow", "Gamma"],
            ));
            let event = DragEvent {
                scope_id: "gal-drag-skip-shift".into(),
                draggable_id: "shift-normal:Alpha".into(),
                droppable_id: Some("shift-pinned".into()),
                from_list: Some("shift-normal".into()),
                from_index: Some(0),
                to_list: Some("shift-pinned".into()),
                to_index: Some(1),
                pointer: [0.0, 0.0],
                delta: [0.0, 0.0],
                phase: jadix::utils::drag::DragPhase::Active,
                pull_clone: false,
                data: None,
                sort_mode: jadix::utils::drag::SortMode::Insert,
                items: Vec::new(),
            };

            apply_skip_flags_event(normal, pinned, follow, &event);

            assert_eq!(
                normal.peek(),
                sortable_flag_items("shift-normal", &["Beta", "Gamma"])
            );
            let mut expected = sortable_flag_items("shift-pinned", &["Alpha", "Pin", "Gamma"]);
            expected.insert(1, sortable_flag_items("shift-normal", &["Alpha"]).remove(0));
            assert_eq!(pinned.peek(), expected);
            assert_eq!(
                follow.peek(),
                sortable_flag_items("shift-follow", &["Alpha", "Follow", "Gamma"])
            );

            let rows = [normal.peek(), pinned.peek(), follow.peek()].concat();
            let ids: std::collections::HashSet<_> = rows.iter().map(|item| &item.key).collect();
            assert_eq!(
                ids.len(),
                9,
                "same labels retain distinct stable drag identities"
            );

            let back = DragEvent {
                droppable_id: Some("shift-normal".into()),
                from_list: Some("shift-pinned".into()),
                from_index: Some(1),
                to_list: Some("shift-normal".into()),
                to_index: Some(0),
                ..event
            };
            apply_skip_flags_event(normal, pinned, follow, &back);
            assert_eq!(
                normal.peek(),
                sortable_flag_items("shift-normal", &["Alpha", "Beta", "Gamma"])
            );
            assert_eq!(
                pinned.peek(),
                sortable_flag_items("shift-pinned", &["Alpha", "Pin", "Gamma"])
            );
        });
    }

    #[test]
    fn handled_resize_stays_in_content_and_never_scrolls_sibling() {
        use jadix::app::pointer_events::{dispatch_pointer, hit_path, MouseEvent, PointerKind};
        use jadix::core::scroll_viewport::{
            scroll_content_h_signal_key, scroll_offset_y_signal_key, scroll_viewport_h_signal_key,
        };
        use jadix::utils::pointer_action::{on_move, on_up, pump_edge_scroll, resize_is_active};
        fn sync_tree(el: &mut Element, layout: &mut jadix::core::tree::LayoutTree) {
            if let Some(sync) = el.reactive_sync.clone() {
                sync.run(el, layout);
            }
            for child in &mut el.children {
                sync_tree(child, layout);
            }
        }
        fn find_box(el: &Element) -> Option<&Element> {
            if el.children.len() == 9 {
                return Some(el);
            }
            el.children.iter().find_map(find_box)
        }
        let (_owner, ()) = create_root(|| {
            on_up();
            let rect = use_signal("gal_drag_resize_rect", || [40.0_f32, 28.0, 160.0, 100.0]);
            let edges_rect = Signal::new([40.0, 28.0, 220.0, 120.0]);
            let mut root = Element::new()
                .flex_col()
                .gap(80.0)
                .child(resize_board(rect))
                .child(resize_edges_board(edges_rect));
            let mut layout = jadix::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
            sync_tree(&mut root, &mut layout);
            let mut views = Vec::new();
            jadix::utils::edge_scroll::collect_scroll_hits(&root, &mut views);
            assert_eq!(views.len(), 2);
            for view in &views {
                use_signal(&scroll_content_h_signal_key(&view.rest), || 0.0_f32).set_silent(448.0);
                use_signal(&scroll_viewport_h_signal_key(&view.rest), || 0.0_f32)
                    .set_silent(view.rect.height);
            }
            let sibling = use_signal(
                &scroll_offset_y_signal_key("gal-drag-resize-edges-scroll"),
                || 0.0_f32,
            );
            let owner_offset = use_signal(
                &scroll_offset_y_signal_key("gal-drag-resize-scroll"),
                || 0.0_f32,
            );
            for (handle_index, target) in [(8, [790.0, 1000.0]), (1, [-800.0, -1000.0])] {
                rect.set_silent([40.0, 28.0, 160.0, 100.0]);
                owner_offset.set_silent(0.0);
                sync_tree(&mut root, &mut layout);
                layout.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
                let handle =
                    find_box(&root).unwrap().children[handle_index].viewport_layout_bounds();
                let press = [handle[0] + 6.0, handle[1] + 6.0];
                let path = hit_path(&root, press[0], press[1]);
                let mut event =
                    MouseEvent::new(press[0], press[1], winit::event::MouseButton::Left, 0);
                dispatch_pointer(&path, &mut event, PointerKind::Down);
                assert!(
                    resize_is_active(),
                    "actual handle {handle_index} at {press:?} must start resize"
                );
                on_move(press[0], views[0].rect.bottom() - 4.0);
                for _ in 0..30 {
                    pump_edge_scroll(&root);
                }
                assert!(owner_offset.peek() > 0.0, "the owner's edge must scroll");
                let after_scroll = rect.peek();
                on_move(press[0], views[0].rect.bottom() - 4.0);
                assert_eq!(
                    rect.peek(),
                    after_scroll,
                    "scroll is not repeatedly added to geometry"
                );
                on_move(press[0], views[1].rect.bottom() - 4.0);
                for _ in 0..30 {
                    pump_edge_scroll(&root);
                }
                assert_eq!(sibling.peek(), 0.0);
                on_move(target[0], target[1]);
                let [x, y, w, h] = rect.peek();
                assert!(
                    x >= 6.0 && y >= 6.0 && x + w <= 794.0 && y + h <= 442.0,
                    "resize must stay within fixed content bounds: {:?}",
                    rect.peek()
                );
                on_up();
                assert!(!pump_edge_scroll(&root), "release stops autoscroll");
                assert_eq!(edges_rect.peek(), [40.0, 28.0, 220.0, 120.0]);
            }
        });
    }

    #[test]
    fn resize_board_updates_box_and_handles_during_drag() {
        fn sync_tree(el: &mut Element, layout: &mut jadix::core::tree::LayoutTree) {
            if let Some(sync) = el.reactive_sync.clone() {
                sync.run(el, layout);
            }
            for child in &mut el.children {
                sync_tree(child, layout);
            }
        }
        fn find_box(el: &Element) -> Option<&Element> {
            if el.children.len() == 9 {
                return Some(el);
            }
            el.children.iter().find_map(find_box)
        }
        let (_owner, ()) = create_root(|| {
            let rect = use_signal("gal_drag_resize_rect", || [40.0_f32, 28.0, 160.0, 100.0]);
            rect.set_silent([40.0, 28.0, 160.0, 100.0]);
            let mut board = resize_board(rect);
            let mut layout = jadix::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut board, 800.0, 500.0, [0.0, 0.0]);
            let before = find_box(&board).unwrap().children[8].layout_bounds;
            begin_resize(
                BoxRect::from_xywh(40.0, 28.0, 160.0, 100.0),
                [200.0, 128.0],
                ResizeOpts::default().edges(Edges::bottom_right()),
                Some("gal_drag_resize_rect".into()),
            );
            assert!(jadix::utils::pointer_action::on_move(240.0, 158.0));
            assert!(jadix::utils::pointer_action::on_up());
            sync_tree(&mut board, &mut layout);
            layout.build_and_layout(&mut board, 800.0, 500.0, [0.0, 0.0]);
            let handles = find_box(&board).expect("resize box and eight handles");
            assert_eq!(rect.peek(), [40.0, 28.0, 200.0, 130.0]);
            assert_eq!(
                handles.style.size,
                Element::new().w(200.0).h(130.0).style.size
            );
            let after = handles.children[8].layout_bounds;
            assert!(
                (after[0] - before[0] - 40.0).abs() < 0.1,
                "right handle must follow width: {before:?} -> {after:?}"
            );
            assert!(
                (after[1] - before[1] - 30.0).abs() < 0.1,
                "bottom handle must follow height: {before:?} -> {after:?}"
            );
        });
    }

    fn sync_line(line: &mut Element) {
        let sync = line
            .reactive_sync
            .clone()
            .expect("line tracks its viewport position");
        sync.run(line, &mut jadix::core::tree::LayoutTree::new());
    }

    #[test]
    fn magnetic_line_follows_layout_and_scroll_and_releases_outside_range() {
        let (mut line, snap) = magnetic_snap_line();
        assert_eq!(snap(120.0, 72.0), None, "unlaid line has no snap target");
        line.layout_bounds = [100.0, 473.0, 600.0, 1.0];
        sync_line(&mut line);
        for y in [459.0, 473.0, 487.0] {
            assert_eq!(snap(120.0, y), Some([120.0, 473.0]));
        }
        for y in [441.0, 458.0, 488.0] {
            assert_eq!(snap(120.0, y), None);
        }
        let mut root = Element::new()
            .w(800.0)
            .h(800.0)
            .overflow("scroll")
            .scroll_offset(25.0, 120.0)
            .child(line.left(100.0).top(473.0).w(600.0).h(1.0));
        jadix::core::tree::LayoutTree::new().build_and_layout(&mut root, 800.0, 800.0, [0.0, 0.0]);
        let mut line = root.children.remove(0);
        sync_line(&mut line);
        assert_eq!(snap(180.0, 360.0), Some([180.0, 353.0]));
        assert_eq!(snap(180.0, 473.0), None, "old screen position must release");
        line.layout_bounds[1] = 523.0;
        sync_line(&mut line);
        assert_eq!(snap(180.0, 410.0), Some([180.0, 403.0]));
    }

    #[test]
    fn magnetic_line_drag_moves_freely_snaps_releases_and_persists_preview() {
        let (_owner, ()) = create_root(|| {
            let pos = Signal::new([16.0, 40.0]);
            let (mut line, snap) = magnetic_snap_line();
            line.layout_bounds = [100.0, 473.0, 600.0, 1.0];
            sync_line(&mut line);
            let _scope = drag_scope("magnetic-line-regression")
                .threshold(0.0)
                .restrict(Restrict::Rect([100.0, 400.0, 600.0, 224.0]))
                .snap_fn(snap)
                .on_drag_end(persist_delta_pos(pos))
                .child(Element::new);
            begin(BeginDrag {
                scope_id: "magnetic-line-regression".into(),
                draggable_id: "card".into(),
                pointer: [137.0, 453.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [117.0, 441.0, 176.0, 92.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            for (pointer, expected) in [
                ([157.0, 455.0], [137.0, 443.0]),
                ([177.0, 471.0], [157.0, 473.0]),
                ([197.0, 501.0], [177.0, 489.0]),
                ([217.0, 471.0], [197.0, 473.0]),
            ] {
                assert!(tick(pointer[0], pointer[1], &[], [800.0, 800.0]));
                assert_eq!(last_overlay_pos("magnetic-line-regression"), Some(expected));
            }
            assert!(end(&[]));
            assert_eq!(pos.peek(), [96.0, 72.0]);
        });
    }

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
    let rows = jadix::for_keyed!(
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
    let rows = jadix::for_keyed!(
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
    batch(|| {
        if from_list == "todo" {
            left.set(src);
            right.set(dst);
        } else {
            right.set(src);
            left.set(dst);
        }
    });
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
    batch(|| {
        if from_list == left_id {
            left.set(src);
            right.set(dst);
        } else {
            right.set(src);
            left.set(dst);
        }
    });
}

fn apply_skip_flags_event(
    normal: Signal<Vec<SortableFlagItem>>,
    pinned: Signal<Vec<SortableFlagItem>>,
    follow: Signal<Vec<SortableFlagItem>>,
    ev: &DragEvent,
) {
    let Some(from_list) = ev.from_list.as_deref() else {
        return;
    };
    let to_list = ev.to_list.as_deref().unwrap_or(from_list);
    let signal_for = |list: &str| match list {
        "shift-normal" => Some(normal),
        "shift-pinned" => Some(pinned),
        "shift-follow" => Some(follow),
        _ => None,
    };
    let (Some(source), Some(target)) = (signal_for(from_list), signal_for(to_list)) else {
        return;
    };

    if from_list == to_list {
        let mut values = source.peek();
        apply_event(&mut values, None, ev);
        source.set(values);
        return;
    }

    let mut source_values = source.peek();
    let mut target_values = target.peek();
    apply_event(&mut source_values, Some(&mut target_values), ev);
    batch(|| {
        source.set(source_values);
        target.set(target_values);
    });
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
    let rows = jadix::for_keyed!(
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
    let snap_end_lines_visible = use_signal("gal_drag_snap_end_lines_visible", || false);
    let start_lock_pos = use_signal("gal_drag_start_lock_pos", || [16.0_f32, 16.0]);
    let fling_pos = use_signal("gal_drag_fling_pos", || [16.0_f32, 16.0]);
    let resize_rect = use_signal("gal_drag_resize_rect", || [40.0_f32, 28.0, 160.0, 100.0]);
    let resize_edges_rect = use_signal("gal_drag_resize_edges_rect", || {
        [40.0_f32, 28.0, 220.0, 120.0]
    });
    let gesture_a = use_signal("gal_drag_gesture_a", || [48.0_f32, 72.0]);
    let gesture_b = use_signal("gal_drag_gesture_b", || [200.0_f32, 72.0]);
    set_gesture_opts(GestureOpts::default().autoscroll(true));
    let veto_open = use_signal("gal_drag_veto_open", || {
        vec!["Move me".to_string(), "Locked".to_string()]
    });
    let veto_empty = use_signal("gal_drag_veto_empty", Vec::<String>::new);
    let clone_tags = use_signal("gal_drag_clone", || {
        vec![
            PullCloneItem {
                key: 1,
                label: "Rust".to_string(),
            },
            PullCloneItem {
                key: 2,
                label: "UI".to_string(),
            },
            PullCloneItem {
                key: 3,
                label: "Drag".to_string(),
            },
        ]
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
        sortable_flag_items("shift-normal", &["Alpha", "Beta", "Gamma"])
    });
    let shift_pinned = use_signal("gal_drag_shift_pinned", || {
        sortable_flag_items("shift-pinned", &["Alpha", "Pin", "Gamma"])
    });
    let shift_follow = use_signal("gal_drag_shift_follow", || {
        sortable_flag_items("shift-follow", &["Alpha", "Follow", "Gamma"])
    });
    let data_note = use_signal("gal_drag_data_note", || {
        "拖起看幽灵上的 payload".to_string()
    });
    let idle_hits_note = use_signal("gal_drag_idle_hits_note", || "尚未发生 drop".to_string());
    let sensor_target = use_signal("gal_drag_sensor_target", || None::<String>);

    let clone_on_end = {
        let clone_tags = clone_tags.clone();
        move |ev: &DragEvent| {
            let mut v = clone_tags.peek();
            apply_pull_clone_event(&mut v, ev);
            clone_tags.set(v);
        }
    };
    let clone_board = rsx_pull_clone_board(clone_tags, false, clone_on_end);
    let clone_preview_tags = use_signal("gal_drag_clone_preview", || clone_tags.peek());
    let clone_preview_board = rsx_pull_clone_board(clone_preview_tags, true, move |ev| {
        let mut items = clone_preview_tags.peek();
        apply_pull_clone_event(&mut items, ev);
        clone_preview_tags.set(items);
    });

    let swap_on_end = {
        let swap_left = swap_left.clone();
        let swap_right = swap_right.clone();
        move |ev: &DragEvent| {
            apply_two_list_event(swap_left, swap_right, "swap-left", ev);
        }
    };
    let swap_board = rsx_swap_board(swap_left, swap_right, false, swap_on_end);
    let swap_preview_left = use_signal("gal_drag_swap_preview_left", || swap_left.peek());
    let swap_preview_right = use_signal("gal_drag_swap_preview_right", || swap_right.peek());
    let swap_preview_board =
        rsx_swap_board(swap_preview_left, swap_preview_right, true, move |ev| {
            apply_two_list_event(
                swap_preview_left,
                swap_preview_right,
                "swap-preview-left",
                ev,
            );
        });

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

    let idle_hits_board = {
        let idle_hits_note = idle_hits_note.clone();
        Element::from(
            drag_scope("gal-drag-idle-hits")
                .collision(CollisionAlg::PointerWithin)
                .on_drag_end({
                    let idle_hits_note = idle_hits_note.clone();
                    move |ev: &DragEvent| {
                        let target = ev.droppable_id.as_deref().unwrap_or("—");
                        idle_hits_note.set(format!(
                            "最近 drop · {} → {} · drag={} drop={}",
                            ev.draggable_id,
                            target,
                            ev.items.len().max(1),
                            usize::from(ev.droppable_id.is_some())
                        ));
                    }
                })
                .child(move || {
                    let idle_hits_note = idle_hits_note.get();
                    ui! {
                        <VStack gap="0.375rem" w="100%">
                            { hits_hud() }
                            <Label text_size="0.75rem" color="muted-foreground">
                                { idle_hits_note }
                            </Label>
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
        )
    };

    let skip_on_end = move |ev: &DragEvent| {
        apply_skip_flags_event(shift_normal, shift_pinned, shift_follow, ev);
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
                                    "`.sensor` + `run_scope_sensors`：按钮模拟键盘 activator，点击后立即显示替身，无需先移动鼠标。"
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

    let (snap_fn_guide, snap_fn_line) = magnetic_snap_line();
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
                min_h="14rem"
                overflow="hidden"
                rounded="0.75rem"
                bg="background"
                border_style="dashed"
                border_width="1px"
                border_color="border"
            >
                { snap_fn_guide }
                <Draggable id="snap-fn-tile" stack="auto">
                    { free_move_tile("F", "上边缘吸附 · 14px", snap_fn_pos, 0, "secondary") }
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
                        { snap_grid_lines(&[(snap_end_grid, 0.16)], snap_end_lines_visible) }
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
                    "两列共享 group=board。同列半透本体留在原槽；跨列时本体平移到目标槽。容器高亮只打当前投放目标（hover_list / jadix_drag_over），源列保持静息样式。颜色由调用方 reactive_sync 自定，不是 CSS :hover。松手 apply_transfer。"
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
                    "自由二维拖拽；卡片上边缘距线 14px 内吸附，拉远可脱离。磁吸线位于区域内 y=72，随页面滚动。"
                </Label>
                { snap_fn_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"dropzone · overlap=center + 生命周期"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "RSX `<Droppable overlap=\"center\" onDragEnter={…} onDrop={…}>` 与 Builder `droppable().overlap(Center)` 双写。"
                </Label>
                { dropzone_board }

                <Separator />
                <HStack w="100%" items="center" justify="between" gap="0.75rem" wrap="wrap">
                    <Label font_weight="600" text_size="0.875rem">"SnapGrid endOnly + range · RestrictAnchor::Center"</Label>
                    <Switch id="gal-drag-snap-end-lines" checked={snap_end_lines_visible}>
                        "网格线"
                    </Switch>
                </HStack>
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

                <Label font_weight="600" text_size="0.875rem">"resize · 无把手边角识别"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "像窗口一样：边缘 8px 自动切换横向、纵向或对角光标；按住左键调整大小，内部区域不启动调整。"
                </Label>
                { resize_edges_board(resize_edges_rect) }

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

                <Label font_weight="600" text_size="0.875rem">"pull_clone · 让位预览"</Label>
                { clone_preview_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§12 · Swap · SortMode"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<DragProvider sort_mode=\"swap\">` — 拖中不让位，高亮对端 `jadix_drag_swap`；松手 `apply_event` 走 `Vec::swap` / 跨列 `mem::swap`。与插入 FLIP 互斥。"
                </Label>
                { swap_board }

                <Label font_weight="600" text_size="0.875rem">"Swap · 让位预览"</Label>
                { swap_preview_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§12 · MultiDrag · 多选拖"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<DragProvider multi_drag={{true}}>` — 单击切换选中，Shift 范围选中；拖已选项一次带走。"
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
                    "仅验证命中注册表，不修改列表；drop 结果显示在区域内。assemble 时 register_drag_slot_meta；未拖时 `use_drag().hits()` 走 registered_hits_in_scope。"
                </Label>
                { idle_hits_board }

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"§11 · skip_shift · skip_follow"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "`<SortableItem skip_shift={{true}}>` / `skip_follow={{true}}>` — Pin 不参与让位平移，Follow 不随指针移动；标记随项目保留，三列支持跨列移动。"
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
