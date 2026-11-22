//! The same bounded, cancellable source policy for native audio backends.
use jadix_core::audio::{AudioSource, MAX_SOUND_BYTES};
use base64::Engine;
use std::{io::Read, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

pub(crate) fn local_path(source: &AudioSource, root: &Path) -> Result<Option<PathBuf>, String> {
    source.validate()?;
    let path = match source {
        AudioSource::File { path } => path.clone(),
        AudioSource::Url { url } if !url.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")) => {
            let relative = jadix_core::web_resource::bundle_path(url).ok_or("invalid bundled audio source")?;
            let root = root.canonicalize().map_err(|e| e.to_string())?;
            let path = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(root) { return Err("audio source escapes the bundle".into()); }
            path
        }
        _ => return Ok(None),
    };
    if !path.metadata().is_ok_and(|m| m.is_file()) { return Err("audio source is not a regular file".into()); }
    Ok(Some(path))
}

pub(crate) fn read(source: AudioSource, root: &Path, curl: &Path, cancel: Arc<AtomicBool>) -> Result<Vec<u8>, String> {
    source.validate()?;
    if cancel.load(Ordering::Acquire) { return Err("audio load cancelled".into()); }
    let bytes = if let Some(path) = local_path(&source, root)? {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_SOUND_BYTES as u64 { return Err("audio source exceeds encoded quota".into()); }
        let mut bytes = Vec::new();
        file.take(MAX_SOUND_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        bytes
    } else {
        match source {
            AudioSource::Bytes { base64 } => base64::engine::general_purpose::STANDARD.decode(base64).map_err(|e| e.to_string())?,
            AudioSource::Url { url } => {
                let mut request = jadix_net::HttpRequest::get(url);
                request.max_response_bytes = MAX_SOUND_BYTES;
                request.timeout = Duration::from_secs(10);
                let stopped = cancel.clone();
                let response = jadix_net::native_https::HttpsClient::new(curl)
                    .get_cancellable(request, move || stopped.load(Ordering::Acquire)).map_err(|e| e.to_string())?;
                if response.status != 200 { return Err("audio source did not return HTTP 200".into()); }
                response.body.to_vec()
            }
            _ => unreachable!(),
        }
    };
    if cancel.load(Ordering::Acquire) { return Err("audio load cancelled".into()); }
    if bytes.is_empty() || bytes.len() > MAX_SOUND_BYTES { return Err("audio source exceeds encoded quota".into()); }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_sources_obey_cancellation_and_bundle_containment() {
        let root = std::env::temp_dir().join(format!("jadix-audio-source-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("中文.wav"), b"RIFF").unwrap();
        let source = AudioSource::Url { url: "app://bundle/%E4%B8%AD%E6%96%87.wav".into() };
        let cancel = Arc::new(AtomicBool::new(false));
        assert_eq!(read(source.clone(), &root, Path::new("unused"), cancel.clone()).unwrap(), b"RIFF");
        cancel.store(true, Ordering::Release);
        assert!(read(source, &root, Path::new("unused"), cancel).is_err());
        assert!(local_path(&AudioSource::Url { url: "app://bundle/%2e%2e/escape".into() }, &root).is_err());
        assert_eq!(local_path(&AudioSource::Url { url: "HTTPS://example.com/sound.wav".into() }, &root).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
