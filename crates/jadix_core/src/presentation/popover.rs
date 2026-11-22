#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopoverPlacement {
    TopStart,
    Top,
    TopEnd,
    RightStart,
    Right,
    RightEnd,
    #[default]
    BottomStart,
    Bottom,
    BottomEnd,
    LeftStart,
    Left,
    LeftEnd,
}

/// Resolved popover geometry in logical points relative to the root's top-left.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverPosition {
    pub frame: super::Rect,
    pub placement: PopoverPlacement,
    pub available_width: f64,
    pub available_height: f64,
}

impl PopoverPosition {
    pub fn is_valid(self) -> bool {
        self.frame.is_valid()
            && [self.available_width, self.available_height]
                .into_iter()
                .all(|size| size.is_finite() && size >= 0.0)
    }
}
