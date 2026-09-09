//! Windows console: enable VT processing on stderr/stdout, local clock via `GetLocalTime`.

use std::sync::Once;

pub(crate) fn enable_ansi_stderr() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetStdHandle(nStdHandle: i32) -> *mut std::ffi::c_void;
            fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
            fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
        }
        const STD_OUTPUT_HANDLE: i32 = -11;
        const STD_ERROR_HANDLE: i32 = -12;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        for handle in [STD_ERROR_HANDLE, STD_OUTPUT_HANDLE] {
            let h = GetStdHandle(handle);
            let mut mode = 0u32;
            if GetConsoleMode(h, &mut mode) != 0 {
                let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    });
}

pub(crate) fn local_hms() -> Option<(u16, u16, u16)> {
    #[repr(C)]
    struct SystemTime {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTime);
    }
    let mut st = SystemTime {
        w_year: 0,
        w_month: 0,
        w_day_of_week: 0,
        w_day: 0,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    Some((st.w_hour, st.w_minute, st.w_second))
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_local_hms_in_range() {
        let (h, m, s) = super::local_hms().expect("GetLocalTime");
        assert!(h < 24 && m < 60 && s < 60, "hms=({h},{m},{s})");
    }

    #[test]
    fn enable_ansi_stderr_does_not_panic() {
        super::enable_ansi_stderr();
        super::enable_ansi_stderr();
    }
}
