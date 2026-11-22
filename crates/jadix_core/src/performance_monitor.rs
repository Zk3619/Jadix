use super::{Resources, Sampler, current_thread_id};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, sync::mpsc, time::{Duration, Instant}};

const CAPACITY: usize = 120;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat { pub mean_ms: f64, pub p95_ms: f64, pub max_ms: f64 }
fn stat(values: impl Iterator<Item = f64>) -> Stat {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() { return Stat::default(); }
    values.sort_by(f64::total_cmp);
    Stat { mean_ms: values.iter().sum::<f64>() / values.len() as f64,
        p95_ms: values[(values.len() * 95).div_ceil(100).saturating_sub(1)],
        max_ms: *values.last().unwrap() }
}

/// QuickJS counters measured after collection; absence means no idle collection yet.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapSample { pub allocated_bytes: u64, pub objects: u64 }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub fps: f64,
    pub refresh_rate: Option<f64>,
    pub frame: Stat,
    pub frame_interval: Stat,
    pub tick_interval: Stat,
    pub js: Stat,
    pub core: Stat,
    pub commits_per_second: usize,
    pub dropped: f64,
    pub frames_ms: Vec<f64>,
    pub process: Resources,
    pub heap: Option<HeapSample>,
    pub live_nodes: usize,
    pub retired_nodes: usize,
}

struct Frame { at: Instant, cost: f64, interval: Option<f64> }
struct Commit { js: f64, core: f64 }

/// One bounded monitor per surface. OS counters are read on an owned worker,
/// never on the UI thread. Dropping the monitor disconnects and stops the worker.
pub struct Monitor {
    frames: VecDeque<Frame>, commits: VecDeque<Commit>, ticks: VecDeque<f64>,
    frame_times: VecDeque<Instant>, commit_times: VecDeque<Instant>,
    last_tick: Option<Instant>, refresh: Option<f64>, heap: Option<HeapSample>,
    resources: Resources, samples: mpsc::Receiver<Resources>,
    _stop: mpsc::Sender<()>,
}
impl Default for Monitor { fn default() -> Self { Self::new() } }
impl Monitor {
    pub fn new() -> Self {
        let thread = current_thread_id();
        let (send, samples) = mpsc::sync_channel(1);
        let (stop, stopped) = mpsc::channel();
        // A failed spawn leaves resource counters explicitly unavailable.
        let _ = std::thread::Builder::new().name("jadix-perf".into()).spawn(move || {
            let mut sampler = Sampler::new(thread);
            while matches!(stopped.recv_timeout(Duration::from_secs(1)), Err(mpsc::RecvTimeoutError::Timeout)) {
                let sample = sampler.sample();
                if sample.sample_seconds >= 0.25 {
                    match send.try_send(sample) {
                        Ok(()) | Err(mpsc::TrySendError::Full(_)) => {},
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
            }
        });
        Self { frames: VecDeque::new(), commits: VecDeque::new(), ticks: VecDeque::new(),
            frame_times: VecDeque::new(), commit_times: VecDeque::new(), last_tick: None,
            refresh: None, heap: None, resources: Resources::default(), samples, _stop: stop }
    }
    /// Call on an actual display tick, including ticks without surface changes.
    pub fn tick(&mut self, refresh_rate: Option<f64>) {
        let now = Instant::now();
        if let Some(last) = self.last_tick.replace(now) {
            push(&mut self.ticks, now.duration_since(last).as_secs_f64() * 1000.0);
        }
        self.refresh = refresh_rate.filter(|v| v.is_finite() && *v > 0.0 && *v <= 1000.0);
    }
    /// Cost of main-thread work producing a changed frame, not its idle gap.
    pub fn frame(&mut self, cost: Duration) {
        let now = Instant::now();
        let interval = self.frames.back().map(|f| now.duration_since(f.at).as_secs_f64() * 1000.0);
        record_rate(&mut self.frame_times, now);
        push(&mut self.frames, Frame { at: now, cost: cost.as_secs_f64() * 1000.0, interval });
    }
    pub fn commit(&mut self, js: Duration, core: Duration) {
        record_rate(&mut self.commit_times, Instant::now());
        push(&mut self.commits, Commit { js: js.as_secs_f64()*1000.0, core: core.as_secs_f64()*1000.0 });
    }
    pub fn set_heap(&mut self, heap: Option<HeapSample>) { self.heap = heap; }
    pub fn snapshot(&mut self, live_nodes: usize, retired_nodes: usize) -> Snapshot {
        while let Ok(sample) = self.samples.try_recv() { self.resources = sample; }
        let now = Instant::now();
        prune_rate(&mut self.frame_times, now);
        prune_rate(&mut self.commit_times, now);
        let budget = self.refresh.map(|hz| 1000.0 / hz);
        Snapshot {
            fps: self.frame_times.len() as f64,
            refresh_rate: self.refresh,
            frame: stat(self.frames.iter().map(|f| f.cost)),
            frame_interval: stat(self.frames.iter().filter_map(|f| f.interval)),
            tick_interval: stat(self.ticks.iter().copied()),
            js: stat(self.commits.iter().map(|c| c.js)), core: stat(self.commits.iter().map(|c| c.core)),
            commits_per_second: self.commit_times.len(),
            dropped: budget.filter(|_| !self.frames.is_empty()).map_or(0.0, |b| self.frames.iter().filter(|f| f.cost>b).count() as f64 / self.frames.len() as f64),
            frames_ms: self.frames.iter().map(|f|f.cost).collect(), process: self.resources.clone(), heap: self.heap,
            live_nodes, retired_nodes,
        }
    }
}
// Keep the one-second rates independent of the shorter graph/statistics history.
// The ceiling bounds memory even for a producer that commits without yielding.
fn record_rate(queue: &mut VecDeque<Instant>, now: Instant) {
    prune_rate(queue, now);
    if queue.len() == 10_000 { queue.pop_front(); }
    queue.push_back(now);
}
fn prune_rate(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue.front().is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1)) {
        queue.pop_front();
    }
}
fn push<T>(queue: &mut VecDeque<T>, value: T) {
    if queue.len() == CAPACITY { queue.pop_front(); }
    queue.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_history_separates_commit_cost_from_frames_and_unknown_heap() {
        let mut monitor = Monitor::new();
        for _ in 0..200 { monitor.commit(Duration::from_millis(2), Duration::from_millis(3)); monitor.frame(Duration::from_millis(20)); }
        monitor.tick(Some(60.0));
        let sample = monitor.snapshot(10, 4);
        assert_eq!(sample.frames_ms.len(), 120);
        assert_eq!(sample.fps, 200.0);
        assert_eq!(sample.commits_per_second, 200);
        assert_eq!(sample.js.mean_ms, 2.0);
        assert_eq!(sample.core.mean_ms, 3.0);
        assert_eq!(sample.dropped, 1.0);
        assert!(sample.heap.is_none());
        assert_eq!(sample.live_nodes, 10);
    }
    #[test]
    fn percentile_uses_nearest_rank() {
        let result = stat((1..=100).map(f64::from));
        assert_eq!(result.p95_ms, 95.0);
        assert_eq!(result.max_ms, 100.0);
    }
}

