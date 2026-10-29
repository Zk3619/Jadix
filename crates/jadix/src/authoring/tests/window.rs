use super::*;
window_local! {
    static COUNT: Cell<usize> = const { Cell::new(0) };
    static RETAINED: RefCell<Option<Rc<()>>> = const { RefCell::new(None) };
}

#[test]
fn identical_slot_names_in_different_windows_do_not_share_values() {
    activate(WindowScope::Primary);
    let _primary = window_scope::enter_primary();
    COUNT.set(3);
    {
        let _secondary = window_scope::enter(WindowScope::Secondary(802));
        activate(WindowScope::Secondary(802));
        assert_eq!(COUNT.get(), 0);
        COUNT.set(7);
    }
    assert_eq!(COUNT.get(), 3);
    forget(WindowScope::Secondary(802));
}

#[test]
fn closing_a_window_releases_captures_without_resurrection() {
    let scope = WindowScope::Secondary(803);
    activate(scope);
    let _scope = window_scope::enter(scope);
    let value = Rc::new(());
    let weak = Rc::downgrade(&value);
    RETAINED.with(|slot| *slot.borrow_mut() = Some(value));
    assert!(weak.upgrade().is_some());
    forget(scope);
    assert!(weak.upgrade().is_none());
    COUNT.set(12);
    assert_eq!(COUNT.get(), 0);
}

#[test]
fn nested_access_does_not_hold_the_storage_map_borrow() {
    let _primary = window_scope::enter_primary();
    activate(WindowScope::Primary);
    COUNT.with(|count| {
        RETAINED.with(|retained| {
            count.set(9);
            retained.borrow_mut().take();
            assert_eq!(COUNT.get(), 9);
        })
    });
}
