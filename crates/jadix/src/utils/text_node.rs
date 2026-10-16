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
    /// CSS `text-align` (see [`crate::style::TextAlign`]).
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
/// 显式 `font_weight(400)` + `underline(false)`（带 attr 标记）以免祖先
/// `font-medium` / `hover:underline` 经 cascade 污染 PUA 字形。
pub fn build_icon_text_node(glyph: impl Into<String>) -> Element {
    Element::new()
        .font_family("lucide")
        .font_weight(400.0)
        .underline(false)
        .text(glyph.into())
        .text_color(parse_text_color("foreground"))
        // SVG-only MDN keyword：命中自身布局盒（容器）。
        .pointer_events("bounding-box")
}

/// Like [`build_icon_text_node`] plus `jadix_icon_name` for SVG path lookup on spin.
pub fn build_named_icon_text_node(name: impl AsRef<str>, glyph: impl Into<String>) -> Element {
    let mut el = build_icon_text_node(glyph);
    el.set_attr(crate::utils::ATTR_ICON_NAME, name.as_ref());
    // Icon 叶：默认命中自身布局盒（MDN SVG `bounding-box`）。
    // 显式 `pointer-events="none"` / 其他值仍可覆盖。
    el
}

/// Lucide icon by **name** (not PUA glyph). The renderer cannot reverse-lookup
/// SVG paths from a codepoint — that needs [`ATTR_ICON_NAME`](crate::utils::ATTR_ICON_NAME).
///
/// If both the PUA glyph (`lucide.json`) and SVG inner (`lucide_paths.json`) are
/// missing, emits Unicode `fallback` **without** `font-family: lucide`. Lucide
/// cannot paint `▾` / `›`; claiming that family would tofu or drop the rotate
/// texture (`rasterize_glyph_rgba` maps the char to glyph id 0).
pub fn build_lucide_icon_text_node(name: impl AsRef<str>) -> Element {
    crate::utils::icon_provider::build_icon_named(name)
}

/// [`build_lucide_icon_text_node`] with an explicit Unicode fallback glyph.
pub fn build_lucide_icon_text_node_or(
    name: impl AsRef<str>,
    fallback: impl Into<String>,
) -> Element {
    let name = name.as_ref();
    let fallback = fallback.into();
    match crate::utils::icon_provider::resolve_icon(name) {
        Some(crate::utils::icon_provider::IconSource::Glyph { family, glyph }) => {
            let mut el = build_named_icon_text_node(name, glyph);
            el.text_style.font_family = Some(family);
            el
        }
        Some(crate::utils::icon_provider::IconSource::Src(src)) => {
            crate::utils::icon_provider::build_icon_from_src(src)
        }
        None => {
            if crate::utils::lucide::lucide_svg_inner(name).is_some() {
                return build_named_icon_text_node(name, fallback);
            }
            Element::new()
                .text(fallback)
                .font_weight(400.0)
                .underline(false)
                .text_color(parse_text_color("foreground"))
                .pointer_events("bounding-box")
        }
    }
}

#[cfg(test)]
mod icon_defaults_tests {
    use super::*;
    use crate::core::tree::PointerEvents;

    #[test]
    fn icon_defaults_to_text_foreground() {
        let el = build_icon_text_node("\u{ea0e}");
        let fg = parse_text_color("foreground");
        assert_eq!(el.text_style.color_resolved(), fg);
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert_eq!(el.pointer_events_mode(), PointerEvents::BoundingBox);
        assert!(el.pointer_events_mode().is_svg_only());
        assert!(el.pointer_events_mode().targets_self());
    }

    #[test]
    fn lucide_named_icon_sets_attr_from_name_only() {
        let el = build_lucide_icon_text_node("chevron-down");
        assert_eq!(
            el.get_attr(crate::utils::ATTR_ICON_NAME)
                .map(String::as_str),
            Some("chevron-down")
        );
        assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert!(el.text.as_deref().is_some_and(|t| t != "▾"));
    }

    #[test]
    fn lucide_miss_fallback_is_not_lucide_family() {
        let el = build_lucide_icon_text_node_or("__no_such_lucide_icon__", "▾");
        assert!(el.get_attr(crate::utils::ATTR_ICON_NAME).is_none());
        assert_ne!(el.text_style.font_family.as_deref(), Some("lucide"));
        assert_eq!(el.text.as_deref(), Some("▾"));
    }
}
