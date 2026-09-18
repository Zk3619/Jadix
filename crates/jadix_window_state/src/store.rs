use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::state::WindowState;

/// On-disk document. Tables are stable-sorted by label.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowStateFile {
    #[serde(default)]
    pub windows: BTreeMap<String, WindowState>,
}

impl WindowStateFile {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)?;
        match toml::from_str::<Self>(&raw) {
            Ok(doc) => Ok(doc),
            Err(e) => {
                let bak = backup_path(path);
                if let Err(io_e) = fs::rename(path, &bak) {
                    log::warn!(
                        "jadix_window_state: corrupt {} ({e}); backup failed: {io_e}",
                        path.display()
                    );
                } else {
                    log::warn!(
                        "jadix_window_state: corrupt {}, moved to {}",
                        path.display(),
                        bak.display()
                    );
                }
                let _ = e;
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let body = toml::to_string_pretty(self)?;
        atomic_write(path, body.as_bytes())
    }
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "window-state.toml".into());
    name.push(".bak");
    path.with_file_name(name)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = {
        let mut name = path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| "window-state.toml".into());
        name.push(".tmp");
        path.with_file_name(name)
    };
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_file() -> PathBuf {
        std::env::temp_dir().join(format!(
            "jadix-ws-store-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn roundtrip_toml() {
        let path = unique_file();
        let mut doc = WindowStateFile::default();
        doc.windows.insert(
            "main".into(),
            WindowState {
                width: 1280,
                height: 800,
                x: 12,
                y: 34,
                prev_x: 12,
                prev_y: 34,
                maximized: false,
                visible: true,
                decorated: true,
                fullscreen: false,
            },
        );
        doc.save(&path).unwrap();
        let loaded = WindowStateFile::load(&path).unwrap();
        assert_eq!(loaded, doc);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[windows.main]"));
        assert!(text.contains("width = 1280"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_empty() {
        let path = unique_file();
        let doc = WindowStateFile::load(&path).unwrap();
        assert!(doc.windows.is_empty());
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let path = unique_file();
        fs::write(&path, "not = [ toml").unwrap();
        let doc = WindowStateFile::load(&path).unwrap();
        assert!(doc.windows.is_empty());
        let bak = backup_path(&path);
        assert!(bak.exists());
        let _ = fs::remove_file(&bak);
        let _ = fs::remove_file(&path);
    }
}
