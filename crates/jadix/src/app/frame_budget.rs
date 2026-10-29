#[cfg(feature = "performance")]
pub use enabled::*;

#[cfg(feature = "performance")]
mod enabled {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::time::Instant;
    use winit::event::WindowEvent;
    use winit::window::WindowId;

    const SAMPLE_LIMIT: usize = 2048;

    /// CPU work charged to one presented window frame, not GPU execution or scanout.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct ApplicationFrameSample {
        pub work_ms: f64,
        pub redraw_ms: f64,
        pub event_ms: f64,
        pub shared_ms: f64,
        pub ime_ms: f64,
        pub semantics_ms: f64,
        pub input_to_present_ms: Option<f64>,
        pub input_events: u64,
        pub ime_events: u64,
    }

    /// Bounded recent samples and all-frame budget counters for one window.
    #[derive(Debug)]
    pub struct ApplicationFrameTimings {
        pub window_id: WindowId,
        pub budget_ms: f64,
        pub frames: u64,
        pub over_budget: u64,
        pub skipped_redraws: u64,
        pub dropped_samples: u64,
        pub p50_ms: f64,
        pub p95_ms: f64,
        pub p99_ms: f64,
        pub samples: Vec<ApplicationFrameSample>,
    }

    #[derive(Default)]
    struct WindowSamples {
        pending_ms: f64,
        pending_ime_ms: f64,
        pending_semantics_ms: f64,
        first_input: Option<Instant>,
        input_events: u64,
        ime_events: u64,
        frames: u64,
        over_budget: u64,
        skipped_redraws: u64,
        dropped_samples: u64,
        samples: VecDeque<ApplicationFrameSample>,
    }

    struct Timings {
        budget_ms: f64,
        windows: HashMap<WindowId, WindowSamples>,
        shared_ms: f64,
        active: Option<ActiveEvent>,
    }

    struct ActiveEvent {
        presented: bool,
        ime_ms: f64,
        semantics_ms: f64,
    }

    thread_local! {
        static TIMINGS: RefCell<Option<Timings>> = RefCell::new(
            std::env::var("JADIX_FRAME_BUDGET_MS").ok().and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value > 0.0).map(Timings::new));
    }

    impl Timings {
        fn new(budget_ms: f64) -> Self {
            Self {
                budget_ms,
                windows: HashMap::new(),
                shared_ms: 0.0,
                active: None,
            }
        }

        fn finish(
            &mut self,
            window: WindowId,
            kind: EventKind,
            start: Instant,
            end: Instant,
            event: ActiveEvent,
        ) {
            let elapsed_ms = end.duration_since(start).as_secs_f64() * 1000.0;
            let samples = self.windows.entry(window).or_default();
            if matches!(kind, EventKind::Input | EventKind::Ime) {
                samples.first_input.get_or_insert(start);
                samples.input_events += 1;
                samples.ime_events += u64::from(matches!(kind, EventKind::Ime));
            }
            if !event.presented {
                samples.pending_ms += elapsed_ms;
                samples.pending_ime_ms += event.ime_ms;
                samples.pending_semantics_ms += event.semantics_ms;
                samples.skipped_redraws += u64::from(matches!(kind, EventKind::Redraw));
                return;
            }
            let sample = ApplicationFrameSample {
                work_ms: elapsed_ms + samples.pending_ms + self.shared_ms,
                redraw_ms: elapsed_ms,
                event_ms: std::mem::take(&mut samples.pending_ms),
                shared_ms: std::mem::take(&mut self.shared_ms),
                ime_ms: event.ime_ms + std::mem::take(&mut samples.pending_ime_ms),
                semantics_ms: event.semantics_ms
                    + std::mem::take(&mut samples.pending_semantics_ms),
                input_to_present_ms: samples
                    .first_input
                    .take()
                    .map(|input| end.duration_since(input).as_secs_f64() * 1000.0),
                input_events: std::mem::take(&mut samples.input_events),
                ime_events: std::mem::take(&mut samples.ime_events),
            };
            samples.frames += 1;
            samples.over_budget += u64::from(sample.work_ms > self.budget_ms);
            if samples.samples.len() == SAMPLE_LIMIT {
                samples.samples.pop_front();
                samples.dropped_samples += 1;
            }
            samples.samples.push_back(sample);
            if trace_enabled() {
                eprintln!(
                    "JADIX_FRAME_BUDGET {}",
                    serde_json::json!({
                        "window": window.into_raw(), "budget_ms": self.budget_ms,
                        "work_ms": sample.work_ms, "redraw_ms": sample.redraw_ms,
                        "event_ms": sample.event_ms, "shared_ms": sample.shared_ms,
                        "ime_ms": sample.ime_ms, "semantics_ms": sample.semantics_ms,
                        "input_to_present_ms": sample.input_to_present_ms,
                        "input_events": sample.input_events, "ime_events": sample.ime_events,
                    })
                );
            }
        }
    }

    fn trace_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var("JADIX_FRAME_BUDGET_TRACE").as_deref() == Ok("1"))
    }

    /// Enable on the UI thread. Invalid/absent budgets disable collection and clear samples.
    pub fn enable_application_frame_timings(budget_ms: Option<f64>) {
        TIMINGS.with(|timings| {
            *timings.borrow_mut() = budget_ms
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(Timings::new)
        });
    }

    /// Drain measurements without losing input work waiting for a presented frame.
    pub fn take_application_frame_timings() -> Vec<ApplicationFrameTimings> {
        TIMINGS.with(|timings| {
            let mut timings = timings.borrow_mut();
            let Some(timings) = timings.as_mut() else {
                return Vec::new();
            };
            let mut reports = Vec::new();
            for (&window_id, window) in &mut timings.windows {
                let samples: Vec<_> = window.samples.drain(..).collect();
                let mut sorted: Vec<_> = samples.iter().map(|sample| sample.work_ms).collect();
                sorted.sort_by(f64::total_cmp);
                reports.push(ApplicationFrameTimings {
                    window_id,
                    budget_ms: timings.budget_ms,
                    frames: std::mem::take(&mut window.frames),
                    over_budget: std::mem::take(&mut window.over_budget),
                    skipped_redraws: std::mem::take(&mut window.skipped_redraws),
                    dropped_samples: std::mem::take(&mut window.dropped_samples),
                    p50_ms: percentile(&sorted, 0.50),
                    p95_ms: percentile(&sorted, 0.95),
                    p99_ms: percentile(&sorted, 0.99),
                    samples,
                });
            }
            reports.sort_by_key(|report| report.window_id.into_raw());
            reports
        })
    }

    fn percentile(sorted: &[f64], fraction: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        sorted[(sorted.len() as f64 * fraction).ceil() as usize - 1]
    }

    #[derive(Clone, Copy)]
    enum EventKind {
        Redraw,
        Input,
        Ime,
        Other,
        Destroyed,
    }

    pub(crate) struct EventGuard {
        window: WindowId,
        kind: EventKind,
        start: Instant,
    }

    pub(crate) fn begin_event(window: WindowId, event: &WindowEvent) -> Option<EventGuard> {
        TIMINGS.with(|timings| {
            let mut timings = timings.borrow_mut();
            let timings = timings.as_mut()?;
            if timings.active.is_some() {
                return None;
            }
            timings.active = Some(ActiveEvent {
                presented: false,
                ime_ms: 0.0,
                semantics_ms: 0.0,
            });
            let kind = match event {
                WindowEvent::RedrawRequested => EventKind::Redraw,
                WindowEvent::Ime(_) => EventKind::Ime,
                WindowEvent::Destroyed => EventKind::Destroyed,
                WindowEvent::KeyboardInput { .. }
                | WindowEvent::PointerMoved { .. }
                | WindowEvent::PointerButton { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::Focused(_) => EventKind::Input,
                _ => EventKind::Other,
            };
            Some(EventGuard {
                window,
                kind,
                start: Instant::now(),
            })
        })
    }

    impl Drop for EventGuard {
        fn drop(&mut self) {
            let end = Instant::now();
            TIMINGS.with(|timings| {
                if let Some(timings) = timings.borrow_mut().as_mut() {
                    if let Some(event) = timings.active.take() {
                        timings.finish(self.window, self.kind, self.start, end, event);
                    }
                }
            });
            if matches!(self.kind, EventKind::Destroyed) {
                forget_window(self.window);
            }
        }
    }

    pub(crate) fn note_presented() {
        TIMINGS.with(|timings| {
            if let Some(event) = timings
                .borrow_mut()
                .as_mut()
                .and_then(|timings| timings.active.as_mut())
            {
                event.presented = true;
            }
        });
    }

    pub(crate) struct WorkGuard {
        start: Instant,
        kind: WorkKind,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum WorkKind {
        Shared,
        Ime,
        #[cfg(feature = "a11y")]
        Semantics,
    }

    pub(crate) fn begin_work(kind: WorkKind) -> Option<WorkGuard> {
        TIMINGS.with(|timings| {
            let timings = timings.borrow();
            let timings = timings.as_ref()?;
            if matches!(kind, WorkKind::Shared) && timings.active.is_some() {
                return None;
            }
            Some(WorkGuard {
                start: Instant::now(),
                kind,
            })
        })
    }

    impl Drop for WorkGuard {
        fn drop(&mut self) {
            let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
            TIMINGS.with(|timings| {
                if let Some(timings) = timings.borrow_mut().as_mut() {
                    match self.kind {
                        WorkKind::Shared => timings.shared_ms += elapsed_ms,
                        WorkKind::Ime => {
                            if let Some(event) = timings.active.as_mut() {
                                event.ime_ms += elapsed_ms;
                            }
                        }
                        #[cfg(feature = "a11y")]
                        WorkKind::Semantics => {
                            if let Some(event) = timings.active.as_mut() {
                                event.semantics_ms += elapsed_ms;
                            }
                        }
                    }
                }
            });
        }
    }

    pub(crate) fn forget_window(window: WindowId) {
        TIMINGS.with(|timings| {
            if let Some(timings) = timings.borrow_mut().as_mut() {
                timings.windows.remove(&window);
            }
        });
    }

    #[cfg(test)]
    #[path = "tests/mod.rs"]
    mod tests;
}

#[cfg(not(feature = "performance"))]
pub(crate) use disabled::*;

#[cfg(not(feature = "performance"))]
mod disabled {
    #[derive(Clone, Copy)]
    pub(crate) enum WorkKind {
        Shared,
        Ime,
        #[cfg(feature = "a11y")]
        Semantics,
    }

    pub(crate) fn begin_event(
        _: winit::window::WindowId,
        _: &winit::event::WindowEvent,
    ) -> Option<()> {
        None
    }

    pub(crate) fn begin_work(_: WorkKind) -> Option<()> {
        None
    }

    pub(crate) fn forget_window(_: winit::window::WindowId) {}
}
