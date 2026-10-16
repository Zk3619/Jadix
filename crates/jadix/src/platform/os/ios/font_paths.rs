//! iOS system font directories (same FS layout as macOS, no `/Library/Fonts`).

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    ["/System/Library/Fonts", "/System/Library/Fonts/Core"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    &[
        "SFNS.ttf",
        "Core/SFUI.ttf",
        "Helvetica.ttc",
        "Core/Helvetica.ttc",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &["PingFang.ttc", "Core/Hiragino Sans GB.ttc"],
        SystemFontScript::Hangul => &["Core/AppleSDGothicNeo.ttc"],
        SystemFontScript::Thai => &["Thonburi.ttc"],
        SystemFontScript::Emoji => &["Core/Apple Color Emoji.ttc"],
        SystemFontScript::Symbols => &["Core/Apple Symbols.ttf"],
        SystemFontScript::Arabic => &["GeezaPro.ttc"],
        SystemFontScript::Devanagari => &["Core/KohinoorDevanagari.ttc"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_dirs_are_system_library() {
        assert_eq!(
            system_font_dirs()[0],
            PathBuf::from("/System/Library/Fonts")
        );
        assert!(!eager_ui_font_files().is_empty());
        assert_eq!(
            lazy_script_font_files(SystemFontScript::Cjk)[0],
            "PingFang.ttc"
        );
    }
}
