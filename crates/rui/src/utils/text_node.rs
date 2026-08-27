//! 组件内嵌文本构建（非 RSX 标签）。
//!
//! 供 Button / Input / Icon 等复用：[`TextNodeConfig`]、[`apply_label_defaults`]、
//! [`build_text_node`]、[`build_icon_text_node`]。应用侧文案请用 [`crate::components::Label`]。

use crate::core::tree::{parse_text_color, Element};

/// 供「内嵌文本」场景复用的构建配置。
#[derive(Clone, Debug)]
pub struct TextNodeConfig {
    pub size_px: f32,
    pub weight: u16,
    pub color: [u8; 4],
    pub underline: bool,
    /// 与 CSS `text-underline-offset` 一致（逻辑像素）；`None` 用引擎默认。
    pub underline_offset_px: Option<f32>,
    /// CSS `text-align`（见 [`crate::style::TextAlign`]）。
    pub text_align: crate::style::TextAlign,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    pub font_family: Option<String>,
}

impl Default for TextNodeConfig {
    fn default() -> Self {
        Self {
            size_px: 14.0,
            weight: 400,
            color: [0, 0, 0, 255],
            underline: false,
            underline_offset_px: None,
            text_align: crate::style::TextAlign::Start,
            line_height_px: None,
            leading_scale: None,
            font_family: None,
        }
    }
}

/// Label 默认：对齐 shadcn/ui `Label`（`text-sm font-medium leading-none` → 14px / 500）。
pub fn apply_label_defaults(el: Element) -> Element {
    el.text_size(0.875 * 16.0)
        .font_weight(500.0)
        .text_leading_none()
        .text_color(parse_text_color("foreground"))
        .min_w("0")
}

/// 构建组件内嵌文本节点，统一行高/对齐/下划线逻辑。
pub fn build_text_node(text: impl Into<String>, mut cfg: TextNodeConfig) -> Element {
    if cfg.font_family.as_deref() == Some("lucide") {
        cfg.weight = 400;
    }
    let mut el = Element::new()
        .text(text.into())
        .text_size(cfg.size_px)
        .font_weight(cfg.weight as f32)
        .underline(cfg.underline)
        .text_color(cfg.color);

    if cfg.text_align != crate::style::TextAlign::Start {
        el = el.text_align(cfg.text_align.as_str());
    }
    if let Some(px) = cfg.line_height_px {
        el = el.text_line_height(px);
    } else if let Some(scale) = cfg.leading_scale {
        el = el.text_leading(scale);
    }
    if let Some(family) = cfg.font_family {
        el = el.font_family(family);
    }
    if let Some(off) = cfg.underline_offset_px {
        el = el.text_underline_offset(off);
    }
    el
}

/// Lucide 图标字形节点（与宏 `<Icon>` 默认行为一致）。
///
/// 默认字色为主题 `foreground`（对齐 Tailwind v4 `text-foreground`），避免
/// [`TextStyle::default`] 的白色在浅色底上看不见。
pub fn build_icon_text_node(glyph: impl Into<String>) -> Element {
    Element::new()
        .font_family("lucide")
        .text(glyph.into())
        .text_color(parse_text_color("foreground"))
        // SVG-only MDN keyword：命中自身布局盒（容器）。
        .pointer_events("bounding-box")
}

/// Like [`build_icon_text_node`] plus `rui_icon_name` for SVG path lookup on spin.
pub fn build_named_icon_text_node(name: impl AsRef<str>, glyph: impl Into<String>) -> Element {
    let mut el = build_icon_text_node(glyph);
    el.set_attr(crate::utils::ATTR_ICON_NAME, name.as_ref());
    // Icon 叶：默认命中自身布局盒（MDN SVG `bounding-box`）。
    // 显式 `pointer-events="none"` / 其他值仍可覆盖。
    el
}

#[cfg(test)]
mod icon_defaults_tests {
    use super::*;
    use crate::core::tree::PointerEvents;

    #[test]
    fn icon_defaults_to_text_foreground() {
        let el = build_icon_text_node("\u{ea0e}");
        let fg = parse_text_color("foreground");
        assert_eq!(el.text_style.color, fg);
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert_eq!(el.pointer_events_mode(), PointerEvents::BoundingBox);
        assert!(el.pointer_events_mode().is_svg_only());
        assert!(el.pointer_events_mode().targets_self());
    }
}
