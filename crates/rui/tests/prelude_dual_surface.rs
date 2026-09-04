//! C-WP-0.3：`rui::prelude` 同时导出标签与 Builder，且可互嵌编译。

use rui::prelude::*;

#[test]
fn prelude_builder_surface_compiles() {
    let el: Element = button("prelude-btn")
        .variant("outline")
        .text("Builder")
        .into();
    let _ = el;
}

#[test]
fn prelude_input_controlled_tag_and_builder() {
    let sig = use_signal("prelude-in-val", || "hi".to_string());
    let _b: Element = input("prelude-in-b").value_signal(sig).into();
    let el = ui! {
        <Input id="prelude-in-t" value={sig} placeholder="typed" />
    };
    let _: Element = el;
}

#[test]
fn prelude_tag_surface_compiles() {
    let el = ui! {
        <Button id="prelude-tag-btn" variant="outline">"Tag"</Button>
    };
    let _: Element = el;
}

#[test]
fn prelude_badge_dual_surface_compiles() {
    let _b: Element = badge().variant("secondary").text("Builder").into();
    let el = ui! {
        <HStack gap="0.5rem">
            <Badge>"Default"</Badge>
            <Badge variant="outline">"Outline"</Badge>
            { Element::from(badge().variant("ghost").text("内嵌")) }
        </HStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_marker_dual_surface_compiles() {
    let _b: Element = ui_marker().variant("separator").content("Today").into();
    let _i: Element = marker_icon().child(Element::new().text("•")).into();
    let _c: Element = marker_content().text("Note").into();
    let el = ui! {
        <VStack gap="0.5rem" w="20rem">
            <Marker>
                <MarkerIcon>
                    <Icon name="git-branch" size="1rem" />
                </MarkerIcon>
                <MarkerContent>"Switched to a new branch"</MarkerContent>
            </Marker>
            <Marker variant="separator">
                <MarkerContent>"Today"</MarkerContent>
            </Marker>
            <Marker variant="border" role="status">
                <MarkerContent>"Thinking..."</MarkerContent>
            </Marker>
            { Element::from(ui_marker().variant("border").content("Builder")) }
        </VStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_message_dual_surface_compiles() {
    let _g: Element = message_group()
        .child(
            message()
                .align("end")
                .header("You")
                .content_child(Element::new().text("On it."))
                .footer("Delivered"),
        )
        .into();
    let _a: Element = message_avatar().into();
    let _c: Element = message_content()
        .header("Oliver")
        .child(Element::new().text("Hello"))
        .footer("Sent")
        .into();
    let el = ui! {
        <MessageGroup>
            <Message align="start">
                <MessageAvatar />
                <MessageContent>
                    <MessageHeader>"Oliver"</MessageHeader>
                    <MessageFooter>"Delivered"</MessageFooter>
                </MessageContent>
            </Message>
            <Message align="end">
                <MessageAvatar />
                <MessageContent>
                    <MessageHeader>"You"</MessageHeader>
                </MessageContent>
            </Message>
            { Element::from(message().align("end").header("Builder").footer("ok")) }
        </MessageGroup>
    };
    let _: Element = el;
}

#[test]
fn prelude_message_scroller_dual_surface_compiles() {
    let _p: Element = message_scroller_provider()
        .auto_scroll(true)
        .default_scroll_position("end")
        .child(
            message_scroller().h("12rem").child(
                message_scroller_viewport().child(
                    message_scroller_content().child(
                        message_scroller_item()
                            .message_id("1")
                            .child(message().align("start").header("Oliver").footer("ok")),
                    ),
                ),
            ),
        )
        .into();
    let el = ui! {
        <MessageScrollerProvider autoScroll={true} defaultScrollPosition="end">
            <MessageScroller h="12rem">
                <MessageScrollerViewport>
                    <MessageScrollerContent>
                        <MessageScrollerItem messageId="1" scrollAnchor={false}>
                            <Message align="start">
                                <MessageContent>
                                    <MessageHeader>"Oliver"</MessageHeader>
                                </MessageContent>
                            </Message>
                        </MessageScrollerItem>
                    </MessageScrollerContent>
                </MessageScrollerViewport>
                <MessageScrollerButton direction="end" />
            </MessageScroller>
        </MessageScrollerProvider>
    };
    let _: Element = el;
}

#[test]
fn ui_message_scroller_keeps_nested_reactive_placeholder() {
    use rui::app::reactive::reactive_view_sized;

    fn find_tid(el: &Element, id: &str) -> bool {
        el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
    }

    let rows = reactive_view_sized("ui-ms-rows-placeholder", "100%", "max-content", || {
        Element::new().text("row")
    });
    let el = ui! {
        <MessageScrollerProvider id="ui-ms-static">
            <MessageScroller h="12rem">
                <MessageScrollerViewport>
                    <MessageScrollerContent>
                        { rows }
                    </MessageScrollerContent>
                </MessageScrollerViewport>
            </MessageScroller>
        </MessageScrollerProvider>
    };
    assert!(
        find_tid(&el, "ui-ms-rows-placeholder"),
        "ui! must not drop a nested reactive_view placeholder inside MessageScrollerContent"
    );
}

#[test]
fn prelude_kbd_dual_surface_compiles() {
    let _b: Element = kbd().text("⌘").into();
    let _g: Element = kbd_group().kbd("Ctrl").kbd("K").into();
    let el = ui! {
        <HStack gap="0.5rem" items="center">
            <Kbd>"⌘"</Kbd>
            <KbdGroup>
                <Kbd>"Ctrl"</Kbd>
                <Kbd>"K"</Kbd>
            </KbdGroup>
            { Element::from(kbd().text("Esc")) }
        </HStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_aspect_ratio_dual_surface_compiles() {
    let _b: Element = aspect_ratio().ratio(16.0 / 9.0).into();
    let el = ui! {
        <HStack gap="0.5rem" w="20rem">
            <AspectRatio ratio="16/9" w="10rem">
                <Box w="100%" h="100%" bg="muted" />
            </AspectRatio>
            { Element::from(aspect_ratio().ratio("1/1").child(Element::new().w("100%").h("100%")))
                .w("6rem") }
            <Image texture="demo" w="6rem" aspect_ratio="4/3" />
            <Box w="6rem" aspect_ratio={1.5} bg="muted" />
        </HStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_ui_embeds_builder() {
    let el = ui! {
        <VStack gap="0.5rem">
            { Element::from(button("prelude-embed-b").variant("ghost").text("内嵌 Builder")) }
            <Button id="prelude-embed-t" variant="outline">"标签"</Button>
        </VStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_picker_tags_and_builders_compile() {
    let _c: Element = calendar("pre-cal")
        .caption_layout(CalendarCaption::Dropdown)
        .default_value("2026-08-14")
        .into();
    let _d: Element = date_picker("pre-dp")
        .kind(DatePickerKind::Date)
        .caption_layout(CalendarCaption::Label)
        .into();
    let _t: Element = time_picker("pre-tp").default_value("09:30").into();
    let _p: Element = color_picker("pre-cp")
        .variant(ColorPickerVariant::Inline)
        .format(ColorFormat::Hex)
        .into();
    let actions = vec![CommandAction::new("x", "Example", || {})];
    let open = use_signal("pre-cmd-open", || false);
    let date_sig = use_signal("pre-cal-sig", || "2026-08-14".to_string());
    let color_sig = use_signal("pre-cp-sig", || "#3b82f6".to_string());
    let el = ui! {
        <VStack gap="0.5rem">
            <Calendar id="pre-cal-tag" captionLayout="dropdown" defaultValue="2026-08-01" />
            <Calendar id="pre-cal-sig-tag" valueSignal={date_sig} captionLayout="label" />
            <DatePicker id="pre-dp-tag" kind="date" captionLayout="label" />
            <TimePicker id="pre-tp-tag" value="09:30" />
            <ColorPicker id="pre-cp-tag" variant="inline" showAlpha={false} />
            <ColorPicker id="pre-cp-sig-tag" valueSignal={color_sig} />
            <Command id="pre-cmd-tag" open={open} actions={actions} />
        </VStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_chart_tags_and_builders_compile() {
    let _b: Element = bar_chart().values([0.2, 0.8]).h("3rem").into();
    let _l: Element = line_chart().values([0.1, 0.9]).h(80.0).into();
    let _a: Element = area_chart().values([0.2, 0.5]).h(80.0).into();
    let _p: Element = pie_chart().values([1.0, 2.0]).inner_radius(0.55).into();
    let rows = [ChartDatum::new("A", 1.0), ChartDatum::new("B", 2.0)];
    let _c: Element = define_chart()
        .mark(line_y(rows.clone()).stroke("#2563eb"))
        .mark(dot(rows).r(3.0))
        .mark(rule_y([1.5]).stroke_dash([4.0, 2.0]))
        .theme(ChartTheme::tokens())
        .tooltip(true)
        .h(80.0)
        .into();
    let t0 = 1_700_000_000_000i64;
    let _t: Element = define_chart()
        .mark(line_y([
            ChartDatum::at(t0, 1.0),
            ChartDatum::at(t0 + 86_400_000, 2.0),
        ]))
        .x_time()
        .h(60.0)
        .into();
    let _r: Element = define_chart()
        .mark(radial_bar([("A", 2.0f32), ("B", 1.0)]))
        .mark(radial_line([("A", 2.0f32), ("B", 1.0)]))
        .size(80.0)
        .into();
    let pick: Option<usize> = None;
    let el = ui! {
        <VStack gap="0.5rem">
            <BarChart values={vec![0.2, 0.8]} h="3rem" selected={pick} />
            <LineChart values={vec![0.1, 0.9]} h={80.0} selected={pick} />
            <AreaChart values={vec![0.2, 0.5]} h={80.0} selected={pick} />
            <PieChart values={vec![1.0, 2.0]} inner_radius={0.55} selected={pick} />
            <Chart definition={define_chart().mark(bar_y([("X", 1.0f32), ("Y", 2.0)])).h(60.0)} />
        </VStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_builder_embeds_ui() {
    let el: Element = vstack()
        .gap("0.5rem")
        .child(ui! { <Label id="prelude-embed-l">"内嵌标签"</Label> })
        .child(Element::from(
            button("prelude-embed-b2")
                .variant("default")
                .text("外层 Builder"),
        ))
        .into();
    assert!(!el.children.is_empty());
}

#[test]
fn prelude_svg_tag_and_builder_compile() {
    let _b: Element = svg()
        .view_box("0 0 100 100")
        .w(80.0)
        .h(80.0)
        .child(circle().cx(50.0).cy(50.0).r(20.0).fill("primary"))
        .child(
            svg_path()
                .d("M10 80 L90 80")
                .stroke("foreground")
                .fill("none"),
        )
        .into();
    let el = ui! {
        <svg viewBox="0 0 100 100" w="5rem" h="5rem">
            <circle cx="50" cy="50" r="20" fill="primary" />
            <path d="M10 80 L90 80" stroke="foreground" fill="none" />
        </svg>
    };
    let _: Element = el;
}

#[test]
fn prelude_hover_card_dual_surface_compiles() {
    let open = use_signal("pre-hc-open", || false);
    let _b = hover_card("pre-hc-b").open_signal(open).child(|| {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        frag.children(vec![
            HoverCardTrigger(HoverCardTriggerProps {
                id: String::new(),
                open_delay: None,
                close_delay: None,
                children: Some(button("pre-hc-btn").variant("link").text("@rui").into()),
                base: None,
            }),
            hover_card_content(HoverCardContentOpts::default(), || {
                Element::from(label("pre-hc-body").text("Builder hover card"))
            }),
        ])
    });
    let el = ui! {
        <HoverCard id="pre-hc-tag" open={open}>
            <HoverCardTrigger openDelay=10 closeDelay=100>
                <Button variant="link" size="sm">"@nextjs"</Button>
            </HoverCardTrigger>
            <HoverCardContent side="bottom" align="center" sideOffset=4 class="w-64">
                <Label font_weight=600>"@nextjs"</Label>
                <Label text_size="0.875" color="muted-foreground">
                    "The React Framework – created and maintained by @vercel."
                </Label>
            </HoverCardContent>
        </HoverCard>
    };
    let _: Element = el;
}

#[test]
fn prelude_input_adornment_dual_surface_compiles() {
    let _b: Element = input_adornment()
        .addon(
            "inline-start",
            InputAdornmentText(InputAdornmentTextProps {
                class: None,
                children: Some(Element::new().text("https://")),
                base: None,
                ..Default::default()
            }),
        )
        .child(
            input("pre-ia-b")
                .placeholder("example.com")
                .plain_chrome(true),
        )
        .into();
    let el = ui! {
        <InputAdornment>
            <InputAdornmentAddon align="inline-start">
                <InputAdornmentText>"https://"</InputAdornmentText>
            </InputAdornmentAddon>
            <InputAdornmentInput id="pre-ia-t" placeholder="example.com" />
            <InputAdornmentAddon align="inline-end">
                <InputAdornmentButton size="xs" variant="ghost">"Go"</InputAdornmentButton>
            </InputAdornmentAddon>
        </InputAdornment>
    };
    let _: Element = el;
}

#[test]
fn prelude_input_otp_dual_surface_compiles() {
    let _b: Element = input_otp("pre-otp-b").max_length(6).separator_at(3).into();
    let el = ui! {
        <InputOTP id="pre-otp-t" maxLength={6}>
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
    };
    let _: Element = el;
}

#[test]
fn prelude_canvas_tag_and_builder_compile() {
    let _b: Element = canvas()
        .w(64.0)
        .h(32.0)
        .paint(|ctx, size| {
            ctx.set_fill("primary");
            ctx.fill_rect(0.0, 0.0, size.w, size.h);
            LayoutStatus::Clean
        })
        .into();
    let el = ui! {
        <canvas w={64.0} h={32.0} paint={move |ctx: &mut dyn Canvas2D, size: CanvasSize| {
            ctx.clear("secondary");
            ctx.set_stroke("foreground");
            ctx.stroke_rect(2.0, 2.0, size.w - 4.0, size.h - 4.0);
            LayoutStatus::Clean
        }} />
    };
    let _: Element = el;
}

#[test]
fn prelude_scene3d_tag_and_builder_compile() {
    let _b: Element = scene3d().w(80.0).h(60.0).into();
    let el = ui! {
        <scene3d w={80.0} h={60.0} />
    };
    let _: Element = el;
}
