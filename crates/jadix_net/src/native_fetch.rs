//! Bounded, application-driven bridge from FetchService to the system HTTPS transport.
//! UI polling never waits for the network. Shutdown cancels and reaps owned workers.
use crate::{native_https::HttpsClient, service::*, HttpError, HttpMethod, HttpRequest};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

struct Job {
    cancel: Arc<AtomicBool>,
    worker: thread::JoinHandle<()>,
}
enum Reply {
    Event(u32, FetchEvent),
    Done(u32),
}
pub struct NativeFetch {
    service: FetchService,
    executable: PathBuf,
    pending: VecDeque<(u32, FetchRequest)>,
    jobs: BTreeMap<u32, Job>,
    sender: mpsc::SyncSender<Reply>,
    receiver: mpsc::Receiver<Reply>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl NativeFetch {
    pub fn new(executable: impl Into<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = mpsc::sync_channel(8);
        Self {
            service: FetchService::default(),
            executable: executable.into(),
            pending: VecDeque::new(),
            jobs: BTreeMap::new(),
            sender,
            receiver,
            wake: Arc::new(wake),
        }
    }
    pub fn service(&self) -> FetchService {
        self.service.clone()
    }
    pub fn pump(&mut self) {
        for command in self.service.take_commands() {
            match command {
                FetchCommand::Send { id, request } => self.pending.push_back((id, request)),
                FetchCommand::Cancel { id } => {
                    self.pending.retain(|(pending, _)| *pending != id);
                    if let Some(job) = self.jobs.get(&id) {
                        job.cancel.store(true, Ordering::Release);
                    }
                }
            }
        }
        // Bound main-thread work, leaving consumers a turn to drain their queues.
        for _ in 0..8 {
            let Ok(reply) = self.receiver.try_recv() else {
                break;
            };
            match reply {
                Reply::Event(id, event) => {
                    if !self.service.event(id, event) {
                        if let Some(job) = self.jobs.get(&id) {
                            job.cancel.store(true, Ordering::Release);
                        }
                    }
                }
                Reply::Done(id) => {
                    if let Some(job) = self.jobs.remove(&id) {
                        let _ = job.worker.join();
                    }
                }
            }
        }
        // A cancelled worker may exit while the bounded reply queue is full.
        let finished: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.worker.is_finished())
            .map(|(id, _)| *id)
            .collect();
        for id in finished {
            if let Some(job) = self.jobs.remove(&id) {
                let _ = job.worker.join();
            }
        }
        while self.jobs.len() < 4 {
            let Some((id, request)) = self.pending.pop_front() else {
                break;
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let (flag, sender, executable, wake) = (
                cancel.clone(),
                self.sender.clone(),
                self.executable.clone(),
                self.wake.clone(),
            );
            let worker = thread::Builder::new()
                .name("jadix-fetch".into())
                .spawn(move || {
                    let emit = |reply| send(&sender, &flag, &wake, reply);
                    let result = convert(request).and_then(|request| {
                        let cancelled = flag.clone();
                        HttpsClient::new(executable)
                            .send_cancellable(request, move || cancelled.load(Ordering::Acquire))
                    });
                    match result {
                        Ok(response) => {
                            if emit(Reply::Event(
                                id,
                                FetchEvent::Headers {
                                    status: response.status,
                                    url: response.url,
                                    headers: response
                                        .headers
                                        .into_iter()
                                        .map(|h| {
                                            (h.name, String::from_utf8_lossy(&h.value).into_owned())
                                        })
                                        .collect(),
                                },
                            )) {
                                let mut delivered = true;
                                for bytes in response.body.chunks(64 * 1024) {
                                    if !emit(Reply::Event(
                                        id,
                                        FetchEvent::Chunk {
                                            bytes: bytes.to_vec(),
                                        },
                                    )) {
                                        delivered = false;
                                        break;
                                    }
                                }
                                if delivered {
                                    emit(Reply::Event(id, FetchEvent::Complete));
                                }
                            }
                        }
                        Err(error) => {
                            let mut message = error.to_string();
                            let mut end = message.len().min(4096);
                            while !message.is_char_boundary(end) {
                                end -= 1;
                            }
                            message.truncate(end);
                            emit(Reply::Event(id, FetchEvent::Failed { message }));
                        }
                    }
                    emit(Reply::Done(id));
                });
            match worker {
                Ok(worker) => {
                    self.jobs.insert(id, Job { cancel, worker });
                }
                Err(_) => {
                    self.service.event(
                        id,
                        FetchEvent::Failed {
                            message: "could not start HTTPS worker".into(),
                        },
                    );
                }
            }
        }
    }
    pub fn dispose(&mut self) {
        self.service.dispose();
        self.pending.clear();
        for job in self.jobs.values() {
            job.cancel.store(true, Ordering::Release);
        }
        for (_, job) in std::mem::take(&mut self.jobs) {
            let _ = job.worker.join();
        }
        while self.receiver.try_recv().is_ok() {}
    }
}
impl Drop for NativeFetch {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn send(
    sender: &mpsc::SyncSender<Reply>,
    cancel: &AtomicBool,
    wake: &Arc<dyn Fn() + Send + Sync>,
    mut reply: Reply,
) -> bool {
    loop {
        if cancel.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(reply) {
            Ok(()) => {
                wake();
                return true;
            }
            Err(mpsc::TrySendError::Full(value)) => {
                reply = value;
                thread::park_timeout(Duration::from_millis(10));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
        }
    }
}
fn convert(request: FetchRequest) -> Result<HttpRequest, HttpError> {
    request.validate()?;
    let method = match request.method.as_str() {
        "GET" => HttpMethod::Get,
        "HEAD" => HttpMethod::Head,
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        "PATCH" => HttpMethod::Patch,
        "DELETE" => HttpMethod::Delete,
        "OPTIONS" => HttpMethod::Options,
        _ => return Err(HttpError::invalid("unsupported HTTP method")),
    };
    let mut http =
        HttpRequest::new(method, request.url).timeout(Duration::from_millis(request.timeout_ms));
    http.max_response_bytes = request.max_response_bytes;
    http.body = request.body.into();
    for (name, value) in request.headers {
        http = http.header(name, value.as_bytes());
    }
    Ok(http)
}
