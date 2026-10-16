pub(crate) fn system_scroll_lines() -> f32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut lines: u32 = 3;
    unsafe {
        if SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            &mut lines as *mut u32 as *mut _,
            0 as SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        ) != 0
        {
            // Windows: -1 (u32::MAX) means scroll a whole page.
            if lines == u32::MAX {
                return 20.0;
            }
            return lines as f32;
        }
    }
    crate::platform::desktop::system_scroll_lines()
}
