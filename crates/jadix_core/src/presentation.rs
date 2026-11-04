use std::sync::atomic::{AtomicU64, Ordering};
use std::{cell::Cell, rc::Rc};

pub mod capabilities;
mod popover;
pub use popover::PopoverPlacement;
pub mod component;
pub mod hybrid;
pub mod style;

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationNode {
    pub id: NodeId,
    pub descriptor: NodeDescriptor,
    pub children: Vec<PresentationNode>,
}

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootId(u64);

impl Default for RootId {
    fn default() -> Self {
        Self::new()
    }
}

impl RootId {
    pub fn token(self) -> String {
        self.0.to_string()
    }

    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("root identity exhausted"),
        )
    }
}

#[derive(Clone, Debug)]
pub struct RootLifetime {
    root: RootId,
    disposed: Rc<Cell<bool>>,
}

impl RootLifetime {
    pub fn new(root: RootId) -> Self {
        Self {
            root,
            disposed: Rc::new(Cell::new(false)),
        }
    }

    pub fn id(&self) -> RootId {
        self.root
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed.get()
    }

    pub fn dispose(&self) {
        self.disposed.set(true);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    pub root: RootId,
    generation: u64,
}

impl NodeId {
    pub fn generation(self) -> u64 {
        self.generation
    }

    pub fn token(self) -> String {
        self.generation.to_string()
    }

    pub fn new(root: RootId) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            root,
            generation: NEXT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("node identity exhausted"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn is_valid(self) -> bool {
        [
            self.x,
            self.y,
            self.width,
            self.height,
            self.x + self.width,
            self.y + self.height,
        ]
        .iter()
        .all(|value| value.is_finite())
            && self.width >= 0.0
            && self.height >= 0.0
    }

    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibleRole {
    RadioGroup,
    RadioItem { tab_stop: bool },
    TabList,
    TabItem,
    TabPanel,
    Tree,
    TreeItem,
    Grid { rows: u32, columns: u32 },
    GridCell { row: u32, column: u32 },
    GridColumnHeader { column: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeContent {
    Container,
    Text(String),
    Button {
        label: String,
        enabled: bool,
    },
    Input {
        value: String,
        placeholder: String,
        enabled: bool,
        password: bool,
        revision: u64,
    },
    Textarea {
        value: String,
        placeholder: String,
        enabled: bool,
        revision: u64,
    },
    Checkbox {
        label: String,
        checked: bool,
        enabled: bool,
        revision: u64,
    },
    Switch {
        label: String,
        checked: bool,
        enabled: bool,
        revision: u64,
    },
    Select {
        options: Vec<SelectOption>,
        value: String,
        placeholder: String,
        enabled: bool,
        revision: u64,
    },
    Scroll,
    Dialog {
        open: bool,
        close_on_overlay_click: bool,
        revision: u64,
    },
    Popover {
        anchor: NodeId,
        placement: PopoverPlacement,
        gap: u16,
        open: bool,
        close_on_outside_click: bool,
        revision: u64,
    },
    Image {
        source: String,
        alt: String,
    },
}

impl NodeContent {
    pub fn image_dimensions(&self) -> Option<[u32; 2]> {
        use base64::Engine;
        let Self::Image { source, alt } = self else {
            return None;
        };
        if source.len() > 1024 * 1024 || alt.len() > 4096 {
            return None;
        }
        let encoded = source.strip_prefix("data:image/png;base64,")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        if bytes.len() < 33
            || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
            || bytes[8..12] != 13u32.to_be_bytes()
            || &bytes[12..16] != b"IHDR"
        {
            return None;
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        (width > 0
            && height > 0
            && width <= 4096
            && height <= 4096
            && u64::from(width) * u64::from(height) <= 4 * 1024 * 1024)
            .then_some([width, height])
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Image { .. } => self.image_dimensions().is_some(),
            Self::Select {
                options,
                value,
                placeholder,
                ..
            } => {
                let mut values = std::collections::BTreeSet::new();
                options.len() <= 256
                    && placeholder.len() <= 4096
                    && options.iter().all(|option| {
                        !option.value.is_empty()
                            && option.value.len() <= 4096
                            && option.label.len() <= 4096
                            && values.insert(option.value.as_str())
                    })
                    && (value.is_empty() || values.contains(value.as_str()))
            }
            _ => true,
        }
    }

    pub fn accepts_children(&self) -> bool {
        matches!(
            self,
            Self::Container | Self::Scroll | Self::Dialog { .. } | Self::Popover { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDescriptor {
    pub content: NodeContent,
    pub frame: Rect,
    pub accessible_name: Option<String>,
    pub accessible_role: Option<AccessibleRole>,
    pub accessible_selected: Option<bool>,
    pub accessible_expanded: Option<bool>,
    pub style: style::NodeStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Create {
        node: NodeId,
        parent: Option<NodeId>,
        index: usize,
        descriptor: NodeDescriptor,
    },
    Update {
        node: NodeId,
        descriptor: NodeDescriptor,
    },
    Move {
        node: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
    Remove {
        node: NodeId,
    },
    Focus {
        node: Option<NodeId>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitBatch {
    pub root: RootId,
    pub previous: u64,
    pub revision: u64,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Tab,
    Enter,
    PageUp,
    PageDown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventPayload {
    Activate,
    Expansion {
        expanded: bool,
    },
    Tree {
        action: TreeAction,
    },
    Drag {
        phase: DragPhase,
        screen_position: [f64; 2],
    },
    Navigate {
        key: NavigationKey,
        shift: bool,
        control: bool,
    },
    Dismiss {
        backdrop: bool,
        revision: u64,
    },
    Toggle {
        checked: bool,
        revision: u64,
    },
    Select {
        value: String,
        revision: u64,
    },
    Edit {
        value: String,
        revision: u64,
        selection_utf16: [usize; 2],
        composing_utf16: Option<[usize; 2]>,
    },
    Focus,
    Scroll {
        x: f64,
        y: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeAction {
    Select,
    AddToSelection,
    RemoveFromSelection,
    Expand,
    Collapse,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformEvent {
    pub node: NodeId,
    pub payload: EventPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

pub fn valid_utf16_range(text: &str, range: [usize; 2]) -> bool {
    if range[0] > range[1] {
        return false;
    }
    let mut offset = 0;
    let mut start_found = range[0] == 0;
    let mut end_found = range[1] == 0;
    for character in text.chars() {
        offset += character.len_utf16();
        start_found |= offset == range[0];
        end_found |= offset == range[1];
    }
    start_found && end_found
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureKey {
    pub node: NodeId,
    pub content_revision: u64,
    pub style_revision: u64,
    pub environment_revision: u64,
    pub max_width: f64,
    pub max_height: f64,
}

impl MeasureKey {
    pub fn is_valid(self) -> bool {
        self.max_width.is_finite()
            && self.max_height.is_finite()
            && self.max_width >= 0.0
            && self.max_height >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureResult {
    pub key: MeasureKey,
    pub width: f64,
    pub height: f64,
    pub baseline: Option<f64>,
}

impl MeasureResult {
    pub fn matches(self, current: MeasureKey) -> bool {
        self.key == current
            && current.is_valid()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
            && self.width <= current.max_width
            && self.height <= current.max_height
            && self.baseline.is_none_or(|baseline| {
                baseline.is_finite() && baseline >= 0.0 && baseline <= self.height
            })
    }
}
