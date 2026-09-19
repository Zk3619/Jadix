//! Extension slot for hosts that are **not** upstream winit (HarmonyOS / custom).
//!
//! Default path is always winit. Register a backend with [`set_external_host`]
//! before [`crate::app::run`] when a future OS needs its own loop. Do **not**
//! fork winit (`jadix_winit`) until this slot and the upstream crate are both
//! proven insufficient.

use std::sync::{Arc, Mutex};

use super::{HostCapabilities, HostKind};

/// Process-wide replacement / overlay for the compiled-in winit host.
pub trait HostBackend: Send + Sync {
    fn kind(&self) -> HostKind;
    fn capabilities(&self) -> HostCapabilities;
    /// If `true`, [`super::build_event_loop`] must not construct a winit
    /// `EventLoop`. The implementor drives frames itself (HarmonyOS later).
    fn take_over_event_loop(&self) -> bool {
        false
    }
}

fn slot() -> &'static Mutex<Option<Arc<dyn HostBackend>>> {
    static SLOT: Mutex<Option<Arc<dyn HostBackend>>> = Mutex::new(None);
    &SLOT
}

/// Install or clear the process-wide external host. Call from `main` / `android_main`
/// before [`crate::app::run`].
pub fn set_external_host(host: Option<Arc<dyn HostBackend>>) {
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = host;
}

pub fn external_host() -> Option<Arc<dyn HostBackend>> {
    slot().lock().unwrap_or_else(|p| p.into_inner()).clone()
}

pub fn external_host_takes_over() -> bool {
    external_host().is_some_and(|h| h.take_over_event_loop())
}

#[cfg(test)]
mod tests {
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
}
