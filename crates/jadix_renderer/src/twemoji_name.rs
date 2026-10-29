//! Twemoji 资源文件名 stem（`*.png` / `*.svg` 共用），与 `scripts/assets/verify_twemoji_names.ps1` 规则一致。

/// 由 emoji 码位序列生成 Twemoji 文件名 **不含扩展名**（小写十六进制，`-` 连接）。
///
/// Twemoji 资源会省略 U+FE0F（emoji 变体选择符）；❤️ 对应 `2764.png` 而不是 `2764-fe0f.png`。
pub fn twemoji_stem(sequence: &str) -> String {
    let mut parts = Vec::new();
    for ch in sequence.chars() {
        let cp = ch as u32;
        if cp == 0xFE0F {
            continue;
        }
        parts.push(format!("{cp:x}"));
    }
    parts.join("-")
}

#[cfg(test)]
#[path = "tests/twemoji_name.rs"]
mod tests;
