use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Default TOML filename next to the chosen storage root.
pub const DEFAULT_FILENAME: &str = "window-state.toml";

/// Override file or directory: `JADIX_WINDOW_STATE_PATH`.
pub const ENV_PATH: &str = "JADIX_WINDOW_STATE_PATH";

/// Where the TOML file is resolved.
///
/// Explicit [`Self::File`] / [`Self::Directory`] always win over the env var.
/// [`Self::CurrentDir`] (default) and [`Self::ExecutableDir`] honour `JADIX_WINDOW_STATE_PATH`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageLocation {
    /// Process working directory — **运行路径** (default).
    CurrentDir,
    /// Directory containing `std::env::current_exe()`.
    ExecutableDir,
    /// Exact TOML file path.
    File(PathBuf),
    /// Directory; `filename` is joined.
    Directory(PathBuf),
}

impl Default for StorageLocation {
    fn default() -> Self {
        Self::CurrentDir
    }
}

/// Resolve the TOML path.
///
/// `env_override` is typically `std::env::var(ENV_PATH).ok()`. Tests inject a value
/// instead of mutating process env.
pub fn resolve_storage_path(
    location: &StorageLocation,
    filename: &str,
    env_override: Option<&str>,
) -> Result<PathBuf> {
    let filename = filename.trim();
    if filename.is_empty() {
        return Err(Error::Path("window state filename is empty".into()));
    }

    match location {
        StorageLocation::File(path) => Ok(path.clone()),
        StorageLocation::Directory(dir) => Ok(dir.join(filename)),
        StorageLocation::CurrentDir | StorageLocation::ExecutableDir => {
            if let Some(raw) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
                return interpret_override(Path::new(raw), filename);
            }
            let root = match location {
                StorageLocation::CurrentDir => {
                    std::env::current_dir().map_err(|e| Error::Location(PathBuf::from("."), e))?
                }
                StorageLocation::ExecutableDir => executable_dir()?,
                _ => unreachable!(),
            };
            Ok(root.join(filename))
        }
    }
}

pub fn executable_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| Error::Location(PathBuf::from("<exe>"), e))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Path(format!("executable has no parent: {}", exe.display())))
}

fn interpret_override(raw: &Path, filename: &str) -> Result<PathBuf> {
    if raw.is_dir() {
        return Ok(raw.join(filename));
    }
    if raw.is_file() {
        return Ok(raw.to_path_buf());
    }
    let looks_like_file = raw
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        || raw
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains('.'));
    if looks_like_file {
        Ok(raw.to_path_buf())
    } else {
        Ok(raw.join(filename))
    }
}

#[cfg(test)]
#[path = "tests/path.rs"]
mod tests;
