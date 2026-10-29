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
