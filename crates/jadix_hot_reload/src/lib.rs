#[cfg(feature = "syntax")]
pub mod syntax;
mod template;
mod transaction;
pub use template::TemplateNode;
pub use transaction::*;

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone)]
pub enum HotReloadCommand {
    Reload,
    Transaction(Transaction, Option<mpsc::Sender<CommitAck>>),
    TemplatePatch(TemplatePatchMsg),
    AssetReload(Vec<String>),
    StylesheetReload(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IncomingMessage {
    Hello,
    Transaction {
        transaction: Transaction,
    },
    Reload,
    TemplatePatch {
        file: String,
        #[serde(default)]
        templates: Vec<TemplateFragment>,
    },
    AssetReload {
        #[serde(default)]
        paths: Vec<String>,
    },
    StylesheetReload {
        #[serde(default)]
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TemplatePatchMsg {
    pub file: String,
    #[serde(default)]
    pub templates: Vec<TemplateFragment>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct TemplateFragment {
    pub id: String,
    pub hash: String,
    pub len: usize,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub declarations_signature: String,
    #[serde(default)]
    pub tree: Option<TemplateNode>,
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub text_literals: Vec<String>,
    #[serde(default)]
    pub text_replacements: Vec<TextReplacement>,
    #[serde(default)]
    pub style_attrs: Vec<StyleAttr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StyleAttr {
    #[serde(default)]
    pub element_index: usize,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TextReplacement {
    #[serde(default)]
    pub index: usize,
    pub from: String,
    pub to: String,
}

pub struct HotReloadReceiver {
    receiver: mpsc::Receiver<HotReloadCommand>,
    pending: Arc<AtomicUsize>,
    session: Arc<Mutex<SessionInfo>>,
    stopping: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    bound: Arc<Mutex<Option<std::net::SocketAddr>>>,
}

impl HotReloadReceiver {
    pub fn spawn(
        addr: String,
        wake: impl Fn() + Send + Sync + 'static,
        ready: impl Fn(&str) + Send + Sync + 'static,
        error: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(32);
        let pending = Arc::new(AtomicUsize::new(0));
        let thread_pending = Arc::clone(&pending);
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = Arc::clone(&stopping);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session = Arc::new(Mutex::new(SessionInfo::new(format!(
            "{}-{nonce}",
            std::process::id()
        ))));
        let thread_session = Arc::clone(&session);
        let bound = Arc::new(Mutex::new(None));
        let thread_bound = bound.clone();
        let worker = std::thread::spawn(move || {
            let listener = match TcpListener::bind(&addr) {
                Ok(listener) => listener,
                Err(reason) => {
                    error(&format!("bind failed on {addr}: {reason}"));
                    return;
                }
            };
            match listener.local_addr() {
                Ok(bound) => {
                    *thread_bound.lock().expect("listener address") = Some(bound);
                    if thread_stopping.load(Ordering::Acquire) {
                        return;
                    }
                    ready(&bound.to_string());
                }
                Err(reason) => {
                    error(&format!("listener address failed: {reason}"));
                    return;
                }
            }
            for connection in listener.incoming() {
                if thread_stopping.load(Ordering::Acquire) {
                    break;
                }
                let stream = match connection {
                    Ok(stream) => stream,
                    Err(reason) => {
                        error(&format!("accept failed: {reason}"));
                        continue;
                    }
                };
                if let Err(reason) = stream.set_nonblocking(false) {
                    error(&format!("blocking connection failed: {reason}"));
                    continue;
                }
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    let length = match reader
                        .by_ref()
                        .take(4 * 1024 * 1024 + 1)
                        .read_line(&mut line)
                    {
                        Ok(length) => length,
                        Err(reason) => {
                            error(&format!("request read failed: {reason}"));
                            break;
                        }
                    };
                    if length == 0
                        || length > 4 * 1024 * 1024
                        || thread_stopping.load(Ordering::Acquire)
                    {
                        break;
                    }
                    if matches!(
                        serde_json::from_str::<IncomingMessage>(line.trim()),
                        Ok(IncomingMessage::Hello)
                    ) {
                        if let Ok(info) = thread_session.lock() {
                            if let Ok(mut payload) = serde_json::to_vec(&*info) {
                                payload.push(b'\n');
                                let _ = reader.get_mut().write_all(&payload);
                            }
                        }
                        continue;
                    }
                    if !matches!(
                        serde_json::from_str::<IncomingMessage>(line.trim()),
                        Ok(IncomingMessage::Transaction { .. })
                    ) {
                        let current = thread_session.lock().expect("session lock").clone();
                        let acknowledgement = CommitAck {
                            build: current.build,
                            revision: current.revision,
                            status: CommitStatus::Rejected,
                            reason: "a versioned transaction is required".into(),
                            changes: Vec::new(),
                        };
                        if let Ok(mut payload) = serde_json::to_vec(&acknowledgement) {
                            payload.push(b'\n');
                            let _ = reader.get_mut().write_all(&payload);
                        }
                        continue;
                    }
                    if let Some(mut command) = parse_command(line.trim()) {
                        let reply =
                            if let HotReloadCommand::Transaction(_, completion) = &mut command {
                                let (completion_sender, completion_receiver) = mpsc::channel();
                                *completion = Some(completion_sender);
                                Some(completion_receiver)
                            } else {
                                None
                            };
                        thread_pending.fetch_add(1, Ordering::Relaxed);
                        match sender.try_send(command) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(_)) => {
                                thread_pending.fetch_sub(1, Ordering::Relaxed);
                                error("hot-reload queue is full");
                                break;
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => {
                                thread_pending.fetch_sub(1, Ordering::Relaxed);
                                return;
                            }
                        }
                        wake();
                        if let Some(reply) = reply {
                            if let Ok(acknowledgement) =
                                reply.recv_timeout(std::time::Duration::from_secs(2))
                            {
                                if let Ok(mut payload) = serde_json::to_vec(&acknowledgement) {
                                    payload.push(b'\n');
                                    let _ = reader.get_mut().write_all(&payload);
                                }
                            }
                        }
                    } else {
                        error("invalid hot-reload command");
                    }
                }
            }
        });
        Self {
            receiver,
            pending,
            session,
            stopping,
            worker: Some(worker),
            bound,
        }
    }

    pub fn try_recv(&self) -> Result<HotReloadCommand, mpsc::TryRecvError> {
        let command = self.receiver.try_recv()?;
        self.pending.fetch_sub(1, Ordering::Relaxed);
        Ok(command)
    }

    pub fn session(&self) -> SessionInfo {
        self.session.lock().expect("session lock").clone()
    }

    pub fn set_revision(&self, revision: u64) {
        self.session.lock().expect("session lock").revision = revision;
    }

    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }
}

impl Drop for HotReloadReceiver {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(mut bound) = *self.bound.lock().expect("listener address") {
            if bound.ip().is_unspecified() {
                bound.set_ip(if bound.is_ipv4() {
                    std::net::Ipv4Addr::LOCALHOST.into()
                } else {
                    std::net::Ipv6Addr::LOCALHOST.into()
                });
            }
            let _ =
                std::net::TcpStream::connect_timeout(&bound, std::time::Duration::from_millis(250));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn parse_command(raw: &str) -> Option<HotReloadCommand> {
    if raw.eq_ignore_ascii_case("reload") {
        return Some(HotReloadCommand::Reload);
    }
    match serde_json::from_str(raw).ok()? {
        IncomingMessage::Hello => None,
        IncomingMessage::Transaction { transaction } => {
            Some(HotReloadCommand::Transaction(transaction, None))
        }
        IncomingMessage::Reload => Some(HotReloadCommand::Reload),
        IncomingMessage::TemplatePatch { file, templates } => {
            Some(HotReloadCommand::TemplatePatch(TemplatePatchMsg {
                file,
                templates,
            }))
        }
        IncomingMessage::AssetReload { paths } => Some(HotReloadCommand::AssetReload(paths)),
        IncomingMessage::StylesheetReload { paths } => {
            Some(HotReloadCommand::StylesheetReload(paths))
        }
    }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
