use super::*;

#[test]
fn placeholder_is_default() {
    assert!(WindowState::default().is_placeholder());
    let mut s = WindowState::default();
    s.width = 800;
    s.height = 600;
    assert!(!s.is_placeholder());
}

#[test]
fn moved_keeps_previous_origin() {
    let mut s = WindowState::default();
    s.note_moved(10, 20);
    s.note_moved(0, 0);
    assert_eq!((s.prev_x, s.prev_y), (10, 20));
    assert_eq!((s.x, s.y), (0, 0));
}

#[test]
fn monitor_intersect_any_corner() {
    let m = MonitorRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    assert!(m.intersects((1900, 10), (40, 40)));
    assert!(!m.intersects((2000, 10), (40, 40)));
    let right = MonitorRect {
        x: 1920,
        y: 0,
        width: 1080,
        height: 1920,
    };
    assert!(right.intersects((1930, 10), (100, 100)));
    assert!(!right.intersects((100, 10), (100, 100)));
}
