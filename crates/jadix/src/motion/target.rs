use std::collections::BTreeMap;

use crate::style::{AnimatableValue, PropertyId};

/// A partial pose. Unspecified properties retain their authored baseline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MotionTarget {
    pub(super) values: BTreeMap<PropertyId, AnimatableValue>,
}

impl MotionTarget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn overlay(mut self, target: &Self) -> Self {
        self.values.extend(target.values.clone());
        self
    }

    fn with(mut self, property: PropertyId, value: AnimatableValue) -> Self {
        assert!(
            super::components(&value)
                .iter()
                .all(|value| value.is_finite()),
            "motion targets must be finite"
        );
        self.values.insert(property, value);
        self
    }

    pub fn translate(self, value: [f32; 2]) -> Self {
        self.with(PropertyId::Translate, AnimatableValue::Vec2(value))
    }
    pub fn scale(self, value: f32) -> Self {
        self.scale_xy([value, value])
    }
    pub fn scale_xy(self, value: [f32; 2]) -> Self {
        self.with(PropertyId::Scale, AnimatableValue::Vec2(value))
    }
    pub fn rotate(self, degrees: f32) -> Self {
        self.with(PropertyId::Rotate, AnimatableValue::Scalar(degrees))
    }
    pub fn opacity(self, value: f32) -> Self {
        self.with(
            PropertyId::Opacity,
            AnimatableValue::Scalar(value.clamp(0.0, 1.0)),
        )
    }
    pub fn width(self, pixels: f32) -> Self {
        assert!(pixels.is_finite(), "motion dimensions must be finite");
        self.with(PropertyId::Width, AnimatableValue::Scalar(pixels.max(0.0)))
    }
    pub fn height(self, pixels: f32) -> Self {
        assert!(pixels.is_finite(), "motion dimensions must be finite");
        self.with(PropertyId::Height, AnimatableValue::Scalar(pixels.max(0.0)))
    }
    pub fn border_radius(self, pixels: f32) -> Self {
        self.radii([pixels; 4])
    }
    pub fn radii(self, pixels: [f32; 4]) -> Self {
        self.with(PropertyId::BorderRadius, AnimatableValue::Vec4(pixels))
    }
    pub fn background_color(self, rgba: [f32; 4]) -> Self {
        self.with(PropertyId::BackgroundColor, AnimatableValue::Color(rgba))
    }
    pub fn color(self, rgba: [f32; 4]) -> Self {
        self.with(PropertyId::Color, AnimatableValue::Color(rgba))
    }
    pub fn border_color(self, rgba: [f32; 4]) -> Self {
        self.with(PropertyId::BorderColor, AnimatableValue::Color(rgba))
    }
    pub fn gap(self, pixels: [f32; 2]) -> Self {
        self.with(PropertyId::Gap, AnimatableValue::Vec2(pixels))
    }
    pub fn padding(self, pixels: [f32; 4]) -> Self {
        self.with(PropertyId::Padding, AnimatableValue::Vec4(pixels))
    }
}
