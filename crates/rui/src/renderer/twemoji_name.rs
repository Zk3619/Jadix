//! Twemoji 资源文件名 stem（`*.png` / `*.svg` 共用），与 `scripts/verify_twemoji_names.ps1` 规则一致。

/// 由 emoji 码位序列生成 Twemoji 文件名 **不含扩展名**（小写十六进制，`-` 连接）。
pub fn twemoji_stem(sequence: &str) -> String {
    let mut parts = Vec::new();
    let has_keycap = sequence.chars().any(|ch| ch as u32 == 0x20E3);
    for ch in sequence.chars() {
        let cp = ch as u32;
        if has_keycap && cp == 0xFE0F {
            continue;
        }
        parts.push(format!("{cp:x}"));
    }
    parts.join("-")
}

#[cfg(test)]
mod tests {
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
        // U+FE0F 在含 U+20E3 时被省略，与 Twemoji 文件名一致
        assert_eq!(twemoji_stem("1️⃣"), "31-20e3");
    }

    #[test]
    fn stem_technologist() {
        assert_eq!(twemoji_stem("🧑‍💻"), "1f9d1-200d-1f4bb");
    }
}
