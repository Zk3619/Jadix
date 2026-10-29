//! Unix / *BSD system font directories and file names.
//!
//! This is the desktop default: Linux forwards here; FreeBSD / NetBSD hit it via
//! the non-Win/macOS/Linux facade branch. Do **not** gate these tables on
//! `target_os = "linux"` only — `/usr/share/fonts` is shared.
//! See `docs/engine/platform.md` §9.3.

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/opentype",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    &[
        "truetype/dejavu/DejaVuSans.ttf",
        "truetype/dejavu/DejaVuSans-Bold.ttf",
        "truetype/dejavu/DejaVuSansMono.ttf",
        "truetype/dejavu/DejaVuSerif.ttf",
        "truetype/liberation/LiberationSerif-Regular.ttf",
        "truetype/liberation2/LiberationSerif-Regular.ttf",
        "liberation/LiberationSerif-Regular.ttf",
        "truetype/noto/NotoSerif-Regular.ttf",
        "noto/NotoSerif-Regular.ttf",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &[
            "opentype/noto/NotoSansCJK-Regular.ttc",
            "noto-cjk/NotoSansCJK-Regular.ttc",
            "truetype/noto/NotoSansCJK-Regular.ttc",
            "truetype/wqy/wqy-microhei.ttc",
        ],
        SystemFontScript::Hangul => &[
            "opentype/noto/NotoSansCJK-Regular.ttc",
            "truetype/noto/NotoSansCJK-Regular.ttc",
            "truetype/nanum/NanumGothic.ttf",
        ],
        SystemFontScript::Thai => &[
            "truetype/noto/NotoSansThai-Regular.ttf",
            "noto/NotoSansThai-Regular.ttf",
            "truetype/tlwg/TlwgTypo.ttf",
        ],
        SystemFontScript::Emoji => &[
            "truetype/noto/NotoColorEmoji.ttf",
            "noto/NotoColorEmoji.ttf",
            "google-noto/NotoColorEmoji.ttf",
        ],
        SystemFontScript::Symbols => &["truetype/dejavu/DejaVuSans.ttf"],
        SystemFontScript::Arabic => &[
            "truetype/dejavu/DejaVuSans.ttf",
            "truetype/liberation/LiberationSans-Regular.ttf",
        ],
        SystemFontScript::Devanagari => &[
            "truetype/noto/NotoSansDevanagari-Regular.ttf",
            "noto/NotoSansDevanagari-Regular.ttf",
        ],
    }
}

#[cfg(test)]
#[path = "tests/font_paths.rs"]
mod tests;
