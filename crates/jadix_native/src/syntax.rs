use crate::{
    presentation::{NodeContent, NodeId, RootLifetime},
    HeadlessRoot,
};
pub use jadix_document::{syntax::HighlightSpan, TextDocument};
use std::sync::{Arc, Condvar, Mutex};

const MAX_SOURCE: usize = 128 * 1024;
const MAX_LINE: usize = 8192;
const MAX_SPANS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxError {
    Disposed,
    Node,
    Limits,
    Worker,
    Exhausted,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for SyntaxError {}

struct Job {
    request: u64,
    document: TextDocument,
    language: String,
    dark: bool,
}

#[derive(Debug)]
pub struct SyntaxResult {
    pub node: NodeId,
    pub request: u64,
    pub document_revision: u64,
    pub spans: Vec<HighlightSpan>,
    pub plain_fallback: bool,
}

#[derive(Default)]
struct State {
    pending: Option<Job>,
    ready: Option<SyntaxResult>,
    latest: u64,
    stopped: bool,
    failed: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

pub struct SyntaxWorker {
    lifetime: RootLifetime,
    node: NodeId,
    shared: Arc<Shared>,
    document: Option<TextDocument>,
    style: Option<(String, bool)>,
    request: u64,
    disposed: bool,
}

impl SyntaxWorker {
    pub fn new(
        model: &HeadlessRoot,
        node: NodeId,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, SyntaxError> {
        if model.lifetime().is_disposed() {
            return Err(SyntaxError::Disposed);
        }
        if !editor(model, node) {
            return Err(SyntaxError::Node);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("jadix-native-syntax".into())
            .spawn(move || loop {
                let job = {
                    let Ok(mut state) = worker.state.lock() else {
                        return;
                    };
                    while state.pending.is_none() && !state.stopped {
                        let Ok(next) = worker.changed.wait(state) else {
                            return;
                        };
                        state = next;
                    }
                    if state.stopped {
                        return;
                    }
                    state.pending.take().unwrap()
                };
                let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    jadix_document::syntax::highlight_code(
                        job.document.text(),
                        &job.language,
                        job.dark,
                    )
                }));
                let Ok(mut state) = worker.state.lock() else {
                    return;
                };
                if state.stopped {
                    return;
                }
                if state.latest != job.request {
                    continue;
                }
                let Ok(mut spans) = parsed else {
                    state.failed = true;
                    drop(state);
                    wake();
                    continue;
                };
                let plain_fallback = spans.len() > MAX_SPANS;
                if plain_fallback {
                    spans = vec![HighlightSpan {
                        text: job.document.text().into(),
                        color: None,
                    }];
                }
                state.ready = Some(SyntaxResult {
                    node,
                    request: job.request,
                    document_revision: job.document.revision(),
                    spans,
                    plain_fallback,
                });
                drop(state);
                wake();
            })
            .map_err(|_| SyntaxError::Worker)?;
        Ok(Self {
            lifetime: model.lifetime(),
            node,
            shared,
            document: None,
            style: None,
            request: 0,
            disposed: false,
        })
    }

    pub fn request(
        &mut self,
        model: &HeadlessRoot,
        document: TextDocument,
        language: &str,
        dark: bool,
    ) -> Result<u64, SyntaxError> {
        self.check(model)?;
        if document.len() > MAX_SOURCE
            || document.line_count() > 4096
            || document.text().lines().any(|line| line.len() > MAX_LINE)
            || language.len() > 64
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        {
            return Err(SyntaxError::Limits);
        }
        if self.document.as_ref() == Some(&document)
            && self
                .style
                .as_ref()
                .is_some_and(|(current, theme)| current == language && *theme == dark)
        {
            return Ok(self.request);
        }
        let request = self.request.checked_add(1).ok_or(SyntaxError::Exhausted)?;
        let mut state = self.shared.state.lock().map_err(|_| SyntaxError::Worker)?;
        if state.stopped {
            return Err(SyntaxError::Disposed);
        }
        state.latest = request;
        state.ready = None;
        state.failed = false;
        state.pending = Some(Job {
            request,
            document: document.clone(),
            language: language.into(),
            dark,
        });
        self.document = Some(document);
        self.style = Some((language.into(), dark));
        self.request = request;
        drop(state);
        self.shared.changed.notify_one();
        Ok(request)
    }

    pub fn poll(&mut self, model: &HeadlessRoot) -> Result<Option<SyntaxResult>, SyntaxError> {
        self.check(model)?;
        let mut state = self.shared.state.lock().map_err(|_| SyntaxError::Worker)?;
        if state.failed {
            state.failed = false;
            return Err(SyntaxError::Worker);
        }
        let result = state.ready.take();
        let Some(document) = &self.document else {
            return Ok(None);
        };
        let text = match &model
            .node(self.node)
            .ok_or(SyntaxError::Node)?
            .descriptor
            .content
        {
            NodeContent::Input { value, .. } | NodeContent::Textarea { value, .. } => value,
            _ => return Err(SyntaxError::Node),
        };
        Ok(result.filter(|result| {
            result.request == self.request
                && result.document_revision == document.revision()
                && text == document.text()
        }))
    }

    pub fn cancel(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.latest = 0;
            state.pending = None;
            state.ready = None;
        }
        self.document = None;
        self.style = None;
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.cancel();
        if let Ok(mut state) = self.shared.state.lock() {
            state.stopped = true;
        }
        self.shared.changed.notify_one();
    }

    fn check(&mut self, model: &HeadlessRoot) -> Result<(), SyntaxError> {
        if self.disposed || self.lifetime.is_disposed() {
            self.dispose();
            return Err(SyntaxError::Disposed);
        }
        if model.id() != self.lifetime.id() {
            return Err(SyntaxError::Node);
        }
        if !editor(model, self.node) {
            self.dispose();
            return Err(SyntaxError::Node);
        }
        Ok(())
    }
}

fn editor(model: &HeadlessRoot, node: NodeId) -> bool {
    node.root == model.id()
        && model.node(node).is_some_and(|node| {
            matches!(
                node.descriptor.content,
                NodeContent::Input { .. } | NodeContent::Textarea { .. }
            )
        })
}

impl Drop for SyntaxWorker {
    fn drop(&mut self) {
        self.dispose();
    }
}
