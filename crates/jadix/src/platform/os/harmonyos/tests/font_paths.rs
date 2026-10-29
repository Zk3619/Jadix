use super::*;

#[test]
fn ohos_prefers_harmonyos_sans() {
    assert_eq!(system_font_dirs()[0], PathBuf::from("/system/fonts"));
    assert_eq!(eager_ui_font_files()[0], "HarmonyOS_Sans.ttf");
    assert_eq!(
        lazy_script_font_files(SystemFontScript::Cjk)[0],
        "HarmonyOS_Sans_SC.ttf"
    );
}
