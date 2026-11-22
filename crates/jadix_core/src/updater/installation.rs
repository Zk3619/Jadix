//! Directory installation. Staging and rollback never remove a path
//! that was not exclusively created or renamed by this transaction.
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Installation {
    root: PathBuf,
    executable: PathBuf,
    metadata: PathBuf,
    id: String,
    identity: (u64, u64),
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn inside(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let path = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("update file escapes application directory".into());
    }
    Ok(path)
}
fn identity(root: &Path, metadata: &Path, id: &str, version: &str) -> Result<(), String> {
    let mut bytes = Vec::new();
    fs::File::open(inside(root, metadata)?)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("application identity exceeds limit".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if value.get("id").and_then(|v| v.as_str()) != Some(id)
        || value.get("version").and_then(|v| v.as_str()) != Some(version)
    {
        return Err("application update identity/version mismatch".into());
    }
    Ok(())
}
#[cfg(unix)]
fn directory_identity(path: &Path) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok((metadata.dev(), metadata.ino()))
}
#[cfg(windows)]
fn directory_identity(path: &Path) -> Result<(u64, u64), String> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let directory = fs::OpenOptions::new().read(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(path).map_err(|e| e.to_string())?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(directory.as_raw_handle(), &mut info) } == 0 { return Err(std::io::Error::last_os_error().to_string()); }
    Ok((u64::from(info.dwVolumeSerialNumber), (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow)))
}
fn executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path).map_err(|e| e.to_string())?.permissions().mode() & 0o111 == 0 {
            return Err("application update is not executable".into());
        }
    }
    #[cfg(windows)]
    {
        let mut magic = [0; 2];
        fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic)).map_err(|e| e.to_string())?;
        if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("exe")) || magic != *b"MZ" {
            return Err("application update is not a Windows executable".into());
        }
    }
    Ok(())
}
fn scratch(parent: &Path) -> Result<PathBuf, String> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    for _ in 0..32 {
        let path = parent.join(format!(
            ".jadix-update-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("cannot reserve update staging directory".into())
}
impl Installation {
    /// `metadata` is packaged JSON with `id` and `version`; paths are relative
    /// to an application directory, never an arbitrary system prefix.
    pub fn new(
        root: &Path,
        executable: &Path,
        metadata: &Path,
        config: &super::Config,
    ) -> Result<Self, String> {
        config.validate()?;
        if !relative(executable) || !relative(metadata) {
            return Err("installation paths must be relative normal components".into());
        }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() || root.parent().is_none() {
            return Err("invalid application installation directory".into());
        }
        inside(&root, executable)?;
        identity(&root, metadata, &config.id, &config.version)?;
        let directory_identity = directory_identity(&root)?;
        Ok(Self {
            root,
            executable: executable.into(),
            metadata: metadata.into(),
            id: config.id.clone(),
            identity: directory_identity,
        })
    }
    pub fn executable(&self) -> PathBuf {
        self.root.join(&self.executable)
    }
    pub fn can_install(&self) -> bool {
        scratch(self.root.parent().unwrap()).is_ok_and(|path| fs::remove_dir(path).is_ok())
    }
    pub fn stage(&self, archive: &Path, version: &str) -> Result<Staged, String> {
        self.stage_with_cancel(archive, version, || false)
    }
    /// Prepare an update off the UI thread, allowing shutdown to cancel extraction.
    pub fn stage_with_cancel(&self, archive: &Path, version: &str, cancelled: impl Fn() -> bool) -> Result<Staged, String> {
        let owned = scratch(self.root.parent().unwrap())?;
        let mut staged = Staged {
            installation: self.clone(),
            owned,
            new: PathBuf::new(),
            retain: false,
            version: version.into(),
        };
        let extracted = staged.owned.join("unpacked");
        super::archive::unpack_with_cancel(archive, &extracted, &cancelled)?;
        let mut candidates = vec![extracted.clone()];
        let entries = fs::read_dir(&extracted)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if entries.len() == 1 && entries[0].file_type().map_err(|e| e.to_string())?.is_dir() {
            candidates.push(entries[0].path());
        }
        for candidate in candidates {
            if cancelled() { return Err("application update cancelled".into()); }
            let candidate = candidate.canonicalize().map_err(|e| e.to_string())?;
            if identity(&candidate, &self.metadata, &self.id, version).is_err() {
                continue;
            }
            executable(&inside(&candidate, &self.executable)?)?;
            staged.new = candidate;
            return Ok(staged);
        }
        Err("archive contains no matching application".into())
    }
}
pub struct Staged {
    installation: Installation,
    owned: PathBuf,
    new: PathBuf,
    retain: bool,
    version: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    schema: u32,
    installation: Installation,
    new: PathBuf,
    version: String,
}
impl Staged {
    /// Transfer an exclusively owned transaction to an external installer.
    /// A failed launch removes the staging directory; a successful launch owns
    /// cleanup and must call `resume_handoff` before touching installation paths.
    pub fn handoff(mut self, launch: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
        use std::io::Write;
        let bytes = serde_json::to_vec(&Handoff { schema: 1, installation: self.installation.clone(), new: self.new.clone(), version: self.version.clone() }).map_err(|e| e.to_string())?;
        if bytes.len() > 65536 { return Err("update transaction exceeds limit".into()); }
        let path = self.owned.join("transaction.json");
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
        drop(file);
        launch(&path)?;
        self.retain = true;
        Ok(())
    }

    /// Resume a local transaction after the process holding executable files
    /// has exited. Invalid plans never acquire cleanup ownership.
    pub fn resume_handoff(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() || path.file_name().is_none_or(|n| n != "transaction.json") { return Err("invalid update transaction path".into()); }
        let owned = path.parent().ok_or("missing update transaction directory")?;
        if owned.canonicalize().map_err(|e| e.to_string())? != owned || !owned.file_name().is_some_and(|n| n.to_string_lossy().starts_with(".jadix-update-")) {
            return Err("invalid update transaction directory".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path).map_err(|e| e.to_string())?.take(65537).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > 65536 { return Err("update transaction exceeds limit".into()); }
        let plan: Handoff = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let installation = plan.installation;
        if plan.schema != 1 || installation.root.parent() != owned.parent() || installation.root == owned
            || !relative(&installation.executable) || !relative(&installation.metadata)
            || installation.root.canonicalize().map_err(|e| e.to_string())? != installation.root
            || directory_identity(&installation.root)? != installation.identity
            || plan.new.canonicalize().map_err(|e| e.to_string())? != plan.new
            || !plan.new.starts_with(owned.join("unpacked"))
        { return Err("update transaction does not match its installation".into()); }
        identity(&plan.new, &installation.metadata, &installation.id, &plan.version)?;
        executable(&inside(&plan.new, &installation.executable)?)?;
        Ok(Self { installation, owned: owned.into(), new: plan.new, version: plan.version, retain: false })
    }

    /// Validate a post-swap action (for example spawning the new executable).
    /// Any action failure restores the original directory before returning.
    pub fn commit(
        mut self,
        action: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<PathBuf, String> {
        if directory_identity(&self.installation.root)? != self.installation.identity {
            return Err("application installation changed while update was staged".into());
        }
        identity(&self.new, &self.installation.metadata, &self.installation.id, &self.version)?;
        executable(&inside(&self.new, &self.installation.executable)?)?;
        let old = &self.installation.root;
        let backup = self.owned.join("previous");
        fs::rename(old, &backup).map_err(|e| format!("reserve previous application: {e}"))?;
        if let Err(error) = fs::rename(&self.new, old) {
            return match fs::rename(&backup, old) {
                Ok(()) => Err(format!("install application: {error}")),
                Err(rollback) => {
                    self.retain = true;
                    Err(format!(
                        "install failed: {error}; original retained at {}: {rollback}",
                        backup.display()
                    ))
                }
            };
        }
        let executable = self.installation.executable();
        let action = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(&executable)));
        if !matches!(action, Ok(Ok(()))) {
            // Keep both versions if rollback itself fails; never delete the backup.
            if let Err(rollback) = fs::rename(old, &self.new).and_then(|_| fs::rename(&backup, old))
            {
                self.retain = true;
                return Err(format!(
                    "post-install failed; rollback failed: {rollback}; original retained at {}",
                    backup.display()
                ));
            }
            return match action {
                Ok(Err(error)) => Err(error),
                Err(panic) => std::panic::resume_unwind(panic),
                _ => unreachable!(),
            };
        }
        Ok(executable)
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        // A failed rollback leaves the original here for manual recovery.
        if !self.retain {
            let _ = fs::remove_dir_all(&self.owned);
        }
    }
}
#[cfg(test)]
mod tests;
