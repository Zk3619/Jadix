//! 通用指针拖拽会话注册表（WP-SI.7）。
//!
//! 对标 [`crate::utils::drag_reorder`] 的 app 路由模式：begin → tick(move) → end。
//! 组件用 [`DragKind`] 区分身份；[`window_events`](crate::app::window_events) 统一 tick。
//!
//! 默认指针 `PRIMARY`（鼠标 / 主触点）。[`begin_pointer`] 允许多指并发
//! （interact `max` / `maxPerElement` 的会话面；配额在
//! [`crate::utils::pointer_action::PointerSet`]）。

use std::cell::RefCell;
use std::collections::HashMap;

/// Mouse / primary-finger slot. Existing [`begin`] / [`tick`] / [`clear`] use this id.
pub const PRIMARY: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Slider,
    ColorPicker,
    Toast,
    Carousel,
    Resizable,
    /// Point-mode [`crate::utils::anchored_overlay`] title-bar drag.
    FloatingPanel,
    /// Headless drag kit ([`crate::utils::drag`]) — payload / sortable session.
    Scope,
    /// [`crate::utils::pointer_action`] resize / gesture handle.
    PointerAction,
}

#[derive(Clone, Debug)]
pub struct PointerDragSession {
    pub kind: DragKind,
    pub id: String,
    pub start_x: f32,
    pub start_y: f32,
    pub last_x: f32,
    pub last_y: f32,
    /// winit touch id, or [`PRIMARY`] for mouse.
    pub pointer_id: u64,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<u64, PointerDragSession>> = RefCell::new(HashMap::new());
}

pub fn begin(kind: DragKind, id: impl Into<String>, x: f32, y: f32) {
    begin_pointer(PRIMARY, kind, id, x, y);
}

pub fn begin_pointer(pointer_id: u64, kind: DragKind, id: impl Into<String>, x: f32, y: f32) {
    SESSIONS.with(|s| {
        s.borrow_mut().insert(
            pointer_id,
            PointerDragSession {
                kind,
                id: id.into(),
                start_x: x,
                start_y: y,
                last_x: x,
                last_y: y,
                pointer_id,
            },
        );
    });
}

/// Update the primary pointer; returns that session if live.
pub fn tick(x: f32, y: f32) -> Option<PointerDragSession> {
    tick_pointer(PRIMARY, x, y)
}

pub fn tick_pointer(pointer_id: u64, x: f32, y: f32) -> Option<PointerDragSession> {
    SESSIONS.with(|s| {
        let mut b = s.borrow_mut();
        let sess = b.get_mut(&pointer_id)?;
        sess.last_x = x;
        sess.last_y = y;
        Some(sess.clone())
    })
}

/// Drop the primary pointer session (mouse / primary finger). Other fingers stay.
pub fn clear() -> Option<PointerDragSession> {
    clear_pointer(PRIMARY)
}

pub fn clear_pointer(pointer_id: u64) -> Option<PointerDragSession> {
    SESSIONS.with(|s| s.borrow_mut().remove(&pointer_id))
}

pub fn clear_all() {
    SESSIONS.with(|s| s.borrow_mut().clear());
}

/// Primary session, or the lowest pointer id if only secondary fingers are down.
pub fn peek() -> Option<PointerDragSession> {
    SESSIONS.with(|s| {
        let b = s.borrow();
        if let Some(p) = b.get(&PRIMARY) {
            return Some(p.clone());
        }
        b.values().min_by_key(|p| p.pointer_id).cloned()
    })
}

pub fn peek_pointer(pointer_id: u64) -> Option<PointerDragSession> {
    SESSIONS.with(|s| s.borrow().get(&pointer_id).cloned())
}

pub fn is_active() -> bool {
    SESSIONS.with(|s| !s.borrow().is_empty())
}

pub fn active_count() -> usize {
    SESSIONS.with(|s| s.borrow().len())
}

pub fn dx_dy(sess: &PointerDragSession) -> (f32, f32) {
    (sess.last_x - sess.start_x, sess.last_y - sess.start_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_tick_clear() {
        clear_all();
        begin(DragKind::Slider, "s1", 10.0, 20.0);
        let t = tick(15.0, 22.0).expect("active");
        assert_eq!(t.kind, DragKind::Slider);
        let (dx, dy) = dx_dy(&t);
        assert!((dx - 5.0).abs() < f32::EPSILON);
        assert!((dy - 2.0).abs() < f32::EPSILON);
        assert!(clear().is_some());
        assert!(!is_active());
    }

    #[test]
    fn two_pointers_do_not_clobber() {
        clear_all();
        begin_pointer(1, DragKind::PointerAction, "a", 0.0, 0.0);
        begin_pointer(2, DragKind::PointerAction, "b", 10.0, 0.0);
        assert_eq!(active_count(), 2);
        tick_pointer(1, 4.0, 1.0);
        tick_pointer(2, 18.0, 2.0);
        let a = peek_pointer(1).unwrap();
        let b = peek_pointer(2).unwrap();
        assert!((a.last_x - 4.0).abs() < f32::EPSILON);
        assert!((b.last_x - 18.0).abs() < f32::EPSILON);
        assert!(clear_pointer(1).is_some());
        assert_eq!(active_count(), 1);
        assert!(peek_pointer(2).is_some());
        clear_all();
    }

    #[test]
    fn clear_primary_keeps_secondary() {
        clear_all();
        begin(DragKind::Slider, "mouse", 0.0, 0.0);
        begin_pointer(7, DragKind::PointerAction, "finger", 1.0, 1.0);
        assert_eq!(clear().unwrap().id, "mouse");
        assert_eq!(active_count(), 1);
        assert_eq!(peek().unwrap().id, "finger");
        clear_all();
    }
}
