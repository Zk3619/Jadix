#![allow(non_snake_case)]

use rui::component;
use rui::components::scroll_area::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
use rui::components::*;
use rui::primitives::{hstack, r#box, tabs_headless, vstack, TABSTRIP_FOCUS_PREFIX};
use rui::style::parse_color;
use rui::core::reactivity::use_signal;
use rui::core::tree::Element;
use rui::ui;

/// Stable `template_id` for fine-grained reconcile (must stay unique per mounted block).
const BUTTON_SHOWCASE_REACTIVE: &str = "examples/hello_world/ButtonShowcase:variants_block";
const TOGGLE_SHOWCASE_REACTIVE: &str = "examples/hello_world/ToggleControlsShowcase:main";
const SELECT_POPOVER_SHOWCASE_REACTIVE: &str = "examples/hello_world/SelectPopoverTabShowcase:block";
const PRIMITIVES_VIRT_REACTIVE: &str = "examples/hello_world/PrimitivesShowcase:virt_list";
const GRID_DRAG_REACTIVE: &str = "examples/hello_world/GridAndDragShowcase:main";
const HW_GRID_SORT_COMMIT: &str = "__rui_drag_commit:hw-grid-sort";
const HW_GRID_SORT_HOVER: &str = "__rui_drag_hover:hw-grid-sort";
const HW_GRID_LIST_ID: &str = "hw-grid-sort";

fn hw_grid_sort_row(row_label: &String, i: usize, _items: &[String], hl: bool) -> Element {
    let z_boost = rui::primitives::drag_indicator_z_boost();
    let grip: Element = r#box()
        .drag_reorder_handle(HW_GRID_LIST_ID, i)
        .p("0.25rem")
        .rounded(4.0)
        .bg(parse_color("secondary"))
        .into();
    let mut caption_el: Element = label(&format!("hw-grid-sort-{i}"))
        .text(row_label.as_str())
        .into();
    caption_el = caption_el.flex_grow(1.0);
    Element::from(
        hstack()
            .w("100%")
            .p("0.5rem")
            .gap("0.5rem")
            .items("center")
            .bg(parse_color(if hl { "accent" } else { "muted" }))
            .children([grip, caption_el]),
    )
    .z_index(if hl { z_boost } else { 0 })
    .rounded(6.0)
}

fn checker_rgba_texture() -> (Vec<u8>, u32, u32) {
    let w = 64u32;
    let h = 64u32;
    let mut v = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let dark = ((x / 8) + (y / 8)) % 2 == 0;
            let (r, g, b) = if dark {
                (240u8, 240, 245)
            } else {
                (90, 95, 115)
            };
            let i = ((y * w + x) * 4) as usize;
            v[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    (v, w, h)
}

fn increment_count(count: rui::core::reactivity::Signal<i32>) {
    count.set(count.get() + 1);
}

fn choose_variant(selected: rui::core::reactivity::Signal<String>, variant: &str) {
    selected.set(variant.to_string());
}

#[component]
fn MyCard(title: String, children: Option<rui::core::tree::Element>) -> rui::core::tree::Element {
    let child = children.unwrap_or_else(|| rui::core::tree::Element::new());
    ui! {
        <VStack w="100%" min_w="0" p="1rem" bg="background" gap="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.125" class="rounded-lg">
            <Label w="100%" text_size="1.25rem" font_weight="700" leading="normal">{ title }</Label>
            { child }
        </VStack>
    }
}

#[component]
fn ButtonShowcase() -> rui::core::tree::Element {
    let count = use_signal("count", || 0);
    let selected = use_signal("selected_variant", || "default".to_string());

    ui! {
        <MyCard title="1) Button Variants + State">
            <Reactive id={BUTTON_SHOWCASE_REACTIVE}>
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                        <Button variant="default" on_click={move || increment_count(count)}>
                            { format!("Default ({})", count.get()) }
                        </Button>
                        <Button variant="secondary" on_click={move || choose_variant(selected, "secondary")}>
                            "Secondary"
                        </Button>
                        <Button variant="outline" on_click={move || choose_variant(selected, "outline")}>
                            "Outline"
                        </Button>
                        <Button variant="ghost">"Ghost"</Button>
                        <Button variant="destructive" on_click={move || choose_variant(selected, "default")}>"Destructive"</Button>
                        <Button variant="link">"Link"</Button>
                        <Button variant="default" disabled=true>"Disabled"</Button>
                    </HStack>
                    <Show when={selected.get() != "default"}>
                        <Label w="100%" text_size="0.875rem" color="muted-foreground" leading="normal">
                            { format!("Selected variant: {}", selected.get()) }
                        </Label>
                    </Show>
                </VStack>
            </Reactive>
        </MyCard>
    }
}

#[component]
fn ButtonHoverEasingShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1b) Hover：弹簧 vs cubic-bezier">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "默认 hover_easing=\"spring\"；设为 \"cubic\" 时对标 CSS transition + cubic-bezier(0.4, 0, 0.2, 1)"
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button id="hw-hover-spring" variant="ghost" hover_easing="spring">"Ghost · spring"</Button>
                    <Button id="hw-hover-cubic" variant="ghost" hover_easing="cubic">"Ghost · cubic"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="2) Input (ShadCN-style)">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Click to focus, type text, Tab to move focus, Backspace to delete"
                </Label>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <Input id="hw-s2-input-a" placeholder="Placeholder…" flex_grow="1" />
                    <Input id="hw-s2-input-b" placeholder="Invalid" invalid=true flex_grow="1" />
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn TypographyShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="3) Button Sizes + Icon + Custom Text Leading">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="outline" size="xs">"XS"</Button>
                    <Button variant="outline" size="sm">"SM"</Button>
                    <Button variant="outline" size="default">"Default"</Button>
                    <Button variant="outline" size="lg">"LG"</Button>
                    <Button variant="default" size="icon-xs"><Icon name="star" color="primary-foreground" size="0.75rem" /></Button>
                    <Button variant="default" size="icon-sm"><Icon name="heart" color="primary-foreground" size="0.875rem" /></Button>
                    <Button variant="default" size="icon"><Icon name="rocket" color="primary-foreground" size="1rem" /></Button>
                    <Button variant="default" size="icon-lg"><Icon name="sparkles" color="primary-foreground" size="1.125rem" /></Button>
                </HStack>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="default" leading="none">"Leading None"</Button>
                    <Button variant="default" leading="tight">"Leading Tight"</Button>
                    <Button variant="default" leading="normal">"Leading Normal"</Button>
                    <Button variant="default" leading="relaxed">"Leading Relaxed"</Button>
                    <Button variant="default" line_height="1.5rem">"LineHeight\n1.5rem (24px)"</Button>
                </HStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label w="100%" text_size="1rem" font_weight="700">"Label Decoration"</Label>
                    <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                        <Label text_decoration="underline" text_decoration_style="solid">"Solid"</Label>
                        <Label text_decoration="underline" text_decoration_style="double">"Double"</Label>
                        <Label text_decoration="underline" text_decoration_style="dotted">"Dotted"</Label>
                        <Label text_decoration="underline" text_decoration_style="dashed">"Dashed"</Label>
                        <Label text_decoration="underline" text_decoration_style="wavy">"Wavy"</Label>
                    </HStack>
                    <Label w="100%" text_size="0.875rem" font_weight="700">"Decoration Line"</Label>
                    <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                        <Label text_decoration_line="underline">"Underline"</Label>
                        <Label text_decoration_line="overline">"Overline"</Label>
                        <Label text_decoration_line="line-through">"Line Through"</Label>
                        <Label text_decoration_line="underline line-through">"Underline + Line Through"</Label>
                    </HStack>
                    <Label w="100%" text_size="0.875rem" font_weight="700">"Decoration Color / Thickness / Style"</Label>
                    <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                        <Label text_decoration_line="underline" text_decoration_color="red-500">"Red"</Label>
                        <Label text_decoration_line="underline" text_decoration_color="emerald-500">"Green"</Label>
                        <Label text_decoration_line="underline" text_decoration_thickness="1">"Thin"</Label>
                        <Label text_decoration_line="underline" text_decoration_thickness="2">"Medium"</Label>
                        <Label text_decoration_line="underline" text_decoration_thickness="3" text_decoration_style="dashed">"Thick Dashed"</Label>
                        <Label text_decoration_line="underline" text_decoration_style="wavy" text_decoration_color="violet-500">"Wavy Violet"</Label>
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ShadowShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="12) Box-shadow & Text-shadow">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "box-shadow：圆角矩形 SDF（outset 用扩张形状；inset 用外框与内缩圆角环 + blur）。text-shadow：glyphon 预绘制。GPU 盒阴影需 blur > 0。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"Box-shadow — outset"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="0.25rem 0.25rem 0.75rem 0 ring" />
                        <Label text_size="0.6rem" color="muted-foreground">"blur + ring"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625"
                            box_shadow="-0.25rem 0.25rem 0.5rem 0 destructive" />
                        <Label text_size="0.6rem" color="muted-foreground">"dx&lt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="0 0 0.5rem -0.125rem border" />
                        <Label text_size="0.6rem" color="muted-foreground">"spread&lt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" box_shadow="0 0.125rem 0.25rem 0.25rem muted-foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"spread&gt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="secondary" rounded="0" box_shadow="0.125rem 0.125rem 0.5rem 0 primary" />
                        <Label text_size="0.6rem" color="muted-foreground">"直角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="accent" rounded="1.25rem" box_shadow="0 0.375rem 1rem 0 ring" />
                        <Label text_size="0.6rem" color="muted-foreground">"大圆角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem"
                            box_shadow_color="primary"
                            box_shadow_offset_x="0.375"
                            box_shadow_offset_y="0.375"
                            box_shadow_blur="0.625"
                            box_shadow_spread="0"
                            box_shadow_inset={false} />
                        <Label text_size="0.6rem" color="muted-foreground">"子属性"</Label>
                    </VStack>
                </HStack>
                <Label font_weight="700" text_size="0.875rem">"Box-shadow — inset"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="secondary" rounded="0.5rem" box_shadow="inset 0 0 0.75rem 0 black" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 均匀"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="accent" rounded="0.5rem" box_shadow="inset 0.125rem 0.125rem 0.5rem 0 ring" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset+offset"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="inset 0 0 0.5rem 0.25rem destructive" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset+正 spread"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625"
                            box_shadow="inset -0.125rem -0.125rem 0.5rem -0.0625 border" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 负 offset+负 spread"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="background" rounded="0" box_shadow="inset 0 0.25rem 0.75rem 0 muted-foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 直角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Rect w="6.5rem" h="3.75rem" bg="primary" rounded="1rem" box_shadow="inset 0 0 1rem 0.125rem primary" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 大圆角"</Label>
                    </VStack>
                </HStack>
                <Label font_weight="700" text_size="0.875rem">"Text-shadow（glyphon）"</Label>
                <HStack gap="1rem" wrap="wrap" items="start">
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="foreground"
                            text_shadow="0.125rem 0.125rem 0.25rem muted-foreground">
                            "简写 dx/dy/blur"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"简写 + 语义色"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        // text_shadow_* 子属性与简写统一：裸数默认按 rem 解释，建议显式写 rem/px 提升可读性。
                        <Label text_size="1.1rem" font_weight="800" color="primary-foreground"
                            text_shadow_color="foreground"
                            text_shadow_offset_x="0.1875rem"
                            text_shadow_offset_y="0.1875rem"
                            text_shadow_blur="0.375rem">
                            "子属性覆盖"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"color + offset + blur"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="destructive"
                            text_shadow="-0.0625rem -0.0625rem 0.125rem muted-foreground">
                            "负偏移"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"左上浅影"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="secondary-foreground"
                            text_shadow="0 0.25rem 0.75rem border">
                            "重模糊"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"仅 dy + 大 blur"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="accent-foreground"
                            text_shadow="0.25rem 0 0 ring">
                            "横向偏移"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"blur=0"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="0.95rem" font_weight="700" color="foreground" leading="tight"
                            text_shadow="0.0625rem 0.0625rem 0.1875rem muted-foreground">
                            "多行 Label\n第二行仍应有阴影"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"换行"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ScrollShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="4) ScrollArea + Scrollbar">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Minimal visual cases: inset (inside/outside/mixed), visibility (auto/always/never), and all 4 corner combos."
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A) Inset: inside / outside / mixed"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-inset-inside" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="inside" scrollbar_visibility="always"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"inside"</Label>
                                <For each={1..=8} key={*item}>
                                    <Label>{ format!("row {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-inset-outside" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="outside" scrollbar_visibility="always"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"outside"</Label>
                                <For each={1..=8} key={*item}>
                                    <Label>{ format!("row {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-inset-mixed-x-out" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset_x="outside" scrollbar_inset_y="inside" scrollbar_visibility="always"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"mixed x:outside y:inside"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("row {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-inset-mixed-y-out" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset_x="inside" scrollbar_inset_y="outside" scrollbar_visibility="always"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"mixed x:inside y:outside"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("row {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"B) Visibility: auto / always / never"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Rect w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-auto" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="auto" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"auto"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("hover/scroll {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-always" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"always"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("always on {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-never" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="never" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"never"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("still scrollable {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"C) Four corners: (left/right) x (top/bottom)"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-corner-lt" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="outside" scrollbar_y_side="left" scrollbar_x_side="top"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"left + top"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("corner {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-corner-lb" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="outside" scrollbar_y_side="left" scrollbar_x_side="bottom"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"left + bottom"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("corner {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-corner-rt" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="outside" scrollbar_y_side="right" scrollbar_x_side="top"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"right + top"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("corner {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                    <Rect w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-corner-rb" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="outside" scrollbar_y_side="right" scrollbar_x_side="bottom"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"right + bottom (default sides)"</Label>
                                <For each={1..=8}>
                                    <Label>{ format!("corner {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Rect>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn LogicalShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="5) Logical Properties & Layout">
            <VStack gap="0.75rem">
                <Rect
                    inline_size="24rem"
                    block_size="auto"
                    bg="background"
                    rounded="0.5rem"
                    px="1rem"
                    py="0.75rem"
                >
                    <Label text_size="0.875rem">
                        "px/py + inline-size + block-size + auto-height"
                    </Label>
                </Rect>
                <HStack gap="0.5rem" wrap="wrap">
                    <Rect bg="background" rounded="0.4rem" px="0.75rem" py="0.5rem"><Label>"px/py"</Label></Rect>
                    <Rect bg="background" rounded="0.4rem" pt="0.75rem" pr="1rem" pb="0.25rem" pl="0.5rem"><Label>"pt/pr/pb/pl"</Label></Rect>
                    <Rect bg="background" rounded="0.4rem" mx="0.75rem" my="0.25rem"><Label>"mx/my"</Label></Rect>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn TextCompatibilityShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="6) Text Compatibility Stress Test">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Use this section to verify multilingual shaping, emoji fallback, combining marks, punctuation width, and bidirectional text."
                </Label>
                <Rect bg="background" rounded="0.5rem" p="0.75rem" w="100%">
                    <VStack gap="0.5rem" w="100%">
                        <Label w="100%" font_weight="700">"English / Latin"</Label>
                        <Label w="100%" leading="normal">
                            "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. 0123456789"
                        </Label>
                        <Label w="100%">"Accents: cafe cafe\u{0301} naive na\u{00EF}ve resume re\u{0301}sume\u{0301} fiance fianc\u{00E9}"</Label>

                        <Label w="100%" font_weight="700">"Chinese / Japanese / Korean"</Label>
                        <Label w="100%" id="hw-s6-label-cjk" leading="normal">
                            "中文测试：输入法、标点符号、全角半角、换行和字距表现。"
                        </Label>
                        <Label w="100%" leading="normal">
                            "日本語テスト：こんにちは世界。漢字・かな・カタカナ・句読点「」『』の表示確認。"
                        </Label>
                        <Label w="100%" leading="normal">
                            "한국어 테스트: 안녕하세요 세계. 받침, 조사, 기호와 줄바꿈 렌더링 확인."
                        </Label>

                        <Label w="100%" font_weight="700">"Other Scripts"</Label>
                        <Label w="100%" leading="normal">
                            "Arabic: مرحبا بالعالم، هذا اختبار لاتجاه الكتابة والمحاذاة وعرض الحروف."
                        </Label>
                        <Label w="100%" leading="normal">
                            "Hindi: नमस्ते दुनिया। संयुक्ताक्षर, मात्रा, और देवनागरी रेंडरिंग परीक्षण।"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Thai: สวัสดีชาวโลก ทดสอบการจัดวรรณยุกต์และระยะบรรทัด"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Russian: Привет, мир! Проверка кириллицы, знаков препинания и переносов."
                        </Label>

                        <Label w="100%" font_weight="700">"Emoji / Symbols / Math"</Label>
                        <Label w="100%" leading="normal">
                            "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦 🧑‍💻 🏳️‍🌈"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Currency & units: $ € £ ¥ ₹ ₽ ₩ ₫ ₺ 10km/h 23°C 5ms 1GiB"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Math & operators: +-*/ = != <= >= ≈ ≠ ≤ ≥ ∑ ∏ √ ∞ ∫ ∂ ∇ → ← ↔"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Box/shape symbols: ■ □ ▲ △ ● ○ ◆ ◇ ★ ☆ ※ ◎ ◉"
                        </Label>

                        <Label w="100%" font_weight="700">"Edge Cases"</Label>
                        <Label w="100%" leading="normal">
                            "Zero-width joiner: 👩‍🚀 👨‍🔬 👩‍💻 | Combining marks: A\u{0301} e\u{0301} o\u{0302} n\u{0303}"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789"
                        </Label>
                        <Label w="100%" leading="normal" id="hw-rtl-emoji-mixed">
                            "RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例"
                        </Label>
                        <Label w="8rem" leading="normal" id="hw-narrow-multiline-emoji">
                            "Narrow wrap: Mixed: Rust 👩‍💻 + 中文 + عربي 🔥 second line 456"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Punctuation width: ,.;:!? 。，；：！？ () [] {} <> 《》【】"
                        </Label>
                    </VStack>
                </Rect>
            </VStack>
        </MyCard>
    }
}

#[component]
fn EmojiRealtimeRegressionShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="6b) Windows Emoji Realtime Regression">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "实时输入回归：输入法候选上屏、粘贴、退格与替换后，Emoji 应在当帧或下一帧可见。"
                </Label>
                <Input id="hw-emoji-regression-input" placeholder="输入或粘贴: 😀 👨‍👩‍👧‍👦 👍🏽 🇨🇳 1️⃣" w="100%" />
                <Rect bg="background" rounded="0.5rem" p="0.75rem" w="100%">
                    <VStack gap="0.5rem" w="100%">
                        <Label w="100%" font_weight="700">"固定样例（视觉比对）"</Label>
                        <Label w="100%" leading="normal">"Family: 👨‍👩‍👧‍👦  Profession: 🧑‍💻"</Label>
                        <Label w="100%" leading="normal">"Skin tone: 👍🏽  Flags: 🇨🇳 🇺🇸"</Label>
                        <Label w="100%" leading="normal">
                            "Hello 😀 😁 😭  Keycap: 1️⃣ 2️⃣ 9️⃣  Mixed: Rust 👩‍💻 + 中文 + عربي"
                        </Label>
                    </VStack>
                </Rect>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ToggleControlsShowcase() -> rui::core::tree::Element {
    let sw = use_signal("demo_switch_checked", || false);
    let cb = use_signal("demo_checkbox_checked", || false);

    ui! {
        <MyCard title="8) Switch · Checkbox · Separator">
            <Reactive id={TOGGLE_SHOWCASE_REACTIVE}>
                <VStack gap="0.75rem" w="100%">
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Tab to focus, Space/Enter toggles (same as buttons). Horizontal Separator uses full width; vertical divider uses programmatic separator().vertical(true)."
                    </Label>

                    <Separator />

                    <Label font_weight="700" text_size="0.875rem">"Uncontrolled (internal state)"</Label>
                    <HStack gap="0.75rem" items="center" wrap="wrap">
                        <Label text_size="0.875rem">"Notifications"</Label>
                        <Switch />
                        <Label text_size="0.875rem">"Terms"</Label>
                        <Checkbox />
                        <Label text_size="0.875rem" color="muted-foreground">"Disabled"</Label>
                        <Checkbox disabled=true />
                    </HStack>

                    <Separator />

                    <Label font_weight="700" text_size="0.875rem">"Controlled (shared Signal + Flip)"</Label>
                    <HStack gap="0.75rem" items="center" wrap="wrap">
                        { Element::from(switch("hw_demo_switch").checked_signal(sw)) }
                        <Label text_size="0.875rem">{ format!("switch = {}", sw.get()) }</Label>
                        <Button variant="outline" size="sm" on_click={move || sw.set(!sw.get())}>
                            "Flip switch"
                        </Button>
                    </HStack>
                    <HStack gap="0.75rem" items="center" wrap="wrap">
                        { Element::from(checkbox("hw_demo_checkbox").checked_signal(cb)) }
                        <Label text_size="0.875rem">{ format!("checkbox = {}", cb.get()) }</Label>
                    </HStack>

                    <Separator />

                    <Label font_weight="700" text_size="0.875rem">"Vertical separator in a row"</Label>
                    <HStack gap="0.5rem" items="stretch" h="3rem" w="100%">
                        <Label>"Panel A"</Label>
                        { Element::from(separator().vertical(true)) }
                        <Label>"Panel B"</Label>
                        { Element::from(separator().vertical(true)) }
                        <Label>"Panel C"</Label>
                    </HStack>
                </VStack>
            </Reactive>
        </MyCard>
    }
}

#[component]
fn RadioSliderShowcase() -> rui::core::tree::Element {
    let tier = use_signal("hello_radio_tier", || "free".to_string());
    let level = use_signal("hello_slider_level", || 0.4_f32);

    ui! {
        <MyCard title="9) Radio group · Slider">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Radio: click a pill. Slider: click track to step, or focus + ArrowLeft/ArrowRight."
                </Label>
                { Element::from(
                    radio_group("hello_world:radio_tier")
                        .selected_signal(tier)
                        .default_value("free")
                        .options(vec![
                            ("free".to_string(), "Free".to_string()),
                            ("pro".to_string(), "Pro".to_string()),
                            ("team".to_string(), "Team".to_string()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("Tier: {}", tier.get()) }</Label>
                { Element::from(slider("hello_world:slider").value_signal(level)) }
                <Label text_size="0.875rem">{ format!("Level: {:.2}", level.get()) }</Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn SelectPopoverTabShowcase() -> rui::core::tree::Element {
    let tab_idx = use_signal("hello_tabs_idx", || 0usize);
    let pop_open = use_signal("hello_popover_open", || false);
    let select_val = use_signal("hello_select_plan", || "basic".to_string());

    ui! {
        <MyCard title="10) Select · Popover · Tab strip (AccessKit)">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Select / Popover: wrap in <Reactive> so `open` / menu updates subscribe. Tab strip: `tabs_headless` focus id uses prefix → TabList."
                </Label>
                <Reactive id={SELECT_POPOVER_SHOWCASE_REACTIVE}>
                    <VStack gap="0.75rem" w="100%">
                        { Element::from(
                            select("hello_world:select_plan")
                                .value_signal(select_val)
                                .default_value("basic")
                                .options(vec![
                                    ("basic".to_string(), "Basic".to_string()),
                                    ("pro".to_string(), "Pro".to_string()),
                                    ("team".to_string(), "Team".to_string()),
                                ]),
                        ) }
                        <Label text_size="0.875rem">{ format!("Selected plan: {}", select_val.get()) }</Label>
                        <Button variant="outline" size="sm" on_click={move || pop_open.set(!pop_open.get())}>
                            { if pop_open.get() { "Close popover" } else { "Open popover" } }
                        </Button>
                        <Popover open={pop_open}>
                            <Label>"Popover panel — Escape closes (last-registered popup)."</Label>
                        </Popover>
                        <Label text_size="0.75rem" color="muted-foreground">
                            { format!(
                                "Shared tab index: {}. Focus id `{}hello_tabs_demo` (Tab to focus order, then ← / →).",
                                tab_idx.get(),
                                TABSTRIP_FOCUS_PREFIX,
                            ) }
                        </Label>
                        <HStack gap="0.35rem" items="center" wrap="wrap">
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(0)}>"0"</Button>
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(1)}>"1"</Button>
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(2)}>"2"</Button>
                        </HStack>
                        { Element::from(
                            tabs_headless("hello_tabs_demo", 3).selected_signal(tab_idx),
                        ) }
                    </VStack>
                </Reactive>
            </VStack>
        </MyCard>
    }
}

#[component]
fn UtilityClassShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="7) Utility Classes (class / tw / uno)">
            <VStack gap="1rem">
                <VStack gap="0.5rem">
                    <Label font_weight="700">"A) class=..."</Label>
                    <Rect class="p-4 rounded-lg bg-secondary">
                        <VStack class="gap-2">
                            <Label class="text-sm text-muted-foreground">"This block uses class=\"p-4 rounded-lg bg-secondary\""</Label>
                            <Label class="text-base font-semibold">"Spacing + radius + color + text size"</Label>
                            <Label class="mt-2 text-xs">"mt-2 applied here"</Label>
                        </VStack>
                    </Rect>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"B) tw=..."</Label>
                    <Rect tw="grid grid-cols-2 grid-rows-2 gap-x-3 gap-y-2 w-full min-h-20 p-2 bg-muted rounded-md">
                        <Rect tw="bg-secondary rounded-md h-8" />
                        <Rect tw="bg-secondary rounded-md h-8" />
                        <Label tw="self-end text-xs text-muted-foreground">"grid-rows-2 + gap-x/y + self-end"</Label>
                        <Rect tw="bg-secondary rounded-md h-8" />
                    </Rect>
                    <HStack tw="gap-3 items-center flex-wrap">
                        <Rect tw="w-24 h-12 rounded-md bg-secondary" />
                        <Rect tw="w-[120px] h-[48px] rounded-xl bg-muted" />
                        <Rect tw="w-20 h-12 rounded-full bg-accent" />
                    </HStack>
                    <Label tw="text-sm text-muted-foreground mt-2">
                        "tw supports numeric scale (e.g. mt-2 => 8px) and arbitrary values [120px]"
                    </Label>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"C) uno=..."</Label>
                    <Card>
                        <Label text_size="0.875rem">"components::Card（tokens 间距 + 圆角）"</Label>
                    </Card>
                    <Rect uno="p-3 rounded-md bg-secondary">
                        <VStack uno="gap-2">
                            <Label uno="text-sm">"uno=\"...\" works the same way as class/tw"</Label>
                            <HStack uno="gap-2">
                                <Button variant="outline" size="sm">"Button"</Button>
                                <Label uno="mt-1 text-xs text-muted-foreground">"Label with mt-1"</Label>
                            </HStack>
                        </VStack>
                    </Rect>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn GridAndDragShowcase() -> Element {
    let order = use_signal("hw_grid_sort_order", || {
        ["Alpha", "Bravo", "Charlie", "Delta"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let commit = use_signal(HW_GRID_SORT_COMMIT, || None::<(usize, usize)>);
    let hover = use_signal(HW_GRID_SORT_HOVER, || None::<usize>);

    // `for_keyed!` 闭包含 `{}`，不能放在 `ui!` 的单层 `{ ... }` 表达式里（会与 RSX 花括号扫描冲突）。
    let sort_list = rui::reactive!(GRID_DRAG_REACTIVE, move || {
        if let Some((from, to)) = commit.get() {
            let mut v = order.get();
            rui::primitives::apply_reorder_vec(&mut v, from, to);
            order.set(v);
            commit.set(None);
        }
        let hover_ix = hover.get();
        let items = order.get();
        let rows = rui::for_keyed!(
            "hw-grid-sort-list",
            items.as_slice(),
            |s: &String| s.clone(),
            |row_label: &String| {
                let i = items.iter().position(|x| x == row_label).unwrap_or(0);
                let hl = hover_ix == Some(i);
                hw_grid_sort_row(row_label, i, items.as_slice(), hl)
            }
        );
        Element::from(vstack().w("100%").gap("0.375rem").children(rows))
    });

    ui! {
        <MyCard title="Grid + drag-reorder (ShadCN Card: rui::components::Card)">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Grid（cols / rows / col_span）沙盘；列表为 for_keyed! + drag_reorder_handle；排序块用 reactive! 订阅 commit/hover。"
                </Label>
                <Grid cols="3" rows="2" gap="0.5rem" w="100%" min_h="7rem">
                    <Rect col_span="2" p="0.75rem" bg="muted" class="rounded-md" items="center" justify="center">
                        <Label>"col_span = 2"</Label>
                    </Rect>
                    <Rect p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"侧栏"</Label>
                    </Rect>
                    <Rect p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"(2,2)"</Label>
                    </Rect>
                    <Rect p="0.75rem" bg="muted" class="rounded-md" items="center" justify="center">
                        <Label>"(3,2)"</Label>
                    </Rect>
                    <Rect p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"(3,3)"</Label>
                    </Rect>
                </Grid>
                { sort_list }
            </VStack>
        </MyCard>
    }
}

#[component]
fn PrimitivesShowcase() -> rui::core::tree::Element {
    let overlay_open = use_signal("prim_overlay_open", || false);
    let trap_on = use_signal("prim_trap_on", || false);

    ui! {
        <VStack w="100%" gap="0">
            <MyCard title="11) Image · Portal · FocusTrap · VirtualList">
                <VStack gap="0.75rem" w="100%">
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "GPU Image (queue_image_rgba8 + <Image>). Portal 提升浮层到根节点。FocusTrap 限制 Tab 顺序。VirtualList：Reactive + visible_item_range；完整滚动条需额外占位总高度（此处 200 行演示）。"
                    </Label>
                    <HStack gap="0.75rem" items="start" wrap="wrap">
                        <VStack gap="0.25rem">
                            <Label font_weight="700" text_size="0.75rem">"Texture quad"</Label>
                            <Image id="demo-checker-img" texture="demo-checker" w="5.5rem" h="5.5rem" rounded="0.375rem" />
                        </VStack>
                        <VStack gap="0.35rem" flex_grow="1" min_w="12rem">
                            <Label font_weight="700" text_size="0.75rem">"Controls"</Label>
                            <HStack gap="0.5rem" wrap="wrap">
                                <Button id="prim-overlay-toggle" variant="outline" on_click={move || overlay_open.set(!overlay_open.get())}>
                                    { if overlay_open.get() { "关闭 Portal 浮层" } else { "打开 Portal 浮层" } }
                                </Button>
                                <Button id="prim-trap-toggle" variant="secondary" on_click={move || trap_on.set(!trap_on.get())}>
                                    { if trap_on.get() { "关闭焦点陷阱" } else { "启用焦点陷阱" } }
                                </Button>
                            </HStack>
                            <FocusTrap id="prim-trap" active={trap_on.get()} order="trap-a,trap-b">
                                <HStack gap="0.5rem" items="center" wrap="wrap">
                                    <Label text_size="0.75rem" color="muted-foreground">"陷阱内 Tab:"</Label>
                                    <Button id="trap-a" variant="default">"A"</Button>
                                    <Button id="trap-b" variant="outline">"B"</Button>
                                </HStack>
                            </FocusTrap>
                        </VStack>
                    </HStack>
                    <Label id="hw-s11-virt-title" font_weight="700" text_size="0.875rem">"虚拟列表窗口（200 行，仅挂载可见区间）"</Label>
                    <ScrollArea id="virt-demo" w="100%" h="11rem" overflow_y="auto" overflow_x="hidden" p="0.5rem" bg="muted" rounded="0.375rem">
                        { rui::reactive!(PRIMITIVES_VIRT_REACTIVE, move || {
                            let sy = use_signal(&scroll_offset_y_signal_key("virt-demo"), || 0.0_f32).get();
                            let vh = use_signal(&scroll_viewport_h_signal_key("virt-demo"), || 160.0_f32).get();
                            let (a, b) = rui::visible_item_range(sy, vh, 22.0, 200, 8);
                            let mut col = vstack().w("100%").gap("2px");
                            for i in a..b {
                                let row: Element = label(&format!("virt-row-{i}"))
                                    .text(format!("virtual row {i:04} (range {a}..{b})"))
                                    .into();
                                col = col.child(row);
                            }
                            Element::from(col)
                        }) }
                    </ScrollArea>
                </VStack>
            </MyCard>
            <Portal id="prim-portal-root">
                <Show when={overlay_open.get()}>
                    <Rect absolute=true inset="0" w="100%" h="100%" items="center" justify="center" bg="black" z_index="900">
                        <VStack gap="0.75rem" p="1.25rem" bg="secondary" rounded="0.75rem" max_w="22rem" m="1rem">
                            <Label font_weight="700" text_size="1.125rem">"Portal 浮层"</Label>
                            <Label text_size="0.875rem" color="muted-foreground">
                                "此节点经 Portal 提升到根，盖在主内容之上。"
                            </Label>
                            <Button id="prim-overlay-close" variant="default" on_click={move || overlay_open.set(false)}>
                                "关闭"
                            </Button>
                        </VStack>
                    </Rect>
                </Show>
            </Portal>
        </VStack>
    }
}

fn main() {
    let (rgba, w, h) = checker_rgba_texture();
    rui::queue_image_rgba8("demo-checker", w, h, rgba);

    rui::app::run(move || {
        let show_extra = use_signal("show_extra", || false);

        ui! {
            <HStack w="100%" h="100%" bg="background" gap="0" relative=true>
                <ScrollArea id="root-scroll" h="100%" overflow_y="auto" overflow_x="auto" grow="1" scroll_behavior="smooth">
                    <VStack items="center" gap="1.5rem" p="2rem" w="100%">
                        <VStack
                            items="center"
                            text_align="center"
                            text_color="muted-foreground"
                            text_size="1rem"
                            leading="normal"
                            gap="0.5rem"
                            w="100%"
                            max_w="48rem"
                            px="1rem"
                        >
                            <Label w="100%" text_size="2.5rem" font_weight="900">"RUI Framework"</Label>
                            <Label w="100%" text_color="foreground" font_weight="500">
                                "Layered Architecture • Functional Components • Hot Reload"
                            </Label>
                        </VStack>

                        <VStack max_w="70rem" w="100%" gap="1rem">
                            <ButtonShowcase />
                            <ButtonHoverEasingShowcase />
                            <InputShowcase />
                            <TypographyShowcase />
                            <ShadowShowcase />
                            <ScrollShowcase />
                            <LogicalShowcase />
                            <TextCompatibilityShowcase />
                            <EmojiRealtimeRegressionShowcase />
                            <UtilityClassShowcase />
                            <ToggleControlsShowcase />
                            <RadioSliderShowcase />
                            <SelectPopoverTabShowcase />
                            <GridAndDragShowcase />
                            <PrimitivesShowcase />

                            <VStack items="center" gap="1rem" p="1rem">
                                <Button variant="secondary" on_click={move || show_extra.set(!show_extra.get())}>
                                    { if show_extra.get() { "Hide Technical Details" } else { "Show Technical Details" } }
                                </Button>

                                { if show_extra.get() {
                                    ui! {
                                        <Rect bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                                            <VStack gap="0.5rem">
                                                <Label font_weight="700">"Technical Stack:"</Label>
                                                <Label>"• Layout: Taffy (Flexbox)"</Label>
                                                <Label>"• Text: Glyphon (Cosmic-text wrapper)"</Label>
                                                <Label>"• Rendering: WGPU (Direct GPU Instances)"</Label>
                                                <Label>"• Reactivity: Signal-based fine-grained updates"</Label>
                                            </VStack>
                                        </Rect>
                                    }
                                } else {
                                    rui::core::tree::Element::new()
                                } }
                            </VStack>
                        </VStack>
                    </VStack>
                </ScrollArea>
            </HStack>
        }
    });
}
