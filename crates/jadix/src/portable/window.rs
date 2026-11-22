//! Desktop window declarations use the root's shared window service.
use super::{Element, Signal};
pub use jadix_core::window::*;

pub struct WindowBuilder {
    id: String,
    options: DeclaredWindowOptions,
    bindings: WindowBindings,
}

pub fn window(id: impl Into<String>) -> WindowBuilder {
    WindowBuilder {
        id: id.into(),
        options: DeclaredWindowOptions::new(640, 480),
        bindings: WindowBindings::default(),
    }
}

impl WindowBuilder {
    pub fn options(mut self, options: DeclaredWindowOptions) -> Self {
        self.options = options;
        self
    }
    pub fn content(mut self, content: WindowContent) -> Self {
        self.options.content = Some(content);
        self
    }
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.bindings.open = Some(open);
        self
    }
    pub fn attributes(mut self, attributes: Signal<WindowAttributes>) -> Self {
        self.bindings.attributes = Some(attributes);
        self
    }
    pub fn state(mut self, state: Signal<Option<WindowState>>) -> Self {
        self.bindings.state = Some(state);
        self
    }
    pub fn error(mut self, error: Signal<Option<String>>) -> Self {
        self.bindings.error = Some(error);
        self
    }
    /// Mount in the current owner. The error receiver remains available to the
    /// application; OS request acknowledgements are asynchronous.
    pub fn mount(self) -> Result<std::sync::mpsc::Receiver<String>, String> {
        let service = use_window_declarations().ok_or("this root has no window service")?;
        service.mount_bound(self.id, move || Some(self.options.clone()), self.bindings)
    }
}

/// Native RSX: `<Window id="settings" options={options} open={open} />`.
/// Window content is selected by its application-registered factory.
#[derive(Default)]
pub struct WindowProps {
    pub id: String,
    pub options: Option<DeclaredWindowOptions>,
    pub content: Option<WindowContent>,
    pub open: Option<Signal<bool>>,
    pub attributes: Option<Signal<WindowAttributes>>,
    pub state: Option<Signal<Option<WindowState>>>,
    pub error: Option<Signal<Option<String>>>,
    pub children: Option<Element>,
    pub base: super::Base,
}

#[allow(non_snake_case)]
pub fn Window(props: WindowProps) -> Element {
    assert!(
        props.children.is_none(),
        "Window content requires a registered root factory"
    );
    let mut builder = window(props.id);
    if let Some(options) = props.options {
        builder.options = options;
    }
    if let Some(content) = props.content {
        builder.options.content = Some(content);
    }
    builder.bindings = WindowBindings {
        open: props.open,
        attributes: props.attributes,
        state: props.state,
        error: props.error,
    };
    builder.mount().expect("mount Window declaration");
    let mut placeholder = Element::new();
    placeholder.visibility = Some(std::rc::Rc::new(|| false));
    placeholder
}
