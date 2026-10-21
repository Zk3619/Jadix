use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::snapshot::SourceLocation;

const MAX_SOURCE_BYTES: u64 = 512 * 1024;

#[derive(Clone)]
pub(super) struct SourceDocument {
    pub path: PathBuf,
    pub text: String,
}

impl SourceDocument {
    pub fn field_id(&self) -> String {
        format!("dt-source-{}", self.path.display())
    }
}

thread_local! {
    static DOCUMENTS: RefCell<HashMap<PathBuf, SourceDocument>> = RefCell::new(HashMap::new());
    static WRITES: Cell<bool> = const { Cell::new(false) };
    static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
}

fn workspace_root() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("JADIX_DEVTOOLS_ROOT") {
        return PathBuf::from(root)
            .canonicalize()
            .map_err(|err| err.to_string());
    }
    let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
    for parent in cwd.ancestors() {
        if std::fs::read_to_string(parent.join("Cargo.toml"))
            .is_ok_and(|text| text.contains("[workspace]"))
        {
            return parent.canonicalize().map_err(|err| err.to_string());
        }
    }
    cwd.canonicalize().map_err(|err| err.to_string())
}

fn resolve_source(root: &Path, path: &str) -> Result<PathBuf, String> {
    let target = root
        .join(path)
        .canonicalize()
        .map_err(|err| err.to_string())?;
    if !target.starts_with(root) || target.extension().is_none_or(|ext| ext != "rs") {
        return Err("Sources is limited to Rust files inside the workspace.".into());
    }
    if std::fs::metadata(&target)
        .map_err(|err| err.to_string())?
        .len()
        > MAX_SOURCE_BYTES
    {
        return Err("Source file exceeds the 512 KiB editor limit.".into());
    }
    Ok(target)
}

pub(super) fn document(location: &SourceLocation) -> Result<(SourceDocument, bool), String> {
    let path = resolve_source(&workspace_root()?, &location.path)?;
    DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        if let Some(document) = documents.get(&path) {
            return Ok((document.clone(), false));
        }
        let document = SourceDocument {
            text: std::fs::read_to_string(&path).map_err(|err| err.to_string())?,
            path: path.clone(),
        };
        documents.insert(path, document.clone());
        Ok((document, true))
    })
}

pub(super) fn can_write() -> bool {
    cfg!(all(debug_assertions, feature = "hot-reload"))
}

pub(super) fn writes_enabled() -> bool {
    can_write() && WRITES.get()
}

pub(super) fn toggle_writes() {
    WRITES.set(!WRITES.get());
    super::DevToolsState::refresh();
}

pub(super) fn status() -> String {
    STATUS.with(|status| status.borrow().clone())
}

pub(super) fn reload(document: &SourceDocument) -> Result<String, String> {
    let path = resolve_source(&workspace_root()?, &document.path.to_string_lossy())?;
    let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
    DOCUMENTS.with(|documents| {
        documents.borrow_mut().insert(
            path.clone(),
            SourceDocument {
                path,
                text: text.clone(),
            },
        );
    });
    Ok(text)
}

fn write_document(document: &SourceDocument, text: &str) -> Result<(), String> {
    if text.len() as u64 > MAX_SOURCE_BYTES {
        return Err("Source file exceeds the 512 KiB editor limit.".into());
    }
    let current = std::fs::read_to_string(&document.path).map_err(|err| err.to_string())?;
    if current != document.text {
        return Err(
            "File changed on disk. Copy your draft, then Reload from disk before saving.".into(),
        );
    }
    let temp = document
        .path
        .with_extension(format!("rs.jadix-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|err| err.to_string())?;
        let result = (|| {
            file.write_all(text.as_bytes())
                .map_err(|err| err.to_string())?;
            file.sync_all().map_err(|err| err.to_string())?;
            drop(file);
            std::fs::rename(&temp, &document.path).map_err(|err| err.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    })();
    result
}

pub(super) fn save(document: &SourceDocument, text: String) {
    let result = (|| {
        if !writes_enabled() {
            return Err("Enable source writes first.".into());
        }
        let path = resolve_source(&workspace_root()?, &document.path.to_string_lossy())?;
        if path != document.path {
            return Err("Source path changed. Reload before saving.".into());
        }
        let latest = DOCUMENTS
            .with(|documents| documents.borrow().get(&path).cloned())
            .ok_or_else(|| "Reload the source before saving.".to_string())?;
        write_document(&latest, &text)?;
        DOCUMENTS.with(|documents| {
            documents.borrow_mut().insert(
                path,
                SourceDocument {
                    path: document.path.clone(),
                    text,
                },
            );
        });
        Ok::<_, String>(())
    })();
    let message = match result {
        Ok(()) => crate::tr_with_default_zh(
            "jadix.devtools.source.saved",
            "Saved {path}. Supported UI changes patch live; other changes need --run or a manual rebuild.",
            "已保存 {path}。支持的 UI 变化即时更新，其余变化需 --run 托管或手动重建。",
            &[("path", &document.path.display().to_string())],
        ),
        Err(err) => crate::tr_with_default_zh("jadix.devtools.source.save_failed", "Save failed: {error}", "保存失败：{error}", &[("error", &super::locale::text(&err))]),
    };
    report(message);
}

pub(super) fn report(message: String) {
    STATUS.with(|status| *status.borrow_mut() = message.clone());
    super::DevToolsState::record_change("Sources", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_save_preserves_disk_edits_and_replaces_complete_file() {
        let root = std::env::temp_dir().join(format!("jadix-source-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.rs");
        std::fs::write(&path, "fn main() {}\r\n").unwrap();
        let document = SourceDocument {
            path: path.clone(),
            text: "fn main() {}\r\n".into(),
        };
        let edited = "fn main() { println!(\"saved\"); }\r\n";
        write_document(&document, edited).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
        assert!(write_document(&document, "stale draft")
            .unwrap_err()
            .contains("changed on disk"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn source_resolver_rejects_outside_workspace_and_non_rust_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(resolve_source(&root, "Cargo.toml").is_err());
        assert!(resolve_source(&root, "../jadix_macros/src/lib.rs").is_err());
        assert!(resolve_source(&root, "src/lib.rs").is_ok());
    }
}
