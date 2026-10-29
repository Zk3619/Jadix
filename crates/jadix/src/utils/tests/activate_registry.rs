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
