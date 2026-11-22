pub mod accessibility;
pub mod keyboard;
pub mod interaction;
pub mod input;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{cell::Cell, rc::Rc};

pub mod capabilities;
mod popover;
pub mod theme;
pub use popover::{PopoverPlacement, PopoverPosition};
pub mod component;
pub mod hybrid;
pub mod retired;
pub mod style;
pub mod viewport;
pub mod scroll;

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
    pub fn from_token(root: RootId, token: &str) -> Option<Self> {
        let generation = token.parse::<u64>().ok()?;
        (generation != 0 && generation.to_string() == token).then_some(Self { root, generation })
    }

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

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
    ListBox,
    ListBoxOption { tab_stop: bool },
    Heading,
    Grid { rows: u32, columns: u32 },
    GridCell {
        row: u32,
        column: u32,
        row_span: u32,
        column_span: u32,
    },
    GridColumnHeader { column: u32, column_span: u32 },
}

impl AccessibleRole {
    /// Stable role vocabulary shared by native wire and GPU/DOM accessibility.
    pub fn aria_role(self) -> &'static str {
        match self {
            Self::RadioGroup => "radiogroup",
            Self::RadioItem { .. } => "radio",
            Self::TabList => "tablist",
            Self::TabItem => "tab",
            Self::TabPanel => "tabpanel",
            Self::Tree => "tree",
            Self::TreeItem => "treeitem",
            Self::ListBox => "listbox",
            Self::ListBoxOption { .. } => "option",
            Self::Heading => "heading",
            Self::Grid { .. } => "grid",
            Self::GridCell { .. } => "gridcell",
            Self::GridColumnHeader { .. } => "columnheader",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeContent {
    /// Canonical JSON validated by `HostedContent`; keeps retained equality exact.
    Hosted { specification: String },
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
        /// UTF-16 insertion limit; controlled values/history are preserved.
        max_length: Option<usize>,
        revision: u64,
    },
    Textarea {
        value: String,
        placeholder: String,
        enabled: bool,
        max_length: Option<usize>,
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
    pub fn input_max_length(&self) -> Option<usize> {
        match self {
            Self::Input { max_length, .. } | Self::Textarea { max_length, .. } => *max_length,
            _ => None,
        }
    }
    pub fn image_dimensions(&self) -> Option<[u32; 2]> {
        let Self::Image { source, alt } = self else {
            return None;
        };
        if alt.len() > 4096 {
            return None;
        }
        Self::image_source_dimensions(source)
    }

    pub fn image_source_dimensions(source: &str) -> Option<[u32; 2]> {
        use base64::Engine;
        if source.len() > 1024 * 1024 {
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

    /// Recognizes bounded base64 data URLs for the additional native raster formats.
    pub fn is_inline_raster_source(source: &str) -> bool {
        use base64::Engine;
        if source.len() > 1024 * 1024 {
            return false;
        }
        let Some((mime, encoded)) = source
            .strip_prefix("data:image/")
            .and_then(|source| source.split_once(";base64,"))
        else {
            return false;
        };
        if !matches!(
            mime,
            "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "x-icon" | "vnd.microsoft.icon" | "icns"
        ) {
            return false;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            return false;
        };
        match mime {
            "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            "bmp" => bytes.starts_with(b"BM"),
            "tiff" => bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*"),
            "x-icon" | "vnd.microsoft.icon" => bytes.starts_with(&[0, 0, 1, 0]),
            "icns" => bytes.starts_with(b"icns"),
            _ => false,
        }
    }

    /// A bounded UTF-8 SVG source encoded as a base64 data URL.
    pub fn is_inline_svg_source(source: &str) -> bool {
        use base64::Engine;
        source.len() <= 1024 * 1024
            && source
                .strip_prefix("data:image/svg+xml;base64,")
                .and_then(|encoded| base64::engine::general_purpose::STANDARD.decode(encoded).ok())
                .is_some_and(|bytes| {
                    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
                    std::str::from_utf8(bytes).is_ok_and(|svg| svg.trim_start().starts_with('<'))
                })
    }

    /// A local file URL or a resource path relative to the application bundle.
    pub fn has_local_image_source(&self) -> bool {
        matches!(self, Self::Image { source, alt } if alt.len() <= 4096 && Self::is_local_image_source(source))
    }

    pub fn is_local_image_source(source: &str) -> bool {
        if source.len() > 4096 || source.chars().any(char::is_control) {
            return false;
        }
        if let Some(path) = source.strip_prefix("file:///") {
            return !path.is_empty()
                && !path.starts_with('/')
                && !path.contains(['?', '#', '\\']);
        }
        source.strip_prefix("asset:/").is_some_and(|path| {
            !path.contains('\\') && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
        })
    }

    /// A remote image served over HTTPS.
    pub fn has_remote_image_source(&self) -> bool {
        matches!(self, Self::Image { source, alt } if alt.len() <= 4096 && Self::is_remote_image_source(source))
    }

    pub fn is_remote_image_source(source: &str) -> bool {
        if source.len() > 4096
            || source.chars().any(|ch| ch.is_control() || ch.is_whitespace())
            || source.contains(['\\', '#'])
        {
            return false;
        }
        source.strip_prefix("https://").is_some_and(|url| {
            let authority = url.split(['/', '?']).next().unwrap_or_default();
            !authority.is_empty() && !authority.contains('@')
        })
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Hosted { specification } => crate::hosted::HostedContent::from_json(specification).is_ok(),
            Self::Input { max_length, .. } | Self::Textarea { max_length, .. } => {
                max_length.is_none_or(|n| n <= input::MAX_INPUT_LENGTH)
            }
            Self::Image { source, alt } => {
                alt.len() <= 4096
                    && (Self::image_source_dimensions(source).is_some()
                        || Self::is_inline_raster_source(source)
                        || Self::is_inline_svg_source(source)
                        || Self::is_local_image_source(source)
                        || Self::is_remote_image_source(source))
            }
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
            Self::Container | Self::Scroll | Self::Button { .. } | Self::Dialog { .. } | Self::Popover { .. }
        ) || matches!(self, Self::Hosted { specification } if crate::hosted::HostedContent::from_json(specification).is_ok_and(|v| v.accepts_children()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDescriptor {
    pub interaction: interaction::InteractionPolicy,
    pub keyboard: keyboard::KeyboardPolicy,
    pub content: NodeContent,
    /// Unmodified keys handled by the component instead of the text editor.
    /// Hosts must preserve IME composition and all modified editing shortcuts.
    pub navigation_keys: Vec<NavigationKey>,
    pub frame: Rect,
    /// Additional nonvisual help, separate from the accessible name.
    pub accessible_description: Option<String>,
    pub accessible_name: Option<String>,
    pub accessible_role: Option<AccessibleRole>,
    pub accessible_selected: Option<bool>,
    pub accessible_expanded: Option<bool>,
    pub accessibility: accessibility::Accessibility,
    pub accessibility_relations: AccessibilityRelations,
    pub style: style::NodeStyle,
}

impl NodeDescriptor {
    pub fn accepts_accessibility_action(&self, id: &str, value: Option<f64>) -> bool {
        self.accessibility.accepts(id, value) && !matches!(self.content,
            NodeContent::Button { enabled: false, .. } | NodeContent::Input { enabled: false, .. }
            | NodeContent::Textarea { enabled: false, .. } | NodeContent::Checkbox { enabled: false, .. }
            | NodeContent::Switch { enabled: false, .. } | NodeContent::Select { enabled: false, .. })
    }
    pub fn accepts_interaction(&self) -> bool {
        matches!(self.content, NodeContent::Container | NodeContent::Text { .. }
            | NodeContent::Image { .. } | NodeContent::Button { enabled: true, .. })
    }
    pub fn can_focus(&self) -> bool {
        matches!(self.content, NodeContent::Button { enabled: true, .. }
            | NodeContent::Input { enabled: true, .. } | NodeContent::Textarea { enabled: true, .. }
            | NodeContent::Checkbox { enabled: true, .. } | NodeContent::Switch { enabled: true, .. }
            | NodeContent::Select { enabled: true, .. })
            || (matches!(self.content, NodeContent::Container | NodeContent::Text { .. } | NodeContent::Image { .. }) && self.keyboard.focusable)
    }
    pub fn accepts_keyboard(&self) -> bool {
        self.can_focus()
            || (matches!(self.content, NodeContent::Container | NodeContent::Text { .. } | NodeContent::Image { .. })
                && (self.keyboard.key_down || self.keyboard.key_up || !self.keyboard.shortcuts.is_empty()))
    }
}

/// Cross-node accessibility relationships, expressed in portable presentation IDs.
/// Targets may be absent from the current snapshot and resolve when they are published.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessibilityRelations {
    pub active_descendant: Option<NodeId>,
    pub controls: Vec<NodeId>,
}

impl AccessibilityRelations {
    /// Resolve authoring IDs after the complete snapshot has been collected.
    /// Missing or ambiguous names stay absent, including virtualized targets.
    pub fn resolve_named(
        active_descendant: Option<&str>,
        controls: impl IntoIterator<Item = impl AsRef<str>>,
        mut resolve: impl FnMut(&str) -> Option<NodeId>,
    ) -> Self {
        let active_descendant = active_descendant.and_then(&mut resolve);
        let mut targets = Vec::new();
        for name in controls {
            if let Some(target) = resolve(name.as_ref()) {
                if !targets.contains(&target) { targets.push(target); }
            }
        }
        Self { active_descendant, controls: targets }
    }

    pub fn is_valid_for(&self, root: RootId) -> bool {
        self.active_descendant
            .is_none_or(|target| target.root == root)
            && self.controls.len() <= 64
            && self.controls.iter().all(|target| target.root == root)
    }
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

/// Acknowledges application of protocol operations, not terminal scanout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyReceipt {
    pub root: RootId,
    pub previous: u64,
    pub revision: u64,
}
impl ApplyReceipt {
    pub fn for_batch(batch: &CommitBatch) -> Self {
        Self {
            root: batch.root,
            previous: batch.previous,
            revision: batch.revision,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    Escape,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventPayload {
    Interaction(interaction::InteractionEvent),
    FileDrop(interaction::FileDropEvent),
    Key(keyboard::KeyEvent),
    Shortcut { id: String },
    AccessibilityAction { id: String, value: Option<f64> },
    Hosted { event: String, data: serde_json::Value },
    Activate,
    /// Pointer presence over an enabled button; keyboard focus is independent.
    Hover { inside: bool },
    /// Actual pointer motion in window logical coordinates.
    PointerMove { x: f64, y: f64 },
    ImageLoad(ImageLoadEvent),
    /// Native keyboard confirmation after committed edits. Uses a fresh input
    /// revision so replayed confirmations are rejected by the shared model.
    Submit {
        revision: u64,
    },
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
    /// An Escape key or outside press on an open popover.
    PopoverCancel { backdrop: bool },
    /// AppKit committed a retained popover's logical open or close state.
    PopoverLifecycle { open: bool },
    /// Resolved panel geometry, or no geometry when its anchor leaves the viewport.
    PopoverPosition { position: Option<PopoverPosition> },
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
    /// Keyboard-driven focus; adapters may render the shared focus-visible cue.
    FocusVisible,
    Focus,
    /// Release this node's focus; a delayed blur must not clear another node.
    Blur,
    ScrollActivity(scroll::ScrollActivity),
    Scroll {
        x: f64,
        y: f64,
    },
    /// Synchronized native scroll geometry, including viewport-only resizes.
    ScrollViewport { x: f64, y: f64, width: f64, height: f64 },
    /// Document-space geometry after viewport magnification. Hosts also report
    /// ScrollViewport so existing virtualization consumers retain their contract.
    ScrollZoom { scale: f64, x: f64, y: f64, width: f64, height: f64 },
    /// Pointer-wheel movement over a scroll container, in Jadix logical pixels.
    Wheel {
        delta_x: f64,
        delta_y: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageLoadEvent {
    Started,
    /// Cumulative response bytes. A total of zero means the length is unknown.
    Progress { loaded: u64, total: u64 },
    Loaded { width: u32, height: u32 },
    Failed { message: String },
    Ended,
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
