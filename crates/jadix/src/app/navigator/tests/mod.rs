use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum R {
    Home,
    Profile,
    Settings,
}

fn nav_home() -> Navigator<R> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = format!(
        "test_nav_{}",
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    Navigator::new(id, R::Home, [R::Home])
}

#[test]
fn root_bring_to_front_truncates() {
    let nav = nav_home();
    nav.push(R::Profile, PushOptions::default());
    nav.push(R::Settings, PushOptions::default());
    assert_eq!(nav.peek_stack().len(), 3);
    assert!(nav.bring_to_front(&R::Home));
    let s = nav.peek_stack();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].route, R::Home);
    assert!(!nav.pop());
}

#[test]
fn non_root_bring_to_front_preserves_below() {
    let nav = nav_home();
    nav.push(R::Profile, PushOptions::default());
    nav.push(R::Settings, PushOptions::default());
    assert!(nav.bring_to_front(&R::Profile));
    let routes: Vec<_> = nav.peek_stack().iter().map(|f| f.route.clone()).collect();
    assert_eq!(routes, vec![R::Home, R::Settings, R::Profile]);
    assert!(nav.pop());
    let routes: Vec<_> = nav.peek_stack().iter().map(|f| f.route.clone()).collect();
    assert_eq!(routes, vec![R::Home, R::Settings]);
}

#[test]
fn push_existing_root_truncates() {
    let nav = nav_home();
    nav.push(R::Settings, PushOptions::default());
    nav.push(R::Home, PushOptions::default());
    let s = nav.peek_stack();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].route, R::Home);
}

#[test]
fn push_a_b_btf_a_pop_non_root() {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1000);
    let id = format!(
        "test_nav_nr_{}",
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let nav = Navigator::new(id, R::Home, [R::Home]);
    nav.push(R::Profile, PushOptions::default());
    nav.push(R::Settings, PushOptions::default());
    nav.bring_to_front(&R::Profile);
    let routes: Vec<_> = nav.peek_stack().iter().map(|f| f.route.clone()).collect();
    assert_eq!(routes, vec![R::Home, R::Settings, R::Profile]);
    nav.pop();
    let routes: Vec<_> = nav.peek_stack().iter().map(|f| f.route.clone()).collect();
    assert_eq!(routes, vec![R::Home, R::Settings]);
}

#[test]
fn slide_pop_exits_then_removes() {
    let nav = nav_home();
    nav.push(
        R::Profile,
        PushOptions {
            transition: Transition::Slide { duration_secs: 0.1 },
            ..PushOptions::default()
        },
    );
    assert_eq!(nav.peek_stack().last().unwrap().state, FrameState::Entering);
    while nav.tick(0.05) {}
    assert_eq!(nav.peek_stack().last().unwrap().state, FrameState::Active);
    assert!(nav.pop());
    assert_eq!(nav.peek_stack().last().unwrap().state, FrameState::Exiting);
    while nav.tick(0.05) {}
    assert_eq!(nav.peek_stack().len(), 1);
    assert_eq!(nav.peek_stack()[0].route, R::Home);
}

#[test]
fn try_pop_active_allows_clear_during_callback() {
    set_active_navigator(
        "reenter_nav".into(),
        Rc::new(|| {
            clear_active_navigator("reenter_nav");
            true
        }) as Rc<dyn Fn() -> bool>,
    );
    assert!(try_pop_active());
    assert!(!try_pop_active());
}

#[test]
fn tick_active_allows_clear_during_callback() {
    set_active_tick(
        "reenter_tick".into(),
        Rc::new(|_dt| {
            clear_active_tick("reenter_tick");
            false
        }) as Rc<dyn Fn(f32) -> bool>,
    );
    assert!(!tick_active(0.016));
    assert!(!tick_active(0.016));
}

#[test]
fn replacing_active_navigator_drops_old_callback_outside_borrow() {
    struct DropClearNav;
    impl Drop for DropClearNav {
        fn drop(&mut self) {
            clear_active_navigator("old_nav");
        }
    }
    let guard = DropClearNav;
    set_active_navigator(
        "old_nav".into(),
        Rc::new(move || {
            let _ = &guard;
            true
        }) as Rc<dyn Fn() -> bool>,
    );
    set_active_navigator("new_nav".into(), Rc::new(|| true) as Rc<dyn Fn() -> bool>);
    assert!(try_pop_active());
}

#[test]
fn clearing_active_navigator_drops_callback_outside_borrow() {
    struct DropClearNav;
    impl Drop for DropClearNav {
        fn drop(&mut self) {
            clear_active_navigator("clear_nav");
        }
    }
    let guard = DropClearNav;
    set_active_navigator(
        "clear_nav".into(),
        Rc::new(move || {
            let _ = &guard;
            true
        }) as Rc<dyn Fn() -> bool>,
    );

    clear_active_navigator("clear_nav");
    assert!(!try_pop_active());
}

#[test]
fn clearing_active_tick_drops_callback_outside_borrow() {
    struct DropClearTick;
    impl Drop for DropClearTick {
        fn drop(&mut self) {
            clear_active_tick("clear_tick");
        }
    }
    let guard = DropClearTick;
    set_active_tick(
        "clear_tick".into(),
        Rc::new(move |_| {
            let _ = &guard;
            true
        }) as Rc<dyn Fn(f32) -> bool>,
    );

    clear_active_tick("clear_tick");
    assert!(!tick_active(0.016));
}
