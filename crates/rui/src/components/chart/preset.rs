//! 预设图表：`bar_chart` / `line_chart` / `area_chart` / `pie_chart` 与 RSX 标签。
//! 内部都走 [`super::define_chart`]，只是固定 mark 组合。

use std::rc::Rc;

use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use rui_macros::component;

use super::datum::{rows_from_values_labels, ChartDatum, ChartSelect, ChartSelected};
use super::define::{define_chart, ChartDefinition};
use super::mark::{area_y, bar_y, line_y, pie};

fn parse_height_px(s: &str) -> f32 {
    let t = s.trim();
    if let Some(n) = t.strip_suffix("rem") {
        n.trim().parse::<f32>().unwrap_or(8.0) * 16.0
    } else if let Some(n) = t.strip_suffix("px") {
        n.trim().parse().unwrap_or(128.0)
    } else {
        t.parse().unwrap_or(128.0)
    }
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
}

impl From<BarChartBuilder> for Element {
    fn from(b: BarChartBuilder) -> Self {
        let h = parse_height_px(&b.height);
        let mut c = define_chart()
            .id(b.id)
            .mark(bar_y(b.rows))
            .x_band()
            .x_padding(0.2)
            .y_linear()
            .guides(b.guides)
            .axis_labels(true)
            .h(h)
            .tooltip(true)
            .selected(b.selected);
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
// Pie
// ---------------------------------------------------------------------------

pub struct PieChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    size: f32,
    selected: Option<usize>,
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    inner_radius: f32,
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
    }
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
// RSX
// ---------------------------------------------------------------------------

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
    b.into()
}
