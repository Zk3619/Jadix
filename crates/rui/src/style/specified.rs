//! Author-specified animatable values (not polluted by tick present writes).
//!
//! CSS keeps specified and animated used values in separate slots. RUI's `Style` is
//! the present (paint/layout) slot; this map is the specified slot (ANIM-1).

use std::collections::HashMap;

use super::animatable::{values_close, AnimatableValue, PropertyId};
use super::element_style::Style;
use super::text_style::TextStyle;
use super::used_layout::UsedLayout;

/// Sparse specified values for properties that animation / transition may touch.
#[derive(Clone, Debug, Default)]
pub struct SpecifiedPresent {
    values: HashMap<PropertyId, AnimatableValue>,
}

impl SpecifiedPresent {
    pub fn get(&self, id: PropertyId) -> Option<&AnimatableValue> {
        self.values.get(&id)
    }

    pub fn insert(&mut self, id: PropertyId, v: AnimatableValue) {
        self.values.insert(id, v);
    }

    /// Update specified from the current style, skipping fields that still match the
    /// last present write (`written`).
    pub fn capture_author(
        &mut self,
        style: &Style,
        text: &TextStyle,
        written: &HashMap<PropertyId, AnimatableValue>,
        ids: impl IntoIterator<Item = PropertyId>,
    ) {
        for id in ids {
            let current = super::animatable::read_value(style, text, id);
            match written.get(&id) {
                Some(w) if values_close(w, &current) => {
                    self.values.entry(id).or_insert(current);
                }
                _ => {
                    self.values.insert(id, current);
                }
            }
        }
    }

    /// Cascaded used value for implicit 0%/100% keyframes.
    pub fn resolved_for_base(
        &self,
        id: PropertyId,
        style: &Style,
        text: &TextStyle,
        used: &UsedLayout,
    ) -> AnimatableValue {
        let raw = self
            .values
            .get(&id)
            .cloned()
            .unwrap_or_else(|| super::animatable::read_value(style, text, id));
        super::animatable::resolve_used(id, raw, used)
    }
}
