//! Validated, cheaply cloned image shared by native windows and tray icons.
//! Launcher / executable icons are build resources; see `docs/application-icons.md`.

use std::path::Path;
use std::sync::Arc;

/// RGBA8 native icon. Decode once, then reuse for windows and trays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIcon(Arc<winit::icon::RgbaIcon>);

impl AppIcon {
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        let expected = width.checked_mul(height).and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || expected.map(|n| n as usize) != Some(rgba.len()) {
            return Err(
                "icon requires nonzero dimensions and exactly width * height * 4 RGBA bytes".into(),
            );
        }
        winit::icon::RgbaIcon::new(rgba, width, height)
            .map(|icon| Self(Arc::new(icon)))
            .map_err(|e| e.to_string())
    }

    /// Decode supported raster formats (PNG/ICO/etc.); SVG requires feature `svg`.
    /// Animated images use their first frame.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let decoded = crate::media::decode_bytes(bytes, crate::media::DecodeOptions::image())
            .ok_or_else(|| "unsupported or invalid icon image".to_string())?;
        Self::from_rgba(decoded.rgba, decoded.width, decoded.height)
    }

    /// Read an explicit filesystem path. Prefer `include_bytes!` for bundled defaults.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn width(&self) -> u32 {
        self.0.width()
    }
    pub fn height(&self) -> u32 {
        self.0.height()
    }
    pub fn rgba(&self) -> &[u8] {
        self.0.buffer()
    }

    /// Runtime window icon support; individual Wayland compositors may ignore it.
    /// macOS, mobile and Web use bundle / launcher resources instead.
    pub const fn window_supported() -> bool {
        cfg!(any(
            windows,
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )) && !cfg!(target_env = "ohos")
    }

    pub(crate) fn to_winit(&self) -> winit::icon::Icon {
        winit::icon::Icon(self.0.clone())
    }
}

#[cfg(test)]
#[path = "tests/icons.rs"]
mod tests;
