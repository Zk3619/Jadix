//! Shared accelerator syntax and keycap labels, independent of an input backend.
//!
//! Characters require Ctrl, Alt or Super; F1–F24 may stand alone. Chord sequences,
//! keyboard-layout translation, dispatch scopes and event consumption belong to hosts.
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutModifier {
    Control,
    Alt,
    Shift,
    Super,
    CommandOrControl,
}

impl ShortcutModifier {
    /// Resolve the portable command modifier for the current host.
    pub fn resolved(self) -> Self {
        match self {
            Self::CommandOrControl if cfg!(any(target_os = "macos", target_os = "ios")) => Self::Super,
            Self::CommandOrControl => Self::Control,
            value => value,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Control => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Super => "Super",
            Self::CommandOrControl => "CmdOrCtrl",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    Character(char),
    Function(u8),
}

/// One validated stroke. Keeps the bound symbol and modifiers for display;
/// hosts must not replace them with a keyboard mapper's transformed event.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    modifiers: Vec<ShortcutModifier>,
    key: ShortcutKey,
}

impl KeyChord {
    pub fn modifiers(&self) -> &[ShortcutModifier] {
        &self.modifiers
    }

    pub fn key(&self) -> ShortcutKey {
        self.key
    }

    /// Labels in binding order. In particular, `Alt+Shift+=` retains three caps.
    pub fn keycaps(&self) -> Vec<String> {
        let mut labels: Vec<_> = self
            .modifiers
            .iter()
            .map(|modifier| {
                match modifier.resolved() {
                    ShortcutModifier::Super if cfg!(any(target_os = "macos", target_os = "ios")) => "Cmd",
                    value => value.name(),
                }
                .to_string()
            })
            .collect();
        labels.push(match self.key {
            ShortcutKey::Character(c) => c.to_ascii_uppercase().to_string(),
            ShortcutKey::Function(n) => format!("F{n}"),
        });
        labels
    }
}

impl FromStr for KeyChord {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        const INVALID: &str = "invalid accelerator (expected modifiers+character or F1–F24)";
        if value.is_empty()
            || value.len() > 64
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
        {
            return Err(INVALID);
        }
        let mut parts = value.split('+').peekable();
        let mut modifiers = Vec::new();
        let key = loop {
            let part = parts.next().ok_or(INVALID)?;
            if parts.peek().is_none() {
                break part.to_ascii_lowercase();
            }
            let modifier = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ShortcutModifier::Control,
                "alt" | "option" => ShortcutModifier::Alt,
                "shift" => ShortcutModifier::Shift,
                "super" | "cmd" | "command" | "meta" => ShortcutModifier::Super,
                "cmdorctrl" => ShortcutModifier::CommandOrControl,
                _ => return Err(INVALID),
            };
            if modifiers
                .iter()
                .any(|m: &ShortcutModifier| m.resolved() == modifier.resolved())
            {
                return Err(INVALID);
            }
            modifiers.push(modifier);
        };
        let key = if key.starts_with('f') && key.len() > 1 {
            let n: u8 = key[1..].parse().map_err(|_| INVALID)?;
            if !(1..=24).contains(&n) {
                return Err(INVALID);
            }
            ShortcutKey::Function(n)
        } else {
            let mut chars = key.chars();
            let c = chars.next().ok_or(INVALID)?;
            if chars.next().is_some() || !modifiers.iter().any(|m| *m != ShortcutModifier::Shift) {
                return Err(INVALID);
            }
            ShortcutKey::Character(c)
        };
        Ok(Self { modifiers, key })
    }
}

impl fmt::Display for KeyChord {
    /// Canonical accelerator text accepted by shared menus and command routers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in &self.modifiers {
            write!(f, "{}+", modifier.name())?;
        }
        match self.key {
            ShortcutKey::Character(c) => write!(f, "{}", c.to_ascii_uppercase()),
            ShortcutKey::Function(n) => write!(f, "F{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_symbols_round_trip_without_losing_shift_or_portable_modifiers() {
        for (source, canonical, labels) in [
            ("option+shift+=", "Alt+Shift+=", vec!["Alt", "Shift", "="]),
            ("Alt+Shift+-", "Alt+Shift+-", vec!["Alt", "Shift", "-"]),
            (
                "control+shift+4",
                "Ctrl+Shift+4",
                vec!["Ctrl", "Shift", "4"],
            ),
            ("ctrl+é", "Ctrl+é", vec!["Ctrl", "é"]),
            ("f24", "F24", vec!["F24"]),
        ] {
            let chord: KeyChord = source.parse().unwrap();
            assert_eq!(chord.to_string(), canonical);
            assert_eq!(chord.keycaps(), labels);
            assert_eq!(canonical.parse::<KeyChord>().unwrap(), chord);
        }
        let portable: KeyChord = "cmdorctrl+s".parse().unwrap();
        assert_eq!(portable.to_string(), "CmdOrCtrl+S");
        assert_eq!(portable.modifiers(), &[ShortcutModifier::CommandOrControl]);
        assert_eq!(
            portable.keycaps(),
            [
                if cfg!(any(target_os = "macos", target_os = "ios")) {
                    "Cmd"
                } else {
                    "Ctrl"
                },
                "S"
            ]
        );
        for invalid in [
            "",
            "S",
            "Shift+S",
            "Ctrl+",
            "Ctrl++",
            "Ctrl+Enter",
            "Ctrl+Ctrl+S",
            "Cmd+Meta+K",
            "Alt+Option+S",
            "F0",
            "F25",
            "Ctrl+foo",
            "Ctrl+\n",
            " Ctrl+S",
        ] {
            assert!(invalid.parse::<KeyChord>().is_err(), "{invalid:?}");
        }
        let duplicate = if cfg!(any(target_os = "macos", target_os = "ios")) {
            "Super+CmdOrCtrl+S"
        } else {
            "Ctrl+CmdOrCtrl+S"
        };
        assert!(duplicate.parse::<KeyChord>().is_err());
        assert!(format!("{}S", "Ctrl+".repeat(14))
            .parse::<KeyChord>()
            .is_err());
    }
}
