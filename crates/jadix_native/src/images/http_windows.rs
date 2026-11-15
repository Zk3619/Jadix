use super::{ImageError, Loaded};
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, FILETIME, SYSTEMTIME},
    Networking::WinHttp::*,
    System::Time::SystemTimeToFileTime,
};

fn freshness(
    headers: &str,
    date: SystemTime,
    now: SystemTime,
    delay: Duration,
) -> Option<Duration> {
    let mut max_age = None;
    let mut age = None;
    let mut dates = 0;
    for line in headers
        .split("\r\n")
        .skip(1)
        .filter(|line| !line.is_empty())
    {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("cache-control") {
            for directive in value.split(',') {
                let (name, value) = directive
                    .trim()
                    .split_once('=')
                    .map_or((directive.trim(), None), |(name, value)| {
                        (name.trim(), Some(value.trim()))
                    });
                if ["no-store", "no-cache", "private"]
                    .iter()
                    .any(|blocked| name.eq_ignore_ascii_case(blocked))
                {
                    return None;
                }
                if name.eq_ignore_ascii_case("max-age") {
                    let value = value?;
                    let value = if value.starts_with('"') {
                        value.strip_prefix('"')?.strip_suffix('"')?
                    } else {
                        value
                    };
                    if max_age.is_some()
                        || value.is_empty()
                        || !value.bytes().all(|byte| byte.is_ascii_digit())
                    {
                        return None;
                    }
                    max_age = Some(value.parse::<u64>().ok()?);
                }
            }
        } else if name.eq_ignore_ascii_case("age") {
            if age.is_some() || value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            age = Some(value.parse::<u64>().ok()?);
        } else if name.eq_ignore_ascii_case("date") {
            dates += 1;
        } else if ["vary", "set-cookie", "pragma", "content-range"]
            .iter()
            .any(|blocked| name.eq_ignore_ascii_case(blocked))
        {
            return None;
        }
    }
    if dates != 1 {
        return None;
    }
    let current_age = now
        .duration_since(date)
        .unwrap_or_default()
        .max(Duration::from_secs(age.unwrap_or(0)).checked_add(delay)?);
    Duration::from_secs(max_age?.min(86400))
        .checked_sub(current_age)
        .filter(|age| !age.is_zero())
}

fn cache_expires(request: &AsyncRequest, started: Instant) -> Option<Instant> {
    let mut bytes = 0;
    unsafe {
        WinHttpQueryHeaders(
            request.handle(),
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            null(),
            null_mut(),
            &mut bytes,
            null_mut(),
        )
    };
    if bytes == 0 || bytes > 32768 || bytes % 2 != 0 {
        return None;
    }
    let mut headers = vec![0u16; bytes as usize / 2];
    check(unsafe {
        WinHttpQueryHeaders(
            request.handle(),
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            null(),
            headers.as_mut_ptr().cast(),
            &mut bytes,
            null_mut(),
        )
    })
    .ok()?;
    let headers = String::from_utf16(&headers[..bytes as usize / 2]).ok()?;
    let headers = headers.trim_end_matches('\0');
    let mut date = SYSTEMTIME::default();
    bytes = std::mem::size_of::<SYSTEMTIME>() as u32;
    check(unsafe {
        WinHttpQueryHeaders(
            request.handle(),
            WINHTTP_QUERY_DATE | WINHTTP_QUERY_FLAG_SYSTEMTIME,
            null(),
            std::ptr::addr_of_mut!(date).cast(),
            &mut bytes,
            null_mut(),
        )
    })
    .ok()?;
    let mut file_time = FILETIME::default();
    if unsafe { SystemTimeToFileTime(&date, &mut file_time) } == 0 {
        return None;
    }
    let ticks = ((file_time.dwHighDateTime as u64) << 32) | file_time.dwLowDateTime as u64;
    let date = UNIX_EPOCH.checked_add(Duration::from_nanos(
        ticks.checked_sub(116444736000000000)?.checked_mul(100)?,
    ))?;
    let received = Instant::now();
    received.checked_add(freshness(
        headers,
        date,
        SystemTime::now(),
        started.elapsed(),
    )?)
}

struct Handle(*mut c_void);

enum Completion {
    Sent,
    Headers,
    Read(u32),
    Error(u32),
}

struct CallbackContext {
    sender: SyncSender<Completion>,
    failed: AtomicBool,
    buffer: Mutex<Box<[u8; 8192]>>,
}

unsafe extern "system" fn callback(
    _: *mut c_void,
    context: usize,
    status: u32,
    information: *mut c_void,
    length: u32,
) {
    if context == 0 {
        return;
    }
    let pointer = context as *const CallbackContext;
    Arc::increment_strong_count(pointer);
    let shared = Arc::from_raw(pointer);
    if status == WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING {
        drop(Arc::from_raw(pointer));
        return;
    }
    let completion = match status {
        WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => Completion::Sent,
        WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => Completion::Headers,
        WINHTTP_CALLBACK_STATUS_READ_COMPLETE => Completion::Read(length),
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR
            if !information.is_null()
                && length as usize >= std::mem::size_of::<WINHTTP_ASYNC_RESULT>() =>
        {
            Completion::Error((*information.cast::<WINHTTP_ASYNC_RESULT>()).dwError)
        }
        _ => return,
    };
    if shared.sender.try_send(completion).is_err() {
        shared.failed.store(true, Ordering::Release);
    }
}

struct AsyncRequest {
    handle: Option<Handle>,
    context: Arc<CallbackContext>,
    receiver: Receiver<Completion>,
}

impl AsyncRequest {
    fn new(handle: Handle) -> Result<Self, ImageError> {
        let (sender, receiver) = sync_channel(8);
        let context = Arc::new(CallbackContext {
            sender,
            failed: AtomicBool::new(false),
            buffer: Mutex::new(Box::new([0; 8192])),
        });
        let previous = unsafe {
            WinHttpSetStatusCallback(
                handle.0,
                Some(callback),
                WINHTTP_CALLBACK_FLAG_SENDREQUEST_COMPLETE
                    | WINHTTP_CALLBACK_FLAG_HEADERS_AVAILABLE
                    | WINHTTP_CALLBACK_FLAG_READ_COMPLETE
                    | WINHTTP_CALLBACK_FLAG_REQUEST_ERROR
                    | WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING,
                0,
            )
        };
        if previous.is_some_and(|callback| callback as usize == usize::MAX) {
            return Err(ImageError::Http(unsafe { GetLastError() }));
        }
        let pointer = Arc::into_raw(context.clone()) as usize;
        let result = check(unsafe {
            WinHttpSetOption(
                handle.0,
                WINHTTP_OPTION_CONTEXT_VALUE,
                std::ptr::addr_of!(pointer).cast(),
                std::mem::size_of::<usize>() as u32,
            )
        });
        if let Err(error) = result {
            unsafe {
                drop(Arc::from_raw(pointer as *const CallbackContext));
            }
            return Err(error);
        }
        Ok(Self {
            handle: Some(handle),
            context,
            receiver,
        })
    }

    fn handle(&self) -> *mut c_void {
        self.handle.as_ref().unwrap().0
    }

    fn wait(&self, cancelled: &AtomicBool, deadline: Instant) -> Result<Completion, ImageError> {
        loop {
            checkpoint(cancelled, deadline)?;
            if self.context.failed.load(Ordering::Acquire) {
                return Err(ImageError::Worker);
            }
            match self.receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(Completion::Error(error)) => return Err(ImageError::Http(error)),
                Ok(completion) => return Ok(completion),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(ImageError::Worker),
            }
        }
    }
}

impl Drop for AsyncRequest {
    fn drop(&mut self) {
        self.handle.take();
    }
}

impl Handle {
    fn new(value: *mut c_void) -> Result<Self, ImageError> {
        if value.is_null() {
            Err(ImageError::Http(unsafe { GetLastError() }))
        } else {
            Ok(Self(value))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}

fn check(result: i32) -> Result<(), ImageError> {
    if result == 0 {
        Err(ImageError::Http(unsafe { GetLastError() }))
    } else {
        Ok(())
    }
}

fn checkpoint(cancelled: &AtomicBool, deadline: Instant) -> Result<(), ImageError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(ImageError::Http(ERROR_WINHTTP_TIMEOUT));
    }
    Ok(())
}

pub(super) fn fetch(url: &str, cancelled: &AtomicBool, limit: usize) -> Result<Loaded, ImageError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    checkpoint(cancelled, deadline)?;
    let url: Vec<_> = url.encode_utf16().chain(Some(0)).collect();
    let mut parts = URL_COMPONENTS {
        dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
        dwSchemeLength: u32::MAX,
        dwHostNameLength: u32::MAX,
        dwUserNameLength: u32::MAX,
        dwPasswordLength: u32::MAX,
        dwUrlPathLength: u32::MAX,
        dwExtraInfoLength: u32::MAX,
        ..Default::default()
    };
    check(unsafe { WinHttpCrackUrl(url.as_ptr(), (url.len() - 1) as u32, 0, &mut parts) })?;
    if parts.dwHostNameLength == 0
        || parts.dwUserNameLength != 0
        || parts.dwPasswordLength != 0
        || (parts.nScheme != WINHTTP_INTERNET_SCHEME_HTTP
            && parts.nScheme != WINHTTP_INTERNET_SCHEME_HTTPS)
    {
        return Err(ImageError::Source);
    }
    let mut host =
        unsafe { std::slice::from_raw_parts(parts.lpszHostName, parts.dwHostNameLength as usize) }
            .to_vec();
    host.push(0);
    let mut path = if parts.dwUrlPathLength == 0 {
        vec![b'/' as u16]
    } else {
        unsafe { std::slice::from_raw_parts(parts.lpszUrlPath, parts.dwUrlPathLength as usize) }
            .to_vec()
    };
    if parts.dwExtraInfoLength > 0 {
        let extra = unsafe {
            std::slice::from_raw_parts(parts.lpszExtraInfo, parts.dwExtraInfoLength as usize)
        };
        path.extend(
            extra
                .iter()
                .copied()
                .take_while(|character| *character != b'#' as u16),
        );
    }
    path.push(0);
    let agent: Vec<_> = "Jadix-Native-Image/1"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let session = Handle::new(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            WINHTTP_FLAG_ASYNC,
        )
    })?;
    check(unsafe { WinHttpSetTimeouts(session.0, 3000, 3000, 3000, 3000) })?;
    let connection =
        Handle::new(unsafe { WinHttpConnect(session.0, host.as_ptr(), parts.nPort, 0) })?;
    checkpoint(cancelled, deadline)?;
    let verb: Vec<_> = "GET".encode_utf16().chain(Some(0)).collect();
    let flags = if parts.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS {
        WINHTTP_FLAG_SECURE
    } else {
        0
    };
    let request = Handle::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            null(),
            null(),
            null(),
            flags,
        )
    })?;
    let disabled = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION;
    let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
    check(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            (&disabled as *const u32).cast(),
            4,
        )
    })?;
    check(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&redirect as *const u32).cast(),
            4,
        )
    })?;
    let request = AsyncRequest::new(request)?;
    let started = Instant::now();
    check(unsafe {
        WinHttpSendRequest(
            request.handle(),
            null(),
            0,
            null(),
            0,
            0,
            Arc::as_ptr(&request.context) as usize,
        )
    })?;
    if !matches!(request.wait(cancelled, deadline)?, Completion::Sent) {
        return Err(ImageError::Worker);
    }
    check(unsafe { WinHttpReceiveResponse(request.handle(), null_mut()) })?;
    if !matches!(request.wait(cancelled, deadline)?, Completion::Headers) {
        return Err(ImageError::Worker);
    }
    let mut status = 0u32;
    let mut bytes = 4u32;
    check(unsafe {
        WinHttpQueryHeaders(
            request.handle(),
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut bytes,
            null_mut(),
        )
    })?;
    if status != 200 {
        return Err(ImageError::Http(status));
    }
    let expires = cache_expires(&request, started);
    let mut output = Vec::new();
    loop {
        checkpoint(cancelled, deadline)?;
        let mut chunk = request
            .context
            .buffer
            .lock()
            .map_err(|_| ImageError::Worker)?;
        check(unsafe {
            WinHttpReadData(
                request.handle(),
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                null_mut(),
            )
        })?;
        let Completion::Read(count) = request.wait(cancelled, deadline)? else {
            return Err(ImageError::Worker);
        };
        if count as usize > chunk.len() {
            return Err(ImageError::Worker);
        }
        if count == 0 {
            return Ok(Loaded {
                bytes: output,
                expires,
            });
        }
        if output.len().saturating_add(count as usize) > limit {
            return Err(ImageError::Limits);
        }
        output.extend_from_slice(&chunk[..count as usize]);
    }
}
