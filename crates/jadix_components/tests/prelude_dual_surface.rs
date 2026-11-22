//! C-WP-0.3：`jadix_components::prelude` 同时导出标签与 Builder，且可互嵌编译。

use jadix_components::prelude::*;

#[test]
fn tree_physical_reveal_rsx_and_builders_share_presence_configuration() {
    use jadix_components::core::reactivity::{create_root, use_signal};
    use jadix_components::style::SpringPhysics;
    use std::collections::HashSet;

    let (_owner, ()) = create_root(|| {
        let spring = SpringPhysics::default_css();
        let expanded = use_signal("spring-trees-expanded", || HashSet::from(["parent".into()]));
        let _roots: Element = tree_view("spring-tree-roots")
            .variant("collapsible")
            .reveal_spring(spring)
            .expanded_signal(expanded)
            .expanded_storage_key("spring-trees-expanded")
            .roots([
                tree_node("parent", "Parent").children([tree_node("nested", "Nested").children([
                    tree_node("inner", "Inner").children([tree_node("leaf", "Leaf")]),
                ])]),
            ])
            .into();
        let _child = tree_view("spring-tree-child")
            .variant("collapsible")
            .reveal_spring(spring)
            .expanded_signal(expanded)
            .expanded_storage_key("spring-trees-expanded")
            .child(|| {
                ui! {
                    <TreeViewItem id="parent" label="Parent">
                        <TreeViewItem id="nested" label="Nested">
                            <TreeViewItem id="inner" label="Inner">
                                <TreeViewItem id="leaf" label="Leaf" />
                            </TreeViewItem>
                        </TreeViewItem>
                    </TreeViewItem>
                }
            });
        let _rsx = ui! {
            <TreeView id="spring-tree-rsx" variant="collapsible" revealSpring={spring}
                expandedSignal={expanded} expandedStorageKey="spring-trees-expanded">
                <TreeViewItem id="parent" label="Parent">
                    <TreeViewItem id="nested" label="Nested">
                        <TreeViewItem id="inner" label="Inner">
                            <TreeViewItem id="leaf" label="Leaf" />
                        </TreeViewItem>
                    </TreeViewItem>
                </TreeViewItem>
            </TreeView>
        };
        for tree in ["spring-tree-roots", "spring-tree-child", "spring-tree-rsx"] {
            assert!(
                jadix_components::utils::use_collapse_panel_presence(&format!(
                    "{tree}::parent:presence"
                ))
                .has_reveal_spring(),
                "missing spring for {tree}"
            );
        }
    });
}

#[test]
fn physical_reveal_rsx_and_builders_share_presence_configuration() {
    use jadix_components::core::reactivity::create_root;
    use jadix_components::style::SpringPhysics;
    let (_owner, ()) = create_root(|| {
        let spring = SpringPhysics::default_css();
        let _collapsible: Element = collapsible("spring-builder-col")
            .content_with(move |content| content.reveal_spring(spring).text("Body"))
            .into();
        let _accordion: Element = accordion("spring-builder-acc")
            .item_with("one", move |item| {
                item.content_with(|content| content.reveal_spring(spring).text("Body"))
            })
            .into();
        let _rsx = ui! {
            <VStack>
                <Collapsible id="spring-rsx-col">
                    <CollapsibleContent reveal_spring={spring}>"Body"</CollapsibleContent>
                </Collapsible>
                <Accordion id="spring-rsx-acc">
                    <AccordionItem value="one">
                        <AccordionContent reveal_spring={spring}>"Body"</AccordionContent>
                    </AccordionItem>
                </Accordion>
            </VStack>
        };
        for key in [
            "spring-builder-col:presence",
            "spring-rsx-col:presence",
            "spring-builder-acc:presence:one",
            "spring-rsx-acc:presence:one",
        ] {
            assert!(jadix_components::utils::use_collapse_panel_presence(key).has_reveal_spring());
        }
    });
}

#[test]
fn slider_accessibility_value_clamps_steps_and_syncs_without_remount() {
    use jadix_components::core::reactivity::{create_root, create_signal};
    use jadix_components::layout::LayoutTree;
    let (_owner, ()) = create_root(|| {
        let value = create_signal(10.0_f32);
        let mut control: Element = slider("numeric-slider")
            .min(0.0)
            .max(100.0)
            .step(5.0)
            .value_signal(value)
            .into();
        let callback = control
            .on_accessibility_value
            .clone()
            .expect("numeric handler");
        let sync = control.reactive_sync.clone().unwrap();
        let mut layout = LayoutTree::new();
        for (input, expected) in [(73.0, 75.0), (-1.0, 0.0), (f64::MAX, 100.0), (41.0, 40.0)] {
            assert!(callback.borrow_mut()(input));
            assert_eq!(value.peek(), expected);
            sync.run(&mut control, &mut layout);
            assert_eq!(
                control.attrs.get("aria-valuenow").unwrap(),
                &expected.to_string()
            );
        }
        assert_eq!(
            control.attrs.get("aria-valuemin").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            control.attrs.get("aria-valuemax").map(String::as_str),
            Some("100")
        );
        let disabled: Element = slider("disabled-slider").disabled(true).into();
        assert!(disabled.on_accessibility_value.is_none());
        assert_eq!(
            disabled.attrs.get("disabled").map(String::as_str),
            Some("true")
        );
    });
}

#[test]
fn prelude_live_text_constructor_dual_surface() {
    use jadix_components::core::reactivity::{create_effect, create_root};
    use jadix_components::core::tree::IntoNodeText;

    fn caption(value: impl IntoNodeText) -> Element {
        text(value).into()
    }

    let (_owner, ()) = create_root(|| {
        let count = create_signal(7);
        create_effect(move || {
            let builder: Element = vstack()
                .child(text(count))
                .child(TextBuilder::new(move || "borrowed caption"))
                .child(caption(move || format!("Count: {}", count.get())))
                .into();
            assert_eq!(builder.children[0].text.as_deref(), Some("7"));
            assert_eq!(
                builder.children[1].text.as_deref(),
                Some("borrowed caption")
            );
            assert_eq!(builder.children[2].text.as_deref(), Some("Count: 7"));
            let _: Element = ui! {
                <VStack>
                    { Element::from(text(count)) }
                    <Text>{count.get().to_string()}</Text>
                </VStack>
            };
        });
    });
}

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
    use jadix_components::app::reactive::reactive_view_sized;

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
            <BarChart values={vec![0.2, 0.8]} plot_h="3rem" selected={pick} />
            <LineChart values={vec![0.1, 0.9]} plot_h={80.0} selected={pick} />
            <AreaChart values={vec![0.2, 0.5]} plot_h={80.0} selected={pick} />
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

#[cfg(feature = "svg")]
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
        frag.set_attr("jadix_fragment", "1");
        frag.children(vec![
            HoverCardTrigger(HoverCardTriggerProps {
                id: String::new(),
                open_delay: None,
                close_delay: None,
                children: Some(button("pre-hc-btn").variant("link").text("@jadix").into()),
                base: None,
            }),
            hover_card_content(HoverCardContentOpts::default(), || {
                Element::from(label("pre-hc-body").text("Builder hover card"))
            }),
        ])
    });
    let el = ui! {
        <HoverCard id="pre-hc-tag" open={open} openDelay=65 closeDelay=120>
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
fn hover_card_uses_shared_hover_delay_and_builder_can_override_it() {
    use jadix_components::core::reactivity::create_root;

    let (_owner, (default_delay, trigger_delay, configured_delay, scoped_delay)) =
        create_root(|| {
            let default_value = create_signal(0_u64);
            let _ = hover_card_scope("pre-hc-default-delay", None, || {
                default_value.set(
                    use_hover_card()
                        .expect("hover card scope")
                        .open_delay_ms
                        .get(),
                );
                Element::new()
            });
            let trigger_value = create_signal(0_u64);
            let _ = hover_card_scope("pre-hc-trigger-delay", None, || {
                let trigger = HoverCardTrigger(HoverCardTriggerProps {
                    id: String::new(),
                    open_delay: Some(45),
                    close_delay: None,
                    children: Some(Element::new().text("Hover")),
                    base: None,
                });
                let _ = trigger;
                trigger_value.set(
                    use_hover_card()
                        .expect("hover card scope")
                        .open_delay_ms
                        .get(),
                );
                Element::new()
            });
            let configured_value = create_signal(0_u64);
            let configured = hover_card("pre-hc-custom-delay")
                .open_delay(85)
                .close_delay(125)
                .child(|| {
                    let ctx = use_hover_card().expect("hover card scope");
                    configured_value.set(ctx.open_delay_ms.get());
                    assert_eq!(ctx.close_delay_ms.get(), 125);
                    Element::new()
                });
            let _ = configured;
            let scoped_value = create_signal(0_u64);
            let _ = hover_card_scope_with_delays(
                "pre-hc-root-delay",
                None,
                Some(65),
                Some(120),
                || {
                    let ctx = use_hover_card().expect("hover card scope");
                    scoped_value.set(ctx.open_delay_ms.get());
                    Element::new()
                },
            );
            (
                default_value.get(),
                trigger_value.get(),
                configured_value.get(),
                scoped_value.get(),
            )
        });

    assert_eq!(DEFAULT_HOVER_OPEN_DELAY_MS, 300);
    assert_eq!(default_delay, DEFAULT_HOVER_OPEN_DELAY_MS);
    assert_eq!(trigger_delay, 45);
    assert_eq!(configured_delay, 85);
    assert_eq!(scoped_delay, 65);
}

#[test]
fn tooltip_uses_shared_hover_delay_and_builder_can_override_it() {
    use jadix_components::core::reactivity::create_root;

    let _rsx = ui! {
        <Tooltip id="pre-tip-rsx-delay" openDelay=65 closeDelay=120>
            <TooltipTrigger openDelay=45>
                <Button>"Hover"</Button>
            </TooltipTrigger>
            <TooltipContent>"Tip"</TooltipContent>
        </Tooltip>
    };

    let (_owner, (default_delay, trigger_delay, configured_delay, scoped_delay)) =
        create_root(|| {
            let default_value = create_signal(0_u64);
            let _ = tooltip_scope("pre-tip-default-delay", None, || {
                default_value.set(use_tooltip().expect("tooltip scope").open_delay_ms.get());
                Element::new()
            });
            let trigger_value = create_signal(0_u64);
            let _ = tooltip_scope("pre-tip-trigger-delay", None, || {
                let trigger = TooltipTrigger(TooltipTriggerProps {
                    id: String::new(),
                    open_delay: Some(45),
                    close_delay: None,
                    children: Some(Element::new().text("Hover")),
                    base: None,
                });
                let _ = trigger;
                trigger_value.set(use_tooltip().expect("tooltip scope").open_delay_ms.get());
                Element::new()
            });
            let configured_value = create_signal(0_u64);
            let configured = tooltip("pre-tip-custom-delay", "Tip")
                .open_delay(85)
                .close_delay(125)
                .trigger(move |_| {
                    let ctx = use_tooltip().expect("tooltip scope");
                    Element::new().on_hover(move |_| {
                        configured_value.set(ctx.open_delay_ms.get());
                        assert_eq!(ctx.close_delay_ms.get(), 125);
                    })
                });
            configured.children[0]
                .on_hover
                .as_ref()
                .expect("tooltip trigger hover handler")
                .borrow_mut()(true);
            (
                default_value.get(),
                trigger_value.get(),
                configured_value.get(),
                {
                    let scoped_value = create_signal(0_u64);
                    let _ = tooltip_scope_with_delays(
                        "pre-tip-root-delay",
                        None,
                        Some(65),
                        Some(120),
                        || {
                            scoped_value
                                .set(use_tooltip().expect("tooltip scope").open_delay_ms.get());
                            Element::new()
                        },
                    );
                    scoped_value.get()
                },
            )
        });

    assert_eq!(default_delay, 300);
    assert_eq!(trigger_delay, 45);
    assert_eq!(configured_delay, 85);
    assert_eq!(scoped_delay, 65);
}

#[test]
fn prelude_input_adornment_dual_surface_compiles() {
    let _b: Element = input_adornment()
        .addon(
            "inline-start",
            InputAdornmentText(InputAdornmentTextProps {
                children: Some(Element::new().text("https://")),
                base: None,
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

#[test]
fn prelude_shader_tag_and_builder_compile() {
    const WGSL: &str = r#"
        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(in.uv, 0.3, 1.0);
        }
    "#;
    let _b: Element = shader()
        .w(80.0)
        .h(60.0)
        .wgsl(WGSL)
        .texture("src", "jadix-gallery-stream")
        .into();
    let el = ui! {
        <shader w={80.0} h={60.0} wgsl={WGSL} texture={"jadix-gallery-stream"} />
    };
    assert_eq!(
        el.get_attr(jadix_components::primitives::shader::ATTR_SHADER_TEX)
            .map(|s| s.as_str()),
        Some("jadix-gallery-stream")
    );
    let _: Element = el;
}
