//! Deferred path texture readback requests and completed pixels.
type CapturedImage = (u32, u32, Vec<u8>);
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
thread_local! {
static RT_READBACK_REQ: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
static RT_READBACK: RefCell<HashMap<String, CapturedImage>> = RefCell::new(HashMap::new());
}
/// Paint called `get_image_data`: after PathPipeline flush, copy the RT (低频, stalls).
pub fn request_path_rt_readback(key: impl Into<String>) {
    RT_READBACK_REQ.with(|s| {
        s.borrow_mut().insert(key.into());
    });
}

pub fn path_rt_readback_is_pending(key: &str) -> bool {
    RT_READBACK_REQ.with(|s| s.borrow().contains(key))
}

pub fn take_path_rt_readback_request(key: &str) -> bool {
    RT_READBACK_REQ.with(|s| s.borrow_mut().remove(key))
}

pub fn store_path_rt_readback(key: String, width: u32, height: u32, pixels: Vec<u8>) {
    RT_READBACK.with(|m| {
        m.borrow_mut().insert(key, (width, height, pixels));
    });
}

/// GPU RT pixels from the last flush that honored [`request_path_rt_readback`].
pub fn take_path_rt_readback(key: &str) -> Option<(u32, u32, Vec<u8>)> {
    RT_READBACK.with(|m| m.borrow_mut().remove(key))
}
