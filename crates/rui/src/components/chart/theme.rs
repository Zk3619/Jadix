//! 图表主题与 mark 绘制风格（对照 TanStack `ChartTheme` + shadcn `ChartConfig` / tooltip indicator）。

use std::collections::HashMap;
use std::str::FromStr;

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
    /// 柱的值端圆角（垂直柱顶 / 水平柱外端）；`None` 时 bar 用默认 4px。
    pub corner_radius: Option<f32>,
    /// 柱的基线端圆角（垂直柱底 / 水平柱贴轴端）；`None` 时有坐标轴为 0，无轴则跟值端。
    pub corner_radius_base: Option<f32>,
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
    pub fn corner_radius(mut self, px: f32) -> Self {
        self.corner_radius = Some(px.max(0.0));
        self
    }
    pub fn corner_radius_base(mut self, px: f32) -> Self {
        self.corner_radius_base = Some(px.max(0.0));
        self
    }
}

/// shadcn `ChartTooltipContent` `indicator`：`dot` | `line` | `dashed`。未知 → [`Dot`](Self::Dot)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipIndicator {
    #[default]
    Dot,
    Line,
    Dashed,
}

impl FromStr for TooltipIndicator {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "line" => Self::Line,
            "dashed" | "dash" => Self::Dashed,
            "dot" | "" => Self::Dot,
            _ => Self::Dot,
        })
    }
}

impl From<&str> for TooltipIndicator {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Dot)
    }
}

impl From<String> for TooltipIndicator {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

/// shadcn `ChartConfig` 单项：系列显示名 + 可选色。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartSeriesConfig {
    pub label: Option<String>,
    pub color: Option<[f32; 4]>,
}

impl ChartSeriesConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn color(mut self, color: impl Into<[f32; 4]>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// shadcn `ChartConfig`：key → label / color（不替代 mark 级 `.fill()`）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartConfig {
    pub entries: HashMap<String, ChartSeriesConfig>,
}

impl ChartConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entry(mut self, key: impl Into<String>, cfg: ChartSeriesConfig) -> Self {
        self.entries.insert(key.into(), cfg);
        self
    }

    pub fn get(&self, key: &str) -> Option<&ChartSeriesConfig> {
        self.entries.get(key)
    }

    pub fn label_for(&self, key: &str) -> Option<&str> {
        self.entries
            .get(key)
            .and_then(|e| e.label.as_deref())
            .filter(|s| !s.is_empty())
    }

    pub fn color_for(&self, key: &str) -> Option<[f32; 4]> {
        self.entries.get(key).and_then(|e| e.color)
    }

    /// 按插入顺序的 key 列表（稳定色板槽映射）。
    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.entries.keys().map(|s| s.as_str()).collect();
        keys.sort_unstable();
        keys
    }

    /// 用 config 色覆盖 theme palette 对应槽；无色条目保持原 palette。
    pub fn apply_palette_overrides(&self, theme: &mut ChartTheme) {
        if self.entries.is_empty() {
            return;
        }
        let keys = self.keys();
        for (i, key) in keys.iter().enumerate() {
            if let Some(c) = self.color_for(key) {
                if i < theme.palette.len() {
                    theme.palette[i] = c;
                } else {
                    theme.palette.push(c);
                }
            }
        }
    }
}

impl FromIterator<(String, ChartSeriesConfig)> for ChartConfig {
    fn from_iter<T: IntoIterator<Item = (String, ChartSeriesConfig)>>(iter: T) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
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
    fn chrome() -> ([f32; 4], [f32; 4], [f32; 4], [f32; 4], [f32; 4]) {
        (
            parse_color("transparent"),
            parse_color("foreground"),
            parse_color("muted-foreground"),
            parse_color("border/50"),
            tokens::border(),
        )
    }

    /// shadcn token 默认（`chart-1..5` + 透明底 / muted 轴文 / border/50 网格）。
    pub fn tokens() -> Self {
        let (background, foreground, muted, grid, border) = Self::chrome();
        Self {
            background,
            foreground,
            muted,
            grid,
            border,
            palette: (1u8..=5).map(tokens::chart).collect(),
        }
    }

    /// 灰度阶梯（TanStack theme palette matrix · neutral）。
    pub fn neutral() -> Self {
        let (background, foreground, muted, grid, border) = Self::chrome();
        Self {
            background,
            foreground,
            muted,
            grid,
            border,
            palette: vec![
                parse_color("zinc-300"),
                parse_color("zinc-400"),
                parse_color("zinc-500"),
                parse_color("zinc-600"),
                parse_color("zinc-700"),
            ],
        }
    }

    /// 更高饱和分类色（仍走 token / Tailwind 族名，无魔法 hex）。
    pub fn vibrant() -> Self {
        let (background, foreground, muted, grid, border) = Self::chrome();
        Self {
            background,
            foreground,
            muted,
            grid,
            border,
            palette: vec![
                parse_color("blue-500"),
                parse_color("orange-500"),
                parse_color("emerald-500"),
                parse_color("violet-500"),
                parse_color("pink-500"),
            ],
        }
    }

    /// 单色阶（foreground 系混色）。
    pub fn monochrome() -> Self {
        let (background, foreground, muted, grid, border) = Self::chrome();
        let fg = foreground;
        let bg = parse_color("background");
        Self {
            background,
            foreground,
            muted,
            grid,
            border,
            palette: vec![
                lerp_rgb(fg, bg, 0.15),
                lerp_rgb(fg, bg, 0.30),
                lerp_rgb(fg, bg, 0.45),
                lerp_rgb(fg, bg, 0.60),
                lerp_rgb(fg, bg, 0.75),
            ],
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
            Some(sel) if sel != index => lerp_rgb(color, self.muted, 0.42),
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

#[cfg(test)]
mod theme_unit_tests {
    use super::*;

    #[test]
    fn tokens_chrome_is_transparent_and_muted_foreground() {
        let t = ChartTheme::tokens();
        assert_eq!(t.background, parse_color("transparent"));
        assert_eq!(t.muted, parse_color("muted-foreground"));
        assert_eq!(t.grid, parse_color("border/50"));
        assert_eq!(t.palette.len(), 5);
    }

    #[test]
    fn unknown_tooltip_indicator_falls_back_to_dot() {
        assert_eq!(TooltipIndicator::from("dot"), TooltipIndicator::Dot);
        assert_eq!(TooltipIndicator::from("line"), TooltipIndicator::Line);
        assert_eq!(TooltipIndicator::from("dashed"), TooltipIndicator::Dashed);
        assert_eq!(TooltipIndicator::from("nope"), TooltipIndicator::Dot);
    }

    #[test]
    fn palette_presets_nonempty() {
        assert_eq!(ChartTheme::neutral().palette.len(), 5);
        assert_eq!(ChartTheme::vibrant().palette.len(), 5);
        assert_eq!(ChartTheme::monochrome().palette.len(), 5);
    }
}
