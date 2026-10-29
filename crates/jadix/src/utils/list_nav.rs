//! 打开列表的 ↑↓ / Home / End 导航（WP-SI.4）。
//!
//! 不含菜单 submenu ←→。Enter / Esc 由调用方处理（激活 / 关闭语义因组件而异）。
//! Command 过滤框可对 [`is_list_nav_key`] 做子集判断（可不含 Home/End）。

use winit::keyboard::{Key, NamedKey};

use crate::utils::roving::{roving_edge, roving_move};

/// ↑↓ / Home / End：返回新的高亮下标；其它键 `None`。
pub fn list_nav_move(key: &Key, count: usize, active: usize) -> Option<usize> {
    match key {
        Key::Named(NamedKey::ArrowDown) => roving_move(count, active, 1),
        Key::Named(NamedKey::ArrowUp) => roving_move(count, active, -1),
        Key::Named(NamedKey::Home) => roving_edge(count, false),
        Key::Named(NamedKey::End) => roving_edge(count, true),
        _ => None,
    }
}

/// 仅 ↑↓（Command 过滤框常用；Home/End 留给文本编辑）。
pub fn list_nav_move_arrows(key: &Key, count: usize, active: usize) -> Option<usize> {
    match key {
        Key::Named(NamedKey::ArrowDown) => roving_move(count, active, 1),
        Key::Named(NamedKey::ArrowUp) => roving_move(count, active, -1),
        _ => None,
    }
}

/// 列表导航相关键（含 Enter / Escape）；**不含** Home/End（留给输入框）。
pub fn is_list_nav_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(NamedKey::ArrowUp)
            | Key::Named(NamedKey::ArrowDown)
            | Key::Named(NamedKey::Enter)
            | Key::Named(NamedKey::Escape)
    )
}

pub fn is_list_activate_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Enter))
}

pub fn is_list_dismiss_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Escape))
}

#[cfg(test)]
#[path = "tests/list_nav.rs"]
mod tests;
