use super::*;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};

pub(super) struct Paint {
    pub foreground: COLORREF,
    pub background: COLORREF,
    pub brush: HBRUSH,
}

impl Paint {
    fn new(foreground: COLORREF, background: COLORREF) -> Result<Self, String> {
        let brush = unsafe { CreateSolidBrush(background) };
        if brush.is_null() {
            return Err(native_error("create native palette brush"));
        }
        Ok(Self {
            foreground,
            background,
            brush,
        })
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.brush) };
    }
}

pub(super) fn high_contrast() -> bool {
    let mut settings = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    (unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            (&mut settings as *mut HIGHCONTRASTW).cast(),
            0,
        )
    }) != 0
        && settings.dwFlags & HCF_HIGHCONTRASTON != 0
}

fn blend(color: [u8; 4], background: COLORREF) -> COLORREF {
    let alpha = u32::from(color[3]);
    (0..3).fold(0, |result, channel| {
        let base = (background >> (channel * 8)) & 255;
        let blended = (u32::from(color[channel]) * alpha + base * (255 - alpha) + 127) / 255;
        result | (blended << (channel * 8))
    })
}

impl WindowsHost {
    pub(super) fn sync_paints(&mut self) -> Result<(), String> {
        if !self.context.palette_dirty.replace(false) {
            return Ok(());
        }
        let high_contrast = self.context.high_contrast.get();
        let mut pending: Vec<_> = self.controls.keys().copied().collect();
        pending.sort_by_key(|identifier| {
            let mut depth = 0;
            let mut parent = self.controls[identifier].parent;
            while let Some(identifier) = parent {
                depth += 1;
                parent = self.controls[&identifier].parent;
            }
            depth
        });
        let mut changed = Vec::new();
        for identifier in pending {
            let control = &self.controls[&identifier];
            let parent_color = control
                .parent
                .and_then(|parent| {
                    self.context
                        .paints
                        .borrow()
                        .get(&(self.controls[&parent].window as usize))
                        .map(|paint| paint.background)
                })
                .unwrap_or_else(|| unsafe { GetSysColor(COLOR_WINDOW) });
            let background = if high_contrast {
                unsafe { GetSysColor(COLOR_WINDOW) }
            } else {
                blend(control.descriptor.style.background, parent_color)
            };
            let foreground = if high_contrast {
                unsafe { GetSysColor(COLOR_WINDOWTEXT) }
            } else {
                blend(control.descriptor.style.foreground, background)
            };
            let unchanged = self
                .context
                .paints
                .borrow()
                .get(&(control.window as usize))
                .is_some_and(|paint| {
                    paint.background == background && paint.foreground == foreground
                });
            if !unchanged {
                self.context
                    .paints
                    .borrow_mut()
                    .insert(control.window as usize, Paint::new(foreground, background)?);
                changed.push(control.window);
            }
        }
        for window in changed {
            unsafe { InvalidateRect(window, null_mut(), 1) };
        }
        Ok(())
    }
}
