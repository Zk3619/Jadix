//! 文本排版与绘制相关样式（字号、字重、颜色、装饰线、阴影、对齐等）。
//! 与 [`super::Style`]（盒模型 / Taffy）分离：后者管布局，本模块管「字怎么画」。

/// 文本装饰线线型（与 CSS `text-decoration-style` 对齐）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// 单行输入框内光标的绘制参数（由 [`crate::components::input`] 写入）。
#[derive(Clone, Debug, PartialEq)]
pub struct InputCaretDraw {
    /// 在 `display` 文本（UTF-8）中的字节下标，光标画在该位置之前。
    pub caret_byte_index: usize,
    /// 选区 `[start, end)`（UTF-8 字节，与 Web `::selection` / Tailwind `selection:` 语义对齐）；`None` 或空区间不画高亮。
    pub selection: Option<(usize, usize)>,
    /// 选区背景色（通常为 `primary` 半透明，对应 `selection:bg-primary`）。
    pub selection_bg: [u8; 4],
    /// 改写（Overwrite）模式下，闪烁高亮「下一待替换字符」`[start, end)`；与 `selection` 互斥，由 [`crate::components::input`] 写入。
    pub overwrite_highlight: Option<(usize, usize)>,
}

/// 单段文本的排版样式；[`crate::core::tree::Element`] 与布局产出的 [`crate::layout::layout_tree::TextLayoutInfo`] 共用。
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font_family: Option<String>,
    pub color: [u8; 4],
    pub size: f32,
    pub weight: u16,
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub decoration_style: TextDecorationStyle,
    pub decoration_color: Option<[u8; 4]>,
    pub decoration_thickness: f32,
    pub underline_offset: f32,
    pub shadow_color: Option<[u8; 4]>,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    /// 绝对行高（逻辑像素）。
    pub line_height_px: Option<f32>,
    /// 行高相对字号的倍率。`None` 表示使用默认策略。
    pub line_height_scale: Option<f32>,
    pub align: glyphon::cosmic_text::Align,
    /// 若存在，在 [`crate::renderer::prepare_text`] 中额外绘制文本插入符（需焦点与闪烁相位）。
    pub input_caret: Option<InputCaretDraw>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            color: [255, 255, 255, 255],
            size: 16.0,
            weight: 400,
            underline: false,
            overline: false,
            line_through: false,
            decoration_style: TextDecorationStyle::Solid,
            decoration_color: None,
            decoration_thickness: 0.0,
            underline_offset: 1.0,
            shadow_color: None,
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            line_height_px: None,
            line_height_scale: None,
            align: glyphon::cosmic_text::Align::Left,
            input_caret: None,
        }
    }
}

fn parse_css_len_px(token: &str, root_font_size_px: f32) -> Option<f32> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f32>() {
        // Unsuffixed numeric values default to rem (align with IntoF32/IntoDimension).
        return Some(v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("px") {
        return raw.trim().parse::<f32>().ok();
    }
    if let Some(raw) = t.strip_suffix("rem") {
        return raw.trim().parse::<f32>().ok().map(|v| v * root_font_size_px);
    }
    if let Some(raw) = t.strip_suffix("em") {
        return raw.trim().parse::<f32>().ok().map(|v| v * root_font_size_px);
    }
    None
}

/// 解析单个 `text-shadow`（不支持逗号多阴影）。
///
/// 语义参考 MDN：`<color>? && <length>{2,3}`。
/// `color` 与本项目语义色系统对齐（`parse_text_color`）。
pub fn parse_text_shadow(s: impl AsRef<str>) -> (Option<[u8; 4]>, [f32; 2], f32) {
    let raw = s.as_ref().trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return (None, [0.0, 0.0], 0.0);
    }

    // Keep in sync with `CSS_ROOT_FONT_SIZE_PX`.
    let root_font_size_px = 16.0_f32;

    let mut lens: Vec<f32> = Vec::with_capacity(3);
    let mut color: Option<[u8; 4]> = None;
    for tok in raw.split_whitespace() {
        if let Some(v) = parse_css_len_px(tok, root_font_size_px) {
            lens.push(v);
        } else {
            color = Some(super::parse_text_color(tok));
        }
    }

    let mut offset = [0.0, 0.0];
    let mut blur = 0.0;
    if lens.len() >= 2 {
        offset = [lens[0], lens[1]];
    }
    if lens.len() >= 3 {
        blur = lens[2].max(0.0);
    }
    (color, offset, blur)
}

#[cfg(test)]
mod tests {
    use super::parse_text_shadow;

    fn approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
    }

    #[test]
    fn text_shadow_unsuffixed_defaults_to_rem() {
        let (_, offset, blur) = parse_text_shadow("1 0.5 0.25");
        approx_eq(offset[0], 16.0);
        approx_eq(offset[1], 8.0);
        approx_eq(blur, 4.0);
    }

    #[test]
    fn text_shadow_supports_px_rem_em_conversion() {
        let (_, offset, blur) = parse_text_shadow("4px 0.5rem 0.25em");
        approx_eq(offset[0], 4.0);
        approx_eq(offset[1], 8.0);
        approx_eq(blur, 4.0);
    }
}
