//! 导航、路由、托盘、窗口外壳。

use crate::imports::*;
use crate::shared::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GalNavRoute {
    Home,
    Detail,
    SheetHelp,
}

#[component]
pub(crate) fn NavigatorShowcase() -> Element {
    let nav = Navigator::new("gal_nav_demo", GalNavRoute::Home, [GalNavRoute::Home]);
    let nav_bar = nav.clone();
    let nav_detail = nav.clone();
    let nav_sheet = nav.clone();
    let nav_home = nav.clone();

    let body = jadix::reactive!(NAVIGATOR_SHOWCASE_REACTIVE, move || {
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

pub(crate) fn gallery_shell_router(id: &'static str) -> Router {
    let mut router = Router::new(id, "inbox");
    router.register("inbox", || {
        ui! { <Label text_size="0.875rem">"Inbox pane"</Label> }
    });
    router.register("settings", || {
        ui! { <Label text_size="0.875rem">"Settings pane"</Label> }
    });
    router
}

pub(crate) fn gallery_secondary_shell_root() -> Element {
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
                    <Select id="gal-sec-select" value={fruit} placeholder="Fruit">
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
pub(crate) fn RouterShowcase() -> Element {
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
                            match open_secondary_window_with(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Shell")
                                    .window_label("gallery-shell"),
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
    static GAL_TRAY: RefCell<Option<jadix::app::TrayIcon>> = const { RefCell::new(None) };
    static GAL_APP_ICON: RefCell<Option<jadix::app::AppIcon>> = const { RefCell::new(None) };
}

#[component]
pub(crate) fn TrayShowcase() -> Element {
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
                                        .icon(GAL_APP_ICON.with(|slot| slot.borrow().clone()).unwrap_or_else(crate::gallery_icon))
                                        .tooltip("Jadix Gallery")
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
                    <Button
                        id="gal_shell_icon_pick"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            let Some(path) = jadix::utils::file_dialog::pick_file(".png,.ico,.webp,.jpg,.svg") else { return; };
                            match jadix::app::AppIcon::from_path(path) {
                                Ok(icon) => {
                                    GAL_APP_ICON.with(|slot| *slot.borrow_mut() = Some(icon.clone()));
                                    let result = jadix::app::current_window()
                                        .ok_or_else(|| "No active window".to_string())
                                        .and_then(|window| window.set_icon(Some(icon.clone())));
                                    if let Err(e) = result { toast_push(&e, 3000); return; }
                                    GAL_TRAY.with(|slot| {
                                        if let Some(tray) = slot.borrow().as_ref().filter(|t| t.is_native()) {
                                            if let Err(e) = tray.set_icon(icon) { toast_push(&e, 3000); }
                                        }
                                    });
                                    toast_push("Window / existing tray icon updated", 2000);
                                }
                                Err(e) => { toast_push(&e, 3000); },
                            }
                        }}
                    >
                        "Choose icon…"
                    </Button>
                    <Button
                        id="gal_shell_icon_reset"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            let icon = crate::gallery_icon();
                            GAL_APP_ICON.with(|slot| *slot.borrow_mut() = None);
                            if let Some(window) = jadix::app::current_window() {
                                if let Err(e) = window.set_icon(Some(icon.clone())) { toast_push(&e, 3000); }
                            }
                            GAL_TRAY.with(|slot| {
                                if let Some(tray) = slot.borrow().as_ref().filter(|t| t.is_native()) {
                                    if let Err(e) = tray.set_icon(icon) { toast_push(&e, 3000); }
                                }
                            });
                        }}
                    >
                        "Reset icon"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn gallery_chrome_fill(alpha: f32) -> String {
    let c = parse_color("background");
    gallery_rgba(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        alpha,
    )
}

/// CSS 8-digit hex (`#RRGGBBAA`). Do not use `#AARRGGBB` here — that is `glass_tint` only.
fn gallery_rgba(r: u8, g: u8, b: u8, alpha: f32) -> String {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

pub(crate) fn gallery_custom_title_bar(title: &'static str) -> Element {
    gallery_title_bar(title, gallery_chrome_fill(0.62))
}

fn gallery_title_bar(title: &'static str, fill: String) -> Element {
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
                <Icon name="minus" size="0.75rem" color="foreground" />
            </Button>
            <Button size="icon-xs" variant="ghost" on_click={move || toggle_window_maximized()}>
                <Icon name="square" size="0.75rem" color="foreground" />
            </Button>
            <Button
                size="icon-xs"
                variant="ghost"
                class="hover:bg-destructive hover:text-destructive-foreground dark:hover:bg-destructive dark:hover:text-destructive-foreground"
                on_click={move || close_window()}
            >
                <Icon name="x" size="0.75rem" color="foreground" />
            </Button>
        </HStack>
    }
}

pub(crate) fn gallery_open_chrome(options: WindowOptions, root: fn() -> Element, ok: &str) {
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

pub(crate) fn gallery_frameless_root() -> Element {
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

pub(crate) fn gallery_transparent_root() -> Element {
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

pub(crate) fn gallery_blur_root() -> Element {
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

pub(crate) fn gallery_recipe_root(
    title: &'static str,
    body: &'static str,
    fill: String,
    bar: String,
) -> Element {
    ui! {
        <VStack w="100%" h="100%" bg={fill.as_str()}>
            { gallery_title_bar(title, bar) }
            <VStack flex_grow="1" p="1rem" gap="0.5rem">
                <Label font_weight="700">{ title }</Label>
                <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                    { body }
                </Label>
            </VStack>
        </VStack>
    }
}

#[component]
pub(crate) fn WindowChromeShowcase() -> Element {
    ui! {
        <MyCard title="无边框 / 透明 / 模糊（外壳）">
            <VStack w="100%" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "次窗演示 WindowOptions：decorations(false)、transparent、blur。Blur 高斯+tint；Aero 薄洗色、失焦变灰、暗色换深蓝、反射视差；Acrylic HSV 亮度色；Mica / MicaAlt（去饱和）。需 cargo run -p gallery --features glass。"
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <Button
                        id="gal_shell_frameless"
                        size="sm"
                        variant="secondary"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Frameless")
                                    .window_label("gallery-frameless")
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
                                    .title("Jadix Gallery — Transparent")
                                    .window_label("gallery-transparent")
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
                                    .title("Jadix Gallery — Blur")
                                    .window_label("gallery-blur")
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
                    <Button
                        id="gal_shell_glass_blur"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Custom Blur")
                                    .size(460.0, 320.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true)
                                    .backdrop(WindowBackdrop::Blur)
                                    .blur_amount(24.0)
                                    .glass_tint("#28FFFFFF"),
                                || gallery_recipe_root(
                                    "Blur recipe",
                                    "高斯 + 淡 tint 叠层。后面的桌面/窗口应仍能透过。",
                                    gallery_rgba(255, 255, 255, 0.16),
                                    gallery_rgba(247, 248, 250, 0.48),
                                ),
                                "Custom blur opened",
                            );
                        }}
                    >
                        "Open Blur recipe"
                    </Button>
                    <Button
                        id="gal_shell_glass_aero"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Aero")
                                    .size(460.0, 320.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true)
                                    .backdrop(WindowBackdrop::Aero)
                                    .blur_amount(16.0)
                                    .glass_tint("#284FC3F7")
                                    .glass_tint_dark("#C0101828")
                                    .aero_afterglow("#40A8E8FF")
                                    .aero_color_balance(0.16)
                                    .aero_afterglow_balance(0.28)
                                    .aero_blur_balance(0.49)
                                    .aero_reflection(0.32)
                                    .aero_parallax(0.12),
                                || gallery_recipe_root(
                                    "Aero",
                                    "霜化桌面 + 薄天蓝洗色；失焦变灰、暗色换深蓝；拖动有高光视差。",
                                    gallery_rgba(79, 195, 247, 0.03),
                                    gallery_rgba(79, 195, 247, 0.03),
                                ),
                                "Aero opened",
                            );
                        }}
                    >
                        "Open Aero"
                    </Button>
                    <Button
                        id="gal_shell_glass_acrylic"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Acrylic recipe")
                                    .size(460.0, 320.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true)
                                    .backdrop(WindowBackdrop::Acrylic)
                                    .blur_amount(22.0)
                                    .glass_tint("#50FFFFFF"),
                                || gallery_recipe_root(
                                    "Acrylic recipe",
                                    "未指定 luminosity 时按 HSV-V 推导亮度色，乳白应比纯叠一层更干净。",
                                    gallery_rgba(244, 246, 248, 0.28),
                                    gallery_rgba(242, 242, 242, 0.52),
                                ),
                                "Acrylic recipe opened",
                            );
                        }}
                    >
                        "Open Acrylic recipe"
                    </Button>
                    <Button
                        id="gal_shell_glass_mica"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Mica recipe")
                                    .size(460.0, 320.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true)
                                    .backdrop(WindowBackdrop::Mica)
                                    .blur_amount(24.0)
                                    .luminosity_opacity(0.48)
                                    .glass_tint("#78E8E4DE"),
                                || gallery_recipe_root(
                                    "Mica recipe",
                                    "壁纸材质、偏暖灰。应半透壁纸；拖到主窗上不应透视主窗（失败则降级 Acrylic）。",
                                    gallery_rgba(237, 232, 228, 0.30),
                                    gallery_rgba(232, 228, 222, 0.55),
                                ),
                                "Mica recipe opened",
                            );
                        }}
                    >
                        "Open Mica recipe"
                    </Button>
                    <Button
                        id="gal_shell_glass_mica_alt"
                        size="sm"
                        variant="outline"
                        on_click={move || {
                            gallery_open_chrome(
                                WindowOptions::new()
                                    .title("Jadix Gallery — Mica Alt")
                                    .size(460.0, 320.0)
                                    .frameless()
                                    .transparent(true)
                                    .blur(true)
                                    .backdrop(WindowBackdrop::MicaAlt)
                                    .glass_tint("#70C8C4C0")
                                    .glass_tint_dark("#80201818"),
                                || gallery_recipe_root(
                                    "Mica Alt",
                                    "去饱和灰调云母。比 Mica 更素，壁纸色彩应被压掉。",
                                    gallery_rgba(200, 198, 196, 0.32),
                                    gallery_rgba(188, 186, 184, 0.55),
                                ),
                                "Mica Alt opened",
                            );
                        }}
                    >
                        "Open Mica Alt"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn SidebarShowcase() -> Element {
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
        jadix::app::reactive::bind_prop(
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
                                chrome: Default::default(),
                            },
                            TreeNode {
                                id: "team".into(),
                                label: "Team".into(),
                                children: vec![],
                                chrome: Default::default(),
                            },
                        ],
                        chrome: Default::default(),
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
                            <Sidebar id="gal-sidebar-demo-a" rail_w="14rem">
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
                            <Sidebar id="gal-sidebar-demo-tree-a" rail_w="14rem">
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
pub(crate) fn NavbarNavigationMenuShowcase() -> Element {
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
            .brand(label("").text("Jadix").font_weight(600.0))
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
                        <Label font_weight="600">"Jadix"</Label>
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
pub(crate) fn PaginationShowcase() -> Element {
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
    let rsx_bar = jadix::reactive!("examples/gallery/PaginationShowcase:rsx", move || {
        let page = page_a.get();
        let mut items: Vec<Element> = Vec::new();
        items.push(ui! {
            <PaginationItem>
                <PaginationPrevious
                    id="gal-pg-a-prev"
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
                    disabled={page >= COUNT_A}
                    on_click={move || {
                        page_a.update(|p| *p = (*p + 1).min(COUNT_A));
                    }}
                />
            </PaginationItem>
        });
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
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
        .jumper(true)
        .into();

    let status_a = jadix::reactive!("examples/gallery/PaginationShowcase:status_a", move || {
        let t = jadix::tr_with_default_zh(
            "gallery.pagination.status_a",
            "A · Page {page} / {pages} (left=1 sibling=1 right=1)",
            "A · 第 {page} / {pages} 页（left=1 sibling=1 right=1）",
            &[
                ("page", &page_a.get().to_string()),
                ("pages", &COUNT_A.to_string()),
            ],
        );
        ui! { <Label text_size="0.875rem">{ t }</Label> }
    });
    let status_b = jadix::reactive!("examples/gallery/PaginationShowcase:status_b", move || {
        let t = jadix::tr_with_default_zh(
            "gallery.pagination.status_b",
            "B · Page {page} / {pages} (left=3 sibling=2 right=4 · auto-fit · jumper)",
            "B · 第 {page} / {pages} 页（left=3 sibling=2 right=4 · auto-fit · jumper）",
            &[
                ("page", &page_b.get().to_string()),
                ("pages", &COUNT_B.to_string()),
            ],
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
pub(crate) fn MenubarShowcase() -> Element {
    ui! {
        <MyCard title="Menubar">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "应用命令栏（复合 Menubar，对齐 solid-ui / ShadCN）：←→ 切换菜单；打开后悬停可切换。"
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
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn StepperShowcase() -> Element {
    let step = use_signal("gal_stepper", || 0usize);
    let step_v = use_signal("gal_stepper_v", || 0usize);
    let builder_el: Element = stepper()
        .id("gal-stepper-builder")
        .current_signal(step)
        .steps([
            ("Account", "Email and password"),
            ("Profile", "Your details"),
            ("Confirm", "Review and submit"),
        ])
        .into();

    ui! {
        <MyCard title="Stepper">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "编号步骤条（Jadix 自研，ShadCN 官方无此控件）：主色圆点 + 勾选完成态 + muted 待办；连线与圆点中线对齐。可点圆点或用 Prev/Next。"
                </Label>

                <Label text_size="0.75rem" font_weight="600">"A. RSX"</Label>
                <Stepper current={step} orientation="horizontal">
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Account"</StepperTitle>
                        <StepperDescription>"Email and password"</StepperDescription>
                    </StepperItem>
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Profile"</StepperTitle>
                        <StepperDescription>"Your details"</StepperDescription>
                    </StepperItem>
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Confirm"</StepperTitle>
                        <StepperDescription>"Review and submit"</StepperDescription>
                    </StepperItem>
                </Stepper>
                <HStack gap="0.5rem" items="center">
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step.peek();
                        if s > 0 { step.set(s - 1); }
                    }}>"Prev"</Button>
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step.peek();
                        if s < 2 { step.set(s + 1); }
                    }}>"Next"</Button>
                    <Label text_size="0.875rem">{ format!("Step {}", step.get() + 1) }</Label>
                </HStack>

                <Label text_size="0.75rem" font_weight="600">"B. Builder"</Label>
                { builder_el }

                <Label text_size="0.75rem" font_weight="600">"C. Vertical"</Label>
                <Stepper current={step_v} orientation="vertical">
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Cart"</StepperTitle>
                        <StepperDescription>"Review items"</StepperDescription>
                    </StepperItem>
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Shipping"</StepperTitle>
                        <StepperDescription>"Address and method"</StepperDescription>
                    </StepperItem>
                    <StepperItem>
                        <StepperIndicator />
                        <StepperTitle>"Payment"</StepperTitle>
                        <StepperDescription>"Card or wallet"</StepperDescription>
                    </StepperItem>
                </Stepper>
                <HStack gap="0.5rem" items="center">
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step_v.peek();
                        if s > 0 { step_v.set(s - 1); }
                    }}>"Prev"</Button>
                    <Button size="sm" variant="outline" on_click={move || {
                        let s = step_v.peek();
                        if s < 2 { step_v.set(s + 1); }
                    }}>"Next"</Button>
                    <Label text_size="0.875rem">{ format!("Step {}", step_v.get() + 1) }</Label>
                </HStack>
            </VStack>
        </MyCard>
    }
}
