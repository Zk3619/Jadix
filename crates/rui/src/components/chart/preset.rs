//! Default charts: `bar_chart` / `line_chart` / `area_chart` / `pie_chart` / `donut_chart` /
//! `radial_chart` 与 RSX 标签。内部都走 [`super::define_chart`]，只是固定 mark 组合。

use std::rc::Rc;

use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use rui_macros::component;

use super::datum::{ChartDatum, ChartSelect, ChartSelected, rows_from_values_labels};
use super::define::{ChartDefinition, define_chart};
use super::mark::{area_y, bar_y, line_y, pie};
use super::polar::radial_bar;
use super::theme::{ChartConfig, ChartTheme, TooltipIndicator};

fn parse_height_px(s: &str) -> (f32, bool) {
    let t = s.trim();
    if t.eq_ignore_ascii_case("full") || t == "100%" {
        return (160.0, true);
    }
    let px = if let Some(n) = t.strip_suffix("rem") {
        n.trim().parse::<f32>().unwrap_or(8.0) * 16.0
    } else if let Some(n) = t.strip_suffix("px") {
        n.trim().parse().unwrap_or(128.0)
    } else {
        t.parse().unwrap_or(128.0)
    };
    (px, false)
}

fn chart_from_rows(
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
) -> Vec<ChartDatum> {
    if let Some(rows) = data {
        return rows;
    }
    rows_from_values_labels(&values.unwrap_or_default(), &labels.unwrap_or_default())
}

fn apply_chart_chrome(
    mut c: super::define::ChartBuilder,
    theme: Option<ChartTheme>,
    config: Option<ChartConfig>,
    tooltip_indicator: Option<TooltipIndicator>,
) -> super::define::ChartBuilder {
    if let Some(t) = theme {
        c = c.theme(t);
    }
    if let Some(cfg) = config {
        c = c.config(cfg);
    }
    if let Some(ind) = tooltip_indicator {
        c = c.tooltip_indicator(ind);
    }
    c
}

// ---------------------------------------------------------------------------
// Bar
// ---------------------------------------------------------------------------

pub struct BarChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    height: String,
    selected: Option<usize>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    guides: bool,
    theme: Option<ChartTheme>,
    config: Option<ChartConfig>,
    tooltip_indicator: Option<TooltipIndicator>,
    corner_radius: Option<f32>,
    corner_radius_base: Option<f32>,
}

pub fn bar_chart() -> BarChartBuilder {
    BarChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        height: "8rem".into(),
        selected: None,
        selected_signal: None,
        on_select: None,
        guides: true,
        theme: None,
        config: None,
        tooltip_indicator: None,
        corner_radius: None,
        corner_radius_base: None,
    }
}

impl BarChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(|s| s.into()).collect();
        let values: Vec<f32> = self.rows.iter().map(|r| r.value).collect();
        self.rows = rows_from_values_labels(&values, &labels);
        self
    }
    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.height = h.into();
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected_signal = Some(s);
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }
    pub fn theme(mut self, theme: ChartTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = Some(config);
        self
    }
    pub fn tooltip_indicator(mut self, indicator: impl Into<TooltipIndicator>) -> Self {
        self.tooltip_indicator = Some(indicator.into());
        self
    }
    /// 柱值端圆角（默认 4）。
    pub fn corner_radius(mut self, px: f32) -> Self {
        self.corner_radius = Some(px.max(0.0));
        self
    }
    /// Fillet the base end of the column. When there is no bottom axis, if it is not set, it will follow the value end.
    pub fn corner_radius_base(mut self, px: f32) -> Self {
        self.corner_radius_base = Some(px.max(0.0));
        self
    }
}

impl From<BarChartBuilder> for Element {
    fn from(b: BarChartBuilder) -> Self {
        let (h, fluid) = parse_height_px(&b.height);
        let mut mark = bar_y(b.rows);
        if let Some(r) = b.corner_radius {
            mark = mark.corner_radius(r);
        }
        if let Some(r) = b.corner_radius_base {
            mark = mark.corner_radius_base(r);
        }
        let mut c = define_chart()
            .id(b.id)
            .mark(mark)
            .x_band()
            .x_padding(0.2)
            .y_linear()
            .guides(b.guides)
            .axis_labels(true)
            .h(h)
            .tooltip(true)
            .selected(b.selected);
        if fluid {
            c = c.h_fluid();
        }
        c = apply_chart_chrome(c, b.theme, b.config, b.tooltip_indicator);
        if let Some(s) = b.selected_signal {
            c = c.selected_signal(s);
        }
        if let Some(cb) = b.on_select {
            c = c.on_select(move |i| cb(i));
        }
        c.into()
    }
}

// ---------------------------------------------------------------------------
// Line / Area
// ---------------------------------------------------------------------------

pub struct LineChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    width: Option<f32>,
    height: f32,
    selected: Option<usize>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    guides: bool,
    fill: bool,
    stroke_width: f32,
    area: bool,
    theme: Option<ChartTheme>,
    config: Option<ChartConfig>,
    tooltip_indicator: Option<TooltipIndicator>,
}

pub fn line_chart() -> LineChartBuilder {
    LineChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        width: None,
        height: 140.0,
        selected: None,
        selected_signal: None,
        on_select: None,
        guides: true,
        fill: false,
        stroke_width: 3.0,
        area: false,
        theme: None,
        config: None,
        tooltip_indicator: None,
    }
}

pub fn area_chart() -> LineChartBuilder {
    LineChartBuilder {
        area: true,
        fill: true,
        stroke_width: 2.0,
        ..line_chart()
    }
}

impl LineChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        self.rows = v.into_iter().map(ChartDatum::from).collect();
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = h;
        self
    }
    pub fn h(mut self, px: f32) -> Self {
        self.width = None;
        self.height = px;
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected_signal = Some(s);
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }
    pub fn fill(mut self, on: bool) -> Self {
        self.fill = on;
        self
    }
    pub fn stroke_width(mut self, px: f32) -> Self {
        self.stroke_width = px.max(0.5);
        self
    }
    pub fn theme(mut self, theme: ChartTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = Some(config);
        self
    }
    pub fn tooltip_indicator(mut self, indicator: impl Into<TooltipIndicator>) -> Self {
        self.tooltip_indicator = Some(indicator.into());
        self
    }
}

impl From<LineChartBuilder> for Element {
    fn from(b: LineChartBuilder) -> Self {
        let mark = if b.area {
            area_y(b.rows).stroke_width(b.stroke_width)
        } else {
            let mut m = line_y(b.rows).stroke_width(b.stroke_width);
            if b.fill {
                m = m.fill_under(true).fill_opacity(0.28);
            }
            m
        };
        let mut c = define_chart()
            .id(b.id)
            .mark(mark)
            .x_linear()
            .y_linear()
            .guides(b.guides)
            .h(b.height)
            .tooltip(true)
            .selected(b.selected);
        c = apply_chart_chrome(c, b.theme, b.config, b.tooltip_indicator);
        if let Some(w) = b.width {
            c = c.width_px(w);
        }
        if let Some(s) = b.selected_signal {
            c = c.selected_signal(s);
        }
        if let Some(cb) = b.on_select {
            c = c.on_select(move |i| cb(i));
        }
        c.into()
    }
}

// ---------------------------------------------------------------------------
// Pie / Donut
// ---------------------------------------------------------------------------

pub struct PieChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    size: f32,
    selected: Option<usize>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    inner_radius: f32,
    theme: Option<ChartTheme>,
    config: Option<ChartConfig>,
    tooltip_indicator: Option<TooltipIndicator>,
}

pub fn pie_chart() -> PieChartBuilder {
    PieChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        size: 180.0,
        selected: None,
        selected_signal: None,
        on_select: None,
        inner_radius: 0.0,
        theme: None,
        config: None,
        tooltip_indicator: None,
    }
}

/// 环形图：`pie_chart` + 默认 `inner_radius(0.55)`。
pub fn donut_chart() -> PieChartBuilder {
    pie_chart().inner_radius(0.55)
}

impl PieChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(|s| s.into()).collect();
        let values: Vec<f32> = self.rows.iter().map(|r| r.value).collect();
        self.rows = rows_from_values_labels(&values, &labels);
        self
    }
    pub fn size(mut self, px: f32) -> Self {
        self.size = px;
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected_signal = Some(s);
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
    pub fn theme(mut self, theme: ChartTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = Some(config);
        self
    }
    pub fn tooltip_indicator(mut self, indicator: impl Into<TooltipIndicator>) -> Self {
        self.tooltip_indicator = Some(indicator.into());
        self
    }
}

impl From<PieChartBuilder> for Element {
    fn from(b: PieChartBuilder) -> Self {
        let mut c = define_chart()
            .id(b.id)
            .mark(
                pie(b.rows)
                    .inner_radius(b.inner_radius)
                    .id(format!("pie:{}", b.inner_radius)),
            )
            .size(b.size)
            .legend(true)
            .tooltip(true)
            .selected(b.selected);
        c = apply_chart_chrome(c, b.theme, b.config, b.tooltip_indicator);
        if let Some(s) = b.selected_signal {
            c = c.selected_signal(s);
        }
        if let Some(cb) = b.on_select {
            c = c.on_select(move |i| cb(i));
        }
        c.into()
    }
}

// ---------------------------------------------------------------------------
// Radial
// ---------------------------------------------------------------------------

pub struct RadialChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    size: f32,
    selected: Option<usize>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    inner_radius: f32,
    guides: bool,
    legend: bool,
    theme: Option<ChartTheme>,
    config: Option<ChartConfig>,
    tooltip_indicator: Option<TooltipIndicator>,
}

pub fn radial_chart() -> RadialChartBuilder {
    RadialChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        size: 180.0,
        selected: None,
        selected_signal: None,
        on_select: None,
        inner_radius: 0.2,
        guides: true,
        legend: true,
        theme: None,
        config: None,
        tooltip_indicator: None,
    }
}

impl RadialChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(|s| s.into()).collect();
        let values: Vec<f32> = self.rows.iter().map(|r| r.value).collect();
        self.rows = rows_from_values_labels(&values, &labels);
        self
    }
    pub fn size(mut self, px: f32) -> Self {
        self.size = px;
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected_signal = Some(s);
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }
    pub fn legend(mut self, on: bool) -> Self {
        self.legend = on;
        self
    }
    pub fn theme(mut self, theme: ChartTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = Some(config);
        self
    }
    pub fn tooltip_indicator(mut self, indicator: impl Into<TooltipIndicator>) -> Self {
        self.tooltip_indicator = Some(indicator.into());
        self
    }
}

impl From<RadialChartBuilder> for Element {
    fn from(b: RadialChartBuilder) -> Self {
        let mut c = define_chart()
            .id(b.id)
            .mark(radial_bar(b.rows).inner_radius(b.inner_radius))
            .size(b.size)
            .guides(b.guides)
            .legend(b.legend)
            .tooltip(true)
            .selected(b.selected);
        c = apply_chart_chrome(c, b.theme, b.config, b.tooltip_indicator);
        if let Some(s) = b.selected_signal {
            c = c.selected_signal(s);
        }
        if let Some(cb) = b.on_select {
            c = c.on_select(move |i| cb(i));
        }
        c.into()
    }
}

// ---------------------------------------------------------------------------
// RSX helpers
// ---------------------------------------------------------------------------

fn parse_optional_indicator(s: Option<String>) -> Option<TooltipIndicator> {
    s.map(|v| TooltipIndicator::from(v.as_str()))
}

/// RSX `<BarChart data={…} h="6rem" />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BarChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    h: Option<String>,
    guides: Option<bool>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    corner_radius: Option<f32>,
    corner_radius_base: Option<f32>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = bar_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(r) = corner_radius {
        b = b.corner_radius(r);
    }
    if let Some(r) = corner_radius_base {
        b = b.corner_radius_base(r);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<LineChart values={…} h={140.0} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn LineChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    h: Option<f32>,
    guides: Option<bool>,
    fill: Option<bool>,
    stroke_width: Option<f32>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = line_chart()
        .id(id)
        .data(chart_from_rows(data, values, None));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(f) = fill {
        b = b.fill(f);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<AreaChart values={…} h={140.0} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AreaChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    h: Option<f32>,
    guides: Option<bool>,
    stroke_width: Option<f32>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = area_chart()
        .id(id)
        .data(chart_from_rows(data, values, None));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<PieChart data={…} inner_radius={0.55} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PieChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    size: Option<f32>,
    inner_radius: Option<f32>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = pie_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(s) = size {
        b = b.size(s);
    }
    if let Some(r) = inner_radius {
        b = b.inner_radius(r);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<DonutChart data={…} />` — 默认 `inner_radius=0.55`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DonutChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    size: Option<f32>,
    inner_radius: Option<f32>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = donut_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(s) = size {
        b = b.size(s);
    }
    if let Some(r) = inner_radius {
        b = b.inner_radius(r);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<RadialChart data={…} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn RadialChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    size: Option<f32>,
    inner_radius: Option<f32>,
    guides: Option<bool>,
    legend: Option<bool>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = radial_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(s) = size {
        b = b.size(s);
    }
    if let Some(r) = inner_radius {
        b = b.inner_radius(r);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(l) = legend {
        b = b.legend(l);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}

/// RSX `<Chart definition={define_chart().mark(line_y(rows))} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Chart(
    id: String,
    definition: Option<ChartDefinition>,
    h: Option<f32>,
    guides: Option<bool>,
    selected: Option<ChartSelected>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<ChartSelect>,
    theme: Option<ChartTheme>,
    tooltip_indicator: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = definition.map(|d| d.0).unwrap_or_else(define_chart).id(id);
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(s) = selected_signal {
        b = b.selected_signal(s);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    if let Some(t) = theme {
        b = b.theme(t);
    }
    if let Some(ind) = parse_optional_indicator(tooltip_indicator) {
        b = b.tooltip_indicator(ind);
    }
    b.into()
}
