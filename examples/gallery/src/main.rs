#![allow(non_snake_case)]
// Release GUI builds should not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RUI Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏分区：基础 · 指针事件 · 表单 · 浮层 · 数据 · 绘制 · 导航（含 Router / 托盘 / 无边框窗） · 布局 · 文本 · 关于。
//! 运行：`cargo run -p gallery`；Inspector：`cargo run -p gallery --features devtools`（F12）；托盘原生：`cargo run -p gallery --features system-tray`

use rui::app::navigator::{navigation_host, Navigator, Presentation, PushOptions};
use rui::app::{
    close_window, open_secondary_window, open_secondary_window_with, request_exit,
    run_on_ui_thread, set_window_minimized, shared_slot, start_window_drag, system_tray_native,
    toggle_window_maximized, tray_icon, Router, TrayMenuItem, WindowOptions,
};
use rui::component;
use rui::components::*;
use rui::components::{tabs_headless, TABSTRIP_FOCUS_PREFIX};
use rui::core::reactivity::{batch, use_signal, Signal};
use rui::core::tree::{Element, LayoutStatus};
use rui::primitives::{
    canvas, circle, hstack, interactive, polyline, r#box, scene3d, svg, svg_path, vstack, Canvas2D,
    CanvasSize, Portal, PortalProps,
};
use rui::style::{parse_color, parse_text_color, TextSpan};
use rui::ui;
use rui::utils::animated_icon;
use rui::utils::lucide::lucide_glyph;
use rui::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use rui::{register_cursor_rgba8, CursorKind, MouseEvent};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

/// Stable `template_id` for fine-grained reconcile (must stay unique per mounted block).
const PRIMITIVES_SHOWCASE_REACTIVE: &str = "examples/gallery/PrimitivesShowcase:controls";
const TEXT_ALIGN_WP21_VIRT: &str = "gal-wp21-virt";
const PRIMITIVES_VIRT: &str = "gal-virt-demo";
/// WP-R.6: stable structure-island `template_id` (hot reload / reconcile slot).
/// Do not switch to `file:line:col` — that moves when the source is edited.
const GRID_DRAG_REACTIVE: &str = "examples/gallery/GridAndDragShowcase:main";
/// WP-R.6: second gallery example of an explicit reactive slot (Navigator body).
const NAVIGATOR_SHOWCASE_REACTIVE: &str = "examples/gallery/NavigatorShowcase:main";
const GAL_SHELL_ROUTER_ID: &str = "gal_shell_router";
const GAL_SHELL_BRIDGE: &str = "gal_shell_bridge_count";
#[cfg(test)]
const GAL_SHELL_TRAY_SHOW: &str = "gal_shell_tray_show";
#[cfg(test)]
const GAL_SHELL_FRAMELESS: &str = "gal_shell_frameless";
#[cfg(test)]
const GAL_SHELL_TRANSPARENT: &str = "gal_shell_transparent";
#[cfg(test)]
const GAL_SHELL_BLUR: &str = "gal_shell_blur";
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
/// `id` 为宏要求的实例键（本组件体未再写入 focus/template）。
#[component(reactive)]
fn ReactiveStatusBadge(
    id: String,
    label: rui::core::reactivity::Signal<String>,
    ok: rui::core::reactivity::Signal<bool>,
) -> rui::core::tree::Element {
    let _ = &id; // 实例键由 #[component(reactive)] 占用；徽标本身展示型可不写 id
    ui! {
        <Badge
            variant={ if ok.get() { "default" } else { "destructive" } }
        >
            <Icon
                name={ if ok.get() { "check".to_string() } else { "ban".to_string() } }
                size="0.75rem"
            />
            { label.get() }
        </Badge>
    }
}

#[component]
fn BadgeShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1c) Badge 变体（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：<Badge variant> 与 badge().variant().text()；success / warning(=warn) / error≡destructive 为 Button soft tint；Icon 用 name+animate-spin，图片请用 Image src"
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Badge>"Default"</Badge>
                    <Badge variant="secondary">"Secondary"</Badge>
                    <Badge variant="success">"Success"</Badge>
                    <Badge variant="warning">"Warning"</Badge>
                    <Badge variant="destructive">"Destructive"</Badge>
                    <Badge variant="error">"Error"</Badge>
                    <Badge variant="outline">"Outline"</Badge>
                    <Badge variant="ghost">"Ghost"</Badge>
                    <Badge variant="link">"Link"</Badge>
                </HStack>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Badge variant="secondary">
                        <Icon name="badge-check" size="0.75rem" />
                        "Verified"
                    </Badge>
                    <Badge variant="outline">
                        <Icon name="clock" size="0.75rem" />
                        "Pending"
                    </Badge>
                    <Badge variant="secondary">
                        <Icon name="loader-circle" size="0.75rem" class="animate-spin" />
                        "Loading"
                    </Badge>
                    { Element::from(badge().variant("warn").text("Builder Warn")) }
                    { Element::from(badge().variant("destructive").text("Builder Error")) }
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn KbdShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1c-k) Kbd（ShadCN keycap）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <Kbd> / <KbdGroup> and kbd() / kbd_group(); muted keycap chrome; presentational (no required id)."
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Single keys"</Label>
                    <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                        <Kbd>"⌘"</Kbd>
                        <Kbd>"⇧"</Kbd>
                        <Kbd>"⌥"</Kbd>
                        <Kbd>"⌃"</Kbd>
                        <Kbd>"Esc"</Kbd>
                        { Element::from(kbd().text("Enter")) }
                    </HStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"KbdGroup"</Label>
                    <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                        <KbdGroup>
                            <Kbd>"⌘"</Kbd>
                            <Kbd>"K"</Kbd>
                        </KbdGroup>
                        <KbdGroup>
                            <Kbd>"Ctrl"</Kbd>
                            <Kbd>"Shift"</Kbd>
                            <Kbd>"P"</Kbd>
                        </KbdGroup>
                        { Element::from(kbd_group().kbd("Ctrl").kbd("B")) }
                    </HStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"In Button / InputAdornment"</Label>
                    <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                        <Button variant="outline">
                            "Search"
                            <KbdGroup>
                                <Kbd>"⌘"</Kbd>
                                <Kbd>"K"</Kbd>
                            </KbdGroup>
                        </Button>
                        <InputAdornment>
                            <InputAdornmentInput id="gal-kbd-search" placeholder="Search…" />
                            <InputAdornmentAddon align="inline-end">
                                <Kbd>"⌘"</Kbd>
                                <Kbd>"K"</Kbd>
                            </InputAdornmentAddon>
                        </InputAdornment>
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn BubbleShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1c-b) Bubble 聊天气泡（ShadCN）">
            <VStack w="100%" min_w="0" gap="1.25rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：<Bubble variant align> + <BubbleContent> / <BubbleReactions>。默认独立行（左气泡右下 / 右气泡左下）；side / align 可覆盖；layout=\"overlay\" 咬住气泡边并内缩。超过 3 个显示圆形 +N，悬停/点击展开浮层。Emoji 默认系统色光栅 + 字形盒内居中；Twemoji 位图需 RUI_EMOJI_CUSTOM_GLYPH=1 或 RUI_TWEMOJI_DIR。"
                </Label>
                <VStack
                    w="100%"
                    min_w="0"
                    gap="0"
                    p="1rem"
                    overflow="visible"
                    bg="background"
                    border_color="border"
                    border_width="1"
                    rounded="0.75rem"
                >
                    <BubbleGroup>
                        <Bubble variant="secondary" align="start">
                            <BubbleContent>"Can you tell me what's the issue?"</BubbleContent>
                        </Bubble>
                        <Bubble variant="default" align="end">
                            <BubbleContent>"You tell me!"</BubbleContent>
                        </Bubble>
                        <Bubble variant="muted" align="start">
                            <BubbleContent>"It worked yesterday."</BubbleContent>
                            <BubbleReactions side="bottom">
                                "👍"
                                "😮"
                            </BubbleReactions>
                        </Bubble>
                    </BubbleGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label w="100%" text_size="0.75rem" font_weight="600">"Variants"</Label>
                    <BubbleGroup>
                        <Bubble variant="tinted" align="start">
                            <BubbleContent>"Tinted variant"</BubbleContent>
                        </Bubble>
                        <Bubble variant="outline" align="start">
                            <BubbleContent>"Outline"</BubbleContent>
                        </Bubble>
                        <Bubble variant="destructive" align="start">
                            <BubbleContent>"Failed to run the command."</BubbleContent>
                            <BubbleReactions side="bottom">
                                "👍"
                                "😮"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="ghost" align="start">
                            <BubbleContent>"Ghost bubbles work for assistant text without a frame."</BubbleContent>
                        </Bubble>
                    </BubbleGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label w="100%" text_size="0.75rem" font_weight="600">"Reactions（+N 悬停 / 点击浮层展开）"</Label>
                    <BubbleGroup>
                        <Bubble variant="secondary" align="end">
                            <BubbleContent>"Reactions"</BubbleContent>
                            <BubbleReactions side="bottom">
                                "👍"
                                "😮"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="secondary" align="end">
                            <BubbleContent>"Party"</BubbleContent>
                            <BubbleReactions side="bottom">
                                "🎉"
                                "👏"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="secondary" align="end">
                            <BubbleContent>"row · 8 reactions → +5"</BubbleContent>
                            <BubbleReactions side="bottom">
                                "👍"
                                "🔥"
                                "👀"
                                "🎉"
                                "👏"
                                "❤️"
                                "🚀"
                                "✨"
                            </BubbleReactions>
                        </Bubble>
                        { Element::from(
                            bubble()
                                .variant("secondary")
                                .align("end")
                                .child(bubble_content().text("Builder bubble"))
                                .child(
                                    bubble_reactions()
                                        .side("bottom")
                                        .emoji("👍")
                                        .emoji("🔥"),
                                ),
                        ) }
                    </BubbleGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label w="100%" text_size="0.75rem" font_weight="600">"Placement（覆盖位置 / 独立行 vs 叠加）"</Label>
                    <BubbleGroup>
                        <Bubble variant="muted" align="start">
                            <BubbleContent>"row · 居中独立行"</BubbleContent>
                            <BubbleReactions side="bottom" align="center">
                                "👍"
                                "🔥"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="secondary" align="start">
                            <BubbleContent>"overlay · 左气泡默认右下"</BubbleContent>
                            <BubbleReactions layout="overlay">
                                "😮"
                                "🎉"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="secondary" align="end">
                            <BubbleContent>"overlay · 右气泡默认左下"</BubbleContent>
                            <BubbleReactions layout="overlay">
                                "👍"
                                "🔥"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="outline" align="start">
                            <BubbleContent>"overlay · 顶左（覆盖）"</BubbleContent>
                            <BubbleReactions side="top" align="start" layout="overlay">
                                "👀"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="secondary" align="start">
                            <BubbleContent>"overlay · 7 reactions → +4"</BubbleContent>
                            <BubbleReactions layout="overlay">
                                "👍"
                                "🔥"
                                "👀"
                                "🎉"
                                "👏"
                                "❤️"
                                "🚀"
                            </BubbleReactions>
                        </Bubble>
                        <Bubble variant="outline" align="end">
                            <BubbleContent>"overlay 顶 · 6 reactions → +3"</BubbleContent>
                            <BubbleReactions side="top" layout="overlay">
                                "👍"
                                "😮"
                                "🎉"
                                "🔥"
                                "👀"
                                "👏"
                            </BubbleReactions>
                        </Bubble>
                    </BubbleGroup>
                </VStack>
            </VStack>
        </MyCard>
    }
}

fn AvatarShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1d) Avatar（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：shape=circle|rounded-square；Fallback 支持文本/Icon/遮罩图片；avatar().shape().fallback*()"
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Basic"</Label>
                        <Avatar>
                            <AvatarImage
                                src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%233b82f6'/></svg>"
                                alt="@demo"
                            />
                            <AvatarFallback>"CN"</AvatarFallback>
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Fallback Icon"</Label>
                        <Avatar>
                            <AvatarFallback>
                                <Icon name="user" />
                            </AvatarFallback>
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Fallback Image"</Label>
                        { Element::from(avatar().fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23f97316'/></svg>")) }
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Rounded sq."</Label>
                        <Avatar shape="rounded-square">
                            <AvatarFallback>"RS"</AvatarFallback>
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Badge"</Label>
                        <Avatar>
                            <AvatarImage
                                src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%2310b981'/></svg>"
                            />
                            <AvatarFallback>"PP"</AvatarFallback>
                            <AvatarBadge variant="success" />
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Badge + Icon"</Label>
                        <Avatar size="lg">
                            <AvatarFallback>"JD"</AvatarFallback>
                            <AvatarBadge variant="destructive">
                                <Icon name="badge-check" size="0.5rem" />
                            </AvatarBadge>
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Load fail"</Label>
                        <Avatar>
                            <AvatarImage src="/nonexistent/rui-gallery-avatar.png" />
                            <AvatarFallback>"404"</AvatarFallback>
                        </Avatar>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Sizes"</Label>
                        <HStack gap="0.5rem" items="center">
                            <Avatar size="sm"><AvatarFallback>"SM"</AvatarFallback></Avatar>
                            <Avatar><AvatarFallback>"MD"</AvatarFallback></Avatar>
                            <Avatar size="lg"><AvatarFallback>"LG"</AvatarFallback></Avatar>
                        </HStack>
                    </VStack>
                </HStack>
                <VStack gap="0.25rem">
                    <Label font_weight="700" text_size="0.75rem">"AvatarGroup + Count"</Label>
                    <Label w="100%" text_size="0.7rem" color="muted-foreground" leading="normal">
                        "悬停 / 点击 +N：原地向下盖住展开（expand_placement / expand_class 可配）；Count 内可嵌隐藏头像参与展开列表"
                    </Label>
                    <AvatarGroup>
                        <Avatar>
                            <AvatarImage src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23ef4444'/></svg>" />
                            <AvatarFallback>"A"</AvatarFallback>
                        </Avatar>
                        <Avatar>
                            <AvatarImage src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%2322c55e'/></svg>" />
                            <AvatarFallback>"B"</AvatarFallback>
                        </Avatar>
                        <Avatar><AvatarFallback>"C"</AvatarFallback></Avatar>
                        <AvatarGroupCount>
                            "+3"
                            <Avatar><AvatarFallback>"D"</AvatarFallback></Avatar>
                            <Avatar><AvatarFallback>"E"</AvatarFallback></Avatar>
                            <Avatar><AvatarFallback>"F"</AvatarFallback></Avatar>
                        </AvatarGroupCount>
                    </AvatarGroup>
                </VStack>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <VStack gap="0.25rem" items="center">
                        <Label font_weight="700" text_size="0.75rem">"Fallback"</Label>
                        <Avatar>
                            <AvatarFallback>"ER"</AvatarFallback>
                        </Avatar>
                    </VStack>
                    { Element::from(avatar().fallback("BLD")) }
                    { Element::from(avatar().fallback_icon("user")) }
                    { Element::from(
                        avatar()
                            .shape("rounded-square")
                            .fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23f97316'/></svg>")
                    ) }
                    { Element::from(
                        avatar()
                            .size("lg")
                            .image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23a855f7'/></svg>")
                            .fallback("BLD")
                    ) }
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn BreadcrumbShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1e) Breadcrumb（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：RSX 复合标签与 breadcrumb().link().page()；Separator / Ellipsis 默认 Lucide 图标"
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX"</Label>
                    <Breadcrumb>
                        <BreadcrumbList>
                            <BreadcrumbItem>
                                <BreadcrumbLink on_click={|| {}}>"Home"</BreadcrumbLink>
                            </BreadcrumbItem>
                            <BreadcrumbSeparator />
                            <BreadcrumbItem>
                                <BreadcrumbLink on_click={|| {}}>"Components"</BreadcrumbLink>
                            </BreadcrumbItem>
                            <BreadcrumbSeparator />
                            <BreadcrumbItem>
                                <BreadcrumbPage>"Breadcrumb"</BreadcrumbPage>
                            </BreadcrumbItem>
                        </BreadcrumbList>
                    </Breadcrumb>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Builder"</Label>
                    { Element::from(
                        breadcrumb()
                            .link("Home", || {})
                            .link("Components", || {})
                            .page("Breadcrumb")
                    ) }
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Ellipsis"</Label>
                    { Element::from(
                        breadcrumb()
                            .link("Home", || {})
                            .ellipsis()
                            .link("Settings", || {})
                            .page("Profile")
                    ) }
                </VStack>
            </VStack>
        </MyCard>
    }
}

fn ButtonGroupShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1e-b) ButtonGroup（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：RSX 复合标签与 button_group().button()；相邻圆角合并 · 绘制期叠边 · Separator / Text"
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · horizontal"</Label>
                    <ButtonGroup>
                        <Button variant="outline">"Copy"</Button>
                        <Button variant="outline">"Paste"</Button>
                        <Button variant="outline">"Cut"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · Text + Separator"</Label>
                    <ButtonGroup>
                        <ButtonGroupText>"Archive"</ButtonGroupText>
                        <Button variant="outline">"Open"</Button>
                        <ButtonGroupSeparator />
                        <Button variant="outline">"Delete"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · vertical"</Label>
                    <ButtonGroup orientation="vertical">
                        <Button variant="outline">"Top"</Button>
                        <Button variant="outline">"Middle"</Button>
                        <Button variant="outline">"Bottom"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Builder"</Label>
                    { Element::from(
                        button_group()
                            .button("Copy", || {})
                            .button("Paste", || {})
                            .separator()
                            .button("Cut", || {})
                    ) }
                </VStack>
            </VStack>
        </MyCard>
    }
}

fn AspectRatioShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="1f) AspectRatio（CSS aspect-ratio）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：<AspectRatio ratio> 与 aspect_ratio().ratio()；Image/Box 亦可写 aspect_ratio=\"16/9\"（MDN：定宽 + 另一轴 auto）"
                </Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="start" wrap="wrap">
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"16:9 容器"</Label>
                        <AspectRatio ratio={16.0 / 9.0} w="10rem" class="rounded-md" bg="muted">
                            <Image texture="demo-checker" w="100%" h="100%" />
                        </AspectRatio>
                    </VStack>
                    <VStack gap="0.25rem" w="8rem">
                        <Label font_weight="700" text_size="0.75rem">"1:1 容器"</Label>
                        <AspectRatio ratio="1/1" w="8rem" class="rounded-md" bg="muted">
                            <Image
                                src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%233b82f6'/><circle cx='32' cy='32' r='18' fill='white'/></svg>"
                                w="100%"
                                h="100%"
                            />
                        </AspectRatio>
                    </VStack>
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"Image 属性"</Label>
                        <Image
                            texture="demo-checker"
                            w="10rem"
                            aspect_ratio="4/3"
                            rounded="0.375rem"
                        />
                    </VStack>
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"Builder 21:9"</Label>
                        { Element::from(
                            aspect_ratio()
                                .ratio("21/9")
                                .child(
                                    Element::new()
                                        .w("100%")
                                        .h("100%")
                                        .bg(rui::core::tree::parse_color("primary")),
                                ),
                        )
                        .w("10rem")
                        .rounded(6.0) }
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn AnimationShowcase() -> rui::core::tree::Element {
    let play = use_signal("gal_anim_play", || true);
    let play_btn = play;
    let play_box = play;
    ui! {
        <MyCard title="1g) CSS animation（MDN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utility：animate-spin / ping / pulse / bounce；属性：animation / animation_play_state / fill-mode；Icon+spin 走字形贴图+transform（非 glyphon）；fade 用 infinite alternate"
                </Label>
                <HStack w="100%" min_w="0" gap="1.25rem" items="start" wrap="wrap" py="0.75rem" overflow="visible">
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"spin"</Label>
                        <Icon name="loader-circle" size="2rem" color="foreground" class="animate-spin" />
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"ping"</Label>
                        <Box w="2rem" h="2rem" relative={true} items="center" justify="center">
                            <Box w="2rem" h="2rem" class="rounded-full" bg="primary" />
                            <Box
                                absolute={true}
                                inset="0"
                                class="rounded-full animate-ping"
                                bg="primary"
                            />
                        </Box>
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"pulse"</Label>
                        <Box w="3rem" h="2rem" bg="secondary" class="rounded-md animate-pulse" />
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"bounce"</Label>
                        <Box w="2rem" h="2rem" bg="accent" class="rounded-md animate-bounce" />
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"alternate pulse"</Label>
                        <Box
                            w="3rem"
                            h="2rem"
                            bg="destructive"
                            class="rounded-md"
                            animation="1s ease-in-out infinite alternate pulse"
                        />
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"play-state"</Label>
                        <Box
                            w="2.5rem"
                            h="2.5rem"
                            bg="primary"
                            class="rounded-md"
                            animation="1s linear infinite spin"
                            animation_play_state={ if play_box.get() { "running" } else { "paused" } }
                        />
                        <Button size="sm" variant="outline" on_click={move || play_btn.set(!play_btn.get())}>
                            { if play.get() { "Pause" } else { "Play" } }
                        </Button>
                    </VStack>
                    <VStack gap="0.35rem" items="center" shrink={0.0}>
                        <Label font_weight="700" text_size="0.75rem">"fade alternate"</Label>
                        <Box
                            w="3rem"
                            h="2rem"
                            bg="muted"
                            class="rounded-md"
                            animation="1.2s ease-in-out infinite alternate fade-in"
                        />
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// WP-R.7：`format!(…, signal.get())` 走自动 bind_text；`#[component(reactive)]` 体不因 count 重跑。
#[component(reactive)]
fn ReactiveCountChip(
    id: String,
    count: rui::core::reactivity::Signal<i32>,
) -> rui::core::tree::Element {
    ui! {
        <HStack gap="0.5rem" items="center" p="0.5rem" class="rounded" bg="muted">
            <Label text_size="0.875rem">{ format!("count {}", count.get()) }</Label>
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
                    <Button variant="success" on_click={move || choose_variant(selected, "success")}>
                        "Success"
                    </Button>
                    <Button variant="warning" on_click={move || choose_variant(selected, "warning")}>
                        "Warning"
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
                    <Button variant="ghost" hover_easing="spring">"Ghost · spring"</Button>
                    <Button variant="ghost" hover_easing="cubic">"Ghost · cubic"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// Fixed rows for the MouseEvent readout. Variable wrap used to grow the card
/// on every `mousemove` (digit width / `relatedTarget` Debug) and collapse the page.
const MOUSE_LOG_LINES: usize = 7;
/// 7×12px×leading-normal(1.5) + 0.75rem×2 padding; keep in sync with [`MOUSE_LOG_LINES`].
const MOUSE_LOG_PANEL_H: f32 = 152.0;

fn clip_mouse_debug(s: Option<&str>) -> String {
    let t = s.unwrap_or("-");
    let n = t.chars().count();
    if n <= 18 {
        format!("{t:<18}")
    } else {
        let head: String = t.chars().take(17).collect();
        format!("{head}…")
    }
}

fn format_mouse_log_fields(
    event_type: &str,
    detail: u8,
    button: i16,
    buttons: u16,
    which: i16,
    client: [f32; 2],
    offset: [f32; 2],
    page: [f32; 2],
    screen: [f32; 2],
    movement: [f32; 2],
    alt: bool,
    ctrl: bool,
    shift: bool,
    meta: bool,
    accel: bool,
    region: Option<&str>,
    related: Option<&str>,
    phase: &str,
    stop: bool,
    prevent: bool,
) -> String {
    format!(
        "{:<10} detail={:<2} button={} buttons={:<3} which={}\n\
client=({:>7.1},{:>7.1})  offset=({:>7.1},{:>7.1})\n\
page=({:>7.1},{:>7.1})    screen=({:>7.1},{:>7.1})\n\
movement=({:>7.1},{:>7.1})\n\
Alt={:<5} Ctrl={:<5} Shift={:<5} Meta={:<5} Accel={}\n\
region={}  related={}\n\
phase={:<8}  stop={}  prevent={}",
        event_type,
        detail,
        button,
        buttons,
        which,
        client[0],
        client[1],
        offset[0],
        offset[1],
        page[0],
        page[1],
        screen[0],
        screen[1],
        movement[0],
        movement[1],
        alt,
        ctrl,
        shift,
        meta,
        accel,
        clip_mouse_debug(region),
        clip_mouse_debug(related),
        phase,
        stop,
        prevent,
    )
}

fn format_mouse_log(e: &MouseEvent) -> String {
    format_mouse_log_fields(
        e.event_type,
        e.detail,
        e.button_index(),
        e.buttons,
        e.which(),
        [e.client_x(), e.client_y()],
        [e.offset_x(), e.offset_y()],
        [e.page_x, e.page_y],
        [e.screen_x, e.screen_y],
        [e.movement_x, e.movement_y],
        e.alt_key,
        e.ctrl_key,
        e.shift_key,
        e.meta_key,
        e.get_modifier_state("Accel"),
        e.region.as_deref(),
        e.related_target.as_deref(),
        match e.phase {
            rui::EventPhase::Capture => "Capture",
            rui::EventPhase::Target => "Target",
            rui::EventPhase::Bubble => "Bubble",
        },
        e.is_consumed(),
        e.default_prevented(),
    )
}

fn mouse_log_placeholder() -> String {
    format_mouse_log_fields(
        "idle",
        0,
        0,
        0,
        1,
        [0.0, 0.0],
        [0.0, 0.0],
        [0.0, 0.0],
        [0.0, 0.0],
        [0.0, 0.0],
        false,
        false,
        false,
        false,
        false,
        None,
        None,
        "Target",
        false,
        false,
    )
}

#[component]
fn MouseEventShowcase() -> Element {
    let log = use_signal("gal_mouse_log", mouse_log_placeholder);
    let clicks = use_signal("gal_mouse_clicks", || 0i32);
    let dbls = use_signal("gal_mouse_dbls", || 0i32);
    let overs = use_signal("gal_mouse_overs", || 0i32);
    let lca = use_signal("gal_mouse_lca_clicks", || 0i32);

    let write = {
        let log = log;
        move |e: &MouseEvent| log.set(format_mouse_log(e))
    };

    let playground: Element = {
        let w_md = write.clone();
        let w_mu = write.clone();
        let w_mm = write.clone();
        let w_ov = write.clone();
        let w_ot = write.clone();
        let w_db = write.clone();
        let clicks = clicks;
        let dbls = dbls;
        let overs = overs;
        interactive(
            r#box()
                .w("100%")
                .h(160.0)
                .p("0.75rem")
                .rounded(8.0)
                .bg(parse_color("muted"))
                .items("center")
                .justify("center")
                .child(
                    Element::from(
                        label("gal-mouse-pad-hint")
                            .text("MouseEvent 命中区（mousedown / up / move / over / out / click / dblclick）")
                            .size(12.0)
                            .color(parse_text_color("muted-foreground")),
                    ),
                )
                .into(),
        )
        .focus_id("gal-mouse-pad")
        .on_mousedown(move |e| w_md(e))
        .on_mouseup(move |e| w_mu(e))
        .on_mousemove(move |e| w_mm(e))
        .on_mouseover(move |e| {
            overs.set(overs.peek() + 1);
            w_ov(e);
        })
        .on_mouseout(move |e| w_ot(e))
        .on_dblclick(move |e| {
            dbls.set(dbls.peek() + 1);
            w_db(e);
        })
        .on_click(move || increment_count(clicks))
        .into()
    };

    let lca_inner: Element = interactive(
        r#box()
            .p("0.75rem")
            .rounded(8.0)
            .bg(parse_color("secondary"))
            .child(Element::from(
                label("gal-mouse-lca-inner")
                    .text("目标：必须在此按下并在此松开")
                    .size(13.0),
            ))
            .into(),
    )
    .focus_id("gal-mouse-lca-target")
    .on_click(move || increment_count(lca))
    .into();

    let lca_outer: Element = r#box()
        .w("100%")
        .min_h(120.0)
        .p("1.25rem")
        .rounded(8.0)
        .bg(parse_color("muted"))
        .items("center")
        .justify("center")
        .child(lca_inner)
        .into();

    let swallow: Element = {
        let w = write.clone();
        interactive(
            r#box()
                .p("0.75rem")
                .rounded(8.0)
                .bg(parse_color("secondary"))
                .child(Element::from(
                    label("gal-mouse-swallow")
                        .text("mousedown stopPropagation（父级不应再收到 down）")
                        .size(12.0),
                ))
                .into(),
        )
        .focus_id("gal-mouse-swallow")
        .on_mousedown(move |e| {
            e.stop_propagation();
            w(e);
        })
        .into()
    };

    ui! {
        <MyCard title="MouseEvent（pointer_events）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对齐 DOM MouseEvent：click / dblclick / mousedown / mouseup / mousemove / mouseover / mouseout。click 落在 mousedown 与 mouseup 的最近公共祖先；在灰色区外按下再拖进目标松开，目标 on_click 不应 +1。"
                </Label>
                { playground }
                <Label w="100%" text_size="0.875rem" leading="normal">
                    { format!("click={}  dblclick={}  mouseover={}", clicks.get(), dbls.get(), overs.get()) }
                </Label>
                <Box
                    id="gal-mouse-log-panel"
                    w="100%"
                    min_w="0"
                    h={MOUSE_LOG_PANEL_H}
                    min_h={MOUSE_LOG_PANEL_H}
                    max_h={MOUSE_LOG_PANEL_H}
                    flex_shrink="0"
                    overflow="hidden"
                    p="0.75rem"
                    bg="secondary"
                    rounded="0.5rem"
                >
                    <Label id="gal-mouse-log" w="100%" min_w="0" text_size="0.75rem" leading="normal" color="muted-foreground">
                        { log.get() }
                    </Label>
                </Box>
                <Label w="100%" font_weight="700" text_size="0.875rem">"click LCA"</Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!("目标 click={} — 在灰色空白处按下，拖进蓝色块再松开，计数不应增加", lca.get()) }
                </Label>
                { lca_outer }
                <Label w="100%" font_weight="700" text_size="0.875rem">"stopPropagation"</Label>
                { swallow }
            </VStack>
        </MyCard>
    }
}

fn ensure_gallery_custom_cursors() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let size = 32u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        let c = 16.0f32;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                let r = (dx * dx + dy * dy).sqrt();
                let i = ((y * size + x) * 4) as usize;
                let on_ring = (r - 10.0).abs() < 2.0;
                let on_dot = r < 2.5;
                if on_ring || on_dot {
                    rgba[i] = 239;
                    rgba[i + 1] = 68;
                    rgba[i + 2] = 68;
                    rgba[i + 3] = 255;
                }
            }
        }
        register_cursor_rgba8("gallery-ring", size, size, rgba);
    });
}

#[component]
fn CursorShowcase() -> Element {
    ensure_gallery_custom_cursors();
    let mut grid = hstack().w("100%").gap("0.5rem").wrap("wrap");
    for kw in CursorKind::ALL_KEYWORDS {
        grid = grid.child(
            r#box()
                .p("0.5rem")
                .rounded(6.0)
                .bg(parse_color("muted"))
                .cursor(*kw)
                .child(Element::from(
                    label(format!("gal-cur-{kw}"))
                        .text(*kw)
                        .size(11.0)
                        .color(parse_text_color("foreground")),
                )),
        );
    }
    let grid: Element = grid.into();

    ui! {
        <MyCard title="CSS cursor">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对齐 MDN cursor 预定义关键字。自定义光标与 Image 共用解码：PNG/ICO/BMP 默认；JPEG/GIF/WebP 需 images；SVG 需 svg。多 url() 回退；光标位图超 128px 跳过，SVG 缩到上限内。"
                </Label>
                <HStack gap="0.5rem" wrap="wrap" items="center">
                    <Button>"默认 pointer"</Button>
                    <Button disabled=true>"disabled not-allowed"</Button>
                    <Button cursor="wait">"cursor=wait"</Button>
                    <Button class="cursor-help">"class cursor-help"</Button>
                    <Box p="0.5rem" rounded="0.375rem" bg="muted" cursor="url(#missing), url(#gallery-ring) 16 16, pointer">
                        <Label text_size="0.75rem">"url(#gallery-ring) 16 16"</Label>
                    </Box>
                </HStack>
                { grid }
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputShowcase() -> rui::core::tree::Element {
    let mark_invalid = use_signal("gal_input_mark_invalid", || false);
    let badge_ok = use_signal("gal_status_badge_ok", || true);
    let badge_label = use_signal("gal_status_badge_label", || "Ready".to_string());
    let chip_count = use_signal("gal_status_chip_count", || 0_i32);

    ui! {
        <MyCard title="2) Input (ShadCN-style)">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "text / password / number / email / tel / url / search — type 过滤与非空校验；clearable 叉号清除；password_toggle 睁眼/闭眼"
                </Label>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                <Input
                        id="gal-s2-input-a"
                        placeholder="Placeholder…"
                        aria_label="Gallery demo name"
                        clearable="true"
                        flex_grow="1"
                    />
                    <Input placeholder="Invalid" invalid=true clearable="true" flex_grow="1" />
                    <Input id="gal-s2-input-pw" type="password" password_toggle="true" clearable="true" flex_grow="1" />
                    <Input type="number" number_steppers=true clearable=true flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <Input type="email" clearable=true flex_grow="1" />
                    <Input type="tel" clearable=true flex_grow="1" />
                    <Input type="url" clearable=true flex_grow="1" />
                    <Input type="search" clearable="true" flex_grow="1" />
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    "Input / Combobox 默认 text-overflow:hidden（原生 input：裁切，光标或选区可滚出）。clip / ellipsis / newline 用于 Label、Button 等不可编辑文本。"
                </Label>
                <HStack gap="0.5rem" items="center" w="10rem">
                    <Input
                        id="gal-s2-input-hidden"
                        w="100%"
                        placeholder="hidden：超长裁切，→ 或向后选中可滚出"
                    />
                </HStack>
                <HStack gap="0.5rem" items="center" w="10rem">
                    <Button truncate=true w="100%" variant="outline" size="sm">
                        "Very long button caption that must not wrap"
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="start" w="10rem">
                    <VStack gap="0.25rem" w="100%">
                        <Label text_overflow="clip" w="100%">
                            "clip：超长单行会被裁切不会出现省略号"
                        </Label>
                        <Label text_overflow="ellipsis" w="100%">
                            "ellipsis：超长单行末尾显示省略号"
                        </Label>
                        <Label text_overflow="newline" w="100%">
                            "newline：允许软换行到下一行而不是裁切或省略"
                        </Label>
                    </VStack>
                </HStack>
                <HStack gap="0.5rem" items="center" wrap="wrap" w="100%">
                    <Input
                        placeholder="invalid={signal}"
                        invalid={mark_invalid.get()}
                        flex_grow="1"
                    />
                    <Button
                        id="gal-s2-mark-invalid"
                        variant="outline"
                        size="sm"
                        on_click={move || {
                            // Solid `batch`: one flush for caption bind_text + Input invalid bind_attr.
                            batch(|| {
                                mark_invalid.set(!mark_invalid.peek());
                            });
                        }}
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
                    <ReactiveCountChip id="gal-count-chip" count={chip_count} />
                    <Button
                        variant="ghost"
                        size="sm"
                        on_click={move || chip_count.set(chip_count.peek() + 1)}
                    >
                        "Bump count"
                    </Button>
                    <Show when={badge_ok.get()}>
                        <Label text_size="0.75rem" color="muted-foreground">"online"</Label>
                    </Show>
                </HStack>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "ReactiveStatusBadge：底层 <Badge variant={…}> + bind 文案；完整变体矩阵见「基础 → Badge」。"
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputAdornmentShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="2b) InputAdornment（对位 ShadCN InputGroup）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "单字段外壳 + addon（≠ 多个 Input 成组）。双写法：标签 / input_adornment().addon().child()"
                </Label>
                <InputAdornment>
                    <InputAdornmentAddon align="inline-start">
                        <InputAdornmentText>
                            <Icon name="search" size="0.875rem" color="muted-foreground" />
                        </InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentInput id="gal-ia-search" placeholder="Search…" />
                    <InputAdornmentAddon align="inline-end">
                        <InputAdornmentText>"12 results"</InputAdornmentText>
                    </InputAdornmentAddon>
                </InputAdornment>
                <InputAdornment>
                    <InputAdornmentAddon align="inline-start">
                        <InputAdornmentText>"https://"</InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentInput id="gal-ia-url" placeholder="example.com" />
                    <InputAdornmentAddon align="inline-end">
                        <InputAdornmentButton size="icon-xs" variant="ghost">
                            <Icon name="info" size="0.75rem" />
                        </InputAdornmentButton>
                    </InputAdornmentAddon>
                </InputAdornment>
                <InputAdornment>
                    <InputAdornmentAddon align="block-start">
                        <InputAdornmentText>"Message"</InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentTextarea id="gal-ia-ta" placeholder="Ask, Search or Chat…" rows={3} />
                    <InputAdornmentAddon align="block-end">
                        <InputAdornmentButton size="xs" variant="default">"Send"</InputAdornmentButton>
                    </InputAdornmentAddon>
                </InputAdornment>
                {
                    Element::from(
                        input_adornment()
                            .addon(
                                "inline-start",
                                InputAdornmentText(InputAdornmentTextProps {
                                    class: None,
                                    children: Some(Element::new().text("$")),
                                    base: None,
                                    ..Default::default()
                                }),
                            )
                            .child(
                                input("gal-ia-builder")
                                    .placeholder("0.00")
                                    .input_type("number")
                                    .plain_chrome(true),
                            )
                            .addon(
                                "inline-end",
                                InputAdornmentText(InputAdornmentTextProps {
                                    class: None,
                                    children: Some(Element::new().text("USD")),
                                    base: None,
                                    ..Default::default()
                                }),
                            ),
                    )
                }
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputOTPShowcase() -> rui::core::tree::Element {
    let code = use_signal("gal_otp_code", || String::new());
    ui! {
        <MyCard title="2c) InputOTP（对位 ShadCN）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "独立复合组件（≠ Input type）。连体分段条（border-y/r · first/last 圆角）；活动槽 ring；Builder separator_at 分段。"
                </Label>
                <Label font_weight="700" text_size="0.75rem">"RSX · 6 位分段"</Label>
                <InputOTP id="gal-otp-rsx" maxLength={6}>
                    <InputOTPGroup>
                        <InputOTPSlot index={0} />
                        <InputOTPSlot index={1} />
                        <InputOTPSlot index={2} />
                    </InputOTPGroup>
                    <InputOTPSeparator />
                    <InputOTPGroup>
                        <InputOTPSlot index={3} />
                        <InputOTPSlot index={4} />
                        <InputOTPSlot index={5} />
                    </InputOTPGroup>
                </InputOTP>
                <Label font_weight="700" text_size="0.75rem">"Builder · 受控 value"</Label>
                {
                    Element::from(
                        input_otp("gal-otp-b")
                            .max_length(4)
                            .value_signal(code)
                            .separator_at(2),
                    )
                }
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("value = {:?}", code.get()) }
                </Label>
            </VStack>
        </MyCard>
    }
}

fn wp39_form_demo() -> Element {
    form_provider("gal-wp39", || {
        let st = use_form().expect("form_provider");
        let st_submit = st.clone();
        let st_clear = st.clone();
        let field = form_field(
            "email",
            "邮箱",
            Element::from(input("gal-wp39-email").placeholder("you@example.com")),
        );
        let submit: Element = button("gal-wp39-submit")
            .text("提交")
            .on_click(move || {
                let _ = st_submit.validate(|| {
                    let key = input_value_signal_key("gal-wp39-email");
                    let v = use_signal(&key, String::new).peek();
                    if v.trim().is_empty() {
                        st_submit.set_field_error("email", "必填");
                    } else if !v.contains('@') {
                        st_submit.set_field_error("email", "需包含 @");
                    }
                });
            })
            .into();
        let clear: Element = button("gal-wp39-clear")
            .variant("outline")
            .text("清除错误")
            .on_click(move || st_clear.clear_all())
            .into();
        vstack()
            .w("100%")
            .gap("0.5rem")
            .child(field)
            .child(
                hstack()
                    .gap("0.5rem")
                    .wrap("wrap")
                    .items("center")
                    .child(submit)
                    .child(clear),
            )
            .into()
    })
}

#[component]
fn FormValidateShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="Form 校验（WP-3.9）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "form_provider + form_field：错误文案与 Input invalid 环同源。空提交 →「必填」+ 红环；清除错误 → 文案与环一起消失。"
                </Label>
                { wp39_form_demo() }
            </VStack>
        </MyCard>
    }
}

#[component]
fn PickerPlaceholdersShowcase() -> rui::core::tree::Element {
    ui! {
        <VStack w="100%" gap="1rem">
            <MyCard title="2b) Pickers (file · color · date/time)">
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                        "WP-3.3：触发器行 + Date/Time Tabs（芯片按内容宽并折行；仅挂载当前项，避免 forms 滚入时一次冷启动整排 picker）。"
                    </Label>
                    <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                        <Button
                            variant="outline"
                            size="sm"
                            flex_grow="1"
                            on_click={move || {
                                let _ = rui::utils::pick_files_into("gal-file", "image/*", false);
                            }}
                        >
                            "Choose image…"
                        </Button>
                        <Button
                            variant="outline"
                            size="sm"
                            flex_grow="1"
                            on_click={move || {
                                let _ = rui::utils::pick_folder_into("gal-dir");
                            }}
                        >
                            "Choose folder…"
                        </Button>
                        <ColorPicker id="gal-color" value="#3b82f6" flex_grow="1" />
                    </HStack>
                    <Tabs id="gal-dp-kinds" default_value="date">
                        <TabsList>
                            <TabsTrigger value="date">"Date"</TabsTrigger>
                            <TabsTrigger value="datetime">"DateTime"</TabsTrigger>
                            <TabsTrigger value="time">"Time"</TabsTrigger>
                            <TabsTrigger value="month">"Month"</TabsTrigger>
                            <TabsTrigger value="week">"Week"</TabsTrigger>
                        </TabsList>
                        <TabsContent value="date">
                            <DatePicker id="gal-date" kind="date" captionLayout="dropdown" />
                        </TabsContent>
                        <TabsContent value="datetime">
                            <DatePicker id="gal-dt" kind="datetime-local" />
                        </TabsContent>
                        <TabsContent value="time">
                            <TimePicker id="gal-time" />
                        </TabsContent>
                        <TabsContent value="month">
                            <DatePicker id="gal-month" kind="month" />
                        </TabsContent>
                        <TabsContent value="week">
                            <DatePicker id="gal-week" kind="week" />
                        </TabsContent>
                    </Tabs>
                </VStack>
            </MyCard>
            <MyCard title="2c) Inline Calendar · ColorPicker">
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <Label w="100%" text_size="0.75rem" color="muted-foreground">
                        "独立卡片：滚入 2b 上半时不连带着 Calendar / 内联 HSV。"
                    </Label>
                    <HStack gap="1rem" items="start" wrap="wrap" w="100%">
                        <Calendar id="gal-cal" captionLayout="dropdown" />
                        <ColorPicker id="gal-color-inline" variant="inline" format="hsla" />
                    </HStack>
                    <Label w="100%" text_size="0.75rem" color="muted-foreground">
                        "ColorPicker 变体（TabsList 折行保完整标签；仅挂载当前项）"
                    </Label>
                    <Tabs id="gal-cp-variants" default_value="sketch">
                        <TabsList>
                            <TabsTrigger value="sketch">"Sketch"</TabsTrigger>
                            <TabsTrigger value="chrome">"Chrome"</TabsTrigger>
                            <TabsTrigger value="github">"GitHub"</TabsTrigger>
                            <TabsTrigger value="twitter">"Twitter"</TabsTrigger>
                            <TabsTrigger value="material">"Material"</TabsTrigger>
                            <TabsTrigger value="block">"Block"</TabsTrigger>
                            <TabsTrigger value="circle">"Circle"</TabsTrigger>
                            <TabsTrigger value="slider">"Slider"</TabsTrigger>
                            <TabsTrigger value="compact">"Compact"</TabsTrigger>
                        </TabsList>
                        <TabsContent value="sketch">
                            <ColorPicker id="gal-cp-sketch" variant="sketch" value="#f47373" />
                        </TabsContent>
                        <TabsContent value="chrome">
                            <ColorPicker id="gal-cp-chrome" variant="chrome" value="#0693E3" />
                        </TabsContent>
                        <TabsContent value="github">
                            <ColorPicker id="gal-cp-github" variant="github" value="#1273DE" />
                        </TabsContent>
                        <TabsContent value="twitter">
                            <ColorPicker id="gal-cp-twitter" variant="twitter" value="#FF6900" />
                        </TabsContent>
                        <TabsContent value="material">
                            <ColorPicker id="gal-cp-material" variant="material" value="#9C27B0" />
                        </TabsContent>
                        <TabsContent value="block">
                            <ColorPicker id="gal-cp-block" variant="block" value="#37D67A" />
                        </TabsContent>
                        <TabsContent value="circle">
                            <ColorPicker id="gal-cp-circle" variant="circle" value="#BA68C8" />
                        </TabsContent>
                        <TabsContent value="slider">
                            <ColorPicker id="gal-cp-slider" variant="slider" value="#2CCCE4" />
                        </TabsContent>
                        <TabsContent value="compact">
                            <ColorPicker id="gal-cp-compact" variant="compact" value="#3b82f6" />
                        </TabsContent>
                    </Tabs>
                </VStack>
            </MyCard>
        </VStack>
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
                        <Label text_decoration_line="underline" text_decoration_thickness="1px">"Thin"</Label>
                        <Label text_decoration_line="underline" text_decoration_thickness="2px">"Medium"</Label>
                        <Label text_decoration_line="underline" text_decoration_thickness="3px" text_decoration_style="dashed">"Thick Dashed"</Label>
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
                        <Box w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="0.25rem 0.25rem 0.75rem 0 black" />
                        <Label text_size="0.6rem" color="muted-foreground">"blur + black"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625"
                            box_shadow="-0.25rem 0.25rem 0.5rem 0 destructive" />
                        <Label text_size="0.6rem" color="muted-foreground">"dx&lt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="0 0 0.5rem -0.125rem foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"spread&lt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" box_shadow="0 0.125rem 0.25rem 0.25rem foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"spread&gt;0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="secondary" rounded="0" box_shadow="0.125rem 0.125rem 0.5rem 0 primary" />
                        <Label text_size="0.6rem" color="muted-foreground">"直角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="accent" rounded="1.25rem" box_shadow="0 0.375rem 1rem 0 black" />
                        <Label text_size="0.6rem" color="muted-foreground">"大圆角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem"
                            box_shadow_color="foreground"
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
                        <Box w="6.5rem" h="3.75rem" bg="secondary" rounded="0.5rem" box_shadow="inset 0 0 0.75rem 0 black" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 均匀"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="accent" rounded="0.5rem" box_shadow="inset 0.125rem 0.125rem 0.5rem 0 black" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset+offset"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="muted" rounded="0.5rem" box_shadow="inset 0 0 0.5rem 0.25rem destructive" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset+正 spread"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="background" rounded="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625"
                            box_shadow="inset -0.125rem -0.125rem 0.5rem -0.0625 foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 负 offset+负 spread"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="background" rounded="0" box_shadow="inset 0 0.25rem 0.75rem 0 foreground" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 直角"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6.5rem" h="3.75rem" bg="secondary" rounded="1rem" box_shadow="inset 0 0 1rem 0.125rem black" />
                        <Label text_size="0.6rem" color="muted-foreground">"inset 大圆角"</Label>
                    </VStack>
                </HStack>
                <Label font_weight="700" text_size="0.875rem">"Text-shadow（glyphon）"</Label>
                <HStack gap="1rem" wrap="wrap" items="start">
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="foreground"
                            text_shadow="0.125rem 0.125rem 0.25rem black">
                            "简写 dx/dy/blur"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"简写 + black"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        // text_shadow_* 子属性与简写统一：裸数默认按 rem 解释，建议显式写 rem/px 提升可读性。
                        <Label text_size="1.1rem" font_weight="800" color="foreground"
                            text_shadow_color="black"
                            text_shadow_offset_x="0.1875rem"
                            text_shadow_offset_y="0.1875rem"
                            text_shadow_blur="0.375rem">
                            "子属性覆盖"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"color + offset + blur"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="destructive"
                            text_shadow="-0.0625rem -0.0625rem 0.125rem black">
                            "负偏移"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"左上浅影"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="foreground"
                            text_shadow="0 0.25rem 0.75rem black">
                            "重模糊"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"仅 dy + 大 blur"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="1.1rem" font_weight="800" color="foreground"
                            text_shadow="0.25rem 0 0 black">
                            "横向偏移"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"blur=0"</Label>
                    </VStack>
                    <VStack gap="0.35rem" items="start">
                        <Label text_size="0.95rem" font_weight="700" color="foreground" leading="tight"
                            text_shadow="0.0625rem 0.0625rem 0.1875rem black">
                            "多行 Label\n第二行仍应有阴影"
                        </Label>
                        <Label text_size="0.65rem" color="muted-foreground">"换行"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// MDN-style filter demo subject (downloaded into `examples/gallery/assets/`).
const FILTER_DEMO_LOGO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/firefox-logo.svg");

fn filter_demo_tile(label: &str, logo: Element) -> Element {
    ui! {
        <VStack gap="0.25rem" items="center">
            { logo }
            <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
        </VStack>
    }
}

fn filter_demo_logo(filter: Option<&str>, class: Option<&str>) -> Element {
    // Padding gives drop-shadow / blur room inside the capture bounds.
    let mut wrap: Element = r#box()
        .p("0.5rem")
        .w("5.5rem")
        .h("5.5rem")
        .items_center()
        .justify_center()
        .into();
    if let Some(f) = filter {
        wrap = wrap.filter(f);
    }
    if let Some(c) = class {
        wrap = rui::style::with_class_list(wrap, c);
    }
    wrap.child(ui! {
        <Image src={FILTER_DEMO_LOGO} w="4.5rem" h="4.5rem" />
    })
}

#[component]
fn FilterShowcase() -> rui::core::tree::Element {
    // Register a sample SVG filter for url(#galleryBlurSepia)
    let _ = rui::style::register_svg_filter(
        "galleryBlurSepia",
        r##"<filter id="galleryBlurSepia">
  <feGaussianBlur stdDeviation="2"/>
  <feColorMatrix type="matrix" values="
    0.393 0.769 0.189 0 0
    0.349 0.686 0.168 0 0
    0.272 0.534 0.131 0 0
    0 0 0 1 0"/>
</filter>"##,
    );

    let original = filter_demo_tile("原图", filter_demo_logo(None, None));
    let tiles_fn: Vec<Element> = [
        ("blur(4px)", Some("blur(4px)"), None),
        ("brightness(1.5)", Some("brightness(1.5)"), None),
        ("contrast(200%)", Some("contrast(200%)"), None),
        ("grayscale(1)", Some("grayscale(1)"), None),
        ("hue-rotate(90deg)", Some("hue-rotate(90deg)"), None),
        ("invert(1)", Some("invert(1)"), None),
        ("opacity(0.4)", Some("opacity(0.4)"), None),
        ("saturate(0.2)", Some("saturate(0.2)"), None),
        ("sepia(1)", Some("sepia(1)"), None),
        (
            "drop-shadow",
            Some("drop-shadow(0.25rem 0.25rem 0.5rem rgba(0,0,0,0.55))"),
            None,
        ),
    ]
    .into_iter()
    .map(|(label, filter, class)| filter_demo_tile(label, filter_demo_logo(filter, class)))
    .collect();

    let tiles_combo: Vec<Element> = [
        ("grayscale+blur", Some("grayscale(0.4) blur(3px)"), None),
        ("class blur-md", None, Some("blur-md brightness-110")),
        ("grayscale+shadow", None, Some("grayscale drop-shadow-lg")),
        ("url(#…)", Some("url(#galleryBlurSepia)"), None),
        (
            "三函数链",
            Some("contrast(1.2) brightness(1.05) saturate(1.3)"),
            None,
        ),
    ]
    .into_iter()
    .map(|(label, filter, class)| filter_demo_tile(label, filter_demo_logo(filter, class)))
    .collect();

    ui! {
        <MyCard title="12b) CSS filter">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对齐 MDN：以彩色 Firefox logo（assets/firefox-logo.svg）为 SourceGraphic，子树捕获后按序应用 filter。支持函数列表、Tailwind（blur-md / grayscale…）与 url(#id)。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"函数"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { original }
                    { tiles_fn }
                </HStack>
                <Label font_weight="700" text_size="0.875rem">"组合 / Tailwind / url()"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { tiles_combo }
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn gradient_demo_tile(label: &str, el: Element) -> Element {
    ui! {
        <VStack gap="0.25rem" items="center">
            { el }
            <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
        </VStack>
    }
}

fn gradient_swatch(css: &str, w: &str, h: &str) -> Element {
    Element::from(r#box().w(w).h(h).rounded("0.5rem").background_image(css))
}

/// MDN-aligned CSS gradient showcase (linear / radial / conic / repeating).
#[component]
fn GradientShowcase() -> rui::core::tree::Element {
    let linear_tiles: Vec<Element> = [
        ("basic ↓", "linear-gradient(blue, pink)", "7.5rem", "4.5rem"),
        (
            "to right",
            "linear-gradient(to right, blue, pink)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "to bottom right",
            "linear-gradient(to bottom right, blue, pink)",
            "9rem",
            "4.5rem",
        ),
        (
            "70deg",
            "linear-gradient(70deg, blue, pink)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "多色",
            "linear-gradient(red, yellow, blue, orange)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "硬边条纹",
            "linear-gradient(to bottom left, cyan 50%, palegoldenrod 50%)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "色带",
            "linear-gradient(to left, lime 20%, red 30% 45%, cyan 55% 70%, yellow 80%)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "color hint",
            "linear-gradient(to top, lime, 20%, cyan)",
            "7.5rem",
            "4.5rem",
        ),
    ]
    .into_iter()
    .map(|(label, css, w, h)| gradient_demo_tile(label, gradient_swatch(css, w, h)))
    .collect();

    let stacked = Element::from(
        r#box()
            .w("8rem")
            .h("8rem")
            .rounded("0.5rem")
            .overflow("hidden")
            // Each stacked layer must also carry the same radii: overflow clip is
            // axis-aligned, so corner masking comes from the gradient texture bake.
            .background_image("linear-gradient(217deg, #ff0000cc, transparent 70.71%)")
            .child(Element::from(
                r#box()
                    .w("100%")
                    .h("100%")
                    .rounded("0.5rem")
                    .background_image("linear-gradient(127deg, #00ff00cc, transparent 70.71%)")
                    .child(Element::from(
                        r#box()
                            .w("100%")
                            .h("100%")
                            .rounded("0.5rem")
                            .background_image(
                                "linear-gradient(336deg, #0000ffcc, transparent 70.71%)",
                            ),
                    )),
            )),
    );

    let radial_tiles: Vec<Element> = [
        ("basic", "radial-gradient(red, blue)", "9rem", "4.5rem"),
        (
            "stops",
            "radial-gradient(red 10%, yellow 30%, dodgerblue 50%)",
            "7.5rem",
            "7.5rem",
        ),
        (
            "at 0% 30%",
            "radial-gradient(at 0% 30%, red 10%, yellow 30%, dodgerblue 50%)",
            "7.5rem",
            "9rem",
        ),
        (
            "ellipse closest-side",
            "radial-gradient(ellipse closest-side, red, yellow 10%, dodgerblue 50%, beige)",
            "9rem",
            "4rem",
        ),
        (
            "circle closest-side",
            "radial-gradient(circle closest-side at 25% 75%, red, yellow 10%, dodgerblue 50%, beige)",
            "9rem",
            "4.5rem",
        ),
        (
            "circle 40%",
            "radial-gradient(circle 40%, red, yellow 10%, dodgerblue 50%, beige)",
            "9rem",
            "4.5rem",
        ),
    ]
    .into_iter()
    .map(|(label, css, w, h)| gradient_demo_tile(label, gradient_swatch(css, w, h)))
    .collect();

    let conic_tiles: Vec<Element> = [
        ("basic", "conic-gradient(red, blue)", "5.5rem", "5.5rem"),
        (
            "at 0% 30%",
            "conic-gradient(at 0% 30%, red 10%, yellow 30%, dodgerblue 50%)",
            "5.5rem",
            "5.5rem",
        ),
        (
            "from 45deg",
            "conic-gradient(from 45deg, red, orange 50%, yellow 85%, green)",
            "5.5rem",
            "5.5rem",
        ),
        (
            "色轮",
            "conic-gradient(red, yellow, lime, aqua, blue, magenta, red)",
            "5.5rem",
            "5.5rem",
        ),
    ]
    .into_iter()
    .map(|(label, css, w, h)| {
        let el = Element::from(
            r#box()
                .w(w)
                .h(h)
                .rounded_full()
                .overflow("hidden")
                .background_image(css),
        );
        gradient_demo_tile(label, el)
    })
    .collect();

    let repeating_tiles: Vec<Element> = [
        (
            "repeating-linear",
            "repeating-linear-gradient(-45deg, red, red 5px, blue 5px, blue 10px)",
            "7.5rem",
            "4.5rem",
        ),
        (
            "repeating-radial",
            "repeating-radial-gradient(black, black 5px, white 5px, white 10px)",
            "5.5rem",
            "5.5rem",
        ),
        (
            "repeating-conic",
            "repeating-conic-gradient(#66ccff 0% 8.25%, #6633ff 8.25% 16.5%, #ff3399 16.5% 25%)",
            "5.5rem",
            "5.5rem",
        ),
    ]
    .into_iter()
    .map(|(label, css, w, h)| {
        let mut b = r#box().w(w).h(h).overflow("hidden").background_image(css);
        if label.contains("conic") || label.contains("radial") {
            b = b.rounded_full();
        } else {
            b = b.rounded("0.5rem");
        }
        gradient_demo_tile(label, Element::from(b))
    })
    .collect();

    let tw_tile = Element::from(rui::style::with_class_list(
        r#box().w("7.5rem").h("4.5rem").rounded("0.5rem").into(),
        "bg-gradient-to-r from-blue-500 via-purple-500 to-pink-500",
    ));

    let color_fmt_tiles: Vec<Element> = [
        (
            "rgba / hsl",
            "linear-gradient(to right, rgba(255,0,0,0.85), hsl(210 100% 50%))",
        ),
        (
            "oklch",
            "linear-gradient(135deg, oklch(0.7 0.18 40), oklch(0.65 0.2 250))",
        ),
        (
            "hwb()",
            "linear-gradient(to right, hwb(0 10% 0%), hwb(240 10% 0%))",
        ),
        (
            "color-mix()",
            "linear-gradient(to right, color-mix(in oklab, red, blue), color-mix(in srgb, lime 40%, blue))",
        ),
        (
            "rgb(from …)",
            "linear-gradient(to right, rgb(from #ff6600 r g b), rgb(from #ff6600 b g r / 70%))",
        ),
        (
            "hsl(from …)",
            "linear-gradient(to bottom, hsl(from dodgerblue h s l), hsl(from hotpink h s l))",
        ),
    ]
    .into_iter()
    .map(|(label, css)| gradient_demo_tile(label, gradient_swatch(css, "7.5rem", "4.5rem")))
    .collect();

    ui! {
        <MyCard title="12c) CSS Gradients">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对齐 MDN Using CSS gradients：linear / radial / conic + repeating；色标支持 hex / rgba / hsl / hwb / oklch / color-mix / rgb(from …)。纹理绘制（无条带缝隙），滚动后自动回补 GPU 纹理。"
                </Label>

                <Label font_weight="700" text_size="0.875rem">"Linear"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { linear_tiles }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Stacked linear（半透明叠层）"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start">
                    { gradient_demo_tile("217°+127°+336°", stacked) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Radial"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { radial_tiles }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Conic"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { conic_tiles }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Repeating"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { repeating_tiles }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Color formats"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { color_fmt_tiles }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"Tailwind utility"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start">
                    { gradient_demo_tile("from / via / to", tw_tile) }
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn BorderStyleShowcase() -> rui::core::tree::Element {
    use rui::style::CornerShape;

    fn demo_box(label: &str, style_name: &str, color: [f32; 4]) -> Element {
        let el = Element::from(
            r#box()
                .w("7rem")
                .h("4rem")
                .bg(parse_color("background"))
                .border(color, 4.0)
                .border_style(style_name)
                .rounded(8.0),
        );
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

    fn per_side_box() -> Element {
        Element::from(
            r#box()
                .w("9rem")
                .h("5rem")
                .bg(parse_color("background"))
                .border_top(parse_color("destructive"), 6.0)
                .border_right(parse_color("primary"), 3.0)
                .border_bottom(parse_color("chart-1"), 8.0)
                .border_left(parse_color("accent"), 4.0)
                .border_top_style("dashed")
                .border_right_style("dotted")
                .border_bottom_style("double")
                .border_left_style("solid")
                .rounded(6.0),
        )
    }

    fn corner_shape_box(label: &str, shape: CornerShape) -> Element {
        let el = Element::from(
            r#box()
                .w("7rem")
                .h("4rem")
                .bg(parse_color("secondary"))
                .border(parse_color("primary"), 3.0)
                .rounded(16.0)
                .border_shape(shape),
        );
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

    let bc = parse_color("primary");

    ui! {
        <MyCard title="13) CSS Border 全能力">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "border-style 全 8 种样式 · 四边独立样式/宽度/颜色 · border-shape/corner-shape（round/angle/superellipse/notch/scoop）。"
                </Label>

                <Label font_weight="700" text_size="0.875rem">"border-style 样式"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { demo_box("solid", "solid", bc) }
                    { demo_box("dotted", "dotted", bc) }
                    { demo_box("dashed", "dashed", bc) }
                    { demo_box("double", "double", bc) }
                    { demo_box("groove", "groove", bc) }
                    { demo_box("ridge", "ridge", bc) }
                    { demo_box("inset", "inset", bc) }
                    { demo_box("outset", "outset", bc) }
                    { demo_box("none", "none", bc) }
                    { demo_box("hidden", "hidden", bc) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"四边独立 width / color / style"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { per_side_box() }
                        <Label text_size="0.6rem" color="muted-foreground">"T:dashed R:dotted B:double L:solid"</Label>
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"border-radius longhand"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("7rem").h("4rem")
                                    .bg(parse_color("muted"))
                                    .border(parse_color("primary"), 2.0)
                                    .border_top_left_radius(20.0)
                                    .border_top_right_radius(4.0)
                                    .border_bottom_right_radius(16.0)
                                    .border_bottom_left_radius(0.0)
                            )
                        }
                        <Label text_size="0.6rem" color="muted-foreground">"TL:20 TR:4 BR:16 BL:0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("4rem").h("4rem")
                                    .bg(parse_color("muted"))
                                    .border(parse_color("accent"), 2.0)
                                    .rounded(32.0)
                            )
                        }
                        <Label text_size="0.6rem" color="muted-foreground">"圆形 (r=32)"</Label>
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"corner-shape（CSS Level 4）"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { corner_shape_box("round（默认）", CornerShape::Round) }
                    { corner_shape_box("angle（斜切）", CornerShape::Angle) }
                    { corner_shape_box("superellipse(4)", CornerShape::Superellipse(4.0)) }
                    { corner_shape_box("superellipse(8)", CornerShape::Superellipse(8.0)) }
                    { corner_shape_box("notch（切口）", CornerShape::Notch) }
                    { corner_shape_box("scoop（内凹）", CornerShape::Scoop) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"border-collapse / border-spacing"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("8rem").h("4rem")
                                    .bg(parse_color("background"))
                                    .border(parse_color("border"), 2.0)
                                    .border_spacing(4.0, 4.0)
                                    .rounded(4.0)
                            )
                        }
                        <Label text_size="0.6rem" color="muted-foreground">"spacing: 4px 4px"</Label>
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"逻辑属性 (LTR)"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("8rem").h("4rem")
                                    .bg(parse_color("background"))
                                    .border_inline(parse_color("primary"), 4.0)
                                    .border_block(parse_color("destructive"), 2.0)
                                    .rounded(4.0)
                            )
                        }
                        <Label text_size="0.6rem" color="muted-foreground">"inline:4+primary block:2+red"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("8rem").h("4rem")
                                    .bg(parse_color("background"))
                                    .border_inline_start(parse_color("chart-1"), 6.0)
                                    .border_inline_end(parse_color("accent"), 2.0)
                                    .border_block_start(parse_color("primary"), 4.0)
                                    .border_block_end(parse_color("destructive"), 8.0)
                                    .rounded(4.0)
                            )
                        }
                        <Label text_size="0.6rem" color="muted-foreground">"四向独立逻辑"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// CSS `outline` 全能力展示。
#[component]
fn OutlineShowcase() -> rui::core::tree::Element {
    fn outline_box(
        label: &str,
        color: [f32; 4],
        width: f32,
        style: &str,
        offset: f32,
        rounded: f32,
    ) -> Element {
        let el: Element = Element::from(
            r#box()
                .w("7rem")
                .h("4rem")
                .bg(parse_color("background"))
                .outline(color, width)
                .outline_style(style)
                .rounded(rounded),
        )
        .outline_offset(offset);
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

    fn shorthand_box(shorthand: &str) -> Element {
        let el = Element::from(
            r#box()
                .w("7rem")
                .h("4rem")
                .bg(parse_color("background"))
                .rounded(8.0),
        )
        .apply_outline_shorthand(shorthand);
        let label = shorthand.to_string();
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

    let oc = parse_color("primary");

    ui! {
        <MyCard title="14) CSS Outline 全能力">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "outline 不占布局空间 · outline-style（solid/dashed/dotted/double/…） · outline-offset · outline 简写。"
                </Label>

                <Label font_weight="700" text_size="0.875rem">"outline-style 样式"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { outline_box("solid", oc, 3.0, "solid", 0.0, 8.0) }
                    { outline_box("dotted", oc, 3.0, "dotted", 0.0, 8.0) }
                    { outline_box("dashed", oc, 3.0, "dashed", 0.0, 8.0) }
                    { outline_box("double", oc, 4.0, "double", 0.0, 8.0) }
                    { outline_box("groove", oc, 4.0, "groove", 0.0, 8.0) }
                    { outline_box("ridge", oc, 4.0, "ridge", 0.0, 8.0) }
                    { outline_box("inset", oc, 4.0, "inset", 0.0, 8.0) }
                    { outline_box("outset", oc, 4.0, "outset", 0.0, 8.0) }
                    { outline_box("none", oc, 3.0, "none", 0.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-width"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { outline_box("1px", oc, 1.0, "solid", 0.0, 6.0) }
                    { outline_box("2px", oc, 2.0, "solid", 0.0, 6.0) }
                    { outline_box("4px", oc, 4.0, "solid", 0.0, 6.0) }
                    { outline_box("8px", oc, 8.0, "solid", 0.0, 6.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-offset"</Label>
                <HStack gap="1.5rem" wrap="wrap" items="start" justify="start">
                    { outline_box("offset: 0", oc, 3.0, "solid", 0.0, 8.0) }
                    { outline_box("offset: 4px", oc, 3.0, "solid", 4.0, 8.0) }
                    { outline_box("offset: 8px", oc, 3.0, "solid", 8.0, 8.0) }
                    { outline_box("offset: 12px", oc, 3.0, "solid", 12.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-color"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { outline_box("primary", parse_color("primary"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("destructive", parse_color("destructive"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("chart-1", parse_color("chart-1"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("accent", parse_color("accent"), 3.0, "dashed", 0.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline 简写"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { shorthand_box("3px solid red") }
                    { shorthand_box("4px dashed blue") }
                    { shorthand_box("2px dotted green") }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"utility class"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="7rem" h="4rem" bg="background" rounded="0.5rem" class="outline outline-2 outline-offset-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"outline outline-2 outline-offset-4"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// CSS `scale` + `zoom` Gallery 展示。
#[component]
#[component]
fn TransitionShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="CSS transition">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Hover each box to see CSS transition effects."
                </Label>

                <Label text_size="0.7rem" font_weight="600">"Opacity fade (transition-opacity)"</Label>
                <HStack gap="0.75rem" wrap="wrap">
                    <VStack items="center" gap="0.2rem">
                        <Box w="5rem" h="3rem" bg="primary" rounded="0.5rem"
                              class="transition-opacity duration-300 hover:opacity-[0.3]" />
                        <Label text_size="0.55rem" color="muted-foreground">"300ms"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="5rem" h="3rem" bg="primary" rounded="0.5rem"
                              class="transition-opacity duration-700 hover:opacity-[0.3]" />
                        <Label text_size="0.55rem" color="muted-foreground">"700ms"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="5rem" h="3rem" bg="primary" rounded="0.5rem"
                              class="transition-opacity duration-300 delay-200 hover:opacity-[0.3]" />
                        <Label text_size="0.55rem" color="muted-foreground">"300ms +delay"</Label>
                    </VStack>
                </HStack>

                <Label text_size="0.7rem" font_weight="600">"Transform (transition-transform)"</Label>
                <HStack gap="0.75rem" wrap="wrap">
                    <VStack items="center" gap="0.2rem">
                        <Box w="5rem" h="3rem" bg="primary" rounded="0.5rem"
                              class="transition-transform duration-300 ease-out hover:scale-125" />
                        <Label text_size="0.55rem" color="muted-foreground">"scale 1.25"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="5rem" h="3rem" bg="accent" rounded="0.5rem"
                              class="scale-100 transition-transform duration-300 ease-in hover:scale-75" />
                        <Label text_size="0.55rem" color="muted-foreground">"scale 0.75"</Label>
                    </VStack>
                </HStack>

                <Label text_size="0.7rem" font_weight="600">"Timing function comparison (transition-all)"</Label>
                <HStack gap="0.75rem" wrap="wrap">
                    <VStack items="center" gap="0.2rem">
                        <Box w="4rem" h="2.5rem" bg="muted" rounded="0.5rem"
                              class="transition-all duration-500 ease-linear hover:opacity-[0.2]" />
                        <Label text_size="0.55rem" color="muted-foreground">"linear"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="4rem" h="2.5rem" bg="muted" rounded="0.5rem"
                              class="transition-all duration-500 ease-in hover:opacity-[0.2]" />
                        <Label text_size="0.55rem" color="muted-foreground">"ease-in"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="4rem" h="2.5rem" bg="muted" rounded="0.5rem"
                              class="transition-all duration-500 ease-out hover:opacity-[0.2]" />
                        <Label text_size="0.55rem" color="muted-foreground">"ease-out"</Label>
                    </VStack>
                    <VStack items="center" gap="0.2rem">
                        <Box w="4rem" h="2.5rem" bg="muted" rounded="0.5rem"
                              class="transition-all duration-500 ease-in-out hover:opacity-[0.2]" />
                        <Label text_size="0.55rem" color="muted-foreground">"ease-in-out"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ScaleZoomShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="CSS scale + zoom">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "CSS scale：纯视觉缩放（不影响布局），围绕元素中心。支持 1–3 个值（X/Y/Z）和百分比。CSS zoom：放大级别，影响布局（缩放 size/padding/border 后传给 Taffy）。"
                </Label>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "1 · scale — 均匀缩放（纯视觉）"
                </Label>
                <HStack gap="1rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="primary" rounded="0.375rem" class="scale-50" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-50 (0.5×)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="primary" rounded="0.375rem" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-100 (1×)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="primary" rounded="0.375rem" class="scale-150" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-150 (1.5×)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="destructive" rounded="0.375rem" class="scale-[0.75]" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-[0.75]"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "2 · scale X/Y — 分轴缩放（纯视觉）"
                </Label>
                <HStack gap="1rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="accent" rounded="0.375rem" class="scale-x-150" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-x-150"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="accent" rounded="0.375rem" class="scale-y-50" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-y-50"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="accent" rounded="0.375rem" class="scale-x-150 scale-y-50" />
                        <Label text_size="0.6rem" color="muted-foreground">"X 1.5 + Y 0.5"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="secondary" rounded="0.375rem" class="scale-x-[0.5] scale-y-[2]" />
                        <Label text_size="0.6rem" color="muted-foreground">"X 0.5 + Y 2"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3 · scale: none 与 hover 过渡"
                </Label>
                <HStack gap="1rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="muted" rounded="0.375rem" class="transition-transform hover:scale-125" />
                        <Label text_size="0.6rem" color="muted-foreground">"hover → 1.25×"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="muted" rounded="0.375rem" class="scale-75 transition-transform hover:scale-100" />
                        <Label text_size="0.6rem" color="muted-foreground">"0.75 → hover 1×"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="destructive" rounded="0.375rem" class="scale-100 transition-transform hover:scale-x-150" />
                        <Label text_size="0.6rem" color="muted-foreground">"hover X 1.5×"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "4 · zoom — 放大级别（影响布局）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "zoom 缩放元素的 size/padding/border/gap 后传入布局引擎。注意与 scale 的区别：scale 不改布局，zoom 改。"
                </Label>
                <HStack gap="1rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="2rem" bg="chart-1" rounded="0.25rem" class="zoom-50" />
                        <Label text_size="0.6rem" color="muted-foreground">"zoom-50 (0.5×)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="2rem" bg="chart-2" rounded="0.25rem" class="zoom-normal" />
                        <Label text_size="0.6rem" color="muted-foreground">"zoom-normal"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="2rem" bg="chart-3" rounded="0.25rem" class="zoom-150" />
                        <Label text_size="0.6rem" color="muted-foreground">"zoom-150 (1.5×)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="2rem" bg="chart-4" rounded="0.25rem" class="zoom-200" />
                        <Label text_size="0.6rem" color="muted-foreground">"zoom-200 (2×)"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "5 · scale vs zoom 对比"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "左：scale-200（视觉缩放，不撑开布局）。右：zoom-200（布局缩放，撑开周围空间）。两盒原始尺寸均为 3rem×2rem。"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="start" justify="start" p="1rem" bg="muted" rounded="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3rem" h="2rem" bg="primary" rounded="0.25rem" class="scale-200" />
                        <Label text_size="0.6rem" color="muted-foreground">"scale-200"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3rem" h="2rem" bg="destructive" rounded="0.25rem" class="zoom-200" />
                        <Label text_size="0.6rem" color="muted-foreground">"zoom-200"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// CSS `transform` 属性族 Gallery 展示。
#[component]
fn TransformShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="CSS transform">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "CSS transform：支持 translate/rotate/scale/skew/matrix/perspective 及其 3D 变体，transform-origin 控制变换原点。3D 函数投影至 2D 渲染。"
                </Label>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "1 · rotate — CSS transform: rotate()"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("primary")).rounded(6.0))
                            .css_transform("rotate(15deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotate(15deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("primary")).rounded(6.0))
                            .css_transform("rotate(45deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotate(45deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("destructive")).rounded(6.0))
                            .css_transform("rotate(-30deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotate(-30deg)"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "2 · skew — CSS transform: skew()"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("accent")).rounded(6.0))
                            .css_transform("skewX(15deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"skewX(15deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("accent")).rounded(6.0))
                            .css_transform("skewY(15deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"skewY(15deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("accent")).rounded(6.0))
                            .css_transform("skew(10deg, 20deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"skew(10,20)"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3 · skew utility class — skew-x-* / skew-y-*"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="chart-1" rounded="0.375rem" class="skew-x-6" />
                        <Label text_size="0.6rem" color="muted-foreground">"skew-x-6"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="chart-2" rounded="0.375rem" class="skew-y-12" />
                        <Label text_size="0.6rem" color="muted-foreground">"skew-y-12"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="4rem" bg="chart-3" rounded="0.375rem" class="skew-x-[20deg]" />
                        <Label text_size="0.6rem" color="muted-foreground">"skew-x-[20deg]"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "4 · 组合变换 — 多函数叠加"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("chart-4")).rounded(6.0))
                            .css_transform("rotate(30deg) scale(1.2)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotate+scale"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("chart-5")).rounded(6.0))
                            .css_transform("translateX(8px) skewX(15deg) rotate(10deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"translate+skew+rotate"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("destructive")).rounded(6.0))
                            .css_transform("matrix(0.87, 0.5, -0.5, 0.87, 0, 0)") }
                        <Label text_size="0.6rem" color="muted-foreground">"matrix()"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "5 · transform-origin"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("primary")).rounded(6.0))
                            .css_transform("rotate(45deg)")
                            .transform_origin("left top") }
                        <Label text_size="0.6rem" color="muted-foreground">"origin: left top"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("primary")).rounded(6.0))
                            .css_transform("rotate(45deg)")
                            .transform_origin("center") }
                        <Label text_size="0.6rem" color="muted-foreground">"origin: center"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("primary")).rounded(6.0))
                            .css_transform("rotate(45deg)")
                            .transform_origin("right bottom") }
                        <Label text_size="0.6rem" color="muted-foreground">"origin: right bottom"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "6 · 3D 投影到 2D — rotateX / rotateY / perspective"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("secondary")).rounded(6.0))
                            .css_transform("rotateX(45deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotateX(45deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("secondary")).rounded(6.0))
                            .css_transform("rotateY(45deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"rotateY(45deg)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("secondary")).rounded(6.0))
                            .css_transform("perspective(200px) rotateY(30deg)") }
                        <Label text_size="0.6rem" color="muted-foreground">"perspective+rotateY"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "7 · Builder API — .skew() / .matrix() / .perspective()"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("chart-1")).rounded(6.0))
                            .skew(15.0, 0.0) }
                        <Label text_size="0.6rem" color="muted-foreground">".skew(15,0)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("chart-2")).rounded(6.0))
                            .skew_x(20.0) }
                        <Label text_size="0.6rem" color="muted-foreground">".skew_x(20)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        { Element::from(r#box().w("4rem").h("4rem").bg(parse_color("chart-3")).rounded(6.0))
                            .matrix(1.0, 0.3, -0.3, 1.0, 0.0, 0.0) }
                        <Label text_size="0.6rem" color="muted-foreground">".matrix()"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// CSS `rotate` + `translate` Gallery 展示。
#[component]
fn RotateTranslateShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="CSS rotate + translate">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "CSS rotate：围绕中心旋转（不影响布局）。支持 deg / turn / rad，默认 Z 轴。CSS translate：视觉平移（不影响布局）。支持 px 和百分比（相对自身 bounding box）。"
                </Label>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "1 · rotate — 预设角度"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-0" />
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-0"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-12" />
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-12"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-45" />
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-45"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-90" />
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-90"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-180" />
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-180"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="destructive" rounded="0.375rem" class="-rotate-45" />
                        <Label text_size="0.6rem" color="muted-foreground">"-rotate-45"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "2 · rotate — 任意角度 + 单位"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="accent" rounded="0.375rem" class="rotate-[30deg]" />
                        <Label text_size="0.6rem" color="muted-foreground">"[30deg]"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="accent" rounded="0.375rem" class="rotate-[0.25turn]" />
                        <Label text_size="0.6rem" color="muted-foreground">"[0.25turn]"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="accent" rounded="0.375rem" class="rotate-[1.57rad]" />
                        <Label text_size="0.6rem" color="muted-foreground">"[1.57rad]"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3 · rotate hover 过渡"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="muted" rounded="0.375rem" class="transition-transform hover:rotate-90" />
                        <Label text_size="0.6rem" color="muted-foreground">"hover → 90°"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="muted" rounded="0.375rem" class="rotate-45 transition-transform hover:rotate-0" />
                        <Label text_size="0.6rem" color="muted-foreground">"45° → hover 0°"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="muted" rounded="0.375rem" class="transition-transform hover:rotate-180" />
                        <Label text_size="0.6rem" color="muted-foreground">"hover → 180°"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3b · 旋转容器内文字（壳+字一同转 · sticky Bake）"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-12 flex items-center justify-center">
                            <Label text_size="0.7rem" color="primary-foreground" font_weight="600">"旋转文字"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-12 + Label"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="accent" rounded="0.375rem" class="rotate-45 flex items-center justify-center">
                            <Label text_size="0.7rem" color="accent-foreground" font_weight="600">"45°"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"rotate-45 + Label"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Button class="rotate-[-15deg]" variant="outline">"倾斜按钮"</Button>
                        <Label text_size="0.6rem" color="muted-foreground">"Button rotate"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="chart-1" rounded="0.375rem" class="animate-spin flex items-center justify-center">
                            <Label text_size="0.7rem" color="primary-foreground" font_weight="600">"旋转中"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"animate-spin + Label"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="muted" rounded="0.375rem" class="transition-transform hover:rotate-45 flex items-center justify-center">
                            <Label text_size="0.7rem" font_weight="600">"悬停转"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"hover:rotate-45 + Label"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Button class="animate-spin" variant="secondary">"Spin"</Button>
                        <Label text_size="0.6rem" color="muted-foreground">"Button spin"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="secondary" rounded="0.375rem" class="scale-[3] flex items-center justify-center">
                            <Label text_size="0.7rem" font_weight="600">"SDF"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"scale-3 → Msdf"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "4 · translate — px 偏移"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "视觉平移，不影响布局。灰色虚框表示原始位置。"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="chart-1" rounded="0.375rem" class="translate-x-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"translate-x-4"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="chart-2" rounded="0.375rem" class="translate-y-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"translate-y-4"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="chart-3" rounded="0.375rem" class="-translate-x-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"-translate-x-4"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="chart-4" rounded="0.375rem" class="translate-x-4 translate-y-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"X+Y 偏移"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "5 · translate — 百分比 + 任意值"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="3rem" bg="primary" rounded="0.375rem" class="translate-x-1/2" />
                        <Label text_size="0.6rem" color="muted-foreground">"x-1/2 (50%)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="3rem" bg="destructive" rounded="0.375rem" class="translate-x-full" />
                        <Label text_size="0.6rem" color="muted-foreground">"x-full (100%)"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="3rem" bg="accent" rounded="0.375rem" class="-translate-x-1/2" />
                        <Label text_size="0.6rem" color="muted-foreground">"-x-1/2"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="3rem" bg="secondary" rounded="0.375rem" class="translate-x-[30px]" />
                        <Label text_size="0.6rem" color="muted-foreground">"x-[30px]"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="4rem" h="3rem" bg="chart-1" rounded="0.375rem" class="translate-y-[50%]" />
                        <Label text_size="0.6rem" color="muted-foreground">"y-[50%]"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "6 · rotate + translate 组合"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem">
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="primary" rounded="0.375rem" class="rotate-45 translate-x-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"旋转+平移"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="destructive" rounded="0.375rem" class="rotate-12 translate-x-1/2 scale-110" />
                        <Label text_size="0.6rem" color="muted-foreground">"旋转+%平移+缩放"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="3.5rem" h="3.5rem" bg="muted" rounded="0.375rem" class="transition-transform hover:rotate-45 hover:translate-x-4 hover:scale-110" />
                        <Label text_size="0.6rem" color="muted-foreground">"hover 组合"</Label>
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
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"B) Visibility: auto / always / never"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Box w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-auto" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="auto" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"auto"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("hover/scroll {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Box>
                    <Box w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-always" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="always" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"always"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("always on {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Box>
                    <Box w="13.5rem" h="6.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-vis-never" w="100%" h="100%" p="0.5rem"
                            scrollbar_visibility="never" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"never"</Label>
                                <For each={1..=7}>
                                    <Label>{ format!("still scrollable {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Box>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"C) Four corners: (left/right) x (top/bottom)"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
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
                    </Box>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn AccordionShowcase() -> rui::core::tree::Element {
    let multi_open = use_signal("gal-acc-multi", || vec!["m-1".to_string()]);

    let builder_el: Element = accordion("gal-acc-builder")
        .default_value(vec!["b-1".into()])
        .item("b-1", "Builder: Is it accessible?", || {
            label("")
                .text("Yes. Builder 与 RSX 共用 accordion_scope / accordion_content。")
                .font_weight(400.0)
                .into()
        })
        .item("b-2", "Builder: Can I use multiple?", || {
            label("")
                .text("Set multiple(true) and value_signal for controlled multi-open.")
                .font_weight(400.0)
                .into()
        })
        .into();

    ui! {
        <MyCard title="Accordion（ShadCN base）">
            <VStack gap="1rem" w="100%" max_w="36rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：复合 RSX（宏 → accordion_scope / accordion_content）与 Builder。单开默认；multiple 允许多项同时展开。面板 `@keyframes accordion-down/up`（200ms，对位 Dialog CSS 动画）。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A · RSX 单开（defaultValue）"</Label>
                <Accordion id="gal-acc-single" defaultValue={vec!["item-1".into()]}>
                    <AccordionItem value="item-1">
                        <AccordionTrigger>"Is it accessible?"</AccordionTrigger>
                        <AccordionContent>
                            <Label font_weight="400" leading="normal">
                                "Yes. It adheres to the WAI-ARIA design pattern."
                            </Label>
                        </AccordionContent>
                    </AccordionItem>
                    <AccordionItem value="item-2">
                        <AccordionTrigger>"Is it styled?"</AccordionTrigger>
                        <AccordionContent>
                            <Label font_weight="400" leading="normal">
                                "Yes. cn-accordion-* 样式：border-b 项、py-4 触发器、muted 正文。"
                            </Label>
                        </AccordionContent>
                    </AccordionItem>
                    <AccordionItem value="item-3">
                        <AccordionTrigger>"Is it animated?"</AccordionTrigger>
                        <AccordionContent>
                            <Label font_weight="400" leading="normal">
                                "Chevron 翻转 + 内容 accordion-down/up 进出场（overflow-hidden，200ms CSS keyframes）。"
                            </Label>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
                <Label font_weight="700" text_size="0.875rem">"B · RSX multiple + valueSignal"</Label>
                <Accordion id="gal-acc-multi" multiple={true} valueSignal={multi_open}>
                    <AccordionItem value="m-1">
                        <AccordionTrigger>"Section one"</AccordionTrigger>
                        <AccordionContent>
                            <Label font_weight="400" leading="normal">"Multiple mode keeps several panels open."</Label>
                        </AccordionContent>
                    </AccordionItem>
                    <AccordionItem value="m-2">
                        <AccordionTrigger>"Section two"</AccordionTrigger>
                        <AccordionContent>
                            <Label font_weight="400" leading="normal">"Toggle independently; chevron reflects each item."</Label>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
                <Label font_weight="700" text_size="0.875rem">"C · Builder 双写"</Label>
                { builder_el }
            </VStack>
        </MyCard>
    }
}

#[component]
fn CollapsibleShowcase() -> rui::core::tree::Element {
    let controlled = use_signal("gal-col-ctrl", || false);

    let builder_el: Element = collapsible("gal-col-builder")
        .default_open(true)
        .trigger(|| Element::new().text("Builder: Can I use this in my project?"))
        .content(|| {
            label("")
                .text("Yes. Builder 与 RSX 共用 collapsible_scope / collapsible_content。")
                .font_weight(400.0)
                .into()
        })
        .into();

    ui! {
        <MyCard title="Collapsible（ShadCN base）">
            <VStack gap="1rem" w="100%" max_w="36rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：复合 RSX（宏 → collapsible_scope / collapsible_content）与 Builder。单面板开合；`exclusive` + `group` 可做同组互斥。受控用 openSignal。面板 `@keyframes collapsible-down/up`（200ms，与 accordion 同形不同名）。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A · RSX defaultOpen"</Label>
                <Collapsible id="gal-col-default" defaultOpen={true}>
                    <CollapsibleTrigger>"Can I use this in my project?"</CollapsibleTrigger>
                    <CollapsibleContent>
                        <Label font_weight="400" leading="normal">
                            "Yes. Free to use for personal and commercial projects. No attribution required."
                        </Label>
                    </CollapsibleContent>
                </Collapsible>
                <Label font_weight="700" text_size="0.875rem">"B · RSX openSignal（受控）"</Label>
                <HStack gap="0.5rem" items="center">
                    <Button
                        variant="outline"
                        size="sm"
                        on_click={move || controlled.set(!controlled.peek())}
                    >
                        { if controlled.get() { "Collapse" } else { "Expand" } }
                    </Button>
                    <Label text_size="0.75rem" color="muted-foreground">
                        { format!("open = {}", controlled.get()) }
                    </Label>
                </HStack>
                <Collapsible id="gal-col-ctrl" openSignal={controlled}>
                    <CollapsibleTrigger>"@peduarte starred 3 repositories"</CollapsibleTrigger>
                    <CollapsibleContent>
                        <VStack gap="0.25rem" w="100%">
                            <Label font_weight="400" leading="normal">"@radix-ui/primitives"</Label>
                            <Label font_weight="400" leading="normal">"@radix-ui/colors"</Label>
                            <Label font_weight="400" leading="normal">"@stitches/react"</Label>
                        </VStack>
                    </CollapsibleContent>
                </Collapsible>
                <Label font_weight="700" text_size="0.875rem">"C · Trigger 包裹 Button"</Label>
                <Collapsible id="gal-col-btn">
                    <CollapsibleTrigger>
                        <Button variant="ghost" size="sm">"Product details"</Button>
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <Label font_weight="400" leading="normal">
                            "自定义 Trigger 子节点只挂点击；外观由 Button 决定。"
                        </Label>
                    </CollapsibleContent>
                </Collapsible>
                <Label font_weight="700" text_size="0.875rem">"D · Builder 双写"</Label>
                { builder_el }
                <Label font_weight="700" text_size="0.875rem">"E · exclusive + group"</Label>
                <Collapsible id="gal-col-ex-a" exclusive={true} group="gal-col-excl" defaultOpen={true}>
                    <CollapsibleTrigger>"Only one of these two stays open"</CollapsibleTrigger>
                    <CollapsibleContent>
                        <Label font_weight="400" leading="normal">
                            "同 group 互斥；打开另一项会关掉这一项。"
                        </Label>
                    </CollapsibleContent>
                </Collapsible>
                <Collapsible id="gal-col-ex-b" exclusive={true} group="gal-col-excl">
                    <CollapsibleTrigger>"The other panel in gal-col-excl"</CollapsibleTrigger>
                    <CollapsibleContent>
                        <Label font_weight="400" leading="normal">"exclusive 默认 false；无 group 时无效。"</Label>
                    </CollapsibleContent>
                </Collapsible>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ResizableShowcase() -> rui::core::tree::Element {
    let builder_el: Element = resizable("gal-rz-builder")
        .orientation("horizontal")
        .height(160.0)
        .default_sizes([35.0, 65.0])
        .min_sizes([20.0, 20.0])
        .with_handle(true)
        .left(|| {
            r#box()
                .w("100%")
                .h("100%")
                .bg(rui::tokens::muted())
                .p("0.75rem")
                .child(
                    rui::primitives::text::text("Builder left")
                        .color(rui::style::parse_text_color("foreground")),
                )
                .into()
        })
        .right(|| {
            r#box()
                .w("100%")
                .h("100%")
                .bg(rui::tokens::background())
                .p("0.75rem")
                .child(
                    rui::primitives::text::text("Builder right")
                        .color(rui::style::parse_text_color("foreground")),
                )
                .into()
        })
        .into();

    ui! {
        <MyCard title="4b) Resizable（ShadCN PanelGroup）">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：复合 RSX（宏 → resizable_panel_group_scope；withHandle / defaultSize）与 Builder。拖拽中间握柄调整比例。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A · RSX 水平 + withHandle"</Label>
                <Box w="100%" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" overflow="hidden" rounded="0.4375rem">
                        <ResizablePanelGroup id="gal-rz-h" orientation="horizontal" h={160.0}>
                            <ResizablePanel defaultSize={30.0} minSize={15.0}>
                                <Box w="100%" h="100%" bg="muted" p="0.75rem">
                                    <Label color="foreground">"Sidebar"</Label>
                                </Box>
                            </ResizablePanel>
                            <ResizableHandle withHandle={true} />
                            <ResizablePanel defaultSize={70.0} minSize={20.0}>
                                <Box w="100%" h="100%" bg="background" p="0.75rem">
                                    <Label color="foreground">"Content"</Label>
                                </Box>
                            </ResizablePanel>
                        </ResizablePanelGroup>
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"B · RSX 垂直三栏"</Label>
                <Box w="100%" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" overflow="hidden" rounded="0.4375rem">
                        <ResizablePanelGroup id="gal-rz-v" orientation="vertical" h={224.0}>
                            <ResizablePanel defaultSize={25.0} minSize={10.0}>
                                <Box w="100%" h="100%" bg="muted" p="0.5rem">
                                    <Label text_size="0.75rem" color="foreground">"Top"</Label>
                                </Box>
                            </ResizablePanel>
                            <ResizableHandle withHandle={true} />
                            <ResizablePanel defaultSize={50.0} minSize={15.0}>
                                <Box w="100%" h="100%" bg="background" p="0.5rem">
                                    <Label text_size="0.75rem" color="foreground">"Middle"</Label>
                                </Box>
                            </ResizablePanel>
                            <ResizableHandle withHandle={true} />
                            <ResizablePanel defaultSize={25.0} minSize={10.0}>
                                <Box w="100%" h="100%" bg="secondary" p="0.5rem">
                                    <Label text_size="0.75rem" color="foreground">"Bottom"</Label>
                                </Box>
                            </ResizablePanel>
                        </ResizablePanelGroup>
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"C · Builder 双写"</Label>
                <Box w="100%" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" overflow="hidden" rounded="0.4375rem">
                        { builder_el }
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"D · 横 + 纵嵌套"</Label>
                <Box w="100%" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" overflow="hidden" rounded="0.4375rem">
                        <ResizablePanelGroup id="gal-rz-nested" orientation="horizontal" h={240.0}>
                            <ResizablePanel defaultSize={28.0} minSize={15.0}>
                                <Box w="100%" h="100%" bg="muted" p="0.75rem">
                                    <Label text_size="0.75rem" color="foreground">"Sidebar"</Label>
                                </Box>
                            </ResizablePanel>
                            <ResizableHandle withHandle={true} />
                            <ResizablePanel defaultSize={72.0} minSize={30.0}>
                                <ResizablePanelGroup id="gal-rz-nested-v" orientation="vertical">
                                    <ResizablePanel defaultSize={40.0} minSize={15.0}>
                                        <Box w="100%" h="100%" bg="background" p="0.5rem">
                                            <Label text_size="0.75rem" color="foreground">"Editor"</Label>
                                        </Box>
                                    </ResizablePanel>
                                    <ResizableHandle withHandle={true} />
                                    <ResizablePanel defaultSize={60.0} minSize={20.0}>
                                        <Box w="100%" h="100%" bg="secondary" p="0.5rem">
                                            <Label text_size="0.75rem" color="foreground">"Terminal"</Label>
                                        </Box>
                                    </ResizablePanel>
                                </ResizablePanelGroup>
                            </ResizablePanel>
                        </ResizablePanelGroup>
                    </Box>
                </Box>
            </VStack>
        </MyCard>
    }
}

/// WP-2.3 Gallery 验收：嵌套 flex 加宽子项 → wrap 行增高，footer 下移（非错误裁剪）。
/// 对照 docs/layout_subtree.md；live 路径为 mark_dirty + app-root compute。
#[component]
fn LayoutSubtreeShowcase() -> rui::core::tree::Element {
    let wide = use_signal("gal_wp23_wide", || false);

    ui! {
        <MyCard title="WP-2.3) 子树布局上溢升父">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 docs/layout_subtree.md。中间行为 Auto 高 + wrap；加宽 chip 后行应折行增高，下方 footer 下移（不得被裁切叠住）。"
                </Label>
                <HStack gap="0.5rem" wrap="wrap">
                    <Button variant="default" on_click={move || wide.set(true)}>
                        "加宽 chip"
                    </Button>
                    <Button variant="outline" on_click={move || wide.set(false)}>
                        "复位"
                    </Button>
                </HStack>
                <Box w="14rem" bg="muted" rounded="0.5rem" p="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                    <VStack w="100%" gap="0.35rem">
                        <Label text_size="0.75rem" font_weight="600">"header"</Label>
                        <HStack w="100%" gap="0.25rem" wrap="wrap">
                            <Box
                                h="1.5rem"
                                rounded="0.25rem"
                                bg="primary"
                                p="0.15rem"
                                w={ if wide.get() { "11rem" } else { "3.5rem" } }
                            >
                                <Label text_size="0.7rem" text_color="primary-foreground">"chip"</Label>
                            </Box>
                            <Box w="3.5rem" h="1.5rem" rounded="0.25rem" bg="secondary" p="0.15rem">
                                <Label text_size="0.7rem">"sib"</Label>
                            </Box>
                        </HStack>
                        <Box w="100%" bg="accent" rounded="0.25rem" p="0.35rem">
                            <Label text_size="0.75rem" font_weight="600">"footer（应随 mid 下移）"</Label>
                        </Box>
                    </VStack>
                </Box>
            </VStack>
        </MyCard>
    }
}

/// WP-2.2 Gallery 验收（策略可测）：同宽 bind_text → 逻辑 PartialLoad；结构变更 → FullClear。
/// GPU PartialLoad 默认关（见 docs/damage.md / 计划 B8）。计数用快照，勿每帧 ReactiveSync。
#[component]
fn DamageShowcase() -> rui::core::tree::Element {
    let count = use_signal("gal_wp22_count", || 0_u32);
    let show_extra = use_signal("gal_wp22_struct", || false);
    let partial_snap = use_signal("gal_wp22_partial_snap", || 0_u64);
    let full_snap = use_signal("gal_wp22_full_snap", || 0_u64);
    let base_partial = use_signal("gal_wp22_base_partial", || 0_u64);
    let base_full = use_signal("gal_wp22_base_full", || 0_u64);

    let snap_stats = move || {
        partial_snap.set(rui::partial_load_frame_count());
        full_snap.set(rui::full_clear_frame_count());
    };

    ui! {
        <MyCard title="WP-2.2) 细粒度 damage（策略可测）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 docs/damage.md。快照为逻辑 Clear/Load 策略累计（默认 GPU 仍 FullClear）。勿每帧悬停读数。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "勾选：① 重置→连点同宽+1→刷新：ΔPartialLoad↑、ΔFullClear≈0 ② 切换结构→刷新：ΔFullClear↑"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!(
                        "快照: PartialLoad={} (+{})  FullClear={} (+{})",
                        partial_snap.get(),
                        partial_snap.get().saturating_sub(base_partial.get()),
                        full_snap.get(),
                        full_snap.get().saturating_sub(base_full.get())
                    ) }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Label text_size="1.5rem" font_weight="700" leading="none">
                        { format!("{:02}", count.get() % 100) }
                    </Label>
                    <Button variant="default" on_click={move || {
                        count.set(count.get().wrapping_add(1));
                        // Present of this click runs after the handler; snap on next refresh.
                    }}>
                        "同宽 +1（00–99）"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        show_extra.set(!show_extra.get());
                    }}>
                        "切换结构（Show）"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        snap_stats();
                    }}>
                        "刷新快照"
                    </Button>
                    <Button variant="secondary" on_click={move || {
                        rui::reset_frame_policy_counters();
                        partial_snap.set(0);
                        full_snap.set(0);
                        base_partial.set(0);
                        base_full.set(0);
                    }}>
                        "重置计数"
                    </Button>
                </HStack>
                <Show when={show_extra.get()}>
                    <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        <Label text_size="0.875rem" leading="normal">
                            "结构变更样例：挂载/卸载此块应走全帧 Clear（ΔFullClear↑）。"
                        </Label>
                    </Box>
                </Show>
            </VStack>
        </MyCard>
    }
}

/// WP-2.4 Gallery 验收：Owner dispose 后 Anchors/Bindings 不残留；scoped id 可演示。
#[component]
fn OwnerRegistryShowcase() -> rui::core::tree::Element {
    use rui::app::reactive::{
        component_props_count, registered_anchor_count, registered_binding_count,
    };
    use rui::core::reactivity::{scoped_focus_id, scoped_template_id};

    let mount = use_signal("gal_wp24_mount", || true);
    let cycles = use_signal("gal_wp24_cycles", || 0_u32);
    let isle_n = use_signal("gal_wp24_isle_n", || 0_u32);
    // Baseline captured while island is *unmounted* (Gallery other islands still counted).
    let base_a = use_signal("gal_wp24_base_a", || None::<usize>);
    let base_b = use_signal("gal_wp24_base_b", || None::<usize>);
    let base_p = use_signal("gal_wp24_base_p", || None::<usize>);
    let cur_a = use_signal("gal_wp24_cur_a", || 0_usize);
    let cur_b = use_signal("gal_wp24_cur_b", || 0_usize);
    let cur_p = use_signal("gal_wp24_cur_p", || 0_usize);

    let refresh_cur = move || {
        cur_a.set(registered_anchor_count());
        cur_b.set(registered_binding_count());
        cur_p.set(component_props_count());
    };

    let capture_baseline = move || {
        refresh_cur();
        // Only valid while this card's island is unmounted; otherwise baseline is inflated.
        if mount.peek() {
            return;
        }
        base_a.set(Some(cur_a.peek()));
        base_b.set(Some(cur_b.peek()));
        base_p.set(Some(cur_p.peek()));
        cycles.set(0);
    };

    ui! {
        <MyCard title="WP-2.4) Owner 注册表 + scoped ID">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威判定：库测全绿即 WP-2.4 通过。本卡片只做手工对照；细则见 docs/AUTHORITY.md「WP-2.4 验收」。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "A · 库测（必须）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命令：cargo test -p rui --lib registry_owner"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "通过：2 passed。失败：任一 FAILED。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B · Gallery 手工（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1) 若岛已挂载 → 先点「卸载岛」  2) 「刷新当前」  3) 「记当前为卸载基线」（仅卸载态有效）  4) 「挂载岛」→「刷新当前」→ a 应 ≥ 基线 a+1  5) 「卸载岛」→「刷新当前」→ a/b/p 均 ≤ 基线  6) 重复至 cycles≥10，最后卸载态仍 ≤ 基线  7) 挂载时两行须含 ::tpl:gal_wp24_isle 与 ::focus:gal_wp24_btn。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "说明：基线含其它 Gallery 岛，故判「≤ 基线」而非归零。卸载后任一计数 > 基线 = 泄漏失败。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { {
                        let ba = base_a.get();
                        let bb = base_b.get();
                        let bp = base_p.get();
                        let mounted = mount.get();
                        match (ba, bb, bp) {
                            (Some(ba), Some(bb), Some(bp)) => format!(
                                "基线(卸载): a={} b={} p={}  |  当前: a={} b={} p={}  |  cycles={}  |  {}",
                                ba,
                                bb,
                                bp,
                                cur_a.get(),
                                cur_b.get(),
                                cur_p.get(),
                                cycles.get(),
                                if mounted {
                                    "状态: 已挂载（卸后再刷新才能判通过）"
                                } else if cur_a.get() <= ba
                                    && cur_b.get() <= bb
                                    && cur_p.get() <= bp
                                {
                                    if cycles.get() >= 10 {
                                        "手工判定: 通过（卸载≤基线 且 cycles≥10）"
                                    } else {
                                        "手工判定: 卸载≤基线（请继续循环到 cycles≥10）"
                                    }
                                } else {
                                    "手工判定: 失败（卸载后计数 > 基线）"
                                }
                            ),
                            _ => {
                                if mount.get() {
                                    "尚未记基线：先「卸载岛」→「刷新当前」→「记当前为卸载基线」。"
                                        .to_string()
                                } else {
                                    "尚未记基线：点「刷新当前」后点「记当前为卸载基线」。"
                                        .to_string()
                                }
                            }
                        }
                    } }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="outline" on_click={move || {
                        let next = !mount.peek();
                        mount.set(next);
                        if !next {
                            cycles.set(cycles.peek().wrapping_add(1));
                        }
                    }}>
                        { if mount.get() { "卸载岛" } else { "挂载岛" } }
                    </Button>
                    <Button variant="secondary" on_click={move || refresh_cur()}>
                        "刷新当前"
                    </Button>
                    <Button variant="default" on_click={move || capture_baseline()}>
                        "记当前为卸载基线"
                    </Button>
                </HStack>
                <Show when={mount.get()}>
                    <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        <VStack gap="0.5rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">"结构岛（挂载时注册 anchor）"</Label>
                            <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                                { format!(
                                    "scoped template_id = {}\nscoped focus_id = {}",
                                    scoped_template_id("gal_wp24_isle"),
                                    scoped_focus_id("gal_wp24_btn")
                                ) }
                            </Label>
                            <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                                "通过条件：上一行须同时出现 \"::tpl:gal_wp24_isle\" 与 \"::focus:gal_wp24_btn\"（前面有 root…/ 路径）。"
                            </Label>
                            <Reactive id="gal_wp24_isle">
                                <VStack gap="0.5rem" w="100%">
                                    <Label text_size="0.875rem">
                                        { format!("reactive ticks = {}", isle_n.get()) }
                                    </Label>
                                    <Button variant="secondary" size="sm" on_click={move || {
                                        isle_n.set(isle_n.peek().wrapping_add(1));
                                    }}>
                                        "岛内 +1"
                                    </Button>
                                </VStack>
                            </Reactive>
                        </VStack>
                    </Box>
                </Show>
            </VStack>
        </MyCard>
    }
}

#[component]
fn StoreResourceShowcase() -> rui::core::tree::Element {
    use rui::app::{error_boundary, throw_resource_error};
    use rui::core::reactivity::{create_effect, create_memo};
    use rui::core::resource::{Resource, ResourceState};
    use rui::core::store::create_store;

    #[derive(Clone, Debug, PartialEq)]
    struct Form {
        name: String,
        age: u32,
    }

    let store = create_store(Form {
        name: "alice".into(),
        age: 20,
    });
    let name = store.field("name", |f| f.name.clone(), |f, v| f.name = v);
    let age = store.field("age", |f| f.age, |f, v| f.age = v);
    let name_ticks = use_signal("gal_wp25_name_ticks", || 0_u32);
    let age_ticks = use_signal("gal_wp25_age_ticks", || 0_u32);
    let action_note = use_signal("gal_wp25_action_note", || "尚未操作".to_string());
    create_effect({
        let name = name.clone();
        let name_ticks = name_ticks;
        move || {
            let _ = name.get();
            name_ticks.update(|n| *n = n.wrapping_add(1));
        }
    });
    create_effect({
        let age = age.clone();
        let age_ticks = age_ticks;
        move || {
            let _ = age.get();
            age_ticks.update(|n| *n = n.wrapping_add(1));
        }
    });

    // Named signal: survives section rebuilds; EB + buttons share one handle.
    let res_state = use_signal("gal_wp25_res", || ResourceState::Ready(1_i32));
    let resource = Resource::from_signal(res_state);
    let res_summary = create_memo(move || match res_state.get() {
        ResourceState::Ready(v) => format!("Ready({v})"),
        ResourceState::Err(e) => format!("Err({e})"),
        ResourceState::Loading { .. } => "Loading".into(),
    });

    let eb = error_boundary(
        "gal_wp25_eb",
        |msg| {
            ui! {
                <Label w="100%" text_size="0.875rem" color="destructive" font_weight="600" leading="normal">
                    { format!("fallback: {msg}") }
                </Label>
            }
        },
        move || {
            throw_resource_error(resource);
            ui! {
                <Label w="100%" text_size="0.875rem" leading="normal">
                    { format!("children ok · resource = {:?}", resource.state()) }
                </Label>
            }
        },
    );

    ui! {
        <MyCard title="WP-2.5) Store produce + Resource→EB">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威判定：库测全绿即 WP-2.5 通过。本卡片只做手工对照；细则见 docs/AUTHORITY.md「WP-2.5 验收」。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "A · 库测（必须）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命令：cargo test -p rui --lib produce_notifies   与   cargo test -p rui --lib resource_err_to_error_boundary   与   cargo test -p rui --lib create_resource_err_sets"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "通过：三测均 passed。失败：任一 FAILED。CHANGELOG 草案：docs/CHANGELOG.md。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B1 · produce 切片（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1) 记下 name_ticks / age_ticks  2) 「age+1」→ 仅 age_ticks +1，name_ticks 不变  3) 「改名」→ 仅 name_ticks +1  4) 「同值再 set」→ ticks 不变，下方 note 写「未通知」。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { format!(
                        "name={:?} age={}  |  name_ticks={}  age_ticks={}",
                        name.get(),
                        age.get(),
                        name_ticks.get(),
                        age_ticks.get()
                    ) }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="outline" on_click={move || {
                        store.produce(|f| f.age = f.age.wrapping_add(1));
                        action_note.set("produce: age+1（应仅 age_ticks +1）".into());
                    }}>
                        "age+1"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        store.produce(|f| {
                            f.name = if f.name == "alice" {
                                "bob".into()
                            } else {
                                "alice".into()
                            };
                        });
                        action_note.set("produce: 改名（应仅 name_ticks +1）".into());
                    }}>
                        "改名"
                    </Button>
                    <Button variant="secondary" on_click={move || {
                        let before_n = name_ticks.peek();
                        let before_a = age_ticks.peek();
                        let same = store.get();
                        store.set(same);
                        let same_ticks =
                            name_ticks.peek() == before_n && age_ticks.peek() == before_a;
                        action_note.set(if same_ticks {
                            "同值再 set：未通知（符合契约）".into()
                        } else {
                            "同值再 set：失败（ticks 不应变化）".into()
                        });
                    }}>
                        "同值再 set"
                    </Button>
                </HStack>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B2 · Resource Err → ErrorBoundary（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "契约：巩固期须显式 throw_resource_error。1) 初始 children ok  2) 「注入 Err」→ fallback: gallery boom  3) 「复位 Ready」→ 回到 children ok。旁路行 resource= 应同步变。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { format!("resource(旁路) = {}  |  note: {}", res_summary.get(), action_note.get()) }
                </Label>
                <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                    { eb }
                </Box>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="destructive" on_click={move || {
                        resource.set_error("gallery boom");
                        action_note.set("已注入 Err → 框内应见 fallback".into());
                    }}>
                        "注入 Err"
                    </Button>
                    <Button variant="default" on_click={move || {
                        resource.mutate(1);
                        action_note.set("已复位 Ready → 框内应见 children ok".into());
                    }}>
                        "复位 Ready"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// WP-2.7 Gallery 验收：根裸 `get()` 可观测；bind / Show 不抬高 UI-root dep。
#[component]
fn RootBareGetShowcase() -> rui::core::tree::Element {
    use rui::core::reactivity::ui_root_effect_dep_count;

    let root_deps = use_signal("gal_wp27_root_deps", || ui_root_effect_dep_count());
    let baseline = use_signal("gal_wp27_baseline", || None::<usize>);
    let label_n = use_signal("gal_wp27_label_n", || 0_u32);
    let panel_open = use_signal("gal_wp27_panel", || true);
    let verdict = use_signal("gal_wp27_verdict", || {
        "尚未判定：先「刷新根 dep」并「记为基线」。".to_string()
    });

    let refresh_deps = move || {
        root_deps.set(ui_root_effect_dep_count());
    };

    let capture_baseline = move || {
        refresh_deps();
        baseline.set(Some(root_deps.peek()));
        verdict.set(format!(
            "已记基线 root_deps={}。再做 B1/B2 后点「对照基线」。",
            root_deps.peek()
        ));
    };

    let compare_baseline = move || {
        refresh_deps();
        let cur = root_deps.peek();
        match baseline.peek() {
            Some(b) if cur == b => verdict.set(format!(
                "手工判定: 通过（root_deps={cur} 相对基线 {b} 未抬高）"
            )),
            Some(b) if cur < b => verdict.set(format!(
                "手工判定: 通过（root_deps={cur} < 基线 {b}；可能其它岛卸载）"
            )),
            Some(b) => verdict.set(format!(
                "手工判定: 失败（root_deps={cur} > 基线 {b}；本卡操作或别处新增了根订阅）"
            )),
            None => verdict.set("尚未记基线：先点「记为基线」。".into()),
        }
    };

    ui! {
        <MyCard title="WP-2.7) 根裸 get() 可观测">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威判定：库测全绿即 WP-2.7 通过。本卡片只做手工对照；细则见 docs/AUTHORITY.md「WP-2.7 验收」。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "A · 库测（必须）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命令：cargo test -p rui --lib root_bare_get"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "通过：3 passed（裸 get 可观测 + bind + Show 根 dep=0）。失败：任一 FAILED。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B · Gallery 手工（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1) 「刷新根 dep」→「记为基线」  2) B1 点「文案 +1」数次，文案应变  3) B2 切换「面板」  4) 「对照基线」→ root_deps 不得高于基线。说明：数值是整棵 Gallery 根 effect 订阅面；卫生目标多为 0；本卡只验证 bind/Show 不抬高。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { {
                        let b = baseline.get();
                        format!(
                            "ui_root_effect_dep_count = {}  |  基线 = {}  |  {}",
                            root_deps.get(),
                            b.map(|n| n.to_string()).unwrap_or_else(|| "—".into()),
                            verdict.get()
                        )
                    } }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="secondary" on_click={move || refresh_deps()}>
                        "刷新根 dep"
                    </Button>
                    <Button variant="default" on_click={move || capture_baseline()}>
                        "记为基线"
                    </Button>
                    <Button variant="outline" on_click={move || compare_baseline()}>
                        "对照基线"
                    </Button>
                </HStack>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B1 · bind_text（正确档）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "属性走 bind，不订根。点「文案 +1」后文案变；对照基线应仍通过。"
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Label text_size="0.875rem" font_weight="600">
                        { format!("bind label = {}", label_n.get()) }
                    </Label>
                    <Button variant="outline" size="sm" on_click={move || {
                        label_n.set(label_n.peek().wrapping_add(1));
                    }}>
                        "文案 +1"
                    </Button>
                </HStack>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B2 · Show（结构档）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "条件挂载走 show_reactive / Show，不在根 builder 裸 if get()。"
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="outline" size="sm" on_click={move || {
                        panel_open.set(!panel_open.peek());
                    }}>
                        { if panel_open.get() { "卸载面板" } else { "挂载面板" } }
                    </Button>
                </HStack>
                <Show when={panel_open.get()}>
                    <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        <Label text_size="0.875rem" leading="normal">
                            "结构岛面板（Show when）— 切换不应抬高根 dep。"
                        </Label>
                    </Box>
                </Show>
            </VStack>
        </MyCard>
    }
}

/// WP-2.8 Gallery 验收：展示型无必填 id；空串不写身份；Toggle scoped 试点。
#[component]
fn IdentityApiShowcase() -> rui::core::tree::Element {
    let note = use_signal("gal_wp28_note", || "尚未操作".to_string());
    let navbar_el: Element = navbar().brand(label("").text("Brand")).into();
    let chart_el: Element = bar_chart().values([0.3, 0.6, 0.4]).h("3rem").into();
    let progress_el: Element = progress().default_value(0.4).into();
    let toggle_el: Element = toggle("").text("B").variant("outline").into();

    ui! {
        <MyCard title="WP-2.8) 身份 API 收敛">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威判定：库测全绿即 WP-2.8 通过。本卡片只做手工对照；细则见 docs/AUTHORITY.md「WP-2.8 验收」。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "A · 库测（必须）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命令：cargo test -p rui --lib navbar_without_id   与   bar_chart_without_id   与   toggle_empty_id   与   toggle_uncontrolled_uses_scoped"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "通过：展示型无 template_id；Toggle 空 id 非空 focus/template；scoped 跨 Owner 独立。失败：任一 FAILED。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "B · Gallery 手工（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1) 下方 Navbar / Chart / Progress / Toggle 均无手写 gal-* id  2) Toggle 可点切换  3) 读屏/焦点不因空串崩溃（I-1）。展示型勿再堆无意义 id。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { note.get() }
                </Label>
                <Label text_size="0.75rem" font_weight="600">"navbar() — 无 id"</Label>
                { navbar_el }
                <Label text_size="0.75rem" font_weight="600">"bar_chart() — 无 id"</Label>
                { chart_el }
                <Label text_size="0.75rem" font_weight="600">"progress() — 自动实例键"</Label>
                { progress_el }
                <Label text_size="0.75rem" font_weight="600">"toggle(\"\") — scoped pressed"</Label>
                <HStack gap="0.5rem" items="center">
                    { toggle_el }
                    <Button variant="secondary" size="sm" on_click={move || {
                        note.set("已点旁路按钮（Toggle 请直接点 B）".into());
                    }}>
                        "记一笔"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn gal_box_model_onion() -> Element {
    let content = Element::from(
        r#box()
            .w("100%")
            .h("3rem")
            .bg(parse_color("primary"))
            .rounded("0.25rem")
            .items_center()
            .justify_center()
            .child(Element::from(
                label("")
                    .text("content")
                    .size(12.0)
                    .color(parse_text_color("primary-foreground")),
            )),
    );
    let padding = Element::from(
        r#box()
            .w("100%")
            .p("1rem")
            .bg(parse_color("secondary"))
            .child(content),
    );
    let border_box = Element::from(
        r#box()
            .w("100%")
            .border_style("solid")
            .border_top(parse_color("destructive"), 8.0)
            .border_right(parse_color("primary"), 4.0)
            .border_bottom(parse_color("accent"), 12.0)
            .border_left(parse_color("chart-1"), 16.0)
            .child(padding),
    );
    Element::from(
        r#box()
            .w("22rem")
            .p("1.25rem")
            .bg(parse_color("muted"))
            .rounded("0.5rem")
            .child(border_box),
    )
}

/// WP-2.9 / B15 Gallery：完整 CSS 盒模型（单位 · box-sizing · 分边 border · outline）。
#[component]
fn BoxModelShowcase() -> rui::core::tree::Element {
    let hits = use_signal("gal_wp29_outline_hits", || 0_u32);
    let outline_box: Element = Element::from(
        r#box()
            .w("10rem")
            .h("3rem")
            .bg(parse_color("primary"))
            .rounded("0.375rem")
            .outline(parse_color("destructive"), 12.0)
            .items_center()
            .justify_center()
            .child(Element::from(
                label("")
                    .text("点内部（勿点红描边）")
                    .size(12.0)
                    .color(parse_text_color("primary-foreground")),
            )),
    )
    .on_click({
        let hits = hits;
        move || hits.update(|n| *n = n.wrapping_add(1))
    });
    let onion = gal_box_model_onion();

    ui! {
        <MyCard title="WP-2.9 / B15) CSS 盒模型">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "手册 docs/box_model.md。默认 BorderBox；% = 父 content-box；outline/shadow 不进布局。库测：cargo test -p rui --lib box_model_contract"
                </Label>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "1 · 洋葱图：margin（外灰）→ 分边 border → padding → content"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="start">
                    { onion }
                    <VStack gap="0.25rem" min_w="10rem">
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "外层 padding 模拟 margin；border 为 T8/R4/B12/L16 且四边异色（B15）。"
                        </Label>
                        <HStack gap="0.5rem" wrap="wrap">
                            <Label text_size="0.75rem">"■ muted = margin"</Label>
                            <Label text_size="0.75rem">"■ 彩边 = border"</Label>
                            <Label text_size="0.75rem">"■ secondary = padding"</Label>
                            <Label text_size="0.75rem">"■ primary = content"</Label>
                        </HStack>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "2 · 单位：拖动窗口边缘验收 vw/vh/%（条应变长/变短）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "灰轨 = 卡片 100%。主色条 = 25vw / 25dvw（相对窗口，不是卡片）。拉宽/拉高窗口，视口条应变；% 条跟卡片宽。"
                </Label>
                <VStack w="100%" gap="0.4rem">
                    <Label text_size="0.75rem" font_weight="600">"卡片宽 100%"</Label>
                    <Box w="100%" h="0.5rem" bg="border" rounded="0.25rem" />
                    <Label text_size="0.75rem" font_weight="600">"25vw"</Label>
                    <Box w="25vw" h="2rem" bg="primary" rounded="0.375rem" items="center" justify="center">
                        <Label text_size="0.75rem" text_color="primary-foreground">"25vw"</Label>
                    </Box>
                    <Label text_size="0.75rem" font_weight="600">"25dvw"</Label>
                    <Box w="25dvw" h="2rem" bg="accent" rounded="0.375rem" items="center" justify="center">
                        <Label text_size="0.75rem" text_color="accent-foreground">"25dvw"</Label>
                    </Box>
                    <HStack gap="0.75rem" wrap="wrap" items="end">
                        <VStack gap="0.25rem">
                            <Label text_size="0.75rem" font_weight="600">"10vh"</Label>
                            <Box w="4.5rem" h="10vh" bg="muted" rounded="0.375rem" items="center" justify="center">
                                <Label text_size="0.75rem">"10vh"</Label>
                            </Box>
                        </VStack>
                        <VStack gap="0.25rem">
                            <Label text_size="0.75rem" font_weight="600">"10dvh"</Label>
                            <Box w="4.5rem" h="10dvh" bg="secondary" rounded="0.375rem" items="center" justify="center">
                                <Label text_size="0.75rem" text_color="secondary-foreground">"10dvh"</Label>
                            </Box>
                        </VStack>
                        <VStack gap="0.25rem">
                            <Label text_size="0.75rem" font_weight="600">"padding 1em @ 20px"</Label>
                            <Box w="8rem" text_size={20.0} p="1em" bg="secondary" rounded="0.375rem" items="center" justify="center">
                                <Label text_size="0.75rem" text_color="secondary-foreground">"1em pad"</Label>
                            </Box>
                        </VStack>
                    </HStack>
                    <Label text_size="0.75rem" font_weight="600">"calc(100% - 2rem)（相对卡片 content，不是视口）"</Label>
                    <Box w="100%" p="0.5rem" bg="muted" rounded="0.375rem">
                        <Box w="calc(100% - 2rem)" h="2.5rem" bg="secondary" rounded="0.25rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="secondary-foreground">"100% - 2rem"</Label>
                        </Box>
                    </Box>
                </VStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3 · box-sizing：同 w+padding，box-content 应更宽"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="end">
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"box-border（默认）"</Label>
                        <Box class="box-border" w="8rem" px="1rem" py="0.5rem" bg="secondary" rounded="0.375rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label text_size="0.75rem" text_color="secondary-foreground">"w=8rem + px"</Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"box-content"</Label>
                        <Box class="box-content" w="8rem" px="1rem" py="0.5rem" bg="accent" rounded="0.375rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label text_size="0.75rem" text_color="accent-foreground">"padding 在外侧"</Label>
                        </Box>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "4 · B15 分边 border：异宽异色 · 1–4 值简写"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="end">
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"T8 R4 B12 L16"</Label>
                        <Box
                            w="10rem"
                            h="4.5rem"
                            bg="background"
                            borderStyle="solid"
                            borderTopWidth={8.0}
                            borderRightWidth={4.0}
                            borderBottomWidth={12.0}
                            borderLeftWidth={16.0}
                            borderTopColor="destructive"
                            borderRightColor="primary"
                            borderBottomColor="accent"
                            borderLeftColor="chart-1"
                            items="center"
                            justify="center"
                        >
                            <Label text_size="0.75rem">"分边色+宽"</Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"border-width: 4px 12px 8px 20px"</Label>
                        <Box
                            w="10rem"
                            h="4.5rem"
                            bg="background"
                            borderStyle="solid"
                            borderColor="primary"
                            borderWidth="4px 12px 8px 20px"
                            items="center"
                            justify="center"
                        >
                            <Label text_size="0.75rem">"TRBL 简写"</Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"仅左边框 6px"</Label>
                        <Box
                            w="10rem"
                            h="4.5rem"
                            bg="muted"
                            px="0.75rem"
                            borderStyle="solid"
                            borderLeftWidth={6.0}
                            borderLeftColor="destructive"
                            items="center"
                            justify="center"
                        >
                            <Label text_size="0.75rem">"border-left"</Label>
                        </Box>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    { format!("5 · outline 不占命中（点内部 +1，点外侧红描边不加）：{}", hits.get()) }
                </Label>
                { outline_box }
            </VStack>
        </MyCard>
    }
}

#[component]
fn LogicalShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="5) Logical Properties & Layout">
            <VStack gap="0.75rem">
                <Box
                    inline_size="24rem"
                    block_size="auto"
                    bg="background"
                    rounded="0.5rem"
                    padding_block="0.5rem 1rem"
                    padding_inline="1rem 1.5rem"
                >
                    <Label text_size="0.875rem">
                        "padding-block/inline 双值 + inline-size + block-size"
                    </Label>
                </Box>
                <HStack gap="0.5rem" wrap="wrap" column_gap="0.75rem" row_gap="0.5rem">
                    <Box bg="background" rounded="0.4rem" px="0.75rem" py="0.5rem"><Label>"px/py"</Label></Box>
                    <Box bg="background" rounded="0.4rem" pt="0.75rem" pr="1rem" pb="0.25rem" pl="0.5rem"><Label>"pt/pr/pb/pl"</Label></Box>
                    <Box bg="background" rounded="0.4rem" mx="0.75rem" my="0.25rem"><Label>"mx/my"</Label></Box>
                    <Box bg="background" rounded="0.4rem" margin_inline="0.5rem" padding_block="0.5rem"><Label>"m-inline / p-block"</Label></Box>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// WP-2.1 visual acceptance: narrow soft-wrap, Chip/Mention atomic, VirtualList font row height.
fn gal_wp21_chip_mention() -> Element {
    Element::new()
        .text_spans(vec![
            TextSpan::new("prefix "),
            TextSpan::new("Chip Label")
                .chip()
                .padding_x(4.0)
                .background([100, 116, 139, 56]),
            TextSpan::new(" and "),
            TextSpan::new("@alice").mention(),
            TextSpan::new(" wrap after atomic spans"),
        ])
        .text_size(14.0)
        .text_leading(1.5)
        .text_color(parse_text_color("foreground"))
        .w("10rem")
        .min_w("0")
}

#[component]
fn TextAlignShowcase() -> rui::core::tree::Element {
    let row_h = rui::item_height_for_text(14.0, 1.5);
    let chip_mention = gal_wp21_chip_mention();
    // Always-on scrollbar: idle `auto` chrome hides and a mid-list offset looks "broken".
    let virt: Element = virtual_list(TEXT_ALIGN_WP21_VIRT)
        .item_count(120)
        .item_height_from_font(14.0, 1.5)
        .height(168.0)
        .overscan(4)
        .scrollbar_visibility("always")
        .render_item(|i| {
            label(&format!("gal-wp21-virt-{i}"))
                .text(format!("virtual row {i:03}"))
                .size(14.0)
                .leading(1.5)
                .into()
        })
        .into();

    ui! {
        <MyCard title="WP-2.1) 测宽 ≡ glyphon 分行">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Gallery 验收（对照 docs/text_align.md）：窄框多行软折不横溢；Chip/Mention 整段原子；VirtualList 行高 = item_height_for_text(14, 1.5)。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!("勾选：① ASCII 窄折 ② CJK 窄折 ③ Chip/Mention 不中段断开 ④ VirtualList 行高 {row_h}px、滚动只挂可见行") }
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" items="start" wrap="wrap">
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"ASCII soft-wrap（~5rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" w="5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label w="100%" text_size="0.875rem" leading="normal">
                                "hello world from rui text measure alignment"
                            </Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"CJK soft-wrap（~6rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" w="6rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label w="100%" text_size="1rem" leading="normal">
                                "中文日文测试测宽分行对齐用例足够长"
                            </Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"Chip + Mention（10rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            { chip_mention }
                        </Box>
                    </VStack>
                </HStack>
                <VStack gap="0.35rem" w="100%" min_w="0">
                    <Label font_weight="700" text_size="0.875rem">
                        { format!("VirtualList · item_height_from_font(14, 1.5) = {row_h}px") }
                    </Label>
                    <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                        "首屏应从 virtual row 000 起；滚动条常显。行高 21px，仅挂载可见区间，无重叠/空隙跳变。"
                    </Label>
                    <Box bg="muted" rounded="0.375rem" p="0.25rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        { virt }
                    </Box>
                </VStack>
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
                <Box bg="background" rounded="0.5rem" p="0.75rem" w="100%" min_w="0">
                    <VStack gap="0.5rem" w="100%" min_w="0">
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
                            "Bidirectional sample: English 123 - \u{2068}العربية\u{2060}٤٥٦\u{2069}\u{200B}- \u{2068}עברית\u{2060}789\u{2069}"
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
                </Box>
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
                <Box bg="background" rounded="0.5rem" p="0.75rem" w="100%">
                    <VStack gap="0.5rem" w="100%">
                        <Label w="100%" font_weight="700">"固定样例（视觉比对）"</Label>
                        <Label w="100%" leading="normal">"Family: 👨‍👩‍👧‍👦  Profession: 🧑‍💻"</Label>
                        <Label w="100%" leading="normal">"Skin tone: 👍🏽  Flags: 🇨🇳 🇺🇸"</Label>
                        <Label w="100%" leading="normal">
                            "Hello 😀 😁 😭  Keycap: 1️⃣ 2️⃣ 9️⃣  Mixed: Rust 👩‍💻 + 中文 + عربي"
                        </Label>
                    </VStack>
                </Box>
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
                    "Checkbox / Switch: embed text / Label inside, or sibling Label with Solid id / for. Tab / Space on the control."
                </Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Checkbox (solid-ui layout)"</Label>

                <Checkbox>"Accept terms (embedded text)"</Checkbox>

                <Checkbox>
                    <Label text_size="0.875rem" font_weight="500">
                        "Accept terms (embedded Label)"
                    </Label>
                </Checkbox>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms" />
                    <Label for="gallery_terms" text_size="0.875rem" font_weight="500">
                        "Accept terms and conditions"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="start">
                    <Checkbox id="gallery_terms_desc" defaultChecked=true />
                    <VStack gap="0.25rem" min_w="0">
                        <Label for="gallery_terms_desc" text_size="0.875rem" font_weight="500">
                            "Accept terms and conditions"
                        </Label>
                        <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                            "By clicking this checkbox, you agree to the terms."
                        </Label>
                    </VStack>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms_disabled" disabled=true />
                    <Label for="gallery_terms_disabled" text_size="0.875rem" color="muted-foreground">
                        "Enable notifications"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms_invalid" invalid=true />
                    <Label for="gallery_terms_invalid" text_size="0.875rem" color="destructive">
                        "Accept terms and conditions"
                    </Label>
                </HStack>

                <HStack
                    gap="0.5rem"
                    items="start"
                    p="1rem"
                    w="100%"
                    rounded="0.5rem"
                    border_width={1.0}
                    border_color="border"
                >
                    <Checkbox id="gallery_terms_card" />
                    <VStack gap="0.25rem" min_w="0">
                        <Label for="gallery_terms_card" text_size="0.875rem" font_weight="500">
                            "Enable notifications"
                        </Label>
                        <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                            "You can enable or disable notifications at any time."
                        </Label>
                    </VStack>
                </HStack>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Switch (solid-ui layout)"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Embed text / Label inside, or sibling Label with Solid id / for. Thumb slides with a short transition."
                </Label>

                <Switch>"Airplane Mode (embedded text)"</Switch>

                <Switch>
                    <Label text_size="0.875rem" font_weight="500">
                        "Airplane Mode (embedded Label)"
                    </Label>
                </Switch>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_airplane" />
                    <Label for="gallery_airplane" text_size="0.875rem" font_weight="500">
                        "Airplane Mode"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_wifi" defaultChecked=true />
                    <Label for="gallery_wifi" text_size="0.875rem" font_weight="500">
                        "Wi-Fi (default on)"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_switch_disabled" disabled=true />
                    <Label for="gallery_switch_disabled" text_size="0.875rem" color="muted-foreground">
                        "Do not disturb"
                    </Label>
                </HStack>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Switch · controlled Checkbox"</Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Switch id="hw_demo_switch" checked={sw} />
                    <Label text_size="0.875rem">{ format!("switch = {}", sw.get()) }</Label>
                    <Button variant="outline" size="sm" on_click={move || sw.set(!sw.get())}>
                        "Flip switch"
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="center">
                    <Checkbox id="hw_demo_checkbox" checked={cb} />
                    <Label for="hw_demo_checkbox" text_size="0.875rem">
                        { format!("checkbox = {}", cb.get()) }
                    </Label>
                    <Button variant="outline" size="sm" on_click={move || cb.set(!cb.get())}>
                        "Flip checkbox"
                    </Button>
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
    let notify = use_signal("gallery_radio_notify", || "all".to_string());
    let tier_opts = use_signal("gallery_radio_tier_opts", || "pro".to_string());
    let level = use_signal("gallery_slider_level", || 0.4_f32);

    ui! {
        <MyCard title="9) Radio group · Slider">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "RadioGroup: solid-ui circular items. Embed text / ItemLabel, or options builder. Tab / Space on the control."
                </Label>

                <Label font_weight="700" text_size="0.875rem">"RadioGroup (compound RSX)"</Label>
                <RadioGroup id="gallery_radio_tier" value={tier} default_value="free">
                    <RadioGroupItem value="free">"Free"</RadioGroupItem>
                    <RadioGroupItem value="pro">
                        <RadioGroupItemLabel>"Pro"</RadioGroupItemLabel>
                    </RadioGroupItem>
                    <RadioGroupItem value="team">"Team"</RadioGroupItem>
                </RadioGroup>
                <Label text_size="0.875rem">{ format!("Tier: {}", tier.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Notifications (default + disabled)"</Label>
                <RadioGroup id="gallery_radio_notify" value={notify} default_value="all">
                    <RadioGroupItem value="all">"All new messages"</RadioGroupItem>
                    <RadioGroupItem value="mentions">"Direct messages and mentions"</RadioGroupItem>
                    <RadioGroupItem value="none" disabled=true>"Nothing"</RadioGroupItem>
                </RadioGroup>
                <Label text_size="0.875rem">{ format!("notify: {}", notify.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Builder · options"</Label>
                { Element::from(
                    radio_group("gallery:radio_options")
                        .value_signal(tier_opts)
                        .default_value("pro")
                        .options(vec![
                            ("free".to_string(), "Free".to_string()),
                            ("pro".to_string(), "Pro".to_string()),
                            ("team".to_string(), "Team".to_string()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("tier_opts: {}", tier_opts.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Slider (solid-ui)"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Click or drag the track; arrow keys step. Track / fill / thumb match solid-ui."
                </Label>
                <Slider id="gallery:slider" value={level} default_value={0.4} />
                <Label text_size="0.875rem">{ format!("Level: {:.0}%", level.get() * 100.0) }</Label>
                <Slider id="gallery:slider_disabled" defaultValue={0.6} disabled=true />
                <Label text_size="0.75rem" color="muted-foreground">"Disabled"</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Slider · orientation=vertical"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "纵向滑块：拖拽 / ↑↓ 调值，与上方横向示例共用 level signal。"
                </Label>
                <HStack gap="1.5rem" items="end">
                    <VStack gap="0.375rem" items="center">
                        <Slider id="gallery:slider_vertical" orientation="vertical" value={level} default_value={0.4} />
                        <Label text_size="0.75rem" color="muted-foreground">{ format!("{:.0}%", level.get() * 100.0) }</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn SelectPopoverTabShowcase() -> rui::core::tree::Element {
    let tab_idx = use_signal("gallery_tabs_idx", || 0usize);
    let pop_open = use_signal("gallery_popover_open", || false);
    let pop_builder_open = use_signal("gallery_popover_builder_open", || false);
    let select_val = use_signal("gallery_select_plan", || "basic".to_string());
    let fruit_val = use_signal("gallery_select_fruit", || String::new());

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
                            .trigger_width("100%")
                            .item(SelectItemData::new("basic", "Basic"))
                            .item(SelectItemData::new("pro", "Pro"))
                            .item(SelectItemData::new("team", "Team")),
                    ) }
                    <Label text_size="0.875rem">{ format!("Selected plan: {}", select_val.get()) }</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "ShadCN 复合写法（SelectTrigger / Content / Item）："
                    </Label>
                    <Select id="gallery:select_fruit" value={fruit_val} placeholder="Select a fruit" class="w-full">
                        <SelectTrigger class="w-full">
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent max_h="12rem" class="bg-popover">
                            <SelectGroup>
                                <SelectLabel>"Fruits"</SelectLabel>
                                <SelectItem value="apple">"Apple"</SelectItem>
                                <SelectItem value="banana">"Banana"</SelectItem>
                                <SelectItem value="blueberry">"Blueberry"</SelectItem>
                            </SelectGroup>
                            <SelectSeparator />
                            <SelectItem value="pineapple">"Pineapple"</SelectItem>
                        </SelectContent>
                    </Select>
                    <Label text_size="0.875rem">{ format!("Selected fruit: {}", fruit_val.get()) }</Label>
                    <Label text_size="0.75rem" color="muted-foreground">
                        "窄触发器：选中文案省略。"
                    </Label>
                    { Element::from(
                        select("gallery:select_trunc")
                            .placeholder("A very long placeholder that should ellipsis")
                            .trigger_width("10rem")
                            .item(SelectItemData::new(
                                "long",
                                "Internationalization-ready extra long option label",
                            ))
                            .item(SelectItemData::new("short", "Short")),
                    ) }
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Popover（RSX 复合：Trigger / Content / Header · sideOffset 宏别名）："
                    </Label>
                    <Popover id="gallery:popover" open={pop_open}>
                        <PopoverTrigger>
                            <Button variant="outline" size="sm">
                                { if pop_open.get() { "Close popover" } else { "Open popover" } }
                            </Button>
                        </PopoverTrigger>
                        <PopoverContent side="bottom" align="center" sideOffset=4>
                            <PopoverHeader>
                                <PopoverTitle>"Dimensions"</PopoverTitle>
                                <PopoverDescription>
                                    "Set the dimensions for the layer. Escape closes (stacked overlay)."
                                </PopoverDescription>
                            </PopoverHeader>
                            <Label text_size="0.875rem">"w-72 · p-4 · bg-popover · shadow-md"</Label>
                        </PopoverContent>
                    </Popover>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Popover Builder 双写（open_signal + PopoverTrigger / popover_content）："
                    </Label>
                    { Element::from(
                        popover("gallery:popover_builder")
                            .open_signal(pop_builder_open)
                            .child(|| {
                                let mut frag = Element::new();
                                frag.set_attr("rui_fragment", "1");
                                frag.children(vec![
                                    PopoverTrigger(PopoverTriggerProps {
                                        id: String::new(),
                                        children: Some(
                                            button("gallery:popover_builder_btn")
                                                .variant("secondary")
                                                .size("sm")
                                                .text("Open (Builder)")
                                                .into(),
                                        ),
                                        base: None,
                                    }),
                                    popover_content(
                                        PopoverContentOpts {
                                            side: "bottom".into(),
                                            align: "start".into(),
                                            side_offset: 4.0,
                                            class: None,
                                            strategy: None,
                                            width: Some("18rem".into()),
                                        },
                                        || {
                                            Element::from(
                                                vstack()
                                                    .w("100%")
                                                    .gap("0.5rem")
                                                    .child(PopoverTitle(PopoverTitleProps {
                                                        children: Some(
                                                            Element::new().text("Builder panel"),
                                                        ),
                                                        base: None,
                                                    }))
                                                    .child(PopoverDescription(
                                                        PopoverDescriptionProps {
                                                            children: Some(Element::new().text(
                                                                "Same surface as RSX PopoverContent.",
                                                            )),
                                                            base: None,
                                                        },
                                                    )),
                                            )
                                        },
                                    ),
                                ])
                            }),
                    ) }
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
                    <Box class="p-4 rounded-lg bg-secondary">
                        <VStack class="gap-2">
                            <Label class="text-sm text-muted-foreground">"This block uses class=\"p-4 rounded-lg bg-secondary\""</Label>
                            <Label class="text-base font-semibold">"Spacing + radius + color + text size"</Label>
                            <Label class="mt-2 text-xs">"mt-2 applied here"</Label>
                        </VStack>
                    </Box>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"B) tw=..."</Label>
                    <Box tw="grid grid-cols-2 grid-rows-2 gap-x-3 gap-y-2 w-full min-h-20 p-2 bg-muted rounded-md">
                        <Box tw="bg-secondary rounded-md h-8" />
                        <Box tw="bg-secondary rounded-md h-8" />
                        <Label tw="self-end text-xs text-muted-foreground">"grid-rows-2 + gap-x/y + self-end"</Label>
                        <Box tw="bg-secondary rounded-md h-8" />
                    </Box>
                    <HStack tw="gap-3 items-center flex-wrap">
                        <Box tw="w-24 h-12 rounded-md bg-secondary" />
                        <Box tw="w-[120px] h-[48px] rounded-xl bg-muted" />
                        <Box tw="w-20 h-12 rounded-full bg-accent" />
                    </HStack>
                    <Label tw="text-sm text-muted-foreground mt-2">
                        "tw supports numeric scale (e.g. mt-2 => 8px) and arbitrary values [120px]"
                    </Label>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"C) Card 复合（ShadCN / solid-ui）"</Label>
                    <Card>
                        <CardHeader>
                            <CardTitle>"Notifications"</CardTitle>
                            <CardDescription>"You have 3 unread messages."</CardDescription>
                            <CardAction>
                                <Button variant="link" size="sm">"Sign Up"</Button>
                            </CardAction>
                        </CardHeader>
                        <CardContent>
                            <Label text_size="0.875rem" w="100%" min_w="0">
                                "Header / Title / Description / Action / Content / Footer"
                            </Label>
                        </CardContent>
                        <CardFooter>
                            <Button w="100%" size="sm">"Mark all as read"</Button>
                        </CardFooter>
                    </Card>
                    <Box uno="p-3 rounded-md bg-secondary">
                        <VStack uno="gap-2">
                            <Label uno="text-sm">"uno=\"...\" works the same way as class/tw"</Label>
                            <HStack uno="gap-2">
                                <Button variant="outline" size="sm">"Button"</Button>
                                <Label uno="mt-1 text-xs text-muted-foreground">"Label with mt-1"</Label>
                            </HStack>
                        </VStack>
                    </Box>
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
                    <Box col_span="2" p="0.75rem" bg="muted" class="rounded-md" items="center" justify="center">
                        <Label>"col_span = 2"</Label>
                    </Box>
                    <Box p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"侧栏"</Label>
                    </Box>
                    <Box p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"(2,2)"</Label>
                    </Box>
                    <Box p="0.75rem" bg="muted" class="rounded-md" items="center" justify="center">
                        <Label>"(3,2)"</Label>
                    </Box>
                    <Box p="0.75rem" bg="secondary" class="rounded-md" items="center" justify="center">
                        <Label>"(3,3)"</Label>
                    </Box>
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
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            let _ = nav_home.bring_to_front(&GalNavRoute::Home);
                        }}
                    >
                        "Home"
                    </Button>
                    <Button
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
                        size="sm"
                        variant="ghost"
                        on_click={move || {
                            let _ = nav_bar.pop();
                        }}
                    >
                        "Back"
                    </Button>
                </HStack>
                <Box w="100%" h="10rem" relative=true class="rounded-md" bg="background">
                    { body }
                </Box>
            </VStack>
        </MyCard>
    }
}

fn gallery_shell_router(id: &'static str) -> Router {
    let mut router = Router::new(id, "inbox");
    router.register("inbox", || {
        ui! { <Label text_size="0.875rem">"Inbox pane"</Label> }
    });
    router.register("settings", || {
        ui! { <Label text_size="0.875rem">"Settings pane"</Label> }
    });
    router
}

fn gallery_secondary_shell_root() -> Element {
    let count = shared_slot(GAL_SHELL_BRIDGE, || 0u32);
    let fruit = use_signal("gal_sec_fruit", || "apple".to_string());
    let dialog_open = use_signal("gal_sec_dialog_open", || false);
    // Root must be a full-window box so hoisted Dialog underlay fills the secondary surface
    // (ScrollArea-as-root clips / sizes the portal layer incorrectly).
    ui! {
        <VStack w="100%" h="100%" bg="background">
            <ScrollArea id="gal-sec-scroll" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto">
                <VStack w="100%" p="1.5rem" gap="0.75rem">
                    <Label font_weight="700" text_size="1.125rem">"Secondary shell"</Label>
                    <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                        "独立树：点击/输入/Esc 只打本窗。Dialog 黑半透明遮罩；Esc 不关主窗浮层。Bridge 共享 gal_shell_bridge_count。"
                    </Label>
                    <HStack gap="0.5rem" wrap="wrap" items="center">
                        <Label font_weight="600">{ format!("Bridge: {}", count.get()) }</Label>
                        <Button size="sm" on_click={move || count.set(count.peek() + 1)}>
                            "Inc"
                        </Button>
                        <Button
                            size="sm"
                            variant="outline"
                            on_click={move || {
                                run_on_ui_thread(|| {
                                    let slot = shared_slot(GAL_SHELL_BRIDGE, || 0u32);
                                    slot.set(slot.peek() + 1);
                                });
                            }}
                        >
                            "Inc via run_on_ui_thread"
                        </Button>
                    </HStack>
                    <Input id="gal-sec-input" placeholder="Type in secondary (focus isolated)" w="100%" />
                    <Select id="gal-sec-select" value={fruit} placeholder="Fruit" class="w-full">
                        <SelectTrigger class="w-full">
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent max_h="10rem">
                            <SelectItem value="apple">"Apple"</SelectItem>
                            <SelectItem value="pear">"Pear"</SelectItem>
                        </SelectContent>
                    </Select>
                    <Dialog id="gal-sec-dialog" open={dialog_open}>
                        <DialogTrigger>
                            <Button variant="outline" size="sm">"Open dialog"</Button>
                        </DialogTrigger>
                        <DialogContent overlay_color="#000000" overlay_opacity={0.55}>
                            <DialogHeader>
                                <DialogTitle>"Secondary dialog"</DialogTitle>
                                <DialogDescription>
                                    "Esc 应只关本窗 Dialog，主窗浮层保持。应看到黑色半透明遮罩。"
                                </DialogDescription>
                            </DialogHeader>
                            <DialogFooter>
                                <Button size="sm" on_click={move || dialog_open.set(false)}>"Close"</Button>
                            </DialogFooter>
                        </DialogContent>
                    </Dialog>
                </VStack>
            </ScrollArea>
        </VStack>
    }
}

#[component]
fn RouterShowcase() -> Element {
    let router = gallery_shell_router(GAL_SHELL_ROUTER_ID);
    let r_inbox = router.clone();
    let r_settings = router.clone();
    let r_back = router.clone();
    let r_fwd = router.clone();
    let route = router.current_signal();
    let bridge = shared_slot(GAL_SHELL_BRIDGE, || 0u32);

    ui! {
        <MyCard title="Router + 次窗（外壳）">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命名路由 + back/forward。与 Navigator（转场栈）分工：Router 管去哪一页。次窗用独立树，勿与主窗共享 Router / Navigator id。"
                </Label>
                <Label text_size="0.75rem">{ format!("Shared bridge: {}", bridge.get()) }</Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <Button size="sm" variant="outline" on_click={move || r_inbox.navigate("inbox")}>
                        "Inbox"
                    </Button>
                    <Button size="sm" variant="outline" on_click={move || r_settings.navigate("settings")}>
                        "Settings"
                    </Button>
                    <Button size="sm" variant="ghost" on_click={move || { let _ = r_back.back(); }}>
                        "Back"
                    </Button>
                    <Button size="sm" variant="ghost" on_click={move || { let _ = r_fwd.forward(); }}>
                        "Forward"
                    </Button>
                    <Button
                        size="sm"
                        variant="secondary"
                        on_click={move || {
                            match open_secondary_window(
                                "RUI Gallery — Shell",
                                gallery_secondary_shell_root,
                            ) {
                                Ok(()) => {
                                    toast_push("Secondary shell opened", 2000);
                                }
                                Err(e) => {
                                    log::warn!("open_secondary_window: {e}");
                                    toast_push(format!("次窗失败: {e}"), 3000);
                                }
                            }
                        }}
                    >
                        "Open secondary"
                    </Button>
                </HStack>
                <Box w="100%" p="0.75rem" class="rounded-md" bg="muted">
                    <VStack gap="0.25rem">
                        <Label font_weight="600">{ format!("Route: {}", route.get()) }</Label>
                        { router.view() }
                        <Label text_size="0.75rem" color="muted-foreground">
                            "两页历史：Inbox → Settings → Back 应回到 Inbox。"
                        </Label>
                    </VStack>
                </Box>
            </VStack>
        </MyCard>
    }
}

thread_local! {
    static GAL_TRAY: RefCell<Option<rui::app::TrayIcon>> = const { RefCell::new(None) };
}

fn TrayShowcase() -> Element {
    let hint = if system_tray_native() {
        "feature system-tray 已开：Show tray 后任务栏出现图标；右键 Ping（toast）/ Quit（request_exit）。句柄须保持存活。"
    } else {
        "当前为 stub。原生接线：cargo run -p gallery --features system-tray"
    };
    ui! {
        <MyCard title="系统托盘（外壳）">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    { hint }
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <Button
                        id="gal_shell_tray_show"
                        size="sm"
                        variant="secondary"
                        on_click={move || {
                            GAL_TRAY.with(|slot| {
                                let mut g = slot.borrow_mut();
                                if g.is_none() {
                                    let tray = tray_icon()
                                        .tooltip("RUI Gallery")
                                        .menu_item(
                                            TrayMenuItem::new("ping", "Ping").on_click(|| {
                                                toast_push("Tray ping", 2000);
                                            }),
                                        )
                                        .menu_item(
                                            TrayMenuItem::new("quit", "Quit").on_click(|| {
                                                GAL_TRAY.with(|slot| {
                                                    drop(slot.borrow_mut().take());
                                                });
                                                request_exit();
                                            }),
                                        )
                                        .build();
                                    tray.show();
                                    *g = Some(tray);
                                    if system_tray_native() {
                                        toast_push("Tray shown", 2000);
                                    } else {
                                        toast_push(
                                            "Stub: rebuild with --features system-tray",
                                            3000,
                                        );
                                    }
                                } else if let Some(tray) = g.as_ref() {
                                    tray.show();
                                    toast_push("Tray shown", 2000);
                                }
                            });
                        }}
                    >
                        "Show tray"
                    </Button>
                    <Button
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            GAL_TRAY.with(|slot| {
                                if let Some(tray) = slot.borrow().as_ref() {
                                    tray.hide();
                                    toast_push("Tray hidden", 2000);
                                } else {
                                    toast_push("No tray yet", 2000);
                                }
                            });
                        }}
                    >
                        "Hide tray"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn gallery_chrome_fill(alpha: f32) -> String {
    let c = parse_color("background");
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        a
    )
}

fn gallery_custom_title_bar(title: &'static str) -> Element {
    let fill = gallery_chrome_fill(0.62);
    let mut drag: Element = ui! {
        <HStack flex_grow="1" h="100%" items="center" cursor="move">
            <Label font_weight="600" text_size="0.875rem">{ title }</Label>
        </HStack>
    };
    drag = drag.on_mousedown(move |_e| start_window_drag());
    ui! {
        <HStack w="100%" h="2.5rem" items="center" px="0.5rem" gap="0.25rem" bg={fill.as_str()}>
            { drag }
            <Button size="icon-xs" variant="ghost" on_click={move || set_window_minimized(true)}>
                <Icon name="minus" size="0.75rem" />
            </Button>
            <Button size="icon-xs" variant="ghost" on_click={move || toggle_window_maximized()}>
                <Icon name="square" size="0.75rem" />
            </Button>
            <Button size="icon-xs" variant="ghost" on_click={move || close_window()}>
                <Icon name="x" size="0.75rem" />
            </Button>
        </HStack>
    }
}

fn gallery_open_chrome(options: WindowOptions, root: fn() -> Element, ok: &str) {
    match open_secondary_window_with(options, root) {
        Ok(()) => {
            toast_push(ok, 2000);
        }
        Err(e) => {
            log::warn!("open_secondary_window_with: {e}");
            toast_push(format!("窗口失败: {e}"), 3000);
        }
    }
}

fn gallery_frameless_root() -> Element {
    ui! {
        <VStack w="100%" h="100%" bg="background">
            { gallery_custom_title_bar("Frameless") }
            <VStack flex_grow="1" p="1rem" gap="0.5rem">
                <Label font_weight="700">"无边框窗口"</Label>
                <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                    "OS 标题栏已关。拖标题栏移动；按钮最小化 / 最大化 / 关闭。关闭只关本窗。"
                </Label>
            </VStack>
        </VStack>
    }
}

fn gallery_transparent_root() -> Element {
    let card = gallery_chrome_fill(0.88);
    ui! {
        <VStack w="100%" h="100%" bg="transparent">
            { gallery_custom_title_bar("Transparent") }
            <VStack flex_grow="1" p="1rem" items="center" justify="center">
                <VStack w="18rem" p="1rem" gap="0.5rem" rounded="0.75rem" bg={card.as_str()}>
                    <Label font_weight="700">"透明窗口"</Label>
                    <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                        "根为 transparent，卡片半透明。应能透过空隙看到桌面。"
                    </Label>
                </VStack>
            </VStack>
        </VStack>
    }
}

fn gallery_blur_root() -> Element {
    let fill = gallery_chrome_fill(0.58);
    ui! {
        <VStack w="100%" h="100%" bg={fill.as_str()}>
            { gallery_custom_title_bar("Acrylic / blur") }
            <VStack flex_grow="1" p="1rem" gap="0.5rem">
                <Label font_weight="700">"透明 + 系统模糊"</Label>
                <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                    "Win11 Acrylic；macOS vibrancy；Wayland kwin blur。半透明填色才能看到材质。"
                </Label>
            </VStack>
        </VStack>
    }
}

fn WindowChromeShowcase() -> Element {
    ui! {
        <MyCard title="无边框 / 透明 / 模糊（外壳）">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "次窗演示 WindowOptions：decorations(false)、transparent、blur。自定义标题栏 mousedown 调 start_window_drag。"
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <Button
                        id="gal_shell_frameless"
                        size="sm"
                        variant="secondary"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("RUI Gallery — Frameless")
                                    .size(420.0, 280.0)
                                    .frameless(),
                                gallery_frameless_root,
                                "Frameless opened",
                            );
                        }}
                    >
                        "Open frameless"
                    </Button>
                    <Button
                        id="gal_shell_transparent"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("RUI Gallery — Transparent")
                                    .size(440.0, 300.0)
                                    .frameless()
                                    .transparent(true),
                                gallery_transparent_root,
                                "Transparent opened",
                            );
                        }}
                    >
                        "Open transparent"
                    </Button>
                    <Button
                        id="gal_shell_blur"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("RUI Gallery — Blur")
                                    .size(460.0, 300.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true),
                                gallery_blur_root,
                                "Blur opened",
                            );
                        }}
                    >
                        "Open blur"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn PrimitivesShowcase() -> rui::core::tree::Element {
    let overlay_open = use_signal("prim_overlay_open", || false);
    let local_open = use_signal("prim_local_portal_open", || false);
    let trap_on = use_signal("prim_trap_on", || false);

    ui! {
        <VStack w="100%" gap="0">
            <MyCard title="11) Image · Portal · focus_trap · VirtualList">
                <Reactive id={PRIMITIVES_SHOWCASE_REACTIVE}>
                    <VStack gap="0.75rem" w="100%">
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "GPU Image：queue_image_rgba8 + texture，或 src（路径 / data:）。PNG/ICO/BMP 默认；JPEG/GIF/WebP 需 images；SVG 需 svg。Portal 默认同层；focus_trap 限制 Tab。VirtualList：visible_item_range。"
                        </Label>
                        <HStack gap="0.75rem" items="start" wrap="wrap">
                            <VStack gap="0.25rem">
                                <Label font_weight="700" text_size="0.75rem">"Texture quad"</Label>
                                <Image texture="demo-checker" w="5.5rem" h="5.5rem" rounded="0.375rem" />
                                <Image
                                    src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><circle cx='16' cy='16' r='14' fill='steelblue'/></svg>"
                                    w="5.5rem"
                                    h="5.5rem"
                                    class="rounded-full"
                                />
                            </VStack>
                            <VStack gap="0.35rem" flex_grow="1" min_w="12rem">
                                <Label font_weight="700" text_size="0.75rem">"Controls"</Label>
                                <HStack gap="0.5rem" wrap="wrap">
                                    <Button variant="outline" on_click={move || overlay_open.set(!overlay_open.get())}>
                                        { if overlay_open.get() { "关闭根 Portal" } else { "打开根 Portal" } }
                                    </Button>
                                    <Button variant="outline" on_click={move || local_open.set(!local_open.get())}>
                                        { if local_open.get() { "关闭局部 Portal" } else { "打开局部 Portal" } }
                                    </Button>
                                    <Button variant="secondary" on_click={move || {
                                        let next = !trap_on.get();
                                        trap_on.set(next);
                                        rui::sync_focus_trap("prim-trap", next, "trap-a,trap-b");
                                    }}>
                                        { if trap_on.get() { "关闭焦点陷阱" } else { "启用焦点陷阱" } }
                                    </Button>
                                </HStack>
                                <Box id="prim-portal-host" relative=true w="100%" min_h="6.5rem" p="0.75rem" bg="muted" rounded="0.5rem" overflow="hidden">
                                    <Label text_size="0.75rem" color="muted-foreground">
                                        "局部宿主 prim-portal-host（mount 叠层裁剪在此框内）"
                                    </Label>
                                </Box>
                                <HStack gap="0.5rem" items="center" wrap="wrap">
                                    <Label text_size="0.75rem" color="muted-foreground">"陷阱内 Tab:"</Label>
                                    <Button id="trap-a" variant="default">"A"</Button>
                                    <Button id="trap-b" variant="outline">"B"</Button>
                                </HStack>
                            </VStack>
                        </HStack>
                        <Label id="gal-s11-virt-title" font_weight="700" text_size="0.875rem">"虚拟列表窗口（200 行，仅挂载可见区间）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.25rem" w="100%">
                            { Element::from(
                                virtual_list(PRIMITIVES_VIRT)
                                    .item_count(200)
                                    .item_height(22.0)
                                    .height(176.0)
                                    .overscan(8)
                                    .scrollbar_visibility("always")
                                    .render_item(|i| {
                                        label(&format!("virt-row-{i}"))
                                            .text(format!("virtual row {i:04}"))
                                            .into()
                                    }),
                            ) }
                        </Box>
                    </VStack>
                </Reactive>
            </MyCard>
            <Portal id="prim-portal-root">
                <Reactive id="examples/gallery/PrimitivesShowcase:portal">
                    <Show when={overlay_open.get()}>
                        <Box absolute=true inset="0" w="100%" h="100%" items="center" justify="center" bg="black" z_index="900">
                            <VStack gap="0.75rem" p="1.25rem" bg="secondary" rounded="0.75rem" max_w="22rem" m="1rem">
                                <Label font_weight="700" text_size="1.125rem">"根 Portal 浮层"</Label>
                                <Label text_size="0.875rem" color="muted-foreground">
                                    "未指定 mount：提升到根叠层（对齐 Solid document.body）。"
                                </Label>
                                <Button variant="default" on_click={move || overlay_open.set(false)}>
                                    "关闭"
                                </Button>
                            </VStack>
                        </Box>
                    </Show>
                </Reactive>
            </Portal>
            <Portal id="prim-portal-local" mount="prim-portal-host">
                <Reactive id="examples/gallery/PrimitivesShowcase:portal_local">
                    <Show when={local_open.get()}>
                        <Box absolute=true inset="0" w="100%" h="100%" items="center" justify="center" bg="black" z_index="40">
                            <VStack gap="0.5rem" p="0.75rem" bg="secondary" rounded="0.5rem" m="0.5rem">
                                <Label font_weight="700" text_size="0.875rem">"局部 Portal"</Label>
                                <Label text_size="0.75rem" color="muted-foreground">
                                    "mount=\"prim-portal-host\"：挂在子容器叠层内。"
                                </Label>
                                <Button size="sm" variant="default" on_click={move || local_open.set(false)}>
                                    "关闭"
                                </Button>
                            </VStack>
                        </Box>
                    </Show>
                </Reactive>
            </Portal>
        </VStack>
    }
}

#[component]
fn TextareaComboboxShowcase() -> Element {
    let notes = use_signal("gal_textarea_notes", String::new);
    let wrap_soft = use_signal("gal_textarea_wrap_soft", || {
        "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px — soft wrap folds this long hard line."
            .to_string()
    });
    let wrap_off = use_signal("gal_textarea_wrap_off", || {
        "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px — wrap=off scrolls horizontally (no clip)."
            .to_string()
    });
    let wrap_char = use_signal("gal_textarea_wrap_char", || {
        "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😀 — char may mid-break."
            .to_string()
    });
    let wrap_bidi = use_signal("gal_textarea_wrap_bidi", || {
        "Hello עברית\u{2060}789 and العربية — soft wrap keeps WJ pairs.".to_string()
    });
    let combo = use_signal("gal_combo_city", || "sh".to_string());
    let tab_view = use_signal("gal_tabs_list_view", || "list".to_string());
    let align = use_signal("gal_toggle_align", || "left".to_string());

    ui! {
        <MyCard title="Textarea · Combobox · Tabs · ToggleGroup">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "多行输入、可过滤 Combobox、复合 Tabs（List / Trigger / Content）与 ToggleGroup。"
                </Label>
                <Textarea id="gal-textarea" placeholder="多行备注…（可拖右下角改高）" rows={4} value={notes} />
                <Textarea id="gal-textarea-fixed" placeholder="固定高度（resizable=false）" rows={3} resizable={false} />
                <Label text_size="0.875rem" leading="normal">{ format!("Textarea: {}", notes.get()) }</Label>
                <Label text_size="0.75rem" font_weight="600" leading="normal">
                    "Textarea wrap · word（默认） / char / off + BiDi"
                </Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "窄栏对照：word 按词折行；char 可中词断开；off 横向滚动。BiDi 样例验证 WJ。拖选出视口应持续跟手滚。"
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" items="start" wrap="wrap">
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"word\"（默认）"</Label>
                        <Textarea
                            id="gal-ta-wrap-soft"
                            wrap="word"
                            rows={5}
                            resizable={false}
                            value={wrap_soft}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"char\""</Label>
                        <Textarea
                            id="gal-ta-wrap-char"
                            wrap="char"
                            rows={5}
                            resizable={false}
                            value={wrap_char}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"off\"（横向滚动）"</Label>
                        <Textarea
                            id="gal-ta-wrap-off"
                            wrap="off"
                            rows={5}
                            resizable={false}
                            value={wrap_off}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"BiDi + WJ（word）"</Label>
                        <Textarea
                            id="gal-ta-wrap-bidi"
                            wrap="word"
                            rows={5}
                            resizable={false}
                            value={wrap_bidi}
                        />
                    </VStack>
                </HStack>
                                <Combobox id="gal-combo" value={combo} placeholder="搜索城市…" clearable={true} class="w-full">
                    <ComboboxInput />
                    <ComboboxContent max_h="16rem">
                        <ComboboxList>
                            <ComboboxGroup>
                                <ComboboxLabel>"East China"</ComboboxLabel>
                                <ComboboxItem value="sh">"Shanghai"</ComboboxItem>
                                <ComboboxItem value="hz">"Hangzhou"</ComboboxItem>
                            </ComboboxGroup>
                            <ComboboxSeparator />
                            <ComboboxGroup>
                                <ComboboxLabel>"South China"</ComboboxLabel>
                                <ComboboxItem value="sz">"Shenzhen"</ComboboxItem>
                                <ComboboxItem value="gz">"Guangzhou"</ComboboxItem>
                            </ComboboxGroup>
                            <ComboboxSeparator />
                            <ComboboxItem value="bj">"Beijing"</ComboboxItem>
                        </ComboboxList>
                        <ComboboxEmpty>"No results."</ComboboxEmpty>
                    </ComboboxContent>
                </Combobox>
                <Label text_size="0.875rem">{ format!("Combobox: {}", combo.get()) }</Label>
                <Tabs id="gal-tabs" value={tab_view} default_value="list">
                    <TabsList>
                        <TabsTrigger value="list">"List"</TabsTrigger>
                        <TabsTrigger value="grid">"Grid"</TabsTrigger>
                        <TabsTrigger value="map">"Map"</TabsTrigger>
                    </TabsList>
                    <TabsContent value="list">
                        <Label text_size="0.875rem" leading="normal">"List panel — click a tab or use ←/→ then Enter."</Label>
                    </TabsContent>
                    <TabsContent value="grid">
                        <Label text_size="0.875rem" leading="normal">"Grid panel"</Label>
                    </TabsContent>
                    <TabsContent value="map">
                        <Label text_size="0.875rem" leading="normal">"Map panel"</Label>
                    </TabsContent>
                </Tabs>
                <Label text_size="0.875rem">{ format!("Tabs: {}", tab_view.get()) }</Label>
                <Tabs id="gal-tabs-line" default_value="one">
                    <TabsList variant="line">
                        <TabsTrigger value="one">"One"</TabsTrigger>
                        <TabsTrigger value="two">"Two"</TabsTrigger>
                    </TabsList>
                    <TabsContent value="one">
                        <Label text_size="0.75rem" color="muted-foreground">"line variant"</Label>
                    </TabsContent>
                    <TabsContent value="two">
                        <Label text_size="0.75rem" color="muted-foreground">"second line tab"</Label>
                    </TabsContent>
                </Tabs>
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

/// 浮层页默认通知：与「Toast」按钮同一 Toaster 根、stack 布局、入场动画。
/// 每次推送前恢复 host 默认（避免点过 Wait/Displace 后 limit=2 / wait 残留）。
fn gallery_feedback_toast(title: impl Into<String>, description: impl Into<String>) {
    toast_set_layout("stack");
    toast_set_overflow("displace");
    toast_set_limit(3);
    toast(title).description(description).duration(5000).show();
}

fn gallery_goto_section(key: &str) {
    use_signal("gal_nav_section", || "basics".to_string()).set(key.to_string());
}

#[component]
fn OverlayFeedbackShowcase() -> Element {
    let cmd_open = use_signal("gal_cmd_open", || false);
    let cmd_dlg_open = use_signal("gal_cmd_dlg_open", || false);
    let dialog_open = use_signal("gal_dialog_open", || false);
    let dialog_select = use_signal("gal_dialog_select", || "opt-a".to_string());
    let sheet_open = use_signal("gal_sheet_open", || false);
    let sheet_name = use_signal("gal_sheet_name", || "Pedro Duarte".to_string());
    let sheet_user = use_signal("gal_sheet_user", || "@peduarte".to_string());
    let drawer_open = use_signal("gal_drawer_open", || false);
    let drawer_link = use_signal("gal_drawer_link", || "https://ui.shadcn.com".to_string());
    let progress_val = use_signal("gal_progress", || 0.45f32);
    let open_cmd = cmd_open;
    let open_cmd_dlg = cmd_dlg_open;
    let bump = progress_val;

    let cmd_el: Element = ui! {
        <CommandOverlay id="gal-cmd" open={cmd_open}>
            <CommandInput placeholder="Type a command or search..." />
            <CommandList>
                <CommandEmpty>"No results found."</CommandEmpty>
                <CommandGroup heading="Suggestions">
                    <CommandItem
                        value="toast"
                        on_click={move || {
                            gallery_feedback_toast(
                                "Hello from Command",
                                "Hover the stack to expand.",
                            );
                        }}
                    >
                        "Show toast"
                    </CommandItem>
                    <CommandItem
                        value="log"
                        on_click={move || {
                            log::info!("gallery command: ping");
                        }}
                    >
                        "Log ping"
                    </CommandItem>
                </CommandGroup>
                <CommandSeparator />
                <CommandGroup heading="Settings">
                    <CommandItem value="profile">
                        "Profile"
                        <CommandShortcut>"⌘P"</CommandShortcut>
                    </CommandItem>
                    <CommandItem value="billing">
                        "Billing"
                        <CommandShortcut>"⌘B"</CommandShortcut>
                    </CommandItem>
                </CommandGroup>
            </CommandList>
        </CommandOverlay>
    };
    let cmd_dlg_el: Element = ui! {
        <CommandDialog id="gal-cmd-dlg" open={cmd_dlg_open}>
            <CommandInput placeholder="Type a command or search..." />
            <CommandList>
                <CommandEmpty>"No results found."</CommandEmpty>
                <CommandGroup heading="Suggestions">
                    <CommandItem
                        value="dlg-toast"
                        on_click={move || {
                            gallery_feedback_toast(
                                "Hello from CommandDialog",
                                "Hover the stack to expand.",
                            );
                        }}
                    >
                        "Show toast"
                    </CommandItem>
                    <CommandItem
                        value="calendar"
                        on_click={move || {
                            gallery_goto_section("forms");
                            gallery_feedback_toast(
                                "Calendar",
                                "已跳到「表单」：Date / Time / Color / Calendar。",
                            );
                        }}
                    >
                        "Calendar"
                    </CommandItem>
                    <CommandItem
                        value="emoji"
                        on_click={move || {
                            gallery_goto_section("text");
                            gallery_feedback_toast("Search Emoji", "已跳到「文本」分区。");
                        }}
                    >
                        "Search Emoji"
                    </CommandItem>
                    <CommandItem value="calc" disabled={true}>"Calculator"</CommandItem>
                </CommandGroup>
                <CommandSeparator />
                <CommandGroup heading="Settings">
                    <CommandItem
                        value="profile"
                        on_click={move || {
                            gallery_feedback_toast("Profile", "CommandDialog 演示项。");
                        }}
                    >
                        "Profile"
                        <CommandShortcut>"⌘P"</CommandShortcut>
                    </CommandItem>
                    <CommandItem
                        value="billing"
                        on_click={move || {
                            gallery_feedback_toast("Billing", "CommandDialog 演示项。");
                        }}
                    >
                        "Billing"
                        <CommandShortcut>"⌘B"</CommandShortcut>
                    </CommandItem>
                    <CommandItem
                        value="settings"
                        on_click={move || {
                            gallery_feedback_toast("Settings", "CommandDialog 演示项。");
                        }}
                    >
                        "Settings"
                        <CommandShortcut>"⌘S"</CommandShortcut>
                    </CommandItem>
                </CommandGroup>
                <CommandSeparator />
                <CommandGroup heading="File">
                    <CommandItem value="new-file">
                        "New File"
                        <CommandShortcut>"⌘N"</CommandShortcut>
                    </CommandItem>
                    <CommandItem value="open">
                        "Open..."
                        <CommandShortcut>"⌘O"</CommandShortcut>
                    </CommandItem>
                    <CommandItem
                        value="save"
                        on_click={move || {
                            toast("Saved")
                                .description("Changes applied.")
                                .variant("success")
                                .duration(2500)
                                .show();
                        }}
                    >
                        "Save"
                        <CommandShortcut>"⌘S"</CommandShortcut>
                    </CommandItem>
                </CommandGroup>
            </CommandList>
        </CommandDialog>
    };
    let cmd_inline: Element = ui! {
        <Command id="gal-cmd-inline">
            <CommandInput placeholder="Type a command or search..." />
            <CommandList>
                <CommandEmpty>"No results found."</CommandEmpty>
                <CommandGroup heading="Suggestions">
                    <CommandItem value="calendar">"Calendar"</CommandItem>
                    <CommandItem value="emoji">"Search Emoji"</CommandItem>
                    <CommandItem value="calc" disabled={true}>"Calculator"</CommandItem>
                </CommandGroup>
                <CommandSeparator />
                <CommandGroup heading="Settings">
                    <CommandItem value="inline-profile">
                        "Profile"
                        <CommandShortcut>"⌘P"</CommandShortcut>
                    </CommandItem>
                </CommandGroup>
            </CommandList>
        </Command>
    };
    let dialog_el: Element = {
        let open = dialog_open;
        let nested_select = dialog_select;
        ui! {
            <Dialog id="gal-dialog" open={open}>
                <DialogTrigger>
                    <Button variant="outline" size="sm">Dialog</Button>
                </DialogTrigger>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>Gallery Dialog</DialogTitle>
                        <DialogDescription>
                            "desktop_smoke #7：Body 内含 Select；先开下拉再 Esc，应只关顶层浮层。"
                        </DialogDescription>
                    </DialogHeader>
                    <DialogBody>
                        <VStack gap="0.5rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">
                                "嵌套浮层（Select）"
                            </Label>
                            <Select
                                id="gal-dialog-nested-select"
                                value={nested_select}
                                placeholder="Pick an option"
                                class="w-full"
                            >
                                <SelectTrigger class="w-full">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent max_h="12rem">
                                    <SelectItem value="opt-a">"Option A"</SelectItem>
                                    <SelectItem value="opt-b">"Option B"</SelectItem>
                                    <SelectSeparator />
                                    <SelectGroup>
                                        <SelectLabel>"More"</SelectLabel>
                                        <SelectItem value="opt-c">"Option C"</SelectItem>
                                        <SelectItem value="opt-d">"Option D — long label"</SelectItem>
                                        <SelectItem value="opt-e">"Option E"</SelectItem>
                                    </SelectGroup>
                                </SelectContent>
                            </Select>
                            <Label text_size="0.75rem" color="muted-foreground">
                                { format!("Selected: {}", nested_select.get()) }
                            </Label>
                            <Label text_size="0.875rem">"行 1 — 演示长内容"</Label>
                            <Label text_size="0.875rem">"行 2"</Label>
                            <Label text_size="0.875rem">"行 3"</Label>
                            <Label text_size="0.875rem">"行 4"</Label>
                            <Label text_size="0.875rem">"行 5"</Label>
                            <Label text_size="0.875rem">"行 6"</Label>
                        </VStack>
                    </DialogBody>
                    <DialogFooter>
                        <Button
                            variant="outline"
                            size="sm"
                            on_click={move || open.set(false)}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="default"
                            size="sm"
                            on_click={move || {
                                toast_push("Dialog confirmed", 1500);
                                open.set(false);
                            }}
                        >
                            Confirm
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        }
    };
    let sheet_el: Element = {
        let open = sheet_open;
        let name = sheet_name;
        let user = sheet_user;
        ui! {
            <Sheet id="gal-sheet" open={open}>
                <SheetTrigger>
                    <Button variant="outline" size="sm">Sheet</Button>
                </SheetTrigger>
                <SheetContent side="right" w="24rem">
                    <SheetHeader>
                        <SheetTitle>Edit profile</SheetTitle>
                        <SheetDescription>
                            "Make changes to your profile here. Click save when you're done."
                        </SheetDescription>
                    </SheetHeader>
                    <SheetBody>
                        <VStack gap="0.75rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">"Name"</Label>
                            <Input id="gal-sheet-name" value={name} class="w-full" />
                            <Label text_size="0.875rem" font_weight="600">"Username"</Label>
                            <Input id="gal-sheet-user" value={user} class="w-full" />
                        </VStack>
                    </SheetBody>
                    <SheetFooter>
                        <Button
                            size="sm"
                            on_click={move || {
                                toast_push("Profile saved", 1500);
                                open.set(false);
                            }}
                        >
                            "Save changes"
                        </Button>
                        <SheetClose>
                            <Button variant="outline" size="sm">Close</Button>
                        </SheetClose>
                    </SheetFooter>
                </SheetContent>
            </Sheet>
        }
    };
    let drawer_el: Element = {
        let open = drawer_open;
        let link = drawer_link;
        ui! {
            <Drawer id="gal-drawer" open={open}>
                <DrawerTrigger>
                    <Button variant="outline" size="sm">Drawer</Button>
                </DrawerTrigger>
                <DrawerContent
                    direction="down"
                    show_swipe_handle={true}
                    snap_points={[0.5_f32, 1.0]}
                    snap_to_sequential_point={true}
                >
                    <DrawerHeader>
                        <DrawerTitle>Share link</DrawerTitle>
                        <DrawerDescription>
                            "Anyone with the link can view this document."
                        </DrawerDescription>
                    </DrawerHeader>
                    <DrawerBody>
                        <VStack gap="0.5rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">"Link"</Label>
                            <Input id="gal-drawer-link" value={link} class="w-full" />
                            <Label text_size="0.875rem" leading="relaxed">
                                "Half-open snap shortens the panel — Body scrolls when content overflows."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Duis aute irure dolor in reprehenderit in voluptate velit esse."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Excepteur sint occaecat cupidatat non proident, sunt in culpa."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Mollit anim id est laborum. Drag the handle to snap or dismiss."
                            </Label>
                        </VStack>
                    </DrawerBody>
                    <DrawerFooter>
                        <Button
                            size="sm"
                            on_click={move || {
                                toast_push("Link copied", 1500);
                                open.set(false);
                            }}
                        >
                            "Copy link"
                        </Button>
                        <DrawerClose>
                            <Button variant="outline" size="sm">Close</Button>
                        </DrawerClose>
                    </DrawerFooter>
                </DrawerContent>
            </Drawer>
        }
    };
    let sheet_sides_el: Element = {
        let open_top = use_signal("gal_sheet_top", || false);
        let open_right = use_signal("gal_sheet_right", || false);
        let open_bottom = use_signal("gal_sheet_bottom", || false);
        let open_left = use_signal("gal_sheet_left", || false);
        let o_top = open_top;
        let o_right = open_right;
        let o_bottom = open_bottom;
        let o_left = open_left;
        ui! {
            <HStack gap="0.5rem" wrap="wrap" items="center">
                <Sheet id="gal-sheet-top" open={open_top}>
                    <SheetTrigger>
                        <Button variant="outline" size="sm">"top"</Button>
                    </SheetTrigger>
                    <SheetContent side="top">
                        <SheetHeader>
                            <SheetTitle>"Sheet · top"</SheetTitle>
                            <SheetDescription>
                                "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                            </SheetDescription>
                        </SheetHeader>
                        <SheetBody>
                            <VStack gap="0.5rem" w="100%">
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Duis aute irure dolor in reprehenderit in voluptate velit."
                                </Label>
                            </VStack>
                        </SheetBody>
                        <SheetFooter>
                            <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_top.set(false); }}>
                                "Save changes"
                            </Button>
                            <SheetClose>
                                <Button variant="outline" size="sm">Cancel</Button>
                            </SheetClose>
                        </SheetFooter>
                    </SheetContent>
                </Sheet>
                <Sheet id="gal-sheet-right" open={open_right}>
                    <SheetTrigger>
                        <Button variant="outline" size="sm">"right"</Button>
                    </SheetTrigger>
                    <SheetContent side="right">
                        <SheetHeader>
                            <SheetTitle>"Sheet · right"</SheetTitle>
                            <SheetDescription>
                                "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                            </SheetDescription>
                        </SheetHeader>
                        <SheetBody>
                            <VStack gap="0.5rem" w="100%">
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Duis aute irure dolor in reprehenderit in voluptate velit."
                                </Label>
                            </VStack>
                        </SheetBody>
                        <SheetFooter>
                            <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_right.set(false); }}>
                                "Save changes"
                            </Button>
                            <SheetClose>
                                <Button variant="outline" size="sm">Cancel</Button>
                            </SheetClose>
                        </SheetFooter>
                    </SheetContent>
                </Sheet>
                <Sheet id="gal-sheet-bottom" open={open_bottom}>
                    <SheetTrigger>
                        <Button variant="outline" size="sm">"bottom"</Button>
                    </SheetTrigger>
                    <SheetContent side="bottom">
                        <SheetHeader>
                            <SheetTitle>"Sheet · bottom"</SheetTitle>
                            <SheetDescription>
                                "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                            </SheetDescription>
                        </SheetHeader>
                        <SheetBody>
                            <VStack gap="0.5rem" w="100%">
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Duis aute irure dolor in reprehenderit in voluptate velit."
                                </Label>
                            </VStack>
                        </SheetBody>
                        <SheetFooter>
                            <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_bottom.set(false); }}>
                                "Save changes"
                            </Button>
                            <SheetClose>
                                <Button variant="outline" size="sm">Cancel</Button>
                            </SheetClose>
                        </SheetFooter>
                    </SheetContent>
                </Sheet>
                <Sheet id="gal-sheet-left" open={open_left}>
                    <SheetTrigger>
                        <Button variant="outline" size="sm">"left"</Button>
                    </SheetTrigger>
                    <SheetContent side="left">
                        <SheetHeader>
                            <SheetTitle>"Sheet · left"</SheetTitle>
                            <SheetDescription>
                                "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                            </SheetDescription>
                        </SheetHeader>
                        <SheetBody>
                            <VStack gap="0.5rem" w="100%">
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                </Label>
                                <Label text_size="0.875rem" leading="relaxed">
                                    "Duis aute irure dolor in reprehenderit in voluptate velit."
                                </Label>
                            </VStack>
                        </SheetBody>
                        <SheetFooter>
                            <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_left.set(false); }}>
                                "Save changes"
                            </Button>
                            <SheetClose>
                                <Button variant="outline" size="sm">Cancel</Button>
                            </SheetClose>
                        </SheetFooter>
                    </SheetContent>
                </Sheet>
            </HStack>
        }
    };
    let tip: Element = ui! {
        <Tooltip id="gal-tip">
            <TooltipTrigger>
                <Button variant="outline" size="sm">
                    "Hover for tooltip"
                </Button>
            </TooltipTrigger>
            <TooltipContent>
                "Hover tip"
            </TooltipContent>
        </Tooltip>
    };
    let hover_card_el: Element = {
        let hc_open = use_signal("gal_hover_card_open", || false);
        let hc_builder_open = use_signal("gal_hover_card_builder_open", || false);
        let rsx: Element = ui! {
            <HoverCard id="gallery:hover_card" open={hc_open}>
                <HoverCardTrigger openDelay=10 closeDelay=100>
                    <Button variant="link" size="sm">"@nextjs"</Button>
                </HoverCardTrigger>
                <HoverCardContent side="bottom" align="center" sideOffset=4 class="w-64">
                    <HStack gap="0.75rem" items="start" w="100%">
                        <Avatar size="lg">
                            <AvatarFallback>"NX"</AvatarFallback>
                        </Avatar>
                        <VStack gap="0.125rem" min_w="0" flex_grow={1.0}>
                            <Label font_weight=600 text_size="0.875">"@nextjs"</Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "The React Framework – created and maintained by @vercel."
                            </Label>
                            <Label text_size="0.75rem" color="muted-foreground">
                                "Joined December 2021"
                            </Label>
                        </VStack>
                    </HStack>
                </HoverCardContent>
            </HoverCard>
        };
        let builder: Element = hover_card("gallery:hover_card_builder")
            .open_signal(hc_builder_open)
            .child(|| {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    HoverCardTrigger(HoverCardTriggerProps {
                        id: String::new(),
                        open_delay: Some(10),
                        close_delay: Some(100),
                        children: Some(
                            button("gallery:hover_card_builder_btn")
                                .variant("outline")
                                .size("sm")
                                .text("HoverCard Builder")
                                .into(),
                        ),
                        base: None,
                    }),
                    hover_card_content(
                        HoverCardContentOpts {
                            side: "bottom".into(),
                            align: "center".into(),
                            side_offset: 4.0,
                            class: Some("w-64".into()),
                            width: None,
                        },
                        || {
                            Element::from(
                                vstack()
                                    .gap("0.25rem")
                                    .child(
                                        label("gal-hc-b-title")
                                            .text("@rui")
                                            .size(14.0)
                                            .font_weight(600.0),
                                    )
                                    .child(
                                        label("gal-hc-b-body")
                                            .text("Same surface as RSX HoverCardContent.")
                                            .size(14.0),
                                    ),
                            )
                        },
                    ),
                ])
            });
        ui! {
            <HStack gap="0.75rem" wrap="wrap" items="center">
                { rsx }
                { builder }
            </HStack>
        }
    };
    let dd: Element = ui! {
        <DropdownMenu id="gal-dd">
            <DropdownMenuTrigger>
                <Button variant="outline" size="sm">
                    "DropdownMenu"
                </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent>
                <DropdownMenuLabel>"Actions"</DropdownMenuLabel>
                <DropdownMenuItem on_click={|| {
                    toast_push("Profile", 1500);
                }}>
                    "Profile"
                    <DropdownMenuShortcut>"⇧⌘P"</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuItem on_click={|| {
                    toast_push("Billing", 1500);
                }}>
                    "Billing"
                    <DropdownMenuShortcut>"⌘B"</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuCheckboxItem defaultChecked={true} on_click={|| {
                    gallery_feedback_toast("Status Bar toggled", "DropdownMenu CheckboxItem");
                }}>
                    "Status Bar"
                </DropdownMenuCheckboxItem>
                <DropdownMenuCheckboxItem on_click={|| {
                    gallery_feedback_toast("Activity Bar toggled", "DropdownMenu CheckboxItem");
                }}>
                    "Activity Bar"
                </DropdownMenuCheckboxItem>
                <DropdownMenuSeparator />
                <DropdownMenuRadioGroup defaultValue="bottom">
                    <DropdownMenuLabel>"Panel Position"</DropdownMenuLabel>
                    <DropdownMenuRadioItem value="top" on_click={|| {
                        gallery_feedback_toast("Panel: Top", "DropdownMenu RadioItem");
                    }}>
                        "Top"
                    </DropdownMenuRadioItem>
                    <DropdownMenuRadioItem value="bottom" on_click={|| {
                        gallery_feedback_toast("Panel: Bottom", "DropdownMenu RadioItem");
                    }}>
                        "Bottom"
                    </DropdownMenuRadioItem>
                    <DropdownMenuRadioItem value="right" on_click={|| {
                        gallery_feedback_toast("Panel: Right", "DropdownMenu RadioItem");
                    }}>
                        "Right"
                    </DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
                <DropdownMenuSeparator />
                <DropdownMenuSub>
                    <DropdownMenuSubTrigger>"Invite users"</DropdownMenuSubTrigger>
                    <DropdownMenuSubContent>
                        <DropdownMenuItem on_click={|| {
                            toast_push("Email…", 1500);
                        }}>
                            "Email…"
                        </DropdownMenuItem>
                        <DropdownMenuItem on_click={|| {
                            toast_push("Message…", 1500);
                        }}>
                            "Message…"
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem on_click={|| {
                            toast_push("More…", 1500);
                        }}>
                            "More…"
                        </DropdownMenuItem>
                    </DropdownMenuSubContent>
                </DropdownMenuSub>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" on_click={|| {
                    toast_push("Log out", 1500);
                }}>
                    "Log out"
                    <DropdownMenuShortcut>"⇧⌘Q"</DropdownMenuShortcut>
                </DropdownMenuItem>
            </DropdownMenuContent>
        </DropdownMenu>
    };
    let ctx: Element = ui! {
        <ContextMenu id="gal-ctx">
            <ContextMenuTrigger>
                <Box p="0.75rem" bg="muted" class="rounded-md" w="100%">
                    <Label text_size="0.875rem">{ "右键此区域打开 ContextMenu" }</Label>
                </Box>
            </ContextMenuTrigger>
            <ContextMenuContent>
                <ContextMenuLabel>"Actions"</ContextMenuLabel>
                <ContextMenuItem on_click={|| {
                    toast_push("Copy", 1500);
                }}>
                    "Copy"
                    <ContextMenuShortcut>"⌘C"</ContextMenuShortcut>
                </ContextMenuItem>
                <ContextMenuItem on_click={|| {
                    toast_push("Paste", 1500);
                }}>
                    "Paste"
                    <ContextMenuShortcut>"⌘V"</ContextMenuShortcut>
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuCheckboxItem defaultChecked={true} on_click={|| {
                    gallery_feedback_toast("Show Bookmarks Bar toggled", "ContextMenu CheckboxItem");
                }}>
                    "Show Bookmarks Bar"
                </ContextMenuCheckboxItem>
                <ContextMenuCheckboxItem on_click={|| {
                    gallery_feedback_toast("Show Full URLs toggled", "ContextMenu CheckboxItem");
                }}>
                    "Show Full URLs"
                </ContextMenuCheckboxItem>
                <ContextMenuSeparator />
                <ContextMenuRadioGroup defaultValue="pedro">
                    <ContextMenuLabel>"People"</ContextMenuLabel>
                    <ContextMenuRadioItem value="pedro" on_click={|| {
                        gallery_feedback_toast("Pedro Duarte", "ContextMenu RadioItem");
                    }}>
                        "Pedro Duarte"
                    </ContextMenuRadioItem>
                    <ContextMenuRadioItem value="colm" on_click={|| {
                        gallery_feedback_toast("Colm Tuite", "ContextMenu RadioItem");
                    }}>
                        "Colm Tuite"
                    </ContextMenuRadioItem>
                </ContextMenuRadioGroup>
                <ContextMenuSeparator />
                <ContextMenuSub>
                    <ContextMenuSubTrigger>"More Tools"</ContextMenuSubTrigger>
                    <ContextMenuSubContent>
                        <ContextMenuItem on_click={|| {
                            toast_push("Save Page As…", 1500);
                        }}>
                            "Save Page As…"
                            <ContextMenuShortcut>"⇧⌘S"</ContextMenuShortcut>
                        </ContextMenuItem>
                        <ContextMenuItem on_click={|| {
                            toast_push("Create Shortcut…", 1500);
                        }}>
                            "Create Shortcut…"
                        </ContextMenuItem>
                        <ContextMenuItem on_click={|| {
                            toast_push("Name Window…", 1500);
                        }}>
                            "Name Window…"
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem on_click={|| {
                            toast_push("Developer Tools", 1500);
                        }}>
                            "Developer Tools"
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem variant="destructive" on_click={|| {
                            toast_push("Delete", 1500);
                        }}>
                            "Delete"
                            <ContextMenuShortcut>"⌘⌫"</ContextMenuShortcut>
                        </ContextMenuItem>
                    </ContextMenuSubContent>
                </ContextMenuSub>
                <ContextMenuSeparator />
                <ContextMenuItem variant="destructive" on_click={|| {
                    toast_push("Delete", 1500);
                }}>
                    "Delete"
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    };
    let progress_el: Element = progress().value_signal(progress_val).into();
    let progress_vertical_el: Element = progress()
        .orientation("vertical")
        .value_signal(progress_val)
        .into();

    ui! {
        <MyCard title="Command · Dialog · Sheet · Drawer · Toast · Tooltip · HoverCard · Dropdown · Progress">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "浮层与反馈：Command 顶部居中浮层 / CommandDialog 模态、Dialog、Sheet（复合 Trigger/Content/Side）、Drawer（底滑 + 拖柄）、Toast、Tooltip、HoverCard（悬停预览）、DropdownMenu、进度条。desktop_smoke #7/#8：Dialog+Select 嵌套 Esc；次窗按钮。"
                </Label>
                <HStack gap="0.5rem" wrap="wrap" items="center">
                    <Button variant="secondary" size="sm" on_click={move || open_cmd.set(true)}>
                        "Command"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || open_cmd_dlg.set(true)}>
                        "Command Dialog"
                    </Button>
                    { dialog_el }
                    { sheet_el }
                    { drawer_el }
                    <Button variant="default" size="sm" on_click={move || {
                        gallery_feedback_toast(
                            "Toast from Gallery",
                            "Hover the stack to expand.",
                        );
                    }}>
                        "Toast"
                    </Button>
                    <Button variant="secondary" size="sm" on_click={move || {
                        toast_set_layout("stack");
                        toast_set_overflow("displace");
                        toast_set_limit(3);
                        let _ = toast("First").description("Stack peek #1").duration(6000).show();
                        let _ = toast("Second").description("Stack peek #2").duration(6000).show();
                        let _ = toast("Third").description("Front of stack — hover to expand").duration(6000).show();
                    }}>
                        "Stack ×3"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast_set_layout("list");
                        toast_set_overflow("displace");
                        toast_set_limit(3);
                        let _ = toast("List toast").description("Single list item (bottom-right)").duration(5000).show();
                    }}>
                        "List"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast_set_layout("list");
                        toast_set_overflow("displace");
                        toast_set_limit(3);
                        let _ = toast("List A").description("Non-stacked list layout").duration(6000).show();
                        let _ = toast("List B").variant("success").duration(6000).show();
                        let _ = toast("List C").variant("warning").duration(6000).show();
                    }}>
                        "List ×3"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast_set_layout("stack");
                        toast_set_overflow("displace");
                        toast_set_limit(2);
                        let _ = toast("Keep").description("limit=2 displace").duration(8000).show();
                        let _ = toast("Stay").duration(8000).show();
                        let _ = toast("New").description("Oldest held, returns later").duration(8000).show();
                    }}>
                        "Displace"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast_set_layout("stack");
                        toast_set_overflow("wait");
                        toast_set_limit(2);
                        let _ = toast("Visible 1").description("limit=2 wait").duration(5000).show();
                        let _ = toast("Visible 2").duration(5000).show();
                        let _ = toast("Waiting A").description("Enters after a slot frees").duration(5000).show();
                        let _ = toast("Waiting B").duration(5000).show();
                    }}>
                        "Wait"
                    </Button>
                    <Button variant="secondary" size="sm" on_click={move || {
                        toast("Saved")
                            .description("Changes applied.")
                            .variant("success")
                            .duration(2500)
                            .show();
                    }}>
                        "Success"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast("FYI")
                            .description("Optional left icon is on by default.")
                            .variant("info")
                            .duration(2500)
                            .show();
                    }}>
                        "Info"
                    </Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        toast("Check network")
                            .variant("warning")
                            .duration(2500)
                            .show();
                    }}>
                        "Warning"
                    </Button>
                    <Button variant="destructive" size="sm" on_click={move || {
                        toast("Something failed")
                            .variant("destructive")
                            .duration(3000)
                            .show();
                    }}>
                        "Error"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast("No icon")
                            .description("Same success tone without the glyph.")
                            .variant("success")
                            .icon(false)
                            .duration(2500)
                            .show();
                    }}>
                        "Success · no icon"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("top-left");
                        toast("Top left").description("Swipe up to dismiss").duration(5000).show();
                    }}>
                        "↖"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("top-center");
                        toast("Top center").duration(5000).show();
                    }}>
                        "↑"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("top-right");
                        toast("Top right").duration(5000).show();
                    }}>
                        "↗"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("bottom-left");
                        toast("Bottom left").description("Swipe down to dismiss").duration(5000).show();
                    }}>
                        "↙"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("bottom-center");
                        toast("Bottom center").duration(5000).show();
                    }}>
                        "↓"
                    </Button>
                    <Button variant="ghost" size="sm" on_click={move || {
                        toast_set_position("bottom-right");
                        toast("Bottom right").duration(5000).show();
                    }}>
                        "↘"
                    </Button>
                    <Button
                        variant="secondary"
                        size="sm"
                        on_click={move || {
                            match open_secondary_window("RUI Gallery — Secondary", gallery_secondary_shell_root) {
                                Ok(()) => {
                                    toast_push("Secondary window opened", 2000);
                                }
                                Err(e) => {
                                    log::warn!("open_secondary_window: {e}");
                                    toast_push(format!("次窗失败: {e}"), 3000);
                                }
                            }
                        }}
                    >
                        "Secondary window"
                    </Button>
                    { tip }
                    { hover_card_el }
                    { dd }
                </HStack>
                { ctx }
                <VStack gap="0.5rem" w="100%">
                    <Label font_weight="600" text_size="0.875rem">{ "Progress" }</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "ShadCN 复合（Label / Value + Track）；value ∈ 0..=1；Bump 演示受控。"
                    </Label>
                    <Progress id="gal-progress" valueSignal={progress_val}>
                        <ProgressLabel>"Upload"</ProgressLabel>
                        <ProgressValue />
                    </Progress>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Builder 双写（简单条，无 Label）："
                    </Label>
                    { progress_el }
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "ProgressCircle（md / sm / lg，与上方共用 value_signal）："
                    </Label>
                    <HStack gap="1rem" items="center" wrap="wrap">
                        <ProgressCircle id="gal-progress-circle-md" valueSignal={progress_val} size={ProgressCircleSize::Md} />
                        <ProgressCircle id="gal-progress-circle-sm" valueSignal={progress_val} size={ProgressCircleSize::Sm} />
                        <ProgressCircle id="gal-progress-circle-lg" valueSignal={progress_val} size={ProgressCircleSize::Lg} />
                    </HStack>
                    <Separator />
                    <Label font_weight="600" text_size="0.875rem">{ "orientation=vertical" }</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "线条型 Progress 纵向：默认高 8rem，与 Bump 共用 value_signal。"
                    </Label>
                    <HStack gap="1.5rem" items="end" wrap="wrap">
                        <VStack gap="0.375rem" items="center">
                            <Label text_size="0.75rem" color="muted-foreground">{ "RSX" }</Label>
                            <Progress id="gal-progress-vertical" orientation="vertical" valueSignal={progress_val}>
                                <ProgressLabel>"Upload"</ProgressLabel>
                                <ProgressValue />
                            </Progress>
                        </VStack>
                        <VStack gap="0.375rem" items="center">
                            <Label text_size="0.75rem" color="muted-foreground">{ "Builder" }</Label>
                            { progress_vertical_el }
                        </VStack>
                    </HStack>
                    <Button variant="outline" size="sm" on_click={move || {
                        let v = bump.peek();
                        bump.set(if v >= 1.0 { 0.0 } else { (v + 0.1).min(1.0) });
                    }}>{ "Bump +10%" }</Button>
                </VStack>
                { cmd_inline }
                { cmd_el }
                { cmd_dlg_el }
                <VStack gap="0.5rem" w="100%">
                    <Label font_weight="600" text_size="0.875rem">{ "Sheet · sides" }</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "top / right / bottom / left 贴边；对齐 shadcn sheet-side。"
                    </Label>
                    { sheet_sides_el }
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn DrawingSvgShowcase() -> Element {
    let builder_el: Element = svg()
        .view_box("0 0 100 100")
        .w(160.0)
        .h(160.0)
        .child(
            circle()
                .cx(50.0)
                .cy(42.0)
                .r(28.0)
                .fill("chart-1")
                .stroke("foreground")
                .stroke_width(4.0),
        )
        .child(
            svg_path()
                .d("M18 82 L50 94 L82 82")
                .fill("none")
                .stroke("destructive")
                .stroke_width(4.0),
        )
        .into();

    ui! {
        <MyCard title="Svg · 矢量场景图（WP-3.11）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "公开 <svg> 叶：圆 fill=chart-1 + stroke=foreground；path 仅描边 destructive。Light 下 primary≡foreground，演示不用 primary 以免看不出描边。子形状不占 taffy。emoji-svg ≠ 本页。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "ui! 标签" }</Label>
                <svg viewBox="0 0 100 100" w="10rem" h="10rem">
                    <circle cx="50" cy="42" r="28" fill="chart-1" stroke="foreground" stroke_width="4" />
                    <path d="M18 82 L50 94 L82 82" fill="none" stroke="destructive" stroke_width="4" />
                </svg>
                <Label font_weight="600" text_size="0.875rem">{ "Builder" }</Label>
                { builder_el }
            </VStack>
        </MyCard>
    }
}

#[component]
fn LucideAnimatedShowcase() -> Element {
    let samples = [
        "heart",
        "bell",
        "check",
        "activity",
        "download",
        "copy",
        "menu",
        "settings",
        "search",
        "sun",
        "moon",
        "wifi",
        "battery",
        "smile",
        "frown",
        "bot",
        "align-left",
        "align-right",
        "grip",
        "grip-horizontal",
        "calendar-days",
        "chart-bar-increasing",
        "chart-bar-decreasing",
        "chart-line",
        "volume",
        "keyboard",
    ];
    let cells: Vec<Element> = samples
        .into_iter()
        .map(|name| {
            // Label defaults min_w=0; in a centered VStack it can shrink below the
            // glyph width and wrap mid-word ("activit"/"y"). Keep captions on one line.
            ui! {
                <VStack
                    items="center"
                    gap="0.4rem"
                    p="0.65rem"
                    min_w="6.5rem"
                    bg="secondary"
                    rounded="0.5rem"
                    borderStyle="solid"
                    borderColor="border"
                    borderWidth="0.0625"
                >
                    <AnimatedIcon name={name} size={32.0} />
                    <Label text_size="0.65rem" color="foreground" class="whitespace-nowrap">{ name }</Label>
                </VStack>
            }
        })
        .collect();
    // activity was previously the builder-only cell; keep a builder demo beside the grid.
    let builder_cell: Element = {
        let icon: Element = animated_icon("activity")
            .size_px(32.0)
            .color(parse_text_color("foreground"))
            .into();
        ui! {
            <VStack
                items="center"
                gap="0.4rem"
                p="0.65rem"
                min_w="6.5rem"
                bg="secondary"
                rounded="0.5rem"
                borderStyle="solid"
                borderColor="border"
                borderWidth="0.0625"
            >
                { icon }
                <Label text_size="0.65rem" color="foreground" class="whitespace-nowrap">{ "activity (builder)" }</Label>
            </VStack>
        }
    };

    ui! {
        <MyCard title="Lucide Animated · hover 播放">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "配方来自 pqoqubbw/icons（lucide-animated）。指针移入图标区域即可播放；字色跟主题 foreground。需 feature svg。"
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="start">
                    { cells }
                    { builder_cell }
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// Chaikin corner-cutting: two passes of subdivision produce a smooth C1 curve
/// that never overshoots the control polygon.  Anchors the first and last
/// points so the stroke starts and ends exactly at the pointer position.
fn chaikin_smooth(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut cur = pts.to_vec();
    for _ in 0..3 {
        let n = cur.len();
        let mut next = Vec::with_capacity(n * 2);
        next.push(cur[0]);
        for i in 0..n - 1 {
            let (ax, ay) = cur[i];
            let (bx, by) = cur[i + 1];
            next.push((0.75 * ax + 0.25 * bx, 0.75 * ay + 0.25 * by));
            next.push((0.25 * ax + 0.75 * bx, 0.25 * ay + 0.75 * by));
        }
        next.push(cur[n - 1]);
        cur = next;
    }
    cur
}

#[component]
fn DrawingCanvasShowcase() -> Element {
    let strokes = Rc::new(RefCell::new(Vec::<Vec<(f32, f32)>>::new()));
    let drawing = Rc::new(Cell::new(false));

    let builder_el: Element = canvas()
        .w(320.0)
        .h(200.0)
        .cursor("crosshair")
        .paint({
            let strokes = Rc::clone(&strokes);
            move |ctx, _size| {
                ctx.clear("secondary");
                ctx.set_stroke("foreground");
                ctx.set_line_width(2.5);
                for line in strokes.borrow().iter() {
                    if line.len() >= 2 {
                        ctx.stroke_path(&polyline(&chaikin_smooth(line)));
                    }
                }
                LayoutStatus::Clean
            }
        })
        .on_mousedown({
            let strokes = Rc::clone(&strokes);
            let drawing = Rc::clone(&drawing);
            move |e| {
                drawing.set(true);
                let p = (e.offset_x(), e.offset_y());
                strokes.borrow_mut().push(vec![p]);
            }
        })
        .on_mousemove({
            let strokes = Rc::clone(&strokes);
            let drawing = Rc::clone(&drawing);
            move |e| {
                if !drawing.get() {
                    return;
                }
                let p = (e.offset_x(), e.offset_y());
                let mut v = strokes.borrow_mut();
                if let Some(last) = v.last_mut() {
                    if last
                        .last()
                        .is_some_and(|q| (q.0 - p.0).hypot(q.1 - p.1) < 1.5)
                    {
                        return;
                    }
                    last.push(p);
                }
            }
        })
        .on_mouseup({
            let drawing = Rc::clone(&drawing);
            move |_| {
                drawing.set(false);
            }
        })
        .into();

    ui! {
        <MyCard title="Canvas · 自定义绘制（WP-3.12）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "公开 <canvas> 叶：paint 回调 + Canvas2D MVP。path 走同一 PathCommand 核。paint 内 peek，不订根 effect。Light 下演示 fill=chart-1 / stroke=foreground。右半 clip + 斜线 rotate（WP-3.13 2D）。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "ui! 标签：fill_rect + clip + rotate" }</Label>
                <canvas w={200.0} h={72.0} paint={move |ctx: &mut dyn Canvas2D, size: CanvasSize| {
                    ctx.clear("secondary");
                    ctx.set_fill("chart-1");
                    ctx.fill_rect(10.0, 10.0, size.w - 20.0, size.h - 20.0);
                    ctx.save();
                    ctx.clip_rect(size.w * 0.5, 8.0, size.w * 0.5 - 10.0, size.h - 16.0);
                    ctx.set_fill("chart-2");
                    ctx.fill_rect(10.0, 10.0, size.w - 20.0, size.h - 20.0);
                    ctx.restore();
                    ctx.set_stroke("foreground");
                    ctx.set_line_width(3.0);
                    ctx.stroke_rect(10.0, 10.0, size.w - 20.0, size.h - 20.0);
                    ctx.save();
                    ctx.translate(size.w * 0.5, size.h * 0.5);
                    ctx.rotate(-25.0);
                    ctx.set_stroke("destructive");
                    ctx.set_line_width(2.0);
                    ctx.stroke_path(&polyline(&[(-36.0, 0.0), (36.0, 0.0)]));
                    ctx.restore();
                    LayoutStatus::Clean
                }} />
                <Label font_weight="600" text_size="0.875rem">{ "Builder：指针拖线（mousedown / mousemove）" }</Label>
                { builder_el }
            </VStack>
        </MyCard>
    }
}

#[component]
fn DrawingScene3dShowcase() -> Element {
    let cube: Element = scene3d().w(220.0).h(160.0).label("RUI 3D").into();
    ui! {
        <MyCard title="Scene3d · 透视立方（WP-3.13）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "实验 feature scene3d（默认关）。同一 wgpu device，无 glTF / 无 three.js。启用：cargo run -p gallery --features scene3d。`.label(\"RUI 3D\")` → SDF bake（bake_text_texture_key_for_3d）经 mat4 投影成 billboard。"
                </Label>
                { cube }
            </VStack>
        </MyCard>
    }
}

fn gal_sample_tree_roots() -> Vec<TreeNode> {
    vec![TreeNode {
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
    }]
}

fn gal_tree_expanded(key: &'static str) -> Signal<HashSet<String>> {
    use_signal(key, || {
        let mut s = HashSet::new();
        s.insert("root".into());
        s.insert("src".into());
        s
    })
}

#[component]
fn DataChartShowcase() -> Element {
    let table_sel = use_signal("gal_dt_sel", || None::<usize>);
    let chart_sel = use_signal("gal_chart_sel", || None::<usize>);
    let expanded_col = gal_tree_expanded("gal_tree_exp_col");
    let expanded_flat = gal_tree_expanded("gal_tree_exp_flat");
    let expanded_rsx = gal_tree_expanded("gal_tree_exp_rsx");
    let sample_roots = gal_sample_tree_roots();

    let table_el: Element = table("gal-table")
        .columns([
            TableColumn::new("name", "Name").width(120.0),
            TableColumn::new("role", "Role").width(100.0),
            TableColumn::new("score", "Score").width(80.0),
        ])
        .row_count(40)
        .height(200.0)
        .selected_signal(table_sel)
        .cell(|r, c| match c {
            0 => format!("Item {r:02}"),
            1 => if r % 2 == 0 { "Admin" } else { "User" }.into(),
            _ => format!("{}", 60 + (r % 40)),
        })
        .into();

    let tree_collapsible_el: Element = tree_view("gal-tree-collapsible")
        .variant(TreeViewVariant::Collapsible)
        .exclusive(true)
        .height(160.0)
        .expanded_signal(expanded_col)
        .expanded_storage_key("gal_tree_exp_col")
        .roots(sample_roots.clone())
        .into();

    let tree_flat_el: Element = tree_view("gal-tree-flat")
        .variant("flat")
        .height(140.0)
        .row_height(32.0)
        .expanded_signal(expanded_flat)
        .expanded_storage_key("gal_tree_exp_flat")
        .roots(sample_roots)
        .into();

    let bar_rows = vec![
        ChartDatum::new("Mon", 0.2),
        ChartDatum::new("Tue", 0.5),
        ChartDatum::new("Wed", 0.8),
        ChartDatum::new("Thu", 0.4),
        ChartDatum::new("Fri", 0.9),
    ];
    let pie_rows = vec![
        ChartDatum::new("Alpha", 32.0),
        ChartDatum::new("Beta", 24.0),
        ChartDatum::new("Gamma", 18.0),
        ChartDatum::new("Delta", 14.0),
        ChartDatum::new("Other", 12.0),
    ];
    let line_vals = vec![0.1, 0.35, 0.55, 0.4, 0.85, 0.7];
    let compose_rows = vec![
        ChartDatum::new("Mon", 42.0),
        ChartDatum::new("Tue", 58.0),
        ChartDatum::new("Wed", 51.0),
        ChartDatum::new("Thu", 73.0),
        ChartDatum::new("Fri", 86.0),
    ];
    let composed: Element = define_chart()
        .id("gal-compose")
        .theme(ChartTheme::tokens())
        .mark(
            area_y(compose_rows.clone())
                .fill("chart-1")
                .fill_opacity(0.28)
                .stroke("chart-1")
                .stroke_width(2.0),
        )
        .mark(
            line_y(compose_rows.clone())
                .stroke("chart-1")
                .stroke_width(2.5),
        )
        .mark(dot(compose_rows.clone()).r(4.0).fill("chart-1"))
        .mark(
            rule_y([60.0])
                .stroke("destructive")
                .stroke_width(1.5)
                .stroke_dash([4.0, 2.0]),
        )
        .y_linear()
        .y_nice()
        .guides(true)
        .tooltip(true)
        .tooltip_indicator("line")
        .crosshair(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(160.0)
        .into();
    let t0 = 1_704_067_200_000i64;
    let time_rows = vec![
        ChartDatum::at(t0, 12.0),
        ChartDatum::at(t0 + 86_400_000, 19.0),
        ChartDatum::at(t0 + 2 * 86_400_000, 15.0),
        ChartDatum::at(t0 + 3 * 86_400_000, 28.0),
        ChartDatum::at(t0 + 4 * 86_400_000, 22.0),
    ];
    let time_chart: Element = define_chart()
        .id("gal-time")
        .mark(
            line_y(time_rows.clone())
                .stroke("chart-2")
                .stroke_width(2.5),
        )
        .mark(dot(time_rows).r(4.0).fill("chart-2"))
        .x_time()
        .y_nice()
        .tooltip(true)
        .crosshair(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(140.0)
        .into();
    let log_rows = vec![
        ChartDatum::new("1", 1.0),
        ChartDatum::new("2", 8.0),
        ChartDatum::new("3", 64.0),
        ChartDatum::new("4", 512.0),
    ];
    let log_chart: Element = define_chart()
        .id("gal-log")
        .mark(line_y(log_rows).stroke("chart-3").stroke_width(2.5))
        .y_log()
        .y_nice()
        .guides(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .h(120.0)
        .into();
    let radial_rows = vec![
        ChartDatum::new("N", 8.0),
        ChartDatum::new("NE", 5.0),
        ChartDatum::new("E", 7.0),
        ChartDatum::new("SE", 3.0),
        ChartDatum::new("S", 6.0),
        ChartDatum::new("SW", 4.0),
        ChartDatum::new("W", 9.0),
        ChartDatum::new("NW", 2.0),
    ];
    let radial_composed: Element = define_chart()
        .id("gal-radial")
        .mark(radial_bar(radial_rows.clone()).inner_radius(0.2))
        .mark(
            radial_line(radial_rows.clone())
                .stroke("chart-1")
                .stroke_width(2.0),
        )
        .mark(radial_dot(radial_rows.clone()).r(3.5).fill("chart-1"))
        .guides(true)
        .legend(true)
        .tooltip(true)
        .on_select(move |i| chart_sel.set(Some(i)))
        .size(180.0)
        .into();
    let donut_el: Element = donut_chart()
        .id("gal-donut")
        .data(pie_rows.clone())
        .size(160.0)
        .theme(ChartTheme::vibrant())
        .tooltip_indicator("dashed")
        .on_select(move |i| chart_sel.set(Some(i)))
        .into();
    let radial_preset: Element = radial_chart()
        .id("gal-radial-preset")
        .data(radial_rows)
        .size(160.0)
        .theme(ChartTheme::tokens())
        .on_select(move |i| chart_sel.set(Some(i)))
        .into();
    let carousel_el = ui! {
        <VStack w="100%" max_w="36rem" mx="auto" gap="1.25rem">
            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselSingle · autoplay" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "variant=single · autoplay=4000 · loop+seamless · dots · controls=outside"
                </Label>
                <CarouselSingle id="gal-carousel-single" controls="outside" autoplay={4000} loop={true} seamless={true}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Slide C" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselSingle>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · fraction peek" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "perView=2.2 · itemSizing=fraction · 首项左对齐 · 右侧裁切 · loop+seamless（无 autoplay 便于看初始态）"
                </Label>
                <CarouselMulti id="gal-carousel-multi-slide" perView={2.2} scrollBy="slide" loop={true} seamless={true} itemSizing="fraction">
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · fixed + free drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "itemSizing=fixed · itemWidth=12rem · snap=false · loop=false · 可随意拖拽"
                </Label>
                <CarouselMulti id="gal-carousel-multi-fixed" itemSizing="fixed" itemWidth="12rem" snap={false} loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · seamless + free drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "loop+seamless · snap=false · perView=2.2 · 自由拖拽可无限绕圈（无格点吸附）"
                </Label>
                <CarouselMulti id="gal-carousel-multi-seamless-free" perView={2.2} loop={true} seamless={true} snap={false} itemSizing="fraction">
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · page + flush" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "scrollBy=page · pageSnap=viewport · endAlign=flush · 整屏推进 · loop=false"
                </Label>
                <CarouselMulti id="gal-carousel-multi-flush" perView={2.2} scrollBy="page" pageSnap="viewport" endAlign="flush" loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselMulti · page + pad" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "scrollBy=page · pageSnap=peek · endAlign=pad · 裁切尾项成为下页首项 · loop=false"
                </Label>
                <CarouselMulti id="gal-carousel-multi-pad" perView={2.2} scrollBy="page" pageSnap="peek" endAlign="pad" loop={false}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselMulti>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselStack · autoplay" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "autoplay=4000 · stackSides=1 · loop=true · 悬停暂停 · 亦可拖拽吸附"
                </Label>
                <CarouselStack id="gal-carousel-stack-auto" controls="outside" stackSides={1} loop={true} autoplay={4000}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Stack C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "Stack D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "Stack E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselStack>
            </VStack>

            <VStack w="100%" gap="0.5rem">
                <Label font_weight="600" text_size="0.875rem">{ "CarouselStack · drag" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "无 autoplay · 拖拽跟手 · 松手吸附 · stackSides=1 · loop=true"
                </Label>
                <CarouselStack id="gal-carousel-stack-drag" controls="outside" stackSides={1} loop={true}>
                    <CarouselContent>
                        <CarouselItem>
                            <Box p="1rem" bg="muted" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag A" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="secondary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag B" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="accent" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600">{ "Drag C" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="primary" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="primary-foreground">{ "Drag D" }</Label>
                            </Box>
                        </CarouselItem>
                        <CarouselItem>
                            <Box p="1rem" bg="destructive" class="rounded-md" w="100%" h="8rem"
                                  items="center" justify="center">
                                <Label font_weight="600" color="destructive-foreground">{ "Drag E" }</Label>
                            </Box>
                        </CarouselItem>
                    </CarouselContent>
                    <CarouselPrevious />
                    <CarouselNext />
                </CarouselStack>
            </VStack>
        </VStack>
    };

    ui! {
        <MyCard title="Table · TreeView · Chart · Carousel">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "数据面：Table / 树 / Chart（预设 bar/line/area/pie/donut/radial + define_chart；shadcn chrome · tooltip indicator · ChartTheme 色板）/ 轮播。点柱、折线或扇区可选中；tooltip 钉在按下点。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "Table" }</Label>
                { table_el }
                <Label text_size="0.875rem">
                    { format!("Selected row: {:?}", table_sel.get()) }
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · collapsible exclusive（同层互斥 · Builder）" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "递归 Collapsible + chevron-right；默认不互斥。此例 `.exclusive(true)`：同层兄弟目录只开一项。旧 accordion 变体已并入 collapsible。"
                </Label>
                { tree_collapsible_el }
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · flat（VirtualList · Builder）" }</Label>
                { tree_flat_el }
                <Label font_weight="600" text_size="0.875rem">{ "TreeView · RSX collapsible（默认不互斥）" }</Label>
                <TreeView id="gal-tree-rsx" variant="collapsible" expandedSignal={expanded_rsx} expandedStorageKey="gal_tree_exp_rsx" height={140.0}>
                    <TreeViewItem id="root" label="Workspace">
                        <TreeViewItem id="src" label="src">
                            <TreeViewItem id="main" label="main.rs" />
                            <TreeViewItem id="lib" label="lib.rs" />
                        </TreeViewItem>
                        <TreeViewItem id="docs" label="Docs">
                            <TreeViewItem id="readme" label="README.md" />
                        </TreeViewItem>
                    </TreeViewItem>
                </TreeView>
                <Label font_weight="600" text_size="0.875rem">{ "BarChart" }</Label>
                <BarChart
                    id="gal-bar"
                    data={bar_rows.clone()}
                    h="6rem"
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "LineChart" }</Label>
                <LineChart
                    id="gal-line"
                    values={line_vals.clone()}
                    h={140.0}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "AreaChart" }</Label>
                <AreaChart
                    id="gal-area"
                    values={line_vals.clone()}
                    h={140.0}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "PieChart" }</Label>
                <PieChart
                    id="gal-pie"
                    data={pie_rows.clone()}
                    on_select={move |i| chart_sel.set(Some(i))}
                />
                <Label font_weight="600" text_size="0.875rem">{ "DonutChart · vibrant · indicator=dashed" }</Label>
                { donut_el }
                <Label font_weight="600" text_size="0.875rem">{ "RadialChart 预设" }</Label>
                { radial_preset }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · Area+Line+Dot+Rule · tooltip indicator=line" }</Label>
                { composed }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · 时间轴" }</Label>
                { time_chart }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · y log" }</Label>
                { log_chart }
                <Label font_weight="600" text_size="0.875rem">{ "define_chart · radial_bar + line + dot" }</Label>
                { radial_composed }
                <Label text_size="0.75rem" color="muted-foreground">
                    { match chart_sel.get() {
                        Some(i) => format!("Chart 选中 index = {i}"),
                        None => "Chart 未选中".into(),
                    } }
                </Label>
                <Label font_weight="600" text_size="0.875rem">{ "Carousel / Multi / Stack" }</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "复合 Content / Item / Previous / Next / Dots；←→ 或拖拽；弹簧动画。"
                </Label>
                { carousel_el }
            </VStack>
        </MyCard>
    }
}

const GAL_WP310_MASONRY: &str = "gal-wp310-masonry";

#[component]
fn VirtualMasonryShowcase() -> Element {
    let masonry: Element = virtual_masonry(GAL_WP310_MASONRY)
        .item_count(1000)
        .columns(3)
        .gap(8.0)
        .height(280.0)
        .overscan_viewports(1.0)
        .scrollbar_visibility("always")
        .item_height(|i, _| 48.0 + (i % 7) as f32 * 16.0)
        .render_item(|i, w, h| {
            let token = ["primary", "secondary", "accent", "muted"][i % 4];
            Element::from(
                r#box()
                    .w(w)
                    .h(h)
                    .rounded(8.0)
                    .bg(parse_color(token))
                    .p("0.35rem")
                    .text(format!("{i:04}")),
            )
        })
        .into();
    ui! {
        <MyCard title="VirtualMasonry（WP-3.10）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "1000 张变高卡片、3 列最短列优先；只挂视口+overscan。滚动应只换可见格，不掉帧。experimental，不进稳定面。"
                </Label>
                { masonry }
            </VStack>
        </MyCard>
    }
}

#[component]
fn SidebarShowcase() -> Element {
    let picked = use_signal("gal_sidebar_picked", || "overview".to_string());
    let tree_exp_a = use_signal("gal_sb_tree_exp_a", || {
        let mut s = HashSet::new();
        s.insert("app".into());
        s
    });
    let tree_exp_c = use_signal("gal_sb_tree_exp_c", || {
        let mut s = HashSet::new();
        s.insert("app".into());
        s
    });

    fn demo_btn(id: &str, label: &str, key: &'static str, picked: Signal<String>) -> Element {
        let btn: Element = sidebar_menu_button(id)
            .size("sm")
            .text(label)
            .on_click({
                let picked = picked;
                move || picked.set(key.to_string())
            })
            .into();
        rui::app::reactive::bind_prop(
            btn,
            format!("{id}__active"),
            move || picked.get() == key,
            |mounted, active| apply_bound_sidebar_menu_button_active(mounted, active),
        )
    }

    let builder_shell: Element = sidebar("gal-sidebar-demo-b")
        .w("14rem")
        .header(Element::from(
            vstack().w("100%").p("0.75rem").gap("0.25rem").child(
                label("gal-sidebar-demo-b-title")
                    .text("Builder")
                    .size(14.0)
                    .font_weight(600.0),
            ),
        ))
        .item(demo_btn(
            "gal-sidebar-demo-b-overview",
            "Overview",
            "overview",
            picked,
        ))
        .item(demo_btn("gal-sidebar-demo-b-team", "Team", "team", picked))
        .footer(Element::from(
            label("gal-sidebar-demo-b-foot")
                .text("sidebar() builder")
                .size(11.0)
                .color(parse_text_color("muted-foreground")),
        ))
        .into();

    let tree_shell: Element = {
        let picked = picked;
        sidebar("gal-sidebar-demo-c")
            .w("14rem")
            .header(Element::from(
                vstack().w("100%").p("0.75rem").gap("0.25rem").child(
                    label("gal-sidebar-demo-c-title")
                        .text("Tree Builder")
                        .size(14.0)
                        .font_weight(600.0),
                ),
            ))
            .tree(
                sidebar_tree("gal-sidebar-tree-b")
                    .expanded_signal(tree_exp_c)
                    .expanded_storage_key("gal_sb_tree_exp_c")
                    .selected_signal(picked)
                    .on_select(move |id| picked.set(id))
                    .roots(vec![TreeNode {
                        id: "app".into(),
                        label: "Application".into(),
                        children: vec![
                            TreeNode {
                                id: "overview".into(),
                                label: "Overview".into(),
                                children: vec![],
                            },
                            TreeNode {
                                id: "team".into(),
                                label: "Team".into(),
                                children: vec![],
                            },
                        ],
                    }]),
            )
            .into()
    };

    let picked_r = picked;
    ui! {
        <MyCard title="Sidebar（ShadCN 复合壳）">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：RSX 复合标签（SidebarProvider / SidebarMenuButton / isActive）与 Builder。语义色 sidebar / sidebar-accent。树形导航：`<TreeView chrome=\"sidebar\">` 或 `sidebar().tree(sidebar_tree(..))`。"
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A · RSX 分组 + 菜单按钮"</Label>
                <Box w="100%" max_w="36rem" h="14rem" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" h="100%" overflow="hidden" rounded="0.4375rem">
                        <HStack w="100%" h="100%" gap="0">
                            <Sidebar id="gal-sidebar-demo-a" w="14rem">
                                <SidebarHeader>
                                    <Label font_weight="600" text_size="0.875rem">"Projects"</Label>
                                </SidebarHeader>
                                <SidebarContent>
                                    <SidebarGroup>
                                        <SidebarGroupLabel>"Application"</SidebarGroupLabel>
                                        <SidebarGroupContent>
                                            <SidebarMenu>
                                                <SidebarMenuItem>
                                                    <SidebarMenuButton
                                                        id="gal-sidebar-demo-a-overview"
                                                        isActive={picked_r.get() == "overview"}
                                                        size="sm"
                                                        on_click={move || picked_r.set("overview".into())}
                                                    >
                                                        "Overview"
                                                    </SidebarMenuButton>
                                                </SidebarMenuItem>
                                                <SidebarMenuItem>
                                                    <SidebarMenuButton
                                                        id="gal-sidebar-demo-a-team"
                                                        isActive={picked_r.get() == "team"}
                                                        size="sm"
                                                        on_click={move || picked_r.set("team".into())}
                                                    >
                                                        "Team"
                                                    </SidebarMenuButton>
                                                </SidebarMenuItem>
                                            </SidebarMenu>
                                        </SidebarGroupContent>
                                    </SidebarGroup>
                                    <SidebarSeparator />
                                    <SidebarInput id="gal-sidebar-demo-a-search" placeholder="Search…" />
                                </SidebarContent>
                            </Sidebar>
                            <Box w="100%" h="100%" bg="background" p="0.75rem" grow="1">
                                <Label color="foreground">{ format!("Picked: {}", picked_r.get()) }</Label>
                            </Box>
                        </HStack>
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"B · Builder 双写"</Label>
                <Box w="100%" max_w="36rem" h="14rem" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" h="100%" overflow="hidden" rounded="0.4375rem">
                        <HStack w="100%" h="100%" gap="0">
                            { builder_shell }
                            <Box w="100%" h="100%" bg="background" p="0.75rem" grow="1">
                                <Label color="foreground">{ format!("Picked: {}", picked.get()) }</Label>
                            </Box>
                        </HStack>
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"C · RSX TreeView chrome=sidebar"</Label>
                <Box w="100%" max_w="36rem" h="16rem" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" h="100%" overflow="hidden" rounded="0.4375rem">
                        <HStack w="100%" h="100%" gap="0">
                            <Sidebar id="gal-sidebar-demo-tree-a" w="14rem">
                                <SidebarHeader>
                                    <Label font_weight="600" text_size="0.875rem">"Workspace"</Label>
                                </SidebarHeader>
                                <SidebarContent>
                                    <TreeView
                                        id="gal-sidebar-tree-a"
                                        variant="collapsible"
                                        chrome="sidebar"
                                        expandedSignal={tree_exp_a}
                                        expandedStorageKey="gal_sb_tree_exp_a"
                                        selectedSignal={picked_r}
                                    >
                                        <TreeViewItem id="app" label="Application">
                                            <TreeViewItem id="overview" label="Overview" />
                                            <TreeViewItem id="team" label="Team" />
                                        </TreeViewItem>
                                    </TreeView>
                                </SidebarContent>
                            </Sidebar>
                            <Box w="100%" h="100%" bg="background" p="0.75rem" grow="1">
                                <Label color="foreground">{ format!("Picked: {}", picked_r.get()) }</Label>
                            </Box>
                        </HStack>
                    </Box>
                </Box>
                <Label font_weight="700" text_size="0.875rem">"D · Builder sidebar_tree"</Label>
                <Box w="100%" max_w="36rem" h="16rem" rounded="0.5rem" bg="border" p="1px">
                    <Box w="100%" h="100%" overflow="hidden" rounded="0.4375rem">
                        <HStack w="100%" h="100%" gap="0">
                            { tree_shell }
                            <Box w="100%" h="100%" bg="background" p="0.75rem" grow="1">
                                <Label color="foreground">{ format!("Picked: {}", picked.get()) }</Label>
                            </Box>
                        </HStack>
                    </Box>
                </Box>
            </VStack>
        </MyCard>
    }
}

#[component]
fn NavbarNavigationMenuShowcase() -> Element {
    let picked = use_signal("gal_navmenu_picked", || "—".to_string());

    // B. Builder（便捷）—— 与下方 RSX 等价
    let builder_bar: Element = {
        let picked = picked;
        let nav_menu: Element = navigation_menu("gal-navmenu-builder")
            .link("overview", "Overview", {
                let picked = picked;
                move || picked.set("Overview".into())
            })
            .trigger(
                "comp",
                "Components",
                vec![
                    NavigationMenuLinkData::new("dialog", "Dialog", {
                        let picked = picked;
                        move || picked.set("Dialog".into())
                    })
                    .description("Modal surface with header / body / footer"),
                    NavigationMenuLinkData::new("tabs", "Tabs", {
                        let picked = picked;
                        move || picked.set("Tabs".into())
                    })
                    .description("Compound Tabs + headless strip"),
                ],
            )
            .link("about", "About", {
                let picked = picked;
                move || picked.set("About".into())
            })
            .into();
        navbar()
            .brand(label("").text("RUI").font_weight(600.0))
            .center_child(nav_menu)
            .center_justify("center")
            .action(button("").variant("outline").size("sm").text("Action"))
            .into()
    };

    ui! {
        <MyCard title="Navbar · NavigationMenu">
            <VStack w="100%" gap="1rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：复合 RSX（推荐，宏 → navigation_menu_scope / navigation_menu_content）与 Builder。Navbar 是布局壳（WP-2.8：无用户 id）；导览非 Menubar 命令、非页面栈。"
                </Label>

                <Label text_size="0.75rem" font_weight="600">"A. 复合 RSX"</Label>
                <Navbar>
                    <NavbarBrand>
                        <Label font_weight="600">"RUI"</Label>
                    </NavbarBrand>
                    <NavbarCenter>
                        <NavigationMenu id="gal-navmenu">
                            <NavigationMenuList>
                                <NavigationMenuItem value="overview">
                                    <NavigationMenuLink on_click={move || {
                                        picked.set("Overview".into());
                                    }}>
                                        "Overview"
                                    </NavigationMenuLink>
                                </NavigationMenuItem>
                                <NavigationMenuItem value="comp">
                                    <NavigationMenuTrigger>"Components"</NavigationMenuTrigger>
                                    <NavigationMenuContent>
                                        <NavigationMenuLink on_click={move || {
                                            picked.set("Dialog".into());
                                        }}>
                                            "Dialog"
                                            <Label text_size="0.75rem" color="muted-foreground">
                                                "Modal surface with header / body / footer"
                                            </Label>
                                        </NavigationMenuLink>
                                        <NavigationMenuLink on_click={move || {
                                            picked.set("Tabs".into());
                                        }}>
                                            "Tabs"
                                            <Label text_size="0.75rem" color="muted-foreground">
                                                "Compound Tabs + headless strip"
                                            </Label>
                                        </NavigationMenuLink>
                                    </NavigationMenuContent>
                                </NavigationMenuItem>
                                <NavigationMenuItem value="about">
                                    <NavigationMenuLink on_click={move || {
                                        picked.set("About".into());
                                    }}>
                                        "About"
                                    </NavigationMenuLink>
                                </NavigationMenuItem>
                            </NavigationMenuList>
                        </NavigationMenu>
                    </NavbarCenter>
                    <NavbarActions>
                        <Button variant="outline" size="sm">"Action"</Button>
                    </NavbarActions>
                </Navbar>

                <Label text_size="0.75rem" font_weight="600">"B. Builder"</Label>
                { builder_bar }

                <Label text_size="0.875rem">{ format!("Selected: {}", picked.get()) }</Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn PaginationShowcase() -> Element {
    let page_a = use_signal("gal_pagination_rsx", || 2usize);
    let page_b = use_signal("gal_pagination_builder", || 8usize);
    const COUNT_A: usize = 12;
    const COUNT_B: usize = 30;
    // A：上限 left=1 / sibling=1 / right=1 → 换页格子数恒定
    const RANGE_A: PaginationRangeOpts = PaginationRangeOpts {
        left_count: 1,
        sibling_count: 1,
        right_count: 1,
        max_items: None,
    };
    // B：左≤3 / 中 sibling≤2 / 右≤4；换页格子数尽量恒定
    const RANGE_B: PaginationRangeOpts = PaginationRangeOpts {
        left_count: 3,
        sibling_count: 2,
        right_count: 4,
        max_items: None,
    };

    // Gallery 分区体在 isolate_reads 下构建：`disabled={sig.get()…}` / `is_active={…}`
    // 会被宏 untrack 冻住。用 reactive! 岛订阅 page，在闭包内用局部页码重建零件。
    let rsx_bar = rui::reactive!("examples/gallery/PaginationShowcase:rsx", move || {
        let page = page_a.get();
        let mut items: Vec<Element> = Vec::new();
        items.push(ui! {
            <PaginationItem>
                <PaginationPrevious
                    id="gal-pg-a-prev"
                    text="上一页"
                    disabled={page <= 1}
                    on_click={move || {
                        page_a.update(|p| *p = (*p).saturating_sub(1).max(1));
                    }}
                />
            </PaginationItem>
        });
        for entry in pagination_range(page, COUNT_A, RANGE_A) {
            match entry {
                PaginationPage::Ellipsis => {
                    items.push(ui! {
                        <PaginationItem>
                            <PaginationEllipsis />
                        </PaginationItem>
                    });
                }
                PaginationPage::Number(n) => {
                    items.push(ui! {
                        <PaginationItem>
                            <PaginationLink
                                id={format!("gal-pg-a-{n}")}
                                is_active={page == n}
                                on_click={move || page_a.set(n)}
                            >{ n.to_string() }</PaginationLink>
                        </PaginationItem>
                    });
                }
            }
        }
        items.push(ui! {
            <PaginationItem>
                <PaginationNext
                    id="gal-pg-a-next"
                    text="下一页"
                    disabled={page >= COUNT_A}
                    on_click={move || {
                        page_a.update(|p| *p = (*p + 1).min(COUNT_A));
                    }}
                />
            </PaginationItem>
        });
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        ui! {
            <Pagination>
                <PaginationContent>
                    { frag.children(items) }
                </PaginationContent>
            </Pagination>
        }
    });

    // Builder 内部 reactive_anchor 订阅 page_signal；分区 effect 下 is_tracking=true，可直接挂载。
    let builder_bar: Element = pagination("gal-pagination-builder")
        .page_count(COUNT_B)
        .page_signal(page_b)
        .range_counts(
            RANGE_B.left_count,
            RANGE_B.sibling_count,
            RANGE_B.right_count,
        )
        .previous_text("上一页")
        .next_text("下一页")
        .jumper(true)
        .jumper_button_text("跳转")
        .into();

    let status_a = rui::reactive!("examples/gallery/PaginationShowcase:status_a", move || {
        let t = format!(
            "A · Page {} / {}（left=1 sibling=1 right=1）",
            page_a.get(),
            COUNT_A
        );
        ui! { <Label text_size="0.875rem">{ t }</Label> }
    });
    let status_b = rui::reactive!("examples/gallery/PaginationShowcase:status_b", move || {
        let t = format!(
            "B · Page {} / {}（left=3 sibling=2 right=4 · auto-fit · jumper）",
            page_b.get(),
            COUNT_B
        );
        ui! { <Label text_size="0.875rem">{ t }</Label> }
    });

    ui! {
        <MyCard title="Pagination">
            <VStack w="100%" gap="1rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：复合 RSX 与 Builder。左/中/右为上限页数；换页时格子数尽量恒定。Builder 默认按容器宽度自动压缩；可选 jumper 提供整数页码输入与跳转按钮。"
                </Label>

                <Label text_size="0.75rem" font_weight="600">"A. 复合 RSX"</Label>
                { rsx_bar }
                { status_a }

                <Label text_size="0.75rem" font_weight="600">"B. Builder"</Label>
                { builder_bar }
                { status_b }
            </VStack>
        </MyCard>
    }
}

#[component]
fn MenubarStepperShowcase() -> Element {
    let step = use_signal("gal_stepper", || 1usize);
    let stepper_el: Element = stepper("gal-stepper")
        .steps(["Account", "Profile", "Confirm"])
        .current_signal(step)
        .into();

    ui! {
        <MyCard title="Menubar · Stepper">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "应用命令栏（复合 Menubar，对齐 solid-ui / ShadCN）：←→ 切换菜单；打开后悬停可切换。下方为 Stepper。"
                </Label>
                <Menubar id="gal-menubar">
                    <MenubarMenu id="file">
                        <MenubarTrigger>"File"</MenubarTrigger>
                        <MenubarContent>
                            <MenubarItem on_click={|| {
                                toast_push("New File", 1500);
                            }}>
                                "New File"
                                <MenubarShortcut>"⌘N"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarItem on_click={|| {
                                toast_push("Open…", 1500);
                            }}>
                                "Open…"
                                <MenubarShortcut>"⌘O"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarSeparator />
                            <MenubarSub>
                                <MenubarSubTrigger>"Share"</MenubarSubTrigger>
                                <MenubarSubContent>
                                    <MenubarItem on_click={|| {
                                        toast_push("Email link", 1500);
                                    }}>
                                        "Email link"
                                    </MenubarItem>
                                    <MenubarItem on_click={|| {
                                        toast_push("Messages", 1500);
                                    }}>
                                        "Messages"
                                    </MenubarItem>
                                </MenubarSubContent>
                            </MenubarSub>
                            <MenubarSeparator />
                            <MenubarItem variant="destructive" on_click={|| {
                                toast_push("Quit", 1500);
                            }}>
                                "Quit"
                                <MenubarShortcut>"⌘Q"</MenubarShortcut>
                            </MenubarItem>
                        </MenubarContent>
                    </MenubarMenu>
                    <MenubarMenu id="edit">
                        <MenubarTrigger>"Edit"</MenubarTrigger>
                        <MenubarContent>
                            <MenubarItem on_click={|| {
                                toast_push("Undo", 1500);
                            }}>
                                "Undo"
                                <MenubarShortcut>"⌘Z"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarItem on_click={|| {
                                toast_push("Redo", 1500);
                            }}>
                                "Redo"
                                <MenubarShortcut>"⇧⌘Z"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarSeparator />
                            <MenubarItem on_click={|| {
                                toast_push("Cut", 1500);
                            }}>
                                "Cut"
                                <MenubarShortcut>"⌘X"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarItem on_click={|| {
                                toast_push("Copy", 1500);
                            }}>
                                "Copy"
                                <MenubarShortcut>"⌘C"</MenubarShortcut>
                            </MenubarItem>
                            <MenubarItem on_click={|| {
                                toast_push("Paste", 1500);
                            }}>
                                "Paste"
                                <MenubarShortcut>"⌘V"</MenubarShortcut>
                            </MenubarItem>
                        </MenubarContent>
                    </MenubarMenu>
                    <MenubarMenu id="view">
                        <MenubarTrigger>"View"</MenubarTrigger>
                        <MenubarContent>
                            <MenubarCheckboxItem defaultChecked={true} on_click={|| {
                                toast_push("Status Bar toggled", 1500);
                            }}>
                                "Status Bar"
                            </MenubarCheckboxItem>
                            <MenubarCheckboxItem on_click={|| {
                                toast_push("Activity Bar toggled", 1500);
                            }}>
                                "Activity Bar"
                            </MenubarCheckboxItem>
                            <MenubarSeparator />
                            <MenubarRadioGroup defaultValue="bottom">
                                <MenubarLabel>"Panel Position"</MenubarLabel>
                                <MenubarRadioItem value="top" on_click={|| {
                                    toast_push("Panel: Top", 1500);
                                }}>
                                    "Top"
                                </MenubarRadioItem>
                                <MenubarRadioItem value="bottom" on_click={|| {
                                    toast_push("Panel: Bottom", 1500);
                                }}>
                                    "Bottom"
                                </MenubarRadioItem>
                                <MenubarRadioItem value="right" on_click={|| {
                                    toast_push("Panel: Right", 1500);
                                }}>
                                    "Right"
                                </MenubarRadioItem>
                            </MenubarRadioGroup>
                        </MenubarContent>
                    </MenubarMenu>
                </Menubar>
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

fn nav_item(
    section: Signal<String>,
    id: &'static str,
    label_text: &'static str,
    icon: &str,
) -> Element {
    let s_click = section;
    let s_bind = section;
    let mut row = hstack()
        .w("100%")
        .items_center()
        .gap("0.5rem")
        .flex_grow(1.0)
        .min_w(0.0);
    if let Some(glyph) = lucide_glyph(icon) {
        row = row.child(
            build_icon_text_node(glyph)
                .text_size(16.0)
                .text_color(parse_text_color("sidebar-foreground"))
                .pointer_events("none"),
        );
    }
    row = row.child(
        build_text_node(
            label_text,
            TextNodeConfig {
                size_px: 14.0,
                weight: 400,
                color: parse_text_color("sidebar-foreground"),
                ..Default::default()
            },
        )
        .flex_grow(1.0)
        .min_w(0.0),
    );
    let btn: Element = sidebar_menu_button(format!("gal-nav-{id}"))
        .on_click(move || s_click.set(id.to_string()))
        .child(Element::from(row))
        .into();
    let btn = rui::app::reactive::bind_prop(
        btn,
        format!("gal-nav-{id}__active"),
        move || s_bind.get() == id,
        |mounted, active| apply_bound_sidebar_menu_button_active(mounted, active),
    );
    SidebarMenuItem(SidebarMenuItemProps {
        class: None,
        children: Some(btn),
        base: None,
    })
}

fn gallery_nav_menu(section: Signal<String>) -> Element {
    let items: Vec<Element> = [
        ("basics", "基础", "layout-dashboard"),
        ("events", "指针事件", "mouse-pointer-click"),
        ("forms", "表单", "file-text"),
        ("overlays", "浮层 / 反馈", "layers"),
        ("data", "数据", "table"),
        ("draw", "绘制", "pencil"),
        ("navigation", "导航", "compass"),
        ("layout", "布局", "layout-grid"),
        ("text", "文本", "type"),
        ("about", "关于", "info"),
    ]
    .iter()
    .map(|(id, label, icon)| nav_item(section, id, label, icon))
    .collect();
    let mut menu_frag = Element::new();
    menu_frag.set_attr("rui_fragment", "1");
    let menu = SidebarMenu(SidebarMenuProps {
        class: None,
        children: Some(menu_frag.children(items)),
        base: None,
    });
    let group_content = SidebarGroupContent(SidebarGroupContentProps {
        class: None,
        children: Some(menu),
        base: None,
    });
    let label_el = SidebarGroupLabel(SidebarGroupLabelProps {
        class: None,
        children: Some(build_text_node(
            "导航",
            TextNodeConfig {
                size_px: 12.0,
                weight: 500,
                color: parse_text_color("sidebar-foreground/70"),
                ..Default::default()
            },
        )),
        base: None,
    });
    let mut group_frag = Element::new();
    group_frag.set_attr("rui_fragment", "1");
    SidebarGroup(SidebarGroupProps {
        class: None,
        children: Some(group_frag.children(vec![label_el, group_content])),
        base: None,
    })
}

fn gallery_section_body(section: &str) -> Element {
    match section {
        "forms" => ui! {
            <ScrollArea id="scroll-forms" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"表单"</Label>
                    <InputShowcase />
                    <InputAdornmentShowcase />
                    <InputOTPShowcase />
                    <FormValidateShowcase />
                    <PickerPlaceholdersShowcase />
                    <TextareaComboboxShowcase />
                    <ToggleControlsShowcase />
                    <RadioSliderShowcase />
                    <SelectPopoverTabShowcase />
                </VStack>
            </ScrollArea>
        },
        "events" => ui! {
            <ScrollArea id="scroll-events" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"指针事件"</Label>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "crates/rui/src/app/pointer_events.rs：MouseEvent 字段、捕获/冒泡、click LCA"
                    </Label>
                    <MouseEventShowcase />
                    <CursorShowcase />
                </VStack>
            </ScrollArea>
        },
        "overlays" => ui! {
            <ScrollArea id="scroll-overlays" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"浮层 / 反馈"</Label>
                    <OverlayFeedbackShowcase />
                    <PrimitivesShowcase />
                </VStack>
            </ScrollArea>
        },
        "draw" => ui! {
            <ScrollArea id="scroll-draw" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"绘制"</Label>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "WP-3.11 <svg> · WP-3.12 <canvas> · WP-3.13 <scene3d> feature · lucide-animated。"
                    </Label>
                    <DrawingSvgShowcase />
                    <LucideAnimatedShowcase />
                    <DrawingCanvasShowcase />
                    <DrawingScene3dShowcase />
                </VStack>
            </ScrollArea>
        },
        "data" => ui! {
            <ScrollArea id="scroll-data" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"数据"</Label>
                    <PaginationShowcase />
                    <DataChartShowcase />
                    <VirtualMasonryShowcase />
                </VStack>
            </ScrollArea>
        },
        "navigation" => ui! {
            <ScrollArea id="scroll-navigation" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"导航"</Label>
                    <SidebarShowcase />
                    <NavbarNavigationMenuShowcase />
                    <MenubarStepperShowcase />
                    <NavigatorShowcase />
                    <RouterShowcase />
                    { TrayShowcase() }
                    { WindowChromeShowcase() }
                    <SelectPopoverTabShowcase />
                </VStack>
            </ScrollArea>
        },
        "layout" => ui! {
            <ScrollArea id="scroll-layout" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"布局"</Label>
                    <LayoutSubtreeShowcase />
                    <DamageShowcase />
                    <OwnerRegistryShowcase />
                    <StoreResourceShowcase />
                    <RootBareGetShowcase />
                    <IdentityApiShowcase />
                    <BoxModelShowcase />
                    <ScrollShowcase />
                    <AccordionShowcase />
                    <CollapsibleShowcase />
                    <ResizableShowcase />
                    <LogicalShowcase />
                    <UtilityClassShowcase />
                    <GridAndDragShowcase />
                    <ShadowShowcase />
                    <FilterShowcase />
                    <GradientShowcase />
                    <BorderStyleShowcase />
                    <OutlineShowcase />
                    <TransitionShowcase />
                    <ScaleZoomShowcase />
                    <RotateTranslateShowcase />
                    <TransformShowcase />
                </VStack>
            </ScrollArea>
        },
        "text" => ui! {
            <ScrollArea id="scroll-text" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"文本"</Label>
                    <TextAlignShowcase />
                    <TextCompatibilityShowcase />
                    <EmojiRealtimeRegressionShowcase />
                    <TypographyShowcase />
                </VStack>
            </ScrollArea>
        },
        "about" => ui! {
            <ScrollArea id="scroll-about" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem" max_w="48rem">
                    <Label text_size="1.25rem" font_weight="700">"关于"</Label>
                    <Box bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                        <VStack gap="0.5rem">
                            <Label font_weight="700">"Technical Stack"</Label>
                            <Label>"Layout: Taffy (Flexbox / Grid)"</Label>
                            <Label>"Text: Glyphon (Cosmic-text)"</Label>
                            <Label>"Rendering: WGPU"</Label>
                            <Label>"Reactivity: Signal fine-grained updates"</Label>
                            <Label>"A11y: AccessKit bridge"</Label>
                        </VStack>
                    </Box>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "侧栏分区对应 ENGINEERING_IMPLEMENTATION_PLAN WP-3.1 Gallery 覆盖矩阵。"
                    </Label>
                </VStack>
            </ScrollArea>
        },
        _ => ui! {
            <ScrollArea id="scroll-basics" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <Label text_size="1.25rem" font_weight="700">"基础"</Label>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Bubble · Avatar · Breadcrumb · ButtonGroup · AspectRatio · CSS animation · 排版装饰"
                    </Label>
                    <ButtonShowcase />
                    <ButtonHoverEasingShowcase />
                    <BadgeShowcase />
                    { KbdShowcase() }
                    <BubbleShowcase />
                    { AvatarShowcase() }
                    { BreadcrumbShowcase() }
                    { ButtonGroupShowcase() }
                    { AspectRatioShowcase() }
                    <AnimationShowcase />
                    <TypographyShowcase />
                </VStack>
            </ScrollArea>
        },
    }
}

fn build_gallery_app() -> Element {
    let section = use_signal("gal_nav_section", || "basics".to_string());
    let sec_nav = section;
    let sec_body = section;

    let nav_menu = gallery_nav_menu(sec_nav);

    let body = rui::app::reactive::switch_reactive(
        "examples/gallery/App:section_body",
        move || sec_body.get(),
        move |key| rui::core::reactivity::isolate_reads(|| gallery_section_body(key.as_str())),
    );

    let chrome: Element = ui! {
        <SidebarProvider id="gal-shell" variant="inset" defaultOpen={true}>
            <Sidebar id="gal-sidebar" variant="inset">
                <SidebarHeader>
                    <SidebarMenu>
                        <SidebarMenuItem>
                            <SidebarMenuButton id="gal-workspace">
                                "RUI Gallery"
                            </SidebarMenuButton>
                        </SidebarMenuItem>
                    </SidebarMenu>
                </SidebarHeader>
                <SidebarSeparator />
                <SidebarContent>
                    { nav_menu }
                </SidebarContent>
                <SidebarSeparator />
                <SidebarFooter>
                    <SidebarMenu>
                        <SidebarMenuItem>
                            <SidebarMenuButton id="gal-user">
                                "Gallery"
                            </SidebarMenuButton>
                        </SidebarMenuItem>
                    </SidebarMenu>
                </SidebarFooter>
            </Sidebar>
            <SidebarInset variant="inset">
                <HStack w="100%" items="center" gap="0.25rem" p="0.5rem" flex_shrink="0">
                    <SidebarTrigger id="gal-sidebar-trigger" />
                </HStack>
                <VStack min_w="0" min_h="0" grow="1">
                    { body }
                </VStack>
            </SidebarInset>
        </SidebarProvider>
    };
    // 窗口根：侧栏/正文 + Toaster 锚点。通知舞台 hoist 到 `__rui_portal_layer`（与 Dialog 同层）。
    // 不要 `ui! { <Toaster/> }` 包一层——会盖掉宿主 template_id（与 CommandOverlay 相同）。
    let toast_host: Element = toaster()
        .variant("stack")
        .overflow("displace")
        .limit(3)
        .position("top-center")
        .into();
    Element::new()
        .w("100%")
        .h("100%")
        .relative()
        .child(chrome)
        .child(toast_host)
}

fn main() {
    #[cfg(feature = "runtime-logging")]
    rui::logging::init_with(rui::logging::InitOptions {
        default_filter:
            "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off,fontdb=error",
    });
    let (rgba, w, h) = checker_rgba_texture();
    rui::queue_image_rgba8("demo-checker", w, h, rgba);

    #[cfg(feature = "devtools")]
    rui::app::run_with(
        rui::app::WindowOptions::new().devtools(true),
        build_gallery_app,
    );
    #[cfg(not(feature = "devtools"))]
    rui::app::run(build_gallery_app);
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    use rui::core::reactivity::{create_effect, create_root};

    #[test]
    fn mouse_event_log_line_count_stable_from_idle_to_move() {
        assert_eq!(mouse_log_placeholder().lines().count(), MOUSE_LOG_LINES);
        let live = format_mouse_log_fields(
            "mousemove",
            0,
            0,
            0,
            1,
            [410.0, 82.0],
            [104.0, 116.0],
            [410.0, 82.0],
            [615.0, 123.0],
            [0.7, -0.7],
            false,
            false,
            false,
            false,
            false,
            None,
            Some("gal-mouse-pad"),
            "Target",
            false,
            false,
        );
        assert_eq!(
            live.lines().count(),
            MOUSE_LOG_LINES,
            "first mousemove must not add wrap rows vs idle placeholder"
        );
    }

    #[test]
    fn mouse_event_log_panel_reserves_height_before_first_move() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let mut tree = rui::core::reactivity::isolate_reads(|| {
                        MouseEventShowcase(MouseEventShowcaseProps::default())
                    });
                    fn find_panel(
                        el: &rui::core::tree::Element,
                    ) -> Option<&rui::core::tree::Element> {
                        if el.template_id.as_deref() == Some("gal-mouse-log-panel") {
                            return Some(el);
                        }
                        el.children.iter().find_map(find_panel)
                    }
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, 720.0, 900.0, [0.0, 0.0]);
                    let panel = find_panel(&tree).expect("gal-mouse-log-panel");
                    assert!(
                        (panel.layout_bounds[3] - MOUSE_LOG_PANEL_H).abs() < 2.0,
                        "idle coordinate panel must already be {}px tall (got {}), or first mousemove collapses the page",
                        MOUSE_LOG_PANEL_H,
                        panel.layout_bounds[3]
                    );
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn forms_section_under_isolate_reads_keeps_mark_invalid_caption() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    // Match App: `switch_reactive` body builds under `isolate_reads`.
                    let mut tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
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
        });
    }

    #[test]
    fn forms_color_picker_variants_tabs_mount_only_active() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    fn find_contains(el: &rui::core::tree::Element, needle: &str) -> bool {
                        el.template_id
                            .as_deref()
                            .is_some_and(|id| id.contains(needle))
                            || el.children.iter().any(|c| find_contains(c, needle))
                    }
                    assert!(
                        find_contains(&tree, "gal-cp-variants"),
                        "forms pickers must host ColorPicker variants under Tabs"
                    );
                    assert!(
                        find_contains(&tree, "gal-cp-variants:content:sketch"),
                        "TabsContent placeholder for default sketch tab"
                    );
                    // Before: nine ColorPicker reactive placeholders were siblings in the
                    // declarative forms tree. TabsContent keeps them out of that tree
                    // (mounted only via show_reactive into the live reconcile slot).
                    assert!(
                        !find_contains(&tree, "gal-cp-chrome__cp_view"),
                        "inactive variant must not be an eager declarative sibling"
                    );
                    assert!(
                        !find_contains(&tree, "gal-cp-compact__cp_view"),
                        "inactive variant must not be an eager declarative sibling"
                    );
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn picker_showcase_tabs_chips_layout_alone() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let mut tree = rui::core::reactivity::isolate_reads(|| {
                        PickerPlaceholdersShowcase(PickerPlaceholdersShowcaseProps::default())
                    });
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
                    eprintln!(
                        "picker alone: root={:?} texts={} instances={}",
                        tree.layout_bounds,
                        result.text_nodes.len(),
                        result.instances.len()
                    );
                    for label in [
                        "Date", "DateTime", "Time", "Month", "Week", "Sketch", "Chrome",
                    ] {
                        match result.text_nodes.iter().find(|n| n.text == label) {
                            Some(n) => eprintln!(
                                "  '{label}' @ ({:.1},{:.1}) {:.1}x{:.1}",
                                n.x, n.y, n.width, n.height
                            ),
                            None => eprintln!("  '{label}' MISSING"),
                        }
                    }
                    fn collect_triggers<'a>(
                        el: &'a rui::core::tree::Element,
                        out: &mut Vec<&'a rui::core::tree::Element>,
                    ) {
                        if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-trigger") {
                            out.push(el);
                        }
                        for c in &el.children {
                            collect_triggers(c, out);
                        }
                    }
                    let mut triggers = Vec::new();
                    collect_triggers(&tree, &mut triggers);
                    eprintln!("triggers={}", triggers.len());
                    for t in &triggers {
                        eprintln!("  {:?} bounds={:?}", t.text, t.layout_bounds);
                    }
                    assert!(
                        triggers.len() >= 5,
                        "expected date/time triggers, got {}",
                        triggers.len()
                    );
                    let date = triggers
                        .iter()
                        .find(|t| t.text.as_deref() == Some("Date"))
                        .expect("Date");
                    let dt = triggers
                        .iter()
                        .find(|t| t.text.as_deref() == Some("DateTime"))
                        .expect("DateTime");
                    assert!(
                        date.layout_bounds[2] > 20.0,
                        "Date chip width crushed: {:?}",
                        date.layout_bounds
                    );
                    assert!(
                        (dt.layout_bounds[0] - date.layout_bounds[0]).abs() > 4.0
                            || (dt.layout_bounds[1] - date.layout_bounds[1]).abs() > 4.0,
                        "Date/DateTime piled: {:?} vs {:?}",
                        date.layout_bounds,
                        dt.layout_bounds
                    );
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn forms_tabs_chips_after_scroll_into_view_do_not_overlap() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let mut tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);

                    fn find_scroll<'a>(
                        el: &'a mut rui::core::tree::Element,
                    ) -> Option<&'a mut rui::core::tree::Element> {
                        if el
                            .focus_id
                            .as_deref()
                            .is_some_and(|f| f.starts_with("scroll:"))
                        {
                            return Some(el);
                        }
                        for c in el.children.iter_mut() {
                            if let Some(hit) = find_scroll(c) {
                                return Some(hit);
                            }
                        }
                        None
                    }
                    // Pickers sit ~y=1287; scroll so TabsList enters the 800px viewport.
                    let scroll = find_scroll(&mut tree).expect("forms ScrollArea viewport");
                    scroll.scroll_offset = [0.0, 1100.0];

                    let result = layout.build_and_layout_fast(
                        &mut tree,
                        900.0,
                        800.0,
                        [0.0, 0.0],
                        true,
                        false,
                    );

                    for label in ["Date", "DateTime", "Time", "Month", "Week"] {
                        match result.text_nodes.iter().find(|n| n.text == label) {
                            Some(n) => eprintln!(
                                "scrolled '{label}' @ ({:.1},{:.1}) {:.1}x{:.1}",
                                n.x, n.y, n.width, n.height
                            ),
                            None => eprintln!("scrolled '{label}' MISSING"),
                        }
                    }
                    let mut xs: Vec<(String, f32, f32)> =
                        ["Date", "DateTime", "Time", "Month", "Week"]
                            .iter()
                            .filter_map(|lab| {
                                let n = result.text_nodes.iter().find(|n| n.text == *lab)?;
                                Some((lab.to_string(), n.x, n.width))
                            })
                            .collect();
                    assert!(
                        xs.len() >= 3,
                        "after scroll, date/time chip labels must paint (got {xs:?})"
                    );
                    xs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    for w in xs.windows(2) {
                        let (la, xa, wa) = (&w[0].0, w[0].1, w[0].2);
                        let (lb, xb, _) = (&w[1].0, w[1].1, w[1].2);
                        assert!(
                            (xb - xa).abs() > 4.0,
                            "after scroll chips share x — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb} (gallery pile-up)"
                        );
                        assert!(
                            xb + 0.5 >= xa + wa.min(8.0),
                            "after scroll chips overlap — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                        );
                    }
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn forms_date_time_tabs_chips_do_not_overlap() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let mut tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let result = layout.build_and_layout(&mut tree, 900.0, 4000.0, [0.0, 0.0]);
                    eprintln!(
                        "forms layout: root_bounds={:?} text_nodes={} instances={}",
                        tree.layout_bounds,
                        result.text_nodes.len(),
                        result.instances.len()
                    );
                    for label in ["Date", "DateTime", "Sketch", "表单", "Mark invalid"] {
                        match result.text_nodes.iter().find(|n| n.text == label) {
                            Some(n) => eprintln!(
                                "  paint '{label}' @ ({:.1},{:.1}) size {:.1}x{:.1}",
                                n.x, n.y, n.width, n.height
                            ),
                            None => eprintln!("  paint '{label}' MISSING"),
                        }
                    }

                    fn collect_triggers<'a>(
                        el: &'a rui::core::tree::Element,
                        out: &mut Vec<&'a rui::core::tree::Element>,
                    ) {
                        if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-trigger") {
                            out.push(el);
                        }
                        for c in &el.children {
                            collect_triggers(c, out);
                        }
                    }
                    fn under_tabs<'a>(
                        el: &'a rui::core::tree::Element,
                        tabs_id: &str,
                    ) -> Option<&'a rui::core::tree::Element> {
                        if el.template_id.as_deref() == Some(tabs_id) {
                            return Some(el);
                        }
                        el.children.iter().find_map(|c| under_tabs(c, tabs_id))
                    }

                    for tabs_id in ["gal-dp-kinds", "gal-cp-variants"] {
                        let tabs = under_tabs(&tree, tabs_id).expect(tabs_id);
                        eprintln!(
                            "{tabs_id} root bounds={:?} kids={}",
                            tabs.layout_bounds,
                            tabs.children.len()
                        );
                        for (i, c) in tabs.children.iter().enumerate() {
                            eprintln!(
                                "  child[{i}] slot={:?} tid={:?} bounds={:?} text={:?} kids={} display={:?}",
                                c.attrs.get("data-slot"),
                                c.template_id,
                                c.layout_bounds,
                                c.text,
                                c.children.len(),
                                c.style.display
                            );
                        }
                        let mut triggers = Vec::new();
                        collect_triggers(tabs, &mut triggers);
                        assert!(
                            triggers.len() >= 3,
                            "{tabs_id}: expected triggers, got {}",
                            triggers.len()
                        );
                        for t in &triggers {
                            eprintln!(
                                "{tabs_id} chip text={:?} bounds={:?}",
                                t.text, t.layout_bounds
                            );
                        }
                        // Prefer paint positions when element bounds were not stamped (0×0).
                        let mut paint_xs: Vec<(String, f32, f32)> = triggers
                            .iter()
                            .filter_map(|t| {
                                let label = t.text.as_deref()?;
                                let n = result.text_nodes.iter().find(|n| n.text == label)?;
                                Some((label.to_string(), n.x, n.width))
                            })
                            .collect();
                        assert!(
                            paint_xs.len() >= 2,
                            "{tabs_id}: missing painted chip labels (got {paint_xs:?})"
                        );
                        paint_xs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                        for w in paint_xs.windows(2) {
                            let (la, xa, wa) = (&w[0].0, w[0].1, w[0].2);
                            let (lb, xb, _wb) = (&w[1].0, w[1].1, w[1].2);
                            assert!(
                                (xb - xa).abs() > 4.0,
                                "{tabs_id}: painted chips share x — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                            );
                            assert!(
                                xb + 0.5 >= xa + wa.min(8.0),
                                "{tabs_id}: painted chips overlap — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                            );
                        }
                    }
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn forms_date_time_pickers_tabs_mount_only_active() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    fn find_contains(el: &rui::core::tree::Element, needle: &str) -> bool {
                        el.template_id
                            .as_deref()
                            .is_some_and(|id| id.contains(needle))
                            || el.children.iter().any(|c| find_contains(c, needle))
                    }
                    assert!(
                        find_contains(&tree, "gal-dp-kinds"),
                        "forms must host Date/Time kinds under Tabs"
                    );
                    assert!(
                        find_contains(&tree, "gal-dp-kinds:content:date"),
                        "default date tab content placeholder"
                    );
                    assert!(
                        !find_contains(&tree, "gal-dt__dp_view"),
                        "inactive DateTime picker must not be an eager declarative sibling"
                    );
                    assert!(
                        !find_contains(&tree, "gal-time__tp_view"),
                        "inactive TimePicker must not be an eager declarative sibling"
                    );
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
    }

    #[test]
    fn forms_password_input_adornments_visible_when_focused() {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let id = "gal-s2-input-pw";
                use_signal(&rui::components::input_value_signal_key(id), String::new)
                    .set("secret1".into());
                use_signal("__rui_button_focus_id", String::new)
                    .set(format!("{}{id}", rui::components::INPUT_FOCUS_PREFIX));

                let mut tree =
                    rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                fn find_id<'a>(
                    el: &'a rui::core::tree::Element,
                    tid: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some(tid) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| find_id(c, tid))
                }
                let pw = find_id(&tree, id).expect("password input root");
                let clear = find_id(pw, &format!("__rui_input:{id}:clear"))
                    .expect("clear adornment must mount for clearable=\"true\"");
                let eye = find_id(pw, &format!("__rui_input:{id}:reveal"))
                    .expect("eye adornment must mount for password_toggle=\"true\"");
                assert!(
                    (clear.style.opacity - 1.0).abs() < 0.001,
                    "clear opacity={}, expected 1",
                    clear.style.opacity
                );
                assert!(
                    (eye.style.opacity - 1.0).abs() < 0.001,
                    "eye opacity={}, expected 1",
                    eye.style.opacity
                );

                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
                let lucide = result
                    .text_nodes
                    .iter()
                    .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
                    .filter(|n| n.style.color[3] > 0 && n.width > 0.5)
                    .count();
                assert!(
                    lucide >= 2,
                    "focused password input must collect visible lucide adornments; got {lucide}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn forms_section_mounts_wp_r7_reactive_chips() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| {
                    InputShowcase(InputShowcaseProps::default())
                });
                fn find_id(el: &rui::core::tree::Element, tid: &str) -> bool {
                    el.template_id.as_deref() == Some(tid)
                        || el.children.iter().any(|c| find_id(c, tid))
                }
                fn find_contains(el: &rui::core::tree::Element, needle: &str) -> bool {
                    el.template_id
                        .as_deref()
                        .is_some_and(|id| id.contains(needle))
                        || el.children.iter().any(|c| find_contains(c, needle))
                }
                assert!(
                    find_id(&tree, "gal-status-badge__component"),
                    "WP-R.7 ReactiveStatusBadge placeholder"
                );
                assert!(
                    find_id(&tree, "gal-count-chip__component"),
                    "WP-R.7 ReactiveCountChip placeholder"
                );
                assert!(
                    find_contains(&tree, ":bind#") || find_contains(&tree, ":bind_icon_name#"),
                    "InputShowcase under isolate_reads must keep auto bind leaves"
                );
                assert!(
                    find_contains(&tree, ":Show#"),
                    "<Show when={{badge_ok.get()}}> must lower under isolate_reads"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn forms_section_mounts_wp39_submit() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| {
                    FormValidateShowcase(FormValidateShowcaseProps::default())
                });
                fn find_id(el: &rui::core::tree::Element, tid: &str) -> bool {
                    el.template_id.as_deref() == Some(tid)
                        || el.children.iter().any(|c| find_id(c, tid))
                }
                assert!(
                    find_id(&tree, "gal-wp39-submit"),
                    "WP-3.9 card must mount submit (gal-wp39-submit)"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn data_section_mounts_wp310_masonry() {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| {
                    VirtualMasonryShowcase(VirtualMasonryShowcaseProps::default())
                });
                fn find_id(el: &rui::core::tree::Element, tid: &str) -> bool {
                    el.template_id.as_deref() == Some(tid)
                        || el.children.iter().any(|c| find_id(c, tid))
                }
                assert!(
                    find_id(&tree, GAL_WP310_MASONRY),
                    "WP-3.10 card must mount masonry (gal-wp310-masonry)"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    fn set_scroll_offset(el: &mut rui::core::tree::Element, rest: &str, offset: [f32; 2]) -> bool {
        let want = format!("scroll:{rest}");
        if el.focus_id.as_deref() == Some(want.as_str()) {
            el.scroll_offset = offset;
            return true;
        }
        el.children
            .iter_mut()
            .any(|c| set_scroll_offset(c, rest, offset))
    }

    fn scroll_metrics_xy(
        result: &rui::core::tree::RenderResult,
        rest: &str,
    ) -> ([f32; 4], [f32; 2], [f32; 2]) {
        result
            .scroll_metrics
            .iter()
            .find(|(id, ..)| id == rest)
            .map(|m| (m.1, m.2, m.3))
            .unwrap_or_else(|| panic!("missing scroll metrics for {rest}"))
    }

    #[test]
    fn data_section_scroll_metrics_stable_across_offsets() {
        // Gallery data page: charts (overflow:visible ticks), nested Table/Tree/Masonry
        // scrollports, and CarouselStack. Outer `scroll-data` content size must not
        // breathe as cards enter/leave the clip — that jitters the page thumb.
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let mut fs = rui::renderer::font_loader::create_font_system();
            rui::renderer::font_loader::apply_default_families(fs.db_mut());
            rui::text::install_shared_font_system(&mut fs);
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    let mut tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("data"));
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let first = layout.build_and_layout(&mut tree, 900.0, 600.0, [-1.0, -1.0]);
                    let (vp0, max0, content0) = scroll_metrics_xy(&first, "scroll-data");
                    assert!(
                        content0[1] > vp0[3] + 8.0 && max0[1] > 8.0,
                        "data section must overflow the 600px viewport (content_h={} vp_h={} max_y={})",
                        content0[1],
                        vp0[3],
                        max0[1]
                    );

                    let samples: Vec<f32> = {
                        let step = (max0[1] / 8.0).max(40.0);
                        let mut v = vec![0.0_f32];
                        let mut y = step;
                        while y < max0[1] {
                            v.push(y);
                            y += step;
                        }
                        v.push(max0[1]);
                        v
                    };

                    for &sy in &samples {
                        assert!(
                            set_scroll_offset(&mut tree, "scroll-data", [0.0, sy]),
                            "scroll:scroll-data viewport must exist"
                        );
                        let fast = layout.build_and_layout_fast(
                            &mut tree,
                            900.0,
                            600.0,
                            [-1.0, -1.0],
                            true,
                            false,
                        );
                        let (vp, max, content) = scroll_metrics_xy(&fast, "scroll-data");
                        assert!(
                            (content[1] - content0[1]).abs() < 1.0
                                && (content[0] - content0[0]).abs() < 1.0
                                && (vp[2] - vp0[2]).abs() < 1.0
                                && (vp[3] - vp0[3]).abs() < 1.0
                                && (max[1] - max0[1]).abs() < 1.0
                                && (max[0] - max0[0]).abs() < 1.0,
                            "scroll_only metrics jitter at sy={sy}: \
                             base content={content0:?} vp={vp0:?} max={max0:?}; \
                             now content={content:?} vp={vp:?} max={max:?}"
                        );
                    }

                    assert!(
                        set_scroll_offset(&mut tree, "scroll-data", [0.0, max0[1]]),
                        "scroll:scroll-data viewport must exist"
                    );
                    let full = layout.build_and_layout(&mut tree, 900.0, 600.0, [-1.0, -1.0]);
                    let (vp, max, content) = scroll_metrics_xy(&full, "scroll-data");
                    assert!(
                        (content[1] - content0[1]).abs() < 1.0
                            && (content[0] - content0[0]).abs() < 1.0
                            && (max[1] - max0[1]).abs() < 1.0,
                        "full collect at bottom must keep metrics: \
                         base content={content0:?} max={max0:?}; now content={content:?} max={max:?} vp={vp:?}"
                    );
                });
            });
            rui::app::reactive::reset_registered_anchors();
        });
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
                fn find_caption<'a>(
                    el: &'a mut rui::core::tree::Element,
                ) -> Option<&'a mut rui::core::tree::Element> {
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
    fn forms_section_does_not_crush_sidebar_width() {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                use_signal("gal_nav_section", || "basics".to_string()).set("forms".to_string());
                let mut app = build_gallery_app();
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 720.0, 700.0, [-1.0, -1.0]);
                fn find_id<'a>(
                    el: &'a rui::core::tree::Element,
                    id: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some(id) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| find_id(c, id))
                }
                let side = find_id(&app, "gal-sidebar").expect("gal-sidebar");
                let w = side.layout_bounds[2];
                assert!(
                    (w - 256.0).abs() < 1.5,
                    "forms page must not crush sidebar below 16rem; got {w}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn overlays_revisit_does_not_shrink_main_pane() {
        // <Portal> on the overlays page was hoisted to the root HStack each visit without
        // cleaning prior lifts → accumulating w/h 100% flex siblings crushed the ScrollArea.
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                fn find_id<'a>(
                    el: &'a rui::core::tree::Element,
                    id: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some(id) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| find_id(c, id))
                }
                let mut widths = Vec::new();
                for section in ["overlays", "basics", "overlays", "basics", "overlays"] {
                    rui::app::reactive::reset_registered_anchors();
                    use_signal("gal_nav_section", || "basics".to_string()).set(section.to_string());
                    let mut app = build_gallery_app();
                    let _ = rui::layout::layout_tree::hoist_portal_subtrees(&mut app);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
                    let pane = find_id(&app, &format!("scroll-{section}")).expect("scroll pane");
                    widths.push((section, pane.layout_bounds[2]));
                }
                let overlay_widths: Vec<f32> = widths
                    .iter()
                    .filter(|(s, _)| *s == "overlays")
                    .map(|(_, w)| *w)
                    .collect();
                assert!(
                    overlay_widths.len() == 3
                        && overlay_widths.iter().all(|w| *w > 700.0)
                        && (overlay_widths[0] - overlay_widths[2]).abs() < 2.0,
                    "overlays main width must stay wide across revisits; got {widths:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn gallery_main_pane_uses_remaining_width() {
        // toast_host must not flex-split the main column (reactive_placeholder is w/h 100%).
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                use_signal("gal_nav_section", || "basics".to_string());
                let mut app = build_gallery_app();
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
                fn find_id<'a>(
                    el: &'a rui::core::tree::Element,
                    id: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some(id) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| find_id(c, id))
                }
                fn find_slot<'a>(
                    el: &'a rui::core::tree::Element,
                    slot: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| find_slot(c, slot))
                }
                let side = find_id(&app, "gal-sidebar").expect("sidebar");
                let inset = find_slot(&app, "sidebar-inset").expect("sidebar-inset");
                let side_w = side.layout_bounds[2];
                let inset_w = inset.layout_bounds[2];
                assert!((side_w - 256.0).abs() < 1.5, "sidebar width, got {side_w}");
                assert!(
                    inset_w > 640.0,
                    "SidebarInset must flex-grow into remaining inset shell width; got {inset_w}"
                );
                assert!(
                    side_w + inset_w < 1000.0,
                    "inset margins/padding must not overflow root; got {}+{}",
                    side_w,
                    inset_w
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn basics_section_scrolls_when_viewport_shorter() {
        // Basics section content (~800px) must enable Y scroll when the ScrollArea
        // viewport is shorter — same failure mode as gallery after a window-height shrink
        // once surface size tracks Resized (see AppState::resolve_surface_size).
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);

        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let body = gallery_section_body("basics");
                let mut root = Element::from(
                    rui::components::scroll_area::scroll_area("diag-root")
                        .w("100%")
                        .h("100%")
                        .overflow_y("auto")
                        .overflow_x("hidden")
                        .child(body),
                );
                let mut layout = rui::core::tree::LayoutTree::new();
                let tall = layout.build_and_layout(&mut root, 600.0, 900.0, [-1.0, -1.0]);
                let (_, tall_vp, _, tall_content) = tall
                    .scroll_metrics
                    .iter()
                    .find(|(id, ..)| id == "diag-root")
                    .expect("diag-root metrics");
                assert!(
                    tall_content[1] <= tall_vp[3] + 0.5,
                    "at 900px tall viewport, basics should fit (content_h={} vp_h={})",
                    tall_content[1],
                    tall_vp[3]
                );

                let short = layout.build_and_layout(&mut root, 600.0, 500.0, [-1.0, -1.0]);
                let (_, short_vp, short_max, short_content) = short
                    .scroll_metrics
                    .iter()
                    .find(|(id, ..)| id == "diag-root")
                    .expect("diag-root metrics");
                assert!(
                    short_content[1] > short_vp[3] + 0.5 && short_max[1] > 1.0,
                    "at 500px viewport, basics must scroll (content_h={} vp_h={} max_y={})",
                    short_content[1],
                    short_vp[3],
                    short_max[1]
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

    fn run_on_large_stack<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(f)
            .expect("spawn gallery test thread")
            .join()
            .expect("gallery test thread");
    }

    #[test]
    fn headless_layout_all_sections_does_not_overflow() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let (_handle, _) = create_root(|| {
                create_effect(|| {
                    for key in [
                        "basics",
                        "forms",
                        "overlays",
                        "data",
                        "navigation",
                        "layout",
                        "text",
                        "about",
                    ] {
                        let mut tree = gallery_section_body(key);
                        let mut layout = rui::core::tree::LayoutTree::new();
                        let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
                    }
                });
            });
        });
    }

    fn find_tid(el: &rui::core::tree::Element, want: &str) -> bool {
        el.template_id.as_deref() == Some(want) || el.children.iter().any(|c| find_tid(c, want))
    }

    #[test]
    fn navigation_section_mounts_shell_router_view() {
        run_on_large_stack(|| {
            rui::app::reactive::reset_registered_anchors();
            let (_handle, tree) = create_root(|| gallery_section_body("navigation"));
            assert!(
                find_tid(&tree, "gal_shell_router__router_view"),
                "WP-3.5: navigation section must mount Router.view() placeholder"
            );
            assert!(
                find_tid(&tree, GAL_SHELL_TRAY_SHOW),
                "WP-3.6: navigation section must mount tray Show button"
            );
            assert!(
                find_tid(&tree, GAL_SHELL_FRAMELESS),
                "navigation section must mount frameless window button"
            );
            assert!(
                find_tid(&tree, GAL_SHELL_TRANSPARENT),
                "navigation section must mount transparent window button"
            );
            assert!(
                find_tid(&tree, GAL_SHELL_BLUR),
                "navigation section must mount blur window button"
            );
            rui::app::reactive::reset_registered_anchors();
        });
    }
}
