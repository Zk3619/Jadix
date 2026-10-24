use super::editor_settings::AutosaveSettings;
use crate::core::reactivity::{create_effect, dispose_effect, on_cleanup, Signal};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRequest {
    pub key: u64,
    pub path: PathBuf,
    pub contents: String,
    pub expected: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AutosaveResult {
    pub request: SaveRequest,
    pub result: Result<(), String>,
}

pub fn install_autosave(
    settings: Signal<AutosaveSettings>,
    source: impl Fn() -> Option<SaveRequest> + 'static,
    on_result: impl Fn(AutosaveResult) + 'static,
) {
    let pending = Arc::new(Mutex::new(None::<(u64, Duration, Option<SaveRequest>)>));
    let alive = Arc::new(AtomicBool::new(true));
    let revision = Arc::new(AtomicU64::new(0));
    let result = Signal::new(None::<AutosaveResult>);
    let (wake, events) = mpsc::sync_channel::<()>(1);
    let jobs = pending.clone();
    let running = alive.clone();
    let generation = revision.clone();
    std::thread::spawn(move || {
        let mut scheduled: Option<(Instant, u64, SaveRequest)> = None;
        loop {
            let timeout = scheduled
                .as_ref()
                .map(|(deadline, ..)| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(3600));
            let received = events.recv_timeout(timeout);
            if !running.load(Ordering::Acquire) {
                break;
            }
            if let Some((version, delay, request)) = jobs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
            {
                scheduled = request.map(|request| (Instant::now() + delay, version, request));
            } else if matches!(received, Err(mpsc::RecvTimeoutError::Timeout)) {
                if let Some((_, version, request)) = scheduled.take() {
                    if generation.load(Ordering::Acquire) != version {
                        continue;
                    }
                    let saved = super::document_file::save_text_document(
                        &request.path,
                        &request.contents,
                        Some(&request.expected),
                    )
                    .map_err(|error| error.to_string());
                    crate::app::run_on_ui_thread(move || {
                        result.try_set(Some(AutosaveResult {
                            request,
                            result: saved,
                        }));
                    });
                }
            } else if received.is_err() {
                break;
            }
        }
    });
    let previous = std::cell::RefCell::new(None);
    let sender = wake.clone();
    let effect = create_effect(move || {
        let settings = settings.get();
        let request =
            source().filter(|request| settings.enabled && request.contents != request.expected);
        let next = (settings, request);
        if previous.borrow().as_ref() == Some(&next) {
            return;
        }
        let version = revision.fetch_add(1, Ordering::AcqRel) + 1;
        *pending.lock().unwrap_or_else(|error| error.into_inner()) = Some((
            version,
            Duration::from_millis(next.0.delay_ms.clamp(250, 3_600_000)),
            next.1.clone(),
        ));
        *previous.borrow_mut() = Some(next);
        let _ = sender.try_send(());
    });
    let feedback = create_effect(move || {
        if let Some(result) = result.get() {
            on_result(result);
        }
    });
    on_cleanup(move || {
        dispose_effect(effect);
        dispose_effect(feedback);
        alive.store(false, Ordering::Release);
        let _ = wake.try_send(());
    });
}
