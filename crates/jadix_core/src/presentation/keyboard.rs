//! Focused hardware keyboard input. Text editors and hosted surfaces retain
//! their platform input pipeline; subscribing does not intercept IME text.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
    pub caps_lock: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyPhase { Down, Up }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyEvent {
    pub phase: KeyPhase,
    /// Layout-aware key (for example "a", "A", "ArrowLeft", "Enter").
    pub key: String,
    /// USB HID keyboard usage, independent of the active keyboard layout.
    pub code: u16,
    pub modifiers: KeyModifiers,
    pub repeat: bool,
}
impl KeyEvent {
    pub fn is_valid(&self) -> bool {
        !self.key.is_empty() && self.key.len() <= 128 && !self.key.contains('\0')
            && (4..=231).contains(&self.code)
    }
}

/// Converts an XKB hardware keycode (evdev code + 8 on X11 and Wayland) to
/// the USB keyboard usage carried by presentation events. Unknown and media
/// codes deliberately return None rather than pretending to be a letter.
pub fn xkb_keycode_to_hid(code: u32) -> Option<u16> {
    let usage = match code.checked_sub(8)? {
        1 => 41, 2..=10 => (code - 8 + 28) as u16, 11 => 39,
        12 => 45, 13 => 46, 14 => 42, 15 => 43,
        16..=25 => [20,26,8,21,23,28,24,12,18,19][(code-24) as usize],
        26 => 47, 27 => 48, 28 => 40, 29 => 224,
        30..=38 => [4,22,7,9,10,11,13,14,15][(code-38) as usize],
        39 => 51, 40 => 52, 41 => 53, 42 => 225, 43 => 49,
        44..=50 => [29,27,6,25,5,17,16][(code-52) as usize],
        51 => 54, 52 => 55, 53 => 56, 54 => 229, 55 => 85,
        56 => 226, 57 => 44, 58 => 57,
        59..=68 => (code - 8 - 1) as u16,
        69 => 83, 70 => 71,
        71 => 95, 72 => 96, 73 => 97, 74 => 86,
        75 => 92, 76 => 93, 77 => 94, 78 => 87,
        79 => 89, 80 => 90, 81 => 91, 82 => 98, 83 => 99,
        86 => 100, 87 => 68, 88 => 69, 96 => 88, 97 => 228,
        98 => 84, 99 => 70, 100 => 230,
        102 => 74, 103 => 82, 104 => 75, 105 => 80, 106 => 79,
        107 => 77, 108 => 81, 109 => 78, 110 => 73, 111 => 76,
        119 => 72, 125 => 227, 126 => 231, 127 => 101,
        183..=194 => (code - 8 - 79) as u16,
        _ => return None,
    };
    Some(usage)
}

/// Convert the set-1 scan code and E0 flag in a Windows keyboard message.
/// Extended navigation keys must not be confused with the numeric keypad.
pub fn windows_scancode_to_hid(scan: u8, extended: bool) -> Option<u16> {
    if !extended {
        return match scan {
            1..=83 | 86..=88 => xkb_keycode_to_hid(u32::from(scan) + 8),
            _ => None,
        };
    }
    Some(match scan {
        0x1c => 88, 0x1d => 228, 0x35 => 84, 0x37 => 70, 0x38 => 230,
        0x47 => 74, 0x48 => 82, 0x49 => 75, 0x4b => 80, 0x4d => 79,
        0x4f => 77, 0x50 => 81, 0x51 => 78, 0x52 => 73, 0x53 => 76,
        0x5b => 227, 0x5c => 231, 0x5d => 101,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyShortcut {
    pub id: String,
    /// Layout-aware key. Special keys use the same names as KeyEvent.
    pub key: String,
    #[serde(default)]
    pub modifiers: KeyModifiers,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyboardPolicy {
    /// Enables focus on generic containers; native controls keep their own policy.
    pub focusable: bool,
    pub key_down: bool,
    pub key_up: bool,
    pub shortcuts: Vec<KeyShortcut>,
}
impl KeyboardPolicy {
    pub fn is_valid(&self) -> bool {
        self.shortcuts.len() <= 64 && self.shortcuts.iter().enumerate().all(|(index, item)| {
            !item.id.is_empty() && item.id.len() <= 128 && !item.id.chars().any(char::is_control)
                && !item.key.is_empty() && item.key.len() <= 128 && !item.key.chars().any(char::is_control)
                && self.shortcuts[..index].iter().all(|prior| prior.id != item.id
                    && (prior.key != item.key || prior.modifiers != item.modifiers))
        })
    }
    pub fn accepts(&self, event: &KeyEvent) -> bool {
        event.is_valid() && match event.phase { KeyPhase::Down => self.key_down, KeyPhase::Up => self.key_up }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_extended_keys_keep_physical_identity() {
        for (scan, normal, extended) in [(0x1c,40,88),(0x1d,224,228),(0x38,226,230),(0x48,96,82),(0x53,99,76)] {
            assert_eq!(windows_scancode_to_hid(scan, false), Some(normal));
            assert_eq!(windows_scancode_to_hid(scan, true), Some(extended));
        }
        assert_eq!(windows_scancode_to_hid(0x1e, false), Some(4));
        assert_eq!(windows_scancode_to_hid(0x1e, true), None);
        assert_eq!(windows_scancode_to_hid(0, false), None);
    }
    #[test]
    fn xkb_physical_usage_is_independent_of_key_layout() {
        for (xkb, hid) in [(9,41),(10,30),(19,39),(24,20),(38,4),(52,29),(67,58),(96,69),(113,80),(133,227)] {
            assert_eq!(xkb_keycode_to_hid(xkb), Some(hid));
        }
        assert_eq!(xkb_keycode_to_hid(0), None);
        assert_eq!(xkb_keycode_to_hid(250), None);
    }
    #[test]
    fn bounded_policy_rejects_ambiguous_shortcuts() {
        let shortcut = KeyShortcut { id: "save".into(), key: "s".into(), modifiers: KeyModifiers { meta: true, ..Default::default() } };
        let mut policy = KeyboardPolicy { shortcuts: vec![shortcut.clone()], ..Default::default() };
        assert!(policy.is_valid());
        policy.shortcuts.push(KeyShortcut { id: "other".into(), ..shortcut });
        assert!(!policy.is_valid());
        assert!(serde_json::from_str::<KeyboardPolicy>(r#"{"unknown":true}"#).is_err());
    }
    #[test]
    fn phases_are_opt_in_and_layout_key_is_independent_from_usage() {
        let mut event = KeyEvent { phase: KeyPhase::Down, key: "Ж".into(), code: 4, modifiers: Default::default(), repeat: false };
        let policy = KeyboardPolicy { key_down: true, ..Default::default() };
        assert!(policy.accepts(&event));
        event.phase = KeyPhase::Up;
        assert!(!policy.accepts(&event));
        event.code = 0;
        assert!(!event.is_valid());
    }
}
