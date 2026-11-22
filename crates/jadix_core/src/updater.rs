//! Application updates, independent of UI, transport and installation adapters.
//! Trust comes from packaged configuration; callers can select only named channels.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::{BTreeMap, VecDeque}, rc::Rc};
use url::Url;

#[cfg(feature = "updater-install")]
pub mod archive;
#[cfg(all(feature = "updater-install", any(unix, windows)))]
pub mod installation;

pub const MANIFEST_LIMIT: usize = 1024 * 1024;
pub const NOTES_LIMIT: usize = 256 * 1024;
pub const DOWNLOAD_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
pub const SIGNATURE_DOMAIN: &[u8] = b"jadix-updater-manifest\0\x01";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub id: String,
    /// The installed application version, never the current script package version.
    pub version: String,
    #[serde(default)] pub github: Option<String>,
    #[serde(default)] pub url: Option<String>,
    #[serde(default)] pub channels: BTreeMap<String, String>,
    #[serde(default)] pub public_key: Option<String>,
}
fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn https(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "invalid update URL")?;
    if value.len() > 8192 || url.scheme() != "https" || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("updates require an HTTPS URL without credentials or fragment".into());
    }
    Ok(url)
}
fn digest_valid(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) }
fn public_key(value: &str) -> Result<VerifyingKey, String> {
    let bytes: [u8; 32] = B64.decode(value).map_err(|_| "invalid update public key")?.try_into().map_err(|_| "invalid update public key length")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid update public key".into())
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !identifier(&self.id) || semver::Version::parse(&self.version).is_err() { return Err("invalid installed update identity or version".into()); }
        if self.github.is_some() == self.url.is_some() { return Err("configure exactly one update source".into()); }
        if let Some(repo) = &self.github {
            let parts: Vec<_> = repo.split('/').collect();
            if parts.len() != 2 || parts.iter().any(|p| !identifier(p) || *p == "." || *p == "..") { return Err("invalid GitHub update repository".into()); }
        }
        if let Some(url) = &self.url { https(url)?; }
        if self.channels.len() > 32 || self.channels.iter().any(|(name, url)| !identifier(name) || https(url).is_err()) { return Err("invalid update channel".into()); }
        if (self.url.is_some() || !self.channels.is_empty()) && self.public_key.is_none() { return Err("custom update sources require a signing key".into()); }
        if let Some(key) = &self.public_key { public_key(key)?; }
        Ok(())
    }
    pub fn manifest_url(&self, channel: Option<&str>) -> Result<Url, String> {
        self.validate()?;
        if let Some(channel) = channel { return https(self.channels.get(channel).ok_or("unknown update channel")?); }
        match &self.url {
            Some(url) => https(url),
            None => https(&format!("https://github.com/{}/releases/latest/download/latest.json", self.github.as_deref().unwrap())),
        }
    }
    pub fn check(&self, manifest_url: &Url, bytes: &[u8], signature: Option<&str>, platform: &str, can_install: bool) -> Result<Option<Available>, String> {
        self.validate()?;
        if bytes.len() > MANIFEST_LIMIT { return Err("update manifest exceeds limit".into()); }
        if let Some(key) = &self.public_key {
            let bytes_signature = B64.decode(signature.ok_or("missing update signature")?.trim()).map_err(|_| "invalid update signature")?;
            let signature = Signature::from_slice(&bytes_signature).map_err(|_| "invalid update signature length")?;
            let mut signed = Vec::with_capacity(SIGNATURE_DOMAIN.len() + bytes.len());
            signed.extend_from_slice(SIGNATURE_DOMAIN); signed.extend_from_slice(bytes);
            public_key(key)?.verify_strict(&signed, &signature).map_err(|_| "update signature verification failed")?;
        }
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| "invalid update manifest")?;
        let version = semver::Version::parse(&manifest.version).map_err(|_| "invalid update version")?;
        if !identifier(&manifest.id) || manifest.notes.as_ref().is_some_and(|n| n.len() > NOTES_LIMIT) || manifest.localized_notes.len() > 128 || manifest.platforms.len() > 64 { return Err("update manifest field limit exceeded".into()); }
        if version.cmp_precedence(&semver::Version::parse(&self.version).map_err(|_| "invalid installed version")?) != std::cmp::Ordering::Greater { return Ok(None); }
        let Some(asset) = manifest.platforms.get(platform) else { return Ok(None); };
        let mobile = matches!(platform, "ios" | "android");
        if !mobile && (!asset.sha256.as_deref().is_some_and(digest_valid) || asset.size.is_some_and(|s| s == 0 || s > DOWNLOAD_LIMIT)) { return Err("update archive requires a valid SHA-256 and bounded size".into()); }
        let resolve = |relative: &str| -> Result<String, String> {
            let resolved = manifest_url.join(relative).map_err(|_| "invalid update asset URL")?;
            Ok(https(resolved.as_str())?.into())
        };
        let mut notes = BTreeMap::new();
        for (locale, file) in manifest.localized_notes {
            if locale.is_empty() || locale.len() > 64 || !digest_valid(&file.sha256) { return Err("invalid localized update notes".into()); }
            notes.insert(locale, NotesFile { url: resolve(&file.url)?, sha256: file.sha256 });
        }
        Ok(Some(Available { id: manifest.id, version: manifest.version, notes: manifest.notes, localized_notes: notes,
            url: resolve(&asset.url)?, sha256: asset.sha256.clone(), size: asset.size, can_install: can_install && !mobile }))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default)] pub notes: Option<String>,
    #[serde(default)] pub localized_notes: BTreeMap<String, NotesFile>,
    pub platforms: BTreeMap<String, Asset>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asset { pub url: String, #[serde(default)] pub sha256: Option<String>, #[serde(default)] pub size: Option<u64> }
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NotesFile { pub url: String, pub sha256: String }

/// Validate every target at release time with the same rules used by clients.
pub fn validate_release(manifest_url: &str, bytes: &[u8]) -> Result<Manifest, String> {
    let url = https(manifest_url)?;
    if bytes.len() > MANIFEST_LIMIT { return Err("update manifest exceeds limit".into()); }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| "invalid update manifest")?;
    let config = Config { id: manifest.id.clone(), version: "0.0.0-0".into(),
        github: Some("jadix/release".into()), url: None, channels: BTreeMap::new(), public_key: None };
    if manifest.platforms.is_empty() { return Err("update release needs a platform".into()); }
    for platform in manifest.platforms.keys() {
        if !identifier(platform) || config.check(&url, bytes, None, platform, true)?.is_none() {
            return Err("invalid update release platform or version".into());
        }
    }
    Ok(manifest)
}

/// Detached signature over the exact published manifest bytes, with domain separation.
pub fn sign_release(seed: &[u8; 32], manifest_url: &str, bytes: &[u8]) -> Result<(String, String), String> {
    use ed25519_dalek::Signer;
    validate_release(manifest_url, bytes)?;
    let key = ed25519_dalek::SigningKey::from_bytes(seed);
    let mut message = Vec::with_capacity(SIGNATURE_DOMAIN.len() + bytes.len());
    message.extend_from_slice(SIGNATURE_DOMAIN); message.extend_from_slice(bytes);
    Ok((B64.encode(key.verifying_key().as_bytes()), B64.encode(key.sign(&message).to_bytes())))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub id: String, pub version: String, pub notes: Option<String>, pub localized_notes: BTreeMap<String, NotesFile>,
    pub url: String, pub sha256: Option<String>, pub size: Option<u64>, pub can_install: bool,
}
pub fn notes_for<'a>(notes: &'a BTreeMap<String, NotesFile>, locale: &str) -> Option<&'a NotesFile> {
    let normalized = locale.replace('_', "-");
    notes.iter().find(|(key, _)| key.replace('_', "-").eq_ignore_ascii_case(&normalized)).map(|(_, file)| file)
        .or_else(|| notes.iter().find(|(key, _)| key.eq_ignore_ascii_case(normalized.split('-').next().unwrap_or_default())).map(|(_, file)| file))
}
pub fn open_notes(file: &NotesFile, bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > NOTES_LIMIT || !sha256(&bytes).eq_ignore_ascii_case(&file.sha256) { return Err("update notes checksum mismatch".into()); }
    String::from_utf8(bytes).map_err(|_| "update notes are not UTF-8".into())
}
pub fn sha256(bytes: &[u8]) -> String { Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect() }

pub struct Download { hash: Sha256, received: u64, digest: String, size: Option<u64>, failed: bool }
impl Download {
    pub fn new(digest: &str, size: Option<u64>) -> Result<Self, String> {
        if !digest_valid(digest) || size.is_some_and(|s| s == 0 || s > DOWNLOAD_LIMIT) { return Err("invalid update download bounds".into()); }
        Ok(Self { hash: Sha256::new(), received: 0, digest: digest.to_ascii_lowercase(), size, failed: false })
    }
    pub fn update(&mut self, chunk: &[u8]) -> Result<u64, String> {
        let next = self.received.saturating_add(chunk.len() as u64);
        if self.failed || next > self.size.unwrap_or(DOWNLOAD_LIMIT) { self.failed = true; return Err("update download exceeds declared size or limit".into()); }
        self.hash.update(chunk); self.received = next; Ok(next)
    }
    pub fn finish(self) -> Result<(), String> {
        let digest: String = self.hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if self.failed || self.received == 0 || self.size.is_some_and(|s| s != self.received) || digest != self.digest { return Err("update download checksum or size mismatch".into()); }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase { Disabled, Idle, Checking, Notes, Available, Downloading, Staging, Ready, Installing, Scheduled, Failed }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot { pub generation: u32, pub phase: Phase, pub available: Option<Available>, pub received: u64, pub error: Option<String> }
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Command {
    Manifest { id: u32, url: String, signature_url: Option<String> },
    Notes { id: u32, file: NotesFile },
    Download { id: u32, url: String, max_bytes: u64 },
    Stage { id: u32, application_id: String, version: String },
    Install { id: u32, restart: bool },
    Cancel { id: u32 },
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Manifest { bytes: Vec<u8>, signature: Option<String> }, Notes { bytes: Vec<u8> },
    Downloaded, Staged, Scheduled, Failed { message: String },
}
struct State {
    config: Option<Config>, platform: String, can_install: bool, snapshot: Snapshot,
    commands: VecDeque<Command>, manifest_url: Option<Url>, locale: Option<String>, notes: Option<NotesFile>, download: Option<Download>,
    wake: Option<Rc<dyn Fn()>>,
}
/// One shared service per application. Platform adapters consume commands and
/// return generation-tagged events; late/cancelled transport results are ignored.
#[derive(Clone)]
pub struct Updater(Rc<RefCell<State>>);
impl Default for Updater { fn default() -> Self { Self::new(None, "", false).expect("disabled updater") } }
impl Updater {
    pub fn new(config: Option<Config>, platform: impl Into<String>, can_install: bool) -> Result<Self, String> {
        if let Some(config) = &config { config.validate()?; }
        let phase = if config.is_some() { Phase::Idle } else { Phase::Disabled };
        Ok(Self(Rc::new(RefCell::new(State { config, platform: platform.into(), can_install, snapshot: Snapshot { generation: 0, phase, available: None, received: 0, error: None }, commands: VecDeque::new(), manifest_url: None, locale: None, notes: None, download: None, wake: None }))))
    }
    pub fn set_command_waker(&self, wake: Option<Rc<dyn Fn()>>) { self.0.borrow_mut().wake = wake; }
    fn wake(&self) { let wake = self.0.borrow().wake.clone(); if let Some(wake) = wake { wake(); } }
    pub fn snapshot(&self) -> Snapshot { self.0.borrow().snapshot.clone() }
    pub fn check(&self, channel: Option<&str>, locale: Option<&str>) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        let Some(config) = &s.config else { return Ok(()); };
        if locale.is_some_and(|l| l.len() > 64) { return Err("update locale exceeds limit".into()); }
        if matches!(s.snapshot.phase, Phase::Downloading | Phase::Staging | Phase::Ready | Phase::Installing | Phase::Scheduled) { return Err("finish or cancel the pending update first".into()); }
        if s.commands.len() > 16 { return Err("update command queue is full".into()); }
        let url = config.manifest_url(channel)?;
        let signature_url = config.public_key.as_ref().map(|_| {
            let mut signature = url.clone(); signature.set_path(&format!("{}.sig", url.path())); signature.to_string()
        });
        let old = s.snapshot.generation;
        let id = old.checked_add(1).ok_or("update generation exhausted")?;
        s.commands.push_back(Command::Cancel { id: old });
        s.commands.push_back(Command::Manifest { id, url: url.to_string(), signature_url });
        s.manifest_url = Some(url); s.locale = locale.map(str::to_owned); s.notes = None;
        s.snapshot = Snapshot { generation: id, phase: Phase::Checking, available: None, received: 0, error: None }; drop(s); self.wake(); Ok(())
    }
    pub fn download(&self) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        if !matches!(s.snapshot.phase, Phase::Available | Phase::Failed) { return Err("no available application update".into()); }
        let asset = s.snapshot.available.as_ref().filter(|a| a.can_install).ok_or("application update cannot be installed on this host")?;
        let download = Download::new(asset.sha256.as_deref().ok_or("update archive has no checksum")?, asset.size)?;
        let old = s.snapshot.generation;
        let id = old.checked_add(1).ok_or("update generation exhausted")?;
        let command = Command::Download { id, url: asset.url.clone(), max_bytes: asset.size.unwrap_or(DOWNLOAD_LIMIT) };
        s.commands.push_back(Command::Cancel { id: old }); s.snapshot.generation = id;
        s.download = Some(download); s.snapshot.received = 0; s.snapshot.error = None; s.snapshot.phase = Phase::Downloading;
        s.commands.push_back(command); drop(s); self.wake(); Ok(())
    }
    pub fn install(&self, restart: bool) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        if s.snapshot.phase != Phase::Ready && !(restart && s.snapshot.phase == Phase::Scheduled) { return Err("application update has not been staged and validated".into()); }
        let id = s.snapshot.generation; s.snapshot.phase = Phase::Installing;
        s.commands.push_back(Command::Install { id, restart }); drop(s); self.wake(); Ok(())
    }
    pub fn cancel(&self) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        if s.snapshot.phase == Phase::Disabled { return Ok(()); }
        let id = s.snapshot.generation;
        let next = id.checked_add(1).ok_or("update generation exhausted")?;
        s.commands.clear(); s.commands.push_back(Command::Cancel { id }); s.download = None; s.notes = None;
        s.snapshot = Snapshot { generation: next, phase: Phase::Idle, available: None, received: 0, error: None }; drop(s); self.wake(); Ok(())
    }
    pub fn take_commands(&self) -> Vec<Command> { self.0.borrow_mut().commands.drain(..).collect() }
    pub fn chunk(&self, id: u32, bytes: &[u8]) -> bool {
        let mut s = self.0.borrow_mut();
        if id != s.snapshot.generation || s.snapshot.phase != Phase::Downloading || bytes.len() > 64 * 1024 { return false; }
        match s.download.as_mut().expect("downloading verifier").update(bytes) {
            Ok(received) => { s.snapshot.received = received; true }
            Err(error) => { fail(&mut s, error); false }
        }
    }
    pub fn event(&self, id: u32, event: Event) -> bool {
        let mut s = self.0.borrow_mut();
        if id != s.snapshot.generation { return false; }
        let result: Result<(), String> = (|| {
            match event {
                Event::Manifest { bytes, signature } if s.snapshot.phase == Phase::Checking => {
                    let available = s.config.as_ref().unwrap().check(s.manifest_url.as_ref().unwrap(), &bytes, signature.as_deref(), &s.platform, s.can_install)?;
                    let notes = available.as_ref().and_then(|a| s.locale.as_ref().and_then(|l| notes_for(&a.localized_notes, l))).cloned();
                    s.snapshot.phase = if available.is_some() { Phase::Available } else { Phase::Idle }; s.snapshot.available = available;
                    if let Some(file) = notes { s.snapshot.phase = Phase::Notes; s.notes = Some(file.clone()); s.commands.push_back(Command::Notes { id, file }); }
                }
                Event::Notes { bytes } if s.snapshot.phase == Phase::Notes => {
                    if let Ok(notes) = open_notes(s.notes.as_ref().unwrap(), bytes) { s.snapshot.available.as_mut().unwrap().notes = Some(notes); }
                    s.notes = None; s.snapshot.phase = Phase::Available;
                }
                Event::Downloaded if s.snapshot.phase == Phase::Downloading => {
                    s.download.take().unwrap().finish()?;
                    let application_id = s.snapshot.available.as_ref().unwrap().id.clone();
                    let version = s.snapshot.available.as_ref().unwrap().version.clone();
                    s.snapshot.phase = Phase::Staging; s.commands.push_back(Command::Stage { id, application_id, version });
                }
                Event::Staged if s.snapshot.phase == Phase::Staging => { s.snapshot.phase = Phase::Ready; }
                Event::Scheduled if s.snapshot.phase == Phase::Installing => { s.snapshot.phase = Phase::Scheduled; }
                Event::Failed { message } if s.snapshot.phase == Phase::Notes => { let _ = message; s.notes = None; s.snapshot.phase = Phase::Available; }
                Event::Failed { message } if matches!(s.snapshot.phase, Phase::Checking | Phase::Downloading | Phase::Staging | Phase::Installing) => { return Err(message.chars().take(1024).collect()); }
                _ => return Ok(()),
            }
            Ok(())
        })();
        if let Err(error) = result { fail(&mut s, error); }
        true
    }
}
fn fail(s: &mut State, error: String) {
    s.snapshot.phase = Phase::Failed; s.snapshot.error = Some(error); s.download = None;
    s.commands.push_back(Command::Cancel { id: s.snapshot.generation });
}
