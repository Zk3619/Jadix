//! No-op stubs when `lucide-animated` feature is off (no serde / recipe JSON).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::tree::{Element, IntoElementList};
use crate::utils::lucide::lucide_glyph;

pub const ATTR_ANIMATED_ICON: &str = "rui_animated_icon";
pub const ATTR_ANIMATED_ICON_STATE: &str = "rui_animated_icon_state";
pub const ATTR_ANIMATED_ICON_CONTROLLED: &str = "rui_animated_icon_controlled";
pub const ATTR_ANIMATED_ICON_SIZE: &str = "rui_animated_icon_size";

#[derive(Debug, Clone, Default)]
pub struct AnimatedIconRecipe {
    pub id: String,
    pub lucide: String,
}

pub fn animated_icon_recipe(_name: &str) -> Option<&'static AnimatedIconRecipe> {
    None
}

pub fn animated_icon_names() -> impl Iterator<Item = &'static str> {
    std::iter::empty()
}

pub fn tick_animated_icon(_el: &mut Element, _dt: f32) -> bool {
    false
}

pub fn set_animated_icon_state(_el: &mut Element, _state: &str) {}

pub fn start_animated_icon(_el: &mut Element) {}

pub fn stop_animated_icon(_el: &mut Element) {}

pub fn ensure_animated_icon_texture(
    _el: &Element,
    _size_px: f32,
    _color: [u8; 4],
    _dpr: f32,
) -> Option<(String, f32)> {
    None
}

pub fn subtree_needs_animated_icon_frame(_el: &Element) -> bool {
    false
}

pub fn tick_animated_icons_subtree(_el: &mut Element, _dt: f32) -> bool {
    false
}

pub fn sync_animated_icon_hover(_el: &mut Element) {}

pub struct AnimatedIconBuilder {
    name: String,
    size: f32,
}

pub fn animated_icon(name: impl Into<String>) -> AnimatedIconBuilder {
    AnimatedIconBuilder {
        name: name.into(),
        size: 24.0,
    }
}

impl AnimatedIconBuilder {
    pub fn size(mut self, px: f32) -> Self {
        self.size = px.clamp(8.0, 256.0);
        self
    }

    pub fn size_px(self, px: f32) -> Self {
        self.size(px)
    }

    pub fn color(self, _rgba: [u8; 4]) -> Self {
        self
    }

    pub fn controlled(self, _yes: bool) -> Self {
        self
    }

    pub fn pointer_events(self, _mode: impl Into<String>) -> Self {
        self
    }
}

impl From<AnimatedIconBuilder> for Element {
    fn from(b: AnimatedIconBuilder) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let name = b.name;
        let size = b.size;
        let mut el = Element::new()
            .w(size)
            .h(size)
            .min_w(size)
            .min_h(size)
            .text_size(size);
        el.set_attr(ATTR_ANIMATED_ICON, &name);
        el.set_attr(ATTR_ANIMATED_ICON_STATE, "normal");
        el.set_attr(ATTR_ANIMATED_ICON_SIZE, format!("{size}"));
        let glyph = lucide_glyph(&name).unwrap_or_else(|| "\u{ea0e}".into());
        el.text = Some(glyph);
        el.text_style.font_family = Some("lucide".to_string());
        el.set_attr(crate::utils::lucide::ATTR_ICON_NAME, &name);
        let _ = SEQ.fetch_add(1, Ordering::Relaxed);
        el
    }
}

impl IntoElementList for AnimatedIconBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// `ui!` / component entry when `lucide-animated` is off (static Lucide glyph fallback).
#[allow(non_snake_case)]
pub fn AnimatedIcon(
    name: impl AsRef<str>,
    size: Option<f32>,
    controlled: Option<bool>,
) -> Element {
    let mut b = animated_icon(name.as_ref()).size_px(size.unwrap_or(24.0));
    if controlled.unwrap_or(false) {
        b = b.controlled(true);
    }
    b.into()
}
