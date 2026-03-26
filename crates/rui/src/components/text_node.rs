use crate::core::tree::{parse_text_color, Element};

/// 供“内嵌文本”场景复用的构建配置。
#[derive(Clone, Debug)]
pub struct TextNodeConfig {
    pub size_px: f32,
    pub weight: u16,
    pub color: [u8; 4],
    pub underline: bool,
    pub center: bool,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    pub font_family: Option<String>,
}

/// ShadCN Label 默认文本样式（text-sm / font-medium / leading-none / foreground）。
pub fn apply_label_defaults(el: Element) -> Element {
    el.text_size(0.875 * 16.0)
        .font_weight(500.0)
        .text_leading_none()
        .text_color(parse_text_color("foreground"))
}

/// 构建一个用于组件内嵌文本的节点，统一行高/对齐/下划线逻辑。
pub fn build_text_node(text: impl Into<String>, mut cfg: TextNodeConfig) -> Element {
    // Lucide 图标字体仅有 Regular（OS/2 usWeightClass=400）。若沿用按钮的 font-medium(500)，
    // fontdb 选不到对应 face 时会整段回退到系统字体，PUA 码位会错成 Segoe 等里的别的符号。
    if cfg.font_family.as_deref() == Some("lucide") {
        cfg.weight = 400;
    }
    let mut el = Element::new()
        .text(text.into())
        .text_size(cfg.size_px)
        .font_weight(cfg.weight as f32)
        .underline(cfg.underline)
        .text_color(cfg.color);

    if cfg.center {
        el = el.text_center();
    }

    if let Some(px) = cfg.line_height_px {
        el = el.text_line_height(px);
    } else if let Some(scale) = cfg.leading_scale {
        el = el.text_leading(scale);
    }

    if let Some(family) = cfg.font_family {
        el = el.font_family(family);
    }

    el
}

/// 构建 icon glyph 文本节点（与 `Icon` 组件在宏中的默认行为一致）。
pub fn build_icon_text_node(glyph: impl Into<String>) -> Element {
    Element::new().font_family("lucide").text(glyph)
}
