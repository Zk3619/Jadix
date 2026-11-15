use super::*;

#[test]
fn push_activate_clear() {
    reset("si1-a");
    assert_eq!(count("si1-a"), 0);
    let mut hit = 0usize;
    let hit_cell = Rc::new(RefCell::new(0usize));
    let c = hit_cell.clone();
    push(
        "si1-a",
        Rc::new(move || {
            *c.borrow_mut() += 1;
        }) as Rc<dyn Fn()>,
    );
    assert_eq!(count("si1-a"), 1);
    if let Some(cb) = take("si1-a", 0) {
        cb();
        hit = *hit_cell.borrow();
    }
    assert_eq!(hit, 1);
    reset("si1-a");
    assert_eq!(count("si1-a"), 0);
    assert!(take("si1-a", 0).is_none());
}

#[test]
fn namespaces_do_not_collide() {
    reset("dd:menu");
    reset("ctx:menu");
    push("dd:menu", Rc::new(|| {}) as Rc<dyn Fn()>);
    assert_eq!(count("dd:menu"), 1);
    assert_eq!(count("ctx:menu"), 0);
}

#[test]
fn enabled_navigation_preserves_indices_and_excludes_disabled_activation() {
    let id = "enabled-menu";
    reset(id);
    for enabled in [false, true, false, true, false] {
        push_enabled(id, Rc::new(|| {}), enabled);
    }
    assert_eq!(count(id), 5);
    assert_eq!(enabled_edge(id, false), Some(1));
    assert_eq!(enabled_edge(id, true), Some(3));
    for (current, delta, expected) in [
        (usize::MAX, 1, 1),
        (usize::MAX, -1, 3),
        (1, 1, 3),
        (3, 1, 1),
        (1, -1, 3),
        (2, -1, 1),
    ] {
        assert_eq!(move_enabled(id, current, delta), Some(expected));
    }
    assert!(take(id, 0).is_none());
    assert!(take(id, 1).is_some());
    reset(id);
    push_enabled(id, Rc::new(|| {}), false);
    assert_eq!(enabled_edge(id, false), None);
    assert_eq!(move_enabled(id, 0, 1), None);
    assert!(take(id, 0).is_none());
    reset(id);
    assert_eq!(move_enabled(id, usize::MAX, -1), None);
}
