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
mod tests {
    use super::*;

    #[test]
    fn native_icon_rejects_invalid_dimensions_and_bytes() {
        for (rgba, w, h) in [
            (vec![], 0, 0),
            (vec![0; 3], 1, 1),
            (vec![0; 8], 1, 1),
            (vec![], u32::MAX, u32::MAX),
        ] {
            assert!(AppIcon::from_rgba(rgba, w, h).is_err());
        }
        assert!(AppIcon::from_bytes(b"invalid PNG").is_err());
    }

    #[test]
    fn native_icon_decodes_and_reaches_both_entry_surfaces() {
        let rgba = image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 33, 55, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        rgba.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let icon = AppIcon::from_bytes(bytes.get_ref()).unwrap();
        assert_eq!((icon.width(), icon.height()), (2, 2));
        assert_eq!(&icon.rgba()[..4], &[17, 33, 55, 255]);
        let opts = crate::app::app().icon(icon.clone()).into_window_options();
        assert_eq!(opts.icon.as_ref(), Some(&icon));
        let attrs = opts.to_window_attributes();
        let native = attrs.window_icon.unwrap();
        assert_eq!(
            native.cast_ref::<winit::icon::RgbaIcon>().unwrap().buffer(),
            icon.rgba()
        );
        let shell = crate::components::App(crate::components::AppProps {
            icon: Some(icon.clone()),
            ..Default::default()
        });
        assert_eq!(shell.get_attr("data-slot").map(String::as_str), Some("app"));
    }
}
