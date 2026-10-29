use super::*;
use crate::app::glass_params::tint_to_accent_abgr;

#[test]
fn accent_struct_layout() {
    assert_eq!(std::mem::size_of::<AccentPolicy>(), 16);
    assert_eq!(WCA_ACCENT_POLICY, 19);
    assert_eq!(ACCENT_ENABLE_BLURBEHIND, 3);
    assert_eq!(ACCENT_ENABLE_ACRYLICBLURBEHIND, 4);
    assert_eq!(BUILD_WIN11_SYSTEM_BACKDROP, 22621);
    assert_eq!(tint_to_accent_abgr([0x64, 0xff, 0xff, 0xff]), 0x64FFFFFF);
}

#[test]
fn win11_22h2_skips_accent_for_system_acrylic() {
    let o = crate::app::WindowOptions::new()
        .blur(true)
        .backdrop(crate::app::WindowBackdrop::Acrylic);
    assert!(!o.wants_custom_dcomp());
    // apply_accent_policy itself is HWND-only; gate is the boolean above + build.
    assert!(nt_build_number() > 0);
}
