//! SVG / Canvas / Scene3d / Shader。

use crate::imports::*;
use crate::shared::*;

#[component]
pub(crate) fn DrawingSvgShowcase() -> Element {
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
pub(crate) fn LucideAnimatedShowcase() -> Element {
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

const MORPH_PLAYGROUND_ICONS: &[&str] = &[
    "x",
    "plus",
    "arrow-right",
    "arrow-down",
    "check",
    "chevron-down",
    "heart",
    "star",
];

/// morphicons.com playground caption: `from → to` + Procrustes θ / residual.
fn morph_plan_caption(from: &str, to: &str) -> (String, String, String) {
    use jadix::utils::icon_morph::{build_plan, icon_input_from_src, resample_icon, SAMPLE_N};
    let pair = format!("{from} → {to}");
    let empty = (pair.clone(), String::new(), String::new());
    let Some(a) = icon_input_from_src(from) else {
        return empty;
    };
    let Some(b) = icon_input_from_src(to) else {
        return empty;
    };
    let Ok(sa) = resample_icon(&a, SAMPLE_N) else {
        return empty;
    };
    let Ok(sb) = resample_icon(&b, SAMPLE_N) else {
        return empty;
    };
    let Ok(plan) = build_plan(&sa, &sb) else {
        return empty;
    };
    if plan.items.is_empty() {
        return empty;
    }
    let degs: Vec<String> = plan
        .items
        .iter()
        .take(4)
        .map(|it| format!("{:.0}°", it.theta.abs() * 180.0 / std::f64::consts::PI))
        .collect();
    let n = plan.items.len() as f64;
    let res = plan.items.iter().map(|it| it.res).sum::<f64>() / n.max(1.0);
    let max_res = plan.items.iter().map(|it| it.res).fold(0.0_f64, f64::max);
    let max_th = plan
        .items
        .iter()
        .map(|it| it.theta.abs())
        .fold(0.0_f64, f64::max);
    let kind = if max_res < 0.05 && max_th > 0.2 {
        "pure rotation, emergent"
    } else if max_res < 0.05 {
        "near identity"
    } else {
        "polar morph"
    };
    (
        pair,
        format!("θ [{}] res {:.3}", degs.join(" "), res),
        kind.to_string(),
    )
}

#[component]
fn MorphPairButton(id: String, a: String, b: String, spring: String) -> Element {
    let key = format!("gal_morph_pair_{id}");
    let on = use_signal(&key, || false);
    let a_s = a.clone();
    let b_s = b.clone();
    ui! {
        <VStack items="center" gap="0.25rem" min_w="4.75rem">
            <Button variant="outline" size="icon" on_click={move || on.set(!on.peek())}>
                <MorphIcon
                    id={format!("gal-morph-pair-{id}")}
                    icon={if on.get() { b.clone() } else { a.clone() }}
                    size={24.0}
                    spring={spring.clone()}
                />
            </Button>
            <Label text_size="0.6rem" color="muted-foreground" class="whitespace-nowrap">
                { format!("{a_s}↔{b_s}") }
            </Label>
        </VStack>
    }
}

/// Exclusive icon picker. ToggleGroup paint-syncs pressed chrome (bg/fg) so
/// selection does not lag behind a hand-written Button `variant={…}` bind.
#[component]
fn MorphIconPickers() -> Element {
    let from = use_signal("gal_morph_pg_from", || "x".to_string());
    let to = use_signal("gal_morph_pg_to", || "plus".to_string());
    let t = use_signal("gal_morph_pg_t", || 1.0_f32);
    let scrubbing = use_signal("gal_morph_pg_scrubbing", || false);
    let seen = use_signal("gal_morph_pg_to_seen", || to.peek());
    let _ = create_effect(move || {
        let next = to.get();
        let prev = seen.peek();
        if prev == next {
            return;
        }
        from.set(prev);
        seen.set(next);
        t.set(1.0);
        scrubbing.set(false);
    });
    ui! {
        <ToggleGroup
            id="gal-morph-pg-icons"
            type="single"
            value={to}
            default_value="plus"
            variant="outline"
            size="sm"
            required={true}
        >
            { MORPH_PLAYGROUND_ICONS
                .iter()
                .map(|name| {
                    let name = *name;
                    ui! {
                        <ToggleGroupItem value={name}>
                            <Icon name={name} size="20px" pointer_events="none" />
                        </ToggleGroupItem>
                    }
                })
                .collect::<Vec<_>>() }
        </ToggleGroup>
    }
}

/// Own effect tree: play↔pause morph must not share the playground card effect.
#[component]
fn MorphPlayPauseButton() -> Element {
    let on = use_signal("gal_morph_pg_playpause", || false);
    let spring = use_signal("gal_morph_pg_spring", || "snappy".to_string());
    let stroke = use_signal("gal_morph_pg_stroke", || "2".to_string());
    ui! {
        <Button
            id="gal-morph-pg-playpause-btn"
            variant="outline"
            size="icon"
            on_click={move || on.set(!on.peek())}
        >
            <MorphIcon
                id="gal-morph-pg-playpause"
                icon={if on.get() { "play" } else { "pause" }}
                size={18.0}
                spring={spring.get()}
                stroke_width={stroke.get().parse::<f32>().unwrap_or(2.0)}
            />
        </Button>
    }
}

#[component]
pub(crate) fn MorphIconShowcase() -> Element {
    let from = use_signal("gal_morph_pg_from", || "x".to_string());
    let to = use_signal("gal_morph_pg_to", || "plus".to_string());
    let t = use_signal("gal_morph_pg_t", || 1.0_f32);
    let spring = use_signal("gal_morph_pg_spring", || "snappy".to_string());
    let stroke = use_signal("gal_morph_pg_stroke", || "2".to_string());
    let interp = use_signal("gal_morph_pg_interp", || "polar".to_string());
    let scrubbing = use_signal("gal_morph_pg_scrubbing", || false);

    let menu = use_signal("gal_morph_menu", || false);
    let play = use_signal("gal_morph_play", || false);
    let sun = use_signal("gal_morph_sun", || false);
    let copied = use_signal("gal_morph_copy", || false);
    let eye = use_signal("gal_morph_eye", || false);

    let builder_el: Element = morph_icon("menu")
        .from_icon("menu")
        .to_icon("x")
        .progress(0.45)
        .size_px(32.0)
        .into();
    let svg_el: Element = {
        let from = jadix::utils::svg_to_icon(
            r#"<svg viewBox="0 0 24 24"><path d="M5 12h14" fill="none" stroke="black" stroke-width="2"/></svg>"#,
        );
        let to = jadix::utils::svg_to_icon(
            r#"<svg viewBox="0 0 24 24"><path d="M12 5v14" fill="none" stroke="black" stroke-width="2"/></svg>"#,
        );
        match (from, to) {
            (Ok(a), Ok(b)) => morph_icon(a.clone())
                .from_icon(a)
                .to_icon(b)
                .progress(0.5)
                .interp("linear")
                .size_px(32.0)
                .into(),
            _ => morph_icon("minus")
                .from_icon("minus")
                .to_icon("plus")
                .progress(0.5)
                .interp("linear")
                .size_px(32.0)
                .into(),
        }
    };

    ui! {
        <MyCard title="MorphIcon · 描边图标互变">
            <VStack w="100%" gap="1rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "任意两枚 Lucide 描边图标 morph。点选图标弹簧互变；拖滑杆进入受控 seek。polar/linear 对照插值。中间按钮演示 play↔pause。需 feature svg + bundled-lucide。"
                </Label>

                <VStack
                    w="100%"
                    items="center"
                    gap="0.65rem"
                    p="1.25rem"
                    bg="secondary"
                    rounded="0.75rem"
                    borderStyle="solid"
                    borderColor="border"
                    borderWidth="0.0625"
                >
                    <MorphIcon
                        id="gal-morph-pg-preview"
                        icon={to.get()}
                        from={if scrubbing.get() { from.get() } else { String::new() }}
                        to={if scrubbing.get() { to.get() } else { String::new() }}
                        progress={if scrubbing.get() { t.get() } else { -1.0 }}
                        size={72.0}
                        spring={spring.get()}
                        stroke_width={stroke.get().parse::<f32>().unwrap_or(2.0)}
                        interp={interp.get()}
                    />
                    <VStack items="center" gap="0.15rem">
                        <Label text_size="0.75rem" font_weight="600">{ morph_plan_caption(&from.get(), &to.get()).0 }</Label>
                        <Label text_size="0.65rem" color="muted-foreground" class="whitespace-nowrap">{ morph_plan_caption(&from.get(), &to.get()).1 }</Label>
                        <Label text_size="0.65rem" color="muted-foreground">{ morph_plan_caption(&from.get(), &to.get()).2 }</Label>
                    </VStack>
                    <HStack w="18rem" max_w="100%" items="center" gap="0.75rem">
                        <Slider
                            id="gal-morph-playground-t"
                            value={t}
                            min={0.0}
                            max={1.0}
                            step={0.01}
                            on_click={move || scrubbing.set(true)}
                        />
                        <Label
                            text_size="0.7rem"
                            color="muted-foreground"
                            class="whitespace-nowrap"
                            min_w="3.2rem"
                        >{ format!("t={:.2}", t.get()) }</Label>
                    </HStack>
                    <HStack gap="0.4rem" wrap="wrap" justify="center" items="center">
                        <MorphIconPickers />
                    </HStack>
                    <MorphPlayPauseButton />
                    <HStack gap="1rem" wrap="wrap" justify="center" items="start">
                        <VStack gap="0.35rem" items="center">
                            <Label text_size="0.65rem" color="muted-foreground">"spring"</Label>
                            <ToggleGroup
                                id="gal-morph-pg-spring"
                                value={spring}
                                default_value="snappy"
                                variant="outline"
                                size="sm"
                            >
                                <ToggleGroupItem value="snappy">"snappy"</ToggleGroupItem>
                                <ToggleGroupItem value="smooth">"smooth"</ToggleGroupItem>
                                <ToggleGroupItem value="bouncy">"bouncy"</ToggleGroupItem>
                            </ToggleGroup>
                        </VStack>
                        <VStack gap="0.35rem" items="center">
                            <Label text_size="0.65rem" color="muted-foreground">"stroke"</Label>
                            <ToggleGroup
                                id="gal-morph-pg-stroke"
                                value={stroke}
                                default_value="2"
                                variant="outline"
                                size="sm"
                            >
                                <ToggleGroupItem value="1">"1"</ToggleGroupItem>
                                <ToggleGroupItem value="1.5">"1.5"</ToggleGroupItem>
                                <ToggleGroupItem value="2">"2"</ToggleGroupItem>
                                <ToggleGroupItem value="2.5">"2.5"</ToggleGroupItem>
                            </ToggleGroup>
                        </VStack>
                        <VStack gap="0.35rem" items="center">
                            <Label text_size="0.65rem" color="muted-foreground">"interp"</Label>
                            <ToggleGroup
                                id="gal-morph-pg-interp"
                                value={interp}
                                default_value="polar"
                                variant="outline"
                                size="sm"
                            >
                                <ToggleGroupItem value="polar">"polar"</ToggleGroupItem>
                                <ToggleGroupItem value="linear">"linear"</ToggleGroupItem>
                            </ToggleGroup>
                        </VStack>
                    </HStack>
                </VStack>

                <Separator />

                <Label font_weight="600" text_size="0.875rem">"常见对 · 点击切换"</Label>
                <HStack w="100%" gap="0.65rem" wrap="wrap" items="start">
                    <Button
                        variant="outline"
                        size="icon"
                        on_click={move || menu.set(!menu.peek())}
                    >
                        <MorphIcon icon={if menu.get() { "x" } else { "menu" }} size={24.0} />
                    </Button>
                    <Button
                        variant="outline"
                        size="icon"
                        on_click={move || play.set(!play.peek())}
                    >
                        <MorphIcon icon={if play.get() { "pause" } else { "play" }} size={24.0} />
                    </Button>
                    <Button
                        variant="outline"
                        size="icon"
                        on_click={move || sun.set(!sun.peek())}
                    >
                        <MorphIcon icon={if sun.get() { "moon" } else { "sun" }} size={24.0} spring="smooth" />
                    </Button>
                    <Button
                        variant="outline"
                        size="icon"
                        on_click={move || copied.set(!copied.peek())}
                    >
                        <MorphIcon icon={if copied.get() { "check" } else { "copy" }} size={24.0} />
                    </Button>
                    <Button
                        variant="outline"
                        size="icon"
                        on_click={move || eye.set(!eye.peek())}
                    >
                        <MorphIcon icon={if eye.get() { "eye-off" } else { "eye" }} size={24.0} />
                    </Button>
                    <MorphPairButton id="chevron" a="chevron-down" b="chevron-up" spring="snappy" />
                    <MorphPairButton id="plus-minus" a="plus" b="minus" spring="snappy" />
                    <MorphPairButton id="volume" a="volume-2" b="volume-x" spring="smooth" />
                    <MorphPairButton id="lock" a="lock" b="unlock" spring="bouncy" />
                    <MorphPairButton id="smile" a="smile" b="frown" spring="smooth" />
                    <MorphPairButton id="panel" a="panel-left" b="panel-left-close" spring="snappy" />
                    <VStack items="center" gap="0.25rem" min_w="4.75rem">
                        { builder_el }
                        <Label text_size="0.6rem" color="muted-foreground" class="whitespace-nowrap">
                            { "builder @ 0.45" }
                        </Label>
                    </VStack>
                    <VStack items="center" gap="0.25rem" min_w="4.75rem">
                        { svg_el }
                        <Label text_size="0.6rem" color="muted-foreground" class="whitespace-nowrap">
                            { "svg_to_icon linear" }
                        </Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// Chaikin corner-cutting: two passes of subdivision produce a smooth C1 curve
/// that never overshoots the control polygon.  Anchors the first and last
/// points so the stroke starts and ends exactly at the pointer position.
pub(crate) fn chaikin_smooth(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
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
pub(crate) fn DrawingCanvasShowcase() -> Element {
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

const IMAGE_STREAM_KEY: &str = "jadix-gallery-stream";
const IMAGE_STREAM_W: u32 = 320;
const IMAGE_STREAM_H: u32 = 180;
const STREAM_BAR_HALF: i32 = 16;

#[cfg(not(feature = "image-stream"))]
fn synthetic_stream_frame(phase: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (IMAGE_STREAM_W * IMAGE_STREAM_H * 4) as usize];
    let bar_x = (phase % IMAGE_STREAM_W) as i32;
    for y in 0..IMAGE_STREAM_H {
        for x in 0..IMAGE_STREAM_W {
            let i = ((y * IMAGE_STREAM_W + x) * 4) as usize;
            let on_bar = (x as i32 - bar_x).abs() < STREAM_BAR_HALF;
            if on_bar {
                rgba[i] = 96;
                rgba[i + 1] = 165;
                rgba[i + 2] = 250;
                rgba[i + 3] = 255;
            } else {
                let stripe = ((x + phase / 2) / 24) % 2 == 0;
                let g = if stripe { 52 } else { 148 };
                rgba[i] = g;
                rgba[i + 1] = g.saturating_add(8);
                rgba[i + 2] = g.saturating_add(28);
                rgba[i + 3] = 255;
            }
        }
    }
    rgba
}

#[cfg(feature = "image-stream")]
fn synthetic_i420_frame(phase: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let w = IMAGE_STREAM_W;
    let h = IMAGE_STREAM_H;
    let cw = w.div_ceil(2);
    let ch = h.div_ceil(2);
    let mut y = vec![0u8; (w * h) as usize];
    let mut u = vec![128u8; (cw * ch) as usize];
    let mut v = vec![128u8; (cw * ch) as usize];
    let bar_x = (phase % w) as i32;
    for py in 0..h {
        for px in 0..w {
            let on_bar = (px as i32 - bar_x).abs() < STREAM_BAR_HALF;
            let stripe = ((px + phase / 2) / 24) % 2 == 0;
            y[(py * w + px) as usize] = if on_bar {
                140
            } else if stripe {
                48
            } else {
                130
            };
        }
    }
    for cy in 0..ch {
        for cx in 0..cw {
            let px = cx * 2;
            let on_bar = (px as i32 - bar_x).abs() < STREAM_BAR_HALF;
            let i = (cy * cw + cx) as usize;
            if on_bar {
                u[i] = 180;
                v[i] = 80;
            }
        }
    }
    (y, u, v)
}

fn stream_phase() -> u32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    (start.elapsed().as_millis() / 16) as u32
}

fn queue_synthetic_stream_frame() {
    #[cfg(feature = "image-stream")]
    {
        let (y, u, v) = synthetic_i420_frame(stream_phase());
        jadix::queue_media_frame(
            IMAGE_STREAM_KEY,
            jadix::MediaFrame::i420(IMAGE_STREAM_W, IMAGE_STREAM_H, &y, &u, &v),
        );
    }
    #[cfg(not(feature = "image-stream"))]
    {
        let rgba = synthetic_stream_frame(stream_phase());
        jadix::queue_media_frame(
            IMAGE_STREAM_KEY,
            jadix::MediaFrame::rgba8(IMAGE_STREAM_W, IMAGE_STREAM_H, &rgba),
        );
    }
}

#[component]
pub(crate) fn ImageStreamShowcase() -> Element {
    // Seed the GPU key before the first collect. A 1×1 clock never rasterizes
    // (`canvas` skips w/h < 2), so the Image leaf used to draw with a missing bind group.
    queue_synthetic_stream_frame();
    let clock: Element = canvas()
        .w(4.0)
        .h(4.0)
        .opacity(0.0)
        .paint(move |ctx: &mut dyn Canvas2D, _size: CanvasSize| {
            queue_synthetic_stream_frame();
            ctx.request_frame();
            LayoutStatus::Clean
        })
        .into();
    let clock = clock.pointer_events_none();

    ui! {
        <MyCard title="ImageStream · 合成 RGBA 流（WP-M.2）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "引擎只收帧：CPU 色条 → queue_media_frame → <Image texture>。无 FFmpeg / 无播放器。应看到亮/暗竖纹 + 一条横向移动的蓝条。`--features image-stream` 时色条以 I420 投递（引擎转 RGBA），不要在热路径上再做 RGBA→YUV→RGBA 往返。"
                </Label>
                { clock }
                <Image texture={IMAGE_STREAM_KEY} w="20rem" h="11.25rem" rounded="0.5rem" />
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn DrawingScene3dShowcase() -> Element {
    let cube: Element = scene3d().w(220.0).h(160.0).label("Jadix 3D").into();
    ui! {
        <MyCard title="Scene3d · 透视立方（WP-3.13）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "实验 feature scene3d（默认关）。同一 wgpu device，无 glTF / 无 three.js。启用：cargo run -p gallery --features scene3d。`.label` 走三通道 MSDF；透视翘曲见下方 Billboard 卡。"
                </Label>
                { cube }
            </VStack>
        </MyCard>
    }
}

fn billboard_leaf(tid: &str, text: &str, cam: Scene3dCamera, yaw: f32) -> Element {
    let el: Element = scene3d()
        .w(280.0)
        .h(200.0)
        .camera(cam)
        .yaw_speed(yaw)
        .label(text)
        .into();
    el.template_id(tid)
}

#[component]
pub(crate) fn DrawingBillboardShowcase() -> Element {
    let mild = billboard_leaf(
        "gal-billboard-default",
        "Ag MSDF",
        Scene3dCamera::default(),
        0.45,
    );
    let steep = billboard_leaf(
        "gal-billboard-pitch",
        "永角",
        Scene3dCamera {
            fov_deg: 52.0,
            pitch: 0.92,
            distance: 1.72,
        },
        0.35,
    );
    ui! {
        <MyCard title="Billboard · 透视翘曲（三通道 MSDF）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "scene3d().label 把 MSDF 叶投成世界平面四角 + clip w（flags 4），不是屏幕 AABB。右卡加大俯仰，远边应收窄成梯形。仍是 RT 上的 2D 合成，不写 scene3d depth。需 --features scene3d。"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="start">
                    <VStack gap="0.25rem">
                        { mild }
                        <Label text_size="0.6rem" color="muted-foreground">"默认 pitch · Ag MSDF"</Label>
                    </VStack>
                    <VStack gap="0.25rem">
                        { steep }
                        <Label text_size="0.6rem" color="muted-foreground">"pitch≈53° · 永角（梯形）"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

const SHADERTOY_FS: &str = r#"
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv * 2.0 - vec2<f32>(1.0);
    let t = u.time;
    let r = length(uv);
    let a = atan2(uv.y, uv.x);
    let bands = 0.5 + 0.5 * sin(r * 12.0 - t * 2.5 + a * 2.0);
    let col = vec3<f32>(
        0.12 + 0.80 * bands,
        0.28 + 0.40 * sin(t + r * 4.0),
        0.88 - 0.35 * bands,
    );
    return vec4<f32>(col * col, 1.0);
}
"#;

const SAMPLE_SRC_FS: &str = r#"
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let warp = vec2<f32>(sin(u.time + in.uv.y * 8.0), cos(u.time + in.uv.x * 8.0)) * 0.035;
    var c = textureSample(t_src, s_src, in.uv + warp);
    c.r = textureSample(t_src, s_src, in.uv + warp * 1.5).r;
    return vec4<f32>(c.rgb, 1.0);
}
"#;

#[component]
pub(crate) fn DrawingShaderShowcase() -> Element {
    queue_synthetic_stream_frame();
    let plasma: Element = shader().w(280.0).h(180.0).wgsl(SHADERTOY_FS).into();
    let sampled: Element = shader()
        .w(280.0)
        .h(180.0)
        .texture("src", IMAGE_STREAM_KEY)
        .wgsl(SAMPLE_SRC_FS)
        .into();
    ui! {
        <MyCard title="Shader · 用户 WGSL 叶（experimental）">
            <VStack w="100%" gap="0.75rem" flex_shrink={0.0}>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "公开 shader() 叶：框架 VS + time/resolution/pointer + t_src / t_src1。用户只写 fs_main。离屏 RT 再 textured 合成。编译失败品红/留上帧，不 panic。`.texture(\"src\"|\"src1\", key)` 吃 jadix_path: / jadix_canvas: / 媒体槽（本卡采样合成色条）。默认 GLES，不是 Cargo feature。"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="start">
                    <VStack gap="0.25rem">
                        { plasma }
                        <Label text_size="0.6rem" color="muted-foreground">"Builder shader().wgsl"</Label>
                    </VStack>
                    <VStack gap="0.25rem">
                        <shader w={280.0} h={180.0} wgsl={SHADERTOY_FS} />
                        <Label text_size="0.6rem" color="muted-foreground">"ui! <shader wgsl>"</Label>
                    </VStack>
                    <VStack gap="0.25rem">
                        { sampled }
                        <Label text_size="0.6rem" color="muted-foreground">"shader().texture src=色条"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}
