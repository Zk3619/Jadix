//! # Chart
//!
//! 图表 MVP：[`bar_chart`] 用堆叠矩形条；[`line_chart`] 挂折线 path attrs（GPU path 见
//! [`crate::primitives::path`] stub）。

use crate::core::tree::Element;
use crate::primitives::path::{attach_polyline, Point2};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

pub struct BarChartBuilder {
    id: String,
    values: Vec<f32>,
    labels: Vec<String>,
    height: String,
}

pub fn bar_chart(id: impl Into<String>) -> BarChartBuilder {
    BarChartBuilder {
        id: id.into(),
        values: Vec::new(),
        labels: Vec::new(),
        height: "8rem".into(),
    }
}

impl BarChartBuilder {
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        self.values = v.into_iter().collect();
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.labels = labels.into_iter().map(|s| s.into()).collect();
        self
    }
    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.height = h.into();
        self
    }
}

impl From<BarChartBuilder> for Element {
    fn from(builder: BarChartBuilder) -> Self {
        let BarChartBuilder {
            id,
            values,
            labels,
            height,
        } = builder;
        let max_v = values.iter().cloned().fold(1.0f32, f32::max).max(1e-6);
        let mut row: Element = hstack()
            .w("100%")
            .h(height.as_str())
            .gap("0.5rem")
            .items("end")
            .into();
        for (i, v) in values.iter().enumerate() {
            let ratio = (*v / max_v).clamp(0.0, 1.0);
            let bar_h = format!("{}%", (ratio * 100.0).max(2.0));
            let mut bar: Element = r#box().w("100%").h(bar_h.as_str()).rounded("0.25rem").into();
            bar = bar.bg(tokens::primary());
            let label = labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("{}", i + 1));
            let caption = build_text_node(
                &label,
                TextNodeConfig {
                    size_px: 11.0,
                    weight: 400,
                    ..Default::default()
                },
            );
            let col: Element = vstack()
                .gap("0.25rem")
                .flex_grow(1.0)
                .items_center()
                .child(bar)
                .child(caption)
                .into();
            row = row.child(col);
        }
        row.template_id = Some(id);
        row
    }
}

pub struct LineChartBuilder {
    id: String,
    values: Vec<f32>,
    width: f32,
    height: f32,
}

pub fn line_chart(id: impl Into<String>) -> LineChartBuilder {
    LineChartBuilder {
        id: id.into(),
        values: Vec::new(),
        width: 240.0,
        height: 120.0,
    }
}

impl LineChartBuilder {
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        self.values = v.into_iter().collect();
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
}

impl From<LineChartBuilder> for Element {
    fn from(builder: LineChartBuilder) -> Self {
        let LineChartBuilder {
            id,
            values,
            width,
            height,
        } = builder;
        let max_v = values.iter().cloned().fold(1.0f32, f32::max).max(1e-6);
        let n = values.len().max(1);
        let mut pts: Vec<Point2> = Vec::with_capacity(n);
        for (i, v) in values.iter().enumerate() {
            let x = if n == 1 {
                0.0
            } else {
                (i as f32 / (n - 1) as f32) * width
            };
            let y = height - (*v / max_v) * height;
            pts.push((x, y));
        }
        // Visual MVP: also draw thin bars so demos show something without GPU path.
        let mut bars = hstack().w(format!("{width}px")).h(format!("{height}px")).gap("2px").items("end");
        for v in &values {
            let ratio = (*v / max_v).clamp(0.0, 1.0);
            let mut b: Element = r#box()
                .w("100%")
                .h(format!("{}%", (ratio * 100.0).max(2.0)))
                .into();
            b = b.bg(tokens::primary());
            bars = bars.child(b);
        }
        let mut el: Element = bars.into();
        el = attach_polyline(el, &pts);
        el.set_attr("rui_chart", "line");
        el.template_id = Some(id);
        el
    }
}
