//! macOS system font directories and file names.

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    &[
        "SFNS.ttf",
        "SFNSText.ttf",
        "Helvetica.ttc",
        "Supplemental/Arial.ttf",
        "Times.ttc",
        "Supplemental/Times New Roman.ttf",
        "Times New Roman.ttf",
        "Supplemental/Georgia.ttf",
        "Georgia.ttf",
        "NewYork.ttf",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &[
            "PingFang.ttc",
            "Hiragino Sans GB.ttc",
            "STHeiti Light.ttc",
            "AppleSDGothicNeo.ttc",
        ],
        SystemFontScript::Hangul => &["AppleSDGothicNeo.ttc", "Supplemental/AppleGothic.ttf"],
        SystemFontScript::Thai => &["Thonburi.ttc", "Supplemental/Thonburi.ttc", "Ayuthaya.ttf"],
        SystemFontScript::Emoji => &["Apple Color Emoji.ttc"],
        SystemFontScript::Symbols => &["Apple Symbols.ttf", "Supplemental/Arial Unicode.ttf"],
        SystemFontScript::Arabic => &["Supplemental/Arial.ttf", "GeezaPro.ttc"],
        SystemFontScript::Devanagari => &[
            "Supplemental/Devanagari Sangam MN.ttc",
            "KohinoorDevanagari.ttc",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_dirs_include_system_library() {
        assert_eq!(
            system_font_dirs()[0],
            PathBuf::from("/System/Library/Fonts")
        );
    }

    #[test]
    fn macos_cjk_prefers_pingfang() {
        assert_eq!(
            lazy_script_font_files(SystemFontScript::Cjk)[0],
            "PingFang.ttc"
        );
    }
}
