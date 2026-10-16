//! Process-wide root font-size: keep this in an integration binary so it cannot
//! race `--lib` tests that read `1rem`.

use std::sync::Mutex;

use jadix::layout::dimension::IntoF32;
use jadix::{
    root_font_size_px, set_root_font_size, set_text_defaults, text_defaults, TextDefaults,
};

static ROOT_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn set_text_defaults_overrides_size_and_weight() {
    let _g = ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = text_defaults();
    set_text_defaults(TextDefaults {
        family: prev.family.clone(),
        size_px: 18.0,
        weight: 500,
        synthetic_bold: false,
    });
    let restored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let d = text_defaults();
        assert_eq!(d.size_px, 18.0);
        assert_eq!(d.weight, 500);
        assert!(!d.synthetic_bold);
        assert!((jadix::css_root_font_size_px() - 18.0).abs() < 1e-6);
    }));
    set_text_defaults(prev);
    restored.unwrap();
}

#[test]
fn set_root_font_size_updates_defaults_and_rem_slot() {
    let _g = ROOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = text_defaults();
    set_root_font_size(22.0);
    let restored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!((text_defaults().size_px - 22.0).abs() < 1e-6);
        assert!((root_font_size_px() - 22.0).abs() < 1e-6);
        assert!((IntoF32::into_f32("1rem") - 22.0).abs() < 1e-6);
        assert!((IntoF32::into_f32(jadix::rem(1.0)) - 22.0).abs() < 1e-6);
    }));
    set_text_defaults(prev);
    restored.unwrap();
}
