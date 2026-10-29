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
