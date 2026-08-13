//! 图表主题与 mark 绘制风格（对照 TanStack `ChartTheme` + constant paint channels）。

use crate::style::parse_color;
use crate::tokens;

/// 色板 / 常量色 / 按系列取色。
#[derive(Clone, Debug, PartialEq)]
pub enum ChartPaint {
    /// 直接 RGBA（0..=1）。
    Rgba([f32; 4]),
    /// [`ChartTheme::palette`] 下标。
    Palette(usize),
    /// 按 series / datum 下标循环色板（TanStack color scale）。
    Series,
}

impl From<[f32; 4]> for ChartPaint {
    fn from(c: [f32; 4]) -> Self {
        Self::Rgba(c)
    }
}

impl From<&str> for ChartPaint {
    fn from(token_or_css: &str) -> Self {
        Self::Rgba(parse_color(token_or_css))
    }
}

impl From<String> for ChartPaint {
    fn from(s: String) -> Self {
        Self::Rgba(parse_color(&s))
    }
}

/// Mark 常量外观（非语义 channel）。缺省由 mark 种类 + theme 决定。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkStyle {
    pub fill: Option<ChartPaint>,
    pub stroke: Option<ChartPaint>,
    pub stroke_width: Option<f32>,
    /// 与 muted/background **混色**（path 核扫描线不吃真 alpha，见 WP-3.2）。
    pub fill_opacity: Option<f32>,
    pub stroke_opacity: Option<f32>,
    /// 虚线：[on, off, on, off, …] 逻辑 px；空 = 实线。
    pub stroke_dash: Option<Vec<f32>>,
}

impl MarkStyle {
    pub fn fill(mut self, paint: impl Into<ChartPaint>) -> Self {
        self.fill = Some(paint.into());
        self
    }
    pub fn stroke(mut self, paint: impl Into<ChartPaint>) -> Self {
        self.stroke = Some(paint.into());
        self
    }
    pub fn stroke_width(mut self, px: f32) -> Self {
        self.stroke_width = Some(px.max(0.5));
        self
    }
    pub fn fill_opacity(mut self, o: f32) -> Self {
        self.fill_opacity = Some(o.clamp(0.0, 1.0));
        self
    }
    pub fn stroke_opacity(mut self, o: f32) -> Self {
        self.stroke_opacity = Some(o.clamp(0.0, 1.0));
        self
    }
    pub fn stroke_dash(mut self, dash: impl IntoIterator<Item = f32>) -> Self {
        let d: Vec<f32> = dash.into_iter().filter(|v| *v > 0.0).collect();
        self.stroke_dash = if d.is_empty() { None } else { Some(d) };
        self
    }
}

/// 对照 TanStack `theme`：foreground / muted / grid / background / palette。
#[derive(Clone, Debug, PartialEq)]
pub struct ChartTheme {
    pub background: [f32; 4],
    pub foreground: [f32; 4],
    pub muted: [f32; 4],
    pub grid: [f32; 4],
    pub border: [f32; 4],
    pub palette: Vec<[f32; 4]>,
}

impl ChartTheme {
    /// shadcn token 默认（`chart-1..5` + chrome）。
    pub fn tokens() -> Self {
        Self {
            background: tokens::muted(),
            foreground: parse_color("foreground"),
            muted: tokens::muted(),
            grid: lerp_rgb(tokens::muted(), tokens::border(), 0.5),
            border: tokens::border(),
            palette: (1u8..=5).map(tokens::chart).collect(),
        }
    }

    pub fn palette(mut self, colors: impl IntoIterator<Item = [f32; 4]>) -> Self {
        let p: Vec<[f32; 4]> = colors.into_iter().collect();
        if !p.is_empty() {
            self.palette = p;
        }
        self
    }

    pub fn background(mut self, c: [f32; 4]) -> Self {
        self.background = c;
        self
    }

    pub fn grid(mut self, c: [f32; 4]) -> Self {
        self.grid = c;
        self
    }

    pub fn color(&self, i: usize) -> [f32; 4] {
        if self.palette.is_empty() {
            return tokens::chart(1);
        }
        self.palette[i % self.palette.len()]
    }

    pub fn resolve(&self, paint: &ChartPaint, series_or_index: usize) -> [f32; 4] {
        match paint {
            ChartPaint::Rgba(c) => *c,
            ChartPaint::Palette(i) => self.color(*i),
            ChartPaint::Series => self.color(series_or_index),
        }
    }

    /// 不透明混色：`opacity=1` 原色，`0` 为 `mix_with`。
    pub fn mix_opacity(&self, color: [f32; 4], opacity: f32, mix_with: [f32; 4]) -> [f32; 4] {
        lerp_rgb(color, mix_with, 1.0 - opacity.clamp(0.0, 1.0))
    }

    pub fn dim_if_unselected(
        &self,
        color: [f32; 4],
        index: usize,
        selected: Option<usize>,
    ) -> [f32; 4] {
        match selected {
            Some(sel) if sel != index => lerp_rgb(color, self.muted, 0.55),
            _ => [color[0], color[1], color[2], 1.0],
        }
    }
}

impl Default for ChartTheme {
    fn default() -> Self {
        Self::tokens()
    }
}

pub(crate) fn lerp_rgb(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        1.0,
    ]
}
