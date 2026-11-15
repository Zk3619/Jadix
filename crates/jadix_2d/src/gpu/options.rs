//! Explicit execution and shader initialization controls.
use super::*;
/// Scene renderer execution controls. CPU mode rasterizes the complete scene,
/// unlike Vello's CPU-coarse/GPU-fine mode. PipelineCache is a caller-owned,
/// same-device wgpu cache; its backend persistence support is not guaranteed.
#[derive(Clone, Debug)]
pub struct RendererOptions {
    pub use_cpu: bool,
    /// Native pipeline creation workers; None uses at most eight available workers.
    /// macOS defaults to one; WebAssembly always initializes serially.
    pub num_init_threads: Option<std::num::NonZeroUsize>,
    pub pipeline_cache: Option<wgpu::PipelineCache>,
    pub antialiasing: AaQuality,
    /// CPU-mode conservative surface/mask preparation budget; maximum 1GiB.
    pub max_surface_bytes: u64,
}
impl Default for RendererOptions {
    fn default() -> Self {
        Self {
            use_cpu: false,
            max_surface_bytes: 512 * 1024 * 1024,
            num_init_threads: if cfg!(target_os = "macos") {
                std::num::NonZeroUsize::new(1)
            } else {
                None
            },
            pipeline_cache: None,
            antialiasing: AaQuality::Balanced,
        }
    }
}
impl RendererOptions {
    pub fn initialization_threads(&self) -> usize {
        if cfg!(target_arch = "wasm32") {
            return 1;
        }
        self.num_init_threads.map_or_else(
            || {
                std::thread::available_parallelism()
                    .map_or(1, |n| n.get().saturating_sub(1).clamp(1, 8))
            },
            |n| n.get(),
        )
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn initialize<T: Send>(
    count: usize,
    threads: usize,
    build: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    let workers = threads.min(count).max(1);
    if workers == 1 {
        return (0..count).map(build).collect();
    }
    std::thread::scope(|scope| {
        let build = &build;
        let jobs = (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    (worker..count)
                        .step_by(workers)
                        .map(|i| (i, build(i)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut values = jobs
            .into_iter()
            .flat_map(|job| {
                job.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>();
        values.sort_by_key(|v| v.0);
        values.into_iter().map(|v| v.1).collect()
    })
}
#[cfg(target_arch = "wasm32")]
pub(super) fn initialize<T>(count: usize, _threads: usize, build: impl Fn(usize) -> T) -> Vec<T> {
    (0..count).map(build).collect()
}
