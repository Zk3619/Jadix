//! 通用指针拖拽会话注册表（WP-SI.7）。
//!
//! 对标 [`crate::utils::drag_reorder`] 的 app 路由模式：begin → tick(move) → end。
//! 组件用 [`DragKind`] 区分身份；[`window_events`](crate::app::window_events) 统一 tick。

use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Slider,
    ColorPicker,
    Toast,
    Carousel,
}

#[derive(Clone, Debug)]
pub struct PointerDragSession {
    pub kind: DragKind,
    pub id: String,
    pub start_x: f32,
    pub start_y: f32,
    pub last_x: f32,
    pub last_y: f32,
}

thread_local! {
    static SESSION: RefCell<Option<PointerDragSession>> = const { RefCell::new(None) };
}

pub fn begin(kind: DragKind, id: impl Into<String>, x: f32, y: f32) {
    SESSION.with(|s| {
        *s.borrow_mut() = Some(PointerDragSession {
            kind,
            id: id.into(),
            start_x: x,
            start_y: y,
            last_x: x,
            last_y: y,
        });
    });
}

/// Update last pointer; returns session clone if active.
pub fn tick(x: f32, y: f32) -> Option<PointerDragSession> {
    SESSION.with(|s| {
        let mut b = s.borrow_mut();
        if let Some(ref mut sess) = *b {
            sess.last_x = x;
            sess.last_y = y;
            Some(sess.clone())
        } else {
            None
        }
    })
}

pub fn clear() -> Option<PointerDragSession> {
    SESSION.with(|s| s.borrow_mut().take())
}

pub fn peek() -> Option<PointerDragSession> {
    SESSION.with(|s| s.borrow().clone())
}

pub fn is_active() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

pub fn dx_dy(sess: &PointerDragSession) -> (f32, f32) {
    (sess.last_x - sess.start_x, sess.last_y - sess.start_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_tick_clear() {
        clear();
        begin(DragKind::Slider, "s1", 10.0, 20.0);
        let t = tick(15.0, 22.0).expect("active");
        assert_eq!(t.kind, DragKind::Slider);
        let (dx, dy) = dx_dy(&t);
        assert!((dx - 5.0).abs() < f32::EPSILON);
        assert!((dy - 2.0).abs() < f32::EPSILON);
        assert!(clear().is_some());
        assert!(!is_active());
    }
}
