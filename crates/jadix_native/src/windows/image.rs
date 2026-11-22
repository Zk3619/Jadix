use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    Arc,
};

use base64::Engine;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows_sys::Win32::{
    Foundation::HWND, Graphics::Gdi::*, UI::WindowsAndMessaging::PostMessageW,
};

use crate::presentation::{NodeContent, NodeId};

pub(super) const READY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 19;

struct Job {
    node: NodeId,
    generation: u64,
    source: String,
    root: std::path::PathBuf,
    cancelled: Arc<AtomicBool>,
}

pub(super) struct Completion {
    pub node: NodeId,
    pub generation: u64,
    pub result: Result<Pixels, String>,
}

pub(super) struct Pixels {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn decode(source: &str) -> Result<Pixels, String> {
    if !(NodeContent::image_source_dimensions(source).is_some()
        || NodeContent::is_inline_raster_source(source) || NodeContent::is_inline_svg_source(source)) {
        return Err("unsupported or oversized native image source".into());
    }
    let (mime, encoded) = source.split_once(";base64,").ok_or("invalid native image URL")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|error| error.to_string())?;
    if mime == "data:image/svg+xml" {
        let (pixels, _) = crate::images::svg::rasterize(&bytes, 1, 1, 3).ok_or("invalid native SVG")?;
        let (width, height) = (pixels.width(), pixels.height());
        let mut bytes = pixels.take();
        for pixel in bytes.chunks_exact_mut(4) { pixel.swap(0, 2); }
        return Ok(Pixels { width, height, bytes });
    }
    // ICNS can carry modern PNG representations; choose a bounded representation.
    let bytes = if mime == "data:image/icns" { icns_png(&bytes).ok_or("unsupported ICNS representation")? } else { &bytes };
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| error.to_string())?;
        let stream = factory.CreateStream().map_err(|error| error.to_string())?;
        stream
            .InitializeFromMemory(bytes)
            .map_err(|error| error.to_string())?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
            .map_err(|error| error.to_string())?;
        let frame = decoder.GetFrame(0).map_err(|error| error.to_string())?;
        let mut width = 0;
        let mut height = 0;
        frame
            .GetSize(&mut width, &mut height)
            .map_err(|error| error.to_string())?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 || u64::from(width) * u64::from(height) > 4 * 1024 * 1024 {
            return Err("native image exceeds decoded pixel quota".into());
        }
        let converter = factory
            .CreateFormatConverter()
            .map_err(|error| error.to_string())?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|error| error.to_string())?;
        let mut pixels = Pixels {
            width,
            height,
            bytes: vec![0; width as usize * height as usize * 4],
        };
        converter
            .CopyPixels(std::ptr::null(), width * 4, &mut pixels.bytes)
            .map_err(|error| error.to_string())?;
        Ok(pixels)
    }
}

fn icns_png(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.get(..4)? != b"icns" || u32::from_be_bytes(bytes.get(4..8)?.try_into().ok()?) as usize != bytes.len() { return None; }
    let mut at = 8;
    let mut selected = None;
    while at < bytes.len() {
        let length = u32::from_be_bytes(bytes.get(at+4..at+8)?.try_into().ok()?) as usize;
        if length < 8 { return None; }
        let payload = bytes.get(at+8..at.checked_add(length)?)?;
        if payload.starts_with(b"\x89PNG\r\n\x1a\n") { selected = Some(payload); }
        at += length;
    }
    selected
}

pub(super) fn resolve(source: &str, root: &std::path::Path, cancelled: &Arc<AtomicBool>) -> Result<String, String> {
    use crate::images::ImageSource;
    if source.starts_with("data:") { return Ok(source.into()); }
    let resource = if NodeContent::is_remote_image_source(source) { ImageSource::Url(source.into()) }
        else if NodeContent::is_local_image_source(source) {
            let path = if source.starts_with("file:///") {
                use windows::{core::{PCWSTR, PWSTR}, Win32::UI::Shell::PathCreateFromUrlW};
                let mut path = vec![0u16; 32768]; let mut length = path.len() as u32;
                unsafe { PathCreateFromUrlW(PCWSTR(super::wide(source).as_ptr()), PWSTR(path.as_mut_ptr()), &mut length, 0) }.map_err(|e| e.to_string())?;
                let length = path.iter().position(|c| *c == 0).unwrap_or(length as usize);
                std::path::PathBuf::from(String::from_utf16(&path[..length]).map_err(|e| e.to_string())?)
            } else {
                let relative = jadix_core::web_resource::bundle_path(source).ok_or("invalid image bundle path")?;
                let root = root.canonicalize().map_err(|e| e.to_string())?;
                let path = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
                if !path.starts_with(root) { return Err("image escapes the bundle".into()); }
                path
            };
            if !path.is_absolute() || !path.is_file() { return Err("image is not a regular absolute file".into()); }
            ImageSource::File(path)
        } else { return Err("invalid native image source".into()); };
    crate::images::load_uri(&resource, cancelled).map_err(|e| format!("{e:?}"))
}

pub(super) struct Loader {
    sender: Option<SyncSender<Job>>,
    receiver: Receiver<Completion>,
    waiting: BTreeMap<NodeId, Job>,
    active: BTreeMap<NodeId, (u64, Arc<AtomicBool>)>,
    closed: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    next: u64,
}

impl Loader {
    pub(super) fn new(window: HWND) -> Result<Self, String> {
        let (sender, jobs) = sync_channel::<Job>(4);
        let (completed, receiver) = sync_channel(4);
        let closed = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let worker_closed = closed.clone();
        let worker_failed = failed.clone();
        let window = window as usize;
        std::thread::Builder::new()
            .name(String::from("jadix-native-images"))
            .spawn(move || {
                let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                if status.is_err() {
                    worker_failed.store(true, Ordering::Release);
                    unsafe { PostMessageW(window as HWND, READY, 0, 0) };
                    return;
                }
                while let Ok(job) = jobs.recv() {
                    if worker_closed.load(Ordering::Acquire) {
                        break;
                    }
                    if job.cancelled.load(Ordering::Acquire) { continue; }
                    let result = std::panic::catch_unwind(|| resolve(&job.source, &job.root, &job.cancelled).and_then(|source| decode(&source)))
                        .unwrap_or_else(|_| Err(String::from("native image decoder panicked")));
                    if worker_closed.load(Ordering::Acquire) {
                        break;
                    }
                    if job.cancelled.load(Ordering::Acquire) { continue; }
                    let completion = Completion {
                        node: job.node,
                        generation: job.generation,
                        result,
                    };
                    if completed.send(completion).is_err() {
                        break;
                    }
                    if worker_closed.load(Ordering::Acquire) {
                        break;
                    }
                    unsafe { PostMessageW(window as HWND, READY, 0, 0) };
                }
                unsafe { CoUninitialize() };
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            waiting: BTreeMap::new(),
            active: BTreeMap::new(),
            closed,
            failed,
            next: 0,
        })
    }

    pub(super) fn request(&mut self, node: NodeId, source: &str, root: &std::path::Path) -> Result<u64, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err(String::from("native image loader closed"));
        }
        if !self.active.contains_key(&node) && self.active.len() >= 32 {
            return Err("native image binding quota exceeded".into());
        }
        self.cancel(node);
        self.next = self
            .next
            .checked_add(1)
            .ok_or("native image generation exhausted")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.active.insert(node, (self.next, cancelled.clone()));
        self.waiting.insert(
            node,
            Job {
                node,
                generation: self.next,
                source: source.into(),
                root: root.to_owned(),
                cancelled,
            },
        );
        self.flush()?;
        Ok(self.next)
    }

    fn flush(&mut self) -> Result<(), String> {
        while let Some((node, job)) = self.waiting.pop_first() {
            match self
                .sender
                .as_ref()
                .ok_or("native image loader closed")?
                .try_send(job)
            {
                Ok(()) => {}
                Err(TrySendError::Full(job)) => {
                    self.waiting.insert(node, job);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err(String::from("native image worker stopped"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn drain(&mut self) -> Result<Vec<Completion>, String> {
        if self.failed.load(Ordering::Acquire) {
            return Err(String::from("native image worker failed"));
        }
        let completed: Vec<_> = self.receiver.try_iter().collect();
        for completion in &completed {
            if self.active.get(&completion.node).is_some_and(|(generation, _)| *generation == completion.generation) {
                self.active.remove(&completion.node);
            }
        }
        self.flush()?;
        Ok(completed)
    }

    pub(super) fn cancel(&mut self, node: NodeId) {
        self.waiting.remove(&node);
        if let Some((_, cancelled)) = self.active.remove(&node) { cancelled.store(true, Ordering::Release); }
    }

    pub(super) fn close(&mut self) {
        self.closed.store(true, Ordering::Release);
        for (_, cancelled) in self.active.values() { cancelled.store(true, Ordering::Release); }
        self.active.clear();
        self.waiting.clear();
        self.sender.take();
        while self.receiver.try_recv().is_ok() {}
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) struct Bitmap {
    handle: HBITMAP,
    width: i32,
    height: i32,
}

impl Bitmap {
    pub(super) fn size(&self) -> [u32; 2] {
        [self.width as u32, self.height as u32]
    }

    pub(super) fn new(pixels: Pixels) -> Result<Self, String> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: pixels.width as i32,
                biHeight: -(pixels.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut address = std::ptr::null_mut();
        let handle = unsafe {
            CreateDIBSection(
                std::ptr::null_mut(),
                &info,
                DIB_RGB_COLORS,
                &mut address,
                std::ptr::null_mut(),
                0,
            )
        };
        if handle.is_null() || address.is_null() {
            let error = super::native_error("create native image bitmap");
            if !handle.is_null() {
                unsafe { DeleteObject(handle) };
            }
            return Err(error);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                pixels.bytes.as_ptr(),
                address.cast::<u8>(),
                pixels.bytes.len(),
            )
        };
        Ok(Self {
            handle,
            width: pixels.width as i32,
            height: pixels.height as i32,
        })
    }

    pub(super) unsafe fn paint(&self, destination: HDC, width: i32, height: i32, fit: crate::presentation::style::ImageFit, mut position: crate::presentation::style::ImagePosition, scale: f64) {
        position.offset = position.offset.map(|v| v * scale as f32);
        let Some(rect) = fit.geometry([width as f64, height as f64], [self.width as f64 * scale, self.height as f64 * scale], position) else { return; };
        // AlphaBlend takes signed integer coordinates and requires positive extents.
        if rect.iter().any(|v| v.abs() > i32::MAX as f64) { return; }
        let [x, y, fitted_width, fitted_height] = rect.map(|v| v.round() as i32);
        if fitted_width <= 0 || fitted_height <= 0 { return; }
        let source = unsafe { CreateCompatibleDC(destination) };
        if source.is_null() {
            return;
        }
        let previous = unsafe { SelectObject(source, self.handle) };
        unsafe {
            AlphaBlend(
                destination,
                x,
                y,
                fitted_width,
                fitted_height,
                source,
                0,
                0,
                self.width,
                self.height,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            );
            SelectObject(source, previous);
            DeleteDC(source);
        }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn uri(mime: &str, bytes: &[u8]) -> String {
        format!("data:image/{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
    }
    fn bmp() -> Vec<u8> {
        let mut bytes = vec![0; 62];
        bytes[..2].copy_from_slice(b"BM");
        for (at, value) in [(2, 62u32), (10, 54), (14, 40), (18, 2), (22, 1), (34, 8)] {
            bytes[at..at+4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[26] = 1; bytes[28] = 24;
        bytes[54..60].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        bytes
    }
    #[test]
    fn native_decoders_produce_bgra_pixels_and_reject_excess_dimensions() {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.unwrap();
        let bitmap = decode(&uri("bmp", &bmp())).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (2, 1));
        assert_eq!(bitmap.bytes, [0, 0, 255, 255, 0, 255, 0, 255]);
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1" fill="red" opacity="0.5"/></svg>"#;
        let pixels = decode(&uri("svg+xml", svg)).unwrap();
        assert_eq!((pixels.width, pixels.height), (2, 1));
        assert_eq!(pixels.bytes, [0, 0, 128, 128, 0, 0, 128, 128]);
        let gif = decode("data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7").unwrap();
        assert_eq!((gif.width, gif.height), (1, 1));
        let mut oversized = bmp(); oversized[18..22].copy_from_slice(&5000u32.to_le_bytes());
        assert!(decode(&uri("bmp", &oversized)).is_err());
        assert!(decode(&uri("svg+xml", b"<not-svg/>")).is_err());
        assert!(decode("https://example.com/image.png").is_err());
        unsafe { CoUninitialize() };
    }

    #[test]
    fn local_resources_decode_unicode_and_deny_escape_and_cancelled_work() {
        let root = std::env::temp_dir().join(format!("jadix-image-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("图片 空格.bmp"); std::fs::write(&path, bmp()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let resource = resolve("asset:/%E5%9B%BE%E7%89%87%20%E7%A9%BA%E6%A0%BC.bmp", &root, &cancel).unwrap();
        assert!(resource.starts_with("data:image/bmp;base64,"));
        assert_eq!(resolve(&format!("file:///{}", path.to_string_lossy().replace('\\', "/")), &root, &cancel).unwrap(), resource);
        assert!(resolve("asset:/%2e%2e/escape.bmp", &root, &cancel).is_err());
        cancel.store(true, Ordering::Release);
        assert!(resolve("asset:/%E5%9B%BE%E7%89%87%20%E7%A9%BA%E6%A0%BC.bmp", &root, &cancel).is_err());
        std::fs::remove_file(path).unwrap(); std::fs::remove_dir(root).unwrap();
    }
}
