use crate::package::{decode_hex, sha256, PackageError, PackagePolicy, SignedManifest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MANIFEST_LIMIT: u64 = 256 * 1024;
const RESOURCE_LIMIT: u64 = 4 * 1024 * 1024;
const STATE_LIMIT: u64 = 8192;
const BUSINESS_LIMIT: u64 = 256 * 1024;
const JOURNAL_LIMIT: u64 = BUSINESS_LIMIT + 1024;
const BUSINESS_JOURNAL: &str = "business-journal.json";

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Locked,
    Format,
    Package(PackageError),
    Pending,
    StaleActivation,
    NoPrevious,
    ReleaseConflict,
    Limit,
    NoCurrent,
    Migration,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for StoreError {}
impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<PackageError> for StoreError {
    fn from(error: PackageError) -> Self {
        Self::Package(error)
    }
}

#[derive(Clone, Debug)]
pub struct StoredPackage {
    pub identity: String,
    pub signed: SignedManifest,
    pub resources: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    package: String,
    revision: u64,
    highest_release: u64,
    highest_identity: Option<String>,
    current: Option<String>,
    previous: Option<String>,
    pending: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    queued: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessState {
    schema: u32,
    identity: String,
    namespace: String,
    version: u32,
    value: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessJournal {
    schema: u32,
    revision: u64,
    checkpoint: BusinessState,
    checksum: String,
}

#[derive(Debug)]
pub struct Activation {
    owner: u64,
    revision: u64,
    identity: String,
}

pub struct PackageStore {
    directory: PathBuf,
    _lock: File,
    state: State,
    owner: u64,
    recovered_pending: bool,
    recovered_business: bool,
    rejected_queued: bool,
}

fn identity(value: &str) -> bool {
    decode_hex::<32>(value).is_some()
}

fn unique() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn regular_read(path: &Path, limit: u64) -> Result<Vec<u8>, StoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(StoreError::Format);
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(StoreError::Format);
    }
    Ok(bytes)
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn sync_directory(directory: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    let _ = directory;
    Ok(())
}

pub(crate) fn replace_file(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
    let temporary = directory.join(format!(
        "checkpoint-{}-{}.tmp",
        std::process::id(),
        unique()
    ));
    let result = (|| {
        write_new(&temporary, bytes)?;
        fs::rename(&temporary, directory.join(name))?;
        sync_directory(directory)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

impl PackageStore {
    pub fn open(directory: impl AsRef<Path>, policy: &PackagePolicy) -> Result<Self, StoreError> {
        if policy.package.is_empty() || policy.package.len() > 128 {
            return Err(StoreError::Format);
        }
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;
        if !fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(StoreError::Format);
        }
        let lock_path = directory.join("writer.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|metadata| !metadata.file_type().is_file()) {
            return Err(StoreError::Format);
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| StoreError::Locked)?;
        let objects = directory.join("objects");
        fs::create_dir_all(&objects)?;
        if !fs::symlink_metadata(&objects)?.file_type().is_dir() {
            return Err(StoreError::Format);
        }
        let checkpoints = directory.join("checkpoints");
        fs::create_dir_all(&checkpoints)?;
        if !fs::symlink_metadata(&checkpoints)?.file_type().is_dir() {
            return Err(StoreError::Format);
        }
        let path = directory.join("activation.json");
        let state = if path.try_exists()? {
            serde_json::from_slice::<State>(&regular_read(&path, STATE_LIMIT)?)
                .map_err(|_| StoreError::Format)?
        } else {
            State {
                schema: 1,
                package: policy.package.clone(),
                revision: 0,
                highest_release: 0,
                highest_identity: None,
                current: None,
                previous: None,
                pending: None,
                queued: None,
            }
        };
        if state.schema != 1
            || state.package != policy.package
            || state
                .current
                .iter()
                .chain(&state.previous)
                .chain(&state.pending)
                .chain(&state.queued)
                .chain(&state.highest_identity)
                .any(|value| !identity(value))
            || (state.highest_release == 0) != state.highest_identity.is_none()
            || (state.highest_release == 0) != state.current.is_none()
            || (state.current.is_none() && state.previous.is_some())
        {
            return Err(StoreError::Format);
        }
        let recovered_pending = state.pending.is_some();
        let mut store = Self {
            directory,
            _lock: lock,
            state,
            owner: unique(),
            recovered_pending,
            recovered_business: false,
            rejected_queued: false,
        };
        if let Some(current) = &store.state.current {
            let current = store.read_package(current, policy)?;
            let release = current
                .signed
                .manifest
                .release
                .parse::<u64>()
                .map_err(|_| StoreError::Format)?;
            if release > store.state.highest_release
                || (release == store.state.highest_release
                    && store.state.highest_identity.as_ref() != Some(&current.identity))
            {
                return Err(StoreError::Format);
            }
        }
        if let Some(highest) = &store.state.highest_identity {
            let package = store.read_package(highest, policy)?;
            if package
                .signed
                .manifest
                .release
                .parse::<u64>()
                .map_err(|_| StoreError::Format)?
                != store.state.highest_release
            {
                return Err(StoreError::Format);
            }
        }
        store.recover_business(policy)?;
        if let Some(queued) = &store.state.queued {
            let valid = store.read_package(queued, policy).is_ok_and(|package| {
                package
                    .signed
                    .manifest
                    .release
                    .parse::<u64>()
                    .is_ok_and(|release| release > store.state.highest_release)
            });
            if !valid {
                let mut candidate = store.state.clone();
                candidate.queued = None;
                store.persist(candidate)?;
                store.rejected_queued = true;
            }
        }
        if recovered_pending {
            let mut candidate = store.state.clone();
            candidate.pending = None;
            store.persist(candidate)?;
        }
        Ok(store)
    }

    pub fn recovered_pending(&self) -> bool {
        self.recovered_pending
    }
    pub fn rejected_queued(&self) -> bool {
        self.rejected_queued
    }

    pub fn recovered_business(&self) -> bool {
        self.recovered_business
    }

    pub fn current(&self, policy: &PackagePolicy) -> Result<Option<StoredPackage>, StoreError> {
        self.state
            .current
            .as_deref()
            .map(|identity| self.read_package(identity, policy))
            .transpose()
    }

    pub fn highest_release(&self) -> u64 {
        self.state.highest_release
    }

    /// Downloaded/verified package, durable across a clean restart before trial.
    pub fn schedule(
        &mut self,
        signed: &SignedManifest,
        resources: &BTreeMap<String, Vec<u8>>,
        policy: &PackagePolicy,
    ) -> Result<(), StoreError> {
        if signed
            .manifest
            .release
            .parse::<u64>()
            .map_err(|_| StoreError::Format)?
            <= self.state.highest_release
        {
            return Err(StoreError::Package(PackageError::Rollback));
        }
        let activation = self.stage(signed, resources, policy)?;
        let mut candidate = self.state.clone();
        candidate.queued = Some(activation.identity);
        candidate.pending = None;
        self.persist(candidate)
    }
    pub fn queued(&self, policy: &PackagePolicy) -> Result<Option<StoredPackage>, StoreError> {
        self.state
            .queued
            .as_deref()
            .map(|identity| self.read_package(identity, policy))
            .transpose()
    }
    /// Persist the trial marker *before* executing candidate code. A process
    /// exit before acceptance is recovered by open(), retaining the good app.
    pub fn begin_trial(
        &mut self,
        policy: &PackagePolicy,
    ) -> Result<Option<Activation>, StoreError> {
        if self.state.pending.is_some() {
            return Err(StoreError::Pending);
        }
        let Some(identity) = self.state.queued.clone() else {
            return Ok(None);
        };
        let package = self.read_package(&identity, policy)?;
        if package
            .signed
            .manifest
            .release
            .parse::<u64>()
            .map_err(|_| StoreError::Format)?
            <= self.state.highest_release
        {
            return Err(StoreError::Package(PackageError::Rollback));
        }
        let mut candidate = self.state.clone();
        candidate.queued = None;
        candidate.pending = Some(identity.clone());
        self.persist(candidate)?;
        Ok(Some(Activation {
            owner: self.owner,
            revision: self.state.revision,
            identity,
        }))
    }

    pub fn previous(&self, policy: &PackagePolicy) -> Result<Option<StoredPackage>, StoreError> {
        self.state
            .previous
            .as_deref()
            .map(|identity| self.read_package(identity, policy))
            .transpose()
    }

    pub fn business(
        &self,
        policy: &PackagePolicy,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        self.current(policy)?
            .map(|package| self.read_business(&package))
            .transpose()
            .map(Option::flatten)
    }

    pub fn previous_business(
        &self,
        policy: &PackagePolicy,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        self.previous(policy)?
            .map(|package| self.read_business(&package))
            .transpose()
            .map(Option::flatten)
    }

    pub fn save_business(
        &mut self,
        policy: &PackagePolicy,
        value: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let current = self.current(policy)?.ok_or(StoreError::NoCurrent)?;
        self.write_business(&current, value, policy)
    }

    pub fn candidate_business(
        &self,
        activation: &Activation,
        policy: &PackagePolicy,
        migrate: impl FnOnce(u32, u32, &serde_json::Value) -> Result<serde_json::Value, StoreError>,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let candidate = self.candidate(activation, policy)?;
        let Some(current) = self.current(policy)? else {
            return Ok(None);
        };
        let Some(value) = self.read_business(&current)? else {
            return Ok(None);
        };
        let from = &current.signed.manifest;
        let to = &candidate.signed.manifest;
        if from.storage_namespace != to.storage_namespace {
            return Err(StoreError::Migration);
        }
        let value = if from.storage_version == to.storage_version {
            value
        } else {
            if !to.migration_from.contains(&from.storage_version) {
                return Err(StoreError::Migration);
            }
            migrate(from.storage_version, to.storage_version, &value)?
        };
        Self::business_bytes(&candidate, &value)?;
        Ok(Some(value))
    }

    pub fn accept_with_business(
        &mut self,
        activation: &Activation,
        policy: &PackagePolicy,
        value: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let candidate = self.candidate(activation, policy)?;
        self.write_business(&candidate, value, policy)?;
        self.accept(activation, policy)
    }

    pub fn stage(
        &mut self,
        signed: &SignedManifest,
        resources: &BTreeMap<String, Vec<u8>>,
        policy: &PackagePolicy,
    ) -> Result<Activation, StoreError> {
        if self.state.pending.is_some() || self.state.queued.is_some() {
            return Err(StoreError::Pending);
        }
        if self.state.package != policy.package {
            return Err(StoreError::Format);
        }
        signed.verify(policy, resources)?;
        let release = signed
            .manifest
            .release
            .parse::<u64>()
            .map_err(|_| StoreError::Format)?;
        if release < self.state.highest_release {
            return Err(StoreError::Package(PackageError::Rollback));
        }
        let bytes = serde_json::to_vec(signed).map_err(|_| StoreError::Format)?;
        if bytes.len() as u64 > MANIFEST_LIMIT {
            return Err(StoreError::Format);
        }
        let identity = sha256(&bytes);
        if release == self.state.highest_release
            && self.state.highest_identity.as_ref() != Some(&identity)
        {
            return Err(StoreError::ReleaseConflict);
        }
        self.store_package(&identity, &bytes, resources, policy)?;
        let mut candidate = self.state.clone();
        candidate.pending = Some(identity.clone());
        self.persist(candidate)?;
        Ok(Activation {
            owner: self.owner,
            revision: self.state.revision,
            identity,
        })
    }

    pub fn candidate(
        &self,
        activation: &Activation,
        policy: &PackagePolicy,
    ) -> Result<StoredPackage, StoreError> {
        self.validate(activation)?;
        self.read_package(&activation.identity, policy)
    }

    pub fn accept(
        &mut self,
        activation: &Activation,
        policy: &PackagePolicy,
    ) -> Result<(), StoreError> {
        let package = self.candidate(activation, policy)?;
        let release = package
            .signed
            .manifest
            .release
            .parse::<u64>()
            .map_err(|_| StoreError::Format)?;
        let mut candidate = self.state.clone();
        if candidate.current.as_ref() != Some(&activation.identity) {
            candidate.previous = candidate.current.take();
        }
        candidate.current = Some(activation.identity.clone());
        candidate.pending = None;
        candidate.highest_release = release;
        candidate.highest_identity = Some(activation.identity.clone());
        self.persist(candidate)
    }

    pub fn reject(&mut self, activation: &Activation) -> Result<(), StoreError> {
        self.validate(activation)?;
        let mut candidate = self.state.clone();
        candidate.pending = None;
        self.persist(candidate)
    }

    pub fn rollback(&mut self, policy: &PackagePolicy) -> Result<StoredPackage, StoreError> {
        if self.state.pending.is_some() {
            return Err(StoreError::Pending);
        }
        let previous = self.state.previous.as_ref().ok_or(StoreError::NoPrevious)?;
        let package = self.read_package(previous, policy)?;
        let mut candidate = self.state.clone();
        std::mem::swap(&mut candidate.current, &mut candidate.previous);
        self.persist(candidate)?;
        Ok(package)
    }

    pub fn collect_unused(&mut self) -> Result<usize, StoreError> {
        if self.directory.join(BUSINESS_JOURNAL).try_exists()? {
            return Err(StoreError::Pending);
        }
        let retained: std::collections::BTreeSet<_> = self
            .state
            .current
            .iter()
            .chain(&self.state.previous)
            .chain(&self.state.pending)
            .chain(&self.state.queued)
            .chain(&self.state.highest_identity)
            .cloned()
            .collect();
        let objects = self.directory.join("objects").canonicalize()?;
        let mut removed = 0;
        for entry in fs::read_dir(&objects)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !identity(name) || retained.contains(name) || !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().canonicalize()?;
            if path.parent() != Some(objects.as_path()) {
                return Err(StoreError::Format);
            }
            fs::remove_dir_all(path)?;
            removed += 1;
        }
        sync_directory(&objects)?;
        let checkpoints = self.directory.join("checkpoints");
        for entry in fs::read_dir(&checkpoints)? {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
                && identity(name)
                && !retained.contains(name)
                && entry.file_type()?.is_file()
            {
                fs::remove_file(path)?;
            }
        }
        sync_directory(&checkpoints)?;
        Ok(removed)
    }

    fn read_business(
        &self,
        package: &StoredPackage,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let path = self
            .directory
            .join("checkpoints")
            .join(format!("{}.json", package.identity));
        if !path.try_exists()? {
            return Ok(None);
        }
        let state: BusinessState = serde_json::from_slice(&regular_read(&path, BUSINESS_LIMIT)?)
            .map_err(|_| StoreError::Format)?;
        Self::validate_business(&state, package)?;
        Ok(Some(state.value))
    }

    fn validate_business(state: &BusinessState, package: &StoredPackage) -> Result<(), StoreError> {
        if state.schema != 1
            || state.identity != package.identity
            || state.namespace != package.signed.manifest.storage_namespace
            || state.version != package.signed.manifest.storage_version
        {
            return Err(StoreError::Format);
        }
        Ok(())
    }

    fn business_bytes(
        package: &StoredPackage,
        value: &serde_json::Value,
    ) -> Result<Vec<u8>, StoreError> {
        let bytes = serde_json::to_vec(&BusinessState {
            schema: 1,
            identity: package.identity.clone(),
            namespace: package.signed.manifest.storage_namespace.clone(),
            version: package.signed.manifest.storage_version,
            value: value.clone(),
        })
        .map_err(|_| StoreError::Format)?;
        if bytes.len() as u64 > BUSINESS_LIMIT {
            return Err(StoreError::Limit);
        }
        Ok(bytes)
    }

    fn write_business(
        &mut self,
        package: &StoredPackage,
        value: &serde_json::Value,
        policy: &PackagePolicy,
    ) -> Result<(), StoreError> {
        let bytes = Self::business_bytes(package, value)?;
        self.recover_business(policy)?;
        let checkpoint = serde_json::from_slice(&bytes).map_err(|_| StoreError::Format)?;
        let mut journal = BusinessJournal {
            schema: 1,
            revision: self.state.revision,
            checkpoint,
            checksum: String::new(),
        };
        journal.checksum = Self::journal_checksum(&journal)?;
        let encoded = serde_json::to_vec(&journal).map_err(|_| StoreError::Format)?;
        if encoded.len() as u64 > JOURNAL_LIMIT {
            return Err(StoreError::Limit);
        }
        replace_file(&self.directory, BUSINESS_JOURNAL, &encoded)?;
        self.finish_business(&journal, &bytes)
    }

    fn journal_checksum(journal: &BusinessJournal) -> Result<String, StoreError> {
        let bytes = serde_json::to_vec(&(journal.schema, journal.revision, &journal.checkpoint))
            .map_err(|_| StoreError::Format)?;
        Ok(sha256(&bytes))
    }

    fn finish_business(&self, journal: &BusinessJournal, bytes: &[u8]) -> Result<(), StoreError> {
        let checkpoints = self.directory.join("checkpoints");
        if !fs::symlink_metadata(&checkpoints)?.file_type().is_dir() {
            return Err(StoreError::Format);
        }
        let name = format!("{}.json", journal.checkpoint.identity);
        let target = checkpoints.join(&name);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if !metadata.file_type().is_file() => return Err(StoreError::Format),
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        replace_file(&checkpoints, &name, bytes)?;
        fs::remove_file(self.directory.join(BUSINESS_JOURNAL))?;
        sync_directory(&self.directory)
    }

    fn recover_business(&mut self, policy: &PackagePolicy) -> Result<(), StoreError> {
        let path = self.directory.join(BUSINESS_JOURNAL);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let journal: BusinessJournal = serde_json::from_slice(&regular_read(&path, JOURNAL_LIMIT)?)
            .map_err(|_| StoreError::Format)?;
        if journal.schema != 1
            || journal.revision != self.state.revision
            || journal.checksum != Self::journal_checksum(&journal)?
            || !identity(&journal.checkpoint.identity)
            || !self
                .state
                .current
                .iter()
                .chain(&self.state.pending)
                .any(|identity| identity == &journal.checkpoint.identity)
        {
            return Err(StoreError::Format);
        }
        let package = self.read_package(&journal.checkpoint.identity, policy)?;
        Self::validate_business(&journal.checkpoint, &package)?;
        let bytes = Self::business_bytes(&package, &journal.checkpoint.value)?;
        self.finish_business(&journal, &bytes)?;
        self.recovered_business = true;
        Ok(())
    }

    fn validate(&self, activation: &Activation) -> Result<(), StoreError> {
        if activation.owner != self.owner
            || activation.revision != self.state.revision
            || self.state.pending.as_ref() != Some(&activation.identity)
        {
            return Err(StoreError::StaleActivation);
        }
        Ok(())
    }

    fn read_package(
        &self,
        identity: &str,
        policy: &PackagePolicy,
    ) -> Result<StoredPackage, StoreError> {
        if !identity.is_ascii() || decode_hex::<32>(identity).is_none() {
            return Err(StoreError::Format);
        }
        let path = self.directory.join("objects").join(identity);
        if !fs::symlink_metadata(&path)?.file_type().is_dir() {
            return Err(StoreError::Format);
        }
        let bytes = regular_read(&path.join("manifest.json"), MANIFEST_LIMIT)?;
        if sha256(&bytes) != identity {
            return Err(StoreError::Package(PackageError::Integrity));
        }
        let signed = SignedManifest::decode(&bytes)?;
        let mut resources = BTreeMap::new();
        let mut total = 0usize;
        for (name, resource) in &signed.manifest.resources {
            let bytes = regular_read(
                &path.join(format!("{}.bin", resource.sha256)),
                RESOURCE_LIMIT,
            )?;
            total = total.checked_add(bytes.len()).ok_or(StoreError::Format)?;
            if total > 16 * 1024 * 1024 {
                return Err(StoreError::Format);
            }
            resources.insert(name.clone(), bytes);
        }
        signed.verify(policy, &resources)?;
        Ok(StoredPackage {
            identity: identity.into(),
            signed,
            resources,
        })
    }

    fn store_package(
        &self,
        identity: &str,
        manifest: &[u8],
        resources: &BTreeMap<String, Vec<u8>>,
        policy: &PackagePolicy,
    ) -> Result<(), StoreError> {
        let objects = self.directory.join("objects");
        let target = objects.join(identity);
        if target.try_exists()? {
            self.read_package(identity, policy)?;
            return Ok(());
        }
        if fs::read_dir(&objects)?.take(8).count() >= 8 {
            return Err(StoreError::Limit);
        }
        let temporary = objects.join(format!("pending-{}-{}", std::process::id(), unique()));
        fs::create_dir(&temporary)?;
        let result = (|| {
            write_new(&temporary.join("manifest.json"), manifest)?;
            let mut written = std::collections::BTreeSet::new();
            for bytes in resources.values() {
                let digest = sha256(bytes);
                if written.insert(digest.clone()) {
                    write_new(&temporary.join(format!("{digest}.bin")), bytes)?;
                }
            }
            sync_directory(&temporary)?;
            fs::rename(&temporary, target)?;
            sync_directory(&objects)
        })();
        if result.is_err() && temporary.try_exists().unwrap_or(false) {
            let resolved = temporary.canonicalize()?;
            if resolved.parent() == Some(objects.canonicalize()?.as_path()) {
                let _ = fs::remove_dir_all(resolved);
            }
        }
        result
    }

    fn persist(&mut self, mut state: State) -> Result<(), StoreError> {
        if self.directory.join(BUSINESS_JOURNAL).try_exists()? {
            return Err(StoreError::Pending);
        }
        state.revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or(StoreError::Format)?;
        let bytes = serde_json::to_vec(&state).map_err(|_| StoreError::Format)?;
        if bytes.len() as u64 > STATE_LIMIT {
            return Err(StoreError::Format);
        }
        replace_file(&self.directory, "activation.json", &bytes)?;
        self.state = state;
        Ok(())
    }
}
