//! Builder 链式双收：CSS / token 字符串 **与** 已解析类型走同一方法。
//!
//! 产品面默认仍是字符串（RSX / `class` / Gallery）。对内枚举与 Taffy 值是真源。
//! 口径见 `docs/COMPONENT_LIBRARY.md` §3.1.1。

use taffy::prelude::{AlignItems, FlexWrap, JustifyContent};
use taffy::style::Overflow;

use super::color::{f32_to_u8, parse_color, parse_text_color, rgba_u8_to_f32};
use super::cursor::{CursorKind, CursorValue};

macro_rules! impl_strish {
    ($trait:ident, $method:ident, $out:ty, $convert:expr) => {
        impl $trait for &str {
            #[inline]
            fn $method(self) -> $out {
                $convert(self)
            }
        }
        impl $trait for String {
            #[inline]
            fn $method(self) -> $out {
                $convert(self.as_str())
            }
        }
        impl $trait for &String {
            #[inline]
            fn $method(self) -> $out {
                $convert(self.as_str())
            }
        }
    };
}

/// Paint fill (`bg` / `border_color`): token string, hex, or already-parsed sRGB.
pub trait IntoPaintColor {
    fn into_paint_color(self) -> [f32; 4];
}

impl IntoPaintColor for [f32; 4] {
    #[inline]
    fn into_paint_color(self) -> [f32; 4] {
        self
    }
}

impl IntoPaintColor for &[f32; 4] {
    #[inline]
    fn into_paint_color(self) -> [f32; 4] {
        *self
    }
}

impl IntoPaintColor for [u8; 4] {
    #[inline]
    fn into_paint_color(self) -> [f32; 4] {
        rgba_u8_to_f32(self)
    }
}

impl_strish!(IntoPaintColor, into_paint_color, [f32; 4], parse_color);

/// Glyph / caption color (`text_color` / `color`): token string or `[u8; 4]` / `[f32; 4]`.
pub trait IntoTextColor {
    fn into_text_color(self) -> [u8; 4];
}

impl IntoTextColor for [u8; 4] {
    #[inline]
    fn into_text_color(self) -> [u8; 4] {
        self
    }
}

impl IntoTextColor for &[u8; 4] {
    #[inline]
    fn into_text_color(self) -> [u8; 4] {
        *self
    }
}

impl IntoTextColor for [f32; 4] {
    #[inline]
    fn into_text_color(self) -> [u8; 4] {
        f32_to_u8(self)
    }
}

impl_strish!(IntoTextColor, into_text_color, [u8; 4], parse_text_color);

/// CSS `overflow` keyword or a Taffy overflow value.
pub trait IntoOverflow {
    fn into_overflow(self) -> Overflow;
}

impl IntoOverflow for Overflow {
    #[inline]
    fn into_overflow(self) -> Overflow {
        self
    }
}

impl_strish!(IntoOverflow, into_overflow, Overflow, parse_overflow_keyword);

pub fn parse_overflow_keyword(mode: &str) -> Overflow {
    match mode.trim().to_ascii_lowercase().as_str() {
        "hidden" => Overflow::Hidden,
        "clip" => Overflow::Clip,
        "scroll" => Overflow::Scroll,
        // Taffy does not expose CSS `auto`; map to scrollable behavior.
        "auto" => Overflow::Scroll,
        _ => Overflow::Visible,
    }
}

pub fn overflow_as_str(o: Overflow) -> &'static str {
    match o {
        Overflow::Visible => "visible",
        Overflow::Clip => "clip",
        Overflow::Hidden => "hidden",
        Overflow::Scroll => "scroll",
    }
}

/// CSS `justify-content` / Tailwind `justify-*`.
pub trait IntoJustifyContent {
    fn into_justify_content(self) -> JustifyContent;
}

impl IntoJustifyContent for JustifyContent {
    #[inline]
    fn into_justify_content(self) -> JustifyContent {
        self
    }
}

impl_strish!(
    IntoJustifyContent,
    into_justify_content,
    JustifyContent,
    parse_justify_content
);

pub fn parse_justify_content(keyword: &str) -> JustifyContent {
    match keyword.trim().to_ascii_lowercase().as_str() {
        "center" => JustifyContent::CENTER,
        "start" | "flex-start" => JustifyContent::FLEX_START,
        "end" | "flex-end" => JustifyContent::FLEX_END,
        "between" | "space-between" => JustifyContent::SPACE_BETWEEN,
        "around" | "space-around" => JustifyContent::SPACE_AROUND,
        "evenly" | "space-evenly" => JustifyContent::SPACE_EVENLY,
        _ => JustifyContent::FLEX_START,
    }
}

/// CSS `align-items` / Tailwind `items-*`.
pub trait IntoAlignItems {
    fn into_align_items(self) -> AlignItems;
}

impl IntoAlignItems for AlignItems {
    #[inline]
    fn into_align_items(self) -> AlignItems {
        self
    }
}

impl_strish!(IntoAlignItems, into_align_items, AlignItems, parse_align_items);

pub fn parse_align_items(keyword: &str) -> AlignItems {
    match keyword.trim().to_ascii_lowercase().as_str() {
        "center" => AlignItems::CENTER,
        "start" | "flex-start" => AlignItems::FLEX_START,
        "end" | "flex-end" => AlignItems::FLEX_END,
        "stretch" => AlignItems::STRETCH,
        "baseline" => AlignItems::BASELINE,
        _ => AlignItems::STRETCH,
    }
}

/// CSS `flex-wrap`.
pub trait IntoFlexWrap {
    fn into_flex_wrap(self) -> FlexWrap;
}

impl IntoFlexWrap for FlexWrap {
    #[inline]
    fn into_flex_wrap(self) -> FlexWrap {
        self
    }
}

impl_strish!(IntoFlexWrap, into_flex_wrap, FlexWrap, parse_flex_wrap);

pub fn parse_flex_wrap(keyword: &str) -> FlexWrap {
    match keyword.trim().to_ascii_lowercase().as_str() {
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" | "wrap_reverse" => FlexWrap::WrapReverse,
        _ => FlexWrap::NoWrap,
    }
}

/// CSS `cursor`: keyword string, [`CursorKind`], or full [`CursorValue`].
/// `inherit` / `unset` / empty clear the local declaration (ancestor wins).
pub trait IntoCursor {
    fn into_cursor(self) -> Option<CursorValue>;
}

impl IntoCursor for CursorValue {
    #[inline]
    fn into_cursor(self) -> Option<CursorValue> {
        Some(self)
    }
}

impl IntoCursor for CursorKind {
    #[inline]
    fn into_cursor(self) -> Option<CursorValue> {
        Some(CursorValue::keyword(self))
    }
}

impl IntoCursor for Option<CursorValue> {
    #[inline]
    fn into_cursor(self) -> Option<CursorValue> {
        self
    }
}

impl_strish!(IntoCursor, into_cursor, Option<CursorValue>, CursorValue::parse_css);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{parse_color, LineStyle, TextAlign, TextAlignDeclaration, TextOverflow};

    #[test]
    fn paint_color_dual_accepts_token_and_array() {
        let from_token = "primary".into_paint_color();
        let parsed = parse_color("primary");
        assert_eq!(from_token, parsed);
        let arr = [0.1, 0.2, 0.3, 1.0];
        assert_eq!(arr.into_paint_color(), arr);
        assert_eq!((&arr).into_paint_color(), arr);
    }

    #[test]
    fn overflow_dual_accepts_keyword_and_enum() {
        assert_eq!("hidden".into_overflow(), Overflow::Hidden);
        assert_eq!(Overflow::Clip.into_overflow(), Overflow::Clip);
        assert_eq!(overflow_as_str(parse_overflow_keyword("auto")), "scroll");
    }

    #[test]
    fn justify_and_items_parse_tailwind_aliases() {
        assert_eq!(
            "between".into_justify_content(),
            JustifyContent::SPACE_BETWEEN
        );
        assert_eq!("center".into_align_items(), AlignItems::CENTER);
        assert_eq!("wrap-reverse".into_flex_wrap(), FlexWrap::WrapReverse);
    }

    #[test]
    fn local_enums_from_str_and_self() {
        let overflow: TextOverflow = "ellipsis".into();
        assert_eq!(overflow, TextOverflow::Ellipsis);
        let align: TextAlign = "center".into();
        assert_eq!(align, TextAlign::Center);
        let decl: TextAlignDeclaration = "inherit".into();
        assert_eq!(decl, TextAlignDeclaration::Inherit);
        let decl2: TextAlignDeclaration = TextAlign::Right.into();
        assert_eq!(decl2, TextAlignDeclaration::Value(TextAlign::Right));
        let style: LineStyle = "dashed".into();
        assert_eq!(style, LineStyle::Dashed);
        assert_eq!(LineStyle::Solid, LineStyle::from("solid"));
    }

    #[test]
    fn cursor_kind_and_inherit() {
        assert_eq!(
            CursorKind::Text.into_cursor().map(|c| c.kind),
            Some(CursorKind::Text)
        );
        assert_eq!("inherit".into_cursor(), None);
        assert_eq!(
            "pointer".into_cursor().map(|c| c.kind),
            Some(CursorKind::Pointer)
        );
    }
}
