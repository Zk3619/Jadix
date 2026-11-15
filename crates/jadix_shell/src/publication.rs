//! Verified immutable releases for an operator-owned static HTTPS origin.
//! Publish resource directories first; atomically replace the signed channel last.
use crate::{
    package::{Manifest, PackageError, PackagePolicy, SignedManifest},
    store::{regular_read, replace_file, sync_directory, write_new, StoreError},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Read declared resources with the same per-resource/aggregate package limits.
/// Canonical containment rejects links that escape the selected resource root.
pub fn read_resources(
    root: &Path,
    manifest: &Manifest,
) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
    manifest.signature_input()?;
    let root = root.canonicalize()?;
    let mut resources = BTreeMap::new();
    let mut total = 0;
    for name in manifest.resources.keys() {
        if name.eq_ignore_ascii_case("manifest.json") || name.eq_ignore_ascii_case("channel.json") {
            return Err(StoreError::Format);
        }
        let path = root.join(name).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(StoreError::Format);
        }
        let bytes = regular_read(&path, 4 * 1024 * 1024)?;
        total += bytes.len();
        if total > 16 * 1024 * 1024 {
            return Err(StoreError::Limit);
        }
        resources.insert(name.clone(), bytes);
    }
    Ok(resources)
}

struct WriterLock(PathBuf);
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn lock(root: &Path) -> Result<WriterLock, StoreError> {
    let path = root.join(".publication.lock");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(_) => Ok(WriterLock(path)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(StoreError::Locked),
        Err(e) => Err(e.into()),
    }
}

/// Verify all bytes before writing; an existing release is never overwritten.
/// Interrupted staging remains unpublished. A crash-held writer lock fails closed
/// until the operator verifies the old process is gone and removes that lock.
pub fn publish_release(
    root: &Path,
    signed: &SignedManifest,
    resources: &BTreeMap<String, Vec<u8>>,
    policy: &PackagePolicy,
) -> Result<PathBuf, StoreError> {
    signed.verify(policy, resources)?;
    if signed.manifest.resources.keys().any(|name| {
        name.eq_ignore_ascii_case("manifest.json") || name.eq_ignore_ascii_case("channel.json")
    }) {
        return Err(StoreError::Format);
    }
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let _lock = lock(&root)?;
    let releases = root.join("releases");
    fs::create_dir_all(&releases)?;
    if fs::symlink_metadata(&releases)?.file_type().is_symlink()
        || releases.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(StoreError::Format);
    }
    let destination = releases.join(&signed.manifest.release);
    if destination.exists() {
        return Err(StoreError::ReleaseConflict);
    }
    let staging = releases.join(format!(".staging-{}", signed.manifest.release));
    fs::create_dir(&staging)?;
    // Keep incomplete staging for diagnosis/recovery. It is never a channel target.
    for (name, bytes) in resources {
        let path = staging.join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        write_new(&path, bytes)?;
        sync_directory(path.parent().unwrap())?;
    }
    let bytes = serde_json::to_vec(signed).map_err(|_| StoreError::Format)?;
    if bytes.len() > 256 * 1024 {
        return Err(StoreError::Limit);
    }
    write_new(&staging.join("manifest.json"), &bytes)?;
    sync_directory(&staging)?;
    fs::rename(&staging, &destination)?;
    sync_directory(&releases)?;
    promote_locked(&root, signed, &bytes, policy)?;
    Ok(destination)
}

/// Resume promotion after an upload failure, or select an already staged newer
/// release. Re-verifies bytes on disk; channel rollback requires a new release ID.
pub fn promote_release(
    root: &Path,
    release: u64,
    policy: &PackagePolicy,
) -> Result<(), StoreError> {
    let root = root.canonicalize()?;
    let _lock = lock(&root)?;
    let path = root.join("releases").join(release.to_string());
    if !path.canonicalize()?.starts_with(&root) {
        return Err(StoreError::Format);
    }
    let bytes = regular_read(&path.join("manifest.json"), 256 * 1024)?;
    let signed = SignedManifest::decode(&bytes)?;
    if signed.manifest.release != release.to_string() {
        return Err(StoreError::Format);
    }
    signed.verify(policy, &read_resources(&path, &signed.manifest)?)?;
    promote_locked(&root, &signed, &bytes, policy)
}
fn promote_locked(
    root: &Path,
    signed: &SignedManifest,
    bytes: &[u8],
    policy: &PackagePolicy,
) -> Result<(), StoreError> {
    let channel = root.join("channel.json");
    if channel.exists() {
        let old = SignedManifest::decode(&regular_read(&channel, 256 * 1024)?)?;
        old.verify_manifest(policy)?;
        let previous: u64 = old
            .manifest
            .release
            .parse()
            .map_err(|_| StoreError::Format)?;
        let next: u64 = signed
            .manifest
            .release
            .parse()
            .map_err(|_| StoreError::Format)?;
        if next < previous || (next == previous && old != *signed) {
            return Err(PackageError::Rollback.into());
        }
    }
    replace_file(root, "channel.json", bytes)
}
