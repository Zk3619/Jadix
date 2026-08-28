//! Lucide icon name → glyph / SVG-path lookup (shared by `<Icon name="…">` binds).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::tree::{Element, LayoutStatus};

pub const ATTR_ICON_NAME: &str = "rui_icon_name";

fn lucide_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/lucide.json")).unwrap_or_default()
    })
}

fn lucide_paths_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        #[cfg(feature = "bundled-lucide")]
        {
            serde_json::from_str(include_str!("../../assets/lucide_paths.json")).unwrap_or_default()
        }
        #[cfg(not(feature = "bundled-lucide"))]
        {
            HashMap::new()
        }
    })
}

/// Resolve a Lucide icon name (e.g. `"check"`) to a single-glyph string for the lucide font.
pub fn lucide_glyph(name: &str) -> Option<String> {
    let hex = lucide_map().get(name)?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|c| c.to_string())
}

/// Inner SVG markup for a Lucide icon (no root `<svg>`), from [`lucide_paths.json`].
pub fn lucide_svg_inner(name: &str) -> Option<&'static str> {
    lucide_paths_map().get(name).map(|s| s.as_str())
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
