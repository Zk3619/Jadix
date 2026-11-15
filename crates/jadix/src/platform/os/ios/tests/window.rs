use super::*;

#[test]
fn decorate_keeps_title() {
    let o = WindowOptions::new().title("iOS");
    let attrs = decorate_window_attributes(o.to_window_attributes(), &o, false);
    assert_eq!(attrs.title, "iOS");
}

#[test]
fn maps_window_orientation_policy_to_uikit() {
    use winit::platform::ios::ValidOrientations;

    for (policy, expected) in [
        (
            crate::app::ScreenOrientations::LandscapeAndPortrait,
            ValidOrientations::LandscapeAndPortrait,
        ),
        (
            crate::app::ScreenOrientations::Landscape,
            ValidOrientations::Landscape,
        ),
        (
            crate::app::ScreenOrientations::Portrait,
            ValidOrientations::Portrait,
        ),
    ] {
        assert_eq!(ios_valid_orientations(policy), expected);
    }
}
