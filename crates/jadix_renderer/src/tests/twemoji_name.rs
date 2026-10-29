use super::*;

#[test]
fn stem_family_zwj() {
    assert_eq!(twemoji_stem("👨‍👩‍👧‍👦"), "1f468-200d-1f469-200d-1f467-200d-1f466");
}

#[test]
fn stem_skin_tone() {
    assert_eq!(twemoji_stem("👍🏽"), "1f44d-1f3fd");
}

#[test]
fn stem_flags() {
    assert_eq!(twemoji_stem("🇨🇳"), "1f1e8-1f1f3");
    assert_eq!(twemoji_stem("🇺🇸"), "1f1fa-1f1f8");
}

#[test]
fn stem_keycap_digit() {
    // U+FE0F 一律省略，与 Twemoji 文件名一致
    assert_eq!(twemoji_stem("1️⃣"), "31-20e3");
}

#[test]
fn stem_red_heart_drops_vs16() {
    assert_eq!(twemoji_stem("❤️"), "2764");
    assert_eq!(twemoji_stem("❤"), "2764");
}

#[test]
fn stem_technologist() {
    assert_eq!(twemoji_stem("🧑‍💻"), "1f9d1-200d-1f4bb");
}
