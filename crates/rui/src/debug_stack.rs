//! Temporary stack-overflow breadcrumbs (file-backed; survives abort).
//! Remove once Gallery STATUS_STACK_OVERFLOW is resolved.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

pub fn crumb(msg: &str) {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let line = format!("[{n}] {msg}\n");
    let path =
        std::env::var("RUI_STACK_CRUMB").unwrap_or_else(|_| "tmp_stack_crumb.txt".to_string());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

pub fn reset_crumb_file() {
    let path =
        std::env::var("RUI_STACK_CRUMB").unwrap_or_else(|_| "tmp_stack_crumb.txt".to_string());
    let _ = std::fs::write(&path, "");
    SEQ.store(0, Ordering::Relaxed);
}
