//! Explicit blocking HTTPS adapter using an application-selected system curl.
//! TLS stays in the installed transport; no executor or TLS stack is added.
use crate::{
    CancellationToken, HttpError, HttpErrorKind as Kind, HttpHeader, HttpMethod, HttpRequest,
    HttpResponse,
};
use std::{
    io::Read,
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
        request.validate()?;
        if request.method != HttpMethod::Get
            || !request.body.is_empty()
            || !request.url.starts_with("https://")
            || request.url.contains('#')
            || request.max_response_bytes > 32 * 1024 * 1024
        {
            return Err(HttpError::new(
                Kind::Unsupported,
                "HTTPS GET with a <=32 MiB body budget required",
            ));
        }
        if cancellation.is_cancelled() {
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = &self.ca_bundle {
            command.arg("--cacert").arg(path);
        }
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
        let cancelled = cancellation.clone();
        let watcher = thread::Builder::new()
            .name("jadix-https-deadline".into())
            .spawn(move || {
                while !completed.load(Ordering::Acquire) {
                    if cancelled.is_cancelled() || Instant::now() >= deadline {
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
        let transfer = (|| {
            // Fixed chunks and one sentinel byte enforce decoded-body bounds even
            // without Content-Length; never trust curl's header size estimate.
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 8192];
            let budget = request.max_response_bytes.saturating_add(64 * 1024);
            loop {
                if cancellation.is_cancelled() {
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
        if cancellation.is_cancelled() {
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
