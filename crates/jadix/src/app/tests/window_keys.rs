use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

#[test]
fn fullscreen_shortcuts_follow_platform_and_modifier_contracts() {
    let none = ModifiersState::empty();
    let f11 = Key::Named(NamedKey::F11);
    let enter = Key::Named(NamedKey::Enter);
    let escape = Key::Named(NamedKey::Escape);
    for windows in [true, false] {
        assert_eq!(
            fullscreen_action(&f11, none, false, windows, false),
            Some(true)
        );
        assert_eq!(
            fullscreen_action(&f11, none, false, windows, true),
            Some(false)
        );
        assert_eq!(
            fullscreen_action(&f11, ModifiersState::SHIFT, false, windows, false),
            None
        );
    }
    assert_eq!(
        fullscreen_action(&enter, ModifiersState::ALT, false, true, false),
        Some(true)
    );
    assert_eq!(
        fullscreen_action(&enter, ModifiersState::ALT, false, false, false),
        None
    );
    assert_eq!(fullscreen_action(&f11, none, true, false, false), None);
    let mac = ModifiersState::CONTROL | ModifiersState::META;
    assert_eq!(
        fullscreen_action(&Key::Character("f".into()), mac, true, false, false),
        Some(true)
    );
    assert_eq!(
        fullscreen_action(&Key::Character("F".into()), mac, true, false, true),
        Some(false)
    );
    assert_eq!(
        fullscreen_action(&escape, none, true, false, true),
        Some(false)
    );
    assert_eq!(fullscreen_action(&escape, none, false, true, false), None);
    assert_eq!(
        fullscreen_action(
            &Key::Character("f".into()),
            ModifiersState::CONTROL,
            true,
            false,
            false
        ),
        None
    );
}
