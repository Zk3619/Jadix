//! Optional event-loop integration. The core also works without a UI host.
use std::sync::RwLock;
pub type UiJob = Box<dyn FnOnce() + Send>;
static CALLBACKS: RwLock<(fn(), fn(UiJob))> = RwLock::new((|| {}, |job| job()));
/// Install process-wide invalidation and UI dispatch callbacks before loading watched catalogs.
pub fn set_host_callbacks(changed: fn(), dispatch: fn(UiJob)) {
    *CALLBACKS.write().unwrap_or_else(|e| e.into_inner()) = (changed, dispatch);
}
pub(super) fn changed() {
    let callback = CALLBACKS.read().unwrap_or_else(|e| e.into_inner()).0;
    callback();
}
pub(super) fn dispatch(job: UiJob) {
    let callback = CALLBACKS.read().unwrap_or_else(|e| e.into_inner()).1;
    callback(job);
}
