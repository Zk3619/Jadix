use super::{style::NodeStyle, NodeContent, PresentationNode};
use crate::environment::{ColorScheme, EnvironmentSnapshot};

#[derive(Clone, Debug, PartialEq)]
pub struct ConsistentTheme {
    pub light_surface: [u8; 4],
    pub light_control: [u8; 4],
    pub light_disabled_control: [u8; 4],
    pub light_text: [u8; 4],
    pub light_disabled_text: [u8; 4],
    pub dark_surface: [u8; 4],
    pub dark_control: [u8; 4],
    pub dark_disabled_control: [u8; 4],
    pub dark_text: [u8; 4],
    pub dark_disabled_text: [u8; 4],
    pub high_contrast_surface: [u8; 4],
    pub high_contrast_control: [u8; 4],
    pub high_contrast_disabled_control: [u8; 4],
    pub high_contrast_text: [u8; 4],
    pub high_contrast_disabled_text: [u8; 4],
    pub font_family: String,
    pub font_size: f32,
    pub control_height: f32,
    pub spacing: f32,
}

impl Default for ConsistentTheme {
    fn default() -> Self {
        Self {
            light_surface: [255, 255, 255, 255],
            light_control: [244, 244, 245, 255],
            light_disabled_control: [228, 228, 231, 255],
            light_text: [24, 24, 27, 255],
            light_disabled_text: [113, 113, 122, 255],
            dark_surface: [24, 24, 27, 255],
            dark_control: [39, 39, 42, 255],
            dark_disabled_control: [63, 63, 70, 255],
            dark_text: [244, 244, 245, 255],
            dark_disabled_text: [161, 161, 170, 255],
            high_contrast_surface: [0, 0, 0, 255],
            high_contrast_control: [0, 0, 128, 255],
            high_contrast_disabled_control: [32, 32, 32, 255],
            high_contrast_text: [255, 255, 255, 255],
            high_contrast_disabled_text: [255, 255, 0, 255],
            font_family: "system-ui".into(),
            font_size: 16.0,
            control_height: 44.0,
            spacing: 8.0,
        }
    }
}

impl ConsistentTheme {
    pub fn project(
        &self,
        roots: &[PresentationNode],
        environment: &EnvironmentSnapshot,
    ) -> Result<Vec<PresentationNode>, &'static str> {
        environment
            .validate()
            .map_err(|_| "invalid theme environment")?;
        if !(NodeStyle {
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            ..NodeStyle::default()
        })
        .is_valid()
            || !self.control_height.is_finite()
            || self.control_height < 0.0
            || !self.spacing.is_finite()
            || self.spacing < 0.0
        {
            return Err("invalid consistent theme");
        }
        let mut projected = roots.to_vec();
        let defaults = NodeStyle::default();
        let dark = environment.color_scheme == Some(ColorScheme::Dark);
        let high_contrast = environment.high_contrast == Some(true);
        let surface = if high_contrast {
            self.high_contrast_surface
        } else if dark {
            self.dark_surface
        } else {
            self.light_surface
        };
        let control = if dark {
            (self.dark_control, self.dark_disabled_control)
        } else {
            (self.light_control, self.light_disabled_control)
        };
        let control = if high_contrast {
            (
                self.high_contrast_control,
                self.high_contrast_disabled_control,
            )
        } else {
            control
        };
        let foreground = if high_contrast {
            (self.high_contrast_text, self.high_contrast_disabled_text)
        } else if dark {
            (self.dark_text, self.dark_disabled_text)
        } else {
            (self.light_text, self.light_disabled_text)
        };
        let mut pending: Vec<_> = projected.iter_mut().map(|node| (node, true)).collect();
        while let Some((node, root)) = pending.pop() {
            let style = &mut node.descriptor.style;
            if !style.is_valid() {
                return Err("invalid source style");
            }
            let enabled = match &node.descriptor.content {
                NodeContent::Button { enabled, .. }
                | NodeContent::Input { enabled, .. }
                | NodeContent::Textarea { enabled, .. }
                | NodeContent::Checkbox { enabled, .. }
                | NodeContent::Switch { enabled, .. }
                | NodeContent::Select { enabled, .. } => Some(*enabled),
                _ => None,
            };
            let disabled = enabled == Some(false);
            if style.foreground == defaults.foreground {
                style.foreground = if disabled { foreground.1 } else { foreground.0 };
            }
            if style.font_family == defaults.font_family {
                style.font_family.clone_from(&self.font_family);
            }
            if style.font_size == defaults.font_size {
                style.font_size = self.font_size;
            }
            if style.direction.is_none() {
                style.direction = environment.layout_direction;
            }
            style.font_size *= environment.font_scale;
            let editable = matches!(
                node.descriptor.content,
                NodeContent::Input { .. }
                    | NodeContent::Textarea { .. }
                    | NodeContent::Select { .. }
            );
            let actionable = editable
                || matches!(
                    node.descriptor.content,
                    NodeContent::Button { .. }
                        | NodeContent::Checkbox { .. }
                        | NodeContent::Switch { .. }
                );
            let panel = matches!(
                node.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            );
            if style.background == defaults.background {
                if editable || actionable {
                    style.background = if disabled { control.1 } else { control.0 };
                } else if root || panel {
                    style.background = surface;
                }
            }
            if actionable {
                style.min_height = style
                    .min_height
                    .max(self.control_height * environment.font_scale);
            }
            if panel && style.padding == defaults.padding {
                style.padding = [self.spacing * 2.0; 4];
            }
            if !node.children.is_empty() && style.gap == defaults.gap {
                style.gap = self.spacing;
            }
            if !style.is_valid() {
                return Err("theme produces invalid style");
            }
            pending.extend(node.children.iter_mut().map(|child| (child, false)));
        }
        Ok(projected)
    }
}
