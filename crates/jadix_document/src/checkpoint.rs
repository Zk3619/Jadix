//! Bounded application checkpoints with checksums, atomic replacement and generation checks.
//! A crash may leave the exclusive `.lock` file behind; recovery is explicit, never guessed.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    schema: u32,
    pub generation: u64,
    pub payload: String,
    sha256: String,
}

pub struct CheckpointStore {
    path: PathBuf,
    max_bytes: usize,
}

struct Lease(PathBuf);
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn regular_or_absent(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err("Checkpoint path must be a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

impl CheckpointStore {
    pub fn new(path: impl Into<PathBuf>, max_bytes: usize) -> Result<Self, String> {
        let path = path.into();
        if max_bytes == 0 || max_bytes > 16 * 1024 * 1024 || path.file_name().is_none() {
            return Err("Invalid checkpoint path or byte limit".into());
        }
        Ok(Self { path, max_bytes })
    }

    pub fn read(&self) -> Result<Option<Checkpoint>, String> {
        if !regular_or_absent(&self.path)? {
            return Ok(None);
        }
        // JSON escaping can expand every payload byte to six bytes.
        let limit = self.max_bytes * 6 + 1024;
        let mut bytes = Vec::new();
        File::open(&self.path)
            .map_err(|e| e.to_string())?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err("Checkpoint file exceeds byte limit".into());
        }
        let value: Checkpoint = serde_json::from_slice(&bytes)
            .map_err(|_| "Malformed checkpoint; original file preserved")?;
        if value.schema != 1
            || value.generation == 0
            || value.payload.len() > self.max_bytes
            || value.sha256 != format!("{:x}", Sha256::digest(value.payload.as_bytes()))
        {
            return Err(
                "Invalid checkpoint checksum, version or size; original file preserved".into(),
            );
        }
        Ok(Some(value))
    }

    /// The caller must supply the generation it read; `None` means create, not overwrite.
    pub fn save(&self, expected: Option<u64>, payload: &str) -> Result<u64, String> {
        if payload.len() > self.max_bytes {
            return Err("Checkpoint payload exceeds byte limit".into());
        }
        let mut lock_name = self.path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock = PathBuf::from(lock_name);
        let mut lock_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .map_err(|_| "Checkpoint is locked; inspect the owner before recovery")?;
        let _lease = Lease(lock);
        writeln!(lock_file, "{}", std::process::id()).map_err(|e| e.to_string())?;
        lock_file.sync_all().map_err(|e| e.to_string())?;
        drop(lock_file);
        let current = self.read()?.map(|value| value.generation);
        if current != expected {
            return Err("Checkpoint generation conflict; reread before saving".into());
        }
        let generation = current
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Checkpoint generation exhausted")?;
        let value = Checkpoint {
            schema: 1,
            generation,
            payload: payload.into(),
            sha256: format!("{:x}", Sha256::digest(payload.as_bytes())),
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{}.new",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = PathBuf::from(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let cleanup = Lease(temporary.clone());
        serde_json::to_writer(&mut file, &value).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        // Same-directory replacement uses the platform's atomic rename operation.
        fs::rename(&temporary, &self.path).map_err(|e| e.to_string())?;
        drop(cleanup);
        Ok(generation)
    }
}
