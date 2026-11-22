//! One in-flight parse, one replaceable pending snapshot, and one completed result.
use crate::{AnnotatedText, TextAnnotation, TextDocument};
use std::{
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
};

type Colors = AnnotatedText<[u8; 4]>;
type Key = (u64, String, bool, Option<Range<usize>>);

struct Job {
    generation: u64,
    document: TextDocument,
    language: String,
    dark: bool,
    visible: Option<Range<usize>>,
}

#[derive(Default)]
struct State {
    generation: u64,
    stopped: bool,
    running: Option<u64>,
    pending: Option<Job>,
    ready: Option<Colors>,
}

/// Background syntax over immutable snapshots. Repeated requests for the same
/// revision/language/theme are free; replacements discard pending and ready work.
/// Dropping or cancelling never waits for the current parser to finish, but its
/// result is discarded. `wake` runs on the worker and must only notify the UI.
/// With `syntax`, edits resume from sparse line checkpoints and reuse an unchanged
/// suffix only after parser/highlight state convergence. Styling changes start a
/// fresh parse. One highlighted snapshot and about one checkpoint per 4 KiB are
/// retained, plus a checkpoint before the final line for streaming appends.
/// Replaced jobs stop at the next logical line. A single unusually expensive
/// grammar line is not preempted and there is no wall-clock deadline.
pub struct SyntaxWorker {
    shared: Arc<(Mutex<State>, Condvar)>,
    current: Option<Key>,
    epoch: Arc<AtomicU64>,
}

impl SyntaxWorker {
    pub fn new(wake: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        #[cfg(feature = "syntax")]
        {
            let mut parser = super::resume::ResumeHighlighter::default();
            Self::spawn_progress(wake, move |document, language, dark, progress| {
                parser.highlight_progress(document, language, dark, progress)
            })
        }
        #[cfg(not(feature = "syntax"))]
        Self::spawn_progress(wake, |document, language, dark, progress| {
            if !progress(0, &[]) {
                return None;
            }
            Some(super::highlight_document(document, language, dark))
        })
    }

    #[cfg(test)]
    fn spawn(
        wake: impl Fn() + Send + 'static,
        mut parse: impl FnMut(TextDocument, &str, bool) -> Colors + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn_progress(wake, move |document, language, dark, _| {
            Some(parse(document, language, dark))
        })
    }

    fn spawn_progress(
        wake: impl Fn() + Send + 'static,
        mut parse: impl FnMut(
                TextDocument,
                &str,
                bool,
                &mut dyn FnMut(usize, &[TextAnnotation<[u8; 4]>]) -> bool,
            ) -> Option<Colors>
            + Send
            + 'static,
    ) -> std::io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        let epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = epoch.clone();
        std::thread::Builder::new()
            .name("jadix-document-syntax".into())
            .spawn(move || {
                loop {
                    let (mutex, changed) = &*worker;
                    let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
                    while state.pending.is_none() && !state.stopped {
                        state = changed.wait(state).unwrap_or_else(|e| e.into_inner());
                    }
                    if state.stopped {
                        return;
                    }
                    let job = state.pending.take().unwrap();
                    state.running = Some(job.generation);
                    drop(state);
                    let mut published_visible = false;
                    let mut progress = |offset: usize, annotations: &[TextAnnotation<[u8; 4]>]| {
                        if worker_epoch.load(Ordering::Acquire) != job.generation {
                            return false;
                        }
                        if let Some(visible) = job
                            .visible
                            .as_ref()
                            .filter(|visible| !published_visible && offset >= visible.end)
                        {
                            published_visible = true;
                            let annotations = annotations
                                .iter()
                                .filter_map(|annotation| {
                                    let range = annotation.range.start.max(visible.start)
                                        ..annotation.range.end.min(visible.end);
                                    (range.start < range.end).then_some(TextAnnotation {
                                        range,
                                        value: annotation.value,
                                    })
                                })
                                .collect();
                            let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
                            if !state.stopped && state.generation == job.generation {
                                state.ready =
                                    Some(AnnotatedText::new(job.document.clone(), annotations));
                                wake();
                            }
                        }
                        true
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        parse(job.document.clone(), &job.language, job.dark, &mut progress)
                    }))
                    .unwrap_or_else(|_| Some(AnnotatedText::new(job.document.clone(), Vec::new())));
                    let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
                    if state.stopped {
                        return;
                    }
                    if state.running == Some(job.generation) {
                        state.running = None;
                    }
                    if let Some(result) = result.filter(|_| state.generation == job.generation) {
                        state.ready = Some(result);
                        // Serialize notification with cancellation/disposal. The callback
                        // must not reenter this worker; it only schedules a UI wakeup.
                        wake();
                    }
                }
            })?;
        Ok(Self {
            shared,
            current: None,
            epoch,
        })
    }

    /// Returns true when the source or styling changed. Clear the displayed old
    /// result immediately in that case, then poll after the next wakeup.
    pub fn request(&mut self, document: TextDocument, language: &str, dark: bool) -> bool {
        self.submit(document, language, dark, None)
    }

    /// Publish the visible logical lines (plus 32 lookahead lines) before finishing
    /// the remaining document. Earlier lines are parsed to retain exact multiline
    /// grammar state; they are never independently guessed from a viewport slice.
    pub fn request_visible(
        &mut self,
        document: TextDocument,
        language: &str,
        dark: bool,
        lines: Range<usize>,
    ) -> bool {
        let start = document
            .line_full_range(lines.start)
            .map_or(document.len(), |range| range.start);
        let last = lines
            .end
            .max(lines.start.saturating_add(1))
            .saturating_add(31);
        let end = document
            .line_full_range(last)
            .map_or(document.len(), |range| range.end);
        self.submit(document, language, dark, Some(start..end))
    }

    fn submit(
        &mut self,
        document: TextDocument,
        language: &str,
        dark: bool,
        visible: Option<Range<usize>>,
    ) -> bool {
        let key = (
            document.revision(),
            language.to_owned(),
            dark,
            visible.clone(),
        );
        if self.current.as_ref() == Some(&key) {
            return false;
        }
        self.current = Some(key);
        let (mutex, changed) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        self.epoch.store(state.generation, Ordering::Release);
        state.ready = None;
        state.pending = Some(Job {
            generation: state.generation,
            document,
            language: language.to_owned(),
            dark,
            visible,
        });
        changed.notify_one();
        true
    }

    /// Take only the latest requested result, without blocking on parsing.
    pub fn poll(&mut self) -> Option<Colors> {
        self.shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ready
            .take()
    }

    /// Whether the current request still has work queued or running. A visible
    /// publication can be ready while the remainder is still being parsed.
    pub fn is_pending(&self) -> bool {
        let state = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        state.pending.is_some() || state.running == Some(state.generation)
    }

    pub fn cancel(&mut self) {
        self.current = None;
        let mut state = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        self.epoch.store(state.generation, Ordering::Release);
        state.pending = None;
        state.running = None;
        state.ready = None;
    }
}

impl Drop for SyntaxWorker {
    fn drop(&mut self) {
        let (mutex, changed) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        self.epoch.fetch_add(1, Ordering::Release);
        state.pending = None;
        state.ready = None;
        changed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn visible_publication_precedes_completion_and_replacement_interrupts_progress() {
        let (reached, checkpoints) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let (woken, wakeups) = mpsc::channel();
        let mut worker = SyntaxWorker::spawn_progress(
            move || {
                let _ = woken.send(());
            },
            move |document, _, _, progress| {
                let colors = vec![TextAnnotation {
                    range: 0..document.len(),
                    value: [1, 2, 3, 255],
                }];
                let end = document.line_full_range(32).unwrap().end;
                if !progress(end, &colors) {
                    return None;
                }
                reached.send(document.clone()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                if !progress(document.len(), &colors) {
                    return None;
                }
                Some(AnnotatedText::new(document, colors))
            },
        )
        .unwrap();
        let first = TextDocument::new("🙂\r\n".repeat(100));
        worker.request_visible(first.clone(), "rs", false, 0..1);
        checkpoints.recv_timeout(Duration::from_secs(5)).unwrap();
        wakeups.recv_timeout(Duration::from_secs(5)).unwrap();
        let visible = worker.poll().unwrap();
        assert!(worker.is_pending(), "visible colors do not finish the background parse");
        assert_eq!(visible.document(), &first);
        assert_eq!(
            visible.annotations()[0].range.end,
            first.line_full_range(32).unwrap().end
        );
        let latest = TextDocument::new("最新\n".repeat(100));
        worker.request_visible(latest.clone(), "rs", false, 0..1);
        release.send(()).unwrap();
        assert_eq!(
            checkpoints.recv_timeout(Duration::from_secs(5)).unwrap(),
            latest
        );
        wakeups.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(worker.poll().unwrap().document(), &latest);
        worker.cancel();
        assert!(!worker.is_pending());
        release.send(()).unwrap();
        drop(worker);
        assert!(wakeups.recv_timeout(Duration::from_secs(5)).is_err());
    }

    #[test]
    fn replacement_cancellation_and_drop_discard_obsolete_parses() {
        let (entered, started) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let (woken, wakeups) = mpsc::channel();
        let mut worker = SyntaxWorker::spawn(
            move || {
                let _ = woken.send(());
            },
            move |document, language, dark| {
                entered
                    .send((document.clone(), language.to_owned(), dark))
                    .unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                AnnotatedText::new(document, Vec::new())
            },
        )
        .unwrap();
        let first = TextDocument::new("/* old */");
        assert!(worker.request(first.clone(), "rs", false));
        assert_eq!(
            started.recv_timeout(Duration::from_secs(5)).unwrap().0,
            first
        );
        assert!(!worker.request(first, "rs", false));
        for _ in 0..100 {
            worker.request(TextDocument::new("superseded"), "js", false);
        }
        let latest = TextDocument::new("最新🙂\r\n");
        worker.request(latest.clone(), "rs", true);
        release.send(()).unwrap();
        assert_eq!(
            started.recv_timeout(Duration::from_secs(5)).unwrap(),
            (latest.clone(), "rs".into(), true)
        );
        assert!(worker.poll().is_none());
        assert!(
            wakeups.try_recv().is_err(),
            "obsolete parse must not wake the UI"
        );
        release.send(()).unwrap();
        wakeups.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(worker.poll().unwrap().document(), &latest);
        assert!(worker.poll().is_none());
        assert!(!worker.is_pending());

        worker.request(latest.clone(), "rs", false); // Theme-only replacement.
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.cancel();
        worker.request(latest, "py", true); // Language-only replacement after cancel.
        release.send(()).unwrap();
        assert_eq!(
            started.recv_timeout(Duration::from_secs(5)).unwrap().1,
            "py"
        );
        drop(worker); // Does not wait for the deliberately blocked parse.
        release.send(()).unwrap();
        assert!(wakeups.recv_timeout(Duration::from_secs(5)).is_err());
    }
}
