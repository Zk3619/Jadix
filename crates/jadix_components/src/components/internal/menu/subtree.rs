//! 嵌套菜单子树生命周期。
//!
//! DropdownMenu / ContextMenu 共用：关子菜单、`open_or_remount`、子序号、打开探测。
//! 键盘绑定仍可由产品层调用本模块 API；active 信号后缀由调用方传入（`dd_active` / `ctx_active`）。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::reactivity::{use_signal, Signal};
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::utils::roving::roving_edge;

thread_local! {
    static SUB_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

/// 关闭 `parent_id` 下全部 `__subN`（最多 16）。
pub fn close_open_subs(parent_id: &str) {
    close_open_subs_except(parent_id, None);
}

/// 关闭子菜单，可保留 `keep_sub_id`。
pub fn close_open_subs_except(parent_id: &str, keep_sub_id: Option<&str>) {
    for i in 0..16 {
        let sid = format!("{parent_id}__sub{i}");
        if keep_sub_id.is_some_and(|k| k == sid) {
            continue;
        }
        let key = format!("{sid}__open");
        let sig = use_signal(key.as_str(), || false);
        if sig.peek() {
            sig.set(false);
        }
    }
}

/// 第一个仍打开的嵌套子菜单 id。
pub fn open_sub_id(parent_id: &str) -> Option<String> {
    for i in 0..16 {
        let sid = format!("{parent_id}__sub{i}");
        let open = use_signal(format!("{sid}__open").as_str(), || false);
        if open.peek() {
            return Some(sid);
        }
    }
    None
}

pub fn reset_sub_seq(parent_id: &str) {
    SUB_SEQ.with(|m| {
        m.borrow_mut().insert(parent_id.to_string(), 0);
    });
}

pub fn next_sub_seq(parent_id: &str) -> usize {
    SUB_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(parent_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

/// `active_slot` is in the form of `"dd_active"` / `"ctx_active"` → signal key `{sid}__{active_slot}`.
pub fn focus_submenu_first_item(sid: &str, active_slot: &str, activate_count: usize) {
    let active = use_signal(format!("{sid}__{active_slot}").as_str(), || usize::MAX);
    let next = if activate_count == 0 {
        0
    } else if let Some(i) = roving_edge(activate_count, false) {
        i
    } else {
        0
    };
    set_menu_active(active, next);
}

pub fn close_open_submenu_keep_parent_highlight(sid: &str, active_slot: &str) {
    let open = use_signal(format!("{sid}__open").as_str(), || false);
    if open.peek() {
        open.set(false);
    }
    use_signal(format!("{sid}__{active_slot}").as_str(), || usize::MAX).set_silent(usize::MAX);
}

pub fn set_menu_active(active: Signal<usize>, next: usize) {
    if active.peek() != next {
        active.set_silent(next);
        mark_hover_paint_dirty();
    }
}

/// 打开或强制 remount 子菜单面板。
///
/// `refresh_hover`: 指针开可 invalidate；**键盘必须 `false`**。
pub fn open_or_remount_sub(sub_id: &str, sub_open: Signal<bool>) {
    open_or_remount_sub_ex(sub_id, sub_open, true);
}

pub fn open_or_remount_sub_ex(sub_id: &str, sub_open: Signal<bool>, refresh_hover: bool) {
    let bump = use_signal(format!("{sub_id}__remount").as_str(), || 0u32);
    let was_open = sub_open.peek();
    if !was_open {
        sub_open.set(true);
    }
    if !sub_open.peek() {
        log::debug!(
            target: "jadix::menu_sub",
            "open_or_remount REJECT id={sub_id} was_open={was_open} now=false"
        );
        return;
    }
    if !was_open && refresh_hover {
        crate::authoring::widgets::hit::invalidate_hover_handlers();
    }
    bump.set(bump.peek().wrapping_add(1));
    crate::app::runtime::request_redraw();
    crate::app::runtime::try_hoist_portals_now();
    log::debug!(
        target: "jadix::menu_sub",
        "open_or_remount id={sub_id} was_open={was_open} now=true remount={} refresh_hover={refresh_hover}",
        bump.peek()
    );
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn close_open_subs_clears_open_flags() {
        let (_o, ()) = create_root(|| {
            let parent = "si3_parent";
            let sid = format!("{parent}__sub0");
            let open = use_signal(format!("{sid}__open").as_str(), || false);
            open.set(true);
            assert!(open.peek());
            close_open_subs(parent);
            assert!(!open.peek());
        });
    }

    #[test]
    fn open_or_remount_keyboard_path_stays_open() {
        let (_o, ()) = create_root(|| {
            let open = use_signal("si3_sub__open", || false);
            open_or_remount_sub_ex("si3_sub", open, false);
            assert!(open.peek());
        });
    }
}
