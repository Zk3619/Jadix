//! 变体 chroming 与用户样式覆盖：动态 `variant={sig}` 重绘时保留显式 `bg` / `border` / `shadow`。
//!
//! - `chrome={false}` → 跳过容器与子树语义色重绘（仍更新 `data-variant` 等元数据）。
//! - 用户经 RSX `bg` / `border_color` / `shadow`（或 [`BaseComponentBuilder`]）写入时打标，重绘跳过对应字段。

use crate::core::tree::{Element, LayoutStatus};
use crate::style::{apply_semantic_border, apply_semantic_shadow, SemanticChrome};

pub const ATTR_VARIANT_CHROME: &str = "rui_variant_chrome";
pub const ATTR_OVERRIDE_BG: &str = "rui_override_bg";
pub const ATTR_OVERRIDE_BORDER: &str = "rui_override_border";
pub const ATTR_OVERRIDE_SHADOW: &str = "rui_override_shadow";
pub const ATTR_OVERRIDE_FG: &str = "rui_override_fg";

/// 用户可覆盖的变体容器/字色字段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantStyleField {
    Bg,
    Border,
    Shadow,
    Fg,
}

impl VariantStyleField {
    fn attr(self) -> &'static str {
        match self {
            Self::Bg => ATTR_OVERRIDE_BG,
            Self::Border => ATTR_OVERRIDE_BORDER,
            Self::Shadow => ATTR_OVERRIDE_SHADOW,
            Self::Fg => ATTR_OVERRIDE_FG,
        }
    }
}

/// 根节点是否应用变体 chroming（默认 `true`；`chrome={false}` 为 `false`）。
pub fn variant_chrome_enabled(attrs: &std::collections::HashMap<String, String>) -> bool {
    !matches!(
        attrs.get(ATTR_VARIANT_CHROME).map(String::as_str),
        Some("0") | Some("false") | Some("off")
    )
}

/// 在根节点记录 `chrome` 开关（供动态 bind 与初次挂载共用）。
pub fn set_variant_chrome_enabled(el: &mut Element, enabled: bool) {
    el.set_attr(ATTR_VARIANT_CHROME, if enabled { "1" } else { "0" });
}

/// 标记用户已显式设置某字段；后续 [`apply_semantic_chrome_respecting_overrides`] 不再改写。
pub fn mark_variant_style_override(el: &mut Element, field: VariantStyleField) {
    el.set_attr(field.attr(), "1");
}

pub fn is_variant_style_overridden(
    attrs: &std::collections::HashMap<String, String>,
    field: VariantStyleField,
) -> bool {
    matches!(
        attrs.get(field.attr()).map(String::as_str),
        Some("1" | "true")
    )
}

/// 将 [`SemanticChrome`] 落到容器，跳过 `chrome={false}` 与用户已覆盖字段。
pub fn apply_semantic_chrome_respecting_overrides(el: &mut Element, chrome: &SemanticChrome) {
    if !variant_chrome_enabled(&el.attrs) {
        return;
    }
    if !is_variant_style_overridden(&el.attrs, VariantStyleField::Bg) {
        el.style.bg_color = chrome.bg;
    }
    if !is_variant_style_overridden(&el.attrs, VariantStyleField::Border) {
        apply_semantic_border(el, chrome.border);
    }
    if !is_variant_style_overridden(&el.attrs, VariantStyleField::Shadow) {
        apply_semantic_shadow(el, chrome.shadow);
    }
}

/// RSX / `bind_prop` 动态 `bg`：写入颜色并打覆盖标。
pub fn apply_user_bg(el: &mut Element, color: [f32; 4]) -> LayoutStatus {
    mark_variant_style_override(el, VariantStyleField::Bg);
    if el.style.bg_color == color {
        LayoutStatus::Clean
    } else {
        el.style.bg_color = color;
        LayoutStatus::StyleChanged
    }
}

/// RSX / `bind_prop` 动态 `border_color`：写入颜色并打覆盖标。
pub fn apply_user_border_color(el: &mut Element, color: [f32; 4]) -> LayoutStatus {
    mark_variant_style_override(el, VariantStyleField::Border);
    if el.style.border_color == color {
        LayoutStatus::Clean
    } else {
        el.style.border_color = color;
        LayoutStatus::StyleChanged
    }
}

/// RSX / `bind_prop` 动态 `color` / `text_color`：写入字色并打覆盖标。
pub fn apply_user_fg(el: &mut Element, color: [u8; 4]) -> LayoutStatus {
    mark_variant_style_override(el, VariantStyleField::Fg);
    if el.text_style.color == Some(color) {
        LayoutStatus::Clean
    } else {
        el.text_style.color = Some(color);
        LayoutStatus::StyleChanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        parse_color, semantic_chrome, SemanticChromeDecor, SemanticChromeOpts, SemanticSurface,
        SemanticTone,
    };

    #[test]
    fn chrome_disabled_skips_container_paint() {
        let mut el = Element::new();
        set_variant_chrome_enabled(&mut el, false);
        let chrome = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Info,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::BORDERED,
        });
        el.style.bg_color = parse_color("background");
        apply_semantic_chrome_respecting_overrides(&mut el, &chrome);
        assert_eq!(el.style.bg_color, parse_color("background"));
    }

    #[test]
    fn bg_override_preserved_on_reapply() {
        let mut el = Element::new();
        mark_variant_style_override(&mut el, VariantStyleField::Bg);
        let custom = parse_color("red-500");
        el.style.bg_color = custom;
        let chrome = semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Success,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::BORDERED,
        });
        apply_semantic_chrome_respecting_overrides(&mut el, &chrome);
        assert_eq!(el.style.bg_color, custom);
    }
}
