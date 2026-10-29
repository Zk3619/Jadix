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
#[path = "tests/emoji_detect.rs"]
mod tests;
