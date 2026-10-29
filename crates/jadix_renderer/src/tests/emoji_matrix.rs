use super::EMOJI_GPU_COMPAT_SECTION1_SAMPLES;
use crate::emoji_detect;

#[test]
fn section1_samples_contain_emoji() {
    for s in EMOJI_GPU_COMPAT_SECTION1_SAMPLES {
        assert!(
            emoji_detect::text_contains_emoji(s),
            "expected emoji grapheme in {s:?}"
        );
    }
}
