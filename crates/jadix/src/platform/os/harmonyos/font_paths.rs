//! HarmonyOS / OpenHarmony system fonts.

#![cfg_attr(not(target_env = "ohos"), allow(dead_code))]

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    ["/system/fonts", "/system/font", "/data/fonts"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    &[
        "HarmonyOS_Sans.ttf",
        "HarmonyOS_Sans_Regular.ttf",
        "HarmonyOS_Sans_SC.ttf",
        "NotoSans-Regular.ttf",
        "Roboto-Regular.ttf",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &[
            "HarmonyOS_Sans_SC.ttf",
            "HarmonyOS_Sans_NaskhArabic.ttf",
            "NotoSansCJK-Regular.ttc",
            "NotoSansSC-Regular.otf",
        ],
        SystemFontScript::Hangul => &["NotoSansCJK-Regular.ttc", "NotoSansKR-Regular.otf"],
        SystemFontScript::Thai => &["NotoSansThai-Regular.ttf"],
        SystemFontScript::Emoji => &["NotoColorEmoji.ttf"],
        SystemFontScript::Symbols => &["NotoSansSymbols-Regular.ttf"],
        SystemFontScript::Arabic => &[
            "HarmonyOS_Sans_NaskhArabic.ttf",
            "NotoNaskhArabic-Regular.ttf",
        ],
        SystemFontScript::Devanagari => &["NotoSansDevanagari-Regular.ttf"],
    }
}

#[cfg(test)]
#[path = "tests/font_paths.rs"]
mod tests;
