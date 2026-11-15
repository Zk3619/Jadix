#![cfg(feature = "ohos")]
use jadix_native::{environment::*, ohos_environment::OhosConfiguration};

#[test]
fn arkui_configuration_maps_physical_os_preferences_and_lifecycle_without_guessing_unknowns() {
    let value = OhosConfiguration::from_json(br#"{"language":"zh_CN","colorMode":0,"fontSizeScale":1.5,"direction":1,"highContrast":true,"screenDensity":480}"#, Lifecycle::Background).unwrap();
    assert_eq!(value.platform.unwrap().os, Some(OperatingSystem::Ohos));
    assert_eq!(value.locale.locale, "zh-CN");
    assert_eq!(value.color_scheme, Some(ColorScheme::Dark));
    assert_eq!(value.layout_direction, Some(LayoutDirection::RightToLeft));
    assert_eq!(value.font_scale, 1.5);
    assert_eq!(value.high_contrast, Some(true));
    assert_eq!(value.lifecycle, Lifecycle::Background);
    let ltr = OhosConfiguration::from_json(br#"{"direction":0}"#, Lifecycle::Active).unwrap();
    assert_eq!(ltr.layout_direction, Some(LayoutDirection::LeftToRight));
    let unknown =
        OhosConfiguration::from_json(br#"{"colorMode":-1,"direction":9}"#, Lifecycle::Active)
            .unwrap();
    assert_eq!(unknown.color_scheme, None);
    assert_eq!(unknown.layout_direction, None);
    assert_eq!(unknown.high_contrast, None);
    assert_eq!(unknown.reduced_motion, None);
    for json in [
        br#"{"fontSizeScale":0}"#.as_slice(),
        br#"{"language":"../../bad"}"#,
        br#"{"fontSizeScale":"NaN"}"#,
    ] {
        assert!(OhosConfiguration::from_json(json, Lifecycle::Active).is_err());
    }
    assert!(OhosConfiguration::from_json(
        &vec![b' '; EnvironmentSnapshot::MAX_JSON_BYTES + 1],
        Lifecycle::Active
    )
    .is_err());
}
