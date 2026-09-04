//! 与 `docs/engine/emoji_gpu_compat_matrix.md` §1 对齐的可执行回归清单（单元测试遍历）。

/// 文档 §「核心文本样例」中的固定字符串，发布前自动化与人工目视共用同一组。
pub const EMOJI_GPU_COMPAT_SECTION1_SAMPLES: &[&str] = &[
    "Hello 😀 😁 😭",
    "Family: 👨‍👩‍👧‍👦",
    "SkinTone: 👍🏽",
    "Flag: 🇨🇳 🇺🇸",
    "Keycap: 1️⃣ 2️⃣ 9️⃣",
    "Mixed: Rust 👩‍💻 + 中文 + عربي",
];

#[cfg(test)]
mod tests {
    use super::EMOJI_GPU_COMPAT_SECTION1_SAMPLES;
    use crate::renderer::emoji_detect;

    #[test]
    fn section1_samples_contain_emoji() {
        for s in EMOJI_GPU_COMPAT_SECTION1_SAMPLES {
            assert!(
                emoji_detect::text_contains_emoji(s),
                "expected emoji grapheme in {s:?}"
            );
        }
    }
}
