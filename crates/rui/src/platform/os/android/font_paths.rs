//! Android system fonts (`/system/fonts`).

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    ["/system/fonts", "/system/font"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    &[
        "Roboto-Regular.ttf",
        "Roboto.ttf",
        "NotoSans-Regular.ttf",
        "DroidSans.ttf",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &[
            "NotoSansCJK-Regular.ttc",
            "NotoSansSC-Regular.otf",
            "NotoSansCJKsc-Regular.otf",
        ],
        SystemFontScript::Hangul => &["NotoSansCJK-Regular.ttc", "NotoSansKR-Regular.otf"],
        SystemFontScript::Thai => &["NotoSansThai-Regular.ttf"],
        SystemFontScript::Emoji => &["NotoColorEmoji.ttf"],
        SystemFontScript::Symbols => &["NotoSansSymbols-Regular.ttf"],
        SystemFontScript::Arabic => &["NotoNaskhArabic-Regular.ttf"],
        SystemFontScript::Devanagari => &["NotoSansDevanagari-Regular.ttf"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_prefers_roboto() {
        assert_eq!(system_font_dirs()[0], PathBuf::from("/system/fonts"));
        assert_eq!(eager_ui_font_files()[0], "Roboto-Regular.ttf");
    }
}
