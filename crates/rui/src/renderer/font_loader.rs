//! Font discovery and loading for the text renderer.
//!
//! **Critical:** do **not** call [`glyphon::FontSystem::new`] — it runs
//! `fontdb::Database::load_system_fonts()`, which on Windows indexes hundreds of
//! faces under `C:\Windows\Fonts` (~600MB+) and routinely pushes RSS past 200MB.
//!
//! Strategy (aligned with EUI-NEO `loadSharedFontStack`):
//! 1. Startup: only UI sans (+ weights) + lucide + optional env/feature embeds.
//! 2. CJK / symbols / Arabic extras stay as **lazy fallback paths**. Color emoji
//!    stays on an **isolated** FontSystem ([`ensure_emoji_font_fallback`]) because
//!    emoji always shapes through alpha=0 slots — CBDT in the UI DB would leak.
//! 3. Before shaping a string, [`ensure_coverage_for_text`] opens only the faces
//!    required by the codepoints actually present.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
#[cfg(any(
    feature = "bundled-lucide",
    feature = "bundled-cjk",
    feature = "bundled-cjk-serif",
    feature = "bundled-emoji",
    feature = "bundled-symbols"
))]
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use glyphon::fontdb::Database;
#[cfg(any(
    feature = "bundled-lucide",
    feature = "bundled-cjk",
    feature = "bundled-cjk-serif",
    feature = "bundled-emoji",
    feature = "bundled-symbols"
))]
use glyphon::fontdb::Source;
use glyphon::FontSystem;

use crate::renderer::emoji_detect;

/// Filenames we may load from a font search directory.
const RUNTIME_CJK_SANS: &str = "NotoSansCJK.ttc";
const RUNTIME_CJK_SERIF: &str = "NotoSerifCJK.ttc";
#[cfg(not(feature = "bundled-lucide"))]
const RUNTIME_LUCIDE: &str = "lucide.ttf";
const RUNTIME_INTER: &[&str] = &[
    "Inter-Variable.ttf",
    "InterVariable.ttf",
    "Inter-Regular.ttf",
    "Inter-Medium.ttf",
    "Inter-SemiBold.ttf",
    "Inter-Bold.ttf",
    "Inter.ttf",
];
const RUNTIME_SYMBOLS: &[&str] = &[
    "NotoSansSymbols-wght.ttf",
    "NotoSansSymbols2-Regular.ttf",
    "NotoSansMath-Regular.ttf",
    "NotoMusic-Regular.ttf",
];
const RUNTIME_EMOJI: &[&str] = &["NotoColorEmoji_WindowsCompatible.ttf", "NotoColorEmoji.ttf"];

/// Script / coverage buckets that may trigger a lazy font open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScriptNeeds {
    pub cjk: bool,
    /// Hangul (Korean) — needs Malgun / Apple SD Gothic, not YaHei alone.
    pub hangul: bool,
    /// Thai script.
    pub thai: bool,
    pub emoji: bool,
    pub symbols: bool,
    pub arabic: bool,
    pub devanagari: bool,
}

impl ScriptNeeds {
    pub fn is_empty(self) -> bool {
        !(self.cjk
            || self.hangul
            || self.thai
            || self.emoji
            || self.symbols
            || self.arabic
            || self.devanagari)
    }

    pub fn merge(&mut self, other: Self) {
        self.cjk |= other.cjk;
        self.hangul |= other.hangul;
        self.thai |= other.thai;
        self.emoji |= other.emoji;
        self.symbols |= other.symbols;
        self.arabic |= other.arabic;
        self.devanagari |= other.devanagari;
    }

    /// Clear bits already attempted (do not retry failed loads every frame).
    fn minus_attempted(self, attempted: Self) -> Self {
        Self {
            cjk: self.cjk && !attempted.cjk,
            hangul: self.hangul && !attempted.hangul,
            thai: self.thai && !attempted.thai,
            emoji: self.emoji && !attempted.emoji,
            symbols: self.symbols && !attempted.symbols,
            arabic: self.arabic && !attempted.arabic,
            devanagari: self.devanagari && !attempted.devanagari,
        }
    }
}

#[derive(Default)]
struct LazyFontState {
    /// Scripts we have already *tried* to open globally (avoid repeated failed FS scans).
    attempted: ScriptNeeds,
    /// Paths that opened successfully at least once (diagnostic / mmap hint).
    loaded_paths: HashSet<String>,
}

fn family_present_any(db: &Database, names: &[&str]) -> bool {
    names.iter().any(|n| family_present(db, n))
}

fn cjk_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Noto Sans CJK KR",
            "Noto Sans CJK TC",
            "Microsoft YaHei UI",
            "Microsoft YaHei",
            "PingFang SC",
            "Hiragino Sans GB",
            "Source Han Sans SC",
            "WenQuanYi Micro Hei",
        ],
    )
}

fn hangul_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Malgun Gothic",
            "Apple SD Gothic Neo",
            "Noto Sans CJK KR",
            "NanumGothic",
        ],
    )
}

fn thai_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Leelawadee UI",
            "Leelawadee",
            "Tahoma",
            "Thonburi",
            "Noto Sans Thai",
        ],
    )
}

fn arabic_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Segoe UI",
            "Arial",
            "Traditional Arabic",
            "Geeza Pro",
            "Noto Naskh Arabic",
        ],
    )
}

fn devanagari_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Nirmala UI",
            "Mangal",
            "Noto Sans Devanagari",
            "Kohinoor Devanagari",
        ],
    )
}

fn symbols_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Segoe UI Symbol",
            "Apple Symbols",
            "Noto Sans Symbols",
            "Noto Sans Symbols 2",
            "Noto Sans Math",
        ],
    )
}

fn emoji_coverage_present(db: &Database) -> bool {
    family_present_any(
        db,
        &[
            "Noto Color Emoji",
            "Noto Color Emoji Windows Compatible",
            "Segoe UI Emoji",
            "Apple Color Emoji",
        ],
    )
}

fn lazy_state() -> std::sync::MutexGuard<'static, LazyFontState> {
    static LAZY_STATE: OnceLock<Mutex<LazyFontState>> = OnceLock::new();
    LAZY_STATE
        .get_or_init(|| Mutex::new(LazyFontState::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
#[cfg(any(
    feature = "bundled-lucide",
    feature = "bundled-cjk",
    feature = "bundled-cjk-serif",
    feature = "bundled-emoji",
    feature = "bundled-symbols"
))]
fn load_static_bytes(db: &mut Database, bytes: &'static [u8]) {
    db.load_font_source(Source::Binary(Arc::new(bytes)));
}

fn font_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(dir) = std::env::var("RUI_FONT_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            dirs.push(p);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("assets"));
            dirs.push(parent.join("fonts"));
            dirs.push(parent.to_path_buf());
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("assets"));
        dirs.push(cwd.join("crates/rui/assets"));
        dirs.push(cwd.join("fonts"));
    }

    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));

    dirs.sort();
    dirs.dedup();
    dirs
}

fn find_font_file(file_name: &str) -> Option<PathBuf> {
    for dir in font_search_dirs() {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn db_has_font_path(db: &Database, path: &Path) -> bool {
    db.faces()
        .any(|face| matches!(&face.source, glyphon::fontdb::Source::File(p) if p.as_path() == path))
}

fn try_load_font_path(db: &mut Database, path: &Path, state: &mut LazyFontState) -> bool {
    let key = path.to_string_lossy().to_string();
    if db_has_font_path(db, path) {
        state.loaded_paths.insert(key);
        return true;
    }
    match db.load_font_file(path) {
        Ok(()) => {
            log::info!("loaded font file: {}", path.display());
            state.loaded_paths.insert(key);
            true
        }
        Err(err) => {
            log::warn!("failed to load font {}: {err}", path.display());
            false
        }
    }
}

fn try_load_font_file(db: &mut Database, file_name: &str, state: &mut LazyFontState) -> bool {
    let Some(path) = find_font_file(file_name) else {
        return false;
    };
    try_load_font_path(db, &path, state)
}

fn try_load_system_font(db: &mut Database, file_name: &str, state: &mut LazyFontState) -> bool {
    #[cfg(target_os = "windows")]
    {
        let path = PathBuf::from(r"C:\Windows\Fonts").join(file_name);
        if path.is_file() {
            return try_load_font_path(db, &path, state);
        }
    }
    #[cfg(target_os = "macos")]
    {
        for dir in [
            "/System/Library/Fonts",
            "/System/Library/Fonts/Supplemental",
            "/Library/Fonts",
        ] {
            let path = PathBuf::from(dir).join(file_name);
            if path.is_file() {
                return try_load_font_path(db, &path, state);
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for dir in [
            "/usr/share/fonts",
            "/usr/local/share/fonts",
            "/usr/share/fonts/truetype",
            "/usr/share/fonts/opentype",
        ] {
            let path = PathBuf::from(dir).join(file_name);
            if path.is_file() {
                return try_load_font_path(db, &path, state);
            }
        }
    }
    let _ = (db, file_name, state);
    false
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let v = v.trim();
            !(v.is_empty()
                || v == "0"
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("off"))
        })
        .unwrap_or(false)
}

fn env_flag_disabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            let v = v.trim();
            v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false)
}

fn family_present(db: &Database, name: &str) -> bool {
    db.faces()
        .any(|face| face.families.iter().any(|(f, _)| f == name))
}

/// Pick default sans/serif family names after fonts are loaded.
pub fn apply_default_families(db: &mut Database) {
    let sans = crate::utils::font::resolved_default_sans_family(db);
    if family_present(db, sans) {
        db.set_sans_serif_family(sans);
        log::debug!("default sans-serif family: {sans}");
    } else {
        let fallback = db.faces().find_map(|face| {
            face.families.iter().find_map(|(f, _)| {
                if f.eq_ignore_ascii_case("lucide") {
                    None
                } else {
                    Some(f.clone())
                }
            })
        });
        if let Some(name) = fallback {
            db.set_sans_serif_family(&name);
            log::debug!("default sans-serif family (first loaded face): {name}");
        } else {
            log::warn!("no UI sans face in fontdb — cosmic-text has no default font");
        }
    }

    let serif = crate::utils::font::resolved_default_serif_family(db);
    if family_present(db, serif) {
        db.set_serif_family(serif);
        log::debug!("default serif family: {serif}");
    }

    let mono = crate::utils::font::resolved_default_mono_family(db);
    if family_present(db, mono) {
        db.set_monospace_family(mono);
        log::debug!("default monospace family: {mono}");
    }
}

/// Detect which lazy font buckets a string needs (EUI-style codepoint scan).
pub fn detect_script_needs(text: &str) -> ScriptNeeds {
    if text.is_ascii() {
        return ScriptNeeds::default();
    }

    let mut needs = ScriptNeeds::default();
    if emoji_detect::text_contains_emoji(text) {
        needs.emoji = true;
    }

    for c in text.chars() {
        let cp = c as u32;
        // Hangul jamo / syllables / compatibility — prefer dedicated KR faces.
        if matches!(
            cp,
            0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7AF | 0xD7B0..=0xD7FF
        ) {
            needs.hangul = true;
        }
        // Thai.
        if matches!(cp, 0x0E00..=0x0E7F) {
            needs.thai = true;
        }
        // CJK Unified + extensions + kana + CJK punctuation / fullwidth.
        // (Hangul excluded here so KR can take Malgun without forcing YaHei-only path.)
        if matches!(
            cp,
            0x2E80..=0x2FFF
                | 0x3000..=0x303F
                | 0x3040..=0x30FF
                | 0x3100..=0x312F
                | 0x31F0..=0x31FF
                | 0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0xFF00..=0xFFEF
                | 0x20000..=0x2CEAF
        ) {
            needs.cjk = true;
        }
        // Arabic + presentation forms.
        if matches!(
            cp,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
        ) {
            needs.arabic = true;
        }
        // Devanagari.
        if matches!(cp, 0x0900..=0x097F | 0xA8E0..=0xA8FF) {
            needs.devanagari = true;
        }
        // Math / misc symbols often missing from UI sans (∑ ∏ √ ∞ ♪ § ★ ※ …).
        if matches!(
            cp,
            0x00A7..=0x00B1
                | 0x0370..=0x03FF
                | 0x2000..=0x206F
                | 0x2070..=0x209F
                | 0x2190..=0x21FF
                | 0x2200..=0x22FF
                | 0x2300..=0x23FF
                | 0x2500..=0x25FF
                | 0x2600..=0x26FF
                | 0x2700..=0x27BF
                | 0x27C0..=0x27EF
                | 0x2980..=0x29FF
                | 0x2A00..=0x2AFF
        ) {
            // Color-emoji / EP codepoints are owned by the emoji pipeline.
            if !emoji_detect::grapheme_is_emoji(c.encode_utf8(&mut [0u8; 4])) {
                needs.symbols = true;
            }
        }
    }
    needs
}

fn load_lazy_hangul(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        // 맑은 고딕 — YaHei Hangul coverage is incomplete → tofu boxes without this.
        let _ = try_load_system_font(db, "malgun.ttf", state)
            || try_load_system_font(db, "malgunbd.ttf", state)
            || try_load_system_font(db, "malgunsl.ttf", state)
            || try_load_system_font(db, "gulim.ttc", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "AppleSDGothicNeo.ttc", state)
            || try_load_system_font(db, "Supplemental/AppleGothic.ttf", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "opentype/noto/NotoSansCJK-Regular.ttc", state)
            || try_load_system_font(db, "truetype/noto/NotoSansCJK-Regular.ttc", state)
            || try_load_system_font(db, "truetype/nanum/NanumGothic.ttf", state);
    }
}

fn load_lazy_thai(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        // Leelawadee UI (Win10+) or Tahoma (has Thai).
        let _ = try_load_system_font(db, "LeelawUI.ttf", state)
            || try_load_system_font(db, "LeelaUIb.ttf", state)
            || try_load_system_font(db, "tahoma.ttf", state)
            || try_load_system_font(db, "tahomabd.ttf", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "Thonburi.ttc", state)
            || try_load_system_font(db, "Supplemental/Thonburi.ttc", state)
            || try_load_system_font(db, "Ayuthaya.ttf", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "truetype/noto/NotoSansThai-Regular.ttf", state)
            || try_load_system_font(db, "noto/NotoSansThai-Regular.ttf", state)
            || try_load_system_font(db, "truetype/tlwg/TlwgTypo.ttf", state);
    }
}

fn load_lazy_cjk(db: &mut Database, state: &mut LazyFontState) {
    if env_flag_disabled("RUI_LOAD_SYSTEM_CJK") && !env_flag_enabled("RUI_LOAD_CJK") {
        log::info!("skipping lazy CJK load (RUI_LOAD_SYSTEM_CJK=0)");
        return;
    }

    // Prefer project Noto when explicitly requested / featured.
    #[cfg(feature = "bundled-cjk")]
    {
        load_static_bytes(db, include_bytes!("../../assets/NotoSansCJK.ttc"));
    }
    #[cfg(not(feature = "bundled-cjk"))]
    {
        if env_flag_enabled("RUI_LOAD_CJK") || cfg!(feature = "fonts-cjk") {
            try_load_font_file(db, RUNTIME_CJK_SANS, state);
        }
    }

    #[cfg(target_os = "windows")]
    {
        // One CJK family is enough (avoid msyhbd/msyhl unless later needed).
        let _ = try_load_system_font(db, "msyh.ttc", state)
            || try_load_system_font(db, "msyhbd.ttc", state)
            || try_load_system_font(db, "simhei.ttf", state)
            || try_load_system_font(db, "simsun.ttc", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "PingFang.ttc", state)
            || try_load_system_font(db, "Hiragino Sans GB.ttc", state)
            || try_load_system_font(db, "STHeiti Light.ttc", state)
            || try_load_system_font(db, "AppleSDGothicNeo.ttc", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "opentype/noto/NotoSansCJK-Regular.ttc", state)
            || try_load_system_font(db, "noto-cjk/NotoSansCJK-Regular.ttc", state)
            || try_load_system_font(db, "truetype/noto/NotoSansCJK-Regular.ttc", state)
            || try_load_system_font(db, "truetype/wqy/wqy-microhei.ttc", state);
    }

    if env_flag_enabled("RUI_LOAD_CJK_SERIF") || cfg!(feature = "fonts-cjk-serif") {
        #[cfg(feature = "bundled-cjk-serif")]
        {
            load_static_bytes(db, include_bytes!("../../assets/NotoSerifCJK.ttc"));
        }
        #[cfg(not(feature = "bundled-cjk-serif"))]
        {
            try_load_font_file(db, RUNTIME_CJK_SERIF, state);
        }
    }
}

fn load_lazy_emoji(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(feature = "bundled-emoji")]
    {
        load_static_bytes(
            db,
            include_bytes!("../../assets/NotoColorEmoji_WindowsCompatible.ttf"),
        );
        load_static_bytes(db, include_bytes!("../../assets/NotoColorEmoji.ttf"));
    }
    #[cfg(not(feature = "bundled-emoji"))]
    {
        if env_flag_enabled("RUI_LOAD_EMOJI") || cfg!(feature = "fonts-emoji") {
            for name in RUNTIME_EMOJI {
                try_load_font_file(db, name, state);
            }
        }
    }

    // System color-emoji faces (mmap). Used for UI FontSystem (system-font mode) and
    // isolated fallback FS (bitmap-override miss).
    if env_flag_disabled("RUI_LOAD_SYSTEM_EMOJI") {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = try_load_system_font(db, "seguiemj.ttf", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "Apple Color Emoji.ttc", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "truetype/noto/NotoColorEmoji.ttf", state)
            || try_load_system_font(db, "noto/NotoColorEmoji.ttf", state)
            || try_load_system_font(db, "google-noto/NotoColorEmoji.ttf", state);
    }
}

fn load_lazy_symbols(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        let _ = try_load_system_font(db, "seguisym.ttf", state)
            || try_load_system_font(db, "seguili.ttf", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "Apple Symbols.ttf", state)
            || try_load_system_font(db, "Supplemental/Arial Unicode.ttf", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "truetype/dejavu/DejaVuSans.ttf", state);
    }

    if env_flag_enabled("RUI_LOAD_SYMBOLS") || cfg!(feature = "bundled-symbols") {
        #[cfg(feature = "bundled-symbols")]
        {
            load_static_bytes(db, include_bytes!("../../assets/NotoSansSymbols-wght.ttf"));
            load_static_bytes(
                db,
                include_bytes!("../../assets/NotoSansSymbols2-Regular.ttf"),
            );
            load_static_bytes(db, include_bytes!("../../assets/NotoSansMath-Regular.ttf"));
            load_static_bytes(db, include_bytes!("../../assets/NotoMusic-Regular.ttf"));
        }
        #[cfg(not(feature = "bundled-symbols"))]
        {
            for name in RUNTIME_SYMBOLS {
                try_load_font_file(db, name, state);
            }
        }
    }
}

fn load_lazy_arabic(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        // Segoe UI already covers much Arabic; add Traditional Arabic / Arial if present.
        let _ = try_load_system_font(db, "tradbdo.ttf", state)
            || try_load_system_font(db, "trado.ttf", state)
            || try_load_system_font(db, "arial.ttf", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "Supplemental/Arial.ttf", state)
            || try_load_system_font(db, "GeezaPro.ttc", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "truetype/dejavu/DejaVuSans.ttf", state)
            || try_load_system_font(db, "truetype/liberation/LiberationSans-Regular.ttf", state);
    }
}

fn load_lazy_devanagari(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        let _ = try_load_system_font(db, "Nirmala.ttc", state)
            || try_load_system_font(db, "mangal.ttf", state);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = try_load_system_font(db, "Supplemental/Devanagari Sangam MN.ttc", state)
            || try_load_system_font(db, "KohinoorDevanagari.ttc", state);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = try_load_system_font(db, "truetype/noto/NotoSansDevanagari-Regular.ttf", state)
            || try_load_system_font(db, "noto/NotoSansDevanagari-Regular.ttf", state);
    }
}

/// Open fonts required by `needs` that have not been attempted yet.
pub fn ensure_scripts(font_system: &mut FontSystem, needs: ScriptNeeds) {
    if needs.is_empty() {
        return;
    }

    let mut state = lazy_state();
    // Global attempted avoids re-scanning missing files every frame, but each FontSystem
    // still needs faces injected into *its* Database.
    let mut todo = needs.minus_attempted(state.attempted);
    {
        let db = font_system.db();
        if needs.cjk && !cjk_coverage_present(db) {
            todo.cjk = true;
        }
        if needs.hangul && !hangul_coverage_present(db) {
            todo.hangul = true;
        }
        if needs.thai && !thai_coverage_present(db) {
            todo.thai = true;
        }
        if needs.symbols && !symbols_coverage_present(db) {
            todo.symbols = true;
        }
        if needs.arabic && !arabic_coverage_present(db) {
            todo.arabic = true;
        }
        if needs.devanagari && !devanagari_coverage_present(db) {
            todo.devanagari = true;
        }
        // Emoji always uses slot layout + CustomGlyph quads — never inject CBDT into
        // the UI DB (glyphon ignores text-color alpha for Color glyphs → slot leak).
        // Rasters come from the isolated fallback FS (`ensure_emoji_font_fallback`).
        // `RUI_LOAD_SYSTEM_EMOJI=1` is a rare debug escape hatch only.
        if needs.emoji && env_flag_enabled("RUI_LOAD_SYSTEM_EMOJI") && !emoji_coverage_present(db) {
            todo.emoji = true;
        }
    }
    if todo.is_empty() {
        return;
    }

    // Mark attempted up-front. Defer emoji on the UI FS unless explicitly eager —
    // slots + isolated fallback own color emoji.
    let mut mark = todo;
    let eager_emoji = env_flag_enabled("RUI_LOAD_SYSTEM_EMOJI");
    if todo.emoji && !eager_emoji {
        mark.emoji = false;
        todo.emoji = false;
    }
    state.attempted.merge(mark);

    let db = font_system.db_mut();
    let mut loaded_any = false;
    if todo.cjk {
        log::info!("lazy-loading CJK fonts for coverage");
        load_lazy_cjk(db, &mut state);
        loaded_any = true;
    }
    if todo.hangul {
        log::info!("lazy-loading Hangul fonts for coverage");
        load_lazy_hangul(db, &mut state);
        loaded_any = true;
    }
    if todo.thai {
        log::info!("lazy-loading Thai fonts for coverage");
        load_lazy_thai(db, &mut state);
        loaded_any = true;
    }
    if todo.emoji {
        log::info!("lazy-loading emoji fonts into UI FS (RUI_LOAD_SYSTEM_EMOJI debug)");
        state.attempted.emoji = true;
        load_lazy_emoji(db, &mut state);
        loaded_any = true;
    }
    if todo.symbols {
        log::info!("lazy-loading symbol fonts for coverage");
        load_lazy_symbols(db, &mut state);
        loaded_any = true;
    }
    if todo.arabic {
        log::info!("lazy-loading Arabic fonts for coverage");
        load_lazy_arabic(db, &mut state);
        loaded_any = true;
    }
    if todo.devanagari {
        log::info!("lazy-loading Devanagari fonts for coverage");
        load_lazy_devanagari(db, &mut state);
        loaded_any = true;
    }

    apply_default_families(db);
    if loaded_any {
        crate::utils::font::clear_resolve_family_cache();
        crate::text::measure::invalidate_prepare_cache();
    }
    log::info!(
        "font database after lazy load: {} faces",
        font_system.db().len()
    );
}

/// Open system color-emoji on an isolated FontSystem used by emoji raster fallback
/// (`prepare_text::emoji_fallback_engines`) and Twemoji-miss.
///
/// Do **not** call this on the UI/layout FontSystem: glyphon ignores text color alpha
/// for `SwashContent::Color`, so CBDT faces in the main DB paint through “hidden” slots.
pub fn ensure_emoji_font_fallback(font_system: &mut FontSystem) {
    let mut state = lazy_state();
    let missing_in_db = !emoji_coverage_present(font_system.db());
    if state.attempted.emoji && !missing_in_db {
        return;
    }
    state.attempted.emoji = true;
    log::info!("emoji raster fallback — lazy-loading system/user emoji font");
    load_lazy_emoji(font_system.db_mut(), &mut state);
    apply_default_families(font_system.db_mut());
    crate::text::measure::invalidate_prepare_cache();
}

/// Scan `text` and lazily open CJK / emoji / symbol / script fonts as needed.
#[inline]
pub fn ensure_coverage_for_text(font_system: &mut FontSystem, text: &str) {
    let needs = detect_script_needs(text);
    ensure_scripts(font_system, needs);
}

/// Eager UI faces only — never msyh / seguimj / full system scan.
fn load_eager_system_ui_fonts(db: &mut Database, state: &mut LazyFontState) {
    #[cfg(target_os = "windows")]
    {
        // Minimal weight set for UI chrome (skip arial/cour — saves mapped pages).
        // Regular / Semibold / Bold plus Light + Semilight so the weight axis is denser
        // (still no Segoe UI Medium 500 — that uses VF interpolation or synthetic bold).
        for name in [
            "segoeui.ttf",
            "segoeuil.ttf",
            "segoeuisl.ttf",
            "seguisb.ttf",
            "segoeuib.ttf",
            "seguibl.ttf",
            "consola.ttf",
            "cascadiacode.ttf",
            "CascadiaCode.ttf",
        ] {
            try_load_system_font(db, name, state);
        }
    }

    #[cfg(target_os = "macos")]
    {
        for name in [
            "SFNS.ttf",
            "SFNSText.ttf",
            "Helvetica.ttc",
            "Supplemental/Arial.ttf",
        ] {
            try_load_system_font(db, name, state);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for name in [
            "truetype/dejavu/DejaVuSans.ttf",
            "truetype/dejavu/DejaVuSans-Bold.ttf",
            "truetype/dejavu/DejaVuSansMono.ttf",
        ] {
            try_load_system_font(db, name, state);
        }
    }
}

/// Optional project UI faces from `RUI_FONT_DIR` / `assets/` (Inter, etc.).
fn load_eager_project_ui_fonts(db: &mut Database, state: &mut LazyFontState) {
    for name in RUNTIME_INTER {
        if try_load_font_file(db, name, state) {
            log::debug!("loaded project UI font: {name}");
        }
    }
}

/// Optional project / bundled assets that are safe to open at startup (icons only by default).
fn load_eager_bundled_fonts(db: &mut Database, state: &mut LazyFontState) {
    load_eager_project_ui_fonts(db, state);

    #[cfg(feature = "bundled-lucide")]
    {
        load_static_bytes(db, include_bytes!("../../assets/lucide.ttf"));
    }
    #[cfg(not(feature = "bundled-lucide"))]
    {
        if !try_load_font_file(db, RUNTIME_LUCIDE, state) {
            log::warn!(
                "lucide.ttf not found beside assets/; icons may be missing (enable feature bundled-lucide)"
            );
        }
    }
    crate::utils::font::clear_resolve_family_cache();

    // Escape hatches that force eager Noto loads (single-file / CI demos).
    #[cfg(feature = "bundled-cjk")]
    {
        load_static_bytes(db, include_bytes!("../../assets/NotoSansCJK.ttc"));
        state.attempted.cjk = true;
    }
    #[cfg(not(feature = "bundled-cjk"))]
    {
        if (env_flag_enabled("RUI_LOAD_CJK") || cfg!(feature = "fonts-cjk"))
            && !env_flag_disabled("RUI_LOAD_CJK")
            && try_load_font_file(db, RUNTIME_CJK_SANS, state)
        {
            state.attempted.cjk = true;
        }
    }

    #[cfg(feature = "bundled-emoji")]
    {
        load_static_bytes(
            db,
            include_bytes!("../../assets/NotoColorEmoji_WindowsCompatible.ttf"),
        );
        load_static_bytes(db, include_bytes!("../../assets/NotoColorEmoji.ttf"));
        state.attempted.emoji = true;
    }

    apply_default_families(db);
}

/// Kept for debug binaries / callers that already hold a `Database`.
pub fn load_bundled_fonts(db: &mut Database) {
    let mut state = lazy_state();
    load_eager_bundled_fonts(db, &mut state);
}

fn resolve_locale() -> String {
    std::env::var("RUI_TEXT_LOCALE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from("en-US"))
}

/// Build a [`FontSystem`] without scanning the entire OS font directory.
pub fn create_font_system() -> FontSystem {
    let mut db = Database::new();
    let mut state = lazy_state();

    if env_flag_enabled("RUI_LOAD_SYSTEM_FONTS") {
        log::warn!("RUI_LOAD_SYSTEM_FONTS=1 — loading ALL system fonts (high memory use)");
        db.load_system_fonts();
        state.attempted = ScriptNeeds {
            cjk: true,
            hangul: true,
            thai: true,
            emoji: true,
            symbols: true,
            arabic: true,
            devanagari: true,
        };
    } else {
        load_eager_system_ui_fonts(&mut db, &mut state);
        // Opt-in eager CJK/emoji (overrides lazy default).
        if env_flag_enabled("RUI_LOAD_SYSTEM_CJK") {
            load_lazy_cjk(&mut db, &mut state);
            state.attempted.cjk = true;
        }
        if env_flag_enabled("RUI_LOAD_SYSTEM_EMOJI") {
            load_lazy_emoji(&mut db, &mut state);
            state.attempted.emoji = true;
        }
    }

    load_eager_bundled_fonts(&mut db, &mut state);

    let face_count = db.len();
    log::info!("font database ready: {face_count} faces (eager UI only; CJK/emoji lazy on demand)");

    drop(state);
    FontSystem::new_with_locale_and_db(resolve_locale(), db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_coverage_injects_cjk_into_each_font_system() {
        let mut layout_fs = create_font_system();
        let mut render_fs = create_font_system();
        ensure_coverage_for_text(&mut layout_fs, "中文");
        ensure_coverage_for_text(&mut render_fs, "日本語");
        assert!(
            cjk_coverage_present(layout_fs.db()),
            "layout FontSystem missing CJK after ensure_coverage"
        );
        assert!(
            cjk_coverage_present(render_fs.db()),
            "render FontSystem missing CJK after ensure_coverage (global loaded_paths short-circuit regression)"
        );
    }
}
