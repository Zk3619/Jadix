//! 菜单 / Command 等「高亮项 Enter 激活」回调注册表（WP-SI.1）。
//!
//! 调用方用独立 `id`（菜单实例键 / command 根 id）分区；同一 id 在每帧 Content
//! 重建前 [`reset`]，再按项序 [`push`]。键盘路径用 [`take`] 取回调。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::roving::roving_move_skipping;

struct Entry {
    callback: Rc<dyn Fn()>,
    enabled: bool,
    bounds: Option<[f32; 4]>,
}

thread_local! {
    static ACTIVATES: RefCell<HashMap<String, Vec<Entry>>> = RefCell::new(HashMap::new());
}

/// 清空并准备接收本帧的 activate 列表（插入空 `Vec`）。
pub fn reset(id: &str) {
    ACTIVATES.with(|m| {
        m.borrow_mut().insert(id.to_string(), Vec::new());
    });
}

/// 追加一项激活回调；返回插入前的下标（与 [`count`] 在 push 前一致）。
pub fn push(id: &str, f: Rc<dyn Fn()>) {
    push_enabled(id, f, true);
}

/// Register an item without changing its stable row index when it is disabled.
pub fn push_enabled(id: &str, callback: Rc<dyn Fn()>, enabled: bool) {
    ACTIVATES.with(|m| {
        m.borrow_mut()
            .entry(id.to_string())
            .or_default()
            .push(Entry {
                callback,
                enabled,
                bounds: None,
            });
    });
}

/// 当前已注册回调数。
pub fn count(id: &str) -> usize {
    ACTIVATES.with(|m| m.borrow().get(id).map(|v| v.len()).unwrap_or(0))
}

/// 按索引取回调（克隆 `Rc`）；越界返回 `None`。
pub fn take(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    ACTIVATES.with(|m| {
        m.borrow()
            .get(id)
            .and_then(|v| v.get(ix))
            .filter(|entry| entry.enabled)
            .map(|entry| entry.callback.clone())
    })
}

/// Home/End target among enabled items; an all-disabled list has no target.
pub fn enabled_edge(id: &str, to_end: bool) -> Option<usize> {
    ACTIVATES.with(|m| {
        let map = m.borrow();
        let entries = map.get(id)?;
        if to_end {
            entries.iter().rposition(|entry| entry.enabled)
        } else {
            entries.iter().position(|entry| entry.enabled)
        }
    })
}

/// Arrow navigation wraps among enabled items. An absent selection enters at an edge.
pub fn move_enabled(id: &str, current: usize, delta: i32) -> Option<usize> {
    if current >= count(id) {
        return enabled_edge(id, delta < 0);
    }
    ACTIVATES.with(|m| {
        let map = m.borrow();
        let entries = map.get(id)?;
        roving_move_skipping(entries.len(), current, delta, |ix| !entries[ix].enabled)
    })
}

/// Publish real row geometry without subscribing menu construction to layout.
pub fn update_bounds(id: &str, index: usize, bounds: [f32; 4]) {
    ACTIVATES.with(|m| {
        if let Some(entry) = m.borrow_mut().get_mut(id).and_then(|v| v.get_mut(index)) {
            entry.bounds =
                (bounds.iter().all(|v| v.is_finite()) && bounds[3] > 0.0).then_some(bounds);
        }
    });
}

/// Page navigation uses measured row centers, skips disabled rows and never wraps.
pub fn page_enabled(id: &str, current: usize, viewport_h: f32, down: bool) -> Option<usize> {
    ACTIVATES.with(|m| {
        let map = m.borrow();
        let entries = map.get(id)?;
        let edge = || {
            if down {
                entries.iter().position(|e| e.enabled)
            } else {
                entries.iter().rposition(|e| e.enabled)
            }
        };
        let Some(current_entry) = entries.get(current) else {
            return edge();
        };
        let center = |b: [f32; 4]| b[1] + b[3] * 0.5;
        let target = current_entry
            .bounds
            .filter(|_| viewport_h.is_finite() && viewport_h > 0.0)
            .map(|b| center(b) + if down { viewport_h } else { -viewport_h });
        let candidates = entries
            .iter()
            .enumerate()
            .filter(|(i, e)| e.enabled && if down { *i > current } else { *i < current });
        if let Some(target) = target {
            if let Some((i, _)) = candidates
                .clone()
                .filter_map(|(i, e)| e.bounds.map(|b| (i, (center(b) - target).abs())))
                .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                return Some(i);
            }
        }
        if down {
            candidates.map(|(i, _)| i).next()
        } else {
            candidates.map(|(i, _)| i).next_back()
        }
        .or_else(|| current_entry.enabled.then_some(current))
    })
}

#[cfg(test)]
#[path = "tests/activate_registry.rs"]
mod tests;
