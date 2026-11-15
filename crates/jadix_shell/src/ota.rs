//! Signed updates over the existing application-owned jadix_net transport seam.
//! No implicit HTTP runtime, TLS stack, worker or VM is created here.
use crate::{
    package::{sha256, PackageError, PackagePolicy, SignedManifest},
    store::{Activation, PackageStore, StoreError, StoredPackage},
};
use jadix_net::{CancellationToken, HttpError, HttpRequest, HttpResponse};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[derive(Debug)]
pub enum UpdateError {
    Store(StoreError),
    Package(PackageError),
    Transport(HttpError),
    Policy,
    Cancelled,
    NotHealthy,
    #[cfg(feature = "native-vm")]
    Application(crate::application::ApplicationError),
}

/// Launches the queued signed bundle in the existing VM. A load fault rejects
/// the trial and boots the last good bundle in the same process.
#[cfg(feature = "native-vm")]
pub struct Launch {
    pub application: crate::application::ScriptApplication,
    trial: Option<Trial>,
    pub fallback_reason: Option<String>,
}
#[cfg(feature = "native-vm")]
impl Launch {
    pub fn start(
        store: &mut PackageStore,
        policy: &PackagePolicy,
        registry: impl Fn() -> crate::wire::ComponentRegistry,
        limits: crate::runtime::ResourceLimits,
    ) -> Result<Self, UpdateError> {
        let trial = Trial::begin(store, policy)?;
        let mut fallback_reason = None;
        if let Some(trial) = trial {
            let package = trial.package(store, policy)?;
            match crate::application::ScriptApplication::from_package(
                &package,
                policy,
                registry(),
                limits,
            ) {
                Ok(application) => {
                    return Ok(Self {
                        application,
                        trial: Some(trial),
                        fallback_reason: None,
                    })
                }
                Err(error) => {
                    trial.reject(store)?;
                    fallback_reason = Some(error.to_string());
                }
            }
        }
        let package = store
            .current(policy)
            .map_err(UpdateError::Store)?
            .ok_or(UpdateError::Store(StoreError::NoCurrent))?;
        let application = crate::application::ScriptApplication::from_package(
            &package,
            policy,
            registry(),
            limits,
        )
        .map_err(UpdateError::Application)?;
        Ok(Self {
            application,
            trial: None,
            fallback_reason,
        })
    }
    /// Call only after a successfully presented frame, not after projection.
    pub fn presented(&mut self) {
        if let Some(trial) = &mut self.trial {
            trial.presented();
        }
    }
    pub fn poll_health(
        &mut self,
        store: &mut PackageStore,
        policy: &PackagePolicy,
    ) -> Result<bool, UpdateError> {
        if let Some(trial) = &self.trial {
            if trial.healthy() {
                trial.accept(store, policy, false)?;
                self.trial = None;
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn clean_quit(
        &mut self,
        store: &mut PackageStore,
        policy: &PackagePolicy,
    ) -> Result<(), UpdateError> {
        if let Some(trial) = &self.trial {
            if trial.first_frame.is_some() {
                trial.accept(store, policy, true)?;
            } else {
                trial.reject(store)?;
            }
            self.trial = None;
        }
        self.application.dispose();
        Ok(())
    }
    pub fn failed(&mut self, store: &mut PackageStore) -> Result<(), UpdateError> {
        self.application.dispose();
        if let Some(trial) = self.trial.take() {
            trial.reject(store)?;
        }
        Ok(())
    }
}
impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for UpdateError {}
/// HTTPS resource base. Loopback HTTP must be explicitly opted into by a
/// local test host; redirects are rejected (including HTTPS->HTTP downgrade).
pub struct DownloadSource {
    base: String,
    timeout: Duration,
}
/// Fetch the signed manifest and resources using the bounded native TLS adapter.
/// Invoke on a host worker and then hand the queued store back to its owner.
#[cfg(all(feature = "native-https", not(target_arch = "wasm32")))]
pub fn download_https_release(
    store: &mut PackageStore,
    manifest_url: &str,
    source: &DownloadSource,
    policy: &PackagePolicy,
    cancel: &CancellationToken,
    client: &jadix_net::native_https::HttpsClient,
) -> Result<(), UpdateError> {
    let manifest = client
        .get(
            HttpRequest::get(manifest_url)
                .timeout(source.timeout)
                .max_response_bytes(256 * 1024),
            cancel,
        )
        .map_err(UpdateError::Transport)?;
    if manifest.status != 200 {
        return Err(UpdateError::Policy);
    }
    download_and_schedule(
        store,
        &manifest.body,
        source,
        policy,
        cancel,
        |request, token| client.get(request, token),
    )
}
/// Fetch an authenticated channel, then derive immutable resource URLs from its
/// signed numeric release. Unsigned input never selects a host or filesystem path.
#[cfg(all(feature = "native-https", not(target_arch = "wasm32")))]
pub fn download_https_channel(
    store: &mut PackageStore,
    source: &DownloadSource,
    policy: &PackagePolicy,
    cancel: &CancellationToken,
    client: &jadix_net::native_https::HttpsClient,
) -> Result<(), UpdateError> {
    let response = client
        .get(
            HttpRequest::get(format!("{}channel.json", source.base))
                .timeout(source.timeout)
                .max_response_bytes(256 * 1024),
            cancel,
        )
        .map_err(UpdateError::Transport)?;
    if response.status != 200 {
        return Err(UpdateError::Policy);
    }
    let signed = SignedManifest::decode(&response.body).map_err(UpdateError::Package)?;
    signed
        .verify_manifest(policy)
        .map_err(UpdateError::Package)?;
    let release_source = DownloadSource {
        base: format!("{}releases/{}/", source.base, signed.manifest.release),
        timeout: source.timeout,
    };
    download_and_schedule(
        store,
        &response.body,
        &release_source,
        policy,
        cancel,
        |r, t| client.get(r, t),
    )
}
impl DownloadSource {
    pub fn https(base: &str) -> Result<Self, UpdateError> {
        Self::new(base, false)
    }
    pub fn local_fixture(base: &str) -> Result<Self, UpdateError> {
        Self::new(base, true)
    }
    fn new(base: &str, local: bool) -> Result<Self, UpdateError> {
        let https = base.strip_prefix("https://");
        let http = local.then(|| base.strip_prefix("http://")).flatten();
        let rest = https.or(http).ok_or(UpdateError::Policy)?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if base.len() > 2048
            || !base.is_ascii()
            || authority.is_empty()
            || authority
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b".-:[]".contains(&b))
            || authority.contains(['@', '\\', '?', '#'])
            || base.bytes().any(|b| b <= 32 || b == 127)
            || path.contains(['?', '#', '\\', '%'])
            || path.split('/').any(|s| s == ".." || s == ".")
        {
            return Err(UpdateError::Policy);
        }
        if http.is_some()
            && !authority
                .strip_prefix("127.0.0.1:")
                .is_some_and(|port| port.parse::<u16>().is_ok_and(|p| p != 0))
        {
            return Err(UpdateError::Policy);
        }
        HttpRequest::get(base)
            .validate()
            .map_err(UpdateError::Transport)?;
        Ok(Self {
            base: format!("{}/", base.trim_end_matches('/')),
            timeout: Duration::from_secs(30),
        })
    }
}
/// fetch must honor full-body deadline/size/cancellation, use verified TLS and
/// reject redirects. This seam checks the returned URL/body/deadline but cannot
/// attest a host adapter's TLS verification or pre-allocation bounds.
pub fn download_and_schedule(
    store: &mut PackageStore,
    manifest: &[u8],
    source: &DownloadSource,
    policy: &PackagePolicy,
    cancel: &CancellationToken,
    mut fetch: impl FnMut(HttpRequest, &CancellationToken) -> Result<HttpResponse, HttpError>,
) -> Result<(), UpdateError> {
    let signed = SignedManifest::decode(manifest).map_err(UpdateError::Package)?;
    signed
        .verify_manifest(policy)
        .map_err(UpdateError::Package)?;
    if signed
        .manifest
        .release
        .parse::<u64>()
        .map_err(|_| UpdateError::Policy)?
        <= store.highest_release()
    {
        return Err(UpdateError::Package(PackageError::Rollback));
    }
    let mut resources = BTreeMap::new();
    for (path, resource) in &signed.manifest.resources {
        if cancel.is_cancelled() {
            return Err(UpdateError::Cancelled);
        }
        let request = HttpRequest::get(format!("{}{path}", source.base))
            .timeout(source.timeout)
            .max_response_bytes(resource.bytes as usize);
        let url = request.url.clone();
        let started = Instant::now();
        let response = fetch(request, cancel).map_err(UpdateError::Transport)?;
        if cancel.is_cancelled() {
            return Err(UpdateError::Cancelled);
        }
        if started.elapsed() > source.timeout || response.status != 200 || response.url != url {
            return Err(UpdateError::Policy);
        }
        if response.body.len() as u64 != resource.bytes || sha256(&response.body) != resource.sha256
        {
            return Err(UpdateError::Package(PackageError::Integrity));
        }
        resources.insert(path.clone(), response.body.to_vec());
    }
    if cancel.is_cancelled() {
        return Err(UpdateError::Cancelled);
    }
    store
        .schedule(&signed, &resources, policy)
        .map_err(UpdateError::Store)
}
/// Candidate remains pending until a host reports its first successful frame
/// and ten healthy seconds, or a clean quit after that frame.
pub struct Trial {
    activation: Activation,
    first_frame: Option<Instant>,
}
impl Trial {
    pub fn begin(
        store: &mut PackageStore,
        policy: &PackagePolicy,
    ) -> Result<Option<Self>, UpdateError> {
        store
            .begin_trial(policy)
            .map(|a| {
                a.map(|activation| Self {
                    activation,
                    first_frame: None,
                })
            })
            .map_err(UpdateError::Store)
    }
    pub fn package(
        &self,
        store: &PackageStore,
        policy: &PackagePolicy,
    ) -> Result<StoredPackage, UpdateError> {
        store
            .candidate(&self.activation, policy)
            .map_err(UpdateError::Store)
    }
    pub fn presented(&mut self) {
        self.first_frame.get_or_insert_with(Instant::now);
    }
    pub fn healthy(&self) -> bool {
        self.first_frame
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(10))
    }
    pub fn accept(
        &self,
        store: &mut PackageStore,
        policy: &PackagePolicy,
        clean_quit: bool,
    ) -> Result<(), UpdateError> {
        if self.first_frame.is_none() || (!clean_quit && !self.healthy()) {
            return Err(UpdateError::NotHealthy);
        }
        store
            .accept(&self.activation, policy)
            .map_err(UpdateError::Store)
    }
    pub fn reject(&self, store: &mut PackageStore) -> Result<(), UpdateError> {
        store.reject(&self.activation).map_err(UpdateError::Store)
    }
}
