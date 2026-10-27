//! Lucide icon name → glyph / SVG-path lookup (shared by `<Icon name="…">` binds).
//!
//! Name resolution for app icons goes through [`crate::utils::icon_provider`]. This
//! module adapts `jadix_lucide`; [`lucide_glyph`] forwards to the provider.

pub(crate) use crate::catalog::glyph as lucide_builtin_glyph;
pub use crate::catalog::svg_inner as lucide_svg_inner;

pub use super::icon_provider::ATTR_ICON_NAME;

pub(crate) struct DefaultProvider;

impl super::icon_provider::IconProvider for DefaultProvider {
    fn font_data(&self) -> Option<&'static [u8]> {
        Some(crate::catalog::FONT_DATA)
    }

    fn resolve(&self, name: &str) -> Option<super::icon_provider::IconSource> {
        Some(super::icon_provider::IconSource::Glyph {
            family: "lucide".into(),
            glyph: lucide_builtin_glyph(name)?,
        })
    }

    fn svg_source(&self, name: &str, source: &super::icon_provider::IconSource) -> Option<String> {
        match source {
            super::icon_provider::IconSource::Glyph { family, glyph }
                if family == "lucide" && lucide_builtin_glyph(name).as_ref() == Some(glyph) =>
            {
                lucide_svg_inner(name).map(str::to_string)
            }
            _ => None,
        }
    }

    #[allow(unexpected_cfgs)]
    fn configure_animation(
        &self,
        element: &mut crate::core::tree::Element,
        name: &str,
        source: &super::icon_provider::IconSource,
    ) -> bool {
        #[cfg(any(feature = "lucide-animated", all(test, feature = "gallery-tests")))]
        {
            use super::lucide_animated as animation;
            let supported = matches!(source, super::icon_provider::IconSource::Glyph { family, glyph }
                if family == "lucide" && lucide_builtin_glyph(name).as_ref() == Some(glyph))
                && animation::animated_icon_recipe(name).is_some();
            if supported {
                if element
                    .get_attr(animation::ATTR_ANIMATED_ICON)
                    .map(String::as_str)
                    != Some(name)
                {
                    crate::authoring::icon::reset(element);
                    element.set_attr(animation::ATTR_ANIMATED_ICON, name);
                }
                animation::attach_playback(element);
                return true;
            }
            crate::authoring::icon::reset(element);
            element.attrs.remove(animation::ATTR_ANIMATED_ICON);
        }
        let _ = (element, name, source);
        false
    }
}

/// Resolve an icon name to a font glyph.
///
/// Uses [`crate::utils::icon_provider::resolve_icon`]: overlay / custom provider /
/// bundled Lucide. Raster `src` names return `None`.
pub fn lucide_glyph(name: &str) -> Option<String> {
    crate::utils::icon_provider::glyph_for_name(name)
}
