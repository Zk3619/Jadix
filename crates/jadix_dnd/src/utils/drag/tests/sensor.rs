use super::*;

#[test]
fn begin_draggable_missing_hit_returns_false() {
    let begin: Rc<dyn Fn(BeginDrag)> = Rc::new(|_| {});
    assert!(!begin_draggable("missing", "x", &begin));
}

#[test]
fn add_sensor_requires_scope() {
    assert!(!add_sensor("missing", DragSensor::new("kbd", |_| false)));
}
