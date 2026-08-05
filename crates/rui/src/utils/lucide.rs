//! Lucide icon name → glyph lookup (shared by compile-time `<Icon name="…">` runtime binds).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::tree::{Element, LayoutStatus};

fn lucide_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/lucide.json")).unwrap_or_default()
    })
}

/// Resolve a Lucide icon name (e.g. `"check"`) to a single-glyph string for the lucide font.
pub fn lucide_glyph(name: &str) -> Option<String> {
    let hex = lucide_map().get(name)?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|c| c.to_string())
}

/// Solid-style in-place bind for `<Icon name={sig.get()}>`.
pub fn apply_bound_icon_name(el: &mut Element, name: impl AsRef<str>) -> LayoutStatus {
    let Some(glyph) = lucide_glyph(name.as_ref()) else {
        return LayoutStatus::Clean;
    };
    let family_ok = el.text_style.font_family.as_deref() == Some("lucide");
    if el.text.as_deref() == Some(glyph.as_str()) && family_ok {
        return LayoutStatus::Clean;
    }
    el.text = Some(glyph);
    el.text_style.font_family = Some("lucide".to_string());
    LayoutStatus::LayoutChanged
}
