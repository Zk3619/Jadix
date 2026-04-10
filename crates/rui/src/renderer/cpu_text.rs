use std::time::{Duration, Instant};

#[derive(Default, Debug, Clone, Copy)]
pub struct CpuTextStats {
    pub frames: u64,
    pub text_nodes: u64,
    pub total_prepare_ms: f64,
}

pub struct CpuTextProfiler {
    stats: CpuTextStats,
    last_log: Instant,
}

impl CpuTextProfiler {
    pub fn new() -> Self {
        Self {
            stats: CpuTextStats::default(),
            last_log: Instant::now(),
        }
    }

    pub fn record(&mut self, prepare_ms: f64, text_nodes: usize) {
        self.stats.frames = self.stats.frames.saturating_add(1);
        self.stats.text_nodes = self.stats.text_nodes.saturating_add(text_nodes as u64);
        self.stats.total_prepare_ms += prepare_ms.max(0.0);
        if self.last_log.elapsed() >= Duration::from_secs(1) {
            let frames = self.stats.frames.max(1) as f64;
            log::warn!(
                "cpu_text_hybrid stats: avg_prepare_ms={:.2}, avg_text_nodes={:.1}, frames={}",
                self.stats.total_prepare_ms / frames,
                self.stats.text_nodes as f64 / frames,
                self.stats.frames
            );
            self.stats = CpuTextStats::default();
            self.last_log = Instant::now();
        }
    }
}

