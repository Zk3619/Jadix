use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
};

use base64::Engine;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
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
}

pub(super) struct Completion {
    pub node: NodeId,
    pub generation: u64,
    pub result: Result<Pixels, String>,
}

pub(super) struct Pixels {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

fn decode(source: &str) -> Result<Pixels, String> {
    let content = NodeContent::Image {
        source: source.into(),
        alt: String::new(),
    };
    let expected = content
        .image_dimensions()
        .ok_or("invalid native PNG header")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            source
                .strip_prefix("data:image/png;base64,")
                .ok_or("unsupported native image source")?,
        )
        .map_err(|error| error.to_string())?;
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| error.to_string())?;
        let stream = factory.CreateStream().map_err(|error| error.to_string())?;
        stream
            .InitializeFromMemory(&bytes)
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
        if [width, height] != expected {
            return Err(String::from("decoded PNG dimensions changed"));
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

pub(super) struct Loader {
    sender: Option<SyncSender<Job>>,
    receiver: Receiver<Completion>,
    waiting: BTreeMap<NodeId, Job>,
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
                    let result = std::panic::catch_unwind(|| decode(&job.source))
                        .unwrap_or_else(|_| Err(String::from("native image decoder panicked")));
                    if worker_closed.load(Ordering::Acquire) {
                        break;
                    }
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
            closed,
            failed,
            next: 0,
        })
    }

    pub(super) fn request(&mut self, node: NodeId, source: &str) -> Result<u64, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err(String::from("native image loader closed"));
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("native image generation exhausted")?;
        self.waiting.insert(
            node,
            Job {
                node,
                generation: self.next,
                source: source.into(),
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
        let completed = self.receiver.try_iter().collect();
        self.flush()?;
        Ok(completed)
    }

    pub(super) fn cancel(&mut self, node: NodeId) {
        self.waiting.remove(&node);
    }

    pub(super) fn close(&mut self) {
        self.closed.store(true, Ordering::Release);
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

    pub(super) unsafe fn paint(&self, destination: HDC, width: i32, height: i32) {
        let source = unsafe { CreateCompatibleDC(destination) };
        if source.is_null() {
            return;
        }
        let previous = unsafe { SelectObject(source, self.handle) };
        unsafe {
            AlphaBlend(
                destination,
                0,
                0,
                width,
                height,
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
