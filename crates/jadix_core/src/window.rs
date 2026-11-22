//! Shared desktop window attributes, modes, and declaration state.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    Normal,
    Maximized,
    Minimized,
    FullScreen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    #[default]
    Normal,
    Dialog,
    Panel,
    Popup,
    Overlay,
}

/// Selects an application-registered root factory when a window opens.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct WindowContent {
    pub factory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
}

impl WindowContent {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.factory.trim().is_empty() || self.factory.len() > 256 || self.factory.chars().any(char::is_control) {
            return Err("a valid window content factory name is required");
        }
        if self.argument.as_ref().is_some_and(|value| value.len() > 64 * 1024) {
            return Err("window content argument exceeds 64 KiB");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct DeclaredWindowOptions {
    pub kind: WindowKind,
    pub width: u32,
    pub height: u32,
    pub modal: bool,
    pub initial_mode: WindowMode,
    pub initially_visible: bool,
    pub attributes: WindowAttributes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<WindowContent>,
}

impl Default for DeclaredWindowOptions {
    fn default() -> Self {
        Self::new(640, 480)
    }
}

impl DeclaredWindowOptions {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            kind: WindowKind::Normal,
            width,
            height,
            modal: false,
            initial_mode: WindowMode::Normal,
            initially_visible: true,
            attributes: WindowAttributes::default(),
            content: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.width == 0 || self.height == 0 {
            return Err("positive window content dimensions are required");
        }
        if let Some(content) = &self.content {
            content.validate()?;
        }
        self.attributes.validate()
    }
}

/// Presence of a declared window across asynchronous host requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowPresence {
    #[default]
    Absent,
    Opening,
    Open,
    ClosedByHost,
}

impl WindowPresence {
    /// A late open acknowledgement must preserve an intervening host close.
    pub fn finish_open(self, accepted: bool) -> Self {
        match (self, accepted) {
            (Self::Opening, true) => Self::Open,
            (Self::Opening, false) => Self::Absent,
            (state, _) => state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowTitleBar {
    Standard,
    Transparent,
    Hidden,
}

/// The stacking level of a desktop window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowLevel {
    #[default]
    Normal,
    Floating,
    AboveDock,
    DesktopIcon,
}

/// The virtual desktops on which a window can appear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowWorkspaces {
    #[default]
    Current,
    All,
}

/// Whether a window inherits its application's appearance or overrides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowAppearance {
    Inherit,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

/// The outer frame origin in the host's screen coordinate system.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

/// Screen orientations accepted by a native window. Platform policy may still
/// restrict the orientations the device can use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenOrientations {
    #[default]
    LandscapeAndPortrait,
    Landscape,
    Portrait,
}

macro_rules! window_attributes {
    ($( $(#[$attribute:meta])* $field:ident: $ty:ty, )*) => {
        /// A partial set of desired attributes. Omitted fields are left as they are.
        /// Content dimensions and position use the host's logical units.
        #[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
        #[serde(default)]
        pub struct WindowAttributes {
            $(
                $(#[$attribute])*
                pub $field: Option<$ty>,
            )*
        }

        impl WindowAttributes {
            /// Add or replace the fields explicitly set in this update.
            pub fn merge(&mut self, update: Self) {
                $(
                    if update.$field.is_some() {
                        self.$field = update.$field;
                    }
                )*
            }

            /// Compare desired attributes with the last successfully applied values.
            pub fn diff(&self, desired: &Self) -> Option<Self> {
                let mut diff = desired.clone();
                let mut changed = false;
                $(
                    if self.$field == desired.$field {
                        diff.$field = None;
                    } else {
                        changed |= diff.$field.is_some();
                    }
                )*
                changed.then_some(diff)
            }
        }
    };
}

window_attributes! {
    title: String,
    resizable: bool,
    /// When false, a user close request leaves the window open.
    close_on_press: bool,
    close_on_escape: bool,
    close_on_blur: bool,
    /// Keeps a secondary window's root mounted when its close policy hides it.
    dismiss_on_close: bool,
    level: WindowLevel,
    workspaces: WindowWorkspaces,
    participates_in_cycle: bool,
    appearance: WindowAppearance,
    /// Per-page device orientation policy; restored when the page leaves the foreground.
    screen_orientations: ScreenOrientations,
    title_bar: WindowTitleBar,
    content_size: WindowSize,
    min_content_size: WindowSize,
    max_content_size: WindowSize,
    position: WindowPosition,
    /// Desired mode. Completion of a host request does not imply the OS transition finished.
    mode: WindowMode,
    /// Show or hide the window while retaining its content and reactive owner.
    visible: bool,
}

impl WindowAttributes {
    /// Validate portable values before submitting a host request. Hosts also
    /// validate capabilities and limits inherited from the actual window.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .position
            .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err("window position must be finite");
        }
        for size in [self.content_size, self.max_content_size]
            .into_iter()
            .flatten()
        {
            if size.width == 0 || size.height == 0 {
                return Err("window content and maximum sizes must be positive");
            }
        }
        if let (Some(min), Some(max)) = (self.min_content_size, self.max_content_size) {
            if min.width > max.width || min.height > max.height {
                return Err("window minimum size exceeds maximum size");
            }
        }
        Ok(())
    }

    /// Validate an update together with previously acknowledged attributes.
    pub fn validate_update(&self, update: &Self) -> Result<(), &'static str> {
        let mut merged = self.clone();
        merged.merge(update.clone());
        merged.validate()
    }
}

/// Observed outer frame, in the host's logical screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowFrame {
    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// Current display geometry, in the same coordinates as the window frame.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WindowDisplay {
    /// Host display identifier, valid for the current display configuration.
    pub id: Option<u32>,
    pub frame: WindowFrame,
    pub work_area: WindowFrame,
}

/// Logical space reserved for a hidden system title bar and window buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WindowTitleBarInsets {
    pub top: f64,
    pub left: f64,
    pub right: f64,
}

/// Actual host observations, kept separate from desired attributes so native
/// animations and user actions cannot be mistaken for request acknowledgements.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WindowState {
    pub id: Option<String>,
    pub frame: WindowFrame,
    #[serde(default)]
    pub normal_frame: Option<WindowFrame>,
    pub content_width: f64,
    pub content_height: f64,
    pub backing_scale: f64,
    pub display: Option<WindowDisplay>,
    #[serde(default)]
    pub title_bar_insets: WindowTitleBarInsets,
    pub mode: WindowMode,
    pub key: bool,
    pub visible: bool,
    pub occluded: bool,
}

impl WindowState {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.frame.is_valid()
            || self.normal_frame.is_some_and(|frame| !frame.is_valid())
            || self
                .display
                .is_some_and(|display| !display.frame.is_valid() || !display.work_area.is_valid())
        {
            return Err("invalid observed window geometry");
        }
        if [
            self.content_width,
            self.content_height,
            self.title_bar_insets.top,
            self.title_bar_insets.left,
            self.title_bar_insets.right,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || value < 0.0)
            || !self.backing_scale.is_finite()
            || self.backing_scale <= 0.0
        {
            return Err("invalid observed window size, insets or scale");
        }
        Ok(())
    }
}

mod service;
mod session;
pub use service::{
    DeclaredWindow, WindowBindings, WindowDeclaration, WindowDeclarations, WindowHost,
    WindowRequest, WindowRequests, WindowResult, provide_window_declarations,
    use_window_declarations,
};
pub use session::{WindowSession, WindowSessionChild};
