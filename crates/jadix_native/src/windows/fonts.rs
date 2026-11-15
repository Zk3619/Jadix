use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_FONTS: usize = 8;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn resolve_family(requested: &str) -> String {
    if requested != "system-ui" {
        return requested.into();
    }
    let mut metrics = NONCLIENTMETRICSW::default();
    metrics.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
    if unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            std::ptr::addr_of_mut!(metrics).cast(),
            0,
        )
    } != 0
    {
        let name = metrics.lfMessageFont.lfFaceName;
        let length = name
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(name.len());
        if let Ok(family) = String::from_utf16(&name[..length]) {
            if !family.is_empty() {
                return family;
            }
        }
    }
    "Segoe UI".into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontRegistration {
    pub id: FontId,
    pub families: Vec<String>,
    pub faces: u32,
    pub bytes: usize,
}

struct Loaded {
    handle: HANDLE,
    bytes: Vec<u8>,
    registration: FontRegistration,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        unsafe {
            RemoveFontMemResourceEx(self.handle);
        }
    }
}

#[derive(Default)]
pub(super) struct FontPool {
    fonts: BTreeMap<FontId, Loaded>,
    bytes: usize,
}

impl FontPool {
    fn install(&mut self, bytes: &[u8]) -> Result<(FontRegistration, bool), String> {
        if let Some(font) = self.fonts.values().find(|font| font.bytes == bytes) {
            return Ok((font.registration.clone(), false));
        }
        if self.fonts.len() >= MAX_FONTS
            || bytes.is_empty()
            || bytes.len() > 4 * 1024 * 1024
            || self.bytes.saturating_add(bytes.len()) > MAX_BYTES
        {
            return Err("native font resource budget exceeded".into());
        }
        let faces = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
        if faces == 0 || faces > 32 {
            return Err("native font collection face limit".into());
        }
        let mut families = BTreeSet::new();
        let mut name_records = 0usize;
        for index in 0..faces {
            let face = ttf_parser::Face::parse(bytes, index)
                .map_err(|error| format!("invalid native font: {error:?}"))?;
            let names = face.names();
            name_records += usize::from(names.len());
            if name_records > 4096 {
                return Err("native font metadata record limit".into());
            }
            for name in names {
                if name.platform_id == ttf_parser::PlatformId::Windows
                    && name.name_id == ttf_parser::name_id::FAMILY
                    && name.name.len() <= 62
                {
                    if let Some(name) = name.to_string().filter(|name| {
                        !name.is_empty() && !name.contains('\0') && name.encode_utf16().count() < 32
                    }) {
                        families.insert(name);
                    }
                }
            }
        }
        if families.is_empty() || families.len() > 64 {
            return Err("native font has no supported GDI family name".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = FontId(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| "native font identity exhausted")?,
        );
        let mut count = 0u32;
        let handle = unsafe {
            AddFontMemResourceEx(
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                null_mut(),
                std::ptr::addr_of_mut!(count),
            )
        };
        if handle.is_null() {
            return Err(native_error("register private font"));
        }
        let registration = FontRegistration {
            id,
            families: families.into_iter().collect(),
            faces: count,
            bytes: bytes.len(),
        };
        let loaded = Loaded {
            handle,
            bytes: bytes.to_vec(),
            registration: registration.clone(),
        };
        if count == 0 {
            return Err("GDI registered no native font faces".into());
        }
        self.bytes += bytes.len();
        self.fonts.insert(id, loaded);
        Ok((registration, true))
    }

    pub(super) fn clear(&mut self) {
        self.fonts.clear();
        self.bytes = 0;
    }
}

impl WindowsHost {
    pub fn install_font(&mut self, bytes: &[u8]) -> Result<FontRegistration, String> {
        if self.is_closed() {
            return Err("native font root is closed".into());
        }
        let environment = self
            .context
            .environment
            .get()
            .checked_add(1)
            .ok_or("native font environment exhausted")?;
        let (registration, changed) = self.fonts.install(bytes)?;
        if changed {
            self.context.environment.set(environment);
            self.fonts_dirty = true;
        }
        Ok(registration)
    }

    pub fn remove_font(&mut self, id: FontId) -> Result<(), String> {
        let font = self
            .fonts
            .fonts
            .get(&id)
            .ok_or("unknown native font lease")?;
        if self.controls.values().any(|control| {
            let requested = resolve_family(&control.descriptor.style.font_family);
            font.registration
                .families
                .iter()
                .any(|family| family.to_lowercase() == requested.to_lowercase())
        }) {
            return Err("native font is still referenced by mounted controls".into());
        }
        let environment = self
            .context
            .environment
            .get()
            .checked_add(1)
            .ok_or("native font environment exhausted")?;
        let font = self
            .fonts
            .fonts
            .remove(&id)
            .ok_or("unknown native font lease")?;
        self.fonts.bytes -= font.bytes.len();
        drop(font);
        self.context.environment.set(environment);
        self.fonts_dirty = true;
        Ok(())
    }

    pub fn font_resources(&self) -> serde_json::Value {
        serde_json::json!({"registrations":self.fonts.fonts.len(),"source_bytes":self.fonts.bytes,"max_registrations":MAX_FONTS,"max_source_bytes":MAX_BYTES,"process_private":true})
    }

    pub fn font_diagnostics(&self, node: NodeId) -> Option<serde_json::Value> {
        let control = self.controls.get(&node)?;
        let _dpi = DpiScope::window(self.window);
        unsafe {
            let font = SendMessageW(control.window, WM_GETFONT, 0, 0) as HFONT;
            if font.is_null() {
                return None;
            }
            let device = GetDC(control.window);
            if device.is_null() {
                return None;
            }
            let previous = SelectObject(device, font);
            let mut name = [0u16; 128];
            let length = GetTextFaceW(device, name.len() as i32, name.as_mut_ptr());
            let mut metrics = TEXTMETRICW::default();
            let measured = GetTextMetricsW(device, &mut metrics);
            SelectObject(device, previous);
            ReleaseDC(control.window, device);
            if length <= 0 || measured == 0 {
                return None;
            }
            let length = name
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(name.len());
            let family = String::from_utf16(&name[..length]).ok()?;
            Some(
                serde_json::json!({"face":family,"height":metrics.tmHeight,"ascent":metrics.tmAscent,"average_width":metrics.tmAveCharWidth}),
            )
        }
    }
}
