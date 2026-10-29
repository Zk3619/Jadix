//! # Chart（experimental · WP-3.2 grammar）
//!
//! Compare the **grammar mind** of [TanStack Charts](https://github.com/TanStack/charts) without moving the runtime:
//! - **Data as rows**：[`ChartDatum`]（label + value + 可选 `series` / `time`）。
//! - **Scales**（Rust compact，对位 TanStack 边界、**不是** JS D3）：linear / band / point /
//!   [`scale_log`] / [`scale_time`]。
//! - **Marks**: Descartes [`bar_y`] [`line_y`] [`area_y`] [`dot`] [`rule_*`]; polar
//!   [`pie`] [`radial_bar`] [`radial_line`] [`radial_area`] [`radial_dot`]；[`custom_mark`]。
//! - **Theme / interaction**：[`ChartTheme`] / [`ChartConfig`] / [`TooltipIndicator`] / [`MarkStyle`]；`.tooltip(true)` / `.crosshair(true)`。
//! - **Composition**: [`define_chart`] stacks in the order of declaration; the default is thin packaging (including `donut_chart` / `radial_chart`).
//! - **Scene**: mark → [`crate::primitives::path::PathCommand`] → same path core.
//!
//! Layering (see `docs/drawing.md` §8 for details):
//!
//! ```text
//! 禁区     DOM · SVG 1.1 全家桶 · JS D3 运行时 · 换渲染器
//! Deepen the polar family · Timeline · Rust-specific scale (log/time) · Chart tooltip/crosshair
//! 别的包   公开 <svg>（WP-3.11）· Canvas paint（WP-3.12）· 通用运动原语（B17）
//! ```
//!
//! Double writing: Builder [`define_chart`] / Default fn; RSX [`Chart`] / [`BarChart`] / …
//!
//! **身份（WP-2.8）**：展示型不要求用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。

mod datum;
mod define;
mod mark;
mod polar;
mod preset;
mod scale;
mod theme;

pub use datum::{ChartDatum, ChartSelect, ChartSelected, ChartTooltipItem};
pub use define::{define_chart, ChartBuilder, ChartDefinition};
pub use mark::{
    area_y, bar_x, bar_y, custom_mark, dot, line_y, pie, rule_x, rule_x_num, rule_y, ChartMark,
    CustomMarkBuilder, MarkChannels, MarkRenderContext, SceneLayer,
};
pub use polar::{radial_area, radial_bar, radial_dot, radial_line};
pub use preset::{
    area_chart, bar_chart, donut_chart, line_chart, pie_chart, radial_chart, AreaChart,
    AreaChartProps, BarChart, BarChartBuilder, BarChartProps, Chart, ChartProps, DonutChart,
    DonutChartProps, LineChart, LineChartBuilder, LineChartProps, PieChart, PieChartBuilder,
    PieChartProps, RadialChart, RadialChartBuilder, RadialChartProps,
};
pub use scale::{
    format_time_label, pie_layout, pie_layout_span, scale_band, scale_linear, scale_log,
    scale_point, scale_time, unix_ms_to_utc, AxisSpec, ScaleKind,
};
pub use theme::{
    ChartConfig, ChartPaint, ChartSeriesConfig, ChartTheme, MarkStyle, TooltipIndicator,
};

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/mod.rs"]
mod tests;
