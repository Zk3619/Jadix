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
