//! WP-DRAG.1: solid-dnd / SortableJS / interact.js kit — RSX + Builder gallery.

use crate::imports::*;
use crate::shared::*;
use rui::utils::drag::{
    Axis, CollisionAlg, DragEvent, Restrict, apply_reorder_vec, apply_transfer, drag_overlay,
    drag_scope, use_drag,
};

fn zone_over_peek(id: &str) -> bool {
    use_drag()
        .and_then(|c| c.active_droppable.peek())
        .as_deref()
        == Some(id)
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
    zone.reactive_sync(move |el, _| {
        let over = zone_over_peek(drop_id);
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
            bg="transparent"
        >
            <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
            <Label text_size="0.875rem" grow="1" color="foreground">{ label.to_string() }</Label>
        </HStack>
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
                    group={group}
                    list={list}
                    index={i}
                >
                    { list_chip(&name) }
                </SortableItem>
            }
        }
    );
    Element::from(vstack().w("100%").gap("0.375rem").children(rows))
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
    let todo = use_signal("gal_drag_todo", || {
        ["Draft RFC", "Review PR"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let doing = use_signal("gal_drag_doing", || vec!["Write tests".to_string()]);

    let inbox = use_signal("gal_drag_inbox", || false);
    let handle_inbox = use_signal("gal_drag_handle_inbox", || false);
    let custom_inbox = use_signal("gal_drag_custom_inbox", || false);
    let ghost_only = use_signal("gal_drag_ghost_only", || false);
    let alpha_park = use_signal("gal_drag_alpha_park", || false);
    let beta_park = use_signal("gal_drag_beta_park", || false);

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
                            { Element::from(drag_overlay().style(|el| el.opacity(0.92))) }
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
                    "空 DragOverlay 跟随本体。松手落在右侧后卡片进入容器；拖到空白处放回。"
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

                <Label font_weight="600" text_size="0.875rem">"DragOverlay · 自定义子树"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "子节点整段替换克隆：拖起时只看到「自定义替身」（hide_source），松手后源卡片进入容器。"
                </Label>
                <DragProvider
                    id="gal-drag-custom-ov"
                    collision="pointer-within"
                    onDragEnd={park_on_drop(custom_inbox, "custom-inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!custom_inbox.get()}>
                            <Draggable id="custom-src" item_type="chip">
                                { card_chip("源卡片") }
                            </Draggable>
                        </Show>
                        <Show when={custom_inbox.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="custom-inbox" accept="chip">
                            { drop_park_zone("custom-inbox", custom_inbox, || {
                                ui! {
                                    <Draggable id="custom-src:inbox" item_type="chip">
                                        { card_chip("源卡片") }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay hide_source={true}>
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
                    "hide_source：拖起后源位留空，只看到跟随指针的浮层。松手进右侧；拖到空白处放回。"
                </Label>
                <DragProvider
                    id="gal-drag-hide-src"
                    collision="pointer-within"
                    onDragEnd={park_on_drop(ghost_only, "ghost-inbox")}
                >
                    <HStack gap="0.75rem" w="100%" items="start" wrap="wrap">
                        <Show when={!ghost_only.get()}>
                            <Draggable id="ghost-chip" item_type="chip">
                                { card_chip("拖我 · 本体消失") }
                            </Draggable>
                        </Show>
                        <Show when={ghost_only.get()}>
                            { vacated_slot("已放入右侧") }
                        </Show>
                        <Droppable id="ghost-inbox" accept="chip">
                            { drop_park_zone("ghost-inbox", ghost_only, || {
                                ui! {
                                    <Draggable id="ghost-chip:inbox" item_type="chip">
                                        { card_chip("拖我 · 本体消失") }
                                    </Draggable>
                                }
                            }) }
                        </Droppable>
                    </HStack>
                    <DragOverlay hide_source={true} />
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
                                <Draggable id="alpha" item_type="alpha">
                                    { card_chip("组件1") }
                                </Draggable>
                            </Show>
                            <Show when={alpha_park.get()}>
                                { vacated_slot("已放入容器2") }
                            </Show>
                            <Show when={!beta_park.get()}>
                                <Draggable id="beta" item_type="beta">
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
                                        <Draggable id="beta:c1" item_type="beta">
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
                                        <Draggable id="alpha:c2" item_type="alpha">
                                            { card_chip("组件1") }
                                        </Draggable>
                                    }
                                }) }
                            </Droppable>
                        </VStack>
                    </HStack>
                    <DragOverlay hide_source={true} />
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
                <Label font_weight="600" text_size="0.875rem">"Sortable 列表"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "同列重排 + 拖起时兄弟让位；靠近列表边缘会自动滚。Esc 取消，↑↓ 改插入点。松手 apply_reorder_vec。"
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
                            { keyed_sortable_chips("gal-drag-sort-rows", sort_items, "planet", "", "planets", "") }
                        </SortableList>
                    </ScrollArea>
                    <DragOverlay hide_source={true} />
                </DragProvider>

                <Separator />
                <Label font_weight="600" text_size="0.875rem">"跨容器（Sortable group）"</Label>
                <Label text_size="0.75rem" color="muted-foreground">
                    "两列共享 group=board；松手 apply_transfer。空列也可作为 Droppable 接收。"
                </Label>
                <DragProvider
                    id="gal-drag-board"
                    collision="most-intersecting"
                    onDragEnd={move |ev: &DragEvent| apply_kanban(todo, doing, ev)}
                >
                    <HStack gap="0.75rem" w="100%" items="stretch">
                        <Droppable id="todo" list="todo" group="board" accept="task">
                            <VStack
                                grow="1"
                                min_h="8rem"
                                p="0.5rem"
                                gap="0.375rem"
                                rounded="0.5rem"
                                bg="muted"
                                w="50%"
                            >
                                <Label font_weight="600" text_size="0.75rem">"Todo"</Label>
                                { keyed_sortable_chips("gal-drag-todo-rows", todo, "task", "board", "todo", "todo") }
                            </VStack>
                        </Droppable>
                        <Droppable id="doing" list="doing" group="board" accept="task">
                            <VStack
                                grow="1"
                                min_h="8rem"
                                p="0.5rem"
                                gap="0.375rem"
                                rounded="0.5rem"
                                bg="muted"
                                w="50%"
                            >
                                <Label font_weight="600" text_size="0.75rem">"Doing"</Label>
                                { keyed_sortable_chips("gal-drag-doing-rows", doing, "task", "board", "doing", "doing") }
                            </VStack>
                        </Droppable>
                    </HStack>
                    <DragOverlay class="opacity-90 shadow-lg" />
                </DragProvider>
            </VStack>
        </MyCard>
    }
}
