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
use std::sync::{Arc, Mutex};

use crate::app::pending_images::{queue_image_from_src, texture_key_for_src};
use crate::core::tree::{Element, IntoCssDim, LayoutStatus};
use crate::style::UnitSlot;
pub const ATTR_ICON_NAME: &str = "jadix_icon_name";
#[cfg(all(test, feature = "lucide"))]
use crate::utils::lucide::lucide_builtin_glyph;
use crate::utils::text_node::build_named_icon_text_node;

pub const ATTR_ICON_ANIMATION_STATE: &str = "jadix_animated_icon_state";

/// Default square size when `<Icon src>` / a registered raster has no explicit `size`.
pub const DEFAULT_ICON_SIZE_PX: f32 = 16.0;

/// Fallback PUA glyph (Lucide "album") when a name does not resolve.
fn fallback_glyph() -> (String, String) {
    match crate::DEFAULT_ICON_PROVIDER.as_ref().and_then(|provider| provider.resolve("album")) {
        Some(IconSource::Glyph { family, glyph }) => (family, glyph),
        _ => ("sans-serif".into(), "?".into()),
    }
}

pub const ATTR_ICON_SRC: &str = "jadix_icon_src";
pub(crate) const ATTR_ICON_HOVER_ANIMATION: &str = "jadix_icon_hover_animation";
const ATTR_ICON_CHANGE_ANIMATION: &str = "jadix_icon_change_animation";

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
    fn svg_source(&self, _name: &str, _source: &IconSource) -> Option<String> { None }
    fn configure_animation(&self, _element: &mut Element, _name: &str, _source: &IconSource) -> bool { false }
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
    let previous = PROVIDER.with(|slot| slot.borrow_mut().replace(provider));
    drop(previous);
}

pub fn clear_icon_provider() {
    let previous = PROVIDER.with(|slot| slot.borrow_mut().take());
    drop(previous);
}

/// Resolve `name` through overlay → custom provider → Lucide.
pub fn resolve_icon(name: &str) -> Option<IconSource> {
    if let Some(src) = OVERLAY.with(|m| m.borrow().get(name).cloned()) {
        return Some(src);
    }
    with_catalog(|provider| provider.resolve(name))
}

fn with_catalog<Value>(mut callback: impl FnMut(&dyn IconProvider) -> Option<Value>) -> Option<Value> {
    let custom = PROVIDER.with(|provider| provider.borrow().clone());
    if let Some(value) = custom.as_ref().and_then(|provider| callback(provider.as_ref())) { return Some(value); }
    crate::DEFAULT_ICON_PROVIDER.as_ref().and_then(|provider| callback(provider.as_ref()))
}

/// Resolve catalog-provided SVG markup for a named glyph, including custom catalogs.
pub fn resolve_icon_svg(name: &str) -> Option<String> {
    let source = resolve_icon(name)?;
    with_catalog(|provider| provider.svg_source(name, &source))
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
        None => { let (family, glyph) = fallback_glyph(); glyph_element(name, family, glyph) },
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
    let is_animated = el.icon_playback.is_some();
    let is_src = el.image_texture.is_some()
        || el.get_attr(ATTR_ICON_SRC).is_some()
        || el.get_attr(ATTR_ICON_CHANGE_ANIMATION).is_some()
        || is_animated;
    let el = el.text_size(px);
    if is_src {
        icon_box(el, px)
    } else {
        el
    }
}

/// Configure `<Icon>` motion. `hover` accepts `animated`, `none`, a built-in
/// keyframe name (e.g. `spin`), or a CSS animation shorthand. `change="morph"`
/// uses MorphIcon for Lucide and local stroke SVG sources (requires `svg`).
/// Call before applying size/classes, as the macro does.
pub fn with_icon_animations(
    mut el: Element,
    hover: impl AsRef<str>,
    change: impl AsRef<str>,
    id: impl AsRef<str>,
    spring: impl AsRef<str>,
) -> Element {
    let hover = hover.as_ref().trim();
    let name = el.get_attr(ATTR_ICON_NAME).cloned();
    let source = el
        .get_attr(ATTR_ICON_SRC)
        .cloned()
        .map(IconSource::Src)
        .or_else(|| name.as_deref().and_then(resolve_icon));
    #[cfg(feature = "svg")]
    if change.as_ref() == "morph" {
        let src = morph_source(name.as_deref(), source.as_ref());
        el = crate::utils::morph_icon(src.as_deref().unwrap_or(""))
            .id(id.as_ref())
            .size_px(DEFAULT_ICON_SIZE_PX)
            .spring(spring.as_ref())
            .into();
        el = el.pointer_events("bounding-box");
        el.attrs.remove("aria-hidden");
        if src.is_none() {
            apply_resolved(&mut el, name.as_deref(), source.clone());
        }
        el.set_attr(ATTR_ICON_CHANGE_ANIMATION, "morph");
        // Keep the provider/source identity separate from the normalized morph path.
        if let Some(name) = &name {
            el.set_attr(ATTR_ICON_NAME, name);
        }
        if let Some(IconSource::Src(src)) = &source {
            el.set_attr(ATTR_ICON_SRC, src);
        }
    }
    #[cfg(not(feature = "svg"))]
    let _ = (change, spring);
    if hover.is_empty() || hover == "none" || hover == "false" {
        return el;
    }
    if el.template_id.is_none() {
        el.template_id = Some(format!("icon-motion:{}", id.as_ref()));
    }
    el.set_attr(ATTR_ICON_HOVER_ANIMATION, hover);
    el.set_attr(ATTR_ICON_ANIMATION_STATE, "normal");
    if hover == "animated" {
        sync_icon_recipe(&mut el, name.as_deref(), source.as_ref());
        if el.icon_playback.is_none() {
            el.attrs.remove(ATTR_ICON_HOVER_ANIMATION);
            el.attrs.remove(ATTR_ICON_ANIMATION_STATE);
            return el;
        }
    }
    with_icon_hover(el.pointer_events("bounding-box"))
}

pub fn with_icon_hover(el: Element) -> Element {
    let tid = el.template_id.clone().expect("icon identity");
    crate::primitives::interactive::interactive(el)
        .cursor("pointer")
        .on_hover(move |on| {
            if let Ok(mut map) = hover_intent_map().lock() {
                map.insert(tid.clone(), on);
            }
            crate::primitives::interactive::mark_hover_paint_dirty();
            crate::app::runtime::request_redraw();
        })
        .into()
}

pub(super) fn hover_intent_map() -> std::rc::Rc<Mutex<HashMap<String, bool>>> {
    crate::authoring::window::window_local! {
        static INTENTS: std::rc::Rc<Mutex<HashMap<String, bool>>> = std::rc::Rc::new(Mutex::new(HashMap::new()));
    }
    INTENTS.with(Clone::clone)
}

pub fn release_icon_hover(template_id: &str) {
    if let Ok(mut map) = hover_intent_map().lock() {
        map.remove(template_id);
    }
}

pub fn sync_icon_hover(el: &mut Element) {
    let intents = hover_intent_map()
        .lock()
        .ok()
        .map(|map| map.clone())
        .unwrap_or_default();
    if !intents.is_empty() {
        sync_hover_rec(el, &intents);
    }
}

fn sync_hover_rec(el: &mut Element, intents: &HashMap<String, bool>) {
    if el.collect_paint_hash != 0 && !el.collect_skip_live {
        return;
    }
    let is_animated = el.icon_playback.is_some();
    let controlled = el.icon_playback.is_some_and(|playback| playback.controlled);
    if (is_animated || el.get_attr(ATTR_ICON_HOVER_ANIMATION).is_some()) && !controlled {
        if let Some(tid) = el.template_id.clone() {
            if let Some(&on) = intents.get(&tid) {
                sync_icon_hover_animation(el, on);
                let want = if on { "animate" } else { "normal" };
                let current = el
                    .get_attr(ATTR_ICON_ANIMATION_STATE)
                    .map(String::as_str)
                    .unwrap_or("normal");
                if current != want {
                    el.set_attr(ATTR_ICON_ANIMATION_STATE, want);
                    if let Some(playback) = el.icon_playback {
                        (playback.set_state)(el, want);
                    }
                }
            }
        }
    }
    for child in &mut el.children {
        sync_hover_rec(child, intents);
    }
}

pub(crate) fn sync_icon_hover_animation(el: &mut Element, on: bool) {
    let Some(spec) = el.get_attr(ATTR_ICON_HOVER_ANIMATION).cloned() else {
        return;
    };
    if spec == "animated" {
        return;
    }
    #[cfg(feature = "svg")]
    let on = on && !crate::utils::morph_icon::blocks_hover_animation(el);
    let on = on && !crate::style::prefers_reduced_motion();
    let active = if on { "1" } else { "0" };
    if el.get_attr("jadix_icon_hover_active").map(String::as_str) == Some(active) {
        return;
    }
    el.set_attr("jadix_icon_hover_active", active);
    // Reuse the CSS keyframe engine, including application-defined keyframes.
    let spec = if spec.split_whitespace().count() == 1 {
        format!(
            "{} 1s linear infinite",
            spec.strip_prefix("animate-").unwrap_or(&spec)
        )
    } else {
        spec
    };
    el.style.animations = if on {
        crate::core::tree::parse_animation_shorthand(&spec)
    } else {
        Vec::new()
    };
    let animations = el.style.animations.clone();
    if let Some(base) = el.class_set.as_mut().and_then(|cs| cs.base_style.as_mut()) {
        base.animations = animations;
    }
}

#[cfg(feature = "svg")]
fn morph_source(name: Option<&str>, source: Option<&IconSource>) -> Option<String> {
    match source? {
        IconSource::Glyph { .. } => {
            let name = name?;
            with_catalog(|provider| provider.svg_source(name, source?)).map(|_| name.to_string())
        }
        IconSource::Src(src) => {
            let bytes = if src.trim_start().starts_with('<') {
                src.as_bytes().to_vec()
            } else if src.starts_with("data:") {
                crate::media::decode_data_url(src)?
            } else {
                std::fs::read(crate::media::resolve_asset_path(src)?).ok()?
            };
            let input = crate::utils::svg_to_icon(std::str::from_utf8(&bytes).ok()?).ok()?;
            crate::utils::canonical_d(&input).ok()
        }
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
    #[cfg(feature = "svg")]
    if el.get_attr(ATTR_ICON_CHANGE_ANIMATION).is_some() {
        if let Some(src) = morph_source(name, source.as_ref()) {
            let family = match source.as_ref() {
                Some(IconSource::Glyph { family, .. }) => family.as_str(),
                _ => "sans-serif",
            };
            let layout_changed = el.image_texture.is_some()
                || el.text_style.font_family.as_deref() != Some(family);
            let status = crate::utils::apply_bound_morph_icon(el, src);
            el.image_texture = None;
            el.text_style.font_family = Some(family.into());
            el.attrs.remove(ATTR_ICON_SRC);
            if let Some(name) = name {
                el.set_attr(ATTR_ICON_NAME, name);
            }
            if let Some(IconSource::Src(src)) = &source {
                el.set_attr(ATTR_ICON_SRC, src);
            }
            sync_icon_recipe(el, name, source.as_ref());
            return status.merge(if layout_changed {
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::StyleChanged
            });
        }
        el.attrs.remove(crate::utils::ATTR_MORPH_ICON);
    }
    sync_icon_recipe(el, name, source.as_ref());
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
            let (family, fallback) = fallback_glyph();
            if el.text.as_deref() == Some(fallback.as_str())
                && el.text_style.font_family.as_deref() == Some(family.as_str())
                && el.image_texture.is_none()
            {
                return LayoutStatus::Clean;
            }
            paint_glyph(el, name, family, fallback);
            LayoutStatus::LayoutChanged
        }
    }
}

pub fn refresh_icon_animation(element: &mut Element, name: &str) {
    let source = resolve_icon(name);
    sync_icon_recipe(element, Some(name), source.as_ref());
}

fn sync_icon_recipe(element: &mut Element, name: Option<&str>, source: Option<&IconSource>) {
    if element.get_attr(ATTR_ICON_HOVER_ANIMATION).map(String::as_str) != Some("animated") { return; }
    let installed = name.zip(source).and_then(|(name, source)| {
        with_catalog(|provider| provider.configure_animation(element, name, source).then_some(()))
    }).is_some();
    if !installed {
        if let Some(playback) = element.icon_playback.take() { (playback.detach)(element); }
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

#[cfg(all(test, feature = "lucide"))]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "animation-base")]
    fn icon_hover_animation_uses_css_and_restores_idle() {
        let mut el = with_icon_animations(
            build_icon_named("settings"),
            "spin",
            "none",
            "icon-spin-test",
            "snappy",
        )
        .class("text-foreground");
        assert!(el.style.animations.is_empty());
        (el.on_hover.as_ref().unwrap().borrow_mut())(true);
        sync_icon_hover(&mut el);
        assert_eq!(el.style.animations[0].name, "spin");
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        el.layout_bounds = [0.0, 0.0, 24.0, 24.0];
        let mut impact = LayoutStatus::Clean;
        crate::style::utility::sync_class_variants_subtree(
            &mut el,
            [10.0, 10.0],
            "",
            false,
            200.0,
            200.0,
            &mut tree,
            &mut impact,
        );
        assert_ne!(
            impact,
            LayoutStatus::Clean,
            "CSS hover must reach the frame engine"
        );
        assert_eq!(
            el.class_set
                .as_ref()
                .unwrap()
                .base_style
                .as_ref()
                .unwrap()
                .animations[0]
                .name,
            "spin"
        );
        (el.on_hover.as_ref().unwrap().borrow_mut())(false);
        sync_icon_hover(&mut el);
        assert!(el.style.animations.is_empty());
        let mut custom = with_icon_animations(
            build_icon_named("heart"),
            "pulse 700ms ease-in-out infinite",
            "none",
            "icon-custom-test",
            "snappy",
        );
        sync_icon_hover_animation(&mut custom, true);
        assert_eq!(custom.style.animations[0].name, "pulse");
        let plain = with_icon_animations(
            build_icon_named("sun"),
            "none",
            "none",
            "icon-none-test",
            "snappy",
        );
        assert!(plain.on_hover.is_none());
    }

    #[cfg(all(
        feature = "svg",
        feature = "bundled-lucide",
        feature = "lucide-animated"
    ))]
    #[test]
    fn icon_motion_macro_binds_name_and_arbitrates_hover_and_change() {
        let mut el = crate::ui! {
            <Icon name="sun" size="28px" hover_animation="animated" change_animation="morph" spring="smooth" />
        };
        assert_eq!(el.text_style.size, Some(28.0));
        assert_eq!(
            el.get_attr(crate::utils::morph_icon::ATTR_MORPH_SPRING)
                .map(String::as_str),
            Some("smooth")
        );
        assert_eq!(
            el.get_attr(crate::utils::ATTR_ANIMATED_ICON)
                .map(String::as_str),
            Some("sun")
        );
        (el.on_hover.as_ref().unwrap().borrow_mut())(true);
        crate::utils::sync_animated_icon_hover(&mut el);
        assert_eq!(
            apply_bound_icon_name(&mut el, "moon"),
            LayoutStatus::StyleChanged
        );
        let recipe_tick = crate::utils::lucide_animated::tick_animated_icon(&mut el, 0.016);
        if cfg!(feature = "animation-ex") {
            assert!(!recipe_tick);
        }
        assert_eq!(
            crate::utils::morph_icon::tick_morph_icon(&mut el, 0.016),
            cfg!(feature = "animation-ex")
        );
        assert_eq!(
            el.get_attr(crate::utils::ATTR_ANIMATED_ICON)
                .map(String::as_str),
            Some("moon")
        );

        let mut spin = crate::ui! {
            <Icon name="sun" size="28px" hover_animation="spin" change_animation="morph" />
        };
        (spin.on_hover.as_ref().unwrap().borrow_mut())(true);
        crate::utils::sync_animated_icon_hover(&mut spin);
        assert!(!spin.style.animations.is_empty());
        apply_bound_icon_name(&mut spin, "moon");
        crate::utils::sync_animated_icon_hover(&mut spin);
        assert_eq!(
            spin.style.animations.is_empty(),
            cfg!(feature = "animation-ex"),
            "only an active morph owns the geometry during a change"
        );
        assert_eq!(
            crate::utils::morph_icon::tick_morph_icon(&mut spin, 10.0),
            cfg!(feature = "animation-ex")
        );
        crate::utils::sync_animated_icon_hover(&mut spin);
        assert_eq!(spin.style.animations[0].name, "spin");
    }

    #[cfg(all(feature = "svg", feature = "bundled-lucide"))]
    #[test]
    fn icon_motion_morphs_svg_sources_and_falls_back_for_raster_and_custom_glyphs() {
        let a = r#"<svg viewBox="0 0 24 24" fill="none"><path d="M4 12h16"/></svg>"#;
        let b = r#"<svg viewBox="0 0 24 24" fill="none"><path d="M12 4v16"/></svg>"#;
        let mut el = with_icon_animations(
            build_icon_from_src(svg_markup_to_data_url(a)),
            "spin",
            "morph",
            "icon-svg-test",
            "snappy",
        );
        assert!(el.image_texture.is_none());
        apply_bound_icon_src(&mut el, svg_markup_to_data_url(b));
        assert_eq!(
            crate::utils::subtree_needs_morph_icon_frame(&el),
            cfg!(feature = "animation-ex")
        );
        assert!(el.image_texture.is_none());
        apply_bound_icon_src(&mut el, RED_PNG);
        assert!(el.image_texture.is_some());
        assert!(el.get_attr(crate::utils::ATTR_MORPH_ICON).is_none());
        sync_icon_hover_animation(&mut el, true);
        if cfg!(feature = "animation-base") {
            assert_eq!(
                el.style.animations[0].name, "spin",
                "a stale SVG flight must not block raster hover"
            );
        } else {
            assert!(el.style.animations.is_empty());
        }
        apply_bound_icon_name(&mut el, "sun");
        assert!(el.image_texture.is_none());
        assert!(el.get_attr(crate::utils::ATTR_MORPH_ICON).is_some());
        assert!(
            !crate::utils::subtree_needs_morph_icon_frame(&el),
            "raster to SVG snaps to the first morphable icon"
        );

        let mut custom = with_icon_animations(
            build_icon_named("sun"),
            "animated",
            "morph",
            "icon-glyph-test",
            "snappy",
        );
        apply_resolved(
            &mut custom,
            Some("sun"),
            Some(IconSource::Glyph {
                family: "custom".into(),
                glyph: "X".into(),
            }),
        );
        assert_eq!(custom.text.as_deref(), Some("X"));
        #[cfg(feature = "lucide-animated")]
        assert!(custom.get_attr(crate::utils::ATTR_ANIMATED_ICON).is_none());
        assert!(custom.get_attr(crate::utils::ATTR_MORPH_ICON).is_none());
    }

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

#[cfg(test)]
mod window_tests {
    #[test]
    fn same_named_hover_intents_are_isolated_and_released_with_the_window() {
        use crate::app::window_scope::{self, WindowScope};
        let primary = WindowScope::Primary;
        let secondary = WindowScope::Secondary(805);
        crate::authoring::window::activate(primary);
        crate::authoring::window::activate(secondary);
        let _primary = window_scope::enter(primary);
        super::hover_intent_map().lock().unwrap().insert("same-icon".into(), true);
        {
            let _secondary = window_scope::enter(secondary);
            assert!(!super::hover_intent_map().lock().unwrap().contains_key("same-icon"));
            super::hover_intent_map().lock().unwrap().insert("same-icon".into(), false);
            crate::authoring::window::forget(secondary);
            assert!(super::hover_intent_map().lock().unwrap().is_empty());
        }
        assert_eq!(super::hover_intent_map().lock().unwrap().get("same-icon"), Some(&true));
        super::release_icon_hover("same-icon");
    }
}
