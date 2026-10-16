pub(crate) fn process_private_mb() -> f64 {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::addr_of_mut!(counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    if ok == 0 {
        return 0.0;
    }
    counters.PrivateUsage as f64 / (1024.0 * 1024.0)
}
