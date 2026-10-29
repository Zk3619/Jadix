use super::*;
use crate::platform::host::{HostCapabilities, HostKind};

struct HarmonyStub;

impl HostBackend for HarmonyStub {
    fn kind(&self) -> HostKind {
        HostKind::HarmonyOs
    }
    fn capabilities(&self) -> HostCapabilities {
        HostCapabilities::for_kind(HostKind::HarmonyOs)
    }
    fn take_over_event_loop(&self) -> bool {
        true
    }
}

#[test]
fn external_host_slot_roundtrip() {
    set_external_host(None);
    assert!(external_host().is_none());
    assert!(!external_host_takes_over());

    set_external_host(Some(Arc::new(HarmonyStub)));
    let host = external_host().expect("slot");
    assert_eq!(host.kind(), HostKind::HarmonyOs);
    assert!(host.take_over_event_loop());
    assert!(external_host_takes_over());
    assert!(!host.capabilities().blocking_run || host.capabilities().suspend_resume);

    set_external_host(None);
    assert!(external_host().is_none());
}
