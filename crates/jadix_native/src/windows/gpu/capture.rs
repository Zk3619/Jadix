//! Bounded client-area capture. Rendering never reparents or hides input HWNDs.
use super::*;
use super::super::image::Pixels;

pub(super) struct Output(pub HWND, RefCell<BTreeSet<usize>>, Vec<u16>);
impl Output {
    pub fn new(parent: HWND) -> Result<Self, String> {
        // A separate child surface can cover native children without taking their
        // focus, accessibility identity, mouse capture or IME ownership.
        let window = unsafe { CreateWindowExW(WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            wide("STATIC").as_ptr(), wide("").as_ptr(), WS_CHILD | WS_VISIBLE,
            0, 0, 1, 1, parent, null_mut(), GetModuleHandleW(null_mut()), null_mut()) };
        if window.is_null() { return Err(native_error("create shader output")); }
        let output = Self(window, RefCell::new(BTreeSet::new()), wide(&format!("Jadix.Shader.Observer.{}", window as usize)));
        if unsafe { SetPropW(window, windows::core::w!("Jadix.Shader.Output").as_ptr(), parent) } == 0 {
            return Err(native_error("mark shader output"));
        }
        if unsafe { SetWindowSubclass(window, Some(procedure), 0x4a44534f, parent as usize) } == 0 {
            return Err(native_error("subclass shader output"));
        }
        Ok(output)
    }
    fn watch(&self, window: HWND) -> Result<(), String> {
        // HWND values can be reused after destruction. The subclass, rather
        // than the cached handle, proves that this generation is observed.
        if unsafe { GetPropW(window, self.2.as_ptr()) }.is_null() {
            let mut watched = self.1.borrow_mut();
            if watched.len() >= 4096 {
                watched.retain(|handle| !unsafe { GetPropW(*handle as HWND, self.2.as_ptr()) }.is_null());
                if watched.len() >= 8192 { return Err("shader repaint observer budget exceeded".into()); }
            }
            if unsafe { SetWindowSubclass(window, Some(input_changed), self.0 as usize, self.0 as usize) } == 0 {
                return Err(native_error("observe shader input repaint"));
            }
            if unsafe { SetPropW(window, self.2.as_ptr(), self.0) } == 0 {
                unsafe { RemoveWindowSubclass(window, Some(input_changed), self.0 as usize); }
                return Err(native_error("mark shader input observer"));
            }
            watched.insert(window as usize);
        }
        Ok(())
    }
    pub fn position(&self, parent: HWND, foreground: bool) -> Result<(), String> {
        let mut rect = RECT::default();
        unsafe { GetClientRect(parent, &mut rect); }
        if unsafe { SetWindowPos(self.0, if foreground { HWND_TOP } else { HWND_BOTTOM }, 0, 0,
            rect.right.max(1), rect.bottom.max(1), SWP_NOACTIVATE | SWP_NOREDRAW | SWP_SHOWWINDOW) } == 0 {
            return Err(native_error("position shader output"));
        }
        Ok(())
    }
}
fn is_output(window: HWND) -> bool {
    !window.is_null() && !unsafe { GetPropW(window, windows::core::w!("Jadix.Shader.Output").as_ptr()) }.is_null()
}
impl Drop for Output {
    fn drop(&mut self) {
        for window in self.1.get_mut().iter() { unsafe { RemoveWindowSubclass(*window as HWND, Some(input_changed), self.0 as usize); RemovePropW(*window as HWND, self.2.as_ptr()); } }
        unsafe { RemovePropW(self.0, windows::core::w!("Jadix.Shader.Output").as_ptr()); }
        unsafe { DestroyWindow(self.0); }
    }
}
unsafe extern "system" fn input_changed(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, id: usize, output: usize) -> LRESULT {
    let result = unsafe { DefSubclassProc(window, message, parameter, data) };
    if message == WM_NCDESTROY { unsafe { RemoveWindowSubclass(window, Some(input_changed), id); } }
    if matches!(message, WM_PAINT | WM_SETTEXT | WM_SIZE | WM_WINDOWPOSCHANGED | WM_SHOWWINDOW | WM_SETFOCUS | WM_KILLFOCUS
        | WM_HSCROLL | WM_VSCROLL | WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION) {
        let parent = unsafe { GetParent(output as HWND) };
        if !parent.is_null() { unsafe { InvalidateRect(parent, null_mut(), 0); } }
    }
    result
}
unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, parent: usize) -> LRESULT {
    match message {
        WM_NCHITTEST => return HTTRANSPARENT as LRESULT,
        WM_GETOBJECT => return 0,
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            unsafe { BeginPaint(window, &mut paint); EndPaint(window, &paint); InvalidateRect(parent as HWND, null_mut(), 0); }
            return 0;
        }
        WM_NCDESTROY => { unsafe { RemoveWindowSubclass(window, Some(procedure), 0x4a44534f); } }
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}

struct Bitmap { dc: HDC, bitmap: HBITMAP, previous: HGDIOBJ, bits: *mut u8, size: [i32; 2] }
impl Bitmap {
    fn new(size: [i32; 2]) -> Result<Self, String> {
        if size.iter().any(|v| !(1..=4096).contains(v)) || i64::from(size[0]) * i64::from(size[1]) > 4_194_304 {
            return Err("shader capture exceeds the 4096-pixel / 4-megapixel budget".into());
        }
        let dc = unsafe { CreateCompatibleDC(null_mut()) };
        if dc.is_null() { return Err(native_error("create shader capture DC")); }
        let mut header = BITMAPINFO::default();
        header.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size[0], biHeight: -size[1], biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..Default::default() };
        let mut bits = null_mut();
        let bitmap = unsafe { CreateDIBSection(dc, &header, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
        if bitmap.is_null() { unsafe { DeleteDC(dc); } return Err(native_error("create shader capture bitmap")); }
        let previous = unsafe { SelectObject(dc, bitmap) };
        if previous.is_null() || previous as isize == -1 {
            unsafe { DeleteObject(bitmap); DeleteDC(dc); }
            return Err(native_error("select shader capture bitmap"));
        }
        unsafe { std::ptr::write_bytes(bits, 0, size[0] as usize * size[1] as usize * 4); }
        Ok(Self { dc, bitmap, previous, bits: bits.cast(), size })
    }
    fn pixels(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.bits, self.size[0] as usize * self.size[1] as usize * 4) } }
    fn opaque(&mut self, rect: RECT) {
        for y in rect.top.max(0)..rect.bottom.min(self.size[1]) {
            for x in rect.left.max(0)..rect.right.min(self.size[0]) {
                unsafe { *self.bits.add((y as usize * self.size[0] as usize + x as usize) * 4 + 3) = 255; }
            }
        }
    }
}
impl Drop for Bitmap {
    fn drop(&mut self) { unsafe { SelectObject(self.dc, self.previous); DeleteObject(self.bitmap); DeleteDC(self.dc); } }
}

struct Capture<'a> { bitmap: Bitmap, origin: POINT, output: &'a Output, stop: HWND, examined: usize }
impl Capture<'_> {
    fn paint(&mut self, window: HWND, depth: usize) -> Result<bool, String> {
        if window == self.stop { return Ok(false); }
        if is_output(window) || unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & WS_VISIBLE == 0 { return Ok(true); }
        self.output.watch(window)?;
        self.examined += 1;
        if depth > 128 || self.examined > 4096 { return Err("shader native capture tree budget exceeded".into()); }
        let mut rect = RECT::default(); let mut point = POINT::default();
        unsafe { GetClientRect(window, &mut rect); ClientToScreen(window, &mut point); }
        let x = point.x - self.origin.x; let y = point.y - self.origin.y;
        let bounds = RECT { left: x, top: y, right: x + rect.right, bottom: y + rect.bottom };
        if bounds.right <= 0 || bounds.bottom <= 0 || bounds.left >= self.bitmap.size[0] || bounds.top >= self.bitmap.size[1] { return Ok(true); }
        let saved = unsafe { SaveDC(self.bitmap.dc) };
        if saved == 0 { return Err(native_error("save shader capture DC")); }
        let ancestor = !self.stop.is_null() && unsafe { IsChild(window, self.stop) } != 0;
        let composed = !ancestor && is_output(unsafe { GetWindow(window, GW_CHILD) });
        unsafe {
            IntersectClipRect(self.bitmap.dc, bounds.left, bounds.top, bounds.right, bounds.bottom);
            SetViewportOrgEx(self.bitmap.dc, x, y, null_mut());
            SendMessageW(window, WM_ERASEBKGND, self.bitmap.dc as usize, 0);
            // A shader ancestor is currently being composed. Only its native
            // underlay belongs in this backdrop, never the previous GPU frame.
            if !ancestor || !composed_parent(window) {
                SendMessageW(window, WM_PRINTCLIENT, self.bitmap.dc as usize, (PRF_CLIENT | PRF_ERASEBKGND) as isize);
            }
            SetViewportOrgEx(self.bitmap.dc, 0, 0, null_mut());
        }
        // GDI controls render RGB, leaving the DIB alpha channel unspecified.
        let mut clip = RECT::default();
        unsafe { GdiFlush(); GetClipBox(self.bitmap.dc, &mut clip); }
        self.bitmap.opaque(clip);
        let children = if composed { Ok(true) } else { self.children(window, depth + 1) };
        unsafe { RestoreDC(self.bitmap.dc, saved); }
        children
    }
    fn children(&mut self, window: HWND, depth: usize) -> Result<bool, String> {
        let mut child = unsafe { GetWindow(window, GW_CHILD) };
        if !child.is_null() { child = unsafe { GetWindow(child, GW_HWNDLAST) }; }
        while !child.is_null() {
            if !self.paint(child, depth)? { return Ok(false); }
            child = unsafe { GetWindow(child, GW_HWNDPREV) };
        }
        Ok(true)
    }
}
fn composed_parent(window: HWND) -> bool {
    let mut child = unsafe { GetWindow(window, GW_CHILD) };
    while !child.is_null() {
        if is_output(child) { return true; }
        child = unsafe { GetWindow(child, GW_HWNDNEXT) };
    }
    false
}

pub(super) fn capture(parent: HWND, output: &Output, root: HWND, padding: i32, backdrop: bool) -> Result<Pixels, String> {
    let mut rect = RECT::default(); let mut origin = POINT::default();
    unsafe { GetClientRect(parent, &mut rect); ClientToScreen(parent, &mut origin); }
    origin.x -= padding; origin.y -= padding;
    let mut capture = Capture { bitmap: Bitmap::new([rect.right.max(1) + 2 * padding, rect.bottom.max(1) + 2 * padding])?, origin,
        output, stop: if backdrop { parent } else { null_mut() }, examined: 0 };
    if backdrop {
        // Stop at the effect in bottom-to-top paint order; later siblings can
        // never enter their own backdrop. Ancestor clipping stays in force.
        capture.paint(root, 0)?;
    } else { capture.children(parent, 0)?; }
    unsafe { GdiFlush(); }
    Ok(Pixels { width: capture.bitmap.size[0] as u32, height: capture.bitmap.size[1] as u32, bytes: capture.bitmap.pixels().to_vec() })
}
