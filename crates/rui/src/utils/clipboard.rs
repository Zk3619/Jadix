use clipboard_rs::{Clipboard, ClipboardContext};
use winit::keyboard::ModifiersState;

/// Windows/Linux 用 Ctrl，macOS 用 Cmd（Super）。
pub fn shortcut_modifiers(m: &ModifiersState) -> bool {
    #[cfg(target_os = "macos")]
    {
        m.super_key()
    }
    #[cfg(not(target_os = "macos"))]
    {
        m.control_key()
    }
}

/// 单行输入：粘贴内容去掉换行，避免插入多行。
pub fn sanitize_pasted_line(s: &str) -> String {
    s.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

pub fn set_text(text: &str) {
    if let Ok(ctx) = ClipboardContext::new() {
        let _ = ctx.set_text(text.to_string());
    }
}

pub fn get_text() -> Option<String> {
    let ctx = ClipboardContext::new().ok()?;
    ctx.get_text().ok()
}
