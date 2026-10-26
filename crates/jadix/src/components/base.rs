pub use crate::authoring::*;

use std::cell::RefCell;

thread_local! {
    static PENDING_ITEM_CHROME: RefCell<SlotChrome> = const {
        RefCell::new(SlotChrome {
            class: String::new(),
            style: None,
        })
    };
}

/// Stash chrome for collect-then-paint items ([`crate::components::StepperItem`] /
/// [`crate::components::TimelineItem`]). The returned marker is discarded by
/// `*_scope`, so RSX `chrome_binds` never reach the painted row.
pub fn push_pending_item_chrome(chrome: SlotChrome) {
    PENDING_ITEM_CHROME.with(|c| *c.borrow_mut() = chrome);
}

pub fn take_pending_item_chrome() -> SlotChrome {
    PENDING_ITEM_CHROME.with(|c| std::mem::take(&mut *c.borrow_mut()))
}
