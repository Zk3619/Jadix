//! Explicit blocking HTTPS adapter using an application-selected system curl.
//! TLS stays in the installed transport; no executor or TLS stack is added.
use crate::{
    CancellationToken, HttpError, HttpErrorKind as Kind, HttpHeader, HttpMethod, HttpRequest,
    HttpResponse,
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub struct HttpsClient {
    executable: PathBuf,
    ca_bundle: Option<PathBuf>,
}
impl HttpsClient {
    /// Stream an archive into a newly reserved file, reporting ordered 64 KiB
    /// chunks without retaining the response in memory. The caller owns and
    /// protects the destination parent. A failed transfer removes only this file.
    pub fn download_to(
        &self, request: HttpRequest, destination: &Path,
        is_cancelled: impl Fn() -> bool,
        mut chunk: impl FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        request.validate().map_err(|e|e.to_string())?;
        if request.method!=HttpMethod::Get || !request.body.is_empty() || !request.headers.is_empty()
            || !request.url.starts_with("https://") || request.url.contains('#')
            || request.max_response_bytes as u64>2*1024*1024*1024 {
            return Err("archive transfer requires bounded HTTPS GET without credentials, fragment or headers".into());
        }
        let file=std::fs::OpenOptions::new().create_new(true).write(true).open(destination).map_err(|e|e.to_string())?;
        let result=(|| {
            let mut reader=std::fs::File::open(destination).map_err(|e|e.to_string())?;
            let mut command=Command::new(&self.executable);
            command.args(["-q","--silent","--fail","--location","--max-redirs","5","--proto","=https","--proto-redir","=https","--no-insecure","--tlsv1.2","--no-netrc"])
                .arg("--max-time").arg(format!("{:.3}",request.timeout.as_secs_f64().max(0.001)))
                .arg("--max-filesize").arg(request.max_response_bytes.to_string());
            if let Some(ca)=&self.ca_bundle {command.arg("--cacert").arg(ca);}
            let child=command.arg("--url").arg(&request.url).stdin(Stdio::null()).stdout(Stdio::from(file.try_clone().map_err(|e|e.to_string())?)).stderr(Stdio::null()).spawn().map_err(|e|e.to_string())?;
            struct Owned(std::process::Child);
            impl Drop for Owned {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
            let mut child=Owned(child);
            let deadline=Instant::now().checked_add(request.timeout).ok_or("archive deadline overflow")?;
            let mut count=0usize;let mut buffer=[0u8;64*1024];let mut status=None;
            loop {
                if is_cancelled(){return Err("archive transfer cancelled".into());}
                if Instant::now()>=deadline{return Err("archive transfer timed out".into());}
                if status.is_none(){status=child.0.try_wait().map_err(|e|e.to_string())?;}
                let n=reader.read(&mut buffer).map_err(|e|e.to_string())?;
                if n>0 {
                    count=count.checked_add(n).filter(|n|*n<=request.max_response_bytes).ok_or("archive response exceeds budget")?;
                    chunk(&buffer[..n])?;
                } else if let Some(status)=status {
                    if !status.success(){return Err(format!("archive HTTPS transfer failed: {status}"));}break;
                } else {thread::sleep(Duration::from_millis(10));}
            }
            file.sync_all().map_err(|e|e.to_string())
        })();
        drop(file);
        if result.is_err(){let _=std::fs::remove_file(destination);}
        result
    }
    /// Select an installed curl executable. Call on an application-owned worker.
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            ca_bundle: None,
        }
    }
    /// An explicit private/enterprise trust bundle, never installed globally.
    /// The bundle is a trust root: callers must protect it like signing keys.
    pub fn with_ca_bundle(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_bundle = Some(path.into());
        self
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn get(
        &self,
        request: HttpRequest,
        cancellation: &CancellationToken,
    ) -> Result<HttpResponse, HttpError> {
        let cancellation = cancellation.clone();
        self.get_cancellable(request, move || cancellation.is_cancelled())
    }

    /// Bridge existing native cancellation flags without another polling thread.
    pub fn get_cancellable(
        &self,
        request: HttpRequest,
        is_cancelled: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<HttpResponse, HttpError> {
        if request.method != HttpMethod::Get
            || !request.body.is_empty()
        {
            return Err(HttpError::new(Kind::Unsupported, "HTTPS GET without a request body required"));
        }
        self.send_cancellable(request, is_cancelled)
    }

    /// Execute a bounded HTTPS request on an application-owned worker. Bodies
    /// are piped directly to curl, never placed in argv or temporary files.
    pub fn send_cancellable(
        &self,
        request: HttpRequest,
        is_cancelled: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<HttpResponse, HttpError> {
        let is_cancelled = Arc::new(is_cancelled);
        request.validate()?;
        if !request.url.starts_with("https://")
            || request.url.contains('#')
            || request.max_response_bytes > 32 * 1024 * 1024
            || request.body.len() > 32 * 1024 * 1024
        {
            return Err(HttpError::new(
                Kind::Unsupported,
                "HTTPS with <=32 MiB request and response budgets required",
            ));
        }
        if is_cancelled() {
            return Err(error(Kind::Cancelled));
        }
        let deadline = Instant::now()
            .checked_add(request.timeout)
            .ok_or_else(|| HttpError::invalid("deadline overflow"))?;
        let mut command = Command::new(&self.executable);
        // -q must be first: user curlrc cannot disable TLS or redirect policy.
        command
            .args([
                "-q",
                "--silent",
                "--show-error",
                "--include",
                "--suppress-connect-headers",
                "--no-location",
                "--max-redirs",
                "0",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--no-insecure",
                "--tlsv1.2",
                "--no-netrc",
                "--compressed",
            ])
            .arg("--max-time")
            .arg(format!("{:.3}", request.timeout.as_secs_f64().max(0.001)))
            .arg("--max-filesize")
            .arg(request.max_response_bytes.to_string())
            .env_remove("CURL_CA_BUNDLE")
            .env_remove("SSL_CERT_FILE")
            .env_remove("SSL_CERT_DIR")
            .stdin(if request.body.is_empty() { Stdio::null() } else { Stdio::piped() })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = &self.ca_bundle {
            command.arg("--cacert").arg(path);
        }
        if request.method == HttpMethod::Head { command.arg("--head"); }
        command.arg("--request").arg(request.method.as_str());
        if !request.body.is_empty() { command.args(["--data-binary", "@-"]); }
        for header in &request.headers {
            let value = std::str::from_utf8(&header.value)
                .map_err(|_| HttpError::invalid("curl request header must be UTF-8"))?;
            command
                .arg("--header")
                .arg(format!("{}: {value}", header.name));
        }
        command.arg("--url").arg(&request.url);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| error(Kind::Backend))?;
        let mut stdout = child.stdout.take().ok_or_else(|| error(Kind::Backend))?;
        let child = Arc::new(Mutex::new(child));
        let stop = Arc::new(AtomicBool::new(false));
        let watched = child.clone();
        let completed = stop.clone();
        let cancelled = is_cancelled.clone();
        let watcher = thread::Builder::new()
            .name("jadix-https-deadline".into())
            .spawn(move || {
                while !completed.load(Ordering::Acquire) {
                    if cancelled() || Instant::now() >= deadline {
                        if let Ok(mut process) = watched.lock() {
                            let _ = process.kill();
                        }
                        break;
                    }
                    thread::park_timeout(Duration::from_millis(10));
                }
            });
        let watcher = match watcher {
            Ok(w) => w,
            Err(_) => {
                if let Ok(mut c) = child.lock() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                return Err(error(Kind::Backend));
            }
        };
        let body_writer = {
            let input = child.lock().ok().and_then(|mut c| c.stdin.take());
            let body = request.body.clone();
            match input.map(|mut input| thread::Builder::new().name("jadix-https-upload".into())
                .spawn(move || input.write_all(&body))).transpose() {
                Ok(writer) => writer,
                Err(_) => {
                    if let Ok(mut process) = child.lock() { let _ = process.kill(); let _ = process.wait(); }
                    stop.store(true, Ordering::Release);
                    watcher.thread().unpark();
                    let _ = watcher.join();
                    return Err(error(Kind::Backend));
                }
            }
        };
        let transfer = (|| {
            // Fixed chunks and one sentinel byte enforce decoded-body bounds even
            // without Content-Length; never trust curl's header size estimate.
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 8192];
            let budget = request.max_response_bytes.saturating_add(64 * 1024);
            loop {
                if is_cancelled() {
                    return Err(error(Kind::Cancelled));
                }
                if Instant::now() >= deadline {
                    return Err(error(Kind::Timeout));
                }
                let count = stdout
                    .read(&mut chunk)
                    .map_err(|_| error(Kind::Disconnected))?;
                if count == 0 {
                    break;
                }
                if bytes.len().saturating_add(count) > budget {
                    return Err(error(Kind::BodyTooLarge));
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some((_, offset, _)) = headers(&bytes)? {
                    if bytes.len() - offset > request.max_response_bytes {
                        return Err(error(Kind::BodyTooLarge));
                    }
                }
            }
            let (status, offset, headers) = headers(&bytes)?.ok_or_else(|| error(Kind::Decode))?;
            if (300..400).contains(&status) {
                return Err(HttpError::new(
                    Kind::HttpStatus,
                    "HTTPS redirects are forbidden",
                ));
            }
            if bytes.len() - offset > request.max_response_bytes {
                return Err(error(Kind::BodyTooLarge));
            }
            Ok(HttpResponse {
                status,
                url: request.url,
                headers,
                body: bytes[offset..].into(),
            })
        })();
        if transfer.is_err() {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
            }
        }
        // Never hold the child lock while waiting for exit: the deadline worker
        // must still be able to kill a transport that closed stdout then stalled.
        let status = loop {
            let status = child
                .lock()
                .map_err(|_| error(Kind::Backend))
                .and_then(|mut child| child.try_wait().map_err(|_| error(Kind::Backend)));
            match status {
                Ok(Some(status)) => break Ok(status),
                Err(error) => break Err(error),
                Ok(None) => thread::park_timeout(Duration::from_millis(5)),
            }
        };
        stop.store(true, Ordering::Release);
        watcher.thread().unpark();
        let _ = watcher.join();
        if let Some(writer) = body_writer { let _ = writer.join(); }
        if is_cancelled() {
            return Err(error(Kind::Cancelled));
        }
        if Instant::now() >= deadline {
            return Err(error(Kind::Timeout));
        }
        match status?.code() {
            Some(0) => transfer,
            Some(28) => Err(error(Kind::Timeout)),
            Some(63) => Err(error(Kind::BodyTooLarge)),
            _ => transfer.and(Err(error(Kind::Backend))),
        }
    }
}
fn error(kind: Kind) -> HttpError {
    HttpError::new(
        kind,
        "HTTPS transfer failed under the configured TLS/deadline/body policy",
    )
}
type Head = (u16, usize, Vec<HttpHeader>);
fn headers(bytes: &[u8]) -> Result<Option<Head>, HttpError> {
    let mut at = 0;
    loop {
        let Some(end) = bytes[at..]
            .windows(4)
            .position(|s| s == b"\r\n\r\n")
            .map(|n| n + at)
        else {
            if bytes.len() > 64 * 1024 {
                return Err(error(Kind::BodyTooLarge));
            }
            return Ok(None);
        };
        if end > 64 * 1024 {
            return Err(error(Kind::BodyTooLarge));
        }
        let text = std::str::from_utf8(&bytes[at..end]).map_err(|_| error(Kind::Decode))?;
        let mut lines = text.split("\r\n");
        let status_line = lines.next().ok_or_else(|| error(Kind::Decode))?;
        if !status_line.starts_with("HTTP/") {
            return Err(error(Kind::Decode));
        }
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|s| (100..600).contains(s))
            .ok_or_else(|| error(Kind::Decode))?;
        at = end + 4;
        if status < 200 {
            continue;
        }
        let headers = lines
            .map(|line| {
                let (name, value) = line.split_once(':').ok_or_else(|| error(Kind::Decode))?;
                let header = HttpHeader::new(name, value.trim().as_bytes().to_vec());
                header.validate()?;
                Ok(header)
            })
            .collect::<Result<_, HttpError>>()?;
        return Ok(Some((status, at, headers)));
    }
}
