use super::*;
use crate::platform::os::harmonyos::ability::{
    install_ohos_content_rect, reset_ohos_ability_for_tests, OhosContentRect,
};

#[test]
fn apply_options_takes_ability_size_and_drops_chrome() {
    reset_ohos_ability_for_tests();
    install_ohos_content_rect(OhosContentRect::new(1080, 1920));
    let o = apply_window_options(WindowOptions::new());
    assert!(!o.decorations);
    assert_eq!(o.inner_width, Some(1080.0));
    assert_eq!(o.inner_height, Some(1920.0));
    reset_ohos_ability_for_tests();
}
