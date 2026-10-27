pub use serde_json;

use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(any(test, feature = "font"))]
pub const FONT_DATA: &[u8] = include_bytes!("../assets/lucide.ttf");

fn glyphs() -> &'static HashMap<String, String> {
    static GLYPHS: OnceLock<HashMap<String, String>> = OnceLock::new();
    GLYPHS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/lucide.json")).unwrap_or_default()
    })
}

pub fn font_path() -> &'static std::path::Path {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/lucide.ttf"))
}

pub fn glyph(name: &str) -> Option<String> {
    let codepoint = u32::from_str_radix(glyphs().get(name)?, 16).ok()?;
    char::from_u32(codepoint).map(|value| value.to_string())
}

pub fn names() -> impl Iterator<Item = &'static str> {
    glyphs().keys().map(String::as_str)
}

pub fn svg_inner(name: &str) -> Option<&'static str> {
    #[cfg(any(test, feature = "paths"))]
    {
        static PATHS: OnceLock<HashMap<String, String>> = OnceLock::new();
        let paths = PATHS.get_or_init(|| {
            serde_json::from_str(include_str!("../assets/lucide_paths.json")).unwrap_or_default()
        });
        paths
            .get(name)
            .or_else(|| match name {
                "unlock" => paths.get("lock-open"),
                _ => None,
            })
            .map(String::as_str)
    }
    #[cfg(not(any(test, feature = "paths")))]
    {
        let _ = name;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_asset_is_valid_and_every_entry_is_resolvable() {
        let parsed: HashMap<String, String> =
            serde_json::from_str(include_str!("../assets/lucide.json")).unwrap();
        assert!(parsed.len() > 1000);
        assert_eq!(names().count(), parsed.len());
        for name in parsed.keys() {
            assert_eq!(glyph(name).unwrap().chars().count(), 1);
        }
        assert!(glyph("not-a-lucide-icon").is_none());
    }

    #[test]
    fn path_asset_tracks_feature_and_legacy_alias() {
        #[cfg(any(test, feature = "paths"))]
        {
            assert!(svg_inner("sun").is_some());
            assert_eq!(svg_inner("unlock"), svg_inner("lock-open"));
        }
        #[cfg(not(any(test, feature = "paths")))]
        assert!(svg_inner("sun").is_none());
        assert!(svg_inner("not-a-lucide-icon").is_none());
    }

    #[cfg(any(test, feature = "font"))]
    #[test]
    fn font_is_shipped_as_a_complete_sfnt() {
        assert!(FONT_DATA.len() > 100_000);
        assert_eq!(&FONT_DATA[..4], &[0, 1, 0, 0]);
    }
}
