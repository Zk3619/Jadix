use super::*;

#[test]
fn android_prefers_roboto() {
    assert_eq!(system_font_dirs()[0], PathBuf::from("/system/fonts"));
    assert_eq!(eager_ui_font_files()[0], "Roboto-Regular.ttf");
}
