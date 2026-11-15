//! Owned hidden Console acceptance only; no control is sent to other consoles.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use jadix_tui::TerminalModes;
    use std::{
        io::{self, Write},
        os::windows::io::AsRawHandle,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetConsoleOutputCP, GetConsoleWindow, SetConsoleCtrlHandler,
    };
    let path = std::env::args().nth(1).ok_or("receipt path required")?;
    unsafe {
        SetConsoleCtrlHandler(None, 0);
    }
    let input = io::stdin().as_raw_handle();
    let output = io::stdout().as_raw_handle();
    let mut before = [0, 0];
    assert_ne!(unsafe { GetConsoleMode(input, &mut before[0]) }, 0);
    assert_ne!(unsafe { GetConsoleMode(output, &mut before[1]) }, 0);
    let cp = unsafe { GetConsoleOutputCP() };
    let mut modes = TerminalModes::enter()?;
    modes.output().write_all(b"owned control fixture")?;
    std::fs::write(
        format!("{path}.ready"),
        serde_json::json!({"pid":std::process::id(),"window":unsafe{GetConsoleWindow()} as usize})
            .to_string(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let signal = loop {
        if let Some(value) = modes.exit_signal() {
            break value;
        }
        if Instant::now() > deadline {
            return Err("control event not delivered".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let start = Instant::now();
    let cleanup = modes.begin_output_cleanup();
    let restored = modes.restore();
    let mut after = [0, 0];
    assert_ne!(unsafe { GetConsoleMode(input, &mut after[0]) }, 0);
    assert_ne!(unsafe { GetConsoleMode(output, &mut after[1]) }, 0);
    std::fs::write(&path,serde_json::json!({"signal":signal,"restored":restored.is_ok(),"cleanup":cleanup.is_ok(),"exact_modes":before==after,"exact_codepage":cp==unsafe{GetConsoleOutputCP()},"restore_ms":start.elapsed().as_millis(),"pid":std::process::id()}).to_string())?;
    restored?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows owned-console fixture required");
}
