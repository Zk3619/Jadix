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
