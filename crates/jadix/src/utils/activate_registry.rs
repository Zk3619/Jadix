//! 菜单 / Command 等「高亮项 Enter 激活」回调注册表（WP-SI.1）。
//!
//! 调用方用独立 `id`（菜单实例键 / command 根 id）分区；同一 id 在每帧 Content
//! 重建前 [`reset`]，再按项序 [`push`]。键盘路径用 [`take`] 取回调。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static ACTIVATES: RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>> = RefCell::new(HashMap::new());
}

/// 清空并准备接收本帧的 activate 列表（插入空 `Vec`）。
pub fn reset(id: &str) {
    ACTIVATES.with(|m| {
        m.borrow_mut().insert(id.to_string(), Vec::new());
    });
}

/// 追加一项激活回调；返回插入前的下标（与 [`count`] 在 push 前一致）。
pub fn push(id: &str, f: Rc<dyn Fn()>) {
    ACTIVATES.with(|m| {
        m.borrow_mut().entry(id.to_string()).or_default().push(f);
    });
}

/// 当前已注册回调数。
pub fn count(id: &str) -> usize {
    ACTIVATES.with(|m| m.borrow().get(id).map(|v| v.len()).unwrap_or(0))
}

/// 按索引取回调（克隆 `Rc`）；越界返回 `None`。
pub fn take(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    ACTIVATES.with(|m| m.borrow().get(id).and_then(|v| v.get(ix).cloned()))
}

#[cfg(test)]
#[path = "tests/activate_registry.rs"]
mod tests;
