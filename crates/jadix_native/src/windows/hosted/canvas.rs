//! Direct2D/DirectWrite implementation of the shared, validated canvas commands.
use super::*;
use jadix_core::hosted::CanvasCommand;
use windows::Win32::Graphics::{Direct2D::*, Direct2D::Common::*, DirectWrite::*, Dxgi::Common::*};
use windows_numerics::Vector2;

pub(super) struct Canvas {
    factory: ID2D1Factory,
    text: IDWriteFactory,
    target: Option<ID2D1HwndRenderTarget>,
}

impl Canvas {
    pub fn new() -> Result<Self, String> {
        unsafe {
            Ok(Self {
                factory: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).map_err(|e| e.to_string())?,
                text: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(|e| e.to_string())?,
                target: None,
            })
        }
    }

    pub fn paint(&mut self, window: HWND, commands: &[CanvasCommand], background: COLORREF) -> Result<(), String> {
        let mut bounds = RECT::default();
        unsafe { GetClientRect(window, &mut bounds); }
        if bounds.right <= 0 || bounds.bottom <= 0 { return Ok(()); }
        let size = D2D_SIZE_U { width: bounds.right as u32, height: bounds.bottom as u32 };
        let dpi = unsafe { GetDpiForWindow(window) }.max(96) as f32;
        // Device loss invalidates only the render target; the shared commands remain
        // available for an immediate retry and later exposure/resize paints.
        for attempt in 0..2 {
            let result = (|| -> windows::core::Result<()> { unsafe {
                if self.target.is_none() {
                    self.target = Some(self.factory.CreateHwndRenderTarget(
                        &D2D1_RENDER_TARGET_PROPERTIES {
                            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                            dpiX: dpi, dpiY: dpi, ..Default::default()
                        },
                        &D2D1_HWND_RENDER_TARGET_PROPERTIES { hwnd: windows::Win32::Foundation::HWND(window), pixelSize: size, ..Default::default() },
                    )?);
                }
                let target = self.target.as_ref().unwrap();
                target.Resize(&size)?;
                target.SetDpi(dpi, dpi);
                target.BeginDraw();
                target.Clear(Some(&rgba([background as u8, (background >> 8) as u8, (background >> 16) as u8, 255])));
                let draw = self.draw(target, commands);
                let end = target.EndDraw(None, None);
                draw.and(end)
            } })();
            match result {
                Ok(()) => return Ok(()),
                Err(error) if error.code() == windows::Win32::Foundation::D2DERR_RECREATE_TARGET => {
                    self.target = None;
                    if attempt == 1 { return Err(format!("canvas device lost: {error}")); }
                }
                Err(error) => return Err(format!("canvas drawing failed: {error}")),
            }
        }
        Ok(())
    }

    pub fn print(&self, window: HWND, dc: HDC, commands: &[CanvasCommand], background: COLORREF) -> Result<(), String> {
        let mut bounds = RECT::default();
        unsafe { GetClientRect(window, &mut bounds); }
        if bounds.right <= 0 || bounds.bottom <= 0 { return Ok(()); }
        let dpi = unsafe { GetDpiForWindow(window) }.max(96) as f32;
        unsafe {
            let target = self.factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: dpi, dpiY: dpi, ..Default::default()
            }).map_err(|e| e.to_string())?;
            target.BindDC(windows::Win32::Graphics::Gdi::HDC(dc), &windows::Win32::Foundation::RECT {
                left: bounds.left, top: bounds.top, right: bounds.right, bottom: bounds.bottom,
            }).map_err(|e| e.to_string())?;
            target.BeginDraw();
            target.Clear(Some(&rgba([background as u8, (background >> 8) as u8, (background >> 16) as u8, 255])));
            let draw = self.draw(&target, commands);
            let end = target.EndDraw(None, None);
            draw.and(end).map_err(|e| format!("canvas capture failed: {e}"))
        }
    }

    unsafe fn draw(&self, target: &ID2D1RenderTarget, commands: &[CanvasCommand]) -> windows::core::Result<()> {
        for command in commands { unsafe { match command {
            CanvasCommand::Fill { rect, color } => {
                let brush = target.CreateSolidColorBrush(&rgba(*color), None)?;
                target.FillRectangle(&D2D_RECT_F { left: rect[0] as f32, top: rect[1] as f32,
                    right: (rect[0] + rect[2]) as f32, bottom: (rect[1] + rect[3]) as f32 }, &brush);
            }
            CanvasCommand::Stroke { points, color, width, closed } if points.len() >= 2 && *width > 0.0 => {
                let path = self.factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(point(points[0]), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddLines(&points[1..].iter().copied().map(point).collect::<Vec<_>>());
                sink.EndFigure(if *closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
                sink.Close()?;
                let brush = target.CreateSolidColorBrush(&rgba(*color), None)?;
                target.DrawGeometry(&path, &brush, *width as f32, None::<&ID2D1StrokeStyle>);
            }
            CanvasCommand::Text { text, position, size, color } if !text.is_empty() => {
                let format = self.text.CreateTextFormat(windows::core::w!("Segoe UI"), None::<&IDWriteFontCollection>,
                    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, *size as f32, windows::core::w!(""))?;
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                let layout = self.text.CreateTextLayout(&text.encode_utf16().collect::<Vec<_>>(), &format, 1e7, 1e7)?;
                let brush = target.CreateSolidColorBrush(&rgba(*color), None)?;
                target.DrawTextLayout(point(*position), &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
            }
            _ => {}
        } } }
        Ok(())
    }
}

fn point(value: [f64; 2]) -> Vector2 { Vector2::new(value[0] as f32, value[1] as f32) }
fn rgba(value: [u8; 4]) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: value[0] as f32 / 255.0, g: value[1] as f32 / 255.0, b: value[2] as f32 / 255.0, a: value[3] as f32 / 255.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{Graphics::Imaging::*, System::Com::*};

    #[test]
    fn direct2d_composites_shared_canvas_alpha_paths_and_unicode() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            struct Apartment;
            impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
            let _apartment = Apartment;
            let canvas = Canvas::new().unwrap();
            let imaging: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let bitmap = imaging.CreateBitmap(240, 100, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).unwrap();
            let target = canvas.factory.CreateWicBitmapRenderTarget(&bitmap, &D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE, dpiX: 96.0, dpiY: 96.0, ..Default::default()
            }).unwrap();
            target.BeginDraw(); target.Clear(Some(&rgba([255; 4])));
            canvas.draw(&target, &[
                CanvasCommand::Fill { rect: [0.0, 0.0, 60.0, 60.0], color: [255, 0, 0, 128] },
                CanvasCommand::Stroke { points: vec![[70.0, 10.0], [110.0, 10.0], [110.0, 50.0]], color: [0, 0, 255, 255], width: 4.0, closed: false },
                CanvasCommand::Text { text: "中文🙂".into(), position: [125.0, 10.0], size: 24.0, color: [0, 0, 0, 255] },
            ]).unwrap();
            target.EndDraw(None, None).unwrap();
            let mut pixels = vec![0; 240 * 100 * 4]; bitmap.CopyPixels(null_mut(), 240 * 4, &mut pixels).unwrap();
            let pixel = |x: usize, y: usize| &pixels[(y * 240 + x) * 4..(y * 240 + x + 1) * 4];
            assert_eq!(pixel(20, 20), [127, 127, 255, 255]);
            assert_eq!(pixel(90, 10), [255, 0, 0, 255]);
            assert_eq!(pixel(230, 90), [255; 4]);
            assert!((10..60).any(|y| (125..225).any(|x| pixel(x, y) != [255; 4])), "DirectWrite must paint Unicode glyphs");
        }
    }
}
