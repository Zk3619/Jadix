use super::*;

#[test]
fn content_rect_roundtrip() {
    reset_ohos_ability_for_tests();
    assert!(!ohos_ability_installed());
    assert!(!ohos_content_rect().is_usable());

    install_ohos_content_rect(OhosContentRect::new(720, 1280));
    assert!(ohos_ability_installed());
    let r = ohos_content_rect();
    assert_eq!((r.width, r.height), (720, 1280));
    reset_ohos_ability_for_tests();
}
