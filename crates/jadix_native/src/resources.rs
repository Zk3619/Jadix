use crate::BackendError;
use crate::presentation::{NodeContent, NodeDescriptor, PresentationNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Nodes,
    RetiredNodes,
    RetiredRanges,
    TextBytes,
    ImageBytes,
    ImagePixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub nodes: usize,
    pub retired_nodes: usize,
    pub retired_ranges: usize,
    pub text_bytes: usize,
    pub image_bytes: usize,
    pub image_pixels: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            nodes: 4096,
            retired_nodes: 65536,
            retired_ranges: 4096,
            text_bytes: 8 * 1024 * 1024,
            image_bytes: 8 * 1024 * 1024,
            image_pixels: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceUsage {
    pub nodes: usize,
    pub retired_nodes: usize,
    pub retired_ranges: usize,
    pub text_bytes: usize,
    pub image_bytes: usize,
    pub image_pixels: u64,
}

impl ResourceUsage {
    fn add_text(&mut self, text: &str, limits: ResourceLimits) -> Result<(), BackendError> {
        self.text_bytes = self
            .text_bytes
            .checked_add(text.len())
            .filter(|bytes| *bytes <= limits.text_bytes)
            .ok_or(BackendError::ResourceLimit(ResourceKind::TextBytes))?;
        Ok(())
    }

    pub(crate) fn add(
        &mut self,
        descriptor: &NodeDescriptor,
        limits: ResourceLimits,
    ) -> Result<(), BackendError> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .filter(|nodes| *nodes <= limits.nodes)
            .ok_or(BackendError::ResourceLimit(ResourceKind::Nodes))?;
        if let Some(name) = &descriptor.accessible_name {
            self.add_text(name, limits)?;
        }
        match &descriptor.content {
            NodeContent::Text(text)
            | NodeContent::Button { label: text, .. }
            | NodeContent::Checkbox { label: text, .. }
            | NodeContent::Switch { label: text, .. } => self.add_text(text, limits)?,
            NodeContent::Input {
                value, placeholder, ..
            }
            | NodeContent::Textarea {
                value, placeholder, ..
            } => {
                self.add_text(value, limits)?;
                self.add_text(placeholder, limits)?;
            }
            NodeContent::Select {
                options,
                value,
                placeholder,
                ..
            } => {
                self.add_text(value, limits)?;
                self.add_text(placeholder, limits)?;
                for option in options {
                    self.add_text(&option.value, limits)?;
                    self.add_text(&option.label, limits)?;
                }
            }
            NodeContent::Image { source, alt } => {
                self.add_text(alt, limits)?;
                self.image_bytes = self
                    .image_bytes
                    .checked_add(source.len())
                    .filter(|bytes| *bytes <= limits.image_bytes)
                    .ok_or(BackendError::ResourceLimit(ResourceKind::ImageBytes))?;
                let [width, height] = descriptor
                    .content
                    .image_dimensions()
                    .ok_or(BackendError::InvalidContent)?;
                self.image_pixels = self
                    .image_pixels
                    .checked_add(u64::from(width) * u64::from(height))
                    .filter(|pixels| *pixels <= limits.image_pixels)
                    .ok_or(BackendError::ResourceLimit(ResourceKind::ImagePixels))?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn tree_usage(
    roots: &[PresentationNode],
    limits: ResourceLimits,
) -> Result<ResourceUsage, BackendError> {
    if roots.len() > limits.nodes {
        return Err(BackendError::ResourceLimit(ResourceKind::Nodes));
    }
    let mut usage = ResourceUsage::default();
    let mut pending: Vec<_> = roots.iter().collect();
    while let Some(node) = pending.pop() {
        usage.add(&node.descriptor, limits)?;
        if node.children.len() > limits.nodes - usage.nodes - pending.len() {
            return Err(BackendError::ResourceLimit(ResourceKind::Nodes));
        }
        pending.extend(&node.children);
    }
    Ok(usage)
}
