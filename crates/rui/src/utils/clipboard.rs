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

/// MIME 类型常量（文本族）。原生 `clipboard-rs` 默认路径以纯文本为主；
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
