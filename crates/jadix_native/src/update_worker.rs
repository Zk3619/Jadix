//! Shared bounded updater I/O. UI adapters only consume replies and install.
use jadix_core::updater::{self as shared, installation::{Installation, Staged}, Event};
use std::{path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc}, thread, time::Duration};
pub(crate) struct Archive(PathBuf);
impl Archive {
    fn new() -> Result<Self, String> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "jadix-update-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            #[allow(unused_mut)]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("cannot reserve update download directory".into())
    }
    pub(crate) fn file(&self) -> PathBuf {
        self.0.join("archive")
    }
}
impl Drop for Archive {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) enum Work {
    Manifest {
        url: String,
        signature_url: Option<String>,
    },
    Notes(shared::NotesFile),
    Download {
        url: String,
        max_bytes: u64,
    },
    Stage {
        archive: Archive,
        version: String,
    },
}
struct Job {
    id: u32,
    work: Work,
    cancel: Arc<AtomicBool>,
}
pub(crate) enum Reply {
    Event(u32, Event),
    Chunk(u32, Vec<u8>),
    Archive(u32, Archive),
    Staged(u32, Staged),
}
fn bytes(
    client: &jadix_net::native_https::HttpsClient,
    url: &str,
    max: usize,
    cancel: &Arc<AtomicBool>,
    stopping: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let archive = Archive::new()?;
    let mut bytes = Vec::new();
    client.download_to(
        jadix_net::HttpRequest::get(url)
            .max_response_bytes(max)
            .timeout(Duration::from_secs(30)),
        &archive.file(),
        || cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire),
        |part| {
            bytes.extend_from_slice(part);
            Ok(())
        },
    )?;
    Ok(bytes)
}

pub(crate) struct Worker {
    jobs: Option<mpsc::SyncSender<Job>>,
    replies: Option<mpsc::Receiver<Reply>>,
    thread: Option<thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}
impl Worker {
    pub fn new(installation: Installation, curl: impl Into<String>, wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let curl = curl.into();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (jobs, receive) = mpsc::sync_channel::<Job>(8);
        let (send, replies) = mpsc::sync_channel::<Reply>(8);
        let worker = thread::Builder::new()
            .name("jadix-updater".into())
            .spawn(move || {
                let client = jadix_net::native_https::HttpsClient::new(curl);
                while let Ok(job) = receive.recv() {
                    let Job { id, work, cancel } = job;
                    if cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire) {
                        continue;
                    }
                    let report = |reply| -> Result<(), String> {
                        send.send(reply)
                            .map_err(|_| "update host disposed".to_string())?;
                        wake();
                        Ok(())
                    };
                    let result = (|| match work {
                        Work::Manifest { url, signature_url } => {
                            let data = bytes(&client, &url, shared::MANIFEST_LIMIT, &cancel, &stopping)?;
                            let signature = signature_url
                                .map(|url| {
                                    bytes(&client, &url, 4096, &cancel, &stopping).and_then(|b| {
                                        String::from_utf8(b)
                                            .map_err(|_| "update signature is not UTF-8".into())
                                    })
                                })
                                .transpose()?;
                            report(Reply::Event(
                                id,
                                Event::Manifest {
                                    bytes: data,
                                    signature,
                                },
                            ))
                        }
                        Work::Notes(file) => report(Reply::Event(
                            id,
                            Event::Notes {
                                bytes: bytes(&client, &file.url, shared::NOTES_LIMIT, &cancel, &stopping)?,
                            },
                        )),
                        Work::Download { url, max_bytes } => {
                            let archive = Archive::new()?;
                            client.download_to(
                                jadix_net::HttpRequest::get(url)
                                    .max_response_bytes(max_bytes as usize)
                                    .timeout(Duration::from_secs(600)),
                                &archive.file(),
                                || cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire),
                                |part| report(Reply::Chunk(id, part.into())),
                            )?;
                            report(Reply::Archive(id, archive))
                        }
                        Work::Stage { archive, version } => {
                            let staged = installation.stage_with_cancel(&archive.file(), &version,
                                || cancel.load(Ordering::Acquire) || stopping.load(Ordering::Acquire))?;
                            report(Reply::Staged(id, staged))
                        }
                    })();
                    if let Err(message) = result {
                        if !cancel.load(Ordering::Acquire) {
                            if report(Reply::Event(id, Event::Failed { message })).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self { jobs: Some(jobs), replies: Some(replies), thread: Some(worker), stop })
    }
    pub fn enqueue(&self, id: u32, work: Work, cancel: Arc<AtomicBool>) -> Result<(), String> {
        self.jobs.as_ref().ok_or("update worker disposed")?.try_send(Job { id, work, cancel }).map_err(|_| "update worker queue is full or closed".into())
    }
    pub fn replies(&self) -> Vec<Reply> {
        self.replies.as_ref().map(|replies| replies.try_iter().collect()).unwrap_or_default()
    }
    pub fn dispose(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.jobs.take();
        self.replies.take();
        if let Some(worker) = self.thread.take() { let _ = worker.join(); }
    }
}
impl Drop for Worker { fn drop(&mut self) { self.dispose(); } }
