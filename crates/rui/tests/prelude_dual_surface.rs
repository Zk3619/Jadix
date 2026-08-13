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
fn prelude_tag_surface_compiles() {
    let el = ui! {
        <Button id="prelude-tag-btn" variant="outline">"Tag"</Button>
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
        .show_alpha(true)
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
