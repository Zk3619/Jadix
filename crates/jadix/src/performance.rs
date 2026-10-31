//! Slow process sampling stays off the render thread. Percentages use measured CPU time;
//! GPU counters describe this process's busiest engine, not CPU submission duration.
//! Requires the `performance` feature. Call `Sampler::sample` on a background thread.
//! Frame and presentation timing APIs are in [`crate::app::frame_stats`].
#[cfg(windows)]
use web_time::Instant;

/// Measured process resource use. CPU percentages use one logical core as 100%.
/// Unsupported or unavailable OS counters are `None`; GPU and memory are process-wide.
#[derive(Debug, Default)]
pub struct Resources {
    pub process_id: u32,
    /// Elapsed wall time since the preceding sample.
    pub sample_seconds: f64,
    /// CPU use of the thread selected when creating the sampler; 100% is one core.
    pub ui_percent: Option<f64>,
    /// Total process CPU use; divide by `logical_cpus` for machine capacity percent.
    pub process_percent: Option<f64>,
    pub logical_cpus: usize,
    /// Resident process memory in bytes.
    pub working_set: Option<u64>,
    /// Private process committed memory in bytes.
    pub private_bytes: Option<u64>,
    /// Utilization of the busiest GPU engine used by this process.
    pub gpu_percent: Option<f64>,
    /// Dedicated GPU memory charged to the process, in bytes.
    pub gpu_dedicated: Option<u64>,
    /// Shared GPU memory charged to the process, in bytes.
    pub gpu_shared: Option<u64>,
}

#[cfg(any(windows, test))]
fn cpu_percent(previous: u64, current: u64, seconds: f64) -> f64 {
    current.saturating_sub(previous) as f64 / 100_000.0 / seconds.max(0.001)
}

#[cfg(not(windows))]
pub struct Sampler;
#[cfg(not(windows))]
impl Sampler {
    pub fn new(_: u32) -> Self {
        Self
    }
    pub fn sample(&mut self) -> Resources {
        Resources {
            process_id: std::process::id(),
            logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
            ..Default::default()
        }
    }
}
#[cfg(not(windows))]
pub fn current_thread_id() -> u32 {
    0
}

#[cfg(windows)]
pub use windows::{current_thread_id, Sampler};

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetProcessTimes, GetThreadTimes};
    type Handle = *mut c_void;
    type FileTime = FILETIME;
    fn filetime_ticks(time: &FileTime) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }
    #[repr(C)]
    #[derive(Default)]
    struct Memory {
        cb: u32,
        faults: u32,
        peak_working: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[repr(C)]
    struct CounterValue {
        status: u32,
        value: f64,
    }
    #[repr(C)]
    struct CounterItem {
        name: *const u16,
        value: CounterValue,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn GetCurrentThreadId() -> u32;
        fn OpenThread(access: u32, inherit: i32, id: u32) -> Handle;
        fn CloseHandle(h: Handle) -> i32;
        fn K32GetProcessMemoryInfo(h: Handle, counters: *mut Memory, size: u32) -> i32;
    }
    #[link(name = "pdh")]
    unsafe extern "system" {
        fn PdhOpenQueryW(source: *const u16, data: usize, query: *mut Handle) -> u32;
        fn PdhAddEnglishCounterW(
            query: Handle,
            path: *const u16,
            data: usize,
            counter: *mut Handle,
        ) -> u32;
        fn PdhCollectQueryData(query: Handle) -> u32;
        fn PdhGetFormattedCounterArrayW(
            counter: Handle,
            format: u32,
            bytes: *mut u32,
            count: *mut u32,
            items: *mut CounterItem,
        ) -> u32;
        fn PdhCloseQuery(query: Handle) -> u32;
    }

    pub fn current_thread_id() -> u32 {
        unsafe { GetCurrentThreadId() }
    }
    fn cpu_ticks(thread: Option<Handle>) -> Option<u64> {
        let (mut create, mut exit, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        let ok = unsafe {
            if let Some(thread) = thread {
                GetThreadTimes(thread, &mut create, &mut exit, &mut kernel, &mut user)
            } else {
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut create,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            }
        };
        (ok != 0).then(|| filetime_ticks(&kernel) + filetime_ticks(&user))
    }

    /// Holds process/thread counter baselines and owns OS sampling handles.
    pub struct Sampler {
        ui: Handle,
        process: Option<u64>,
        thread: Option<u64>,
        last: Instant,
        query: Handle,
        gpu: Handle,
        dedicated: Handle,
        shared: Handle,
    }
    impl Sampler {
        /// Create on the sampling thread, passing the UI thread ID captured with
        /// [`current_thread_id`]. Sampling may block on OS counters.
        pub fn new(thread_id: u32) -> Self {
            let ui = unsafe { OpenThread(0x0800, 0, thread_id) };
            let mut sampler = Self {
                ui,
                process: cpu_ticks(None),
                thread: cpu_ticks(Some(ui)),
                last: Instant::now(),
                query: ptr::null_mut(),
                gpu: ptr::null_mut(),
                dedicated: ptr::null_mut(),
                shared: ptr::null_mut(),
            };
            if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut sampler.query) } == 0 {
                let pid = std::process::id();
                sampler.gpu =
                    sampler.counter(&format!(r"\GPU Engine(pid_{pid}_*)\Utilization Percentage"));
                sampler.dedicated = sampler.counter(&format!(
                    r"\GPU Process Memory(pid_{pid}_*)\Dedicated Usage"
                ));
                sampler.shared =
                    sampler.counter(&format!(r"\GPU Process Memory(pid_{pid}_*)\Shared Usage"));
                unsafe {
                    PdhCollectQueryData(sampler.query);
                }
            }
            sampler
        }
        fn counter(&self, path: &str) -> Handle {
            let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut counter = ptr::null_mut();
            if unsafe { PdhAddEnglishCounterW(self.query, path.as_ptr(), 0, &mut counter) } != 0 {
                return ptr::null_mut();
            }
            counter
        }
        fn values(counter: Handle) -> Option<Vec<(String, f64)>> {
            if counter.is_null() {
                return None;
            }
            let (mut bytes, mut count) = (0, 0);
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    0x200 | 0x1000 | 0x8000,
                    &mut bytes,
                    &mut count,
                    ptr::null_mut(),
                )
            };
            if status != 0x800007D2 || bytes == 0 {
                return None;
            } // PDH_MORE_DATA
              // PDH items include pointers into the buffer; keep aligned storage alive until copied.
            let mut buffer = vec![0_u64; (bytes as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr().cast::<CounterItem>();
            if unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    0x200 | 0x1000 | 0x8000,
                    &mut bytes,
                    &mut count,
                    items,
                )
            } != 0
            {
                return None;
            }
            let mut values = Vec::new();
            for item in unsafe { std::slice::from_raw_parts(items, count as usize) } {
                if item.value.status > 1 || !item.value.value.is_finite() {
                    continue;
                }
                let mut len = 0;
                unsafe {
                    while *item.name.add(len) != 0 {
                        len += 1;
                    }
                }
                let name =
                    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.name, len) });
                values.push((name, item.value.value.max(0.0)));
            }
            (!values.is_empty()).then_some(values)
        }
        pub fn sample(&mut self) -> Resources {
            let now = Instant::now();
            let seconds = now.duration_since(self.last).as_secs_f64();
            self.last = now;
            let process = cpu_ticks(None);
            let thread = cpu_ticks(Some(self.ui));
            let mut result = Resources {
                process_id: std::process::id(),
                sample_seconds: seconds,
                ui_percent: self
                    .thread
                    .zip(thread)
                    .map(|(p, c)| cpu_percent(p, c, seconds)),
                process_percent: self
                    .process
                    .zip(process)
                    .map(|(p, c)| cpu_percent(p, c, seconds)),
                logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
                ..Default::default()
            };
            self.process = process;
            self.thread = thread;
            let mut memory = Memory {
                cb: std::mem::size_of::<Memory>() as u32,
                ..Default::default()
            };
            if unsafe {
                K32GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    &mut memory,
                    std::mem::size_of::<Memory>() as u32,
                )
            } != 0
            {
                result.working_set = Some(memory.working as u64);
                result.private_bytes = Some(memory.private as u64);
            }
            if !self.query.is_null() && unsafe { PdhCollectQueryData(self.query) } == 0 {
                // Per-process GPU counters can have several contexts on one physical engine.
                // Sum those contexts, then report the busiest engine (as Task Manager does).
                result.gpu_percent = Self::values(self.gpu).map(|items| {
                    let mut engines = std::collections::HashMap::<String, f64>::new();
                    for (name, value) in items {
                        let engine = name
                            .split_once("_luid_")
                            .map_or(name.as_str(), |(_, suffix)| suffix)
                            .split('#')
                            .next()
                            .unwrap_or(&name)
                            .to_string();
                        *engines.entry(engine).or_default() += value;
                    }
                    engines.values().copied().fold(0.0, f64::max).min(100.0)
                });
                result.gpu_dedicated =
                    Self::values(self.dedicated).map(|v| v.iter().map(|(_, n)| *n as u64).sum());
                result.gpu_shared =
                    Self::values(self.shared).map(|v| v.iter().map(|(_, n)| *n as u64).sum());
            }
            result
        }
    }
    impl Drop for Sampler {
        fn drop(&mut self) {
            unsafe {
                if !self.query.is_null() {
                    PdhCloseQuery(self.query);
                }
                if !self.ui.is_null() {
                    CloseHandle(self.ui);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/performance.rs"]
mod tests;
