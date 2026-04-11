//! 统一的 emoji grapheme 检测（`contains` 与切分共用一套规则）。

use std::sync::OnceLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

fn extended_pictographic_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\p{Extended_Pictographic}").expect("Extended_Pictographic class")
    })
}

/// UTS #51 Extended_Pictographic（随 `regex` 内置 Unicode 数据版本升级），补全手写区间漏检。
#[inline]
fn char_is_extended_pictographic(c: char) -> bool {
    let mut buf = [0u8; 4];
    extended_pictographic_re().is_match(c.encode_utf8(&mut buf))
}

#[inline]
fn char_is_emoji_codepoint(cp: u32) -> bool {
    matches!(
        cp,
        0x1F300..=0x1FAFF
            | 0x2600..=0x27BF
            | 0xFE00..=0xFE0F
            | 0x1F1E6..=0x1F1FF
    )
}

/// 是否为 keycap 类 grapheme（如 `1️⃣`、`#️⃣`）：数字/`#`/`*` + 可选 VS16 + U+20E3。
fn grapheme_is_keycap_sequence(g: &str) -> bool {
    let mut has_base = false;
    let mut has_combining_keycap = false;
    for c in g.chars() {
        let cp = c as u32;
        match cp {
            0x20E3 => has_combining_keycap = true,
            0x23 | 0x2A | 0x30..=0x39 => has_base = true,
            _ => {}
        }
    }
    has_base && has_combining_keycap
}

/// 单个 Unicode grapheme是否按 emoji 处理（与 Twemoji 切分一致）。
pub fn grapheme_is_emoji(g: &str) -> bool {
    if g.chars().any(|c| {
        char_is_emoji_codepoint(c as u32) || char_is_extended_pictographic(c)
    }) {
        return true;
    }
    grapheme_is_keycap_sequence(g)
}

/// 整段文本是否包含 emoji grapheme。
pub fn text_contains_emoji(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    text.graphemes(true).any(|g| grapheme_is_emoji(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_count_as_emoji() {
        assert!(text_contains_emoji("🇨🇳"));
        assert!(grapheme_is_emoji("🇺🇸"));
    }

    #[test]
    fn classic_emoji() {
        assert!(grapheme_is_emoji("😀"));
        assert!(text_contains_emoji("Hello 😀"));
    }

    #[test]
    fn keycap() {
        assert!(grapheme_is_emoji("1️⃣"));
        assert!(text_contains_emoji("9️⃣"));
    }

    #[test]
    fn plain_text_negative() {
        assert!(!text_contains_emoji("abc 中文"));
        assert!(!grapheme_is_emoji("a"));
    }

    /// U+203C ‼：Extended_Pictographic，但不在原 `2600..=0x27BF` 等手写块内。
    #[test]
    fn extended_pictographic_double_exclamation() {
        assert!(grapheme_is_emoji("‼"));
        assert!(text_contains_emoji("Alert ‼ now"));
    }

    /// U+231A ⌚：同上，校验表驱动属性补洞。
    #[test]
    fn extended_pictographic_watch() {
        assert!(grapheme_is_emoji("⌚"));
        assert!(text_contains_emoji("time ⌚"));
    }

    /// 与 §1 矩阵及 `split_text_with_emoji` 消费侧一致：整段须判为含 emoji。
    #[test]
    fn matrix_mixed_line_contains_emoji() {
        let s = "Mixed: Rust 👩‍💻 + 中文 + عربي";
        assert!(text_contains_emoji(s));
    }
}
