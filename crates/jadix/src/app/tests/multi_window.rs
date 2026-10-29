use super::*;

#[test]
fn open_secondary_window_needs_proxy() {
    // Without an installed proxy this process cannot create windows.
    let err = open_secondary_window("Secondary", Element::new).expect_err("needs proxy");
    assert!(
        err.contains("proxy not installed") || err.contains("failed to queue"),
        "unexpected message: {err}"
    );
    let err2 = open_secondary_window_title_only("Secondary").expect_err("needs proxy");
    assert!(
        err2.contains("proxy not installed") || err2.contains("failed to queue"),
        "unexpected message: {err2}"
    );
    let err3 = open_secondary_window_with(
        WindowOptions::new().title("Glass").frameless().blur(true),
        Element::new,
    )
    .expect_err("needs proxy");
    assert!(
        err3.contains("proxy not installed") || err3.contains("failed to queue"),
        "unexpected message: {err3}"
    );
}

#[test]
fn window_registry_starts_empty_and_tracks_api() {
    let reg = WindowRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
    assert!(reg.primary_id().is_none());
    assert!(open_secondary_window_title_only("extra").is_err());
}

#[test]
fn window_registry_drain_secondaries_on_empty() {
    let mut reg = WindowRegistry::new();
    assert!(reg.drain_secondaries().is_empty());
}
