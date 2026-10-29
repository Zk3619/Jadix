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
