use super::*;

#[test]
fn unix_dirs_keep_usr_share_and_local() {
    let dirs = system_font_dirs();
    assert_eq!(
        dirs.iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>(),
        vec![
            "/usr/share/fonts",
            "/usr/local/share/fonts",
            "/usr/share/fonts/truetype",
            "/usr/share/fonts/opentype",
        ]
    );
}

#[test]
fn unix_eager_ui_is_dejavu() {
    assert_eq!(
        eager_ui_font_files(),
        &[
            "truetype/dejavu/DejaVuSans.ttf",
            "truetype/dejavu/DejaVuSans-Bold.ttf",
            "truetype/dejavu/DejaVuSansMono.ttf",
            "truetype/dejavu/DejaVuSerif.ttf",
            "truetype/liberation/LiberationSerif-Regular.ttf",
            "truetype/liberation2/LiberationSerif-Regular.ttf",
            "liberation/LiberationSerif-Regular.ttf",
            "truetype/noto/NotoSerif-Regular.ttf",
            "noto/NotoSerif-Regular.ttf",
        ]
    );
}

#[test]
fn unix_cjk_prefers_noto_then_wenquanyi() {
    assert_eq!(
        lazy_script_font_files(SystemFontScript::Cjk),
        &[
            "opentype/noto/NotoSansCJK-Regular.ttc",
            "noto-cjk/NotoSansCJK-Regular.ttc",
            "truetype/noto/NotoSansCJK-Regular.ttc",
            "truetype/wqy/wqy-microhei.ttc",
        ]
    );
}
