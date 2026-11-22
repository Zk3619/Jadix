//! Linux/Android process counters. Read only this process and its selected UI
//! thread; denied counters stay unavailable instead of becoming measured zeroes.

fn stat(text: &str) -> Option<(u64, u64)> {
    // comm is parenthesized and may itself contain spaces and ')' characters.
    let fields: Vec<_> = text.rsplit_once(") ")?.1.split_whitespace().collect();
    let ticks = fields.get(11)?.parse::<u64>().ok()?.checked_add(fields.get(12)?.parse().ok()?)?;
    let resident_pages = fields.get(21)?.parse::<u64>().ok()?;
    Some((ticks, resident_pages))
}

fn private_bytes(text: &str) -> Option<u64> {
    let mut clean = None;
    let mut dirty = None;
    let mut huge = 0;
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else { continue };
        if !matches!(name, "Private_Clean" | "Private_Dirty" | "Private_Hugetlb") { continue; }
        let mut value = value.split_whitespace();
        let bytes = value.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
        if value.next()? != "kB" || value.next().is_some() { return None; }
        match name {
            "Private_Clean" => { if clean.replace(bytes).is_some() { return None; } }
            "Private_Dirty" => { if dirty.replace(bytes).is_some() { return None; } }
            _ => huge = bytes,
        }
    }
    clean?.checked_add(dirty?)?.checked_add(huge)
}

#[cfg(any(target_os = "android", all(target_os = "linux", not(target_env = "ohos"))))]
mod os {
    use super::*;
    use crate::performance::{Instant, Resources};
    use std::io::Read;

    fn read(path: &str) -> Option<String> {
        let mut text = String::new();
        std::fs::File::open(path).ok()?.take(65537).read_to_string(&mut text).ok()?;
        (text.len() <= 65536).then_some(text)
    }
    fn counters(path: &str) -> Option<(u64, u64)> { stat(&read(path)?) }
    pub fn current_thread_id() -> u32 {
        // gettid is available on every supported Android API level.
        unsafe { libc::gettid() as u32 }
    }
    pub struct Sampler {
        thread_path: String,
        process: Option<u64>,
        thread: Option<u64>,
        ticks_per_second: Option<f64>,
        page_bytes: Option<u64>,
        last: Instant,
    }
    impl Sampler {
        pub fn new(ui: u32) -> Self {
            let thread_path = format!("/proc/self/task/{ui}/stat");
            // sysconf reports failures as -1; never cast these into huge counters.
            let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            let pages = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            Self {
                process: counters("/proc/self/stat").map(|v| v.0),
                thread: counters(&thread_path).map(|v| v.0),
                thread_path,
                ticks_per_second: (ticks > 0).then_some(ticks as f64),
                page_bytes: (pages > 0).then_some(pages as u64),
                last: Instant::now(),
            }
        }
        pub fn sample(&mut self) -> Resources {
            let now = Instant::now();
            let seconds = now.duration_since(self.last).as_secs_f64();
            self.last = now;
            let process = counters("/proc/self/stat");
            let thread = counters(&self.thread_path).map(|v| v.0);
            let percent = |before: Option<u64>, after: Option<u64>| {
                before.zip(after).zip(self.ticks_per_second).and_then(|((p, c), rate)| {
                    c.checked_sub(p).map(|delta| delta as f64 * 100.0 / rate / seconds.max(0.001))
                })
            };
            let sample = Resources {
                process_id: std::process::id(), sample_seconds: seconds,
                process_percent: percent(self.process, process.map(|v| v.0)),
                ui_percent: percent(self.thread, thread),
                logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
                working_set: process.and_then(|v| v.1.checked_mul(self.page_bytes?)),
                private_bytes: read("/proc/self/smaps_rollup").and_then(|text| private_bytes(&text)),
                ..Default::default()
            };
            self.process = process.map(|v| v.0);
            self.thread = thread;
            sample
        }
    }
}
#[cfg(any(target_os = "android", all(target_os = "linux", not(target_env = "ohos"))))]
pub use os::{current_thread_id, Sampler};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cpu_and_resident_pages_with_parentheses_in_thread_name() {
        let mut fields = vec!["0"; 22];
        fields[0] = "R"; fields[11] = "900"; fields[12] = "42"; fields[21] = "1234";
        let text = format!("42 (thread ) ( with spaces) {}", fields.join(" "));
        assert_eq!(stat(&text), Some((942, 1234)));
        assert_eq!(stat("42 (truncated) R 1"), None);
        fields[21] = "-1";
        assert!(stat(&format!("42 (thread) {}", fields.join(" "))).is_none());
        fields[11] = "18446744073709551615";
        fields[21] = "0";
        assert!(stat(&format!("42 (thread) {}", fields.join(" "))).is_none());
    }
    #[test]
    fn private_memory_requires_complete_measured_counters() {
        assert_eq!(private_bytes("Rss: 99 kB\nPrivate_Clean: 3 kB\nPrivate_Dirty: 5 kB\nPrivate_Hugetlb: 2 kB\n"), Some(10240));
        assert_eq!(private_bytes("Private_Dirty: 0 kB\nPrivate_Clean: 0 kB"), Some(0));
        for input in ["", "Private_Dirty: 3 kB", "Private_Clean: 3 B\nPrivate_Dirty: 5 kB", "Private_Clean: 3 kB\nPrivate_Clean: 4 kB\nPrivate_Dirty: 5 kB"] {
            assert_eq!(private_bytes(input), None);
        }
    }
    #[cfg(any(target_os = "android", all(target_os = "linux", not(target_env = "ohos"))))]
    #[test]
    fn samples_own_process_and_selected_thread() {
        let mut sampler = Sampler::new(current_thread_id());
        let sample = sampler.sample();
        assert_eq!(sample.process_id, std::process::id());
        assert!(sample.process_percent.is_some()); assert!(sample.ui_percent.is_some());
        assert!(sample.working_set.is_some_and(|bytes| bytes > 0));
        assert!(sample.gpu_percent.is_none());
    }
}
