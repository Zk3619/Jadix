use super::{NodeContent, NodeDescriptor};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Hosted,
    Container,
    Text,
    Button,
    Input,
    Textarea,
    Checkbox,
    Switch,
    Select,
    Scroll,
    Image,
    Dialog,
    Popover,
}

impl NodeContent {
    pub fn kind(&self) -> NodeKind {
        match self {
            Self::Hosted { .. } => NodeKind::Hosted,
            Self::Container => NodeKind::Container,
            Self::Text(_) => NodeKind::Text,
            Self::Button { .. } => NodeKind::Button,
            Self::Input { .. } => NodeKind::Input,
            Self::Textarea { .. } => NodeKind::Textarea,
            Self::Checkbox { .. } => NodeKind::Checkbox,
            Self::Switch { .. } => NodeKind::Switch,
            Self::Select { .. } => NodeKind::Select,
            Self::Scroll => NodeKind::Scroll,
            Self::Image { .. } => NodeKind::Image,
            Self::Dialog { .. } => NodeKind::Dialog,
            Self::Popover { .. } => NodeKind::Popover,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub node_kinds: Vec<NodeKind>,
    pub text_editing: bool,
    pub focus: bool,
    pub scrolling: bool,
    pub inline_raster_sources: bool,
    pub inline_svg_sources: bool,
    pub local_image_sources: bool,
    pub remote_image_sources: bool,
}

impl BackendCapabilities {
    pub fn headless() -> Self {
        Self {
            node_kinds: vec![
                NodeKind::Hosted,
                NodeKind::Container,
                NodeKind::Text,
                NodeKind::Button,
                NodeKind::Input,
                NodeKind::Textarea,
                NodeKind::Checkbox,
                NodeKind::Switch,
                NodeKind::Select,
                NodeKind::Scroll,
                NodeKind::Image,
                NodeKind::Dialog,
                NodeKind::Popover,
            ],
            text_editing: true,
            focus: true,
            scrolling: true,
            inline_raster_sources: false,
            inline_svg_sources: false,
            local_image_sources: false,
            remote_image_sources: false,
        }
    }

    pub fn supports(&self, content: &NodeContent) -> bool {
        self.node_kinds.contains(&content.kind())
            && (!matches!(
                content,
                NodeContent::Input { .. } | NodeContent::Textarea { .. }
            ) || self.text_editing)
            && (!matches!(content, NodeContent::Scroll) || self.scrolling)
            && match content {
                NodeContent::Image { source, .. } => self.supports_image_source(source),
                _ => true,
            }
    }

    pub fn supports_descriptor(&self, descriptor: &NodeDescriptor) -> bool {
        self.supports(&descriptor.content)
            && descriptor
                .style
                .image_placeholder
                .as_deref()
                .is_none_or(|source| self.supports_image_source(source))
    }

    fn supports_image_source(&self, source: &str) -> bool {
        (self.inline_raster_sources || !NodeContent::is_inline_raster_source(source))
            && (self.inline_svg_sources || !NodeContent::is_inline_svg_source(source))
            && (self.local_image_sources || !NodeContent::is_local_image_source(source))
            && (self.remote_image_sources || !NodeContent::is_remote_image_source(source))
    }
}
