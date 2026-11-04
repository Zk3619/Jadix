use super::NodeContent;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
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
}

impl BackendCapabilities {
    pub fn headless() -> Self {
        Self {
            node_kinds: vec![
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
        }
    }

    pub fn supports(&self, content: &NodeContent) -> bool {
        self.node_kinds.contains(&content.kind())
            && (!matches!(
                content,
                NodeContent::Input { .. } | NodeContent::Textarea { .. }
            ) || self.text_editing)
            && (!matches!(content, NodeContent::Scroll) || self.scrolling)
    }
}
