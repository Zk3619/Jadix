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

pub fn svg_document(name: &str) -> Option<String> {
    let body = svg_inner(name)?;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n  {body}\n</svg>\n"
    ))
}

#[cfg(test)]
#[path = "tests/catalog.rs"]
mod tests;
