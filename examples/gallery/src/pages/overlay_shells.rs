//! WP-OVL.2 / WP-OVL.3：`anchored_overlay` 三种定位 + RSX/Builder 双写。

use crate::imports::*;
use crate::shared::*;
use rui::app::portal_overlay::OverlayKind;
use rui::primitives::r#box::vstack;
use rui::tokens;
use rui::utils::anchored_overlay::{anchored_overlay, drag_handle, FloatingStrategy, Placement};
use rui::utils::popup::{disarm_underlay_close, PopupPlacement};
use rui::utils::presence::presence;

fn shell_panel(title: &str, body: &str, extra: Element) -> Element {
    let mut panel: Element = vstack()
        .w("16rem")
        .p("0.75rem")
        .gap("0.5rem")
        .rounded("0.5rem")
        .into();
    panel = panel
        .bg(tokens::popover())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.12], 10.0);
    panel = panel.child(ui! {
        <VStack gap="0.35rem" w="100%">
            <Label font_weight="600" text_size="0.875rem">{ title.to_string() }</Label>
            <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                { body.to_string() }
            </Label>
            { extra }
        </VStack>
    });
    panel
}

#[component]
pub(crate) fn AnchoredOverlayShowcase() -> Element {
    let trig_open = use_signal("gal_ao_trig_open", || false);
    let trig_b_open = use_signal("gal_ao_trig_b_open", || false);
    let point_open = use_signal("gal_ao_point_open", || false);
    let point_pos = use_signal("gal_ao_point_pos", || [280.0_f32, 180.0]);
    let point_b_open = use_signal("gal_ao_point_b_open", || false);
    let point_b_pos = use_signal("gal_ao_point_b_pos", || [420.0_f32, 220.0]);
    let hud_rsx_open = use_signal("gal_ao_hud_rsx_open", || false);
    let hud_b_open = use_signal("gal_ao_hud_b_open", || false);

    let trig_b = {
        let open = trig_b_open;
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    id="gal-ao-trig-b-btn"
                    variant="outline"
                    size="sm"
                    on_click={move || {
                        let next = !open.peek();
                        if next {
                            disarm_underlay_close();
                        }
                        open.set(next);
                    }}
                >
                    { if open.get() { "关闭 Builder Trigger" } else { "打开 Builder Trigger" } }
                </Button>
                { Element::from(
                    anchored_overlay("gal-ao-trig-b")
                        .open(open)
                        .placement(Placement::Trigger {
                            focus_id: "gal-ao-trig-b-btn".into(),
                            placement: PopupPlacement::BottomStart,
                            gap: 4.0,
                            strategy: FloatingStrategy::Auto,
                        })
                        .kind(OverlayKind::Popover)
                        .child(move || {
                            shell_panel(
                                "Trigger · Builder",
                                "跟按钮走；Esc / 外点关闭走 portal 栈。",
                                {
                                    let open = open;
                                    ui! {
                                        <Button size="sm" on_click={move || open.set(false)}>"关闭"</Button>
                                    }
                                },
                            )
                        }),
                ) }
            </HStack>
        }
    };

    let point_b = {
        let open = point_b_open;
        let pos = point_b_pos;
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    variant="outline"
                    size="sm"
                    on_click={move || {
                        let next = !open.peek();
                        if next {
                            disarm_underlay_close();
                        }
                        open.set(next);
                    }}
                >
                    { if point_b_open.get() { "关闭 Builder Point" } else { "打开 Builder Point" } }
                </Button>
                { Element::from(
                    anchored_overlay("gal-ao-point-b")
                        .open(open)
                        .placement(Placement::Point { pos, gap: 0.0 })
                        .kind(OverlayKind::Popover)
                        .child(move || {
                            let header = drag_handle(
                                "gal-ao-point-b",
                                pos,
                                ui! {
                                    <HStack
                                        w="100%"
                                        items="center"
                                        justify="between"
                                        p="0.35rem 0.5rem"
                                        bg="muted"
                                        rounded="0.375rem"
                                        cursor="grab"
                                    >
                                        <Label text_size="0.75rem" font_weight="600">"Builder Point"</Label>
                                        <Icon name="grip-horizontal" size="14px" color="muted-foreground" pointer_events="none" />
                                    </HStack>
                                },
                            );
                            let mut panel: Element = vstack()
                                .w("15rem")
                                .p("0.5rem")
                                .gap("0.5rem")
                                .rounded("0.5rem")
                                .into();
                            panel = panel
                                .bg(tokens::popover())
                                .border_style("solid")
                                .border_width(1.0)
                                .border_color(tokens::border())
                                .shadow([0.0, 0.0, 0.0, 0.14], 12.0);
                            panel.child(header).child(
                                ui! {
                                    <VStack gap="0.35rem" w="100%">
                                        <Label text_size="0.75rem" color="muted-foreground">
                                            { format!(
                                                "({:.0}, {:.0})",
                                                pos.peek()[0],
                                                pos.peek()[1]
                                            ) }
                                        </Label>
                                        <Button size="sm" variant="outline" on_click={move || open.set(false)}>
                                            "关闭"
                                        </Button>
                                    </VStack>
                                },
                            )
                        }),
                ) }
            </HStack>
        }
    };

    let hud_b = {
        let open = hud_b_open;
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    id="gal-ao-hud-b-btn"
                    variant="outline"
                    size="sm"
                    on_click={move || {
                        let next = !open.peek();
                        if next {
                            disarm_underlay_close();
                        }
                        open.set(next);
                    }}
                >
                    { if open.get() { "关闭 Builder HUD" } else { "打开 Builder Unanchored" } }
                </Button>
                { Element::from(
                    anchored_overlay("gal-ao-hud-b")
                        .open(open)
                        .trigger_id("gal-ao-hud-b-btn")
                        .placement(Placement::Unanchored)
                        .kind(OverlayKind::Popover)
                        .child(move || {
                            let mut panel: Element = vstack()
                                .w("14rem")
                                .p("0.75rem")
                                .gap("0.35rem")
                                .rounded("0.5rem")
                                .into();
                            panel = panel
                                .fixed()
                                .top("1.25rem")
                                .right("1.25rem")
                                .bg(tokens::popover())
                                .border_style("solid")
                                .border_width(1.0)
                                .border_color(tokens::border())
                                .shadow([0.0, 0.0, 0.0, 0.16], 14.0);
                            panel.child(ui! {
                                <VStack gap="0.35rem" w="100%">
                                    <Label font_weight="600" text_size="0.875rem">"Unanchored · Builder"</Label>
                                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                                        "自管 fixed / inset；开关按钮登记为 trigger，避免外点对打。"
                                    </Label>
                                    <Button size="sm" on_click={move || open.set(false)}>"关闭"</Button>
                                </VStack>
                            })
                        }),
                ) }
            </HStack>
        }
    };

    let point_stage = {
        let pos = point_pos;
        let open = point_open;
        let mut stage: Element = rui::primitives::r#box::r#box()
            .w("100%")
            .min_h("8rem")
            .p("0.75rem")
            .rounded("0.5rem")
            .child(ui! {
                <Label text_size="0.75rem" color="muted-foreground">
                    "点击此处放置 Point 面板"
                </Label>
            })
            .into();
        stage = stage.bg(tokens::muted()).cursor("crosshair").on_mousedown(
            move |e: &rui::app::pointer_events::PointerEvent| {
                if e.phase == rui::app::pointer_events::EventPhase::Capture {
                    return;
                }
                pos.set([e.x, e.y]);
                disarm_underlay_close();
                open.set(true);
                e.stop_propagation();
            },
        );
        stage
    };

    ui! {
        <MyCard title="AnchoredOverlay 无头壳（Trigger / Point / Unanchored）">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utils::anchored_overlay：同一壳三种定位。不进 prelude。RSX 与 Builder 语义等价。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">"Trigger · RSX"</Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <AnchoredOverlay id="gal-ao-trig-rsx" openSignal={trig_open}>
                        <Button
                            id="gal-ao-trig-rsx-btn"
                            variant="default"
                            size="sm"
                            on_click={move || {
                                let next = !trig_open.peek();
                                if next {
                                    disarm_underlay_close();
                                }
                                trig_open.set(next);
                            }}
                        >
                            { if trig_open.get() { "关闭 RSX Trigger" } else { "打开 RSX Trigger" } }
                        </Button>
                        <AnchoredOverlayContent
                            side="bottom"
                            align="start"
                            sideOffset={4}
                            focusId="gal-ao-trig-rsx-btn"
                            kind="popover"
                        >
                            { shell_panel(
                                "Trigger · RSX",
                                "跟 focus_id 走 compute_anchored_placement。",
                                {
                                    let open = trig_open;
                                    ui! {
                                        <Button size="sm" on_click={move || open.set(false)}>"关闭"</Button>
                                    }
                                },
                            ) }
                        </AnchoredOverlayContent>
                    </AnchoredOverlay>
                </HStack>
                <Label font_weight="600" text_size="0.875rem">"Trigger · Builder"</Label>
                { trig_b }
                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Point · 点定位 + 标题栏拖拽"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "拖标题栏改 pos；collect 原点跟手，不订 pos 以免整岛重挂。"
                </Label>
                { point_stage }
                <AnchoredOverlay id="gal-ao-point-rsx" openSignal={point_open}>
                    <AnchoredOverlayContent placement="point" pos={point_pos} kind="popover">
                        { {
                            let header = drag_handle(
                                "gal-ao-point-rsx",
                                point_pos,
                                ui! {
                                    <HStack
                                        w="100%"
                                        items="center"
                                        justify="between"
                                        p="0.35rem 0.5rem"
                                        bg="muted"
                                        rounded="0.375rem"
                                        cursor="grab"
                                    >
                                        <Label text_size="0.75rem" font_weight="600">"RSX Point"</Label>
                                        <Icon name="grip-horizontal" size="14px" color="muted-foreground" pointer_events="none" />
                                    </HStack>
                                },
                            );
                            let mut panel: Element = vstack()
                                .w("15rem")
                                .p("0.5rem")
                                .gap("0.5rem")
                                .rounded("0.5rem")
                                .into();
                            panel = panel
                                .bg(tokens::popover())
                                .border_style("solid")
                                .border_width(1.0)
                                .border_color(tokens::border())
                                .shadow([0.0, 0.0, 0.0, 0.14], 12.0);
                            panel.child(header).child(
                                ui! {
                                    <Button size="sm" variant="outline" on_click={move || point_open.set(false)}>
                                        "关闭"
                                    </Button>
                                },
                            )
                        } }
                    </AnchoredOverlayContent>
                </AnchoredOverlay>
                { point_b }
                <Separator />
                <Label font_weight="600" text_size="0.875rem">"Unanchored · RSX"</Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <AnchoredOverlay id="gal-ao-hud-rsx" openSignal={hud_rsx_open}>
                        <Button
                            id="gal-ao-hud-rsx-btn"
                            variant="secondary"
                            size="sm"
                            on_click={move || {
                                let next = !hud_rsx_open.peek();
                                if next {
                                    disarm_underlay_close();
                                }
                                hud_rsx_open.set(next);
                            }}
                        >
                            { if hud_rsx_open.get() { "关闭 RSX HUD" } else { "打开 RSX Unanchored" } }
                        </Button>
                        <AnchoredOverlayContent
                            placement="unanchored"
                            focusId="gal-ao-hud-rsx-btn"
                            kind="popover"
                        >
                            { {
                                let mut panel: Element = vstack()
                                    .w("14rem")
                                    .p("0.75rem")
                                    .gap("0.35rem")
                                    .rounded("0.5rem")
                                    .into();
                                panel = panel
                                    .fixed()
                                    .bottom("1.25rem")
                                    .right("1.25rem")
                                    .bg(tokens::popover())
                                    .border_style("solid")
                                    .border_width(1.0)
                                    .border_color(tokens::border())
                                    .shadow([0.0, 0.0, 0.0, 0.16], 14.0);
                                panel.child(
                                    ui! {
                                        <VStack gap="0.35rem" w="100%">
                                            <Label font_weight="600" text_size="0.875rem">"Unanchored · RSX"</Label>
                                            <Label text_size="0.75rem" color="muted-foreground">
                                                "贴视口右下；作者 fixed + bottom/right，无视口舞台。"
                                            </Label>
                                            <Button size="sm" on_click={move || hud_rsx_open.set(false)}>
                                                "关闭"
                                            </Button>
                                        </VStack>
                                    },
                                )
                            } }
                        </AnchoredOverlayContent>
                    </AnchoredOverlay>
                </HStack>
                <Label font_weight="600" text_size="0.875rem">"Unanchored · Builder"</Label>
                { hud_b }
            </VStack>
        </MyCard>
    }
}

fn modal_card(title: &str, body: &str, close: Element) -> Element {
    let mut panel: Element = vstack()
        .w("20rem")
        .max_w("90dvw")
        .p("1.25rem")
        .gap("0.75rem")
        .rounded("0.75rem")
        .into();
    panel = panel
        .bg(tokens::card())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.2], 24.0);
    panel.child(ui! {
        <VStack gap="0.5rem" w="100%">
            <Label font_weight="600" text_size="1rem">{ title.to_string() }</Label>
            <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                { body.to_string() }
            </Label>
            { close }
        </VStack>
    })
}

#[component]
pub(crate) fn ViewportModalShowcase() -> Element {
    let dlg_open = use_signal("gal_vm_dlg_open", || false);
    let sheet_open = use_signal("gal_vm_sheet_open", || false);

    let sheet_b = {
        let open = sheet_open;
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    variant="outline"
                    size="sm"
                    on_click={move || {
                        let next = !open.peek();
                        if next {
                            disarm_underlay_close();
                        }
                        open.set(next);
                    }}
                >
                    { if open.get() { "关闭 Builder Sheet" } else { "打开 Builder ViewportModal" } }
                </Button>
                { Element::from(
                    rui::utils::viewport_modal::viewport_modal("gal-vm-sheet-b")
                        .open(open)
                        .overlay_opacity(0.3)
                        .motion(rui::utils::viewport_modal::ModalSurfaceMotion::SlideFromRight)
                        .child(move || {
                            let mut panel: Element = vstack()
                                .w("18rem")
                                .h("100%")
                                .p("1rem")
                                .gap("0.5rem")
                                .into();
                            panel = panel
                                .bg(tokens::card())
                                .border_style("solid")
                                .border_width(1.0)
                                .border_color(tokens::border())
                                .shadow([-8.0, 0.0, 0.0, 0.16], 18.0);
                            panel.child(ui! {
                                <VStack gap="0.5rem" w="100%">
                                    <Label font_weight="600">"ViewportModal · Builder"</Label>
                                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                                        "SlideFromRight；壳包视口舞台 + underlay。Esc / 点遮罩关闭。"
                                    </Label>
                                    <Button size="sm" on_click={move || open.set(false)}>"关闭"</Button>
                                </VStack>
                            })
                        }),
                ) }
            </HStack>
        }
    };

    ui! {
        <MyCard title="ViewportModal 无头壳（居中 Dialog / 贴边 Sheet）">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utils::viewport_modal：underlay + 视口舞台 + presence。不进 prelude。RSX 与 Builder 语义等价。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">"DialogFade · RSX"</Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <ViewportModal id="gal-vm-dlg-rsx" openSignal={dlg_open} overlayOpacity={0.28}>
                        <Button
                            variant="default"
                            size="sm"
                            on_click={move || {
                                let next = !dlg_open.peek();
                                if next {
                                    disarm_underlay_close();
                                }
                                dlg_open.set(next);
                            }}
                        >
                            { if dlg_open.get() { "关闭 RSX 模态" } else { "打开 RSX ViewportModal" } }
                        </Button>
                        <ViewportModalContent motion="dialog-fade">
                            { modal_card(
                                "ViewportModal · RSX",
                                "居中卡片；点遮罩 / Esc 走 Modal 栈。壳不写产品 chrome。",
                                {
                                    let open = dlg_open;
                                    ui! {
                                        <Button size="sm" on_click={move || open.set(false)}>"关闭"</Button>
                                    }
                                },
                            ) }
                        </ViewportModalContent>
                    </ViewportModal>
                </HStack>
                <Label font_weight="600" text_size="0.875rem">"SheetSlide · Builder"</Label>
                { sheet_b }
            </VStack>
        </MyCard>
    }
}

fn local_scrim_key_scene() -> Element {
    ui! {
        <Box
            id="gal-local-scrim-host"
            relative=true
            w="22rem"
            h="14rem"
            rounded="0.75rem"
            bg="#2563eb"
            overflow="hidden"
        >
            <VStack w="100%" h="100%" p="0.35rem" gap="0.12rem" absolute=true inset="0" justify="center" z_index={0} pointer_events="none">
                <Label color="#fde047" text_size="1.15rem" font_weight="800" leading="tight" text_align="center">
                    "YELLOW 高对比 BLUR"
                </Label>
                <Label color="white" text_size="1.05rem" font_weight="800" leading="tight" text_align="center">
                    "WHITE ABCXYZ 1234567890"
                </Label>
                <Label color="#facc15" text_size="0.95rem" font_weight="800" leading="tight" text_align="center">
                    "The quick brown fox 衬线对比"
                </Label>
                <Label color="#fef08a" text_size="0.9rem" font_weight="700" leading="tight" text_align="center">
                    "遮罩下方应可见模糊，而非锐利字形"
                </Label>
                <Label color="#fde047" text_size="0.85rem" font_weight="800" leading="tight" text_align="center">
                    "BLUE HOST · 重复行校验 backdrop-blur"
                </Label>
                <Label color="white" text_size="0.85rem" font_weight="700" leading="tight" text_align="center">
                    "MMMMWWWW #### 8888 高对比色块"
                </Label>
                <Label color="#facc15" text_size="0.8rem" font_weight="700" leading="tight" text_align="center">
                    "CENTER RING TEXT · 应落在 75% 遮罩下"
                </Label>
            </VStack>
            <Label color="white" text_size="0.75rem" p="0.6rem" z_index={1} pointer_events="none">
                "蓝色宿主（四周 12.5% 露出）"
            </Label>
            <Portal id="gal-local-scrim" mount="gal-local-scrim-host">
                <Box w="100%" h="100%" items="center" justify="center" z_index={2}>
                    <Box
                        w="75%"
                        h="75%"
                        rounded="0.65rem"
                        bg="rgba(255,255,255,0.2)"
                        backdrop_blur="0.125"
                        items="center"
                        justify="center"
                    >
                        <Box
                            w="50%"
                            h="50%"
                            rounded="0.5rem"
                            bg="black"
                            items="center"
                            justify="center"
                            p="0.5rem"
                        >
                            <Label color="white" text_size="0.75rem" font_weight="600" text_align="center">
                                "遮罩 50% · 上层黑底"
                            </Label>
                        </Box>
                    </Box>
                </Box>
            </Portal>
        </Box>
    }
}

fn presence_demo_card(title: &str, body: &str) -> Element {
    let mut panel: Element = vstack()
        .w("12rem")
        .p("0.75rem")
        .gap("0.35rem")
        .rounded("0.5rem")
        .into();
    panel = panel
        .bg(tokens::popover())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.12], 10.0);
    panel.child(ui! {
        <VStack gap="0.35rem" w="100%">
            <Label font_weight="600" text_size="0.875rem">{ title.to_string() }</Label>
            <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                { body.to_string() }
            </Label>
        </VStack>
    })
}

#[component]
pub(crate) fn PresenceShowcase() -> Element {
    let instant_open = use_signal("gal_pres_instant_open", || false);
    let presence_open = use_signal("gal_pres_exit_open", || false);

    let instant_b = {
        let open = instant_open;
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    variant="outline"
                    size="sm"
                    on_click={move || open.set(!open.peek())}
                >
                    { if open.get() { "关闭 Builder 即时卸树" } else { "打开 Builder 即时卸树" } }
                </Button>
                <Show when={open.get()}>
                    { presence_demo_card(
                        "即时卸树 · Builder",
                        "open=false 后子树立刻消失，无退场窗口。",
                    ) }
                </Show>
            </HStack>
        }
    };

    let presence_rsx = presence("gal-pres-exit-rsx", presence_open, 320, move || {
        let exiting = !presence_open.peek();
        let mut card = presence_demo_card(
            "presence() · RSX",
            "仍挂载至 exit_ms；作者用 opacity 播退场。",
        );
        card = card
            .opacity(if exiting { 0.0 } else { 1.0 })
            .transition("opacity 320ms ease");
        card
    });

    let presence_b = {
        let open = presence_open;
        let panel = presence("gal-pres-exit-b", open, 320, move || {
            let exiting = !open.peek();
            let mut card = presence_demo_card(
                "presence() · Builder",
                "open=false 后仍挂载 320ms；opacity 过渡模拟退场。",
            );
            card = card
                .opacity(if exiting { 0.0 } else { 1.0 })
                .transition("opacity 320ms ease");
            card
        });
        ui! {
            <HStack gap="0.5rem" items="center" wrap="wrap">
                <Button
                    variant="outline"
                    size="sm"
                    on_click={move || open.set(!open.peek())}
                >
                    { if open.get() { "关闭 Builder presence" } else { "打开 Builder presence" } }
                </Button>
                { panel }
            </HStack>
        }
    };

    ui! {
        <MyCard title="Presence：即时卸树 vs 退场保留">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utils::presence::presence：show=false 进入 Exiting，墙钟到期再 Absent。对比 Show when / if 的即时卸树。"
                </Label>
                <Label font_weight="600" text_size="0.875rem">"即时卸树 · RSX"</Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <Button
                        variant="default"
                        size="sm"
                        on_click={move || instant_open.set(!instant_open.peek())}
                    >
                        { if instant_open.get() { "关闭 RSX 即时卸树" } else { "打开 RSX 即时卸树" } }
                    </Button>
                    <Show when={instant_open.get()}>
                        { presence_demo_card(
                            "即时卸树 · RSX",
                            "条件块 false 后子树当帧卸掉。",
                        ) }
                    </Show>
                </HStack>
                <Label font_weight="600" text_size="0.875rem">"即时卸树 · Builder"</Label>
                { instant_b }
                <Separator />
                <Label font_weight="600" text_size="0.875rem">"presence() 退场 · RSX"</Label>
                <HStack gap="0.5rem" items="center" wrap="wrap">
                    <Button
                        variant="secondary"
                        size="sm"
                        on_click={move || presence_open.set(!presence_open.peek())}
                    >
                        { if presence_open.get() { "关闭 RSX presence" } else { "打开 RSX presence" } }
                    </Button>
                    { presence_rsx }
                </HStack>
                <Label font_weight="600" text_size="0.875rem">"presence() 退场 · Builder"</Label>
                { presence_b }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn LocalOverlayShowcase() -> Element {
    ui! {
        <MyCard title="局部遮罩关键场景（Portal mount，非整窗 overlay）">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "overlay() 只做全屏 underlay。容器内遮罩：relative 宿主 overflow hidden + Portal 写在宿主内并 mount 回该 id。蓝底 100%；以其为中心的 75% 白色半透明圆角遮罩（backdrop-blur，与上层黑底区分）；遮罩中心再叠 50% 圆角黑底 + 白字 Label。局部 mount 随宿主滚动，不能溢出宿主。"
                </Label>
                { local_scrim_key_scene() }
            </VStack>
        </MyCard>
    }
}
