use super::*;

#[test]
fn chrome_translates_without_requiring_catalog_features() {
    let before = crate::chrome_locale();
    crate::set_chrome_locale("zh-CN");
    assert_eq!(text("Elements"), "元素");
    assert_eq!(text("Apply CSS"), "应用 CSS");
    crate::set_chrome_locale("en-US");
    assert_eq!(text("Elements"), "Elements");
    crate::set_chrome_locale(&before);
}
