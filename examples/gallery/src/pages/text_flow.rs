use crate::imports::*;
use crate::shared::*;
use jadix::text::{LayoutCursor, TextExclusion, TextFlowRect, layout_text_flow, prepare};

const ENGLISH: &str = "A page is more than a stack of rectangles. Words can follow the edge of an illustration, divide around a circle, and meet again on the other side. The empty space becomes part of the story.\n\nPrepare the paragraph once. Then give each row its own position and available width. When an object moves, the same measured words find a new route without measuring the whole paragraph again.\n\nThis is continuous text, not separate captions placed beside a picture. Read from left to right, then down the column. At the bottom, the cursor carries the next word into the following column or page. Nothing is clipped or discarded.\n\nTry a narrower page, change the column height, or click either colored shape. The circular contour changes a little on every row; the rectangular pull quote leaves a clean edge. Hide the obstacles to see the original flow return.\n\nGood typography is a conversation between content and space. A native interface can offer the same playful freedom as an editorial page, while keeping text sharp at every scale.";

const CHINESE: &str = "页面不只是一组矩形的堆叠。文字可以沿着插图的边缘流动，在圆形两侧分开，再在下方重新汇合。留白也成为叙事的一部分。\n\n段落只需准备一次。排版时，为每一行提供位置与可用宽度。当图形移动，已测量的文字就会寻找新的路径，不必重新测量整段内容。\n\n这是一段连续的文章，而不是图片旁边拼接的几条说明。先从左到右阅读，再沿着栏向下。到达栏底时，游标把尚未排下的内容带到下一栏或下一页，不会截断或丢弃。\n\n试着缩窄页面、改变栏高，或者点击彩色图形。圆形的轮廓让每一行的起点与宽度逐渐变化；矩形引文则留下整齐的边缘。隐藏障碍物，文字就恢复普通分栏。\n\n中文与 English 可以共同排版。文字并不需要被锁在固定的盒子里：内容决定阅读顺序，几何形状决定每一行可以经过的空间。\n\n好的排版是内容与空间之间的对话。原生界面同样可以拥有杂志般的自由，同时保持清晰、连续、可以随时重排的文字。";

fn flow_label(text: &str, x: f32, y: f32, width: f32, size: f32, height: f32) -> Element {
    Element::new()
        .absolute()
        .left(x)
        .top(y)
        .w(width)
        .h(height)
        .text(text)
        .text_size(size)
        .text_line_height(height)
        .text_color("#23352f")
        .white_space("nowrap")
        .pointer_events("none")
}

pub(crate) fn text_flow_scene(
    width: f32,
    column_height: f32,
    shifted: Signal<bool>,
    obstacles: bool,
    chinese: bool,
    clearance: f32,
) -> Element {
    let moved = shifted.get();
    let columns = if width >= 660.0 { 2 } else { 1 };
    let column_width = (width - 48.0 - (columns - 1) as f32 * 28.0) / columns as f32;
    let radius = (column_width * 0.23).min(68.0);
    let center = [
        24.0 + column_width * if moved { 0.32 } else { 0.65 },
        142.0 + radius,
    ];
    let quote = TextFlowRect {
        x: 24.0 + (columns - 1) as f32 * (column_width + 28.0),
        y: if columns == 1 {
            (170.0 + radius * 2.0 + if moved { 24.0 } else { 0.0 }).min(24.0 + column_height)
        } else if moved {
            270.0
        } else {
            220.0
        },
        width: column_width * 0.46,
        height: 76.0,
    };
    let exclusions = if obstacles {
        vec![
            TextExclusion::Circle {
                center,
                radius: radius + clearance,
            },
            TextExclusion::Rect(TextFlowRect {
                x: quote.x - clearance,
                y: quote.y - clearance,
                width: quote.width + clearance * 2.0,
                height: quote.height + clearance * 2.0,
            }),
        ]
    } else {
        Vec::new()
    };
    let source = if chinese { CHINESE } else { ENGLISH };
    let prepared = prepare(source, 15.0, 400, 24.0);
    let expected_segment_count = prepared.segment_count();
    let page_height = column_height + 152.0;
    let mut scene = Element::new().w(width).min_w(width).bg("#f6f3e9");
    let mut cursor = LayoutCursor::default();
    let mut page = 0;
    while cursor.segment_index < prepared.segment_count() {
        let page_top = page as f32 * page_height;
        scene = scene.child(flow_label(
            "JADIX JOURNAL     /     TYPE & SPACE",
            24.0,
            page_top + 18.0,
            width - 48.0,
            11.0,
            20.0,
        ));
        scene = scene.child(
            flow_label(
                if page == 0 {
                    "LET THE WORDS FLOW"
                } else {
                    "THE STORY CONTINUES"
                },
                24.0,
                page_top + 48.0,
                width - 48.0,
                if width < 400.0 { 21.0 } else { 30.0 },
                42.0,
            )
            .font_weight(700.0),
        );
        let before = cursor;
        for column in 0..columns {
            let region = TextFlowRect {
                x: 24.0 + column as f32 * (column_width + 28.0),
                y: page_top + 112.0,
                width: column_width,
                height: column_height,
            };
            let flow = layout_text_flow(
                &prepared,
                cursor,
                region,
                if page == 0 { &exclusions } else { &[] },
            );
            for fragment in flow.lines {
                scene = scene.child(flow_label(
                    &fragment.line.text,
                    fragment.x,
                    fragment.y,
                    fragment.line.width.max(1.0),
                    15.0,
                    24.0,
                ));
            }
            cursor = flow.cursor;
        }
        scene = scene.child(flow_label(
            &format!("{:02}     /     NATIVE TEXT FLOW", page + 1),
            24.0,
            page_top + column_height + 120.0,
            width - 48.0,
            10.0,
            20.0,
        ));
        page += 1;
        assert!(cursor != before, "text flow made no progress");
    }
    debug_assert_eq!(cursor.segment_index, expected_segment_count);
    if obstacles {
        scene = scene.child(
            Element::new()
                .template_id("gal-flow-orb")
                .absolute()
                .left(center[0] - radius)
                .top(center[1] - radius)
                .w(radius * 2.0)
                .h(radius * 2.0)
                .rounded("999px")
                .bg("#c8633f")
                .text("MOVE")
                .text_size(15.0)
                .text_line_height(radius * 2.0)
                .text_align("center")
                .text_color("#ffffff")
                .on_click(move || shifted.set(!shifted.peek())),
        );
        scene = scene.child(
            Element::new()
                .template_id("gal-flow-quote")
                .absolute()
                .left(quote.x)
                .top(quote.y)
                .w(quote.width)
                .h(quote.height)
                .rounded("0px")
                .bg("#244a3c")
                .flex_col()
                .flex_items("center")
                .justify_center()
                .child(Element::new()
                    .template_id("gal-flow-quote-label")
                    .text("MAKE\nSPACE")
                    .text_size(15.0)
                    .text_line_height(24.0)
                    .text_align("center")
                    .text_color("#ffffff"))
                .on_click(move || shifted.set(!shifted.peek())),
        );
    }
    scene.h(page as f32 * page_height)
}

#[component]
pub(crate) fn TextFlowShowcase() -> Element {
    let width = use_signal("gal-flow-width", || 720.0_f32);
    let height = use_signal("gal-flow-height", || 432.0_f32);
    let shifted = use_signal("gal-flow-shifted", || false);
    let obstacles = use_signal("gal-flow-obstacles", || true);
    let chinese = use_signal("gal-flow-chinese", || false);
    let clearance = use_signal("gal-flow-clearance", || 0.0_f32);
    let last = Rc::new(Cell::new(None));
    let scene = Element::new()
        .w("100%")
        .min_w("0")
        .child(text_flow_scene(
            width.peek(),
            height.peek(),
            shifted,
            obstacles.peek(),
            chinese.peek(),
            clearance.peek(),
        ))
        .reactive_sync(move |host, _| {
            let available = host.layout_bounds[2];
            let page_width = if available > 0.0 {
                width.peek().min(available.max(220.0))
            } else {
                width.peek()
            };
            let settings = (
                page_width,
                height.peek(),
                shifted.peek(),
                obstacles.peek(),
                chinese.peek(),
                clearance.peek(),
            );
            if last.get() == Some(settings) {
                return LayoutStatus::Clean;
            }
            last.set(Some(settings));
            host.children = vec![jadix::core::reactivity::isolate_reads(|| {
                text_flow_scene(settings.0, settings.1, shifted, settings.3, settings.4, settings.5)
            })];
            LayoutStatus::StructureChanged
        });
    ui! {
        <MyCard title="Pretext · 文字环绕 / 连续分栏">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.875rem" color="muted-foreground">
                    "拖动页宽上限与栏高滑块，或缩放窗口；点击圆形或引文移动障碍物。窄页自动切为单栏，正文连续流入下一页。"
                </Label>
                <HStack w="100%" gap="1rem" wrap="wrap">
                    <VStack w="12rem" min_w="0" gap="0.5rem">
                        <Label text_size="0.75rem">{format!("页宽  {:.0} px", width.get())}</Label>
                        { slider("gal-flow-width-slider").min(320.0).max(1000.0).step(8.0).value_signal(width) }
                    </VStack>
                    <VStack w="12rem" min_w="0" gap="0.5rem">
                        <Label text_size="0.75rem">{format!("栏高  {:.0} px", height.get())}</Label>
                        { slider("gal-flow-height-slider").min(288.0).max(576.0).step(24.0).value_signal(height) }
                    </VStack>
                    <VStack w="12rem" min_w="0" gap="0.5rem">
                        <Label text_size="0.75rem">{format!("避让间距  {:.0} px", clearance.get())}</Label>
                        { slider("gal-flow-clearance-slider").min(0.0).max(24.0).step(1.0).value_signal(clearance) }
                    </VStack>
                </HStack>
                <HStack gap="0.5rem" wrap="wrap">
                    <Button id="gal-flow-toggle" variant="outline" size="sm" on_click={move || obstacles.set(!obstacles.peek())}>
                        {if obstacles.get() { "隐藏障碍物" } else { "显示障碍物" }}
                    </Button>
                    <Button id="gal-flow-language" variant="outline" size="sm" on_click={move || chinese.set(!chinese.peek())}>
                        {if chinese.get() { "English" } else { "中文 / English" }}
                    </Button>
                </HStack>
                <ScrollArea id="gal-flow-preview" w="100%" h="640px" overflow_x="auto" overflow_y="auto">
                    { scene }
                </ScrollArea>
            </VStack>
        </MyCard>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadix::core::reactivity::{create_effect, create_root};

    #[test]
    fn text_flow_gallery_keeps_all_body_text_inside_native_line_boxes() {
        jadix::app::with_ui_build_stack(|| {
            let mut fonts = jadix::renderer::font_loader::create_font_system();
            jadix::text::install_shared_font_system(&mut fonts);
            let (owner, _) = create_root(|| {
                create_effect(|| {
                    jadix::core::reactivity::isolate_reads(|| {
                        let shifted = Signal::new(false);
                        for width in [320.0, 656.0, 664.0, 720.0, 800.0, 1000.0] {
                            for height in [288.0, 576.0] {
                                for chinese in [false, true] {
                                    for obstacles in [false, true] {
                                        let mut scene = text_flow_scene(
                                            width, height, shifted, obstacles, chinese, 0.0,
                                        );
                                        let mut source_text = String::new();
                                        for node in &scene.children {
                                            if node.text_style.size == Some(15.0)
                                                && node.template_id.is_none()
                                            {
                                                if let Some(text) = node.text.as_deref() {
                                                    source_text.push_str(text);
                                                }
                                            }
                                        }
                                        let expected = if chinese { CHINESE } else { ENGLISH };
                                        assert_eq!(source_text, expected.replace('\n', ""));
                                        let mut layout = jadix::core::tree::LayoutTree::new();
                                        let output = layout.build_and_layout(
                                            &mut scene,
                                            width,
                                            4000.0,
                                            [-1.0, -1.0],
                                        );
                                        if obstacles {
                                            let quote_box =
                                                jadix::core::tree::find_in_tree(&scene, |node| {
                                                    node.template_id.as_deref()
                                                        == Some("gal-flow-quote")
                                                })
                                                .unwrap()
                                                .layout_bounds;
                                            let label =
                                                jadix::core::tree::find_in_tree(&scene, |node| {
                                                    node.template_id.as_deref()
                                                        == Some("gal-flow-quote-label")
                                                })
                                                .unwrap()
                                                .layout_bounds;
                                            assert!((label[0] + label[2] * 0.5 - quote_box[0] - quote_box[2] * 0.5).abs() < 0.5);
                                            assert!((label[1] + label[3] * 0.5 - quote_box[1] - quote_box[3] * 0.5).abs() < 0.5);
                                        }
                                        let body: Vec<_> = output
                                            .text_nodes
                                            .iter()
                                            .filter(|node| {
                                                node.style.size == Some(15.0)
                                                    && node.template_id.is_none()
                                            })
                                            .collect();
                                        let columns = if width >= 660.0 { 2 } else { 1 };
                                        let column_width =
                                            (width - 48.0 - (columns - 1) as f32 * 28.0)
                                                / columns as f32;
                                        let radius = (column_width * 0.23).min(68.0);
                                        let center = [
                                            24.0 + column_width * if shifted.peek() {
                                                0.32
                                            } else {
                                                0.65
                                            },
                                            142.0 + radius,
                                        ];
                                        let quote = TextFlowRect {
                                            x: 24.0
                                                + (columns - 1) as f32 * (column_width + 28.0),
                                            y: if columns == 1 {
                                                (170.0 + radius * 2.0).min(24.0 + height)
                                            } else {
                                                220.0
                                            },
                                            width: column_width * 0.46,
                                            height: 76.0,
                                        };
                                        for node in body {
                                            assert!(
                                                node.x >= 23.5
                                                    && node.x + node.width <= width - 23.5,
                                                "{width}x{height}: {:?} at {} + {}",
                                                node.text,
                                                node.x,
                                                node.width
                                            );
                                            assert_eq!(
                                                node.style.white_space,
                                                jadix::style::WhiteSpace::Nowrap
                                            );
                                            assert!((node.height - 24.0).abs() < 0.5);
                                            if obstacles && node.y < height + 152.0 {
                                                let closest_x = center[0]
                                                    .max(node.x)
                                                    .min(node.x + node.width);
                                                let closest_y = center[1]
                                                    .max(node.y)
                                                    .min(node.y + node.height);
                                                let circle_overlap = (closest_x - center[0]).powi(2)
                                                    + (closest_y - center[1]).powi(2)
                                                    < (radius - 0.01).powi(2);
                                                assert!(
                                                    !circle_overlap,
                                                    "{width}x{height}: {:?} crosses circle at {} + {}",
                                                    node.text,
                                                    node.x,
                                                    node.y
                                                );
                                                let quote_overlap = node.x < quote.x + quote.width
                                                    && node.x + node.width > quote.x
                                                    && node.y < quote.y + quote.height
                                                    && node.y + node.height > quote.y;
                                                assert!(
                                                    !quote_overlap,
                                                    "{width}x{height}: {:?} crosses quote at {} + {}",
                                                    node.text,
                                                    node.x,
                                                    node.y
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let original = text_flow_scene(800.0, 432.0, shifted, true, false, 0.0);
                        let orb = original
                            .children
                            .iter()
                            .find(|child| child.template_id.as_deref() == Some("gal-flow-orb"))
                            .unwrap();
                        (orb.on_click.as_ref().unwrap().borrow_mut())();
                        assert!(shifted.peek());
                        let moved = text_flow_scene(800.0, 432.0, shifted, true, false, 0.0);
                        let positions = |scene: &Element| {
                            scene
                                .children
                                .iter()
                                .filter(|node| node.text_style.size == Some(15.0))
                                .map(|node| {
                                    (
                                        node.style.inset.left,
                                        node.style.inset.top,
                                        node.text.clone(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        };
                        assert_ne!(positions(&original), positions(&moved));
                    })
                });
            });
            owner.dispose();
            jadix::text::clear_shared_font_system();
        });
    }
}
