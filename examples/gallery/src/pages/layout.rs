//! 布局 / 视觉效果分区。

use crate::imports::*;
use crate::shared::*;

#[component]
pub(crate) fn ResponsiveShowcase() -> Element {
    let panels = [
        ("内容优先", "使用流式宽度和自动换行，避免固定宽度挤压内容。"),
        ("清晰的断点边界", "max-md 在 768px 以下生效，md 从 768px 开始生效。"),
        ("操作始终可达", "操作按钮在窄窗口纵向排列，宽窗口恢复横向排列。"),
    ].into_iter().enumerate().map(|(index, (title, description))| {
        use jadix::motion::{use_layout_motion, LayoutAnimation};
        let title = ui! { <Label font_weight="600">{ title }</Label> }
            .layout_motion(&use_layout_motion(&format!("gal-responsive-title-{index}"), LayoutAnimation::Position));
        let description = ui! { <Label w="100%">{ description }</Label> }
            .layout_motion(&use_layout_motion(&format!("gal-responsive-body-{index}"), LayoutAnimation::Position));
        Element::new().template_id(format!("gal-responsive-panel-{index}"))
            .flex_col().min_w(0.0).class("rounded-md bg-muted p-3 sm:p-4").gap("0.5rem")
            .children([title, description])
            .layout_motion(&use_layout_motion(&format!("gal-responsive-panel-{index}"), LayoutAnimation::Both))
    }).collect::<Vec<_>>();
    let grid = ui! {
        <GStack w="100%" min_w="0" class="grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 sm:gap-4">
            { panels }
        </GStack>
    }.layout_motion(&jadix::motion::use_layout_motion("gal-responsive-grid", jadix::motion::LayoutAnimation::Position));
    ui! {
        <MyCard title="响应式布局">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" color="muted-foreground" text_size="0.875rem">
                    "缩放窗口：窄窗口单列，768px 起双列，1280px 起三列；卡片位置、尺寸与间距平滑过渡，文字不随投影拉伸。"
                </Label>
                { grid }
                <DialogFooter>
                    <Button variant="outline">"取消更改"</Button>
                    <Button>"保存更改"</Button>
                </DialogFooter>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ShadowShowcase() -> jadix::core::tree::Element {
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
#[cfg(test)]
pub(crate) const FILTER_DEMO_LOGO: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/assets/firefox-logo.svg");

pub(crate) fn filter_demo_tile(label: &str, logo: Element) -> Element {
    ui! {
        <VStack gap="0.25rem" items="center">
            { logo }
            <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
        </VStack>
    }
}

pub(crate) fn filter_demo_logo(filter: Option<&str>, class: Option<&str>) -> Element {
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
        wrap = jadix::style::with_class_list(wrap, c);
    }
    wrap.child(ui! {
        <Image src={format!("{}/assets/firefox-logo.svg", gallery_resource_root().display())} w="4.5rem" h="4.5rem" />
    })
}

#[component]
pub(crate) fn FilterShowcase() -> jadix::core::tree::Element {
    // Register a sample SVG filter for url(#galleryBlurSepia)
    let _ = jadix::style::register_svg_filter(
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

    let tiles_tw_blur: Vec<Element> = [
        ("blur-none", "blur-none"),
        ("blur-xs · 4px", "blur-xs"),
        ("blur-sm · 8px", "blur-sm"),
        ("blur-md · 12px", "blur-md"),
        ("blur-lg · 16px", "blur-lg"),
        ("blur-xl · 24px", "blur-xl"),
        ("blur-2xl · 40px", "blur-2xl"),
        ("blur-3xl · 64px", "blur-3xl"),
        ("blur-[2px]", "blur-[2px]"),
        ("blur-(--blur-sm)", "blur-(--blur-sm)"),
    ]
    .into_iter()
    .map(|(label, class)| filter_demo_tile(label, filter_demo_logo(None, Some(class))))
    .collect();

    ui! {
        <MyCard title="12b) CSS filter">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对齐 MDN：以彩色 Firefox logo（assets/firefox-logo.svg）为 SourceGraphic，子树捕获后按序应用 filter。支持函数列表、Tailwind v4 blur-*（xs=4px…3xl=64px；无单位任意值按 rem）与 url(#id)。"
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
                <Label font_weight="700" text_size="0.875rem">"Tailwind blur-*"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { filter_demo_tile("原图", filter_demo_logo(None, None)) }
                    { tiles_tw_blur }
                </HStack>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn gradient_demo_tile(label: &str, el: Element) -> Element {
    ui! {
        <VStack gap="0.25rem" items="center">
            { el }
            <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
        </VStack>
    }
}

pub(crate) fn gradient_swatch(css: &str, w: &str, h: &str) -> Element {
    Element::from(r#box().w(w).h(h).rounded("0.5rem").background_image(css))
}

/// MDN-aligned CSS gradient showcase (linear / radial / conic / repeating).
#[component]
pub(crate) fn GradientShowcase() -> jadix::core::tree::Element {
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
            // Overflow clip follows border-radius (SDF round-box). Nested gradient
            // layers still carry matching radii so textured fills mask their own corners.
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

    let tw_tile = Element::from(jadix::style::with_class_list(
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
pub(crate) fn BorderStyleShowcase() -> jadix::core::tree::Element {
    use jadix::style::CornerShape;

    fn caption(label: &str) -> Element {
        ui! {
            <Label text_size="0.6rem" color="muted-foreground" leading="normal">{ label }</Label>
        }
    }

    fn demo_box(label: &str, style_name: &str, color: [f32; 4], width: f32) -> Element {
        // 直角 + 对比填充：dotted/dashed 的间隙才能从底色里露出来，3D 样式也需要足够线宽。
        let el = Element::from(
            r#box()
                .w("6.5rem")
                .h("3.5rem")
                .bg(parse_color("muted"))
                .border(color, width)
                .border_style(style_name),
        );
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                { caption(label) }
            </VStack>
        }
    }

    fn per_side_box() -> Element {
        // 有圆角时引擎走单色 SDF 描边，分边 style/color/width 会被丢掉；showcase 必须直角。
        Element::from(
            r#box()
                .w("10rem")
                .h("5.5rem")
                .bg(parse_color("muted"))
                .border_top(parse_color("destructive"), 6.0)
                .border_right(parse_color("primary"), 3.0)
                .border_bottom(parse_color("chart-1"), 8.0)
                .border_left(parse_color("accent"), 4.0)
                .border_top_style("dashed")
                .border_right_style("dotted")
                .border_bottom_style("double")
                .border_left_style("solid"),
        )
    }

    fn corner_shape_box(label: &str, shape: CornerShape) -> Element {
        let el = Element::from(
            r#box()
                .w("5.5rem")
                .h("5.5rem")
                .bg(parse_color("primary"))
                .border(parse_color("foreground"), 3.0)
                .rounded(26.0)
                .border_shape(shape),
        );
        ui! {
            <VStack gap="0.25rem" items="center">
                { el }
                { caption(label) }
            </VStack>
        }
    }

    fn spacing_cell() -> Element {
        Element::from(
            r#box()
                .w("2.75rem")
                .h("2.75rem")
                .bg(parse_color("muted"))
                .border(parse_color("primary"), 2.0),
        )
    }

    let bc = parse_color("primary");

    ui! {
        <MyCard title="13) CSS Border 全能力">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "border-style（实线/点/虚线/双线/3D）· 四边独立宽/色/样式（直角才能分边绘制）· corner-shape（大圆角才看得出）。"
                </Label>

                <Label font_weight="700" text_size="0.875rem">"border-style 样式"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { demo_box("solid", "solid", bc, 6.0) }
                    { demo_box("dotted", "dotted", bc, 6.0) }
                    { demo_box("dashed", "dashed", bc, 6.0) }
                    { demo_box("double", "double", bc, 8.0) }
                    { demo_box("groove", "groove", bc, 10.0) }
                    { demo_box("ridge", "ridge", bc, 10.0) }
                    { demo_box("inset", "inset", bc, 10.0) }
                    { demo_box("outset", "outset", bc, 10.0) }
                    { demo_box("none", "none", bc, 6.0) }
                    { demo_box("hidden", "hidden", bc, 6.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"四边独立 width / color / style"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        { per_side_box() }
                        { caption("上 dashed 红 · 右 dotted 主色") }
                        { caption("下 double chart · 左 solid accent") }
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"border-radius longhand"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("6rem").h("6rem")
                                    .bg(parse_color("muted"))
                                    .border(parse_color("primary"), 3.0)
                                    .border_top_left_radius(28.0)
                                    .border_top_right_radius(2.0)
                                    .border_bottom_right_radius(28.0)
                                    .border_bottom_left_radius(0.0)
                            )
                        }
                        { caption("TL 28 · TR 2 · BR 28 · BL 0") }
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("5.5rem").h("5.5rem")
                                    .bg(parse_color("muted"))
                                    .border(parse_color("accent"), 3.0)
                                    .rounded_full()
                            )
                        }
                        { caption("rounded-full 正圆") }
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"corner-shape（CSS Level 4）"</Label>
                <Label text_size="0.65rem" color="muted-foreground" leading="normal">
                    "同一 border-radius=26：round 圆弧 · angle 斜切 · superellipse 更方 · notch 切角 · scoop 内凹。"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    { corner_shape_box("round 圆弧", CornerShape::Round) }
                    { corner_shape_box("angle 斜切", CornerShape::Angle) }
                    { corner_shape_box("superellipse(4)", CornerShape::Superellipse(4.0)) }
                    { corner_shape_box("superellipse(8)", CornerShape::Superellipse(8.0)) }
                    { corner_shape_box("notch 切口", CornerShape::Notch) }
                    { corner_shape_box("scoop 内凹", CornerShape::Scoop) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"border-collapse / border-spacing"</Label>
                <Label text_size="0.65rem" color="muted-foreground" leading="normal">
                    "布局尚未把 table 的 collapse/spacing 接入 Taffy；下面用格子 gap 对照 separate vs 贴合。"
                </Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        <GStack cols="2" gap="0.5rem">
                            { spacing_cell() }
                            { spacing_cell() }
                            { spacing_cell() }
                            { spacing_cell() }
                        </GStack>
                        { caption("separate · gap 8px") }
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <GStack cols="2" gap="0">
                            { spacing_cell() }
                            { spacing_cell() }
                            { spacing_cell() }
                            { spacing_cell() }
                        </GStack>
                        { caption("collapse 视觉 · gap 0") }
                    </VStack>
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"逻辑属性 (LTR)"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="start" justify="start">
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("8rem").h("4.5rem")
                                    .bg(parse_color("muted"))
                                    .border_inline(parse_color("primary"), 6.0)
                                    .border_block(parse_color("destructive"), 3.0)
                            )
                        }
                        { caption("左右 primary 6 · 上下 red 3") }
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        {
                            Element::from(
                                r#box()
                                    .w("8rem").h("4.5rem")
                                    .bg(parse_color("muted"))
                                    .border_inline_start(parse_color("chart-1"), 6.0)
                                    .border_inline_end(parse_color("accent"), 2.0)
                                    .border_block_start(parse_color("primary"), 4.0)
                                    .border_block_end(parse_color("destructive"), 8.0)
                            )
                        }
                        { caption("上主色4 · 右accent2 · 下红8 · 左chart6") }
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// CSS `outline` 全能力展示。
#[component]
pub(crate) fn OutlineShowcase() -> jadix::core::tree::Element {
    fn outline_box(
        label: &str,
        color: [f32; 4],
        width: f32,
        style: &str,
        offset: f32,
        rounded: f32,
    ) -> Element {
        // outline 画出 border-box 之外且不占布局；用 padding 把描边+offset 从邻居里隔开。
        let pad = (width + offset + 10.0).max(12.0);
        let pad_s = format!("{pad}px");
        let el: Element = Element::from(
            r#box()
                .w("7rem")
                .h("4rem")
                .bg(parse_color("muted"))
                .outline(color, width)
                .outline_style(style)
                .rounded(rounded),
        )
        .outline_offset(offset);
        ui! {
            <VStack gap="0.25rem" items="center" p={pad_s.as_str()} overflow="visible">
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
                .bg(parse_color("muted"))
                .rounded(8.0),
        )
        .apply_outline_shorthand(shorthand);
        let label = shorthand.to_string();
        ui! {
            <VStack gap="0.25rem" items="center" p="14px" overflow="visible">
                { el }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

    let oc = parse_color("primary");

    ui! {
        <MyCard title="14) CSS Outline 全能力">
            <VStack gap="1rem" w="100%" overflow="visible">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "outline 不占布局空间，会画出盒子外沿；示例用 padding 预留描边+offset，避免相邻描边重叠。圆角上的 dotted/dashed 沿周长取样。"
                </Label>

                <Label font_weight="700" text_size="0.875rem">"outline-style 样式"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    { outline_box("solid", oc, 3.0, "solid", 0.0, 8.0) }
                    { outline_box("dotted", oc, 3.0, "dotted", 0.0, 8.0) }
                    { outline_box("dashed", oc, 3.0, "dashed", 0.0, 8.0) }
                    { outline_box("double", oc, 5.0, "double", 0.0, 8.0) }
                    { outline_box("groove", oc, 8.0, "groove", 0.0, 8.0) }
                    { outline_box("ridge", oc, 8.0, "ridge", 0.0, 8.0) }
                    { outline_box("inset", oc, 8.0, "inset", 0.0, 8.0) }
                    { outline_box("outset", oc, 8.0, "outset", 0.0, 8.0) }
                    { outline_box("none", oc, 3.0, "none", 0.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-width"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    { outline_box("1px", oc, 1.0, "solid", 0.0, 8.0) }
                    { outline_box("2px", oc, 2.0, "solid", 0.0, 8.0) }
                    { outline_box("4px", oc, 4.0, "solid", 0.0, 8.0) }
                    { outline_box("8px", oc, 8.0, "solid", 0.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-offset"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    { outline_box("offset: 0", oc, 3.0, "solid", 0.0, 8.0) }
                    { outline_box("offset: 4px", oc, 3.0, "solid", 4.0, 8.0) }
                    { outline_box("offset: 8px", oc, 3.0, "solid", 8.0, 8.0) }
                    { outline_box("offset: 12px", oc, 3.0, "solid", 12.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline-color"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    { outline_box("primary", parse_color("primary"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("destructive", parse_color("destructive"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("chart-1", parse_color("chart-1"), 3.0, "solid", 0.0, 8.0) }
                    { outline_box("accent", parse_color("accent"), 3.0, "dashed", 0.0, 8.0) }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"outline 简写"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    { shorthand_box("3px solid red") }
                    { shorthand_box("4px dashed blue") }
                    { shorthand_box("2px dotted green") }
                </HStack>

                <Label font_weight="700" text_size="0.875rem">"utility class"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="start" justify="start" overflow="visible">
                    <VStack gap="0.25rem" items="center" p="16px" overflow="visible">
                        <Box w="7rem" h="4rem" bg="muted" rounded="0.5rem" class="outline outline-2 outline-offset-4" />
                        <Label text_size="0.6rem" color="muted-foreground">"outline outline-2 outline-offset-4"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// ANIM 对齐：steps()、text color transition、layout 属性、reduced-motion。
fn ensure_gallery_motion_extras() {
    use jadix::style::{
        AnimatableValue, KeyframeStop, PropertyId, parse_clip_path, register_clip_path,
        register_keyframes,
    };
    register_keyframes(
        "gal-linear-slide",
        vec![
            KeyframeStop::at(0.0).translate([0.0, 0.0]),
            KeyframeStop::at(1.0).translate([88.0, 0.0]),
        ],
    );
    register_keyframes(
        "gal-offset-run",
        vec![
            KeyframeStop::at(0.0).set(PropertyId::OffsetDistance, AnimatableValue::Scalar(0.0)),
            KeyframeStop::at(1.0).set(PropertyId::OffsetDistance, AnimatableValue::Scalar(1.0)),
        ],
    );
    register_keyframes(
        "gal-origin-swing",
        vec![
            KeyframeStop::at(0.0)
                .set(
                    PropertyId::TransformOrigin,
                    AnimatableValue::Vec2([0.0, 0.0]),
                )
                .rotate_deg(-18.0),
            KeyframeStop::at(1.0)
                .set(
                    PropertyId::TransformOrigin,
                    AnimatableValue::Vec2([1.0, 1.0]),
                )
                .rotate_deg(18.0),
        ],
    );
    if let Some(clip) = parse_clip_path("polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)") {
        register_clip_path("gal-diamond", clip);
    }
}

#[component]
pub(crate) fn AnimAlignmentShowcase() -> jadix::core::tree::Element {
    ensure_gallery_motion_extras();
    ui! {
        <MyCard title="ANIM 对齐（CSS animation / transition / keyframes）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "完整对齐面：steps() 步进、transition-colors 文本色、padding 布局动画、will-change 提示。`transition: transform spring()` 是原生弹簧腿（可中断、不靠 duration）。系统 JADIX_REDUCED_MOTION=1 时时长归零。Accordion 备选：JADIX_ACCORDION_GRID_FR=1 走 grid-template-rows 0fr→1fr（默认仍是 accordion-down max-height）。"
                </Label>
                <Label text_size="0.7rem" font_weight="600">"steps(4, jump-end) opacity"</Label>
                <Box w="6rem" h="3rem" bg="primary" rounded="0.5rem"
                    class="will-change-opacity"
                    animation="1s steps(4, jump-end) infinite fade-in-0" />
                <Label text_size="0.7rem" font_weight="600">"linear() 分段 easing（停住 → 冲刺 → 停住）"</Label>
                <Box w="8rem" h="2.5rem" relative={true} bg="muted" rounded="0.5rem" overflow="hidden">
                    <Box w="1.5rem" h="1.5rem" bg="primary" rounded="0.25rem"
                        absolute={true} top="0.5rem" left="0.5rem"
                        animation="2.4s linear(0, 0 30%, 1 70%, 1) infinite alternate gal-linear-slide" />
                </Box>
                <Label text_size="0.7rem" font_weight="600">"clip-path: path() / url(#id) / 两层相交"</Label>
                <HStack gap="0.75rem" wrap="wrap" items="end">
                    <VStack gap="0.2rem" items="center">
                        <Box w="4rem" h="4rem" bg="primary"
                            clip_path={r#"path("M 0 64 L 32 0 L 64 64 Z")"#} />
                        <Label text_size="0.55rem" color="muted-foreground">"path()"</Label>
                    </VStack>
                    <VStack gap="0.2rem" items="center">
                        <Box w="4rem" h="4rem" bg="destructive" clip_path="url(#gal-diamond)" />
                        <Label text_size="0.55rem" color="muted-foreground">"url(#gal-diamond)"</Label>
                    </VStack>
                    <VStack gap="0.2rem" items="center">
                        <Box w="4.5rem" h="4.5rem" bg="muted" clip_path="polygon(0% 0%, 100% 0%, 80% 100%, 20% 100%)">
                            <Box w="100%" h="100%" bg="primary" clip_path="circle(45% at 50% 50%)" />
                        </Box>
                        <Label text_size="0.55rem" color="muted-foreground">"祖先∩本地"</Label>
                    </VStack>
                </HStack>
                <Label text_size="0.7rem" font_weight="600">"offset-path + transform-origin 动画"</Label>
                <HStack gap="1.25rem" wrap="wrap" items="center">
                    <Box w="8rem" h="3rem" relative={true} bg="muted" rounded="0.5rem">
                        <Box w="1.25rem" h="1.25rem" bg="primary" rounded="0.25rem"
                            offset_path={r#"path("M 8 12 L 112 12")"#}
                            animation="2.4s linear infinite gal-offset-run" />
                    </Box>
                    <Box w="3rem" h="3rem" bg="accent" rounded="0.35rem"
                        animation="1.6s ease-in-out infinite alternate gal-origin-swing" />
                </HStack>
                <Label text_size="0.7rem" font_weight="600">"extra_shadows 双层 box-shadow"</Label>
                <Box w="7rem" h="3rem" bg="background" rounded="0.5rem"
                    box_shadow="6px 6px 0 0 hsl(222 47% 11% / 0.22), 12px 12px 0 0 hsl(222 47% 11% / 0.1)" />
                <Label text_size="0.7rem" font_weight="600">"transition-colors（含 color）"</Label>
                <Label class="transition-colors duration-300 hover:text-destructive" text_size="0.875rem">
                    "悬停变色"
                </Label>
                <Label text_size="0.7rem" font_weight="600">"padding 布局动画"</Label>
                <Box w="8rem" h="3.5rem" bg="muted" rounded="0.5rem" overflow="hidden">
                    <Box w="100%" h="100%" class="p-1 transition-padding duration-300 hover:p-4">
                        <Label text_size="0.65rem">"hover padding"</Label>
                    </Box>
                </Box>
                <Label text_size="0.7rem" font_weight="600">"spring() 可中断 hover"</Label>
                <Box w="4rem" h="4rem" bg="primary" rounded="0.375rem"
                    class="hover:scale-125"
                    transition="transform spring()" />
            </VStack>
        </MyCard>
    }
}

/// CSS `scale` + `zoom` Gallery 展示。
#[component]
pub(crate) fn TransitionShowcase() -> jadix::core::tree::Element {
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
pub(crate) fn ScaleZoomShowcase() -> jadix::core::tree::Element {
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
pub(crate) fn TransformShowcase() -> jadix::core::tree::Element {
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
pub(crate) fn RotateTranslateShowcase() -> jadix::core::tree::Element {
    use jadix::style::with_class_list;

    fn translate_demo(
        label: &str,
        color_token: &str,
        utility_class: &str,
        box_w: &str,
        box_h: &str,
        cell_w: &str,
        cell_h: &str,
    ) -> Element {
        let ghost = r#box()
            .w(box_w)
            .h(box_h)
            .border(parse_color("muted-foreground"), 1.0)
            .border_style("dashed")
            .rounded(6.0)
            .absolute(true);
        let moved = with_class_list(
            Element::from(
                r#box()
                    .w(box_w)
                    .h(box_h)
                    .bg(parse_color(color_token))
                    .rounded(6.0)
                    .absolute(true),
            ),
            utility_class,
        );
        let slot = r#box()
            .w(box_w)
            .h(box_h)
            .relative(true)
            .overflow("visible")
            .children([Element::from(ghost), moved]);
        let cell = r#box()
            .w(cell_w)
            .h(cell_h)
            .overflow("visible")
            .items("center")
            .justify("center")
            .child(slot);
        ui! {
            <VStack gap="0.25rem" items="center">
                { Element::from(cell) }
                <Label text_size="0.6rem" color="muted-foreground">{ label }</Label>
            </VStack>
        }
    }

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
                        <Box w="5.5rem" h="3.5rem" bg="chart-2" rounded="0.375rem" class="rotate-45 flex items-center justify-center">
                            <Label text_size="0.7rem" color="primary-foreground" font_weight="600">"45°"</Label>
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
                        <Box w="5.5rem" h="3.5rem" bg="chart-4" rounded="0.375rem" class="transition-transform hover:rotate-45 flex items-center justify-center">
                            <Label text_size="0.7rem" color="primary-foreground" font_weight="600">"悬停转"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"hover:rotate-45 + Label"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Button class="animate-spin" variant="secondary">"Spin"</Button>
                        <Label text_size="0.6rem" color="muted-foreground">"Button spin"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="5.5rem" h="3.5rem" bg="secondary" rounded="0.375rem" class="scale-[3] flex items-center justify-center">
                            <Label text_size="0.7rem" font_weight="600">"MSDF"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"scale-3 → 三通道 MSDF"</Label>
                    </VStack>
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "4 · translate — px 偏移"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "视觉平移，不影响布局。灰色虚框表示原始位置。"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem" overflow="visible">
                    { translate_demo("translate-x-4", "chart-1", "translate-x-4", "3.5rem", "3.5rem", "7.5rem", "6rem") }
                    { translate_demo("translate-y-4", "chart-2", "translate-y-4", "3.5rem", "3.5rem", "7.5rem", "6rem") }
                    { translate_demo("-translate-x-4", "chart-3", "-translate-x-4", "3.5rem", "3.5rem", "7.5rem", "6rem") }
                    { translate_demo("X+Y 偏移", "chart-4", "translate-x-4 translate-y-4", "3.5rem", "3.5rem", "7.5rem", "6.5rem") }
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "5 · translate — 百分比 + 任意值"
                </Label>
                <HStack gap="1.5rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem" overflow="visible">
                    { translate_demo("x-1/2 (50%)", "chart-1", "translate-x-1/2", "4rem", "3rem", "9rem", "6.5rem") }
                    { translate_demo("x-full (100%)", "chart-2", "translate-x-full", "4rem", "3rem", "10rem", "6.5rem") }
                    { translate_demo("-x-1/2", "chart-3", "-translate-x-1/2", "4rem", "3rem", "9rem", "6.5rem") }
                    { translate_demo("x-[30px]", "chart-4", "translate-x-[30px]", "4rem", "3rem", "9rem", "6.5rem") }
                    { translate_demo("y-[50%]", "chart-5", "translate-y-[50%]", "4rem", "3rem", "9rem", "7rem") }
                </HStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "6 · rotate + translate 组合"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="center" justify="start" p="1rem" bg="muted" rounded="0.5rem" overflow="visible">
                    { translate_demo("旋转+平移", "primary", "rotate-45 translate-x-4", "3.5rem", "3.5rem", "8.5rem", "8.5rem") }
                    { translate_demo("旋转+%平移+缩放", "destructive", "rotate-12 translate-x-1/2 scale-110", "3.5rem", "3.5rem", "9rem", "8.5rem") }
                    { translate_demo("hover 组合", "chart-4", "transition-transform hover:rotate-45 hover:translate-x-4 hover:scale-110", "3.5rem", "3.5rem", "8.5rem", "8.5rem") }
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ScrollShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="4) ScrollArea + Scrollbar">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "inside = overlay on content (no padding reserve, any side); outside = dedicated flex lane. Visibility: auto / always / never."
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A) Inset: overlay (inside) / lane (outside) / mixed"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-inset-inside" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="inside" scrollbar_visibility="always"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"inside (overlay)"</Label>
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

                <Label font_weight="700" text_size="0.875rem">"D) Overlay chrome: transparent track + thumb only"</Label>
                <HStack gap="0.5rem" items="start" wrap="wrap">
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-overlay-thumb" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="inside" scrollbar_visibility="always"
                            scrollbar_track_color="transparent"
                            scrollbar_thumb_color="scrollbar-thumb/50"
                            scrollbar_variant="capsule" overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"thumb only"</Label>
                                <For each={1..=8} key={*item}>
                                    <Label>{ format!("overlay row {:02}", item) }</Label>
                                </For>
                            </VStack>
                        </ScrollArea>
                    </Box>
                    <Box w="13.5rem" h="7.5rem" bg="background" rounded="0.5rem" p="0.25rem">
                        <ScrollArea id="sa-overlay-left" w="100%" h="100%" p="0.5rem"
                            scrollbar_inset="inside" scrollbar_y_side="left" scrollbar_visibility="always"
                            scrollbar_track_color="transparent"
                            scrollbar_thumb_color="scrollbar-thumb/50"
                            overflow_y="auto" overflow_x="auto">
                            <VStack gap="0.25rem" min_w="18rem">
                                <Label font_weight="700">"left overlay"</Label>
                                <For each={1..=8} key={*item}>
                                    <Label>{ format!("same inset {:02}", item) }</Label>
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
pub(crate) fn AccordionShowcase() -> jadix::core::tree::Element {
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
pub(crate) fn CollapsibleShowcase() -> jadix::core::tree::Element {
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
pub(crate) fn ResizableShowcase() -> jadix::core::tree::Element {
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
                .bg(jadix::tokens::muted())
                .p("0.75rem")
                .child(
                    jadix::primitives::text::text("Builder left")
                        .color(jadix::style::parse_text_color("foreground")),
                )
                .into()
        })
        .right(|| {
            r#box()
                .w("100%")
                .h("100%")
                .bg(jadix::tokens::background())
                .p("0.75rem")
                .child(
                    jadix::primitives::text::text("Builder right")
                        .color(jadix::style::parse_text_color("foreground")),
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
/// 对照 docs/engine/layout_subtree.md；live 路径为 mark_dirty + app-root compute。
#[component]
pub(crate) fn LayoutSubtreeShowcase() -> jadix::core::tree::Element {
    let wide = use_signal("gal_wp23_wide", || false);

    ui! {
        <MyCard title="WP-2.3) 子树布局上溢升父">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 docs/engine/layout_subtree.md。中间行为 Auto 高 + wrap；加宽 chip 后行应折行增高，下方 footer 下移（不得被裁切叠住）。"
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
/// 计数在动作的 **下一帧 present 后** 写入，刷新快照会 suppress 自身那帧，避免 +1 污染。
#[component]
pub(crate) fn DamageShowcase() -> jadix::core::tree::Element {
    use jadix::core::tree::LayoutStatus;

    let count = use_signal("gal_wp22_count", || 0_u32);
    let show_extra = use_signal("gal_wp22_struct", || false);
    let partial_snap = use_signal("gal_wp22_partial_snap", || 0_u64);
    let full_snap = use_signal("gal_wp22_full_snap", || 0_u64);
    let skip_snap = use_signal("gal_wp22_skip_snap", || 0_u64);
    let patch_snap = use_signal("gal_wp22_patch_snap", || 0_u64);
    let base_partial = use_signal("gal_wp22_base_partial", || 0_u64);
    let base_full = use_signal("gal_wp22_base_full", || 0_u64);
    let base_skip = use_signal("gal_wp22_base_skip", || 0_u64);
    let base_patch = use_signal("gal_wp22_base_patch", || 0_u64);
    let scene_line = use_signal("gal_wp22_scene", || String::new());

    let apply_snap = move |s: jadix::PolicySnapshot| {
        partial_snap.set(s.partial_load);
        full_snap.set(s.full_clear);
        skip_snap.set(s.skipped_siblings);
        patch_snap.set(s.patched_dl);
        let b = jadix::last_scene_batches();
        scene_line.set(format!(
            "scene batches: quad={} surface={} underline={} text={} shadow_tex={}",
            b.quad, b.surface, b.underline, b.text, b.shadow_tex
        ));
    };

    let mut root = ui! {
        <MyCard title="WP-2.2) 细粒度 damage（策略可测）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 docs/engine/damage.md。数字是上次动作 present 后的累计（不含本卡片改字那一帧）。paint-only 默认 GPU retain+裁剪；滚动失效视口层；结构仍 FullClear。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "勾选：① 重置→连点同宽+1（不必再点刷新）：ΔPartialLoad↑、ΔFullClear≈0、Δskipped_siblings↑ ② 切换结构：ΔFullClear↑ 且不应整窗卡顿"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!(
                        "累计: PartialLoad={} (+{})  FullClear={} (+{})  skipped_siblings={} (+{})  patched_dl={} (+{})",
                        partial_snap.get(),
                        partial_snap.get().saturating_sub(base_partial.get()),
                        full_snap.get(),
                        full_snap.get().saturating_sub(base_full.get()),
                        skip_snap.get(),
                        skip_snap.get().saturating_sub(base_skip.get()),
                        patch_snap.get(),
                        patch_snap.get().saturating_sub(base_patch.get())
                    ) }
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { scene_line.get() }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Label text_size="1.5rem" font_weight="700" leading="none">
                        { format!("{:02}", count.get() % 100) }
                    </Label>
                    <Button variant="default" on_click={move || {
                        count.set(count.get().wrapping_add(1));
                        jadix::arm_policy_snapshot();
                    }}>
                        "同宽 +1（00–99）"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        show_extra.set(!show_extra.get());
                        jadix::arm_policy_snapshot();
                    }}>
                        "切换结构（Show）"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        jadix::suppress_policy_counts(2);
                        jadix::arm_policy_snapshot();
                    }}>
                        "刷新快照"
                    </Button>
                    <Button variant="secondary" on_click={move || {
                        jadix::reset_frame_policy_counters();
                        jadix::suppress_policy_counts(2);
                        partial_snap.set(0);
                        full_snap.set(0);
                        skip_snap.set(0);
                        patch_snap.set(0);
                        base_partial.set(0);
                        base_full.set(0);
                        base_skip.set(0);
                        base_patch.set(0);
                        scene_line.set(String::new());
                    }}>
                        "重置计数"
                    </Button>
                </HStack>
                <Show when={show_extra.get()}>
                    <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        <Label text_size="0.875rem" leading="normal">
                            "结构变更样例：挂载/卸载此块应走全帧 Clear（ΔFullClear↑），但不应重建整页 Taffy。"
                        </Label>
                    </Box>
                </Show>
            </VStack>
        </MyCard>
    };
    root = root.reactive_sync(move |_, _| {
        if let Some(s) = jadix::take_policy_snapshot() {
            jadix::suppress_policy_counts(1);
            apply_snap(s);
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    });
    root
}

/// Fine-grained matrix (suggested): who ran this `set`. Not a D1–D7 / B8 exit gate.
#[component]
pub(crate) fn FgMatrixShowcase() -> jadix::core::tree::Element {
    use jadix::app::reactive::{bind_miss_count, unkeyed_for_rebuild_count};
    use jadix::core::reactivity::{EffectPhase, take_flushed_effects, ui_root_effect_dep_count};

    let n = use_signal("gal_fg_n", || 0_u32);
    let last_render = use_signal("gal_fg_render", || 0_usize);
    let last_total = use_signal("gal_fg_total", || 0_usize);
    let root_deps = use_signal("gal_fg_root", || ui_root_effect_dep_count());
    let miss = use_signal("gal_fg_miss", || bind_miss_count());
    let unkeyed = use_signal("gal_fg_unkeyed", || unkeyed_for_rebuild_count());

    ui! {
        <MyCard title="FG 矩阵（建议 · 不替代库测）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 plans/true-fine-grained.md §5。点「同宽 +1」后读本次 flush：期望 Render≈1、根 dep 不抬、无 bind miss。不要把 PartialLoad / GPU 填色当 D1–D7 退出门。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威仍是库测（builder_live_ / graph_flush_ / menubar_trigger_open_patches）。本卡只做手工对照；点按会 drain take_flushed_effects，勿与其它诊断卡同时点。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { format!(
                        "n={:02}  |  last Render={}  |  last flush={}  |  root_deps={}  |  bind_miss={}  |  unkeyed_For={}",
                        n.get() % 100,
                        last_render.get(),
                        last_total.get(),
                        root_deps.get(),
                        miss.get(),
                        unkeyed.get()
                    ) }
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Label text_size="1.5rem" font_weight="700" leading="none">
                        { format!("{:02}", n.get() % 100) }
                    </Label>
                    <Button variant="default" on_click={move || {
                        let _ = take_flushed_effects();
                        n.set(n.peek().wrapping_add(1));
                        let flushed = take_flushed_effects();
                        last_render.set(
                            flushed
                                .iter()
                                .filter(|(_, phase)| *phase == EffectPhase::Render)
                                .count(),
                        );
                        last_total.set(flushed.len());
                        root_deps.set(ui_root_effect_dep_count());
                        miss.set(bind_miss_count());
                        unkeyed.set(unkeyed_for_rebuild_count());
                    }}>
                        "同宽 +1"
                    </Button>
                    <Button variant="outline" on_click={move || {
                        root_deps.set(ui_root_effect_dep_count());
                        miss.set(bind_miss_count());
                        unkeyed.set(unkeyed_for_rebuild_count());
                    }}>
                        "刷新计数"
                    </Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// WP-2.4 Gallery 验收：Owner dispose 后 Anchors/Bindings 不残留；scoped id 可演示。
#[component]
pub(crate) fn OwnerRegistryShowcase() -> jadix::core::tree::Element {
    use jadix::app::reactive::{
        component_props_count, registered_anchor_count, registered_binding_count,
    };
    use jadix::core::reactivity::{scoped_focus_id, scoped_template_id};

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
                    "命令：cargo test -p jadix --lib registry_owner"
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
pub(crate) fn StoreResourceShowcase() -> jadix::core::tree::Element {
    use jadix::core::reactivity::{create_effect, create_memo};
    use jadix::core::resource::{Resource, ResourceState};
    use jadix::core::store::create_store;
    use jadix::prelude::throw_resource_error;

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

    ui! {
        <MyCard title="WP-2.5) Store produce + Resource→EB">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "权威判定：库测全绿即 WP-2.5 通过。本卡片只做手工对照；细则见 development-docs/AUTHORITY.md「WP-2.5 验收」与 development-docs/error_handling.md。"
                </Label>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "A · 库测（必须）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "命令：cargo test -p jadix --lib produce_notifies   与   cargo test -p jadix --lib resource_err_to_error_boundary   与   cargo test -p jadix --lib create_resource_err_sets"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "通过：三测均 passed。失败：任一 FAILED。"
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
                    "B2 · Resource Err → ErrorBoundary；Loading → Suspense（建议）"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "契约：巩固期须显式 throw_resource_error。1) 初始 children ok  2) 「注入 Loading」→ 框内 loading…  3) 「注入 Err」→ fallback: gallery boom  4) 「复位 Ready」→ children ok。旁路行 resource= 应同步变。"
                </Label>
                <Label w="100%" text_size="0.875rem" font_weight="600" leading="normal">
                    { format!("resource(旁路) = {}  |  note: {}", res_summary.get(), action_note.get()) }
                </Label>
                <Box bg="muted" rounded="0.375rem" p="0.75rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                    <ErrorBoundary id="gal_wp25_eb" fallback={|msg| {
                        ui! {
                            <Label w="100%" text_size="0.875rem" color="destructive" font_weight="600" leading="normal">
                                { format!("fallback: {msg}") }
                            </Label>
                        }
                    }}>
                        { throw_resource_error(resource); ui! {
                            <Suspense id="gal_wp25_sus" fallback={|| {
                                ui! {
                                    <Label w="100%" text_size="0.875rem" color="muted-foreground" leading="normal">
                                        "loading…"
                                    </Label>
                                }
                            }}>
                                <Label w="100%" text_size="0.875rem" leading="normal">
                                    { format!("children ok · resource = {:?}", resource.get()) }
                                </Label>
                            </Suspense>
                        } }
                    </ErrorBoundary>
                </Box>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Button variant="secondary" on_click={move || {
                        res_state.set(ResourceState::Loading { previous: None });
                        action_note.set("已注入 Loading → 框内应见 loading…".into());
                    }}>
                        "注入 Loading"
                    </Button>
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
pub(crate) fn RootBareGetShowcase() -> jadix::core::tree::Element {
    use jadix::core::reactivity::ui_root_effect_dep_count;

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
                    "命令：cargo test -p jadix --lib root_bare_get"
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
pub(crate) fn IdentityApiShowcase() -> jadix::core::tree::Element {
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
                    "命令：cargo test -p jadix --lib navbar_without_id   与   bar_chart_without_id   与   toggle_empty_id   与   toggle_uncontrolled_uses_scoped"
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

/// Light zinc: `muted` ≈ `secondary` ≈ `accent` (oklch 0.97). Adjacent onion
/// layers must use `chart-*` / `destructive` or padding/border disappear.
/// Keep tokens in sync with `crate_tests/layout/box_model_gallery_colors.rs`.
const GAL_BM_MARGIN: &str = "muted";
const GAL_BM_PADDING: &str = "chart-1";
const GAL_BM_CONTENT: &str = "chart-4";
const GAL_BM_BORDER_T: &str = "destructive";
const GAL_BM_BORDER_R: &str = "chart-2";
const GAL_BM_BORDER_B: &str = "chart-5";
const GAL_BM_BORDER_L: &str = "chart-3";

fn gal_legend_chip(token: &'static str, caption: &'static str) -> Element {
    ui! {
        <HStack gap="0.25rem" items="center">
            <Box
                w="0.75rem"
                h="0.75rem"
                min_w="0.75rem"
                min_h="0.75rem"
                rounded="0.125rem"
                bg={token}
                borderStyle="solid"
                borderColor="border"
                borderWidth={1.0}
            />
            <Label text_size="0.75rem">{ caption }</Label>
        </HStack>
    }
}

pub(crate) fn gal_box_model_onion() -> Element {
    let content = Element::from(
        r#box()
            .w("100%")
            .h("3rem")
            .bg(parse_color(GAL_BM_CONTENT))
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
            .bg(parse_color(GAL_BM_PADDING))
            .child(content),
    );
    let border_box = Element::from(
        r#box()
            .w("100%")
            .border_style("solid")
            .border_top(parse_color(GAL_BM_BORDER_T), 8.0)
            .border_right(parse_color(GAL_BM_BORDER_R), 4.0)
            .border_bottom(parse_color(GAL_BM_BORDER_B), 12.0)
            .border_left(parse_color(GAL_BM_BORDER_L), 16.0)
            .child(padding),
    );
    Element::from(
        r#box()
            .w("22rem")
            .p("1.25rem")
            .bg(parse_color(GAL_BM_MARGIN))
            .rounded("0.5rem")
            .child(border_box),
    )
}

/// WP-2.9 / B15 Gallery：完整 CSS 盒模型（单位 · box-sizing · 分边 border · outline）。
#[component]
pub(crate) fn BoxModelShowcase() -> jadix::core::tree::Element {
    let hits = use_signal("gal_wp29_outline_hits", || 0_u32);
    let outline_box: Element = Element::from(
        r#box()
            .w("10rem")
            .h("3rem")
            .bg(parse_color("chart-2"))
            .rounded("0.375rem")
            .outline(parse_color("destructive"), 8.0)
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
                    "手册 docs/engine/box_model.md。默认 BorderBox；% = 父 content-box；outline/shadow 不进布局。库测：cargo test -p jadix --lib box_model_contract"
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
                            { gal_legend_chip(GAL_BM_MARGIN, "muted = margin") }
                            { gal_legend_chip(GAL_BM_BORDER_T, "彩边 = border") }
                            { gal_legend_chip(GAL_BM_PADDING, "chart-1 = padding") }
                            { gal_legend_chip(GAL_BM_CONTENT, "chart-4 = content") }
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
                    <Box w="25vw" h="2rem" bg="chart-2" rounded="0.375rem" items="center" justify="center">
                        <Label text_size="0.75rem" text_color="primary-foreground">"25vw"</Label>
                    </Box>
                    <Label text_size="0.75rem" font_weight="600">"25dvw"</Label>
                    <Box w="25dvw" h="2rem" bg="chart-1" rounded="0.375rem" items="center" justify="center">
                        <Label text_size="0.75rem" text_color="primary-foreground">"25dvw"</Label>
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
                            <Box w="4.5rem" h="10dvh" bg="chart-3" rounded="0.375rem" items="center" justify="center">
                                <Label text_size="0.75rem" text_color="primary-foreground">"10dvh"</Label>
                            </Box>
                        </VStack>
                        <VStack gap="0.25rem">
                            <Label text_size="0.75rem" font_weight="600">"padding 1em @ 20px"</Label>
                            <Box w="8rem" text_size={20.0} p="1em" bg="chart-1" rounded="0.375rem" items="center" justify="center">
                                <Label text_size="0.75rem" text_color="primary-foreground">"1em pad"</Label>
                            </Box>
                        </VStack>
                    </HStack>
                    <Label text_size="0.75rem" font_weight="600">"calc(100% - 2rem)（相对卡片 content，不是视口）"</Label>
                    <Box w="100%" p="0.5rem" bg="muted" rounded="0.375rem">
                        <Box w="calc(100% - 2rem)" h="2.5rem" bg="chart-2" rounded="0.25rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"100% - 2rem"</Label>
                        </Box>
                    </Box>
                </VStack>

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "3 · box-sizing：同 w+padding，box-content 应更宽"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="end">
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"box-border（默认）"</Label>
                        <Box class="box-border" w="8rem" px="1rem" py="0.5rem" bg="chart-2" rounded="0.375rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label text_size="0.75rem" text_color="primary-foreground">"w=8rem + px"</Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.25rem">
                        <Label text_size="0.75rem" font_weight="600">"box-content"</Label>
                        <Box class="box-content" w="8rem" px="1rem" py="0.5rem" bg="chart-1" rounded="0.375rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label text_size="0.75rem" text_color="primary-foreground">"padding 在外侧"</Label>
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
                            borderRightColor="chart-2"
                            borderBottomColor="chart-5"
                            borderLeftColor="chart-3"
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

                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "6 · overflow + border-radius：子块探出圆角父级应被切掉（clip 边仍是 border-box）"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="center">
                    <Box
                        w="8rem"
                        h="8rem"
                        rounded="2rem"
                        overflow="hidden"
                        bg="muted"
                        relative={true}
                    >
                        <Box
                            w="10rem"
                            h="10rem"
                            bg="destructive"
                            absolute={true}
                            left="-1.25rem"
                            top="-1.25rem"
                        />
                    </Box>
                    <Label text_size="0.75rem" color="muted-foreground" w="14rem" leading="normal">
                        "红块比圆角父级大并偏移到角外；可见轮廓应是圆角，不是方裁。"
                    </Label>
                </HStack>
            </VStack>
        </MyCard>
    }
}

fn writing_mode_sample(
    mode: &'static str,
    sample: &'static str,
    w: &'static str,
    h: &'static str,
) -> Element {
    ui! {
        <VStack items="center" gap="0.35rem">
            <Label text_size="0.75rem" font_weight="600" leading="normal">{ mode }</Label>
            <Box
                writing_mode={mode}
                w={w}
                h={h}
                bg="muted"
                rounded="0.5rem"
                p="0.5rem"
                items="center"
                justify="center"
            >
                <Label text_size="0.875rem" font_weight="600" leading="normal">{ sample }</Label>
            </Box>
        </VStack>
    }
}

/// CSS `writing-mode` 五值：关键字在盒外横排，盒内文案随模式旋转。
#[component]
pub(crate) fn WritingModeShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="writing-mode（CSS）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "MDN：horizontal-tb / vertical-rl / vertical-lr / sideways-rl / sideways-lr（含 SVG 别名）。垂直模式把 flex 主轴映到物理列，文本 sideways 旋转。text-orientation: mixed 的直立汉字尚未做。"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="end">
                    { writing_mode_sample("horizontal-tb", "横排 Hello", "9.5rem", "3.25rem") }
                    { writing_mode_sample("vertical-rl", "竖排文字", "3.25rem", "10rem") }
                    { writing_mode_sample("vertical-lr", "竖排文字", "3.25rem", "10rem") }
                    { writing_mode_sample("sideways-rl", "侧转 Abc", "3.25rem", "10rem") }
                    { writing_mode_sample("sideways-lr", "侧转 Abc", "3.25rem", "10rem") }
                </HStack>
                <Label w="100%" text_size="0.75rem" font_weight="600" leading="normal">
                    "vertical-rl + flex-direction: row → 子项沿块轴（竖向）排列"
                </Label>
                <HStack gap="0.75rem" wrap="wrap" items="center">
                    <Box
                        writing_mode="vertical-rl"
                        class="flex-row"
                        w="4.25rem"
                        h="11rem"
                        bg="muted"
                        rounded="0.5rem"
                        p="0.4rem"
                        gap="0.35rem"
                    >
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-1" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"1"</Label>
                        </Box>
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-2" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"2"</Label>
                        </Box>
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-3" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"3"</Label>
                        </Box>
                    </Box>
                    <Label text_size="0.75rem" color="muted-foreground" w="16rem" leading="normal">
                        "三个色块应上下叠，而不是左右排。对照：同样 flex-row 在 horizontal-tb 下是横排。"
                    </Label>
                    <Box
                        writing_mode="horizontal-tb"
                        class="flex-row"
                        w="11rem"
                        h="4.25rem"
                        bg="muted"
                        rounded="0.5rem"
                        p="0.4rem"
                        gap="0.35rem"
                    >
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-1" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"1"</Label>
                        </Box>
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-2" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"2"</Label>
                        </Box>
                        <Box w="2.4rem" h="2.4rem" min_w="2.4rem" min_h="2.4rem" bg="chart-3" rounded="0.35rem" items="center" justify="center">
                            <Label text_size="0.75rem" text_color="primary-foreground">"3"</Label>
                        </Box>
                    </Box>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn LogicalShowcase() -> jadix::core::tree::Element {
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
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "完整五值展示见上方「writing-mode（CSS）」卡片。此处只留逻辑盒属性。"
                </Label>
            </VStack>
        </MyCard>
    }
}

/// WP-2.1 visual acceptance: narrow soft-wrap, Chip/Mention atomic, VirtualList font row height.

#[component]
pub(crate) fn UtilityClassShowcase() -> jadix::core::tree::Element {
    let tone = use_signal("gal-util-class-tone", || "bg-secondary".to_string());
    let busy = use_signal("gal-util-class-busy", || false);
    let loud = use_signal("gal-util-class-loud", || false);
    let forwarded = "p-3 rounded-md bg-muted";
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

                <VStack gap="0.5rem" w="100%" min_w="0">
                    <Label font_weight="700">"D) class={{变量}} / classes!"</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "局部 &str 构建期一次应用；classes! 拼接 + (token, bool) 当 classList；含 .get() 走 bind_class（本页 isolate_reads 下仍会更新）。点色板：底色应变；busy：图标旋转；outline：描边。"
                    </Label>
                    <HStack gap="0.5rem" wrap="wrap" items="center">
                        <Box class={forwarded}>
                            <Label text_size="0.75rem">"Box class={{forwarded}}"</Label>
                        </Box>
                        <Button class={forwarded} variant="outline" size="sm">"Button class={{变量}}"</Button>
                    </HStack>
                    <HStack gap="0.5rem" wrap="wrap">
                        <Button size="sm" variant="outline" on_click={move || tone.set("bg-secondary".into())}>"secondary"</Button>
                        <Button size="sm" variant="outline" on_click={move || tone.set("bg-primary".into())}>"primary"</Button>
                        <Button size="sm" variant="outline" on_click={move || tone.set("bg-destructive".into())}>"destructive"</Button>
                        <Button size="sm" variant="outline" on_click={move || busy.set(!busy.get())}>"busy"</Button>
                        <Button size="sm" variant="outline" on_click={move || loud.set(!loud.get())}>"outline"</Button>
                    </HStack>
                    <VStack
                        w="100%"
                        min_w="0"
                        gap="0.35rem"
                        class={classes!("p-4 rounded-lg", tone.get(), ("outline outline-2 outline-offset-2", loud.get()))}
                    >
                        <HStack gap="0.5rem" items="center">
                            <Icon
                                name="loader-circle"
                                size="1rem"
                                class={classes!(("animate-spin", busy.get()))}
                            />
                            <Label class={classes!("text-sm", ("font-semibold", loud.get()), ("opacity-50", busy.get()))}>
                                { if busy.get() { "处理中…" } else { "空闲" } }
                            </Label>
                        </HStack>
                        <Label text_size="0.7rem" color="muted-foreground" leading="normal" w="100%" min_w="0">
                            "下面是拼合后的 class 字符串（说明，不是漏画的样式）："
                        </Label>
                        <Label class="font-mono text-xs" w="100%" min_w="0">
                            { classes!("p-4 rounded-lg", tone.get(), ("outline outline-2 outline-offset-2", loud.get())) }
                        </Label>
                    </VStack>
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
pub(crate) fn GridAndDragShowcase() -> Element {
    let order = use_signal("gal_grid_sort_order", || {
        ["Alpha", "Bravo", "Charlie", "Delta"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    });
    let commit = use_signal(GAL_GRID_SORT_COMMIT, || None::<(usize, usize)>);
    let hover = use_signal(GAL_GRID_SORT_HOVER, || None::<usize>);

    // `for_keyed!` 闭包含 `{}`，不能放在 `ui!` 的单层 `{ ... }` 表达式里（会与 RSX 花括号扫描冲突）。
    let sort_list = jadix::reactive!(GRID_DRAG_REACTIVE, move || {
        if let Some((from, to)) = commit.get() {
            let mut v = order.get();
            jadix::utils::drag_reorder::apply_reorder_vec(&mut v, from, to);
            order.set(v);
            commit.set(None);
        }
        let hover_ix = hover.get();
        let items = order.get();
        let rows = jadix::for_keyed!(
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
        <MyCard title="Grid + drag-reorder (ShadCN Card: jadix::components::Card)">
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
