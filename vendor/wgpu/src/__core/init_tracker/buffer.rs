use super::{InitTracker, MemoryInitKind};
use crate::__core::resource::Buffer;
use alloc::sync::Arc;
use core::ops::Range;

#[derive(Debug, Clone)]
pub(crate) struct BufferInitTrackerAction {
    pub buffer: Arc<Buffer>,
    pub range: Range<crate::__types::BufferAddress>,
    pub kind: MemoryInitKind,
}

pub(crate) type BufferInitTracker = InitTracker<crate::__types::BufferAddress>;

impl BufferInitTracker {
    /// Checks if an action has/requires any effect on the initialization status
    /// and shrinks its range if possible.
    pub(crate) fn check_action(
        &self,
        action: &BufferInitTrackerAction,
    ) -> Option<BufferInitTrackerAction> {
        self.create_action(&action.buffer, action.range.clone(), action.kind)
    }

    /// Creates an action if it would have any effect on the initialization
    /// status and shrinks the range if possible.
    pub(crate) fn create_action(
        &self,
        buffer: &Arc<Buffer>,
        query_range: Range<crate::__types::BufferAddress>,
        kind: MemoryInitKind,
    ) -> Option<BufferInitTrackerAction> {
        self.check(query_range)
            .map(|range| BufferInitTrackerAction {
                buffer: buffer.clone(),
                range,
                kind,
            })
    }
}
