//! Run only inside the owned ConPTY acceptance fixture, with its reader paused.
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use jadix_tui::TerminalModes;
    use std::{
        io::{self, Write},
        os::windows::io::AsRawHandle,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetConsoleOutputCP};
    let args: Vec<_> = std::env::args().collect();
    let index = args
        .iter()
        .position(|s| s == "--receipt")
        .ok_or_else(|| io::Error::other("private fixture receipt required"))?;
    let path = &args[index + 1];
    let cancel = args.iter().any(|s| s == "--cancel");
    let continuous = args.iter().any(|s| s == "--continuous-stall");
    let query = || -> io::Result<[u32; 3]> {
        let mut modes = [0; 3];
        // SAFETY: live borrowed Console handles and initialized mode outputs.
        unsafe {
            if GetConsoleMode(io::stdin().as_raw_handle(), &mut modes[0]) == 0
                || GetConsoleMode(io::stdout().as_raw_handle(), &mut modes[1]) == 0
            {
                return Err(io::Error::last_os_error());
            }
            modes[2] = GetConsoleOutputCP();
        }
        Ok(modes)
    };
    let before = query()?;
    let mut owner = TerminalModes::enter_with_output_timeout(Duration::from_millis(if cancel {
        5000
    } else {
        60
    }))?;
    let mut output = owner.output();
    output.flush()?;
    std::fs::write(format!("{path}.ready"), b"ready")?;
    // The fixture pauses its consumer before delivering this trigger.
    while !std::path::Path::new(&format!("{path}.go")).exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let cancellation = output.cancellation().unwrap();
    let worker = cancel.then(|| {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            cancellation.cancel();
        })
    });
    let start = Instant::now();
    let chunk = vec![b'X'; 16384];
    let outcome = (|| -> io::Result<()> {
        for _ in 0..4096 {
            output.write_all(&chunk)?;
            output.flush()?;
        }
        Err(io::Error::other("fixture did not create backpressure"))
    })();
    let elapsed_ms = start.elapsed().as_millis();
    std::fs::write(format!("{path}.output-done"), elapsed_ms.to_string())?;
    if let Some(worker) = worker {
        worker.join().unwrap();
    }
    let error = outcome.unwrap_err().kind();
    assert_eq!(
        error,
        if cancel {
            io::ErrorKind::ConnectionAborted
        } else {
            io::ErrorKind::TimedOut
        }
    );
    assert!(elapsed_ms < 2000, "unbounded output: {elapsed_ms} ms");
    if continuous {
        let start = Instant::now();
        let cleanup = owner.begin_output_cleanup();
        let restored = owner.restore();
        let restore_ms = start.elapsed().as_millis();
        assert!(
            restore_ms < 1000,
            "Console restoration blocked UI: {restore_ms} ms"
        );
        assert_eq!(
            output.write_all(b"late").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        std::fs::write(path,serde_json::json!({"status":"pass","continuous_stall":true,"elapsed_ms":elapsed_ms,"restore_ms":restore_ms,"restore_complete":restored.is_ok(),"restore_error":restored.as_ref().err().map(ToString::to_string),"cleanup_complete":cleanup.is_ok(),"late_writer_rejected":true,"consumer_resumed_before_receipt":false}).to_string())?;
        return Ok(());
    }
    // ConHost serializes even mode RPCs behind its stalled screen pipe. The
    // fixture observes the bounded IO result before resuming that consumer.
    while !std::path::Path::new(&format!("{path}.resume")).exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let cleanup = owner.begin_output_cleanup().and_then(|()| owner.restore());
    // A stalled pipe can prevent VT cleanup; Console flags/codepage must still
    // restore and stale writer handles must reject output on every exit path.
    owner.restore()?;
    assert_eq!(query()?, before);
    assert_eq!(
        output.write_all(b"late").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    std::fs::write(path, serde_json::json!({"status":"pass", "cancelled":cancel,
        "error":format!("{error:?}"), "elapsed_ms":elapsed_ms, "console_modes_and_codepage_restored":true,
        "late_writer_rejected":true, "vt_cleanup_complete":cleanup.is_ok()}).to_string())?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("private Windows ConPTY fixture only");
}
