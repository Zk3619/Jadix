//! Run only on a fixture-owned Unix PTY; checks process-global ownership cleanup.
#[cfg(unix)]
fn main() -> std::io::Result<()> {
    use jadix_tui::TerminalModes;
    use std::io::Write;
    let signals = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];
    let handlers: Vec<_> = signals
        .iter()
        .map(|number| {
            // SAFETY: initialized output record, read-only process action query.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                assert_eq!(libc::sigaction(*number, std::ptr::null(), &mut action), 0);
                action.sa_sigaction
            }
        })
        .collect();
    // SAFETY: read the fixture's live stdout descriptor flags.
    let flags = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_GETFL) };
    assert!(flags >= 0);
    let mut owner = TerminalModes::enter()?;
    assert!(matches!(TerminalModes::enter(),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists));
    let mut late = owner.output();
    late.write_all(b"session contract")?;
    late.flush()?;
    late.write_all(b"discard this unfinished frame")?;
    owner.begin_output_cleanup()?;
    late.write_all(b"cleanup command")?;
    late.flush()?;
    owner.restore()?;
    assert_eq!(
        late.write_all(b"must not reach stdout").unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    let mut next = TerminalModes::enter()?;
    next.restore()?;
    // SAFETY: read actions/flags only. Old closed writers remain alive here;
    // dropping them later must not affect the new session or its flags.
    unsafe {
        for (number, handler) in signals.into_iter().zip(handlers) {
            let mut action: libc::sigaction = std::mem::zeroed();
            assert_eq!(libc::sigaction(number, std::ptr::null(), &mut action), 0);
            assert_eq!(action.sa_sigaction, handler);
        }
        assert_eq!(libc::fcntl(libc::STDOUT_FILENO, libc::F_GETFL), flags);
    }
    let args: Vec<_> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--receipt") {
        std::fs::write(
            &args[index + 1],
            r#"{"status":"pass","single_owner":true,"late_writer_rejected":true,"signal_handlers_restored":true,"flags_restored":true}"#,
        )?;
    }
    Ok(())
}
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use jadix_tui::TerminalModes;
    use std::io::{self, Write};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetConsoleOutputCP};
    let modes = || -> io::Result<[u32; 2]> {
        let mut modes = [0; 2];
        // SAFETY: borrowed live Console handles and initialized mode outputs.
        unsafe {
            if GetConsoleMode(io::stdin().as_raw_handle(), &mut modes[0]) == 0
                || GetConsoleMode(io::stdout().as_raw_handle(), &mut modes[1]) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(modes)
    };
    let before = modes()?;
    // SAFETY: read-only query of the fixture-owned Console.
    let code_page = unsafe { GetConsoleOutputCP() };
    let mut owner = TerminalModes::enter()?;
    assert!(matches!(TerminalModes::enter(),Err(e) if e.kind()==io::ErrorKind::AlreadyExists));
    let active = modes()?;
    assert_ne!(before, active);
    let mut output = owner.output();
    output.write_all(b"Windows Console contract")?;
    output.flush()?;
    owner.begin_output_cleanup()?;
    owner.restore()?;
    assert_eq!(
        output.write_all(b"late").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    // SAFETY: read-only query after explicit restoration.
    assert_eq!(unsafe { GetConsoleOutputCP() }, code_page);
    let mut next = TerminalModes::enter()?;
    next.restore()?;
    assert_eq!(modes()?, before);
    let args: Vec<_> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--receipt") {
        std::fs::write(&args[index + 1], serde_json::json!({"status":"pass", "console_modes_restored":true, "single_owner":true,"late_writer_rejected":true,"code_page_restored":true,"before":before, "active":active}).to_string())?;
    }
    Ok(())
}
#[cfg(not(any(unix, windows)))]
fn main() {
    eprintln!("session_contract requires a private Unix PTY or Windows Console");
}
