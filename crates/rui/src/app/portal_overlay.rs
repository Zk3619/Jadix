//! Portal 浮层 LIFO 栈：临时浮层登记、Esc 关顶层、按深度分配 z-index。
//!
//! 与 [`crate::app::navigator`]（页面栈）分离。常驻 UI 用 [`OverlayMode::Pinned`]，不进栈。
//! 组件在构建时调用 [`portal_sync`]；内容仍用树内 `rui_portal` 提升。

use std::cell::RefCell;

use crate::core::reactivity::Signal;

const OVERLAY_Z_BASE: i32 = 1000;
const OVERLAY_Z_STEP: i32 = 10;
/// Pinned 浮层约定 z（不入栈；仍低于 Stacked 的 1000+）。
const PINNED_Z_UNDERLAY: i32 = 900;

/// 浮层类别（调试 / 文档；z 一律按栈序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    Modal,
    Menu,
    Popover,
    Tooltip,
}

/// 是否进入 Esc LIFO 栈。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OverlayMode {
    /// 进栈；Esc / `try_close_top` 可关。
    #[default]
    Stacked,
    /// 仅 portal 提升用；不进栈、不吃 Esc。
    Pinned,
}

/// 一层 overlay 的 underlay / panel z-index。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayZ {
    pub underlay: i32,
    pub panel: i32,
}

struct OverlayLayer {
    id: String,
    open: Signal<bool>,
    #[allow(dead_code)]
    kind: OverlayKind,
}

thread_local! {
    static OVERLAY_STACK: RefCell<Vec<OverlayLayer>> = const { RefCell::new(Vec::new()) };
}

fn z_for_index(i: usize) -> OverlayZ {
    let underlay = OVERLAY_Z_BASE + (i as i32) * OVERLAY_Z_STEP;
    OverlayZ {
        underlay,
        panel: underlay + 1,
    }
}

fn prune_closed(stack: &mut Vec<OverlayLayer>) {
    stack.retain(|l| l.open.peek());
}

/// 与组件 `open` 态对齐。
///
/// - [`OverlayMode::Stacked`]：开则 push，关则按 id 弹出；返回栈深度 z。
/// - [`OverlayMode::Pinned`]：不入栈；开时返回约定 z，关时 `None`。
pub fn portal_sync(
    id: impl Into<String>,
    open: Signal<bool>,
    kind: OverlayKind,
    mode: OverlayMode,
) -> Option<OverlayZ> {
    let id = id.into();
    match mode {
        OverlayMode::Pinned => {
            if open.peek() {
                Some(OverlayZ {
                    underlay: PINNED_Z_UNDERLAY,
                    panel: PINNED_Z_UNDERLAY + 1,
                })
            } else {
                None
            }
        }
        OverlayMode::Stacked => OVERLAY_STACK.with(|cell| {
            let mut stack = cell.borrow_mut();
            prune_closed(&mut stack);
            if open.peek() {
                if let Some(pos) = stack.iter().position(|l| l.id == id) {
                    stack[pos].open = open;
                    stack[pos].kind = kind;
                    Some(z_for_index(pos))
                } else {
                    stack.push(OverlayLayer { id, open, kind });
                    Some(z_for_index(stack.len() - 1))
                }
            } else {
                stack.retain(|l| l.id != id);
                None
            }
        }),
    }
}

/// Esc：先 prune 已关层，再关闭并弹出顶层 Stacked。返回是否关掉了一层。
pub fn try_close_top() -> bool {
    OVERLAY_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        prune_closed(&mut stack);
        let Some(top) = stack.pop() else {
            return false;
        };
        if top.open.peek() {
            top.open.set(false);
            true
        } else {
            false
        }
    })
}

/// Drop portal stack entries (call on app shutdown so Signal handles do not linger).
pub fn clear_overlay_stack() {
    OVERLAY_STACK.with(|cell| cell.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::Signal;

    fn clear() {
        OVERLAY_STACK.with(|c| c.borrow_mut().clear());
    }

    fn depth() -> usize {
        OVERLAY_STACK.with(|c| c.borrow().len())
    }

    #[test]
    fn push_two_esc_closes_top_only() {
        clear();
        let a = Signal::new(true);
        let b = Signal::new(true);
        let za = portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).unwrap();
        let zb = portal_sync("b", b, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        assert_eq!(za.underlay, 1000);
        assert_eq!(zb.underlay, 1010);
        assert_eq!(depth(), 2);

        assert!(try_close_top());
        assert!(!b.peek());
        assert!(a.peek());
        assert_eq!(depth(), 1);

        assert!(try_close_top());
        assert!(!a.peek());
        assert_eq!(depth(), 0);
        assert!(!try_close_top());
    }

    #[test]
    fn pinned_does_not_enter_stack() {
        clear();
        let open = Signal::new(true);
        let z = portal_sync("pin", open, OverlayKind::Popover, OverlayMode::Pinned).unwrap();
        assert_eq!(z.underlay, 900);
        assert_eq!(depth(), 0);
        assert!(!try_close_top());
        assert!(open.peek());
    }

    #[test]
    fn sync_false_pops_by_id() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        open.set(false);
        assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_none());
        assert_eq!(depth(), 0);
    }

    #[test]
    fn same_id_does_not_duplicate() {
        clear();
        let open = Signal::new(true);
        let z1 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        let z2 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        assert_eq!(z1, z2);
        assert_eq!(depth(), 1);
    }
}
