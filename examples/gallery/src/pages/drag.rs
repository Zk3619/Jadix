//! WP-DRAG.1: solid-dnd / SortableJS / interact.js kit — RSX + Builder gallery.

use crate::imports::*;
use crate::shared::*;
use rui::utils::drag::{
    apply_reorder_vec, apply_transfer, drag_scope, last_hit_rect, last_pointer, sortable_item,
    sortable_list, use_drag, Axis, CollisionAlg, DragCtx, DragEvent, Restrict,
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

/// Overlay rotate from pointer vs arena: NW −45°, NE 45°, SW −135°, SE 135°.
/// Center stays 0°. Appearance only — CSS rotate on the overlay, not a kit modifier.
fn corner_card_angle(pointer: [f32; 2], bounds: [f32; 4]) -> f32 {
    let w = bounds[2].max(1.0);
    let h = bounds[3].max(1.0);
    let u = ((pointer[0] - bounds[0]) / w).clamp(0.0, 1.0);
    let v = ((pointer[1] - bounds[1]) / h).clamp(0.0, 1.0);
    let top = -45.0 + 90.0 * u;
    let bot = -135.0 + 270.0 * u;
    top + (bot - top) * v
}

fn corner_overlay_angle(scope: &str, arena: &str) -> f32 {
    last_pointer()
        .zip(last_hit_rect(scope, arena))
        .map(|(p, r)| corner_card_angle(p, r))
        .unwrap_or(0.0)
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
    Element::from(vstack().w("100%").min_h(0.0).h("auto").gap("0.375rem").children(rows))
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
                    // Folder island must track the inner vec so the card hugs
                    // remaining files after a cross-list transfer.
                    let _ = files.get().len();
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
    Element::from(vstack().w("100%").min_h(0.0).h("auto").gap("0.5rem").children(rows))
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

    ui! {
        <MyCard title="Drag（utils::drag · WP-DRAG.1–2）">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utils::drag：solid-dnd 原语 + 超越 Sortable 的自动滚、实时让位、Esc 取消、方向键改插入点。替身默认跟随本体并钉在视口（才能拖出父级进另一个容器）；DragOverlay 子树可整段替换。松手后由 onDragEnd 把源项挂进 Droppable。不进 prelude。"
                </Label>

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
                    "容器内一张卡片。restrict 填容器 id（corners-arena）或 parent（无需给父级分配 id，钳作者容器并跳过 kit shell）。本例用 arena id，浮层钳在整块虚线框内而不是整个视口：左上 −45°、右上 45°、左下 −135°、右下 135°（中心 0°，按指针在容器内的位置插值）。缺省/viewport 仍可拖出父级做跨容器投放。source_opacity 0，只看到旋转浮层；松手卡片仍留在容器里。"
                </Label>
                <DragProvider id="gal-drag-corners" collision="pointer-within">
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

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 自定义子树"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "子节点整段替换克隆：拖起时源卡片 source_opacity 0，只看到「自定义替身」。松手后源卡片进入容器。"
                </Label>
                <DragProvider
                    id="gal-drag-custom-ov"
                    collision="pointer-within"
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
                    "源项 source_opacity 0：拖起后源位留空，只看到跟随指针的浮层。松手进右侧；拖到空白处放回。"
                </Label>
                <DragProvider
                    id="gal-drag-hide-src"
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
            </VStack>
        </MyCard>
    }
}
