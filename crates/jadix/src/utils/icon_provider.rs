//! Pluggable icon catalog + custom SVG / raster files.
//!
//! Resolution order for [`resolve_icon`]:
//! 1. [`register_icon`] overlay
//! 2. [`set_icon_provider`] (if any)
//! 3. built-in Lucide catalog
//!
//! `<Icon name="check">` still works. Custom assets:
//!
//! ```ignore
//! use jadix::prelude::*;
//!
//! register_icon("brand", "assets/logo.png");
//! register_icon("mark", "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'>…</svg>");
//!
//! ui! {
//!     <Icon name="brand" size="1.25rem" />
//!     <Icon src="assets/logo.svg" size="1.25rem" />
//! }
//! ```
//!
//! File `src` is the same pipeline as [`crate::components::Image`]: local path / `data:` URL
//! (no `http(s)`). PNG/JPEG/WebP/GIF/ICO always; SVG needs the `svg` feature.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::app::pending_images::{queue_image_from_src, texture_key_for_src};
use crate::core::tree::{Element, IntoCssDim, LayoutStatus};
use crate::style::UnitSlot;
use crate::utils::lucide::{lucide_builtin_glyph, ATTR_ICON_NAME};
use crate::utils::text_node::build_named_icon_text_node;

/// Default square size when `<Icon src>` / a registered raster has no explicit `size`.
pub const DEFAULT_ICON_SIZE_PX: f32 = 16.0;

/// Fallback PUA glyph (Lucide "album") when a name does not resolve.
const FALLBACK_GLYPH: &str = "\u{ea0e}";

pub const ATTR_ICON_SRC: &str = "jadix_icon_src";

/// What a name (or explicit `src`) paints as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSource {
    /// Font glyph (Lucide PUA or a custom family).
    Glyph { family: String, glyph: String },
    /// Local path, `data:` URL, or `http(s)` when `remote-asset` + [`crate::set_remote_asset_fetcher`].
    Src(String),
}

impl IconSource {
    pub fn path(src: impl Into<String>) -> Self {
        Self::Src(src.into())
    }

    /// Inline SVG document (or inner markup without a root `<svg>`).
    pub fn svg(markup: impl AsRef<str>) -> Self {
        Self::Src(svg_markup_to_data_url(markup.as_ref()))
    }
}

impl From<&str> for IconSource {
    fn from(s: &str) -> Self {
        let t = s.trim();
        if t.starts_with('<') {
            Self::svg(t)
        } else {
            Self::path(t)
        }
    }
}

impl From<String> for IconSource {
    fn from(s: String) -> Self {
        IconSource::from(s.as_str())
    }
}

/// App-wide icon catalog. The default adapter is built-in Lucide.
pub trait IconProvider: Send + Sync {
    fn resolve(&self, name: &str) -> Option<IconSource>;
}

struct LucideIconProvider;

impl IconProvider for LucideIconProvider {
    fn resolve(&self, name: &str) -> Option<IconSource> {
        let glyph = lucide_builtin_glyph(name)?;
        Some(IconSource::Glyph {
            family: "lucide".into(),
            glyph,
        })
    }
}

thread_local! {
    static OVERLAY: RefCell<HashMap<String, IconSource>> = RefCell::new(HashMap::new());
    static PROVIDER: RefCell<Option<Arc<dyn IconProvider>>> = RefCell::new(None);
}

/// Overlay a name. Wins over [`set_icon_provider`] and Lucide.
///
/// Thread-local (register on the UI thread before `jadix::run`).
pub fn register_icon(name: impl Into<String>, source: impl Into<IconSource>) {
    OVERLAY.with(|m| {
        m.borrow_mut().insert(name.into(), source.into());
    });
}

pub fn unregister_icon(name: &str) {
    OVERLAY.with(|m| {
        m.borrow_mut().remove(name);
    });
}

pub fn clear_registered_icons() {
    OVERLAY.with(|m| m.borrow_mut().clear());
}

/// Replace the custom catalog layer (Lucide remains the fallback; overlay still wins).
pub fn set_icon_provider(provider: Arc<dyn IconProvider>) {
    PROVIDER.with(|s| {
        *s.borrow_mut() = Some(provider);
    });
}

pub fn clear_icon_provider() {
    PROVIDER.with(|s| {
        *s.borrow_mut() = None;
    });
}

/// Resolve `name` through overlay → custom provider → Lucide.
pub fn resolve_icon(name: &str) -> Option<IconSource> {
    if let Some(src) = OVERLAY.with(|m| m.borrow().get(name).cloned()) {
        return Some(src);
    }
    if let Some(src) = PROVIDER.with(|s| s.borrow().as_ref().and_then(|p| p.resolve(name))) {
        return Some(src);
    }
    LucideIconProvider.resolve(name)
}

/// Glyph string for a name, if the resolved source is a font glyph.
pub fn glyph_for_name(name: &str) -> Option<String> {
    match resolve_icon(name)? {
        IconSource::Glyph { glyph, .. } => Some(glyph),
        IconSource::Src(_) => None,
    }
}

/// Build `<Icon name="…">` (Lucide, overlay, or custom provider).
pub fn build_icon_named(name: impl AsRef<str>) -> Element {
    let name = name.as_ref();
    match resolve_icon(name) {
        Some(IconSource::Glyph { family, glyph }) => glyph_element(name, family, glyph),
        Some(IconSource::Src(src)) => build_icon_from_src_named(Some(name), src),
        None => glyph_element(name, "lucide".to_string(), FALLBACK_GLYPH),
    }
}

/// Build `<Icon src="…">`.
pub fn build_icon_from_src(src: impl Into<String>) -> Element {
    build_icon_from_src_named(None, src.into())
}

pub fn icon_named(name: impl AsRef<str>) -> Element {
    build_icon_named(name)
}

pub fn icon_src(src: impl Into<String>) -> Element {
    build_icon_from_src(src)
}

/// Apply `size` on `<Icon>`: text size always; box size for raster / SVG `src` icons.
pub fn with_icon_size(el: Element, px: f32) -> Element {
    let px = px.max(1.0);
    let is_src = el.image_texture.is_some() || el.get_attr(ATTR_ICON_SRC).is_some();
    let el = el.text_size(px);
    if is_src {
        icon_box(el, px)
    } else {
        el
    }
}

/// Solid-style `<Icon name={…}>`.
pub fn apply_bound_icon_name(el: &mut Element, name: impl AsRef<str>) -> LayoutStatus {
    let name = name.as_ref();
    let next = resolve_icon(name);
    apply_resolved(el, Some(name), next)
}

/// Solid-style `<Icon src={…}>`.
pub fn apply_bound_icon_src(el: &mut Element, src: impl AsRef<str>) -> LayoutStatus {
    let src = src.as_ref().trim();
    if src.is_empty() {
        let name = el.get_attr(ATTR_ICON_NAME).cloned();
        return apply_resolved(el, name.as_deref(), None);
    }
    apply_resolved(el, None, Some(IconSource::Src(src.to_string())))
}

fn apply_resolved(
    el: &mut Element,
    name: Option<&str>,
    source: Option<IconSource>,
) -> LayoutStatus {
    match source {
        Some(IconSource::Glyph { family, glyph }) => {
            let name_ok = match name {
                Some(n) => el.get_attr(ATTR_ICON_NAME).map(|s| s.as_str()) == Some(n),
                None => el.get_attr(ATTR_ICON_NAME).is_none(),
            };
            let src_cleared = el.get_attr(ATTR_ICON_SRC).is_none() && el.image_texture.is_none();
            if el.text.as_deref() == Some(glyph.as_str())
                && el.text_style.font_family.as_deref() == Some(family.as_str())
                && name_ok
                && src_cleared
            {
                return LayoutStatus::Clean;
            }
            paint_glyph(el, name, family, glyph);
            LayoutStatus::LayoutChanged
        }
        Some(IconSource::Src(src)) => {
            let src_ok = el.get_attr(ATTR_ICON_SRC).map(|s| s.as_str()) == Some(src.as_str());
            let name_ok = match name {
                Some(n) => el.get_attr(ATTR_ICON_NAME).map(|s| s.as_str()) == Some(n),
                None => true,
            };
            if src_ok && name_ok && el.image_texture.is_some() && el.text.is_none() {
                return LayoutStatus::Clean;
            }
            paint_src(el, name, &src);
            LayoutStatus::LayoutChanged
        }
        None => {
            let fallback = FALLBACK_GLYPH;
            if el.text.as_deref() == Some(fallback)
                && el.text_style.font_family.as_deref() == Some("lucide")
                && el.image_texture.is_none()
            {
                return LayoutStatus::Clean;
            }
            paint_glyph(el, name, "lucide", fallback);
            LayoutStatus::LayoutChanged
        }
    }
}

fn glyph_element(name: &str, family: String, glyph: impl Into<String>) -> Element {
    let mut el = build_named_icon_text_node(name, glyph);
    el.text_style.font_family = Some(family);
    el
}

fn build_icon_from_src_named(name: Option<&str>, src: String) -> Element {
    let mut el = Element::new().pointer_events("bounding-box");
    paint_src(&mut el, name, &src);
    icon_box(el, DEFAULT_ICON_SIZE_PX)
}

fn paint_glyph(
    el: &mut Element,
    name: Option<&str>,
    family: impl Into<String>,
    glyph: impl Into<String>,
) {
    el.text = Some(glyph.into());
    el.text_style.font_family = Some(family.into());
    el.image_texture = None;
    el.attrs.remove(ATTR_ICON_SRC);
    match name {
        Some(n) => {
            el.set_attr(ATTR_ICON_NAME, n);
        }
        None => {
            el.attrs.remove(ATTR_ICON_NAME);
        }
    }
}

fn paint_src(el: &mut Element, name: Option<&str>, src: &str) {
    let key = texture_key_for_src(src);
    queue_image_from_src(&key, src);
    el.text = None;
    el.text_style.font_family = None;
    el.image_texture = Some(key);
    el.set_attr(ATTR_ICON_SRC, src);
    match name {
        Some(n) => {
            el.set_attr(ATTR_ICON_NAME, n);
        }
        None => {
            el.attrs.remove(ATTR_ICON_NAME);
        }
    }
    let px = el.text_style.size.unwrap_or(DEFAULT_ICON_SIZE_PX).max(1.0);
    let dim = px.into_css_dim();
    el.style.set_css_dim(UnitSlot::Width, dim.clone());
    el.style.set_css_dim(UnitSlot::Height, dim.clone());
    el.style.set_css_dim(UnitSlot::MinWidth, dim.clone());
    el.style.set_css_dim(UnitSlot::MinHeight, dim);
    el.style.flex_shrink = 0.0;
}

fn icon_box(el: Element, px: f32) -> Element {
    el.w(px).h(px).min_w(px).min_h(px).flex_shrink(0.0)
}

fn svg_markup_to_data_url(markup: &str) -> String {
    let body = if markup.trim_start().to_ascii_lowercase().starts_with("<svg") {
        markup.trim().to_string()
    } else {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{markup}</svg>"#
        )
    };
    let mut out = String::from("data:image/svg+xml,");
    for &b in body.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
fn with_icon_test<R>(f: impl FnOnce() -> R) -> R {
    clear_registered_icons();
    clear_icon_provider();
    let out = f();
    clear_registered_icons();
    clear_icon_provider();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1×1 red PNG.
    const RED_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    struct MapProvider(HashMap<String, IconSource>);

    impl IconProvider for MapProvider {
        fn resolve(&self, name: &str) -> Option<IconSource> {
            self.0.get(name).cloned()
        }
    }

    #[test]
    fn lucide_name_resolves_to_glyph() {
        with_icon_test(|| {
            let src = resolve_icon("check").expect("lucide check");
            match src {
                IconSource::Glyph { family, glyph } => {
                    assert_eq!(family, "lucide");
                    assert_eq!(glyph, lucide_builtin_glyph("check").unwrap());
                }
                IconSource::Src(_) => panic!("expected glyph"),
            }
            let el = build_icon_named("check");
            assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
            assert_eq!(
                el.get_attr(ATTR_ICON_NAME).map(String::as_str),
                Some("check")
            );
            assert!(el.image_texture.is_none());
        });
    }

    #[test]
    fn register_icon_overlays_lucide() {
        with_icon_test(|| {
            register_icon("check", RED_PNG);
            match resolve_icon("check").unwrap() {
                IconSource::Src(s) => assert_eq!(s, RED_PNG),
                IconSource::Glyph { .. } => panic!("overlay should win"),
            }
            let el = build_icon_named("check");
            assert!(el.image_texture.is_some());
            assert_eq!(
                el.get_attr(ATTR_ICON_SRC).map(String::as_str),
                Some(RED_PNG)
            );
            unregister_icon("check");
            match resolve_icon("check").unwrap() {
                IconSource::Glyph { .. } => {}
                IconSource::Src(_) => panic!("lucide should return"),
            }
        });
    }

    #[test]
    fn custom_provider_shadows_lucide_but_not_overlay() {
        with_icon_test(|| {
            let mut map = HashMap::new();
            map.insert("check".into(), IconSource::path(RED_PNG));
            map.insert("brand".into(), IconSource::path("assets/brand.png"));
            set_icon_provider(Arc::new(MapProvider(map)));
            match resolve_icon("check").unwrap() {
                IconSource::Src(s) => assert_eq!(s, RED_PNG),
                IconSource::Glyph { .. } => panic!("provider should win over lucide"),
            }
            match resolve_icon("brand").unwrap() {
                IconSource::Src(s) => assert_eq!(s, "assets/brand.png"),
                IconSource::Glyph { .. } => panic!("expected src"),
            }
            register_icon("brand", "assets/overlay.png");
            match resolve_icon("brand").unwrap() {
                IconSource::Src(s) => assert_eq!(s, "assets/overlay.png"),
                IconSource::Glyph { .. } => panic!("overlay should win"),
            }
        });
    }

    #[test]
    fn from_str_svg_becomes_data_url() {
        let src = IconSource::from("<svg xmlns='http://www.w3.org/2000/svg'></svg>");
        match src {
            IconSource::Src(s) => assert!(s.starts_with("data:image/svg+xml,"), "{s}"),
            IconSource::Glyph { .. } => panic!("expected src"),
        }
        let path = IconSource::from("assets/logo.png");
        match path {
            IconSource::Src(s) => assert_eq!(s, "assets/logo.png"),
            IconSource::Glyph { .. } => panic!("expected src"),
        }
    }

    #[test]
    fn build_icon_from_src_queues_texture() {
        with_icon_test(|| {
            let el = build_icon_from_src(RED_PNG);
            assert!(el.image_texture.as_deref().unwrap().contains("jadix-src:"));
            assert_eq!(
                el.get_attr(ATTR_ICON_SRC).map(String::as_str),
                Some(RED_PNG)
            );
            assert!(el.text.is_none());
        });
    }

    #[test]
    fn apply_bound_icon_name_switches_glyph_to_src() {
        with_icon_test(|| {
            let mut el = build_icon_named("check");
            assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
            register_icon("brand", RED_PNG);
            assert_eq!(
                apply_bound_icon_name(&mut el, "brand"),
                LayoutStatus::LayoutChanged
            );
            assert!(el.text.is_none());
            assert!(el.image_texture.is_some());
            assert_eq!(apply_bound_icon_name(&mut el, "brand"), LayoutStatus::Clean);
            assert_eq!(
                apply_bound_icon_name(&mut el, "check"),
                LayoutStatus::LayoutChanged
            );
            assert_eq!(el.text_style.font_family.as_deref(), Some("lucide"));
            assert!(el.image_texture.is_none());
        });
    }

    #[test]
    fn with_icon_size_boxes_src_only() {
        with_icon_test(|| {
            let glyph = with_icon_size(build_icon_named("check"), 20.0);
            assert!((glyph.text_style.size_resolved() - 20.0).abs() < f32::EPSILON);
            let src = with_icon_size(build_icon_from_src(RED_PNG), 20.0);
            assert!((src.text_style.size_resolved() - 20.0).abs() < f32::EPSILON);
        });
    }
}
