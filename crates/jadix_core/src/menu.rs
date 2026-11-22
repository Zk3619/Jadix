//! Host-independent menu definitions; callbacks run on the owning UI thread.
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRole {
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    SelectAll,
    Quit,
}

impl MenuRole {
    /// Built-in command captions follow the application's locale, independently
    /// of the OS language. Unsupported locales fall back to English.
    pub fn label(self, locale: &str) -> &'static str {
        let tag = locale.to_ascii_lowercase().replace('_', "-");
        let mut parts = tag.split('-');
        let language = parts.next().unwrap_or_default();
        let subtags: Vec<_> = parts.take_while(|part| part.len() != 1).collect();
        let traditional = if subtags.contains(&"hans") {
            false
        } else {
            subtags
                .iter()
                .any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo"))
        };
        let labels = match language {
            "ja" => [
                "コピー",
                "切り取り",
                "貼り付け",
                "取り消す",
                "やり直す",
                "すべて選択",
                "終了",
            ],
            "zh" if traditional => ["複製", "剪下", "貼上", "還原", "重做", "全選", "結束"],
            "zh" => ["复制", "剪切", "粘贴", "撤销", "重做", "全选", "退出"],
            _ => ["Copy", "Cut", "Paste", "Undo", "Redo", "Select All", "Quit"],
        };
        labels[match self {
            Self::Copy => 0,
            Self::Cut => 1,
            Self::Paste => 2,
            Self::Undo => 3,
            Self::Redo => 4,
            Self::SelectAll => 5,
            Self::Quit => 6,
        }]
    }
}
/// Controlled snapshots: hosts never toggle the application's state implicitly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MenuState {
    #[default]
    Plain,
    Checked(bool),
    Radio {
        group: String,
        selected: bool,
    },
    Role(MenuRole),
    /// Native role whose caption is supplied by the shared locale policy.
    LocalizedRole(MenuRole),
}

/// Resolve only opt-in built-in captions. Explicit labels remain application
/// authored. Validation happens before mutation, including node/depth bounds.
pub fn localize(items: &mut [MenuItem], locale: &str) -> Result<(), String> {
    validate(items)?;
    let mut pending: Vec<_> = items.iter_mut().collect();
    while let Some(item) = pending.pop() {
        match item {
            MenuItem::Command {
                label,
                state: MenuState::LocalizedRole(role),
                ..
            } => {
                *label = role.label(locale).into();
            }
            MenuItem::Submenu { items, .. } => pending.extend(items.iter_mut()),
            _ => {}
        }
    }
    Ok(())
}

/// Validate before a host replaces its installed menu. Bounded even for callers
/// constructing the public enum directly; duplicate command IDs are rejected.
pub fn validate(items: &[MenuItem]) -> Result<(), String> {
    if items.len() > 1024 {
        return Err("menu exceeds node budget".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut selected = std::collections::HashSet::new();
    let mut pending: Vec<_> = items.iter().map(|i| (i, 0)).collect();
    let mut count = 0;
    let mut icon_bytes = 0;
    let text = |s: &str| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
    while let Some((item, depth)) = pending.pop() {
        count += 1;
        if count > 1024 || depth > 8 {
            return Err("menu exceeds node/depth budget".into());
        }
        if let Some(icon) = item.icon() {
            if icon.source.is_empty()
                || icon.source.len() > 1024 * 1024 - icon_bytes
                || icon.source.chars().any(char::is_control)
            {
                return Err("invalid menu icon source or menu exceeds icon source budget".into());
            }
            icon_bytes += icon.source.len();
        }
        match item {
            MenuItem::Command {
                id,
                label,
                accelerator,
                state,
                ..
            } => {
                if !text(id)
                    || !text(label)
                    || !ids.insert(id)
                    || accelerator
                        .as_ref()
                        .is_some_and(|s| !text(s) || s.len() > 64)
                {
                    return Err("invalid menu command, label or duplicate ID".into());
                }
                if let MenuState::Radio {
                    group,
                    selected: checked,
                } = state
                {
                    if !text(group) || (*checked && !selected.insert(group)) {
                        return Err("invalid radio group or multiple selected commands".into());
                    }
                }
            }
            MenuItem::Submenu { label, items, .. } => {
                if !text(label) {
                    return Err("invalid submenu label".into());
                }
                if count + pending.len() + items.len() > 1024 {
                    return Err("menu exceeds node budget".into());
                }
                pending.extend(items.iter().map(|i| (i, depth + 1)));
            }
            MenuItem::Separator => {}
        }
    }
    Ok(())
}

/// Image source for a menu item, using the same URI syntax as native images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuIcon {
    pub source: String,
    pub template: bool,
}

/// A command, nested submenu, or separator shared by native and terminal menus.
#[derive(Clone)]
pub enum MenuItem {
    Command {
        id: String,
        label: String,
        enabled: bool,
        /// Muda syntax, e.g. `CmdOrCtrl+Shift+S`.
        accelerator: Option<String>,
        state: MenuState,
        icon: Option<MenuIcon>,
        on_select: Rc<dyn Fn()>,
    },
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<MenuItem>,
        icon: Option<MenuIcon>,
    },
    Separator,
}

impl MenuItem {
    /// A system command with a caption that follows the application's locale.
    /// Use `command(...).role(...)` to retain an explicit caption instead.
    pub fn localized_role(
        id: impl Into<String>,
        role: MenuRole,
        on_select: impl Fn() + 'static,
    ) -> Self {
        let mut item = Self::command(id, role.label("en"), on_select);
        if let Self::Command { state, .. } = &mut item {
            *state = MenuState::LocalizedRole(role);
        }
        item
    }
    pub fn command(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self::Command {
            id: id.into(),
            label: label.into(),
            enabled: true,
            accelerator: None,
            state: MenuState::Plain,
            icon: None,
            on_select: Rc::new(on_select),
        }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<Self>) -> Self {
        Self::Submenu {
            label: label.into(),
            enabled: true,
            items,
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: MenuIcon) -> Self {
        match &mut self {
            Self::Command { icon: current, .. } | Self::Submenu { icon: current, .. } => {
                *current = Some(icon);
            }
            Self::Separator => {}
        }
        self
    }

    pub fn icon(&self) -> Option<&MenuIcon> {
        match self {
            Self::Command { icon, .. } | Self::Submenu { icon, .. } => icon.as_ref(),
            Self::Separator => None,
        }
    }

    pub fn enabled(mut self, value: bool) -> Self {
        match &mut self {
            Self::Command { enabled, .. } | Self::Submenu { enabled, .. } => *enabled = value,
            Self::Separator => {}
        }
        self
    }

    pub fn accelerator(mut self, value: impl Into<String>) -> Self {
        if let Self::Command { accelerator, .. } = &mut self {
            *accelerator = Some(value.into());
        }
        self
    }
    /// Use the same validated stroke for command routing and visible keycaps.
    pub fn key_chord(self, value: &crate::shortcut::KeyChord) -> Self {
        self.accelerator(value.to_string())
    }
    pub fn checked(mut self, checked: bool) -> Self {
        if let Self::Command { state, .. } = &mut self {
            *state = MenuState::Checked(checked);
        }
        self
    }
    pub fn radio(mut self, group: impl Into<String>, selected: bool) -> Self {
        if let Self::Command { state, .. } = &mut self {
            *state = MenuState::Radio {
                group: group.into(),
                selected,
            };
        }
        self
    }
    pub fn role(mut self, role: MenuRole) -> Self {
        if let Self::Command { state, .. } = &mut self {
            *state = MenuState::Role(role);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_roles_preserve_explicit_labels_callbacks_and_validation() {
        let called = Rc::new(std::cell::Cell::new(0));
        let observed = called.clone();
        let mut items = vec![MenuItem::submenu(
            "应用菜单",
            vec![
                MenuItem::localized_role("copy", MenuRole::Copy, move || {
                    observed.set(observed.get() + 1)
                })
                .accelerator("Ctrl+C"),
                MenuItem::command("paste", "Custom Paste", || {}).role(MenuRole::Paste),
            ],
        )];
        for (locale, expected) in [
            ("zh_CN", "复制"),
            ("zh-TW", "複製"),
            ("zh-HK", "複製"),
            ("zh-MO", "複製"),
            ("zh-Hans-TW", "复制"),
            ("ja-JP", "コピー"),
            ("fr-FR", "Copy"),
            ("zh-u-rg-twzzzz", "复制"),
        ] {
            localize(&mut items, locale).unwrap();
            let MenuItem::Submenu {
                items: children, ..
            } = &items[0]
            else {
                unreachable!()
            };
            let MenuItem::Command {
                label,
                accelerator,
                on_select,
                ..
            } = &children[0]
            else {
                unreachable!()
            };
            assert_eq!(label, expected);
            assert_eq!(accelerator.as_deref(), Some("Ctrl+C"));
            on_select();
            assert!(
                matches!(&children[1], MenuItem::Command { label, .. } if label == "Custom Paste")
            );
        }
        assert_eq!(called.get(), 8);
        items.push(MenuItem::command("invalid", "", || {}));
        assert!(localize(&mut items, "ja").is_err());
        let MenuItem::Submenu { items, .. } = &items[0] else {
            unreachable!()
        };
        assert!(matches!(&items[0], MenuItem::Command { label, .. } if label == "复制"));
    }
}
