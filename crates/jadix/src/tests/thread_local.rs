use super::*;

thread_local! {
    static FIRST: Cell<usize> = const { Cell::new(0) };
    static SECOND: RefCell<Vec<usize>> = RefCell::new(vec![FIRST.get()]);
    static PANICS_ONCE: usize = {
        assert!(FIRST.replace(1) != 0, "initialization fixture panic");
        42
    };
}

#[test]
fn nested_initialization_and_access_release_registry_borrows() {
    FIRST.set(7);
    SECOND.with_borrow_mut(|values| {
        values.push(FIRST.get());
        FIRST.set(9);
    });
    SECOND.with_borrow(|values| assert_eq!(values, &[7, 7]));
    assert_eq!(FIRST.take(), 9);
}

#[test]
fn values_are_isolated_and_dropped_per_thread() {
    static DROPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Dropped;
    impl Drop for Dropped {
        fn drop(&mut self) {
            DROPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    thread_local! { static RESOURCE: Dropped = Dropped; }
    FIRST.set(17);
    std::thread::spawn(|| {
        assert_eq!(FIRST.get(), 0);
        FIRST.set(23);
        RESOURCE.with(|_| {});
    })
    .join()
    .unwrap();
    assert_eq!(FIRST.get(), 17);
    assert_eq!(DROPS.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn failed_initializer_can_be_retried() {
    FIRST.set(0);
    assert!(std::panic::catch_unwind(|| PANICS_ONCE.with(|_| {})).is_err());
    assert_eq!(PANICS_ONCE.with(|value| *value), 42);
}

#[test]
fn many_keys_share_one_registry() {
    let keys: Vec<_> = (0..512)
        .map(|_| Box::leak(Box::new(LocalKey::new(|| Cell::new(0)))))
        .collect();
    for (index, key) in keys.into_iter().enumerate() {
        key.set(index);
        assert_eq!(key.get(), index);
    }
}
