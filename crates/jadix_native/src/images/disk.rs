use super::{ImageError, ImageSource, Loaded};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HEADER: usize = 56;
const MAGIC: &[u8; 8] = b"JDXIMG01";

#[derive(Clone, Debug)]
pub struct ImageDiskCache {
    pub directory: PathBuf,
    pub bytes: usize,
    pub entries: usize,
    pub age: Duration,
}

impl ImageDiskCache {
    pub(super) fn validate(&self) -> Result<(), ImageError> {
        if !self.directory.is_absolute() || self.directory.as_os_str().len() > 4096 {
            return Err(ImageError::Source);
        }
        if self.bytes <= HEADER
            || self.bytes > 64 * 1024 * 1024
            || self.entries == 0
            || self.entries > 128
            || self.age.as_millis() == 0
            || self.age > Duration::from_secs(86400)
        {
            return Err(ImageError::Limits);
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Statistics {
    pub entries: AtomicUsize,
    pub bytes: AtomicUsize,
}

pub(super) struct WorkerActivity(pub Arc<AtomicBool>);

impl Drop for WorkerActivity {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct Cache {
    config: ImageDiskCache,
    directory: PathBuf,
    _lock: File,
    stats: Arc<Statistics>,
}

fn io(error: std::io::Error) -> ImageError {
    ImageError::Io(error.kind())
}

fn millis() -> Result<u64, ImageError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ImageError::Source)?
        .as_millis()
        .try_into()
        .map_err(|_| ImageError::Limits)
}

fn key(source: &ImageSource) -> String {
    let digest = match source {
        ImageSource::File(path) => Sha256::digest(path.as_os_str().as_encoded_bytes()),
        ImageSource::Url(url) => Sha256::new()
            .chain_update(b"url\0")
            .chain_update(url.as_bytes())
            .finalize(),
    };
    format!("{digest:x}")
}

fn managed(name: &str) -> bool {
    name.len() == 68
        && name.ends_with(".jic")
        && name.as_bytes()[..64]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

impl Cache {
    pub fn open(config: ImageDiskCache, stats: Arc<Statistics>) -> Result<Self, ImageError> {
        let directory = config.directory.join("jadix-images-v1");
        std::fs::create_dir_all(&directory).map_err(io)?;
        if !std::fs::symlink_metadata(&directory)
            .map_err(io)?
            .file_type()
            .is_dir()
        {
            return Err(ImageError::Source);
        }
        let lock_path = directory.join(".lock");
        if let Ok(metadata) = std::fs::symlink_metadata(&lock_path) {
            if !metadata.file_type().is_file() {
                return Err(ImageError::Source);
            }
        }
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(lock_path)
            .map_err(io)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => ImageError::Busy,
            std::fs::TryLockError::Error(error) => io(error),
        })?;
        let cache = Self {
            config,
            directory,
            _lock: lock,
            stats,
        };
        let pending = cache.directory.join(".pending");
        if pending.exists() {
            if !std::fs::symlink_metadata(&pending)
                .map_err(io)?
                .file_type()
                .is_file()
            {
                return Err(ImageError::Source);
            }
            std::fs::remove_file(pending).map_err(io)?;
        }
        cache.trim(0)?;
        Ok(cache)
    }

    fn path(&self, source: &ImageSource) -> PathBuf {
        self.directory.join(format!("{}.jic", key(source)))
    }

    pub fn invalidate(&self, source: &ImageSource) -> Result<(), ImageError> {
        match std::fs::remove_file(self.path(source)) {
            Ok(()) => self.trim(0),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io(error)),
        }
    }

    pub fn read(&self, source: &ImageSource, limit: usize) -> Result<Option<Loaded>, ImageError> {
        let path = self.path(source);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        if !metadata.file_type().is_file() {
            return Err(ImageError::Source);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(io)?;
        let mut record = Vec::new();
        (&file)
            .take((limit + HEADER + 1) as u64)
            .read_to_end(&mut record)
            .map_err(io)?;
        let valid = record.len() >= HEADER
            && record.len() <= limit + HEADER
            && &record[..8] == MAGIC
            && u64::from_le_bytes(record[16..24].try_into().unwrap())
                == (record.len() - HEADER) as u64;
        if !valid {
            drop(file);
            self.invalidate(source)?;
            return Err(ImageError::Format);
        }
        let expires = u64::from_le_bytes(record[8..16].try_into().unwrap());
        let now = millis()?;
        let digest = Sha256::new()
            .chain_update(key(source).as_bytes())
            .chain_update(&record[..24])
            .chain_update(&record[HEADER..])
            .finalize();
        if digest.as_slice() != &record[24..HEADER] {
            drop(file);
            self.invalidate(source)?;
            return Err(ImageError::Format);
        }
        if expires <= now || expires - now > self.config.age.as_millis() as u64 {
            drop(file);
            self.invalidate(source)?;
            return Ok(None);
        }
        file.set_modified(SystemTime::now()).map_err(io)?;
        Ok(Some(Loaded {
            bytes: record.split_off(HEADER),
            expires: Instant::now().checked_add(Duration::from_millis(expires - now)),
        }))
    }

    pub fn write(&self, source: &ImageSource, loaded: &Loaded) -> Result<(), ImageError> {
        let age = match source {
            ImageSource::File(_) => self.config.age,
            ImageSource::Url(_) => match loaded
                .expires
                .and_then(|expires| expires.checked_duration_since(Instant::now()))
            {
                Some(age) if !age.is_zero() => age.min(self.config.age),
                _ => return Ok(()),
            },
        };
        let bytes = &loaded.bytes;
        let size = bytes.len().checked_add(HEADER).ok_or(ImageError::Limits)?;
        if size > self.config.bytes {
            return Err(ImageError::Limits);
        }
        self.invalidate(source)?;
        self.trim(size)?;
        let expires = millis()?
            .checked_add(age.as_millis() as u64)
            .ok_or(ImageError::Limits)?;
        let mut record = Vec::with_capacity(size);
        record.extend_from_slice(MAGIC);
        record.extend_from_slice(&expires.to_le_bytes());
        record.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        let digest = Sha256::new()
            .chain_update(key(source).as_bytes())
            .chain_update(&record)
            .chain_update(bytes)
            .finalize();
        record.extend_from_slice(&digest);
        record.extend_from_slice(bytes);
        let pending = self.directory.join(".pending");
        let result = (|| {
            let mut file = File::create_new(&pending).map_err(io)?;
            file.write_all(&record).map_err(io)?;
            file.sync_data().map_err(io)?;
            drop(file);
            std::fs::rename(&pending, self.path(source)).map_err(io)?;
            self.trim(0)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(pending);
        }
        result
    }

    fn trim(&self, reserve: usize) -> Result<(), ImageError> {
        let mut entries = Vec::new();
        for (index, entry) in std::fs::read_dir(&self.directory).map_err(io)?.enumerate() {
            if index >= 132 {
                return Err(ImageError::Limits);
            }
            let entry = entry.map_err(io)?;
            if !entry.file_name().to_str().is_some_and(managed) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(io)?;
            if !metadata.file_type().is_file() {
                return Err(ImageError::Source);
            }
            entries.push((
                metadata.modified().map_err(io)?,
                entry.path(),
                metadata.len(),
            ));
        }
        entries.sort();
        let mut bytes = entries
            .iter()
            .fold(0u64, |total, entry| total.saturating_add(entry.2));
        let mut count = entries.len();
        for (_, path, size) in entries {
            if bytes.saturating_add(reserve as u64) <= self.config.bytes as u64
                && count + usize::from(reserve > 0) <= self.config.entries
            {
                break;
            }
            std::fs::remove_file(path).map_err(io)?;
            bytes -= size;
            count -= 1;
        }
        self.stats.bytes.store(bytes as usize, Ordering::Release);
        self.stats.entries.store(count, Ordering::Release);
        Ok(())
    }
}
