#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Row,
    #[default]
    Column,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeStyle {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub axis: Axis,
    pub gap: f32,
    pub padding: [f32; 4],
    pub grow: f32,
    pub shrink: f32,
    pub scrollbars: [bool; 2],
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
            axis: Axis::Column,
            gap: 0.0,
            padding: [0.0; 4],
            grow: 0.0,
            shrink: 1.0,
            scrollbars: [true; 2],
            foreground: [0, 0, 0, 255],
            background: [0, 0, 0, 0],
            font_family: String::from("system-ui"),
            font_size: 16.0,
            font_weight: 400,
        }
    }
}

impl NodeStyle {
    pub fn is_valid(&self) -> bool {
        let nonnegative = |value: f32| value.is_finite() && value >= 0.0;
        [self.width, self.height, self.max_width, self.max_height]
            .into_iter()
            .flatten()
            .all(nonnegative)
            && [
                self.min_width,
                self.min_height,
                self.gap,
                self.grow,
                self.shrink,
            ]
            .into_iter()
            .chain(self.padding)
            .all(nonnegative)
            && self.max_width.is_none_or(|max| max >= self.min_width)
            && self.max_height.is_none_or(|max| max >= self.min_height)
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && (1..=1000).contains(&self.font_weight)
            && !self.font_family.is_empty()
            && self.font_family.len() <= 1024
    }
}
