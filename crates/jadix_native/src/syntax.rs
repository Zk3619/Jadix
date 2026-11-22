use crate::{
    presentation::{NodeContent, NodeId, RootLifetime},
    HeadlessRoot,
};
pub use jadix_document::{syntax::HighlightSpan, TextDocument};
use std::ops::Range;

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

#[derive(Debug)]
pub struct SyntaxResult {
    pub node: NodeId,
    pub request: u64,
    pub document_revision: u64,
    pub spans: Vec<HighlightSpan>,
    pub plain_fallback: bool,
}

pub struct SyntaxWorker {
    lifetime: RootLifetime,
    node: NodeId,
    worker: Option<jadix_document::syntax::SyntaxWorker>,
    document: Option<TextDocument>,
    style: Option<(String, bool)>,
    visible: Option<Range<usize>>,
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
        let worker =
            jadix_document::syntax::SyntaxWorker::new(wake).map_err(|_| SyntaxError::Worker)?;
        Ok(Self {
            lifetime: model.lifetime(),
            node,
            worker: Some(worker),
            document: None,
            style: None,
            visible: None,
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
        self.submit(model, document, language, dark, None)
    }

    /// Preserve full grammar state, publishing the visible lines and 32 lines of
    /// lookahead early. Only that window is converted into native styled spans.
    pub fn request_visible(
        &mut self,
        model: &HeadlessRoot,
        document: TextDocument,
        language: &str,
        dark: bool,
        lines: Range<usize>,
    ) -> Result<u64, SyntaxError> {
        self.submit(model, document, language, dark, Some(lines))
    }

    fn submit(
        &mut self,
        model: &HeadlessRoot,
        document: TextDocument,
        language: &str,
        dark: bool,
        visible: Option<Range<usize>>,
    ) -> Result<u64, SyntaxError> {
        self.check(model)?;
        if document.len() > 2 * 1024 * 1024
            || document.text().lines().any(|line| line.len() > 64 * 1024)
            || language.len() > 64
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        {
            return Err(SyntaxError::Limits);
        }
        if self.document.as_ref() == Some(&document)
            && self.visible == visible
            && self
                .style
                .as_ref()
                .is_some_and(|(current, theme)| current == language && *theme == dark)
        {
            return Ok(self.request);
        }
        let request = self.request.checked_add(1).ok_or(SyntaxError::Exhausted)?;
        let worker = self.worker.as_mut().ok_or(SyntaxError::Disposed)?;
        if let Some(lines) = visible.clone() {
            worker.request_visible(document.clone(), language, dark, lines);
        } else {
            worker.request(document.clone(), language, dark);
        }
        self.document = Some(document);
        self.style = Some((language.into(), dark));
        self.visible = visible;
        self.request = request;
        Ok(request)
    }

    pub fn poll(&mut self, model: &HeadlessRoot) -> Result<Option<SyntaxResult>, SyntaxError> {
        self.check(model)?;
        let result = self.worker.as_mut().ok_or(SyntaxError::Disposed)?.poll();
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
        Ok(result
            .filter(|result| result.document() == document && text == document.text())
            .map(|result| {
                let mut spans = Vec::new();
                let window = self
                    .visible
                    .as_ref()
                    .map(|lines| {
                        document
                            .line_full_range(lines.start)
                            .map_or(document.len(), |r| r.start)
                            ..document
                                .line_full_range(
                                    lines
                                        .end
                                        .max(lines.start.saturating_add(1))
                                        .saturating_add(31),
                                )
                                .map_or(document.len(), |r| r.end)
                    })
                    .unwrap_or(0..document.len());
                let mut offset = 0;
                for annotation in result.annotations() {
                    let range = annotation.range.start.max(window.start)
                        ..annotation.range.end.min(window.end);
                    if range.start >= range.end {
                        continue;
                    }
                    if offset < range.start {
                        spans.push(HighlightSpan {
                            text: text[offset..range.start].into(),
                            color: None,
                        });
                    }
                    spans.push(HighlightSpan {
                        text: text[range.clone()].into(),
                        color: Some(annotation.value),
                    });
                    offset = range.end;
                    if spans.len() > MAX_SPANS {
                        break;
                    }
                }
                let plain_fallback = spans.len() > MAX_SPANS;
                if plain_fallback {
                    spans = vec![HighlightSpan {
                        text: text.clone(),
                        color: None,
                    }];
                } else if offset < text.len() {
                    spans.push(HighlightSpan {
                        text: text[offset..].into(),
                        color: None,
                    });
                }
                SyntaxResult {
                    node: self.node,
                    request: self.request,
                    document_revision: document.revision(),
                    spans,
                    plain_fallback,
                }
            }))
    }

    pub fn cancel(&mut self) {
        if let Some(worker) = self.worker.as_mut() {
            worker.cancel();
        }
        self.document = None;
        self.style = None;
        self.visible = None;
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.cancel();
        self.worker.take();
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
