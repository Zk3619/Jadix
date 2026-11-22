use crate::{
    presentation::{NodeContent, NodeId, RootLifetime},
    HeadlessRoot,
};
use base64::Engine;
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    time::{Duration, Instant},
};

#[cfg(all(windows, feature = "windows"))]
#[path = "images/http_windows.rs"]
mod http_windows;

mod disk;
pub use disk::ImageDiskCache;
#[cfg(any(feature = "apple", feature = "android", feature = "linux", feature = "windows"))]
pub(crate) mod svg;

const MAX_SOURCE_BYTES: usize = 768 * 1024 - 24;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageSource {
    File(PathBuf),
    Url(String),
}

#[derive(Clone, Copy, Debug)]
pub struct ImageLimits {
    pub bindings: usize,
    pub pending: usize,
    pub source_bytes: usize,
    pub cache_bytes: usize,
    pub cache_entries: usize,
    pub cache_age: Duration,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            bindings: 32,
            pending: 4,
            source_bytes: MAX_SOURCE_BYTES,
            cache_bytes: 8 * 1024 * 1024,
            cache_entries: 32,
            cache_age: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    Limits,
    Disposed,
    Node,
    Source,
    Busy,
    Worker,
    Cancelled,
    Io(std::io::ErrorKind),
    Http(u32),
    Unsupported,
    Format,
}

pub struct ImageResult {
    pub node: NodeId,
    pub request: u64,
    pub result: Result<Arc<str>, ImageError>,
    pub cache_hit: bool,
    pub disk_hit: bool,
    pub disk_error: Option<ImageError>,
    cache_expires: Option<Instant>,
}

struct Loaded {
    bytes: Vec<u8>,
    expires: Option<Instant>,
}

struct Binding {
    source: ImageSource,
    request: u64,
    cancelled: Arc<AtomicBool>,
    cacheable: bool,
}

struct Job {
    node: NodeId,
    source: ImageSource,
    request: u64,
    cancelled: Arc<AtomicBool>,
    refresh: bool,
}

struct Cached {
    uri: Arc<str>,
    expires: Instant,
    used: Instant,
}

pub struct ImageLoader {
    lifetime: RootLifetime,
    limits: ImageLimits,
    sender: Option<SyncSender<Job>>,
    receiver: Option<Receiver<ImageResult>>,
    bindings: BTreeMap<NodeId, Binding>,
    cache: BTreeMap<ImageSource, Cached>,
    ready: VecDeque<ImageResult>,
    cache_bytes: usize,
    next: u64,
    disk_enabled: bool,
    disk_stats: Arc<disk::Statistics>,
    worker_active: Arc<AtomicBool>,
}

impl ImageLimits {
    pub fn is_valid(&self) -> bool {
        !(self.bindings == 0
            || self.bindings > 128
            || self.pending == 0
            || self.pending > 8
            || self.source_bytes == 0
            || self.source_bytes > MAX_SOURCE_BYTES
            || self.cache_bytes > 32 * 1024 * 1024
            || self.cache_entries > 128
            || self.cache_age > Duration::from_secs(86400))
    }
}

impl ImageLoader {
    pub fn new(
        lifetime: RootLifetime,
        limits: ImageLimits,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, ImageError> {
        Self::create(lifetime, limits, None, wake)
    }

    pub fn with_disk_cache(
        lifetime: RootLifetime,
        limits: ImageLimits,
        disk: ImageDiskCache,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, ImageError> {
        disk.validate()?;
        Self::create(lifetime, limits, Some(disk), wake)
    }

    fn create(
        lifetime: RootLifetime,
        limits: ImageLimits,
        disk: Option<ImageDiskCache>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, ImageError> {
        if lifetime.is_disposed() {
            return Err(ImageError::Disposed);
        }
        if !limits.is_valid() { return Err(ImageError::Limits); }
        let (sender, jobs) = sync_channel::<Job>(limits.pending);
        let (finished, receiver) = sync_channel(limits.pending);
        let disk_enabled = disk.is_some();
        let disk_stats = Arc::new(disk::Statistics::default());
        let worker_stats = disk_stats.clone();
        let worker_active = Arc::new(AtomicBool::new(true));
        let worker_state = worker_active.clone();
        std::thread::Builder::new()
            .name("jadix-image-source".into())
            .spawn(move || {
                let _activity = disk::WorkerActivity(worker_state);
                let mut disk = disk.map(|config| disk::Cache::open(config, worker_stats));
                while let Ok(job) = jobs.recv() {
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    let mut disk_hit = false;
                    let mut disk_error = None;
                    let mut loaded = None;
                    if let Some(cache) = &mut disk {
                        match cache {
                            Ok(cache) => {
                                let cached = if job.refresh {
                                    cache.invalidate(&job.source).map(|()| None)
                                } else {
                                    cache.read(&job.source, limits.source_bytes)
                                };
                                match cached {
                                    Ok(cached) => {
                                        disk_hit = cached.is_some();
                                        loaded = cached;
                                    }
                                    Err(error) => disk_error = Some(error),
                                }
                            }
                            Err(error) => disk_error = Some(error.clone()),
                        }
                    }
                    let mut cache_expires = None;
                    let result = loaded
                        .map_or_else(
                            || {
                                fetch(
                                    &job.source,
                                    &job.cancelled,
                                    limits.source_bytes,
                                    limits.cache_age,
                                )
                            },
                            Ok,
                        )
                        .and_then(|loaded| {
                            let uri = image_uri(&loaded.bytes)?;
                            if !disk_hit && !job.cancelled.load(Ordering::Acquire) {
                                if let Some(Ok(cache)) = &mut disk {
                                    if let Err(error) = cache.write(&job.source, &loaded) {
                                        disk_error = Some(error);
                                    }
                                }
                            }
                            cache_expires = loaded.expires;
                            Ok(Arc::<str>::from(uri))
                        });
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    if finished
                        .send(ImageResult {
                            node: job.node,
                            request: job.request,
                            result,
                            cache_hit: disk_hit,
                            disk_hit,
                            disk_error,
                            cache_expires,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wake();
                }
            })
            .map_err(|_| ImageError::Worker)?;
        Ok(Self {
            lifetime,
            limits,
            sender: Some(sender),
            receiver: Some(receiver),
            bindings: BTreeMap::new(),
            cache: BTreeMap::new(),
            ready: VecDeque::new(),
            cache_bytes: 0,
            next: 0,
            disk_enabled,
            disk_stats,
            worker_active,
        })
    }

    pub fn request(
        &mut self,
        model: &HeadlessRoot,
        node: NodeId,
        source: ImageSource,
    ) -> Result<u64, ImageError> {
        self.queue(model, node, source, false)
    }

    pub fn reload(
        &mut self,
        model: &HeadlessRoot,
        node: NodeId,
        source: ImageSource,
    ) -> Result<u64, ImageError> {
        self.queue(model, node, source, true)
    }

    fn queue(
        &mut self,
        model: &HeadlessRoot,
        node: NodeId,
        source: ImageSource,
        refresh: bool,
    ) -> Result<u64, ImageError> {
        if model.id() != self.lifetime.id() {
            return Err(ImageError::Node);
        }
        self.prune(model);
        if self.lifetime.is_disposed() || model.lifetime().is_disposed() {
            return Err(ImageError::Disposed);
        }
        if model.id() != self.lifetime.id()
            || node.root != self.lifetime.id()
            || !model
                .node(node)
                .is_some_and(|node| matches!(node.descriptor.content, NodeContent::Image { .. }))
        {
            return Err(ImageError::Node);
        }
        let valid = match &source {
            ImageSource::File(path) => path.is_absolute() && path.as_os_str().len() <= 4096,
            ImageSource::Url(url) => {
                url.len() <= 4096
                    && !url.contains(['\0', '\r', '\n'])
                    && !url
                        .split_once("://")
                        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default())
                        .unwrap_or_default()
                        .contains('@')
                    && (url.starts_with("https://") || url.starts_with("http://"))
            }
        };
        if !valid {
            return Err(ImageError::Source);
        }
        if !refresh {
            if let Some(binding) = self
                .bindings
                .get(&node)
                .filter(|binding| binding.source == source)
            {
                return Ok(binding.request);
            }
        }
        if !self.bindings.contains_key(&node) && self.bindings.len() >= self.limits.bindings {
            return Err(ImageError::Limits);
        }
        let request = self.next.checked_add(1).ok_or(ImageError::Limits)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cached = if refresh {
            None
        } else {
            self.cache
                .get_mut(&source)
                .filter(|cached| cached.expires > Instant::now())
        };
        if let Some(cached) = cached {
            if self.ready.len() >= self.limits.pending {
                return Err(ImageError::Busy);
            }
            cached.used = Instant::now();
            self.ready.push_back(ImageResult {
                node,
                request,
                result: Ok(cached.uri.clone()),
                cache_hit: true,
                disk_hit: false,
                disk_error: None,
                cache_expires: Some(cached.expires),
            });
        } else {
            let job = Job {
                node,
                source: source.clone(),
                request,
                cancelled: cancelled.clone(),
                refresh,
            };
            self.sender
                .as_ref()
                .ok_or(ImageError::Disposed)?
                .try_send(job)
                .map_err(|error| match error {
                    std::sync::mpsc::TrySendError::Full(_) => ImageError::Busy,
                    std::sync::mpsc::TrySendError::Disconnected(_) => ImageError::Worker,
                })?;
        }
        if refresh {
            if let Some(previous) = self.cache.remove(&source) {
                self.cache_bytes -= previous.uri.len();
            }
            for binding in self
                .bindings
                .values_mut()
                .filter(|binding| binding.source == source)
            {
                binding.cacheable = false;
            }
        }
        if let Some(previous) = self.bindings.insert(
            node,
            Binding {
                source,
                request,
                cancelled,
                cacheable: true,
            },
        ) {
            previous.cancelled.store(true, Ordering::Release);
        }
        self.next = request;
        Ok(request)
    }

    pub fn cancel(&mut self, node: NodeId) {
        if let Some(binding) = self.bindings.remove(&node) {
            binding.cancelled.store(true, Ordering::Release);
        }
        self.ready.retain(|result| result.node != node);
    }

    fn prune(&mut self, model: &HeadlessRoot) {
        let removed: Vec<_> = self
            .bindings
            .keys()
            .copied()
            .filter(|node| {
                self.lifetime.is_disposed()
                    || model.id() != self.lifetime.id()
                    || !model.node(*node).is_some_and(|node| {
                        matches!(node.descriptor.content, NodeContent::Image { .. })
                    })
            })
            .collect();
        for node in removed {
            self.cancel(node);
        }
        if self.lifetime.is_disposed() {
            self.dispose();
        }
    }

    pub fn poll(&mut self, model: &HeadlessRoot) -> Vec<ImageResult> {
        if model.id() != self.lifetime.id() {
            return Vec::new();
        }
        self.prune(model);
        let mut results: Vec<_> = self.ready.drain(..).collect();
        if let Some(receiver) = &self.receiver {
            results.extend(receiver.try_iter());
        }
        results.retain(|result| {
            self.bindings
                .get(&result.node)
                .is_some_and(|binding| binding.request == result.request)
        });
        for result in &results {
            if !result.cache_hit || result.disk_hit {
                if let Ok(uri) = &result.result {
                    if let Some(binding) = self
                        .bindings
                        .get(&result.node)
                        .filter(|binding| binding.cacheable)
                    {
                        let source = binding.source.clone();
                        if let Some(old) = self.cache.remove(&source) {
                            self.cache_bytes -= old.uri.len();
                        }
                        let now = Instant::now();
                        let expires = result
                            .cache_expires
                            .map(|expires| expires.min(now + self.limits.cache_age));
                        if uri.len() <= self.limits.cache_bytes
                            && self.limits.cache_entries > 0
                            && expires.is_some_and(|expires| expires > now)
                        {
                            while self.cache_bytes + uri.len() > self.limits.cache_bytes
                                || self.cache.len() >= self.limits.cache_entries
                            {
                                let Some(oldest) = self
                                    .cache
                                    .iter()
                                    .min_by_key(|(_, entry)| entry.used)
                                    .map(|(source, _)| source.clone())
                                else {
                                    break;
                                };
                                if let Some(old) = self.cache.remove(&oldest) {
                                    self.cache_bytes -= old.uri.len();
                                }
                            }
                            self.cache_bytes += uri.len();
                            self.cache.insert(
                                source,
                                Cached {
                                    uri: uri.clone(),
                                    expires: expires.unwrap(),
                                    used: Instant::now(),
                                },
                            );
                        }
                    }
                }
            }
        }
        results
    }

    pub fn inspection(&self) -> serde_json::Value {
        serde_json::json!({"bindings":self.bindings.len(),"cache_entries":self.cache.len(),"cache_bytes":self.cache_bytes,
            "source_byte_limit":self.limits.source_bytes,"network_available":cfg!(any(all(windows,feature="windows"), all(target_os="linux",not(target_env="ohos"),feature="linux"))),"source_values_redacted":true,
            "disk_enabled":self.disk_enabled,"disk_file_sources_only":false,
            "url_cache_policy":"explicit max-age; no stale fallback; no-store/private/no-cache/Vary/Set-Cookie rejected",
            "disk_entries":self.disk_stats.entries.load(Ordering::Acquire),
            "disk_bytes":self.disk_stats.bytes.load(Ordering::Acquire),"worker_active":self.worker_active.load(Ordering::Acquire)})
    }

    pub fn dispose(&mut self) {
        for (_, binding) in std::mem::take(&mut self.bindings) {
            binding.cancelled.store(true, Ordering::Release);
        }
        self.sender.take();
        self.receiver.take();
        self.cache.clear();
        self.ready.clear();
        self.cache_bytes = 0;
    }
}

impl Drop for ImageLoader {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn image_uri(bytes: &[u8]) -> Result<String, ImageError> {
    #[cfg(any(feature = "apple", feature = "android", feature = "linux", feature = "windows"))]
    let expanded = svg::source_bytes(bytes).ok_or(ImageError::Format)?;
    #[cfg(any(feature = "apple", feature = "android", feature = "linux", feature = "windows"))]
    let bytes = expanded.as_ref();
    #[cfg(any(feature = "apple", feature = "android", feature = "linux", feature = "windows"))]
    let mime = {
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
            else if bytes.starts_with(b"\xff\xd8\xff") { "image/jpeg" }
            else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { "image/gif" }
            else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { "image/webp" }
            else if bytes.starts_with(b"BM") { "image/bmp" }
            else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") { "image/tiff" }
            else if bytes.starts_with(&[0, 0, 1, 0]) { "image/x-icon" }
            else if bytes.starts_with(b"icns") { "image/icns" }
            else if svg::rasterize(bytes, 1, 1, 3).is_some() { "image/svg+xml" }
            else { return Err(ImageError::Format); };
        if mime != "image/svg+xml" {
            let size = imagesize::blob_size(bytes).map_err(|_|ImageError::Format)?;
            if size.width == 0 || size.height == 0 || size.width > 4096 || size.height > 4096 || size.width.saturating_mul(size.height) > 4 * 1024 * 1024 { return Err(ImageError::Limits); }
        }
        mime
    };
    #[cfg(not(any(feature = "apple", feature = "android", feature = "linux", feature = "windows")))]
    let mime = "image/png";
    let uri = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    if uri.len() > 1024 * 1024 { return Err(ImageError::Limits); }
    #[cfg(not(any(feature = "apple", feature = "android", feature = "linux", feature = "windows")))]
    NodeContent::Image { source: uri.clone(), alt: String::new() }.image_dimensions().ok_or(ImageError::Format)?;
    Ok(uri)
}

#[cfg(all(windows, feature = "windows"))]
pub(crate) fn load_uri(source: &ImageSource, cancelled: &Arc<AtomicBool>) -> Result<String, ImageError> {
    if cancelled.load(Ordering::Acquire) { return Err(ImageError::Cancelled); }
    let loaded = fetch(source, cancelled, MAX_SOURCE_BYTES, Duration::from_secs(30))?;
    if cancelled.load(Ordering::Acquire) { return Err(ImageError::Cancelled); }
    image_uri(&loaded.bytes)
}

fn fetch(
    source: &ImageSource,
    cancelled: &Arc<AtomicBool>,
    limit: usize,
    cache_age: Duration,
) -> Result<Loaded, ImageError> {
    match source {
        ImageSource::File(path) => {
            let mut file =
                std::fs::File::open(path).map_err(|error| ImageError::Io(error.kind()))?;
            if file
                .metadata()
                .map_err(|error| ImageError::Io(error.kind()))?
                .len()
                > limit as u64
            {
                return Err(ImageError::Limits);
            }
            let mut output = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                if cancelled.load(Ordering::Acquire) {
                    return Err(ImageError::Cancelled);
                }
                let count = file
                    .read(&mut chunk)
                    .map_err(|error| ImageError::Io(error.kind()))?;
                if count == 0 {
                    return Ok(Loaded {
                        bytes: output,
                        expires: Instant::now().checked_add(cache_age),
                    });
                }
                if output.len().saturating_add(count) > limit {
                    return Err(ImageError::Limits);
                }
                output.extend_from_slice(&chunk[..count]);
            }
        }
        ImageSource::Url(url) => {
            #[cfg(all(windows, feature = "windows"))]
            {
                http_windows::fetch(url, cancelled, limit)
            }
            #[cfg(all(target_os = "linux", not(target_env = "ohos"), feature = "linux"))]
            {
                let cancellation = cancelled.clone();
                let mut request = jadix_net::HttpRequest::get(url);
                request.max_response_bytes = limit;
                request.timeout = Duration::from_secs(10);
                let started = Instant::now();
                let response = jadix_net::native_https::HttpsClient::new("/usr/bin/curl")
                    .get_cancellable(request, move || cancellation.load(Ordering::Acquire))
                    .map_err(|error| match error.kind {
                        jadix_net::HttpErrorKind::Cancelled => ImageError::Cancelled,
                        jadix_net::HttpErrorKind::BodyTooLarge => ImageError::Limits,
                        _ => ImageError::Source,
                    })?;
                if response.status != 200 { return Err(ImageError::Source); }
                let expires = jadix_net::cache::freshness(&response.headers, std::time::SystemTime::now(), started.elapsed())
                    .and_then(|age| Instant::now().checked_add(age.min(cache_age)));
                Ok(Loaded { bytes: response.body.to_vec(), expires })
            }
            #[cfg(not(any(all(windows, feature = "windows"), all(target_os = "linux", not(target_env = "ohos"), feature = "linux"))))]
            {
                let _ = url;
                Err(ImageError::Unsupported)
            }
        }
    }
}
