//! Compact CLDR plural categories (no ICU crate).

/// CLDR plural category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "zero" => Some(Self::Zero),
            "one" => Some(Self::One),
            "two" => Some(Self::Two),
            "few" => Some(Self::Few),
            "many" => Some(Self::Many),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// Select a category for `n` in `language` (ISO 639, lower-case).
///
/// Integer operands only (`n` is truncated toward zero). Prefers ICU4X cardinal rules.
pub fn category(language: &str, n: i64) -> PluralCategory {
    if let Some(cat) = super::icu_data::plural_category(language, n) {
        return cat;
    }
    category_table(language, n)
}

fn category_table(language: &str, n: i64) -> PluralCategory {
    let n = n.unsigned_abs();
    match language {
        // Asian + others with no distinction
        "zh" | "ja" | "ko" | "vi" | "th" | "id" | "ms" | "lo" | "my" | "km" | "yue" => {
            PluralCategory::Other
        }
        "fr" | "ff" | "hy" | "kab" => {
            if n == 0 || n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        "ar" => match n {
            0 => PluralCategory::Zero,
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            n if (3..=10).contains(&(n % 100)) => PluralCategory::Few,
            n if (11..=99).contains(&(n % 100)) => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        "ru" | "uk" | "be" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if mod10 == 1 && mod100 != 11 {
                PluralCategory::One
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        "pl" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        "cs" | "sk" => match n {
            1 => PluralCategory::One,
            2..=4 => PluralCategory::Few,
            _ => PluralCategory::Other,
        },
        "he" => match n {
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            n if n != 0 && n % 10 == 0 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        // English and default
        _ => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_one_other() {
        assert_eq!(category("en", 1), PluralCategory::One);
        assert_eq!(category("en", 0), PluralCategory::Other);
        assert_eq!(category("en", 2), PluralCategory::Other);
    }

    #[test]
    fn chinese_always_other() {
        assert_eq!(category("zh", 0), PluralCategory::Other);
        assert_eq!(category("zh", 1), PluralCategory::Other);
        assert_eq!(category("zh", 8), PluralCategory::Other);
    }

    #[test]
    fn russian_few() {
        assert_eq!(category("ru", 1), PluralCategory::One);
        assert_eq!(category("ru", 2), PluralCategory::Few);
        assert_eq!(category("ru", 5), PluralCategory::Many);
        assert_eq!(category("ru", 11), PluralCategory::Many);
        assert_eq!(category("ru", 21), PluralCategory::One);
    }
}
