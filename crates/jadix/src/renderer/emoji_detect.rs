//! 统一的 emoji grapheme 检测（`contains` 与切分共用一套规则）。

use unicode_segmentation::UnicodeSegmentation;

use super::extended_pictographic::is_extended_pictographic as char_is_extended_pictographic;

#[inline]
fn char_is_emoji_codepoint(cp: u32) -> bool {
    // Prefer Extended_Pictographic for Misc Symbols / Dingbats. The old blanket
    // `0x2600..=0x27BF` stole text stars/shapes (★ ☆) into the emoji pipeline.
    matches!(
        cp,
        0x1F300..=0x1FAFF // emoji blocks
            | 0xFE00..=0xFE0F // variation selectors (☀️ etc.)
            | 0x1F1E6..=0x1F1FF // regional indicators
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

/// 区域指示符国旗 grapheme（如 `🇨🇳`）：恰好两个 Regional Indicator。
pub fn grapheme_is_regional_flag(g: &str) -> bool {
    let mut n = 0usize;
    for c in g.chars() {
        let cp = c as u32;
        if (0x1F1E6..=0x1F1FF).contains(&cp) {
            n += 1;
        } else if cp == 0xFE0F {
            continue;
        } else {
            return false;
        }
    }
    n == 2
}

/// 单个 Unicode grapheme是否按 emoji 处理（与 Twemoji 切分一致）。
pub fn grapheme_is_emoji(g: &str) -> bool {
    if g.chars()
        .any(|c| char_is_emoji_codepoint(c as u32) || char_is_extended_pictographic(c))
    {
        return true;
    }
    grapheme_is_keycap_sequence(g)
}

/// 整段文本是否包含 emoji grapheme。
pub fn text_contains_emoji(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    // Fast reject for CJK / Arabic / Cyrillic / etc.: no emoji-capable codepoints →
    // skip grapheme segmentation (gallery text-page scroll: dozens of labels / frame).
    if !text.chars().any(|c| {
        let cp = c as u32;
        char_is_emoji_codepoint(cp)
            || char_is_extended_pictographic(c)
            || cp == 0x200D // ZWJ
            || cp == 0x20E3 // combining keycap
            || cp == 0xFE0F // VS16
    }) {
        return false;
    }
    text.graphemes(true).any(grapheme_is_emoji)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_count_as_emoji() {
        assert!(text_contains_emoji("🇨🇳"));
        assert!(grapheme_is_emoji("🇺🇸"));
        assert!(grapheme_is_regional_flag("🇨🇳"));
        assert!(grapheme_is_regional_flag("🇺🇸"));
        assert!(!grapheme_is_regional_flag("😀"));
        assert!(!grapheme_is_regional_flag("🏳️‍🌈"));
    }

    #[test]
    fn box_shape_symbols_are_text_not_emoji() {
        // Gallery "Box/shape symbols" must stay text glyphs — emoji pipeline + Twemoji miss
        // turns them into tiny/missing custom quads.
        for g in [
            "■", "□", "▲", "△", "●", "○", "◆", "◇", "★", "☆", "※", "◎", "◉",
        ] {
            assert!(
                !grapheme_is_emoji(g),
                "{g} (U+{:04X}) must not be classified as emoji",
                g.chars().next().unwrap() as u32
            );
        }
        // Still treat true emoji / emoji-presentation symbols as emoji.
        assert!(grapheme_is_emoji("☀")); // often emoji-style
        assert!(grapheme_is_emoji("✅"));
        assert!(grapheme_is_emoji("😀"));
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
    fn plain_cjk_skips_grapheme_walk() {
        assert!(!text_contains_emoji("abc 中文"));
        assert!(!text_contains_emoji(
            "中文测试：输入法、标点符号、全角半角、换行和字距表现。"
        ));
        assert!(!text_contains_emoji(
            "Arabic: مرحبا بالعالم — without emoji codepoints"
        ));
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
