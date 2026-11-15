//! A level-triggered OS wake source. Queue state owns reset/notify ordering.
use std::{io, time::Duration};

#[cfg(unix)]
pub(crate) struct Wake {
    read: std::os::unix::net::UnixStream,
    write: std::os::unix::net::UnixStream,
}
#[cfg(unix)]
impl Wake {
    pub fn new() -> io::Result<Self> {
        let (read, write) = std::os::unix::net::UnixStream::pair()?;
        read.set_nonblocking(true)?;
        write.set_nonblocking(true)?;
        Ok(Self { read, write })
    }
    pub fn notify(&self) -> io::Result<()> {
        use std::io::Write;
        match (&self.write).write(&[1]) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn reset(&self) {
        use std::io::Read;
        let mut bytes = [0; 64];
        while (&self.read).read(&mut bytes).is_ok_and(|n| n > 0) {}
    }
    /// true means wake, false means input/deadline. Never consumes stdin.
    pub fn wait(&self, input: Option<i32>, timeout: Duration) -> io::Result<bool> {
        use std::os::fd::AsRawFd;
        let mut fds = [
            libc::pollfd {
                fd: self.read.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: input.unwrap_or(-1),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: both initialized pollfd entries live for the synchronous call.
        let result = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                2,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(e);
        }
        Ok(fds[0].revents != 0)
    }
}

#[cfg(windows)]
pub(crate) struct Wake(windows_sys::Win32::Foundation::HANDLE);
// SAFETY: the manual-reset kernel event supports concurrent waits/signals.
// Its handle remains owned until the last Arc<Queue> has been dropped.
#[cfg(windows)]
unsafe impl Send for Wake {}
#[cfg(windows)]
unsafe impl Sync for Wake {}
#[cfg(windows)]
impl Wake {
    pub fn new() -> io::Result<Self> {
        // SAFETY: default security, unnamed manual-reset event, initially clear.
        let handle = unsafe {
            windows_sys::Win32::System::Threading::CreateEventW(
                std::ptr::null(),
                1,
                0,
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
    pub fn notify(&self) -> io::Result<()> {
        // SAFETY: the event is owned by self for the complete call.
        if unsafe { windows_sys::Win32::System::Threading::SetEvent(self.0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn reset(&self) {
        // SAFETY: reset is serialized with producer admission by the queue mutex.
        unsafe {
            windows_sys::Win32::System::Threading::ResetEvent(self.0);
        }
    }
    pub fn wait(
        &self,
        input: Option<windows_sys::Win32::Foundation::HANDLE>,
        timeout: Duration,
    ) -> io::Result<bool> {
        use windows_sys::Win32::{
            Foundation::WAIT_FAILED, System::Threading::WaitForMultipleObjects,
        };
        let handles = [self.0, input.unwrap_or(std::ptr::null_mut())];
        // SAFETY: handles are borrowed and valid for the wait; count excludes null.
        let result = unsafe {
            WaitForMultipleObjects(
                if input.is_some() { 2 } else { 1 },
                handles.as_ptr(),
                0,
                timeout.as_millis().min(u32::MAX as u128 - 1) as u32,
            )
        };
        if result == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(result == 0)
    }
}
#[cfg(windows)]
impl Drop for Wake {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner and all borrowed calls have returned.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
