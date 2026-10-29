use super::*;
#[cfg(windows)]
#[test]
fn windows_sampler_reads_live_process_and_thread_counters() {
    let mut sampler = Sampler::new(current_thread_id());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let sample = sampler.sample();
    assert_eq!(sample.process_id, std::process::id());
    assert!(sample.process_percent.is_some());
    assert!(sample.ui_percent.is_some());
    assert!(sample.working_set.is_some_and(|bytes| bytes > 0));
    assert!(sample.private_bytes.is_some_and(|bytes| bytes > 0));
    assert!(sample.logical_cpus >= 1);
}

#[test]
fn cpu_percent_uses_measured_time_and_core_capacity() {
    assert_eq!(cpu_percent(0, 20_000_000, 0.5), 400.0);
    assert_eq!(cpu_percent(20_000_000, 0, 0.5), 0.0);
}
