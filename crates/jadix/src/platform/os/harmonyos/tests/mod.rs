use super::*;

#[test]
fn host_matches_gpui_template_contract() {
    let host = HarmonyOsHost;
    assert_eq!(host.kind(), HostKind::HarmonyOs);
    assert!(!host.take_over_event_loop());
    assert!(host.capabilities().suspend_resume);
    assert!(!host.capabilities().blocking_run);
    assert!(host.capabilities().recreate_window_after_destroy);
    assert!(!host.capabilities().multi_window);
}
