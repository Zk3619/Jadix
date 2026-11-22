use super::*;
use jadix_core::clipboard::{ClipboardItem, ClipboardRequest, ClipboardResponse, ClipboardResult, MAX_BYTES};
use windows_sys::Win32::System::{DataExchange::*, Memory::*};

const UNICODE: u32 = 13;
const PRIVATE: &str = "Jadix MIME ";
struct Clipboard;
impl Drop for Clipboard { fn drop(&mut self) { unsafe { CloseClipboard(); } } }
struct Memory(*mut c_void);
impl Drop for Memory { fn drop(&mut self) { if !self.0.is_null() { unsafe { GlobalFree(self.0); } } } }
impl Memory {
    fn new(bytes: &[u8]) -> Result<Self, String> {
        let handle = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1)) });
        if handle.0.is_null() { return Err(native_error("GlobalAlloc clipboard")); }
        let pointer = unsafe { GlobalLock(handle.0) };
        if pointer.is_null() { return Err(native_error("GlobalLock clipboard")); }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len()); GlobalUnlock(handle.0); }
        Ok(handle)
    }
}
fn format(name: &str) -> Result<u32, String> {
    let id = unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) };
    if id == 0 { Err(native_error("RegisterClipboardFormatW")) } else { Ok(id) }
}
fn bytes(id: u32) -> Result<Vec<u8>, String> {
    let handle = unsafe { GetClipboardData(id) };
    if handle.is_null() { return Err(native_error("GetClipboardData")); }
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > MAX_BYTES + 8 { return Err("clipboard allocation outside budget".into()); }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() { return Err(native_error("GlobalLock clipboard")); }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
    unsafe { GlobalUnlock(handle); }
    Ok(bytes)
}
fn read(mime: &str) -> Result<Option<ClipboardItem>, String> {
    let private = format(&format!("{PRIVATE}{mime}"))?;
    let data = if unsafe { IsClipboardFormatAvailable(private) } != 0 {
        let packet = bytes(private)?;
        let size = u64::from_le_bytes(packet.get(..8).ok_or("invalid MIME clipboard header")?.try_into().unwrap());
        let size = usize::try_from(size).map_err(|e| e.to_string())?;
        if size > MAX_BYTES { return Err("MIME clipboard exceeds budget".into()); }
        packet.get(8..8 + size).ok_or("truncated MIME clipboard")?.to_vec()
    } else {
        let id = match mime { "text/plain" => UNICODE, "text/html" => format("HTML Format")?, "image/png" => format("PNG")?, _ => return Ok(None) };
        if unsafe { IsClipboardFormatAvailable(id) } == 0 { return Ok(None); }
        let data = bytes(id)?;
        match mime {
            "text/plain" => {
                let units: Vec<u16> = data.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).take_while(|u| *u != 0).collect();
                String::from_utf16(&units).map_err(|e| e.to_string())?.into_bytes()
            }
            "text/html" => jadix_core::clipboard::html::decode(&data)?.as_bytes().to_vec(),
            _ => data,
        }
    };
    if data.len() > MAX_BYTES { return Err("clipboard representation exceeds budget".into()); }
    Ok(Some(ClipboardItem { mime: mime.into(), bytes: data }))
}

pub(super) fn request(owner: HWND, request: &ClipboardRequest) -> ClipboardResult {
    request.validate()?;
    // Allocate and validate every representation before changing OS ownership.
    let mut prepared = Vec::new();
    if let ClipboardRequest::Write { items } = request {
        for item in items {
            let mut packet = (item.bytes.len() as u64).to_le_bytes().to_vec();
            packet.extend_from_slice(&item.bytes);
            prepared.push((format(&format!("{PRIVATE}{}", item.mime))?, Memory::new(&packet)?));
            let standard = match item.mime.as_str() {
                "text/plain" => {
                    let text = std::str::from_utf8(&item.bytes).map_err(|e| e.to_string())?;
                    if text.contains('\0') { return Err("plain clipboard text contains NUL".into()); }
                    Some((UNICODE, wide(text).iter().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>()))
                }
                "text/html" => Some((format("HTML Format")?, jadix_core::clipboard::html::encode(std::str::from_utf8(&item.bytes).map_err(|e| e.to_string())?)?)),
                "image/png" => Some((format("PNG")?, item.bytes.clone())),
                _ => None,
            };
            if let Some((id, bytes)) = standard { prepared.push((id, Memory::new(&bytes)?)); }
        }
    }
    if unsafe { OpenClipboard(owner) } == 0 { return Err(native_error("OpenClipboard")); }
    let _clipboard = Clipboard;
    match request {
        ClipboardRequest::Write { .. } | ClipboardRequest::Clear => {
            if unsafe { EmptyClipboard() } == 0 { return Err(native_error("EmptyClipboard")); }
            for (id, mut memory) in prepared {
                if unsafe { SetClipboardData(id, memory.0) }.is_null() { return Err(native_error("SetClipboardData")); }
                memory.0 = null_mut(); // ownership transfers only on success
            }
            Ok(ClipboardResponse::Complete)
        }
        ClipboardRequest::Read { mime_types } => {
            for mime in mime_types { if let Some(item) = read(mime)? { return Ok(ClipboardResponse::Item(Some(item))); } }
            Ok(ClipboardResponse::Item(None))
        }
        ClipboardRequest::Formats => {
            let mut result = BTreeSet::new();
            let mut id = 0;
            for _ in 0..1024 {
                id = unsafe { EnumClipboardFormats(id) };
                if id == 0 { break; }
                if id == UNICODE { result.insert("text/plain".into()); }
                if id >= 0xc000 {
                    let mut name = [0u16; 512];
                    let size = unsafe { GetClipboardFormatNameW(id, name.as_mut_ptr(), name.len() as i32) };
                    let name = String::from_utf16_lossy(&name[..size.max(0) as usize]);
                    match name.as_str() {
                        "HTML Format" => { result.insert("text/html".into()); }
                        "PNG" => { result.insert("image/png".into()); }
                        name => if let Some(mime) = name.strip_prefix(PRIVATE) { result.insert(mime.into()); },
                    }
                }
            }
            Ok(ClipboardResponse::Formats(result.into_iter().collect()))
        }
    }
}
