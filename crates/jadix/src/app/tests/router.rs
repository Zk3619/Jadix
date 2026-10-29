use super::*;
use crate::core::reactivity::create_root;

#[test]
fn router_navigate_back_forward() {
    let _ = create_root(|| {
        let mut r = Router::new("test-router", "home");
        r.register("home", || Element::new().text("home"));
        r.register("settings", || Element::new().text("settings"));
        assert_eq!(r.current(), "home");
        r.navigate("settings");
        assert_eq!(r.current(), "settings");
        assert!(r.can_back());
        assert!(r.back());
        assert_eq!(r.current(), "home");
        assert!(r.can_forward());
        assert!(r.forward());
        assert_eq!(r.current(), "settings");
    });
}

#[test]
fn router_two_hops_back_reaches_start() {
    let _ = create_root(|| {
        let mut r = Router::new("test-router-2hop", "home");
        r.register("home", || Element::new().text("home"));
        r.register("inbox", || Element::new().text("inbox"));
        r.register("settings", || Element::new().text("settings"));
        r.navigate("inbox");
        r.navigate("settings");
        assert_eq!(r.current(), "settings");
        assert!(r.back());
        assert_eq!(r.current(), "inbox");
        assert!(r.back());
        assert_eq!(r.current(), "home");
        assert!(!r.can_back());
        assert!(r.can_forward());
    });
}
