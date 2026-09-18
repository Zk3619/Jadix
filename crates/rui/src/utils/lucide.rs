//! Lucide icon name → glyph / SVG-path lookup (shared by `<Icon name="…">` binds).
//!
//! Name resolution for app icons goes through [`crate::utils::icon_provider`]. This
//! module owns the bundled Lucide tables; [`lucide_glyph`] forwards to the provider.

use std::collections::HashMap;
use std::sync::OnceLock;

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

pub(crate) fn lucide_builtin_glyph(name: &str) -> Option<String> {
    let hex = lucide_map().get(name)?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|c| c.to_string())
}

/// Resolve an icon name to a font glyph.
///
/// Uses [`crate::utils::icon_provider::resolve_icon`]: overlay / custom provider /
/// bundled Lucide. Raster `src` names return `None`.
pub fn lucide_glyph(name: &str) -> Option<String> {
    crate::utils::icon_provider::glyph_for_name(name)
}

/// Inner SVG markup for a Lucide icon (no root `<svg>`), from [`lucide_paths.json`].
/// Lucide renamed several outline icons; MorphIcon looks up SVG inners, not PUA glyphs.
fn lucide_path_alias(name: &str) -> Option<&'static str> {
    match name {
        "unlock" => Some("lock-open"),
        _ => None,
    }
}

pub fn lucide_svg_inner(name: &str) -> Option<&'static str> {
    lucide_paths_map()
        .get(name)
        .or_else(|| lucide_path_alias(name).and_then(|alias| lucide_paths_map().get(alias)))
        .map(|s| s.as_str())
}
