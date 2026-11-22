use super::*;
use windows::{core::PCWSTR, Win32::{Foundation::HWND as Window, Graphics::{Direct2D::*, Direct2D::Common::*, DirectWrite::*, Dxgi::Common::*, Imaging::*}, System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER}, UI::{Shell::{ITaskbarList3, TaskbarList}, WindowsAndMessaging::HICON as IconHandle}}};
use windows_numerics::Vector2;

#[derive(Default)]
pub(super) struct Badge { label: RefCell<Option<String>> }

impl Badge {
    pub fn set(&self, window: HWND, label: Option<&str>) -> Result<(), String> {
        if label.is_some_and(|s| s.len() > 64 || s.contains('\0')) { return Err("invalid badge".into()); }
        // Explorer has no taskbar button for a not-yet-shown window. Keep the
        // intent until TaskbarButtonCreated, including clearing a pending badge.
        if unsafe { IsWindowVisible(window) } != 0 { self.publish(window, label)?; }
        *self.label.borrow_mut() = label.map(str::to_owned);
        Ok(())
    }

    pub fn restore(&self, window: HWND) -> Result<(), String> {
        let label = self.label.borrow().clone();
        if let Some(label) = label { self.publish(window, Some(&label)) } else { Ok(()) }
    }

    fn publish(&self, window: HWND, label: Option<&str>) -> Result<(), String> {
        unsafe {
            let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
            taskbar.HrInit().map_err(|e| e.to_string())?;
            let icon = label.filter(|s| !s.is_empty()).map(icon).transpose()?;
            let description = wide(label.unwrap_or(""));
            taskbar.SetOverlayIcon(Window(window), IconHandle(icon.as_ref().map_or(null_mut(), |icon| icon.0)), PCWSTR(description.as_ptr())).map_err(|e| e.to_string())
        }
    }
}

struct Icon(HICON);
impl Drop for Icon { fn drop(&mut self) { unsafe { DestroyIcon(self.0); } } }
struct Bitmap(HBITMAP);
impl Drop for Bitmap { fn drop(&mut self) { unsafe { DeleteObject(self.0); } } }

fn icon(label: &str) -> Result<Icon, String> {
    unsafe {
        let pixels = pixels(label).map_err(|e| e.to_string())?;
        let color = Bitmap(CreateBitmap(32, 32, 1, 32, pixels.as_ptr().cast()));
        let mask_data = [0u8; 128];
        let mask = Bitmap(CreateBitmap(32, 32, 1, 1, mask_data.as_ptr().cast()));
        if color.0.is_null() || mask.0.is_null() { return Err(native_error("create badge bitmap")); }
        let icon = CreateIconIndirect(&ICONINFO { fIcon: 1, hbmMask: mask.0, hbmColor: color.0, ..Default::default() });
        if icon.is_null() { Err(native_error("create badge icon")) } else { Ok(Icon(icon)) }
    }
}

fn pixels(label: &str) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let bitmap = imaging.CreateBitmap(32, 32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let target = factory.CreateWicBitmapRenderTarget(&bitmap, &D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96., dpiY: 96., ..Default::default()
        })?;
        let background = target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.8, g: 0.08, b: 0.12, a: 1. }, None)?;
        let foreground = target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1., g: 1., b: 1., a: 1. }, None)?;
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let format = write.CreateTextFormat(windows::core::w!("Segoe UI"), None, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 18., windows::core::w!(""))?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let text: Vec<u16> = label.encode_utf16().collect();
        let layout = write.CreateTextLayout(&text, &format, 28., 28.)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        if metrics.width > 28. { layout.SetFontSize((18. * 28. / metrics.width).max(7.), DWRITE_TEXT_RANGE { startPosition: 0, length: text.len() as u32 })?; }
        let ellipsis = write.CreateEllipsisTrimmingSign(&format)?;
        layout.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, ..Default::default() }, &ellipsis)?;
        target.BeginDraw();
        target.Clear(Some(&D2D1_COLOR_F::default()));
        target.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: 16., Y: 16. }, radiusX: 16., radiusY: 16. }, &background);
        target.DrawTextLayout(Vector2 { X: 2., Y: 2. }, &layout, &foreground, D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        target.EndDraw(None, None)?;
        let mut pixels = vec![0; 32 * 32 * 4];
        bitmap.CopyPixels(null_mut(), 32 * 4, &mut pixels)?;
        Ok(pixels)
    }
}

pub(super) fn taskbar_button_created() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarButtonCreated").as_ptr()) })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn badge_renders_alpha_and_unicode_to_owned_icon() {
        let _apartment = super::super::accessibility::Accessibility::new().unwrap();
        let plain = pixels("").unwrap(); let text = pixels("新9").unwrap();
        assert_eq!(&text[..4], &[0, 0, 0, 0]);
        assert_eq!(text[(16 * 32 + 16) * 4 + 3], 255);
        assert!(plain.iter().zip(&text).filter(|(a,b)| a != b).count() > 30);
        let icon = icon("99+").unwrap(); let mut info = ICONINFO::default();
        assert_ne!(unsafe { GetIconInfo(icon.0, &mut info) }, 0);
        let _color = Bitmap(info.hbmColor); let _mask = Bitmap(info.hbmMask);
        let mut bitmap = BITMAP::default();
        assert_ne!(unsafe { GetObjectW(info.hbmColor, std::mem::size_of::<BITMAP>() as i32, (&mut bitmap as *mut BITMAP).cast()) }, 0);
        assert_eq!((bitmap.bmWidth, bitmap.bmHeight, bitmap.bmBitsPixel), (32, 32, 32));
        let mut host = WindowsHost::new("Jadix pending badge", [320, 240]).unwrap();
        host.context.badge.set(host.window, Some("消息9")).unwrap();
        assert_eq!(host.context.badge.label.borrow().as_deref(), Some("消息9"));
        host.context.badge.set(host.window, None).unwrap();
        assert!(host.context.badge.label.borrow().is_none());
        host.dispose();
    }
}
