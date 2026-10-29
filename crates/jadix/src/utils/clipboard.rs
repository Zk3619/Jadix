use winit::event::KeyEvent;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

/// Windows/Linux 用 Ctrl，macOS 用 Cmd（Super）。
pub fn shortcut_modifiers(m: &ModifiersState) -> bool {
    crate::platform::edit_shortcut_modifiers(m)
}

/// Edit shortcut letter under Ctrl/Cmd: prefer `logical_key`, fall back to `physical_key`
/// (some layouts omit `Key::Character` while the modifier is held).
pub fn edit_shortcut_letter(event: &KeyEvent) -> Option<char> {
    if let Key::Character(ch) = &event.logical_key {
        let mut it = ch.chars();
        if let (Some(c), None) = (it.next(), it.next()) {
            if c.is_ascii_alphabetic() {
                return Some(c.to_ascii_lowercase());
            }
        }
    }
    match event.physical_key {
        PhysicalKey::Code(KeyCode::KeyA) => Some('a'),
        PhysicalKey::Code(KeyCode::KeyC) => Some('c'),
        PhysicalKey::Code(KeyCode::KeyV) => Some('v'),
        PhysicalKey::Code(KeyCode::KeyX) => Some('x'),
        PhysicalKey::Code(KeyCode::KeyY) => Some('y'),
        PhysicalKey::Code(KeyCode::KeyZ) => Some('z'),
        _ => None,
    }
}

/// 单行输入：粘贴内容去掉换行，避免插入多行。
pub fn sanitize_pasted_line(s: &str) -> String {
    s.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

/// Writes system text when `native-clipboard` is enabled; otherwise does nothing.
pub fn set_text(text: &str) {
    crate::platform::clipboard_set_text(text);
}

/// Returns `None` when `native-clipboard` is disabled or the platform has no text.
pub fn get_text() -> Option<String> {
    crate::platform::clipboard_get_text()
}

/// MIME 类型常量（文本族）。`jadix_clipboard` 默认路径以纯文本为主；
/// 自定义 MIME 在无平台扩展时退化为 UTF-8 文本写入。
pub mod mime {
    pub const TEXT_PLAIN: &str = "text/plain";
    pub const TEXT_HTML: &str = "text/html";
    pub const TEXT_URI_LIST: &str = "text/uri-list";
}

/// 按 MIME 写入；未知类型仍走纯文本。
pub fn set_with_mime(mime_type: &str, data: &str) {
    let _ = mime_type;
    set_text(data);
}

/// 按 MIME 读取；当前实现仅返回系统剪贴板纯文本（若有）。
pub fn get_with_mime(mime_type: &str) -> Option<String> {
    let _ = mime_type;
    get_text()
}

/// 写入 HTML（以纯文本形式落到系统剪贴板；平台富文本后续加深）。
pub fn set_html(html: &str) {
    set_with_mime(mime::TEXT_HTML, html);
}

/// 便捷：复制多行文本（保留换行）。
pub fn set_multiline(text: &str) {
    set_text(text);
}

/// 便捷：读取并按行拆分（去掉 `\r`）。
pub fn get_lines() -> Option<Vec<String>> {
    let t = get_text()?;
    Some(
        t.replace('\r', "")
            .split('\n')
            .map(|s| s.to_string())
            .collect(),
    )
}

/// Ctrl/Cmd+Z, or the OS `Undo` key.
pub fn is_undo_shortcut(event: &KeyEvent, modifiers: &ModifiersState) -> bool {
    if matches!(event.logical_key, Key::Named(NamedKey::Undo)) {
        return true;
    }
    shortcut_modifiers(modifiers)
        && !modifiers.shift_key()
        && edit_shortcut_letter(event) == Some('z')
}

/// Ctrl/Cmd+Y, Ctrl/Cmd+Shift+Z, or the OS `Redo` key.
pub fn is_redo_shortcut(event: &KeyEvent, modifiers: &ModifiersState) -> bool {
    if matches!(event.logical_key, Key::Named(NamedKey::Redo)) {
        return true;
    }
    if !shortcut_modifiers(modifiers) {
        return false;
    }
    match edit_shortcut_letter(event) {
        Some('y') => !modifiers.shift_key(),
        Some('z') => modifiers.shift_key(),
        _ => false,
    }
}

/// Letter chords that `input_controller` swallows instead of inserting `z`/`y`.
/// Independent of the `text-undo` feature (empty stack and feature-off still consume).
pub(crate) fn consumes_undo_redo_letter(letter: Option<char>, modifiers: &ModifiersState) -> bool {
    if !shortcut_modifiers(modifiers) {
        return false;
    }
    match letter {
        Some('z') => true,
        Some('y') => !modifiers.shift_key(),
        _ => false,
    }
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
