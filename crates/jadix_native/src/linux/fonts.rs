//! Private Fontconfig configuration; app fonts never enter the process default.
use super::*;
use glib::translate::ToGlibPtr;
use jadix_core::fonts::{FontCollection, FontData};
use std::{
    ffi::{c_char, c_int, c_void, CString},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::PathBuf,
};

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcInitLoadConfigAndFonts() -> *mut c_void;
    fn FcConfigDestroy(config: *mut c_void);
    fn FcConfigAppFontAddFile(config: *mut c_void, file: *const c_char) -> c_int;
}
#[link(name = "pangoft2-1.0")]
unsafe extern "C" {
    fn pango_fc_font_map_get_type() -> usize;
    fn pango_fc_font_map_set_config(map: *mut c_void, config: *mut c_void);
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        // This exclusively created private directory contains only our font files.
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            log::debug!("font cache cleanup: {error}");
        }
    }
}
pub(super) struct Fonts {
    pub map: gtk::pango::FontMap,
    fonts: RefCell<FontCollection>,
    files: RefCell<BTreeMap<[u8; 32], PathBuf>>,
    directory: Rc<Directory>,
}
impl Fonts {
    pub fn new() -> Result<Rc<Self>, String> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "jadix-fonts-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|error| error.to_string())?;
        let directory = Rc::new(Directory(path));
        let map = pangocairo::FontMap::new();
        set_files(&map, &[])?;
        // SAFETY: GObject owns the Rc until the last font-map reference is gone.
        // Widgets retained by an embedding caller therefore retain their files.
        unsafe {
            map.set_data("jadix-font-directory", directory.clone());
        }
        Ok(Rc::new(Self {
            map,
            fonts: Default::default(),
            files: Default::default(),
            directory,
        }))
    }
    pub fn revision(&self) -> u64 {
        self.fonts.borrow().revision()
    }
    pub fn register(&self, bytes: &[u8]) -> Result<Vec<String>, String> {
        use std::io::Write;
        let font = FontData::parse(bytes)?;
        let families = font.families().to_vec();
        let mut staged = self.fonts.borrow().clone();
        if !staged.insert(font.clone())? {
            return Ok(families);
        }
        let name: String = font
            .digest()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect();
        let path = self.directory.0.join(format!("{name}.ttf"));
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(font.bytes())?;
            file.flush()
        };
        if let Err(error) = write() {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
        let mut files: Vec<_> = self.files.borrow().values().cloned().collect();
        files.push(path.clone());
        if let Err(error) = set_files(&self.map, &files) {
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        self.files.borrow_mut().insert(*font.digest(), path);
        *self.fonts.borrow_mut() = staged;
        Ok(families)
    }
}
fn set_files(map: &gtk::pango::FontMap, files: &[PathBuf]) -> Result<(), String> {
    let pointer: *mut gtk::pango::ffi::PangoFontMap = map.to_glib_none().0;
    // SAFETY: pointer is a live GObject. Only the checked PangoFc subclass is cast.
    if unsafe {
        glib::gobject_ffi::g_type_check_instance_is_a(pointer.cast(), pango_fc_font_map_get_type())
    } == 0
    {
        return Err("Pango font map does not support Fontconfig".into());
    }
    struct Config(*mut c_void);
    impl Drop for Config {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    FcConfigDestroy(self.0);
                }
            }
        }
    }
    // SAFETY: creates a new owned configuration, without mutating the current one.
    let config = Config(unsafe { FcInitLoadConfigAndFonts() });
    if config.0.is_null() {
        return Err("cannot load Fontconfig configuration".into());
    }
    for file in files {
        let path = CString::new(file.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        // SAFETY: the config is private and the path lives through the call.
        if unsafe { FcConfigAppFontAddFile(config.0, path.as_ptr()) } == 0 {
            return Err("Fontconfig rejected font".into());
        }
    }
    // SAFETY: checked subclass; Pango takes its own configuration reference.
    unsafe {
        pango_fc_font_map_set_config(pointer.cast(), config.0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "fontconfig")]
    unsafe extern "C" {
        fn FcConfigGetCurrent() -> *mut c_void;
    }

    #[test]
    fn registration_is_private_deduplicated_and_files_follow_the_font_map() {
        let current = unsafe { FcConfigGetCurrent() };
        let fonts = Fonts::new().unwrap();
        let bytes = include_bytes!("../../../jadix_cosmic_text/fonts/NotoSansHebrew.ttf");
        let families = fonts.register(bytes).unwrap();
        assert!(!families.is_empty());
        assert!(fonts
            .map
            .list_families()
            .iter()
            .any(|family| families.iter().any(|name| family.name() == name.as_str())));
        assert_eq!(fonts.revision(), 1);
        assert_eq!(fonts.register(bytes).unwrap(), families);
        assert_eq!(fonts.revision(), 1);
        assert!(fonts.register(b"invalid font").is_err());
        assert_eq!(fonts.revision(), 1);
        assert_eq!(unsafe { FcConfigGetCurrent() }, current);
        let directory = fonts.directory.0.clone();
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        let map = fonts.map.clone();
        drop(fonts);
        assert!(directory.exists());
        drop(map);
        assert!(!directory.exists());
    }
}
