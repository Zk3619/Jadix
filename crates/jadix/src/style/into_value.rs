//! Builder 链式双收：CSS / token 字符串 **与** 已解析类型走同一方法。
//!
//! 产品面默认仍是字符串（RSX / `class` / Gallery）。对内枚举与 Taffy 值是真源。
//! 口径见 `docs/COMPONENT_LIBRARY.md` §3.1.1。

use taffy::prelude::{AlignItems, FlexWrap, JustifyContent};
use taffy::style::Overflow;

use super::color::{f32_to_u8, parse_color, parse_text_color, rgba_u8_to_f32};
use super::cursor::{CursorKind, CursorValue};

macro_rules! impl_strish {
    ($trait:ident, $method:ident, $out:ty, $convert:expr $(, $checked_method:ident, $checked_convert:expr)?) => {
        impl $trait for &str {
            #[inline]
            fn $method(self) -> $out {
                $convert(self)
            }
            $(fn $checked_method(self) -> Option<$out> { $checked_convert(self) })?
        }
        impl $trait for String {
            #[inline]
            fn $method(self) -> $out {
                $convert(self.as_str())
            }
            $(fn $checked_method(self) -> Option<$out> { $checked_convert(self.as_str()) })?
        }
        impl $trait for &String {
            #[inline]
            fn $method(self) -> $out {
                $convert(self.as_str())
            }
            $(fn $checked_method(self) -> Option<$out> { $checked_convert(self.as_str()) })?
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

impl_strish!(
    IntoOverflow,
    into_overflow,
    Overflow,
    parse_overflow_keyword
);

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
    fn try_into_justify_content(self) -> Option<JustifyContent>
    where
        Self: Sized,
    {
        Some(self.into_justify_content())
    }
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
    parse_justify_content,
    try_into_justify_content,
    try_parse_justify_content
);

pub fn parse_justify_content(keyword: &str) -> JustifyContent {
    try_parse_justify_content(keyword).unwrap_or(JustifyContent::NORMAL)
}

pub fn try_parse_justify_content(keyword: &str) -> Option<JustifyContent> {
    let normalized = normalize_alignment_keyword(keyword);
    let keyword = match normalized.as_str() {
        "between" => "space-between",
        "around" => "space-around",
        "evenly" => "space-evenly",
        keyword => keyword,
    };
    keyword.parse().ok()
}

/// CSS `align-items` / Tailwind `items-*`.
pub trait IntoAlignItems {
    fn into_align_items(self) -> AlignItems;
    fn try_into_align_items(self) -> Option<AlignItems>
    where
        Self: Sized,
    {
        Some(self.into_align_items())
    }
}

impl IntoAlignItems for AlignItems {
    #[inline]
    fn into_align_items(self) -> AlignItems {
        self
    }
}

impl_strish!(
    IntoAlignItems,
    into_align_items,
    AlignItems,
    parse_align_items,
    try_into_align_items,
    try_parse_align_items
);

pub fn parse_align_items(keyword: &str) -> AlignItems {
    try_parse_align_items(keyword).unwrap_or(AlignItems::NORMAL)
}

pub fn try_parse_align_items(keyword: &str) -> Option<AlignItems> {
    normalize_alignment_keyword(keyword).parse().ok()
}

pub(crate) fn normalize_alignment_keyword(keyword: &str) -> String {
    keyword
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
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

impl_strish!(
    IntoCursor,
    into_cursor,
    Option<CursorValue>,
    CursorValue::parse_css
);

#[cfg(test)]
#[path = "tests/into_value.rs"]
mod tests;
