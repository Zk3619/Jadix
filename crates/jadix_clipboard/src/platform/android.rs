//! Android clipboard stub. JNI / `android.content.ClipboardManager` comes later.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::common::Result;
#[cfg(feature = "image")]
use crate::common::RustImageData;
use crate::{Clipboard, ClipboardContent, ClipboardHandler, ClipboardWatcher, ContentFormat};

const UNSUPPORTED: &str = "android clipboard is not implemented yet";

pub struct ClipboardContext;

impl ClipboardContext {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl Clipboard for ClipboardContext {
    fn available_formats(&self) -> Result<Vec<String>> {
        Err(UNSUPPORTED.into())
    }

    fn has(&self, _format: ContentFormat) -> bool {
        false
    }

    fn clear(&self) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn get_buffer(&self, _format: &str) -> Result<Vec<u8>> {
        Err(UNSUPPORTED.into())
    }

    fn get_text(&self) -> Result<String> {
        Err(UNSUPPORTED.into())
    }

    fn get_rich_text(&self) -> Result<String> {
        Err(UNSUPPORTED.into())
    }

    fn get_html(&self) -> Result<String> {
        Err(UNSUPPORTED.into())
    }

    #[cfg(feature = "image")]
    fn get_image(&self) -> Result<RustImageData> {
        Err(UNSUPPORTED.into())
    }

    fn get_files(&self) -> Result<Vec<String>> {
        Err(UNSUPPORTED.into())
    }

    fn get(&self, _formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
        Err(UNSUPPORTED.into())
    }

    fn set_buffer(&self, _format: &str, _buffer: Vec<u8>) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set_text(&self, _text: String) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set_rich_text(&self, _text: String) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set_html(&self, _html: String) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    #[cfg(feature = "image")]
    fn set_image(&self, _image: RustImageData) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set_files(&self, _files: Vec<String>) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set(&self, _contents: Vec<ClipboardContent>) -> Result<()> {
        Err(UNSUPPORTED.into())
    }
}

pub struct ClipboardWatcherContext<T: ClipboardHandler> {
    handlers: Vec<T>,
    stop_signal: Sender<()>,
    stop_receiver: Receiver<()>,
    running: bool,
}

impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
    pub fn new() -> Result<Self> {
        Self::new_with_interval(Duration::from_millis(500))
    }

    pub fn new_with_interval(_interval: Duration) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            handlers: Vec::new(),
            stop_signal: tx,
            stop_receiver: rx,
            running: false,
        })
    }
}

impl<T: ClipboardHandler + Send> ClipboardWatcher<T> for ClipboardWatcherContext<T> {
    fn add_handler(&mut self, handler: T) -> &mut Self {
        self.handlers.push(handler);
        self
    }

    fn start_watch(&mut self) {
        if self.running || self.handlers.is_empty() {
            return;
        }
        self.running = true;
        let _ = self.stop_receiver.recv();
        self.running = false;
    }

    fn get_shutdown_channel(&self) -> WatcherShutdown {
        WatcherShutdown {
            stop_signal: self.stop_signal.clone(),
        }
    }
}

pub struct WatcherShutdown {
    stop_signal: Sender<()>,
}

impl Drop for WatcherShutdown {
    fn drop(&mut self) {
        let _ = self.stop_signal.send(());
    }
}
