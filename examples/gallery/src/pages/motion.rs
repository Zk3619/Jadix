use crate::imports::*;
use crate::shared::*;
use jadix::motion::{
    LayoutAnimation, Motion, MotionTarget, PreparedPath, bind_path_follow, bind_path_reveal,
    use_layout_motion, use_motion,
};
use jadix::primitives::path::{PathCommand, PathStyle, attach_path};
use jadix::style::{AnimatedNumber, TimingFunction, ValueDriver};

#[component]
pub(crate) fn LayoutMotionShowcase() -> Element {
    jadix::app::reactive::reactive_component_live_sized(
        "gal-layout-motion-live",
        (),
        "100%",
        "auto",
        |_| build_layout_motion_showcase(),
    )
}

pub(crate) fn build_layout_motion_showcase() -> Element {
    let vertical = use_signal("gal-motion-vertical", || false);
    let spacious = use_signal("gal-motion-spacious", || false);
    let reversed = use_signal("gal-motion-reversed", || false);
    let destination = use_signal("gal-motion-destination", || false);
    let expanded = use_signal("gal-motion-nested-expanded", || false);
    let vertical_value = vertical.get();
    let spacious_value = spacious.get();
    let reversed_value = reversed.get();
    let destination_value = destination.get();
    let expanded_value = expanded.get();
    let order = if reversed_value { [2, 1, 0] } else { [0, 1, 2] };
    let tiles = order
        .into_iter()
        .map(|index| {
            let label = Element::new()
                .p(12.0)
                .child(
                    Element::new()
                        .text(format!("Card {}", index + 1))
                        .text_color(if index == 0 { "foreground" } else { "white" }),
                )
                .layout_motion(&use_layout_motion(
                    &format!("gal-motion-tile-label-{index}"),
                    LayoutAnimation::Position,
                ));
            Element::new()
                .template_id(format!("gal-motion-tile-{index}"))
                .w(112.0)
                .h(44.0)
                .flex_shrink(0.0)
                .rounded(10.0)
                .bg(["chart-1", "chart-2", "chart-3"][index])
                .child(label)
                .layout_motion(&use_layout_motion(
                    &format!("gal-motion-tile-{index}"),
                    LayoutAnimation::Both,
                ))
        })
        .collect::<Vec<_>>();
    let stage = Element::new()
        .template_id("gal-motion-flow-stage")
        .w("100%")
        .h(200.0)
        .p(12.0)
        .rounded(12.0)
        .bg("muted")
        .class(if vertical_value {
            "flex-col"
        } else {
            "flex-row flex-wrap"
        })
        .gap(if spacious_value { 24.0 } else { 8.0 })
        .children(tiles);
    let shared_label = Element::new()
        .p(12.0)
        .child(Element::new().text("layout_id").text_color("white"))
        .layout_motion(&use_layout_motion(
            "gal-motion-shared-label",
            LayoutAnimation::Position,
        ));
    let shared = Element::new()
        .template_id("gal-motion-shared-chip")
        .absolute()
        .left(12.0)
        .top(12.0)
        .w(if destination_value { 156.0 } else { 100.0 })
        .h(if destination_value { 60.0 } else { 44.0 })
        .rounded(10.0)
        .bg("chart-4")
        .child(shared_label)
        .layout_motion(&use_layout_motion(
            "gal-motion-shared-chip",
            LayoutAnimation::Both,
        ))
        .layout_id("gallery-shared-chip");
    let mut source = Element::new()
        .template_id("gal-motion-shared-source")
        .relative()
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_w(0.0)
        .h(88.0)
        .rounded(12.0)
        .bg("muted");
    let mut target = Element::new()
        .template_id("gal-motion-shared-target")
        .relative()
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_w(0.0)
        .h(88.0)
        .rounded(12.0)
        .bg("muted");
    if destination_value {
        target.children.push(shared);
    } else {
        source.children.push(shared);
    }
    let nested_label = Element::new()
        .p(12.0)
        .child(Element::new().text("字号 / 圆点保持不变"))
        .layout_motion(&use_layout_motion(
            "gal-motion-nested-label",
            LayoutAnimation::Position,
        ));
    let dot = Element::new()
        .absolute()
        .left(12.0)
        .top(48.0)
        .w(16.0)
        .h(16.0)
        .rounded_full()
        .bg("chart-2")
        .layout_motion(&use_layout_motion(
            "gal-motion-nested-dot",
            LayoutAnimation::Position,
        ));
    let nested = Element::new()
        .template_id("gal-motion-nested")
        .relative()
        .w(if expanded_value { 320.0 } else { 200.0 })
        .h(if expanded_value { 100.0 } else { 72.0 })
        .rounded(12.0)
        .bg("secondary")
        .children([nested_label, dot])
        .layout_motion(&use_layout_motion(
            "gal-motion-nested",
            LayoutAnimation::Both,
        ));
    ui! {
        <MyCard title="布局 Motion · 排列 / 间距 / 共享元素 / 嵌套校正">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <HStack gap="0.5rem" wrap="wrap">
                    <Button id="gal-motion-toggle-direction" variant="outline" size="sm" on_click={move || vertical.set(!vertical.peek())}>"切换横纵排列"</Button>
                    <Button id="gal-motion-toggle-gap" variant="outline" size="sm" on_click={move || spacious.set(!spacious.peek())}>"切换间距"</Button>
                    <Button id="gal-motion-toggle-order" variant="outline" size="sm" on_click={move || reversed.set(!reversed.peek())}>"反转顺序"</Button>
                </HStack>
                { stage }
                <Button id="gal-motion-toggle-shared" variant="outline" size="sm" on_click={move || destination.set(!destination.peek())}>"跨父容器移动 layout_id"</Button>
                <HStack w="100%" gap="1rem">{ source }{ target }</HStack>
                <Button id="gal-motion-toggle-nested" variant="outline" size="sm" on_click={move || expanded.set(!expanded.peek())}>"缩放容器，保留子内容尺寸"</Button>
                <Box h={100.0} w="100%">{ nested }</Box>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn RetainedMotionShowcase() -> Element {
    jadix::app::reactive::reactive_component_live_sized(
        "gal-retained-motion-live",
        (),
        "100%",
        "auto",
        |_| build_retained_motion_showcase(),
    )
}

fn build_retained_motion_showcase() -> Element {
    let interaction = use_motion("gal_retained_interaction");
    let expanded = use_signal("gal_retained_expanded", || false);
    let expanded_value = expanded.get();
    let layout = use_layout_motion("gal_retained_layout", LayoutAnimation::Both);
    let layout_label = use_layout_motion("gal_retained_layout_label", LayoutAnimation::Position);
    let progress = use_signal("gal_retained_path", || AnimatedNumber::follow(0.0)).peek();
    let replay = progress.clone();
    let commands = vec![
        PathCommand::MoveTo((12.0, 48.0)),
        PathCommand::CubicTo {
            c1: (72.0, 0.0),
            c2: (152.0, 96.0),
            to: (220.0, 48.0),
        },
    ];
    let path = use_signal("gal_retained_prepared_path", move || {
        Rc::new(PreparedPath::new(&commands))
    })
    .peek();
    let drawing = attach_path(
        Element::new().w(240.0).h(96.0),
        &path.trim(0.0, 1.0),
        &PathStyle::stroke(parse_color("chart-1"), 3.0),
    );
    let drawing = bind_path_reveal(drawing, path.clone(), progress.clone());
    let follower = bind_path_follow(
        Element::new()
            .absolute()
            .w(12.0)
            .h(12.0)
            .translate(-6.0, -6.0)
            .rounded_full()
            .bg("chart-2"),
        path,
        progress,
        true,
    );
    let interactive = Element::new()
        .w(180.0)
        .h(48.0)
        .rounded(10.0)
        .bg("secondary")
        .focus_id("gal-retained-motion-action")
        .focusable(true)
        .on_click(|| {})
        .child(
            Element::new()
                .p(12.0)
                .child(Element::new().text("Hover / Press / Tab")),
        )
        .motion(
            &interaction,
            Motion::new(MotionTarget::new())
                .while_hover(
                    MotionTarget::new()
                        .scale(1.05)
                        .background_color(parse_color("accent")),
                )
                .while_tap(MotionTarget::new().scale(0.94))
                .while_focus(MotionTarget::new().border_radius(20.0)),
        );
    let card = Element::new()
        .template_id("gal-retained-layout-card")
        .w(if expanded_value { 240.0 } else { 120.0 })
        .h(if expanded_value { 72.0 } else { 48.0 })
        .rounded(12.0)
        .bg("chart-3")
        .layout_motion(&layout)
        .child(
            Element::new()
                .p(12.0)
                .layout_motion(&layout_label)
                .child(Element::new().text("Layout").text_color("white")),
        );
    ui! {
        <MyCard title="1j) 交互 / 布局 / 矢量 Motion">
            <VStack w="100%" min_w="0" gap="1rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "保留状态句柄：hover/tap/focus 叠加；布局一次提交后投影；路径按弧长揭示并跟随。遵循 reduced-motion。"
                </Label>
                { interactive }
                <HStack w="100%" gap="1rem" items="center" wrap="wrap">
                    <Button variant="outline" size="sm" on_click={move || expanded.set(!expanded.peek())}>"切换布局大小"</Button>
                    { card }
                </HStack>
                <Button variant="outline" size="sm" on_click={move || {
                    replay.jump(0.0);
                    replay.restart(1.0, ValueDriver::tween(1200.0, TimingFunction::EaseInOut));
                    jadix::app::invalidation::request_frame();
                }}>"重播矢量路径"</Button>
                <Box w={240.0} h={96.0} class="relative">{ drawing }{ follower }</Box>
            </VStack>
        </MyCard>
    }
}
