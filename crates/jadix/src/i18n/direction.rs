//! Writing direction derived from locale (Unicode CLDR).

use crate::style::TextDirection;

/// Paragraph / document writing direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WritingDirection {
    #[default]
    Ltr,
    Rtl,
}

impl WritingDirection {
    /// RTL scripts: Arabic, Hebrew, Persian, Urdu, and related.
    pub fn for_language(language: &str) -> Self {
        match language {
            "ar" | "he" | "fa" | "ur" | "yi" | "ps" | "sd" | "ug" | "dv" | "ckb" | "ku" | "arc"
            | "syr" | "nqo" | "haq" => Self::Rtl,
            _ => Self::Ltr,
        }
    }

    pub fn is_rtl(self) -> bool {
        matches!(self, Self::Rtl)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

impl From<WritingDirection> for TextDirection {
    fn from(d: WritingDirection) -> Self {
        match d {
            WritingDirection::Ltr => TextDirection::Ltr,
            WritingDirection::Rtl => TextDirection::Rtl,
        }
    }
}

#[cfg(test)]
#[path = "tests/direction.rs"]
mod tests;
