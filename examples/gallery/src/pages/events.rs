//! 指针事件 / 光标。

use crate::imports::*;
use crate::shared::*;

/// Fixed rows for the MouseEvent readout. Variable wrap used to grow the card
/// on every `mousemove` (digit width / `relatedTarget` Debug) and collapse the page.
#[cfg(test)]
pub(crate) const MOUSE_LOG_LINES: usize = 7;
/// 7×12px×leading-normal(1.5) + 0.75rem×2 padding; keep in sync with mouse log row count.
pub(crate) const MOUSE_LOG_PANEL_H: f32 = 152.0;

pub(crate) fn clip_mouse_debug(s: Option<&str>) -> String {
    let t = s.unwrap_or("-");
    let n = t.chars().count();
    if n <= 18 {
        format!("{t:<18}")
    } else {
        let head: String = t.chars().take(17).collect();
        format!("{head}…")
    }
}

pub(crate) fn format_mouse_log_fields(
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

pub(crate) fn format_mouse_log(e: &MouseEvent) -> String {
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

pub(crate) fn mouse_log_placeholder() -> String {
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
pub(crate) fn MouseEventShowcase() -> Element {
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

pub(crate) fn ensure_gallery_custom_cursors() {
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
pub(crate) fn CursorShowcase() -> Element {
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
