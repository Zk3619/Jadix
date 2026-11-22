//! Document-scoped native evaluation. The UI pump wakes futures after releasing
//! its application borrow, so user executors can safely reenter the root.
use crate::presentation::{NodeContent, NodeId, PresentationNode};
use jadix_core::{hosted::HostedContent, web_evaluation::*};
use std::{collections::BTreeMap, time::Instant};

struct Document { specification: String, ready: bool, session: WebEvaluationSession }
pub(crate) struct NativeWeb {
    documents: BTreeMap<NodeId, Document>,
    replies: BTreeMap<u64, EvaluationReply>,
    batches: Vec<(NodeId, EvaluationBatch)>,
    clock: Instant,
    serial: u64,
}
impl Default for NativeWeb {
    fn default() -> Self { Self { documents: BTreeMap::new(), replies: BTreeMap::new(), batches: Vec::new(), clock: Instant::now(), serial: 0 } }
}
impl NativeWeb {
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ { self.documents.keys().copied() }
    #[cfg(any(feature = "android", feature = "linux", feature = "windows"))]
    pub fn has_pending(&self) -> bool { !self.batches.is_empty() || self.documents.values().any(|doc| doc.session.pending_count() != 0) }
    pub fn contains(&self, node: NodeId) -> bool { self.documents.contains_key(&node) }
    pub fn sync(&mut self, roots: &[PresentationNode]) {
        fn collect(nodes: &[PresentationNode], out: &mut BTreeMap<NodeId, String>) {
            for node in nodes {
                if let NodeContent::Hosted { specification } = &node.descriptor.content {
                    if matches!(HostedContent::from_json(specification), Ok(HostedContent::Web { .. })) { out.insert(node.id, specification.clone()); }
                }
                collect(&node.children, out);
            }
        }
        let mut live = BTreeMap::new(); collect(roots, &mut live);
        for id in self.documents.keys().copied().collect::<Vec<_>>() {
            if live.get(&id).is_none_or(|spec| *spec != self.documents[&id].specification) {
                if let Some(mut document) = self.documents.remove(&id) { self.batches.push((id, document.session.abort())); }
            }
        }
        for (id, specification) in live { self.documents.entry(id).or_insert_with(|| Document { specification, ready: false, session: WebEvaluationSession::default() }); }
    }
    pub fn ready(&mut self, id: NodeId, ready: bool) {
        if let Some(document) = self.documents.get_mut(&id) {
            if !ready { self.batches.push((id, document.session.abort())); }
            document.ready = ready;
        }
    }
    pub fn evaluate(&mut self, id: NodeId, script: &str) -> WebEvaluation {
        if self.documents.values().map(|doc| doc.session.pending_count()).sum::<usize>() >= MAX_PENDING { return WebEvaluation::failed(WebEvaluationError::TooManyPending); }
        let Some(document) = self.documents.get_mut(&id) else { return WebEvaluation::failed(WebEvaluationError::Unavailable); };
        if !document.ready { return WebEvaluation::failed(WebEvaluationError::NotReady); }
        let (future, batch) = document.session.start(script, self.clock.elapsed());
        self.batches.push((id, batch)); future
    }
    pub fn complete(&mut self, request: u64, result: Result<String, String>) -> bool {
        let Some(reply) = self.replies.remove(&request) else { return false; };
        reply.complete(result); true
    }
    pub fn pump(&mut self) -> (Vec<(NodeId, u64, String)>, Vec<EvaluationBatch>) {
        for (id, document) in &mut self.documents { self.batches.push((*id, document.session.advance(self.clock.elapsed()))); }
        self.replies.retain(|_, reply| reply.is_active());
        let mut work = Vec::new(); let mut wake = Vec::new();
        for (id, mut batch) in std::mem::take(&mut self.batches) {
            for item in std::mem::take(&mut batch.work) {
                if !item.cleanup && !item.reply.is_active() { continue; }
                // Only explicitly marked cleanup may outlive its reply.
                if let Some(serial) = self.serial.checked_add(1) {
                    self.serial = serial;
                    if item.reply.is_active() { self.replies.insert(serial, item.reply); }
                    work.push((id, serial, item.script));
                } else { item.reply.complete(Err("evaluation IDs exhausted".into())); }
            }
            wake.push(batch);
        }
        (work, wake)
    }
    pub fn dispose(&mut self) {
        for (id, mut document) in std::mem::take(&mut self.documents) { self.batches.push((id, document.session.abort())); }
        self.replies.clear();
    }
}

impl Drop for NativeWeb {
    fn drop(&mut self) {
        self.dispose();
        // The owning application must be removed from its registry before drop.
        // Abort batches otherwise outlive the last UI pump and strand waiters.
        for (_, batch) in std::mem::take(&mut self.batches) { batch.wake(); }
    }
}
