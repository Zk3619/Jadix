//! Windows system font directory and file names (`C:\Windows\Fonts`).

use std::path::PathBuf;

use crate::platform::SystemFontScript;

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from(r"C:\Windows\Fonts")]
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    // Minimal UI set (skip arial/cour — saves mapped pages).
    // Regular / Semibold / Bold plus Light + Semilight so the weight axis is denser
    // (still no Segoe UI Medium 500 — that uses VF interpolation or synthetic bold).
    // Times / Georgia Regular+Bold: `font-serif` must not collapse to Segoe.
    &[
        "segoeui.ttf",
        "segoeuil.ttf",
        "segoeuisl.ttf",
        "seguisb.ttf",
        "segoeuib.ttf",
        "seguibl.ttf",
        "consola.ttf",
        "cascadiacode.ttf",
        "CascadiaCode.ttf",
        // Generic `font-serif` / Times / Georgia — Regular only (not a full system scan).
        "times.ttf",
        "timesbd.ttf",
        "georgia.ttf",
        "georgiab.ttf",
    ]
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    match script {
        SystemFontScript::Cjk => &["msyh.ttc", "msyhbd.ttc", "simhei.ttf", "simsun.ttc"],
        // 맑은 고딕 — YaHei Hangul coverage is incomplete → tofu boxes without this.
        SystemFontScript::Hangul => &["malgun.ttf", "malgunbd.ttf", "malgunsl.ttf", "gulim.ttc"],
        // Leelawadee UI (Win10+) or Tahoma (has Thai).
        SystemFontScript::Thai => &["LeelawUI.ttf", "LeelaUIb.ttf", "tahoma.ttf", "tahomabd.ttf"],
        SystemFontScript::Emoji => &["seguiemj.ttf"],
        SystemFontScript::Symbols => &["seguisym.ttf", "seguili.ttf"],
        // Segoe UI already covers much Arabic; add Traditional Arabic / Arial if present.
        SystemFontScript::Arabic => &["tradbdo.ttf", "trado.ttf", "arial.ttf"],
        SystemFontScript::Devanagari => &["Nirmala.ttc", "mangal.ttf"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_fonts_dir() {
        assert_eq!(system_font_dirs(), vec![PathBuf::from(r"C:\Windows\Fonts")]);
    }

    #[test]
    fn windows_eager_ui_starts_with_segoe() {
        assert_eq!(eager_ui_font_files()[0], "segoeui.ttf");
        assert!(eager_ui_font_files().contains(&"consola.ttf"));
        assert!(eager_ui_font_files().contains(&"times.ttf"));
        assert!(eager_ui_font_files().contains(&"georgia.ttf"));
    }

    #[test]
    fn windows_cjk_is_yahei_then_simsun() {
        assert_eq!(
            lazy_script_font_files(SystemFontScript::Cjk),
            &["msyh.ttc", "msyhbd.ttc", "simhei.ttf", "simsun.ttc"]
        );
    }
}
