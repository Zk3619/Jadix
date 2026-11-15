//! Isolate synchronous Console/pipe writes on one owned, cancellable thread.
use crate::CancellationToken;
use std::{
    cell::{Cell, RefCell},
    fs::File,
    io::{self, Write},
    os::windows::io::{AsRawHandle, BorrowedHandle},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::System::{Console::SetConsoleCtrlHandler, IO::CancelSynchronousIo};
static OWNED: AtomicBool = AtomicBool::new(false);
static EXIT: AtomicU32 = AtomicU32::new(0);
static UI_THREAD: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
unsafe extern "system" fn control(value: u32) -> i32 {
    // Publish intent without allocation or Console IO. Close/logoff/shutdown
    // terminate the process when the handler returns, so allow the UI owner
    // a bounded grace interval to restore while this OS handler stays alive.
    if matches!(value, 0 | 1 | 2 | 5 | 6) {
        let _ = EXIT.compare_exchange(0, value + 1, Ordering::Release, Ordering::Relaxed);
        if matches!(value, 2 | 5 | 6) {
            let owner = UI_THREAD.load(Ordering::Acquire);
            if !owner.is_null() {
                // Windows kills the process when a Close handler returns. Let
                // the owner finish persistence/cleanup and return normally.
                unsafe {
                    windows_sys::Win32::System::Threading::WaitForSingleObject(owner, 4000);
                }
            }
        }
        1
    } else {
        0
    }
}
pub(crate) fn exit_signal() -> Option<i32> {
    let value = EXIT.load(Ordering::Relaxed);
    (OWNED.load(Ordering::Acquire) && value != 0).then_some(value.wrapping_sub(1) as i32)
}
// Loading user32 (including a GPU host) disables Console logoff/shutdown
// delivery. A hidden top-level window receives WM_QUERYENDSESSION/WM_ENDSESSION
// as well; message-only HWNDs do not receive those broadcasts.
unsafe extern "system" fn session_window_proc(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    match message {
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION => {
            if wparam != 0 {
                unsafe {
                    control(if lparam as u32 & 0x80000000 != 0 {
                        5
                    } else {
                        6
                    });
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(window);
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
fn session_window() -> io::Result<(usize, JoinHandle<()>)> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("jadix-terminal-system-events".into())
        .spawn(move || {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let name: Vec<u16> = "JadixTerminalSessionMonitor\0".encode_utf16().collect();
            let instance = unsafe {
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null())
            };
            let class = WNDCLASSW {
                lpfnWndProc: Some(session_window_proc),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                ..unsafe { std::mem::zeroed() }
            };
            unsafe {
                RegisterClassW(&class);
            }
            let window = unsafe {
                CreateWindowExW(
                    0,
                    name.as_ptr(),
                    name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                )
            };
            if window.is_null() {
                let _ = sender.send(Err(io::Error::last_os_error()));
                return;
            }
            if sender.send(Ok(window as usize)).is_err() {
                unsafe {
                    DestroyWindow(window);
                }
                return;
            }
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })?;
    match receiver.recv() {
        Ok(Ok(window)) => Ok((window, worker)),
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(error)
        }
        Err(_) => {
            let _ = worker.join();
            Err(io::Error::other("system-event monitor did not start"))
        }
    }
}
struct Signals {
    window: usize,
    worker: Option<JoinHandle<()>>,
}
impl Signals {
    fn install() -> io::Result<Self> {
        if OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        EXIT.store(0, Ordering::Relaxed);
        let owner = unsafe {
            windows_sys::Win32::System::Threading::OpenThread(
                0x00100000,
                0,
                windows_sys::Win32::System::Threading::GetCurrentThreadId(),
            )
        };
        if owner.is_null() {
            OWNED.store(false, Ordering::Release);
            return Err(io::Error::last_os_error());
        }
        UI_THREAD.store(owner, Ordering::Release);
        // SAFETY: process-local static handler; removed by this owner only.
        if unsafe { SetConsoleCtrlHandler(Some(control), 1) } == 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(
                    UI_THREAD.swap(std::ptr::null_mut(), Ordering::AcqRel),
                );
            }
            OWNED.store(false, Ordering::Release);
            return Err(io::Error::last_os_error());
        }
        let (window, worker) = match session_window() {
            Ok(value) => value,
            Err(error) => {
                unsafe {
                    SetConsoleCtrlHandler(Some(control), 0);
                    windows_sys::Win32::Foundation::CloseHandle(
                        UI_THREAD.swap(std::ptr::null_mut(), Ordering::AcqRel),
                    );
                }
                OWNED.store(false, Ordering::Release);
                return Err(error);
            }
        };
        Ok(Self {
            window,
            worker: Some(worker),
        })
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        if matches!(EXIT.load(Ordering::Acquire), 3 | 6 | 7) {
            // Unregistering can wait for the Close callback, which waits for the
            // owner to exit. Retain this one handler/thread handle until process
            // termination instead of creating that circular wait. No new session
            // is admitted after an OS terminal-close request.
            return;
        }
        // SAFETY: unregister precisely this handler, preserving other handlers.
        unsafe {
            SetConsoleCtrlHandler(Some(control), 0);
        }
        let owner = UI_THREAD.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !owner.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(owner);
            }
        }
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                self.window as _,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                0,
                0,
            );
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        EXIT.store(0, Ordering::Relaxed);
        OWNED.store(false, Ordering::Release);
    }
}
struct Request {
    bytes: Vec<u8>,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<io::Result<usize>>,
}
pub(crate) struct Session {
    sender: RefCell<Option<mpsc::SyncSender<Request>>>,
    worker: RefCell<Option<JoinHandle<()>>>,
    stop: Arc<AtomicBool>,
    cancellation: CancellationToken,
    signals: RefCell<Option<Signals>>,
    closed: Cell<bool>,
    failed: Cell<bool>,
    cleaning: Cell<bool>,
    unresponsive: Cell<bool>,
    written: Arc<AtomicU64>,
    last_signal: Cell<Option<i32>>,
    timeout: Duration,
    restore_status: Arc<AtomicU32>,
}
impl Session {
    pub(crate) fn new(timeout: Duration) -> io::Result<Rc<Self>> {
        let signals = Signals::install()?;
        // SAFETY: borrow stdout only long enough to duplicate its handle. The
        // worker owns the duplicate and never closes the process standard handle.
        let owned = unsafe { BorrowedHandle::borrow_raw(io::stdout().as_raw_handle()) }
            .try_clone_to_owned()?;
        Self::with_file(File::from(owned), timeout, Some(signals))
    }
    fn with_file(file: File, timeout: Duration, signals: Option<Signals>) -> io::Result<Rc<Self>> {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let written = Arc::new(AtomicU64::new(0));
        let count = written.clone();
        let worker = thread::Builder::new()
            .name("jadix-terminal-output".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let result = if stopped.load(Ordering::Acquire)
                        || request.cancelled.load(Ordering::Acquire)
                    {
                        Err(io::ErrorKind::ConnectionAborted.into())
                    } else {
                        (&file).write(&request.bytes)
                    };
                    if let Ok(n) = &result {
                        count.fetch_add(*n as u64, Ordering::Relaxed);
                    }
                    let _ = request.reply.send(result);
                }
            })?;
        Ok(Rc::new(Self {
            sender: RefCell::new(Some(sender)),
            worker: RefCell::new(Some(worker)),
            stop,
            cancellation: CancellationToken::new(),
            signals: RefCell::new(signals),
            closed: Cell::new(false),
            failed: Cell::new(false),
            cleaning: Cell::new(false),
            unresponsive: Cell::new(false),
            written,
            last_signal: Cell::new(None),
            timeout,
            restore_status: Arc::new(AtomicU32::new(0)),
        }))
    }
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub(crate) fn signal(&self) -> Option<i32> {
        self.last_signal.get().or_else(|| {
            if self.closed.get() {
                None
            } else {
                exit_signal()
            }
        })
    }
    pub(crate) fn written_bytes(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }
    pub(crate) fn console_restore_status(&self) -> io::Result<()> {
        match self.restore_status.load(Ordering::Acquire) {
            0 | 2 => Ok(()),
            1 => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Console restoration is pending; output/driver remains stalled",
            )),
            _ => Err(io::Error::other("Console restoration failed")),
        }
    }
    /// Console mode RPCs can block behind a stopped ConPTY reader too. Keep
    /// them off the UI thread, retain the session lease until restoration ends,
    /// and join a non-cancellable output worker before changing its Console.
    pub(crate) fn restore_console(
        &self,
        input: Option<u32>,
        output: Option<u32>,
        code_page: Option<u32>,
    ) -> io::Result<()> {
        if self.restore_status.load(Ordering::Acquire) != 0 {
            return self.console_restore_status();
        }
        let duplicate = |handle| unsafe { BorrowedHandle::borrow_raw(handle) }.try_clone_to_owned();
        let stdin = duplicate(io::stdin().as_raw_handle())?;
        let stdout = duplicate(io::stdout().as_raw_handle())?;
        let signals = self.signals.borrow_mut().take();
        let _ = self.finish();
        let worker = self.worker.borrow_mut().take();
        let status = self.restore_status.clone();
        status.store(1, Ordering::Release);
        let (reply, completion) = mpsc::sync_channel(1);
        let spawned = thread::Builder::new()
            .name("jadix-console-restore".into())
            .spawn(move || {
                if let Some(worker) = worker {
                    let _ = worker.join();
                }
                let result = (|| {
                    crossterm::terminal::disable_raw_mode()?;
                    use windows_sys::Win32::System::Console::{SetConsoleMode, SetConsoleOutputCP};
                    for (handle, mode) in [
                        (stdin.as_raw_handle(), input),
                        (stdout.as_raw_handle(), output),
                    ] {
                        if let Some(mode) = mode {
                            if unsafe { SetConsoleMode(handle, mode) } == 0 {
                                return Err(io::Error::last_os_error());
                            }
                        }
                    }
                    if let Some(cp) = code_page {
                        if unsafe { SetConsoleOutputCP(cp) } == 0 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    Ok(())
                })();
                // Release the OS session lease only after all owned IO/mode work.
                drop(signals);
                status.store(if result.is_ok() { 2 } else { 3 }, Ordering::Release);
                let _ = reply.send(());
            });
        if let Err(error) = spawned {
            self.restore_status.store(3, Ordering::Release);
            return Err(error);
        }
        let _ = completion.recv_timeout(Duration::from_millis(100));
        self.console_restore_status()
    }
    pub(crate) fn check_write(&self) -> io::Result<()> {
        if self.closed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.cleaning.get() {
            return if self.unresponsive.get() {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(())
            };
        }
        if self.failed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.cancellation.is_cancelled() || exit_signal().is_some() {
            self.failed.set(true);
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        Ok(())
    }
    fn cancel_native(&self) {
        if let Some(worker) = self.worker.borrow().as_ref() {
            // SAFETY: borrowed owned worker thread handle, no expression or
            // forced thread termination. Repeat to cover check→WriteFile races.
            unsafe {
                CancelSynchronousIo(worker.as_raw_handle());
            }
        }
    }
    pub(crate) fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        self.check_write()?;
        if bytes.is_empty() {
            return Ok(0);
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = Request {
            bytes: bytes[..bytes.len().min(16 * 1024)].to_vec(),
            cancelled: cancelled.clone(),
            reply,
        };
        self.sender
            .borrow()
            .as_ref()
            .ok_or(io::ErrorKind::BrokenPipe)?
            .try_send(request)
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => io::Error::from(io::ErrorKind::WouldBlock),
                mpsc::TrySendError::Disconnected(_) => io::Error::from(io::ErrorKind::BrokenPipe),
            })?;
        let deadline = Instant::now()
            + if self.cleaning.get() {
                Duration::from_millis(20)
            } else {
                self.timeout
            };
        loop {
            let reason = if !self.cleaning.get()
                && (self.cancellation.is_cancelled() || exit_signal().is_some())
            {
                Some(io::ErrorKind::ConnectionAborted)
            } else if Instant::now() >= deadline {
                Some(io::ErrorKind::TimedOut)
            } else {
                None
            };
            if let Some(reason) = reason {
                self.failed.set(true);
                cancelled.store(true, Ordering::Release);
                // Cancellation is a request, not completion. Keep the buffer on
                // the worker and observe completion before permitting cleanup.
                for _ in 0..10 {
                    self.cancel_native();
                    match receiver.recv_timeout(Duration::from_millis(10)) {
                        Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err(reason.into())
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                // A driver that refuses cancellation cannot block UI teardown.
                // Its owned worker may outlive the session; no further writes
                // are accepted and its buffer/handle remain owned until exit.
                self.unresponsive.set(true);
                return Err(reason.into());
            }
            match receiver.recv_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(16)),
            ) {
                Ok(result) => {
                    if result.is_err() {
                        self.failed.set(true);
                    }
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.failed.set(true);
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    pub(crate) fn is_cleaning(&self) -> bool {
        self.cleaning.get()
    }
    pub(crate) fn begin_cleanup(&self) -> io::Result<()> {
        if self.closed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.cleaning.replace(true) {
            return Ok(());
        }
        self.cleanup(b"\x18\x1b\\")
    }
    pub(crate) fn cleanup(&self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let n = self.write(bytes)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> io::Result<()> {
        if self.closed.replace(true) {
            return Ok(());
        }
        self.last_signal.set(exit_signal());
        self.stop.store(true, Ordering::Release);
        self.sender.borrow_mut().take();
        for _ in 0..10 {
            if self
                .worker
                .borrow()
                .as_ref()
                .is_none_or(JoinHandle::is_finished)
            {
                break;
            }
            self.cancel_native();
            thread::sleep(Duration::from_millis(2));
        }
        if self
            .worker
            .borrow()
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            if let Some(worker) = self.worker.borrow_mut().take() {
                let _ = worker.join();
            }
        }
        self.signals.borrow_mut().take();
        if self.worker.borrow().is_some() {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_handler_accepts_only_documented_windows_control_events() {
        for value in [0, 1, 2, 5, 6] {
            EXIT.store(0, Ordering::Relaxed);
            assert_eq!(unsafe { control(value) }, 1);
            assert_eq!(EXIT.load(Ordering::Relaxed), value + 1);
        }
        for value in [3, 4, 7, u32::MAX] {
            EXIT.store(0, Ordering::Relaxed);
            assert_eq!(unsafe { control(value) }, 0);
            assert_eq!(EXIT.load(Ordering::Relaxed), 0);
        }
    }
    use std::{
        io::Read,
        os::windows::io::{FromRawHandle, OwnedHandle},
    };
    fn pipe() -> (File, File) {
        let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: initialized output handles; each success is uniquely owned.
        assert_ne!(
            unsafe {
                windows_sys::Win32::System::Pipes::CreatePipe(
                    &mut read,
                    &mut write,
                    std::ptr::null(),
                    4096,
                )
            },
            0
        );
        unsafe {
            (
                File::from(OwnedHandle::from_raw_handle(read)),
                File::from(OwnedHandle::from_raw_handle(write)),
            )
        }
    }
    #[test]
    fn stalled_pipe_cancels_native_io_and_rejects_late_writers() {
        let (_reader, writer) = pipe();
        let session = Session::with_file(writer, Duration::from_millis(40), None).unwrap();
        let start = Instant::now();
        let mut error = None;
        for _ in 0..100 {
            if let Err(e) = session.write(&[b'x'; 16384]) {
                error = Some(e.kind());
                break;
            }
        }
        assert_eq!(error, Some(io::ErrorKind::TimedOut));
        assert!(start.elapsed() < Duration::from_secs(2));
        session.finish().unwrap();
        assert_eq!(
            session.write(b"late").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
    #[test]
    fn cancellation_from_another_thread_and_successful_byte_accounting() {
        let (mut reader, writer) = pipe();
        let session = Session::with_file(writer, Duration::from_secs(5), None).unwrap();
        assert_eq!(session.write(b"hello").unwrap(), 5);
        let mut data = [0; 5];
        reader.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"hello");
        assert_eq!(session.written_bytes(), 5);
        let token = session.cancellation();
        let cancel = thread::spawn(move || {
            thread::sleep(Duration::from_millis(40));
            token.cancel();
        });
        let start = Instant::now();
        let error = session.write(&[b'x'; 16384]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(start.elapsed() < Duration::from_secs(2));
        cancel.join().unwrap();
        session.begin_cleanup().ok();
        session.finish().unwrap();
    }
}
