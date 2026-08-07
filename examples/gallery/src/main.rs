#![allow(non_snake_case)]
// Release GUI builds should not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RUI Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏分区：基础 · 表单 · 浮层 · 数据 · 导航 · 布局 · 文本 · 关于。
//! 运行：`cargo run -p gallery`

use rui::app::navigator::{Navigator, Presentation, PushOptions};
use rui::component;
use rui::components::scroll_area::{scroll_offset_y_signal_key, scroll_viewport_h_signal_key};
use rui::components::*;
use rui::components::{navigation_host, tabs_headless, TABSTRIP_FOCUS_PREFIX};
use rui::core::reactivity::{use_signal, Signal};
use rui::core::tree::Element;
use rui::primitives::{hstack, r#box, vstack};
use rui::style::{parse_color, parse_text_color};
use rui::ui;
use std::collections::HashSet;

/// Stable `template_id` for fine-grained reconcile (must stay unique per mounted block).
const PRIMITIVES_SHOWCASE_REACTIVE: &str = "examples/gallery/PrimitivesShowcase:controls";
const PRIMITIVES_VIRT_REACTIVE: &str = "examples/gallery/PrimitivesShowcase:virt_list";
const GRID_DRAG_REACTIVE: &str = "examples/gallery/GridAndDragShowcase:main";
const NAVIGATOR_SHOWCASE_REACTIVE: &str = "examples/gallery/NavigatorShowcase:main";
const GAL_GRID_SORT_COMMIT: &str = "__rui_drag_commit:gal-grid-sort";
const GAL_GRID_SORT_HOVER: &str = "__rui_drag_hover:gal-grid-sort";
const GAL_GRID_LIST_ID: &str = "gal-grid-sort";

fn gal_grid_sort_row(row_label: &str, i: usize, _items: &[String], hl: bool) -> Element {
    let grip: Element = r#box()
        .drag_reorder_handle(GAL_GRID_LIST_ID, i)
        .p("0.25rem")
        .rounded(4.0)
        .bg(parse_color("secondary"))
        .into();
    let mut caption_el: Element = label(&format!("gal-grid-sort-{i}")).text(row_label).into();
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
    // 仅相对同级默认 0 抬一层；勿依赖全局 magic z（自定义 z 仍可能盖住）。
    .z_index(if hl { 1 } else { 0 })
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
    let child = children.unwrap_or_else(rui::core::tree::Element::new);
    ui! {
        <VStack w="100%" min_w="0" p="1rem" bg="background" gap="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.125" class="rounded-lg">
            <Label w="100%" text_size="1.25rem" font_weight="700" leading="normal">{ title }</Label>
            { child }
        </VStack>
    }
}

/// Solid getter-props 试点：`#[component(reactive)]` + Signal 字段；文案/图标走自动 bind，不整卡重建。
#[component(reactive)]
fn ReactiveStatusBadge(
    id: String,
    label: rui::core::reactivity::Signal<String>,
    ok: rui::core::reactivity::Signal<bool>,
) -> rui::core::tree::Element {
    ui! {
        <HStack gap="0.5rem" items="center" p="0.5rem" class="rounded" bg="secondary">
            <Icon
                name={ if ok.get() { "check".to_string() } else { "ban".to_string() } }
                size="1rem"
                color="foreground"
            />
            <Label text_size="0.875rem">{ label.get() }</Label>
        </HStack>
    }
}

#[component]
fn ButtonShowcase() -> rui::core::tree::Element {
    let count = use_signal("gallery_btn_count", || 0);
    let selected = use_signal("gallery_selected_variant", || "default".to_string());

    ui! {
        <MyCard title="1) Button Variants + State">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="default" on_click={move || {
                        increment_count(count);
                        choose_variant(selected, "default");
                    }}>
                        { format!("Default ({})", count.get()) }
                    </Button>
                    <Button variant="secondary" on_click={move || choose_variant(selected, "secondary")}>
                        "Secondary"
                    </Button>
                    <Button variant="outline" on_click={move || choose_variant(selected, "outline")}>
                        "Outline"
                    </Button>
                    <Button variant="ghost" on_click={move || choose_variant(selected, "ghost")}>
                        "Ghost"
                    </Button>
                    <Button variant="destructive" on_click={move || choose_variant(selected, "destructive")}>
                        "Destructive"
                    </Button>
                    <Button variant="link" on_click={move || choose_variant(selected, "link")}>
                        "Link"
                    </Button>
                    <Button variant="default" disabled=true>"Disabled"</Button>
                </HStack>
                <Label w="100%" text_size="0.875rem" color="muted-foreground" leading="normal">
                    { format!("Selected variant: {}", selected.get()) }
                </Label>
            </VStack>
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
                    <Button id="gal-hover-spring" variant="ghost" hover_easing="spring">"Ghost · spring"</Button>
                    <Button id="gal-hover-cubic" variant="ghost" hover_easing="cubic">"Ghost · cubic"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputShowcase() -> rui::core::tree::Element {
    let mark_invalid = use_signal("gal_input_mark_invalid", || false);
    let badge_ok = use_signal("gal_status_badge_ok", || true);
    let badge_label = use_signal("gal_status_badge_label", || "Ready".to_string());

    ui! {
        <MyCard title="2) Input (ShadCN-style)">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "text / password / number / email / tel / url / search — type 过滤与非空校验"
                </Label>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <Input id="gal-s2-input-a" placeholder="Placeholder…" flex_grow="1" />
                    <Input id="gal-s2-input-b" placeholder="Invalid" invalid=true flex_grow="1" />
                    <Input id="gal-s2-input-pw" type="password" flex_grow="1" />
                    <Input id="gal-s2-input-num" type="number" flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <Input id="gal-s2-email" type="email" flex_grow="1" />
                    <Input id="gal-s2-tel" type="tel" flex_grow="1" />
                    <Input id="gal-s2-url" type="url" flex_grow="1" />
                    <Input id="gal-s2-search" type="search" flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="center" wrap="wrap" w="100%">
                    <Input
                        id="gal-s2-input-bound-invalid"
                        placeholder="invalid={signal}"
                        invalid={mark_invalid.get()}
                        flex_grow="1"
                    />
                    <Button
                        id="gal-s2-mark-invalid"
                        variant="outline"
                        size="sm"
                        on_click={move || mark_invalid.set(!mark_invalid.peek())}
                    >
                        { if mark_invalid.get() { "Clear invalid" } else { "Mark invalid" } }
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="center" wrap="wrap" w="100%">
                    <ReactiveStatusBadge
                        id="gal-status-badge"
                        label={badge_label}
                        ok={badge_ok}
                    />
                    <Button
                        variant="ghost"
                        size="sm"
                        on_click={move || {
                            let next = !badge_ok.peek();
                            badge_ok.set(next);
                            badge_label.set(if next { "Ready".into() } else { "Blocked".into() });
                        }}
                    >
                        "Toggle badge"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn PickerPlaceholdersShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="2b) Picker placeholders (WIP)">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "file / directory / color / date* / time — 分模块占位（非 Input type）"
                </Label>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <FilePicker id="gal-file" accept="image/*" flex_grow="1" />
                    <DirectoryPicker id="gal-dir" flex_grow="1" />
                    <ColorPicker id="gal-color" value="#3b82f6" flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <DatePicker id="gal-date" kind="date" flex_grow="1" />
                    <DatePicker id="gal-dt" kind="datetime-local" flex_grow="1" />
                    <TimePicker id="gal-time" flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <DatePicker id="gal-month" kind="month" flex_grow="1" />
                    <DatePicker id="gal-week" kind="week" flex_grow="1" />
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
                        <Label w="100%" id="gal-s6-label-cjk" leading="normal">
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
                        <Label w="100%" leading="normal" id="gal-rtl-emoji-mixed">
                            "RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例"
                        </Label>
                        <Label w="8rem" leading="normal" id="gal-narrow-multiline-emoji">
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
                <Input id="gal-emoji-regression-input" placeholder="输入或粘贴: 😀 👨‍👩‍👧‍👦 👍🏽 🇨🇳 1️⃣" w="100%" />
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
        </MyCard>
    }
}

#[component]
fn RadioSliderShowcase() -> rui::core::tree::Element {
    let tier = use_signal("gallery_radio_tier", || "free".to_string());
    let level = use_signal("gallery_slider_level", || 0.4_f32);

    ui! {
        <MyCard title="9) Radio group · Slider">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Radio / Slider 自带组件级响应式边界；下方标签用自动 bind_text 展示受控值。"
                </Label>
                { Element::from(
                    radio_group("gallery:radio_tier")
                        .selected_signal(tier)
                        .default_value("free")
                        .options(vec![
                            ("free".to_string(), "Free".to_string()),
                            ("pro".to_string(), "Pro".to_string()),
                            ("team".to_string(), "Team".to_string()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("Tier: {}", tier.get()) }</Label>
                { Element::from(slider("gallery:slider").value_signal(level)) }
                <Label text_size="0.875rem">{ format!("Level: {:.2}", level.get()) }</Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn SelectPopoverTabShowcase() -> rui::core::tree::Element {
    let tab_idx = use_signal("gallery_tabs_idx", || 0usize);
    let pop_open = use_signal("gallery_popover_open", || false);
    let select_val = use_signal("gallery_select_plan", || "basic".to_string());

    ui! {
        <MyCard title="10) Select · Popover · Tab strip (AccessKit)">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Select / Popover 自带组件级响应式边界；Tab 文案与开关用自动 bind_text。"
                </Label>
                <VStack gap="0.75rem" w="100%">
                    { Element::from(
                        select("gallery:select_plan")
                            .value_signal(select_val)
                            .default_value("basic")
                            .options(vec![
                                ("basic".to_string(), "Basic".to_string()),
                                ("pro".to_string(), "Pro".to_string()),
                                ("team".to_string(), "Team".to_string()),
                            ]),
                    ) }
                    <Label text_size="0.875rem">{ format!("Selected plan: {}", select_val.get()) }</Label>
                    <Button
                        variant="outline"
                        size="sm"
                        on_click={move || pop_open.set(!pop_open.peek())}
                    >
                        { if pop_open.get() { "Close popover" } else { "Open popover" } }
                    </Button>
                    <Popover open={pop_open}>
                        <Label>"Popover panel — Escape closes (last-registered popup)."</Label>
                    </Popover>
                    <VStack gap="0.75rem" w="100%">
                        <Label text_size="0.75rem" color="muted-foreground">
                            { format!(
                                "Shared tab index: {}. Focus id `{}gallery_tabs_demo` (Tab to focus order, then ← / →).",
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
                            tabs_headless("gallery_tabs_demo", 3).selected_signal(tab_idx),
                        ) }
                    </VStack>
                </VStack>
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
    let order = use_signal("gal_grid_sort_order", || {
        ["Alpha", "Bravo", "Charlie", "Delta"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let commit = use_signal(GAL_GRID_SORT_COMMIT, || None::<(usize, usize)>);
    let hover = use_signal(GAL_GRID_SORT_HOVER, || None::<usize>);

    // `for_keyed!` 闭包含 `{}`，不能放在 `ui!` 的单层 `{ ... }` 表达式里（会与 RSX 花括号扫描冲突）。
    let sort_list = rui::reactive!(GRID_DRAG_REACTIVE, move || {
        if let Some((from, to)) = commit.get() {
            let mut v = order.get();
            rui::utils::drag_reorder::apply_reorder_vec(&mut v, from, to);
            order.set(v);
            commit.set(None);
        }
        let hover_ix = hover.get();
        let items = order.get();
        let rows = rui::for_keyed!(
            "gal-grid-sort-list",
            items.as_slice(),
            |s: &String| s.clone(),
            |row_label: &String| {
                let i = items.iter().position(|x| x == row_label).unwrap_or(0);
                let hl = hover_ix == Some(i);
                gal_grid_sort_row(row_label, i, items.as_slice(), hl)
            }
        );
        Element::from(vstack().w("100%").gap("0.375rem").children(rows))
    });

    ui! {
        <MyCard title="Grid + drag-reorder (ShadCN Card: rui::components::Card)">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "GStack（cols / rows / col_span）沙盘；列表为 for_keyed! + drag_reorder_handle；排序块用 reactive! 订阅 commit/hover。"
                </Label>
                <GStack cols="3" rows="2" gap="0.5rem" w="100%" min_h="7rem">
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
                </GStack>
                { sort_list }
            </VStack>
        </MyCard>
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GalNavRoute {
    Home,
    Detail,
    SheetHelp,
}

#[component]
fn NavigatorShowcase() -> Element {
    let nav = Navigator::new("gal_nav_demo", GalNavRoute::Home, [GalNavRoute::Home]);
    let nav_bar = nav.clone();
    let nav_detail = nav.clone();
    let nav_sheet = nav.clone();
    let nav_home = nav.clone();

    let body = rui::reactive!(NAVIGATOR_SHOWCASE_REACTIVE, move || {
        let n = nav.clone();
        Element::from(navigation_host(n, |route| match route {
            GalNavRoute::Home => ui! {
                <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="muted">
                    <Label font_weight="600">"Home（root）"</Label>
                    <Label text_size="0.75rem" color="muted-foreground">
                        "Esc：先关 portal 浮层，再 pop 页面。侧栏回 Home 会截断栈。"
                    </Label>
                </VStack>
            },
            GalNavRoute::Detail => ui! {
                <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="secondary">
                    <Label font_weight="600">"Detail（Host）"</Label>
                    <Label text_size="0.75rem" color="muted-foreground">"keep-alive；返回 pop 到上一页"</Label>
                </VStack>
            },
            GalNavRoute::SheetHelp => ui! {
                <VStack w="16rem" p="1rem" gap="0.5rem" bg="background" class="rounded-md">
                    <Label font_weight="600">"Help Sheet"</Label>
                    <Label text_size="0.75rem" color="muted-foreground">
                        "点遮罩或 Esc 关闭（Navigator::pop）"
                    </Label>
                </VStack>
            },
        }))
    });

    ui! {
        <MyCard title="Navigator（portal_overlay + view 双栈）">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Host / Sheet；Home 为 root（截断）。Detail 带 Slide 转场。埋层禁命中；Host 卸载自动 dispose。"
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <Button
                        id="gal-nav-home"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            let _ = nav_home.bring_to_front(&GalNavRoute::Home);
                        }}
                    >
                        "Home"
                    </Button>
                    <Button
                        id="gal-nav-detail"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            nav_detail.push(
                                GalNavRoute::Detail,
                                PushOptions::default().with_slide(),
                            );
                        }}
                    >
                        "Push Detail"
                    </Button>
                    <Button
                        id="gal-nav-sheet"
                        size="sm"
                        variant="secondary"
                        on_click={move || {
                            nav_sheet.push(
                                GalNavRoute::SheetHelp,
                                PushOptions {
                                    presentation: Presentation::Sheet,
                                    ..PushOptions::default()
                                },
                            );
                        }}
                    >
                        "Open Sheet"
                    </Button>
                    <Button
                        id="gal-nav-back"
                        size="sm"
                        variant="ghost"
                        on_click={move || {
                            let _ = nav_bar.pop();
                        }}
                    >
                        "Back"
                    </Button>
                </HStack>
                <Rect w="100%" h="10rem" relative=true class="rounded-md" bg="background">
                    { body }
                </Rect>
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
                <Reactive id={PRIMITIVES_SHOWCASE_REACTIVE}>
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
                        <Label id="gal-s11-virt-title" font_weight="700" text_size="0.875rem">"虚拟列表窗口（200 行，仅挂载可见区间）"</Label>
                        <ScrollArea id="virt-demo" w="100%" h="11rem" overflow_y="auto" overflow_x="hidden" p="0.5rem" bg="muted" rounded="0.375rem">
                            { rui::reactive!(PRIMITIVES_VIRT_REACTIVE, move || {
                                let sy = use_signal(&scroll_offset_y_signal_key("virt-demo"), || 0.0_f32).get();
                                let vh = use_signal(&scroll_viewport_h_signal_key("virt-demo"), || 160.0_f32).get();
                                let (a, b) = rui::visible_item_range(sy, vh, 22.0, 200, 8);
                                let mut col = vstack().w("100%").gap("2px");
                                // 行文案勿夹每帧变化的 window 边界，否则滚动时整窗 text 全变、glyphon 每帧重跑。
                                for i in a..b {
                                    let row: Element = label(&format!("virt-row-{i}"))
                                        .text(format!("virtual row {i:04}"))
                                        .into();
                                    col = col.child(row);
                                }
                                Element::from(col)
                            }) }
                        </ScrollArea>
                    </VStack>
                </Reactive>
            </MyCard>
            <Portal id="prim-portal-root">
                <Reactive id="examples/gallery/PrimitivesShowcase:portal">
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
                </Reactive>
            </Portal>
        </VStack>
    }
}

#[component]
fn TextareaComboboxShowcase() -> Element {
    let combo = use_signal("gal_combo_city", || "sh".to_string());
    let seg = use_signal("gal_segmented_view", || "list".to_string());
    let align = use_signal("gal_toggle_align", || "left".to_string());

    ui! {
        <MyCard title="Textarea · Combobox · Segmented · ToggleGroup">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "多行输入、可过滤 Combobox、分段控件与 ToggleGroup。"
                </Label>
                <Textarea id="gal-textarea" placeholder="多行备注…" rows={4} />
                { Element::from(
                    combobox("gal-combo")
                        .value_signal(combo)
                        .placeholder("搜索城市…")
                        .options([
                            ("sh", "Shanghai"),
                            ("bj", "Beijing"),
                            ("sz", "Shenzhen"),
                            ("gz", "Guangzhou"),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("Combobox: {}", combo.get()) }</Label>
                { Element::from(
                    segmented("gal-seg")
                        .value_signal(seg)
                        .default_value("list")
                        .options(vec![
                            ("list".into(), "List".into()),
                            ("grid".into(), "Grid".into()),
                            ("map".into(), "Map".into()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("Segmented: {}", seg.get()) }</Label>
                { Element::from(
                    toggle_group("gal-align")
                        .value_signal(align)
                        .default_value("left")
                        .options(vec![
                            ("left".into(), "Left".into()),
                            ("center".into(), "Center".into()),
                            ("right".into(), "Right".into()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("Align: {}", align.get()) }</Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn OverlayFeedbackShowcase() -> Element {
    let cmd_open = use_signal("gal_cmd_open", || false);
    let dialog_open = use_signal("gal_dialog_open", || false);
    let alert_open = use_signal("gal_alert_open", || false);
    let sheet_open = use_signal("gal_sheet_open", || false);
    let progress_val = use_signal("gal_progress", || 0.45f32);
    let open_cmd = cmd_open;
    let open_dialog = dialog_open;
    let open_alert = alert_open;
    let open_sheet = sheet_open;
    let bump = progress_val;

    let cmd_el: Element = command("gal-cmd")
        .open_signal(cmd_open)
        .action(CommandAction::new("toast", "Show toast", || {
            toast_push("Hello from Command", 2500);
        }))
        .action(CommandAction::new("log", "Log ping", || {
            log::info!("gallery command: ping");
        }))
        .into();
    let dialog_el: Element = dialog_panel(dialog_open, "Gallery Dialog")
        .id("gal-dialog")
        .child(|| {
            ui! {
                <VStack gap="0.5rem">
                    <Label text_size="0.875rem" color="muted-foreground">
                        "Esc 关闭；面板经 portal_overlay 栈管理。"
                    </Label>
                </VStack>
            }
        })
        .into();
    let alert_el: Element = alert_dialog("gal-alert", "确认操作")
        .open_signal(alert_open)
        .description("此操作仅作 Gallery 演示，不会修改数据。")
        .confirm_label("确认")
        .cancel_label("取消")
        .on_confirm(|| {
            toast_push("Confirmed", 2000);
        })
        .into();
    let sheet_el: Element = sheet("gal-sheet")
        .open_signal(sheet_open)
        .w("18rem")
        .child(|| {
            ui! {
                <VStack gap="0.5rem" p="1rem">
                    <Label font_weight="700">"Sheet"</Label>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "侧滑面板；点遮罩或 Esc 关闭。"
                    </Label>
                </VStack>
            }
        })
        .into();
    let tip: Element = tooltip("gal-tip", "Hover tip").trigger(|_open| {
        ui! { <Button id="gal-tip-trigger" variant="outline" size="sm">{ "Hover for tooltip" }</Button> }
    });
    let ctx: Element = context_menu("gal-ctx")
        .item("Copy", || {
            toast_push("Copy", 1500);
        })
        .item("Paste", || {
            toast_push("Paste", 1500);
        })
        .trigger(|| {
            ui! {
                <Rect id="gal-ctx-trigger" p="0.75rem" bg="muted" class="rounded-md" w="100%">
                    <Label text_size="0.875rem">{ "右键此区域打开 ContextMenu" }</Label>
                </Rect>
            }
        });
    let progress_el: Element = progress("gal-progress").value_signal(progress_val).into();

    ui! {
        <MyCard title="Command · Dialog · Alert · Sheet · Toast · Tooltip · Progress">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "浮层与反馈：命令面板、模态、确认框、侧滑、Toast、Tooltip、进度条。"
                </Label>
                <HStack gap="0.5rem" wrap="wrap" items="center">
                    <Button id="gal-cmd-open" variant="secondary" size="sm" on_click={move || open_cmd.set(true)}>
                        "Command"
                    </Button>
                    <Button id="gal-dialog-open" variant="outline" size="sm" on_click={move || open_dialog.set(true)}>
                        "Dialog"
                    </Button>
                    <Button id="gal-alert-open" variant="outline" size="sm" on_click={move || open_alert.set(true)}>
                        "Alert"
                    </Button>
                    <Button id="gal-sheet-open" variant="outline" size="sm" on_click={move || open_sheet.set(true)}>
                        "Sheet"
                    </Button>
                    <Button id="gal-toast-push" variant="default" size="sm" on_click={move || {
                        toast_push("Toast from Gallery", 3000);
                    }}>
                        "Toast"
                    </Button>
                    { tip }
                </HStack>
                { ctx }
                <VStack gap="0.35rem" w="100%">
                    <Label font_weight="600" text_size="0.875rem">{ "Progress" }</Label>
                    { progress_el }
                    <Button id="gal-progress-bump" variant="outline" size="sm" on_click={move || {
                        let v = bump.peek();
                        bump.set(if v >= 1.0 { 0.0 } else { (v + 0.1).min(1.0) });
                    }}>{ "Bump +10%" }</Button>
                </VStack>
                { cmd_el }
                { dialog_el }
                { alert_el }
                { sheet_el }
            </VStack>
        </MyCard>
    }
}

#[component]
fn DataChartShowcase() -> Element {
    let table_sel = use_signal("gal_dt_sel", || None::<usize>);
    let expanded = use_signal("gal_tree_exp", || {
        let mut s = HashSet::new();
        s.insert("root".into());
        s
    });

    let table_el: Element = data_table("gal-table")
        .columns([
            DataColumn {
                header: "Name".into(),
                width: 120.0,
            },
            DataColumn {
                header: "Role".into(),
                width: 100.0,
            },
            DataColumn {
                header: "Score".into(),
                width: 80.0,
            },
        ])
        .row_count(40)
        .height(200.0)
        .selected_signal(table_sel)
        .render_cell(|r, c| match c {
            0 => format!("Item {r:02}"),
            1 => if r % 2 == 0 { "Admin" } else { "User" }.into(),
            _ => format!("{}", 60 + (r % 40)),
        })
        .into();

    let tree_el: Element = tree_view("gal-tree")
        .height(180.0)
        .expanded_signal(expanded)
        .roots([TreeNode {
            id: "root".into(),
            label: "Workspace".into(),
            children: vec![
                TreeNode {
                    id: "src".into(),
                    label: "src".into(),
                    children: vec![
                        TreeNode {
                            id: "main".into(),
                            label: "main.rs".into(),
                            children: vec![],
                        },
                        TreeNode {
                            id: "lib".into(),
                            label: "lib.rs".into(),
                            children: vec![],
                        },
                    ],
                },
                TreeNode {
                    id: "docs".into(),
                    label: "Docs".into(),
                    children: vec![TreeNode {
                        id: "readme".into(),
                        label: "README.md".into(),
                        children: vec![],
                    }],
                },
            ],
        }])
        .into();

    let bars: Element = bar_chart("gal-bars")
        .values([0.2, 0.5, 0.8, 0.4, 0.9])
        .labels(["Mon", "Tue", "Wed", "Thu", "Fri"])
        .h("6rem")
        .into();
    let line: Element = line_chart("gal-line")
        .values([0.1, 0.35, 0.55, 0.4, 0.85, 0.7])
        .size(320.0, 120.0)
        .into();
    let carousel_el: Element = carousel("gal-carousel")
        .slide(|| {
            ui! {
                <Rect p="1rem" bg="muted" class="rounded-md" w="100%" h="5rem" items="center" justify="center">
                    <Label font_weight="600">{ "Slide A" }</Label>
                </Rect>
            }
        })
        .slide(|| {
            ui! {
                <Rect p="1rem" bg="secondary" class="rounded-md" w="100%" h="5rem" items="center" justify="center">
                    <Label font_weight="600">{ "Slide B" }</Label>
                </Rect>
            }
        })
        .slide(|| {
            ui! {
                <Rect p="1rem" bg="accent" class="rounded-md" w="100%" h="5rem" items="center" justify="center">
                    <Label font_weight="600">{ "Slide C" }</Label>
                </Rect>
            }
        })
        .into();

    ui! {
        <MyCard title="DataTable · TreeView · Chart · Carousel">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "数据面：表格 / 树 / 柱状与折线 / 轮播。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "DataTable" }</Label>
                { table_el }
                <Label text_size="0.875rem">
                    { format!("Selected row: {:?}", table_sel.get()) }
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "TreeView" }</Label>
                { tree_el }
                <Label font_weight="600" text_size="0.875rem">{ "BarChart" }</Label>
                { bars }
                <Label font_weight="600" text_size="0.875rem">{ "LineChart" }</Label>
                { line }
                <Label font_weight="600" text_size="0.875rem">{ "Carousel" }</Label>
                { carousel_el }
            </VStack>
        </MyCard>
    }
}

#[component]
fn MenubarStepperShowcase() -> Element {
    let step = use_signal("gal_stepper", || 1usize);
    let menu: Element = menubar("gal-menubar")
        .section(MenuSection::new(
            "file",
            "File",
            vec![
                MenuItem::new("new", "New", || {
                    toast_push("New", 1500);
                }),
                MenuItem::new("open", "Open", || {
                    toast_push("Open", 1500);
                }),
            ],
        ))
        .section(MenuSection::new(
            "edit",
            "Edit",
            vec![
                MenuItem::new("undo", "Undo", || {
                    toast_push("Undo", 1500);
                }),
                MenuItem::new("redo", "Redo", || {
                    toast_push("Redo", 1500);
                }),
            ],
        ))
        .into();
    let stepper_el: Element = stepper("gal-stepper")
        .steps(["Account", "Profile", "Confirm"])
        .current_signal(step)
        .into();

    ui! {
        <MyCard title="Menubar · Stepper">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "应用外壳导航：菜单栏与步骤条。"
                </Label>
                { menu }
                { stepper_el }
                <HStack gap="0.5rem">
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step.peek();
                        if s > 0 { step.set(s - 1); }
                    }}>{ "Prev" }</Button>
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step.peek();
                        if s < 2 { step.set(s + 1); }
                    }}>{ "Next" }</Button>
                    <Label text_size="0.875rem">{ format!("Step {}", step.get()) }</Label>
                </HStack>
            </VStack>
        </MyCard>
    }
}


fn nav_item(section: Signal<String>, id: &'static str, label: &'static str) -> Element {
    let s_click = section;
    let s_bind = section;
    let active = section.peek() == id;
    let el: Element = button(format!("gal-nav-{id}"))
        .variant(if active { "secondary" } else { "ghost" })
        .size("sm")
        .w("100%")
        .on_click(move || s_click.set(id.to_string()))
        .text(label)
        .into();
    rui::app::reactive::bind_prop(
        el,
        format!("gal-nav-{id}__variant"),
        move || {
            if s_bind.get() == id {
                "secondary".to_string()
            } else {
                "ghost".to_string()
            }
        },
        |mounted, variant| apply_bound_button_variant(mounted, variant),
    )
}

fn gallery_section_body(section: &str) -> Element {
    match section {
        "forms" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"表单"</Label>
                <InputShowcase />
                <PickerPlaceholdersShowcase />
                <TextareaComboboxShowcase />
                <ToggleControlsShowcase />
                <RadioSliderShowcase />
                <SelectPopoverTabShowcase />
            </VStack>
        },
        "overlays" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"浮层 / 反馈"</Label>
                <OverlayFeedbackShowcase />
                <PrimitivesShowcase />
            </VStack>
        },
        "data" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"数据"</Label>
                <DataChartShowcase />
            </VStack>
        },
        "navigation" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"导航"</Label>
                <MenubarStepperShowcase />
                <NavigatorShowcase />
                <SelectPopoverTabShowcase />
            </VStack>
        },
        "layout" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"布局"</Label>
                <ScrollShowcase />
                <LogicalShowcase />
                <UtilityClassShowcase />
                <GridAndDragShowcase />
                <ShadowShowcase />
            </VStack>
        },
        "text" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"文本"</Label>
                <TextCompatibilityShowcase />
                <EmojiRealtimeRegressionShowcase />
                <TypographyShowcase />
            </VStack>
        },
        "about" => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem" max_w="48rem">
                <Label text_size="1.25rem" font_weight="700">"关于"</Label>
                <Rect bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                    <VStack gap="0.5rem">
                        <Label font_weight="700">"Technical Stack"</Label>
                        <Label>"Layout: Taffy (Flexbox / Grid)"</Label>
                        <Label>"Text: Glyphon (Cosmic-text)"</Label>
                        <Label>"Rendering: WGPU"</Label>
                        <Label>"Reactivity: Signal fine-grained updates"</Label>
                        <Label>"A11y: AccessKit bridge"</Label>
                    </VStack>
                </Rect>
                <Label text_size="0.875rem" color="muted-foreground">
                    "侧栏分区对应 ENGINEERING_IMPLEMENTATION_PLAN WP-3.1 Gallery 覆盖矩阵。"
                </Label>
            </VStack>
        },
        _ => ui! {
            <VStack w="100%" gap="1rem" p="1.5rem">
                <Label text_size="1.25rem" font_weight="700">"基础"</Label>
                <Label text_size="0.875rem" color="muted-foreground">
                    "Button 变体 / 尺寸 / hover 缓动 · 排版装饰"
                </Label>
                <ButtonShowcase />
                <ButtonHoverEasingShowcase />
                <TypographyShowcase />
            </VStack>
        },
    }
}

fn build_gallery_app() -> Element {
    let section = use_signal("gal_nav_section", || "basics".to_string());
    let sec_side = section;
    let sec_body = section;

    let side = Element::from(
        sidebar("gal-sidebar")
            .w("13rem")
            .header(
                Element::from(
                    vstack()
                        .w("100%")
                        .p("0.75rem")
                        .gap("0.25rem")
                        .child(
                            Element::from(
                                label("gal-side-title")
                                    .text("RUI Gallery")
                                    .size(16.0)
                                    .font_weight(700.0),
                            ),
                        )
                        .child(
                            Element::from(
                                label("gal-side-sub")
                                    .text("组件与能力")
                                    .size(12.0)
                                    .color(parse_text_color("muted-foreground")),
                            ),
                        ),
                ),
            )
            .item(nav_item(sec_side, "basics", "基础"))
            .item(nav_item(sec_side, "forms", "表单"))
            .item(nav_item(sec_side, "overlays", "浮层 / 反馈"))
            .item(nav_item(sec_side, "data", "数据"))
            .item(nav_item(sec_side, "navigation", "导航"))
            .item(nav_item(sec_side, "layout", "布局"))
            .item(nav_item(sec_side, "text", "文本"))
            .item(nav_item(sec_side, "about", "关于"))
            .footer(
                Element::from(
                    label("gal-side-footer")
                        .text("cargo run -p gallery")
                        .size(11.0)
                        .color(parse_text_color("muted-foreground")),
                )
                .p("0.5rem"),
            ),
    );

    let body = rui::app::reactive::switch_reactive(
        "examples/gallery/App:section_body",
        move || sec_body.get(),
        move |key| rui::core::reactivity::isolate_reads(|| gallery_section_body(key.as_str())),
    );

    ui! {
        <HStack w="100%" h="100%" bg="background" gap="0" relative=true>
            { side }
            <ScrollArea id="root-scroll" h="100%" overflow_y="auto" overflow_x="auto" grow="1" scroll_behavior="smooth">
                { body }
            </ScrollArea>
            { Element::from(toast_host()) }
        </HStack>
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off",
    ))
    .init();
    let (rgba, w, h) = checker_rgba_texture();
    rui::queue_image_rgba8("demo-checker", w, h, rgba);

    rui::app::run(build_gallery_app);
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    use rui::core::reactivity::{create_effect, create_root};

    #[test]
    fn forms_section_under_isolate_reads_keeps_mark_invalid_caption() {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                // Match App: `switch_reactive` body builds under `isolate_reads`.
                let mut tree = rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                fn find_btn<'a>(
                    el: &'a rui::core::tree::Element,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some("gal-s2-mark-invalid") {
                        return Some(el);
                    }
                    el.children.iter().find_map(find_btn)
                }
                let btn = find_btn(&tree).expect("gal-s2-mark-invalid button");
                assert!(
                    btn.children
                        .iter()
                        .any(|c| c.text.as_deref() == Some("Mark invalid")),
                    "button children={:?}",
                    btn.children
                        .iter()
                        .map(|c| (c.template_id.clone(), c.text.clone(), c.text_style.size))
                        .collect::<Vec<_>>()
                );
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
                let node = result
                    .text_nodes
                    .iter()
                    .find(|n| n.text == "Mark invalid")
                    .expect("forms section must layout Mark invalid");
                assert!(
                    node.width > 40.0,
                    "caption width too small: {} (likely empty outline button)",
                    node.width
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn forms_input_showcase_mark_invalid_button_has_caption() {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = InputShowcase(InputShowcaseProps::default());
                fn find_caption<'a>(el: &'a mut rui::core::tree::Element) -> Option<&'a mut rui::core::tree::Element> {
                    if el.text.as_deref() == Some("Mark invalid") {
                        return Some(el);
                    }
                    for child in el.children.iter_mut() {
                        if let Some(found) = find_caption(child) {
                            return Some(found);
                        }
                    }
                    None
                }
                let caption = find_caption(&mut tree).expect("caption node");
                let color = caption.text_style.color;
                assert!(
                    color[3] > 0,
                    "caption alpha must be visible, got {:?}",
                    color
                );
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 600.0, [0.0, 0.0]);
                let node = result
                    .text_nodes
                    .iter()
                    .find(|n| n.text == "Mark invalid")
                    .expect("layout must collect Mark invalid");
                assert!(
                    node.width > 4.0 && node.height > 4.0,
                    "caption box must be visible, got {}x{} at ({},{})",
                    node.width,
                    node.height,
                    node.x,
                    node.y
                );
                assert!(
                    node.x >= 0.0 && node.y >= 0.0 && node.x < 900.0 && node.y < 600.0,
                    "caption must be on-screen, got ({},{})",
                    node.x,
                    node.y
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn headless_mount_gallery_app_does_not_overflow() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = build_gallery_app();
                assert!(
                    !tree.children.is_empty(),
                    "gallery root should have children"
                );
            });
        });
    }

    #[test]
    fn button_showcase_format_get_lowers_to_bind_text() {
        // `gallery_section_body` builds under `isolate_reads`; without bind_text lowering,
        // `format!(…, count.get())` would bake a static label and never update.
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = ButtonShowcase(ButtonShowcaseProps::default());
                fn find_bind(el: &rui::core::tree::Element) -> bool {
                    if el
                        .template_id
                        .as_deref()
                        .is_some_and(|id| id.contains(":bind#"))
                        && el.text.as_deref().is_some_and(|t| {
                            t.starts_with("Default (") || t.starts_with("Selected variant:")
                        })
                    {
                        return true;
                    }
                    el.children.iter().any(find_bind)
                }
                assert!(
                    find_bind(&tree),
                    "ButtonShowcase must auto-lower format!(…, signal.get()) to bind_text"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn headless_layout_basics_section_does_not_overflow() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = gallery_section_body("basics");
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
    }

    #[test]
    fn headless_layout_all_sections_does_not_overflow() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for key in [
                    "basics", "forms", "overlays", "data", "navigation", "layout", "text", "about",
                ] {
                    let mut tree = gallery_section_body(key);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
                }
            });
        });
    }
}