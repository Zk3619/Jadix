use std::{
    num::NonZeroU32,
    sync::{Arc, RwLock},
    time::Duration,
};

use moka::sync::Cache;

use super::remote::{is_remote_src, RemoteAssetFetcher};

type ByteCache = Cache<String, Arc<[u8]>>;

/// Opt-in cache of encoded HTTP(S) assets, shared by Image, Avatar and custom loaders.
///
/// Equal URL misses coalesce. Failed loads are not retained. Capacity weighs
/// payload bytes (empty values weigh one), not keys, in-flight work or external
/// Arc references. The backend still owns transport timeouts and streaming limits.
#[derive(Clone)]
pub struct CachedRemoteAssetFetcher {
    fetcher: Arc<dyn RemoteAssetFetcher>,
    cache: Arc<RwLock<ByteCache>>,
    capacity: NonZeroU32,
    time_to_live: Duration,
}

impl CachedRemoteAssetFetcher {
    /// Use a separate instance per authentication/session scope. A response larger
    /// than capacity is rejected; TTL applies to subsequent fetches, not live textures.
    pub fn new(
        fetcher: impl RemoteAssetFetcher + 'static,
        capacity: NonZeroU32,
        time_to_live: Duration,
    ) -> Self {
        Self {
            fetcher: Arc::new(fetcher),
            cache: Arc::new(RwLock::new(Self::build_cache(capacity, time_to_live))),
            capacity,
            time_to_live,
        }
    }

    fn build_cache(capacity: NonZeroU32, time_to_live: Duration) -> ByteCache {
        Cache::builder()
            .max_capacity(u64::from(capacity.get()))
            .weigher(|_key: &String, value: &Arc<[u8]>| value.len().max(1) as u32)
            .time_to_live(time_to_live)
            .build()
    }

    /// Atomically start a fresh cache through all clones. Earlier loads can finish
    /// for their existing callers but cannot repopulate the new cache.
    pub fn clear(&self) {
        let fresh = Self::build_cache(self.capacity, self.time_to_live);
        let previous = {
            let mut cache = self
                .cache
                .write()
                .unwrap_or_else(|error| error.into_inner());
            std::mem::replace(&mut *cache, fresh)
        };
        drop(previous);
    }

    /// Blocking: call from a worker. A caller's departure does not cancel a load
    /// shared with other consumers; the backend controls the underlying operation.
    pub fn fetch_shared(&self, url: &str) -> Result<Arc<[u8]>, String> {
        let url = url.trim();
        if !is_remote_src(url) {
            return Err("not an http(s) URL".into());
        }
        let cache = self
            .cache
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        cache
            .try_get_with_by_ref(url, || {
                let bytes = self.fetcher.fetch_shared(url)?;
                if bytes.len() > self.capacity.get() as usize {
                    return Err("remote asset exceeds cache byte capacity".to_string());
                }
                Ok(bytes)
            })
            .map_err(|error: Arc<String>| (*error).clone())
    }
}

impl RemoteAssetFetcher for CachedRemoteAssetFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        self.fetch_shared(url).map(|bytes| bytes.to_vec())
    }

    fn fetch_shared(&self, url: &str) -> Result<Arc<[u8]>, String> {
        CachedRemoteAssetFetcher::fetch_shared(self, url)
    }
}

#[cfg(test)]
mod tests;
