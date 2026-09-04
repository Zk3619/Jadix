//! SVG / Canvas / Scene3d。

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

#[component]
pub(crate) fn DrawingScene3dShowcase() -> Element {
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
