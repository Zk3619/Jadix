//! Unstyled semantic link. Navigation and URL policy belong to the application.
//! `href` alone never opens a browser; install `open_with` and/or `on_activate`.
use crate::{
    authoring::SlotChrome,
    core::tree::{Element, IntoElementList},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Activation = Rc<RefCell<dyn FnMut()>>;
#[derive(Clone)]
struct LinkState {
    disabled: Rc<Cell<bool>>,
    activation: Option<Activation>,
    tab_stop: bool,
}
pub struct LinkBuilder {
    element: Element,
    href: Option<String>,
    open: Option<Box<dyn FnMut(&str)>>,
    activate: Option<Box<dyn FnMut()>>,
    disabled: bool,
    tab_stop: bool,
    chrome: SlotChrome,
}
/// Stable, focusable link without button padding or colors.
pub fn link(id: impl Into<String>) -> LinkBuilder {
    let id = id.into();
    let mut element = Element::new().focus_id(id.clone());
    if !id.trim().is_empty() {
        element.template_id = Some(id);
    }
    LinkBuilder {
        element,
        href: None,
        open: None,
        activate: None,
        disabled: false,
        tab_stop: true,
        chrome: SlotChrome::default(),
    }
}
impl LinkBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }
    /// Application-owned navigation; runs before `on_activate`.
    pub fn open_with(mut self, open: impl FnMut(&str) + 'static) -> Self {
        self.open = Some(Box::new(open));
        self
    }
    pub fn on_activate(mut self, activate: impl FnMut() + 'static) -> Self {
        self.activate = Some(Box::new(activate));
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.element.set_attr("aria-label", label.into());
        self
    }
    pub fn text(mut self, text: impl crate::core::tree::IntoNodeText) -> Self {
        self.element = self.element.text(text);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.element
            .children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.element.children.extend(children);
        self
    }
}
fn sync_disabled(element: &mut Element, disabled: bool) -> bool {
    let Some(state) = element.authoring_state.get::<LinkState>().cloned() else {
        return false;
    };
    let changed = state.disabled.replace(disabled) != disabled;
    element.on_click = if disabled { None } else { state.activation };
    element.set_focusable(!disabled && state.tab_stop);
    element.set_attr("aria-disabled", if disabled { "true" } else { "false" });
    changed
}
impl From<LinkBuilder> for Element {
    fn from(builder: LinkBuilder) -> Self {
        let LinkBuilder {
            mut element,
            href,
            mut open,
            mut activate,
            disabled,
            tab_stop,
            chrome,
        } = builder;
        let disabled_state = Rc::new(Cell::new(disabled));
        let activation: Option<Activation> =
            if activate.is_some() || (href.is_some() && open.is_some()) {
                let disabled = disabled_state.clone();
                Some(Rc::new(RefCell::new(move || {
                    // A pointer gesture may retain this callback before a reactive disable.
                    if disabled.get() {
                        return;
                    }
                    if let (Some(href), Some(open)) = (href.as_deref(), open.as_mut()) {
                        open(href);
                    }
                    if let Some(activate) = activate.as_mut() {
                        activate();
                    }
                })))
            } else {
                None
            };
        element.set_attr("role", "link");
        element.set_attr("accesskit_role", "link");
        element.set_attr("data-slot", "link");
        let mut element = chrome.apply(element);
        element = element.focusable(!disabled && tab_stop);
        if element.cursor.is_none() {
            element.cursor = Some(crate::core::tree::CursorKind::Pointer.into());
        }
        for down in [true, false] {
            let disabled = disabled_state.clone();
            let handler = move |event: &crate::app::pointer_events::MouseEvent| {
                if disabled.get() && event.button == winit::event::MouseButton::Left {
                    event.prevent_default();
                    event.stop_propagation();
                }
            };
            element = if down {
                element.on_mousedown(handler)
            } else {
                element.on_mouseup(handler)
            };
        }
        element.authoring_state.set(LinkState {
            disabled: disabled_state,
            activation,
            tab_stop,
        });
        element.disabled_sync = Some(sync_disabled);
        crate::primitives::apply_disabled(&mut element, disabled);
        element
    }
}
impl IntoElementList for LinkBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
