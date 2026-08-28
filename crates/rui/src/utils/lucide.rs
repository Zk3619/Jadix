//! Lucide icon name → glyph / SVG-path lookup (shared by `<Icon name="…">` binds).
//!
//! Glyph and path tables are **compiled** into perfect-hash maps by `build.rs` from the
//! full `assets/lucide*.json` catalogs (no workspace subset scan).

use crate::core::tree::{Element, LayoutStatus};

pub const ATTR_ICON_NAME: &str = "rui_icon_name";

mod glyph_map {
    include!(concat!(env!("OUT_DIR"), "/lucide_glyph_map.rs"));
}

#[cfg(feature = "bundled-lucide")]
mod paths_map {
    include!(concat!(env!("OUT_DIR"), "/lucide_paths_map.rs"));
}

/// Resolve a Lucide icon name (e.g. `"check"`) to a single-glyph string for the lucide font.
pub fn lucide_glyph(name: &str) -> Option<String> {
    let hex = glyph_map::LUCIDE_GLYPHS.get(name)?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|c| c.to_string())
}

/// Inner SVG markup for a Lucide icon (no root `<svg>`), from [`lucide_paths.json`].
pub fn lucide_svg_inner(name: &str) -> Option<&'static str> {
    #[cfg(feature = "bundled-lucide")]
    {
        return paths_map::LUCIDE_PATHS.get(name).copied();
    }
    #[cfg(not(feature = "bundled-lucide"))]
    {
        let _ = name;
        None
    }
}

/// Attach / refresh glyph + `rui_icon_name` for Solid-style `<Icon name={…}>`.
pub fn apply_bound_icon_name(el: &mut Element, name: impl AsRef<str>) -> LayoutStatus {
    let name = name.as_ref();
    let Some(glyph) = lucide_glyph(name) else {
        return LayoutStatus::Clean;
    };
    let family_ok = el.text_style.font_family.as_deref() == Some("lucide");
    let name_ok = el.get_attr(ATTR_ICON_NAME).map(|s| s.as_str()) == Some(name);
    if el.text.as_deref() == Some(glyph.as_str()) && family_ok && name_ok {
        return LayoutStatus::Clean;
    }
    el.text = Some(glyph);
    el.text_style.font_family = Some("lucide".to_string());
    el.set_attr(ATTR_ICON_NAME, name);
    LayoutStatus::LayoutChanged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_map_has_check() {
        assert!(lucide_glyph("check").is_some());
    }
}
