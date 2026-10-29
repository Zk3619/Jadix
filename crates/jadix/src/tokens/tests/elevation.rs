use super::*;

#[test]
fn elevation_alphas_match_legacy_chrome() {
    assert!((shadow_rgba(Elevation::Xs)[3] - 0.06).abs() < 1e-6);
    assert!((shadow_rgba(Elevation::Sm)[3] - 0.08).abs() < 1e-6);
    assert!((shadow_rgba(Elevation::Md)[3] - 0.12).abs() < 1e-6);
    assert!((shadow_rgba(Elevation::Lg)[3] - 0.14).abs() < 1e-6);
    assert!((shadow_rgba(Elevation::Xl)[3] - 0.16).abs() < 1e-6);
    assert_eq!(shadow_rgba(Elevation::Md)[0], 0.0);
}

#[test]
fn popup_shadow_is_omnidirectional_md() {
    let el = crate::core::tree::Element::new().popup_shadow();
    assert_eq!(el.style.shadow_color, shadow_rgba(Elevation::Md));
    assert!((el.style.shadow_blur - Elevation::Md.blur()).abs() < 1e-6);
    assert_eq!(el.style.shadow_offset, [0.0, 0.0]);
    assert_eq!(el.style.shadow_spread, 0.0);
    assert!(!el.style.shadow_inset);
}

#[test]
fn overlay_scrim_clamps_alpha() {
    assert!((overlay_scrim(0.35)[3] - 0.35).abs() < 1e-6);
    assert_eq!(overlay_scrim(-1.0)[3], 0.0);
    assert_eq!(overlay_scrim(2.0)[3], 1.0);
}
