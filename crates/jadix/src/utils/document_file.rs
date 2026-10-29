//! UTF-8 document saves through a sibling temporary file.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SAVE: AtomicU64 = AtomicU64::new(1);

pub fn save_text_document(path: &Path, text: &str, expected: Option<&str>) -> io::Result<()> {
    let check = || -> io::Result<()> {
        if let Some(expected) = expected {
            if fs::read_to_string(path)? != expected {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Document changed on disk; save to another path",
                ));
            }
        }
        Ok(())
    };
    check()?;
    let temporary = path.with_extension(format!(
        "jadix-{}-{}.tmp",
        std::process::id(),
        NEXT_SAVE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        check()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "tests/document_file.rs"]
mod tests;
